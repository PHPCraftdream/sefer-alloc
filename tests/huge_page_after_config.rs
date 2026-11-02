//! `LargeCacheConfig::huge_page_after_segments`: resolution of the threshold
//! (default, explicit values, `0` = never, out-of-range = never).
#![cfg(feature = "internals")]

use sefer_alloc::LargeCacheConfig;

fn resolved(requested: Option<usize>) -> usize {
    let cfg = match requested {
        Some(n) => LargeCacheConfig::new().huge_page_after_segments(n),
        None => LargeCacheConfig::new(),
    };
    cfg.dbg_resolved_huge_page_after_segments()
}

#[test]
fn default_is_the_sixteenth_segment() {
    assert_eq!(resolved(None), 16);
    assert_eq!(
        LargeCacheConfig::DEFAULT.dbg_resolved_huge_page_after_segments(),
        16
    );
}

#[test]
fn explicit_values_are_kept() {
    for n in [1usize, 2, 7, 16, 64, 1 << 20, u32::MAX as usize] {
        assert_eq!(resolved(Some(n)), n, "threshold {n}");
    }
}

#[test]
fn zero_means_never() {
    assert_eq!(resolved(Some(0)), 0);
}

#[test]
fn out_of_range_means_never() {
    assert_eq!(resolved(Some(u32::MAX as usize + 1)), 0);
    assert_eq!(resolved(Some(usize::MAX)), 0);
}
