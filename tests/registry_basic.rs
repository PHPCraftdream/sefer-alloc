//! Phase 12.2 — single-thread unit tests for the global heap registry.
//!
//! Covers the §2.1 API contract of `MALLOC_PLAN_PHASE12-13.md`:
//! - `claim` hands out distinct slots.
//! - `recycle` → `claim` reuses a slot and BUMPS its generation.
//! - `push_abandoned_segment` → `pop_abandoned_segment` round-trip.
//! - bootstrap is idempotent (a second `ensure` does NOT re-initialise).
//! - `free_slots` LIFO order is correct.
//!
//! NON-VACUOUS: every assertion is built so that flipping the implementation
//! (e.g. claim not bumping generation, recycle not pushing, pop returning the
//! wrong slot) makes the test FAIL. The registry is exercised only by these
//! tests in Phase 12.2 — it is not yet wired into `SeferMalloc`/TLS (12.3).
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
//! [`count_at_entry`]). The abandoned-segments stack is drained at the start
//! of each test that touches it. This gives test isolation without leaking.

#![cfg(feature = "alloc-global")]

use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, heap_slot::STATE_LIVE, HeapRegistry, HeapSlot};

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

/// Drain any leftover abandoned-segment entries so a test starts from a
/// known-empty stack.
fn drain_abandoned() {
    while HeapRegistry::pop_abandoned_segment().is_some() {}
}

/// Read a slot's `state` atomically (test helper — the field is `pub` under
/// `#[doc(hidden)]`).
fn slot_state(idx: usize) -> u8 {
    let reg = bootstrap::ensure();
    reg.slots[idx].state.load(Ordering::Acquire)
}

