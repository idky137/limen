#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![deny(unsafe_code)]
//! # limen-light
//!
//! `limen-light` provides **P0** (no_std + no_alloc) and **P1** (no_std + alloc)
//! runtimes implementations on top of [`limen-core`]. It keeps the data path fully
//! generic and monomorphized; there is **no dynamic dispatch** in the hot path.
//!
//! ## What is here
//!
//! ## Features
//! - `p0` (default): enable P0 runtime and static ring queues.
//! - `p1`: enable P1 runtime and heap-backed queues (requires `alloc`).
//! - `std`: enables `std` conveniences; implies `alloc`.

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod runtime;
pub mod util;
