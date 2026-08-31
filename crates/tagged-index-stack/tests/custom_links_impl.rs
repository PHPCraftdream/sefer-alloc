//! [`Links`] is documented as "intentionally OPEN to external implementation"
//! (slot-resident links in caller-owned storage is the whole design point),
//! and `push`/`pop` both carry a `?Sized` bound precisely so `&dyn Links`
//! works. Every other test in this crate exercises only [`ArrayLinks`], so
//! neither claim was ever exercised by a real second implementor. This file
//! pins both: a small `Vec`-backed `Links` impl used directly, and the same
//! backing driven through `&dyn Links` to pin object-safety.

#![cfg(not(loom))]

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{Links, TaggedIndexStack};

/// A `Vec`-backed [`Links`] implementation, deliberately NOT [`ArrayLinks`]:
/// heap-allocated rather than an owned array, otherwise upholding the same
/// ordering contract (`Acquire` load, `Release` store).
struct VecLinks {
    next: Vec<AtomicU32>,
}

impl VecLinks {
    fn new(n: usize) -> Self {
        Self {
            next: (0..n).map(|_| AtomicU32::new(0)).collect(),
        }
    }
}

impl Links for VecLinks {
    fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

/// An external, non-`ArrayLinks` [`Links`] implementor works with
/// `TaggedIndexStack` exactly like the owned-array backing does: push/pop
/// round-trips and LIFO order hold.
#[test]
fn vec_backed_links_push_pop_round_trips() {
    let links = VecLinks::new(8);
    let stack = TaggedIndexStack::<16>::new();

    for i in 0..4u32 {
        stack.push(&links, i);
    }
    let mut got = Vec::new();
    while let Some(i) = stack.pop(&links) {
        got.push(i);
    }
    assert_eq!(
        got,
        vec![3, 2, 1, 0],
        "LIFO order over a non-ArrayLinks backing"
    );
    assert_eq!(stack.pop(&links), None);
}

/// `push`/`pop` are generic over `L: Links + ?Sized`, so `&dyn Links` must
/// compile and behave identically -- this pins object-safety, part of the
/// frozen 0.1.0 surface.
#[test]
fn push_pop_through_dyn_links() {
    let backing = VecLinks::new(4);
    let dyn_links: &dyn Links = &backing;
    let stack = TaggedIndexStack::<16>::new();

    stack.push(dyn_links, 2);
    assert_eq!(stack.pop(dyn_links), Some(2));
    assert_eq!(stack.pop(dyn_links), None);
}
