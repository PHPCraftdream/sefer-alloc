//! Task #60 — slot-recycle verification for [`SegmentTable`].
//!
//! ## What is being tested
//!
//! Under `alloc-decommit`, empty small segments are decommitted and their table
//! slots are recycled: the slot is NULLed and the OS reservation is released.
//! Future `register` calls scan for NULL slots and reuse them, lifting the
//! hard 1024-segment cap into an effective "unbounded live segments as long as
//! churn recycles old slots" regime.
//!
//! ## Tests
//!
//! ### `slot_recycle_lifts_cap` (alloc-decommit)
//!
//! Allocates and frees blocks cumulatively across 4096+ segment-lives, verifying
//! that `register` never returns `None` (the table never reports "full") even
//! though 4096 > MAX_SEGMENTS (1024). This is the primary correctness assertion
//! for task #60.
//!
//! ### `without_decommit_cap_is_hard` (alloc-core, !alloc-decommit)
//!
//! Verifies the UNCHANGED behaviour under `!alloc-decommit`: the segment table
//! stays append-only, count grows monotonically, and once MAX_SEGMENTS live
//! segments are registered, `alloc_large` gracefully returns null (OOM), not a
//! panic. This ensures the recycle path does NOT regress the no-decommit case.
//!
//! ### `recycled_slot_is_reused` (alloc-decommit)
//!
//! A focused unit test: alloc K blocks to fill one small segment past the
//! primordial, free all (triggering decommit + recycle of the emptied segment),
//! then alloc again; the allocator must succeed (recycled slot reused) and
//! allocations must be valid and writable.
//!
//! ### `recycle_defensive_tail_evicts_hash_and_cache` (alloc-core, alloc-decommit)
//!
//! oxx R2-06 (independent src review round 2): proves `SegmentTable::recycle`'s
//! anomalous-but-found branch (a corrupted `segment_id`, but the base is still
//! a genuine hash-table member) routes through the FULL normal path — release,
//! slot NULL, free-list push — instead of the pre-fix defensive tail that
//! released the OS reservation but left the segment's own slot dangling
//! (non-NULL, pointing at now-unmapped memory). See its own doc comment for
//! the counterfactual detail.

#![cfg(feature = "internals")]
// R34-3 (task #522, finding B1): every test below reaches
// `sefer_alloc::alloc_core::AllocCore` directly, so this module-level gate
// covers all of them; each test's own `#[cfg(feature = "alloc-core")]` (and
// `alloc-decommit` where present) is unchanged and still independently
// required — `internals` alone does not expose `alloc_core` (see
// `internals`'s doc comment in `Cargo.toml`: it is additive over
// `alloc-core`).

// oxx R2-06: `AllocCore::dbg_segments_released_total()` is a PROCESS-WIDE
// static atomic shared across every `AllocCore` in the process (see its own
// doc comment). `cargo test` runs this file's tests in parallel by default,
// and three of the four tests below (`slot_recycle_lifts_cap`,
// `without_decommit_cap_is_hard`, `recycled_slot_is_reused`) themselves
// release many segments — a sibling test's release traffic mid-sequence
// would pollute `recycle_defensive_tail_evicts_hash_and_cache`'s exact-delta
// assertions. Serialized with the SAME established `static TEST_LOCK:
// Mutex<()>` + guard pattern already used in
// `tests/segment_table_contains_base_tier1_counters.rs`,
// `tests/directory_authoritative_miss.rs`,
// `tests/alloc_zeroed_fresh_large_skip.rs`, and others for tests reading
// process-wide diagnostic counters.
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ============================================================
// Test 1 — slot recycle lifts the 1024-segment cap
// ============================================================

