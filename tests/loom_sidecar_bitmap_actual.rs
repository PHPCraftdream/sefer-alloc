//! P1-2 / PRF-01 §4.3.1 actual-source Loom refinement: the REAL
//! `bitmap_cut.rs`, `bitmap_record.rs`, `bitmap_scan.rs`, and
//! `sidecar_bitmap.rs` are compiled as top-level modules; the remote_bitmap
//! shim only reexports them. The class map is dense `AtomicU8` backing via
//! `from_initialized`; the included `leaf_classes.rs` is the actual leaf source
//! file but its `Leaves` type is not modeled. This is a proof of the dense
//! backing path, not production `ClassLeaves`' standard-atomic leaf path.
//!
//! Reduced geometry: 3 pending words (`SEGMENT = 3 * 64 * MIN_BLOCK`). The
//! protocol is CURRENT production code: producer terminal `fetch_or(AcqRel)`
//! (`SidecarBitmap::publish`) and owner plain `swap(0, AcqRel)` per word
//! (`BitmapScan::next_cut`; the bench feature is off). In particular this is
//! not the load-before-swap shadow in `tests/loom_sidecar_bitmap.rs`.
//!
//! Negative shadow controls demonstrate the load+store and masking loss
//! mechanisms; they do not mutate or validate the actual-source implementation.
#![cfg(loom)]

mod alloc_core {
    pub mod os {
        /// 3 pending words at `MIN_BLOCK = 16`.
        pub const SEGMENT: usize = 3 * 64 * 16;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
}

// The included production files resolve `super` at this root module.
pub(crate) use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};

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

use remote_bitmap::{BitmapCut, BitmapRecord, BitmapScan, SidecarBitmap};

use loom::sync::Arc;
use loom::thread;

const HIGH_WATER: usize = 3 * 64 * 16;
const TRIM_HIGH_WATER: usize = 1040; // strict trim includes offset 1024 at MIN_BLOCK 16

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

/// Owner discovery: every word's cut through the real `BitmapScan::next_cut`
/// (plain `swap(0, AcqRel)`), records tagged with the cut's word index.
fn drain(map: &SidecarBitmap<'_>, high_water: usize, records: &mut Vec<(usize, BitmapRecord)>) {
    let mut scan = map.scan(high_water).expect("in-bounds high water");
    let mut word = 0;
    while let Some(mut cut) = scan.next_cut() {
        assert!(
            !cut.is_empty() || cut.pop().is_none(),
            "empty cut pops nothing"
        );
        while let Some(record) = cut.pop() {
            records.push((word, record));
        }
        word += 1;
    }
}

fn assert_union(records: &[(usize, BitmapRecord)], expected: &[(usize, u64, u8)]) {
    let mut seen = records.to_vec();
    seen.sort_by_key(|(word, record)| (*word, record.offset));
    let mut expected = expected.to_vec();
    expected.sort();
    let actual: Vec<(usize, u64, u8)> = seen
        .iter()
        .map(|(word, r)| (*word, u64::from(r.offset), r.class))
        .collect();
    assert_eq!(
        actual, expected,
        "union of all cuts must equal the published set, without duplicates"
    );
}

#[test]
fn loom_actual_two_producers_same_word_two_discoveries_no_loss_no_dup() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        assert_eq!(SidecarBitmap::WORDS, 3);
        assert!(map.issue(0, 3));
        assert!(map.issue(16, 4));
        assert!(!map.publish(32), "never-issued granule must not publish");
        let p1 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(0));
            })
        };
        let p2 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(16));
            })
        };
        let mut records = Vec::new();
        drain(&map, HIGH_WATER, &mut records);
        p1.join().unwrap();
        p2.join().unwrap();
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0, 3), (0, 16, 4)]);
        let mut drained = Vec::new();
        drain(&map, HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "third discovery finds nothing");
    });
}

