#![allow(deprecated)]
//! R2-03 (independent src review round 2, task #2005) — cross-instance
//! handle identity regression.
//!
//! Before this fix, `EpochHandle<T>`/`LockFreeHandle<T>` carried only an
//! `(index, generation)` pair — no identity of the *region* that minted
//! them. Two freshly-created regions of the same `T` routinely produce
//! numerically-identical handles (the first insert always claims index 0 at
//! generation 0), so a handle minted by region A was silently accepted by
//! region B's `get_with`/`remove`/`remote_evict`. Confirmed empirically
//! before the fix (not just asserted from reading the code): a handle from
//! one `EpochRegion` applied to a completely different, unrelated
//! `EpochRegion` (a) returned that OTHER region's real value out of
//! `get_with`, and (b) won `remove`'s generation CAS against a slot that was
//! NEVER installed (vacant at generation 0), reporting a phantom `true` and
//! underflowing the target region's `len()` to `usize::MAX`.
//!
//! The fix stamps a never-reused, monotonically-assigned `region_id` into
//! every handle at mint time and checks it FIRST, before any slot lookup or
//! CAS, in every operation that takes a handle. This file exercises the
//! full state matrix the review's acceptance criteria named — vacant,
//! occupied, and (for the epoch tier, where the review specifically flagged
//! "generation CAS can succeed on a VACANT slot") saturated target slots —
//! confirming a rejected cross-instance operation touches neither the
//! target's returned value/bool NOR its `len()`.
//!
//! ## Scope note: sharded and lock-free tiers
//!
//! `ShardedRegion<T>` is pure delegation to per-shard `EpochRegion<T>`
//! instances (see `sharded_region.rs`'s own module docs), so its
//! cross-instance protection is the SAME `region_id` check, inherited
//! automatically — this file proves the delegation reaches it (vacant +
//! occupied; the saturated case is already fully proven at the epoch-tier
//! level above, since it is the identical code path).
//!
//! `LockFreeRegion<T>`'s `remove`/`get` already explicitly matched on
//! `SlotState::Occupied`/`Vacant` before this fix (never inferred occupancy
//! from a generation match alone), so it never had the epoch tier's
//! specific "phantom removal on a vacant slot" defect class — only the
//! "resolves to a DIFFERENT region's real value" class, which this file's
//! vacant/occupied cases cover. There is no existing test-only hook to force
//! a `LockFreeRegion` slot to a saturated generation without ~4 billion real
//! reuse cycles (unlike `EpochRegion::_set_slot_generation_for_tests`), and
//! adding one solely for this one matrix cell would be scope creep for a
//! `#[deprecated]`, legacy-tier module — the saturated case is out of scope
//! here for that reason.

#![cfg(feature = "experimental")]

use sefer_alloc::{EpochRegion, LockFreeRegion, ShardedRegion};

// ---------------------------------------------------------------------------
// Epoch tier: full vacant / occupied / saturated matrix.
// ---------------------------------------------------------------------------

#[test]
fn epoch_cross_instance_vacant_target_is_rejected_without_state_change() {
    let region_b: EpochRegion<&'static str> = EpochRegion::with_capacity(4);
    // region_b: nothing ever inserted. Slot 0 is vacant at generation 0.
    let region_a: EpochRegion<&'static str> = EpochRegion::with_capacity(4);
    let handle_a = region_a.insert("mine").unwrap();

    assert_eq!(region_b.get_with(handle_a, |v| *v), None);
    assert!(!region_b.remove(handle_a));
    assert_eq!(
        region_b.len(),
        0,
        "len() must be untouched by a rejected cross-instance op"
    );
}

#[test]
fn epoch_cross_instance_occupied_target_is_rejected_without_state_change() {
    let region_b: EpochRegion<&'static str> = EpochRegion::with_capacity(4);
    let _handle_b = region_b.insert("victim").unwrap();
    let region_a: EpochRegion<&'static str> = EpochRegion::with_capacity(4);
    let handle_a = region_a.insert("mine").unwrap();

    assert_eq!(
        region_b.get_with(handle_a, |v| *v),
        None,
        "must not leak region_b's real value under region_a's handle"
    );
    assert!(!region_b.remove(handle_a));
    assert_eq!(
        region_b.len(),
        1,
        "region_b's real entry must remain live and untouched"
    );
    // region_b's own handle still resolves correctly (the fix did not break
    // legitimate same-instance resolution).
    assert_eq!(region_b.get_with(_handle_b, |v| *v), Some("victim"));
}

