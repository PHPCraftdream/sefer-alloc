//! P1-2 / PRF-01 §4.3.1 actual-source Loom refinement: the REAL
//! `bitmap_cut.rs`, `bitmap_record.rs`, `bitmap_scan.rs`, and
//! `sidecar_bitmap.rs` are compiled as top-level modules; the remote_bitmap
//! shim only reexports them. The class map is dense `AtomicU8` backing via
//! `from_initialized` AND the production `ClassLeaves` mixed-leaf
//! map via `from_leaves`: `leaf_classes.rs` takes its atomics from `super`, so the
//! included source runs on loom atomics too.
//!
//! Reduced geometry: 3 pending words (`SEGMENT = 3 * 64 * MIN_BLOCK`). The
//! protocol is CURRENT production code: producer terminal `fetch_or(AcqRel)`
//! (`SidecarBitmap::publish`) and the owner's B4 guarded cut per word
//! (`BitmapScan::next_cut`: plain `load(Acquire)` then `swap(0, AcqRel)` only
//! when the word is nonzero — the bench feature is off, so the guarded
//! production arm compiles, not the bench baseline). In particular this is
//! not a load-before-swap shadow.
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
pub(crate) use loom::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};

// Ghost checks in the included production files (docs/GHOST_CHECKS_PLAN.md):
// ghost builds (loom, miri, --cfg sefer_ghost) compile them.
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_record.rs"]
mod bitmap_record;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/ghost/mod.rs"]
mod ghost;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;

mod remote_bitmap {
    pub(crate) use super::bitmap_cut::BitmapCut;
    pub(crate) use super::bitmap_record::BitmapRecord;
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::ClassLeaves;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}

use remote_bitmap::{BitmapCut, BitmapRecord, BitmapScan, ClassLeaves, SidecarBitmap};

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

/// ClassLeaves backing for the mixed-leaf tests below: the pending words the
/// production `from_leaves` geometry requires, plus one System-backed leaf
/// set initialised exactly like `tests/class_leaves_publication_miri.rs`
/// does.
struct LeafBacking {
    words: Vec<AtomicU64>,
    leaves: Box<ClassLeaves>,
}

impl LeafBacking {
    fn new() -> Self {
        let words = (0..SidecarBitmap::WORDS)
            .map(|_| AtomicU64::new(0))
            .collect();
        let mut slot = Box::new(core::mem::MaybeUninit::<ClassLeaves>::uninit());
        // SAFETY: the Box owns a private, aligned, writable ClassLeaves-sized
        // slot; nothing references it until `initialize_at` returns. Loom
        // atomics are scheduler handles: byte-zeroing the slot would not
        // construct valid atomic cells, so `initialize_at` is mandatory here.
        unsafe { ClassLeaves::initialize_at(slot.as_mut_ptr()) };
        // SAFETY: every field was initialized above; the Box keeps ownership
        // of its original allocation and layout.
        let leaves = unsafe { slot.assume_init() };
        Self { words, leaves }
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_leaves(&self.words, &self.leaves).unwrap()
    }
}

/// Loom runner for the `ClassLeaves` cells: preparing a leaf writes every
/// granule's class byte, and each of those atomic stores is a loom branch
/// point, so the default branch limit is too low. The preemption bound keeps
/// the run small.
fn leaf_model<F>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    let mut builder = loom::model::Builder::new();
    builder.max_branches = 100_000;
    builder.preemption_bound = Some(2);
    builder.check(f);
}

