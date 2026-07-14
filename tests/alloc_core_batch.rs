//! Integration tests for `AllocCore::refill_class` / `flush_class` (P1 of the
//! fastbin / tcache substrate). Verifies that the batch APIs are thin wrappers
//! around the existing single-block primitives and produce identical observable
//! effects.
//!
//! Per project convention: tests live in `tests/`, not inline.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::{AllocCore, SegmentLayout};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Derive the class index for a given (size, align) via the public
/// `dbg_layout_class_for` accessor.
fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

/// Compute segment base of a pointer (public via `SegmentLayout`).
fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

// ---------------------------------------------------------------------------
// T-refill-equiv: refill_class returns N non-null, distinct pointers; they
// round-trip through individual dealloc without panic.
// ---------------------------------------------------------------------------

#[test]
fn t_refill_equiv_class0_n8() {
    t_refill_equiv_inner(16, 8, 8);
}

#[test]
fn t_refill_equiv_class0_n16() {
    t_refill_equiv_inner(16, 8, 16);
}

#[test]
fn t_refill_equiv_class0_n64() {
    t_refill_equiv_inner(16, 8, 64);
}

#[test]
fn t_refill_equiv_medium_n16() {
    // A medium class: 256 bytes, align 8.
    t_refill_equiv_inner(256, 8, 16);
}

fn t_refill_equiv_inner(size: usize, align: usize, n: usize) {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);

    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n, "refill_class returned {got}, expected {n} (OOM?)");

    // Every pointer is non-null.
    for (i, &ptr) in buf.iter().enumerate() {
        assert!(!ptr.is_null(), "buf[{i}] is null after refill");
    }

    // Every pointer is distinct (conservation: no duplicates).
    let unique: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(unique.len(), n, "refill_class returned duplicate pointers");

    // Round-trip: dealloc each individually through the public API.
    let layout = Layout::from_size_align(size, align).unwrap();
    for &ptr in &buf {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(ptr, layout) };
    }

    // Allocator still works after the round-trip.
    let check = core.alloc(layout);
    assert!(!check.is_null(), "alloc after round-trip returned null");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(check, layout) };
}

// ---------------------------------------------------------------------------
// T-refill-bump-equiv (P3, Э1): refill_class_bump produces the same
// observable end-state as refill_class — N non-null, distinct pointers that
// round-trip through dealloc — while skipping the BinTable carve→pop
// tautology for freshly-carved blocks.
// ---------------------------------------------------------------------------

#[test]
fn t_refill_bump_equiv_class0_n8() {
    t_refill_bump_equiv_inner(16, 8, 8);
}

#[test]
fn t_refill_bump_equiv_class0_n64() {
    t_refill_bump_equiv_inner(16, 8, 64);
}

#[test]
fn t_refill_bump_equiv_medium_n16() {
    t_refill_bump_equiv_inner(256, 8, 16);
}

fn t_refill_bump_equiv_inner(size: usize, align: usize, n: usize) {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);

    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    // NOTE: refill_class_bump fills up to out.len(); pass an exactly-sized
    // slice so `want == n`.
    let got = core.refill_class_bump(c, &mut buf);
    assert_eq!(got, n, "refill_class_bump returned {got}, expected {n}");

    for (i, &ptr) in buf.iter().enumerate() {
        assert!(!ptr.is_null(), "buf[{i}] is null after bump refill");
    }
    let unique: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(
        unique.len(),
        n,
        "refill_class_bump returned duplicate pointers"
    );

    // Round-trip: every bump-carved block must free cleanly. If bump-direct
    // had wrongly left a block bitmap-FREE, dealloc_small's is_free guard
    // would no-op the free (M2), and a subsequent re-refill would hand back a
    // DIFFERENT pointer (see the counterfactual in this file).
    let layout = Layout::from_size_align(size, align).unwrap();
    for &ptr in &buf {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(ptr, layout) };
    }

    // Re-refill: the just-freed blocks (now on the BinTable) must be reused —
    // free-drain runs BEFORE any new carve, so the SAME address set comes
    // back (LIFO order aside).
    let mut buf2 = vec![core::ptr::null_mut::<u8>(); n];
    let got2 = core.refill_class_bump(c, &mut buf2);
    assert_eq!(got2, n, "re-refill after free returned {got2}");
    let reused: HashSet<usize> = buf2.iter().map(|p| *p as usize).collect();
    assert_eq!(
        reused, unique,
        "bump refill did not reuse freed blocks (source order broken — \
         carved fresh instead of draining the free list first)"
    );

    for &ptr in &buf2 {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(ptr, layout) };
    }
}

