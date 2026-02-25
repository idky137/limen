//! SpscAtomicRing: high-performance SPSC ring using atomics (P2 high-perf).
//!
//! **Unsafe implementation** gated behind `ring_unsafe`.
//! Uses a power-of-two capacity array of `MaybeUninit<T>` with atomic head/tail.

#![allow(unsafe_code)]

use core::ptr;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::edge::{Edge, EdgeOccupancy, EnqueueResult};
use crate::errors::QueueError;
use crate::message::{payload::Payload, Message};
use crate::policy::EdgePolicy;
use crate::prelude::AdmissionDecision;
use crate::prelude::BatchView;

/// A high-performance, bounded, single-producer single-consumer ring buffer.
///
/// Stores elements in a power-of-two array of `MaybeUninit<T>` and advances
/// head and tail indices with atomic operations. Also tracks a byte counter
/// for admission control policies. This type assumes strict single-producer
/// single-consumer usage throughout its lifetime.
pub struct SpscAtomicRing<T> {
    buf: Box<[MaybeUninit<T>]>,
    cap: usize,
    head: AtomicUsize, // consumer index
    tail: AtomicUsize, // producer index
    bytes_in_queue: AtomicUsize,
}

impl<T> SpscAtomicRing<T> {
    /// Creates a ring with the given item capacity.
    ///
    /// The capacity must be a power of two; indices wrap using a bit mask.
    ///
    /// # Safety
    ///
    /// This constructor initializes unfilled storage via `Vec::set_len` and
    /// returns a queue that relies on strict usage invariants which the caller
    /// must uphold for the entire lifetime of the queue:
    ///
    /// - **Single producer, single consumer discipline**: exactly one producer
    ///   thread may call the enqueue operations and exactly one consumer thread
    ///   may call the dequeue and peek operations. No other threads may access
    ///   the queue concurrently.
    /// - **Do not read uninitialized slots**: only dequeue or peek when the
    ///   queue is known to be non-empty. A referenced item obtained from `peek`
    ///   becomes invalid as soon as the consumer advances the head index.
    /// - **Do not overwrite live elements**: only enqueue when the queue is
    ///   not full. The producer must write the element to the slot before it
    ///   advances the tail index; the consumer must read the element before it
    ///   advances the head index.
    /// - **Capacity constraint**: `capacity` must be a power of two. This
    ///   function asserts that condition; violating it would break index
    ///   masking assumptions elsewhere.
    /// - **Element drop behavior**: any elements left in the queue at drop
    ///   time will be read and dropped during the queue’s `Drop` implementation.
    ///   If `T` has side effects or can panic on drop, those effects will occur
    ///   at that time.
    pub unsafe fn with_capacity(capacity: usize) -> Self {
        assert!(
            capacity.is_power_of_two(),
            "capacity must be a power of two"
        );
        let mut v: Vec<MaybeUninit<T>> = Vec::with_capacity(capacity);
        unsafe {
            v.set_len(capacity);
        }
        Self {
            buf: v.into_boxed_slice(),
            cap: capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            bytes_in_queue: AtomicUsize::new(0),
        }
    }

    #[inline]
    fn mask(&self) -> usize {
        self.cap - 1
    }

    #[inline]
    fn len(&self) -> usize {
        let h = self.head.load(Ordering::Acquire);
        let t = self.tail.load(Ordering::Acquire);
        t.wrapping_sub(h) & self.mask()
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len() == self.cap - 1
    }

    #[inline]
    fn push_raw(&self, item: T) {
        // Compute a *mut T to the target slot and write without dropping the old value.
        let t = self.tail.load(Ordering::Relaxed);
        let idx = t & self.mask();
        let base: *mut MaybeUninit<T> = self.buf.as_ptr() as *mut MaybeUninit<T>;
        let slot: *mut T = unsafe { base.add(idx) as *mut T };
        unsafe { ptr::write(slot, item) };
        self.tail.store(t.wrapping_add(1), Ordering::Release);
    }

    #[inline]
    fn pop_raw(&self) -> T {
        // Compute a *const T to the current head slot and read it by value.
        let h = self.head.load(Ordering::Relaxed);
        let idx = h & self.mask();
        let base: *const MaybeUninit<T> = self.buf.as_ptr();
        let slot: *const T = unsafe { base.add(idx) as *const T };
        let item = unsafe { ptr::read(slot) };
        self.head.store(h.wrapping_add(1), Ordering::Release);
        item
    }

    /// Borrow a reference to the item at `head + offset` without advancing head.
    ///
    /// # Safety
    /// Requires SPSC discipline. The returned reference must not outlive `&self`
    /// and is only valid while the corresponding slot remains logically occupied.
    #[inline]
    fn peek_ref_at_offset(&self, offset: usize) -> &T {
        let h = self.head.load(Ordering::Acquire);
        let idx = h.wrapping_add(offset) & self.mask();
        let base: *const MaybeUninit<T> = self.buf.as_ptr();
        let slot: *const T = unsafe { base.add(idx) as *const T };
        unsafe { &*slot }
    }
}

