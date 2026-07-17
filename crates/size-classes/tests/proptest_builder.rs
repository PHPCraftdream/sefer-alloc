//! Property tests over the `const`-generic builder for DIFFERENT
//! parameterizations — the payoff of extraction: the in-tree constants are
//! baked, but the crate lets a proptest vary `(min_block, growth, geo_count,
//! extras)` and still assert the two structural guarantees for every scheme:
//!
//! 1. **jump ≡ walk** — the alignment slow path's jump is bit-identical to a
//!    naive step-by-1 walk (the counterfactual the in-tree
//!    `size_classes_slow_path_equivalence` test pins for sefer's one scheme,
//!    here generalized across schemes).
//! 2. **fidelity** — whenever `class_for` resolves `Some(idx)`, the class block
//!    is `>= max(size, align)` and a multiple of `align`; and it resolves
//!    `Some` exactly when the reference scan does.

use proptest::prelude::*;
use size_classes::{build_size2class, build_table, size2class_len, Params, SizeClasses};

/// The PRE-jump reference: seed at the lookup, then step ONE class at a time
/// until the first whose block is a multiple of `align`. This is the algorithm
/// the crate's jump path must be equivalent to.
fn walk_class_for(
    table: &[usize],
    s2c: &[u8],
    min_block: usize,
    size: usize,
    align: usize,
) -> Option<usize> {
    let shift = min_block.trailing_zeros();
    let small_align_max = min_block;
    let small_max = *table.last().unwrap();
    let need = size.max(align);
    if need > small_max {
        return None;
    }
    let seed = s2c[(need - 1) >> shift] as usize;
    if align <= small_align_max {
        return Some(seed);
    }
    let mut i = seed;
    while i < table.len() {
        if table[i].is_multiple_of(align) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Reference scan (independent of both jump and walk): smallest class with
/// `block >= max(size, align)` AND `block % align == 0`.
fn scan_class_for(table: &[usize], size: usize, align: usize) -> Option<usize> {
    let need = size.max(align);
    table
        .iter()
        .position(|&b| b >= need && b.is_multiple_of(align))
}

// Several concrete schemes, const-built with DIFFERENT params. proptest varies
// (size, align) over each; the schemes themselves vary min_block / growth /
// geo_count / extras — coverage the baked in-tree table cannot reach.

// Scheme A: sefer-like (min_block 16, 1.25×, page-aligned + 256 extras).
const A_MB: usize = 16;
const A_EX: &[usize] = &[256, 512, 1024, 2048, 4096];
const A_N: usize = 32 + A_EX.len();
const A_P: Params = Params {
    min_block: A_MB,
    growth: (5, 4),
    geo_count: 32,
    extras: A_EX,
    huge_threshold: 1 << 20,
};
const A_T: [usize; A_N] = build_table::<A_N>(&A_P);
const A_MAX: usize = A_T[A_N - 1];
const A_L: usize = size2class_len(A_MAX, A_MB);
const A_SC: SizeClasses<A_N, A_L> = SizeClasses::build(A_P);
static A_S2C: [u8; A_L] = build_size2class::<A_N, A_L>(&A_T, A_MB);

// Scheme B: min_block 8, steeper 1.5× growth, no extras.
const B_MB: usize = 8;
const B_EX: &[usize] = &[];
const B_N: usize = 24;
const B_P: Params = Params {
    min_block: B_MB,
    growth: (3, 2),
    geo_count: 24,
    extras: B_EX,
    huge_threshold: 1 << 20,
};
const B_T: [usize; B_N] = build_table::<B_N>(&B_P);
const B_MAX: usize = B_T[B_N - 1];
const B_L: usize = size2class_len(B_MAX, B_MB);
const B_SC: SizeClasses<B_N, B_L> = SizeClasses::build(B_P);
static B_S2C: [u8; B_L] = build_size2class::<B_N, B_L>(&B_T, B_MB);

// Scheme C: min_block 64 (large fundamental alignment), gentle 1.125× growth,
// a couple of big page-aligned extras.
const C_MB: usize = 64;
const C_EX: &[usize] = &[8192, 16384, 65536];
const C_N: usize = 30 + C_EX.len();
const C_P: Params = Params {
    min_block: C_MB,
    growth: (9, 8),
    geo_count: 30,
    extras: C_EX,
    huge_threshold: 1 << 20,
};
const C_T: [usize; C_N] = build_table::<C_N>(&C_P);
const C_MAX: usize = C_T[C_N - 1];
const C_L: usize = size2class_len(C_MAX, C_MB);
const C_SC: SizeClasses<C_N, C_L> = SizeClasses::build(C_P);
static C_S2C: [u8; C_L] = build_size2class::<C_N, C_L>(&C_T, C_MB);

fn pow2_up_to(max: usize) -> impl Strategy<Value = usize> {
    // Exponents 0..=log2(max) → 1,2,4,...
    let hi = (usize::BITS - 1 - max.leading_zeros()) as usize;
    (0..=hi).prop_map(|e| 1usize << e)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn scheme_a_jump_eq_walk_and_fidelity(
        size in 1usize..=(2 * A_MAX),
        align in pow2_up_to(A_MAX),
    ) {
        let got = A_SC.class_for(size, align);
        prop_assert_eq!(got, walk_class_for(&A_T, &A_S2C, A_MB, size, align), "jump != walk (A)");
        prop_assert_eq!(got, scan_class_for(&A_T, size, align), "jump != scan (A)");
        if let Some(idx) = got {
            prop_assert!(A_T[idx] >= size.max(align));
            prop_assert!(A_T[idx].is_multiple_of(align));
        }
    }

    #[test]
    fn scheme_b_jump_eq_walk_and_fidelity(
        size in 1usize..=(2 * B_MAX),
        align in pow2_up_to(B_MAX),
    ) {
        let got = B_SC.class_for(size, align);
        prop_assert_eq!(got, walk_class_for(&B_T, &B_S2C, B_MB, size, align), "jump != walk (B)");
        prop_assert_eq!(got, scan_class_for(&B_T, size, align), "jump != scan (B)");
        if let Some(idx) = got {
            prop_assert!(B_T[idx] >= size.max(align));
            prop_assert!(B_T[idx].is_multiple_of(align));
        }
    }

    #[test]
    fn scheme_c_jump_eq_walk_and_fidelity(
        size in 1usize..=(2 * C_MAX),
        align in pow2_up_to(C_MAX),
    ) {
        let got = C_SC.class_for(size, align);
        prop_assert_eq!(got, walk_class_for(&C_T, &C_S2C, C_MB, size, align), "jump != walk (C)");
        prop_assert_eq!(got, scan_class_for(&C_T, size, align), "jump != scan (C)");
        if let Some(idx) = got {
            prop_assert!(C_T[idx] >= size.max(align));
            prop_assert!(C_T[idx].is_multiple_of(align));
        }
    }
}

// Structural invariants that must hold for ALL three schemes regardless of
// params (not (size,align)-parameterized; a plain per-scheme assertion loop).
#[test]
fn every_scheme_table_is_strictly_increasing_and_min_block_aligned() {
    fn check(table: &[usize], min_block: usize) {
        assert_eq!(table[0], min_block, "first class must be min_block");
        for w in table.windows(2) {
            assert!(w[0] < w[1], "not strictly increasing: {w:?}");
        }
        for &b in table {
            assert!(
                b.is_multiple_of(min_block),
                "class {b} not a multiple of min_block {min_block}"
            );
        }
    }
    check(&A_T, A_MB);
    check(&B_T, B_MB);
    check(&C_T, C_MB);
}