// ---------------------------------------------------------------------------
// T-refill-bump-partial (P3): with a short OUT slice, refill_class_bump fills
// exactly out.len() and no more; want==0 fills nothing.
// ---------------------------------------------------------------------------

#[test]
fn t_refill_bump_len_bound() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);

    let mut empty: [*mut u8; 0] = [];
    assert_eq!(core.refill_class_bump(c, &mut empty), 0);

    let mut buf = vec![core::ptr::null_mut::<u8>(); 5];
    let got = core.refill_class_bump(c, &mut buf);
    assert_eq!(got, 5, "expected exactly out.len() filled");
    let layout = Layout::from_size_align(16, 8).unwrap();
    for &p in &buf {
        assert!(!p.is_null());
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
}

// ---------------------------------------------------------------------------
// T-flush-equiv: refill N, then flush_class all N back. After flush, a
// re-refill returns valid pointers (round-trip).
// ---------------------------------------------------------------------------

#[test]
fn t_flush_equiv_class0_n16() {
    t_flush_equiv_inner(16, 8, 16);
}

#[test]
fn t_flush_equiv_medium_n16() {
    t_flush_equiv_inner(256, 8, 16);
}

fn t_flush_equiv_inner(size: usize, align: usize, n: usize) {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);

    // Phase 1: refill.
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n);

    // Phase 2: flush all back.
    core.flush_class(c, &buf);

    // Phase 3: re-refill — the flushed blocks should be reusable. The
    // allocator may hand them back in any order (the BinTable is LIFO), but
    // every returned pointer must be non-null and distinct.
    let mut buf2 = vec![core::ptr::null_mut::<u8>(); n];
    let got2 = core.refill_class(c, n, &mut buf2);
    assert_eq!(got2, n, "re-refill after flush returned {got2}");
    for (i, &ptr) in buf2.iter().enumerate() {
        assert!(!ptr.is_null(), "buf2[{i}] is null after re-refill");
    }
    let unique: HashSet<usize> = buf2.iter().map(|p| *p as usize).collect();
    assert_eq!(unique.len(), n);

    // Clean up.
    let layout = Layout::from_size_align(size, align).unwrap();
    for &ptr in &buf2 {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(ptr, layout) };
    }
}

// ---------------------------------------------------------------------------
// T-refill-spans-segments: refill enough blocks to span multiple segments.
// ---------------------------------------------------------------------------

#[test]
fn t_refill_spans_segments() {
    let mut core = AllocCore::new().unwrap();
    // 16-byte blocks. A small segment is ~4 MiB; metadata takes some space,
    // so we need enough blocks to overflow one segment. 2000 × 16 = 32 KiB
    // which easily fits in one segment. We need more: a segment can hold
    // roughly (4 MiB - metadata) / 16 blocks. Let's ask for a number that
    // requires at least 2 segments. We'll use a conservative estimate.
    //
    // Actually: the refill batch carving (REFILL_BATCH=31) means one segment
    // fills faster because each alloc_small carves 32 blocks. A 4 MiB segment
    // with ~16 B blocks holds ~250K blocks. That's a lot. Let's use a larger
    // block size (1024 B) and a higher N to force segment overflow more easily.
    //
    // 4 MiB / 1024 B = ~4096 blocks per segment (minus metadata). Asking for
    // 5000 should span at least 2 segments.
    let size = 1024usize;
    let align = 8usize;
    let n = 5000usize;
    let c = class_for(&core, size, align);

    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n, "refill_class returned {got}, expected {n}");

    // Every pointer is non-null and distinct.
    for (i, &ptr) in buf.iter().enumerate() {
        assert!(!ptr.is_null(), "buf[{i}] is null");
    }
    let unique: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(
        unique.len(),
        n,
        "duplicate pointers in multi-segment refill"
    );

    // At least 2 distinct segment bases.
    let bases: HashSet<usize> = buf.iter().map(|&p| seg_base(p)).collect();
    assert!(
        bases.len() >= 2,
        "expected >= 2 segment bases, got {} (all pointers in one segment?)",
        bases.len(),
    );

    // Clean up.
    let layout = Layout::from_size_align(size, align).unwrap();
    for &ptr in &buf {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(ptr, layout) };
    }
}

