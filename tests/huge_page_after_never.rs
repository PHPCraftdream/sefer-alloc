//! `huge_page_after_segments(0)`: the transparent-huge-page hint is never
//! issued, however many small segments the process reserves. The counters and
//! the threshold are process-global: one test in this binary.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware"),
    not(miri)
))]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

static A: SeferAlloc = SeferAlloc::with_config(LargeCacheConfig::new().huge_page_after_segments(0));

#[test]
fn zero_never_hints() {
    let l = Layout::from_size_align(1024, 8).unwrap();
    let mut v = Vec::new();
    // ~80 MiB of 1 KiB blocks: past the default threshold of 16 segments.
    while AllocCore::dbg_small_segments_reserved() < 20 {
        // SAFETY: `l` is non-zero-sized; every block is freed below with `l`.
        let p = unsafe { A.alloc(l) };
        assert!(!p.is_null());
        // SAFETY: `p` holds 1024 bytes.
        unsafe { p.write(0x5A) };
        v.push(p);
        assert!(v.len() < 1 << 20, "20 small segments within 1 GiB");
    }
    assert!(AllocCore::dbg_small_segments_reserved() >= 20);
    assert_eq!(AllocCore::dbg_huge_page_after_segments(), 0);
    assert_eq!(
        AllocCore::dbg_thp_hinted_small_segments(),
        0,
        "threshold 0 never hints, even past the default threshold"
    );
    for p in v {
        // SAFETY: allocated above with `l`, freed once.
        unsafe { A.dealloc(p, l) };
    }
}
