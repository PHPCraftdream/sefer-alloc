//! Phase 12.2 — single-thread unit tests for the global heap registry.
//!
//! Covers the §2.1 API contract of `ALLOC_PLAN_PHASE12-13.md`:
//! - `claim` hands out distinct slots.
//! - `recycle` → `claim` reuses a slot and BUMPS its generation.
//! - bootstrap is idempotent (a second `ensure` does NOT re-initialise).
//! - The latest reuse hint precedes fresh claims; older FREE slots are deferred.
//!
//! NON-VACUOUS: every assertion is built so that flipping the implementation
//! (e.g. claim not bumping generation, recycle not pushing, pop returning the
//! wrong slot) makes the test FAIL. The registry is exercised only by these
//! tests in Phase 12.2 — it is not yet wired into `SeferAlloc`/TLS (12.3).
//!
//! Single-threaded by design (the concurrent case is Phase 12.4's loom). The
//! orderings are written for the concurrent case from day one; these tests
//! verify the SEQUENTIAL contract, not the memory model.
//!
//! ## Test isolation
//!
//! The registry is a process-global `static`; its slot array is NEVER reset
//! (resetting would leak the lazily-materialised `HeapCore`'s OS segments).
//! `count` is monotonic across the suite, so each test derives its expected
//! slot indices RELATIVE to the `count` it observed at entry (via
//! [`count_at_entry`]). This gives test isolation without leaking.
//!
//! (The abandoned-segments stack round-trip tests that previously lived here
//! were removed with that substrate — task #97 / R4-5. The legacy
//! raw-pointer protocol tests (`recycle(null)` no-op, double-recycle no-op)
//! were moved into `src/registry/heap_registry/claim.rs`'s
//! `legacy_semantics` unit module — they pin legacy-API semantics that
//! [`HeapLease`] cannot express, and the legacy surface is `pub(crate)` now.)

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::HeapLease;
use sefer_alloc::registry::{
    bootstrap,
    heap_slot::{STATE_FREE, STATE_LIVE},
    HeapRegistry, HeapSlot,
};

// The registry is a process-global static; tests that touch it MUST run
// serially (a parallel claim race makes absolute-slot-index assertions
// meaningless and would also exercise the lock-free path that Phase 12.4's
// loom is the right tool for, not these sequential-contract tests). We gate
// every test on this one-shot mutex: the first test to grab it runs, the rest
// block. (Equivalent to the `serial_test` crate, without adding a dev-dep.)
static SERIAL: AtomicBool = AtomicBool::new(false);

/// RAII guard that holds the serial flag for the duration of a test.
struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// Acquire the serial guard at the top of each test. The tests below share
/// the global registry; running them serially gives deterministic slot-index
/// arithmetic (the count observed at entry is stable for the test's duration).
macro_rules! serial {
    () => {
        let _serial = SerialGuard::acquire();
    };
}

/// Snapshot the high-water `count` at test entry, so the test can derive its
/// expected slot indices relative to it (the suite shares the global
/// registry; under the serial guard `count` is stable for the test's body).
fn count_at_entry() -> u32 {
    bootstrap::count_for_test()
}

/// Read a slot's `state` atomically (test helper — the field is `pub(crate)`;
/// reached via the narrow `Registry::dbg_slot_state` accessor).
fn slot_state(idx: usize) -> u8 {
    let reg = bootstrap::ensure();
    reg.dbg_slot_state(idx)
}

/// Read a slot's `generation` atomically (test helper). `generation` is
/// `AtomicU64` since task W7a (widened from `AtomicU32` to move the recycle→
/// reclaim ABA wrap from `2^32` to an unreachable `2^64`); reached via
/// `Registry::dbg_slot_generation`.
fn slot_generation(idx: usize) -> u64 {
    let reg = bootstrap::ensure();
    reg.dbg_slot_generation(idx)
}

