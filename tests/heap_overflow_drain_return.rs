//! R2-4 regression: [`HeapOverflow::drain`] must return its ACTUAL stop
//! position (the `head` value it published), NOT the entry-time `tail`
//! snapshot.
//!
//! When `drain` stops early at a reserved-but-not-yet-published slot (a
//! producer won its tail CAS but hasn't stored `base` yet), `h < t` at the
//! break. The pre-fix code returned `t` (the stale tail snapshot); the caller
//! (`HeapCore::drain_heap_overflow`) cached that as `overflow_tail_cache`, and
//! since `tail` does not advance again when the producer merely completes its
//! publish store, [`HeapOverflow::is_likely_empty`]`(cache == t)` then
//! returned `true` forever — a pending cross-heap free got stuck until an
//! unrelated later push incidentally moved `tail`.
//!
//! This file deterministically reproduces that half-published state on a
//! SINGLE thread via [`HeapOverflow::dbg_reserve_unpublished_for_test`] (the
//! real `push` completes both the CAS and the publish before returning, so the
//! half-published state is otherwise unreachable from the public API) and
//! asserts `drain` returns the stop position, not the stale snapshot.
//!
//! The full concurrent guard/stuck-free protocol — where the `drain` races a
//! real producer across the reserve/publish gap and the consequence (a stuck
//! free) manifests through the `is_likely_empty` cache — is model-checked
//! across interleavings in `tests/loom_heap_overflow_drain_guard.rs`.

#![cfg(feature = "alloc-xthread")]

use sefer_alloc::registry::heap_overflow::HeapOverflow;

/// Synthetic, never-dereferenced non-null "segment base". `HeapOverflow` only
/// ever stores/compares these as `usize` (never reads through them — see its
/// module doc's "no block-byte writes" discipline), so any non-zero value is a
/// valid base for this ring's contract (`ENTRY_EMPTY_BASE == 0`).
fn synthetic_base(tag: usize) -> *mut u8 {
    core::ptr::without_provenance_mut((tag + 1) * 64)
}

/// Fully-drained case (no unpublished slot): `drain` returns the final head,
/// which EQUALS the tail snapshot — both `h` and `t` agree, so this case is
/// NOT the R2-4 discriminator but pins the return-value contract for the
/// common path (regression guard against a future change that returns
/// something other than the stop position).
#[test]
fn drain_fully_drained_returns_stop_position_equal_to_tail() {
    let ring = HeapOverflow::new_boxed_for_test();
    assert!(ring.push(synthetic_base(0), 111));
    assert!(ring.push(synthetic_base(1), 222));

    let mut reclaimed = 0u32;
    let ret = ring.drain(|_base, _packed| {
        reclaimed += 1;
    });

    assert_eq!(reclaimed, 2, "both published entries must be reclaimed");
    assert_eq!(
        ret, 2,
        "fully-drained: return == final head == tail (both cursors at 2)"
    );
}

/// R2-4 discriminator. With one published entry (slot 0) followed by a
/// reserved-but-NOT-published slot (slot 1, injected by
/// [`HeapOverflow::dbg_reserve_unpublished_for_test`]), `drain` reclaims ONLY
/// slot 0 and stops at slot 1. It MUST return `h = 1` (the actual stop
/// position), NOT `t = 2` (the stale tail snapshot read at entry).
///
/// Counterfactual (RED before the fix): the pre-fix `return t` yields `ret =
/// 2` here, failing the `== 1` assertion. The downstream consequence (the
/// stuck free through the `is_likely_empty` cache) is model-checked in
/// `loom_heap_overflow_drain_guard.rs`.
#[test]
fn drain_stopping_at_unpublished_slot_returns_stop_position_not_tail() {
    let ring = HeapOverflow::new_boxed_for_test();
    // Slot 0: fully published. tail: 0 -> 1.
    assert!(ring.push(synthetic_base(0), 111));
    // Slot 1: reserved but NOT published (the mid-push window). tail: 1 -> 2;
    // bases[1] stays ENTRY_EMPTY_BASE, exactly as if a producer won the tail
    // CAS and was preempted before its base publish store.
    ring.dbg_reserve_unpublished_for_test();

    let mut reclaimed = 0u32;
    let ret = ring.drain(|_base, _packed| {
        reclaimed += 1;
    });

    assert_eq!(
        reclaimed, 1,
        "only the one published entry (slot 0) may be reclaimed; slot 1 is \
         still unpublished"
    );
    // R2-4: the fix returns the actual stop position h = 1. The pre-fix code
    // returned the stale tail snapshot t = 2, which (once cached by the caller
    // as overflow_tail_cache) made `is_likely_empty` skip every subsequent
    // re-drain — the pending free stuck until an unrelated push moved tail.
    assert_eq!(
        ret, 1,
        "drain must return the actual stop position (h = 1), not the stale \
         tail snapshot (t = 2) — R2-4"
    );
}

/// Second drain after an early stop reclaims nothing extra while the slot
/// remains unpublished, and returns the SAME stop position — confirming the
/// fix is stable across repeated drains (the cursor genuinely parks at the
/// gap, it does not "catch up" to `t` on a second call).
#[test]
fn second_drain_after_early_stop_parks_at_the_gap() {
    let ring = HeapOverflow::new_boxed_for_test();
    assert!(ring.push(synthetic_base(0), 111));
    ring.dbg_reserve_unpublished_for_test();

    let mut first = 0u32;
    let ret1 = ring.drain(|_, _| {
        first += 1;
    });
    assert_eq!((first, ret1), (1, 1));

    // The reserved slot 1 is STILL unpublished, so a second drain must again
    // stop at h = 1 and reclaim nothing new.
    let mut second = 0u32;
    let ret2 = ring.drain(|_, _| {
        second += 1;
    });
    assert_eq!(
        (second, ret2),
        (0, 1),
        "second drain must park at the same gap (h = 1) while slot 1 stays \
         unpublished — not advance the return toward the stale tail"
    );
}
