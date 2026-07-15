//! `segment_directory_sweep` — deterministic multi-segment directory-scan
//! JUDGE (task R6-OPT-A4, `radical_optimization_review` §4 P1-6 measurement
//! plan / §5.5 item 7 / §6 Stage A.5).
//!
//! ## Why this harness exists — the O(S) curve no existing judge can show
//!
//! `AllocCore::find_segment_with_free_impl`
//! (`src/alloc_core/alloc_core_small.rs`) is, by its own doc comment, an
//! O(segments) scan: on a free-list miss it walks every registered segment
//! `[0, count)` looking for one whose `BinTable[class_idx]` is non-empty.
//! Three prior tasks (X5, T10, R5-R1) tried to optimise this scan and each
//! measured ~zero improvement — not because the scan is secretly O(1), but
//! because the only existing judge that exercises it,
//! `benches/perf_gate_iai.rs`'s `multiseg_cold_256k`, deliberately builds
//! only **3** registered segments (documented in its own comment: primordial
//! + 2 fresh, `MULTISEG_BATCH = 34` spanning `15 + 15 + 4`).
//!
//! Three points is deep in the flat region of any O(S) curve for small S —
//! you cannot see a linear cost you never scaled far enough to observe. This
//! harness is Stage A precisely so a future task (R6-OPT-P1-6, a full
//! per-class segment directory to replace the linear scan) can be honestly
//! evaluated against a judge that actually crosses into the region where
//! O(S) bites.
//!
//! This is a MEASUREMENT harness, not a source change. `find_segment_with_free_impl`
//! and its callers are read-only inputs here; nothing in `src/` is touched.
//!
//! ## Controlled construction of exactly S "scannable" segments — verifiably,
//! not assumed
//!
//! `find_segment_with_free` is `pub(crate)` (no test seam exposes it
//! directly — confirmed by grep; this task's brief explicitly forbids
//! touching allocation-routing source code, so no new seam is added here
//! either). The harness instead drives the SAME public entry point every
//! caller uses — `AllocCore::alloc(Layout)` — and constructs the segment
//! population by allocating blocks of one size class, recording each
//! returned pointer's segment base via `SegmentLayout::segment_base_of` (the
//! same technique `tests/regression_t10_find_segment_multiseg_recovery.rs`
//! uses). Blocks are grouped into per-segment buckets in FIRST-SEEN order —
//! exactly the scan order `find_segment_with_free_impl` walks
//! (`SegmentTable::base_at(i)` for `i` in `[0, count)`, i.e. registration
//! order).
//!
//! **`construct_s_segments` carves `S + 1` registered segments, not `S`** —
//! see that function's doc comment for the full story of the two real
//! construction bugs this design avoids (found and fixed during this
//! harness's own development, per its module-doc verification discipline):
//! (1) `alloc_small`'s step 1 (`pop_free(small_cur, ..)`) always runs BEFORE
//! step 2 (the scan), and `small_cur` is always the LAST-carved segment — so
//! the scan target's free block cannot live in the last-carved segment or
//! step 1 would serve it directly, making the scan structurally unreachable;
//! the fix carves ONE EXTRA, always-kept-100%-full "current" segment beyond
//! the `S` scannable ones, so `small_cur` never has a free block and the scan
//! is unconditionally reached. (2) the carve path
//! (`carve_block_with_refill`) also carves a refill batch of up to 31 extra
//! blocks per fresh segment; every segment (scannable and the extra current
//! one) is drained to its EXACT known capacity (via `measure_segment_capacity`,
//! which empirically measures the real carve/refill pattern on a throwaway
//! `AllocCore` rather than assuming layout-offset arithmetic) so no segment
//! is left with a hidden residual. `buckets`/`bases` returned to the caller
//! cover only the `S` scannable segments (indices `[0, S)`); the extra
//! current segment is tracked internally and never touched again.
//! `AllocCore::dbg_table_count() == S + 1` is independently VERIFIED after
//! construction — never assumed. `dbg_table_count` is a pre-existing
//! `#[doc(hidden)]` test seam (task #135, used by `tests/segment_table_o1.rs`);
//! no new seam was added for this harness.
//!
//! `dbg_table_count` starts at 1 (the primordial segment is itself segment
//! #1) — so `S = 1` means one scannable segment (the primordial) plus one
//! extra fresh current segment (`dbg_table_count() == 2` total), matching the
//! existing IAI judge's implicit floor and `multiseg_cold_256k`'s own
//! "primordial + 2 fresh = 3" geometry for its `S = 3` case (3 scannable
//! segments there, no extra current segment needed in that bench's own
//! two-round alloc/free-all/realloc-all shape, which is architecturally
//! different from this harness's single-scan-per-timed-call design).
//!
//! ## Holes: how many segments the scan must walk before success
//!
//! For a given `(S, holes_pct)` cell, after construction every SCANNABLE
//! segment `0..S-1` ("non-target") has `holes_pct%` of ITS OWN blocks freed
//! back to its `BinTable`, and segment `S-1` ("target", the one the scan is
//! meant to find) is left with **exactly one** free block — enough to end
//! the scan there and nowhere more at `holes_pct = 0`. Two cases:
//!
//! - **`holes_pct = 0`**: every non-target segment stays completely full
//!   (zero free blocks) — the worst case named in the task brief: the scan
//!   is FORCED to walk all `S - 1` non-target segments (each paying the
//!   per-segment scan cost: `kind_at` read, ring-drain guard, `BinTable`
//!   head read) before reaching the target at index `S - 1` and succeeding.
//!   This is the cell with the clearest O(S) signal and the one most
//!   directly comparable to `multiseg_cold_256k` at `S = 3`.
//! - **`holes_pct > 0`**: every non-target segment ALSO gets `holes_pct%` of
//!   its own blocks freed. Since `find_segment_with_free_impl` returns the
//!   FIRST segment (in scan order) whose `BinTable[class_idx]` is non-empty,
//!   a non-target segment with a hole now satisfies the scan itself —
//!   `holes_pct > 0` therefore does NOT preserve "must walk all S-1", it
//!   models the task's own "higher hole percentages simulate a more
//!   favorable free-block distribution" case: the scan is expected to
//!   terminate EARLY (near the front of the scan order) with high
//!   probability once any earlier segment has a hole. This is measured and
//!   reported honestly as a *contrast* point against the 0%-holes worst
//!   case, not forced to walk the full S-1 (forcing that shape at holes>0
//!   would misrepresent what "holes" means per the task brief).
//!
//! Only the SUBSTRATE `AllocCore` path is used (no magazine/fastbin layer):
//! `alloc_small`'s free-list-miss scan is the path under test; a magazine
//! layer would intercept most alloc/dealloc traffic before it ever reaches
//! `find_segment_with_free`, which is exactly why the sibling
//! `regression_t10_find_segment_multiseg_recovery.rs` test also drives
//! `AllocCore` directly rather than `SeferAlloc`/`HeapCore`.
//!
//! ## Feature gating
//!
//! The S/holes/class matrix needs only `alloc-core` (a bare `AllocCore`, no
//! magazine, no cross-thread ring). The separate remote-dirty-density matrix
//! additionally needs `alloc-xthread` (a real per-segment `RemoteFreeRing`)
//! and drives it via the SAME production path
//! `tests/remote_fanin.rs`/`benches/heap_fanin_persistent.rs` use — genuine
//! `std::thread::spawn` producer threads doing real cross-thread
//! `HeapCore::dealloc` calls, never the `#[doc(hidden)] dbg_push_to_ring`
//! test seam (`benches/heap_xthread.rs`'s seam), per this task's explicit
//! instruction to use the real production path for the remote-density
//! seeding.
//!
//! ## Metrics
//!
//! - **Segment-scan COUNT** — the primary, always-available proxy for the
//!   O(S) cost. `find_segment_with_free_impl` is `pub(crate)` with no
//!   runtime instrumentation counter and no test seam exposing one (adding
//!   one would touch allocation-routing source code, which this task
//!   forbids); instead the scan count is DETERMINISTIC BY CONSTRUCTION at
//!   `holes_pct = 0` (exactly `S - 1` non-target segments must be walked
//!   before the target succeeds) and is reported analytically alongside the
//!   measured wall-clock for that cell, cross-checked against the growth
//!   shape of the wall-clock numbers themselves (Verification step 1 below).
//! - **Wall-clock latency** of the `alloc()` call that triggers the scan
//!   (own-segment free-list miss -> `find_segment_with_free` -> success),
//!   isolated from construction/hole-punching (untimed) exactly as the
//!   sibling A2/A3 harnesses isolate their timed windows. `SCANS_PER_TRIAL`
//!   back-to-back scans are batched under one timer call per trial (see
//!   `time_n_scans`'s doc comment for why per-scan timing was abandoned —
//!   timer-call overhead measured on this harness's build host was
//!   comparable to a single scan's true cost); mean, p50, and p99 are
//!   computed by pooling one per-scan-mean sample from each of
//!   `adaptive_repeats(class_idx, s)` independently re-constructed trials.
//! - **Refill-cycle count**: not separately obtainable without instrumenting
//!   `find_segment_with_free`'s caller (`alloc_small`) — the number of
//!   magazine-refill invocations is a `fastbin`-layer concept and this
//!   harness deliberately stays below the magazine (see "Only the SUBSTRATE
//!   `AllocCore` path" above); honestly reported as N/A rather than
//!   fabricated.
//! - **loads/L1/LLC cache counters**: not cleanly obtainable from a portable
//!   Rust harness on this platform (Windows, no `perf`/PMU access without an
//!   external profiler) — honestly reported as unavailable rather than
//!   fabricated, matching this task brief's explicit instruction. Users
//!   wanting cache-level detail should run this binary's alloc pattern under
//!   `iai-callgrind` (Linux-only, see `perf_gate_iai.rs`) or a native
//!   profiler.
//! - **Directory-update cost**: `find_segment_with_free_impl`'s own
//!   bookkeeping on a hit is the un-pool call (`alloc-decommit` only, not
//!   compiled into this harness's default `alloc-core`-only build) and the
//!   ring-drain guard (`alloc-xthread` only). Under the default `alloc-core`
//!   build neither exists, so directory-update cost is bundled into the
//!   reported wall-clock (there is nothing to separate); the `alloc-xthread`
//!   remote-density matrix's per-segment ring-drain-guard cost is the
//!   closest analogue and is visible in that matrix's own wall-clock deltas
//!   at `dirty_pct = 0` vs `dirty_pct > 0`.
//!
//! ## Repeated-measurement discipline (the R6-OPT-A2 lesson)
//!
//! R6-OPT-A2's sibling agent found a real state-leak bug in ITS OWN harness
//! that only surfaced on a repeated measurement of the SAME configuration
//! within one run — caught by the orchestrator's independent re-run, not the
//! implementing agent's own testing. Applying that discipline here:
//! [`verify_repeated_measurement_consistency`] re-runs one fixed
//! `(S, holes_pct, class_idx)` cell `REPEAT_CONSISTENCY_COUNT` times,
//! interleaved with unrelated cells (not back-to-back — a fix that only
//! works back-to-back would not prove a leak is gone), and reports the
//! spread. Each cell builds a BRAND NEW `AllocCore` (no shared mutable state
//! across cells other than process-global atomics this harness never reads),
//! so there is no `AllocCore`-level state to leak between cells — this check
//! exists to positively DEMONSTRATE that isolation, not merely assert it.
//!
//! ## Tiering
//!
//! Mirrors the `--reduced` / `--full-matrix` convention established by
//! `benches/heap_fanin_persistent.rs` / `benches/medium_size_sweep.rs`:
//!
//! - **default (no flag)**: S in {1, 3, 16, 64, 256, 1023} x holes in
//!   {0, 50} x THREE representative classes (smallest/16 B, mid/~4 KiB,
//!   largest/`SMALL_MAX`) at `holes=0`, plus the full 49-class sweep ONLY at
//!   a point cheap enough for EVERY class including the smallest (`S = 16`,
//!   `holes = 0`) — enough to see the growth curve and confirm no per-class
//!   anomaly, in well under a minute. Cells whose block size x S would need
//!   an excessive allocation count (small block, large S) are SKIPPED with
//!   an honest message, never silently downgraded (see
//!   `QUICK_TIER_MAX_BLOCKS_PER_TRIAL`).
//! - **`--reduced`**: full S x holes (4 points: 0/25/50/75) cross product at
//!   the three representative classes, plus the full 49-class sweep at two
//!   S points (64 and 1023).
//! - **`--full-matrix`**: full S x holes x all-49-classes cross product.
//!   `S = 1023` alone means up to 1024 segment carves per cell (`S`
//!   scannable segments plus one extra always-full "current" segment — see
//!   `construct_s_segments`'s doc comment for why); the full matrix is NOT
//!   time-budgeted — run it deliberately.
//! - `S = 1023`, not the task brief's literal `1024`, throughout this
//!   harness (both the main matrix and the remote-density matrix below):
//!   `construct_s_segments` carves `S + 1` registered segments total, and
//!   `MAX_SEGMENTS` (`src/alloc_core/segment_table.rs`) hard-caps the
//!   `SegmentTable` at 1024 slots — `S = 1024` would need 1025. `1023` sits
//!   comfortably inside that ceiling and is functionally indistinguishable
//!   from `1024` for showing the O(S) divergence at the ceiling.
//!
//! The remote-dirty-density matrix (`alloc-xthread`-gated) is reported
//! separately, always at a small fixed representative S/class subset (it is
//! the more expensive, thread-spawning axis): dirty_pct in {0, 1, 10, 100}
//! at S in {3, 64, 1023}, one class (`SMALL_MAX`, so blocks are large enough
//! that a handful of allocations already spans multiple segments — the same
//! geometry choice `multiseg_cold_256k` and `medium_size_sweep` document).

