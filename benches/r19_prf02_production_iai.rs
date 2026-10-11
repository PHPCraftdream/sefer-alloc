#[cfg(target_os = "linux")]
mod gate {
    use core::alloc::Layout;
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};
    use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
    use std::hint::black_box;

    fn heap() -> &'static mut HeapCore {
        let _ = bootstrap::ensure();
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
        let ptr = lease.core() as *mut HeapCore;
        core::mem::forget(lease);
        // SAFETY: forgotten exclusive lease retains the static registry slot LIVE.
        unsafe { &mut *ptr }
    }

    #[inline(always)]
    fn free_prefix<const N: usize>() {
        let heap = heap();
        let layout = Layout::from_size_align(16, 8).unwrap();
        let mut ptrs = [core::ptr::null_mut(); 64];
        for slot in &mut ptrs {
            *slot = heap.alloc(layout);
        }
        black_box(&ptrs);
        for &ptr in &ptrs[..N] {
            if !ptr.is_null() {
                // SAFETY: distinct live block with matching layout, freed once.
                unsafe { heap.dealloc(ptr, layout) };
            }
        }
    }

    #[library_benchmark]
    fn dealloc_prealloc_only_16b() {
        free_prefix::<0>();
    }
    #[library_benchmark]
    fn dealloc_free_only_16b_n8() {
        free_prefix::<8>();
    }
    #[library_benchmark]
    fn dealloc_free_only_16b_n9() {
        free_prefix::<9>();
    }
    #[library_benchmark]
    fn dealloc_free_only_16b_n16() {
        free_prefix::<16>();
    }

    #[inline(always)]
    fn churn<const N: usize>() {
        let heap = heap();
        let layout = Layout::from_size_align(16, 8).unwrap();
        for _ in 0..N {
            let ptr = heap.alloc(layout);
            black_box(ptr);
            if !ptr.is_null() {
                // SAFETY: live block from the immediately preceding matching alloc.
                unsafe { heap.dealloc(ptr, layout) };
            }
        }
    }
    #[library_benchmark]
    fn small_churn_16b() {
        churn::<64>();
    }
    #[library_benchmark]
    fn small_churn_16b_2n() {
        churn::<128>();
    }

    #[inline(always)]
    fn magazine<const HIT: bool>() {
        let heap = heap();
        let layout = Layout::from_size_align(16, 8).unwrap();
        let mut ptrs = [core::ptr::null_mut(); 16];
        for _ in 0..4 {
            for slot in &mut ptrs {
                *slot = heap.alloc(layout);
            }
            black_box(&ptrs);
            for &ptr in &ptrs {
                if !ptr.is_null() {
                    // SAFETY: distinct live block from the matching allocation loop.
                    unsafe { heap.dealloc(ptr, layout) };
                }
            }
        }
        if HIT {
            for slot in &mut ptrs {
                *slot = heap.alloc(layout);
            }
            black_box(&ptrs);
        }
    }
    #[library_benchmark]
    fn alloc_magazine_prefill_only_16b() {
        magazine::<false>();
    }
    #[library_benchmark]
    fn alloc_magazine_hit_only_16b() {
        magazine::<true>();
    }
    #[library_benchmark]
    fn large_alloc_free_cycle() {
        use std::alloc::GlobalAlloc;
        let sefer = sefer_alloc::SeferAlloc::new();
        let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
        // SAFETY: nonzero valid allocation layout.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: live allocation freed once with its matching layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
    #[inline(always)]
    fn mimalloc_churn<const N: usize>() {
        use std::alloc::GlobalAlloc;
        let mi = mimalloc::MiMalloc;
        let layout = Layout::from_size_align(16, 8).unwrap();
        for _ in 0..N {
            // SAFETY: valid nonzero layout.
            let ptr = unsafe { mi.alloc(layout) };
            black_box(ptr);
            if !ptr.is_null() {
                // SAFETY: immediately preceding live allocation with matching layout.
                unsafe { mi.dealloc(ptr, layout) };
            }
        }
    }
    #[library_benchmark]
    fn mimalloc_small_churn_16b() {
        mimalloc_churn::<64>();
    }
    #[library_benchmark]
    fn mimalloc_small_churn_16b_2n() {
        mimalloc_churn::<128>();
    }

    library_benchmark_group!(name = perf_gate; benchmarks =
        dealloc_prealloc_only_16b, dealloc_free_only_16b_n8,
        dealloc_free_only_16b_n9, dealloc_free_only_16b_n16,
        small_churn_16b, small_churn_16b_2n,
        alloc_magazine_prefill_only_16b, alloc_magazine_hit_only_16b,
        large_alloc_free_cycle, mimalloc_small_churn_16b, mimalloc_small_churn_16b_2n);
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
