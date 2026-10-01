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
//! were removed with that substrate — task #97 / R4-5.)

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::sync::atomic::{AtomicBool, Ordering};

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

/// `claim` returns non-null distinct slots, and each claimed slot is `LIVE`.
#[test]
fn claim_yields_distinct_live_slots() {
    serial!();
    let base = count_at_entry();
    let a = HeapRegistry::claim();
    let b = HeapRegistry::claim();
    let c = HeapRegistry::claim();
    assert!(
        !a.is_null(),
        "claim must not return null on a fresh registry"
    );
    assert!(!b.is_null(), "second claim must not return null");
    assert!(!c.is_null(), "third claim must not return null");
    assert_ne!(a, b, "claim must hand out DISTINCT slots (a vs b)");
    assert_ne!(b, c, "claim must hand out DISTINCT slots (b vs c)");
    assert_ne!(a, c, "claim must hand out DISTINCT slots (a vs c)");

    // Each claimed slot is LIVE and its heap id matches its expected index
    // (count was `base` at entry, so the three claims mint indices
    // `base`, `base+1`, `base+2`).
    let id_a = unsafe { (*a).id() } as usize;
    let id_b = unsafe { (*b).id() } as usize;
    let id_c = unsafe { (*c).id() } as usize;
    assert_eq!(
        id_a, base as usize,
        "first claim mints the next count index"
    );
    assert_eq!(id_b, base as usize + 1);
    assert_eq!(id_c, base as usize + 2);
    assert_eq!(slot_state(id_a), STATE_LIVE, "claimed slot must be LIVE");
    assert_eq!(slot_state(id_b), STATE_LIVE);
    assert_eq!(slot_state(id_c), STATE_LIVE);
}

/// `recycle` followed by `claim` reuses the recycled slot (LIFO) and BUMPS
/// its generation — the M8/M9 coherence key.
#[test]
fn recycle_then_claim_reuses_slot_and_bumps_generation() {
    serial!();
    let _base = count_at_entry();
    let a = HeapRegistry::claim();
    assert!(!a.is_null());
    let id_a = unsafe { (*a).id() } as usize;
    let gen_after_first_claim = slot_generation(id_a);
    // The first-ever claim of a slot yields generation 1 (slot starts at 0).
    // If this slot was claimed by an earlier test... it cannot be: count is
    // monotonic and each test mints fresh slots, so this slot is being
    // claimed for the first time in the suite.
    assert_eq!(
        gen_after_first_claim, 1,
        "first claim of a fresh slot must produce generation 1 \
         (started at 0, bumped once)"
    );

    // SAFETY: `a` was returned by `claim` above and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };
    assert_ne!(
        slot_state(id_a),
        STATE_LIVE,
        "recycled slot must NOT be LIVE (it is FREE)"
    );

    // The next claim should reuse the slot we just recycled (free_slots LIFO).
    let b = HeapRegistry::claim();
    assert!(!b.is_null(), "claim after recycle must not return null");
    let id_b = unsafe { (*b).id() } as usize;
    assert_eq!(
        id_b, id_a,
        "claim after recycle must reuse the SAME slot (free_slots LIFO)"
    );
    let gen_after_second_claim = slot_generation(id_b);
    assert_eq!(
        gen_after_second_claim,
        gen_after_first_claim + 1,
        "re-claim must BUMP the generation (M8/M9 coherence key)"
    );
}

