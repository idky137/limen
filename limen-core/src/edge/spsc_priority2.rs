//! Two-lane priority wrapper for SPSC queues (feature: `priority_lanes`).
//!
//! This composes two underlying SPSC queues (hi/lo) that store the same `Item`
//! and routes `try_push` by inspecting the message header's QoS class. `try_pop`
//! always prefers the high-priority lane when available.

use crate::edge::{Edge, EdgeOccupancy, EnqueueResult};
use crate::errors::QueueError;
use crate::message::{payload::Payload, Message};
use crate::policy::EdgePolicy;
use crate::prelude::BatchView;
use crate::types::QoSClass;

extern crate alloc;

/// Two-lane priority queue.
pub struct Priority2<QHi, QLo, P>
where
    QHi: Edge<Item = Message<P>>,
    QLo: Edge<Item = Message<P>>,
    P: Payload,
{
    hi: QHi,
    lo: QLo,
    _p: core::marker::PhantomData<P>,
}

impl<QHi, QLo, P> Priority2<QHi, QLo, P>
where
    QHi: Edge<Item = Message<P>>,
    QLo: Edge<Item = Message<P>>,
    P: Payload,
{
    /// Build a two-lane priority queue from hi/lo queues.
    pub fn new(hi: QHi, lo: QLo) -> Self {
        Self {
            hi,
            lo,
            _p: core::marker::PhantomData,
        }
    }
}

