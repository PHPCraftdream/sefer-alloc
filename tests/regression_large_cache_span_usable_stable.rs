//! Regression test — bug #134 (large-cache `usable_size` shrinks on
//! cache-hit reuse → RSS amplification + corrupted byte-budget accounting).
//!
//! `AllocCore::alloc_large`'s cache-HIT path can reuse a cached span for a
//! SMALLER request than the span was originally sized for (the admission
//! rule is `slot.usable_size >= usable && slot.usable_size <=
//! usable * LARGE_CACHE_SIZE_FACTOR`). The hit path overwrites the segment's
//! header with the NEW (smaller) `size`/`align`. The physical OS reservation
//! backing the segment does NOT shrink — it is the same span as before.
//!
//! The bug: on the NEXT deposit (`dealloc`/`reclaim_large_segment`), the old
//! code recomputed `usable_size` from the header's CURRENT `large_size`/
//! `large_align` — which, after a hit-and-shrink, describes the smaller
//! request, not the segment's true physical footprint. This under-reported
//! `usable_size` corrupts `large_cache_used_bytes` (the byte-budget D1
//! counter) and the cached slot's own `usable_size` label, both of which
//! should reflect the ACTUAL bytes of OS memory pinned by the cache.
//!
//! The fix: `SegmentHeader::span_usable` is stamped ONCE at the segment's
//! original OS reservation and carried forward verbatim through every
//! cache-hit reuse (never recomputed from `large_size`/`large_align`).
//! Deposit sites read `span_usable` instead of recomputing.
//!
//! This test allocates a 2-segment span A (usable = 8 MiB under the default
//! rounding), deposits it, then allocates a SMALLER 1-segment request B
//! (usable = 4 MiB under the default rounding) that is admissible against
//! A's cached slot (8 MiB is within `[4 MiB, 4 MiB * 2]` — a real cache HIT,
//! verified via `dbg_large_cache_hits`), and deposits B. It asserts the
//! slot's usable size AND `large_cache_used_bytes` are still A's ORIGINAL
//! physical span — NOT B's smaller logical size.
//!
//! Counterfactual (verified manually — see task #134 report): reverting the
//! `dealloc` Large-branch deposit site to recompute `usable_size` from
//! `stale.large_size`/`stale.large_align` (the pre-fix arithmetic) makes
//! this test's post-redeposit assertions fail (`used_bytes` and the slot's
//! usable size drop to B's logical size instead of staying at A's span).
//!
//! ## R12-3 (`exact-span-large`) interaction
//!
//! The EXPERIMENTAL `exact-span-large` feature (opt-in, not part of
//! `production`) replaces the physical `usable` computation with
//! `round_up(header + size, PAGE)` instead of `round_up(header + size,
//! SEGMENT)` — so A's actual physical span is no longer exactly `2 *
//! SEGMENT` (8 MiB); it is instead just page-rounded above the 6 MiB
//! payload. The bug #134 invariant under test — "the cached slot's
//! `usable_size` is A's TRUE physical span, carried forward verbatim, not
//! recomputed from B's smaller logical size" — is unaffected by WHICH
//! rounding policy computed that physical span, so this test reads A's own
//! `dbg_span_usable_of` value back (instead of hardcoding `2 * SEGMENT`) to
//! stay correct under both configurations while still exercising the exact
//! same bug #134 code path.

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

const MIB: usize = 1024 * 1024;
const SEGMENT: usize = 4 * MIB;

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

