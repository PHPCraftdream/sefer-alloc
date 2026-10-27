//! Ph4b (task #2092, chunk B): loom shadow-model of publish ‖ recycle ‖ drain
//! — a foreign producer publishing into a slot's sidecar while the owner
//! tears down (TORN stamp, trim, `HeapLease::drop`'s `LIVE → FREE` Release
//! CAS) and a fresh claimant races for the recycled slot and runs a bounded
//! drain. Shadow model by the pattern of `tests/loom_r11_ph4a_heap_lease.rs`:
//! the real lease cannot run under loom (registry bootstrap), so the
//! production CAS orderings are mirrored exactly.
//!
//! Proven here:
//! - a publication made before, during, or after the owner's trim either gets
//!   consumed exactly once (by the trim or the claimant's drain) or survives
//!   pending for the next owner — it is never lost and never consumed twice;
//! - the CAS loser never drains: only the winner of the granting CAS may
//!   touch the core (`active` must always observe 0 on entry);
//! - every lease holder's core write happens-before the next holder's: the
//!   owner's Release `LIVE → FREE` pairs with the claimant's Acquire
//!   `FREE → LIVE`. The two lines marked `ghost-bind` carry the production
//!   orderings (rule R8 checks them against the R1 rules on
//!   `src/registry/heap_registry/claim.rs`); weakening either one (mutants
//!   L1/L2 of `scripts/ghost_mutants.mjs`) turns the `core` cell into a loom
//!   causality violation. The owner writes `core` after its own drain and the
//!   claimant before its drain, so the `pending` swaps add no edge between them.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;
use sefer_alloc::registry::heap_slot::SlotState;
use sefer_alloc::registry::heap_slot::SlotState::{Free, Live};

/// The single value a publication carries (the S1 happens-before payload).
const PUBLISHED: u64 = 42;

struct Slot {
    state: AtomicU8,
    /// Exactly-one-mutator guard: `fetch_add` must always observe 0.
    active: AtomicUsize,
    /// Sidecar ingress bits: bit 0 set == one pending publication.
    pending: AtomicUsize,
    /// Producer scratch — the payload carried by the publication.
    payload: UnsafeCell<u64>,
    /// The TORN stamp (`mark_local_torn`): TLS-local, no cross-thread duty.
    torn: AtomicBool,
    /// Test oracle: how many drains consumed a publication (0 or 1).
    consumed: AtomicUsize,
    /// Owner-private core metadata (the `HeapCore` stand-in): every lease
    /// holder mutates it, so only the S2 edge orders consecutive holders.
    core: UnsafeCell<u64>,
}

/// One state CAS (`HeapSlot::cas_state`).
fn cas(slot: &Slot, from: SlotState, to: SlotState, success: Ordering, failure: Ordering) -> bool {
    slot.state
        .compare_exchange(from as u8, to as u8, success, failure)
        .is_ok()
}

/// `claim_impl`'s FREE → LIVE grant; a loser gets no authority.
fn claim(slot: &Slot) -> bool {
    // ghost-bind: heap-lease/claim-free-live
    cas(slot, Free, Live, Ordering::AcqRel, Ordering::Acquire)
}

/// `HeapLease::drop`: Release-publish the holder's writes (S2), authority ends.
fn release(slot: &Slot) {
    // ghost-bind: heap-lease/drop-release
    let won = cas(slot, Live, Free, Ordering::Release, Ordering::Relaxed);
    assert!(won, "a lease holder's LIVE -> FREE CAS cannot lose");
}

/// A lease holder's core mutation (trim, drain bookkeeping).
fn touch_core(slot: &Slot) {
    slot.core.with_mut(|pointer| {
        // SAFETY: the lease holder is the only core mutator (S1); the previous
        // holder's writes happen-before via its Release drop and this Acquire claim.
        unsafe { *pointer += 1 };
    });
}

