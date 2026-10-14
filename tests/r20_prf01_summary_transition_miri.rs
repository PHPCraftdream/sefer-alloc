//! Miri real RoutePin P2 pause across strict trim and Small reservation release.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::{LargeCacheConfig, SeferAlloc, SmallSegmentPoolConfig};
use std::{
    alloc::{GlobalAlloc, Layout},
    ptr,
    sync::mpsc,
    thread,
};

#[test]
fn miri_real_pin_p2_strict_trim_releases_small_reservation_before_p3_resume() {
    let layout = Layout::from_size_align(sefer_alloc::SegmentLayout::SMALL_MAX, 8).unwrap();
    let (allocation_tx, allocation_rx) = mpsc::channel();
    let (paused_tx, paused_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let (trim_tx, trim_rx) = mpsc::channel();
    let (owner_done_tx, owner_done_rx) = mpsc::channel();

    let owner = thread::spawn(move || {
        let allocator = SeferAlloc::with_config(
            LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0)),
        );
        // Large-enough Small allocations fill the first Small segment; retain
        // that segment's blocks as anchors and target the first block in the next.
        const ANCHOR_CAPACITY: usize =
            sefer_alloc::SegmentLayout::SEGMENT / sefer_alloc::SegmentLayout::SMALL_MAX + 1;
        let mut anchors = [core::ptr::null_mut(); ANCHOR_CAPACITY];
        let mut anchor_count = 0usize;
        let mut first_base = None;
        let target = loop {
            // SAFETY: valid layout and live allocator; this allocation remains an anchor.
            let candidate = unsafe { allocator.alloc(layout) };
            assert!(!candidate.is_null());
            let base = candidate.addr() & !(sefer_alloc::SegmentLayout::SEGMENT - 1);
            match first_base {
                None => {
                    first_base = Some(base);
                    anchors[anchor_count] = candidate;
                    anchor_count += 1;
                }
                Some(initial) if base == initial => {
                    anchors[anchor_count] = candidate;
                    anchor_count += 1;
                }
                Some(_) => break candidate,
            }
            assert!(
                anchor_count < anchors.len(),
                "fixture failed to reach a second Small segment"
            );
        };
        let pin = RouteDirectory::global()
            .lookup(target)
            .expect("issued Small route");
        assert_eq!(pin.kind(), RouteKind::Small);
        drop(pin);
        allocation_tx.send(target.addr()).unwrap();

        let before = allocator.dbg_prf01_strict_words_visited();
        let released_before = allocator.stats().segments_released_total;
        paused_rx.recv().unwrap(); // notification is inside real RoutePin hook after P2
        allocator.trim_current_thread();
        let released_after = allocator.stats().segments_released_total;
        let after = allocator.dbg_prf01_strict_words_visited();
        trim_tx
            .send((
                after.1 - before.1,
                after.0 - before.0,
                before.2,
                before.3,
                released_after - released_before,
            ))
            .unwrap();
        owner_done_rx.recv().unwrap();
        // SAFETY: every anchor is a distinct current allocation; free each once.
        unsafe {
            for &anchor in &anchors[..anchor_count] {
                allocator.dealloc(anchor, layout);
            }
        }
        allocator.trim_current_thread();
    });

    let address = allocation_rx.recv().unwrap();
    let producer = thread::spawn(move || {
        let ptr = ptr::without_provenance_mut::<u8>(address);
        let pin = RouteDirectory::global()
            .lookup(ptr)
            .expect("target route pin");
        assert_eq!(pin.kind(), RouteKind::Small);
        let offset = (address & (sefer_alloc::SegmentLayout::SEGMENT - 1)) as u32;
        // SAFETY: owner transfers this live Small allocation once. P2 is the real
        // sidecar publication; callback blocks without touching reservation/sidecar.
        assert!(unsafe {
            pin.dbg_publish_small_paused_after_p2(offset, || {
                paused_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            })
        });
    });

    let (records, visited, end, start, released) = trim_rx.recv().unwrap();
    assert_eq!(
        records, 1,
        "strict trim reclaims P2 despite absent summary P3"
    );
    assert_eq!(
        visited,
        end - start,
        "strict scan visits only payload words"
    );
    assert!(start >= 64 && end > start);
    assert_eq!(
        released, 1,
        "trim releases target reservation while producer pin remains live"
    );
    resume_tx.send(()).unwrap();
    producer.join().unwrap();
    owner_done_tx.send(()).unwrap();
    owner.join().unwrap();
}
