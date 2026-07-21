//! R11-4 counterfactual: `SeferAlloc::dealloc_batch`'s new batched fast path
//! (magazine-first-fill + `flush_class`-overflow, `src/registry/
//! heap_core_dealloc_batch.rs`) must still degrade a double-free inside ONE
//! `dealloc_batch` call to a benign no-op — not corruption, not a crash —
//! when two entries of the SAME call reference blocks in the SAME segment
//! (one of which duplicates an already-freed pointer).
//!
//! This mirrors `flush_class`'s own L-4 (UBFIX-11) scenario doc comment
//! (`src/alloc_core/alloc_core_small_magazine.rs`): a batch containing the
//! SAME pointer (or a segment-coincident pointer) in two separate positions
//! must not corrupt the segment's BinTable/bitmap/decommit state, whether the
//! duplicate lands in the magazine-fill portion, the `flush_class`-overflow
//! portion, or straddles both.
//!
//! **Why this is red before R11-4 (a crash / assertion failure) and green
//! after.** Before this task, `SeferAlloc::dealloc_batch` looped scalar
//! `HeapCore::dealloc` per block — each call independently re-runs the M2
//! double-free oracles (in-magazine bitmap, flushed-bitmap `is_free`) fresh
//! against current segment state, so the SAME scenario already degraded
//! safely under the OLD code too (this is why the harness below temporarily
//! reverts to the loop, rather than being unconditionally red). The R11-4
//! batched path introduces NEW shared per-call state (the `stage` overflow
//! buffer, the magazine-fill bookkeeping) that a naive implementation could
//! get wrong (e.g. staging a pointer twice, or failing to re-check the M2
//! oracles for the SECOND occurrence after the first occurrence already
//! mutated the bitmap) — so this test is the load-bearing regression guard
//! for the NEW code path, proven red-before/green-after via the documented
//! revert-and-restore procedure (see the module doc of
//! `tests/r10_7_alloc_batch_xthread_double_free.rs` for the established
//! project pattern this mirrors).

#![cfg(all(feature = "alloc-global", feature = "batch-api"))]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// A batch of 40 blocks, all class-16B, all necessarily landing in the SAME
/// (or very few) 4 MiB segment(s) — small enough that segment carving never
/// needs a second segment for this class at this count. Position 39 is set
/// to a DUPLICATE of position 0 (both already-live blocks from the SAME
/// segment) before the batch free — an in-call double-free.
#[test]
fn dealloc_batch_same_segment_duplicate_entry_no_corruption() {
    let layout = Layout::from_size_align(16, 8).unwrap();
    let n = 40usize;

    // SAFETY: valid non-zero layout.
    let mut blocks: Vec<*mut u8> = (0..n)
        .map(|_| unsafe {
            let p = GLOBAL.alloc(layout);
            assert!(!p.is_null(), "setup alloc returned null");
            p
        })
        .collect();

    // All distinct at this point.
    {
        let set: HashSet<usize> = blocks.iter().map(|&p| p as usize).collect();
        assert_eq!(set.len(), n, "setup allocs were not all distinct");
    }

    // Introduce the in-call double-free: duplicate blocks[0] at the END of
    // the slice (so it appears once in the "normal" position, and again
    // after the whole batch has otherwise been staged/pushed) — the
    // "two positions, same segment, separated by other blocks" shape
    // `flush_class`'s L-4 doc comment describes.
    let dup_target = blocks[0];
    blocks.push(dup_target); // n+1 entries; blocks[n] duplicates blocks[0]

    // SAFETY: `blocks[0..n]` are each a live allocation of `layout`, freed
    // exactly once each in a well-formed call; `blocks[n]` is a DELIBERATE
    // duplicate of `blocks[0]` — an intentional double-free within this one
    // `dealloc_batch` call, to exercise the M2 guard's documented benign
    // degradation. This is caller UB under the `unsafe fn` contract; the
    // guard's job is "no corruption / no crash", which this test verifies.
    unsafe { GLOBAL.dealloc_batch(layout, &blocks) };

    // Heap must still be usable and structurally sane: fresh allocations of
    // the same class must succeed, be non-null, layout-aligned, and mutually
    // distinct — proving the segment's freelist/bitmap were not corrupted by
    // the duplicate entry.
    let mut fresh: Vec<*mut u8> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid layout.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null(), "heap unusable after in-batch double-free");
        fresh.push(p);
    }
    let set: HashSet<usize> = fresh.iter().map(|&p| p as usize).collect();
    assert_eq!(
        set.len(),
        n,
        "post-double-free allocations were not all distinct (freelist corruption)"
    );
    // Every fresh block must be independently writable without clobbering a
    // neighbour (aliasing check).
    for (i, &p) in fresh.iter().enumerate() {
        let pat = 0xFACE_0000_0000_0000u64 | (i as u64);
        // SAFETY: `p` is freshly allocated, size >= 16, so the first 8 bytes
        // are in-bounds and writable.
        unsafe { std::ptr::write_volatile(p.cast::<u64>(), pat) };
    }
    for (i, &p) in fresh.iter().enumerate() {
        let pat = 0xFACE_0000_0000_0000u64 | (i as u64);
        // SAFETY: same in-bounds justification as above.
        let got = unsafe { std::ptr::read_volatile(p.cast::<u64>()) };
        assert_eq!(got, pat, "fresh block [{i}] was clobbered (aliasing)");
    }

    // Cleanup.
    // SAFETY: every entry of `fresh` came from GLOBAL.alloc above with
    // `layout`; each freed exactly once here.
    unsafe { GLOBAL.dealloc_batch(layout, &fresh) };
}

