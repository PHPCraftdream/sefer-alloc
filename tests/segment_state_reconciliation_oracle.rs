//! Runtime oracle for the task #1081 (F6 sweep, site 4) `meta_bytes` fix in
//! `AllocCore::dbg_segment_state_reconciliation` — the committed-bytes
//! accounting of the `small_decommitted_retained` state (task #1087, finding
//! M4).
//!
//! # Why this file exists (the false claim it corrects)
//!
//! Commit `b98edb0` (task #1081) and `docs/CORRECTNESS_OPEN_ITEMS.md` item 74
//! both recorded this fix as having "NO runtime oracle", on the grounds that
//! "the `small_decommitted_retained` state has zero production callers today,
//! so no test can drive a segment into it; verified by reading only". That
//! claim was FALSE. The state has zero production callers, true — but
//! `AllocCore::dbg_force_decommit_retain_for` (same
//! `internals` + `alloc-decommit` + `bench-internals` gate as
//! `dbg_segment_state_reconciliation`, present since R12-10/task #261 and
//! `pub unsafe fn` since R29-8/task #439) drives exactly the
//! `release_follows == false` retain leg of `decommit_empty_segment_impl`,
//! and was ALREADY used by `tests/alloc_zeroed_virgin_small_skip.rs` for the
//! sibling virgin-bit regression guard. `decommit_empty_segment_impl` sets
//! `set_decommitted(true)` and leaves the segment registered, so the
//! reconciliation classifies it into `small_decommitted_retained` — the state
//! was never unreachable from a test.
//!
//! # Oracle design
//!
//! Per arm (CLAUDE.md's R26-4 config-evidence + R30-8 path-activation
//! disciplines):
//!
//! 1. Baseline: a fresh heap's reconciliation must classify ZERO segments
//!    into the target state (proves the test is not matching a pre-existing
//!    state) and must see the primordial segment (proves the classification
//!    loop actually iterates the table).
//! 2. Reserve TWO fresh, registered, empty `Small` segments via
//!    `dbg_decomp_reserve_and_keep` (the R30-1-correct measurement route:
//!    `reserve_small_segment_impl` registers + initialises but never
//!    publishes `small_cur`).
//! 3. Precondition evidence: `dbg_live_count_for == Some(0)` and
//!    `dbg_is_decommitted_for == Some(false)` on both segments (the exact
//!    preconditions the retain leg's `# Safety` documents).
//! 4. Force the retain leg on both; assert the hook returned `true` (a
//!    `false` return would make the test vacuously green).
//! 5. ACTIVATION oracle: `dbg_is_decommitted_for == Some(true)` on both —
//!    `set_decommitted(true)` genuinely ran — and, under
//!    `small-segment-lazy-commit`, `dbg_committed_payload_end_for` equals the
//!    initial frontier `small_decommit_start() + LAZY_FIRST_CHUNK` — the lazy
//!    leg's `set_committed_payload_end` genuinely ran with the very quantity
//!    `meta_bytes` mirrors.
//! 6. THE assertion: `small_decommitted_retained.count == 2` AND
//!    `committed_bytes == 2 * expected_meta_bytes` where `expected_meta_bytes`
//!    is the cfg-accurate formula (`small_decommit_start()` alone on the eager
//!    path, `+ LAZY_FIRST_CHUNK` under `small-segment-lazy-commit`).
//!    Counterfactual (task #1087): reverting `meta_bytes` to the tight
//!    `small_meta_end()` fails HERE — under the lazy feature by the missing
//!    `LAZY_FIRST_CHUNK` term on ANY host, and under the eager feature by the
//!    page-round-up delta on a forced 16/64 KiB runtime page.
//!
//! # Arms and feature combinations
//!
//! - `retain_..._lazy_real_page` — `small-segment-lazy-commit` on, the host's
//!   real page. Distinguishes fix from revert on ANY host (the 256 KiB
//!   `LAZY_FIRST_CHUNK` term).
//! - `retain_..._forced_pages` — the `--cfg aligned_vmem_page_size_override`
//!   seam (16 KiB and 64 KiB), driving the REAL retain leg + reconciliation
//!   on REAL reserved segments under a simulated >4 KiB runtime page, in
//!   whichever policy world the feature set selects (eager under plain
//!   `production internals bench-internals`; lazy when
//!   `small-segment-lazy-commit` is added).
//!
//! Deliberately ZERO tests run in the (no-override, no-lazy) combination: on
//! this 4 KiB-page host `small_decommit_start() == small_meta_end()`, so
//! every committed-bytes assertion there passes under BOTH the fix and the
//! revert — a vacuous test. The eager-policy oracle REQUIRES the forced-page
//! seam to be non-vacuous; that is why the forced-page arm exists. For the
//! same reason the shared helpers live in `mod oracle`, gated by the exact
//! union of the two test cfgs — in a combination where no test compiles, the
//! helpers must vanish with them (`-D warnings` dead_code).
//!
//! Mirrors `tests/decomp_hooks_forced_page.rs`'s override-guard and
//! serialization discipline (the override is process-global).

