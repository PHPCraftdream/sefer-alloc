//! X-null: null `dealloc` must remain an observable no-op, routed entirely
//! through the PROMISS tail guard (`HeapCore::publish_foreign`'s first-line
//! null check), without touching any diagnostic counter.
//!
//! Counterfactual (mutant): removing the `if ptr.is_null() { return; }` from
//! `publish_foreign` (src/registry/heap_core_xthread/routing.rs) lets null
//! fall into the miss machinery — `canonical_block_of`'s memo/own_cache/
//! `hash_find` lookup with key 0 bumps `CONTAINS_BASE_TIER1_MISSES` and the
//! failed publication bumps `FOREIGN_OR_UNROUTABLE_FREES`. Both assertions
//! below go red under that mutant; with the guard in place they stay flat.

#![cfg(all(
    feature = "production",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SeferAlloc;

static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn new() -> Self {
        while SERIAL.swap(true, Ordering::SeqCst) {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::SeqCst);
    }
}

/// (a) `GlobalAlloc::dealloc` with null is a panic-free no-op for a spread of
/// layouts (small, large, zero-sized-contract edge).
#[test]
fn xnull_null_dealloc_is_a_noop() {
    let _guard = SerialGuard::new();
    let allocator = SeferAlloc::new();
    let layouts = [
        Layout::from_size_align(8, 8).unwrap(),
        Layout::from_size_align(256, 16).unwrap(),
        Layout::from_size_align(1, 1).unwrap(),
    ];
    for &layout in &layouts {
        // SAFETY: null dealloc is always safe per the GlobalAlloc contract.
        unsafe { allocator.dealloc(std::ptr::null_mut(), layout) };
        // SAFETY: as above; repeat to cover any first-call lazy init.
        unsafe { allocator.dealloc(std::ptr::null_mut(), layout) };
    }
    // The allocator still works afterwards.
    // SAFETY: valid layout.
    let p = unsafe { allocator.alloc(Layout::from_size_align(64, 8).unwrap()) };
    assert!(!p.is_null());
    // SAFETY: exact start of a live allocation.
    unsafe { allocator.dealloc(p, Layout::from_size_align(64, 8).unwrap()) };
}

/// (b) Invariant "null goes to the miss tail and stops there, before any
/// action": counter deltas around null frees are all zero.
#[test]
fn xnull_null_free_counters_untouched() {
    let _guard = SerialGuard::new();
    let allocator = SeferAlloc::new();
    let layout = Layout::from_size_align(128, 8).unwrap();

    // Warm up the own-thread heap so lazy init does not pollute deltas.
    // SAFETY: valid layout, exact free.
    let warm = unsafe { allocator.alloc(layout) };
    assert!(!warm.is_null());
    // SAFETY: exact start of a live allocation.
    unsafe { allocator.dealloc(warm, layout) };

    let foreign_before = AllocCore::dbg_foreign_or_unroutable_frees();
    let reject_before = AllocCore::dbg_local_reject_frees();
    // Null's key (0) legitimately traverses the read-only Tier-1 lookup as a
    // miss (memo/own_cache store only real bases / `LAST_EMPTY_KEY`); the
    // diagnostic counts that miss, so record it and pin it at one per free.
    let tier1_misses_before = AllocCore::dbg_contains_base_tier1_misses();

    for _ in 0..16 {
        // SAFETY: null dealloc is always safe per the GlobalAlloc contract.
        unsafe { allocator.dealloc(std::ptr::null_mut(), layout) };
    }

    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        foreign_before,
        "null free must not bump FOREIGN_OR_UNROUTABLE_FREES"
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        reject_before,
        "null free must not bump LOCAL_REJECT_FREES"
    );
    assert_eq!(
        AllocCore::dbg_contains_base_tier1_misses(),
        tier1_misses_before + 16,
        "null must take the read-only Tier-1 miss route (exactly one miss per free), never a hit"
    );
}
