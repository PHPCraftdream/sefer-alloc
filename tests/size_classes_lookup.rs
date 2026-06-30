//! Phase 13.1 — O(1) size-class lookup correctness.
//!
//! `SizeClasses::class_for` was a per-alloc **linear scan** of the 40-entry
//! `SIZE_CLASS_TABLE`; Phase 13.1 replaced the small path with an O(1)
//! `const` lookup table `SIZE2CLASS` (derived from the table at compile time,
//! so the two cannot drift). This test asserts that replacement is exact: for
//! every small `size` (1..=SMALL_MAX) and a spread of alignments, the O(1)
//! lookup agrees with an **independently re-implemented linear scan** over the
//! public `SIZE_CLASS_TABLE` — the same algorithm the old `class_for` used.
//!
//! Non-vacuous: if the lookup's index arithmetic, the `MIN_BLOCK_SHIFT`, or
//! the derived table ever drifts from the source-of-truth table (off-by-one,
//! wrong bucket boundary, a class gap), this test fails. The counterfactual
//! holds — reverting `class_for` to a naive/wrong lookup breaks at least one
//! assertion here.

#![cfg(feature = "alloc-core")]

use sefer_alloc::SegmentLayout;

/// The reference classifier — a faithful re-implementation of the current
/// contract, rebuilt here over the public `SIZE_CLASS_TABLE` so the test does
/// not trust the crate's own `class_for`. Returns the smallest class index
/// whose `block_size >= max(size, align)` AND `block_size % align == 0`
/// (the divisibility predicate established in task #114 — without it any
/// `align > MIN_BLOCK` request would burn a full Large segment, exhausting
/// `SegmentTable` under concurrent task-spawning workloads).
fn linear_scan_class_for(size: usize, align: usize) -> Option<usize> {
    let need = if size > align { size } else { align };
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let mut i = 0;
    while i < table.len() {
        if table[i] >= need && table[i] % align == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The O(1) lookup under test, accessed via the same public hatch. This is
/// `SegmentLayout::class_for` (which delegates to `SizeClasses::class_for`).
fn o1_class_for(size: usize, align: usize) -> Option<usize> {
    SegmentLayout::class_for(size, align)
}

// ---------------------------------------------------------------------------
// 1. Full sweep: every small size, every small alignment → lookup == scan.
// ---------------------------------------------------------------------------

#[test]
fn o1_lookup_matches_linear_scan_for_every_small_size_and_align() {
    let small_aligns = [1usize, 2, 4, 8, 16];
    // size starts at 1 (the raw layout contract minimum); class_for's caller
    // clamps to MIN_BLOCK, but the lookup is well-defined for size >= 1 here
    // because (size-1) >> shift stays in-range for 1..=SMALL_MAX.
    for align in small_aligns {
        for size in 1..=SegmentLayout::SMALL_MAX {
            let got = o1_class_for(size, align);
            let want = linear_scan_class_for(size, align);
            assert_eq!(
                got, want,
                "drift at size={size} align={align}: O(1)={got:?} scan={want:?}"
            );
            // And the resolved class's block must actually fit the request
            // (M4 fidelity — the lookup must never return a too-small class).
            if let Some(idx) = got {
                let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
                let need = if size > align { size } else { align };
                assert!(
                    block >= need,
                    "size={size} align={align}: class {idx} block={block} < need={need}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Direct table-vs-derived agreement: SIZE2CLASS[k] == scan for every bucket.
//    This isolates the compile-time derivation (build_size2class) from the
//    per-call index arithmetic, so a bug in either is pinned separately.
// ---------------------------------------------------------------------------

#[test]
fn size2class_table_matches_linear_scan_for_every_bucket() {
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let s2c = SegmentLayout::SIZE2CLASS;
    let min_block = SegmentLayout::MIN_BLOCK;
    let small_max = SegmentLayout::SMALL_MAX;
    // Bucket k (indexed via (size-1)>>shift) covers sizes in
    // (k*MIN_BLOCK, (k+1)*MIN_BLOCK]; the class stored must cover the *largest*
    // size in that bucket, i.e. need = min((k+1)*MIN_BLOCK, SMALL_MAX).
    for (k, &class_idx) in s2c.iter().enumerate() {
        let need = ((k + 1) * min_block).min(small_max);
        // Reference: smallest class whose block >= need.
        let mut want = 0;
        while want < table.len() {
            if table[want] >= need {
                break;
            }
            want += 1;
        }
        assert_eq!(
            class_idx as usize, want,
            "SIZE2CLASS[{k}] = {class_idx} but scan says {want} (need={need})"
        );
        // Sanity: the class block really covers the bucket's largest size.
        assert!(
            table[class_idx as usize] >= need,
            "SIZE2CLASS[{k}] → block {} < need {need}",
            table[class_idx as usize]
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Boundary cases (the off-by-one traps).
// ---------------------------------------------------------------------------

#[test]
fn boundary_size_one_returns_class_zero() {
    // size==1, align==1: need = max(1,1) = 1 → smallest class (class 0,
    // block == MIN_BLOCK). The (1-1)>>shift = 0 index must hit class 0.
    assert_eq!(o1_class_for(1, 1), Some(0));
    assert_eq!(linear_scan_class_for(1, 1), Some(0));
}

#[test]
fn boundary_size_min_block_returns_class_zero() {
    // size == MIN_BLOCK exactly: (MIN_BLOCK - 1) >> shift == 0 → class 0.
    let mb = SegmentLayout::MIN_BLOCK;
    assert_eq!(o1_class_for(mb, 1), Some(0));
    assert_eq!(o1_class_for(mb, mb), Some(0));
}

#[test]
fn boundary_size_just_above_a_class_uses_next_class() {
    // Pick class 0's block (== MIN_BLOCK). size = MIN_BLOCK + 1 must resolve to
    // class 1 (the next class), NOT class 0 — this is the classic off-by-one
    // the (size-1) shift is designed to get right.
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let just_over = table[0] + 1; // MIN_BLOCK + 1
    assert_eq!(o1_class_for(just_over, 1), Some(1));
    assert_eq!(
        o1_class_for(just_over, 1),
        linear_scan_class_for(just_over, 1)
    );
    // And sitting exactly on a class boundary returns that class (not the next).
    let on_boundary = table[5]; // an interior class
    let idx = o1_class_for(on_boundary, 1).expect("interior class is small");
    assert_eq!(table[idx], on_boundary);
}

#[test]
fn boundary_size_small_max_is_small_not_large() {
    // size == SMALL_MAX exactly is still the small path (the largest class).
    let smax = SegmentLayout::SMALL_MAX;
    let last = SegmentLayout::SIZE_CLASS_TABLE.len() - 1;
    assert_eq!(o1_class_for(smax, 1), Some(last));
    assert_eq!(
        o1_class_for(smax, SegmentLayout::SMALL_ALIGN_MAX),
        Some(last)
    );
}

#[test]
fn boundary_size_above_small_max_is_large() {
    // size == SMALL_MAX + 1 → large path (None), even with small align.
    let smax = SegmentLayout::SMALL_MAX;
    assert_eq!(o1_class_for(smax + 1, 1), None);
    assert_eq!(
        o1_class_for(smax + 1, 1),
        linear_scan_class_for(smax + 1, 1)
    );
}

#[test]
fn boundary_align_above_small_align_max_resolves_to_divisible_class() {
    // Task #114: align > SMALL_ALIGN_MAX no longer unconditionally forces the
    // Large path. The new contract is "smallest class with block_size
    // >= max(size, align) AND block_size % align == 0".
    //
    // For align=32, size=1: need=32, and every class block_size is a
    // multiple of MIN_BLOCK=16, so the smallest class with block ≥ 32 AND
    // divisible by 32 must exist — and it must equal the linear-scan result
    // (the reference impl in this file mirrors the new contract).
    let over = SegmentLayout::SMALL_ALIGN_MAX * 2; // 32
    let got = o1_class_for(1, over);
    let want = linear_scan_class_for(1, over);
    assert_eq!(got, want, "o1 vs scan drift at size=1 align={over}");
    // It must actually resolve (Some), not fall through to Large — class 1
    // has block_size = 32 which fits both the size and the divisibility
    // predicate. (If `build_table` ever changes the geometry such that
    // there is no align-32-divisible class ≤ SMALL_MAX, this assertion
    // becomes the canary.)
    let idx = got.expect("a class with block_size divisible by 32 must exist");
    let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
    assert!(block >= over);
    assert_eq!(block % over, 0, "block {block} not divisible by {over}");
}

#[test]
fn tokio_task_cell_shape_resolves_to_small_class_not_large() {
    // Task #114 regression — the OOM shape that motivated the fix.
    //
    // `tokio::runtime::task::core::Cell<T,S>` is `#[repr(align(128))]` against
    // false sharing and lays out ~640 B per task. Pre-fix, every `tokio::spawn`
    // burned one whole 4 MiB Large segment (align=128 > SMALL_ALIGN_MAX=16 →
    // Large path), and under a concurrent spawn workload the `SegmentTable`
    // (MAX_SEGMENTS=1024) exhausted, then `alloc_large_slow → register` returned
    // None → the `GlobalAlloc` face returned null → `handle_alloc_error` aborted
    // the process with "memory allocation of 640 bytes failed".
    //
    // Post-fix: (640, 128) must resolve to a small class whose block_size is
    // ≥ 640 AND a multiple of 128. The small path serves it from a normal
    // per-segment free list — no per-spawn 4 MiB segment, no `SegmentTable`
    // pressure.
    //
    // Counterfactual: reverting `class_for` to the pre-fix "if align >
    // SMALL_ALIGN_MAX → None" gate makes this test fail with `got=None`.
    let got = o1_class_for(640, 128);
    let idx = got.expect("(size=640, align=128) must resolve to a small class");
    let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
    assert!(block >= 640, "block {block} too small for size=640");
    assert_eq!(block % 128, 0, "block {block} not divisible by align=128");
    // Same property for adjacent async-runtime alignments (32 / 64 / 256):
    // a small class with the right divisibility must exist and the resolver
    // must find it (not fall through to Large).
    for &align in &[32usize, 64, 128, 256] {
        let idx = o1_class_for(640, align)
            .unwrap_or_else(|| panic!("(640, {align}) should resolve to small"));
        let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
        assert!(block >= 640.max(align));
        assert_eq!(block % align, 0, "block {block} not multiple of align={align}");
    }
}

// ---------------------------------------------------------------------------
// 4. Structural sanity: MIN_BLOCK_SHIFT is log2(MIN_BLOCK); SMALL_MAX is a
//    multiple of MIN_BLOCK (so the lookup index stays in-bounds at the top).
//    These pin the invariants the O(1) arithmetic leans on.
// ---------------------------------------------------------------------------

#[test]
fn min_block_shift_is_log2_of_min_block() {
    let mb = SegmentLayout::MIN_BLOCK;
    assert!(mb.is_power_of_two(), "MIN_BLOCK must be a power of two");
    assert_eq!(
        SegmentLayout::MIN_BLOCK_SHIFT,
        mb.trailing_zeros(),
        "MIN_BLOCK_SHIFT must equal log2(MIN_BLOCK)"
    );
    assert_eq!(1usize << SegmentLayout::MIN_BLOCK_SHIFT, mb);
}

#[test]
fn small_max_is_a_multiple_of_min_block_so_top_index_is_in_bounds() {
    let mb = SegmentLayout::MIN_BLOCK;
    let smax = SegmentLayout::SMALL_MAX;
    assert_eq!(
        smax % mb,
        0,
        "SMALL_MAX must be a multiple of MIN_BLOCK (else SIZE2CLASS top index is wrong)"
    );
    // The top live index (size == SMALL_MAX) must be the last valid array slot.
    let top_idx = (smax - 1) >> SegmentLayout::MIN_BLOCK_SHIFT;
    assert!(top_idx < SegmentLayout::SIZE2CLASS.len());
}

// ---------------------------------------------------------------------------
// 5. Counterfactual sanity: a deliberately-wrong lookup must fail this suite.
//    (Documented, not executed — proves the suite is non-vacuous by showing
//    what it catches. A buggy `(size) >> shift` (forgetting the -1) would
//    mis-resolve size==MIN_BLOCK to a too-large class and fail test #3.)
// ---------------------------------------------------------------------------

#[test]
fn counterfactual_naive_shift_without_minus_one_would_be_wrong() {
    // The CORRECT arithmetic is (size - 1) >> shift. A naive `size >> shift`
    // sends size==MIN_BLOCK to bucket 1 instead of 0 — wrong. We compute both
    // and assert they differ at size==MIN_BLOCK, demonstrating this suite would
    // catch that regression (test #3's boundary_size_min_block_returns_class_zero
    // directly asserts class 0).
    let mb = SegmentLayout::MIN_BLOCK;
    let shift = SegmentLayout::MIN_BLOCK_SHIFT;
    let correct = (mb - 1) >> shift;
    let naive = mb >> shift;
    assert_ne!(correct, naive, "counterfactual no longer distinguishes");
    assert_eq!(correct, 0);
}
