//! SpscRingbuf: safe wrapper over the `ringbuf` crate (P2 default).
//!
//! Feature gates: `std` and `queue_ringbuf` (enables optional `ringbuf` dep).

use ringbuf::traits::{
    consumer::Consumer as _, observer::Observer as _, producer::Producer as _, Split as _,
};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use crate::edge::{Edge, EdgeOccupancy, EnqueueResult, PeekResponse};
use crate::errors::QueueError;
use crate::message::{payload::Payload, Message};
use crate::policy::{AdmissionPolicy, EdgePolicy, WatermarkState, WindowKind};
use crate::prelude::BatchView;
use crate::types::Ticks;

/// A single-producer single-consumer (SPSC) queue backed by the [`ringbuf`] crate.
///
/// This implementation wraps [`HeapRb`] to provide a safe, bounded ring buffer with
/// additional accounting for item count and payload size in bytes. It enforces
/// capacity constraints and admission policies defined by [`EdgePolicy`].
///
/// Intended as the default SPSC queue for Limen (P2).
pub struct SpscRingbuf<T> {
    prod: HeapProd<T>,
    cons: HeapCons<T>,
    cap: usize,
    bytes: usize,
}

impl<T> SpscRingbuf<T> {
    /// Create with capacity (items).
    /// The underlying `HeapRb` does not require power-of-two,
    /// but we keep `next_power_of_two()` to align with other queues.
    pub fn with_capacity(capacity: usize) -> Self {
        let rb = HeapRb::<T>::new(capacity.next_power_of_two());
        let (prod, cons) = rb.split();
        Self {
            prod,
            cons,
            cap: capacity,
            bytes: 0,
        }
    }

    #[inline]
    fn len_internal(&self) -> usize {
        // Number of elements available to consume
        self.cons.occupied_len()
    }

    #[inline]
    fn is_full(&self) -> bool {
        // Logical full if we've hit configured cap or producer reports full.
        self.len_internal() >= self.cap || self.prod.is_full()
    }
}

impl<P: Payload + std::clone::Clone> Edge for SpscRingbuf<Message<P>> {
    type Item = Message<P>;

    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        let items = self.len_internal();
        let bytes = self.bytes;

        if policy.caps.at_or_above_hard(items, bytes) && self.is_full() {
            return EnqueueResult::Rejected;
        }

        match policy.watermark(items, bytes) {
            WatermarkState::BetweenSoftAndHard
                if matches!(policy.admission, AdmissionPolicy::DropOldest)
                    && self.len_internal() > 0 =>
            {
                if let Some(ev) = self.cons.try_pop() {
                    self.bytes = self.bytes.saturating_sub(*ev.header().payload_size_bytes());
                }
            }
            _ => {}
        }

        if self.is_full() {
            return EnqueueResult::Rejected;
        }

        let payload_bytes = *item.header().payload_size_bytes();
        if let Err(_item_back) = self.prod.try_push(item) {
            return EnqueueResult::Rejected;
        }
        self.bytes = self.bytes.saturating_add(payload_bytes);
        EnqueueResult::Enqueued
    }

    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        match self.cons.try_pop() {
            Some(item) => {
                self.bytes = self
                    .bytes
                    .saturating_sub(*item.header().payload_size_bytes());
                Ok(item)
            }
            None => Err(QueueError::Empty),
        }
    }

    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        let items = self.len_internal();
        let bytes = self.bytes;
        let watermark = policy.watermark(items, bytes);
        EdgeOccupancy {
            items,
            bytes,
            watermark,
        }
    }

    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        match self.cons.try_peek() {
            Some(msg) => Ok(PeekResponse::Borrowed(msg)),
            None => Err(QueueError::Empty),
        }
    }

    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        let available = self.len_internal();
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

        #[inline]
        fn apply_fixed(limit: usize, effective_fixed: Option<usize>) -> usize {
            if let Some(n) = effective_fixed {
                core::cmp::min(limit, n)
            } else {
                limit
            }
        }

        // Helper: get the i-th item in logical order using as_slices().
        #[inline]
        fn get_in_order<'a, T>(a: &'a [T], b: &'a [T], idx: usize) -> &'a T {
            if idx < a.len() {
                &a[idx]
            } else {
                &b[idx - a.len()]
            }
        }

        // Compute how many items are within max_delta_t relative to the front.
        let mut delta_count = available;
        if let Some(cap) = delta_t_opt {
            let (a, b) = self.cons.as_slices();

            let front_ticks: Ticks = *get_in_order(a, b, 0).header().creation_tick();

            let mut c = 0usize;
            while c < available {
                let tick = *get_in_order(a, b, c).header().creation_tick();
                let delta = tick.saturating_sub(front_ticks);
                if delta <= cap {
                    c += 1;
                } else {
                    break;
                }
            }
            delta_count = c;
        }

        // --- Disjoint windows: pop items into an owned Vec.
        if let WindowKind::Disjoint = window_kind {
            let take_n = apply_fixed(core::cmp::min(available, delta_count), effective_fixed);
            if take_n == 0 {
                return Err(QueueError::Empty);
            }

            let mut out = alloc::vec::Vec::with_capacity(take_n);
            for _ in 0..take_n {
                match self.cons.try_pop() {
                    Some(item) => {
                        self.bytes = self
                            .bytes
                            .saturating_sub(*item.header().payload_size_bytes());
                        out.push(item);
                    }
                    None => break,
                }
            }

            if out.is_empty() {
                return Err(QueueError::Empty);
            }
            return Ok(BatchView::from_owned(out));
        }

        // --- Sliding windows: return `size` items (cloned), pop only `stride`.
        if let WindowKind::Sliding(sw) = window_kind {
            let stride = *sw.stride();
            let size = *sw.size();

            // Present is bounded by availability, size, delta_count, and fixed.
            let mut present_n = core::cmp::min(available, size);
            present_n = core::cmp::min(present_n, delta_count);
            present_n = apply_fixed(present_n, effective_fixed);

            if present_n == 0 {
                return Err(QueueError::Empty);
            }

            // Clone the visible window into an owned Vec (covers both popped + peeked).
            let (a, b) = self.cons.as_slices();
            let mut out = alloc::vec::Vec::with_capacity(present_n);
            for i in 0..present_n {
                out.push(get_in_order(a, b, i).clone());
            }

            // Now advance only stride items.
            let stride_to_pop = core::cmp::min(stride, available);
            for _ in 0..stride_to_pop {
                match self.cons.try_pop() {
                    Some(item) => {
                        self.bytes = self
                            .bytes
                            .saturating_sub(*item.header().payload_size_bytes());
                    }
                    None => break,
                }
            }

            return Ok(BatchView::from_owned(out));
        }

        // --- Fixed-N and/or max_delta_t (non-sliding, non-disjoint).
        let mut take_n = core::cmp::min(available, delta_count);
        take_n = apply_fixed(take_n, effective_fixed);

        if take_n == 0 {
            return Err(QueueError::Empty);
        }

        let mut out = alloc::vec::Vec::with_capacity(take_n);
        for _ in 0..take_n {
            match self.cons.try_pop() {
                Some(item) => {
                    self.bytes = self
                        .bytes
                        .saturating_sub(*item.header().payload_size_bytes());
                    out.push(item);
                }
                None => break,
            }
        }

        if out.is_empty() {
            return Err(QueueError::Empty);
        }

        Ok(BatchView::from_owned(out))
    }
}
