//! Limen-core batch types and helpers.
//!
//! This module provides:
//! - `Batch<'a, P>`: a thin, immutable view over a slice of `Message<P>` used
//!   by policies, telemetry and nodes, and
//! - `BatchView<'a, P>`: an internal container (owned or borrowed) used by the
//!   runtime / NodeLink / StepContext to assemble batches before handing a
//!   borrowed `Batch<'a, P>` to nodes.
//!
//! `BatchView` intentionally provides both an owned (`Vec`) variant (alloc)
//! and a borrowed, stack/heapless-friendly variant so the runtime can operate
//! in both `alloc` and `no-alloc` builds.

use crate::{
    memory::{BufferDescriptor, MemoryClass},
    message::{payload::Payload, Message, MessageHeader},
};

use core::{mem, slice};

/// A thin batch view over a slice of messages.
///
/// Batch formation is runtime-specific; the core only provides
/// a convenient immutable view for policies and telemetry.
#[derive(Debug, Copy, Clone)]
pub struct Batch<'a, P: Payload> {
    /// The ordered messages in the batch.
    messages: &'a [Message<P>],
}

impl<'a, P: Payload> Batch<'a, P> {
    /// Construct a new batch view over a slice of messages.
    #[inline]
    pub const fn new(messages: &'a [Message<P>]) -> Self {
        Self { messages }
    }

    /// Return the underlying messages slice.
    #[inline]
    pub fn messages(&self) -> &'a [Message<P>] {
        self.messages
    }

    /// Return the number of messages in the batch.
    #[inline]
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Return `true` if the batch is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Total byte size across message payloads.
    pub fn total_payload_bytes(&self) -> usize {
        self.messages
            .iter()
            .map(|m| m.header.payload_size_bytes)
            .sum()
    }

    /// Iterate over messages.
    #[inline]
    pub fn iter(&self) -> core::slice::Iter<'_, Message<P>> {
        self.messages.iter()
    }

    /// Convenience: is the first message marked FIRST_IN_BATCH (if present)?
    #[inline]
    pub fn first_flagged(&self) -> bool {
        self.messages
            .first()
            .map(|m| m.header.flags.is_first())
            .unwrap_or(false)
    }

    /// Convenience: is the last message marked LAST_IN_BATCH (if present)?
    #[inline]
    pub fn last_flagged(&self) -> bool {
        self.messages
            .last()
            .map(|m| m.header.flags.is_last())
            .unwrap_or(false)
    }

    /// (Optional) Validate flags are consistent with batch boundaries.
    /// Enable only when you want assertions (e.g., in tests) via a feature flag.
    // #[cfg(feature = "validate_batches")]
    #[inline]
    pub fn assert_flags_consistent(&self) {
        if self.is_empty() {
            return;
        }
        debug_assert!(
            self.first_flagged(),
            "batch: first item missing FIRST_IN_BATCH"
        );
        debug_assert!(
            self.last_flagged(),
            "batch: last item missing LAST_IN_BATCH"
        );
        // Optional: internal items should have neither FIRST nor LAST
        for m in &self.messages[1..self.messages.len().saturating_sub(1)] {
            debug_assert!(
                !m.header.flags.is_first() && !m.header.flags.is_last(),
                "batch: internal item has boundary flag"
            );
        }
    }
}

impl<'a, P: Payload> Payload for Batch<'a, P> {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        // Sum payload bytes across messages and add header size per message.
        let total_payload_bytes: usize = self
            .messages
            .iter()
            .map(|m| {
                // Use the header field stored on message as the authoritative payload size.
                // This avoids re-inspecting m.payload() which might be expensive for some payloads.
                m.header.payload_size_bytes
            })
            .sum();

        let header_bytes = self.messages.len() * mem::size_of::<MessageHeader>();
        BufferDescriptor::new(total_payload_bytes + header_bytes, MemoryClass::Host)
    }
}

// Provide also for borrowed Batch reference to match other Payload impls pattern.
impl<'a, P: Payload> Payload for &'a Batch<'a, P> {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        (*self).buffer_descriptor()
    }
}