/// Read a slot's `generation` atomically (test helper).
fn slot_generation(idx: usize) -> u32 {
    let reg = bootstrap::ensure();
    reg.slots[idx].generation.load(Ordering::Acquire)
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

/// `free_slots` LIFO order: recycle A then B, the next two claims pop B then A.
#[test]
fn free_slots_is_lifo() {
    serial!();
    let _base = count_at_entry();
    let a = HeapRegistry::claim();
    let b = HeapRegistry::claim();
    let id_a = unsafe { (*a).id() } as usize;
    let id_b = unsafe { (*b).id() } as usize;
    assert_ne!(id_a, id_b);

    // Recycle A then B → stack top is B.
    // SAFETY: `a` and `b` were returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };
    unsafe { HeapRegistry::recycle(b) };

    // Next claim must pop B (LIFO), then A.
    let c = HeapRegistry::claim();
    let d = HeapRegistry::claim();
    assert!(!c.is_null() && !d.is_null());
    let id_c = unsafe { (*c).id() } as usize;
    let id_d = unsafe { (*d).id() } as usize;
    assert_eq!(
        id_c, id_b,
        "LIFO: first re-claim must pop the last recycled (B)"
    );
    assert_eq!(
        id_d, id_a,
        "LIFO: second re-claim must pop the earlier recycled (A)"
    );
}

/// `push_abandoned_segment` → `pop_abandoned_segment` round-trip returns the
/// pushed base; an empty pop returns `None`.
///
/// FINDINGS №1 regression test: the abandoned-segments stack now packs the
/// FULL 64-bit base (SEGMENT-aligned bases have zero low 22 bits, reused for
/// the ABA tag). The pop reads the segment's `next_abandoned` header field, so
/// it requires a REAL segment base (a fake non-segment address would crash on
/// the header read). We obtain a real segment base from a claimed heap's
/// `segment_bases()` iterator. To exercise the >4 GiB path we would need a
/// real high mapping; on hosts where the OS returns low addresses (miri) we
/// still assert the round-trip is exact — the PACKING correctness (no
/// truncation) is unit-tested separately in `bootstrap`'s pack/unpack, and
/// the loom harness exercises the push/pop CAS protocol.
#[test]
fn abandon_pop_round_trip() {
    serial!();
    drain_abandoned();
    // Empty pop returns None.
    assert!(
        HeapRegistry::pop_abandoned_segment().is_none(),
        "pop on an empty abandoned stack must return None"
    );

    // Obtain a REAL segment base from a claimed heap (the pop reads the
    // segment header, so a fake address would crash). A fresh heap owns its
    // primordial segment.
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    // SAFETY: `heap` was returned by `claim` and is the slot's sole writer.
    let heap_ref: &mut sefer_alloc::registry::HeapCore = unsafe { &mut *heap };
    let base = heap_ref
        .segment_bases()
        .next()
        .expect("a fresh heap owns at least its primordial segment");
    assert!(!base.is_null(), "segment base must be non-null");

    HeapRegistry::push_abandoned_segment(base);
    let popped = HeapRegistry::pop_abandoned_segment().expect("pop must return the pushed base");
    assert_eq!(
        popped, base,
        "pop must return the exact base that was pushed (no address truncation)"
    );

    // After a pop the stack is empty again.
    assert!(
        HeapRegistry::pop_abandoned_segment().is_none(),
        "after popping the only entry, the stack must be empty"
    );
}

/// FINDINGS №1 — the abandoned-head PACKING preserves a full >4 GiB address.
/// This is a pure-arithmetic unit test of `pack_abandoned_head` /
/// `unpack_abandoned_head`: a SEGMENT-aligned address above 4 GiB round-trips
/// intact. On the OLD (pre-12.4) packing — base in the low 32 bits — this
/// address would be truncated to a DIFFERENT value and the test would FAIL
/// (the counterfactual that makes this test non-vacuous).
///
/// We test the packing directly (not through the registry's push/pop, which
/// dereference the segment header) so we can feed a synthetic high address
/// without needing a real OS mapping at that address.
#[test]
fn abandoned_head_packing_preserves_high_address() {
    use sefer_alloc::registry::bootstrap::*;

    const SEGMENT: u64 = 1 << 22; // matches os::SEGMENT
                                  // A realistic ASLR-style address: ~127 GiB into the address space, well
                                  // above 4 GiB. Masked to SEGMENT alignment.
    let high = 0x7f_0123_4000_u64;
    let base = (high & !(SEGMENT - 1)) as *mut u8;
    assert_eq!(
        base as u64 % SEGMENT,
        0,
        "fake base must be SEGMENT-aligned (the packing requires it)"
    );
    assert!(
        (base as u64) > (1u64 << 32),
        "fake base must be above 4 GiB to exercise the >4 GiB path"
    );

    // Pack with a non-zero tag, then unpack — base must round-trip EXACTLY.
    let word = pack_abandoned_head(base, 0x123456);
    let (recovered_base, recovered_tag) = unpack_abandoned_head(word);
    assert_eq!(
        recovered_base, base,
        "pack/unpack must preserve the full >4 GiB base (FINDINGS №1 fix)"
    );
    assert_eq!(recovered_tag, 0x123456, "pack/unpack must preserve the tag");

    // The empty sentinel round-trips as empty.
    assert!(
        abandoned_head_is_empty(ABANDONED_HEAD_EMPTY),
        "the empty sentinel must denote an empty stack"
    );
    assert!(
        !abandoned_head_is_empty(word),
        "a packed non-null base must NOT denote an empty stack"
    );
}

/// `abandon_segments` is now a REAL walk (Phase 12.4): it stamps each owned
/// segment `owner_state = ABANDONED` and pushes its base onto the abandoned
/// stack. This test pins that contract: after abandoning a heap, the segments
/// ARE on the abandoned stack (a pop returns a non-null base). This replaces
/// the Phase 12.2 no-op stub test — the walk is implemented and segments no
/// longer leak on thread exit.
#[test]
fn abandon_segments_walks_owned_segments() {
    serial!();
    drain_abandoned();
    let a = HeapRegistry::claim();
    assert!(!a.is_null());
    // SAFETY: `a` was returned by `claim` and is the slot's sole writer.
    let heap: &mut sefer_alloc::registry::HeapCore = unsafe { &mut *a };
    // Allocate from the heap so it owns at least one segment with a stamped
    // owner_state (the primordial segment is reserved at HeapCore::new).
    let layout = core::alloc::Layout::from_size_align(64, 8).unwrap();
    let ptr = heap.alloc(layout);
    assert!(!ptr.is_null(), "alloc must succeed on a fresh heap");
    // SAFETY: `a` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::abandon_segments(a) };
    // The heap owned ≥1 segment (the primordial); abandoning pushed it.
    let popped = HeapRegistry::pop_abandoned_segment();
    assert!(
        popped.is_some(),
        "abandon_segments must push owned segments onto the abandoned stack \
         (Phase 12.4: it is a real walk, not a no-op)"
    );
    // Drain any remaining (a heap may own several segments).
    while HeapRegistry::pop_abandoned_segment().is_some() {}
}

/// Bootstrap idempotency: a second `ensure` returns the SAME static and does
/// NOT re-initialise. We observe this by claiming a slot (advancing `count`),
/// then resetting ONLY the init-state, then re-calling `ensure`: `count` must
/// be unchanged (the registry state survived), proving it was not
/// reconstructed.
#[test]
fn bootstrap_is_idempotent() {
    serial!();
    let count_before_claim = count_at_entry();
    let _ = HeapRegistry::claim(); // advances count by 1
    let reg_before = bootstrap::ensure();
    let count_before = reg_before.count.load(Ordering::Acquire);
    assert_eq!(
        count_before,
        count_before_claim + 1,
        "claim must advance count by exactly 1"
    );

    // Reset ONLY the init-state word (simulate a re-entry of ensure) but
    // leave the dynamic atomics intact. A correct bootstrap must NOT zero
    // `count`.
    bootstrap::reset_for_test();
    let reg_after = bootstrap::ensure();
    let count_after = reg_after.count.load(Ordering::Acquire);
    assert_eq!(
        count_before, count_after,
        "a second ensure must not re-initialise the registry (count preserved)"
    );

    // The slot array is the SAME static (identity check).
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
