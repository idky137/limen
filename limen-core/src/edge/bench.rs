//! (Work)bench [test] Queue implementation.

use crate::edge::{EnqueueResult, QueueOccupancy, SpscQueue};
use crate::errors::QueueError;
use crate::policy::{AdmissionPolicy, EdgePolicy, OverBudgetAction, QueueCaps, WatermarkState};

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
    /// Circular storage for enqueued items.
    /// A slot is `Some(T)` when occupied and `None` when free.
    buf: [Option<T>; N],
    /// Read cursor: index of the next element to be popped.
    /// Advances modulo `N` after every successful `try_pop`.
    head: usize,
    /// Write cursor: index of the next free slot to write to.
    /// Advances modulo `N` after every successful enqueue.
    tail: usize,
    /// Current number of items stored in the buffer (0..=N).
    /// Used for fast emptiness/fullness checks and occupancy reporting.
    len: usize,
}

impl<T: Clone, const N: usize> TestSpscRingBuf<T, N> {
    #[inline]
    fn item_size_bytes() -> usize {
        core::mem::size_of::<T>()
    }

    #[inline]
    fn current_bytes(&self) -> usize {
        self.len.saturating_mul(Self::item_size_bytes())
    }

    #[inline]
    fn watermark_state(&self, caps: &QueueCaps) -> WatermarkState {
        let items = self.len;
        let bytes = self.current_bytes();
        if caps.at_or_above_hard(items, bytes) {
            WatermarkState::AtOrAboveHard
        } else if caps.below_soft(items, bytes) {
            WatermarkState::BelowSoft
        } else {
            WatermarkState::BetweenSoftAndHard
        }
    }

    /// Creates an empty ring buffer.
    pub fn new() -> Self {
        // Runtime init avoids requiring `T: Copy`.
        Self {
            buf: core::array::from_fn(|_| None),
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    /// Push, evicting the oldest item if full.
    #[inline]
    fn push_overwrite(&mut self, item: T) {
        self.buf[self.tail] = Some(item);
        self.tail = (self.tail + 1) % N;
        if self.len == N {
            self.head = (self.head + 1) % N; // drop oldest
        } else {
            self.len += 1;
        }
    }

    /// Handle a hard-cap situation using `EdgePolicy`.
    #[inline]
    fn handle_hard_cap(&mut self, item: T, policy: &EdgePolicy) -> EnqueueResult {
        match policy.admission {
            AdmissionPolicy::DropNewest => EnqueueResult::DroppedNewest,
            AdmissionPolicy::DropOldest => {
                self.push_overwrite(item);
                EnqueueResult::Enqueued
            }
            AdmissionPolicy::Block => EnqueueResult::Rejected, // no blocking in tests
            AdmissionPolicy::DeadlineAndQoSAware => match policy.over_budget {
                OverBudgetAction::Drop => EnqueueResult::DroppedNewest,
                OverBudgetAction::SkipStage
                | OverBudgetAction::Degrade
                | OverBudgetAction::DefaultOnTimeout => EnqueueResult::Rejected,
            },
        }
    }
}

impl<T: Clone, const N: usize> SpscQueue for TestSpscRingBuf<T, N> {
    type Item = T;

    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        let items = self.len;
        let bytes = self.current_bytes();

        if policy.caps.at_or_above_hard(items, bytes) {
            return self.handle_hard_cap(item, policy);
        }

        if policy
            .caps
            .at_or_above_hard(items + 1, bytes.saturating_add(Self::item_size_bytes()))
        {
            return self.handle_hard_cap(item, policy);
        }

        self.buf[self.tail] = Some(item);
        self.tail = (self.tail + 1) % N;
        self.len += 1;
        EnqueueResult::Enqueued
    }

    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        if self.len == 0 {
            return Err(QueueError::Empty);
        }
        let idx = self.head;
        let item = self.buf[idx].take().expect("len>0 guarantees Some");
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Ok(item)
    }

    fn occupancy(&self, policy: &EdgePolicy) -> QueueOccupancy {
        let items = self.len;
        let bytes = self.current_bytes();
        let watermark = self.watermark_state(&policy.caps);
        QueueOccupancy {
            items,
            bytes,
            watermark,
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn try_peek(&self) -> Result<&Self::Item, QueueError> {
        if self.len == 0 {
            return Err(QueueError::Empty);
        }
        Ok(self.buf[self.head].as_ref().expect("len>0 guarantees Some"))
    }
}

impl<T, const N: usize> Default for TestSpscRingBuf<T, N> {
    fn default() -> Self {
        Self {
            buf: core::array::from_fn(|_| None),
            head: 0,
            tail: 0,
            len: 0,
        }
    }
}
