//! AO-1: a magazine refill's fresh carve run is issued in ascending address
//! order (`try_carve_batch` writes a MAG run top-down, the magazine pops
//! from the top). Before AO-1 each 16-block refill came out descending: 15
//! of every 16 same-segment steps went down. Reuse stays LIFO.
#![cfg(all(feature = "alloc-global", feature = "fastbin", not(miri)))]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::{SeferAlloc, SegmentLayout};

// The only test in this binary, on a private (not global) instance: the
// harness and `Vec` stay on the system heap, so nothing else allocates
// through this thread's heap and every refill of the class is a fresh carve.
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
    let steps: Vec<isize> = v
        .windows(2)
        .filter(|w| seg(w[0]) == seg(w[1]))
        .map(|w| w[1] as isize - w[0] as isize)
        .collect();
    assert!(
        steps.len() >= n - 3,
        "only {} same-segment steps",
        steps.len()
    );
    let block = steps[0];
    assert!(block >= 208, "first step {block}");
    let off = steps.iter().filter(|&&d| d != block).count();
    assert_eq!(
        off,
        0,
        "{off} of {} same-segment steps differ from +{block}: {v:x?}",
        steps.len()
    );

    // Reuse is unchanged: the block freed last is the next one issued.
    let last = v.pop().unwrap();
    // SAFETY: `last` came from `A.alloc(l)` and is freed once here.
    unsafe { A.dealloc(last as *mut u8, l) };
    // SAFETY: as above.
    let again = unsafe { A.alloc(l) };
    assert_eq!(again as usize, last, "LIFO reuse of the last freed block");
    v.push(again as usize);

    for p in v {
        // SAFETY: `p` came from `A.alloc(l)` above and is freed once.
        unsafe { A.dealloc(p as *mut u8, l) };
    }
}
