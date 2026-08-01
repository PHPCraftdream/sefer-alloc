// R31-3 (task #466, step 3) shared workload for the
// `r31_3_large_cache_extended_narrow_{off,on}` process-level A/B judge
// binaries — the N=1/2/4 TIMING regression check
// `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` item 4 named as a
// deferred follow-up (`docs/perf/OPEN_ITEMS.md` item 7, `[L]` tier): item 4's
// own `tests/large_cache_extended_narrow_working_set_after_materialization.rs`
// proved N=1/2/4 CORRECTNESS after sidecar materialisation, but explicitly
// left the wall-clock cost of the widened O(40) scan bound on that same
// narrow-working-set shape unmeasured ("a dedicated timing gate for this
// specific narrow-working-set-after-burst shape is deferred as a follow-up").
//
// **CORRECTED 2026-08-01 (task #487, a Round-31 review response):** this file
// originally (a) hardcoded `2 * 1024 * 1024usize` as a stand-in for
// `SegmentLayout::SEGMENT` with a comment claiming the two were equal and
// that a real import was being avoided — both false: `SegmentLayout::SEGMENT`
// is `4 MiB` (`src/alloc_core/os.rs`'s `SEGMENT` constant), not 2 MiB, and it
// was always `pub` and importable (`src/alloc_core/segment_layout.rs`); and
// (b) had NO in-run materialisation oracle — the only asserts in this file
// were an alloc-succeeded null check and a length check on `materialize_sizes`
// own output, with the actual "did the sidecar materialise" proof deferred to
// a DIFFERENT test file running a DIFFERENT configuration entirely (a
// CLAUDE.md R30-8 path-activation-oracle violation: each arm must prove IT
// took the mechanism it claims to measure, in its own timed run). Both are
// fixed below: `SegmentLayout::SEGMENT` is now imported and used for real,
// every downstream size shifts accordingly (see `materialize_sizes`'s own
// updated doc for the new geometric-doubling sums this produces — verified
// against the existing proven-correct `force_materialisation` helper in
// `tests/large_cache_extended_narrow_working_set_after_materialization.rs`,
// which already performs an equivalent 9-size sweep with the REAL constant in
// CI today), and `run_narrow_ab_workload` now hard-asserts, INSIDE the same
// child process the timed region runs in and BEFORE the timed region is
// trusted: the resolved large-cache budget, whether the extension
// materialised (ON arm only — the OFF arm never has a sidecar to
// materialise, matching `tests/…_after_materialization.rs`'s own `#[cfg]`
// gating), and the resolved total-slot count (`8` OFF, `40` ON) — mirroring
// the `MIN_ACTIVATION_PCT`-style hard-assert pattern in
// `benches/r30_3_virgin_zero_skip_native_gate.rs` and the
// `admissions_ok`/`hits_ok`-style hard-asserts in
// `examples/r30_6_large_cache_headroom_ab_gate.rs`, per CLAUDE.md's R30-8
// rule. Getting this oracle wired up surfaced a THIRD, more serious defect
// the original file's missing oracle had hidden: `SeferAlloc::new()`'s
// default resolved large-cache budget under `large-cache-extended` is 256
// MiB (`DEFAULT_EXTENDED_BUDGET_BYTES`, R17-9), but
// `large_cache_deposit_budget_infeasible` rejects a deposit whose OWN
// `usable_size` exceeds the budget — a per-deposit check, not a running-total
// check (`src/alloc_core/alloc_core_large_cache.rs:164-166`). With the
// corrected (real) 4 MiB `SEGMENT` constant, `materialize_sizes(9)`'s
// geometric-doubling ladder produces sizes up to ~2 GiB by the 9th entry —
// individually far above 256 MiB — so under the DEFAULT config the base
// cache would only ever admit the first 6 (≤ 256 MiB) of the 9 sizes,
// NEVER overflowing the base 8 slots, and the ON arm's sidecar would NEVER
// materialise at all. The ON arm below therefore explicitly overrides the
// budget to `usize::MAX` (documented, established "recover unbounded"
// idiom — `LargeCacheConfig`'s own `budget_bytes` doc,
// `src/alloc_core/large_cache_config.rs:130-136` — the same technique
// `examples/r14_5_large_cache_extended_rss_measure.rs`'s "unbounded" mode
// already uses one layer down) so the materialisation burst can actually
// succeed; the new in-run oracle below is what makes this reasoning
// verifiable at run time instead of assumed. This file's own git history
// (`docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md`'s §3
// narrow-timing numbers, measured against the BROKEN pre-fix workload) is
// superseded by a re-measurement filed as a separate follow-up task (#488) —
// this fix does not itself re-derive or re-publish a new verdict.
//
// ## Why this exists (the concern this gate answers)
//
// `large-cache-extended` is sized for WIDE working sets (up to 40 distinct
// Large sizes — see `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md`
// §6's turnover-profile win, refreshed in
// `docs/perf/_raw_r31_3_paired_ab_turnover_refresh.log`). Most real programs'
// Large-object working sets are much narrower (1-4 distinct sizes) most of
// the time. The open concern (task #466's brief, mirroring the R14-5 report's
// own §4 closing paragraph): once the sidecar has materialised (e.g. from an
// earlier burst of size diversity), does every SUBSEQUENT narrow-working-set
// alloc/free pay a hidden fixed cost from the wider O(40) scan bound
// (`large_cache_scan_bound`), relative to the base 8-slot cache that never
// widens?
//
// ## Workload shape
//
// Two phases per process, both timed separately so the "does materialisation
// itself cost something" question and the "does the SUBSEQUENT narrow phase
// cost something" question are not conflated:
//
// 1. **Materialisation burst (untimed in the reported RESULT, but its own
//    Instant is captured for context):** batch-alloc-all/dealloc-all across
//    `MATERIALIZE_N = 9` distinct sizes — the same proven pattern
//    `tests/large_cache_extended_narrow_working_set_after_materialization.rs`
//    uses to force the sidecar to materialise (overflows the base 8 by
//    exactly 1). In the OFF arm (extension compiled out) this phase simply
//    exercises the base 8-slot cache's own FIFO eviction — there is no
//    sidecar to materialise, so this phase is present in BOTH arms for
//    workload-shape symmetry (the OFF arm's timing baseline should reflect
//    "a real program that once had a size-diverse burst", not an
//    artificially cache-virgin state).
// 2. **Narrow phase (THIS is the timed, reported region):** the working set
//    narrows to the first `N` of the 9 materialisation sizes (still genuinely
//    distinct, still each an individually-resident cache entry from the
//    burst). `WARMUP_ROUNDS` untimed cycles, then `ROUNDS` timed
//    batch-alloc-all/dealloc-all cycles over just the `N` sizes — mirrors
//    `run_turnover_workload`'s single-`Instant`-pair-around-many-rounds
//    structure (`examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`).
//
// ## Path-activation oracle (CLAUDE.md's R30-8 rule)
//
// Both arms report `large_cache_hits` accumulated during the TIMED narrow
// phase (via `SeferAlloc::stats()`, the real `#[global_allocator]` counter —
// same layer-identification pattern as the turnover workload) — every
// narrow-phase alloc after warm-up MUST hit the cache in both arms (each of
// the N sizes was deposited into its own slot during warm-up and never
// evicted, since N <= 4 is far below either 8 or 40 slots), so a 100% hit
// rate in BOTH arms is the mechanism-activation proof this comparison
// measures like-for-like servicing (a cache hit vs a cache hit), not
// "sidecar scan overhead" contaminated by one arm actually missing and
// paying a real OS round-trip. The materialisation burst (Phase 1) forces
// the ON arm's sidecar to the widened 40-slot scan bound using the same
// proven pattern
// `tests/large_cache_extended_narrow_working_set_after_materialization.rs::scan_bound_stays_forty_during_narrow_working_set_phase`
// already established stays at 40 throughout a subsequent narrow phase
// (materialisation is one-way, never reverts, per
// `large_cache_extended.rs`'s documented design) — this timing gate reuses
// that existing correctness fact rather than re-deriving it with a second
// hook; the per-arm 100% `large_cache_hits` activation proof above is this
// gate's OWN mechanism-activation evidence for the TIMED region
// specifically.

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::{AllocCore, SegmentLayout};

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