#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

/// Shared oracle machinery, compiled exactly when at least one arm compiles.
#[cfg(any(
    all(
        feature = "small-segment-lazy-commit",
        not(aligned_vmem_page_size_override)
    ),
    all(
        aligned_vmem_page_size_override,
        not(feature = "numa-aware"),
        not(miri)
    )
))]
mod oracle {
    use std::sync::{Mutex, MutexGuard};

    use sefer_alloc::{AllocCore, SegmentLayout};

    /// Serialise every test in this file (the forced-page arms mutate the
    /// process-global page-size override). Poison-tolerant: a failed test
    /// must not cascade `PoisonError` into the others.
    static SERIAL: Mutex<()> = Mutex::new(());

    pub(super) fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The task #1081 `meta_bytes` formula, mirrored from the FIX side
    /// (`dbg_segment_state_reconciliation`'s decommitted arm): the committed
    /// prefix is `small_decommit_start()` on the eager path, plus the
    /// retained initial chunk (`LAZY_FIRST_CHUNK`, 256 KiB) under
    /// `small-segment-lazy-commit` — matching `decommit_empty_segment_impl`'s
    /// decommit boundary in both policy worlds.
    #[cfg(feature = "small-segment-lazy-commit")]
    fn lazy_extra(ac: &AllocCore) -> usize {
        ac.dbg_lazy_first_chunk()
    }
    #[cfg(not(feature = "small-segment-lazy-commit"))]
    fn lazy_extra(_ac: &AllocCore) -> usize {
        0
    }

    /// Restores the real OS page size even if the test panics mid-observation
    /// (`None` re-arms the query-on-next-call sentinel in the page-size
    /// cache).
    #[cfg(aligned_vmem_page_size_override)]
    pub(super) struct RestorePageSize;
    #[cfg(aligned_vmem_page_size_override)]
    impl Drop for RestorePageSize {
        fn drop(&mut self) {
            aligned_vmem::page_size_override::set_page_size_override(None);
        }
    }

