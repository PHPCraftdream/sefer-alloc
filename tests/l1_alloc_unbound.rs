//! L-1: a thread's first `alloc`/`alloc_zeroed` runs through the out-of-line
//! `alloc_unbound` (bind, then allocate). Oracle: a worker dirties and frees
//! a batch, exits, and a fresh thread's very first call — `alloc_zeroed` on
//! the recycled slot — must hand back one of those dirty blocks zeroed. If
//! `alloc_unbound` dropped the `zeroed` flag, the reused block would come
//! back dirty.
#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;
use std::thread;

use sefer_alloc::SeferAlloc;

static ALLOC: SeferAlloc = SeferAlloc::new();

const N: usize = 256;

fn layout() -> Layout {
    Layout::from_size_align(64, 8).unwrap()
}

/// Dirty and free `N` blocks on a worker that then exits; returns their addresses.
fn dirty_and_exit() -> HashSet<usize> {
    thread::spawn(|| {
        let mut seen = HashSet::new();
        let mut blocks = Vec::with_capacity(N);
        // SAFETY: valid nonzero layout; each block is written within its size
        // and freed exactly once with the same layout.
        unsafe {
            for _ in 0..N {
                let p = ALLOC.alloc(layout());
                assert!(!p.is_null());
                p.write_bytes(0xAA, layout().size());
                blocks.push(p);
            }
            for &p in &blocks {
                seen.insert(p as usize);
                ALLOC.dealloc(p, layout());
            }
        }
        seen
    })
    .join()
    .expect("worker panicked")
}

#[test]
fn first_calls_bind_through_alloc_unbound() {
    // Sequential in one test: a concurrent test thread could claim the slot.
    let dirty = dirty_and_exit();
    thread::spawn(move || {
        // SAFETY: valid nonzero layout; each block is read or written within
        // its size and freed once with the same layout.
        unsafe {
            let p = ALLOC.alloc_zeroed(layout());
            assert!(!p.is_null());
            assert!(
                dirty.contains(&(p as usize)),
                "the first call must reuse a block the exited worker dirtied"
            );
            for i in 0..layout().size() {
                assert_eq!(p.add(i).read(), 0, "nonzero byte at {i}");
            }
            ALLOC.dealloc(p, layout());
        }
    })
    .join()
    .expect("reader panicked");

    thread::spawn(|| {
        // SAFETY: as above.
        unsafe {
            let a = ALLOC.alloc(layout());
            let b = ALLOC.alloc(layout());
            assert!(!a.is_null() && !b.is_null() && a != b);
            ALLOC.dealloc(a, layout());
            ALLOC.dealloc(b, layout());
            assert_eq!(ALLOC.alloc(layout()), b, "the magazine reissues LIFO");
            ALLOC.dealloc(b, layout());
        }
    })
    .join()
    .expect("worker panicked");
}
