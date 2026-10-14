//! PRF-04 L2 installed-allocator iai cells; toggles name real allocation/release targets.
#[cfg(target_os = "linux")]
#[path = "support/r20_prf04_heap_reuse.rs"]
mod hr;
#[cfg(target_os = "linux")]
mod gate {
    use super::hr;
    use iai_callgrind::{
        library_benchmark, library_benchmark_group, main, EntryPoint, LibraryBenchmarkConfig,
    };

    fn alloc_toggle() -> LibraryBenchmarkConfig {
        let mut config = LibraryBenchmarkConfig::default();
        config
            .entry_point(EntryPoint::Custom(
                "r20_prf04_heap_reuse_l2_iai::hr::first_global_alloc".into(),
            ))
            .callgrind_args(["--separate-threads=yes"]);
        config
    }
    fn wave_toggle() -> LibraryBenchmarkConfig {
        let mut config = LibraryBenchmarkConfig::default();
        config
            .entry_point(EntryPoint::Custom(
                "r20_prf04_heap_reuse_l2_iai::hr::first_global_alloc".into(),
            ))
            .callgrind_args(["--separate-threads=yes"]);
        config
    }
    fn release_toggle() -> LibraryBenchmarkConfig {
        let mut config = LibraryBenchmarkConfig::default();
        config
            .entry_point(EntryPoint::Custom(
                "std::sys::thread_local::native::eager::destroy::<sefer_alloc::global::tls_heap::AbandonGuard>".into(),
            ))
            .callgrind_args(["--separate-threads=yes"]);
        config
    }

    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_cold_n() {
        hr::l2_cold(hr::N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_cold_2n() {
        hr::l2_cold(2 * hr::N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_cold_4n() {
        hr::l2_cold(4 * hr::N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_recycled_alloc_n() {
        hr::l2_recycled_alloc(hr::N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_recycled_alloc_2n() {
        hr::l2_recycled_alloc(2 * hr::N);
    }
    #[library_benchmark(config = alloc_toggle())]
    fn r20_prf04hr_l2_recycled_alloc_4n() {
        hr::l2_recycled_alloc(4 * hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_recycled_release_n() {
        hr::l2_recycled_release(hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_recycled_release_2n() {
        hr::l2_recycled_release(2 * hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_recycled_release_4n() {
        hr::l2_recycled_release(4 * hr::N);
    }
    #[library_benchmark(config = wave_toggle())]
    fn r20_prf04hr_l2_wave_n() {
        hr::l2_wave(hr::N);
    }
    #[library_benchmark(config = wave_toggle())]
    fn r20_prf04hr_l2_wave_2n() {
        hr::l2_wave(2 * hr::N);
    }
    #[library_benchmark(config = wave_toggle())]
    fn r20_prf04hr_l2_wave_4n() {
        hr::l2_wave(4 * hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_wave_release_n() {
        hr::l2_wave_release(hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_wave_release_2n() {
        hr::l2_wave_release(2 * hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_l2_wave_release_4n() {
        hr::l2_wave_release(4 * hr::N);
    }

    library_benchmark_group!(name = heap_reuse_l2; benchmarks =
        r20_prf04hr_l2_cold_n, r20_prf04hr_l2_cold_2n, r20_prf04hr_l2_cold_4n,
        r20_prf04hr_l2_recycled_alloc_n, r20_prf04hr_l2_recycled_alloc_2n,
        r20_prf04hr_l2_recycled_alloc_4n, r20_prf04hr_l2_recycled_release_n,
        r20_prf04hr_l2_recycled_release_2n, r20_prf04hr_l2_recycled_release_4n,
        r20_prf04hr_l2_wave_n, r20_prf04hr_l2_wave_2n, r20_prf04hr_l2_wave_4n,
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
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: hr::FirstAlloc<false> = hr::FirstAlloc;
