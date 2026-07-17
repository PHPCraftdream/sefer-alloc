//! libFuzzer target for the Phase 8–11 allocator descent — exercises
//! `sefer_alloc::AllocCore` (the segment substrate that `SeferAlloc` /
//! `GlobalAlloc` is built on) with an `arbitrary`-derived op stream, checking
//! the M1–M4 invariants from `docs/INVARIANTS.md`.
//!
//! The op model, reference model, and M1–M4 oracles now live in the shared
//! `globalalloc-model` crate — this is the third consumer of that one harness
//! (the sibling proptest copies are `tests/alloc_core_differential.rs` and
//! `tests/heap_differential.rs`). This target keeps ONLY the sefer-specific
//! wiring: the `AllocCore`-under-test adapter and the fuzz size/align
//! distribution (via the crate's `OpStream` `Arbitrary` front-end). An oracle
//! improvement in the crate reaches proptest, miri, AND this fuzzer at once.
//!
//! ## Why `AllocCore`, not the installed `SeferAlloc` global allocator
//!
//! `AllocCore` is the single-threaded engine under the `GlobalAlloc` face; it
//! has a plain owned API (`new` / `alloc` / `dealloc` / `realloc` /
//! `alloc_zeroed`) that drops cleanly per fuzz input. Routing the libFuzzer
//! harness's own allocations through the installed process-wide `SeferAlloc`
//! `#[global_allocator]` is out of scope for op-stream invariant fuzzing (the
//! installed path is proven separately by `tests/global_alloc_installed.rs`
//! and `examples/tokio_burn_in.rs`).
//! The cross-thread ordering path is covered by the TSan + aarch64 CI gates and
//! the loom harnesses, not by this single-threaded structure-aware fuzzer.
//!
//! # How to run (Linux only)
//!
//! libFuzzer requires the nightly toolchain and does NOT run on Windows. From
//! the `fuzz/` directory:
//!
//! ```text
//! cargo +nightly fuzz run global_alloc_ops
//! cargo +nightly fuzz run global_alloc_ops -- -max_total_time=3600
//! cargo +nightly fuzz run global_alloc_ops -- artifact.bin
//! ```

// This target drives the allocator's raw-pointer API via the shared harness;
// the writes/reads through returned pointers are confined inside the crate. The
// only local `unsafe` is the `RawAllocator` adapter forwarding to `AllocCore`.

#![no_main]

use std::alloc::Layout;
use std::cell::RefCell;

use globalalloc_model::{drive, Config, OpStream, RawAllocator};
use libfuzzer_sys::fuzz_target;
use sefer_alloc::AllocCore;

/// The allocator-under-test adapter: `AllocCore`'s methods take `&mut self`, so
/// wrap it in a `RefCell` to present the shared harness's `&self` `RawAllocator`
/// surface. Single-threaded, no reentrancy — the borrow never overlaps.
struct CoreUnderTest(RefCell<AllocCore>);

// SAFETY: forwards each `RawAllocator` method to the matching `&mut self`
// inherent method under a non-reentrant single-threaded borrow.
unsafe impl RawAllocator for CoreUnderTest {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0.borrow_mut().alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        self.0.borrow_mut().alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarding the caller's `dealloc` contract to `AllocCore`.
        unsafe { self.0.borrow_mut().dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarding the caller's `realloc` contract to `AllocCore`.
        unsafe { self.0.borrow_mut().realloc(ptr, old_layout, new_size) }
    }
}

fuzz_target!(|stream: OpStream| {
    let alloc = match AllocCore::new() {
        Some(a) => CoreUnderTest(RefCell::new(a)),
        None => return, // primordial bootstrap failed (OS refused mmap); skip.
    };
    // `double_free: true` — `AllocCore`'s M2 contract is that a redundant free
    // of an already-freed pointer is a safe no-op; the crate's `OpStream` front
    // end already bounds sizes (1..=2 MiB) and aligns (2^0..2^21).
    let config = Config { double_free: true, ..Config::default() };
    drive(&alloc, config, &stream.ops);
});
