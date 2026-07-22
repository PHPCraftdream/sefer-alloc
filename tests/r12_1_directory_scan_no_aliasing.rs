//! R12-1 (task #252) regression test — the directory-driven scan loop in
//! `find_segment_with_free_impl` must NOT hold a live `&SegmentDirectory`
//! reference across a call that can mutate the same sidecar allocation.
//!
//! ## The bug
//!
//! Before the fix, the scan loop (`alloc_core_small.rs`, around the R7-A3
//! block) did:
//!
//! ```text
//! let dir = os::deref_directory_sidecar(self.directory_sidecar); // &'static SegmentDirectory
//! for &nb in buckets... {
//!     let words = &dir.class_nonempty_by_node[nb][class_idx];
//!     for ... {
//!         self.validate_directory_candidate(...) // can call publish_empty /
//!                                                 // sync_directory_for_segment_classes,
//!                                                 // which call
//!                                                 // os::deref_directory_sidecar_mut(...)
//!                                                 // -> &'static mut SegmentDirectory
//!                                                 // on the SAME allocation
//!                                                 // while `dir` is still live.
//!     }
//! }
//! ```
//!
//! `&T` and `&mut T` simultaneously live over one allocation is aliasing UB
//! under Stacked/Tree Borrows, independent of the single-threaded owner
//! discipline (which only rules out a DATA RACE, not the aliasing-model
//! violation). The fix reads each word-array BY VALUE via
//! `os::read_directory_class_words` (a raw-pointer `.read()`, no reference
//! retained) instead of holding `dir` across the validation call.
//!
//! ## What this test proves
//!
//! A true Stacked/Tree-Borrows violation is not necessarily observable from
//! safe code in a debug build (the miri matrix is the tool that would catch
//! it directly, and the directory's above-threshold path is documented as
//! impractically slow under miri — see `segment_directory_a5_miri.rs`'s doc
//! comment; this was independently re-confirmed while building this fix: a
//! `push_past_threshold`-style materialisation did not finish in 180s under
//! `cargo miri test`). This test instead pins BEHAVIORAL EQUIVALENCE for the
//! exact interleaving the bug involved: it manufactures a directory hit whose
//! validation triggers the mutable-sidecar self-heal path (a ring-drain that
//! causes `sync_directory_for_segment_classes` to run) DURING the scan, and
//! asserts:
//!   1. the scan still returns a correct candidate (a real free block for
//!      `class_idx`), and
//!   2. the directory afterward EXACTLY matches a from-scratch rebuild (the
//!      established `assert_directory_equals_rebuild` oracle from
//!      `segment_directory_a2.rs` / `dirty_directory_incremental_sync.rs`).
//!
//! A regression that reintroduced the long-lived shared reference would still
//! pass this test in a debug build (aliasing UB is not guaranteed to corrupt
//! data under rustc's current codegen) — the hard guarantee against
//! reintroduction is the source-level shape of the fix (no `&SegmentDirectory`
//! binding spans a `validate_directory_candidate` call) plus this test
//! exercising the mutating interleaving so ANY future change that makes the
//! self-heal path actually corrupt state under the current toolchain would be
//! caught here.
//!
//! Deterministic and single-threaded: the cross-thread free is SIMULATED via
//! `dbg_push_to_ring` (no OS threads needed).
//!
//! Feature-gated behind `alloc-xthread` (ring/drain path) PLUS
//! `alloc-segment-directory` (the directory scan under test).

#![cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// Allocate until `table.count() > threshold`, returning pointers + class.
/// (Established helper pattern, copied from `segment_directory_a2.rs`.)
fn push_past_threshold(core: &mut AllocCore) -> (Vec<*mut u8>, usize) {
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let small_max = SegmentLayout::SMALL_MAX;
    let layout = Layout::from_size_align(small_max, 1).unwrap();
    let class_idx =
        SegmentLayout::class_for(small_max, 1).expect("SMALL_MAX must resolve to a small class");

    let mut ptrs: Vec<*mut u8> = Vec::new();
    let max_allocs = (threshold as usize + 5) * 20;
    for _ in 0..max_allocs {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "alloc returned null");
        ptrs.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(
        core.dbg_table_count() > threshold,
        "failed to push table count past threshold"
    );
    (ptrs, class_idx)
}

