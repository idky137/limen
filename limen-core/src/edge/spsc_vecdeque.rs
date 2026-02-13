//! Heap-backed SPSC ring buffer for P1 (no_std + alloc), **safe version**.
//!
//! Uses VecDeque as the backing ring; we enforce a fixed logical capacity and
//! keep byte occupancy accounting for admission / watermark decisions.

use alloc::collections::VecDeque;

use crate::edge::{Edge, EdgeOccupancy, EnqueueResult, PeekResponse};
use crate::errors::QueueError;
use crate::message::{payload::Payload, Message};
use crate::policy::{AdmissionDecision, EdgePolicy, WindowKind};
use crate::prelude::BatchView;
use crate::types::Ticks;

/// Heap ring with fixed item capacity.
pub struct HeapRing<T> {
    buf: VecDeque<T>,
    cap: usize,
    bytes: usize,
}

impl<T> HeapRing<T> {
    /// Create a new ring with the given fixed capacity in items.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap),
            cap,
            bytes: 0,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.buf.len()
    }
    #[inline]
    fn is_full(&self) -> bool {
        self.len() >= self.cap
    }
}

impl<P: Payload + Clone> Edge for HeapRing<Message<P>> {
    type Item = Message<P>;

    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        // Ask the policy what to do (pure decision).
        let decision = self.get_admission_decision(policy, &item);

        // Incoming item size.
        let item_bytes = *item.header().payload_size_bytes();