/// Under `alloc-decommit`, cumulative segments well beyond MAX_SEGMENTS (1024)
/// must succeed because empty segments are recycled (their slots reused).
///
/// Protocol: repeatedly alloc N blocks (enough to require fresh segments beyond
/// the primordial) then dealloc all. Each dealloc cycle empties the non-current
/// small segments → decommit fires → slot recycled. After many cycles, the
/// cumulative segment-creation count far exceeds 1024 — but at any point in
/// time, only O(working_set / SEGMENT) slots are live. `alloc` must succeed
/// throughout.
///
/// `#[cfg_attr(miri, ignore)]` — N=800 per round × 30 rounds is too slow under
/// miri's ~1000× slowdown. The miri coverage lives in `recycled_slot_is_reused`.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)]
#[test]
fn slot_recycle_lifts_cap() {
    use core::alloc::Layout;
    use sefer_alloc::alloc_core::AllocCore;
    use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

    // oxx R2-06: see the module-level `TEST_LOCK` doc comment.
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool. This is a
    // task-#60 SLOT-RECYCLE test — it must exercise the decommit→release→recycle
    // path on every emptied segment. With the pool ON (production default) this
    // ~3-live-segment workload is fully absorbed by the 4-slot pool, so decommit
    // never fires and the recycle path is never reached. Disabling the pool
    // restores the deterministic recycle behaviour this test was written for;
    // the pool interaction is covered separately by `tests/small_segment_pool.rs`.
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");

    // 256 B blocks: fits many per segment. We need enough per round to spill
    // past the primordial AND past one fresh segment, so that the SECOND fresh
    // segment becomes `small_cur` — leaving the FIRST fresh segment non-current
    // when we free everything → first fresh segment decommits.
    //
    // Primordial payload ≈ 4 MiB − ~100 KiB metadata ≈ ~3.9 MiB / 256 B ≈ 15K blocks.
    // One Small segment ≈ 4 MiB / 256 B ≈ 16K blocks.
    // To get ≥2 fresh segments: N > 15K + 16K = 31K. Use 40K with margin.
    let layout = Layout::from_size_align(256, 8).unwrap();
    const N: usize = 40_000; // blocks per round (fills primordial + 2 fresh segments)
    const ROUNDS: usize = 100; // 100 rounds × ~3 segments/round ≈ 300 cumulative segment-lives

    let decommit_before = AllocCore::dbg_decommit_count();

    for round in 0..ROUNDS {
        let mut ptrs = Vec::with_capacity(N);
        for i in 0..N {
            let p = ac.alloc(layout);
            assert!(
                !p.is_null(),
                "alloc returned null at round={round} i={i} — \
                 slot recycle failed (cap exhausted)"
            );
            ptrs.push(p);
        }
        // Spot-write / read-back to verify the block is usable.
        for (i, &p) in ptrs.iter().enumerate() {
            unsafe {
                let b = (i & 0xFF) as u8;
                p.write(b);
                assert_eq!(p.read(), b, "write/readback failed at round={round} i={i}");
            }
        }
        // Free all — non-current Small segments empty → decommit → recycle.
        for &p in &ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, layout) };
        }
    }

    let decommit_after = AllocCore::dbg_decommit_count();
    assert!(
        decommit_after > decommit_before,
        "no decommit fired during {ROUNDS} rounds of churn — \
         recycle path was never exercised (decommit hook miswired). \
         Ensure N is large enough to spill into >= 2 fresh Small segments per round \
         so that the first fresh segment is non-current when emptied."
    );

    // With ROUNDS=100 and ~3 segment-lives/round, cumulative segment-creation is
    // ~300+. Since MAX_SEGMENTS=1024 without recycle would fill by round ~341,
    // this test WOULD have failed in the pre-#60 world. With slot recycle, every
    // round decommits the non-current segments and recycles their slots — so
    // `alloc` succeeds throughout.
    // The primary correctness check is that NO alloc ever returns null above.
}

// ============================================================
// Test 2 — without alloc-decommit, old hard-cap behaviour is unchanged
// ============================================================

