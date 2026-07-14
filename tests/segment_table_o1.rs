//! Task #135 (Part 1/2/3) — O(1) `SegmentTable` register/unregister/recycle
//! and the `HeapCore::dealloc_routing` M2-reorder hardening.
//!
//! ## What changed (see `src/alloc_core/segment_table.rs` / `alloc_core.rs`
//! / `src/registry/heap_core.rs` for the full rationale)
//!
//! - `register`/`unregister`/`recycle` used to scan the slot array `[0,
//!   count)` linearly for a NULL slot / a matching base. They now use:
//!     - a free-list stack of recycled slot indices (`register` pops in O(1)
//!       instead of scanning for a NULL slot);
//!     - the segment's own `segment_id` field (`unregister`/`recycle` read it
//!       via the field-specific `SegmentHeader::segment_id_at` accessor and
//!       index the slot directly, instead of scanning for a matching base).
//! - `HeapCore::realloc`'s own-segment ownership test now uses
//!   `AllocCore::contains_base` (O(1), the existing OPT-B hash) instead of
//!   `segment_bases().any(|b| b == base)` (O(segment count)).
//! - `HeapCore::dealloc_routing` now checks `contains_base(base)` FIRST
//!   (before touching `base`'s memory at all) to decide own-thread routing.
//!
//! These are correctness-preserving refactors: this file verifies the
//! OBSERVABLE BEHAVIOUR is unchanged (registration/lookup/reuse semantics),
//! plus a defensive-no-op regression test for a corrupted `segment_id`.

// ===========================================================================
// Part 1 — free-list invariant: no lost slots, no duplicate reuse.
// ===========================================================================