/// Assert that the incremental directory bitmap equals a fresh rebuild for
/// ALL (class, slot) pairs. (Established oracle, copied from
/// `segment_directory_a2.rs`.)
fn assert_directory_equals_rebuild(core: &mut AllocCore) {
    let class_count = AllocCore::dbg_small_class_count();
    let mut incremental = vec![vec![false; 1024]; class_count];
    for (c, row) in incremental.iter_mut().enumerate() {
        for (s, cell) in row.iter_mut().enumerate() {
            *cell = core.dbg_directory_get_bit(c, s).unwrap_or(false);
        }
    }

    let rebuilt = core.dbg_rebuild_directory();
    assert!(
        rebuilt,
        "directory should be materialised for this assertion"
    );

    for (c, row) in incremental.iter().enumerate() {
        for (s, &inc_val) in row.iter().enumerate() {
            let fresh = core.dbg_directory_get_bit(c, s).unwrap_or(false);
            assert_eq!(
                inc_val, fresh,
                "directory mismatch at class={c} slot={s}: \
                 incremental={inc_val}, rebuild={fresh}",
            );
        }
    }
}

/// The load-bearing scenario: a directory-driven scan hit whose validation
/// (`validate_directory_candidate`) triggers a ring drain that in turn calls
/// `sync_directory_for_segment_classes` — the mutable-sidecar self-heal path
/// — WHILE the scan loop is still iterating the same word snapshot the hit
/// came from. Before the fix this ran with a live `&SegmentDirectory` from
/// the SAME call still lexically in scope (aliasing UB); after the fix no
/// reference survives past the per-word `read_directory_class_words` copy.
///
/// ## R12-14 (task #265): `p`/`p2` use the SMALLEST class, not `SMALL_MAX`
///
/// The original version of this test carved BOTH `p` and `p2` at
/// `SegmentLayout::SMALL_MAX` — the same class `push_past_threshold` uses to
/// materialise the directory. That is fine under `production`/
/// `production,numa-aware-mock`, where `SMALL_MAX` (~253 KiB) still packs
/// ~16 blocks into one 4 MiB segment, so the segment `push_past_threshold`
/// left `small_cur` pointing at still has several residual free blocks from
/// its own refill batch by the time `p` is carved and freed.
///
/// Under `--all-features` (`medium-classes-wide` raises `SMALL_MAX` to
/// 1.75 MiB), a segment's PAYLOAD minus metadata overhead fits only ONE
/// `SMALL_MAX` block — every such segment is a class-`SMALL_MAX` singleton.
/// Freeing `p` (the sole occupant) sets the bit, but the immediately-following
/// `core.alloc(layout)` for `p2` pops that exact freed block right back
/// (`alloc_small`'s step-1 `pop_free(small_cur)` fast path) — `p2 == p`, the
/// free list is empty again, and the directory bit is cleared BEFORE the
/// scan under test ever runs, so `dbg_find_segment_with_free` correctly
/// returns `None` and the test's own premise ("the freed block is still
/// there to be found") never held for this feature combination — not a
/// scanner bug, a test-density assumption that only `production`'s smaller
/// `SMALL_MAX` satisfied.
///
/// Fix: use the smallest small class (a 1-byte request, resolved via
/// `dbg_layout_class_for` rather than a hardcoded class index) for `p`/`p2`
/// instead. A `MIN_BLOCK`-sized block packs thousands-per-segment
/// under EVERY feature combination (including `medium-classes-wide`), so `p`
/// and `p2` reliably land in the same segment as two genuinely DISTINCT live
/// pointers, exactly matching the scenario's intent: `p` is freed (sets the
/// bit), `p2` stays live and is pushed to the ring (simulating a pending
/// cross-thread free) — independent of `SMALL_MAX`'s per-feature block
/// density. `push_past_threshold` still uses `SMALL_MAX` (unchanged) purely
/// to cross `DIRECTORY_MATERIALIZE_THRESHOLD` in the fewest allocations.
#[test]
fn directory_hit_triggers_mutation_during_scan_stays_consistent() {
    let mut core = AllocCore::new().unwrap();

    // Materialise the directory (still via SMALL_MAX — fewest allocations to
    // cross the threshold; unrelated to the p/p2 density concern below).
    let (_threshold_ptrs, _threshold_class_idx) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let small_max = SegmentLayout::SMALL_MAX;
    let large_layout = Layout::from_size_align(small_max, 1).unwrap();

    // R12-14: the smallest small class — packs many blocks per segment
    // regardless of `medium-classes-wide` (see the doc comment above).
    let tiny_layout = Layout::from_size_align(1, 1).unwrap();
    let class_idx = core
        .dbg_layout_class_for(tiny_layout)
        .expect("a 1-byte allocation must resolve to the smallest small class");

    // Carve two tiny-class blocks in a row. The bump cursor is still on the
    // segment `push_past_threshold` left it on, which has ample room left
    // for many MIN_BLOCK-sized blocks even after being (near-)exhausted for
    // SMALL_MAX — so `p` and `p2` reliably land in the SAME segment as two
    // distinct live pointers.
    let p = core.alloc(tiny_layout);
    assert!(!p.is_null());
    let p2 = core.alloc(tiny_layout);
    assert!(!p2.is_null());
    assert_ne!(p, p2, "p and p2 must be distinct live pointers");
    assert_eq!(
        core.dbg_segment_id_of(p),
        core.dbg_segment_id_of(p2),
        "expected p and p2 (tiny class, carved back-to-back) to land in the \
         same segment"
    );

    // Free `p` locally — this creates a segment with a real free block (bit
    // SET) that the directory scan will pick up as a candidate. `p2` is
    // still live, so unlike the SMALL_MAX-density case this bit stays set
    // (the free list is not emptied by this single free).
    unsafe { core.dealloc(p, tiny_layout) };
    assert_eq!(
        core.dbg_directory_get_bit(class_idx, core.dbg_segment_id_of(p) as usize),
        Some(true),
        "freeing the block must set the directory bit for its segment"
    );

    // Now push a cross-thread-free note for `p2` — still live — into the
    // SAME segment's ring (simulated, single-threaded). This does not touch
    // the BinTable/directory yet — only `find_segment_with_free`'s ring-drain
    // step (inside `validate_directory_candidate`) will consume it, which is
    // exactly the call that runs `sync_directory_for_segment_classes` (the
    // mutable-sidecar self-heal path) DURING candidate validation.
    //
    // SAFETY: `p2` is a live pointer from `core.alloc(tiny_layout)` above, of
    // the same layout/class, not touched again until the drain below
    // consumes this note (per `dbg_push_to_ring`'s contract).
    let pushed = unsafe { core.dbg_push_to_ring(p2, class_idx) };
    assert!(pushed, "dbg_push_to_ring must succeed for a live block");

    // Load-bearing counterfactual: the ring note is UNCONSUMED at this point
    // (`p2` is not yet marked free) — proves the mutation-during-scan branch
    // below has not fired yet, so the assertion after the scan is meaningful
    // rather than vacuously true.
    assert!(
        !core.dbg_is_free_for(p2),
        "the pushed ring note must not be consumed before the scan runs"
    );

    // Directly drive the directory-driven scan (bypassing `alloc_small`'s
    // `pop_free(small_cur)` fast path) so the directory lookup — and thus
    // `validate_directory_candidate`'s ring-drain-triggered mutation — is
    // provably what serves this call.
    let found = core.dbg_find_segment_with_free(class_idx);
    assert!(
        found.is_some(),
        "the directory-driven scan must find the segment with the freed block"
    );

    // Load-bearing counterfactual (continued): `p2` MUST now be free — proves
    // the scan's `validate_directory_candidate` call actually drained the
    // ring (and therefore ran `sync_directory_for_segment_classes`, the
    // mutable-sidecar self-heal path) DURING this very scan, exercising the
    // exact interleaving the aliasing fix targets.
    assert!(
        core.dbg_is_free_for(p2),
        "the scan must have drained the ring note, marking p2 free"
    );

    // The oracle: after a scan that mutated the sidecar mid-iteration, the
    // incrementally-maintained directory must EXACTLY equal a fresh rebuild.
    assert_directory_equals_rebuild(&mut core);

    // Clean up remaining live allocations from `push_past_threshold` (not
    // load-bearing for the assertion above, but keeps the test tidy under
    // the allocator's own bookkeeping).
    for ptr in _threshold_ptrs {
        unsafe { core.dealloc(ptr, large_layout) };
    }
}
