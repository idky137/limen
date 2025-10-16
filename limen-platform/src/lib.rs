#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![deny(unsafe_code)]
//! Platform adapters for Limen.

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod linux;
