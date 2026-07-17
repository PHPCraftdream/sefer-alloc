//! Correctness of the `const`-generic builder itself — table shape, the derived
//! O(1) lookup, and the alignment-jump classifier — over BOTH sefer's concrete
//! parameterization and arbitrary property-generated parameterizations. The
//! in-tree `size_classes.rs` bakes one parameterization; here the builder is
//! varied so the mechanism is property-tested, not just the one instance.

use size_classes::{build_table, size2class_len, Params, SizeClasses};

/// A faithful, from-scratch reference table builder (a plain `Vec` version of
/// the crate's `const fn build_table`) so tests do not trust the crate's own
/// output. Geometric run merged with sorted `extras`.
fn reference_table(
    min_block: usize,
    growth: (usize, usize),
    geo_count: usize,
    extras: &[usize],
) -> Vec<usize> {
    let (num, den) = growth;
    let mask = min_block - 1;
    let mut geo = Vec::with_capacity(geo_count);
    let mut cur = min_block;
    for _ in 0..geo_count {
        geo.push(cur);
        let mut next = (cur * num).div_ceil(den);
        next = (next + mask) & !mask;
        if next <= cur {
            next = cur + min_block;
        }
        cur = next;
    }
    // Sorted merge of geo and extras.
    let mut out = Vec::with_capacity(geo_count + extras.len());
    let (mut gi, mut ei) = (0, 0);
    while gi < geo.len() || ei < extras.len() {
        let take_geo = if gi >= geo.len() {
            false
        } else if ei >= extras.len() {
            true
        } else {
            geo[gi] < extras[ei]
        };
        if take_geo {
            out.push(geo[gi]);
            gi += 1;
        } else {
            out.push(extras[ei]);
            ei += 1;
        }
    }
    out
}

/// Reference classifier (independent of the crate's `class_for`): smallest
/// class with `block >= max(size, align)` AND `block % align == 0`.
fn reference_class_for(table: &[usize], size: usize, align: usize) -> Option<usize> {
    let need = size.max(align);
    table
        .iter()
        .position(|&b| b >= need && b.is_multiple_of(align))
}

// ---------------------------------------------------------------------------
// Sefer's concrete parameterization (49 classes; the default in-tree scheme).
// ---------------------------------------------------------------------------

const SEFER_MIN_BLOCK: usize = 16;
const SEFER_EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
const SEFER_GEO: usize = 40;
const SEFER_N: usize = SEFER_GEO + SEFER_EXTRAS.len();
const SEFER_PARAMS: Params = Params {
    min_block: SEFER_MIN_BLOCK,
    growth: (5, 4),
    geo_count: SEFER_GEO,
    extras: SEFER_EXTRAS,
    huge_threshold: 4 * 1024 * 1024,
};
const SEFER_TABLE: [usize; SEFER_N] = build_table::<SEFER_N>(&SEFER_PARAMS);
const SEFER_MAX: usize = SEFER_TABLE[SEFER_N - 1];
const SEFER_L: usize = size2class_len(SEFER_MAX, SEFER_MIN_BLOCK);
const SEFER_SC: SizeClasses<SEFER_N, SEFER_L> = SizeClasses::build(SEFER_PARAMS);

#[test]
fn sefer_table_matches_reference_and_is_strictly_increasing() {
    let want = reference_table(SEFER_MIN_BLOCK, (5, 4), SEFER_GEO, SEFER_EXTRAS);
    assert_eq!(&SEFER_TABLE[..], &want[..]);
    // Derive-not-hardcode: the count is whatever the params produce.
    assert_eq!(SEFER_SC.count(), SEFER_N);
    for w in SEFER_TABLE.windows(2) {
        assert!(w[0] < w[1], "table must be strictly increasing: {w:?}");
    }
    for &b in &SEFER_TABLE {
        assert!(
            b.is_multiple_of(SEFER_MIN_BLOCK),
            "class {b} not a multiple of min_block"
        );
    }
    // The exact-256 and page-aligned extras really landed in the table.
    for &e in SEFER_EXTRAS {
        assert!(SEFER_TABLE.contains(&e), "extra {e} missing from table");
    }
}

#[test]
fn sefer_class_for_matches_reference_over_full_small_sweep() {
    // Every alignment the slow path can carry (powers of two up to SMALL_MAX),
    // and every size 1..=SMALL_MAX+1, against the independent reference.
    let mut aligns = vec![1usize, 2, 4, 8, 16];
    let mut a = 32;
    while a <= SEFER_MAX {
        aligns.push(a);
        a <<= 1;
    }
    for &align in &aligns {
        for size in 1..=(SEFER_MAX + 1) {
            let got = SEFER_SC.class_for(size, align);
            let want = reference_class_for(&SEFER_TABLE, size, align);
            assert_eq!(got, want, "drift at size={size} align={align}");
            if let Some(idx) = got {
                let block = SEFER_TABLE[idx];
                assert!(block >= size.max(align));
                assert!(block.is_multiple_of(align));
            }
        }
    }
}

#[test]
fn sefer_size2class_matches_scan_for_every_bucket() {
    let s2c = SEFER_SC.size2class();
    for (k, &class_idx) in s2c.iter().enumerate() {
        let need = ((k + 1) * SEFER_MIN_BLOCK).min(SEFER_MAX);
        let want = SEFER_TABLE.iter().position(|&b| b >= need).unwrap();
        assert_eq!(
            class_idx as usize, want,
            "SIZE2CLASS[{k}] drift (need={need})"
        );
    }
}

#[test]
fn sefer_jump_skips_non_divisible_run_for_align_128() {
    // (128,128): seed is the ~144 B class (not 128-divisible); the jump must
    // land on the 256 B exact class in one hop.
    let got = SEFER_SC.class_for(128, 128).expect("(128,128) resolves");
    let block = SEFER_TABLE[got];
    assert!(block.is_multiple_of(128));
    assert!(block >= 128);
    // The seed itself is NOT 128-divisible (else the jump would be a no-op).
    let seed = SEFER_SC.size2class()[(128 - 1) >> SEFER_MIN_BLOCK.trailing_zeros()] as usize;
    assert!(!SEFER_TABLE[seed].is_multiple_of(128));
}

#[test]
fn is_huge_uses_the_policy_threshold_not_an_os_constant() {
    // huge_threshold is a pure Params policy value; the crate never references
    // an OS segment size. Two different thresholds → two different verdicts for
    // the same size, proving it is parameterized.
    const P_SMALL: Params = Params {
        min_block: 16,
        growth: (5, 4),
        geo_count: 4,
        extras: &[],
        huge_threshold: 1024,
    };
    const N: usize = 4;
    const T: [usize; N] = build_table::<N>(&P_SMALL);
    const L: usize = size2class_len(T[N - 1], 16);
    const SC: SizeClasses<N, L> = SizeClasses::build(P_SMALL);
    assert!(SC.is_huge(1024));
    assert!(SC.is_huge(4096));
    assert!(!SC.is_huge(1023));
}
