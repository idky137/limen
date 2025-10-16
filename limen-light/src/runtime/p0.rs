//! P0 runtime: deterministic single-thread cooperative loop.
//!
//! This runtime targets `no_std + no_alloc` devices. It assumes a statically-wired
//! graph and uses round-robin fairness across nodes.
