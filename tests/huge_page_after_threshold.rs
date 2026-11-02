//! `huge_page_after_segments(4)`: the transparent-huge-page hint goes to the
//! small segments from the 4th reservation of the process on, never before.
//! The counters and the threshold are process-global: one test in this binary.
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

const AFTER: u64 = 4;

static A: SeferAlloc = SeferAlloc::with_config(LargeCacheConfig::new().huge_page_after_segments(4));

#[test]
fn hint_starts_at_the_configured_segment() {
    let l = Layout::from_size_align(1024, 8).unwrap();
    let mut v = Vec::new();
    // ~40 MiB of 1 KiB blocks: about 10 small segments.
    while AllocCore::dbg_small_segments_reserved() < 10 {
        // SAFETY: `l` is non-zero-sized; every block is freed below with `l`.
        let p = unsafe { A.alloc(l) };
        assert!(!p.is_null());
        // SAFETY: `p` holds 1024 bytes.
        unsafe { p.write(0x5A) };
        v.push(p);
        assert!(v.len() < 1 << 20, "10 small segments within 1 GiB");
        let reserved = AllocCore::dbg_small_segments_reserved();
        if reserved < AFTER {
            assert_eq!(
                AllocCore::dbg_thp_hinted_small_segments(),
                0,
                "no hint below the configured segment"
            );
        }
    }
    assert_eq!(
        AllocCore::dbg_huge_page_after_segments(),
        AFTER,
        "the resolved threshold is the configured one"
    );
    let reserved = AllocCore::dbg_small_segments_reserved();
    assert_eq!(
        AllocCore::dbg_thp_hinted_small_segments(),
        reserved - (AFTER - 1),
        "every small segment from the configured one on is hinted"
    );
    for p in v {
        // SAFETY: allocated above with `l`, freed once.
        unsafe { A.dealloc(p, l) };
    }
}
