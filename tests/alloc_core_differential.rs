//! Differential property test for the Phase 8 segment substrate (`alloc-core`).
//!
//! Models `AllocCore` against the reference model + M1–M4 oracles that now live
//! in the `globalalloc-model` crate (the single shared harness for all three
//! in-repo differential copies — this test, `tests/heap_differential.rs`, and
//! `fuzz/fuzz_targets/global_alloc_ops.rs`). See `docs/INVARIANTS.md` M1–M4 and
//! the crate's docs for the oracle definitions. This file keeps ONLY the
//! sefer-specific wiring: the `AllocCore`-under-test adapter and the size
//! distribution / case count.
//!
//! Per the short-scenario policy (`CLAUDE.md`): ~64 cases, small sizes so the
//! suite (and miri over it) finishes quickly. Sizes are kept small so most
//! allocations exercise the small free-list path; a few large ones exercise the
//! dedicated-segment path. `double_free: true` — `AllocCore`'s contract is that
//! a redundant `dealloc` of an already-freed pointer is a safe no-op (M2), so
//! the shared oracle is asked to exercise it here.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::cell::RefCell;

use globalalloc_model::{drive, op_strategy, Config, RawAllocator};
use proptest::prelude::*;
use sefer_alloc::AllocCore;

/// The allocator-under-test adapter: `AllocCore`'s methods take `&mut self`, so
/// wrap it in a `RefCell` to present the shared harness's `&self` `RawAllocator`
/// surface. Single-threaded, no reentrancy — the borrow never overlaps.
struct CoreUnderTest(RefCell<AllocCore>);

// SAFETY: each method takes `&mut` on the inner `AllocCore` for the duration of
// one non-reentrant call and forwards to the matching inherent method, honoring
// the `RawAllocator` contract (valid/aligned pointers, no-op double-free per
// `AllocCore`'s own M2 guarantee, realloc prefix preservation).
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

/// Size distribution for this copy: small (`1..=4096`) and large (`4097..=2 MiB`)
/// arms in equal proportion, aligns up to 4096 — the historical
/// `alloc_core_differential` shape. `double_free` exercises `AllocCore`'s M2
/// no-op guarantee.
fn config() -> Config {
    Config {
        small_max: 4096,
        large_max: 2 * 1024 * 1024,
        small_weight: 1,
        large_weight: 1,
        max_align: 4096,
        double_free: true,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn alloc_core_matches_reference_model(ops in op_strategy(config(), 0..200)) {
        let alloc = CoreUnderTest(RefCell::new(AllocCore::new().expect("primordial bootstrap")));
        drive(&alloc, config(), &ops);
    }
}
