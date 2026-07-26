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
//! Both scenarios were derived empirically against the real carve-order
//! arithmetic (a throwaway probe enumerated every medium-class transition's
//! last-carved-block offset before this file was written — see this task's
//! own R21-2 report, `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md`, for how
//! 768→1024 KiB was selected as the one transition, among the six-class
//! ladder, whose last-carved-in-a-fresh-segment offset happens to be both
//! tail-adjacent AND 1 MiB-aligned).
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
//! `LARGE_ZERO_PASS_CALLS`). Both tests in this file take a shared `Mutex`
//! before reading/incrementing so concurrent `cargo test` execution of this
//! binary's own tests cannot interleave and corrupt each other's before/after
//! deltas (same pattern as `tests/alloc_zeroed_fresh_large_skip.rs`'s
//! `TEST_LOCK`).
//!
//! Requires `medium-classes` (the six exact classes this scenario is built
//! from) + `alloc-stats` (the counters read 0, and never move, without it) +
//! `alloc-core` (for `AllocCore` itself). Compiles as an empty test binary (0
//! tests, pass by absence) under any other feature configuration.

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
    let l = layout(OLD_SIZE);
    let mut out = Vec::with_capacity(count);
    let mut first_base: Option<usize> = None;
    for i in 0..count {
        let p = a.alloc(l);
        assert!(!p.is_null(), "alloc #{i} of {OLD_SIZE} bytes failed");
        let base = SegmentLayout::segment_base_of(p as usize);
        match first_base {
            None => first_base = Some(base),
            Some(b) => assert_eq!(
                base, b,
                "alloc #{i} landed in a DIFFERENT segment than alloc #0 — the \
                 scenario's assumption that {count} objects of {OLD_SIZE} \
                 bytes fit in one fresh segment is broken; adjust `count` or \
                 investigate a size-class/metadata-layout change"
            ),
        }
        let off = p as usize - base;
        out.push((p, off));
    }
    out
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

/// Scenario 2 — the load-bearing negative case: the SAME cross-class grow
/// shape applied to a block that is NOT the segment's tail (three more
/// 768 KiB objects were carved after it) increments `dbg_opt_h_attempts()`
/// but must NOT increment `dbg_opt_h_hits()`.
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
        "expected the NON-tail-adjacent grow to fail precondition 3 — \
         dbg_opt_h_hits() must NOT increment here. An increment would mean \
         the precondition logic is not actually checking tail-adjacency (a \
         vacuously-true check), which would make a real OPT-H \
         implementation built on this logic corrupt a live neighboring \
         block"
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
