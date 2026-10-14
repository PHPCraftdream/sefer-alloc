//! Actual-source Loom models for fixed and summary sidecar scans.
//! Reduced geometry preserves the 64-word prefix and three payload words.
#![cfg(loom)]

mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = 67 * 64 * 16;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
}
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};
// Classes are initialized before workers start and immutable in these models.
pub(crate) use std::sync::atomic::AtomicU8;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_record.rs"]
mod bitmap_record;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;
mod remote_bitmap {
    pub(crate) use super::bitmap_cut::BitmapCut;
    pub(crate) use super::bitmap_record::BitmapRecord;
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}
use loom::sync::{mpsc, Arc};
use loom::thread;
#[allow(unused_imports)]
use remote_bitmap::{BitmapCut, BitmapRecord, BitmapScan, SidecarBitmap};
const START: usize = 64;
const HIGH_WATER: usize = 67 * 64 * 16;

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
}
fn drain_fixed(map: &SidecarBitmap<'_>, high_water: usize, out: &mut Vec<(usize, BitmapRecord)>) {
    let mut scan = map.scan_from(high_water, START).unwrap();
    let mut word = START;
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            out.push((word, record));
        }
        word += 1;
    }
}
fn drain_summary(map: &SidecarBitmap<'_>, high_water: usize, out: &mut Vec<(usize, BitmapRecord)>) {
    let mut scan = map.discover_from(high_water, START).unwrap();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            out.push((cut.word, record));
        }
    }
}
fn assert_union(records: &[(usize, BitmapRecord)], expected: &[(usize, u64, u8)]) {
    let mut actual: Vec<_> = records
        .iter()
        .map(|(word, r)| (*word, u64::from(r.offset), r.class))
        .collect();
    actual.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(actual, expected, "cut union equals published set exactly");
}

#[test]
fn loom_actual_two_producers_same_word_two_discoveries_no_loss_no_dup() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let a = (64 * 64 * 16) as u32;
        let z = a + 16;
        assert!(map.issue(a, 3));
        assert!(map.issue(z, 4));
        let p = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish(a)))
        };
        let q = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish(z)))
        };
        let mut out = Vec::new();
        drain_fixed(&map, HIGH_WATER, &mut out);
        p.join().unwrap();
        q.join().unwrap();
        drain_fixed(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(a), 3), (64, u64::from(z), 4)]);
    });
}
#[test]
fn loom_actual_two_producers_distinct_words_two_discoveries_no_loss_no_dup() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let a = (64 * 64 * 16) as u32;
        let z = (65 * 64 * 16) as u32;
        assert!(map.issue(a, 3));
        assert!(map.issue(z, 5));
        let p = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish(a)))
        };
        let q = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish(z)))
        };
        let mut out = Vec::new();
        drain_fixed(&map, HIGH_WATER, &mut out);
        p.join().unwrap();
        q.join().unwrap();
        drain_fixed(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(a), 3), (65, u64::from(z), 5)]);
    });
}
#[test]
fn loom_actual_publish_racing_strict_trim_union_is_complete() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let a = (64 * 64 * 16) as u32;
        let z = (65 * 64 * 16) as u32;
        assert!(map.issue(a, 3));
        assert!(map.issue(z, 6));
        let p = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish(a)))
        };
        let q = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish(z)))
        };
        let mut out = Vec::new();
        drain_fixed(&map, HIGH_WATER, &mut out);
        p.join().unwrap();
        q.join().unwrap();
        drain_fixed(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(a), 3), (65, u64::from(z), 6)]);
    });
}
#[test]
fn loom_actual_publish_paused_after_p2_fixed_scan_recovers() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let off = (64 * 64 * 16) as u32;
        assert!(map.issue(off, 2));
        let producer_backing = Arc::clone(&b);
        let (p2_tx, p2_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let producer = thread::spawn(move || {
            assert!(producer_backing
                .bitmap()
                .publish_hinted_with_pause_for_test(off, || {
                    p2_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                }))
        });
        p2_rx.recv().unwrap();
        let mut out = Vec::new();
        drain_fixed(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(off), 2)]);
        assert_eq!(
            b.words[1].load(Ordering::Acquire),
            0,
            "P3 has not run while producer is paused"
        );
        resume_tx.send(()).unwrap();
        producer.join().unwrap();
        drain_summary(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(off), 2)]);
    });
}
#[test]
fn loom_actual_hinted_same_word_producers_two_discoveries_exact_union() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let a = (64 * 64 * 16) as u32;
        let z = a + 16;
        assert!(map.issue(a, 3));
        assert!(map.issue(z, 4));
        let p = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish_hinted(a)))
        };
        let q = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish_hinted(z)))
        };
        let mut out = Vec::new();
        drain_summary(&map, HIGH_WATER, &mut out);
        p.join().unwrap();
        q.join().unwrap();
        drain_summary(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(a), 3), (64, u64::from(z), 4)]);
    });
}
#[test]
fn loom_actual_hinted_different_words_share_hint_exact_union() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let a = (64 * 64 * 16) as u32;
        let z = (65 * 64 * 16) as u32;
        assert!(map.issue(a, 2));
        assert!(map.issue(z, 5));
        let p = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish_hinted(a)))
        };
        let q = {
            let b = Arc::clone(&b);
            thread::spawn(move || assert!(b.bitmap().publish_hinted(z)))
        };
        let mut out = Vec::new();
        drain_summary(&map, HIGH_WATER, &mut out);
        p.join().unwrap();
        q.join().unwrap();
        drain_summary(&map, HIGH_WATER, &mut out);
        assert_union(&out, &[(64, u64::from(a), 2), (65, u64::from(z), 5)]);
    });
}
#[test]
#[should_panic(expected = "no-setter mutant left published payload undiscovered")]
fn loom_actual_negative_no_summary_setter() {
    loom::model(|| {
        let b = Backing::new();
        let map = b.bitmap();
        let off = (64 * 64 * 16) as u32;
        assert!(map.issue(off, 2));
        assert!(map.publish(off));
        let mut out = Vec::new();
        drain_summary(&map, HIGH_WATER, &mut out);
        assert!(
            !out.is_empty(),
            "no-setter mutant left published payload undiscovered"
        );
    });
}