#![cfg(feature = "alloc-core")]
#![allow(clippy::cast_precision_loss)]

use std::alloc::Layout;
use std::time::{Duration, Instant};

use sefer_alloc::{AllocCore, SegmentLayout};

/// Segment counts to sweep — the entire point of this harness. `1` and `3`
/// sit inside the region the existing `multiseg_cold_256k` IAI judge already
/// covers (the kill-gate check); `16`/`64` cross into "measurably rising";
/// `256`/`1023` is where the O(S) cost should clearly diverge. `1023`, not
/// the task brief's literal `1024`: `construct_s_segments` carves `S + 1`
/// registered segments total (the `S` SCANNABLE ones this parameter
/// controls, plus one extra always-full "current" segment — see that
/// function's doc comment for why the extra segment is structurally
/// required), and `MAX_SEGMENTS` (`src/alloc_core/segment_table.rs`) hard-caps
/// the `SegmentTable` at 1024 registered slots — `S = 1024` would need 1025,
/// exceeding it. `S = 1023` is the largest value this harness can honestly
/// reach while still sitting comfortably inside the table's capacity, and it
/// is functionally indistinguishable from `1024` for the purpose of showing
/// the O(S) divergence at the ceiling.
const S_VALUES: &[u32] = &[1, 3, 16, 64, 256, 1023];

/// Hole percentages to sweep (task brief: 0/25/50/75).
const HOLES_VALUES: &[u32] = &[0, 25, 50, 75];

/// Repeated (re-constructed) trials per `(S, holes, class)` cell, AT THE
/// CHEAPEST cell (large block size, small S) — the ceiling
/// [`adaptive_repeats`] scales down from. Each trial yields exactly ONE
/// sample (the mean of `SCANS_PER_TRIAL` back-to-back scans, batched under a
/// single timer call — see [`time_n_scans`]'s doc comment for why
/// per-SCAN timing was abandoned in favour of per-BATCH timing), so this
/// ceiling is what gives the pooled mean/p50/p99 its sample count across
/// INDEPENDENT constructions (a fresh `AllocCore` / fresh segment carve
/// pattern each trial). Kept low enough that even the smallest (16 B) class
/// at large S stays inside the "seconds-to-minutes" fast-profile budget.
const MAX_REPEATS_PER_CELL: usize = 30;