// ---------------------------------------------------------------------------
// T-flush-decommit (alloc-decommit only): flush all blocks of a segment,
// verify live_count reaches 0 and decommit fires.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-decommit")]
#[test]
fn t_flush_decommit() {
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool so this
    // decommit-hook test stays deterministic. With the pool ON (production
    // default) the ≥3 emptied segments here are ABSORBED by the 4-slot pool
    // (retained committed, no decommit) — the hook would never fire. Disabling
    // the pool exercises exactly the flush→decommit path this test covers (still
    // fully live under `production`, whenever the pool is full or disabled). Pool
    // behaviour is covered by `tests/small_segment_pool.rs`.
    let mut core = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .unwrap();
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);

    // We need at least 3 segments: the primordial (kind=Primordial, never
    // decommitted), a second (kind=Small), and a third (kind=Small, becomes
    // small_cur). The second segment, once fully flushed, should decommit.
    //
    // A 4 MiB segment with ~1024-byte blocks holds ~4000 blocks (minus
    // metadata). To span 3 segments we need ~12000 blocks.
    let n = 12_000usize;
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n);

    // Group blocks by segment base.
    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    for &ptr in &buf {
        by_base.entry(seg_base(ptr)).or_default().push(ptr);
    }
    assert!(
        by_base.len() >= 3,
        "need >= 3 segments for decommit test (primordial + small + small_cur), got {}",
        by_base.len(),
    );

    let before = AllocCore::dbg_decommit_count();

    // Flush ALL blocks. The non-current Small segments should decommit when
    // their live_count reaches 0. The primordial never decommits (by design:
    // it hosts the self-hosted registry). The current segment (small_cur)
    // won't decommit either.
    core.flush_class(c, &buf);

    let after = AllocCore::dbg_decommit_count();
    // At least one Small segment (the second one that filled up) should have
    // decommitted.
    assert!(
        after > before,
        "dbg_decommit_count did not increase after flushing all blocks \
         (before={before}, after={after}, segments={}). \
         Decommit should fire for empty non-current Small segments.",
        by_base.len(),
    );
}

// ---------------------------------------------------------------------------
// T-flush-null-defensive: flush_class with nulls in the slice does not panic.
// ---------------------------------------------------------------------------

#[test]
fn t_flush_null_defensive() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);

    // A slice containing only nulls — should be a no-op.
    let nulls = [core::ptr::null_mut::<u8>(); 4];
    core.flush_class(c, &nulls);

    // Allocator still works.
    let layout = Layout::from_size_align(16, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(ptr, layout) };
}

// ---------------------------------------------------------------------------
// T-refill-zero: refill with want=0 returns 0 and does nothing.
// ---------------------------------------------------------------------------

#[test]
// The zero-sized array literal is intentional: `refill_class` must accept an
// empty output slice when `want == 0` and do nothing. Clippy's lint flags
// the repeat-expr-in-zero-len-array pattern generically, but there is no
// side effect here to hoist — `null_mut()` is a pure const-like call.
#[allow(clippy::zero_repeat_side_effects)]
fn t_refill_zero() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let mut buf = [core::ptr::null_mut::<u8>(); 0];
    let got = core.refill_class(c, 0, &mut buf);
    assert_eq!(got, 0);
}

// ---------------------------------------------------------------------------
// T-flush-double-free-guard: flush the same blocks twice — the second flush
// is a no-op (M2 double-free guard in dealloc_small catches it).
// ---------------------------------------------------------------------------

#[test]
fn t_flush_double_free_is_noop() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 64, 8);
    let n = 8;
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n);

    // First flush — normal.
    core.flush_class(c, &buf);
    // Second flush of the same pointers — M2 double-free guard should no-op.
    core.flush_class(c, &buf);

    // Allocator still works.
    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(ptr, layout) };
}

// ---------------------------------------------------------------------------
// T-counterfactual (documentation, not a breakage test):
//
// If mark_alloc were skipped inside refill_class (i.e., alloc_small were
// bypassed and we just did raw pointer arithmetic), then:
//   - flush_class -> dealloc_small -> is_free would see the bitmap bit as 0
//     (free), triggering the M2 double-free guard, and the flush would be a
//     no-op instead of actually freeing the block.
//   - T-flush-equiv's re-refill would then get DIFFERENT pointers (the
//     flushed blocks never actually returned to the free list), and eventually
//     OOM when segments fill up.
//
// This validates that refill_class correctly runs the full alloc_small path
// (bitmap mark_alloc + inc_live) so that flush_class's dealloc_small sees
// the block as "allocated" and correctly frees it.
// ---------------------------------------------------------------------------