/// Claim leases until the registry MINTS a fresh slot (index == count at
/// entry). Post-Ph4c the other tests in this suite recycle their leases on
/// Drop, so a plain `dbg_claim_lease()` may legitimately hand back a recycled
/// slot; this drains those (leaking them — see below) until the
/// fresh mint happens. Pre-migration the tests leaked their claims, so every
/// `claim` minted; the migrated suite must tolerate the recycled fast path.
fn claim_fresh_lease() -> HeapLease {
    let base = count_at_entry();
    for _ in 0..4096 {
        let lease = HeapRegistry::dbg_claim_lease().expect("claim must not return null");
        if lease.slot_index() == base {
            return lease;
        }
        // Leak (not drop): dropping would publish a reuse hint for this slot
        // and the next claim would pop it right back — an infinite drain.
        // Leaked LIVE slots shrink the reclaimable pool every iteration, so
        // the loop terminates (same shape as the pre-migration leaked claims).
        std::mem::forget(lease);
    }
    panic!("registry never minted the fresh slot {base}");
}

/// `claim` hands out distinct slots, each of which is `LIVE`. Migrated to the
/// typed [`HeapLease`] surface (Ph4c): slot distinctness is asserted via
/// `slot_index()` (stable per-slot identity) instead of raw-pointer
/// inequality.
#[test]
fn claim_yields_distinct_live_slots() {
    serial!();
    let base = count_at_entry();
    let a = claim_fresh_lease();
    let b = claim_fresh_lease();
    let c = claim_fresh_lease();
    assert_ne!(
        a.slot_index(),
        b.slot_index(),
        "claim must hand out DISTINCT slots (a vs b)"
    );
    assert_ne!(
        b.slot_index(),
        c.slot_index(),
        "claim must hand out DISTINCT slots (b vs c)"
    );
    assert_ne!(
        a.slot_index(),
        c.slot_index(),
        "claim must hand out DISTINCT slots (a vs c)"
    );

    // Each claimed slot is LIVE and its index matches its expected slot
    // (count was `base` at entry, so the three claims mint indices
    // `base`, `base+1`, `base+2`).
    let id_a = a.slot_index() as usize;
    let id_b = b.slot_index() as usize;
    let id_c = c.slot_index() as usize;
    assert_eq!(
        id_a, base as usize,
        "first claim mints the next count index"
    );
    assert_eq!(id_b, base as usize + 1);
    assert_eq!(id_c, base as usize + 2);
    assert_eq!(slot_state(id_a), STATE_LIVE, "claimed slot must be LIVE");
    assert_eq!(slot_state(id_b), STATE_LIVE);
    assert_eq!(slot_state(id_c), STATE_LIVE);
    // The leases are dropped (LIVE → FREE) at scope exit, exactly like
    // `recycle`.
}

/// Drop of a claim lease (the lease analogue of `recycle`) followed by a new
/// claim reuses the recycled slot (LIFO) and BUMPS its generation — the
/// M8/M9 coherence key.
#[test]
fn recycle_then_claim_reuses_slot_and_bumps_generation() {
    serial!();
    let _base = count_at_entry();
    let a = claim_fresh_lease();
    let id_a = a.slot_index() as usize;
    assert_eq!(
        a.generation(),
        1,
        "first claim of a fresh slot must produce generation 1 \
         (started at 0, bumped once)"
    );

    // Dropping the lease returns the slot LIVE → FREE, exactly like `recycle`.
    drop(a);
    assert_ne!(
        slot_state(id_a),
        STATE_LIVE,
        "recycled slot must NOT be LIVE (it is FREE)"
    );

    // The next claim should reuse the slot we just dropped (free_slots LIFO).
    let b = HeapRegistry::dbg_claim_lease().expect("claim after recycle must not return null");
    assert_eq!(
        b.slot_index() as usize,
        id_a,
        "claim after recycle must reuse the SAME slot (free_slots LIFO)"
    );
    assert_eq!(
        b.generation(),
        2,
        "re-claim must BUMP the generation (M8/M9 coherence key)"
    );
}

