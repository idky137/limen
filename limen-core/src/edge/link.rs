//! Edge graph-link and descriptor types.

#[cfg(feature = "std")]
use crate::edge::PeekResponse;
use crate::{
    edge::{Edge, EdgeOccupancy, EnqueueResult},
    errors::QueueError,
    message::{payload::Payload, Message},
    policy::EdgePolicy,
    prelude::BatchView,
    types::{EdgeIndex, PortId},
};

/// A lightweight descriptor that **links to** the concrete queue instance
/// backing a graph edge, along with its routing and policy metadata.
///
/// Unlike a pure descriptor, `EdgeLink` **borrows** (`&'a mut`) the queue
/// implementation. This keeps it zero-alloc and allows direct, policy-aware
/// operations on the buffer.
///
/// - `'a`: lifetime of the borrowed queue
/// - `Q`: concrete queue type implementing `SpscQueue<Item = Message<P>>`
/// - `P`: message payload type
#[non_exhaustive]
#[derive(Debug)]
pub struct EdgeLink<Q, P>
where
    P: Payload,
    Q: Edge<Item = Message<P>>,
{
    /// Borrowed handle to the concrete queue instance for this edge.
    queue: Q,

    /// Unique identifier of this edge in the graph.
    id: EdgeIndex,

    /// Upstream node's output port.
    upstream_port: PortId,

    /// Downstream node's input port.
    downstream_port: PortId,

    /// Admission and scheduling policy applied to this edge.
    policy: EdgePolicy,

    /// Optional static name used for diagnostics or graph tooling.
    name: Option<&'static str>,

    /// Marker to bind `P` without storing it.
    _payload_marker: core::marker::PhantomData<P>,
}

impl<Q, P> EdgeLink<Q, P>
where
    P: Payload,
    Q: Edge<Item = Message<P>>,
{
    /// Construct a new `EdgeLink` that borrows the given queue and records its metadata.
    #[inline]
    pub fn new(
        queue: Q,
        id: EdgeIndex,
        upstream_port: PortId,
        downstream_port: PortId,
        policy: EdgePolicy,
        name: Option<&'static str>,
    ) -> Self {
        Self {
            queue,
            id,
            upstream_port,
            downstream_port,
            policy,
            name,
            _payload_marker: core::marker::PhantomData,
        }
    }

    /// Get a reference to the inner queue.
    #[inline]
    pub fn queue(&self) -> &Q {
        &self.queue
    }

    /// Get a mutable reference to the inner queue
    #[inline]
    pub fn queue_mut(&mut self) -> &mut Q {
        &mut self.queue
    }

    /// Get the unique identifier of this edge.
    #[inline]
    pub fn id(&self) -> &EdgeIndex {
        &self.id
    }

    /// Get the upstream output port index.
    #[inline]
    pub fn upstream_port(&self) -> &PortId {
        &self.upstream_port
    }

    /// Get the downstream input port index.
    #[inline]
    pub fn downstream_port(&self) -> &PortId {
        &self.downstream_port
    }

    /// Get the edge policy applied to this queue.
    #[inline]
    pub fn policy(&self) -> &EdgePolicy {
        &self.policy
    }

    /// Get the optional static name of this queue link.
    #[inline]
    pub fn name(&self) -> Option<&'static str> {
        self.name
    }

    /// Return the `EdgeDescriptor` for this `EdgeLink`.
    #[inline]
    pub fn descriptor(&self) -> EdgeDescriptor {
        EdgeDescriptor {
            id: self.id,
            upstream: self.upstream_port,
            downstream: self.downstream_port,
            name: self.name,
        }
    }
}

impl<Q, P> Edge for EdgeLink<Q, P>
where
    P: Payload,
    Q: Edge<Item = Message<P>>,
{
    type Item = Message<P>;

    #[inline]
    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult {
        self.queue.try_push(item, policy)
    }

    #[inline]
    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        self.queue.try_pop()
    }

    #[inline]
    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy {
        self.queue.occupancy(policy)
    }

    #[inline]
    fn try_peek(&self) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        self.queue.try_peek()
    }

    #[inline]
    fn try_peek_at(
        &self,
        index: usize,
    ) -> Result<crate::edge::PeekResponse<'_, Self::Item>, QueueError> {
        self.queue.try_peek_at(index)
    }

    #[inline]
    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        self.queue.try_pop_batch(policy)
    }
}