#[test]
fn cache_hit_reuse_preserves_physical_span_usable() {
    let mut ac = AllocCore::new().expect("primordial");
    // Unbounded budget: isolate the span-usable-stability effect from the
    // byte-budget eviction effect.
    ac.dbg_set_large_cache_budget(None);

    // A: sized to require more than one segment's worth of payload under
    // the DEFAULT rounding (> 1*SEGMENT, <= 2*SEGMENT) — usable_A is
    // 2*SEGMENT under the default policy, or just page-rounded above 6 MiB
    // under `exact-span-large` (see the file-level doc comment).
    let a_size = SEGMENT + (SEGMENT / 2); // 6 MiB payload
    let la = layout(a_size);
    let pa = ac.alloc(la);
    assert!(!pa.is_null(), "OOM allocating A — cannot run test");
    // Read A's TRUE physical span BEFORE freeing it — this is the value the
    // cache deposit/reuse cycle below must preserve verbatim, whatever its
    // absolute magnitude is under the active rounding policy.
    let usable_a_expected = ac.dbg_span_usable_of(pa);
    #[cfg(not(feature = "exact-span-large"))]
    assert_eq!(
        usable_a_expected,
        2 * SEGMENT,
        "without exact-span-large, A's physical span must be exactly 2*SEGMENT (8 MiB)"
    );
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(pa, la) };

    let slot_sizes_after_a = ac.dbg_large_cache_slot_sizes();
    let usable_a = slot_sizes_after_a
        .iter()
        .find_map(|s| *s)
        .expect("A must be deposited into the large cache");
    assert_eq!(
        usable_a, usable_a_expected,
        "A's cached slot should report A's true physical span"
    );
    assert_eq!(
        ac.dbg_large_cache_used(),
        usable_a_expected,
        "large_cache_used_bytes should equal A's physical span after deposit"
    );

    // B: sized to HALF of A's logical payload (NOT a fixed `SEGMENT / 2`
    // literal — see below for why). Under the default rounding this is
    // exactly `SEGMENT` (4 MiB): admission check is slot.usable_size (8 MiB)
    // >= usable_B (4 MiB) AND <= usable_B * LARGE_CACHE_SIZE_FACTOR (4 MiB *
    // 2 = 8 MiB) -> 8 <= 8, a guaranteed cache HIT against A's slot.
    //
    // Under `exact-span-large`, `usable` is `header + payload` (page-rounded)
    // rather than a whole-SEGMENT multiple, so the header's few-KiB overhead
    // is no longer negligible against the ADMISSION RATIO once the payload
    // is fixed at a constant like `SEGMENT / 2`: `usable_A = header + a_size`
    // and `usable_B = header + a_size/2` no longer satisfy `usable_A <=
    // usable_B * 2` for every `a_size` (the header adds MORE than half of
    // itself extra to the ratio's numerator). Deriving `b_size` from `a_size`
    // (not from `SEGMENT`) keeps `usable_A / usable_B` close to 2 regardless
    // of which rounding policy computed the absolute byte counts, so the
    // cache-HIT precondition this test needs holds under both.
    let b_size = a_size / 2;
    let lb = layout(b_size);

    let hits_before = ac.dbg_large_cache_hits();
    let pb = ac.alloc(lb);
    assert!(!pb.is_null(), "OOM allocating B — cannot run test");
    let hits_after = ac.dbg_large_cache_hits();
    // The hit-COUNT assertion needs the per-hit increment, which is gated
    // behind `alloc-stats` (task W3, default OFF). The hit itself is proven
    // feature-independently by the slot-vacated / usable-size checks below.
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        hits_after - hits_before,
        1,
        "B must be served as a cache HIT against A's slot (verifies we exercise the buggy path)"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = (hits_before, hits_after);
    // A's slot must have been vacated by the hit.
    assert!(
        ac.dbg_large_cache_slot_sizes().iter().all(|s| s.is_none()),
        "the cache should be empty right after B's hit consumes A's only slot"
    );
    assert_eq!(
        ac.dbg_large_cache_used(),
        0,
        "large_cache_used_bytes should be 0 with no slots occupied"
    );

    // Redeposit B (the reused, now-shrunk segment) into the cache.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(pb, lb) };

    let slot_sizes_after_b = ac.dbg_large_cache_slot_sizes();
    let usable_after_redeposit = slot_sizes_after_b
        .iter()
        .find_map(|s| *s)
        .expect("B must be deposited into the large cache");

    // THE ASSERTION THAT CATCHES BUG #134: the redeposited slot must still
    // report the segment's TRUE PHYSICAL span (`usable_a_expected`, from A's
    // original reservation) — not B's smaller logical request size. Pre-fix,
    // the deposit site recomputed usable_size from the header's CURRENT
    // (shrunk) large_size/large_align, which by this point describe B, not
    // the segment's physical footprint.
    assert_eq!(
        usable_after_redeposit, usable_a_expected,
        "BUG #134: redeposited slot's usable_size shrank to B's logical size \
         instead of preserving the segment's true physical span"
    );
    assert_eq!(
        ac.dbg_large_cache_used(),
        usable_a_expected,
        "BUG #134: large_cache_used_bytes under-reports the true physical \
         RSS pinned by the cache after a cache-hit-and-shrink redeposit"
    );
}
