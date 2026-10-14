#![cfg(all(feature = "alloc-global", feature = "internals", not(miri)))]

use std::alloc::{GlobalAlloc, Layout, System};

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::SeferAlloc;
use sefer_alloc::SegmentLayout;

#[global_allocator]
static ALLOCATOR: SeferAlloc = SeferAlloc::new();

#[test]
fn system_backed_route_metadata_does_not_reenter_global_allocator() {
    const START_WORD: usize = SegmentLayout::SMALL_PAYLOAD_START_WORD;
    const PAYLOAD_START: usize = START_WORD * 64 * SegmentLayout::MIN_BLOCK;
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
    assert!(route.issue_small(PAYLOAD_START as u32, 0));
    let pin = directory.lookup(root).unwrap();
    assert_eq!(pin.owner(), 23);
    // SAFETY: the test owns the issued standalone block and transfers it once.
    assert!(unsafe { pin.publish_small(PAYLOAD_START as u32) });
    let mut scan = route
        .small_sidecar()
        .unwrap()
        .scan_from(PAYLOAD_START + 16, START_WORD)
        .unwrap();
    let mut cut = scan.next_cut().unwrap();
    assert_eq!(cut.pop().unwrap().offset as usize, PAYLOAD_START);
    assert!(cut.pop().is_none());
    drop(cut);
    assert!(scan.next_cut().is_none());
    drop(scan);
    drop(route);
    assert!(directory.lookup(root).is_none());
    // SAFETY: route and pin are gone; no directory entry retains a usable
    // reservation reference, and this is the original System layout.
    unsafe { System.dealloc(root, layout) };
}
