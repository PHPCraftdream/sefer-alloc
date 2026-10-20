//! N9-v3 — pin for the CHANGED memo fill policy of the one-entry
//! last-segment memo in `SegmentTable::canonical_base_of_mut`
//! (`docs/perf/OPEN_ITEMS.md` item 102 (а), acceptance N9).
//!
//! Pre-v3 policy: the memo was filled ONLY from a won `hash_find` probe
//! (a full Tier-1 miss). N9-v3 policy: the memo is filled ONLY from an
//! exact-base `own_cache` HIT; a won probe fills `own_cache` but
//! deliberately does NOT touch the memo (on the miss-heavy thrash64
//! workload the 1-entry memo never hits between misses, so the fill was
//! 2 dead stores + a compare on every full miss).
//!
//! The policy is observed directly via the test-only, read-only
//! `AllocCore::dbg_segment_memo_holds` hook (`bench-internals` gated):
//! the `CONTAINS_BASE_TIER1_*` counters cannot distinguish the two
//! policies (a memo hit and an own_cache hit increment the same counter).
//!
//! Determinism: `register`/`alloc` never fill the memo or `own_cache`
//! (only a won probe or an own_cache hit does), so on a fresh `AllocCore`
//! the FIRST dealloc is a guaranteed won probe with an EMPTY memo — under
//! N9-v3 the memo must stay EMPTY; under the pre-v3 policy it would hold
//! the base (red-on-revert). The SECOND dealloc of the same segment is a
//! guaranteed own_cache hit and MUST arm the memo under N9-v3.
//!
//! Requires `alloc-core` + `bench-internals` + `internals`; compiled out
//! otherwise.

#![cfg(all(
    feature = "alloc-core",
    feature = "bench-internals",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::AllocCore;
use std::sync::Mutex;

// The memo is per-`AllocCore` state and each test builds its own, but the
// established pattern for internals-observing tests in this crate is a
// process-wide serialization lock — keep it (sibling tests in other files
// share the Tier-1 counter statics this test's hook deliberately does NOT
// touch, but the lock costs nothing and keeps the file future-proof).
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn won_probe_does_not_arm_memo_own_cache_hit_does() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut ac = AllocCore::new().expect("primordial");

    let layout = Layout::from_size_align(16, 8).unwrap();
    let p1 = ac.alloc(layout);
    assert!(!p1.is_null());
    let p2 = ac.alloc(layout);
    assert!(!p2.is_null());
    let p3 = ac.alloc(layout);
    assert!(!p3.is_null());

    // Sanity: no dealloc has happened yet, so no fill site has ever run —
    // the memo must be EMPTY (it cannot hold p1's base).
    assert!(
        !ac.dbg_segment_memo_holds(p1),
        "pre-dealloc: a fresh AllocCore's memo must be empty (alloc/register \
         never fill the memo)"
    );

    // ── Assertion 1 (RED-ON-REVERT): the first dealloc of a fresh
    //    AllocCore is a guaranteed won `hash_find` probe (own_cache
    //    all-EMPTY, memo empty). Under N9-v3 a won probe must NOT arm the
    //    memo; the pre-v3 policy DID (won-probe fill) — this assertion is
    //    what fails when the policy is reverted.
    // SAFETY (R6-MS-1/2): p1 was returned by `alloc` above and is freed
    // exactly once here.
    unsafe { ac.dealloc(p1, layout) };
    assert!(
        !ac.dbg_segment_memo_holds(p1),
        "N9-v3: a won hash_find probe (full Tier-1 miss) must NOT arm the \
         last-segment memo — if this fails, the pre-v3 won-probe fill is \
         back"
    );

    // ── Assertion 2: the second dealloc of the SAME segment is a
    //    guaranteed own_cache HIT (the first dealloc's won probe filled
    //    the slot) — under N9-v3 the hit-path fill MUST arm the memo.
    // SAFETY: as above, for p2.
    unsafe { ac.dealloc(p2, layout) };
    assert!(
        ac.dbg_segment_memo_holds(p2),
        "N9-v3: an exact-base own_cache HIT must arm the last-segment memo"
    );

    // ── Assertion 3: once armed by the hit-path fill, the memo serves the
    //    third dealloc of the same segment (memo hit) and stays armed.
    // SAFETY: as above, for p3.
    unsafe { ac.dealloc(p3, layout) };
    assert!(
        ac.dbg_segment_memo_holds(p3),
        "N9-v3: a memo hit must leave the memo armed (single-entry memo \
         holds the last resolved segment)"
    );
}
