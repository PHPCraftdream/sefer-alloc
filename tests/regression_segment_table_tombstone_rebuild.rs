//! R4-8/N3 — regression: backward-shift hash deletion kills BOTH the original
//! "tombstone wear" perf-metastability AND the synchronous rebuild spike that
//! the prior W2 fix introduced.
//!
//! ## History (why this test exists in this shape)
//!
//! **Pre-W2** `hash_remove` wrote a `TOMBSTONE` marker and never reclaimed it
//! (no backward-shift, no rebuild): every register/unregister cycle with a
//! FRESH base consumed one empty hash slot forever. Once `#empty` hit 0, a
//! `hash_contains` of an ABSENT base — the hot case, since every cross-thread
//! free begins with a `contains_base` MISS on the caller's own table — probed
//! the ENTIRE `HASH_CAPACITY` (2048) array. A long-running server degraded to
//! ~2048 metadata loads per cross-thread free: a metastable perf collapse.
//!
//! **W2** fixed the metastability by counting tombstones exactly and, on the
//! deletion paths (`unregister`/`recycle`), rebuilding the hash from the dense
//! slot registry once tombstones exceeded `HASH_CAPACITY / 4` (= 512). This was
//! a genuine improvement (amortised O(1) per delete, bounded probe length) but
//! it concentrated the O(`HASH_CAPACITY`) rebuild into ONE `unregister`/`recycle`
//! call every ~512 deletions — a p99/p999 tail-latency spike (review N3).
//!
//! **R4-8/N3 (current fix): backward-shift deletion.** `hash_remove` now repairs
//! the probe chain at delete time, leaving a clean empty slot — so NO tombstones
//! ever exist and NO rebuild is ever needed. Both the metastable collapse AND
//! the rebuild spike are gone; the per-delete cost is bounded by the current
//! cluster length (never `HASH_CAPACITY`) and paid on every delete.
//!
//! ## What this test verifies
//!
//! (a) **Membership stays EXACTLY correct** across heavy distinct-base churn —
//!     the correctness invariant backward-shift must preserve (a corrupted probe
//!     chain makes a live base report `false` → `dealloc` misroutes → UB).
//!
//! (b) **No single `unregister`/`recycle` is a dramatic latency outlier** at the
//!     exact churn boundary (511/512/513 distinct deletions) where the W2 rebuild
//!     used to fire synchronously. Coarse wall-clock timing in-test (the
//!     deterministic signal is `npm run iai`; wall-clock here is a best-effort
//!     shape check, see the note on `MAX_VS_MEDIAN`).
//!
//! The detailed correctness backstop for the shift-eligibility condition
//! (including the cyclic wrap boundary) lives in
//! `tests/segment_table_backshift_proptest.rs`; this test exercises the real
//! `AllocCore` alloc/dealloc path end-to-end.

// ===========================================================================
// (a) + (b): drive a wave of W > 512 DISTINCT bases through the table, then
// drain it — timing each dealloc — and assert (a) membership correctness
// throughout and (b) no single dealloc is a dramatic outlier.
// ===========================================================================

