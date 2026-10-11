#[cfg(target_os = "linux")]
mod gate {
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};
    use mimalloc::MiMalloc;
    use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
    use sefer_alloc::SeferAlloc;
    use std::alloc::{GlobalAlloc, Layout};
    use std::hint::black_box;
    const CHURN_OPS: usize = 64;
    const CHURN_OPS_2N: usize = CHURN_OPS * 2;
    const MAGAZINE_FILL: usize = 16;
    const PREFILL_CYCLES: usize = CHURN_OPS / MAGAZINE_FILL;
    fn claim_leaked_heap() -> &'static mut HeapCore {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned null");
        // SAFETY: the slot lives in the `'static` registry array; forgetting the
        // lease suppresses its LIVE -> FREE `Drop`, so the slot stays LIVE and
        // the exclusive borrow stays valid for `'static` exactly like the legacy
        // raw pointer. No other claimant can win while the slot is LIVE.
        let heap = unsafe { &mut *(lease.core() as *mut HeapCore) };
        core::mem::forget(lease);
        heap
    }
    #[library_benchmark]
    fn small_churn_16b() {
        let sefer = SeferAlloc::new();
        let layout = Layout::from_size_align(16, 8).unwrap();
        for _ in 0..CHURN_OPS {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            let ptr = unsafe { sefer.alloc(layout) };
            black_box(ptr);
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the immediately preceding `alloc`
                // call with the same layout.
                unsafe { sefer.dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn small_churn_16b_2n() {
        let sefer = SeferAlloc::new();
        let layout = Layout::from_size_align(16, 8).unwrap();
        for _ in 0..CHURN_OPS_2N {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            let ptr = unsafe { sefer.alloc(layout) };
            black_box(ptr);
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the immediately preceding `alloc`
                // call with the same layout.
                unsafe { sefer.dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn dealloc_prealloc_only_16b() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);
        // Deliberately leaked (never freed): this arm exists ONLY to measure the
        // shared pre-allocation prefix's own Ir, common to both sibling arms
        // below. Each `#[library_benchmark]` runs in its own fresh process under
        // callgrind, so leaking here has no effect on any other bench.
    }

    #[library_benchmark]
    fn dealloc_free_only_16b_n8() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);

        // Timed region: free the first 8 of the 64 pre-allocated pointers.
        for &ptr in &ptrs[..8] {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the same
                // layout, and is freed exactly once.
                unsafe { (*heap).dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn dealloc_free_only_16b_n9() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);

        // Timed region: free the first 9 of the 64 pre-allocated pointers.
        for &ptr in &ptrs[..9] {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the same
                // layout, and is freed exactly once.
                unsafe { (*heap).dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn dealloc_free_only_16b_n16() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);

        // Timed region: free the first 16 of the 64 pre-allocated pointers.
        for &ptr in &ptrs[..16] {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the same
                // layout, and is freed exactly once.
                unsafe { (*heap).dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn mimalloc_small_churn_16b() {
        let mi = MiMalloc;
        let layout = Layout::from_size_align(16, 8).unwrap();
        for _ in 0..CHURN_OPS {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            let ptr = unsafe { mi.alloc(layout) };
            black_box(ptr);
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the immediately preceding `alloc`
                // call with the same layout.
                unsafe { mi.dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn mimalloc_small_churn_16b_2n() {
        let mi = MiMalloc;
        let layout = Layout::from_size_align(16, 8).unwrap();
        for _ in 0..CHURN_OPS_2N {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            let ptr = unsafe { mi.alloc(layout) };
            black_box(ptr);
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the immediately preceding `alloc`
                // call with the same layout.
                unsafe { mi.dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn large_alloc_free_cycle() {
        let sefer = SeferAlloc::new();
        let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
        // SAFETY: layout has non-zero size and valid alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the `alloc` call directly above with
            // the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }

    #[library_benchmark]
    fn alloc_magazine_hit_only_16b() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; MAGAZINE_FILL] = [core::ptr::null_mut(); MAGAZINE_FILL];
        for _ in 0..PREFILL_CYCLES {
            for slot in ptrs.iter_mut() {
                *slot = (*heap).alloc(layout);
            }
            black_box(&ptrs);
            for &ptr in &ptrs {
                if !ptr.is_null() {
                    // SAFETY: ptr was returned by the alloc loop above with the
                    // same layout, freed exactly once.
                    unsafe { (*heap).dealloc(ptr, layout) };
                }
            }
        }
        // Timed-in-spirit region: drain the magazine ONE more time. `count`
        // starts at 16 (just populated by the last prefill cycle) and every one
        // of these 16 `alloc` calls pops from it -- all 16 are magazine HITS,
        // never a miss/refill.
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);
    }

    #[library_benchmark]
    fn alloc_magazine_prefill_only_16b() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; MAGAZINE_FILL] = [core::ptr::null_mut(); MAGAZINE_FILL];
        for _ in 0..PREFILL_CYCLES {
            // Carve 16 fresh blocks (never magazine-resident before -- these are
            // carve/refill misses, not hits).
            for slot in ptrs.iter_mut() {
                *slot = (*heap).alloc(layout);
            }
            black_box(&ptrs);
            // Populate the magazine: free all 16 -- each push lands in the
            // magazine (count 0 -> 16), through the real
            // `dealloc_own_thread_with_base` push path. After the LAST cycle,
            // the magazine is left holding 16 resident blocks and this arm does
            // NOT drain them -- that is exactly the shared prefix
            // `alloc_magazine_hit_only_16b` below adds ONE more step onto.
            for &ptr in &ptrs {
                if !ptr.is_null() {
                    // SAFETY: ptr was returned by the alloc loop above with the
                    // same layout, freed exactly once.
                    unsafe { (*heap).dealloc(ptr, layout) };
                }
            }
        }
    }

    #[library_benchmark]
    fn dealloc_own_thread_body_only_16b() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);

        // Timed region: `segment_base_of_ptr` + the real own-thread free body,
        // deliberately skipping `contains_base` (already isolated by R23-1).
        // Actually frees every block through the production push/oracle path --
        // no bypass, no alternate implementation.
        for &ptr in &ptrs {
            if !ptr.is_null() {
                let base = (*heap).dbg_segment_base_of_ptr(ptr);
                // SAFETY: `ptr` was returned by the alloc loop above with `layout`
                // and is freed exactly once here; `base` is `ptr`'s true segment
                // base (`dealloc_routing`'s own `contains_base` check already
                // proved this same relationship in the sibling production path).
                unsafe { (*heap).dbg_dealloc_own_thread_with_base(ptr, layout, base) };
            }
        }
    }

    #[library_benchmark]
    fn dealloc_segment_base_of_ptr_probe_only_16b() {
        let _ = bootstrap::ensure();
        let heap = claim_leaked_heap();
        let layout = Layout::from_size_align(16, 8).unwrap();

        let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
        for slot in ptrs.iter_mut() {
            *slot = (*heap).alloc(layout);
        }
        black_box(&ptrs);

        // Timed region: `segment_base_of_ptr` ALONE -- deliberately does NOT call
        // `dbg_contains_base`, unlike the composite probe arm above. Isolates the
        // base-computation half of the routing prefix.
        for &ptr in &ptrs {
            if !ptr.is_null() {
                let base = (*heap).dbg_segment_base_of_ptr(ptr);
                black_box(base);
            }
        }
    }
    library_benchmark_group!(name = perf_gate; benchmarks = small_churn_16b,small_churn_16b_2n,dealloc_prealloc_only_16b,dealloc_free_only_16b_n8,dealloc_free_only_16b_n9,dealloc_free_only_16b_n16,mimalloc_small_churn_16b,mimalloc_small_churn_16b_2n,large_alloc_free_cycle,alloc_magazine_hit_only_16b,alloc_magazine_prefill_only_16b,dealloc_own_thread_body_only_16b,dealloc_segment_base_of_ptr_probe_only_16b);
    main!(library_benchmark_groups = perf_gate);
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