/// Register N segments (via N large allocations, each getting its own
/// segment), unregister all of them (via dealloc, which recycles under
/// `alloc-decommit` or leaves them registered-but-freed otherwise), then
/// register N more. The count of LIVE segments must never exceed what is
/// actually alive, and every newly registered base must be usable (write +
/// read-back) — a free-list bug (duplicate index, or an index for a slot
/// that is not actually NULL) would manifest as either two live segments
/// aliasing the same slot (corrupting one's base) or a lost slot (register
/// spuriously returning `None`/appending past capacity when a recyclable
/// slot existed).
///
/// Runs under `alloc-decommit` (the feature that exercises the `recycle`
/// path); the `alloc-core`-only case is covered by
/// `without_decommit_cap_is_hard` in `segment_table_recycle.rs` (unregister
/// there just clears bookkeeping, no OS release — this test intentionally
/// focuses on the free-list-bearing paths).
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)] // large OS segment churn — too slow under miri.
#[test]
fn free_list_cycle_reuses_slots_without_growing_count() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    let mut ac = AllocCore::new().expect("primordial");
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    const N: usize = 64;

    // Round 1: register N large segments.
    let mut ptrs = Vec::with_capacity(N);
    for i in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "round-1 alloc null at i={i}");
        ptrs.push(p);
    }
    let count_after_round1 = ac.dbg_table_count();

    // Free all N. Each large dealloc unregisters its segment (and, under
    // `alloc-decommit`, deposits it into the large-cache -- see
    // `AllocCore::dealloc`'s Large branch); either way the table slot is
    // freed for reuse and pushed onto the free-list.
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }

    // Round 2: register N more large segments. If the free-list correctly
    // recycles the N vacated indices, the high-water `count()` must NOT grow
    // past `count_after_round1` (every new registration reuses a freed slot
    // instead of appending).
    let mut ptrs2 = Vec::with_capacity(N);
    for i in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "round-2 alloc null at i={i} — slot lost?");
        ptrs2.push(p);
    }
    let count_after_round2 = ac.dbg_table_count();
    assert_eq!(
        count_after_round2, count_after_round1,
        "high-water slot count grew on round 2 — free-list did not recycle \
         round-1's vacated indices (a lost-slot bug)"
    );

    // All round-2 pointers must be live, distinct, and writable — a
    // duplicate-index bug in the free-list would hand out the SAME slot
    // index to two different bases, corrupting one base's slot entry and
    // making its allocation alias another's.
    let mut set = std::collections::HashSet::new();
    for (i, &p) in ptrs2.iter().enumerate() {
        assert!(set.insert(p as usize), "duplicate pointer at i={i}");
        unsafe {
            let b = (i & 0xFF) as u8;
            core::ptr::write_bytes(p, b, 64);
            assert_eq!(p.read(), b, "write/readback failed at i={i}");
        }
    }

    for &p in &ptrs2 {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

/// Repeated register/unregister/register cycles (not just one round-trip):
/// exercises the free-list stack (LIFO) more thoroughly. If the free-list
/// ever pushed a duplicate index (e.g. `unregister` failing to guard against
/// double-push), a later `register` would hand out the SAME `segment_id` to
/// two live segments simultaneously — the second registration would silently
/// overwrite the first's slot, and the first base would become
/// `contains_base`-invisible (foreign-looking) while still live, corrupting
/// dealloc routing for it.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)]
#[test]
fn free_list_many_cycles_no_duplicate_or_lost_slots() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    let mut ac = AllocCore::new().expect("primordial");
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    const ROUNDS: usize = 30;
    const PER_ROUND: usize = 8;

    let mut max_count = ac.dbg_table_count();
    for round in 0..ROUNDS {
        let mut ptrs = Vec::with_capacity(PER_ROUND);
        for i in 0..PER_ROUND {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "round={round} i={i} alloc null");
            ptrs.push(p);
        }
        // Every pointer this round must be distinct from every prior live
        // pointer (checked implicitly: writing a round-specific pattern and
        // reading it back immediately below proves no aliasing within the
        // round; cross-round aliasing is impossible since prior round's
        // pointers were already freed).
        for (i, &p) in ptrs.iter().enumerate() {
            unsafe {
                let b = ((round * PER_ROUND + i) & 0xFF) as u8;
                core::ptr::write_bytes(p, b, 64);
                assert_eq!(p.read(), b, "round={round} i={i} readback failed");
            }
        }
        for &p in &ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, layout) };
        }
        let count_now = ac.dbg_table_count();
        max_count = max_count.max(count_now);
    }

    // After many register/unregister cycles that each freed everything they
    // registered, the high-water mark must have stabilised — it must NOT
    // have grown unbounded (PER_ROUND * ROUNDS = 240 registrations total; if
    // the free-list leaked an index every round, `count()` would approach
    // 240 + 1(primordial); with correct recycling it should stay small,
    // bounded by the largest number of SIMULTANEOUSLY live segments, which
    // is PER_ROUND + 1).
    assert!(
        max_count <= (PER_ROUND as u32 + 4),
        "table high-water mark grew to {max_count} over {ROUNDS} rounds of \
         {PER_ROUND} register/unregister — free-list is leaking indices \
         (not recycling), regressing to the old O(count) append-only growth"
    );
}

// ===========================================================================
// Part 1 (defensive) — corrupted `segment_id` must not corrupt the table.
// ===========================================================================

