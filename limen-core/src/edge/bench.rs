//! (Work)bench [test] Queue implementation.
//!
//! A small, safe, no-alloc SPSC ring buffer intended for tests. This
//! implementation stores `T` directly in a `[T; N]` array so it can return
//! contiguous `&mut [T]` batch slices without unsafe code. To allow safe
//! initialization of the array we require `T: Default + Clone` on the impl.

use crate::edge::{Edge, EdgeOccupancy, EnqueueResult, PeekResponse};
use crate::errors::QueueError;
use crate::message::batch::BatchView;
use crate::message::payload::Payload;
use crate::message::Message;
use crate::policy::{AdmissionPolicy, EdgePolicy, WatermarkState};

use core::mem;

/// A simple, no-alloc, no-unsafe SPSC ring buffer for tests that honors `EdgePolicy`.
///
/// Notes:
/// - Byte accounting is estimated as `len * size_of::<T>()`, which is
///   sufficient for tests with fixed-size items (e.g., `Message<u32>`).
/// - `AdmissionPolicy::DeadlineAndQoSAware` maps over-budget cases to either
///   `DroppedNewest` (when `OverBudgetAction::Drop`) or `Rejected` for the
///   other actions, since those behaviors live outside a queue.
/// - `AdmissionPolicy::Block` returns `Rejected` in this test queue (no blocking).
pub struct TestSpscRingBuf<T, const N: usize> {
    /// Backing storage: always initialized with `T::default()`.
    buf: [T; N],

    /// Index of the front element (logical head).
    head: usize,

    /// Index one past the last live element (logical tail = (head + len) % N).
    tail: usize,

    /// Number of live elements currently in the ring.
    len: usize,

    /// Aggregate bytes currently stored (sum of message header+payload sizes).
    bytes: usize,
}

impl<T: Default + Clone, const N: usize> TestSpscRingBuf<T, N> {
    /// Create a new empty ring.
    #[inline]
    pub fn new() -> Self {
        Self {
            buf: core::array::from_fn(|_| T::default()),
            head: 0,
            tail: 0,
            len: 0,
            bytes: 0,
        }
    }

    /// `true` when the ring is full.
    #[inline]
    fn is_full(&self) -> bool {
        self.len == N
    }

    /// Internal: push an item into the tail (assumes capacity available).
    ///
    /// Overwrites the slot at `tail` (which should be a default placeholder).
    #[inline]
    fn push_raw(&mut self, item: T) {
        self.buf[self.tail] = item;
        self.tail = (self.tail + 1) % N;
        self.len += 1;
    }

    /// Internal: pop an item from the head (assumes len > 0).
    ///
    /// Replaces the vacated slot with `T::default()` and returns the previous
    /// value stored there.
    #[inline]
    fn pop_raw(&mut self) -> T {
        let item = mem::take(&mut self.buf[self.head]);
        self.head = (self.head + 1) % N;
        self.len -= 1;
        item
    }

    /// Normalize the ring so the live region is contiguous at `buf[0..len]`.
    ///
    /// Moves items in-place using `mem::replace`, leaving default placeholders
    /// in unused slots. After normalization `head == 0` and `tail == len % N`.
    fn normalize(&mut self) {
        if self.len == 0 {
            // Empty ring: canonicalize indices.
            self.head = 0;
            self.tail = 0;
            return;
        }
        if self.head == 0 {
            // Already contiguous at start.
            self.tail = (self.head + self.len) % N;
            return;
        }

        // Move live items to the beginning in order.
        for i in 0..self.len {
            let src_idx = (self.head + i) % N;
            // Extract src value into tmp, leaving default in src slot.
            let tmp = mem::take(&mut self.buf[src_idx]);
            // Place the extracted value into destination slot `i`.
            self.buf[i] = tmp;
        }

        // Ensure remaining slots are defaulted (not strictly required but clearer).
        for i in self.len..N {
            let _ = mem::take(&mut self.buf[i]);
        }

        self.head = 0;
        self.tail = (self.head + self.len) % N;
    }
}

impl<P: Payload + Clone + Default, const N: usize> Edge for TestSpscRingBuf<Message<P>, N> {
    type Item = Message<P>;

    /// Try to push a single message into the ring using the given admission policy.
    ///
    /// Respects watermark-based `DropOldest` admission and the hard-cap rejection.
    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        let items = self.len;
        let bytes = self.bytes;

        // If hard cap is reached and ring is full, reject.
        if policy.caps.at_or_above_hard(items, bytes) && self.is_full() {
            return EnqueueResult::Rejected;
        }

        // If between soft & hard and admission policy is DropOldest, evict one item.
        match policy.watermark(items, bytes) {
            WatermarkState::BetweenSoftAndHard
                if matches!(policy.admission, AdmissionPolicy::DropOldest) && self.len > 0 =>
            {
                let evicted = self.pop_raw();
                self.bytes = self
                    .bytes
                    .saturating_sub(*evicted.header().payload_size_bytes());
            }
            _ => {}
        }

        if self.is_full() {
            return EnqueueResult::Rejected;
        }