/// An internal batch container used by the runtime/nodelink/stepcontext.
///
/// - `Owned(Vec<Message<P>>)`: when `alloc` feature is enabled and we can own a Vec.
/// - `Borrowed(&'a mut [Message<P>], len)`: stack/heapless-backed buffer with explicit length.
#[derive(Debug)]
pub enum BatchView<'a, P>
where
    P: Payload,
{
    /// Owned variant (alloc-enabled). Stores the entire `Vec<Message<P>>`.
    #[cfg(feature = "alloc")]
    Owned(alloc::vec::Vec<Message<P>>),

    /// Borrowed variant: a mutable slice plus a length indicating the valid prefix.
    Borrowed(&'a mut [Message<P>], usize),
}

impl<'a, P> BatchView<'a, P>
where
    P: Payload,
{
    /// Construct from an owned Vec (alloc feature required).
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn from_owned(v: alloc::vec::Vec<Message<P>>) -> Self {
        BatchView::Owned(v)
    }

    /// Construct from a borrowed slice + length.
    #[inline]
    pub fn from_borrowed(buf: &'a mut [Message<P>], len: usize) -> Self {
        debug_assert!(len <= buf.len());
        BatchView::Borrowed(buf, len)
    }

    /// Number of messages in the batch.
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => v.len(),
            BatchView::Borrowed(_, n) => *n,
        }
    }

    /// Is the batch empty?
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Immutable iterator over messages.
    #[inline]
    pub fn iter(&self) -> slice::Iter<'_, Message<P>> {
        match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => v.as_slice().iter(),
            BatchView::Borrowed(buf, n) => buf[..*n].iter(),
        }
    }

    /// Mutable iterator over messages (rarely used externally).
    #[inline]
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, Message<P>> {
        match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => v.as_mut_slice().iter_mut(),
            BatchView::Borrowed(buf, n) => buf[..*n].iter_mut(),
        }
    }

    /// Convert to the public, borrowed `Batch<'_, P>` view.
    ///
    /// The returned `Batch` borrows from `self` (i.e., the `BatchView` must
    /// remain alive for the duration of the `Batch` borrow).
    #[inline]
    pub fn as_batch(&self) -> Batch<'_, P> {
        let slice: &[Message<P>] = match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => v.as_slice(),
            BatchView::Borrowed(buf, n) => &buf[..*n],
        };
        Batch::new(slice)
    }

    /// Mutable access to the first message header if present.
    #[inline]
    pub fn first_header_mut(&mut self) -> Option<&mut MessageHeader> {
        if self.is_empty() {
            return None;
        }
        Some(match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => v[0].header_mut(),
            BatchView::Borrowed(buf, _) => buf[0].header_mut(),
        })
    }

    /// Mutable access to the last message header if present.
    #[inline]
    pub fn last_header_mut(&mut self) -> Option<&mut MessageHeader> {
        let n = self.len();
        if n == 0 {
            return None;
        }
        Some(match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => v[n - 1].header_mut(),
            BatchView::Borrowed(buf, _) => buf[n - 1].header_mut(),
        })
    }

    /// Consume and return the owned Vec if present (alloc-only).
    ///
    /// Returns `Some(Vec<Message<P>>)` for the Owned variant, otherwise `None`.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn into_owned_vec(self) -> Option<alloc::vec::Vec<Message<P>>> {
        match self {
            BatchView::Owned(v) => Some(v),
            _ => None,
        }
    }

    /// Try to convert into a borrowed Batch<'_, P> while keeping `self` alive.
    /// Equivalent to `self.as_batch()` but offered for clarity.
    #[inline]
    pub fn into_batch_ref(&self) -> Batch<'_, P> {
        self.as_batch()
    }
}

impl<'a, P: Payload> Payload for BatchView<'a, P> {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        // We'll iterate the messages according to the variant and compute the same aggregate.
        match self {
            #[cfg(feature = "alloc")]
            BatchView::Owned(v) => {
                let total_payload_bytes: usize =
                    v.iter().map(|m| m.header.payload_size_bytes).sum();
                let header_bytes = v.len() * mem::size_of::<MessageHeader>();
                BufferDescriptor::new(total_payload_bytes + header_bytes, MemoryClass::Host)
            }

            BatchView::Borrowed(buf, n) => {
                let total_payload_bytes: usize =
                    buf[..*n].iter().map(|m| m.header.payload_size_bytes).sum();
                let header_bytes = *n * mem::size_of::<MessageHeader>();
                BufferDescriptor::new(total_payload_bytes + header_bytes, MemoryClass::Host)
            }
        }
    }
}

