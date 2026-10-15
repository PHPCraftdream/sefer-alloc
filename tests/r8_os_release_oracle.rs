//! Exact-token OS release after owner exit and the last remote free.
#![cfg(all(
    not(miri),
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals",
    any(target_os = "linux", all(windows, target_pointer_width = "64"))
))]

#[path = "support/r8_native_os_mapping.rs"]
mod native;

use std::time::{Duration, Instant};

use native::{mapping_at, Mapping};
use sefer_alloc::global::MaintenanceService;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::{SeferAlloc, SegmentLayout};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const BOUND: Duration = Duration::from_secs(15);

#[repr(align(8388608))]
#[allow(dead_code)]
struct Overaligned([u8; 1]);

type Geometry = (usize, usize, usize); // OS token, token length, usable root.

fn geometry(ptr: *mut u8) -> Geometry {
    // SAFETY: caller supplies the exact start of this owner's live allocation.
    unsafe { GLOBAL.dbg_current_reservation_for_test(ptr) }.expect("owner's canonical reservation")
}

fn route_at(address: usize) -> Option<RouteKind> {
    RouteDirectory::global()
        .lookup(core::ptr::without_provenance_mut::<u8>(address))
        .map(|pin| pin.kind())
}

fn assert_live_geometry(address: usize, (token, len, root): Geometry, kind: RouteKind) {
    assert_eq!(route_at(address), Some(kind));
    assert_ne!(token, 0);
    assert!(len > 0);
    assert!(token <= root && root < token.checked_add(len).expect("token extent"));
    assert!(root <= address && address < token + len);
    assert_eq!(mapping_at(token), Mapping::Mapped);
}

fn wait_for_release(address: usize, token: usize) {
    let deadline = Instant::now() + BOUND;
    loop {
        let before = MaintenanceService::passes_for_test();
        if route_at(address).is_none() {
            let after_unlink = MaintenanceService::passes_for_test();
            assert!(
                MaintenanceService::wait_after_for_test(
                    after_unlink,
                    deadline.saturating_duration_since(Instant::now()),
                ),
                "no completed worker pass after route unlink"
            );
            assert!(route_at(address).is_none(), "retired route reappeared");
            assert_eq!(mapping_at(token), Mapping::Unmapped);
            return;
        }
        assert!(
            MaintenanceService::wait_after_for_test(
                before,
                deadline.saturating_duration_since(Instant::now()),
            ),
            "worker stopped before route retirement"
        );
        assert!(Instant::now() < deadline, "route was not retired");
    }
}

fn unlink_without_release_negative_control() {
    let reservation = aligned_vmem::reserve_aligned(SegmentLayout::SEGMENT, SegmentLayout::SEGMENT)
        .expect("native calibration reservation");
    let root = reservation.as_ptr();
    let address = root.addr();
    let token = reservation.reservation_ptr().addr();
    let route = RouteDirectory::global()
        .register(root, reservation.len(), root, usize::MAX, RouteKind::Large)
        .expect("synthetic route registration");
    assert_eq!(route_at(address), Some(RouteKind::Large));
    assert_eq!(mapping_at(token), Mapping::Mapped);
    drop(route); // Unlink only; reservation remains owned and mapped.
    assert_eq!(route_at(address), None);
    assert_eq!(mapping_at(token), Mapping::Mapped);
    drop(reservation);
    assert_eq!(mapping_at(token), Mapping::Unmapped);
}

#[test]
fn owner_exit_last_remote_free_returns_exact_os_tokens() {
    assert_eq!(
        core::mem::align_of::<Overaligned>(),
        2 * SegmentLayout::SEGMENT
    );
    assert_eq!(
        core::mem::size_of::<Overaligned>(),
        2 * SegmentLayout::SEGMENT
    );
    unlink_without_release_negative_control();
    SeferAlloc::start_maintenance().expect("explicit maintenance activation");
    let (small, small_geo, large, large_geo, biased, biased_geo) = std::thread::spawn(|| {
        let count = 2 * SegmentLayout::SEGMENT / 2048;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            blocks.push(vec![0xa5u8; 2048].into_boxed_slice());
        }
        let small = blocks.pop().expect("ordinary Small segment");
        drop(blocks);
        let large = vec![0x5au8; 5 * 1024 * 1024].into_boxed_slice();
        let biased = Box::<Overaligned>::new_uninit();
        let small_geo = geometry(small.as_ptr().cast_mut());
        let large_geo = geometry(large.as_ptr().cast_mut());
        let biased_geo = geometry(biased.as_ptr().cast_mut().cast::<u8>());
        (small, small_geo, large, large_geo, biased, biased_geo)
    })
    .join()
    .expect("owner exits normally");

    let small_address = small.as_ptr().addr();
    let large_address = large.as_ptr().addr();
    let biased_address = biased.as_ptr().addr();
    assert_live_geometry(small_address, small_geo, RouteKind::Small);
    assert_live_geometry(large_address, large_geo, RouteKind::Large);
    assert_live_geometry(biased_address, biased_geo, RouteKind::Large);
    assert_eq!(biased_address % (2 * SegmentLayout::SEGMENT), 0);
    assert_eq!(small[2047], 0xa5);
    assert_eq!(large[large.len() - 1], 0x5a);

    drop(small);
    drop(large);
    drop(biased); // Last free; only passive pass waits and OS queries follow.
    wait_for_release(large_address, large_geo.0);
    wait_for_release(biased_address, biased_geo.0);
    #[cfg(feature = "alloc-decommit")]
    wait_for_release(small_address, small_geo.0);
}
