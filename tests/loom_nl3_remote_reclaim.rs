//! NL3 reduced owner-reclaim model. Included sidecar/bitmap source exercises publish/cut
//! identity conservation, followed by modeled owner insertion and reuse. No lease claim
//! protocol is modeled. This does NOT include or prove the full path in
//! `src/alloc_core/small/alloc_core_small_reclaim.rs`, whose production raw-pointer
//! reclaim still reads the state byte. Identity/free-list and credit fields are bookkeeping.
#![cfg(all(loom, feature = "alloc-global", feature = "alloc-xthread"))]
mod alloc_core {
    pub mod fatal {
        pub fn fatal(_: &'static [u8]) -> ! {
            panic!("fatal in Loom model")
        }
    }
    pub mod os {
        pub const SEGMENT: usize = 3 * 64 * 16;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize = 16 * 64;
    }
}
pub(crate) use loom::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};
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
pub(crate) use bitmap_cut::BitmapCut;
pub(crate) use bitmap_record::BitmapRecord;
pub(crate) use bitmap_scan::BitmapScan;
mod remote_bitmap {
    #[allow(unused_imports)]
    pub(crate) use super::bitmap_cut::BitmapCut;
    #[allow(unused_imports)]
    pub(crate) use super::bitmap_record::BitmapRecord;
    #[allow(unused_imports)]
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}
use loom::sync::Arc;
use loom::thread;
use remote_bitmap::SidecarBitmap;

type Id = (u32, u8); // (offset, encoded production class)
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
    fn map(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_initialized(&self.words, &self.classes).unwrap()
    }
}
// Bound traversal: a production pop mutant that fails to clear cut bits fails promptly.
fn drain(map: &SidecarBitmap<'_>, limit: usize) -> Vec<Id> {
    let mut scan = map.scan(3 * 64 * 16).unwrap();
    let mut out = Vec::new();
    let mut steps = 0;
    while let Some(mut cut) = scan.next_cut() {
        while let Some(r) = cut.pop() {
            steps += 1;
            assert!(steps <= limit, "cut failed to retire bits");
            out.push((r.offset, r.class + 1));
        }
    }
    out
}
fn assert_exact(actual: &[Id], expected: &[Id]) {
    let mut a = actual.to_vec();
    a.sort_unstable();
    let mut e = expected.to_vec();
    e.sort_unstable();
    assert_eq!(a, e, "block identity/class multiset");
}
// Same commit path is used by positive and negative controls; fault repeats an
// actual detached record as if owner retirement ran twice.
fn commit(free: &mut Vec<Id>, credits: &mut usize, records: &[Id], double_retire: bool) {
    for &record in records {
        free.push(record);
        if double_retire {
            free.push(record);
        }
        *credits -= 1;
    }
}

#[test]
fn actual_cut_races_distinct_word_producers_and_reuses_taken_identity() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let m = b.map();
        assert!(m.issue(0, 0));
        assert!(m.issue(1024, 1));
        let p0 = {
            let b = b.clone();
            thread::spawn(move || {
                thread::yield_now();
                assert!(b.map().publish(0));
            })
        };
        let p1 = {
            let b = b.clone();
            thread::spawn(move || {
                thread::yield_now();
                assert!(b.map().publish(1024));
            })
        };
        let first = drain(&m, 2);
        p0.join().unwrap();
        p1.join().unwrap();
        let mut all = first;
        all.extend(drain(&m, 2));
        assert_exact(&all, &[(0, 1), (1024, 2)]);
        let mut free = Vec::new();
        let mut credits = 2;
        commit(&mut free, &mut credits, &all, false);
        assert_eq!(credits, 0);
        assert_exact(&free, &[(0, 1), (1024, 2)]);
        // Pop the exact identity before a new allocation instance is issued.
        let index = free.iter().position(|id| *id == (1024, 2)).unwrap();
        let reused = free.swap_remove(index);
        credits += 1;
        assert!(m.issue(reused.0, reused.1 - 1));
        assert!(m.publish(reused.0));
        let next = drain(&m, 1);
        commit(&mut free, &mut credits, &next, false);
        assert_eq!(credits, 0);
        assert_exact(&free, &[(0, 1), (1024, 2)]);
    });
}

#[test]
fn prepublished_same_word_pair_is_cut_as_two_records() {
    loom::model(|| {
        let b = Backing::new();
        let m = b.map();
        assert!(m.issue(64, 0));
        assert!(m.issue(128, 1));
        assert!(m.publish(64));
        assert!(m.publish(128));
        let actual = drain(&m, 2);
        assert_exact(&actual, &[(64, 1), (128, 2)]);
    });
}

#[test]
fn detached_record_keeps_credit_until_owner_insertion() {
    loom::model(|| {
        let b = Backing::new();
        let m = b.map();
        assert!(m.issue(32, 0));
        let mut credits = 1usize;
        assert!(m.publish(32));
        let detached = drain(&m, 1);
        assert_eq!(credits, 1, "cut/detach does not retire credit");
        let mut free = Vec::new();
        commit(&mut free, &mut credits, &detached, false);
        assert_eq!(credits, 0);
        assert_exact(&free, &[(32, 1)]);
    });
}

#[test]
#[should_panic(expected = "block identity/class multiset")]
fn modeled_double_retirement_negative_control() {
    loom::model(|| {
        let b = Backing::new();
        let m = b.map();
        assert!(m.issue(32, 0));
        assert!(m.publish(32));
        let detached = drain(&m, 1);
        let mut credits = 1;
        let mut free = Vec::new();
        commit(&mut free, &mut credits, &detached, true);
        assert_exact(&free, &[(32, 1)]);
    });
}
