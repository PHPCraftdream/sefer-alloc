//! `LargeThpRequestGroup::index` is the stable `0..=5` group index and agrees
//! with `of_bytes` at every bucket boundary.
#![cfg(feature = "alloc-decommit")]

use sefer_policy::policy::large_thp::LargeThpRequestGroup as G;

#[test]
fn index_matches_of_bytes_at_every_boundary() {
    let boundaries = [G::B1_MIB, G::B8_MIB, G::B32_MIB, G::B128_MIB, G::B512_MIB];
    assert_eq!(G::of_bytes(0).index(), 0);
    for (k, b) in boundaries.into_iter().enumerate() {
        assert_eq!(G::of_bytes(b - 1).index(), k);
        assert_eq!(G::of_bytes(b).index(), k + 1);
    }
    assert_eq!(G::of_bytes(u64::MAX).index(), 5);
}

#[test]
fn index_values_are_distinct_and_cover_zero_to_five() {
    let all = [
        G::Under1MiB,
        G::MiB1To8,
        G::MiB8To32,
        G::MiB32To128,
        G::MiB128To512,
        G::Over512MiB,
    ];
    for (i, g) in all.into_iter().enumerate() {
        assert_eq!(g.index(), i);
    }
}