    /// The oracle body (runs under whatever runtime page is currently active
    /// — the real host page, or the forced override set by the caller).
    pub(super) fn oracle_body() {
        let mut ac = AllocCore::new().expect("AllocCore::new must survive the active page size");

        // ── 1. Baseline: the target state starts EMPTY on this fresh heap. ──
        let before = ac.dbg_segment_state_reconciliation();
        assert_eq!(
            before.small_decommitted_retained.count, 0,
            "baseline: a fresh heap must not yet classify anything into \
             small_decommitted_retained"
        );
        assert_eq!(
            before.unknown_count, 0,
            "baseline: no corrupt segment headers on a fresh heap"
        );
        assert!(
            before.primordial.count >= 1,
            "activation precondition: the reconciliation must see the heap's own \
             primordial segment (a zero count would mean the classification loop \
             never ran at all)"
        );

        // ── 2. Reserve two fresh, registered, EMPTY Small segments. ─────────
        let h1 = ac
            .dbg_decomp_reserve_and_keep()
            .expect("first small-segment reservation must succeed");
        let h2 = ac
            .dbg_decomp_reserve_and_keep()
            .expect("second small-segment reservation must succeed");
        let b1 = h1.dbg_base();
        let b2 = h2.dbg_base();
        assert_ne!(b1, b2, "two reservations must yield two distinct segments");

        // ── 3. Precondition evidence: both segments are genuinely EMPTY and
        //       not yet decommitted — exactly what the retain leg's `# Safety`
        //       documents but does not itself verify.
        assert_eq!(
            ac.dbg_live_count_for(b1),
            Some(0),
            "precondition: a freshly reserved segment must read live_count == 0"
        );
        assert_eq!(
            ac.dbg_live_count_for(b2),
            Some(0),
            "precondition: a freshly reserved segment must read live_count == 0"
        );
        assert_eq!(
            ac.dbg_is_decommitted_for(b1),
            Some(false),
            "precondition: a freshly reserved segment must not be decommitted yet"
        );
        assert_eq!(
            ac.dbg_is_decommitted_for(b2),
            Some(false),
            "precondition: a freshly reserved segment must not be decommitted yet"
        );

        // ── 4. Force the `release_follows == false` retain leg on both
        //       segments.
        //
        // SAFETY: h1/h2 were reserved immediately above and NOTHING has been
        // carved from either segment — `live_count == 0` genuinely holds
        // (asserted immediately above), which is the exact precondition
        // `dbg_force_decommit_retain_for`'s `# Safety` section requires but
        // does not itself verify.
        assert!(
            unsafe { ac.dbg_force_decommit_retain_for(b1) },
            "the force hook must find and retain b1's Small segment (a false \
             return is the no-op path — the test would be vacuously green)"
        );
        assert!(
            unsafe { ac.dbg_force_decommit_retain_for(b2) },
            "the force hook must find and retain b2's Small segment (a false \
             return is the no-op path — the test would be vacuously green)"
        );

        // ── 5. ACTIVATION oracle: the retain leg genuinely executed on BOTH
        //       segments (the decommit flag flipped), not just a number
        //       matched.
        assert_eq!(
            ac.dbg_is_decommitted_for(b1),
            Some(true),
            "activation: set_decommitted(true) must have run on b1 (the retain \
             leg's own flag)"
        );
        assert_eq!(
            ac.dbg_is_decommitted_for(b2),
            Some(true),
            "activation: set_decommitted(true) must have run on b2 (the retain \
             leg's own flag)"
        );
        #[cfg(feature = "small-segment-lazy-commit")]
        {
            let frontier = SegmentLayout::small_decommit_start() + ac.dbg_lazy_first_chunk();
            assert_eq!(
                ac.dbg_committed_payload_end_for(b1),
                Some(frontier),
                "activation: the lazy retain leg must have reset the committed \
                 frontier to the initial chunk end ({frontier}) — the exact \
                 quantity meta_bytes mirrors"
            );
            assert_eq!(
                ac.dbg_committed_payload_end_for(b2),
                Some(frontier),
                "activation: the lazy retain leg must have reset the committed \
                 frontier to the initial chunk end ({frontier})"
            );
        }

        // ── 6. Snapshot the reconciliation, then RELEASE both handles BEFORE
        //       any load-bearing assertion: the handle's `Drop` leak-detector
        //       fires a SECOND panic during a failed assertion's unwind
        //       (panic-while-panicking → STATUS_STACK_BUFFER_OVERRUN abort
        //       that swallows the assertion message — the same hazard
        //       R31-15's "Ordering note" documents in `dbg_decomp_release`).
        //       Consuming the handles first keeps a failing run's output
        //       readable.
        let rec = ac.dbg_segment_state_reconciliation();
        let expected_each = SegmentLayout::small_decommit_start() + lazy_extra(&ac);

        // SAFETY: both handles were minted by THIS `ac`'s
        // `dbg_decomp_reserve_and_keep` above and have not been released in
        // the interim — the exact pairing `# Safety` requires.
        unsafe { ac.dbg_decomp_release(h1) };
        unsafe { ac.dbg_decomp_release(h2) };

        // ── THE assertion: both segments classified into the target state,
        //       with the cfg-accurate committed-bytes figure.
        assert!(
            expected_each >= SegmentLayout::SMALL_META_END,
            "sanity: the committed prefix ({expected_each}) cannot be smaller than \
             the metadata region ({})",
            SegmentLayout::SMALL_META_END
        );
        assert!(
            expected_each <= SegmentLayout::SEGMENT,
            "sanity: the committed prefix ({expected_each}) cannot exceed the \
             segment ({})",
            SegmentLayout::SEGMENT
        );
        assert_eq!(
            rec.small_decommitted_retained.count, 2,
            "both forced-retain segments must be classified into \
             small_decommitted_retained (fewer means the arm did not fire for both)"
        );
        assert_eq!(
            rec.small_decommitted_retained.committed_bytes,
            (2 * expected_each) as u64,
            "task #1081 meta_bytes oracle: committed bytes must equal \
             2 x small_decommit_start(){} == {} — reverting meta_bytes to the \
             tight small_meta_end() ({} per segment, {}/2) fails HERE",
            if cfg!(feature = "small-segment-lazy-commit") {
                " + LAZY_FIRST_CHUNK"
            } else {
                ""
            },
            2 * expected_each,
            SegmentLayout::SMALL_META_END,
            2 * SegmentLayout::SMALL_META_END
        );
        assert_eq!(
            rec.small_decommitted_retained.reserved_bytes,
            (2 * SegmentLayout::SEGMENT) as u64,
            "each retained segment still holds its full 4 MiB reservation"
        );
        assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
        // Reconciliation identity: every non-NULL slot classified exactly once.
        assert_eq!(
            rec.total.count,
            rec.primordial.count
                + rec.small_pooled.count
                + rec.small_active.count
                + rec.small_empty_orphan.count
                + rec.small_decommitted_retained.count
                + rec.large_active.count
                + rec.large_cached.count
                + rec.unknown_count,
            "identity: total.count must equal the sum of per-state counts plus \
             unknown_count"
        );

        println!(
            "oracle: page={} lazy={} meta_bytes_each={} (tight small_meta_end={}) \
             count={} committed={} reserved={}",
            aligned_vmem::page_size(),
            cfg!(feature = "small-segment-lazy-commit"),
            expected_each,
            SegmentLayout::SMALL_META_END,
            rec.small_decommitted_retained.count,
            rec.small_decommitted_retained.committed_bytes,
            rec.small_decommitted_retained.reserved_bytes,
        );
    }
}