/// An edge couples one output port to one input port with an admission policy.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeDescriptor {
    /// Unique identifier of this edge in the graph.
    id: EdgeIndex,
    /// Identifier of the upstream node / port.
    upstream: PortId,
    /// Identifier of the downstream node / port.
    downstream: PortId,
    /// Optional static name (for diagnostics or graph tooling).
    name: Option<&'static str>,
}

impl EdgeDescriptor {
    /// Construct a new `EdgeDescriptor`.
    #[inline]
    pub fn new(
        id: EdgeIndex,
        upstream: PortId,
        downstream: PortId,
        name: Option<&'static str>,
    ) -> Self {
        Self {
            id,
            upstream,
            downstream,
            name,
        }
    }

    /// Unique identifier of this edge in the graph.
    #[inline]
    pub fn id(&self) -> &EdgeIndex {
        &self.id
    }

    /// Identifier of the upstream node / port.
    #[inline]
    pub fn upstream(&self) -> &PortId {
        &self.upstream
    }

    /// Identifier of the downstream node / port.
    #[inline]
    pub fn downstream(&self) -> &PortId {
        &self.downstream
    }

    /// Optional static name (for diagnostics or graph tooling).
    #[inline]
    pub fn name(&self) -> Option<&'static str> {
        self.name
    }
}

/// Std-only, owning edge link that stores the concrete queue behind an
/// `Arc<Mutex<Q>>` plus static routing/policy metadata.
///
/// This is the thread-safe sibling of `EdgeLink`: instead of borrowing a queue,
/// it **owns** it in an `Arc<Mutex<_>>` so concurrent runtimes can hand out
/// cloned, thread-safe endpoints to worker threads.
#[cfg(feature = "std")]
#[non_exhaustive]
#[derive(Debug)]
pub struct ConcurrentEdgeLink<Q, P>
where
    P: Payload,
    Q: Edge<Item = Message<P>>,
{
    /// Shared, thread-safe buffer for this edge.
    buf: std::sync::Arc<std::sync::Mutex<Q>>,
    /// Unique identifier for this edge in the graph.
    id: EdgeIndex,
    /// Upstream node/port (producer).
    upstream: PortId,
    /// Downstream node/port (consumer).
    downstream: PortId,
    /// Admission/scheduling policy for this edge.
    policy: EdgePolicy,
    /// Optional static name for diagnostics/tooling.
    name: Option<&'static str>,
    /// Bind payload type at the type level without storing it.
    _marker: core::marker::PhantomData<P>,
}

#[cfg(feature = "std")]
impl<Q, P> ConcurrentEdgeLink<Q, P>
where
    P: Payload,
    Q: Edge<Item = Message<P>>,
{
    /// Create from a concrete queue and edge metadata.
    ///
    /// The queue is placed behind an `Arc<Mutex<_>>` so callers can construct
    /// concurrent producer/consumer endpoints that share the same buffer.
    #[inline]
    pub fn new(
        queue: Q,
        id: EdgeIndex,
        upstream: PortId,
        downstream: PortId,
        policy: EdgePolicy,
        name: Option<&'static str>,
    ) -> Self {
        Self {
            buf: std::sync::Arc::new(std::sync::Mutex::new(queue)),
            id,
            upstream,
            downstream,
            policy,
            name,
            _marker: core::marker::PhantomData,
        }
    }

    /// Return a lightweight descriptor for this edge (IDs/ports/name).
    #[inline]
    pub fn descriptor(&self) -> EdgeDescriptor {
        EdgeDescriptor {
            id: self.id,
            upstream: self.upstream,
            downstream: self.downstream,
            name: self.name,
        }
    }

    /// Get the edge policy (admission/watermarks/over-budget behavior).
    #[inline]
    pub fn policy(&self) -> &EdgePolicy {
        &self.policy
    }

    /// Access the shared `Arc<Mutex<Q>>` to build thread-safe endpoints.
    #[inline]
    pub fn arc(&self) -> std::sync::Arc<std::sync::Mutex<Q>> {
        std::sync::Arc::clone(&self.buf)
    }
}

