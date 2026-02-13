//! Limen single-producer single-consumer edge trait and related types.

use crate::errors::QueueError;
use crate::message::{payload::Payload, Message};
use crate::policy::{AdmissionDecision, BatchingPolicy, EdgePolicy, WatermarkState};
use crate::prelude::BatchView;

pub mod link;

pub mod spsc_array;

#[cfg(feature = "alloc")]
pub mod spsc_vecdeque;

#[cfg(feature = "std")]
pub mod spsc_ringbuf;

#[cfg(feature = "std")]
pub mod spsc_concurrent;

#[cfg(feature = "spsc_raw")]
pub mod spsc_raw;

pub mod spsc_priority2;

#[cfg(any(test, feature = "bench"))]
pub mod bench;

/// Push result for enqueue attempts.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueResult {
    /// Item was enqueued successfully.
    Enqueued,
    /// Item was dropped per policy (DropNewest).
    DroppedNewest,
    /// Item could not be enqueued due to backpressure or full capacity.
    Rejected,
}

/// Queue occupancy snapshot used for decisions.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeOccupancy {
    /// Number of items currently in the queue.
    items: usize,
    /// Estimated bytes currently in the queue.
    bytes: usize,
    /// Watermark state derived from capacities.
    watermark: WatermarkState,
}

impl EdgeOccupancy {
    /// Create a new `EdgeOccupancy`.
    #[inline]
    pub const fn new(items: usize, bytes: usize, watermark: WatermarkState) -> Self {
        Self {
            items,
            bytes,
            watermark,
        }
    }

    /// Number of items currently in the queue.
    #[inline]
    pub fn items(&self) -> &usize {
        &self.items
    }

    /// Estimated bytes currently in the queue.
    #[inline]
    pub fn bytes(&self) -> &usize {
        &self.bytes
    }

    /// Watermark state derived from capacities.
    #[inline]
    pub fn watermark(&self) -> &WatermarkState {
        &self.watermark
    }
}

/// A single-producer, single-consumer queue contract.
///
/// The `Item` type is typically a [`Message<P>`](Message) with some payload `P`,
/// but the trait is generic and can be used for other types as needed.
pub trait Edge {
    /// The type of items stored in the queue.
    type Item;

    /// Attempt to push an item onto the queue using the given edge policy.
    ///
    /// Implementations may evict an existing item if `DropOldest` is configured.
    fn try_push(&mut self, item: Self::Item, policy: &EdgePolicy) -> EnqueueResult;

    /// Attempt to pop an item from the queue.
    fn try_pop(&mut self) -> Result<Self::Item, QueueError>;

    /// Return a snapshot of occupancy used for telemetry and admission.
    ///
    /// Implementations should avoid blocking. If a concurrent backend might fail
    /// to sample (e.g., poisoned lock), provide a fallible path in the backend and
    /// map that to `GraphError::OccupancySampleFailed` in your `GraphApi` impl.
    fn occupancy(&self, policy: &EdgePolicy) -> EdgeOccupancy;

    /// Return `true` if the queue is empty.
    fn is_empty(&self) -> bool
    where
        Self::Item: Payload,
    {
        matches!(self.try_peek(), Err(QueueError::Empty))
    }

    /// Peek at the front item without removing it.
    ///
    /// Returns a `MessagePeek<'_, Self::Item>`. Implementations should prefer
    /// returning `MessagePeek::Borrowed(&Self::Item)` for zero-copy paths.
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload;

    /// Attempt to pop a batch of items according to the provided batching policy.
    ///
    /// The returned `BatchView<'_, Self::Item>` is allowed to borrow from `self`
    /// (for zero-copy / heapless implementations) or to be an owned collection
    /// (when allocation is available). The lifetime of the `BatchView` is tied
    /// to `&mut self`, so callers must not outlive the borrow.
    ///
    /// Implementations MUST honour the semantics of `BatchingPolicy` (fixed-N,
    /// max-Δt partial batches, and windowing style). Error handling mirrors
    /// `try_pop` and should return a `QueueError` on failure (including empty).
    fn try_pop_batch(
        &mut self,
        policy: &BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload;
}

/// Convenience helper to enqueue a message using policy-derived admission logic.
pub fn enqueue_with_admission<P: Payload, Q: Edge<Item = Message<P>>>(
    queue: &mut Q,
    policy: &EdgePolicy,
    msg: Message<P>,
) -> EnqueueResult {
    let occ = queue.occupancy(policy);
    match policy.decide(
        occ.items,
        occ.bytes,
        *msg.header().deadline_ns(),
        *msg.header().qos(),
    ) {
        AdmissionDecision::Admit => queue.try_push(msg, policy),
        AdmissionDecision::Reject => EnqueueResult::Rejected,
    }
}

/// Unified single-item peek result returned by `Edge::try_peek`.
///
/// Generic over the *item* type `I` stored in the edge. Two variants only:
/// - `Borrowed(&'a I)` for zero-copy/no-alloc SPSC paths.
/// - `Owned(I)` for alloc-enabled fallbacks / concurrent queues.
///
/// Callers should use `as_ref()` to get a `&I` regardless of variant.
/// When `I = Message<P>` additional conveniences are provided (header/payload
/// access and `into_owned()`).
#[derive(Debug)]
pub enum PeekResponse<'a, I: 'a> {
    /// Borrowed, zero-alloc view into the queue (SPSC / no-alloc).
    Borrowed(&'a I),

    /// Owned item returned by the queue (alloc required).
    #[cfg(feature = "alloc")]
    Owned(I),
}

impl<'a, I: 'a> AsRef<I> for PeekResponse<'a, I> {
    #[inline]
    fn as_ref(&self) -> &I {
        match self {
            PeekResponse::Borrowed(r) => r,
            #[cfg(feature = "alloc")]
            PeekResponse::Owned(o) => o,
        }
    }
}

/// Convenience methods for the common case where the item is a `Message<P>`.
impl<'a, P: crate::message::payload::Payload + 'a> PeekResponse<'a, crate::message::Message<P>> {
    /// Convenience: return the header reference.
    #[inline]
    pub fn header(&self) -> &crate::message::MessageHeader {
        self.as_ref().header()
    }

