//! Real ClassLeaves spill/read publication oracle; the Relaxed signal supplies
//! no happens-before edge for fresh-leaf initialization.
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

// Only ClassLeaves is exercised; the included scanner/bitmap siblings are unused.
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

use remote_bitmap::ClassLeaves;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn leaves() -> Arc<ClassLeaves> {
    let mut raw = Box::new(MaybeUninit::<ClassLeaves>::uninit());
    // SAFETY: raw owns a private, aligned, writable ClassLeaves-sized slot;
    // no reference to ClassLeaves or Drop accesses it before initialization.
    #[allow(unsafe_code)]
    unsafe {
        ClassLeaves::initialize_at(raw.as_mut_ptr());
    }
    // SAFETY: initialize_at initialized both complete atomic arrays; the Box
    // retains exclusive ownership and its original allocation/layout.
    #[allow(unsafe_code)]
    let initialized = unsafe { raw.assume_init() };
    Arc::from(initialized)
}

/// First granule of leaf 1 (leaf 0 covers granules 0..256).
const LEAF1_GRANULE: usize = 256;

#[test]
fn mixed_leaf_initialization_is_visible_to_reader() {
    let leaves = leaves();
    // A never-promoted leaf is not prepared and refuses the direct issue.
    assert!(!leaves.prepared(0, 3));
    assert!(!leaves.issue(0, 3));
    assert!(leaves.prepare(0, 3));
    assert!(leaves.issue(0, 3));
    // `prepared` is per-leaf: the promoted leaf 0 is prepared for ANY class.
    assert!(leaves.prepared(1, 4));
    let spilled = Arc::new(AtomicBool::new(false));
    let reader_leaves = Arc::clone(&leaves);
    let reader_spilled = Arc::clone(&spilled);
    let reader = std::thread::spawn(move || {
        while !reader_spilled.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        // The Relaxed flag orders neither the spill nor the issue: the reader
        // may see the null leaf or the published zero fill (0), or the issued
        // code (5). Miri's race detector checks the fill's publication.
        for _ in 0..64 {
            let code = reader_leaves.encoded(LEAF1_GRANULE);
            assert!(code == 0 || code == 5, "unexpected class code {code}");
            std::thread::yield_now();
        }
    });
    // Leaf 1 is still virgin: the prepare below is a real spill whose Release
    // publication of `mixed` must order the zero fill. The Relaxed flag
    // supplies no happens-before edge, so the reader's access to the fresh
    // leaf is ordered only by that publication.
    let prepared = leaves.prepare(LEAF1_GRANULE, 4);
    // Spill alone stores zeros; the start-granule code appears only on issue.
    let issued = leaves.issue(LEAF1_GRANULE, 4);
    // Deliberately not Release: only mixed's publication may synchronize the read.
    spilled.store(true, Ordering::Relaxed);
    reader.join().expect("ClassLeaves reader completed");
    assert!(prepared);
    assert!(issued);
    assert!(leaves.prepared(LEAF1_GRANULE, 4));
    assert_eq!(leaves.encoded(LEAF1_GRANULE), 5);
}