#[cfg(feature = "std")]
impl<Q, P> crate::edge::Edge for ConcurrentEdgeLink<Q, P>
where
    P: crate::message::payload::Payload + Clone,
    Q: crate::edge::Edge<Item = crate::message::Message<P>> + Send + 'static,
{
    type Item = crate::message::Message<P>;

    #[inline]
    fn try_push(
        &mut self,
        item: Self::Item,
        policy: &crate::policy::EdgePolicy,
    ) -> crate::edge::EnqueueResult {
        match self.buf.lock() {
            Ok(mut q) => q.try_push(item, policy),
            Err(_) => crate::edge::EnqueueResult::Rejected,
        }
    }

    #[inline]
    fn try_pop(&mut self) -> Result<Self::Item, crate::errors::QueueError> {
        match self.buf.lock() {
            Ok(mut q) => q.try_pop(),
            Err(_) => Err(crate::errors::QueueError::Poisoned),
        }
    }

    #[inline]
    fn occupancy(&self, policy: &crate::policy::EdgePolicy) -> crate::edge::EdgeOccupancy {
        match self.buf.lock() {
            Ok(q) => q.occupancy(policy),
            Err(_) => crate::edge::EdgeOccupancy {
                items: 0,
                bytes: 0,
                watermark: crate::policy::WatermarkState::AtOrAboveHard,
            },
        }
    }

    #[inline]
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        match self.buf.lock() {
            Ok(q) => match q.try_peek() {
                Ok(peek) => match peek {
                    // Convert a borrowed reference into an owned item while holding the lock,
                    // so the returned PeekResponse::Owned does not borrow from the mutex guard.
                    PeekResponse::Borrowed(b) => {
                        let owned = b.clone();
                        Ok(PeekResponse::Owned(owned))
                    }

                    #[cfg(feature = "alloc")]
                    PeekResponse::Owned(o) => Ok(crate::edge::PeekResponse::Owned(o)),
                },
                Err(e) => Err(e),
            },
            Err(_) => Err(crate::errors::QueueError::Poisoned),
        }
    }

    #[inline]
    fn try_peek_at(&self, index: usize) -> Result<PeekResponse<'_, Self::Item>, QueueError> {
        match self.buf.lock() {
            Ok(q) => match q.try_peek_at(index) {
                Ok(peek) => match peek {
                    // Must not return a borrow tied to the mutex guard.
                    PeekResponse::Borrowed(b) => {
                        let owned = b.clone();
                        Ok(PeekResponse::Owned(owned))
                    }

                    #[cfg(feature = "alloc")]
                    PeekResponse::Owned(o) => Ok(PeekResponse::Owned(o)),
                },
                Err(e) => Err(e),
            },
            Err(_) => Err(crate::errors::QueueError::Poisoned),
        }
    }

    #[inline]
    fn try_pop_batch(
        &mut self,
        policy: &crate::policy::BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        match self.buf.lock() {
            Ok(mut q) => {
                let batch = q.try_pop_batch(policy)?;

                // We must not return a borrow tied to the mutex guard, so convert to owned.
                // This preserves semantics for disjoint windows (moved items) and sliding
                // windows (moved + cloned peek items) as established in the inner queues.
                Ok(batch.into_owned())
            }
            Err(_) => Err(crate::errors::QueueError::Poisoned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::edge::bench::TestSpscRingBuf;
    use crate::message::MessageHeader;
    use crate::policy::{AdmissionPolicy, EdgePolicy, OverBudgetAction, QueueCaps};
    use crate::types::{EdgeIndex, NodeIndex, PortId, PortIndex, Ticks};

    const POLICY: EdgePolicy = EdgePolicy::new(
        QueueCaps::new(8, 6, None, None),
        AdmissionPolicy::DropNewest,
        OverBudgetAction::Drop,
    );

    fn make_link() -> EdgeLink<TestSpscRingBuf<Message<u32>, 16>, u32> {
        let queue = TestSpscRingBuf::<Message<u32>, 16>::new();

        let id = EdgeIndex::new(0);

        let upstream_port = PortId::new(NodeIndex::new(0), PortIndex::new(0));
        let downstream_port = PortId::new(NodeIndex::new(1), PortIndex::new(0));

        EdgeLink::new(
            queue,
            id,
            upstream_port,
            downstream_port,
            POLICY,
            Some("edge:hi"),
        )
    }

    crate::run_edge_contract_tests!(edge_link_contract, || make_link());

    #[test]
    fn edge_link_metadata_accessors_and_descriptor() {
        let link = make_link();

        assert_eq!(link.id(), &EdgeIndex::new(0));
        assert_eq!(
            link.upstream_port(),
            &PortId::new(NodeIndex::new(0), PortIndex::new(0))
        );
        assert_eq!(
            link.downstream_port(),
            &PortId::new(NodeIndex::new(1), PortIndex::new(0))
        );
        assert_eq!(link.policy(), &POLICY);
        assert_eq!(link.name(), Some("edge:hi"));

        let d = link.descriptor();
        assert_eq!(d.id(), &EdgeIndex::new(0));
        assert_eq!(
            d.upstream(),
            &PortId::new(NodeIndex::new(0), PortIndex::new(0))
        );
        assert_eq!(
            d.downstream(),
            &PortId::new(NodeIndex::new(1), PortIndex::new(0))
        );
        assert_eq!(d.name(), Some("edge:hi"));
    }

    #[test]
    fn edge_link_forwards_to_inner_queue_smoke() {
        let mut link = make_link();

        let mut header = MessageHeader::empty();
        header.set_creation_tick(Ticks::new(123));
        let msg = Message::new(header, 0u32);

        assert_eq!(
            link.try_push(msg, &POLICY),
            crate::edge::EnqueueResult::Enqueued
        );
        let popped = link.try_pop().expect("pop");
        assert_eq!((*popped.header().creation_tick()).as_u64(), &123u64);
    }

    #[cfg(all(test, feature = "std"))]
    mod concurrent_tests {
        use super::*;

        use crate::edge::bench::TestSpscRingBuf;
        use crate::message::MessageHeader;
        use crate::policy::{AdmissionPolicy, EdgePolicy, OverBudgetAction, QueueCaps};
        use crate::types::{EdgeIndex, NodeIndex, PortId, PortIndex, Ticks};

        const POLICY: EdgePolicy = EdgePolicy::new(
            QueueCaps::new(8, 6, None, None),
            AdmissionPolicy::DropNewest,
            OverBudgetAction::Drop,
        );

        fn make_concurrent_link() -> ConcurrentEdgeLink<TestSpscRingBuf<Message<u32>, 16>, u32> {
            let queue = TestSpscRingBuf::<Message<u32>, 16>::new();

            let id = EdgeIndex::new(0);

            let upstream = PortId::new(NodeIndex::new(0), PortIndex::new(0));
            let downstream = PortId::new(NodeIndex::new(1), PortIndex::new(0));

            ConcurrentEdgeLink::new(
                queue,
                id,
                upstream,
                downstream,
                POLICY,
                Some("edge:concurrent"),
            )
        }

        crate::run_edge_contract_tests!(concurrent_edge_link_contract, || make_concurrent_link());

        #[test]
        fn concurrent_edge_link_metadata_and_descriptor() {
            let link = make_concurrent_link();

            assert_eq!(link.policy(), &POLICY);

            let d = link.descriptor();
            assert_eq!(d.id(), &EdgeIndex::new(0));
            assert_eq!(
                d.upstream(),
                &PortId::new(NodeIndex::new(0), PortIndex::new(0))
            );
            assert_eq!(
                d.downstream(),
                &PortId::new(NodeIndex::new(1), PortIndex::new(0))
            );
            assert_eq!(d.name(), Some("edge:concurrent"));
        }

        #[test]
        fn concurrent_edge_link_arc_shares_underlying_queue() {
            let link = make_concurrent_link();
            let arc_a = link.arc();
            let arc_b = link.arc();

            {
                let mut q = arc_a.lock().expect("lock a");
                let mut header = MessageHeader::empty();
                header.set_creation_tick(Ticks::new(55));
                let msg = Message::new(header, 0u32);

                let res = q.try_push(msg, &POLICY);
                assert_eq!(res, crate::edge::EnqueueResult::Enqueued);
            }

            {
                let mut q = arc_b.lock().expect("lock b");
                let popped = q.try_pop().expect("pop");
                assert_eq!((*popped.header().creation_tick()).as_u64(), &55u64);
            }
        }
    }
}
