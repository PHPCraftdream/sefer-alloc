//! `globalalloc-model` — differential-test any allocator against a reference model.
//!
//! Apply a random stream of `alloc` / `dealloc` / `realloc` / `alloc_zeroed`
//! operations to the allocator under test AND to a trivial reference model
//! (a `Vec` of live blocks), asserting the **M1–M4 oracles** on every step:
//!
//! - **M1 (validity):** every returned pointer is non-null, aligned to the
//!   requested align, and writable for the requested size (write a distinctive
//!   fill byte, read it back — failures name the op index, byte offset, and
//!   both byte values).
//! - **M2 (no double-free / UAF):** the model only frees live pointers; with
//!   `Config::double_free: Some(DoubleFreeOk::new())` (opt-in via an
//!   unforgeable token whose constructor is `const unsafe`, **off by default**
//!   — a real system malloc treats a double-free as undefined behaviour) a
//!   second `dealloc` of the same pointer is issued and must not corrupt the
//!   allocator.
//! - **M3 (no overlap):** two simultaneously-live allocations never share a
//!   byte — checked on EVERY block-creating op (`alloc`, `alloc_zeroed`, and
//!   `realloc`'s new extent) against every live block, and re-checked at run
//!   end via a per-block fill (see the fill-cycle limit below).
//! - **M4 (alignment & size fidelity):** every returned pointer satisfies the
//!   requested align. Size fidelity is established indirectly: the fill
//!   read-back proves the requested size is writable, and M3's overlap check
//!   over the requested extents catches an undersized block once a neighbour
//!   lands inside the missing tail.
//! - **`alloc_zeroed` contract:** every byte of a zeroed allocation reads as 0.
//! - **`realloc` prefix preservation:** the `min(old, new)` prefix is preserved.
//!
//! One model, two front-ends. The same `drive` loop powers both:
//! - a proptest `op_strategy` over `Vec<Op>` (the `proptest` feature) — for
//!   `cargo test` and the bounded miri run, and
//! - an `Arbitrary` impl for `OpStream` (the `arbitrary` feature) — for
//!   `cargo fuzz` / libFuzzer.
//!
//! (These two names are intentionally plain code, not intra-doc links: they
//! are feature-gated and must not break the default-feature docs build.)
//!
//! An oracle improvement thus reaches proptest, miri, AND libFuzzer at once.
//!
//! # The fill-cycle limit (M3, run-end sweep)
//!
//! The fill byte is a `u8` cycling `1..=255`, so once blocks beyond the first
//! 255 fill assignments are simultaneously live, two live blocks can share a
//! fill byte and a cross-contamination between exactly those two is invisible
//! to the run-end sweep (it stays sound: no false positives). The incremental
//! overlap check above still guards allocation-time overlap.
//!
//! # Limitations
//!
//! `drive` reads bytes it has not itself written in exactly two places: the
//! `alloc_zeroed` zero-check and the `realloc` prefix check. Those reads are
//! defined only if the allocator under test honours the *initialization*
//! half of the [`RawAllocator`] contract (see the
//! trait's `# Safety`: initialized bytes are a safety obligation, distinct
//! from the zero/prefix *values*, which are oracles). An allocator whose
//! `alloc_zeroed` hands back genuinely uninitialized memory, or whose
//! `realloc` moves a block without copying the old bytes, violates that
//! obligation: natively, `drive` still reports the oracle failure correctly,
//! but under miri it reports undefined behaviour inside `drive` itself
//! rather than a clean oracle failure. (`read_volatile` is not a fix: it is
//! equally UB on uninitialized memory in the abstract machine, so it would
//! only hide the report.)
//!
//! # The allocator seam
//!
//! The driver is generic over a minimal `RawAllocator` trait — exactly the
//! `alloc` / `dealloc` / `realloc` / `alloc_zeroed` surface of
//! `core::alloc::GlobalAlloc`, for which a blanket impl is provided. A plain
//! owned allocator with the same four methods (e.g. sefer's `AllocCore`) can
//! implement the trait directly. (Coherence note: because of that blanket
//! impl, a type that already implements `GlobalAlloc` cannot ALSO implement
//! `RawAllocator` — wrap it in a newtype if you need to override the
//! forwarding.)
//!
//! # Reentrancy
//!
//! `drive` allocates its own bookkeeping (one `Vec<Live>`) through the
//! *global* allocator. If the allocator under test is also the installed
//! `#[global_allocator]`, those internal allocations interleave with the ops
//! under test, and a reentrant allocator will deadlock or recurse. Drive the
//! *engine* behind your `GlobalAlloc`, not the installed global allocator
//! itself.
//!
//! # Example
//!
//! ```text
//! use globalalloc_model::{drive, op_strategy, Config};
//! use std::alloc::System;
//!
//! // proptest (`proptest!` comes from the `proptest` crate):
//! proptest! {
//!     #[test]
//!     fn matches_model(ops in op_strategy(Config::default(), 0..200)) {
//!         drive(&System, Config::default(), &ops);
//!     }
//! }
//!
//! // libFuzzer (`fuzz_target!` comes from `libfuzzer-sys`):
//! fuzz_target!(|stream: globalalloc_model::OpStream| {
//!     drive(&System, Config::default(), &stream.ops);
//! });
//! ```

// This crate holds `unsafe` for two reasons. (1) Its one job includes calling
// the allocator-under-test's raw-pointer API and dereferencing the pointers it
// hands back — inherently `unsafe`: the `RawAllocator` trait is `unsafe` (its
// impls must return valid pointers for the requested layout), and the oracle
// loop writes/reads through a pointer the allocator just returned for the size
// it was asked for. Every such site carries a `// SAFETY:` note. (2)
// `DoubleFreeOk::new` is an unforgeable consent token for the M2 double-free
// oracle — an `unsafe fn` declaration with no raw pointer involved, carrying
// its own `# Safety` doc rather than a `// SAFETY:` note.
#![allow(unsafe_code)]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// The model itself (this crate) needs only `core` + `alloc` — no
// allocator-under-test needs `std` to be differential-tested, so the default
// build stays usable against a `no_std` allocator's own test suite. The
// `proptest` front-end is no_std-compatible too: it is declared with
// `default-features = false, features = ["alloc", "no_std"]` (which routes
// proptest's float samplers through num-traits/libm) and was verified to
// build against `thumbv7em-none-eabi`. The one exception is the `arbitrary`
// front-end: `derive_arbitrary`'s generated recursion guard unconditionally
// references `std::thread_local!` — an upstream limitation, not this crate's
// choice.
#![cfg_attr(not(feature = "arbitrary"), no_std)]

extern crate alloc;

#[cfg(feature = "arbitrary")]
mod arbitrary_stream;
mod config;
mod double_free_ok;
mod drive;
mod op;
mod raw_allocator;
#[cfg(feature = "proptest")]
mod strategy;

#[cfg(feature = "arbitrary")]
#[cfg_attr(docsrs, doc(cfg(feature = "arbitrary")))]
pub use arbitrary_stream::OpStream;
pub use config::Config;
pub use double_free_ok::DoubleFreeOk;
pub use drive::drive;
pub use op::Op;
pub use raw_allocator::RawAllocator;
#[cfg(feature = "proptest")]
#[cfg_attr(docsrs, doc(cfg(feature = "proptest")))]
pub use strategy::op_strategy;