/// The latest hint wins once, then fresh capacity precedes an older FREE heap.
#[test]
fn latest_hint_then_fresh_defers_older_free() {
    serial!();
    let _base = count_at_entry();
    let a = HeapRegistry::dbg_claim_lease().expect("claim must not return null");
    let b = HeapRegistry::dbg_claim_lease().expect("claim must not return null");
    let id_a = a.slot_index() as usize;
    let id_b = b.slot_index() as usize;
    assert_ne!(id_a, id_b);
    let generation_a = a.generation();
    let generation_b = b.generation();
    let next_fresh = count_at_entry();

    // B overwrites A's advisory hint; A's slot state remains authoritative.
    // Dropping the leases recycles both slots (A first, then B — B's hint is
    // the latest).
    drop(a);
    drop(b);
    assert_eq!(slot_state(id_a), STATE_FREE);
    assert_eq!(slot_state(id_b), STATE_FREE);

    let c = HeapRegistry::dbg_claim_lease().expect("claim must not return null");
    assert_eq!(c.slot_index() as usize, id_b, "latest hint must reuse B");
    assert_eq!(count_at_entry(), next_fresh);
    assert_eq!(c.generation(), generation_b + 1);

    let d = HeapRegistry::dbg_claim_lease().expect("claim must not return null");
    assert_eq!(
        d.slot_index() as usize,
        next_fresh as usize,
        "fresh capacity precedes older FREE"
    );
    assert_eq!(count_at_entry(), next_fresh + 1);
    assert_eq!(slot_state(id_b), STATE_LIVE);
    assert_eq!(slot_state(next_fresh as usize), STATE_LIVE);
    assert_eq!(slot_state(id_a), STATE_FREE, "older A remains claimable");
    assert_eq!(slot_generation(id_a), generation_a, "A was not reclaimed");

    // Older FREE A remains discoverable via maintenance. Earlier tests'
    // recycled slots may also be claimable (and scan order may surface them
    // first), so drain maintenance leases until A itself surfaces; the
    // intermediates are leaked LIVE (same shape as `claim_fresh_lease`), so
    // the pool shrinks each iteration and the loop terminates.
    let mut lease = HeapRegistry::dbg_try_maintenance().expect("older FREE A remains discoverable");
    while lease.slot_index() != id_a {
        std::mem::forget(lease);
        lease = HeapRegistry::dbg_try_maintenance()
            .expect("maintenance must keep discovering FREE slots");
    }
    drop(lease);
    let recovered = HeapRegistry::dbg_claim_lease().expect("claim must not return null");
    assert_eq!(recovered.slot_index() as usize, id_a);
    assert_eq!(slot_state(id_a), STATE_LIVE);
    assert_eq!(slot_generation(id_a), generation_a + 1);
    assert_eq!(count_at_entry(), next_fresh + 1);
    // The LIVE leases (c, d, recovered) are held to the end of the test — the
    // state assertions above observe them — and their Drop recycles the
    // slots.
}

/// Bootstrap idempotency: every call to `ensure` returns the SAME pointer and
/// does NOT re-initialise the registry. With the lazy-allocation design the
/// registry is allocated exactly once (via `aligned_vmem::reserve_aligned`)
/// and the pointer is published with Release; every subsequent call observes
/// the same pointer under Acquire and returns immediately.
///
/// We verify: (a) two consecutive `ensure` calls return the SAME pointer
/// (identity), and (b) a `claim` that advanced `count` is visible after the
/// second `ensure` (no re-init zeroed `count`).
#[test]
fn bootstrap_is_idempotent() {
    serial!();
    let count_before_claim = count_at_entry();
    // Advance count by EXACTLY 1 via a fresh mint (`claim_fresh_lease`
    // drains the recycled pool the other tests' lease Drops leave behind —
    // a plain claim would legally pop a FREE slot and not mint). The lease
    // is intentionally leaked (the pre-migration shape `let _ =
    // HeapRegistry::claim()`): its Drop would publish a reuse hint that
    // poisons the other tests' fresh-mint index assertions.
    std::mem::forget(claim_fresh_lease());
    let reg_before = bootstrap::ensure();
    let count_before = bootstrap::count_for_test();
    assert_eq!(
        count_before,
        count_before_claim + 1,
        "claim must advance count by exactly 1"
    );

    // Call ensure() again — with the lazy-allocation design, this must return
    // the SAME pointer (the fast path: Acquire load of REGISTRY_PTR, non-null
    // non-sentinel → return immediately). The registry is NOT reconstructed.
    let reg_after = bootstrap::ensure();
    let count_after = bootstrap::count_for_test();
    assert_eq!(
        count_before, count_after,
        "a second ensure must not re-initialise the registry (count preserved)"
    );

    // The slot array is the SAME heap allocation (identity check).
    assert!(
        std::ptr::eq(reg_before, reg_after),
        "ensure must return the SAME &'static Registry on every call"
    );
}

/// Compile-time sanity: the `HeapSlot` type is `Sync` (required for the
/// process-global `static REGISTRY`). This is a static assertion: if the
/// `unsafe impl Sync` is ever removed, this fails to compile.
#[test]
fn heap_slot_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<HeapSlot>();
}
