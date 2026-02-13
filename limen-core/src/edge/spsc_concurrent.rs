//! Concurrent Queue generic impl, TODO: update doc comment.

use std::sync::{Arc, Mutex};

use crate::edge::{Edge, EdgeOccupancy, EnqueueResult, PeekResponse};
use crate::errors::QueueError;
use crate::message::{payload::Payload, Message};
use crate::policy::{EdgePolicy, WatermarkState};
use crate::prelude::BatchView;

/// Thread-safe wrapper: makes ANY `Q: SpscQueue` cloneable + `Send + 'static`.
pub struct ConcurrentQueue<Q> {
    inner: Arc<Mutex<Q>>,
}

impl<Q> ConcurrentQueue<Q> {
    /// Creates a new ConcurrentQueue from the given queue.
    pub fn new(inner: Q) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Creates a new ConcurrentQueue from the given Arc<Mutex<queue>>.
    pub fn from_arc(inner: Arc<Mutex<Q>>) -> Self {
        Self { inner }
    }

    /// Returns an arc clone of the inner queue.
    pub fn arc(&self) -> Arc<Mutex<Q>> {
        Arc::clone(&self.inner)
    }
}

impl<P, Q> Edge for ConcurrentQueue<Q>
where
    P: Payload + Clone,
    Q: Edge<Item = Message<P>> + Send + 'static,
{
    type Item = Message<P>;

    #[inline]
    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        match self.inner.lock() {
            Ok(mut q) => q.try_push(item, policy),
            Err(_) => EnqueueResult::Rejected,
        }
    }

    #[inline]
    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        match self.inner.lock() {
            Ok(mut q) => q.try_pop(),
            Err(_) => Err(QueueError::Poisoned),
        }
    }

    #[inline]
    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        match self.inner.lock() {
            Ok(q) => q.occupancy(policy),
            Err(_) => EdgeOccupancy {
                items: 0,
                bytes: 0,
                watermark: WatermarkState::AtOrAboveHard,
            },
        }
    }

    #[inline]
    fn try_peek(&self) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        match self.inner.lock() {
            Ok(q) => match q.try_peek() {
                Ok(peek) => match peek {
                    // If inner returned a borrowed reference, clone the item while we hold
                    // the lock and return an owned copy so the returned reference doesn't
                    // outlive the mutex guard.
                    crate::edge::PeekResponse::Borrowed(b) => {
                        let owned = b.clone();
                        Ok(crate::edge::PeekResponse::Owned(owned))
                    }

                    // If inner returned an owned item we can forward it directly.
                    #[cfg(feature = "alloc")]
                    crate::edge::PeekResponse::Owned(o) => Ok(crate::edge::PeekResponse::Owned(o)),
                },
                Err(e) => Err(e),
            },
            Err(_) => Err(QueueError::Poisoned),
        }
    }

    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        match self.inner.lock() {
            Ok(mut q) => {
                let batch_view = q.try_pop_batch(policy)?;

                // Materialize into owned messages while the mutex is held,
                // so no borrowed references escape the lock guard.
                let mut owned: alloc::vec::Vec<Message<P>> =
                    alloc::vec::Vec::with_capacity(batch_view.len());

                for item in batch_view.iter() {
                    owned.push(item.clone());
                }

                Ok(BatchView::from_owned(owned))
            }
            Err(_) => Err(QueueError::Poisoned),
        }
    }
}

impl<Q> Clone for ConcurrentQueue<Q> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Producer endpoint: push + occupancy only.
#[derive(Clone)]
pub struct ProducerEndpoint<P, QWrap>
where
    P: Payload,
    QWrap: Edge<Item = Message<P>> + Send + 'static,
{
    q: QWrap,
    _p: core::marker::PhantomData<P>,
}

impl<P, QWrap> ProducerEndpoint<P, QWrap>
where
    P: Payload,
    QWrap: Edge<Item = Message<P>> + Send + 'static,
{
    /// Creates a new ProducerEndpoint.
    pub fn new(q: QWrap) -> Self {
        Self {
            q,
            _p: core::marker::PhantomData,
        }
    }

    /// Returns thhe inner queue.
    pub fn into_inner(self) -> QWrap {
        self.q
    }
}

