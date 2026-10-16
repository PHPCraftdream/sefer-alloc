//! R6-T10 Ir cells: Large cache hits with many live Large objects (own iai
//! target; `perf_gate_iai` is over its file-length budget and must not grow).
//!
//! Every Large alloc walks the heap's live Large sidecar slots before the
//! cache search, so the per-hit cost grows with the number of live Large
//! objects. The prefill cell pins `LIVE_LARGE` 8 MiB objects (a size the
//! 4 MiB cache slot never serves) and warms the cache once; the hit cell adds
//! `LIVE_LARGE_HITS` 4 MiB cache-hit rounds on top. The difference divided by
//! `LIVE_LARGE_HITS` is the cost of one cache hit with `LIVE_LARGE` live
//! Large objects. Entry layer: `HeapCore::alloc`/`dealloc`, the chain the
//! `#[global_allocator]` uses.

#[cfg(target_os = "linux")]
mod gate {
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};
    use std::hint::black_box;

    #[cfg(feature = "alloc-decommit")]
    use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
    #[cfg(feature = "alloc-decommit")]
    use std::alloc::Layout;

    #[cfg(feature = "alloc-decommit")]
    const LIVE_LARGE: usize = 16;
    #[cfg(feature = "alloc-decommit")]
    const LIVE_LARGE_BYTES: usize = 8 * 1024 * 1024;
    #[cfg(feature = "alloc-decommit")]
    const LIVE_LARGE_HITS: usize = 16;
    #[cfg(feature = "alloc-decommit")]
    const CACHED_BYTES: usize = 4 * 1024 * 1024;

    /// Exclusive heap whose lease is never released: every
    /// `#[library_benchmark]` arm runs in its own process (same shim as
    /// `perf_gate_iai`'s `claim_leaked_heap`).
    #[cfg(feature = "alloc-decommit")]
    fn claim_leaked_heap() -> &'static mut HeapCore {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
        // SAFETY: the slot lives in the `'static` registry array; forgetting
        // the lease keeps it LIVE, so the exclusive borrow stays valid for
        // `'static`. No other claimant can win while the slot is LIVE.
        let heap = unsafe { &mut *(lease.core() as *mut HeapCore) };
        core::mem::forget(lease);
        heap
    }

    #[cfg(feature = "alloc-decommit")]
    fn live_large_prefill(heap: &mut HeapCore) -> Layout {
        let live = Layout::from_size_align(LIVE_LARGE_BYTES, 8).unwrap();
        for _ in 0..LIVE_LARGE {
            let ptr = heap.alloc(live);
            black_box(ptr);
            assert!(!ptr.is_null(), "live Large allocation failed");
            // Deliberately never freed: these objects stay live for the cell.
        }
        let layout = Layout::from_size_align(CACHED_BYTES, 8).unwrap();
        let ptr = heap.alloc(layout);
        black_box(ptr);
        assert!(!ptr.is_null(), "cache warm-up allocation failed");
        // SAFETY: ptr was returned by the alloc call directly above with the
        // same layout, freed exactly once; the free deposits it into the
        // large cache.
        unsafe { heap.dealloc(ptr, layout) };
        layout
    }

    #[cfg(feature = "alloc-decommit")]
    #[library_benchmark]
    fn large_cache_live16_prefill_only() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        black_box(live_large_prefill(heap));
    }

    #[cfg(feature = "alloc-decommit")]
    #[library_benchmark]
    fn large_cache_live16_hit_only() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = live_large_prefill(heap);
        for _ in 0..LIVE_LARGE_HITS {
            let ptr = heap.alloc(layout);
            black_box(ptr);
            assert!(!ptr.is_null(), "cache-hit allocation failed");
            // SAFETY: ptr was returned by the alloc call directly above with
            // the same layout, freed exactly once.
            unsafe { heap.dealloc(ptr, layout) };
        }
    }

    #[cfg(not(feature = "alloc-decommit"))]
    #[library_benchmark]
    fn large_cache_live16_prefill_only() {
        black_box(0u8);
    }

    #[cfg(not(feature = "alloc-decommit"))]
    #[library_benchmark]
    fn large_cache_live16_hit_only() {
        black_box(0u8);
    }

    library_benchmark_group!(
        name = live_large;
        benchmarks = large_cache_live16_prefill_only, large_cache_live16_hit_only);
    main!(library_benchmark_groups = live_large);
    pub fn run() {
        main();
    }
}

#[cfg(target_os = "linux")]
fn main() {
    gate::run();
}
#[cfg(not(target_os = "linux"))]
fn main() {}