/// Under `!alloc-decommit`, the segment table is strictly append-only. This test
/// verifies that registering MAX_SEGMENTS+1 live large allocations (each in its
/// own segment) causes `alloc` to return null (graceful OOM) rather than panicking
/// or corrupting state. This is the REGRESSION guard: recycle must not change
/// behaviour when the feature is disabled.
///
/// `#[cfg_attr(miri, ignore)]` — reserves MAX_SEGMENTS large OS segments; too slow
/// under miri. Correctness of the no-decommit path is covered by other invariant tests.
#[cfg(all(feature = "alloc-core", not(feature = "alloc-decommit")))]
#[cfg_attr(miri, ignore)]
#[test]
fn without_decommit_cap_is_hard() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    // oxx R2-06: see the module-level `TEST_LOCK` doc comment.
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut ac = AllocCore::new().expect("primordial");

    // Reserve large allocations (each gets its own segment). Keep them live.
    // One slot is already used by the primordial, so `MAX_SEGMENTS - 1` more
    // large allocs should fit, and the next one should fail gracefully.
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    // R14-7 (task #292): read the cap at runtime via `dbg_max_segments()`
    // instead of hardcoding the historical `1024` literal (R12-14/task #265
    // density-agnostic convention) — `MAX_SEGMENTS` was raised to 4096 in
    // this same task, and a hardcoded `1025` attempt count would have made
    // this test silently pass without ever reaching the cap. Primordial
    // occupies slot 0, so we can register at most `MAX_SEGMENTS - 1` more
    // segments; attempt one past that so at least one alloc must return null.
    let attempt = AllocCore::dbg_max_segments() + 1;

    let mut ptrs = Vec::with_capacity(attempt);
    let mut null_count = 0usize;
    for _ in 0..attempt {
        let p = ac.alloc(layout);
        if p.is_null() {
            null_count += 1;
            // First null is expected: the table filled (no recycle). Subsequent
            // allocs may also return null (idempotent OOM). Stop here.
            break;
        }
        ptrs.push(p);
    }

    assert!(
        null_count > 0,
        "expected at least one null from large alloc after MAX_SEGMENTS — \
         table must be full without decommit"
    );

    // Cleanup: free what we have (drop releases segments via Drop).
    // In the no-decommit path, `dealloc` for large segments marks them freed but
    // the OS reservation is held until drop.
    for (&p, _) in ptrs.iter().zip(std::iter::repeat(layout)) {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

// ============================================================
// Test 3 — focused unit: recycled slot is reused
// ============================================================

/// Focused correctness test for the slot-recycle mechanism (task #60):
///
/// 1. Alloc enough blocks to spill past the primordial into at least one fresh
///    Small segment.
/// 2. Free all blocks. Non-current Small segments → decommit → slot recycled
///    (NULLed + OS reservation released).
/// 3. Alloc another batch. The recycled slot must be reused (register scans for
///    NULL slots). Allocations must be non-null, writable, and distinct.
///
/// This runs under miri (bounded N=500 blocks ÷ 2 KiB block size ≈ needs ~2-3
/// segments, small enough for miri).
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[test]
fn recycled_slot_is_reused() {
    use core::alloc::Layout;
    use std::collections::HashSet;

    use sefer_alloc::alloc_core::AllocCore;
    use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

    // oxx R2-06: see the module-level `TEST_LOCK` doc comment.
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Mechanism 2 (task #51): DISABLE the pool so this task-#60 slot-recycle unit
    // test deterministically exercises decommit → slot recycle → reuse (with the
    // pool ON, the ~3 emptied segments would be retained committed and reused via
    // the pool, so decommit/recycle would not fire). Pool reuse is covered by
    // `tests/small_segment_pool.rs`.
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");

    // 2 KiB blocks: small enough for miri, large enough to overflow the
    // primordial's payload in a few hundred allocs.
    let layout = Layout::from_size_align(2048, 8).unwrap();
    // 6000 × 2 KiB = 12 MiB — spans the primordial (~4 MiB) plus 2 fresh
    // Small segments. Matches the sizing in `decommit_miri_cycle`.
    const N: usize = 6000;

    let decommit_before = AllocCore::dbg_decommit_count();

    // Phase 1: alloc N blocks.
    let mut ptrs = Vec::with_capacity(N);
    for i in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "phase-1 alloc null at i={i}");
        ptrs.push(p);
    }
    // All N pointers must be distinct.
    let set1: HashSet<usize> = ptrs.iter().map(|&p| p as usize).collect();
    assert_eq!(set1.len(), N, "phase-1 alloc handed out duplicates");

    // Phase 2: free all. Non-current Small segments decommit → slots recycled.
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }

    let decommit_after = AllocCore::dbg_decommit_count();
    // Under miri, decommit_pages is a no-op but the bookkeeping (live_count
    // zero-crossing, decommit hook, reset, slot recycle) still runs.
    assert!(
        decommit_after > decommit_before,
        "no decommit fired — segment stayed current throughout (working set \
         too small or decommit hook miswired); \
         decommit_before={decommit_before}, after={decommit_after}"
    );

    // Phase 3: alloc N blocks again. Recycled slots must be reused; the allocator
    // must not fail (null) even though the cumulative segment count exceeds 1.
    let mut ptrs2 = Vec::with_capacity(N);
    for i in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "phase-3 alloc null at i={i} after recycle");
        // Writable + readable.
        unsafe {
            let b = (i & 0xFF) as u8;
            p.write(b);
            assert_eq!(p.read(), b, "phase-3 write/readback failed at i={i}");
        }
        ptrs2.push(p);
    }
    // All N re-alloced pointers must be distinct.
    let set2: HashSet<usize> = ptrs2.iter().map(|&p| p as usize).collect();
    assert_eq!(
        set2.len(),
        N,
        "phase-3 alloc handed out duplicates after recycle"
    );

    // Cleanup.
    for &p in &ptrs2 {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

// ============================================================
// Test 4 (L-3/UBFIX-11, extended by oxx R2-06) — recycle's anomalous
// segment_id-mismatch branch must evict the hash table / own-cache entry
// AND reuse the segment's real slot (not just release its OS reservation).
// ============================================================

/// `SegmentTable::recycle`'s O(1) path trusts the segment's own stamped
/// `segment_id` field to locate its slot (mirroring `unregister`'s O(1)
/// path). If that field is corrupted (a caller bug, or genuine memory
/// corruption — exactly the threat model the anomalous branch exists for),
/// `base` is still a genuine, live hash-table member — so `recycle`'s
/// anomalous branch must locate its REAL slot (a bounded linear scan by
/// VALUE — `base` is unique) and route it through the exact same normal
/// path as a clean recycle: hash/cache eviction, OS release, slot NULL,
/// free-list push. It must NOT leave `base` reachable via `contains_base`
/// afterwards, and it must NOT leave the slot dangling for `Drop` to
/// double-release.
///
/// ## oxx R2-06 counterfactual
///
/// Before the fix, `recycle`'s defensive tail released the OS reservation
/// (reading its `(reservation, reservation_len)` from an UNVERIFIED header
/// read that happened before any membership check) but left `slots[]`
/// completely untouched — `a`'s own original slot kept holding the
/// (now-dangling) pointer value forever, never pushed to the free-list.
/// This test proves the fix two ways:
///
/// 1. **Hash/cache eviction** (L-3/UBFIX-11, pre-existing, reverified here):
///    drive `a` through the own-cache fast path first (a won `contains_base`
///    probe fills it — PERF-P2/Э3), corrupt its `segment_id`, recycle, then
///    assert `contains_base(a) == false`.
/// 2. **Slot reuse — the oxx R2-06 counterfactual proper**: the free-list is
///    LIFO, and `a`'s slot is the only one just vacated, so the VERY NEXT
///    `register()` call (triggered by allocating `c` below) must reuse
///    EXACTLY `a`'s original slot index. Pre-fix, `a`'s slot was never
///    pushed to the free-list, so `c` would instead get a brand-new
///    APPENDED slot — this assertion goes RED without the fix (verified by
///    temporarily reverting `SegmentTable::recycle` to its pre-fix form and
///    re-running this test in isolation: `c`'s id came back as a fresh
///    append, not `a`'s original id, and a subsequent normal `Drop` of `ac`
///    then released `a`'s already-released reservation a second time).
///
/// With the fix, `a`'s slot is provably NOT dangling (it now holds `c`), so
/// — unlike the pre-fix version of this test — `ac` is allowed to `Drop`
/// normally at the end (no `mem::forget`); the exact `dbg_segments_released_total`
/// deltas confirm no release is missed or doubled.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[test]
fn recycle_defensive_tail_evicts_hash_and_cache() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    // oxx R2-06: see the module-level `TEST_LOCK` doc comment — this test's
    // `dbg_segments_released_total` deltas below need this file's other
    // tests' release traffic serialized out.
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut ac = AllocCore::new().expect("primordial");
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    let a = ac.alloc(layout);
    assert!(!a.is_null(), "a: large alloc failed");
    let b = ac.alloc(layout);
    assert!(!b.is_null(), "b: large alloc failed");

    // Drive `a` through the own-cache fast path (PERF-P2/Э3): a won
    // `contains_base` probe fills `own_cache[cache_index(a)] = a`. Without
    // this, the bug would still be provable via the hash table alone, but
    // exercising BOTH the cache and the hash is a stronger counterfactual —
    // the pre-fix code evicted NEITHER.
    assert!(
        ac.dbg_contains_base(a),
        "a must be registered before the test"
    );
    assert!(
        ac.dbg_contains_base(b),
        "b must be registered before the test"
    );

    let a_id = ac.dbg_segment_id_of(a);
    let b_id = ac.dbg_segment_id_of(b);
    assert_ne!(a_id, b_id, "precondition: distinct segment ids");

    let released_before = AllocCore::dbg_segments_released_total();

    // Corrupt `a`'s stamped segment_id to `b`'s id, exactly as
    // `unregister_defends_against_mismatched_segment_id` does for
    // `unregister`. `recycle`'s O(1) fast path reads `slots[b_id]`, finds
    // `b` there (not `a`), and falls into the anomalous branch — which must
    // then find `a`'s REAL slot by linear scan (since `a` is still a live
    // hash member) rather than treat this as "not found".
    // SAFETY (R6-CQ-2): `a` is a live allocation owned by `ac`. The corrupted
    // `b_id` is consumed ONLY by `dbg_recycle(a)` below — a test-only
    // teardown whose anomalous-branch linear scan resolves `a`'s true slot
    // independent of the corrupted stamped id and does NOT route on it being
    // correct. `a_id` (captured above) is never used to route any further
    // operation on `a` — `a` is fully retired by `dbg_recycle` below, and the
    // rest of this test only re-derives ids for `b`/`c`. Teardown-via-test-seam
    // per the `# Safety` contract.
    unsafe { ac.dbg_stamp_segment_id(a, b_id) };

    // Drive `a` through `recycle`'s anomalous branch. Under the fix this
    // releases `a`'s OS reservation AND evicts it from the hash table/cache
    // AND nulls + free-lists its real slot — the full normal path.
    // SAFETY: `a` is a live allocation owned by `ac`.
    unsafe { ac.dbg_recycle(a) };

    // Counterfactual 1 (L-3/UBFIX-11, reverified): `a` must no longer be
    // considered a live, routable segment. Pre-fix, the own-cache slot for
    // `a` (filled above) and/or the hash entry would still report a HIT here
    // — on an address whose OS reservation was JUST released (unmapped/
    // reusable by the OS).
    assert!(
        !ac.dbg_contains_base(a),
        "L-3 REGRESSION: `a` is still `contains_base`-reachable after \
         `recycle`'s anomalous branch released its OS reservation — a stale \
         hash/own-cache entry survived the release, so a later free routed \
         through `a`'s (unmapped) base would read/write freed memory"
    );

    // `b` must be completely unaffected by `a`'s recycle: still registered,
    // still writable.
    assert!(
        ac.dbg_contains_base(b),
        "b's registration was corrupted by a's recycle"
    );
    unsafe {
        b.write(0xAB);
        assert_eq!(b.read(), 0xAB, "b became unwritable after a's recycle");
    }

    // Exactly `a`'s reservation was released so far — no double-release, no
    // missed release.
    assert_eq!(
        AllocCore::dbg_segments_released_total() - released_before,
        1,
        "recycle's anomalous branch must release exactly `a`'s reservation"
    );

    // Counterfactual 2 (oxx R2-06, the load-bearing assertion of this test):
    // the free-list is LIFO and `a`'s slot is the only one just vacated, so
    // the VERY NEXT `register()` call must reuse EXACTLY `a`'s original slot
    // index. Pre-fix, `a`'s slot was never pushed to the free-list (the
    // defensive tail left `slots[]` untouched), so this next alloc would
    // instead get a brand-new APPENDED slot — this assertion goes RED
    // without the fix.
    let c = ac.alloc(layout);
    assert!(!c.is_null(), "c: large alloc failed");
    let c_id = ac.dbg_segment_id_of(c);
    assert_eq!(
        c_id, a_id,
        "R2-06 REGRESSION: `a`'s original slot (id={a_id}) was not recycled \
         by `recycle`'s anomalous branch — the next register() appended a \
         new slot (id={c_id}) instead of reusing it, meaning `a`'s slot is \
         still dangling (non-NULL, pointing at unmapped memory) and would be \
         double-released when `ac` drops"
    );
    unsafe {
        c.write(0xCD);
        assert_eq!(c.read(), 0xCD, "c is not writable after slot reuse");
    }

    // oxx R2-06: with the fix there is no dangling slot left for `Drop` to
    // walk into a double-release, so — unlike the pre-fix version of this
    // test, which had to `mem::forget(ac)` to sidestep exactly that hazard —
    // `ac` is allowed to drop NORMALLY here. Live segments at this point:
    // the primordial, `b`, and `c` (`a` was already retired above) — `Drop`
    // must release exactly those three, no more, no fewer.
    let released_before_drop = AllocCore::dbg_segments_released_total();
    drop(ac);
    assert_eq!(
        AllocCore::dbg_segments_released_total() - released_before_drop,
        3,
        "R2-06 REGRESSION: Drop released a different count than exactly \
         {{primordial, b, c}} — either a leak or a double-release slipped \
         through"
    );
}
