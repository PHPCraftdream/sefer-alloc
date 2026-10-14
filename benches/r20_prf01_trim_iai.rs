//! Fixed-work R20 strict trim and bounded maintenance on real SeferAlloc paths.
#[cfg(target_os = "linux")]
mod gate {
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};
    use sefer_alloc::SeferAlloc;
    use std::alloc::{GlobalAlloc, Layout};
    use std::thread;
    fn workload(maintenance: bool) {
        let owner = SeferAlloc::new();
        let layout = Layout::from_size_align(16, 8).unwrap();
        let mut addresses = Vec::with_capacity(20);
        for _ in 0..20 {
            // SAFETY: valid non-zero allocation layout.
            let ptr = unsafe { owner.alloc(layout) };
            assert!(!ptr.is_null());
            addresses.push(ptr.expose_provenance());
        }
        thread::spawn(move || {
            let producer = SeferAlloc::new();
            for address in addresses {
                let ptr = core::ptr::with_exposed_provenance_mut::<u8>(address);
                // SAFETY: transferred live allocation, freed once with matching layout.
                unsafe { producer.dealloc(ptr, layout) };
            }
        })
        .join()
        .unwrap();
        if maintenance {
            let (n, units) = owner.dbg_background_maintenance_current_thread(1);
            assert!(units <= 1 && n <= 20);
        } else {
            owner.trim_current_thread();
        }
    }
    #[library_benchmark]
    fn r20_prf01_strict_trim_payload() {
        workload(false);
    }
    // Control: bounded maintenance already begins at payload start, so its expected strict-scan delta is 0.
    #[library_benchmark]
    fn r20_prf01_bounded_maintenance() {
        workload(true);
    }
    library_benchmark_group!(name = trim; benchmarks = r20_prf01_strict_trim_payload, r20_prf01_bounded_maintenance);
    main!(library_benchmark_groups = trim);
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
