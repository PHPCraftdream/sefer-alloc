#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use core::mem::size_of;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};
use std::sync::Mutex;

const SEGMENT: usize = 4 * 1024 * 1024;
// D1' phase-1a: the producer payload filter rejects offsets below the
// route kind's metadata end, so every PUBLISHED offset in these tests is
// based at OFF_BASE (above both `small_meta_end` and `primordial_meta_end`).
const OFF_BASE: u32 = 384 * 1024;
// D1' phase-1b: the sidecar carries pending (32 KiB) + starts (32 KiB) and
// NOTHING else — its total System footprint is exactly 65 536 B.
const SIDECAR_BYTES: usize = 65_536;
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn sidecar_is_exactly_two_32kib_banks() {
    assert_eq!(size_of::<SmallSidecar>(), SIDECAR_BYTES);
}

#[test]
fn actual_system_requests_single_sidecar_and_last_pin_release() {
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
    // SmallSidecar size: pending (4096 × 8 B) + starts (4096 × 8 B) — one
    // System allocation, no promoted leaves (D1' phase-1b).
    assert_eq!(size_of::<SmallSidecar>(), SIDECAR_BYTES);
    assert_eq!(base.0 - before.0, size_of::<SmallSidecar>());
    assert_eq!(base.1 - before.1, 0); // R6-T40 OPT-04: unzeroed System storage.
    assert_eq!(base.3 - before.3, 1);

    // Owner start-bit issue (D1' phase-1b: infallible, no spill, no class
    // carrier — the record's class lives only in the owner state byte).
    for offset in [OFF_BASE, OFF_BASE + 16, OFF_BASE + 4096] {
        sidecar.issue_for_test(offset);
    }

    let pinned_after_terminal = directory.lookup(root).unwrap();
    for offset in [OFF_BASE, OFF_BASE + 16, OFF_BASE + 32, OFF_BASE + 4096] {
        let pin = directory
            .lookup(root.with_addr(root.addr() + offset as usize))
            .unwrap();
        // SAFETY: the offset's start bit was set once above and the transfer
        // happens once here. (OFF_BASE + 32 was never issued: the publication
        // must be rejected by the start-bit gate.)
        assert_eq!(
            unsafe { pin.publish_small(offset) },
            offset != OFF_BASE + 32
        );
    }
    let mut scan = sidecar.scan((OFF_BASE as usize) + 4096 + 16).unwrap();
    let mut records = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(offset) = cut.pop() {
            records.push(offset);
        }
    }
    // D1' phase-1a: cuts carry the offset only.
    assert_eq!(records, [OFF_BASE, OFF_BASE + 16, OFF_BASE + 4096]);
    drop(scan);
    drop(route);
    assert!(directory.lookup(root).is_none());
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    let held = SmallSidecar::system_totals_for_test();
    // D1' phase-1b: no promoted-leaf allocations — the held bytes are still
    // exactly one sidecar.
    assert_eq!(held.0 - before.0, size_of::<SmallSidecar>());
    assert_eq!(held.2 - before.2, 0);
    let replacement = directory
        .register(root, SEGMENT, root, 11, RouteKind::Small)
        .unwrap();
    assert!(replacement.incarnation() > old_incarnation);
    // A fresh incarnation's sidecar is fully zeroed: nothing publishes
    // before a fresh owner issue.
    // SAFETY: a stale publication of the OLD incarnation's granule; the new
    // sidecar's zeroed start bank must reject it without mutations.
    let pin = directory
        .lookup(root.with_addr(root.addr() + OFF_BASE as usize))
        .unwrap();
    assert!(!unsafe { pin.publish_small(OFF_BASE) });
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
fn adversarial_dense_issue_publish_drain_has_fixed_one_sidecar_cost() {
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
    // Dense issue across many distinct 4 KiB spans (the former leaf-domain
    // sweep): each span's start granule is issued and published.
    const SPANS: usize = 896;
    for span in 0..SPANS {
        let offset = OFF_BASE + (span * 4096) as u32;
        sidecar.issue_for_test(offset);
        sidecar.issue_for_test(offset + 16);
        for published in [offset, offset + 16] {
            let ptr = root.with_addr(root.addr() + published as usize);
            let pin = directory.lookup(ptr).unwrap();
            // SAFETY: this route owns each modeled block until this one transfer.
            assert!(unsafe { pin.publish_small(published) });
        }
    }
    let mut scan = sidecar.scan(SEGMENT).unwrap();
    // D1' phase-1a: the cut no longer carries a class, so the drained
    // per-class split is asserted as one total (2 records per span).
    let mut drained = 0usize;
    while let Some(mut cut) = scan.next_cut() {
        while let Some(_offset) = cut.pop() {
            drained += 1;
        }
    }
    assert_eq!(drained, 2 * SPANS);
    drop(scan);
    let live = SmallSidecar::system_totals_for_test();
    // D1' phase-1b: the dense sweep costs exactly ONE fixed sidecar — no
    // per-leaf promotions, no per-issue System requests.
    assert_eq!(live.0 - before.0, size_of::<SmallSidecar>());
    assert_eq!(live.1 - before.1, 0);
    assert_eq!(live.3 - before.3, 1);
    drop(route);
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    drop(reservation);
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(after.0 - before.0, after.2 - before.2);
    assert_eq!(after.3 - before.3, after.4 - before.4);
}