/// One owner-side bounded drain (`trim_for_recycle` / claimant's re-claim
/// trim): swap-consume the pending bits. `swap(0)` makes consumption unique —
/// exactly-once by construction, asserted by the `consumed` oracle.
fn drain_once(slot: &Slot) {
    let word = slot.pending.swap(0, Ordering::AcqRel);
    if word & 1 == 1 {
        let value = slot.payload.with(|pointer| {
            // SAFETY: the Acquire half of the swap above pairs with the
            // publisher's Release `fetch_or`, so the payload write
            // happens-before this read.
            unsafe { *pointer }
        });
        assert_eq!(value, PUBLISHED, "a consumed publication must be whole");
        assert_eq!(
            slot.consumed.fetch_add(1, Ordering::Relaxed),
            0,
            "a publication must be consumed exactly once"
        );
    }
}

/// Foreign producer: writes the payload, then Release-publishes the pending
/// bit. Independent of the slot state — sidecar publication never touches the
/// core, so it may interleave with trim, teardown and claim freely.
fn publisher(slot: &Slot) {
    slot.payload.with_mut(|pointer| {
        // SAFETY: the only payload writer; readers gate on the pending bit.
        unsafe { *pointer = PUBLISHED };
    });
    slot.pending.fetch_or(1, Ordering::AcqRel);
}

/// Owner: binds (FREE → LIVE), stamps TORN, runs one trim drain, trims its
/// core, then drops the lease — the Release `LIVE → FREE` CAS.
fn owner_binds_trims_and_recycles(slot: &Slot) {
    if !claim(slot) {
        return; // lost claimant CAS — no authority, no core access
    }
    // S1: only the CAS winner runs its body.
    assert_eq!(slot.active.fetch_add(1, Ordering::Relaxed), 0);
    // mark_local_torn: poisons this thread's TLS cache; no ordering duty.
    slot.torn.store(true, Ordering::Relaxed);
    drain_once(slot); // trim_for_recycle: terminal sidecar cut
    touch_core(slot); // trim_for_recycle: owner cache trim
    assert_eq!(slot.active.fetch_sub(1, Ordering::Relaxed), 1);
    release(slot);
}

/// Claimant: races for the recycled slot; on a win, mutates its core, runs
/// its bounded drain and drops its own lease. A CAS loser returns without
/// draining.
fn claimant_claims_and_drains(slot: &Slot) {
    if !claim(slot) {
        return; // the loser must never enter the drain
    }
    assert_eq!(slot.active.fetch_add(1, Ordering::Relaxed), 0);
    touch_core(slot);
    drain_once(slot);
    assert_eq!(slot.active.fetch_sub(1, Ordering::Relaxed), 1);
    release(slot);
}

#[test]
fn publication_survives_recycle_and_is_consumed_exactly_once() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let slot = Arc::new(Slot {
            state: AtomicU8::new(SlotState::Free as u8),
            active: AtomicUsize::new(0),
            pending: AtomicUsize::new(0),
            payload: UnsafeCell::new(0),
            torn: AtomicBool::new(false),
            consumed: AtomicUsize::new(0),
            core: UnsafeCell::new(0),
        });
        let publisher_slot = slot.clone();
        let producer = thread::spawn(move || publisher(&publisher_slot));
        let owner_slot = slot.clone();
        let owner = thread::spawn(move || owner_binds_trims_and_recycles(&owner_slot));
        let claimant_slot = slot.clone();
        let claimant = thread::spawn(move || claimant_claims_and_drains(&claimant_slot));
        producer.join().expect("publisher");
        owner.join().expect("owner");
        claimant.join().expect("claimant");

        // Every join observes the final Release publication.
        assert_eq!(slot.state.load(Ordering::Acquire), SlotState::Free as u8);
        assert_eq!(slot.active.load(Ordering::Relaxed), 0);
        // Exactly-once / never-lost: the publication was either consumed by
        // the trim or the claimant's drain, or it is still pending.
        let consumed = slot.consumed.load(Ordering::Relaxed);
        let pending = slot.pending.load(Ordering::Relaxed) & 1;
        assert_eq!(
            consumed + pending,
            1,
            "the publication must be consumed exactly once or survive pending"
        );
        // Every winner mutated the core exactly once; a loser never did.
        let holders = slot.core.with(|pointer| {
            // SAFETY: all threads joined; no lease holder remains.
            unsafe { *pointer }
        });
        assert!(
            (1..=2).contains(&holders),
            "core holders {holders} outside 1..=2"
        );
    });
}