struct ShadowWord {
    bits: AtomicU64,
}
fn mutant_load_store(word: &ShadowWord) -> u64 {
    let bits = word.bits.load(Ordering::Acquire);
    thread::yield_now();
    word.bits.store(0, Ordering::Release);
    bits
}
fn mutant_mask(word: &ShadowWord) -> u64 {
    word.bits.swap(0, Ordering::AcqRel) & 1
}
#[test]
#[should_panic(expected = "published bit lost by load+store cut")]
fn loom_actual_negative_cut_via_load_store_loses_concurrent_bit() {
    loom::model(|| {
        let word = Arc::new(ShadowWord {
            bits: AtomicU64::new(0),
        });
        let p = {
            let w = Arc::clone(&word);
            thread::spawn(move || {
                w.bits.fetch_or(1, Ordering::AcqRel);
            })
        };
        let owner = {
            let w = Arc::clone(&word);
            thread::spawn(move || mutant_load_store(&w))
        };
        let cut = owner.join().unwrap();
        p.join().unwrap();
        assert_eq!(
            cut | word.bits.load(Ordering::Acquire),
            1,
            "published bit lost by load+store cut"
        );
    });
}
#[test]
#[should_panic(expected = "masked cut dropped pending bits")]
fn loom_actual_negative_masked_cut_drops_pending_bits() {
    loom::model(|| {
        let word = Arc::new(ShadowWord {
            bits: AtomicU64::new(0),
        });
        let a = {
            let w = Arc::clone(&word);
            thread::spawn(move || {
                thread::yield_now();
                w.bits.fetch_or(1, Ordering::AcqRel);
            })
        };
        let b = {
            let w = Arc::clone(&word);
            thread::spawn(move || {
                thread::yield_now();
                w.bits.fetch_or(2, Ordering::AcqRel);
            })
        };
        let mut cut = mutant_mask(&word);
        a.join().unwrap();
        b.join().unwrap();
        cut |= word.bits.swap(0, Ordering::AcqRel);
        assert_eq!(cut, 3, "masked cut dropped pending bits");
    });
}
#[test]
#[should_panic(expected = "owner load+store lost pending hint")]
fn loom_actual_negative_owner_load_store_loses_hint() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let map = b.bitmap();
        let off = (64 * 64 * 16) as u32;
        assert!(map.issue(off, 1));
        let (tx, rx) = mpsc::channel();
        let producer_map = Arc::clone(&b);
        let producer = thread::spawn(move || {
            rx.recv().unwrap();
            assert!(producer_map.bitmap().publish_hinted(off));
        });
        let mut first = map.scan_from(HIGH_WATER, 64).unwrap();
        let first_cut = first.next_cut().unwrap();
        let first_bits = first_cut.bits;
        tx.send(()).unwrap();
        producer.join().unwrap();
        let summary = &b.words[1];
        let stale = summary.load(Ordering::Acquire);
        summary.store(0, Ordering::Release); // mutant stale clear overwrites P3
        let mut recovered = Vec::new();
        drain_summary(&map, HIGH_WATER, &mut recovered);
        let consumed = first_bits != 0;
        assert!(
            consumed || !recovered.is_empty(),
            "owner load+store lost pending hint"
        );
        assert_eq!(stale, 1, "producer published summary bit");
    });
}
#[test]
#[should_panic(expected = "range mask truncated final payload word")]
fn loom_actual_negative_range_mask_truncates_final_word() {
    loom::model(|| {
        let b = Backing::new();
        let map = b.bitmap();
        let off = (66 * 64 * 16) as u32;
        assert!(map.issue(off, 6));
        assert!(map.publish_hinted(off));
        let mut out = Vec::new();
        drain_summary(&map, 66 * 64 * 16, &mut out);
        assert!(!out.is_empty(), "range mask truncated final payload word");
    });
}