/// 9 distinct, pairwise->2x-apart Large sizes — identical derivation to
/// `tests/large_cache_extended_narrow_working_set_after_materialization.rs::large_test_sizes`
/// (which itself uses the real `SegmentLayout::SEGMENT`, not a duplicated
/// magic number — see this file's 2026-08-01 correction note in the module
/// doc above). Duplicated here (not shared via a crate: no `examples/`-support
/// crate exists in this project for cross-file helpers) so materialisation is
/// forced the same proven way.
///
/// With the real 4 MiB `SEGMENT` constant, the geometric-doubling ladder
/// grows fast: `materialize_sizes(9)` produces (approximately) 4, 12, 28, 60,
/// 124, 252, 508, 1020, 2044 MiB — individually verified against
/// `tests/large_cache_extended_narrow_working_set_after_materialization.rs`'s
/// own `force_materialisation` helper, which already performs an equivalent
/// 9-size sweep with this exact formula and the real constant in CI today (so
/// this is the PROVEN-correct shape, not a new risk introduced by fixing the
/// constant). The last three sizes individually exceed the default 256 MiB
/// `large-cache-extended` budget — see `run_narrow_ab_workload`'s caller
/// contract below for why the ON arm must override the budget for
/// materialisation to actually succeed.
fn materialize_sizes(n: usize) -> Vec<usize> {
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let segment = SegmentLayout::SEGMENT;
    let mut size = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(n);
    for _ in 0..n {
        sizes.push(size);
        size = (2 * size + 1).div_ceil(segment) * segment;
    }
    sizes
}

