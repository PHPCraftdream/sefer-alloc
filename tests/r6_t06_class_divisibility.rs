#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::SegmentLayout;

#[test]
fn reciprocal_divisibility_matches_remainder_and_limit_witness() {
    for (idx, &d) in SegmentLayout::SIZE_CLASS_TABLE.iter().enumerate() {
        for off in 0usize..=65_536 {
            assert_eq!(
                SegmentLayout::dbg_size_class_divides(idx, off),
                off % d == 0,
                "class={idx} divisor={d} offset={off}"
            );
        }
        let mut k = d;
        while k < SegmentLayout::SEGMENT {
            for off in [k - 1, k, k + 1] {
                if off < SegmentLayout::SEGMENT {
                    assert_eq!(
                        SegmentLayout::dbg_size_class_divides(idx, off),
                        off % d == 0
                    );
                }
            }
            k += d;
        }
        #[cfg(target_pointer_width = "64")]
        if d > 1 {
            let (inv, limit, shift) = SegmentLayout::dbg_size_class_divisor(idx);
            let witness = (u64::MAX / d as u64) * d as u64;
            // This exact-multiple boundary lies beyond SEGMENT; bounded offsets alone miss limit-1 mutants.
            assert_eq!(witness % d as u64, 0);
            assert!(SegmentLayout::dbg_size_class_divides(idx, witness as usize));
            assert!(witness.wrapping_mul(inv).rotate_right(shift) <= limit);
            assert!(witness.wrapping_mul(inv).rotate_right(shift) > limit - 1);
        }
    }
}
