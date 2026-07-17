//! Differential property test for the Phase 8 segment substrate (`alloc-core`
//! feature). Models `AllocCore` against the reference model + M1–M4 oracles that
//! now live in the `globalalloc-model` crate (the single shared harness — see
//! `tests/alloc_core_differential.rs` and `fuzz/fuzz_targets/global_alloc_ops.rs`
//! for the sibling consumers). This file keeps ONLY the sefer-specific wiring:
//! the `AllocCore`-under-test adapter and this copy's size distribution.
//!
//! Per the short-scenario policy: ~64 cases, small sizes.
//!
//! (An earlier version drove this through the now-removed `Heap` wrapper;
//! `Heap` was a pure pass-through to `AllocCore` on the single-thread `alloc`
//! feature, so this is a faithful 1:1 substitution.)
//!
//! This copy deliberately does NOT set `double_free`: it exercises the
//! own-thread free path once per pointer (mirroring the historical
//! `heap_differential`, which — unlike `alloc_core_differential` — never
//! double-freed). The realloc-tail re-fill and prefix-preservation oracles are
//! shared with its siblings via the crate.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::cell::RefCell;

use globalalloc_model::{drive, op_strategy, Config, RawAllocator};
use proptest::prelude::*;
use sefer_alloc::AllocCore;

/// The allocator-under-test adapter (see `tests/alloc_core_differential.rs` for
/// the rationale): `AllocCore`'s methods take `&mut self`, wrapped in a
/// `RefCell` to present the shared harness's `&self` `RawAllocator` surface.
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

/// Size distribution for this copy: mostly small (`9`), occasionally large
/// (`1`), capped at 128 KiB (SMALL_MAX is ~94 KiB) to keep the suite fast per
/// the short-scenario policy (no multi-MiB byte-by-byte writes). Aligns up to
/// 4096. `double_free: false` — matches the historical `heap_differential`.
fn config() -> Config {
    Config {
        small_max: 4096,
        large_max: 128 * 1024,
        small_weight: 9,
        large_weight: 1,
        max_align: 4096,
        double_free: false,
    }
}

proptest! {
    // `failure_persistence: None` — do not write a regressions file (avoids the
    // "SourceParallel failed to find lib.rs" abort under some run layouts and
    // keeps runs hermetic), matching the Phase 7d / Phase 8 differential tests.
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn heap_matches_reference_model(ops in op_strategy(config(), 0..200)) {
        let heap = CoreUnderTest(RefCell::new(AllocCore::new().expect("heap bootstrap")));
        drive(&heap, config(), &ops);
    }
}
