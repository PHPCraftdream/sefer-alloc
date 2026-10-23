//! D1′ retained-pin memory/lifetime oracle, measurement-only.
//!
//! Uses the existing RouteDirectory owner API and a counted RoutePin. It is a
//! component lifetime probe, not an allocator timing benchmark: the route root
//! is a valid System allocation and no reservation bytes are dereferenced.

#![cfg(feature = "internals")]

use core::alloc::Layout;
use proc_probe::snapshot;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};
use sefer_alloc::SegmentLayout;
use std::alloc::{GlobalAlloc, System};

fn memory_kib() -> (u64, u64) {
    let snapshot = snapshot();
    (
        snapshot.rss / 1024,
        snapshot.charged_or_reserved_bytes() / 1024,
    )
}

fn main() {
    let directory = RouteDirectory::with_initial_incarnation_for_test(0);
    let layout = Layout::from_size_align(SegmentLayout::SEGMENT, SegmentLayout::SEGMENT).unwrap();
    let memory_before = memory_kib();
    // SAFETY: the valid aligned System allocation is retained until all route
    // descriptors and pins are dropped, then deallocated with this exact layout.
    let root = unsafe { System.alloc(layout) };
    assert!(!root.is_null());
    let memory_after_root = memory_kib();
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
    let memory_registered = memory_kib();
    let totals_registered = SmallSidecar::system_totals_for_test();
    let sidecar_alloc_bytes = totals_registered.0 - totals_before.0;
    assert_eq!(sidecar_alloc_bytes, core::mem::size_of::<SmallSidecar>());

    #[cfg(feature = "bench-internals")]
    let pin_refs_before_unlink = RouteDirectory::dbg_route_pin_refs(&pin);
    #[cfg(feature = "bench-internals")]
    let constructed_chunks = pin
        .dbg_small_sidecar_for_test()
        .expect("registered small route omitted sidecar")
        .initialized_chunks_for_test();
    #[cfg(feature = "bench-internals")]
    assert_eq!(constructed_chunks, 1);

    drop(registration);
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    assert!(!pin.linked(), "unlink must retire new lookup visibility");
    assert!(
        !pin.pending_for_test(root),
        "existing pin retains readable sidecar state"
    );
    let totals_owner_unlinked = SmallSidecar::system_totals_for_test();
    assert_eq!(
        totals_owner_unlinked.2 - totals_before.2,
        0,
        "owner unlink must not free a sidecar retained by a pin"
    );
    let memory_pin_held = memory_kib();
    #[cfg(feature = "bench-internals")]
    let pin_refs_after_unlink = RouteDirectory::dbg_route_pin_refs(&pin);
    #[cfg(feature = "bench-internals")]
    assert_eq!(pin_refs_after_unlink, 1);

    drop(pin);
    let totals_last_pin_dropped = SmallSidecar::system_totals_for_test();
    assert_eq!(
        totals_last_pin_dropped.2 - totals_before.2,
        sidecar_alloc_bytes,
        "last pin must release the sidecar backing"
    );
    assert_eq!(totals_last_pin_dropped.4 - totals_before.4, 1);
    let memory_after_pin_drop = memory_kib();
    // SAFETY: the local directory has no linked routes and the counted route
    // pin was dropped above; `root` is still this oracle's System allocation.
    unsafe { System.dealloc(root, layout) };
    let memory_after_root_free = memory_kib();

    println!(
        "RESULT sidecar_size_bytes={}",
        core::mem::size_of::<SmallSidecar>()
    );
    println!("RESULT sidecar_alloc_bytes={sidecar_alloc_bytes}");
    println!(
        "RESULT sidecar_free_bytes_after_owner_unlink={}",
        totals_owner_unlinked.2 - totals_before.2
    );
    println!(
        "RESULT sidecar_free_bytes_after_last_pin={}",
        totals_last_pin_dropped.2 - totals_before.2
    );
    println!(
        "RESULT sidecar_alloc_calls_after_last_pin={}",
        totals_last_pin_dropped.3 - totals_before.3
    );
    println!(
        "RESULT sidecar_free_calls_after_last_pin={}",
        totals_last_pin_dropped.4 - totals_before.4
    );
    #[cfg(feature = "bench-internals")]
    println!("RESULT constructed_chunks_while_pinned={constructed_chunks}");
    #[cfg(feature = "bench-internals")]
    println!("RESULT pin_refs_before_unlink={pin_refs_before_unlink}");
    #[cfg(feature = "bench-internals")]
    println!("RESULT pin_refs_after_unlink={pin_refs_after_unlink}");
    println!("RESULT rss_before_kib={}", memory_before.0);
    println!("RESULT rss_after_root_kib={}", memory_after_root.0);
    println!("RESULT rss_after_registration_kib={}", memory_registered.0);
    println!(
        "RESULT rss_after_owner_unlink_pin_held_kib={}",
        memory_pin_held.0
    );
    println!(
        "RESULT rss_after_last_pin_drop_kib={}",
        memory_after_pin_drop.0
    );
    println!(
        "RESULT rss_after_root_free_kib={}",
        memory_after_root_free.0
    );
    println!("RESULT commit_before_kib={}", memory_before.1);
    println!("RESULT commit_after_root_kib={}", memory_after_root.1);
    println!(
        "RESULT commit_after_registration_kib={}",
        memory_registered.1
    );
    println!(
        "RESULT commit_after_owner_unlink_pin_held_kib={}",
        memory_pin_held.1
    );
    println!(
        "RESULT commit_after_last_pin_drop_kib={}",
        memory_after_pin_drop.1
    );
    println!(
        "RESULT commit_after_root_free_kib={}",
        memory_after_root_free.1
    );
}