/// Two leaves promoted into one slab chunk must not overlap: loom atomics are
/// not byte-sized, so the slot stride has to be counted in `AtomicU8`
/// elements. Constructing the second leaf's atomics must leave the first
/// leaf's issued class code intact (and the second's own code readable).
#[test]
fn loom_actual_two_leaves_of_one_chunk_do_not_overlap() {
    leaf_model(|| {
        let backing = LeafBacking::new();
        let leaves = &backing.leaves;
        assert!(leaves.prepare(0, 3));
        assert!(leaves.issue(0, 3));
        // Leaf 1 (granule 256) takes the next slot of the same chunk.
        assert!(leaves.prepare(256, 4));
        assert!(leaves.issue(256, 4));
        assert_eq!(leaves.encoded(0), 4, "first leaf survives the second slot");
        assert_eq!(leaves.encoded(256), 5);
        assert_eq!(leaves.encoded(1), 0);
        assert_eq!(leaves.encoded(257), 0);
    });
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

/// Ghost ledger over the production `sidecar/published` / `sidecar/cut-bits`
/// events (keyed by pending-word address): within one execution every
/// publication is consumed by exactly one cut bit. The ghost table is process
/// global, so the ledger compares deltas from a per-execution snapshot.
struct GhostLedger(Vec<(i64, i64)>);

impl GhostLedger {
    fn counts(words: &[AtomicU64]) -> Vec<(i64, i64)> {
        words
            .iter()
            .map(|w| {
                let key = core::ptr::from_ref(w).addr() as u64;
                (
                    ghost::count("sidecar/published", key),
                    ghost::count("sidecar/cut-bits", key),
                )
            })
            .collect()
    }

    fn new(words: &[AtomicU64]) -> Self {
        Self(Self::counts(words))
    }

    fn assert_balanced(&self, words: &[AtomicU64], published: i64) {
        let now = Self::counts(words);
        let mut total = 0;
        for (word, ((p0, c0), (p1, c1))) in self.0.iter().zip(&now).enumerate() {
            assert_eq!(
                p1 - p0,
                c1 - c0,
                "word {word}: every publication is cut exactly once"
            );
            total += p1 - p0;
        }
        assert_eq!(
            total, published,
            "ghost publications match the successful publishes"
        );
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
        let ledger = GhostLedger::new(&backing.words);
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
        ledger.assert_balanced(&backing.words, 2);
    });
}

#[test]
fn loom_actual_two_producers_distinct_words_two_discoveries_no_loss_no_dup() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let ledger = GhostLedger::new(&backing.words);
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
        ledger.assert_balanced(&backing.words, 2);
    });
}

#[test]
fn loom_actual_publish_racing_strict_trim_union_is_complete() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let ledger = GhostLedger::new(&backing.words);
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
        ledger.assert_balanced(&backing.words, 2);
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

/// Actual-source `ClassLeaves` publication: the owner's `prepare` (System leaf
/// allocation, Release pointer publication) and `issue` race a producer's
/// `publish` through `ClassMap::Leaves`. Whatever the interleaving, the
/// producer either sees no leaf yet or the fully initialized one, a completed
/// issue is never lost for a later discovery, and a never-issued granule in a
/// prepared leaf never publishes.
#[test]
fn loom_actual_leaves_prepare_race_publish_completed_issue_is_discoverable() {
    leaf_model(move || {
        let backing = Arc::new(LeafBacking::new());
        let owner = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                assert!(backing.leaves.prepare(0, 3));
                assert!(backing.bitmap().issue(0, 3));
            })
        };
        let producer = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                backing.bitmap().publish(0)
            })
        };
        owner.join().unwrap();
        let _published_early = producer.join().unwrap();
        let map = backing.bitmap();
        assert!(!map.publish(16), "never-issued granule must not publish");
        assert!(map.publish(0), "completed issue must remain discoverable");
        let mut records = Vec::new();
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0, 3)]);
        let mut drained = Vec::new();
        drain(&map, HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "second discovery finds nothing");
    });
}

/// Two producers publishing distinct granules of the SAME mixed leaf. The
/// owner's `prepare` (System leaf allocation, Release pointer publication)
/// and both `issue` stores complete before the producers spawn, so each
/// publish observes the issued classes; the racing part is the two producers
/// against each other and the owner's cut. Every schedule keeps the cut
/// union exact: no loss, no duplicates, and unissued granules never publish.
/// (The prepare-vs-producer race is covered by
/// `loom_actual_leaves_prepare_race_publish_completed_issue_is_discoverable`,
/// whose final re-publish keeps it schedule-uniform.)
#[test]
fn loom_actual_leaves_two_producers_same_leaf_no_loss_no_dup() {
    leaf_model(move || {
        let backing = Arc::new(LeafBacking::new());
        assert!(backing.leaves.prepare(0, 3));
        assert!(backing.bitmap().issue(0, 3));
        assert!(backing.bitmap().issue(16, 4));
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
        p1.join().unwrap();
        p2.join().unwrap();
        let map = backing.bitmap();
        assert!(!map.publish(32), "never-issued granule must not publish");
        let mut records = Vec::new();
        drain(&map, HIGH_WATER, &mut records);
        assert_union(&records, &[(0, 0, 3), (0, 16, 4)]);
        let mut drained = Vec::new();
        drain(&map, HIGH_WATER, &mut drained);
        assert!(drained.is_empty(), "third discovery finds nothing");
    });
}