impl<T> Drop for SpscAtomicRing<T> {
    fn drop(&mut self) {
        // Drain any initialized items to drop them safely.
        while self.len() > 0 {
            let _ = self.pop_raw();
        }
    }
}

impl<P: Payload + std::clone::Clone> Edge for SpscAtomicRing<Message<P>> {
    type Item = Message<P>;

    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        // Ask the policy for a pure admission decision.
        let decision = self.get_admission_decision(policy, &item);

        // Item bytes (header.payload_size_bytes is used elsewhere in this module).
        let item_bytes = item.header().payload_size_bytes();

        match decision {
            AdmissionDecision::Admit => {
                // Ensure physical and logical capacity.
                let items = self.len();
                let bytes = self.bytes_in_queue.load(Ordering::Acquire);
                if self.is_full() || policy.caps.at_or_above_hard(items, bytes) {
                    return EnqueueResult::Rejected;
                }

                // Publish bytes then write the item.
                self.bytes_in_queue.fetch_add(*item_bytes, Ordering::AcqRel);
                self.push_raw(item);
                EnqueueResult::Enqueued
            }

            AdmissionDecision::DropNewest => EnqueueResult::DroppedNewest,

            AdmissionDecision::Reject => EnqueueResult::Rejected,

            AdmissionDecision::Block => {
                // Non-blocking test environment: translate to Rejected.
                EnqueueResult::Rejected
            }

            AdmissionDecision::Evict(n) => {
                // Evict up to n oldest items (or fewer if empty).
                for _ in 0..n {
                    if self.len() == 0 {
                        break;
                    }
                    let ev = self.pop_raw();
                    self.bytes_in_queue
                        .fetch_sub(*ev.header().payload_size_bytes(), Ordering::AcqRel);
                }

                // Check if we can now accept the item.
                let items = self.len();
                let bytes = self.bytes_in_queue.load(Ordering::Acquire);
                if self.is_full() || policy.caps.at_or_above_hard(items, bytes) {
                    return EnqueueResult::Rejected;
                }

                self.bytes_in_queue.fetch_add(*item_bytes, Ordering::AcqRel);
                self.push_raw(item);
                EnqueueResult::Enqueued
            }

            AdmissionDecision::EvictUntilBelowHard => {
                // Evict until below hard cap or queue empty.
                while policy
                    .caps
                    .at_or_above_hard(self.len(), self.bytes_in_queue.load(Ordering::Acquire))
                    && self.len() > 0
                {
                    let ev = self.pop_raw();
                    self.bytes_in_queue
                        .fetch_sub(*ev.header().payload_size_bytes(), Ordering::AcqRel);
                }

                // If single item cannot fit in an empty queue, reject.
                if policy.caps.at_or_above_hard(0, *item_bytes) {
                    return EnqueueResult::Rejected;
                }

                // Ensure physical capacity and logical caps satisfied.
                let items = self.len();
                let bytes = self.bytes_in_queue.load(Ordering::Acquire);
                if self.is_full() || policy.caps.at_or_above_hard(items, bytes) {
                    return EnqueueResult::Rejected;
                }

                self.bytes_in_queue.fetch_add(*item_bytes, Ordering::AcqRel);
                self.push_raw(item);
                EnqueueResult::Enqueued
            }
        }
    }

    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        if self.len() == 0 {
            return Err(QueueError::Empty);
        }
        let item = self.pop_raw();
        self.bytes_in_queue
            .fetch_sub(*item.header().payload_size_bytes(), Ordering::AcqRel);
        Ok(item)
    }

    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        let items = self.len();
        let bytes = self.bytes_in_queue.load(Ordering::Acquire);
        let watermark = policy.watermark(items, bytes);
        EdgeOccupancy {
            items,
            bytes,
            watermark,
        }
    }

    fn try_peek(&self) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        if self.len() == 0 {
            return Err(QueueError::Empty);
        }
        let h = self.head.load(Ordering::Acquire);
        let idx = h & self.mask();
        let base: *const MaybeUninit<Self::Item> = self.buf.as_ptr();
        let slot: *const Self::Item = unsafe { base.add(idx) as *const Self::Item };
        // SAFETY: Producer writes only after consumer advances `head`. Under the
        // single-producer/single-consumer discipline this reference is valid for
        // the borrow of &self.
        let r = unsafe { &*slot };
        Ok(crate::edge::PeekResponse::Borrowed(r))
    }

    #[inline]
    fn try_peek_at(
        &self,
        index: usize,
    ) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        let available = self.len();
        if index >= available {
            return Err(QueueError::Empty);
        }

        // SAFETY: Under strict SPSC discipline, the slot at `head + index` is valid
        // for the duration of this borrow, provided the consumer does not advance head
        // past it during the borrow (which is the caller’s responsibility under SPSC).
        let r: &Self::Item = self.peek_ref_at_offset(index);
        Ok(crate::edge::PeekResponse::Borrowed(r))
    }

    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        use crate::policy::WindowKind;

        let available = self.len();
        if available == 0 {
            return Err(QueueError::Empty);
        }

        let fixed_opt = *policy.fixed_n();
        let delta_t_opt = *policy.max_delta_t();
        let window_kind = policy.window_kind();

        // If both caps are absent, treat as fixed_n = 1.
        let effective_fixed: Option<usize> = if fixed_opt.is_none() && delta_t_opt.is_none() {
            Some(1)
        } else {
            fixed_opt
        };

        // Compute how many items are within max_delta_t relative to the front, if any.
        let mut delta_count = available;
        if let Some(cap) = delta_t_opt {
            let front_ticks = *self.peek_ref_at_offset(0).header().creation_tick();
            let mut c = 0usize;
            while c < available {
                let tick = *self.peek_ref_at_offset(c).header().creation_tick();
                let delta = tick.saturating_sub(front_ticks);
                if delta <= cap {
                    c += 1;
                } else {
                    break;
                }
            }
            delta_count = c;
        }

        // Apply effective fixed-N cap if present.
        let apply_fixed = |limit: usize| -> usize {
            if let Some(n) = effective_fixed {
                core::cmp::min(limit, n)
            } else {
                limit
            }
        };

        // --- Disjoint windows: pop up to fixed / delta_count.
        if let WindowKind::Disjoint = window_kind {
            let take_n = apply_fixed(core::cmp::min(available, delta_count));
            if take_n == 0 {
                return Err(QueueError::Empty);
            }

            let mut out: alloc::vec::Vec<Self::Item> = alloc::vec::Vec::with_capacity(take_n);
            for _ in 0..take_n {
                let item = self.pop_raw();
                self.bytes_in_queue
                    .fetch_sub(*item.header().payload_size_bytes(), Ordering::AcqRel);
                out.push(item);
            }

            return Ok(BatchView::from_owned(out));
        }

        // --- Sliding windows: present `size` but pop `stride`.
        if let WindowKind::Sliding(sw) = window_kind {
            let stride = *sw.stride();
            let size = effective_fixed.unwrap_or(1);

            let mut max_present = core::cmp::min(available, size);
            max_present = apply_fixed(core::cmp::min(max_present, delta_count));

            if max_present == 0 {
                return Err(QueueError::Empty);
            }

            let stride_to_pop = core::cmp::min(stride, available);

            let mut out: alloc::vec::Vec<Self::Item> = alloc::vec::Vec::with_capacity(max_present);

            // Pop (move) the first `stride_to_pop` items.
            for _ in 0..stride_to_pop {
                let item = self.pop_raw();
                self.bytes_in_queue
                    .fetch_sub(*item.header().payload_size_bytes(), Ordering::AcqRel);
                out.push(item);
            }

            // For the remainder, clone from the queue without advancing head.
            // These are "peeked" items for sliding semantics.
            for i in stride_to_pop..max_present {
                let cloned = self.peek_ref_at_offset(i - stride_to_pop).clone();
                out.push(cloned);
            }

            return Ok(BatchView::from_owned(out));
        }

        // --- Fixed-N and/or max_delta_t (non-sliding, non-disjoint).
        let mut take_n = core::cmp::min(available, delta_count);
        take_n = apply_fixed(take_n);

        if take_n == 0 {
            return Err(QueueError::Empty);
        }

        let mut out: alloc::vec::Vec<Self::Item> = alloc::vec::Vec::with_capacity(take_n);
        for _ in 0..take_n {
            let item = self.pop_raw();
            self.bytes_in_queue
                .fetch_sub(*item.header().payload_size_bytes(), Ordering::AcqRel);
            out.push(item);
        }

        Ok(BatchView::from_owned(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::message::Message;

    /// Build a fresh SpscAtomicRing with a power-of-two backing capacity.
    ///
    /// Note: this ring uses the classic "one slot empty" scheme, so usable
    /// capacity is `cap - 1`. Pick a value large enough for the contract tests.
    fn make_ring() -> SpscAtomicRing<Message<u32>> {
        // Needs to be power-of-two; usable capacity is 31 here.
        const CAPACITY: usize = 32;

        // SAFETY:
        // - We only use the queue under single-threaded test execution (SPSC discipline).
        // - Capacity is power-of-two.
        // - Contract tests do not attempt to read from empty or write to full without
        //   going through the queue API.
        unsafe { SpscAtomicRing::<Message<u32>>::with_capacity(CAPACITY) }
    }

    // Run the full Edge contract suite against SpscAtomicRing<Message<u32>>.
    crate::run_edge_contract_tests!(spsc_atomic_ring_contract, || make_ring());
}
