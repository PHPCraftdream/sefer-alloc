//! R21-2 (task #351) — direct, non-vacuous proof that the OPT-H Stage-1
//! diagnostic precondition-checking logic
//! (`AllocCore::realloc_inplace_fast_path_known_base`,
//! `src/alloc_core/alloc_core.rs`) actually DISCRIMINATES tail-adjacent from
//! non-tail-adjacent cross-class Small/medium grows — not merely "compiles
//! and doesn't crash".
//!
//! ## What this proves
//!
//! OPT-H (`docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`) is NOT
//! implemented by this task — only its six-precondition CHECK is, behind two
//! `alloc-stats`-gated counters, `OPT_H_ATTEMPTS`/`OPT_H_HITS`
//! (`AllocCore::dbg_opt_h_attempts`/`dbg_opt_h_hits`). The function's
//! observable behavior (what pointer is returned, what memory is touched) is
//! byte-for-byte unchanged by this task — every cross-class grow still falls
//! through to `None`, letting the caller's existing move-leg/promotion path
//! run exactly as before. This file is the counterfactual proof that the
//! new counters nonetheless correctly distinguish the two cases the design's
//! preconditions 3+4 (tail-adjacency, new-class alignment) are supposed to
//! separate:
//!
//! 1. **`opt_h_hits_increments_for_a_genuinely_tail_adjacent_aligned_grow`** —
//!    hand-constructs a 768 KiB→1 MiB cross-class grow at the 4th (LAST)
//!    768 KiB block carved into a fresh segment (offset `4 * 768 KiB == 3
//!    MiB`, verified via `SegmentLayout::segment_base_of` to be the
//!    segment's current bump tail), where the offset is ALSO a legal 1
//!    MiB-class carve position (`3 MiB % 1 MiB == 0`) and the grown size
//!    still fits the segment (`3 MiB + 1 MiB == 4 MiB == SEGMENT`). Asserts
//!    `dbg_opt_h_hits()` increments by EXACTLY 1 across the realloc call.
//! 2. **`opt_h_attempts_but_not_hits_for_a_non_tail_adjacent_grow`** — the
//!    SAME cross-class grow shape (768 KiB → 1 MiB), but applied to the
//!    1st-carved 768 KiB block in a fresh segment (offset 768 KiB) — which
//!    is no longer the bump tail once three more 768 KiB blocks have been
//!    carved after it (the segment holds exactly 4 objects of 768 KiB
//!    before it fills). Asserts `dbg_opt_h_attempts()` increments by 1 (the
//!    grow attempt reaches OPT-H's precondition-1 check) but
//!    `dbg_opt_h_hits()` does NOT increment (precondition 3 correctly
//!    fails).
//!
//! Scenarios 1 and 2 were derived empirically against the real carve-order
//! arithmetic (a throwaway probe enumerated every medium-class transition's
//! last-carved-block offset before this file was written — see this task's
//! own R21-2 report, `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md`, for how
//! 768→1024 KiB was selected as the one transition, among the six-class
//! ladder, whose last-carved-in-a-fresh-segment offset happens to be both
//! tail-adjacent AND 1 MiB-aligned).
//!
//! **R22-2 (task #353) added a third scenario, closing a coverage gap in
//! scenarios 1/2 discovered by this task:**
//!
//! 3. **`opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow`** —
//!    isolates precondition 3 (tail-adjacency) SPECIFICALLY, unlike scenario
//!    2. Scenario 2's target offset (768 KiB, the 1st-carved block) is
//!    independently non-1-MiB-aligned (`786432 % 1048576 != 0` — precondition
//!    4 ALSO fails there), so scenario 2's negative result is explained by
//!    EITHER precondition 3 or precondition 4 failing; it does not, on its
//!    own, prove precondition 3 is doing real discriminating work (a build
//!    with `tail_adjacent` hardcoded to `true` would still fail scenario 2,
//!    via precondition 4 alone — verified directly, see this task's
//!    red/green mutation counterfactual below). Scenario 3 uses a DIFFERENT
//!    class pair, 384 KiB → 1 MiB (`SCENARIO_3_OLD_SIZE`/
//!    `SCENARIO_3_NEW_SIZE`), and targets whichever carved block is BOTH
//!    new-class-aligned (a multiple of `SCENARIO_3_NEW_SIZE`) AND not the
//!    segment's tail — found by DISCOVERING, at test time, how many objects
//!    of `SCENARIO_3_OLD_SIZE` actually fit in a fresh segment under the
//!    CURRENT build's feature combination (`discover_fresh_segment_capacity`),
//!    then searching the real carved offsets for a candidate, rather than
//!    hardcoding an assumed count/index (see the R22-15 note below — the
//!    original hardcoded "9 objects, 8th-carved" assumption broke under
//!    `--all-features`, where fewer objects fit). Asserts
//!    `dbg_opt_h_attempts()` increments by 1 and `dbg_opt_h_hits()` does
//!    NOT increment — and because alignment is independently satisfied
//!    here, a hit could ONLY be explained by the tail-adjacency check being
//!    skipped, making this the test that actually isolates precondition 3.
//!    See `discover_fresh_segment_capacity`'s doc comment for the full
//!    carve-order derivation, with the same `assert_eq!`/`assert_ne!`
//!    intermediate-state sanity checks scenarios 1/2 use.
//!
//! **Non-vacuity, personally re-verified for this task (R22-2):** temporarily
//! hardcoding `let tail_adjacent = true;` immediately after its real
//! computation in `alloc_core.rs` (neutralizing ONLY precondition 3, leaving
//! precondition 4 real) makes scenario 3 flip to a hit (FAIL) while scenario
//! 2 stays green (its offset is still blocked by precondition 4 alone) —
//! confirming scenario 3, not scenario 2, is the one sensitive to
//! precondition 3 specifically. Reverting the hardcode restores all three
//! tests to green with a byte-identical `alloc_core.rs` diff to before the
//! experiment.
//!
//! ## Why `AllocCore` directly (not `HeapCore`/`SeferAlloc`)
//!
//! This test needs precise control over carve order within ONE segment,
//! without registry-level promotion (`try_promote_to_large`,
//! `src/registry/heap_core_free.rs`) intercepting the grow before OPT-H's
//! own check is ever reached. `AllocCore::realloc` has no promotion logic —
//! it always tries the in-place fast paths (OPT-G/OPT-F/OPT-H's diagnostic)
//! then falls through to its own move-leg. This mirrors the precedent in
//! `tests/alloc_zeroed_fresh_large_skip.rs` and `tests/r9_6_class_aware_dirty_judge.rs`
//! for exercising `AllocCore` below the `HeapCore`/`SeferAlloc` registry
//! layer.
//!
//! ## Process-wide counter serialization
//!
//! `OPT_H_ATTEMPTS`/`OPT_H_HITS` are process-wide `AtomicU64`s (mirroring
//! every other diagnostic counter in this crate — `WASTED_DIRTY_DRAINS`,
//! `LARGE_ZERO_PASS_CALLS`). All three tests in this file take a shared
//! `Mutex` before reading/incrementing so concurrent `cargo test` execution
//! of this binary's own tests cannot interleave and corrupt each other's
//! before/after deltas (same pattern as
//! `tests/alloc_zeroed_fresh_large_skip.rs`'s `TEST_LOCK`).
//!
//! Requires `medium-classes` (the six exact classes this scenario is built
//! from) + `alloc-stats` (the counters read 0, and never move, without it) +
//! `alloc-core` (for `AllocCore` itself). Compiles as an empty test binary (0
//! tests, pass by absence) under any other feature configuration.
//!
//! **Verified under `--all-features`** (task #362, R22-11) — the only CI row
//! that satisfies all three required features simultaneously
//! (`test (gated bodies + all-features)`'s closing `cargo test --all-features`
//! step, `.github/workflows/ci.yml`) — scenarios 1/2 passed unchanged at
//! commit `00fb53c`, confirming the hand-derived carve-order geometry
//! (`EXPECTED_TAIL_OFFSET` and the LIFO call-order note above) still holds
//! with `hardened`, `medium-classes-wide`, `numa-aware`,
//! `small-segment-lazy-commit`, `experimental`, and every other feature this
//! crate defines also turned on.
//!
//! **R22-15 (task #366): scenario 3's own `--all-features` gap, found and
//! closed.** Scenario 3 (added in R22-2, task #353, after the R22-11
//! verification above) had NOT itself been re-run under `--all-features`
//! before this task — exactly the gap R22-11 had closed for scenarios 1/2 —
//! and when `npm run check`'s `test (--all-features)` step finally exercised
//! it, it failed: scenario 3's original design hardcoded "a fresh segment
//! fits exactly 9 objects of 384 KiB" and "the target is the 8th-carved
//! block (call #2)", both true under `production,medium-classes,alloc-stats`
//! but NOT under `--all-features`, where the segment's larger metadata
//! footprint (additional per-block bookkeeping pulled in by `hardened` and
//! possibly other features) leaves room for only **8** objects of 384 KiB,
//! not 9 — confirmed by direct measurement (`alloc #8` panicked, i.e. the
//! 9th object, index 8, spilled). Fixed by making the test DISCOVER its own
//! carve-count and target offset at run time instead of hardcoding them —
//! see `discover_fresh_segment_capacity` and the target-search loop in
//! `opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow` — so the
//! scenario now self-adapts to whatever segment capacity the CURRENT
//! feature combination produces, rather than assuming one measured under a
//! single config. Confirmed by direct measurement that under
//! `--all-features` the discovered capacity is 8 (carve order starts at the
//! 2nd 384-KiB granule, offset `786432`, rather than the 1st, because the
//! larger metadata region consumes one more granule up front), the
//! discovered tail is the 9th-carved granule (offset `3538944`), and the
//! discovered target (first non-tail, new-class-aligned candidate) is the
//! 8th-carved granule (offset `3145728`) — the same NUMERIC offsets the
//! original R22-2 hardcoding used, just reached by search instead of
//! assumption, and now additionally correct under the 9-object config where
//! carving starts at the 1st granule instead. All three scenarios,
//! including scenario 3 under this fix, are verified passing under both
//! `--features "alloc-core,medium-classes,alloc-stats"` and
//! `--all-features` (see this task's own commit for the exact command
//! output); the tail_adjacent-hardcoded-to-true mutation counterfactual
//! (see the R22-2 non-vacuity note above) was independently re-run against
//! this new discovery-based geometry and still correctly flips scenario 3
//! red while scenario 2 stays green.