/// Target total blocks allocated across all `MAX_REPEATS_PER_CELL` trials of
/// one cell, used by [`adaptive_repeats`] to scale the repeat count down as
/// `S * (blocks needed to fill S segments of this class)` grows — the actual
/// cost driver (a 16 B-class cell at `S = 1024` needs ~1024 * 262,000 blocks
/// per SINGLE trial; running that 20x would blow the fast-profile budget by
/// orders of magnitude, exactly what stalled the first run of this harness
/// during development — see the module's "adaptive repeats" note).
const TARGET_TOTAL_BLOCKS_BUDGET: u64 = 200_000;

/// Blocks needed to fill `s` segments of `class_idx`, approximated as
/// `s * (SEGMENT / block_size)` — an upper-bound estimate (ignores the
/// smaller primordial-segment capacity and per-segment metadata overhead,
/// both of which only REDUCE the true blocks-per-segment count, so this is a
/// safe overestimate that never under-scales the repeat count).
fn approx_blocks_for_s(class_idx: usize, s: u32) -> u64 {
    let block_size = AllocCore::dbg_block_size(class_idx) as u64;
    let per_segment = (SegmentLayout::SEGMENT as u64 / block_size).max(1);
    per_segment.saturating_mul(s as u64)
}

/// How many repeated trials to run for a `(class_idx, s)` cell: scales down
/// from [`MAX_REPEATS_PER_CELL`] so that `repeats * approx_blocks_for_s(..)`
/// stays near [`TARGET_TOTAL_BLOCKS_BUDGET`], with a floor of 1 (every cell
/// is measured at least once) — see the module's "adaptive repeats" note for
/// why a flat repeat count across the whole S x class matrix is not viable.
fn adaptive_repeats(class_idx: usize, s: u32) -> usize {
    let per_trial = approx_blocks_for_s(class_idx, s).max(1);
    let scaled = TARGET_TOTAL_BLOCKS_BUDGET / per_trial;
    scaled.clamp(1, MAX_REPEATS_PER_CELL as u64) as usize
}

/// Hard ceiling on blocks-per-SINGLE-trial for the default (quick) tier.
/// `adaptive_repeats` alone cannot save a cell whose single trial already
/// needs, e.g., ~268M allocations (class 0 = 16 B, S = 1024:
/// `1024 * 262,144`) — that one trial alone would blow the "seconds to a
/// couple of minutes" fast-profile budget on its own, repeats or not. Cells
/// exceeding this ceiling are SKIPPED (not silently truncated to a smaller
/// S — that would misreport what was actually measured) with an honest
/// `eprintln!`, only in the quick (default) tier; `--reduced`/`--full-matrix`
/// intentionally accept the cost (their own doc comments already warn they
/// are not time-budgeted).
const QUICK_TIER_MAX_BLOCKS_PER_TRIAL: u64 = 5_000_000;

fn quick_tier_cell_too_expensive(class_idx: usize, s: u32) -> bool {
    approx_blocks_for_s(class_idx, s) > QUICK_TIER_MAX_BLOCKS_PER_TRIAL
}

/// How many times [`verify_repeated_measurement_consistency`] re-runs the
/// SAME fixed cell, interleaved with unrelated cells — the R6-OPT-A2
/// state-leak discipline.
const REPEAT_CONSISTENCY_COUNT: usize = 5;

fn base_of(p: *mut u8) -> usize {
    SegmentLayout::segment_base_of(p as usize)
}

/// Build a layout for small class `class_idx`, using `AllocCore::dbg_block_size`
/// (the exact block size the class resolves to) so the constructed layout is
/// guaranteed to route to `class_idx` via `class_for`'s fast path (`align = 8
/// <= SMALL_ALIGN_MAX`).
fn layout_for_class(class_idx: usize) -> Layout {
    let size = AllocCore::dbg_block_size(class_idx);
    Layout::from_size_align(size, 8).expect("class block size is a valid layout")
}

/// How many `class_idx`-sized blocks a segment holds TOTAL (the caller's
/// block plus its full refill batch), for the PRIMORDIAL segment (index 0,
/// which hosts the self-hosted `SegmentTable` registry alongside the small
/// payload, so it has LESS usable space than a plain small segment) and for
/// an ordinary FRESH small segment (every segment after the primordial).
/// Measured empirically on a THROWAWAY `AllocCore` (never assumed from
/// layout-offset arithmetic this harness has no public access to): allocate
/// `class_idx`-sized blocks one at a time and record how many land in each
/// of the first two distinct segments before a third appears. This is exact
/// -- the real production carve/refill path is exercised directly, not
/// approximated -- and paid ONCE per class (cached by the caller), not per
/// cell.
struct SegmentCapacity {
    primordial: usize,
    fresh: usize,
}

fn measure_segment_capacity(class_idx: usize) -> SegmentCapacity {
    let mut core = AllocCore::new().expect("AllocCore::new (capacity probe)");
    let layout = layout_for_class(class_idx);
    let mut counts = [0usize; 2];
    let mut bases: Vec<usize> = Vec::new();
    loop {
        let p = core.alloc(layout);
        assert!(
            !p.is_null(),
            "measure_segment_capacity: alloc returned null"
        );
        let b = base_of(p);
        let idx = match bases.iter().position(|&sb| sb == b) {
            Some(i) => i,
            None => {
                bases.push(b);
                bases.len() - 1
            }
        };
        if idx >= 2 {
            // A third distinct segment appeared -- the second segment's
            // capacity is now fully known (its LAST block was the one just
            // before this one). Stop; `core` (and everything it carved) is
            // simply dropped/leaked, same convention as every other
            // throwaway `AllocCore` in this harness.
            break;
        }
        counts[idx] += 1;
    }
    SegmentCapacity {
        primordial: counts[0],
        fresh: counts[1],
    }
}

/// Result of constructing exactly `s` "scannable" segments for one class,
/// PLUS one extra always-full "current" segment (see the module note on
/// `small_cur` below) — `s + 1` registered segments total.
struct Constructed {
    core: AllocCore,
    layout: Layout,
    /// `buckets[i]` = every live block base-pointer in the i-th SCANNABLE
    /// segment (scan order), `i in [0, s)`. Does NOT include the extra
    /// always-full "current" segment (index `s`, tracked separately —
    /// `time_n_scans` never touches it).
    buckets: Vec<Vec<*mut u8>>,
    /// `bases[i]` = the segment base of `buckets[i]`, captured at
    /// construction time (independent of whatever `buckets[i]` still holds
    /// after `punch_holes` empties most of it) — used by
    /// [`time_n_scans`]'s verification that a found block really came from
    /// the TARGET segment (`bases[S-1]`), not an earlier one.
    bases: Vec<usize>,
}