impl<QHi, QLo, P> Edge for Priority2<QHi, QLo, P>
where
    QHi: Edge<Item = Message<P>>,
    QLo: Edge<Item = Message<P>>,
    P: Payload + Clone,
{
    type Item = Message<P>;

    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        match item.header().qos() {
            QoSClass::LatencyCritical => self.hi.try_push(item, policy),
            _ => self.lo.try_push(item, policy),
        }
    }

    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        match self.hi.try_pop() {
            Ok(it) => Ok(it),
            Err(QueueError::Empty) => self.lo.try_pop(),
            Err(e) => Err(e),
        }
    }

    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        let hi = self.hi.occupancy(policy);
        let lo = self.lo.occupancy(policy);
        let items = hi.items + lo.items;
        let bytes = hi.bytes + lo.bytes;
        let watermark = policy.watermark(items, bytes);
        EdgeOccupancy {
            items,
            bytes,
            watermark,
        }
    }

    fn try_peek(&self) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        match self.hi.try_peek() {
            Ok(pr) => Ok(pr),
            Err(QueueError::Empty) => self.lo.try_peek(),
            Err(e) => Err(e),
        }
    }

    #[inline]
    fn try_peek_at(
        &self,
        index: usize,
    ) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        match self.hi.try_peek_at(index) {
            Ok(pr) => Ok(pr),
            Err(QueueError::Empty) => self.lo.try_peek_at(index),
            Err(e) => Err(e),
        }
    }

    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        match self.hi.try_pop_batch(policy) {
            Ok(batch) => Ok(batch),
            Err(QueueError::Empty) => self.lo.try_pop_batch(policy),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::edge::bench::TestSpscRingBuf;
    use crate::message::{Message, MessageHeader};
    use crate::policy::{AdmissionPolicy, EdgePolicy, OverBudgetAction, QueueCaps};
    use crate::types::{QoSClass, Ticks};

    const POLICY: EdgePolicy = EdgePolicy::new(
        QueueCaps::new(8, 6, None, None),
        AdmissionPolicy::DropNewest,
        OverBudgetAction::Drop,
    );

    fn msg_with_qos(tick: u64, qos: QoSClass) -> Message<u32> {
        let mut header = MessageHeader::empty();
        header.set_creation_tick(Ticks::new(tick));
        header.set_qos(qos);
        Message::new(header, 0u32)
    }

    // --- 1) Run the full Edge contract suite against Priority2 ---
    crate::run_edge_contract_tests!(priority2_edge_contract, || {
        let hi = TestSpscRingBuf::<Message<u32>, 16>::new();
        let lo = TestSpscRingBuf::<Message<u32>, 16>::new();
        Priority2::<_, _, u32>::new(hi, lo)
    });

    // --- 2) Priority-specific behaviour tests ---

    #[test]
    fn routes_latency_critical_to_hi_and_others_to_lo() {
        let hi = TestSpscRingBuf::<Message<u32>, 16>::new();
        let lo = TestSpscRingBuf::<Message<u32>, 16>::new();
        let mut q = Priority2::<_, _, u32>::new(hi, lo);

        assert_eq!(
            q.try_push(msg_with_qos(1, QoSClass::LatencyCritical), &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );
        assert_eq!(
            q.try_push(msg_with_qos(2, QoSClass::BestEffort), &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );

        // `try_pop` should return hi first (tick=1), then lo (tick=2)
        let a = q.try_pop().expect("pop hi");
        let b = q.try_pop().expect("pop lo");
        assert_eq!((*a.header().creation_tick()).as_u64(), &1u64);
        assert_eq!((*b.header().creation_tick()).as_u64(), &2u64);
    }

    #[test]
    fn peek_prefers_hi_when_both_non_empty() {
        let hi = TestSpscRingBuf::<Message<u32>, 16>::new();
        let lo = TestSpscRingBuf::<Message<u32>, 16>::new();
        let mut q = Priority2::<_, _, u32>::new(hi, lo);

        assert_eq!(
            q.try_push(msg_with_qos(10, QoSClass::BestEffort), &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );
        assert_eq!(
            q.try_push(msg_with_qos(20, QoSClass::LatencyCritical), &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );

        let peek = q.try_peek().expect("peek");
        assert_eq!((*peek.as_ref().header().creation_tick()).as_u64(), &20u64);

        // Ensure peek did not remove it.
        let popped = q.try_pop().expect("pop");
        assert_eq!((*popped.header().creation_tick()).as_u64(), &20u64);
    }

    #[test]
    fn pop_batch_prefers_hi_when_non_empty() {
        let hi = TestSpscRingBuf::<Message<u32>, 16>::new();
        let lo = TestSpscRingBuf::<Message<u32>, 16>::new();
        let mut q = Priority2::<_, _, u32>::new(hi, lo);

        // lo: 1,2,3
        for t in 1..=3 {
            assert_eq!(
                q.try_push(msg_with_qos(t, QoSClass::BestEffort), &POLICY),
                crate::edge::EnqueueResult::Enqueued
            );
        }

        // hi: 100,101
        for t in 100..=101 {
            assert_eq!(
                q.try_push(msg_with_qos(t, QoSClass::LatencyCritical), &POLICY),
                crate::edge::EnqueueResult::Enqueued
            );
        }

        let batch_policy = crate::policy::BatchingPolicy::fixed(4);

        // Should batch from hi lane first (only 2 items available there).
        let batch = q.try_pop_batch(&batch_policy).expect("batch");
        let ticks: alloc::vec::Vec<u64> = batch
            .as_batch()
            .iter()
            .map(|m| *(m.header().creation_tick()).as_u64())
            .collect();
        assert_eq!(ticks.as_slice(), &[100, 101]);

        // Remaining should be lo items in order.
        let a = q.try_pop().expect("lo-1");
        let b = q.try_pop().expect("lo-2");
        let c = q.try_pop().expect("lo-3");
        assert_eq!((*a.header().creation_tick()).as_u64(), &1u64);
        assert_eq!((*b.header().creation_tick()).as_u64(), &2u64);
        assert_eq!((*c.header().creation_tick()).as_u64(), &3u64);
    }

    #[test]
    fn occupancy_is_sum_of_lanes() {
        let hi = TestSpscRingBuf::<Message<u32>, 16>::new();
        let lo = TestSpscRingBuf::<Message<u32>, 16>::new();
        let mut q = Priority2::<_, _, u32>::new(hi, lo);

        assert_eq!(
            q.try_push(msg_with_qos(1, QoSClass::LatencyCritical), &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );
        assert_eq!(
            q.try_push(msg_with_qos(2, QoSClass::BestEffort), &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );

        let occ = q.occupancy(&POLICY);
        assert_eq!(*occ.items(), 2usize);

        // bytes depends on MessageHeader sizing + payload descriptor; don’t assert exact value,
        // but should be non-zero for real messages.
        assert!(*occ.bytes() > 0usize);
    }

    #[test]
    fn lane_specific_admission_interactions_smoke_test() {
        // This test checks that hi/lo lane policies can diverge *by construction* of the lanes.
        // Priority2 forwards the *same* EdgePolicy to whichever lane it routes into.
        //
        // If you later decide that hi/lo should have distinct policies, this is where you’d assert it.
        let hi = TestSpscRingBuf::<Message<u32>, 2>::new();
        let lo = TestSpscRingBuf::<Message<u32>, 2>::new();
        let mut q = Priority2::<_, _, u32>::new(hi, lo);

        let small_caps_policy = EdgePolicy::new(
            QueueCaps::new(2, 1, None, None),
            AdmissionPolicy::DropOldest,
            OverBudgetAction::Drop,
        );

        assert_eq!(
            q.try_push(
                msg_with_qos(1, QoSClass::LatencyCritical),
                &small_caps_policy
            ),
            crate::edge::EnqueueResult::Enqueued
        );
        assert_eq!(
            q.try_push(
                msg_with_qos(2, QoSClass::LatencyCritical),
                &small_caps_policy
            ),
            crate::edge::EnqueueResult::Enqueued
        );

        // With DropOldest and soft_items=1, pushing the 2nd item put it between soft/hard and should evict oldest.
        // After two pushes, only the newest should remain in hi lane.
        let popped = q.try_pop().expect("pop");
        assert_eq!((*popped.header().creation_tick()).as_u64(), &2u64);
    }
}
