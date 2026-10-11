//! VER-03 / owner decision 8: remote Large and Small frees must not touch
//! reservation bytes, including after terminal publication. Small publishes
//! a sidecar bitmap bit; Large claims the sidecar phase/generation word.
//! Sidecar pin release and notifications may follow, but reservation/header
//! and owner-only ledger accesses may not (REMOTE_FREE_SIDECAR_REVISION).
//!
//! The owner keeps its exclusive HeapLease and repeatedly calls
//! dbg_segment_state_reconciliation without allocating, draining or trimming.
//! Its Large arm copies the full header via SegmentMeta::header/read_at; its
//! Small arm currently reads individual owner-only header fields, NOT a full
//! Small header copy. Both kinds must remain registered until the final join.
//! Publication alone cannot change this owner-only reconciliation snapshot.
//!
//! The start barrier orders setup before both arms, not producer frees before
//! owner reads. There is deliberately no completion signal, per-free handshake
//! or join until ALL snapshots finish. Yields encourage interleaving; they do
//! not prove physical overlap. Miri's race detector is the race oracle; native
//! assertions additionally prove both routes were exercised and consumed once.
//!
//! Observed negative control (optimized fixture): after the anchored real
//! producer dealloc, a temporary `usize` header-word read/write at the saved
//! canonical base produced `Data race detected`: owner header-copy read versus
//! producer write (node.rs:310; temporary test line 122). Plain Miri with
//! preemption-rate=0.5, isolation disabled: exit 1 after 376 seconds (rerun).
//! Backup restore plus cmp exited 0; git diff --stat was checked afterward.
//! No src mutation was needed. A payload-only write is NOT observed by this
//! header oracle. A green run is not an exhaustive no-reservation-access proof.
//!
//! Cost: a clean plain-Miri run takes about 524 seconds on a loaded dev host
//! (the earlier full-primordial-burn fixture took 900 seconds). Almost all of
//! it is Small setup: the first Small allocation after the primordial high
//! water is marked exhausted. The 2 frees per kind, 8 snapshots and 16-byte
//! payloads are already minimal. The <=200 s goal was NOT met; reaching it
//! needs a cheaper way to open a non-primordial Small segment in `src/`.
//!
//! Intended for plain Miri: address transport deliberately uses exposed
//! provenance; this is not a strict-provenance/Stacked-Borrows acceptance test.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]

use std::alloc::Layout;
use std::sync::{Arc, Barrier};
use std::thread;

use sefer_alloc::alloc_core::SegmentLayout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn owner_reconciliation_overlaps_remote_large_and_small_free() {
    // This binary contains one test; no other test shares its global registry.
    let _ = bootstrap::ensure();
    let mut owner = HeapRegistry::dbg_claim_lease().expect("owner lease");
    const REMOTE_PER_KIND: usize = 2;
    const SNAPSHOTS: usize = 8;
    // Alignment above SMALL_MAX forces Large even for a tiny payload.
    let large_layout =
        Layout::from_size_align(16, SegmentLayout::SMALL_MAX.next_power_of_two()).unwrap();
    let small_layout = Layout::from_size_align(16, 16).unwrap();

    // Each record carries the original allocation address, matching Layout,
    // and an owner-canonical header base for the later test-only counterfactual.
    // The unmutated producer never dereferences that saved base.
    let mut transfers = Vec::with_capacity(2 * REMOTE_PER_KIND);
    for _ in 0..REMOTE_PER_KIND {
        let ptr = owner.core().alloc(large_layout);
        assert!(!ptr.is_null(), "Large allocation");
        let pin = RouteDirectory::global().lookup(ptr).expect("Large route");
        assert_eq!(pin.kind(), RouteKind::Large);
        let base = owner
            .core()
            .segment_bases()
            .find(|base| base.addr() == SegmentLayout::segment_base_of(ptr.addr()))
            .expect("registered Large root");
        transfers.push((ptr as usize, large_layout, base as usize));
    }

    // The reserve-and-keep hook registers a Small root but deliberately does
    // not switch the carve cursor; it cannot by itself bypass primordial.
    // Skip virgin primordial payload instead: bump is the first repr(C) header
    // field. No payload has been issued there, and no free lists exist yet.
    let primordial_base = owner
        .core()
        .segment_bases()
        .next()
        .expect("primordial root");
    // SAFETY: the exclusive owner lease controls this live, aligned header;
    // setup is single-threaded, bump is an initialized usize at offset zero,
    // and SEGMENT marks unused payload exhausted without issuing live credits.
    unsafe {
        primordial_base
            .cast::<usize>()
            .write(SegmentLayout::SEGMENT)
    };
    for _ in 0..REMOTE_PER_KIND {
        let ptr = owner.core().alloc(small_layout);
        assert!(!ptr.is_null(), "Small allocation");
        let pin = RouteDirectory::global().lookup(ptr).expect("Small route");
        assert_eq!(pin.kind(), RouteKind::Small, "not Primordial");
        let base = owner
            .core()
            .segment_bases()
            .find(|base| base.addr() == SegmentLayout::segment_base_of(ptr.addr()))
            .expect("registered Small root");
        transfers.push((ptr as usize, small_layout, base as usize));
        // SAFETY: no primordial payload was ever issued; the cursor has moved
        // to Small, so restore its original high water before further scans.
        unsafe {
            primordial_base
                .cast::<usize>()
                .write(SegmentLayout::PRIMORDIAL_META_END)
        };
    }
    let before = owner.core().dbg_segment_state_reconciliation();
    assert_eq!(before.unknown_count, 0);
    assert_eq!(before.large_active.count, REMOTE_PER_KIND);
    assert!(before.small_active.count > 0, "registered Small fixture");

    let start = Arc::new(Barrier::new(2));
    let remote_start = Arc::clone(&start);
    let remote = thread::spawn(move || {
        let mut producer = HeapRegistry::dbg_claim_lease().expect("producer lease");
        remote_start.wait();
        let count = transfers.len();
        for (address, layout, _header_base_address) in transfers {
            let ptr = address as *mut u8;
            // SAFETY: ptr is the exact live start issued by the still-owned
            // owner heap, layout matches, and this thread alone transfers it
            // once. The owner does not access payload or reclaim before join.
            unsafe { producer.core().dealloc(ptr, layout) };
            // VER03_POST_FREE_MUTANT_ANCHOR: no reservation access after free.
            thread::yield_now();
        }
        count
    });

    start.wait();
    for _ in 0..SNAPSHOTS {
        let snapshot = owner.core().dbg_segment_state_reconciliation();
        // Remote publication changes sidecars, not table/header/cache/ledger.
        assert_eq!(snapshot, before, "publication changed owner-only state");
        thread::yield_now();
    }
    assert_eq!(
        remote.join().expect("producer panicked"),
        2 * REMOTE_PER_KIND
    );
    assert_eq!(owner.core().dbg_segment_state_reconciliation(), before);

    // No allocation-based implicit drain: observe actual exactly-once ingress.
    assert_eq!(
        owner.core().dbg_drain_sidecar_ingress(),
        2 * REMOTE_PER_KIND
    );
    assert_eq!(owner.core().dbg_drain_sidecar_ingress(), 0);
    let after = owner.core().dbg_segment_state_reconciliation();
    assert_eq!(after.large_active.count, 0, "Large publications retired");
    assert_eq!(after.unknown_count, 0);
    assert_eq!(
        after.total.count + after.unknown_count + after.table_recycled_null_slots,
        after.table_high_water + after.large_cached.count
    );

    // Lease teardown applies owner-side retention/recycle policy after join.
    drop(owner);
}