#[test]
fn loom_actual_two_producers_distinct_words_two_discoveries_no_loss_no_dup() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        assert!(map.issue(0, 3));
        assert!(map.issue(64 * 16, 5));
        let p1 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(0));
            })
        };
        let p2 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(64 * 16));
            })
        };
        let mut records = Vec::new();
        drain(&map, HIGH_WATER, &mut records);
        p1.join().unwrap();
        p2.join().unwrap();
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0, 3), (1, 64 * 16, 5)]);
        let mut drained = Vec::new();
        drain(&map, HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "third discovery finds nothing");
    });
}

#[test]
fn loom_actual_publish_racing_strict_trim_union_is_complete() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        assert!(map.issue(0, 3));
        assert!(map.issue(1024, 6));
        let p0 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(0));
            })
        };
        let p1 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(1024));
            })
        };
        // Strict scan's 1040-byte bound covers both word 0 and offset 1024.
        let mut records = Vec::new();
        drain(&map, TRIM_HIGH_WATER, &mut records);
        p0.join().unwrap();
        p1.join().unwrap();
        // The union of the racing strict scan and later discovery must be exact.
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0, 3), (1, 1024, 6)]);
        let mut drained = Vec::new();
        drain(&map, HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "discovery after recovery finds nothing");
    });

    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        assert!(map.issue(1024, 6));
        let producer = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || assert!(backing.bitmap().publish(1024)))
        };
        producer.join().unwrap();

        let mut records = Vec::new();
        drain(&map, TRIM_HIGH_WATER, &mut records);
        assert_union(&records, &[(1, 1024, 6)]);
        let mut drained = Vec::new();
        drain(&map, TRIM_HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "strict scan itself found offset 1024");
    });
}

/// Shadow of one pending word for the negative cut mechanisms.
struct ShadowWord {
    bits: AtomicU64,
}

/// Mutant cut: read then clear (no RMW) — a concurrent `fetch_or` between the
/// load and the store is lost.
fn mutant_cut_load_store(word: &ShadowWord) -> u64 {
    let bits = word.bits.load(Ordering::Acquire);
    thread::yield_now();
    word.bits.store(0, Ordering::Release);
    bits
}

#[test]
#[should_panic(expected = "published bit lost by load+store cut")]
fn loom_actual_negative_cut_via_load_store_loses_concurrent_bit() {
    loom::model(move || {
        let word = Arc::new(ShadowWord {
            bits: AtomicU64::new(0),
        });
        let producer = {
            let word = Arc::clone(&word);
            thread::spawn(move || {
                word.bits.fetch_or(1, Ordering::AcqRel);
            })
        };
        let owner = {
            let word = Arc::clone(&word);
            thread::spawn(move || mutant_cut_load_store(&word))
        };
        let cut = owner.join().unwrap();
        producer.join().unwrap();
        let remaining = word.bits.load(Ordering::Acquire);
        assert_eq!(cut | remaining, 1, "published bit lost by load+store cut");
    });
}

/// Mutant cut: correct `swap` but the cut reports only bit 0, dropping every
/// other bit detached from the word.
fn mutant_cut_masked(word: &ShadowWord) -> u64 {
    word.bits.swap(0, Ordering::AcqRel) & 1
}

#[test]
#[should_panic(expected = "masked cut dropped pending bits")]
fn loom_actual_negative_masked_cut_drops_pending_bits() {
    loom::model(move || {
        let word = Arc::new(ShadowWord {
            bits: AtomicU64::new(0),
        });
        let pa = {
            let word = Arc::clone(&word);
            thread::spawn(move || {
                thread::yield_now();
                word.bits.fetch_or(1, Ordering::AcqRel);
            })
        };
        let pb = {
            let word = Arc::clone(&word);
            thread::spawn(move || {
                thread::yield_now();
                word.bits.fetch_or(2, Ordering::AcqRel);
            })
        };
        let mut seen = mutant_cut_masked(&word);
        pa.join().unwrap();
        pb.join().unwrap();
        seen |= word.bits.swap(0, Ordering::AcqRel);
        assert_eq!(seen, 3, "masked cut dropped pending bits");
    });
}