#![cfg(all(
    feature = "alloc-core",
    feature = "medium-classes",
    feature = "alloc-stats"
))]

use std::alloc::Layout;
use std::sync::Mutex;

use sefer_alloc::{AllocCore, SegmentLayout};

/// Serialises this file's tests against each other (see module doc).
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

const KIB: usize = 1024;
const ALIGN: usize = 8;

/// The class this scenario grows FROM (one of the `medium-classes` EXTRAS
/// rungs — `src/alloc_core/size_classes.rs`).
const OLD_SIZE: usize = 768 * KIB;
/// The class this scenario grows TO — the next (and last) rung.
const NEW_SIZE: usize = 1024 * KIB;

/// Scenario 3's OLD class — 384 KiB, a DIFFERENT medium-classes rung than
/// scenarios 1/2's 768 KiB, chosen because (per the exhaustive per-pair
/// carve-order enumeration this task ran — see that scenario's own doc
/// comment) the 768 KiB→1 MiB pair has no candidate offset that is BOTH
/// new-class-aligned AND non-tail within a single fresh segment: of its four
/// carve positions (768 KiB, 1536 KiB, 2304 KiB, 3072 KiB), only the true
/// tail (3072 KiB) is a multiple of 1 MiB. 384 KiB→1 MiB does have such a
/// position under every feature combination checked so far (see
/// `discover_fresh_segment_capacity` and
/// `opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow`, which
/// DISCOVER that position at test time rather than hardcoding it — R22-15),
/// so scenario 3 uses this different pair instead of forcing the 768 KiB
/// pair to do double duty.
const SCENARIO_3_OLD_SIZE: usize = 384 * KIB;
/// Scenario 3's NEW class — same 1 MiB top rung scenarios 1/2 grow into.
const SCENARIO_3_NEW_SIZE: usize = 1024 * KIB;

