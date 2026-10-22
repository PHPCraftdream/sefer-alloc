#![cfg(feature = "alloc-core")]

// Compile the actual primitive in this integration crate. These aliases are
// pinned to the production geometry, not independent test constants.
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
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

// Allocation-discovery routing is outside this source-included primitive test.
// Included scanner siblings unused by this test (same pattern as
// `tests/class_leaves_publication_miri.rs`).
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

mod tests {
    use super::remote_bitmap::SidecarBitmap;
    use core::sync::atomic::AtomicU64;
    use std::sync::{Arc, Barrier};

    // D1' phase-1b: pending + start banks only — the class-leaves carrier is
    // gone and the class lives only in the owner state byte.
    fn backing() -> (Vec<AtomicU64>, Vec<AtomicU64>) {
        (
            (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            // Start bank, zero-initialized (payload_start 0: the whole
            // segment is payload in this harness).
            (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
        )
    }

    #[test]
    fn r18_concurrent_publications_survive_empty_cuts_and_resume() {
        let (words, starts) = backing();
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();
        map.issue(1024);
        map.issue(1040);
        let mut first = map.scan_from(2048, 1).unwrap();
        let mut records = Vec::new();
        std::thread::scope(|scope| {
            let map_ref = &map;
            let a = scope.spawn(move || assert!(map_ref.publish(1024, 0)));
            let b = scope.spawn(move || assert!(map_ref.publish(1040, 0)));
            let mut cut = first.next_cut().unwrap();
            while let Some(record) = cut.pop() {
                records.push(record);
            }
            assert!(first.next_cut().is_none());
            a.join().unwrap();
            b.join().unwrap();
        });
        let mut later = map.scan_from(2048, 1).unwrap();
        let mut cut = later.next_cut().unwrap();
        while let Some(record) = cut.pop() {
            records.push(record);
        }
        records.sort_by_key(|record| *record);
        assert_eq!(
            records,
            [1024, 1040,],
            "publication lost or duplicated across cuts"
        );
        assert!(later.next_cut().is_none());
        assert!(map
            .scan_from(2048, 1)
            .unwrap()
            .next_cut()
            .unwrap()
            .is_empty());
        map.issue(1024);
        assert!(map.publish(1024, 0));
        let mut valid = map.scan_from(2048, 1).unwrap().next_cut().unwrap();
        assert_eq!(valid.pop().unwrap(), 1024, "nonempty control must drain");
        assert!(valid.pop().is_none());
    }

    #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
    #[test]
    fn r18_candidate_skips_empty_exchange_and_drains_exact_record() {
        use super::remote_bitmap::BitmapScan;

        let (words, starts) = backing();
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();
        BitmapScan::measure_scans(true, false);
        let mut empty = map.scan(1024).unwrap();
        assert!(empty.next_cut().unwrap().is_empty());
        assert!(empty.next_cut().is_none());
        // An unconditional exchange would report [1, 1, 1, 0].
        assert_eq!(&BitmapScan::measure_scans(true, false)[..4], &[1, 1, 0, 0]);

        map.issue(64);
        assert!(map.publish(64, 0));
        let mut scan = map.scan(1024).unwrap();
        let mut cut = scan.next_cut().unwrap();
        assert_eq!(cut.pop(), Some(64));
        assert!(cut.pop().is_none());
        assert!(scan.next_cut().is_none());
        assert_eq!(&BitmapScan::measure_scans(false, true)[..4], &[1, 0, 1, 1]);
        assert!(map.scan(1024).unwrap().next_cut().unwrap().is_empty());
        assert_eq!(BitmapScan::measure_scans(false, true), [0; 5]);
    }

    #[test]
    fn r6_geometry_and_fixed_high_water() {
        let (words, starts) = backing();
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();
        assert_eq!(SidecarBitmap::WORDS, 4096);
        assert_eq!(SidecarBitmap::GRANULES, 262_144);
        assert!(!map.publish(4 * 1024 * 1024, 0));
        map.issue(1024);
        map.issue(4 * 1024 * 1024 - 16);
        assert!(map.publish(1024, 0));
        assert!(map.publish(4 * 1024 * 1024 - 16, 0));
        let mut scan = map.scan(2048).unwrap();
        let mut records = Vec::new();
        let mut cuts = 0;
        while let Some(mut cut) = scan.next_cut() {
            cuts += 1;
            while let Some(record) = cut.pop() {
                records.push(record);
            }
        }
        assert_eq!(cuts, 2);
        assert_eq!(records, [1024]);
        let mut full = map.scan(4 * 1024 * 1024).unwrap();
        let mut final_record = None;
        let mut visits = 0;
        while let Some(mut cut) = full.next_cut() {
            visits += 1;
            if let Some(record) = cut.pop() {
                final_record = Some(record);
            }
        }
        assert_eq!(visits, 4096);
        assert_eq!(final_record, Some(4 * 1024 * 1024 - 16));
    }

    #[test]
    fn r6_paused_before_and_after_terminal_publish() {
        let (words, starts) = backing();
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();
        map.issue(16);
        let mut before = map.scan(1024).unwrap();
        assert!(before.next_cut().unwrap().is_empty());
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let entered_producer = Arc::clone(&entered);
            let resume_producer = Arc::clone(&resume);
            let map_ref = &map;
            scope.spawn(move || {
                entered_producer.wait();
                resume_producer.wait();
                assert!(map_ref.publish(16, 0));
            });
            entered.wait();
            let cut = map.scan(1024).unwrap().next_cut().unwrap();
            assert!(cut.is_empty());
            resume.wait();
        });
        map.issue(32);
        let published = Arc::new(Barrier::new(2));
        let return_now = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let published_producer = Arc::clone(&published);
            let return_producer = Arc::clone(&return_now);
            let map_ref = &map;
            scope.spawn(move || {
                assert!(map_ref.publish(32, 0));
                published_producer.wait();
                return_producer.wait();
            });
            published.wait();
            let mut cut = map.scan(1024).unwrap().next_cut().unwrap();
            assert_eq!(cut.pop().unwrap(), 16);
            assert_eq!(cut.pop().unwrap(), 32);
            assert!(cut.pop().is_none());
            return_now.wait();
        });
    }

    #[test]
    fn r6_reuse_bit_and_republish_class() {
        let (words, starts) = backing();
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();
        let offset = 64;
        map.issue(offset);
        assert!(map.publish(offset, 0));
        let mut first = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(first.pop().unwrap(), 64);
        assert!(first.pop().is_none());
        map.issue(offset);
        assert!(map.publish(offset, 0));
        let mut second = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(second.pop().unwrap(), 64);
        assert!(second.pop().is_none());
    }

    // D1' phase-1b REWRITE: there is no class carrier left to corrupt — the
    // sidecar stores no class state (the former leaf-corruption injection is
    // meaningless). Publication is gated by the start bit alone; the class is
    // decided from the owner state byte in `reclaim_sidecar_record`, which
    // keeps its class >= COUNT fatal.
    #[test]
    fn r6_publication_gated_by_start_bit_alone() {
        let (words, starts) = backing();
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();
        // A never-issued granule: publication is dropped by the start-bit gate.
        assert!(!map.publish(16, 0), "never-issued granule must not publish");
        // Issue sets the start bit; the same granule now publishes and drains.
        map.issue(16);
        assert!(map.publish(16, 0), "publication consults the start bit");
        let mut cut = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(cut.pop(), Some(16));
        assert!(cut.pop().is_none());
    }
}
