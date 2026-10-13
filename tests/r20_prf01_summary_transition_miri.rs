//! Miri check of the real `SidecarBitmap` C'' summary transition: a producer's
//! first free into an empty pending word sets the summary bit (0 -> non-0), a
//! repeated free into the SAME word touches no summary, and owner discovery
//! (summary cleared before pending cuts) still reclaims both records.
#![cfg(feature = "alloc-core")]
#![deny(unsafe_code)]

mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
}

// Only SidecarBitmap/ClassLeaves are exercised; the included siblings are partly unused.
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

use remote_bitmap::{ClassLeaves, SidecarBitmap};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const WORDS: usize = 4096;
const SUMMARY_WORDS: usize = 64;

struct Backing {
    pending: Box<[AtomicU64; WORDS]>,
    summary: Box<[AtomicU64; SUMMARY_WORDS]>,
    leaves: ClassLeaves,
}

fn backing() -> Arc<Backing> {
    let mut raw = Box::new(MaybeUninit::<ClassLeaves>::uninit());
    // SAFETY: raw owns a private, aligned, writable ClassLeaves-sized slot;
    // nothing observes it before initialization and assume_init keeps the
    // original allocation, which then owns the fully initialized value.
    #[allow(unsafe_code)]
    let leaves = unsafe {
        ClassLeaves::initialize_at(raw.as_mut_ptr());
        // ClassLeaves is plain initialized atomics with no Drop; the box move
        // keeps the original allocation, which then owns the value.
        *raw.assume_init()
    };
    Arc::new(Backing {
        pending: Box::new([const { AtomicU64::new(0) }; WORDS]),
        summary: Box::new([const { AtomicU64::new(0) }; SUMMARY_WORDS]),
        leaves,
    })
}

fn records(scan: remote_bitmap::BitmapScan<'_>) -> Vec<(u32, u8)> {
    let mut scan = scan;
    let mut records = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            records.push((record.offset, record.class));
        }
    }
    records
}

#[test]
fn summary_set_once_on_zero_to_nonzero_transition() {
    let backing = backing();
    let bitmap =
        SidecarBitmap::from_leaves(&backing.pending[..], &backing.summary[..], &backing.leaves)
            .expect("backing lengths");
    // Owner-side class issue for the two granules the producer will free.
    assert!(bitmap.issue(0, 3));
    assert!(bitmap.issue(16, 3));
    let producer_backing = Arc::clone(&backing);
    let producer = std::thread::spawn(move || {
        let producer_bitmap = SidecarBitmap::from_leaves(
            &producer_backing.pending[..],
            &producer_backing.summary[..],
            &producer_backing.leaves,
        )
        .expect("backing lengths");
        // Same pending word (both offsets < 64 * MIN_BLOCK): the first free is
        // the word's 0 -> non-0 transition; the second must not touch summary.
        assert!(producer_bitmap.publish(0));
        // Erase the hint: a free that sees a non-zero prev must leave it erased.
        producer_backing.summary[0].store(0, Ordering::Relaxed);
        assert!(producer_bitmap.publish(16));
        assert_eq!(
            producer_backing.summary[0].load(Ordering::Relaxed),
            0,
            "a free into a non-empty pending word touched the summary"
        );
        // Restore the hint so owner discovery below can find the word.
        producer_backing.summary[0].fetch_or(1, Ordering::Release);
    });
    producer.join().expect("producer completed");
    assert!(
        backing.summary[0].load(Ordering::Relaxed) & 1 == 1,
        "0 -> non-0 transition must set the summary bit"
    );
    let discovery = bitmap.scan_summarized_from(32, 0).expect("scan");
    assert_eq!(records(discovery), [(0, 3), (16, 3)]);
    // The cut cleared every summary word; nothing remains anywhere.
    let again = bitmap.scan_summarized_from(32, 0).expect("scan");
    assert!(records(again).is_empty(), "discovery must be idempotent");
    assert!(records(bitmap.scan(32).expect("scan")).is_empty());
}
