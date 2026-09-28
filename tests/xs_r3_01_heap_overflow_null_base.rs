//! P3-2 (`docs/reviews/2026-09-28-232143-src-review-xs-sol-round-3.md`) —
//! `HeapOverflow::push`/`push_uncounted` must reject a null `base` in EVERY
//! build, not just under `debug_assertions`.
//!
//! ## Background
//!
//! `push_impl`'s only null-`base` guard used to be `debug_assert_ne!`, a
//! no-op in release builds. `push(null_mut(), packed)` would then win the
//! tail CAS and publish `ENTRY_EMPTY_BASE` (`null`) into a real ring slot.
//! `try_drain` uses that exact sentinel as its "reserved but not yet
//! published" stop condition (`src/registry/heap_overflow/drain.rs`), so a
//! published null permanently wedges the ring at that slot: every later
//! valid entry, from any producer, becomes unreachable forever.
//!
//! `HeapOverflow::new_boxed_for_test` (`internals`-gated) makes this
//! reachable through a fully safe API — see that constructor's doc comment.
//!
//! This file proves: (1) a null push returns `false` and leaves the ring's
//! cursors and slot layout untouched — no wedge; (2) subsequent valid
//! pushes still succeed and drain in full; (3) same for `push_uncounted`.
//!
//! Counterfactual (see the task report for the exact commands/output):
//! restoring the old `debug_assert_ne!`-only body makes
//! `null_push_does_not_wedge_the_ring` and
//! `null_push_uncounted_does_not_wedge_the_ring` fail in BOTH debug and
//! `--release` (release: the null base is actually published and the drain
//! after it sees zero of the two real entries that follow; debug: the
//! `debug_assert_ne!` panics before any assertion in this file even runs).

#![cfg(all(
    all(feature = "alloc-global", feature = "alloc-xthread"),
    feature = "internals"
))]

use sefer_alloc::registry::heap_overflow::HeapOverflow;

/// Synthetic, never-dereferenced "segment base" — same construction as
/// `tests/miri_heap_overflow_unit.rs::synthetic_base`. `HeapOverflow` never
/// reads through `base`, only compares/stores it, so an arbitrary non-null
/// address is a sound stand-in for a real segment base here.
fn synthetic_base(tag: usize) -> *mut u8 {
    core::ptr::without_provenance_mut((tag + 1) * 64)
}

#[test]
fn null_push_does_not_wedge_the_ring() {
    let ring = HeapOverflow::new_boxed_for_test();
    let (head_before, tail_before) = ring.cursors_for_test();

    assert!(
        !ring.push(core::ptr::null_mut(), 0xDEAD_BEEF),
        "push(null, ..) must return false, never true"
    );

    let (head_after, tail_after) = ring.cursors_for_test();
    assert_eq!(
        (head_before, tail_before),
        (head_after, tail_after),
        "a rejected null push must not reserve a cursor (tail unchanged) or \
         touch head"
    );

    // Valid pushes after the rejected null must still succeed and drain in
    // full — the null must never occupy or wedge a slot.
    for i in 0..8usize {
        assert!(
            ring.push(synthetic_base(i), i as u32),
            "valid push after a rejected null push must still succeed"
        );
    }
    let mut count = 0u32;
    ring.try_drain(|_base, _packed| count += 1)
        .expect("drain must not be busy on this single-threaded test ring");
    assert_eq!(
        count, 8,
        "all 8 valid entries must drain; a wedge would stop the drain early \
         (at 0, if the null had been published as the very first slot)"
    );
}

#[test]
fn null_push_uncounted_does_not_wedge_the_ring() {
    let ring = HeapOverflow::new_boxed_for_test();
    let (head_before, tail_before) = ring.cursors_for_test();

    assert!(
        !ring.push_uncounted(core::ptr::null_mut(), 0xDEAD_BEEF),
        "push_uncounted(null, ..) must return false, never true"
    );

    let (head_after, tail_after) = ring.cursors_for_test();
    assert_eq!(
        (head_before, tail_before),
        (head_after, tail_after),
        "a rejected null push_uncounted must not reserve a cursor"
    );

    for i in 0..8usize {
        assert!(
            ring.push_uncounted(synthetic_base(i), i as u32),
            "valid push_uncounted after a rejected null push must still succeed"
        );
    }
    let mut count = 0u32;
    ring.try_drain(|_base, _packed| count += 1)
        .expect("drain must not be busy on this single-threaded test ring");
    assert_eq!(count, 8, "all 8 valid entries must drain with no wedge");
}