const MATERIALIZE_N: usize = 9;
const WARMUP_ROUNDS: usize = 3;
const ROUNDS: usize = 400;
const ALIGN: usize = 8;
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

fn alloc_one(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `layout` has non-zero size and valid (power-of-two, <=
    // usize::MAX/2) alignment (8), satisfying `GlobalAlloc::alloc`'s
    // preconditions.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed -- probe is invalid");
    // SAFETY: caller guarantees `p` points to at least 16 writable bytes
    // (every size in this workload is well above small_max, comfortably >
    // 16 bytes).
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
        std::ptr::write_volatile(p.cast::<u64>().add(1), TOUCH);
    }
    p
}

fn dealloc_one(p: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with this exact `layout` by `alloc_one`, and
    // is freed exactly once.
    unsafe { std::alloc::dealloc(p, layout) };
}

fn batch_cycle(sizes: &[usize]) {
    let mut ptrs = Vec::with_capacity(sizes.len());
    for &sz in sizes {
        let p = alloc_one(sz);
        black_box(p);
        ptrs.push((p, sz));
    }
    for &(p, sz) in &ptrs {
        dealloc_one(p, sz);
    }
}

/// **Caller contract (why the ON binary must NOT use a plain
/// `SeferAlloc::new()`):** `materialize_sizes(MATERIALIZE_N)`'s last three
/// entries individually exceed the default 256 MiB `large-cache-extended`
/// budget, and `large_cache_deposit_budget_infeasible`
/// (`src/alloc_core/alloc_core_large_cache.rs:164-166`) rejects a deposit
/// whose OWN `usable_size` exceeds the budget — a per-deposit check, so those
/// three sizes would never even reach the free-slot scan that could trigger
/// materialisation. Under the plain default config the base cache would only
/// ever admit the first 6 (≤ 256 MiB) of the 9 materialisation sizes, NEVER
/// overflowing the base 8 slots — the ON arm's sidecar would never
/// materialise, silently invalidating the whole "widened O(40) scan bound"
/// premise this gate exists to test. The ON binary's `#[global_allocator]`
/// MUST therefore be built with
/// `SeferAlloc::with_config(LargeCacheConfig::new().budget_bytes(usize::MAX))`
/// (the documented "recover unbounded" idiom —
/// `src/alloc_core/large_cache_config.rs:130-136`), not `SeferAlloc::new()`.
/// The in-run oracle below hard-asserts this actually happened (resolved
/// budget is `Some(usize::MAX)` -- under `large-cache-extended`,
/// `resolved_budget_bytes()` always returns `Some(...)`, never `None`, so
/// `usize::MAX` is itself the "unbounded" signal, not the `Option` variant)
/// rather than assuming the caller wired it up correctly.
///
/// Returns `(narrow_elapsed_ns, narrow_hits, narrow_total_deallocs,
/// rss_after_kib, commit_after_kib)`. `n` is the narrow working-set size
/// (1, 2, or 4). `global` is the process's installed `SeferAlloc`
/// `#[global_allocator]` static.
pub fn run_narrow_ab_workload(
    global: &'static sefer_alloc::SeferAlloc,
    n: usize,
) -> (u128, u64, u64, u64, u64) {
    let nine_sizes = materialize_sizes(MATERIALIZE_N);
    assert_eq!(nine_sizes.len(), MATERIALIZE_N);

    // Phase 1: materialisation burst (untimed in the reported metric) — one
    // pass is enough to force the sidecar (ON arm) / populate all 8+1
    // distinct-size FIFO churn (OFF arm, base cache only ever holds 8 of the
    // 9 at once by construction, exactly like the turnover workload's
    // baseline).
    batch_cycle(&nine_sizes);

    // ── In-run materialisation oracle (CLAUDE.md's R30-8 path-activation
    // rule) ─────────────────────────────────────────────────────────────
    //
    // Proven INSIDE this same child process, right after the burst that is
    // supposed to trigger it and BEFORE the timed region below is trusted —
    // not deferred to a different test file running a different
    // configuration. Mirrors the `MIN_ACTIVATION_PCT`-style hard-assert
    // pattern in `benches/r30_3_virgin_zero_skip_native_gate.rs` and the
    // `admissions_ok`/`hits_ok`-style hard-asserts in
    // `examples/r30_6_large_cache_headroom_ab_gate.rs`.
    #[cfg(all(feature = "large-cache-extended", feature = "bench-internals"))]
    {
        // NOTE: under `large-cache-extended`, `resolved_budget_bytes()`
        // ALWAYS returns `Some(...)` -- an unset `budget_bytes` still
        // resolves to `Some(DEFAULT_EXTENDED_BUDGET_BYTES)`
        // (`src/alloc_core/large_cache_config.rs:402-409`); there is no
        // resolved `None` state under this feature. The "recover unbounded"
        // idiom is therefore an explicit `Some(usize::MAX)`, not `None` --
        // confirmed empirically the first time this assertion ran (it
        // originally expected `None` and the oracle itself caught that
        // mistake before this file was trusted).
        let resolved_budget = global.dbg_current_large_cache_budget();
        assert_eq!(
            resolved_budget,
            Some(usize::MAX),
            "R31-3 narrow ON arm: resolved large-cache budget must be \
             Some(usize::MAX) (the explicit \"recover unbounded\" idiom) -- \
             the ON binary must install SeferAlloc::with_config(\
             LargeCacheConfig::new().budget_bytes(usize::MAX)), not \
             SeferAlloc::new(), or the materialisation burst's largest sizes \
             are budget-rejected before they ever reach the free-slot scan \
             (got resolved budget {resolved_budget:?})"
        );
        let materialised = global.dbg_current_large_cache_extension_materialised();
        assert!(
            materialised,
            "R31-3 narrow ON arm: large-cache extension sidecar did not \
             materialise after the {MATERIALIZE_N}-distinct-size burst -- the \
             widened-scan-bound premise this gate measures never actually \
             activated"
        );
        let total_slots = global.dbg_current_large_cache_total_slots();
        assert_eq!(
            total_slots, 40,
            "R31-3 narrow ON arm: resolved total large-cache slot count must \
             be 40 (8 base + 32 extension) once materialised, got {total_slots}"
        );
    }
    #[cfg(all(feature = "bench-internals", not(feature = "large-cache-extended")))]
    {
        // OFF arm: no sidecar exists at all -- confirm the resolved total
        // slot count stays at the base 8 (the OFF-arm mirror of the ON arm's
        // `total_slots == 40` assertion above), so a build misconfiguration
        // (e.g. accidentally compiling the OFF binary WITH the extension
        // feature) cannot silently pass as a false negative.
        let total_slots = global.dbg_current_large_cache_total_slots();
        assert_eq!(
            total_slots, 8,
            "R31-3 narrow OFF arm: resolved total large-cache slot count must \
             stay at the base 8 (no extension compiled in), got {total_slots}"
        );
    }

    // Phase 2: narrow to the first `n` of the 9 materialisation sizes — still
    // genuinely distinct, still each an individually cache-resident entry
    // from the burst above (dealloc's best-fit is not consulted; each entry
    // occupies its own slot per the established `large_cache_extended_*`
    // proven pattern).
    let working_set: Vec<usize> = nine_sizes[..n].to_vec();

    // Untimed warm-up: re-populate exactly the N working-set entries so the
    // timed region starts in steady state (every entry already resident,
    // no first-touch admission cost counted against the timed region).
    for _ in 0..WARMUP_ROUNDS {
        batch_cycle(&working_set);
    }

    let hits_before = global.stats().large_cache_hits;
    let t = Instant::now();
    let mut total_deallocs: u64 = 0;
    for _ in 0..ROUNDS {
        batch_cycle(&working_set);
        total_deallocs += n as u64;
    }
    let narrow_elapsed_ns = t.elapsed().as_nanos();
    let narrow_hits = global.stats().large_cache_hits - hits_before;

    // Post-timed-region re-check: materialisation (ON) / non-materialisation
    // (OFF) must still hold after the narrow-phase churn -- the sidecar is
    // documented one-way/never-dematerialises
    // (`tests/large_cache_extended_narrow_working_set_after_materialization.rs::scan_bound_stays_forty_during_narrow_working_set_phase`),
    // so this is a cheap confirmation the timed region did not somehow
    // regress the state the oracle above already proved.
    #[cfg(all(feature = "large-cache-extended", feature = "bench-internals"))]
    assert_eq!(
        global.dbg_current_large_cache_total_slots(),
        40,
        "R31-3 narrow ON arm: total large-cache slot count drifted away from \
         40 during the timed narrow phase"
    );
    #[cfg(all(feature = "bench-internals", not(feature = "large-cache-extended")))]
    assert_eq!(
        global.dbg_current_large_cache_total_slots(),
        8,
        "R31-3 narrow OFF arm: total large-cache slot count drifted away from \
         8 during the timed narrow phase"
    );

    (
        narrow_elapsed_ns,
        narrow_hits,
        total_deallocs,
        rss_kib(),
        commit_kib(),
    )
}
