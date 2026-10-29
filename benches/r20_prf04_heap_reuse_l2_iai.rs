//! R20 PRF-04 heap-reuse (P4-1) L2 Ir cells — the shipping layer
//! (`#[global_allocator] SeferAlloc`), own iai target.
//!
//! Nine cells per the frozen prereg
//! (`docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md`): `r20_prf04hr_l2_cold_n|2n|4n`,
//! `r20_prf04hr_l2_recycled_n|2n|4n`, `r20_prf04hr_l2_wave_release_n|2n|4n`.
//! Collection is a per-thread callgrind custom toggle on a named symbol with
//! `--separate-threads=yes` (the runner supplies `--separate-threads=yes`, as
//! in the PRF-01 precedent; see `scripts/r19_prf01_sidecar_summary_run.mjs`).
//!
//! L2 cold / recycled cells toggle collection on `hr::child_first_alloc`;
//! recycled uses the primary allocator plus an explicit `GUARD` toggle unless
//! the preregistered L1 release fallback is enabled. Wave-release toggles the
//! exit-path release symbol
//! `<sefer_alloc::global::tls_heap::AbandonGuard as core::ops::Drop>::drop`.

#[cfg(target_os = "linux")]
#[path = "support/r20_prf04_heap_reuse.rs"]
mod hr;

#[cfg(target_os = "linux")]
mod gate {
    use super::hr;
    use iai_callgrind::{
        library_benchmark, library_benchmark_group, main, EntryPoint, LibraryBenchmarkConfig,
    };

    /// The child-first-allocation symbol this bench binary defines
    /// (`#[inline(never)]`, so the exact path is stable).
    fn alloc_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default()
            .entry_point(EntryPoint::Custom(
                "r20_prf04_heap_reuse_l2_iai::hr::child_first_alloc".to_owned(),
            ))
            .raw_callgrind_args(["--separate-threads=yes"])
    }

    #[cfg(not(prf04_heap_reuse_release_l1))]
    fn recycled_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default()
            .entry_point(EntryPoint::Custom(
                "r20_prf04_heap_reuse_l2_iai::hr::child_first_alloc".to_owned(),
            ))
            .raw_callgrind_args(["--separate-threads=yes", "--toggle-collect=GUARD"])
    }

    #[cfg(prf04_heap_reuse_release_l1)]
    fn recycled_toggle() -> LibraryBenchmarkConfig {
        alloc_toggle()
    }

    /// The thread-exit release symbol (main's abandon guard on the TLS heap
    /// path). Verified by symbol receipt in the A-only trial run.
    fn release_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default()
            .entry_point(EntryPoint::Custom(
                "<sefer_alloc::global::tls_heap::AbandonGuard as core::ops::Drop>::drop".to_owned(),
            ))
            .raw_callgrind_args(["--separate-threads=yes"])
    }

    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_cold_n() {
        hr::l2_child_cold(hr::HR_N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_cold_2n() {
        hr::l2_child_cold(hr::HR_2N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_cold_4n() {
        hr::l2_child_cold(hr::HR_4N);
    }

    #[library_benchmark(config = recycled_toggle())]
    fn r20_prf04hr_l2_recycled_n() {
        hr::l2_recycled_warmup();
        hr::l2_recycled(hr::HR_N);
    }
    #[library_benchmark(config = recycled_toggle())]
    fn r20_prf04hr_l2_recycled_2n() {
        hr::l2_recycled_warmup();
        hr::l2_recycled(hr::HR_2N);
    }
    #[library_benchmark(config = recycled_toggle())]
    fn r20_prf04hr_l2_recycled_4n() {
        hr::l2_recycled_warmup();
        hr::l2_recycled(hr::HR_4N);
    }

    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_wave_release_n() {
        hr::l2_wave_release_first(hr::HR_N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_wave_release_2n() {
        hr::l2_wave_release_first(hr::HR_2N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_wave_release_4n() {
        hr::l2_wave_release_first(hr::HR_4N);
    }

    library_benchmark_group!(name = heap_reuse_l2; benchmarks =
        r20_prf04hr_l2_cold_n, r20_prf04hr_l2_cold_2n, r20_prf04hr_l2_cold_4n,
        r20_prf04hr_l2_recycled_n, r20_prf04hr_l2_recycled_2n, r20_prf04hr_l2_recycled_4n,
        r20_prf04hr_l2_wave_release_n, r20_prf04hr_l2_wave_release_2n,
        r20_prf04hr_l2_wave_release_4n);
    main!(library_benchmark_groups = heap_reuse_l2);
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

/// Install the shipping allocator so each child's first allocation is a real
/// `SeferAlloc` bind (the default-profile constructor; see
/// `benches/global_alloc.rs` / `examples/global_allocator.rs` for precedent).
#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();
