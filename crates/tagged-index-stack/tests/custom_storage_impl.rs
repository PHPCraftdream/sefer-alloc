//! [`StackStorage`] is documented as "intentionally OPEN to external
//! implementation" (slot-resident links in caller-owned storage is the whole
//! design point), and the crate blanket-implements [`StackOps`] for every
//! `S: StackStorage<B> + ?Sized` — precisely so `&dyn StackStorage` works.
//! Every other test in this crate exercises only [`ArrayIndexStack`], so
//! neither claim was ever exercised by a real second implementor. This file
//! pins both: a small `Vec`-backed `StackStorage` impl used directly, and the
//! same storage driven through `&dyn StackStorage<16>` to pin the blanket
//! impl's `?Sized` coverage.

#![cfg(not(loom))]

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{StackHead, StackOps, StackStorage};

/// A `Vec`-backed [`StackStorage`] implementation, deliberately NOT
/// [`ArrayLinks`]: heap-allocated rather than an owned array, otherwise
/// upholding the same ordering contract (`Acquire` load, `Release` store).
struct VecStorage {
    head: StackHead<16>,
    next: Vec<AtomicU32>,
}

impl VecStorage {
    fn new(n: usize) -> Self {
        Self {
            head: StackHead::new(),
            next: (0..n).map(|_| AtomicU32::new(0)).collect(),
        }
    }
}

impl StackStorage<16> for VecStorage {
    fn head(&self) -> &StackHead<16> {
        &self.head
    }

    fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

/// An external, non-`ArrayLinks` [`StackStorage`] implementor works exactly
/// like the owned-array type does: push/pop round-trips and LIFO order hold.
#[test]
fn vec_backed_storage_push_pop_round_trips() {
    let storage = VecStorage::new(8);

    for i in 0..4u32 {
        storage.push_index(i);
    }
    let mut got = Vec::new();
    while let Some(i) = storage.pop_index() {
        got.push(i);
    }
    assert_eq!(
        got,
        vec![3, 2, 1, 0],
        "LIFO order over a non-ArrayLinks storage"
    );
    assert_eq!(storage.pop_index(), None);
}

/// The [`StackOps`] blanket impl covers `S: StackStorage<B> + ?Sized`, so
/// `&dyn StackStorage<16>` must compile and behave identically — this pins
/// that coverage of unsized implementors, part of the frozen 0.1.0 surface.
#[test]
fn push_pop_through_dyn_storage() {
    let vec_storage = VecStorage::new(4);
    let storage_dyn: &dyn StackStorage<16> = &vec_storage;

    storage_dyn.push_index(2);
    assert_eq!(storage_dyn.pop_index(), Some(2));
    assert_eq!(storage_dyn.pop_index(), None);
}