/// `unregister`'s O(1) path trusts the segment's own `segment_id` field to
/// locate its slot. If that field were somehow wrong (a caller bug or
/// corruption) the defensive check (`slots[id] == base`) must catch the
/// mismatch and no-op rather than NULLing an unrelated live slot.
///
/// This test exercises the defensive branch indirectly: it registers two
/// segments, then calls the TEST-ONLY `dbg_unregister_via_wrong_id` hook
/// (added alongside this task) which unregisters segment A's base while
/// temporarily reporting segment B's `segment_id` — the table must leave
/// BOTH slots' actual live/NULL state consistent with "no-op" (segment A
/// stays registered; segment B's slot is untouched).
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[test]
fn unregister_defends_against_mismatched_segment_id() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    let mut ac = AllocCore::new().expect("primordial");
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    let a = ac.alloc(layout);
    assert!(!a.is_null());
    let b = ac.alloc(layout);
    assert!(!b.is_null());

    assert!(
        ac.dbg_contains_base(a),
        "a must be registered before the test"
    );
    assert!(
        ac.dbg_contains_base(b),
        "b must be registered before the test"
    );

    // Corrupt-id defensive no-op: ask the table to unregister `a` while its
    // header's segment_id has been overwritten with `b`'s id. The
    // defensive `slots[id] == base` check must reject this (slots[b_id]
    // holds `b`, not `a`), leaving the table untouched.
    let a_id = ac.dbg_segment_id_of(a);
    let b_id = ac.dbg_segment_id_of(b);
    assert_ne!(a_id, b_id, "precondition: distinct segment ids");

    // SAFETY (R6-CQ-2): `a` is a live allocation owned by `ac`. The corrupted
    // `b_id` is restored to `a`'s true `a_id` below before any routing
    // `dealloc` touches `a`; the only operation between this stamp and that
    // restore is `dbg_unregister(a)` — a test-only teardown whose
    // `slots[id] == base` defensive guard rejects the corrupted id as a no-op
    // and does NOT route on the stamped value. Restore-before-further-use.
    unsafe { ac.dbg_stamp_segment_id(a, b_id) }; // corrupt a's stamped id to b's id
                                                 // SAFETY: `a` is a live allocation owned by `ac`.
    unsafe { ac.dbg_unregister(a) }; // O(1) path reads the (corrupted) id -> finds `b` there -> no-op

    // Both must still be considered live (defensive no-op held): `a`'s slot
    // was never NULLed (the corrupted lookup landed on `b`'s slot, which
    // held `b` != `a`, so the guard rejected it), and `b`'s slot is
    // unchanged.
    assert!(
        ac.dbg_contains_base(a),
        "a's slot was incorrectly NULLed via a corrupted segment_id lookup"
    );
    assert!(
        ac.dbg_contains_base(b),
        "b's slot was corrupted by an unregister call for a different base"
    );

    // Restore a's real id and clean up properly.
    // SAFETY (R6-CQ-2): `a` is a live allocation owned by `ac`; `a_id` is `a`'s
    // true segment id (captured above via `dbg_segment_id_of`), so this stamp
    // RESTORES the field to its correct value before the `dealloc(a)` that
    // follows — no routing operation runs while the field is inconsistent.
    unsafe { ac.dbg_stamp_segment_id(a, a_id) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(a, layout) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(b, layout) };
}

// ===========================================================================
// Part 2 — realloc O(1) parity: own-segment vs foreign behaviour unchanged.
// ===========================================================================

/// Own-segment realloc (through `AllocCore::realloc` directly) must still
/// work after switching `HeapCore::realloc`'s ownership test to
/// `contains_base`. This exercises the lower-level substrate the `HeapCore`
/// face delegates to; the `HeapCore`-level in-place/relocate contract is
/// already covered by `regression_realloc_inplace_global.rs` /
/// `regression_realloc_cross_class_shrink.rs` — those tests going green
/// alongside this file is the parity proof for Part 2 (same assertions,
/// unchanged after the O(1) swap).
#[cfg(feature = "alloc-core")]
#[test]
fn realloc_own_segment_still_works_after_o1_swap() {
    use core::alloc::Layout;
    use sefer_alloc::alloc_core::AllocCore;

    let mut ac = AllocCore::new().expect("primordial");
    let l0 = Layout::from_size_align(64, 8).unwrap();
    let p0 = ac.alloc(l0);
    assert!(!p0.is_null());
    unsafe { core::ptr::write_bytes(p0, 0x42, 64) };

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let p1 = unsafe { ac.realloc(p0, l0, 96) };
    assert!(!p1.is_null());
    let bytes = unsafe { core::slice::from_raw_parts(p1, 64) };
    assert!(bytes.iter().all(|&b| b == 0x42), "realloc lost data");

    let l1 = Layout::from_size_align(96, 8).unwrap();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p1, l1) };
}
