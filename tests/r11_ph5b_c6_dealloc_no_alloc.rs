//! C6 (ph5b hardening matrix, gap #4, task #2094): explicit oracle that
//! **`dealloc` does not allocate** — freeing Small and Large blocks must not
//! reserve a single new OS segment, mint a heap slot, or drop a free.
//!
//! Oracle: process-wide `SeferAlloc::stats()` counters are snapshotted
//! immediately before and immediately after a pure-dealloc phase (warm-up and
//! the live blocks are allocated BEFORE the first snapshot, so any reservation
//! they need happens outside the measured window). Asserted deltas:
//!   - `segments_reserved_total` UNCHANGED — the core "no allocation happened"
//!     signal, monotonic and always available in every feature combination;
//!   - `foreign_or_unroutable_frees` UNCHANGED — the deallocs were all routed
//!     normally, none were dropped;
//!   - `heaps_claimed_high_water` UNCHANGED — no registry slot was minted.
//!
//! Deltas only, never absolute values: test files are separate processes but
//! other `#[test]`s in THIS binary share the process-global counters, and the
//! counters are cumulative since process start.

#![cfg(feature = "alloc-global")]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::{AllocStats, SeferAlloc};

/// One Small shape and one Large shape (comfortably above `SMALL_MAX` in
/// every feature combination this file builds under, so it always classifies
/// Large and exercises the Large dealloc/cache leg).
const SMALL_LAYOUT: Layout = match Layout::from_size_align(512, 8) {
    Ok(l) => l,
    Err(_) => panic!("bad small layout"),
};
const LARGE_LAYOUT: Layout = match Layout::from_size_align(2 * 1024 * 1024, 8) {
    Ok(l) => l,
    Err(_) => panic!("bad large layout"),
};

fn snapshot(a: &SeferAlloc) -> AllocStats {
    a.stats()
}

#[test]
fn dealloc_of_small_and_large_blocks_allocates_nothing() {
    let a = SeferAlloc::new();

    // ── Warm-up: one full round-trip per shape so tcache magazines, the
    // large-object cache, and any lazily-created segments are in their steady
    // state BEFORE the measured window. (After warm-up a dealloc deposits
    // into caches instead of touching the OS; a hypothetical phantom alloc
    // inside dealloc would have to come from somewhere observable.)
    // SAFETY: `a` is a live allocator handle; layouts are non-zero.
    unsafe {
        for _ in 0..2 {
            let s = a.alloc(SMALL_LAYOUT);
            assert!(!s.is_null(), "warm-up small alloc failed");
            a.dealloc(s, SMALL_LAYOUT);
            let l = a.alloc(LARGE_LAYOUT);
            assert!(!l.is_null(), "warm-up large alloc failed");
            a.dealloc(l, LARGE_LAYOUT);
        }
    }

    // ── Materialise the live set BEFORE the first snapshot: 8 Small + 4
    // Large. Any segment reservations these need happen now, outside the
    // measured window.
    const N_SMALL: usize = 8;
    const N_LARGE: usize = 4;
    let mut small = [core::ptr::null_mut::<u8>(); N_SMALL];
    let mut large = [core::ptr::null_mut::<u8>(); N_LARGE];
    // SAFETY: live allocator, non-zero layouts.
    unsafe {
        for slot in small.iter_mut() {
            *slot = a.alloc(SMALL_LAYOUT);
            assert!(!slot.is_null(), "live small alloc failed");
        }
        for slot in large.iter_mut() {
            *slot = a.alloc(LARGE_LAYOUT);
            assert!(!slot.is_null(), "live large alloc failed");
        }
    }

    let before = snapshot(&a);

    // ── The measured window: PURE dealloc. N Small + N Large frees, nothing
    // else.
    // SAFETY: each pointer is a live allocation with its exact layout.
    unsafe {
        for slot in small.iter() {
            a.dealloc(*slot, SMALL_LAYOUT);
        }
        for slot in large.iter() {
            a.dealloc(*slot, LARGE_LAYOUT);
        }
    }

    let after = snapshot(&a);

    assert_eq!(
        after.segments_reserved_total, before.segments_reserved_total,
        "dealloc reserved OS segments (before={}, after={}) — the free path \
         ALLOCATED; dealloc must be allocation-free",
        before.segments_reserved_total, after.segments_reserved_total
    );
    assert_eq!(
        after.foreign_or_unroutable_frees, before.foreign_or_unroutable_frees,
        "dealloc dropped frees (before={}, after={}) — the free path did not \
         route normally",
        before.foreign_or_unroutable_frees, after.foreign_or_unroutable_frees
    );
    assert_eq!(
        after.heaps_claimed_high_water, before.heaps_claimed_high_water,
        "dealloc minted registry heap slots (before={}, after={})",
        before.heaps_claimed_high_water, after.heaps_claimed_high_water
    );
}
