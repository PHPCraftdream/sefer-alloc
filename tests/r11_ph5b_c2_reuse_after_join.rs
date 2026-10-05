//! Cross-thread allocation lifetime smoke test: a worker writes and frees a
//! block, exits, and the owner allocates again and checks the returned contents.
//!
//! This checks the zero-initialization contract and allocator correctness after
//! `join`; it does not assert that the second allocation has the same virtual
//! address or proves that storage was reused.

#![cfg(feature = "alloc-global")]

use std::alloc::{GlobalAlloc, Layout};
use std::thread;

use sefer_alloc::SeferAlloc;

static ALLOC: SeferAlloc = SeferAlloc::new();

const SMALL: Layout = match Layout::from_size_align(512, 8) {
    Ok(layout) => layout,
    Err(_) => panic!("invalid small layout"),
};
const LARGE: Layout = match Layout::from_size_align(2 * 1024 * 1024, 8) {
    Ok(layout) => layout,
    Err(_) => panic!("invalid large layout"),
};

fn exercise(layout: Layout, pattern: u8) {
    let worker = thread::spawn(move || {
        // SAFETY: the allocation uses a valid nonzero layout and remains owned
        // by this worker until its matching deallocation.
        unsafe {
            let ptr = ALLOC.alloc(layout);
            assert!(!ptr.is_null(), "worker allocation failed");
            ptr.write_bytes(pattern, layout.size());
            ALLOC.dealloc(ptr, layout);
        }
    });
    worker.join().expect("worker panicked");

    // SAFETY: valid layout; owner inspects the fresh allocation before freeing
    // it exactly once with the same layout.
    unsafe {
        let ptr = ALLOC.alloc_zeroed(layout);
        assert!(!ptr.is_null(), "owner allocation after join failed");
        for index in 0..layout.size() {
            assert_eq!(ptr.add(index).read(), 0, "nonzero byte at {index}");
        }
        ALLOC.dealloc(ptr, layout);
    }
}

#[test]
fn allocation_after_worker_join_has_clean_contents() {
    exercise(SMALL, 0x5a);
    exercise(LARGE, 0xa5);
}
