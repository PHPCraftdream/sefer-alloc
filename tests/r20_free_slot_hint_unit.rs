//! Local-instance tests of the production FREE-slot bitmap (no global state).
#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![forbid(unsafe_code)]
#![allow(unused_imports, dead_code)]

const MAX_HEAPS: usize = 4096;
// The mounted hint files take their atomics from `super`.
use core::sync::atomic::{AtomicU64, Ordering};

#[allow(dead_code, unused_imports)]
#[path = "../src/registry/bootstrap/free_slot_hint.rs"]
mod real_free_slot_hint;
use real_free_slot_hint::FreeSlotHint;

#[test]
fn empty_and_publish_take_round_trip() {
    let set = FreeSlotHint::new();
    assert_eq!(set.take_nonempty(), None);
    set.publish(17);
    assert_eq!(set.take_nonempty(), Some(17));
    assert_eq!(set.take_nonempty(), None);
}

#[test]
fn take_selects_lowest_index_across_bitmap_words() {
    let set = FreeSlotHint::new();
    for index in [4095, 64, 63, 0, 2048] {
        set.publish(index);
    }
    for index in [0, 63, 64, 2048, 4095] {
        assert_eq!(set.take_nonempty(), Some(index));
    }
    assert_eq!(set.take_nonempty(), None);
}

#[test]
fn publication_is_idempotent_and_forget_preserves_neighbours() {
    let set = FreeSlotHint::new();
    for _ in 0..64 {
        set.publish(99);
    }
    assert_eq!(set.take_nonempty(), Some(99));
    assert_eq!(set.take_nonempty(), None);
    for index in [62, 63, 64, 65] {
        set.publish(index);
    }
    set.forget(63);
    set.forget(64);
    assert_eq!(set.take_nonempty(), Some(62));
    assert_eq!(set.take_nonempty(), Some(65));
    assert_eq!(set.take_nonempty(), None);
}

#[test]
fn all_supported_indices_are_returned_once() {
    let set = FreeSlotHint::new();
    for index in (0..MAX_HEAPS).rev() {
        set.publish(index);
    }
    for index in 0..MAX_HEAPS {
        assert_eq!(set.take_nonempty(), Some(index));
    }
    assert_eq!(set.take_nonempty(), None);
}
