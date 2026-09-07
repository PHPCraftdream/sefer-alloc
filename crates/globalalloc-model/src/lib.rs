//! Reference-model testing for raw Rust allocators.
//!
//! [`drive`] applies `alloc`, `dealloc`, `realloc`, and `alloc_zeroed`
//! operations while tracking the expected live blocks. It checks each property
//! when that property is observable:
//!
//! - **M1 (allocation response and read-back):** the driver treats null from
//!   `alloc` or `alloc_zeroed` as a test failure even though `GlobalAlloc`
//!   permits it as an allocation-failure result. A written fill is then read
//!   back from each non-null extent. Null `realloc` is accepted and leaves the
//!   old block live.
//! - **M2 (authorized double-free is a no-op):** an unsafe, off-by-default
//!   [`DoubleFreeOk`] token authorizes one immediate repeated `dealloc`. The
//!   oracle can observe only corruption visible at a later check; it cannot
//!   directly prove that the repeated call did nothing.
//! - **M3 (no live-block overlap):** each non-null block-creating return is
//!   compared with the other live extents before entering the model.
//! - **M4 (alignment):** each non-null return is checked before access.
//! - `alloc_zeroed` bytes are checked for zero, and a non-null `realloc` result
//!   is checked for preservation of its initialized old prefix.
//!
//! Live fills are verified before a block is passed to `dealloc` or `realloc`
//! and before each teardown deallocation. These are discrete observation
//! points, not continuous monitoring.
//!
//! # Front-ends and features
//!
//! The core API and hand-built [`Op`] streams need no feature. The optional
//! `proptest` feature exposes `op_strategy`; consumers also need their own
//! `proptest` dependency for its macros and traits. The optional `arbitrary`
//! feature exposes `OpStream` for cargo-fuzz/libFuzzer consumers. The two
//! features are independent and additive.
//!
//! The names `op_strategy` and `OpStream` are intentionally plain code rather
//! than intra-doc links because they are absent from a default-feature docs
//! build.
//!
//! # `no_std`
//!
//! Without `arbitrary`, the crate uses only `core` and `alloc`. The `proptest`
//! dependency is configured for its `alloc` and `no_std` modes. A no-std
//! proptest consumer gets deterministic seeding unless another dependency
//! enables proptest's `std` support. The `arbitrary` front-end currently needs
//! `std` because derive-generated code uses `std::thread_local!`.
//!
//! # Safety and oracle limits
//!
//! [`RawAllocator`] is unsafe because the driver cannot safely discover every
//! invalid allocator result. A non-null pointer must already denote a live
//! allocation with the promised extent. A dangling or undersized result can
//! make the driver's first access undefined behavior instead of producing an
//! oracle failure.
//!
//! The driver reads bytes it did not write in the `alloc_zeroed` zero check and
//! the `realloc` prefix check. The former bytes, and the old prefix supplied to
//! the latter operation, must be initialized under the [`RawAllocator`]
//! contract. Reading genuinely uninitialized memory is undefined behavior both
//! natively and under Miri; neither execution mode reliably reports it as an
//! oracle failure.
//!
//! Fill bytes cycle through `1..=255`. Once more than 255 fill assignments are
//! represented among live blocks, two can share a marker, so corruption from
//! one such block into the other may collide with the expected value. Direct
//! extent-overlap checks still run when each block is created. Corruption that
//! occurs and is restored between observation points is also invisible.
//!
//! On normal return, every surviving modeled block is deallocated. An oracle
//! panic may intentionally leak tracked and candidate allocations: after an
//! invalid or overlapping pointer is observed, there is no generic cleanup
//! sequence that can deallocate every suspect pointer without risking undefined
//! behavior or a double-free. Callers that catch the panic and continue the
//! process must account for those leaks.
//!
//! The generators bound individual allocation requests, but cannot guarantee
//! the absence of OOM: live requests can accumulate and allocators may have
//! tighter limits. [`drive`] treats null from `alloc` or `alloc_zeroed` as a
//! test failure. Null from `realloc` is the supported failure result and leaves
//! the old block live.
//!
//! Hand-built zero and oversized sizes are clamped into the common
//! `GlobalAlloc` size preconditions. An inadmissible alignment (including zero
//! or a non-power-of-two value) is rejected instead. For generated streams,
//! `small_max == 0` produces a size-1 small arm, while
//! `large_max <= small_max` produces a large arm at `small_max + 1`; that
//! degenerate arm can exceed `large_max`.
//!
//! # Allocator seam and reentrancy
//!
//! Every `GlobalAlloc` has a blanket [`RawAllocator`] implementation. A plain
//! allocator engine can implement the trait directly; a type that already
//! implements `GlobalAlloc` needs a newtype to override the blanket behavior.
//!
//! [`drive`] allocates its `Vec` bookkeeping through the installed global
//! allocator. If that allocator is also under test, bookkeeping allocations
//! can interleave with the explicit stream. This does not inherently deadlock
//! (driving `std::alloc::System` is a normal use), but it can perturb allocator
//! state, and an implementation unsafe against its own re-entry may recurse or
//! deadlock. Drive the underlying engine directly when bookkeeping must be
//! isolated from the operations under test.
//!
//! # Manual example
//!
//! Add `globalalloc-model = "0.1"` under `[dev-dependencies]`, then use a
//! complete program of this shape:
//!
//! ```text
//! use globalalloc_model::{drive, Config, Op};
//! use std::alloc::System;
//!
//! fn main() {
//!     let ops = [
//!         Op::Alloc { size: 64, align: 8 },
//!         Op::Realloc { i: 0, new_size: 96 },
//!         Op::Dealloc(0),
//!     ];
//!     drive(&System, Config::default(), &ops);
//! }
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
