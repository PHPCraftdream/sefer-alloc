//! Linux D1′ retained-pin RSS/PSS/private-dirty oracle, measurement-only.

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod linux {
    use core::alloc::Layout;
    use proc_probe::snapshot;
    use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};
    use sefer_alloc::SegmentLayout;
    use std::alloc::{GlobalAlloc, System};

    fn smaps_kib(field: &str) -> u64 {
        let contents = std::fs::read_to_string("/proc/self/smaps_rollup")
            .expect("cannot read /proc/self/smaps_rollup");
        contents
            .lines()
            .find_map(|line| {
                let mut parts = line.split_whitespace();
                (parts.next() == Some(field))
                    .then(|| parts.next().and_then(|value| value.parse::<u64>().ok()))
                    .flatten()
            })
            .unwrap_or_else(|| panic!("{field} missing from smaps_rollup"))
    }

    fn mem() -> (u64, u64, u64, u64) {
        let snapshot = snapshot();
        (
            snapshot.rss / 1024,
            snapshot.charged_or_reserved_bytes() / 1024,
            smaps_kib("Pss:"),
            smaps_kib("Private_Dirty:"),
        )
    }

    pub(super) fn run() {
        let directory = RouteDirectory::with_initial_incarnation_for_test(0);
        let layout =
            Layout::from_size_align(SegmentLayout::SEGMENT, SegmentLayout::SEGMENT).unwrap();
        let before = mem();
        // SAFETY: the valid aligned System allocation remains live until the
        // route, owner registration, and all pins have been dropped.
        let root = unsafe { System.alloc(layout) };
        assert!(!root.is_null());
        let after_root = mem();
        let totals_before = SmallSidecar::system_totals_for_test();

        let registration = directory
            .register(root, layout.size(), root, 1, RouteKind::Small)
            .expect("route registration failed");
        #[cfg(feature = "bench-internals")]
        registration.issue_small_for_test(0);
        let pin = directory
            .lookup(root)
            .expect("registered route not pinnable");
        assert!(pin.linked());
        let after_registration = mem();
        let totals_registration = SmallSidecar::system_totals_for_test();
        let sidecar_bytes = totals_registration.0 - totals_before.0;
        assert_eq!(sidecar_bytes, core::mem::size_of::<SmallSidecar>());

        #[cfg(feature = "bench-internals")]
        let chunks_while_pinned = pin
            .dbg_small_sidecar_for_test()
            .expect("registered small route omitted sidecar")
            .initialized_chunks_for_test();
        #[cfg(feature = "bench-internals")]
        assert_eq!(chunks_while_pinned, 1);

        drop(registration);
        assert!(!pin.linked());
        assert!(!pin.pending_for_test(root));
        assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
        let after_unlink_pin_held = mem();
        let totals_unlink = SmallSidecar::system_totals_for_test();
        assert_eq!(totals_unlink.2 - totals_before.2, 0);
        #[cfg(feature = "bench-internals")]
        let refs_after_unlink = RouteDirectory::dbg_route_pin_refs(&pin);
        #[cfg(feature = "bench-internals")]
        assert_eq!(refs_after_unlink, 1);

        drop(pin);
        let after_pin_drop = mem();
        let totals_pin_drop = SmallSidecar::system_totals_for_test();
        assert_eq!(totals_pin_drop.2 - totals_before.2, sidecar_bytes);
        assert_eq!(totals_pin_drop.4 - totals_before.4, 1);

        // SAFETY: owner and pin counts are both released; `root` is still the
        // unique System allocation created above.
        unsafe { System.dealloc(root, layout) };
        let after_root_free = mem();

        println!(
            "RESULT sidecar_size_bytes={}",
            core::mem::size_of::<SmallSidecar>()
        );
        println!("RESULT sidecar_alloc_bytes={sidecar_bytes}");
        println!(
            "RESULT sidecar_free_bytes_owner_unlink={}",
            totals_unlink.2 - totals_before.2
        );
        println!(
            "RESULT sidecar_free_bytes_last_pin={}",
            totals_pin_drop.2 - totals_before.2
        );
        println!(
            "RESULT sidecar_alloc_calls={}",
            totals_pin_drop.3 - totals_before.3
        );
        println!(
            "RESULT sidecar_free_calls={}",
            totals_pin_drop.4 - totals_before.4
        );
        #[cfg(feature = "bench-internals")]
        println!("RESULT constructed_chunks_while_pinned={chunks_while_pinned}");
        #[cfg(feature = "bench-internals")]
        println!("RESULT pin_refs_after_unlink={refs_after_unlink}");
        for (stage, values) in [
            ("before", before),
            ("root", after_root),
            ("registration", after_registration),
            ("owner_unlink_pin_held", after_unlink_pin_held),
            ("last_pin_drop", after_pin_drop),
            ("root_free", after_root_free),
        ] {
            println!("RESULT rss_{stage}_kib={}", values.0);
            println!("RESULT vm_size_{stage}_kib={}", values.1);
            println!("RESULT pss_{stage}_kib={}", values.2);
            println!("RESULT private_dirty_{stage}_kib={}", values.3);
        }
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}
