//! P1-2 / PRF-01 §4.3.1 actual-source Loom refinement: the REAL
//! `bitmap_cut.rs`, `bitmap_scan.rs`, and `sidecar_bitmap.rs` are compiled as
//! top-level modules; the remote_bitmap shim only reexports them. D1′
//! phase-1b: the class-leaves carrier is GONE — the sidecar runs the pending
//! bank plus the owner's start-issue map via `from_pending`, and the
//! carve-clear tests below mirror `SmallSidecar::clear_pending_start_bit`'s
//! RMW (`load(Acquire)` skip + `fetch_and(!mask, AcqRel)`) against a real
//! publisher on the included pending bank.
//!
//! Reduced geometry: 3 pending words (`SEGMENT = 3 * 64 * MIN_BLOCK`). The
//! protocol is CURRENT production code: producer terminal `fetch_or(AcqRel)`
//! (`SidecarBitmap::publish`) and the owner's B4 guarded cut per word
//! (`BitmapScan::next_cut`: plain `load(Acquire)` then `swap(0, AcqRel)` only
//! when the word is nonzero — the bench feature is off, so the guarded
//! production arm compiles, not the bench baseline). In particular this is
//! not the load-before-swap shadow in `tests/loom_sidecar_bitmap.rs`.
//!
//! Negative shadow controls demonstrate the load+store and masking loss
//! mechanisms; they do not mutate or validate the actual-source implementation.
#![cfg(loom)]

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
        /// 3 pending words at `MIN_BLOCK = 16`.
        pub const SEGMENT: usize = 3 * 64 * 16;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

// The included production files resolve `super` at this root module.
pub(crate) use loom::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;

mod remote_bitmap {
    pub(crate) use super::bitmap_cut::BitmapCut;
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}

use remote_bitmap::{BitmapCut, BitmapScan, SidecarBitmap};

use loom::sync::Arc;
use loom::thread;

const HIGH_WATER: usize = 3 * 64 * 16;
const TRIM_HIGH_WATER: usize = 1040; // strict trim includes offset 1024 at MIN_BLOCK 16

struct Backing {
    words: Vec<AtomicU64>,
    starts: Vec<AtomicU64>,
}

impl Backing {
    fn new() -> Self {
        Self {
            words: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            // D1' phase-1a start bank, zero-initialized.
            starts: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
        }
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_pending(&self.words, &self.starts).unwrap()
    }
}

/// Owner discovery: every word's cut through the real `BitmapScan::next_cut`
/// (plain `swap(0, AcqRel)`), records tagged with the cut's word index.
// D1' phase-1a: cuts yield the OFFSET only.
fn drain(map: &SidecarBitmap<'_>, high_water: usize, records: &mut Vec<(usize, u32)>) {
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

fn assert_union(records: &[(usize, u32)], expected: &[(usize, u64)]) {
    let mut seen = records.to_vec();
    seen.sort_by_key(|(word, record)| (*word, *record));
    let mut expected = expected.to_vec();
    expected.sort();
    let actual: Vec<(usize, u64)> = seen
        .iter()
        .map(|(word, r)| (*word, u64::from(*r)))
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
        map.issue(0);
        map.issue(16);
        assert!(!map.publish(32, 0), "never-issued granule must not publish");
        let p1 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(0, 0));
            })
        };
        let p2 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(16, 0));
            })
        };
        let mut records = Vec::new();
        drain(&map, HIGH_WATER, &mut records);
        p1.join().unwrap();
        p2.join().unwrap();
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0), (0, 16)]);
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
        map.issue(0);
        map.issue(64 * 16);
        let p1 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(0, 0));
            })
        };
        let p2 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(64 * 16, 0));
            })
        };
        let mut records = Vec::new();
        drain(&map, HIGH_WATER, &mut records);
        p1.join().unwrap();
        p2.join().unwrap();
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0), (1, 64 * 16)]);
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
        map.issue(0);
        map.issue(1024);
        let p0 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(0, 0));
            })
        };
        let p1 = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                assert!(backing.bitmap().publish(1024, 0));
            })
        };
        // Strict scan's 1040-byte bound covers both word 0 and offset 1024.
        let mut records = Vec::new();
        drain(&map, TRIM_HIGH_WATER, &mut records);
        p0.join().unwrap();
        p1.join().unwrap();
        // The union of the racing strict scan and later discovery must be exact.
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0), (1, 1024)]);
        let mut drained = Vec::new();
        drain(&map, HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "discovery after recovery finds nothing");
    });

    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        map.issue(1024);
        let producer = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || assert!(backing.bitmap().publish(1024, 0)))
        };
        producer.join().unwrap();

        let mut records = Vec::new();
        drain(&map, TRIM_HIGH_WATER, &mut records);
        assert_union(&records, &[(1, 1024)]);
        let mut drained = Vec::new();
        drain(&map, TRIM_HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "strict scan itself found offset 1024");
    });
}

