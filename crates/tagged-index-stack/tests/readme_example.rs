//! Mirrors README.md's `## Example` section, including its slot-resident
//! `StackStorage` implementation, so the public example's imports, unsafe impl,
//! and operations compile and run in CI.

#![cfg(not(loom))]

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{ArrayIndexStack, StackHead, StackOps as _, StackStorage, TAIL};

struct SlotStorage {
    head: StackHead<16>,
    links: [AtomicU32; 8],
}
impl SlotStorage {
    fn new() -> Self {
        Self {
            head: StackHead::new(),
            links: [const { AtomicU32::new(TAIL) }; 8],
        }
    }
}

// SAFETY: one private head has one stable backing; each index in 0..8 has a
// dedicated atomic link cell with Acquire/Release access; only this binding's
// stack algorithm mutates those cells under valid publish/recycle authority;
// and callers provide disjoint authority for the in-domain indices.
unsafe impl StackStorage<16> for SlotStorage {
    unsafe fn head(&self) -> &StackHead<16> {
        &self.head
    }

    unsafe fn load_next(&self, index: u32) -> u32 {
        self.links[index as usize].load(Ordering::Acquire)
    }

    unsafe fn store_next(&self, index: u32, next: u32) {
        self.links[index as usize].store(next, Ordering::Release);
    }
}

#[test]
fn owned_stack_readme_example_compiles_and_runs() {
    let stack = ArrayIndexStack::<16, 1024>::new();

    // SAFETY: index 7 is in-domain, fresh, and published exactly once.
    unsafe { stack.push(7) }.expect("fresh head has tag budget");
    assert_eq!(stack.pop(), Some(7));
    assert_eq!(stack.pop(), None);
}

#[test]
fn slot_resident_readme_example_compiles_and_runs() {
    let storage = SlotStorage::new();
    for index in 0..4 {
        // SAFETY: each index is in 0..8, fresh, and published exactly once.
        unsafe { storage.push_index(index) }.expect("fresh head has tag budget");
    }

    assert_eq!(storage.pop_index(), Some(3));
    assert_eq!(storage.pop_index(), Some(2));
    assert_eq!(storage.pop_index(), Some(1));
    assert_eq!(storage.pop_index(), Some(0));
    assert_eq!(storage.pop_index(), None);
}