        match decision {
            AdmissionDecision::Admit => {
                // Ensure physical capacity and logical hard-cap satisfied.
                if self.is_full() || policy.caps.at_or_above_hard(self.len(), self.bytes) {
                    return EnqueueResult::Rejected;
                }

                self.bytes = self.bytes.saturating_add(item_bytes);
                self.buf.push_back(item);
                EnqueueResult::Enqueued
            }

            AdmissionDecision::DropNewest => EnqueueResult::DroppedNewest,

            AdmissionDecision::Reject => EnqueueResult::Rejected,

            AdmissionDecision::Block => {
                // This P1 test ring cannot block; translate to Reject.
                EnqueueResult::Rejected
            }

            AdmissionDecision::Evict(n) => {
                for _ in 0..n {
                    if let Some(ev) = self.buf.pop_front() {
                        self.bytes = self.bytes.saturating_sub(*ev.header().payload_size_bytes());
                    } else {
                        break;
                    }
                }

                // After eviction, ensure we can accept the item.
                if policy.caps.at_or_above_hard(self.len(), self.bytes) || self.is_full() {
                    return EnqueueResult::Rejected;
                }

                self.bytes = self.bytes.saturating_add(item_bytes);
                self.buf.push_back(item);
                EnqueueResult::Enqueued
            }

            AdmissionDecision::EvictUntilBelowHard => {
                while policy.caps.at_or_above_hard(self.len(), self.bytes) && !self.buf.is_empty() {
                    if let Some(ev) = self.buf.pop_front() {
                        self.bytes = self.bytes.saturating_sub(*ev.header().payload_size_bytes());
                    }
                }

                // If item alone cannot fit under hard caps, reject.
                if policy.caps.at_or_above_hard(0, item_bytes) {
                    return EnqueueResult::Rejected;
                }

                if self.is_full() || policy.caps.at_or_above_hard(self.len(), self.bytes) {
                    return EnqueueResult::Rejected;
                }

                self.bytes = self.bytes.saturating_add(item_bytes);
                self.buf.push_back(item);
                EnqueueResult::Enqueued
            }
        }
    }

    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        match self.buf.pop_front() {
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
        let items = self.len();
        let bytes = self.bytes;
        let watermark = policy.watermark(items, bytes);
        EdgeOccupancy {
            items,
            bytes,
            watermark,
        }
    }
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        match self.buf.front() {
            Some(msg) => Ok(PeekResponse::Borrowed(msg)),
            None => Err(QueueError::Empty),
        }
    }

    /// Peek at the item at logical position `index` from the front without removing it.
    ///
    /// - `index == 0` is equivalent to `try_peek()`.
    /// - Returns `QueueError::Empty` if the queue is empty **or** if `index >= len`.
    ///
    /// This is used by schedulers/contexts to check `(fixed_n, max_delta_t)` readiness
    /// without mutating the queue.
    #[inline]
    fn try_peek_at(&self, index: usize) -> Result<PeekResponse<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        match self.buf.get(index) {
            Some(item) => Ok(PeekResponse::Borrowed(item)),
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
        let len = self.len();
        if len == 0 {
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
        let mut delta_count = len;
        if let Some(cap) = delta_t_opt {
            // front creation tick
            let front_ticks: Ticks = *self.buf.front().expect("len > 0").header().creation_tick();
            let mut c = 0usize;
            for m in self.buf.iter() {
                let tick = *m.header().creation_tick();
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
            let take_n = apply_fixed(core::cmp::min(self.len(), delta_count));
            if take_n == 0 {
                return Err(QueueError::Empty);
            }

            let mut out: alloc::vec::Vec<Message<P>> = alloc::vec::Vec::with_capacity(take_n);
            let mut popped_bytes = 0usize;
            for _ in 0..take_n {
                if let Some(item) = self.buf.pop_front() {
                    popped_bytes = popped_bytes.saturating_add(*item.header().payload_size_bytes());
                    out.push(item);
                } else {
                    break;
                }
            }
            self.bytes = self.bytes.saturating_sub(popped_bytes);
            return Ok(BatchView::from_owned(out));
        }

        // --- Sliding windows: present `size` but pop `stride`.
        if let WindowKind::Sliding(sw) = window_kind {
            let stride = *sw.stride();
            let size = *sw.size();

            // Determine how many items we can present, bounded by availability, size, delta_count, and fixed.
            let mut max_present = core::cmp::min(self.len(), size);
            max_present = apply_fixed(core::cmp::min(max_present, delta_count));

            // How many to actually pop from the front (stride), bounded by availability.
            let stride_to_pop = core::cmp::min(stride, self.len());

            if max_present == 0 {
                return Err(QueueError::Empty);
            }

            let mut out: alloc::vec::Vec<Message<P>> = alloc::vec::Vec::with_capacity(max_present);
            let mut popped_bytes = 0usize;

            // Pop stride_to_pop items (remove from deque)
            for _ in 0..stride_to_pop {
                if let Some(item) = self.buf.pop_front() {
                    popped_bytes = popped_bytes.saturating_add(*item.header().payload_size_bytes());
                    out.push(item);
                }
            }

            // Need to include more items (peek) to reach max_present; clone them without popping.
            let need_more = max_present.saturating_sub(out.len());
            if need_more > 0 {
                // iterate front to take clones of the first `need_more` elements
                for m in self.buf.iter().take(need_more) {
                    out.push(m.clone());
                }
            }

            self.bytes = self.bytes.saturating_sub(popped_bytes);
            return Ok(BatchView::from_owned(out));
        }

        // --- Fixed-N and/or max_delta_t (non-sliding, non-disjoint).
        let mut take_n = core::cmp::min(self.len(), delta_count);
        take_n = apply_fixed(take_n);

        if take_n == 0 {
            return Err(QueueError::Empty);
        }

        let mut out: alloc::vec::Vec<Message<P>> = alloc::vec::Vec::with_capacity(take_n);
        let mut popped_bytes = 0usize;
        for _ in 0..take_n {
            if let Some(item) = self.buf.pop_front() {
                popped_bytes = popped_bytes.saturating_add(*item.header().payload_size_bytes());
                out.push(item);
            } else {
                break;
            }
        }
        self.bytes = self.bytes.saturating_sub(popped_bytes);
        Ok(BatchView::from_owned(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::run_edge_contract_tests!(heap_ring_contract, || {
        HeapRing::<Message<u32>>::with_capacity(16)
    });
}