        self.bytes = self
            .bytes
            .saturating_add(*item.header().payload_size_bytes());
        self.push_raw(item);
        EnqueueResult::Enqueued
    }

    /// Try to pop a single message.
    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        if self.len == 0 {
            return Err(QueueError::Empty);
        }
        let item = self.pop_raw();
        self.bytes = self
            .bytes
            .saturating_sub(*item.header().payload_size_bytes());
        Ok(item)
    }

    /// Snapshot occupancy for telemetry / admission decisions.
    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        let items = self.len;
        let bytes = self.bytes;
        let watermark = policy.watermark(items, bytes);
        EdgeOccupancy {
            items,
            bytes,
            watermark,
        }
    }

    /// Peek at the front item without removing it.
    ///
    /// Returns `PeekResponse::Borrowed(&Message<P>)` for zero-copy paths.
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        if self.len == 0 {
            return Err(QueueError::Empty);
        }
        Ok(PeekResponse::Borrowed(&self.buf[self.head]))
    }

    /// Pop a batch of items according to `BatchingPolicy`.
    ///
    /// - Disjoint windows: pop up to `fixed_n` or `max_delta_t`.
    /// - Sliding windows: return `size` items but only pop/advance `stride` items.
    /// - `fixed_n` and `max_delta_t` combine: stop when either limit reached.
    /// - No `fixed_n` or `max_delta_t`: treat as `fixed_n` = 1.
    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        use crate::policy::WindowKind;

        if self.len == 0 {
            return Err(QueueError::Empty);
        }

        // Normalize to make live items contiguous at buf[0..len].
        self.normalize();

        // Keep old len for later arithmetic.
        let old_len = self.len;

        // Extract policy knobs.
        let fixed_opt = *policy.fixed_n();
        let delta_t_opt = *policy.max_delta_t();
        let window_kind = policy.window_kind();

        // If both caps are absent, treat as fixed_n = 1 (per your tightened semantics).
        let effective_fixed: Option<usize> = if fixed_opt.is_none() && delta_t_opt.is_none() {
            Some(1)
        } else {
            fixed_opt
        };

        // Compute how many items are within max_delta_t relative to the front, if any.
        let mut delta_count = self.len;
        if let Some(cap) = delta_t_opt {
            // Use creation_tick for age calculation.
            let front_ticks: crate::types::Ticks = *self.buf[0].header().creation_tick();
            let mut c = 0usize;
            while c < self.len {
                let tick = *self.buf[c].header().creation_tick();
                let delta = tick.saturating_sub(front_ticks);
                if delta <= cap {
                    c += 1;
                } else {
                    break;
                }
            }
            delta_count = c;
        }

        // Helper to apply effective fixed-N cap (if present).
        let apply_fixed = |limit: usize| -> usize {
            if let Some(n) = effective_fixed {
                core::cmp::min(limit, n)
            } else {
                limit
            }
        };

        // --- Disjoint windows: pop up to fixed / delta_count.
        if let WindowKind::Disjoint = window_kind {
            let take_n = apply_fixed(core::cmp::min(self.len, delta_count));
            if take_n == 0 {
                return Err(QueueError::Empty);
            }

            // Advance logical state first (normalized head == 0).
            let new_head = take_n % N;
            self.len = old_len - take_n;
            self.head = new_head;
            self.tail = (self.head + self.len) % N;

            // Subtract bytes for the popped items.
            let mut dropped_bytes = 0usize;
            for i in 0..take_n {
                dropped_bytes =
                    dropped_bytes.saturating_add(*self.buf[i].header().payload_size_bytes());
            }
            self.bytes = self.bytes.saturating_sub(dropped_bytes);

            // Return a borrowed slice covering the popped items.
            let slice = &mut self.buf[..take_n];
            return Ok(BatchView::from_borrowed(slice, take_n));
        }

        // --- Sliding windows: present `size` but pop `stride`.
        if let WindowKind::Sliding(sw) = window_kind {
            let stride = *sw.stride();
            let size = *sw.size();

            // Determine how many items we can present, bounded by availability, size, delta_count, and fixed.
            let mut max_present = core::cmp::min(self.len, size);
            max_present = apply_fixed(core::cmp::min(max_present, delta_count));

            // How many to actually pop from the front (stride), bounded by availability.
            let stride_to_pop = core::cmp::min(stride, self.len);

            if max_present == 0 {
                return Err(QueueError::Empty);
            }

            // Advance logical state by stride_to_pop (we pop only stride).
            let new_head = stride_to_pop % N;
            self.len = old_len - stride_to_pop;
            self.head = new_head;
            self.tail = (self.head + self.len) % N;

            // Subtract bytes only for the popped items.
            let mut popped_bytes = 0usize;
            for i in 0..stride_to_pop {
                popped_bytes =
                    popped_bytes.saturating_add(*self.buf[i].header().payload_size_bytes());
            }
            self.bytes = self.bytes.saturating_sub(popped_bytes);

            // Return a borrowed slice of `max_present` items (first `stride_to_pop` are popped).
            let slice = &mut self.buf[..max_present];
            return Ok(BatchView::from_borrowed(slice, max_present));
        }

        // --- Fixed-N and/or max_delta_t (non-sliding, non-disjoint).
        let mut take_n = core::cmp::min(self.len, delta_count);
        take_n = apply_fixed(take_n);

        if take_n == 0 {
            return Err(QueueError::Empty);
        }

        // Advance logical state by take_n.
        let new_head = take_n % N;
        self.len = old_len - take_n;
        self.head = new_head;
        self.tail = (self.head + self.len) % N;

        // Subtract bytes for the popped items.
        let mut dropped_bytes = 0usize;
        for i in 0..take_n {
            dropped_bytes =
                dropped_bytes.saturating_add(*self.buf[i].header().payload_size_bytes());
        }
        self.bytes = self.bytes.saturating_sub(dropped_bytes);

        let slice = &mut self.buf[..take_n];
        Ok(BatchView::from_borrowed(slice, take_n))
    }
}

impl<T: Default + Clone, const N: usize> Default for TestSpscRingBuf<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
