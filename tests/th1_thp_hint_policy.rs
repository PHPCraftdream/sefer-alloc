//! TH-1: the transparent-huge-page hint goes to small segments from the 16th
//! small-segment reservation of the process on, never before. The counters
//! are process-global: one test in this binary.
#![cfg(all(
    feature = "alloc-global",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware"),
    not(miri)
))]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SeferAlloc;

static A: SeferAlloc = SeferAlloc::new();

#[test]
fn hint_starts_at_the_sixteenth_small_segment() {
    let l = Layout::from_size_align(1024, 8).unwrap();
    let mut v = Vec::new();
    // SAFETY: `l` is non-zero-sized; every block is freed below with `l`.
    let first = unsafe { A.alloc(l) };
    assert!(!first.is_null());
    v.push(first);
    let before = AllocCore::dbg_small_segments_reserved();
    assert!(before < 16, "a fresh process starts below the threshold");
    assert_eq!(
        AllocCore::dbg_thp_hinted_small_segments(),
        0,
        "no hint below 16"
    );
    // ~80 MiB of 1 KiB blocks: about 20 small segments.
    while AllocCore::dbg_small_segments_reserved() < 20 {
        // SAFETY: as above.
        let p = unsafe { A.alloc(l) };
        assert!(!p.is_null());
        // SAFETY: `p` holds 1024 bytes.
        unsafe { p.write(0x5A) };
        v.push(p);
        assert!(v.len() < 1 << 20, "20 small segments within 1 GiB");
    }
    let reserved = AllocCore::dbg_small_segments_reserved();
    assert_eq!(
        AllocCore::dbg_thp_hinted_small_segments(),
        reserved - 15,
        "every small segment from the 16th on is hinted"
    );
    for p in v {
        // SAFETY: allocated above with `l`, freed once.
        unsafe { A.dealloc(p, l) };
    }
}