// ---------------------------------------------------------------------------
// D1' phase-1b carve-clear (design §2.4) against a real publisher, on the
// SAME included pending bank. The owner clear mirrors
// `SmallSidecar::clear_pending_start_bit` verbatim: load(Acquire) to skip
// clean words, then fetch_and(!mask, AcqRel) — never a plain load+store.
// ---------------------------------------------------------------------------

/// One-bit owner clear of `offset`'s granule (mask form of
/// `clear_pending_start_bit`).
fn clear_pending_bit(map_words: &[AtomicU64], offset: usize) {
    let granule = offset / 16;
    let mask = 1u64 << (granule % 64);
    if map_words[granule / 64].load(Ordering::Acquire) & mask != 0 {
        map_words[granule / 64].fetch_and(!mask, Ordering::AcqRel);
    }
}

/// A legitimate pending bit of a NEIGHBOURING granule in the same word must
/// survive the owner's masked carve-clear in every interleaving, and the
/// cleared granule must stop publishing (its stale bit is gone).
#[test]
fn loom_actual_carve_clear_keeps_neighbour_bit_and_drops_cleared_one() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        map.issue(0);
        map.issue(16);
        // Plant the stale pending bit on granule 0 (the garbage-free record
        // the carve-clear must drop) BEFORE the spawn, so the owner's
        // clear_pending_bit actually takes its RMW branch.
        assert!(map.publish(0, 0));
        // One legitimate publication on granule 1, racing the owner's carve-clear
        // of granule 0.
        let p = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                backing.bitmap().publish(16, 0)
            })
        };
        let o = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                clear_pending_bit(&backing.words, 0);
            })
        };
        o.join().unwrap();
        let neighbour_published = p.join().unwrap();
        // Word state: bit 0 cleared-or-never-set (the stale bit cannot come
        // back), bit 1 set iff the publish RMW won the race.
        let word = backing.words[0].load(Ordering::Acquire);
        assert_eq!(
            word & 1,
            0,
            "carve-clear must drop the cleared granule's bit in every interleaving"
        );
        assert_eq!(
            (word >> 1) & 1,
            u64::from(neighbour_published),
            "the neighbour's legitimate bit survives exactly when its publish won the race"
        );
        if neighbour_published {
            let mut records = Vec::new();
            drain(&map, HIGH_WATER, &mut records);
            assert_union(&records, &[(0, 16)]);
        }
    });
}

/// NEGATIVE control: a plain load+store clear (FORBIDDEN form) loses a
/// concurrently published neighbouring bit in the same word. Legality
/// preconditions: granule 0's pending bit was legitimately published
/// BEFORE the spawn (a stale record the owner's carve-clear of the
/// recarved offset must drop), granule 1's start bit is pre-issued, and
/// the concurrent producer publishes granule 1 while the owner runs the
/// forbidden load+store form. The RMW positive model
/// (`clear_pending_bit` +
/// `loom_actual_carve_clear_keeps_neighbour_bit_and_drops_cleared_one`)
/// keeps the neighbour's bit in every interleaving; this mutant form
/// loses it whenever the producer RMW lands between the load and the
/// store.
#[test]
#[should_panic(expected = "load+store carve-clear lost the concurrent bit")]
fn loom_actual_negative_carve_clear_via_load_store_loses_concurrent_bit() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        map_issue(backing.as_ref(), 0);
        map_issue(backing.as_ref(), 16);
        // Stale pending bit on granule 0, planted BEFORE the spawn.
        assert!(backing.bitmap().publish(0, 0));
        let p = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                backing.bitmap().publish(16, 0)
            })
        };
        let o = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                // FORBIDDEN form: plain load + masked store (the yield
                // between them opens the producer's loss window).
                let w = backing.words[0].load(Ordering::Acquire);
                thread::yield_now();
                backing.words[0].store(w & !1, Ordering::Release);
            })
        };
        o.join().unwrap();
        let neighbour_published = p.join().unwrap();
        let word = backing.words[0].load(Ordering::Acquire);
        assert_eq!(
            (word >> 1) & 1,
            u64::from(neighbour_published),
            "load+store carve-clear lost the concurrent bit"
        );
    });
}

fn map_issue(backing: &Backing, offset: u32) {
    backing.bitmap().issue(offset);
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