/// The offset (segment-relative) of the LAST 768 KiB block carved into a
/// fresh segment, empirically verified: a fresh small/primordial segment
/// fits exactly 4 objects of 768 KiB (carve order 768 KiB, 1536 KiB, 2304
/// KiB, 3072 KiB — the metadata region plus 4 × 768 KiB is the most that
/// fits before the 5th would exceed `SEGMENT`), so the last (4th) carved
/// block sits at offset `4 * 768 KiB == 3 MiB == 3145728`. This offset
/// happens to be an exact multiple of the 1 MiB new-class block size
/// (precondition 4) AND leaves exactly enough room for the 1 MiB grow to
/// just fit (`3145728 + 1048576 == 4194304 == SEGMENT`, precondition 5) —
/// the single transition, among the six-class ladder, where the "last
/// carved in a fresh segment" position satisfies both. Asserted explicitly
/// below rather than assumed, so a future size-class table change fails
/// this test loudly (not silently vacuously) instead of silently breaking
/// the scenario.
///
/// **Carve order vs. call order — why `objs[1]`, not `objs[3]`, is the
/// tail.** `alloc_small`'s refill batch (`carve_block_with_refill`,
/// `src/alloc_core/alloc_core_small.rs`) carves the CALLER's block directly
/// (call #0 gets the 1st-carved block, offset `768 KiB`), then carves up to
/// 31 MORE blocks and pushes each onto the class's free list — a LIFO
/// stack. So call #1 pops the free list's head, which is the
/// MOST-RECENTLY-carved (i.e. LAST, 4th) extra block — offset `3145728`.
/// Calls #2/#3 pop the 3rd/2nd carved blocks in descending offset order. The
/// segment fills after exactly 4 objects of 768 KiB (carve order 1st..4th),
/// so `alloc_into_first_segment(&mut a, 4)`'s **call-order** vector has the
/// TAIL block (4th carved) at **index 1**, not index 3.
const EXPECTED_TAIL_OFFSET: usize = 4 * OLD_SIZE;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// Allocate objects of `OLD_SIZE` from a fresh `AllocCore` until either
/// `count` objects have been carved OR a NEW segment is reserved (the
/// segment filled) — returning `(ptr, offset)` pairs for objects carved into
/// the FIRST segment only. Panics if fewer than `count` objects fit (the
/// scenario's own precondition).
fn alloc_into_first_segment(a: &mut AllocCore, count: usize) -> Vec<(*mut u8, usize)> {
    let (out, spilled) = alloc_sized_into_first_segment(a, OLD_SIZE, count);
    assert!(
        !spilled,
        "alloc #{} landed in a DIFFERENT segment than alloc #0 — the \
         scenario's assumption that {count} objects of {OLD_SIZE} bytes fit \
         in one fresh segment is broken; adjust `count` or investigate a \
         size-class/metadata-layout change",
        out.len()
    );
    out
}

