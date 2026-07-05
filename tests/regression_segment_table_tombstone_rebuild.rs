//! W2 — regression: `SegmentTable` tombstone-rebuild kills the long-horizon
//! perf-metastability ("tombstone wear").
//!
//! ## The bug this guards against
//!
//! `SegmentTable`'s open-addressing hash answers `contains_base` in O(1).
//! Deletion (`unregister`/`recycle`) writes a `TOMBSTONE` sentinel. Pre-W2,
//! tombstones NEVER converted back to empty (no backward-shift deletion, no
//! rebuild): the transitions empty→live, live→tombstone, tombstone→live all
//! existed but nothing→empty did not. So `#empty` was monotonically
//! non-increasing over the process lifetime. Every register/unregister cycle
//! with a FRESH base (large-cache eviction, decommit-recycle, ASLR) consumed
//! one empty slot forever. Once `#empty` reached 0 (live ≤ 1024, tombstones ≥
//! 1024), `hash_contains` of an ABSENT base — the HOT case, since every
//! cross-thread free begins with a `contains_base` MISS on the caller's own
//! table — probed the ENTIRE `HASH_CAPACITY` (2048) array before returning
//! `false`. A long-running server degraded to ~2048 metadata loads per
//! cross-thread free: a metastable perf collapse in exactly the DBMS/async
//! profile the crate targets. Not UB — a should-tier perf cliff.
//!
//! ## The fix (verified here)
//!
//! `SegmentTable` now counts tombstones exactly and, on the deletion paths
//! (`unregister`/`recycle`), rebuilds the hash from the authoritative dense
//! slot registry once tombstones exceed `HASH_CAPACITY / 4` (= 512). The
//! rebuild clears all tombstones (resetting `#empty`) and is amortised O(1)
//! per deletion.
//!
//! ## How this test drives distinct bases (the mechanism that creates wear)
//!
//! One alloc/free at a time does NOT create tombstone wear: the OS immediately
//! re-hands the just-freed virtual address, so the next `register` reuses the
//! very tombstone the `unregister` just wrote (a tombstone→live transition,
//! net zero). To force DISTINCT bases — the actual production trigger — we
//! HOLD a wave of `W` simultaneously-live large segments (each in its own
//! distinct 4 MiB+ segment, so the OS cannot reuse addresses across them),
//! then drain the whole wave. Draining `W > 512` distinct bases in one wave
//! guarantees the tombstone count crosses the rebuild threshold. We set the
//! large-cache budget to 0 so each free eagerly releases its OS reservation
//! (no cache retention masking the churn), matching the eviction/decommit
//! profile the fix targets.

// ===========================================================================
// (a) tombstone count stays BOUNDED (rebuild fires) AND
// (b) `contains_base` stays CORRECT across rebuilds.
// ===========================================================================