/// Construct exactly `s` "scannable" registered segments for `class_idx`,
/// PLUS one extra always-full "current" segment — `s + 1` segments total,
/// `dbg_table_count() == s + 1` on return.
///
/// **Why an EXTRA segment is required — a real structural bug found and
/// fixed during this harness's own development.** `alloc_small`'s step 1
/// (`pop_free(self.small_cur, ..)`) ALWAYS runs before step 2
/// (`find_segment_with_free`'s scan), and `reserve_small_segment` always
/// ends by pointing `small_cur` at the JUST-CARVED (i.e. most recent)
/// segment. This harness's first working design left the free block meant
/// for the scan to find IN that most-recently-carved segment — which is
/// ALWAYS `small_cur` — so `pop_free(small_cur, ..)` served it directly
/// EVERY time, and `find_segment_with_free` was structurally unreachable no
/// matter how many other segments existed (confirmed by a real panic during
/// development: `time_n_scans`'s own `dbg_table_count()` invariant check
/// caught the scan falling through to a fresh carve instead of finding the
/// intended block — a hard proof, not a guess, that the "hit" was never
/// coming from the scan). The fix: construct `s` SCANNABLE segments (which
/// `find_segment_with_free` is meant to walk) as segments `[0, s)`, THEN
/// carve one MORE, always-kept-100%-full segment as segment `s` — THAT
/// becomes `small_cur`, and being full, `pop_free(small_cur, ..)` always
/// misses, forcing every `alloc()` call to genuinely fall through to
/// `find_segment_with_free`, which walks `[0, s)` — exactly the `s`
/// scannable segments this harness's `S` parameter is meant to control.
///
/// **Secondary correctness note (also found during development): the
/// refill-batch residual.** The carve path (`carve_block_with_refill`,
/// `src/alloc_core/alloc_core_small.rs`) does not just carve the ONE block a
/// caller asked for — on every fresh carve it ALSO carves a refill batch of
/// up to 31 EXTRA blocks (capped by however many more fit in that segment)
/// and pushes them onto the SAME segment's own `BinTable`. A naive "stop the
/// moment a fresh segment's first block is seen" loop therefore leaves up to
/// 30 undrained residual free blocks sitting on that segment — this
/// function fully drains each segment's EXACT known capacity (via
/// `measure_segment_capacity`, called once per class by the caller and
/// passed in) before moving on, so every constructed bucket accurately
/// reflects "this segment is completely full" with no hidden slack.
fn construct_s_segments(class_idx: usize, s: u32, capacity: &SegmentCapacity) -> Constructed {
    let mut core = AllocCore::new().expect("AllocCore::new (OS reservation)");
    let layout = layout_for_class(class_idx);

    let mut buckets: Vec<Vec<*mut u8>> = Vec::new();
    let mut seen_bases: Vec<usize> = Vec::new();

    // Carve `s + 1` segments total: `s` scannable ones (indices 0..s) plus
    // one extra "current" segment (index s) that stays `small_cur` after
    // this function returns and is kept 100% full so `pop_free(small_cur,
    // ..)` always misses (see the fn doc above). Each segment is fully
    // drained to its EXACT known capacity (`capacity.primordial` for index
    // 0, `capacity.fresh` for every later index) before construction moves
    // to the next, so no segment is left with a hidden refill-batch residual.
    let total_segments = s as usize + 1;
    while seen_bases.len() < total_segments {
        let this_idx = seen_bases.len();
        let this_cap = if this_idx == 0 {
            capacity.primordial
        } else {
            capacity.fresh
        };
        let mut bucket: Vec<*mut u8> = Vec::with_capacity(this_cap);
        let mut this_base: Option<usize> = None;
        for _ in 0..this_cap {
            let p = core.alloc(layout);
            assert!(!p.is_null(), "construct_s_segments: alloc returned null");
            let b = base_of(p);
            match this_base {
                None => this_base = Some(b),
                Some(established) => assert_eq!(
                    b, established,
                    "construct_s_segments: a block landed on a DIFFERENT segment than \
                     expected before this segment's known capacity was reached -- \
                     measure_segment_capacity's count diverged from this construction's \
                     actual carve pattern"
                ),
            }
            bucket.push(p);
        }
        seen_bases.push(this_base.expect("this_cap > 0 for every real class"));
        buckets.push(bucket);
    }

    assert_eq!(
        core.dbg_table_count(),
        total_segments as u32,
        "construct_s_segments: dbg_table_count() diverged from S+1 (the S scannable segments \
         plus the extra always-full current segment) (S={s}, class_idx={class_idx}) — \
         construction bug, not a scan finding"
    );
    assert_eq!(
        buckets.len(),
        total_segments,
        "construct_s_segments: bucket count diverged from S+1"
    );

    // The extra "current" segment (index s, last carved -- guaranteed to be
    // `small_cur` by `reserve_small_segment`'s invariant) is intentionally
    // left OUT of `buckets`/`bases` returned to the caller: `time_n_scans`
    // and `punch_holes` only ever see the `s` SCANNABLE segments, never
    // touching the current one (kept 100% full for the whole trial).
    let current_bucket = buckets.pop().expect("at least s+1 buckets");
    let _ = current_bucket; // fully-carved, deliberately never freed: stays 100% full.
    seen_bases.pop();

    Constructed {
        core,
        layout,
        buckets,
        bases: seen_bases,
    }
}

/// Punch holes per the module doc's rule: segments `[0, S-1)` get `holes_pct%`
/// of their own blocks freed (0% => none, i.e. stay completely full); segment
/// `S-1` (the target) gets exactly one block freed for [`time_n_scans`] to
/// find.
///
/// Works uniformly for every `S >= 1`, including `S = 1` (a single scannable
/// segment — the primordial): `small_cur` is the SEPARATE, always-full
/// "current" segment `construct_s_segments` carves beyond the `s` scannable
/// ones (see that function's doc comment), never touched here, so
/// `pop_free(small_cur, ..)` always misses and `find_segment_with_free` is
/// unconditionally reached on every `alloc()` call regardless of S — at
/// `S = 1` the scan trivially walks 0 non-target segments before finding the
/// one (and only) scannable segment's freed block.
fn punch_holes(c: &mut Constructed, holes_pct: u32) {
    let s = c.buckets.len();
    assert!(s >= 1, "punch_holes: need at least one segment");
    for i in 0..s.saturating_sub(1) {
        let bucket = &mut c.buckets[i];
        let n_free = (bucket.len() * holes_pct as usize) / 100;
        for _ in 0..n_free {
            if let Some(p) = bucket.pop() {
                // SAFETY: `p` was returned by a prior `c.core.alloc(c.layout)`
                // call in `construct_s_segments` above, is still live (never
                // freed before this point), and is freed exactly once here.
                unsafe { c.core.dealloc(p, c.layout) };
            }
        }
    }
    // Target segment (the last scannable one, index S-1): free exactly one
    // block, just enough to end the scan there and nowhere earlier.
    let target = c.buckets.last_mut().expect("at least one bucket");
    let victim = target.pop().expect("target segment has at least one block");
    // SAFETY: `victim` was returned by a prior `c.core.alloc(c.layout)` call
    // in `construct_s_segments`, is still live, and is freed exactly once.
    unsafe { c.core.dealloc(victim, c.layout) };
}

/// How many back-to-back scans to time WITHIN one construction, per trial.
/// A single scan call is often only tens to a few hundred nanoseconds even
/// at large S (a handful of pointer-sized reads per segment) — comparable to
/// or below `Instant::now()`'s own resolution/overhead on some platforms,
/// which would swamp the signal with timer-quantization noise rather than
/// the O(S) cost this harness exists to show. Batching `SCANS_PER_TRIAL`
/// scans under ONE `Instant::now()`/`elapsed()` pair (dividing the total by
/// the count) amortizes that per-call timer overhead, the same reasoning
/// `benches/perf_gate_iai.rs`'s `CHURN_OPS`/`COLD_BATCH` batching and this
/// project's other custom timing-loop harnesses already apply.
const SCANS_PER_TRIAL: usize = 256;