/// Generalized form of [`alloc_into_first_segment`], parameterized on the
/// object size — needed by scenario 3, which carves `SCENARIO_3_OLD_SIZE`
/// (384 KiB) objects rather than scenarios 1/2's `OLD_SIZE` (768 KiB).
///
/// Allocates up to `count` objects of `size` bytes, stopping EARLY (without
/// carving the one that would spill) the moment an allocation's segment base
/// differs from the first one's — i.e. this never asserts a fixed carve
/// count holds; it discovers how many actually fit in the fresh segment
/// under the CURRENT build's exact feature combination (segment/metadata
/// footprint varies by feature set, e.g. `hardened`'s per-`MIN_BLOCK`-granule
/// generation table — see the module doc's R22-2/R22-15 note). Returns
/// `(carved, spilled)`: `carved` holds only the objects that landed in the
/// first segment (in call order), and `spilled` is `true` iff fewer than
/// `count` objects fit (i.e. the `count`-th call would have, or did, spill
/// into a new segment) — callers that need an exact count to hold treat
/// `spilled` as a hard failure (see [`alloc_into_first_segment`]); callers
/// that only need "as many as actually fit" (scenario 3) use `carved.len()`
/// directly instead of asserting against a hardcoded expectation.
fn alloc_sized_into_first_segment(
    a: &mut AllocCore,
    size: usize,
    count: usize,
) -> (Vec<(*mut u8, usize)>, bool) {
    let l = layout(size);
    let mut out = Vec::with_capacity(count);
    let mut first_base: Option<usize> = None;
    for _ in 0..count {
        let p = a.alloc(l);
        assert!(!p.is_null(), "alloc of {size} bytes failed");
        let base = SegmentLayout::segment_base_of(p as usize);
        match first_base {
            None => first_base = Some(base),
            Some(b) if base != b => {
                // This object spilled into a new segment — it was not
                // requested to be freed by this helper (the scenario's own
                // `AllocCore` is dropped whole at the end of the test), so
                // simply stop collecting and report the spill.
                return (out, true);
            }
            Some(_) => {}
        }
        let off = p as usize - base;
        out.push((p, off));
    }
    (out, false)
}

