//! Ph5c gap (б) — registry saturation → fallback → recovery (matrix #2095,
//! inventory gap P1).
//!
//! Mechanism under test: when every registry slot (`MAX_HEAPS`, see
//! `src/registry/bootstrap/registry.rs`) is LIVE and the free pool is empty,
//! `HeapRegistry::claim_lease()` returns `None`, `finish_bind(None)` maps that
//! to `CurrentHeap::Fallback` (`src/global/tls_heap.rs`) and the alloc face
//! serves the thread through the always-live primordial fallback heap
//! (`src/global/fallback.rs`, spinlock-guarded).
//!
//! What is proven here, in order:
//!
//! 1. **Saturation**: holding a live lease for every registry slot
//!    (`HeapRegistry::dbg_claim_lease()` drained to `None`) makes a fresh
//!    thread's `alloc` resolve to the fallback — observed through the
//!    process-wide `dbg_fallback_lock_acquisitions()` counter, which advances
//!    exactly when the fallback spinlock is taken by the fallback alloc route.
//! 2. **Cross-thread dealloc of a fallback block**: a fallback-issued block
//!    freed from a *different* (never-bound) thread completes correctly and
//!    its contents are intact up to the free.
//! 3. **Recovery**: after one lease is dropped (its `Drop` flips the slot
//!    `LIVE → FREE`, slot stays materialised), a fresh thread is served by the
//!    REGISTRY again — asserted *negatively*: the fallback-lock-acquisitions
//!    counter does not move across the whole alloc + use + dealloc of that
//!    thread.
//!
//! ## Hygiene (CI findings)
//!
//! - The whole body runs on a dedicated worker thread; the harness thread
//!   only `join()`s it (panic propagates through `join().unwrap()`).
//! - No unbounded spin-waiting anywhere: thread handoff uses channels
//!   (`recv()` blocks on the peer, and a hung peer surfaces as a hung
//!   `join`, not a busy loop) and every peer's panic is re-raised via
//!   `join().unwrap()`.
//! - The registry is process-global for this binary; the test makes NO
//!   assertion about specific slot indices — saturation is observed
//!   behaviourally (claim returns `None`; fallback counter moves), never by
//!   poking slot identities.
//! - Not built under `numa-aware`: there every `HeapCore`'s primordial is an
//!   EAGER full-commit 4 MiB reservation (lazy commit is off to keep NUMA
//!   placement, `src/alloc_core/alloc_core/bootstrap.rs`), so draining the
//!   registry commits ~`MAX_HEAPS` x 4 MiB (~9 GiB) of host commit charge —
//!   a machine-load test, not a routing test. Routing is NUMA-independent.
//! - The repeated-bind-after-TORN surface is covered elsewhere
//!   (`tls_heap_teardown_torn_sentinel.rs`, `r31_10` ac4b/ac4c,
//!   `dealloc_only_no_bind_torn.rs`) and deliberately NOT duplicated here.
//!
//! Run:
//!   cargo test --test r11_ph5c_registry_saturation_fallback --features "production internals"

#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::global::dbg_fallback_lock_acquisitions;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

const BLOCK: Layout = match Layout::from_size_align(64, 16) {
    Ok(l) => l,
    Err(_) => panic!("bad const layout"),
};

#[test]
fn registry_saturation_routes_to_fallback_and_back() {
    // Hygiene: the entire body on its own thread; the harness thread only
    // joins. A panic inside the body is re-raised here via `unwrap()`.
    let handle = std::thread::Builder::new()
        .name("ph5c-saturation-body".into())
        .spawn(saturate_fallback_recover)
        .expect("spawn body thread");
    handle.join().expect("body thread panicked");
}

fn saturate_fallback_recover() {
    // ---- Phase 0: saturate the registry -----------------------------------
    // Hold a live lease per slot until the claim itself reports exhaustion
    // (`None`). No slot-index assertions: the process-global registry may
    // already hold live slots from earlier activity in this binary — the
    // drain-to-`None` loop is the saturation oracle, not any count.
    let mut leases = Vec::new();
    while let Some(lease) = HeapRegistry::dbg_claim_lease() {
        leases.push(lease);
    }
    assert!(
        !leases.is_empty(),
        "registry never yielded a single lease — bootstrap broken"
    );

    // ---- Phase 1: saturated alloc is served by the fallback ---------------
    let (tx, rx) = std::sync::mpsc::channel::<usize>();
    let before_alloc = dbg_fallback_lock_acquisitions();
    let producer = std::thread::spawn(move || {
        let ptr = unsafe { SeferAlloc::new().alloc(BLOCK) };
        assert!(!ptr.is_null(), "fallback alloc returned null (M10)");
        // Stamp the block so cross-thread free integrity is observable.
        // SAFETY: `ptr` is a live, uniquely owned `BLOCK` allocation.
        unsafe { ptr.write_bytes(0xA5, BLOCK.size()) };
        tx.send(ptr as usize).expect("receiver hung up");
    });
    producer.join().expect("producer thread panicked");
    let after_alloc = dbg_fallback_lock_acquisitions();
    assert!(
        after_alloc > before_alloc,
        "saturated alloc did not take the fallback spinlock \
         (acquisitions {before_alloc} -> {after_alloc}): fallback route not exercised"
    );
    let addr = rx.recv().expect("producer did not send the block");

    // ---- Phase 2: cross-thread dealloc of the fallback block --------------
    let (freed_tx, freed_rx) = std::sync::mpsc::channel::<()>();
    let freer = std::thread::spawn(move || {
        let ptr = std::ptr::with_exposed_provenance_mut::<u8>(addr);
        // Contents survive the handoff (still live, not yet freed).
        // SAFETY: `ptr` is the live fallback block handed over by value.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, BLOCK.size()) };
        assert!(
            bytes.iter().all(|&b| b == 0xA5),
            "fallback block corrupted before the cross-thread free"
        );
        // SAFETY: exact original layout, single free of a live block.
        unsafe { SeferAlloc::new().dealloc(ptr, BLOCK) };
        freed_tx.send(()).expect("main body hung up");
    });
    freer.join().expect("freer thread panicked");
    freed_rx
        .recv()
        .expect("freer finished without signalling (join raced)");

    // ---- Phase 3: free one slot → registry service is restored ------------
    drop(leases.pop().expect("at least one lease held"));
    let before_recovered = dbg_fallback_lock_acquisitions();
    let recovered = std::thread::spawn(move || {
        let ptr = unsafe { SeferAlloc::new().alloc(BLOCK) };
        assert!(!ptr.is_null(), "post-recovery alloc returned null");
        // SAFETY: live uniquely owned allocation.
        unsafe { ptr.write_bytes(0x5A, BLOCK.size()) };
        // SAFETY: exact original layout, single free.
        unsafe { SeferAlloc::new().dealloc(ptr, BLOCK) };
    });
    recovered.join().expect("recovered thread panicked");
    let after_recovered = dbg_fallback_lock_acquisitions();
    assert_eq!(
        after_recovered, before_recovered,
        "post-slot-release alloc still took the fallback spinlock: \
         freed slot was not reclaimed for registry service"
    );

    // All remaining leases drop here (LIVE → FREE each); nothing else to
    // assert — slot reuse recycling is covered by dedicated tests.
    drop(leases);
}