// Borrowed ref as well
impl<'a, P: Payload> Payload for &'a BatchView<'a, P> {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        (*self).buffer_descriptor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    /// Helper: construct a Message<u32> with an empty header and given payload.
    fn make_msg_u32(v: u32) -> Message<u32> {
        Message::new(MessageHeader::empty(), v)
    }

    #[test]
    fn batch_basic_props() {
        // Build a small array of messages and create a Batch view.
        let arr: [Message<u32>; 3] = [make_msg_u32(10), make_msg_u32(11), make_msg_u32(12)];

        // Build a Batch directly over a slice.
        let batch = Batch::new(&arr[..2]); // first two items
        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
        assert_eq!(batch.messages().len(), 2);

        // total_payload_bytes should be sum of u32 sizes
        assert_eq!(batch.total_payload_bytes(), 2 * size_of::<u32>());

        // initially flags are not set
        assert!(!batch.first_flagged());
        assert!(!batch.last_flagged());
    }

    #[test]
    fn batchview_borrowed_basic_and_mutation() {
        // Prepare 4 messages but claim only first 3 are valid for the batch.
        let mut arr: [Message<u32>; 4] = [
            make_msg_u32(100),
            make_msg_u32(101),
            make_msg_u32(102),
            make_msg_u32(103),
        ];

        // Create Borrowed BatchView with length = 3
        let mut bv = BatchView::from_borrowed(&mut arr, 3);
        assert_eq!(bv.len(), 3);
        assert!(!bv.is_empty());

        // Mutate payloads via iter_mut()
        for (i, m) in bv.iter_mut().enumerate() {
            *m.payload_mut() = 200 + (i as u32);
        }

        // Convert to public Batch and inspect values without using vec
        let batch = bv.as_batch();
        let mut vals = [0u32; 3];
        let mut i = 0;
        for m in batch.iter() {
            vals[i] = *m.payload();
            i += 1;
        }
        assert_eq!(vals, [200u32, 201u32, 202u32]);

        // Set first/last flags through BatchView helpers
        {
            let fh = bv.first_header_mut().expect("first header");
            fh.set_first_in_batch();
            let lh = bv.last_header_mut().expect("last header");
            lh.set_last_in_batch();
        }

        let batch2 = bv.as_batch();
        assert!(batch2.first_flagged());
        assert!(batch2.last_flagged());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn batchview_owned_basic_and_into_owned() {
        use alloc::vec::Vec;

        // Create an owned vector of messages.
        let mut vec: Vec<Message<u32>> = Vec::new();
        vec.push(make_msg_u32(1));
        vec.push(make_msg_u32(2));
        vec.push(make_msg_u32(3));

        let mut bv = BatchView::from_owned(vec);
        assert_eq!(bv.len(), 3);
        assert!(!bv.is_empty());

        // Mutate last payload via iter_mut()
        for (i, m) in bv.iter_mut().enumerate() {
            if i == 2 {
                *m.payload_mut() = 42u32;
            }
        }

        // Inspect via as_batch into an owned Vec
        let batch = bv.as_batch();
        let mut xs: Vec<u32> = Vec::new();
        for m in batch.iter() {
            xs.push(*m.payload());
        }
        // compare to a slice instead of using `vec![]` macro
        assert_eq!(xs.as_slice(), &[1u32, 2u32, 42u32]);

        // Check header helpers and then consume owned vec
        {
            let fh = bv.first_header_mut().expect("first header");
            fh.set_first_in_batch();
            let lh = bv.last_header_mut().expect("last header");
            lh.set_last_in_batch();
        }
        let batch2 = bv.as_batch();
        assert!(batch2.first_flagged());
        assert!(batch2.last_flagged());

        // Consume the owned vec
        let ov = bv.into_owned_vec().expect("owned vec present");
        assert_eq!(ov.len(), 3);
        // Confirm the final payload value (42) survived.
        assert_eq!(*ov.last().unwrap().payload(), 42u32);
    }

    #[test]
    fn batch_assert_flags_consistent_no_panic_when_correct() {
        let mut arr: [Message<u32>; 2] = [make_msg_u32(7), make_msg_u32(8)];

        // set flags explicitly on headers and make a Batch
        {
            let mut bv = BatchView::from_borrowed(&mut arr, 2);
            bv.first_header_mut().unwrap().set_first_in_batch();
            bv.last_header_mut().unwrap().set_last_in_batch();
            let batch = bv.as_batch();
            // This should not panic (debug_assert runs in test builds)
            batch.assert_flags_consistent();
        }
    }
}