/// Discover how many objects of `size` bytes actually fit in a FRESH
/// segment, under whatever feature combination this binary was compiled
/// with — by carving objects one at a time into a brand-new `AllocCore`
/// until one spills into a second segment, then reporting how many landed
/// in the first. This is the "derive geometry from real observed behavior,
/// don't hardcode an assumption" pattern `EXPECTED_TAIL_OFFSET`'s doc
/// comment already establishes, applied to scenario 3's carve COUNT itself
/// (not just its offset arithmetic) — needed because `hardened` and other
/// `--all-features` combinations shrink the segment's usable payload space
/// enough that fewer objects fit than under the config this scenario was
/// originally authored against.
///
/// `upper_bound` must be large enough that the (`upper_bound`+1)-th object
/// is guaranteed to spill under every feature combination this crate
/// supports — it exists only to keep the probe finite, not as an assumed
/// answer.
///
/// **Why this replaced a hardcoded count (R22-15).** Scenario 3's original
/// design (R22-2) hardcoded "a fresh segment fits exactly 9 objects of
/// `SCENARIO_3_OLD_SIZE` (384 KiB)" — true under
/// `production,medium-classes,alloc-stats`, but NOT under `--all-features`,
/// where the segment's usable payload shrinks (additional per-block
/// metadata — `hardened`'s per-`MIN_BLOCK`-granule generation table, and
/// possibly `medium-classes-wide`/`numa-aware`/`small-segment-lazy-commit`'s
/// own overhead — leaves room for only 8), so the hardcoded "9" panicked
/// (`alloc #8 landed in a DIFFERENT segment`). Rather than feature-gate a
/// second hardcoded constant (equally brittle to the NEXT feature
/// combination), the test now derives "how many objects of
/// `SCENARIO_3_OLD_SIZE` fit in a fresh segment" from the real allocator
/// behavior of the CURRENT build via this function, then searches the
/// actual carved offsets for one that is new-class-aligned AND not the
/// tail (see `opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow`)
/// — extending `EXPECTED_TAIL_OFFSET`'s "derive, don't hardcode" house
/// style to the carve COUNT itself, not just to which index holds the tail.
///
/// The reasoning that motivates 384 KiB→1 MiB over the 768 KiB→1 MiB pair
/// scenarios 1/2 use is unchanged from the original R22-2 design: of the
/// 768 KiB pair's four carve positions (768 KiB, 1536 KiB, 2304 KiB, 3072
/// KiB) in one fresh segment, only the true tail (3072 KiB) is a multiple
/// of 1 MiB — so that pair has no candidate that is BOTH new-class-aligned
/// AND non-tail, and can't isolate precondition 3 on its own. The 384 KiB
/// pair carves more, smaller blocks per segment, giving more 1-MiB-aligned
/// candidate offsets to search among — the search in
/// `opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow` finds
/// whichever ones the ACTUAL carve count under this build produces, rather
/// than assuming a fixed index.
///
/// **Carve order vs. call order.** Exactly as `EXPECTED_TAIL_OFFSET`'s doc
/// derives for the 768 KiB case: call #0 gets the 1st-carved block directly,
/// then the LIFO refill free list means call #1 pops the LAST-carved (true
/// tail) block, call #2 pops the 2nd-to-last-carved block, and so on in
/// descending carve order. `alloc_sized_into_first_segment`'s returned
/// vector is in CALL order, so `carved[0]` is the 1st-carved block and
/// `carved[1..]` are the remaining blocks in descending carve-order (i.e.
/// `carved[1]` is the true tail).
fn discover_fresh_segment_capacity(a: &mut AllocCore, size: usize, upper_bound: usize) -> usize {
    let (carved, spilled) = alloc_sized_into_first_segment(a, size, upper_bound);
    assert!(
        spilled,
        "expected at least one object of {size} bytes, among {upper_bound} \
         carved into a fresh segment, to spill into a second segment — \
         `upper_bound` is too small to observe the fresh-segment capacity; \
         raise it"
    );
    carved.len()
}

