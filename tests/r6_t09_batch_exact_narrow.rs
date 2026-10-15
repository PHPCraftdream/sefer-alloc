//! R6-T09 (R6-CR-1 / R6-SEC-11a): `alloc_batch`/`dealloc_batch` must go
//! through the same `ExactNarrow` fork as the scalar `GlobalAlloc` methods.
//! Harness-free: one process, sequential checks in `main`.
//!
//! Counterfactual: without the routing, `dealloc_batch` sends the narrow
//! block into `HeapCore::dealloc_batch`/`publish_foreign`, no route matches,
//! `foreign_or_unroutable_frees` grows and `ExactNarrow::dbg_counts().1`
//! stays flat; `alloc_batch` slots come from the heap, so their addresses
//! have no exact-table descriptor.

use sefer_alloc::global::exact_object::ExactNarrow;
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn narrow_layout() -> Layout {
    Layout::from_size_align(8, 8).expect("valid layout")
}

fn main() {
    check_dealloc_batch();
    check_alloc_batch();
    println!("[r6_t09_batch_exact_narrow] COMPLETE");
}

fn check_dealloc_batch() {
    let layout = narrow_layout();
    let (a0, d0) = ExactNarrow::dbg_counts();
    let foreign0 = GLOBAL.stats().foreign_or_unroutable_frees;
    // SAFETY: matched alloc/dealloc pair with the same layout.
    let p = unsafe { GLOBAL.alloc(layout) };
    assert!(!p.is_null(), "narrow alloc failed");
    assert!(
        ExactNarrow::dbg_generation_of(p.addr()).is_some(),
        "scalar alloc did not register an exact descriptor"
    );
    // SAFETY: live allocation from this allocator, exact layout; the null
    // entry is always skipped by contract.
    unsafe { GLOBAL.dealloc_batch(layout, &[p, core::ptr::null_mut()]) };
    let (_, d1) = ExactNarrow::dbg_counts();
    assert_eq!(
        d1 - d0,
        1,
        "dealloc_batch must free the narrow block through ExactNarrow"
    );
    assert_eq!(
        GLOBAL.stats().foreign_or_unroutable_frees,
        foreign0,
        "routed narrow free must not be counted as foreign/unroutable"
    );
    let _ = a0;
}

fn check_alloc_batch() {
    let layout = narrow_layout();
    let (a0, _) = ExactNarrow::dbg_counts();
    let mut out: [*mut u8; 4] = [core::ptr::null_mut(); 4];
    // SAFETY: non-zero layout; every filled slot is freed below.
    let filled = unsafe { GLOBAL.alloc_batch(layout, &mut out) };
    assert_eq!(filled, out.len(), "narrow alloc_batch must fill every slot");
    for &p in out.iter().take(filled) {
        assert!(!p.is_null());
        assert!(
            ExactNarrow::dbg_generation_of(p.addr()).is_some(),
            "alloc_batch slot was not served by ExactNarrow"
        );
    }
    let (a1, _) = ExactNarrow::dbg_counts();
    assert!(
        a1 - a0 >= out.len(),
        "alloc_batch must be served by ExactNarrow"
    );
    // SAFETY: each slot is a live allocation made with `layout`.
    unsafe { GLOBAL.dealloc_batch(layout, &out) };
}