    /// Convenience: return the payload reference.
    #[inline]
    pub fn payload(&self) -> &P {
        self.as_ref().payload()
    }

    /// Convert into an owned `Message<P>`.
    ///
    /// - Available when `P: Clone`. This method is **not** gated on `alloc`.
    /// - If this enum is `Owned` (alloc), the owned value is returned directly.
    /// - If `Borrowed`, the message is cloned into an owned `Message<P>`.
    #[inline]
    pub fn into_owned(self) -> crate::message::Message<P>
    where
        P: Clone,
    {
        match self {
            PeekResponse::Borrowed(b) => (*b).clone(),
            #[cfg(feature = "alloc")]
            PeekResponse::Owned(o) => o,
        }
    }
}

/// Generic `Clone` impl: clones the owned item when present, otherwise copies the borrow.
///
/// This requires `I: Clone` so that the `Owned(I)` variant can be cloned.
/// Borrowed(&I) is cheap to clone (copies the reference).
impl<'a, I: Clone + 'a> Clone for PeekResponse<'a, I> {
    fn clone(&self) -> Self {
        match self {
            PeekResponse::Borrowed(r) => PeekResponse::Borrowed(r),
            #[cfg(feature = "alloc")]
            PeekResponse::Owned(o) => PeekResponse::Owned(o.clone()),
        }
    }
}

/// A no-op queue implementation used for phantom inputs and outputs.
///
/// `NoQueue` acts as a placeholder in the graph where a queue is required by
/// type but no actual buffering or message transfer is desired. All enqueue
/// attempts are rejected, and all dequeue or peek attempts return empty.
///
/// This is primarily useful for:
/// - Phantom or unconnected ports in a graph.
/// - Simplifying generic code that expects a queue type, without allocating
///   unnecessary resources.
/// - Static analysis or testing scenarios where message flow is disabled.
///
/// # Type Parameters
/// - `P`: Payload type of the [`Message`] carried by this queue.
///
/// # Behavior
/// - [`SpscQueue::try_push`] always returns [`EnqueueResult::Rejected`].
/// - [`SpscQueue::try_pop`] always returns [`QueueError::Empty`].
/// - [`SpscQueue::try_peek`] always returns [`QueueError::Empty`].
/// - [`SpscQueue::occupancy`] always reports zero items, zero bytes, and
///   [`WatermarkState::AtOrAboveHard`] (fully saturated, disallowing admission).
pub struct NoQueue<P: Payload>(core::marker::PhantomData<P>);

impl<P: Payload> Edge for NoQueue<P> {
    type Item = Message<P>;
    #[inline]
    fn try_push(&mut self, _item: Self::Item, _policy: &EdgePolicy) -> EnqueueResult {
        EnqueueResult::Rejected
    }
    #[inline]
    fn try_pop(&mut self) -> Result<Self::Item, QueueError> {
        Err(QueueError::Empty)
    }
    #[inline]
    fn occupancy(&self, _policy: &EdgePolicy) -> EdgeOccupancy {
        EdgeOccupancy {
            items: 0,
            bytes: 0,
            watermark: WatermarkState::AtOrAboveHard,
        }
    }

    #[inline]
    fn try_peek(&self) -> Result<PeekResponse<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        Err(QueueError::Empty)
    }

    #[inline]
    fn try_pop_batch(
        &mut self,
        _policy: &BatchingPolicy,
    ) -> Result<BatchView<'_, Self::Item>, QueueError>
    where
        Self::Item: Payload,
    {
        Err(QueueError::Empty)
    }
}
