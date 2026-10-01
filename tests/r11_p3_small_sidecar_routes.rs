#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};
use std::sync::Mutex;

const SEGMENT: usize = 4 * 1024 * 1024;
#[cfg(not(miri))]
const LEAVES: usize = 1024;
const LEAF: usize = 4096;
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn actual_system_requests_spills_failure_retry_and_last_pin_release() {
    let _guard = TEST_LOCK.lock().unwrap();
    let before = SmallSidecar::system_totals_for_test();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 9, RouteKind::Small)
        .unwrap();
    let old_incarnation = route.incarnation();
    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(directory.live_route_census_for_test(), (1, 0, 0));
    let base = SmallSidecar::system_totals_for_test();
    assert_eq!(base.0 - before.0, 41_984);
    assert_eq!(base.1 - before.1, 41_984);
    assert_eq!(base.3 - before.3, 1);
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);

    for (offset, class) in [(0, 0), (16, 0), (LEAF as u32, 0)] {
        assert!(sidecar.prepare(offset, class));
        assert!(sidecar.issue(offset, class));
    }
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    assert!(sidecar.prepare(32, 1));
    assert!(sidecar.issue(32, 1));
    assert_eq!(sidecar.class_at_for_test(0), Some(0));
    assert_eq!(sidecar.class_at_for_test(32), Some(1));
    assert_eq!(sidecar.mixed_leaves_for_test(), 1);

    SmallSidecar::fail_spill_after_for_test(1);
    assert!(!sidecar.prepare(LEAF as u32 + 16, 1));
    assert_eq!(sidecar.mixed_leaves_for_test(), 1);
    assert_eq!(sidecar.class_at_for_test(LEAF as u32), Some(0));
    assert!(sidecar.prepare(LEAF as u32 + 16, 1));
    assert!(sidecar.issue(LEAF as u32 + 16, 1));
    assert_eq!(sidecar.mixed_leaves_for_test(), 2);

    let pinned_after_terminal = directory.lookup(root).unwrap();
    for offset in [0, 16, 32, LEAF as u32, LEAF as u32 + 16] {
        let pin = directory
            .lookup(root.with_addr(root.addr() + offset as usize))
            .unwrap();
        // SAFETY: each offset was issued once above and is transferred once.
        assert!(unsafe { pin.publish_small(offset) });
    }
    let mut scan = sidecar.scan(LEAF + 32).unwrap();
    let mut records = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            records.push((record.offset, record.class));
        }
    }
    assert_eq!(
        records,
        [
            (0, 0),
            (16, 0),
            (32, 1),
            (LEAF as u32, 0),
            (LEAF as u32 + 16, 1)
        ]
    );
    drop(scan);
    drop(route);
    assert!(directory.lookup(root).is_none());
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    let held = SmallSidecar::system_totals_for_test();
    assert_eq!(held.0 - before.0, 41_984 + 2 * 256);
    assert_eq!(held.2 - before.2, 0);
    let replacement = directory
        .register(root, SEGMENT, root, 11, RouteKind::Small)
        .unwrap();
    assert!(replacement.incarnation() > old_incarnation);
    assert_eq!(
        replacement.small_sidecar().unwrap().mixed_leaves_for_test(),
        0
    );
    assert_eq!(
        replacement.small_sidecar().unwrap().class_at_for_test(0),
        None
    );
    assert_eq!(directory.live_route_census_for_test(), (1, 0, 0));
    drop(replacement);
    drop(reservation);
    drop(pinned_after_terminal);
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(after.0 - before.0, after.2 - before.2);
    assert_eq!(after.3 - before.3, after.4 - before.4);
}

#[cfg(not(miri))]
#[test]
fn adversarial_all_mixed_real_route_has_no_universal_small_budget() {
    let _guard = TEST_LOCK.lock().unwrap();
    let before = SmallSidecar::system_totals_for_test();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 10, RouteKind::Primordial)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(directory.live_route_census_for_test(), (0, 1, 0));
    for leaf in 0..LEAVES {
        let offset = (leaf * LEAF) as u32;
        assert!(sidecar.prepare(offset, 0));
        assert!(sidecar.issue(offset, 0));
        assert!(sidecar.prepare(offset + 16, 1));
        assert!(sidecar.issue(offset + 16, 1));
        for published in [offset, offset + 16] {
            let ptr = root.with_addr(root.addr() + published as usize);
            let pin = directory.lookup(ptr).unwrap();
            // SAFETY: this route owns each modeled block until this one transfer.
            assert!(unsafe { pin.publish_small(published) });
        }
    }
    let mut scan = sidecar.scan(SEGMENT).unwrap();
    let mut counts = [0usize; 2];
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            counts[record.class as usize] += 1;
        }
    }
    assert_eq!(counts, [LEAVES, LEAVES]);
    drop(scan);
    assert_eq!(sidecar.mixed_leaves_for_test(), LEAVES);
    let live = SmallSidecar::system_totals_for_test();
    assert_eq!(live.0 - before.0, 41_984 + LEAVES * 256);
    assert_eq!(live.1 - before.1, 41_984);
    assert_eq!(live.3 - before.3, 1 + LEAVES);
    drop(route);
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    drop(reservation);
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(after.0 - before.0, after.2 - before.2);
    assert_eq!(after.3 - before.3, after.4 - before.4);
}