/// Drive many waves of hold-then-drain with `W > HASH_CAPACITY/4` distinct
/// bases per wave (so each wave crosses the rebuild threshold several times
/// over the run) and assert:
///
/// (a) `dbg_hash_tombstones()` stays BOUNDED — never exceeds
///     `HASH_CAPACITY/4` + a small margin — i.e. the rebuild actually fires and
///     resets the count. The counterfactual (verified by hand during
///     development: comment out the `maybe_rebuild_hash()` calls in
///     `segment_table.rs`) makes this counter climb monotonically past the
///     threshold toward `W` and stay there — the assertion then FAILS.
///
/// (b) `contains_base` remains EXACTLY correct across rebuilds: every
///     still-held base reads `true`; every freed base reads `false`. Checked
///     both while a wave is fully live (all `true`) and immediately after each
///     individual free (that base flips to `false`), so a rebuild that
///     corrupted membership would be caught at the free that triggered it.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)] // reserves hundreds of 4 MiB OS segments — too slow under miri.
#[test]
fn tombstone_rebuild_bounds_count_and_preserves_membership() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    let mut ac = AllocCore::new().expect("primordial");
    // Budget 0: every large free is rejected by the large-cache and eagerly
    // releases its OS reservation → the churn is not masked by cache retention.
    ac.dbg_set_large_cache_budget(Some(0));

    // A Large allocation (> SMALL_MAX) gets its own dedicated segment, so each
    // distinct pointer is a distinct segment base in the table.
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    // `HASH_CAPACITY = 2 * MAX_SEGMENTS = 2048`; the rebuild threshold is
    // `HASH_CAPACITY / 4 = 512`. `W` must exceed 512 so a single wave's drain
    // crosses the threshold. Mirrored here as a literal because the constants
    // are `pub(crate)` (not reachable from an integration test).
    const HASH_CAPACITY: u32 = 2048;
    const THRESHOLD: u32 = HASH_CAPACITY / 4; // 512
    const W: usize = 650; // > THRESHOLD (512), < MAX_SEGMENTS (1024)
    const WAVES: usize = 4;
    // The rebuild fires when tombstones EXCEED the threshold, so the observed
    // post-deletion maximum is exactly `THRESHOLD`. Allow a tiny margin for
    // robustness against off-by-one accounting.
    const BOUND: u32 = THRESHOLD + 8;

    let mut max_tombstones: u32 = 0;
    let mut crossed_threshold_at_least_once = false;

    for wave in 0..WAVES {
        // --- Hold: allocate W distinct live large segments. ---
        let mut ptrs = Vec::with_capacity(W);
        for i in 0..W {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "wave {wave}: alloc null at i={i}");
            ptrs.push(p);
        }

        // (b) While the whole wave is live, EVERY held base must be contained.
        for (i, &p) in ptrs.iter().enumerate() {
            assert!(
                ac.dbg_contains_base(p),
                "wave {wave}: held base i={i} not reported as contained \
                 (membership broken while live)"
            );
        }

        // --- Drain: free the whole wave. Each free unregisters/recycles a
        //     DISTINCT base → a distinct tombstone. Crossing THRESHOLD fires a
        //     rebuild. ---
        for (i, &p) in ptrs.iter().enumerate() {
            ac.dealloc(p, layout);

            let t = ac.dbg_hash_tombstones();
            max_tombstones = max_tombstones.max(t);
            if t >= THRESHOLD {
                crossed_threshold_at_least_once = true;
            }

            // (b) The just-freed base must now read as foreign (false),
            //     regardless of whether this free triggered a rebuild.
            assert!(
                !ac.dbg_contains_base(p),
                "wave {wave}: freed base i={i} still reported contained \
                 (membership corrupted, possibly by a rebuild)"
            );

            // (b) Every STILL-held base (later in the wave) must remain
            //     contained across the rebuild that a threshold-crossing free
            //     just performed. Only checked on the free that could have
            //     triggered a rebuild (t just dropped back to ~THRESHOLD/2),
            //     to keep the test fast: a rebuild that dropped a live base
            //     would flip one of these to false.
            if i + 1 < ptrs.len() && ac.dbg_hash_tombstones() < THRESHOLD / 2 {
                for &q in &ptrs[i + 1..] {
                    assert!(
                        ac.dbg_contains_base(q),
                        "wave {wave}: a still-held base vanished across a \
                         rebuild triggered at free i={i} (rebuild dropped a \
                         live entry)"
                    );
                }
            }
        }
    }

    // (a) The rebuild must actually have fired: without it, tombstones would
    //     climb monotonically toward W (650) and stay there. With it, the
    //     count is bounded at ~THRESHOLD.
    assert!(
        crossed_threshold_at_least_once,
        "test did not exercise the rebuild path — tombstones never reached the \
         threshold ({THRESHOLD}); driver is not creating enough distinct-base \
         churn to be a valid counterfactual"
    );
    assert!(
        max_tombstones <= BOUND,
        "tombstone count reached {max_tombstones}, exceeding the bound {BOUND} \
         (= HASH_CAPACITY/4 + margin) — the rebuild did NOT fire / did not \
         reset the count. This is the tombstone-wear perf cliff: #empty is \
         being consumed without bound."
    );
}
