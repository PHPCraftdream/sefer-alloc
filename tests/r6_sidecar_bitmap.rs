// Compile the actual primitive in this integration crate. These aliases are
// pinned to the production geometry, not independent test constants.
mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
}

#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

mod tests {
    use super::remote_bitmap::{BitmapRecord, SidecarBitmap};
    use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::{Arc, Barrier};

    fn backing() -> (Vec<AtomicU64>, Vec<AtomicU8>) {
        (
            (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            (0..SidecarBitmap::GRANULES)
                .map(|_| AtomicU8::new(0))
                .collect(),
        )
    }

    #[test]
    fn r6_geometry_and_fixed_high_water() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        assert_eq!(SidecarBitmap::WORDS, 4096);
        assert_eq!(SidecarBitmap::GRANULES, 262_144);
        assert!(!map.issue(1, 0));
        assert!(!map.publish(4 * 1024 * 1024));
        assert!(!map.issue(16, u8::MAX));
        assert!(map.issue(1024, 0));
        assert!(map.issue(4 * 1024 * 1024 - 16, 1));
        assert!(map.publish(1024));
        assert!(map.publish(4 * 1024 * 1024 - 16));
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
        assert_eq!(
            records,
            [BitmapRecord {
                offset: 1024,
                class: 0
            }]
        );
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
        assert_eq!(
            final_record,
            Some(BitmapRecord {
                offset: 4 * 1024 * 1024 - 16,
                class: 1
            })
        );
    }

    #[test]
    fn r6_paused_before_and_after_terminal_publish() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        assert!(map.issue(16, 3));
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
                assert!(map_ref.publish(16));
            });
            entered.wait();
            let cut = map.scan(1024).unwrap().next_cut().unwrap();
            assert!(cut.is_empty());
            resume.wait();
        });
        assert!(map.issue(32, 4));
        let published = Arc::new(Barrier::new(2));
        let return_now = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let published_producer = Arc::clone(&published);
            let return_producer = Arc::clone(&return_now);
            let map_ref = &map;
            scope.spawn(move || {
                assert!(map_ref.publish(32));
                published_producer.wait();
                return_producer.wait();
            });
            published.wait();
            let mut cut = map.scan(1024).unwrap().next_cut().unwrap();
            assert_eq!(
                cut.pop().unwrap(),
                BitmapRecord {
                    offset: 16,
                    class: 3
                }
            );
            assert_eq!(
                cut.pop().unwrap(),
                BitmapRecord {
                    offset: 32,
                    class: 4
                }
            );
            assert!(cut.pop().is_none());
            return_now.wait();
        });
    }

    #[test]
    fn r6_reuse_bit_and_republish_class() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        let offset = 64;
        assert!(map.issue(offset, 2));
        assert!(map.publish(offset));
        let mut first = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(first.pop().unwrap().class, 2);
        assert!(first.pop().is_none());
        assert!(map.issue(offset, 5));
        assert!(map.publish(offset));
        let mut second = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(second.pop().unwrap().class, 5);
        assert!(second.pop().is_none());
    }

    #[test]
    fn r6_invalid_class_aborts_without_unwind() {
        const CHILD: &str = "SEFER_R6_SIDECAR_INVALID_CLASS_CHILD";
        if let Some(encoded) = std::env::var_os(CHILD) {
            let encoded: u8 = encoded.to_string_lossy().parse().unwrap();
            let (words, classes) = backing();
            let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
            classes[1].store(encoded, Ordering::Relaxed);
            assert!(map.publish(16));
            let mut cut = map.scan(1024).unwrap().next_cut().unwrap();
            let _ = cut.pop();
            return;
        }

        let executable = std::env::current_exe().unwrap();
        for encoded in [0u8, u8::MAX] {
            let status = std::process::Command::new(&executable)
                .args(["--exact", "tests::r6_invalid_class_aborts_without_unwind"])
                .env(CHILD, encoded.to_string())
                .status()
                .unwrap();
            assert!(
                !status.success(),
                "invalid class {encoded} returned normally"
            );
            assert_ne!(status.code(), Some(101), "invalid class {encoded} unwound");
        }
    }
}