/// Time [`SCANS_PER_TRIAL`] back-to-back `alloc()` calls that must each fall
/// through to `find_segment_with_free`, re-arming the target segment's one
/// free slot between scans so every one of the `SCANS_PER_TRIAL` calls walks
/// the SAME (S, holes) geometry.
///
/// **No "drain" step is needed here** (an earlier working draft had one, and
/// it was a real bug — see below): `construct_s_segments` carves a SEPARATE
/// always-full "current" segment beyond the `s` scannable ones specifically
/// so that `small_cur` (which `reserve_small_segment` always points at the
/// most-recently-carved segment, i.e. that extra one) NEVER has a free
/// block. `pop_free(small_cur, ..)` (step 1 of `alloc_small`) therefore
/// always misses, unconditionally, on every call — every `alloc()` in the
/// loop below genuinely falls through to `find_segment_with_free` (step 2),
/// which walks `[0, s)`, the `s` scannable segments, before finding the one
/// [`punch_holes`] left with a free slot.
///
/// After each timed scan succeeds, the found block is freed straight back
/// (own-thread dealloc routes it to ITS OWN segment's BinTable, i.e. the
/// target's — see `dealloc_small`'s doc comment), re-arming the target's one
/// free slot for the next iteration — so `SCANS_PER_TRIAL` iterations walk
/// the IDENTICAL (S, holes) geometry every time, not a slowly-draining one.
///
/// **Two real bugs found and fixed during this harness's own development —
/// both caught by this harness's own "a flat curve is a bug" verification
/// discipline, not assumed away:**
///
/// 1. **Structural: the free block was originally placed IN `small_cur`
///    itself.** The first working draft left the scan's target free block in
///    the LAST-carved segment, which (per `reserve_small_segment`'s
///    invariant) is ALWAYS `small_cur` — so `pop_free(small_cur, ..)` served
///    it directly every time and `find_segment_with_free` was structurally
///    UNREACHABLE, no matter how many other segments existed. A "drain
///    first" probe alloc was tried to work around this, but that just moved
///    the bug: draining `small_cur`'s one free block consumed the very block
///    the scan needed to find, so the FOLLOWING (timed) alloc found nothing
///    anywhere and fell through to a fresh carve instead — caught by a
///    `dbg_table_count()` mismatch panic inside this function's own
///    correctness assertions (a hard proof, not a guess). Fixed at the
///    ROOT — `construct_s_segments` now carves an extra, dedicated,
///    always-full "current" segment so `small_cur` can never satisfy step 1;
///    see that function's doc comment for the full story.
/// 2. **Timer resolution/overhead.** SEPARATELY, `Instant::now()`/
///    `.elapsed()` on this harness's build host measured ~60-125 ns of pure
///    timer-call overhead per pair (verified with a standalone
///    microbenchmark) — comparable to a single scan's true cost even at
///    large S, which would swamp the signal in timer noise even with bug 1
///    fixed. Fixed by batching `SCANS_PER_TRIAL` scans under ONE
///    `Instant::now()`/`.elapsed()` pair (the same amortisation
///    `perf_gate_iai.rs`'s `CHURN_OPS`/`COLD_BATCH` loops and this project's
///    other custom timing-loop harnesses already apply), dividing by the
///    count for a per-scan mean. This folds the correctness assertions' own
///    cost (run every iteration, identical at every S) into every sample as
///    a constant additive offset that raises the absolute floor but does not
///    change the GROWTH SHAPE across S, which is what this harness exists to
///    show.
///
/// Returns a SINGLE per-scan-mean sample per call (the caller runs this once
/// per independent trial and pools samples across trials for mean/p50/p99 —
/// see [`run_cell`]).
fn time_n_scans(c: &mut Constructed, out: &mut Vec<Duration>) {
    let target_base = *c.bases.last().expect("at least one segment");
    let count_before = c.core.dbg_table_count();
    let t0 = Instant::now();
    for _ in 0..SCANS_PER_TRIAL {
        let found = c.core.alloc(c.layout);
        std::hint::black_box(found);
        // Correctness verification, every iteration (see the fn doc: cheap,
        // single-threaded, identical cost at every S -- no cross-cell bias).
        assert!(!found.is_null(), "time_n_scans: scan-alloc returned null");
        assert_eq!(
            c.core.dbg_table_count(),
            count_before,
            "time_n_scans: a NEW segment was carved mid-scan -- find_segment_with_free \
             returned None (scan MISSED the target), not the intended multi-segment hit"
        );
        // At holes_pct == 0 every non-target segment (0..S-1) is completely
        // full (`punch_holes`'s worst-case rule), so the scan can ONLY
        // succeed at the target (index S-1) -- exact-match assertion. At
        // holes_pct > 0, `punch_holes` ALSO frees a fraction of every
        // non-target segment's blocks, so `find_segment_with_free` (which
        // returns the FIRST segment in scan order with a free block) may
        // legitimately terminate at an EARLIER segment than the target --
        // per this file's module doc ("holes_pct > 0 ... does NOT preserve
        // 'must walk all S-1'"), so the assertion only requires `found` to
        // come from ONE of the `s` scannable segments (never a phantom
        // segment outside that set, which the `dbg_table_count()` check
        // above already rules out via a different mechanism, but this
        // catches a same-count substitution too).
        let found_base = base_of(found);
        assert!(
            c.bases.contains(&found_base),
            "time_n_scans: scan-alloc came from a base NOT in this trial's known scannable \
             segment set -- unexpected: dbg_table_count() didn't change, so this should be \
             impossible unless a segment base was misrecorded during construction"
        );
        // Re-arm whichever segment's free slot was just consumed: `found`'s
        // OWN segment (own-thread dealloc always routes to the block's
        // actual owner via `dealloc_small`, regardless of which segment that
        // is) -- SAFETY: `found` was returned by the immediately preceding
        // `alloc` call on the same layout, still live, freed exactly once.
        unsafe { c.core.dealloc(found, c.layout) };
        let _ = target_base; // informational identifier only at holes_pct > 0.
    }
    let batch_elapsed = t0.elapsed();
    let per_scan = batch_elapsed / (SCANS_PER_TRIAL as u32);
    out.push(per_scan);
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

struct CellResult {
    s: u32,
    holes_pct: u32,
    class_idx: usize,
    mean: Duration,
    p50: Duration,
    p99: Duration,
    /// Analytically-known expected non-target segments walked before success
    /// (only exact at `holes_pct == 0`; at `holes_pct > 0` this is the
    /// worst-case upper bound, not the expected value — an earlier segment's
    /// hole may end the scan sooner, see the module doc's "holes_pct > 0"
    /// discussion).
    expected_max_segments_walked: u32,
}

/// Run one `(S, holes_pct, class_idx)` cell: construct, punch holes,
/// time [`SCANS_PER_TRIAL`] back-to-back scans, repeat that whole
/// construct+measure cycle [`adaptive_repeats`] times (fresh construction
/// each trial — no shared mutable state across trials), pool every
/// individual scan sample across all trials, report mean/p50/p99.
fn run_cell(class_idx: usize, s: u32, holes_pct: u32) -> CellResult {
    let repeats = adaptive_repeats(class_idx, s);
    let capacity = measure_segment_capacity(class_idx);
    let mut samples: Vec<Duration> = Vec::with_capacity(repeats * SCANS_PER_TRIAL);
    for _ in 0..repeats {
        let mut c = construct_s_segments(class_idx, s, &capacity);
        punch_holes(&mut c, holes_pct);
        time_n_scans(&mut c, &mut samples);
        // `c` (and every block it carved) is dropped here -- `AllocCore` has
        // no `Drop` impl that releases segments (by design; see
        // `AllocCore`'s doc comments elsewhere in the crate), so each trial's
        // OS reservations are simply abandoned (leaked for the process
        // lifetime) exactly as `perf_gate_iai.rs` and the sibling harnesses
        // already do for their own per-iteration `AllocCore`/`SeferAlloc`
        // instances -- acceptable for a short-lived bench process.
    }
    samples.sort_unstable();
    let sum: Duration = samples.iter().sum();
    let mean = sum / (samples.len() as u32);
    let p50 = percentile(&samples, 0.50);
    let p99 = percentile(&samples, 0.99);
    let expected_max_segments_walked = s.saturating_sub(1);
    CellResult {
        s,
        holes_pct,
        class_idx,
        mean,
        p50,
        p99,
        expected_max_segments_walked,
    }
}

fn report_cell(r: &CellResult) {
    eprintln!(
        "segment_directory_sweep: S={:>4} holes={:>3}% class={:>2} \
         max_segments_walked<={:>4} mean={:>9.1}ns p50={:>9.1}ns p99={:>9.1}ns",
        r.s,
        r.holes_pct,
        r.class_idx,
        r.expected_max_segments_walked,
        r.mean.as_secs_f64() * 1e9,
        r.p50.as_secs_f64() * 1e9,
        r.p99.as_secs_f64() * 1e9,
    );
}

/// Representative classes for the default/quick tier: smallest (index 0,
/// 16 B), a mid-range class (~4 KiB, one of the explicit page-aligned
/// classes from `size_classes.rs`'s `PAGE_ALIGNED_EXTRA`), and the largest
/// (`SMALL_CLASS_COUNT - 1`, `SMALL_MAX`, the same class
/// `multiseg_cold_256k` and `medium_size_sweep` use).
fn representative_classes() -> Vec<usize> {
    let n = AllocCore::dbg_small_class_count();
    // `dbg_layout_class_for` is an instance method (it goes through
    // `AllocCore::classify`, mirroring the real alloc-time dispatch); a
    // throwaway instance is enough for this one-off "which class covers
    // 4096 B" lookup — no segment is carved by the lookup itself.
    let probe = AllocCore::new().expect("AllocCore::new (probe instance)");
    let mid = probe
        .dbg_layout_class_for(Layout::from_size_align(4096, 8).unwrap())
        .unwrap_or(n / 2);
    vec![0, mid, n - 1]
}

fn all_classes() -> Vec<usize> {
    (0..AllocCore::dbg_small_class_count()).collect()
}

/// Kill-gate check: S<=3 must stay consistent (same order of magnitude, same
/// qualitative behaviour) with what `multiseg_cold_256k`
/// (`benches/perf_gate_iai.rs`) already reports for its 3-segment case.
/// `multiseg_cold_256k` is an `iai-callgrind` (Linux-only, Valgrind
/// instruction counts) judge -- this harness cannot literally reproduce its
/// numbers (different methodology, different platform, wall-clock not
/// instruction count) -- so this check reports THIS harness's own S=1/S=3
/// wall-clock at the SAME size class (`SMALL_MAX`, 258,752 B --
/// `multiseg_cold_256k`'s `MULTISEG_BLOCK`) for a human/CI-log side-by-side
/// sanity comparison, and asserts the internal invariant the kill-gate
/// actually needs: S=1 and S=3 must be FLAT relative to each other (no
/// dramatic multi-x jump this early in a 6-point 1..1023 curve) --  the
/// divergence must show up at HIGH S, not low S.
fn run_kill_gate_check() {
    let class_idx = AllocCore::dbg_small_class_count() - 1; // SMALL_MAX
    let s1 = run_cell(class_idx, 1, 0);
    let s3 = run_cell(class_idx, 3, 0);
    let s1023 = run_cell(class_idx, 1023, 0);
    eprintln!(
        "segment_directory_sweep: KILL-GATE (S<=3 vs multiseg_cold_256k's S=3 case, \
         class=SMALL_MAX={} B):",
        SegmentLayout::SMALL_MAX
    );
    report_cell(&s1);
    report_cell(&s3);
    report_cell(&s1023);
    let ratio_3_over_1 = s3.mean.as_secs_f64() / s1.mean.as_secs_f64().max(1e-12);
    let ratio_1023_over_3 = s1023.mean.as_secs_f64() / s3.mean.as_secs_f64().max(1e-12);
    eprintln!(
        "segment_directory_sweep: KILL-GATE ratios: mean(S=3)/mean(S=1)={ratio_3_over_1:.2}x \
         mean(S=1023)/mean(S=3)={ratio_1023_over_3:.2}x -- gate requires the SECOND ratio to be \
         visibly larger than the FIRST (divergence at high S, not low S; S<=3 stays close to \
         flat, matching the existing IAI judge's small-S region)."
    );
    assert!(
        ratio_1023_over_3 > ratio_3_over_1,
        "KILL-GATE VIOLATION: S=1->S=3 grew MORE than S=3->S=1023 on a per-step basis -- \
         this would mean the region the existing multiseg_cold_256k judge already covers (S<=3) \
         is where the cost is concentrated, not the high-S region this harness exists to expose. \
         Either a harness construction bug, or a genuine surprising finding that needs \
         investigation before trusting the rest of this sweep."
    );
}

/// Verification step 2: repeated measurement of the SAME fixed cell,
/// interleaved with unrelated cells, must be consistent within one run (the
/// R6-OPT-A2 state-leak discipline).
fn verify_repeated_measurement_consistency() {
    let class_idx = AllocCore::dbg_small_class_count() - 1;
    let fixed_s = 64;
    let fixed_holes = 0;
    eprintln!(
        "segment_directory_sweep: repeated-cell consistency check (S={fixed_s}, \
         holes={fixed_holes}%, class=SMALL_MAX, {REPEAT_CONSISTENCY_COUNT} repeats \
         interleaved with unrelated cells):"
    );
    let mut means_ns: Vec<f64> = Vec::with_capacity(REPEAT_CONSISTENCY_COUNT);
    for i in 0..REPEAT_CONSISTENCY_COUNT {
        let r = run_cell(class_idx, fixed_s, fixed_holes);
        eprintln!(
            "  repeat[{i}]: mean={:.1}ns p50={:.1}ns p99={:.1}ns",
            r.mean.as_secs_f64() * 1e9,
            r.p50.as_secs_f64() * 1e9,
            r.p99.as_secs_f64() * 1e9,
        );
        means_ns.push(r.mean.as_secs_f64() * 1e9);
        // Interleave an unrelated cell between repeats -- the exact shape
        // that exposed R6-OPT-A2's leak (a fix that only works back-to-back
        // would not prove the leak is gone).
        if i + 1 < REPEAT_CONSISTENCY_COUNT {
            let _ = run_cell(class_idx, 16, 50);
        }
    }
    let mean_of_means = means_ns.iter().sum::<f64>() / means_ns.len() as f64;
    let max_dev = means_ns
        .iter()
        .map(|&m| (m - mean_of_means).abs())
        .fold(0.0_f64, f64::max);
    let rel_spread = max_dev / mean_of_means.max(1e-9);
    eprintln!(
        "segment_directory_sweep: repeated-cell spread: mean_of_means={mean_of_means:.1}ns \
         max_abs_dev={max_dev:.1}ns rel_spread={:.1}%",
        rel_spread * 100.0
    );
    // Generous bound (this is wall-clock on a shared CI/dev machine, not
    // instruction count) -- the point is catching a GROSS state leak
    // (unboundedly growing table, stale segments accumulating across
    // "independent" cells), not asserting tight timing determinism. A leak
    // would show as a clear monotonic drift, not a bounded jitter -- a 5x
    // relative-spread ceiling comfortably separates the two.
    assert!(
        rel_spread < 5.0,
        "repeated-cell consistency check FAILED: rel_spread={:.1}% -- possible state leak \
         across 'independent' AllocCore instances (each run_cell call builds a brand new \
         AllocCore; a leak here would mean some process-global state is bleeding between \
         supposedly isolated trials)",
        rel_spread * 100.0
    );
}

/// Run one `(class_idx, s, holes_pct)` cell UNLESS it exceeds
/// [`QUICK_TIER_MAX_BLOCKS_PER_TRIAL`], in which case it is skipped with an
/// honest `eprintln!` explaining why (never silently downgraded to a
/// smaller/misreported S). Only used by [`run_quick_matrix`] — `--reduced`
/// and `--full-matrix` accept the full cost of every requested cell.
fn report_cell_quick_tier(class_idx: usize, s: u32, holes_pct: u32) {
    if quick_tier_cell_too_expensive(class_idx, s) {
        eprintln!(
            "segment_directory_sweep: SKIPPED S={s:>4} holes={holes_pct:>3}% class={class_idx:>2} \
             (block_size={} B -> ~{} blocks/trial exceeds the quick-tier {}-block ceiling; \
             re-run with --reduced or --full-matrix for this cell)",
            AllocCore::dbg_block_size(class_idx),
            approx_blocks_for_s(class_idx, s),
            QUICK_TIER_MAX_BLOCKS_PER_TRIAL,
        );
        return;
    }
    report_cell(&run_cell(class_idx, s, holes_pct));
}

fn run_quick_matrix() {
    eprintln!("segment_directory_sweep: quick (default) matrix");
    let classes = representative_classes();
    // S x holes={0,50} at each representative class. Cells whose single
    // trial would need an excessive block count (small block size x large S)
    // are skipped with an honest message — see `QUICK_TIER_MAX_BLOCKS_PER_TRIAL`.
    for &class_idx in &classes {
        for &s in S_VALUES {
            for &holes in &[0, 50] {
                report_cell_quick_tier(class_idx, s, holes);
            }
        }
    }
    // Full 49-class sweep at a point cheap enough for EVERY class (including
    // the 16 B smallest one) to actually run in the default tier: S=16 is
    // already past the "flat" S<=3 region and comfortably inside every
    // class's quick-tier budget (16 * 262,144 = ~4.2M, under the 5M ceiling
    // even for the smallest class). The higher-S full-class points (64,
    // 1023) are deliberately deferred to `--reduced`/`--full-matrix`.
    eprintln!("segment_directory_sweep: full-class sweep at S=16, holes=0%:");
    for class_idx in all_classes() {
        report_cell_quick_tier(class_idx, 16, 0);
    }
}

fn run_reduced_matrix() {
    eprintln!("segment_directory_sweep: --reduced matrix");
    let classes = representative_classes();
    for &class_idx in &classes {
        for &s in S_VALUES {
            for &holes in HOLES_VALUES {
                report_cell(&run_cell(class_idx, s, holes));
            }
        }
    }
    eprintln!("segment_directory_sweep: full-class sweep at S=64 and S=1023, holes=0%:");
    for &s in &[64u32, 1023] {
        for class_idx in all_classes() {
            report_cell(&run_cell(class_idx, s, 0));
        }
    }
}

fn run_full_matrix() {
    eprintln!("segment_directory_sweep: --full-matrix (S x holes x all 49 classes)");
    for class_idx in all_classes() {
        for &s in S_VALUES {
            for &holes in HOLES_VALUES {
                report_cell(&run_cell(class_idx, s, holes));
            }
        }
    }
}

// ── Remote-dirty-density matrix (alloc-global + alloc-xthread only) ────────
//
// `sefer_alloc::registry` (HeapCore/HeapRegistry) is itself gated on
// `alloc-global` (`src/lib.rs`), not just `alloc-core` — the same feature
// pair `tests/remote_fanin.rs` requires (`#![cfg(all(feature =
// "alloc-global", feature = "alloc-xthread"))]`).

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
mod remote_density {
    use super::{base_of, Duration, Instant};
    use sefer_alloc::registry::HeapRegistry;
    use std::alloc::Layout;
    use std::sync::{Arc, Barrier};
    use std::thread;

    /// Dirty-density percentages to sweep: fraction of segments that have a
    /// PENDING cross-thread-freed block sitting in their `RemoteFreeRing`
    /// (not yet drained) at scan time.
    const DIRTY_PCTS: &[u32] = &[0, 1, 10, 100];

    /// S values for the remote-density matrix -- a smaller, fixed
    /// representative subset (thread-spawning axis is more expensive than
    /// the pure-`AllocCore` axis; see the module doc's tiering note). `1023`,
    /// not `1024` -- see `super::S_VALUES`'s doc comment (this function also
    /// carves an extra always-full "current" segment beyond the `s`
    /// scannable ones, so `S = 1024` would need 1025 registered segments,
    /// exceeding `MAX_SEGMENTS`).
    const DENSITY_S_VALUES: &[u32] = &[3, 64, 1023];

    /// Construct `s` SCANNABLE segments of `SMALL_MAX`-sized blocks via a
    /// REAL `HeapCore` (through `HeapRegistry::claim`), PLUS one extra
    /// always-full "current" segment -- the SAME structural requirement
    /// `super::construct_s_segments`'s doc comment explains in full: without
    /// it, the free block meant for the scan to find would sit in
    /// `small_cur` itself (`reserve_small_segment` always points `small_cur`
    /// at the most-recently-carved segment), and `pop_free(small_cur, ..)`
    /// would serve it directly, making `find_segment_with_free` structurally
    /// unreachable -- a real bug this harness found and fixed in the
    /// non-xthread matrix first, then mirrored here. Every segment (target
    /// segment included) is fully drained to `SMALL_MAX`'s exact known
    /// capacity via `super::measure_segment_capacity`, so no segment is left
    /// with an undrained carve-refill-batch residual either (the harness's
    /// OTHER real construction bug, also fixed in the non-xthread matrix
    /// first). Returns the owning heap pointer, the layout used, and
    /// per-segment buckets (scan order) of live block pointers for the `s`
    /// SCANNABLE segments only (the extra current segment is deliberately
    /// left untracked/never touched, exactly like
    /// `super::construct_s_segments`).
    fn construct_s_segments_via_heap(
        s: u32,
    ) -> (
        *mut sefer_alloc::registry::HeapCore,
        Layout,
        Vec<Vec<*mut u8>>,
    ) {
        let heap = HeapRegistry::claim();
        assert!(!heap.is_null(), "HeapRegistry::claim returned null");
        let layout = Layout::from_size_align(sefer_alloc::SegmentLayout::SMALL_MAX, 8).unwrap();
        // `SMALL_MAX` is `AllocCore::dbg_small_class_count() - 1` by
        // construction (the largest small class) -- reuse the same
        // capacity-measurement helper the non-xthread matrix uses.
        let small_max_class = sefer_alloc::AllocCore::dbg_small_class_count() - 1;
        let capacity = super::measure_segment_capacity(small_max_class);

        let mut buckets: Vec<Vec<*mut u8>> = Vec::new();
        let mut seen_bases: Vec<usize> = Vec::new();
        let total_segments = s as usize + 1;
        while seen_bases.len() < total_segments {
            let this_idx = seen_bases.len();
            let this_cap = if this_idx == 0 {
                capacity.primordial
            } else {
                capacity.fresh
            };
            let mut bucket: Vec<*mut u8> = Vec::with_capacity(this_cap);
            let mut this_base: Option<usize> = None;
            for _ in 0..this_cap {
                // SAFETY: `heap` is a live claimed heap pointer from
                // `HeapRegistry::claim` above, used single-threaded here (no
                // other thread touches it until the deliberate cross-thread
                // dealloc phase below, which only targets specific
                // already-recorded pointers, never `heap` itself).
                let p = unsafe { (*heap).alloc(layout) };
                assert!(!p.is_null(), "construct_s_segments_via_heap: alloc null");
                let b = base_of(p);
                match this_base {
                    None => this_base = Some(b),
                    Some(established) => assert_eq!(
                        b, established,
                        "construct_s_segments_via_heap: a block landed on a DIFFERENT \
                         segment than expected before this segment's known capacity was \
                         reached -- measure_segment_capacity's count diverged from this \
                         construction's actual carve pattern"
                    ),
                }
                bucket.push(p);
            }
            seen_bases.push(this_base.expect("this_cap > 0 for SMALL_MAX"));
            buckets.push(bucket);
        }
        assert_eq!(
            buckets.len(),
            total_segments,
            "construct_s_segments_via_heap: bucket count diverged from S+1"
        );
        // The extra "current" segment (index s, last carved -- guaranteed to
        // be `small_cur`) is intentionally left OUT of the buckets returned
        // to the caller: never touched, kept 100% full for the whole trial.
        let current_bucket = buckets.pop().expect("at least s+1 buckets");
        let _ = current_bucket;
        seen_bases.pop();
        (heap, layout, buckets)
    }

    /// Seed `dirty_pct%` of the NON-TARGET segments (`[0, S-1)`) with exactly
    /// one PENDING remote-free each, via a real cross-thread `HeapCore::dealloc`
    /// call from a spawned producer thread -- the genuine production path
    /// (`dealloc_foreign_slow` -> `push_with_overflow_retry` ->
    /// `RemoteFreeRing::push`), never `dbg_push_to_ring`. The freed block is
    /// deliberately left UNDRAINED (the owner does not `alloc()` again before
    /// the timed scan), so it is still sitting in the ring when the scan
    /// walks that segment and must pay the lazy ring-drain cost.
    fn seed_remote_dirty(
        heap: *mut sefer_alloc::registry::HeapCore,
        layout: Layout,
        buckets: &mut [Vec<*mut u8>],
        dirty_pct: u32,
    ) {
        let s = buckets.len();
        let n_non_target = s.saturating_sub(1);
        let n_dirty = ((n_non_target as u64 * dirty_pct as u64) / 100) as usize;
        if n_dirty == 0 {
            return;
        }
        let heap_addr = heap as usize;
        let barrier = Arc::new(Barrier::new(2));
        for bucket in buckets.iter_mut().take(n_dirty) {
            let victim = bucket
                .pop()
                .expect("non-target segment has a block to seed");
            let addr = victim as usize;
            let b = barrier.clone();
            let h = thread::spawn(move || {
                let _ = sefer_alloc::registry::bootstrap::ensure();
                b.wait();
                let p = addr as *mut u8;
                // SAFETY: `p` was returned by `heap.alloc(layout)` on the
                // owner thread above, is still live, and is freed exactly
                // once here from a different thread -- the genuine
                // cross-thread free path this matrix is meant to exercise.
                unsafe {
                    (*(heap_addr as *mut sefer_alloc::registry::HeapCore)).dealloc(p, layout)
                };
            });
            barrier.wait();
            h.join().expect("producer thread panicked");
        }
    }

    /// Punch the "must walk all non-target segments" hole shape on top of
    /// the remote-dirty seeding: target segment (`S-1`) gets exactly one
    /// block freed via an OWN-THREAD dealloc (so the scan can still succeed
    /// there without depending on ring-drain timing at the target itself).
    fn punch_target_hole(
        heap: *mut sefer_alloc::registry::HeapCore,
        layout: Layout,
        buckets: &mut [Vec<*mut u8>],
    ) {
        let target = buckets.last_mut().expect("at least one bucket");
        let victim = target.pop().expect("target segment has a block");
        // SAFETY: `victim` was returned by `heap.alloc(layout)` on this same
        // (owner) thread, is still live, freed exactly once, own-thread.
        unsafe { (*heap).dealloc(victim, layout) };
    }

    /// Time the ONE `alloc()` call that must fall through to
    /// `find_segment_with_free` (which also lazily drains any dirty rings it
    /// walks past). No "drain" probe is needed first: `construct_s_segments_via_heap`
    /// carves a dedicated always-full "current" segment beyond the `s`
    /// scannable ones, so `small_cur` never has a free block and
    /// `pop_free(small_cur, ..)` (step 1 of `alloc_small`) always misses --
    /// the SAME fix as `super::time_n_scans` (see that function's doc
    /// comment for the full story of why an earlier "drain first" design was
    /// a real bug: draining `small_cur` when the target block WAS `small_cur`
    /// consumed the very block the scan needed to find).
    fn time_scan_alloc(heap: *mut sefer_alloc::registry::HeapCore, layout: Layout) -> Duration {
        let t0 = Instant::now();
        // SAFETY: `heap` is a live claimed heap; single-threaded use here
        // (all producer threads have already joined by this point).
        let found = unsafe { (*heap).alloc(layout) };
        let elapsed = t0.elapsed();
        assert!(!found.is_null());
        // Free the found block back so a subsequent measurement of this same
        // heap (if any) would find consistent state -- SAFETY: `found` was
        // returned by the immediately preceding `alloc` call on this thread
        // with the same layout, still live, freed exactly once.
        unsafe { (*heap).dealloc(found, layout) };
        elapsed
    }

    /// Repeated (re-constructed, re-seeded) trials per `(S, dirty_pct)` remote
    /// cell, AT THE CHEAPEST S. Unlike the non-xthread matrix's
    /// [`super::time_n_scans`], a single trial here cannot be re-armed and
    /// re-timed multiple times: the dirty ring state is consumed by its
    /// FIRST drain (a re-armed target segment's ring is empty on the second
    /// scan), so getting a stable mean/p50/p99 requires repeating the whole
    /// construct+seed+measure cycle instead — and per
    /// `run_remote_density_matrix`'s doc comment, every trial LEAKS a whole
    /// heap's worth of segments (never recycled). At `S = 1023` that is
    /// ~1024 real 4 MiB OS reservations PER TRIAL (~4 GiB); repeating that
    /// `REMOTE_REPEATS_PER_CELL` (originally a flat 10) times across
    /// `DIRTY_PCTS.len()` cells genuinely exhausted process memory during
    /// this harness's own development (`alloc()` legitimately returned
    /// null). [`remote_repeats_for`] scales the repeat count down at large S
    /// the same way [`super::adaptive_repeats`] does for the non-xthread
    /// matrix.
    const REMOTE_REPEATS_PER_CELL: usize = 10;

    /// How many repeated trials to run for a given `s` in the remote-density
    /// matrix — see [`REMOTE_REPEATS_PER_CELL`]'s doc comment for why a flat
    /// count is not viable at large S. Scales down so
    /// `repeats * (s + 1)` (roughly the segment count leaked per trial)
    /// stays bounded; floor of 1 (every cell still gets measured at least
    /// once).
    fn remote_repeats_for(s: u32) -> usize {
        const TARGET_TOTAL_SEGMENTS_BUDGET: u64 = 200;
        let per_trial = (s as u64 + 1).max(1);
        let scaled = TARGET_TOTAL_SEGMENTS_BUDGET / per_trial;
        scaled.clamp(1, REMOTE_REPEATS_PER_CELL as u64) as usize
    }

    /// **Heaps are claimed once and NEVER recycled across trials — a real
    /// bug found and fixed during this harness's own development.**
    /// `HeapRegistry::recycle` only returns a slot to the FREE pool; it does
    /// NOT reset/tear down the recycled `HeapCore`'s `AllocCore` (no segment
    /// is un-carved, `dbg_table_count()` and `small_cur` are left exactly as
    /// the previous trial left them — confirmed by `HeapRegistry::claim`'s
    /// own `initialised` gate, which skips `HeapCore::new()` entirely on
    /// every claim AFTER the slot's first-ever materialisation). A
    /// `recycle`-then-`claim` cycle between trials therefore handed back a
    /// `HeapCore` whose primordial segment was ALREADY partially or fully
    /// carved from the PRIOR trial — `construct_s_segments_via_heap`'s
    /// pre-measured `capacity.primordial`/`capacity.fresh` counts (measured
    /// on a FRESH, never-before-used `AllocCore`) then diverged from the
    /// REUSED heap's actual remaining capacity, producing a real panic
    /// ("a block landed on a DIFFERENT segment than expected") during this
    /// harness's own development. `MAX_HEAPS` (`src/registry/bootstrap.rs`)
    /// is 4096; this matrix's total trial count (`DENSITY_S_VALUES.len() *
    /// DIRTY_PCTS.len() * remote_repeats_for(s)`, `remote_repeats_for` scaled
    /// down at large S) is comfortably under that ceiling, so leaking one
    /// fresh heap per trial for this short-lived bench process (the SAME
    /// "leaked `AllocCore`, acceptable for a bench binary" convention the
    /// non-xthread matrix's `run_cell` already documents) is the simplest
    /// correct fix — never call `HeapRegistry::recycle` in this loop. A
    /// SEPARATE real memory-exhaustion bug (also found during this harness's
    /// own development, `alloc()` genuinely returning null) motivated
    /// `remote_repeats_for`'s S-scaling in the first place — at `S = 1023`,
    /// a flat repeat count of 10 leaked ~10 * 1024 real 4 MiB OS
    /// reservations (~40 GiB) across just ONE `dirty_pct` cell.
    pub(super) fn run_remote_density_matrix() {
        eprintln!("segment_directory_sweep: remote-dirty-density matrix (alloc-xthread)");
        for &s in DENSITY_S_VALUES {
            let repeats = remote_repeats_for(s);
            for &dirty_pct in DIRTY_PCTS {
                let mut samples: Vec<Duration> = Vec::with_capacity(repeats);
                for _ in 0..repeats {
                    let (heap, layout, mut buckets) = construct_s_segments_via_heap(s);
                    seed_remote_dirty(heap, layout, &mut buckets, dirty_pct);
                    punch_target_hole(heap, layout, &mut buckets);
                    samples.push(time_scan_alloc(heap, layout));
                    // `heap` is deliberately LEAKED here, not recycled -- see
                    // this function's doc comment above.
                }
                samples.sort_unstable();
                let sum: Duration = samples.iter().sum();
                let mean = sum / (samples.len() as u32);
                let p50 = super::percentile(&samples, 0.50);
                let p99 = super::percentile(&samples, 0.99);
                eprintln!(
                    "segment_directory_sweep: [remote] S={s:>4} dirty={dirty_pct:>3}% \
                     mean={:>9.1}ns p50={:>9.1}ns p99={:>9.1}ns",
                    mean.as_secs_f64() * 1e9,
                    p50.as_secs_f64() * 1e9,
                    p99.as_secs_f64() * 1e9,
                );
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let full_matrix = args.iter().any(|a| a == "--full-matrix");
    let reduced = args.iter().any(|a| a == "--reduced");

    run_kill_gate_check();
    verify_repeated_measurement_consistency();

    if full_matrix {
        run_full_matrix();
    } else if reduced {
        run_reduced_matrix();
    } else {
        run_quick_matrix();
    }

    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    remote_density::run_remote_density_matrix();
    #[cfg(not(all(feature = "alloc-global", feature = "alloc-xthread")))]
    eprintln!(
        "segment_directory_sweep: remote-dirty-density matrix SKIPPED (build without \
         --features \"alloc-global alloc-xthread\"; re-run with `--features \
         \"alloc-core alloc-global alloc-xthread\"` for it)."
    );
}