/// Arm 1 — the lazy policy world on the host's REAL page. Non-vacuous on any
/// host: the `LAZY_FIRST_CHUNK` (256 KiB) term is present in the fix and
/// absent from the revert.
#[cfg(all(
    feature = "small-segment-lazy-commit",
    not(aligned_vmem_page_size_override)
))]
#[test]
fn retain_decommitted_retained_meta_bytes_oracle_lazy_real_page() {
    let _serial = oracle::serial();
    oracle::oracle_body();
}

/// Arms 2+3 — forced 16 KiB and 64 KiB runtime pages via the
/// `--cfg aligned_vmem_page_size_override` seam, in whichever policy world
/// the active feature set selects (eager under plain
/// `production internals bench-internals`; lazy with
/// `small-segment-lazy-commit` added). ONE test on purpose: the override is
/// process-global.
#[cfg(all(
    aligned_vmem_page_size_override,
    not(feature = "numa-aware"),
    not(miri)
))]
#[test]
fn retain_decommitted_retained_meta_bytes_oracle_forced_pages() {
    let _serial = oracle::serial();

    // Task #1085 interlock (added at merge, task #1092): the override seam now
    // REJECTS any page below the host's REAL page size — forcing a smaller page
    // than the hardware's is the data-destruction hazard that fix closed. On a
    // 64 KiB-page host (aarch64-64k Linux) that rejects this loop's 16 KiB arm,
    // so an unusable arm is SKIPPED here rather than asserted-accepted; the
    // `executed` counter below keeps the skip honest — if EVERY arm were below
    // the real page, the test would otherwise pass having verified nothing.
    // Same shape as tests/decomp_hooks_forced_page.rs (task #1086).
    let real_page = aligned_vmem::page_size();
    let mut executed = 0usize;

    // Both sizes where the tight const boundary and the runtime-safe boundary
    // diverge for every known layout.
    for &forced in &[16 * 1024, 64 * 1024] {
        if forced < real_page {
            eprintln!(
                "skipping the {forced}-byte forced-page arm: the host's real page \
                 is {real_page} bytes, and the override seam correctly rejects \
                 forcing a smaller page (task #1085)"
            );
            continue;
        }

        // Guard FIRST: everything after this line runs under the override,
        // and a panic unwinds through this Drop before any sibling
        // observation. Restores the real page at the end of EACH iteration.
        let _restore = oracle::RestorePageSize;

        assert!(
            aligned_vmem::page_size_override::set_page_size_override(Some(forced)),
            "{forced} is a power of two, >= PAGE, and >= the host's real page \
             ({real_page}); the override seam must accept it"
        );
        // R26-4 evidence discipline: assert the config took effect before
        // trusting any observation made under it.
        assert_eq!(
            aligned_vmem::page_size(),
            forced,
            "config evidence: the page-size override must be active before the \
             retain/reconciliation observations below mean anything"
        );

        oracle::oracle_body();
        executed += 1;
    }

    assert!(
        executed >= 1,
        "no forced-page arm was executable on this host (real page {real_page} \
         bytes): every candidate forced page was below the real page and was \
         skipped — this host cannot run the forced-page reconciliation oracle, \
         and passing vacuously would be worse than failing loudly (task #1092)"
    );
}
