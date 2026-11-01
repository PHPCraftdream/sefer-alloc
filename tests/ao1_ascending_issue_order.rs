//! AO-1: a run of same-class allocations carved from fresh segment space is
//! issued in ascending address order (the magazine is refilled reversed, so
//! its top-first pops walk the carve upward). Before AO-1 each 16-block
//! refill came out descending: 15 of every 16 same-segment steps went down.
#![cfg(all(feature = "alloc-global", feature = "fastbin", not(miri)))]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::{SeferAlloc, SegmentLayout};

// Not the global allocator: the harness and `Vec` stay on the system heap,
// so nothing else allocates through this thread's heap.
static A: SeferAlloc = SeferAlloc::new();

#[test]
fn fresh_carve_run_is_issued_ascending() {
    let l = Layout::from_size_align(208, 16).unwrap();
    let n = 256;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: `l` is non-zero-sized; every block is freed below with `l`.
        let p = unsafe { A.alloc(l) };
        assert!(!p.is_null());
        v.push(p as usize);
    }
    let seg = |p: usize| p & !(SegmentLayout::SEGMENT - 1);
    let same: Vec<_> = v.windows(2).filter(|w| seg(w[0]) == seg(w[1])).collect();
    assert!(
        same.len() >= n - 3,
        "only {} same-segment steps",
        same.len()
    );
    let non_increasing = same.iter().filter(|w| w[1] <= w[0]).count();
    assert_eq!(
        non_increasing,
        0,
        "{non_increasing} of {} same-segment steps do not increase: {v:x?}",
        same.len()
    );
    for p in v {
        // SAFETY: `p` came from `A.alloc(l)` above and is freed once.
        unsafe { A.dealloc(p as *mut u8, l) };
    }
}