/// Scenario 1 — the load-bearing positive case: a genuinely tail-adjacent,
/// new-class-aligned, in-capacity cross-class grow increments
/// `dbg_opt_h_hits()` by exactly 1.
#[test]
fn opt_h_hits_increments_for_a_genuinely_tail_adjacent_aligned_grow() {
    let _guard = serial();

    let mut a = AllocCore::new().expect("AllocCore::new");
    // Carve exactly 4 objects of 768 KiB into the first segment. The
    // refill batch's LIFO free-list means CALL #1 (not call #3) receives
    // the 4th/LAST-carved block — see `EXPECTED_TAIL_OFFSET`'s doc comment
    // for the full carve-order-vs-call-order derivation.
    let objs = alloc_into_first_segment(&mut a, 4);
    let (tail_ptr, tail_off) = objs[1];
    assert_eq!(
        tail_off, EXPECTED_TAIL_OFFSET,
        "call #1's object offset does not match the hand-verified expected \
         tail offset — the scenario's geometry assumption is stale \
         (metadata layout, refill-batch size, or size-class table changed)"
    );

    let attempts_before = AllocCore::dbg_opt_h_attempts();
    let hits_before = AllocCore::dbg_opt_h_hits();

    // SAFETY: `tail_ptr` is a live allocation made with `layout(OLD_SIZE)`,
    // freed exactly once (never, in this test — the `AllocCore` is dropped
    // at the end); `NEW_SIZE > 0`.
    let grown = unsafe { a.realloc(tail_ptr, layout(OLD_SIZE), NEW_SIZE) };
    assert!(!grown.is_null(), "grow realloc failed");

    let attempts_after = AllocCore::dbg_opt_h_attempts();
    let hits_after = AllocCore::dbg_opt_h_hits();

    assert_eq!(
        attempts_after - attempts_before,
        1,
        "expected exactly one OPT-H precondition-1 attempt for this \
         cross-class grow"
    );
    assert_eq!(
        hits_after - hits_before,
        1,
        "expected the tail-adjacent, aligned, in-capacity grow to satisfy \
         ALL SIX OPT-H preconditions — dbg_opt_h_hits() did not increment; \
         the precondition-checking logic is not correctly recognizing a \
         genuinely eligible grow"
    );

    // Sanity: the function's OBSERVABLE behavior is unchanged by this task
    // — OPT-H is not implemented, so the grow still relocates via the
    // ordinary move-leg (a fresh pointer), never aliases `tail_ptr` in
    // place. This is the direct "zero behavior change" counterfactual: if a
    // future edit accidentally wired the counter-increment branch to ALSO
    // return `Some(ptr)` (i.e. accidentally implemented OPT-H's action
    // instead of only observing it), this assertion would catch the
    // resulting stale-pointer aliasing immediately.
    assert_ne!(
        grown, tail_ptr,
        "OPT-H's Stage-1 diagnostic must NOT change realloc's observable \
         behavior — the grow must still relocate via the existing move-leg, \
         never alias `tail_ptr` in place"
    );

    // The grown block must be genuinely usable at its new size (proves the
    // move-leg fallback still works correctly alongside the new diagnostic
    // code sitting next to it).
    // SAFETY: `grown` is valid for NEW_SIZE bytes per `realloc`'s contract.
    unsafe {
        std::ptr::write_bytes(grown, 0xCD, NEW_SIZE);
        assert_eq!(grown.read(), 0xCD);
        assert_eq!(grown.add(NEW_SIZE - 1).read(), 0xCD);
    }

    // SAFETY: `grown` was returned by the immediately preceding `realloc`
    // call with `NEW_SIZE`, is live, and is freed exactly once here.
    unsafe { a.dealloc(grown, layout(NEW_SIZE)) };
}