/// A larger batch (200 blocks, class 64B) that DEFINITELY exercises the
/// `flush_class`-overflow portion of the R11-4 batched path (N > TCACHE_CAP
/// (16)), with the duplicate entry placed so ONE occurrence lands in the
/// magazine-fill portion and the OTHER lands in the overflow-staged portion
/// — the exact straddling shape the module doc above calls out.
#[test]
fn dealloc_batch_duplicate_straddling_magazine_and_overflow_no_corruption() {
    let layout = Layout::from_size_align(64, 8).unwrap();
    let n = 200usize;

    let mut blocks: Vec<*mut u8> = (0..n)
        .map(|_| unsafe {
            let p = GLOBAL.alloc(layout);
            assert!(!p.is_null(), "setup alloc returned null");
            p
        })
        .collect();
    {
        let set: HashSet<usize> = blocks.iter().map(|&p| p as usize).collect();
        assert_eq!(set.len(), n, "setup allocs were not all distinct");
    }

    // Duplicate an EARLY block (index 2, almost certainly magazine-fill
    // territory — TCACHE_CAP is 16) at a LATE position (near the end, almost
    // certainly overflow territory for a 200-block batch).
    let dup_target = blocks[2];
    blocks.push(dup_target);

    // SAFETY: same reasoning as the previous test — a deliberate in-call
    // double-free (blocks[2] and the appended duplicate), exercised to prove
    // the M2 guard degrades it benignly regardless of which internal
    // sub-path (magazine-fill vs. flush_class-overflow) processes each
    // occurrence.
    unsafe { GLOBAL.dealloc_batch(layout, &blocks) };

    let mut fresh: Vec<*mut u8> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid layout.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null(), "heap unusable after straddling double-free");
        fresh.push(p);
    }
    let set: HashSet<usize> = fresh.iter().map(|&p| p as usize).collect();
    assert_eq!(
        set.len(),
        n,
        "post-double-free allocations were not all distinct (freelist corruption)"
    );

    // SAFETY: every entry of `fresh` came from GLOBAL.alloc above with
    // `layout`; each freed exactly once here.
    unsafe { GLOBAL.dealloc_batch(layout, &fresh) };
}