/// The latest hint wins once, then fresh capacity precedes an older FREE heap.
#[test]
fn latest_hint_then_fresh_defers_older_free() {
    serial!();
    let _base = count_at_entry();
    let a = HeapRegistry::claim();
    let b = HeapRegistry::claim();
    assert!(!a.is_null() && !b.is_null());
    // SAFETY: both pointers are distinct LIVE claims owned by this test.
    let id_a = unsafe { (*a).id() } as usize;
    // SAFETY: b remains this test's LIVE claim until recycle below.
    let id_b = unsafe { (*b).id() } as usize;
    assert_ne!(id_a, id_b);
    let generation_a = slot_generation(id_a);
    let generation_b = slot_generation(id_b);
    let next_fresh = count_at_entry();

    // B overwrites A's advisory hint; A's slot state remains authoritative.
    // SAFETY: `a` and `b` were returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };
    // SAFETY: b was returned by claim and has not yet been recycled.
    unsafe { HeapRegistry::recycle(b) };
    assert_eq!(slot_state(id_a), STATE_FREE);
    assert_eq!(slot_state(id_b), STATE_FREE);

    let c = HeapRegistry::claim();
    assert!(!c.is_null());
    // SAFETY: c is this test's newly acquired LIVE claim.
    let id_c = unsafe { (*c).id() } as usize;
    assert_eq!(id_c, id_b, "latest hint must reuse B");
    assert_eq!(count_at_entry(), next_fresh);
    assert_eq!(slot_generation(id_b), generation_b + 1);

    let d = HeapRegistry::claim();
    assert!(!d.is_null());
    // SAFETY: d is a distinct LIVE claim owned by this test.
    let id_d = unsafe { (*d).id() } as usize;
    assert_eq!(
        id_d, next_fresh as usize,
        "fresh capacity precedes older FREE"
    );
    assert_eq!(count_at_entry(), next_fresh + 1);
    assert_eq!(slot_state(id_c), STATE_LIVE);
    assert_eq!(slot_state(id_d), STATE_LIVE);
    assert_eq!(slot_state(id_a), STATE_FREE, "older A remains claimable");
    assert_eq!(slot_generation(id_a), generation_a, "A was not reclaimed");

    let lease = HeapRegistry::try_maintenance().expect("older FREE A remains discoverable");
    assert_eq!(lease.slot_index(), id_a);
    drop(lease);
    let recovered = HeapRegistry::claim();
    assert!(!recovered.is_null());
    // SAFETY: recovered is this test's newly acquired LIVE claim.
    assert_eq!(unsafe { (*recovered).id() } as usize, id_a);
    assert_eq!(slot_state(id_a), STATE_LIVE);
    assert_eq!(slot_generation(id_a), generation_a + 1);
    assert_eq!(count_at_entry(), next_fresh + 1);
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
    let _ = HeapRegistry::claim(); // advances count by 1
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

/// `recycle` of a null pointer is a safe no-op (defensive).
#[test]
fn recycle_null_is_noop() {
    serial!();
    let base = count_at_entry();
    // SAFETY: null is explicitly allowed (a no-op per the contract).
    unsafe { HeapRegistry::recycle(core::ptr::null_mut()) };
    // No crash, no state change observable: the next claim mints the next
    // count index (does not pop a phantom slot from free_slots).
    let a = HeapRegistry::claim();
    assert!(!a.is_null());
    let id_a = unsafe { (*a).id() } as usize;
    assert_eq!(
        id_a, base as usize,
        "after a null recycle, the first real claim mints the next count index"
    );
}

/// Double-recycle is a safe no-op: recycling the same heap twice does not
/// corrupt the free_slots stack (the CAS LIVE→FREE fails on the second call).
/// We verify by checking that two claims after a double-recycle pop the slot
/// exactly once and then mint a fresh one.
#[test]
fn double_recycle_is_safe_noop() {
    serial!();
    let base = count_at_entry();
    let a = HeapRegistry::claim();
    let id_a = unsafe { (*a).id() } as usize;
    // SAFETY: `a` was returned by `claim`. The first recycle is valid; the
    // second is a contract violation (double-recycle) that the registry
    // handles defensively (CAS LIVE→FREE fails, no-op).
    unsafe { HeapRegistry::recycle(a) };
    unsafe { HeapRegistry::recycle(a) }; // defensive: must be a no-op, not a double-push

    // Two claims: the first reuses the recycled slot; the second must NOT
    // also resolve to the same slot (which would happen if the double-recycle
    // pushed it twice and corrupted the stack).
    let b = HeapRegistry::claim();
    let c = HeapRegistry::claim();
    assert!(!b.is_null() && !c.is_null());
    let id_b = unsafe { (*b).id() } as usize;
    let id_c = unsafe { (*c).id() } as usize;
    assert_eq!(
        id_b, id_a,
        "first re-claim pops the once-pushed recycled slot"
    );
    assert_ne!(
        id_b, id_c,
        "second claim must mint a DIFFERENT slot (no phantom duplicate from double-recycle)"
    );
    // The fresh slot is the next count index.
    assert_eq!(
        id_c,
        base as usize + 1,
        "the second claim after a double-recycle mints the next count index"
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