/// Scenario 2 — a negative case: the SAME cross-class grow shape applied to
/// a block that is NOT the segment's tail (three more 768 KiB objects were
/// carved after it) increments `dbg_opt_h_attempts()` but must NOT increment
/// `dbg_opt_h_hits()`.
///
/// **What this scenario does NOT isolate.** `objs[0]`'s offset (768 KiB) is
/// independently non-1-MiB-aligned (`786432 % 1048576 != 0`, precondition 4
/// also fails here), so this scenario cannot by itself prove precondition 3
/// (tail-adjacency) specifically is doing the discriminating work — a build
/// with `tail_adjacent` hardcoded `true` would STILL fail this scenario via
/// precondition 4 alone, and the assertion below would not catch that
/// mutation. Scenario 3 (below) is the one built to isolate precondition 3:
/// its target offset independently satisfies precondition 4, so its
/// negative result depends on precondition 3 alone.
#[test]
fn opt_h_attempts_but_not_hits_for_a_non_tail_adjacent_grow() {
    let _guard = serial();

    let mut a = AllocCore::new().expect("AllocCore::new");
    // Carve 4 objects of 768 KiB into the first segment; keep the FIRST one
    // (offset 0-relative-to-metadata, i.e. NOT the tail once the other 3
    // have been carved after it).
    let objs = alloc_into_first_segment(&mut a, 4);
    let (first_ptr, first_off) = objs[0];
    assert_ne!(
        first_off, EXPECTED_TAIL_OFFSET,
        "the 1st carved object's offset unexpectedly matches the tail \
         offset — the scenario needs at least 2 distinct carve slots"
    );

    let attempts_before = AllocCore::dbg_opt_h_attempts();
    let hits_before = AllocCore::dbg_opt_h_hits();

    // SAFETY: `first_ptr` is a live allocation made with `layout(OLD_SIZE)`,
    // freed exactly once (never, in this test); `NEW_SIZE > 0`.
    let grown = unsafe { a.realloc(first_ptr, layout(OLD_SIZE), NEW_SIZE) };
    assert!(!grown.is_null(), "grow realloc failed");

    let attempts_after = AllocCore::dbg_opt_h_attempts();
    let hits_after = AllocCore::dbg_opt_h_hits();

    assert_eq!(
        attempts_after - attempts_before,
        1,
        "expected exactly one OPT-H precondition-1 attempt for this \
         cross-class grow (same shape as the tail-adjacent scenario, just a \
         different block)"
    );
    assert_eq!(
        hits_after - hits_before,
        0,
        "expected the NON-tail-adjacent, NON-aligned grow to fail (at \
         least) precondition 4 — dbg_opt_h_hits() must NOT increment here. \
         An increment would mean the precondition logic is not actually \
         checking new-class alignment (a vacuously-true check), which would \
         make a real OPT-H implementation built on this logic corrupt a \
         live neighboring block. NOTE: because this offset independently \
         fails precondition 4 too, this assertion alone does not prove \
         precondition 3 (tail-adjacency) is doing real work — see scenario \
         3, `opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow`, \
         for the isolated proof of precondition 3 specifically"
    );

    // SAFETY: `grown` is valid for NEW_SIZE bytes.
    unsafe {
        std::ptr::write_bytes(grown, 0xEF, NEW_SIZE);
        assert_eq!(grown.read(), 0xEF);
    }
    // SAFETY: `grown` was returned by the immediately preceding `realloc`
    // call with `NEW_SIZE`, is live, and is freed exactly once here.
    unsafe { a.dealloc(grown, layout(NEW_SIZE)) };
}