impl<P, QWrap> Edge for ProducerEndpoint<P, QWrap>
where
    P: Payload + Clone,
    QWrap: Edge<Item = Message<P>> + Send + 'static,
{
    type Item = Message<P>;
    #[inline]
    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        self.q.try_push(item, policy)
    }
    #[inline]
    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        Err(QueueError::Empty)
    }
    #[inline]
    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        self.q.occupancy(policy)
    }
    #[inline]
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        Err(QueueError::Unsupported)
    }

    fn try_pop_batch(
        &mut self,
        _policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        Err(QueueError::Unsupported)
    }
}

/// Consumer endpoint: pop + occupancy only.
#[derive(Clone)]
pub struct ConsumerEndpoint<P, QWrap>
where
    P: Payload,
    QWrap: Edge<Item = Message<P>> + Send + 'static,
{
    q: QWrap,
    _p: core::marker::PhantomData<P>,
}

impl<P, QWrap> ConsumerEndpoint<P, QWrap>
where
    P: Payload,
    QWrap: Edge<Item = Message<P>> + Send + 'static,
{
    /// Creates a new ConsumerEndpoint.
    pub fn new(q: QWrap) -> Self {
        Self {
            q,
            _p: core::marker::PhantomData,
        }
    }

    /// Returns the inner queue.
    pub fn into_inner(self) -> QWrap {
        self.q
    }
}

impl<P, QWrap> Edge for ConsumerEndpoint<P, QWrap>
where
    P: Payload + Clone,
    QWrap: Edge<Item = Message<P>> + Send + 'static,
{
    type Item = Message<P>;
    #[inline]
    fn try_push(&mut self, _item: Self::Item, _policy: &EdgePolicy) -> EnqueueResult {
        EnqueueResult::Rejected
    }
    #[inline]
    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        self.q.try_pop()
    }
    #[inline]
    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        self.q.occupancy(policy)
    }
    #[inline]
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        self.q.try_peek()
    }
    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        self.q.try_pop_batch(policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::edge::bench::TestSpscRingBuf;

    crate::run_edge_contract_tests!(concurrent_queue_contract, || {
        let inner = TestSpscRingBuf::<Message<u32>, 16>::new();
        ConcurrentQueue::new(inner)
    });

    mod endpoint_tests {
        use super::*;

        use crate::errors::QueueError;
        use crate::message::{Message, MessageHeader};
        use crate::policy::{AdmissionPolicy, EdgePolicy, OverBudgetAction, QueueCaps};
        use crate::types::Ticks;

        const POLICY: EdgePolicy = EdgePolicy::new(
            QueueCaps::new(8, 6, None, None),
            AdmissionPolicy::DropNewest,
            OverBudgetAction::Drop,
        );

        fn msg(tick: u64) -> Message<u32> {
            let mut header = MessageHeader::empty();
            header.set_creation_tick(Ticks::new(tick));
            Message::new(header, 0u32)
        }

        #[test]
        fn producer_endpoint_is_write_only() {
            let inner = ConcurrentQueue::new(TestSpscRingBuf::<Message<u32>, 16>::new());
            let mut producer = ProducerEndpoint::<u32, _>::new(inner);

            assert_eq!(
                producer.try_push(msg(1), &POLICY),
                crate::edge::EnqueueResult::Enqueued
            );
            assert!(matches!(producer.try_pop(), Err(QueueError::Empty)));
            assert!(matches!(producer.try_peek(), Err(QueueError::Unsupported)));
            assert!(matches!(
                producer.try_pop_batch(&crate::policy::BatchingPolicy::fixed(2)),
                Err(QueueError::Unsupported)
            ));
        }

        #[test]
        fn consumer_endpoint_is_read_only() {
            let mut q = ConcurrentQueue::new(TestSpscRingBuf::<Message<u32>, 16>::new());
            assert_eq!(
                q.try_push(msg(1), &POLICY),
                crate::edge::EnqueueResult::Enqueued
            );

            let mut consumer = ConsumerEndpoint::<u32, _>::new(q);

            assert_eq!(
                consumer.try_push(msg(2), &POLICY),
                crate::edge::EnqueueResult::Rejected
            );

            let peek = consumer.try_peek().expect("peek");
            assert_eq!((*peek.as_ref().header().creation_tick()).as_u64(), &1u64);

            let popped = consumer.try_pop().expect("pop");
            assert_eq!((*popped.header().creation_tick()).as_u64(), &1u64);
        }
    }
}
