#![allow(deprecated)]
//! R2-04 (independent src review round 2, task #2006) — the generation-setter
//! lockdown regression.
//!
//! Before this fix, `EpochRegion::_set_slot_generation_for_tests` /
//! `AtomicSlot::set_generation_for_tests` took `&self` and never checked
//! occupancy at all: the doc comment's "the caller MUST only target a
//! currently-vacant slot" was prose only, never enforced by the code. Taking
//! `&self` also meant the hook was reachable under plain `experimental` (no
//! `internals` needed) on ANY `&EpochRegion<T>` -- a call could hand an old,
//! already-stale handle CAS rights back, including mid-race with a
//! concurrent eviction.
//!
//! The fix does two independent things, both load-bearing:
//! 1. Changes the signature to `&mut self`, so the borrow checker proves
//!    EXCLUSIVE access to the whole region for the call -- this is a
//!    compile-time property (every existing call site in `tests/epoch.rs`
//!    and `tests/regression_r2_03_cross_instance_identity.rs` needed a
//!    `let mut` added, confirming the tightened signature is load-bearing,
//!    not just documentation).
//! 2. Adds a runtime vacancy check: the call now panics if the target slot
//!    currently holds a live value, turning the prose-only "vacant slots
//!    only" contract into an enforced one. This file exercises exactly that
//!    check.

#![cfg(feature = "experimental")]

use sefer_alloc::EpochRegion;

#[test]
#[should_panic(expected = "OCCUPIED")]
fn set_slot_generation_for_tests_panics_on_occupied_slot() {
    // Single-slot region: `insert` and `_set_slot_generation_for_tests(0,
    // ..)` are then UNAMBIGUOUSLY the same slot. `EpochRegion`'s free list
    // is LIFO (the first insert into a multi-slot region claims the HIGHEST
    // index, not index 0, despite a doc comment on `with_capacity` claiming
    // otherwise -- confirmed empirically; that stale comment is a separate,
    // out-of-scope defect for R2-23's doc/code sweep), so a capacity > 1
    // region cannot reliably target "the slot the next insert will claim"
    // by index 0 alone.
    let mut region: EpochRegion<&'static str> = EpochRegion::with_capacity(1);
    let _handle = region.insert("live").unwrap();

    // Slot 0 (the region's only slot) is occupied. Forcing its generation
    // must panic rather than silently desyncing the generation from the
    // installed value's true generation.
    region._set_slot_generation_for_tests(0, 42);
}

#[test]
fn set_slot_generation_for_tests_still_works_on_a_genuinely_vacant_slot() {
    let mut region: EpochRegion<&'static str> = EpochRegion::with_capacity(1);
    // Slot 0 was never installed -- genuinely vacant. This must NOT panic,
    // confirming the vacancy check does not over-reject the sanctioned use
    // case (the pre-existing `tests/epoch.rs` saturation-boundary test
    // exercises the same path end-to-end; this is a focused unit check).
    region._set_slot_generation_for_tests(0, u32::MAX - 1);
    let handle = region.insert("mine").unwrap();
    assert_eq!(region.get_with(handle, |v| *v), Some("mine"));
}
