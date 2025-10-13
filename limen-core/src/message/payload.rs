//! Minimal, zero-cost payload descriptors for generic Rust data.

use crate::memory::{BufferDescriptor, MemoryClass};
use core::mem;

/// Trait for payload types that can provide byte length and memory class.
pub trait Payload {
    /// Return the buffer descriptor (byte size & memory class).
    fn buffer_descriptor(&self) -> BufferDescriptor;
}

/* ---------- Generic slices & arrays (cover all element types T) ---------- */

#[allow(clippy::manual_slice_size_calculation)]
impl<T> Payload for [T] {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        BufferDescriptor {
            bytes: self.len() * mem::size_of::<T>(),
            class: MemoryClass::Host,
        }
    }
}

#[allow(clippy::needless_lifetimes, clippy::manual_slice_size_calculation)]
impl<'a, T> Payload for &'a [T] {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        BufferDescriptor {
            bytes: self.len() * mem::size_of::<T>(),
            class: MemoryClass::Host,
        }
    }
}

impl<T, const N: usize> Payload for [T; N] {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        BufferDescriptor {
            bytes: N * mem::size_of::<T>(),
            class: MemoryClass::Host,
        }
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'a, T, const N: usize> Payload for &'a [T; N] {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        BufferDescriptor {
            bytes: N * mem::size_of::<T>(),
            class: MemoryClass::Host,
        }
    }
}

/* ------------------------- Common scalar payloads ------------------------ */

impl Payload for () {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        BufferDescriptor {
            bytes: 0,
            class: MemoryClass::Host,
        }
    }
}

impl Payload for u32 {
    #[inline]
    fn buffer_descriptor(&self) -> BufferDescriptor {
        BufferDescriptor {
            bytes: mem::size_of::<u32>(),
            class: MemoryClass::Host,
        }
    }
}