/// Scenario 3 — the isolated negative case: a grow whose offset
/// INDEPENDENTLY satisfies precondition 4 (new-class alignment) but fails
/// precondition 3 (tail-adjacency), because three more objects were carved
/// after it in the same segment. Unlike scenario 2, this offset's alignment
/// holds regardless of tail-adjacency — so a hit here can ONLY be explained
/// by precondition 3 being (incorrectly) skipped, making this the test that
/// actually proves the tail-adjacency check is load-bearing (see the
/// red/green mutation counterfactual in this task's own verification: with
/// `tail_adjacent` hardcoded to `true`, THIS test flips to a hit while
/// scenario 2 does not move, because scenario 2's offset is blocked by
/// precondition 4 independently of precondition 3).
///
/// Uses a DIFFERENT class pair (384 KiB → 1 MiB, `SCENARIO_3_OLD_SIZE`/
/// `SCENARIO_3_NEW_SIZE`) than scenarios 1/2 (768 KiB → 1 MiB), because the
/// 768 KiB pair has no carve position that is both new-class-aligned and
/// non-tail within one fresh segment. The exact carve COUNT and target
/// offset are DISCOVERED at test time (not hardcoded — see
/// `discover_fresh_segment_capacity`'s doc comment for why R22-15 replaced
/// the original hardcoded "9 objects, index 2" assumption, which broke
/// under `--all-features`).
#[test]
fn opt_h_attempts_but_not_hits_for_an_aligned_but_non_tail_grow() {
    let _guard = serial();

    // Discover how many objects of SCENARIO_3_OLD_SIZE actually fit in a
    // fresh segment under THIS build's exact feature combination, using a
    // throwaway `AllocCore` (its segment is never touched again). 64 is a
    // generous upper bound: SEGMENT (4 MiB) / SCENARIO_3_OLD_SIZE (384 KiB)
    // is under 11 even with zero metadata overhead, so 64 carves are
    // guaranteed to spill into a second segment well before the bound is
    // reached, under any plausible metadata footprint.
    let capacity = {
        let mut probe = AllocCore::new().expect("AllocCore::new (capacity probe)");
        discover_fresh_segment_capacity(&mut probe, SCENARIO_3_OLD_SIZE, 64)
    };
    assert!(
        capacity >= 3,
        "need at least 3 objects of {SCENARIO_3_OLD_SIZE} bytes to fit in a \
         fresh segment for this scenario to have a non-tail candidate at \
         all (got {capacity}) — the size-class table or segment size \
         changed enough that this scenario's premise no longer holds"
    );

    // Carve `capacity` objects into a FRESH AllocCore (the probe above
    // already spilled its own segment, so it can't be reused) — this is the
    // exact number that fits, verified moments ago against this build's own
    // real behavior, not assumed.
    let mut a = AllocCore::new().expect("AllocCore::new");
    let (objs, spilled) = alloc_sized_into_first_segment(&mut a, SCENARIO_3_OLD_SIZE, capacity);
    assert!(
        !spilled && objs.len() == capacity,
        "discovered capacity ({capacity}) did not reproduce identically on \
         a second fresh AllocCore — fresh-segment carving is expected to be \
         deterministic; got {} objects before a spill",
        objs.len()
    );

    // `objs` is in CALL order: `objs[0]` is the 1st-carved block (received
    // directly), `objs[1]` is the LAST-carved (true tail) block (the LIFO
    // refill free list's head — see `EXPECTED_TAIL_OFFSET`'s doc comment for
    // the full carve-order-vs-call-order derivation, which applies
    // identically here), and `objs[2..]` are the remaining blocks in
    // descending carve order. The true tail is therefore `objs[1]`; search
    // every OTHER carved offset for one that independently satisfies
    // precondition 4 (new-class alignment) — that is this scenario's
    // target, isolating precondition 3 exactly as the original design
    // intended, just without assuming its position in advance.
    let (_, tail_off) = objs[1];
    let target = objs
        .iter()
        .copied()
        .enumerate()
        .find(|(i, (_, off))| *i != 1 && *off % SCENARIO_3_NEW_SIZE == 0)
        .map(|(_, pair)| pair);
    let (target_ptr, target_off) = target.unwrap_or_else(|| {
        panic!(
            "no carved offset among {} objects (fresh-segment capacity for \
             {SCENARIO_3_OLD_SIZE} bytes under this build) is BOTH \
             new-class-aligned (multiple of {SCENARIO_3_NEW_SIZE}) AND \
             distinct from the tail offset ({tail_off}) — this build's \
             carve-order geometry no longer admits a candidate that \
             isolates precondition 3 this way; the scenario needs a \
             different class pair or a wider capacity search",
            objs.len()
        )
    });
    assert_ne!(
        target_off, tail_off,
        "the target offset unexpectedly matches the tail offset — the \
         scenario needs the target to be a DISTINCT, earlier-carved slot \
         than the tail"
    );
    assert_eq!(
        target_off % SCENARIO_3_NEW_SIZE,
        0,
        "the target offset must be an exact multiple of the new-class \
         block size (precondition 4) — this is the property that makes \
         this scenario ISOLATE precondition 3, unlike scenario 2's offset \
         (which independently fails precondition 4 too)"
    );

    let attempts_before = AllocCore::dbg_opt_h_attempts();
    let hits_before = AllocCore::dbg_opt_h_hits();

    // SAFETY: `target_ptr` is a live allocation made with
    // `layout(SCENARIO_3_OLD_SIZE)`, freed exactly once (never, in this
    // test); `SCENARIO_3_NEW_SIZE > 0`.
    let grown = unsafe { a.realloc(target_ptr, layout(SCENARIO_3_OLD_SIZE), SCENARIO_3_NEW_SIZE) };
    assert!(!grown.is_null(), "grow realloc failed");

    let attempts_after = AllocCore::dbg_opt_h_attempts();
    let hits_after = AllocCore::dbg_opt_h_hits();

    assert_eq!(
        attempts_after - attempts_before,
        1,
        "expected exactly one OPT-H precondition-1 attempt for this \
         cross-class grow"
    );
    assert_eq!(
        hits_after - hits_before,
        0,
        "expected the ALIGNED-but-non-tail-adjacent grow to fail \
         precondition 3 specifically — dbg_opt_h_hits() must NOT increment \
         here. Because this offset independently satisfies precondition 4 \
         (new-class alignment), an increment here could ONLY mean the \
         tail-adjacency check itself is not doing real discriminating work \
         (unlike scenario 2, whose negative result is also explained by \
         precondition 4 failing) — this is the isolated proof that \
         precondition 3 alone is load-bearing"
    );

    // SAFETY: `grown` is valid for SCENARIO_3_NEW_SIZE bytes.
    unsafe {
        std::ptr::write_bytes(grown, 0xAB, SCENARIO_3_NEW_SIZE);
        assert_eq!(grown.read(), 0xAB);
    }
    // SAFETY: `grown` was returned by the immediately preceding `realloc`
    // call with `SCENARIO_3_NEW_SIZE`, is live, and is freed exactly once
    // here.
    unsafe { a.dealloc(grown, layout(SCENARIO_3_NEW_SIZE)) };
}
