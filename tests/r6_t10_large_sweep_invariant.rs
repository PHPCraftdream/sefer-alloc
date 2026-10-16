//! R6-T10(b): every Large alloc pays only a bounded sidecar probe (at most
//! LARGE_HOT_BUDGET slots through the persistent cursor), and the rotating
//! probe retires a remotely published Large block within a bounded number of
//! allocs (the R12-05 design `alloc_batch` already follows).
//!
//! - (i) During a Large cache-hit loop the number of sidecar slot
//!   inspections stays within LARGE_HOT_BUDGET (4) per alloc. Red on the
//!   pre-T10(b) code, where every Large alloc swept every active Large slot.
//! - (ii) The probe is not vacuous while live Large routes exist.
//! - Step 5: a block is published from another thread; further cache hits
//!   must each stay within the budget and must retire the published route
//!   within ceil((K + 1) / LARGE_HOT_BUDGET) + 1 hits.
//!
//! Layouts: size_a = SMALL_MAX + PAGE has a 1*SEGMENT usable span; size_b =
//! SEGMENT has a 2*SEGMENT span, so the K live size_b blocks never feed the
//! size_a cache slot the hit loop uses.
//!
//! Requires `alloc-decommit` (the large cache lives under it). Raw pointers
//! are not Send: the publishing thread gets the exposed address and rebuilds
//! the pointer (pattern of `tests/regression_xthread_large_free_no_leak.rs`).
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]
#![allow(unused_unsafe)] // The RoutePin API is becoming unsafe in the parent branch.

use core::alloc::Layout;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

// Tests share the process-wide route directory and allocator; a block freed
// by one test can be reissued to another at the same address. Serialize them.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn large_alloc_probes_boundedly_and_never_loses_remote_publication() {
    let _guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Step 1: owner claims the exclusive lease.
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();

    // Step 2: two Large layouts with distinct usable spans.
    // size_a: > SMALL_MAX (Large class), align PAGE < SEGMENT (large-cache
    // eligible under alloc-decommit); usable span rounds to 1 * SEGMENT.
    let size_a = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    // size_b: exactly SEGMENT bytes → `hdr_aligned + size` crosses into the
    // second segment → usable span 2 * SEGMENT. Large (SEGMENT > SMALL_MAX),
    // align PAGE < SEGMENT. A size_b request can never be served from a
    // size_a span (see header).
    let size_b = Layout::from_size_align(SegmentLayout::SEGMENT, SegmentLayout::PAGE).unwrap();

    // Step 3: K live Large blocks of size_b.
    const K: usize = 24;
    let mut live = Vec::with_capacity(K);
    for _ in 0..K {
        let ptr = heap.alloc(size_b);
        assert!(!ptr.is_null());
        live.push(ptr);
    }

    // Step 4: warm the large cache with a size_a span. Iteration 1 is a MISS
    // (cache empty): it pays the hot probe (4) PLUS the miss-path full sweep
    // (over all 24 active slots) — that is exactly the machinery under test,
    // so it is EXCLUDED from the bounded-probe accounting. Iterations 2..=6
    // are pure cache hits (each alloc takes the size_a span the previous
    // dealloc deposited; the byte budget is unbounded by default —
    // LargeCacheConfig `n: None`, src/alloc_core/config/large_cache_config.rs).
    let warm = heap.alloc(size_a);
    assert!(!warm.is_null());
    // SAFETY: the block returned immediately above; freed once here.
    unsafe { heap.dealloc(warm, size_a) };
    const WARM_HITS: u64 = 5; // warm iterations 2..=6 (6 total with the miss)
    let before = HeapCore::dbg_large_sidecar_slot_inspections();
    for _ in 0..WARM_HITS {
        let p = heap.alloc(size_a);
        assert!(!p.is_null(), "cache-hit alloc failed");
        // SAFETY: the block returned immediately above; freed once here.
        unsafe { heap.dealloc(p, size_a) };
    }
    let d = HeapCore::dbg_large_sidecar_slot_inspections().saturating_sub(before);

    // (i) bounded probe: at most LARGE_HOT_BUDGET (4) inspections per hit.
    // RED on pre-T10(b) full-sweep-per-alloc code (~24 inspections/alloc).
    assert!(
        d <= 4 * WARM_HITS,
        "Large alloc inspected {d} sidecar slots over {WARM_HITS} cache hits; \
         expected <= LARGE_HOT_BUDGET * hits = {} (full-sweep pre-T10(b) code \
         inspects ~every active Large slot per alloc)",
        4 * WARM_HITS
    );
    // (ii) the probe is not vacuous while live Large routes exist.
    assert!(d > 0, "probe never inspected any Large slot");

    // Step 5: publish one live block from another thread, then keep doing
    // size_a cache hits. Each hit may inspect at most LARGE_HOT_BUDGET slots,
    // and the rotating cursor must reach and retire the published block
    // within ceil(active / LARGE_HOT_BUDGET) + 1 hits. Without the bounded
    // probe on Large alloc (pre-R6-T10 code swept every slot) the per-hit
    // bound fails; without any probe the block is never retired.
    const LARGE_HOT_BUDGET: u64 = 4;
    let victim_index = K - 1;
    let victim = live[victim_index];
    // Raw pointers are not Send: hand the exposed address to the thread.
    let address = victim.expose_provenance();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let ptr = std::ptr::with_exposed_provenance_mut::<u8>(address);
            let pin = RouteDirectory::global().lookup(ptr).expect("issued route");
            assert_eq!(pin.kind(), RouteKind::Large);
            // SAFETY: victim is one live Large allocation issued by the
            // owning thread; this call is its only ownership transfer and no
            // local dealloc runs for it afterward.
            assert!(unsafe { pin.publish_large() });
        });
    });

    let max_hits = (K as u64 + 1).div_ceil(LARGE_HOT_BUDGET) + 1;
    let mut hits = 0u64;
    while RouteDirectory::global().lookup(victim).is_some() {
        assert!(
            hits < max_hits,
            "published Large not retired after {hits} hits"
        );
        let before = HeapCore::dbg_large_sidecar_slot_inspections();
        let p = heap.alloc(size_a);
        assert!(!p.is_null(), "cache-hit alloc failed");
        // SAFETY: the block returned immediately above; freed once here.
        unsafe { heap.dealloc(p, size_a) };
        let inspected = HeapCore::dbg_large_sidecar_slot_inspections() - before;
        assert!(
            inspected <= LARGE_HOT_BUDGET,
            "hit {hits} inspected {inspected} slots; budget {LARGE_HOT_BUDGET}"
        );
        hits += 1;
    }

    // Step 6: cleanup. The published block was retired by the probe, not
    // freed locally; every other block is freed with its own layout.
    for (i, ptr) in live.iter().enumerate() {
        if i == victim_index {
            continue;
        }
        // SAFETY: each is a live allocation issued by this heap, freed once.
        unsafe { heap.dealloc(*ptr, size_b) };
    }
    let _ = heap.dbg_drain_sidecar_ingress();
    drop(lease);
}