#[test]
fn epoch_cross_instance_saturated_target_is_rejected_without_state_change() {
    // Single-slot regions: `EpochRegion`'s free list is LIFO (the first
    // insert into a multi-slot region claims the HIGHEST index, not index
    // 0, despite a doc comment on `with_capacity` claiming otherwise --
    // confirmed empirically; that stale comment is a separate, out-of-scope
    // defect for R2-23's doc/code sweep), so a capacity > 1 region cannot
    // reliably target "the slot the next insert will claim" by index 0
    // alone. With capacity 1, `_set_slot_generation_for_tests(0, ..)` and
    // the following `insert` are UNAMBIGUOUSLY the same slot.
    let mut region_a: EpochRegion<&'static str> = EpochRegion::with_capacity(1);
    // Force slot 0 to the saturation generation BEFORE installing, then
    // insert: install() does not bump the generation, so the minted handle
    // carries generation == u32::MAX (the documented, sanctioned use of
    // this test-only hook — see its own doc comment). R2-04 (task #2006)
    // made this hook take &mut self, so this call needs exclusive access --
    // trivially available here since region_a has no other borrows yet.
    region_a._set_slot_generation_for_tests(0, u32::MAX);
    let handle_a = region_a.insert("attacker").unwrap();

    let mut region_b: EpochRegion<&'static str> = EpochRegion::with_capacity(1);
    // region_b: slot 0 vacant at generation u32::MAX too (a genuine
    // retired/saturated slot) -- the numeric (index, generation) match is
    // real, not coincidental, so only the region_id check can reject this.
    region_b._set_slot_generation_for_tests(0, u32::MAX);

    assert_eq!(region_b.get_with(handle_a, |v| *v), None);
    assert!(!region_b.remove(handle_a));
    assert_eq!(region_b.len(), 0);
}

// ---------------------------------------------------------------------------
// Sharded tier: delegation proof (vacant + occupied).
// ---------------------------------------------------------------------------

#[test]
fn sharded_cross_instance_vacant_target_is_rejected_without_state_change() {
    let region_b: ShardedRegion<&'static str> = ShardedRegion::with_shards(1, 4);
    let region_a: ShardedRegion<&'static str> = ShardedRegion::with_shards(1, 4);
    let handle_a = region_a.insert("mine").unwrap();

    assert_eq!(region_b.get_with(handle_a, |v| *v), None);
    assert!(!region_b.remove(handle_a));
    assert_eq!(region_b.len(), 0);
}

#[test]
fn sharded_cross_instance_occupied_target_is_rejected_without_state_change() {
    let region_b: ShardedRegion<&'static str> = ShardedRegion::with_shards(1, 4);
    let handle_b = region_b.insert("victim").unwrap();
    let region_a: ShardedRegion<&'static str> = ShardedRegion::with_shards(1, 4);
    let handle_a = region_a.insert("mine").unwrap();

    assert_eq!(region_b.get_with(handle_a, |v| *v), None);
    assert!(!region_b.remove(handle_a));
    assert_eq!(region_b.len(), 1);
    assert_eq!(region_b.get_with(handle_b, |v| *v), Some("victim"));
}

// ---------------------------------------------------------------------------
// Lock-free tier: vacant + occupied (see module doc for saturated-case
// scoping).
// ---------------------------------------------------------------------------

#[test]
fn lock_free_cross_instance_vacant_target_is_rejected_without_state_change() {
    let region_b: LockFreeRegion<&'static str> = LockFreeRegion::new();
    let region_a: LockFreeRegion<&'static str> = LockFreeRegion::new();
    let handle_a = region_a.insert("mine");

    assert_eq!(region_b.get(handle_a), None);
    assert!(!region_b.contains(handle_a));
    assert_eq!(region_b.remove(handle_a), None);
    assert_eq!(region_b.len(), 0);
}

#[test]
fn lock_free_cross_instance_occupied_target_is_rejected_without_state_change() {
    let region_b: LockFreeRegion<&'static str> = LockFreeRegion::new();
    let handle_b = region_b.insert("victim");
    let region_a: LockFreeRegion<&'static str> = LockFreeRegion::new();
    let handle_a = region_a.insert("mine");

    assert_eq!(
        region_b.get(handle_a),
        None,
        "must not leak region_b's real value under region_a's handle"
    );
    assert_eq!(region_b.remove(handle_a), None);
    assert_eq!(region_b.len(), 1);
    assert_eq!(region_b.get(handle_b).as_deref(), Some(&"victim"));
}
