//! R6-T55 (review R6-CR-10): `foreign_or_unroutable_frees` must count ONLY
//! foreign route drops — an address-only route-lookup miss or a rejected
//! terminal sidecar publication. A LOCAL (own-thread) rejection is
//! caller-misuse diagnostics, not a routing failure, and must leave the
//! public counter flat: an interior-address Large free on the owner's own
//! path lands in the internals-gated `LOCAL_REJECT_FREES` counter instead.
//!
//! Counterfactual (RED before R6-T55): `reject_large_interior_free` bumped
//! the public counter on the local interior Large free, so the first public
//! assertion below failed — a local double-free probe was indistinguishable
//! from a routing drop.
//!
//! One test owns every process-global counter observation; no subprocesses,
//! no concurrent tests in this file, no global-allocator installation (the
//! `tests/r19_large_interior_free.rs` convention).

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;
use std::thread;

use sefer_alloc::{AllocCore, SeferAlloc, SegmentLayout};

#[test]
fn local_rejects_are_not_foreign_or_unroutable_frees() {
    let allocator = SeferAlloc::new();
    // Segment-aligned Large: a valid exact free releases the segment in every
    // feature row (the large cache does not retain `align >= SEGMENT` blocks),
    // so the re-alloc below is a real liveness witness, not a cache hit.
    let big = Layout::from_size_align(4 * 1024 * 1024, SegmentLayout::SEGMENT).unwrap();

    // ── Local interior Large free, exact Large layout ──────────────────────
    // SAFETY: valid nonzero layout; the block stays live until its exact free.
    let ptr = unsafe { allocator.alloc(big) };
    assert!(!ptr.is_null(), "large allocation failed");

    let stats_before = SeferAlloc::new().stats();
    let rejects_before = AllocCore::dbg_local_reject_frees();

    // SAFETY: deliberate interior-address rejection probe into a mapped, live
    // Large block; the guard must reject it as a local no-op.
    unsafe { allocator.dealloc(ptr.wrapping_add(64), big) };

    let stats_after = SeferAlloc::new().stats();
    assert_eq!(
        stats_after.foreign_or_unroutable_frees, stats_before.foreign_or_unroutable_frees,
        "a local interior Large free must NOT bump foreign_or_unroutable_frees"
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        rejects_before + 1,
        "the local interior Large free must land in LOCAL_REJECT_FREES"
    );

    // M2 witness: the probe consumed no live credit — the exact start still
    // frees exactly once (a consumed credit aborts here, r19's convention).
    // SAFETY: exact issued start/layout, still owned after the rejection.
    unsafe { allocator.dealloc(ptr, big) };

    // ── Same rule for a small-classified layout on the same block ──────────
    // SAFETY: valid nonzero layout; freed exactly once below.
    let ptr = unsafe { allocator.alloc(big) };
    assert!(!ptr.is_null(), "re-allocation failed");
    let small = Layout::from_size_align(64, 16).unwrap();
    let stats_before = SeferAlloc::new().stats();
    let rejects_before = AllocCore::dbg_local_reject_frees();
    // SAFETY: deliberate interior probe with a deliberately wrong (small)
    // layout; every configuration rejects it before touching the block.
    unsafe { allocator.dealloc(ptr.wrapping_add(64), small) };
    let stats_after = SeferAlloc::new().stats();
    assert_eq!(
        stats_after.foreign_or_unroutable_frees, stats_before.foreign_or_unroutable_frees,
        "a local small-layout interior free must NOT bump foreign_or_unroutable_frees"
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        rejects_before + 1,
        "the local small-layout interior free must land in LOCAL_REJECT_FREES"
    );
    // SAFETY: exact issued start/layout; this is its only free.
    unsafe { allocator.dealloc(ptr, big) };

    // ── Control: a FOREIGN route miss still grows the public counter ───────
    // The complement (r12_01's producer shape, without its abort probe): a
    // free of a never-issued metadata address, from a thread that never bound
    // a heap, is dropped by `publish_foreign` and MUST grow the public
    // counter while `LOCAL_REJECT_FREES` stays flat.
    let (tx, rx) = mpsc::channel();
    let (drain_tx, drain_rx) = mpsc::channel::<()>();
    let owner = thread::spawn(move || {
        let allocator = SeferAlloc::new();
        // SAFETY: valid layout; the block stays live until after the probe.
        let live = unsafe { allocator.alloc(small) };
        assert!(!live.is_null());
        let base = live.addr() & !(SegmentLayout::SEGMENT - 1);
        tx.send(base).expect("send base");
        drain_rx.recv().expect("drain signal");
        allocator.trim_current_thread();
        // SAFETY: this thread's only live allocation, freed once.
        unsafe { allocator.dealloc(live, small) };
    });
    let base = rx.recv().expect("owner base");
    let before = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    let rejects_before = AllocCore::dbg_local_reject_frees();
    let foreign = thread::spawn(move || {
        // SAFETY: malformed pointer on purpose — segment metadata that is
        // never an issued block; it must be dropped, never dereferenced.
        unsafe { SeferAlloc::new().dealloc((base + 16) as *mut u8, small) };
    });
    foreign.join().expect("foreign free thread");
    drain_tx.send(()).expect("drain");
    owner.join().expect("owner thread");
    let after = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    assert!(
        after > before,
        "a foreign route miss must still grow foreign_or_unroutable_frees: \
         {before} -> {after}"
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        rejects_before,
        "a foreign drop must not touch LOCAL_REJECT_FREES"
    );
}
