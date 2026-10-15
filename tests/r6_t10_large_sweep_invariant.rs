//! R6-T10(b): a Large cache hit pays only a bounded sidecar probe, and a
//! cache miss runs one full sweep + cache retry so a remotely freed Large
//! span is re-used instead of forcing a fresh OS reservation.
//!
//! - (i) During a Large cache-hit loop the number of sidecar slot
//!   inspections stays within LARGE_HOT_BUDGET (4) per alloc. Red on the
//!   pre-T10(b) code, where every Large alloc swept every active Large slot.
//! - (ii) The probe is not vacuous while live Large routes exist.
//! - Step 5: a block is published from another thread, then a size_b alloc
//!   (a size the cache cannot serve from the warm size_a span) decides. An
//!   alloc that inspected more than LARGE_HOT_BUDGET slots ran the miss
//!   sweep; on that path the reserved-segment counter must not move. If the
//!   bounded probe happened to reach the published slot first, the attempt
//!   is inconclusive and another block is published. Red without the miss
//!   sweep + retry: either a fresh segment is reserved or the miss path is
//!   never observed.
//!
//! Layouts: size_a = SMALL_MAX + PAGE has a 1*SEGMENT usable span; size_b =
//! SEGMENT has a 2*SEGMENT span (header + payload crosses into a second
//! segment). Best-fit admits a cached span only if it is at least the request
//! and at most LARGE_CACHE_SIZE_FACTOR (2) times it, so a size_b request is
//! never served from a size_a span.
//!
//! Requires `alloc-decommit` (the large cache and the miss sweep live under
//! it). Raw pointers are not Send: the publishing thread gets the exposed
//! address and rebuilds the pointer (pattern of
//! `tests/regression_xthread_large_free_no_leak.rs`).
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

use sefer_alloc::alloc_core::AllocCore;
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
    const WARM_HITS: usize = 5; // warm iterations 2..=6 (6 total with the miss)
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

    // Step 5: remote publication + deciding size_b alloc. size_b is never in
    // the cache (the cache holds only the warm 1*SEGMENT size_a span), so the
    // deciding alloc is served either by the bounded entry probe (it happened
    // to reach the victim's slot and deposited its span into the cache) or by
    // the miss sweep + cache retry. Which one runs depends on the hot cursor,
    // so the test observes it: a deciding alloc that inspected more than
    // LARGE_HOT_BUDGET slots ran the miss sweep, and on that path no fresh OS
    // reservation may happen. If the probe caught the victim, the attempt
    // says nothing about the miss path and the next attempt publishes a
    // different block. Every deciding alloc's block stays live until cleanup,
    // so its 2*SEGMENT span cannot re-enter the cache and mask a later miss.
    // Without the miss sweep + retry, an attempt either reserves a fresh
    // segment or never takes the miss path; both fail below.
    const LARGE_HOT_BUDGET: u64 = 4;
    let mut published = Vec::new();
    let mut deciding = Vec::new();
    let mut miss_path_seen = false;
    for attempt in 0..6 {
        let index = K - 1 - attempt * 5;
        let victim = live[index];
        published.push(index);
        // Raw pointers are not Send: hand the exposed address to the thread.
        let address = victim.expose_provenance();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                let ptr = std::ptr::with_exposed_provenance_mut::<u8>(address);
                let pin = RouteDirectory::global().lookup(ptr).expect("issued route");
                assert_eq!(pin.kind(), RouteKind::Large);
                // SAFETY: victim is one live Large allocation issued by the
                // owning thread; this call is its only ownership transfer and
                // no local dealloc runs for it afterward.
                assert!(unsafe { pin.publish_large() });
            });
        });

        let reserved_before = AllocCore::dbg_segments_reserved_total();
        let inspected_before = HeapCore::dbg_large_sidecar_slot_inspections();
        let fresh = heap.alloc(size_b);
        assert!(!fresh.is_null(), "deciding alloc failed");
        let inspected = HeapCore::dbg_large_sidecar_slot_inspections() - inspected_before;
        let reserved = AllocCore::dbg_segments_reserved_total() - reserved_before;
        deciding.push(fresh);
        assert_eq!(
            heap.dbg_drain_sidecar_ingress(),
            0,
            "the remote publication must be consumed by the deciding alloc"
        );
        if inspected > LARGE_HOT_BUDGET {
            assert_eq!(
                reserved, 0,
                "the miss sweep ran ({inspected} slots) but the remote span was \
                 not re-used: a fresh OS reservation happened"
            );
            miss_path_seen = true;
            break;
        }
    }
    assert!(
        miss_path_seen,
        "no deciding alloc took the miss sweep path in 6 attempts"
    );

    // Step 6: cleanup. Published blocks were reclaimed by the sweep or the
    // probe, not freed locally; every other block is freed with its layout.
    for ptr in deciding {
        // SAFETY: each is a live allocation issued by this heap, freed once.
        unsafe { heap.dealloc(ptr, size_b) };
    }
    for (i, ptr) in live.iter().enumerate() {
        if published.contains(&i) {
            continue;
        }
        // SAFETY: each is a live allocation issued by this heap, freed once.
        unsafe { heap.dealloc(*ptr, size_b) };
    }
    let _ = heap.dbg_drain_sidecar_ingress();
    drop(lease);
}