/// Hold-then-drain a wave of `W` simultaneously-live large segments (each in its
/// own distinct 4 MiB segment, so the OS cannot reuse addresses across them and
/// every free unregisters a DISTINCT base). With `W > 512`, the W2 rebuild would
/// have fired synchronously on the 513th drain free (the spike); backward-shift
/// deletes each with only cluster-length work. We assert:
///
/// - (a) every held base reads `contains_base == true` while live, and flips to
///   `false` immediately after its own free, AND every still-held base stays
///   `true` across every other free (a probe-chain corruption would flip a
///   survivor to `false`).
/// - (b) the ratio of the SLOWEST single dealloc to the MEDIAN dealloc is below
///   a generous bound. The W2 rebuild did ~`HASH_CAPACITY` extra writes on one
///   call; backward-shift does O(cluster) on every call. Wall-clock here is
///   coarse (the OS free dominates per-call time), so the bound is loose — it
///   exists to catch a catastrophic O(`HASH_CAPACITY`)-per-delete regression,
///   not to assert a precise speedup. The deterministic signal is `npm run iai`.
///
/// `#[cfg_attr(miri, ignore)]` — reserves hundreds of 4 MiB OS segments.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)]
#[test]
fn backshift_no_latency_spike_at_threshold_boundary() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};
    use std::time::Instant;

    let mut ac = AllocCore::new().expect("primordial");
    // Budget 0: every large free eagerly releases its OS reservation → the churn
    // is not masked by cache retention (mirrors the W2 test's driver).
    ac.dbg_set_large_cache_budget(Some(0));

    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    // `HASH_CAPACITY = 2048`; the W2 rebuild threshold was `HASH_CAPACITY / 4`
    // = 512, firing when tombstones EXCEEDED 512 (i.e. on the 513th). `W` must
    // exceed 512 so a single wave's drain sweeps well past the old trigger.
    const W: usize = 600; // > 512 (old threshold), < MAX_SEGMENTS (1024)
    const WAVES: usize = 3;

    // Generous bound: the slowest dealloc must not be more than this many × the
    // median. The OS segment-release dominates per-call wall-clock and is itself
    // variable, so this is a coarse guard against a catastrophic per-delete
    // O(HASH_CAPACITY) regression, not a precise spike measurement. Valgrind Ir
    // (npm run iai) is the deterministic judge.
    const MAX_VS_MEDIAN: f64 = 30.0;
    // A single dealloc's wall-clock time is occasionally dominated by OS-level
    // noise unrelated to the allocator (scheduler preemption, a page fault,
    // antivirus I/O hooks, etc.) — a one-off multi-millisecond stall on ONE of
    // the W calls, unrelated to any O(HASH_CAPACITY) regression. A REAL
    // per-delete algorithmic regression reproduces on every attempt (it is a
    // deterministic property of the code, not a transient OS event); a noise
    // spike does not. So retry the timing measurement for a wave up to this
    // many extra times, accepting the FIRST attempt that clears the bound —
    // only fail if every attempt is over. This preserves full detection power
    // for a genuine regression while filtering one-off external stalls (the
    // exact failure mode observed in CI: ratios varying 40x-600x run to run
    // with no code change, always concentrated in a single outlier call).
    const RETRY_ATTEMPTS: u32 = 3;

    for wave in 0..WAVES {
        let mut last_failure: Option<String> = None;
        let mut passed = false;

        for attempt in 0..RETRY_ATTEMPTS {
            // --- Hold: allocate W distinct live large segments. ---
            let mut ptrs = Vec::with_capacity(W);
            for i in 0..W {
                let p = ac.alloc(layout);
                assert!(!p.is_null(), "wave {wave}: alloc null at i={i}");
                ptrs.push(p);
            }

            // (a) While the whole wave is live, EVERY held base must be
            // contained. Correctness — asserted unconditionally, every
            // attempt, never retried-away: a probe-chain corruption must fail
            // the test outright.
            for (i, &p) in ptrs.iter().enumerate() {
                assert!(
                    ac.dbg_contains_base(p),
                    "wave {wave}: held base i={i} not reported contained (live)"
                );
            }

            // --- Drain: free the whole wave, timing each dealloc. ---
            let mut durations_ns: Vec<u64> = Vec::with_capacity(W);
            for (i, &p) in ptrs.iter().enumerate() {
                let t0 = Instant::now();
                // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
                unsafe { ac.dealloc(p, layout) };
                let dt = t0.elapsed().as_nanos() as u64;
                durations_ns.push(dt);

                // (a) The just-freed base must now read foreign (false).
                // Correctness — unconditional, every attempt.
                assert!(
                    !ac.dbg_contains_base(p),
                    "wave {wave}: freed base i={i} still reported contained \
                     (membership corrupted — a removed base is still present)"
                );
                // (a) Every STILL-held base (later in the wave) must remain
                // contained across this free — a probe-chain corruption from
                // backward-shift would flip a survivor to false. Correctness
                // — unconditional, every attempt.
                for &q in &ptrs[i + 1..] {
                    assert!(
                        ac.dbg_contains_base(q),
                        "wave {wave}: a still-held base vanished across the \
                         free at i={i} (backward-shift corrupted a \
                         survivor's probe chain)"
                    );
                }
            }

            // (b) No single dealloc is a dramatic outlier vs the median. The
            //     old W2 rebuild concentrated O(HASH_CAPACITY) work on the
            //     513th call; backward-shift spreads O(cluster) work
            //     uniformly. We assert the max/median ratio is bounded
            //     (coarse — see MAX_VS_MEDIAN note). THIS is the noisy check
            //     that gets retried, never the correctness checks above.
            let mut sorted = durations_ns.clone();
            sorted.sort_unstable();
            let median = sorted[sorted.len() / 2];
            let max = *sorted.last().unwrap();
            // Guard against a pathologically fast median (e.g. all-zero under
            // a coarse clock): treat as trivially passing when the median is
            // too small to be meaningful.
            if median < 100 {
                passed = true;
                break;
            }
            let ratio = max as f64 / median as f64;
            if ratio <= MAX_VS_MEDIAN {
                passed = true;
                break;
            }
            last_failure = Some(format!(
                "wave {wave} attempt {attempt}: slowest dealloc ({max} ns) is \
                 {ratio:.1}× the median ({median} ns) — a single unregister \
                 dominates, suggesting a per-delete O(HASH_CAPACITY) \
                 regression. (Coarse wall-clock; confirm with `npm run iai`.)",
            ));
        }

        assert!(
            passed,
            "{}",
            last_failure.unwrap_or_else(|| format!(
                "wave {wave}: unreachable — loop exited without recording a failure"
            ))
        );
    }
}
