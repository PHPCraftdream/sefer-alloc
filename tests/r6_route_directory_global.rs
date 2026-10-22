#![cfg(all(feature = "alloc-global", feature = "internals", not(miri)))]

use std::alloc::{GlobalAlloc, Layout, System};

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::SeferAlloc;

#[global_allocator]
static ALLOCATOR: SeferAlloc = SeferAlloc::new();

#[test]
fn system_backed_route_metadata_does_not_reenter_global_allocator() {
    const SEGMENT: usize = 4 * 1024 * 1024;
    let layout = Layout::from_size_align(SEGMENT, SEGMENT).unwrap();
    // SAFETY: System allocation is independent of the installed allocator
    // and receives the same pointer/layout exactly once at the end.
    let root = unsafe { System.alloc(layout) };
    assert!(!root.is_null());
    let directory = RouteDirectory::global();
    let route = directory
        .register(root, SEGMENT, root, 23, RouteKind::Small)
        .unwrap();
    route.issue_small_for_test(0);
    assert_eq!(directory.lookup(root).unwrap().owner(), 23);
    // D1' phase-1a: the producer filter rejects any granule below the route
    // kind's metadata end (`payload_start_granule`), so offset 0 — now pure
    // metadata — can never publish. Probe the granule grid until the first
    // publishable offset; rejected probes plant no pending bit (the payload
    // filter fires before the RMW), only a start bit that never becomes a
    // pending cut.
    const MIN_BLOCK: u32 = 16;
    let mut off = 0u32;
    loop {
        route.issue_small_for_test(off);
        let pin = directory.lookup(root).unwrap();
        // SAFETY: the test owns the issued standalone block and transfers it
        // once.
        if unsafe { pin.publish_small(off) } {
            break;
        }
        off += MIN_BLOCK;
    }
    let mut scan = route.small_sidecar().unwrap().scan(SEGMENT).unwrap();
    // `pop` yields byte offsets; empty (pending-free) words still surface as
    // empty cuts, so drain every cut and assert exactly one record.
    let mut records = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(rec) = cut.pop() {
            records.push(rec);
        }
    }
    assert_eq!(records, vec![off]);
    drop(route);
    assert!(directory.lookup(root).is_none());
    // SAFETY: route and pin are gone; no directory entry retains a usable
    // reservation reference, and this is the original System layout.
    unsafe { System.dealloc(root, layout) };
}
