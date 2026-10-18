#![cfg(feature = "alloc-core")]

// Production geometry and production source, not a reduced shadow.
mod alloc_core {
    // Mirrors `alloc_core::fatal` (`src/alloc_core/platform/fatal.rs`) for the
    // source-included primitives.
    pub mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

#[allow(dead_code)] // Dense test-only constructor is deliberately not exercised.
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

use core::sync::atomic::{AtomicU64, Ordering};
use remote_bitmap::{BitmapRecord, ClassLeaves, SidecarBitmap};

#[test]
fn sidecar_granule_index_exhaustive() {
    let segment = alloc_core::os::SEGMENT;
    let stride = alloc_core::size_classes::MIN_BLOCK;
    let classes = alloc_core::size_classes::SMALL_CLASS_COUNT;
    let granules = segment / stride;
    let words = segment / (stride * u64::BITS as usize);
    assert_eq!(granules, 262_144, "exhaustive production domain");
    assert_eq!(SidecarBitmap::GRANULES, granules);
    assert_eq!(SidecarBitmap::WORDS, words);
    assert!(classes >= 2);
    let pending: Vec<_> = (0..words).map(|_| AtomicU64::new(0)).collect();
    // SAFETY: the all-zero byte pattern is a valid ClassLeaves value (null
    // leaf pointers, zero presence bits) — the same contract the directory's
    // System.alloc_zeroed backing relies on. The Box keeps ownership and its
    // original allocation/layout; it outlives the bitmap and every cut.
    let leaves = unsafe { Box::<ClassLeaves>::new_zeroed().assume_init() };
    let map = SidecarBitmap::from_leaves(&pending, &leaves).unwrap();

    // Oracle: 4 KiB class leaves; bit position comes from the byte position
    // within a u64-sized pending word, independently of SidecarBitmap::granule.
    let uniform_class = |offset: usize| u8::try_from((offset / 4096) % classes).unwrap();
    let mixed_class =
        |offset: usize| u8::try_from(((offset / 4096) + (offset / stride) % 2) % classes).unwrap();
    let mut issued = [0usize; 2];
    let mut published = [0usize; 2];
    let mut decoded = [0usize; 2];
    let mut cut_counts = [0usize; 2];
    for pass in 0..2 {
        for invalid in [
            u32::try_from(segment - 1).unwrap(),
            u32::try_from(segment).unwrap(),
            u32::MAX,
        ] {
            assert!(!map.issue(invalid, 0), "invalid offset {invalid}");
            assert!(!map.publish(invalid), "invalid offset {invalid}");
        }
        assert!(pending.iter().all(|word| word.load(Ordering::Relaxed) == 0));
        let mut expected = vec![0u64; words];
        for offset in (0..segment).step_by(stride) {
            let granule = offset / stride;
            let word = offset / (stride * u64::BITS as usize);
            let bit = (offset % (stride * u64::BITS as usize)) / stride;
            let class = if pass == 0 {
                uniform_class(offset)
            } else {
                mixed_class(offset)
            };
            assert!(leaves.prepare(granule, class), "prepare {offset}");
            assert!(
                map.issue(u32::try_from(offset).unwrap(), class),
                "issue {offset}"
            );
            issued[pass] += 1;
            // encoded has no validation: only use the bounded oracle granule
            // after issue accepted this aligned in-segment offset.
            assert_eq!(leaves.encoded(granule), class + 1, "encoded {offset}");
            assert!(
                map.publish(u32::try_from(offset).unwrap()),
                "publish {offset}"
            );
            published[pass] += 1;
            expected[word] |= 1u64 << bit;
            assert_eq!(
                pending[word].load(Ordering::Relaxed),
                expected[word],
                "pending word {word}, offset {offset}"
            );
        }
        for (word, expected_bits) in pending.iter().zip(&expected) {
            assert_eq!(word.load(Ordering::Relaxed), *expected_bits);
            assert_eq!(*expected_bits, u64::MAX);
        }
        let mut scan = map.scan(segment).unwrap();
        for (word, pending_word) in pending.iter().enumerate() {
            let mut cut = scan.next_cut().expect("one cut per production word");
            cut_counts[pass] += 1;
            assert_eq!(
                pending_word.load(Ordering::Relaxed),
                0,
                "cut clears word {word}"
            );
            for bit in 0..u64::BITS as usize {
                let offset = word * (stride * u64::BITS as usize) + bit * stride;
                let class = if pass == 0 {
                    uniform_class(offset)
                } else {
                    mixed_class(offset)
                };
                assert_eq!(
                    cut.pop(),
                    Some(BitmapRecord {
                        offset: u32::try_from(offset).unwrap(),
                        class
                    }),
                    "inverse word {word}, bit {bit}"
                );
                decoded[pass] += 1;
            }
            assert!(cut.is_empty());
            assert!(cut.pop().is_none());
        }
        assert!(scan.next_cut().is_none());
        assert!(pending.iter().all(|word| word.load(Ordering::Relaxed) == 0));
    }
    assert_eq!(issued, [granules; 2]);
    assert_eq!(published, [granules; 2]);
    assert_eq!(decoded, [granules; 2]);
    assert_eq!(cut_counts, [words; 2]);
    println!("granules={granules} uniform_records={} mixed_records={} total_records={} cuts={} boundary_rejections=12", decoded[0], decoded[1], decoded.iter().sum::<usize>(), cut_counts.iter().sum::<usize>());
}
