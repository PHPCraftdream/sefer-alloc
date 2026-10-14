//! Source-included C‴ summary protocol tests using production bitmap geometry.
#![cfg(feature = "alloc-core")]
// Included production files carry items this test does not exercise.
#![allow(dead_code)]

// Geometry and atomics are the only fixture substitutions; algorithms below
// are the actual production files, as in tests/r6_sidecar_bitmap.rs.
mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
}
#[allow(unused_imports)]
pub(crate) use core::sync::atomic::AtomicU8;
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
// The fixture replaces only instrumentation callbacks; protocol atomics/scans are real.
#[cfg(feature = "bench-internals")]
pub(crate) struct Prf01SidecarOracle;
#[cfg(feature = "bench-internals")]
impl Prf01SidecarOracle {
    pub(crate) fn summary_hint_load() {}
    pub(crate) fn summary_hint_rmw() {}
    pub(crate) fn summary_word_visited() {}
}
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_record.rs"]
mod bitmap_record;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;
mod remote_bitmap {
    #[allow(unused_imports)]
    pub(crate) use super::bitmap_cut::BitmapCut;
    #[allow(unused_imports)]
    pub(crate) use super::bitmap_record::BitmapRecord;
    #[allow(unused_imports)]
    pub(crate) use super::bitmap_scan::BitmapScan;
    #[allow(unused_imports)]
    pub(crate) use super::sidecar_bitmap::{SidecarBitmap, SummaryScan};
    #[allow(unused_imports)]
    pub(crate) use super::AtomicU64;
    #[allow(unused_imports)]
    pub(crate) use super::AtomicU8;
    #[allow(unused_imports)]
    pub(crate) use super::Ordering;
}

#[allow(unused_imports)]
use remote_bitmap::{BitmapCut, BitmapRecord, BitmapScan};
#[allow(unused_imports)]
use remote_bitmap::{SidecarBitmap, SummaryScan};
use std::thread;

struct Backing {
    words: Vec<AtomicU64>,
    classes: Vec<AtomicU8>,
}
impl Backing {
    fn new() -> Self {
        Self {
            words: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            classes: (0..SidecarBitmap::GRANULES)
                .map(|_| AtomicU8::new(0))
                .collect(),
        }
    }
    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_initialized(&self.words, &self.classes).unwrap()
    }
    fn quiescent_missing(&self, start: usize, end: usize) -> u64 {
        (start..end)
            .filter(|&word| {
                self.words[word].load(Ordering::Relaxed) != 0
                    && self.words[word / 64].load(Ordering::Relaxed) & (1 << (word % 64)) == 0
            })
            .count() as u64
    }
}

fn discover(
    map: &SidecarBitmap<'_>,
    high_water: usize,
    start_word: usize,
) -> Vec<(usize, usize, u8)> {
    let mut scan = map.discover_from(high_water, start_word).unwrap();
    let mut found = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            found.push((cut.word, record.offset as usize, record.class));
        }
    }
    found.sort_unstable();
    found
}

#[test]
fn c3_hinted_same_word_and_distinct_words_discover_once() {
    let b = Backing::new();
    let map = b.bitmap();
    let word_bytes = 64 * sefer_alloc::SegmentLayout::MIN_BLOCK;
    let first = 127 * word_bytes;
    let offsets = [first as u32, (first + 16) as u32, (128 * word_bytes) as u32];
    for (i, offset) in offsets.iter().enumerate() {
        assert!(map.issue(*offset, i as u8 + 1));
    }
    thread::scope(|scope| {
        for offset in offsets {
            let map = &map;
            scope.spawn(move || assert!(map.publish_hinted(offset)));
        }
    });
    let end = 129 * word_bytes + 16;
    assert_eq!(
        discover(&map, end, 64),
        [
            (127, first, 1),
            (127, first + 16, 2),
            (128, 128 * word_bytes, 3)
        ]
    );
    assert!(
        discover(&map, end, 64).is_empty(),
        "discovery consumes summary and pending cuts"
    );
}

#[test]
fn c3_summary_boundaries_and_final_partial_word_are_included() {
    let b = Backing::new();
    let map = b.bitmap();
    let word_bytes = 64 * sefer_alloc::SegmentLayout::MIN_BLOCK;
    let start_word = 64;
    let final_word = 129;
    let high_water = final_word * word_bytes + 16;
    for (i, word) in [127usize, 128, final_word].into_iter().enumerate() {
        let offset = (word * word_bytes) as u32;
        assert!(map.issue(offset, i as u8 + 1));
        assert!(map.publish_hinted(offset));
    }
    let found = discover(&map, high_water, start_word);
    assert_eq!(
        found.iter().map(|(word, _, _)| *word).collect::<Vec<_>>(),
        [127, 128, 129]
    );
    assert_eq!(
        found.last().unwrap().0,
        final_word,
        "partial final word is scanned"
    );
    assert_eq!(SummaryScan::SUMMARY_WORDS, 64);
    assert!(
        map.discover_from(high_water, 63).is_none(),
        "dead prefix cannot be scanned"
    );
}

#[test]
fn c3_quiescent_oracle_after_join_before_discovery() {
    let b = Backing::new();
    let map = b.bitmap();
    let word_bytes = 64 * sefer_alloc::SegmentLayout::MIN_BLOCK;
    let offsets: Vec<u32> = (64..192).map(|word| (word * word_bytes) as u32).collect();
    for (i, offset) in offsets.iter().enumerate() {
        assert!(map.issue(*offset, (i % 8) as u8));
    }
    thread::scope(|scope| {
        for chunk in offsets.chunks(16) {
            let map = &map;
            let chunk = chunk.to_vec();
            scope.spawn(move || {
                for offset in chunk {
                    assert!(map.publish_hinted(offset));
                }
            });
        }
    });
    assert_eq!(
        b.quiescent_missing(64, 192),
        0,
        "after joins, before discovery"
    );
    assert_eq!(discover(&map, 192 * word_bytes, 64).len(), offsets.len());
}
