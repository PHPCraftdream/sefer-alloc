//! PRF-04 L1 iai cells with explicit workload entry-point toggles.
#[cfg(target_os = "linux")]
#[path = "support/r20_prf04_heap_reuse.rs"]
mod hr;
#[cfg(target_os = "linux")]
mod gate {
    use super::hr;
    use iai_callgrind::{
        library_benchmark, library_benchmark_group, main, EntryPoint, LibraryBenchmarkConfig,
    };
    fn cold_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default().entry_point(EntryPoint::Custom(
            "r20_prf04_heap_reuse_iai::hr::cold".into(),
        ))
    }
    fn recycled_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default().entry_point(EntryPoint::Custom(
            "r20_prf04_heap_reuse_iai::hr::recycled".into(),
        ))
    }
    fn wave_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default().entry_point(EntryPoint::Custom(
            "r20_prf04_heap_reuse_iai::hr::wave_claim".into(),
        ))
    }
    fn release_toggle() -> LibraryBenchmarkConfig {
        LibraryBenchmarkConfig::default().entry_point(EntryPoint::Custom(
            "r20_prf04_heap_reuse_iai::hr::wave_release_measured".into(),
        ))
    }
    #[library_benchmark(config = cold_toggle())]
    fn r20_prf04hr_start() {
        hr::cold(1);
    }
    #[library_benchmark(config = cold_toggle())]
    fn r20_prf04hr_cold_n() {
        hr::cold(hr::N);
    }
    #[library_benchmark(config = cold_toggle())]
    fn r20_prf04hr_cold_2n() {
        hr::cold(2 * hr::N);
    }
    #[library_benchmark(config = cold_toggle())]
    fn r20_prf04hr_cold_4n() {
        hr::cold(4 * hr::N);
    }
    #[library_benchmark(config = recycled_toggle())]
    fn r20_prf04hr_recycled_n() {
        hr::recycled_setup();
        hr::recycled(hr::N);
    }
    #[library_benchmark(config = recycled_toggle())]
    fn r20_prf04hr_recycled_2n() {
        hr::recycled_setup();
        hr::recycled(2 * hr::N);
    }
    #[library_benchmark(config = recycled_toggle())]
    fn r20_prf04hr_recycled_4n() {
        hr::recycled_setup();
        hr::recycled(4 * hr::N);
    }
    #[library_benchmark(config = wave_toggle())]
    fn r20_prf04hr_wave_n() {
        hr::wave_setup();
        hr::wave_claim(hr::N);
    }
    #[library_benchmark(config = wave_toggle())]
    fn r20_prf04hr_wave_2n() {
        hr::wave_setup();
        hr::wave_claim(2 * hr::N);
    }
    #[library_benchmark(config = wave_toggle())]
    fn r20_prf04hr_wave_4n() {
        hr::wave_setup();
        hr::wave_claim(4 * hr::N);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_wave_release_n() {
        let mut h = hr::wave_release_setup();
        hr::wave_release_measured(&mut h, hr::N);
        std::mem::forget(h);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_wave_release_2n() {
        let mut h = hr::wave_release_setup();
        hr::wave_release_measured(&mut h, 2 * hr::N);
        std::mem::forget(h);
    }
    #[library_benchmark(config = release_toggle())]
    fn r20_prf04hr_wave_release_4n() {
        let mut h = hr::wave_release_setup();
        hr::wave_release_measured(&mut h, 4 * hr::N);
        std::mem::forget(h);
    }
    library_benchmark_group!(name = heap_reuse; benchmarks =
        r20_prf04hr_start, r20_prf04hr_cold_n, r20_prf04hr_cold_2n,
        r20_prf04hr_cold_4n, r20_prf04hr_recycled_n, r20_prf04hr_recycled_2n,
        r20_prf04hr_recycled_4n, r20_prf04hr_wave_n, r20_prf04hr_wave_2n,
        r20_prf04hr_wave_4n, r20_prf04hr_wave_release_n,
        r20_prf04hr_wave_release_2n, r20_prf04hr_wave_release_4n);
    main!(library_benchmark_groups = heap_reuse);
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
