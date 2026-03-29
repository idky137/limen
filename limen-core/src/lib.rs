#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![deny(unsafe_code)]
//! # limen-core
//!
//! **Limen Core** defines the *stable contracts and primitives* for the Limen
//! graph-driven, edge inference runtime. It is `no_std` by default and uses
//! feature gates to enable `alloc` and `std`-specific conveniences. The data
//! plane is designed for *monomorphization* via generics and const generics,
//! avoiding dynamic dispatch in the hot path.
//!
//! This crate intentionally **does not** provide graph construction, concrete
//! schedulers, or queue implementations. Those are provided by `limen-light`
//! (P0/P1) and `limen` (P2).
//!
//! ## Modules Overview
//! - [`types`]: small newtypes and shared enums (QoS, identifiers).
//! - [`memory`]: memory classes and placement descriptors for zero-copy paths.
//! - [`message`]: message header, payload contract, and message types.
//! - [`policy`]: batching, budgets, deadlines, admission and edge policies.
//! - [`edge`]: single-producer single-consumer queue trait and results.
//! - [`node`]: uniform node contract and step lifecycle.
//! - [`routing`]: split (fan-out) and join (fan-in) operator traits.
//! - [`telemetry`]: counters, histograms, and tracing interfaces.
//! - [`platform`]: platform abstractions (clock, timers, affinities).
//! - [`scheduling`]: readiness and dequeue policy traits (EDF hooks).
//! - [`graph`]: port indices, edge descriptors, invariant validation traits.
//! - [`errors`]: error families for nodes, queues, and runtime surfaces.
//! - [`prelude`]: convenient re-exports for implementers.
//!
//! ## Feature Flags
//! - `alloc`: enables optional APIs using `alloc` types.
//! - `std`: enables `std`-specific conveniences; implies `alloc`.
//!
//! ## Versioning and Stability
//! The contracts defined here are intended to be *stable* so higher-level
//! runtimes can evolve independently. Avoid adding trait objects or dynamic
//! allocation requirements to keep the core maximally portable.

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod errors;
pub mod memory;
pub mod types;

pub mod message;
pub mod platform;
pub mod policy;

pub mod compute;
pub mod scheduling;
pub mod telemetry;

pub mod edge;
pub mod graph;
pub mod node;
pub mod runtime;

pub mod prelude;

// **PR NOTES**:
//
// ----------
// 1.
//
// Phase 3d: Eviction design note
//
//  The current Evict(n) path can evict multiple tokens but EnqueueResult::Evicted only returns one. For v0.1.0, this is acceptable because:
//  - Evict(n) with n>1 is rare (only EvictUntilBelowHard evicts multiple)
//  - StepContext needs to free ALL evicted tokens, not just the last one
//  Better approach: StepContext should handle eviction pre-push (call get_admission_decision, pop evicted tokens itself, free them, then push). The edge's try_push would only handle Admit/DropNewest/Reject/Block. Eviction
//
// ----------
// 2.
//
// REMOVE ADMISSION INFO!!!
//
// ----------
// 3.
//
// FINAL CLEANUP AT END OF PR:
// - batch types
// - tensor types
// - extra message description trait ^^^ (Admission info or different???)
//
// ----------
// 4.
//
// Ensure concurrent runtime only accepts concurrent safe graph at compile time.
//
// ----------
// 5.
//
// Node::pr0pcess_message:
// - Fonce gives message and recieves message back!
// - Node implementers don't have to worry about graph behaviour / wiring at all!
//
// ***FINAL***
// - remove telemetryfrom node, purely internal!

// please help me finish C1a, there is an old plan for this final work in: claude_context_docs/planned/C1a_final_cleanup_plan.md, however this is wrong:
// - we are keeping outstepcontext, it it just not part of process message.
//
// For B still we need to:
// - finish updating sink, source and model.
// - update nodelink (keep telemetry in process_message as is, I need it.
// - update bench node impls
// - update contract tests
//
// then I have the following errors:
// error[E0499]: cannot borrow `*ctx` as mutable more than once at a time
//     --> limen-core/src/node.rs:1000:9
//      |
//  998 |         let telemetry = ctx.telemetry_mut();
//      |                         --- first mutable borrow occurs here
//  999 |
// 1000 |         ctx.pop_and_process(port, |msg| self.process_message(msg, clock, telemetry))
//      |         ^^^ second mutable borrow occurs here                            --------- first borrow later captured here by closure
//
// error[E0499]: cannot borrow `*ctx` as mutable more than once at a time
//     --> limen-core/src/node.rs:1041:9
//      |
// 1039 |         let telemetry = ctx.telemetry_mut();
//      |                         --- first mutable borrow occurs here
// 1040 |
// 1041 |         ctx.pop_batch_and_process(port, nmax, &node_policy, |msg| {
//      |         ^^^ second mutable borrow occurs here
// 1042 |             self.process_message(msg, clock telemetry)
//      |                                             --------- first borrow later captured here by closure
//
// then C and D.
//
// please create a full plan to fix all of this.
