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
use remote_bitmap::SidecarBitmap;

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
    // D1' phase-1b: pending + start banks only — the class carrier is gone.
    let pending: Vec<_> = (0..words).map(|_| AtomicU64::new(0)).collect();
    let starts: Vec<_> = (0..words).map(|_| AtomicU64::new(0)).collect();
    // payload_start 0: the whole segment is payload in this harness.
    let map = SidecarBitmap::from_pending(&pending, &starts).unwrap();
    let mut issued = 0usize;
    let mut published = 0usize;
    let mut decoded = 0usize;
    let mut cut_count = 0usize;
    for invalid in [
        u32::try_from(segment - 1).unwrap(),
        u32::try_from(segment).unwrap(),
        u32::MAX,
    ] {
        // D1′ issue is owner-infallible; publication still rejects invalid
        // offsets without an RMW.
        assert!(!map.publish(invalid, 0), "invalid offset {invalid}");
    }
    assert!(pending.iter().all(|word| word.load(Ordering::Relaxed) == 0));

    let mut expected = vec![0u64; words];
    for offset in (0..segment).step_by(stride) {
        let word = offset / (stride * u64::BITS as usize);
        let bit = (offset % (stride * u64::BITS as usize)) / stride;
        map.issue(u32::try_from(offset).unwrap());
        issued += 1;
        assert_eq!(
            starts[word].load(Ordering::Relaxed) >> bit & 1,
            1,
            "start word {word}, offset {offset}"
        );
        assert!(
            map.publish(u32::try_from(offset).unwrap(), 0),
            "publish {offset}"
        );
        published += 1;
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
        cut_count += 1;
        assert_eq!(
            pending_word.load(Ordering::Relaxed),
            0,
            "cut clears word {word}"
        );
        for bit in 0..u64::BITS as usize {
            let offset = word * (stride * u64::BITS as usize) + bit * stride;
            assert_eq!(
                cut.pop(),
                Some(u32::try_from(offset).unwrap()),
                "inverse word {word}, bit {bit}"
            );
            decoded += 1;
        }
        assert!(cut.is_empty());
        assert!(cut.pop().is_none());
    }
    assert!(scan.next_cut().is_none());
    assert!(pending.iter().all(|word| word.load(Ordering::Relaxed) == 0));
    assert_eq!(issued, granules);
    assert_eq!(published, granules);
    assert_eq!(decoded, granules);
    assert_eq!(cut_count, words);
    println!("granules={granules} records={decoded} cuts={cut_count} boundary_rejections=3");
}
