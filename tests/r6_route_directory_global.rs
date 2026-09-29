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
    assert!(route.small_sidecar().unwrap().issue(0, 0));
    let pin = directory.lookup(root).unwrap();
    assert_eq!(pin.owner(), 23);
    assert!(pin.publish_small(0));
    let mut scan = route.small_sidecar().unwrap().scan(16).unwrap();
    let mut cut = scan.next_cut().unwrap();
    assert_eq!(cut.pop().unwrap().offset, 0);
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
