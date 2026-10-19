//! R6-T32 — the segment directory must be rebuilt lazily on fallback-scan
//! entry (`find_segment_with_free_impl`), so a non-growing heap recovers
//! directory routing after a cold trim without new reservations.
//!
//! Counterfactual — what is NOT usable as evidence: on the broken (null
//! sidecar) code path the DIRECTORY_FALLBACK_SCANS counter is useless as a
//! red oracle, because its increment lives INSIDE the non-null-sidecar block
//! (src/alloc_core/small/alloc_core_small/find_segment/scan.rs, `linear_scan`) — with a
//! null sidecar it stays at zero on BOTH old and fixed code. The oracles
//! actually asserted here are:
//!   * FULL_SCAN_SLOTS_EXAMINED — on the broken code the sidecar stays null
//!     after the trim, so every post-trim lookup falls into the linear scan
//!     (it stops at the first segment with a free block, so the delta is
//!     positive but not a fixed multiple of the segment count); on the fixed
//!     code each lookup is a validated directory hit that returns BEFORE the
//!     fallback scan (find_segment.rs: directory-hit return ~:404 precedes the
//!     scan ~:452), so the delta over the measured section is 0.
//!   * DIRECTORY_HITS — grows by ≥ 11 on the fixed code, grows by 0 on the
//!     broken code (no sidecar to route through).
//!
//! Note on `trust_negative`: this test drives a standalone `AllocCore::new()`
//! with no owner, so directory negatives are trusted here; the oracles above
//! do not depend on that.
//!
//! Run (this exact line also compiles/runs the file in CI, .github/workflows/
//! ci.yml:1997):
//!   cargo test --features "production alloc-stats bench-internals internals" --test r6_t32_directory_rebuild_after_trim

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-segment-directory",
    feature = "alloc-stats",
    feature = "bench-internals",
    feature = "internals"
))]

use core::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn directory_rebuilds_after_cold_trim_without_growth() {
    let mut core = AllocCore::new().expect("primordial");

    // ── Phase A: push the heap's small-segment count past
    // DIRECTORY_MATERIALIZE_THRESHOLD so the directory materialises. Mirrors
    // tests/segment_directory_a2.rs `push_past_threshold`: allocate
    // SMALL_MAX-sized blocks (few per 2 MiB segment) so a handful of allocs
    // cross the threshold. Keep ALL pointers live.
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let small_max = SegmentLayout::SMALL_MAX;
    let layout = Layout::from_size_align(small_max, 1).expect("valid layout");
    let hit_class =
        SegmentLayout::class_for(small_max, 1).expect("SMALL_MAX must resolve to a small class");

    let mut ptrs: Vec<*mut u8> = Vec::new();
    let max_allocs = (threshold as usize + 5) * 20;
    for _ in 0..max_allocs {
        let p = core.alloc(layout);
        if p.is_null() {
            // OOM in a constrained environment — clean up and skip.
            eprintln!("OOM during phase-A fill in constrained environment — skipping");
            for &q in &ptrs {
                // SAFETY (R6-MS-1/2): q from a prior matching alloc above,
                // live, freed exactly once.
                unsafe { core.dealloc(q, layout) };
            }
            return;
        }
        ptrs.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(
        core.dbg_table_count() > threshold,
        "failed to push table count past the materialise threshold"
    );
    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised past the threshold"
    );

    // ── Phase B: create a directory-HIT class. Free exactly ONE held pointer
    // of `hit_class` — this sets the class_nonempty bit for (hit_class, slot).
    let freed = ptrs.pop().expect("at least one held pointer");
    // SAFETY (R6-MS-1/2): `freed` was returned by a prior matching alloc in
    // this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(freed, layout) };

    // Pre-trim sanity: the oracle counters actually move on a directory hit.
    let hits_before = AllocCore::dbg_directory_hits();
    let slots_before = AllocCore::dbg_full_scan_slots_examined();
    let seg = core
        .dbg_find_segment_with_free(hit_class)
        .expect("pre-trim directory hit");
    assert!(!seg.is_null());
    let hits_before2 = AllocCore::dbg_directory_hits();
    assert!(
        hits_before2 > hits_before,
        "DIRECTORY_HITS must increment on a pre-trim directory hit (oracle sanity)"
    );
    let slots_before2 = AllocCore::dbg_full_scan_slots_examined();
    let _ = (slots_before, slots_before2); // pre-trim scan cost is irrelevant

    // ── Phase C: cold trim via the test-only forwarder of
    // `release_directory_for_cold_trim` (its only production caller,
    // HeapCore::trim_cold_retention, is unreachable in an alloc-core build).
    core.dbg_release_directory_for_cold_trim();
    assert!(
        !core.dbg_directory_is_materialised(),
        "cold trim must drop the directory sidecar"
    );

    // ── Phase D: warmup rebuild. One discovery call on a DIFFERENT class; hit
    // or miss does not matter — on the FIXED code this call's fallback path
    // lazily re-materialises the directory. On the BROKEN code the sidecar
    // stays null here, so this assert alone is already red pre-fix.
    let warm_class = if hit_class == 1 { 2 } else { 1 };
    let _ = core.dbg_find_segment_with_free(warm_class);
    assert!(
        core.dbg_directory_is_materialised(),
        "first post-trim discovery must lazily rebuild the directory (R6-T32)"
    );

    // ── Phase E: measured section. Record counters AFTER the rebuild, then do
    // 11 lookups of the SAME hit_class. Each is a validated directory hit (the
    // freed block stays free — we never pop via alloc), and each returns
    // BEFORE the fallback scan on the fixed code.
    let hits0 = AllocCore::dbg_directory_hits();
    let slots0 = AllocCore::dbg_full_scan_slots_examined();
    for i in 0..11 {
        let seg = core
            .dbg_find_segment_with_free(hit_class)
            .unwrap_or_else(|| panic!("post-trim directory hit #{i}"));
        assert!(!seg.is_null());
    }
    let hits_delta = AllocCore::dbg_directory_hits() - hits0;
    let slots_delta = AllocCore::dbg_full_scan_slots_examined() - slots0;
    assert!(
        hits_delta >= 11,
        "directory routing must be restored after the cold trim: \
         DIRECTORY_HITS delta = {hits_delta} over 11 lookups (R6-T32)"
    );
    assert_eq!(
        slots_delta, 0,
        "post-trim hit-class lookups must be short-circuited by directory hits \
         (0 full-scan slots); on the broken code (sidecar never rebuilt without \
         growth) each lookup walks all ≥33 Small slots → delta ≥ 33·11 (R6-T32)"
    );

    // Remaining held blocks stay live until drop — AllocCore::drop handles
    // teardown (mirrors tests/segment_directory_a2.rs).
    drop(ptrs);
}
