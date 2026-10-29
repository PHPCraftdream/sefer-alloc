//! R20 PRF-04 heap-reuse (P4-1) L1 Ir cells (own iai target).
//!
//! Thirteen cells per the frozen prereg
//! (`docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md`): the `start` construction
//! cell and the `cold`, `recycled`, `wave` families re-using the frozen
//! claim-path bodies, plus the NEW `wave_release` family (release-only:
//! drops of N/2N/4N of the 64 held leases). Bodies live in
//! `support/r20_prf04_heap_reuse.rs`; NO counter reads in any cell. Judged
//! on the registered windows: recycled affine w = Ir(2N) − Ir(N); cold,
//! wave and wave-release on the three scenario windows w0 = Ir(N),
//! w1 = Ir(2N) − Ir(N), w2 = Ir(4N) − Ir(2N).

#[cfg(target_os = "linux")]
#[path = "support/r20_prf04_heap_reuse.rs"]
mod hr;

#[cfg(target_os = "linux")]
mod gate {
    use super::hr;
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};

    #[library_benchmark]
    fn r20_prf04hr_start() {
        hr::prf04::start_first_claim();
    }

    #[library_benchmark]
    fn r20_prf04hr_cold_n() {
        hr::prf04::cold_first_claim(hr::HR_N);
    }
    #[library_benchmark]
    fn r20_prf04hr_cold_2n() {
        hr::prf04::cold_first_claim(hr::HR_2N);
    }
    #[library_benchmark]
    fn r20_prf04hr_cold_4n() {
        hr::prf04::cold_first_claim(hr::HR_4N);
    }

    #[library_benchmark]
    fn r20_prf04hr_recycled_n() {
        hr::prf04::recycled_first_claim(hr::HR_N);
    }
    #[library_benchmark]
    fn r20_prf04hr_recycled_2n() {
        hr::prf04::recycled_first_claim(hr::HR_2N);
    }
    #[library_benchmark]
    fn r20_prf04hr_recycled_4n() {
        hr::prf04::recycled_first_claim(hr::HR_4N);
    }

    #[library_benchmark]
    fn r20_prf04hr_wave_n() {
        hr::prf04::wave_first_claim(hr::HR_N);
    }
    #[library_benchmark]
    fn r20_prf04hr_wave_2n() {
        hr::prf04::wave_first_claim(hr::HR_2N);
    }
    #[library_benchmark]
    fn r20_prf04hr_wave_4n() {
        hr::prf04::wave_first_claim(hr::HR_4N);
    }

    #[library_benchmark]
    fn r20_prf04hr_wave_release_n() {
        hr::wave_release_first(hr::HR_N);
    }
    #[library_benchmark]
    fn r20_prf04hr_wave_release_2n() {
        hr::wave_release_first(hr::HR_2N);
    }
    #[library_benchmark]
    fn r20_prf04hr_wave_release_4n() {
        hr::wave_release_first(hr::HR_4N);
    }

    library_benchmark_group!(name = heap_reuse; benchmarks =
        r20_prf04hr_start, r20_prf04hr_cold_n, r20_prf04hr_cold_2n, r20_prf04hr_cold_4n,
        r20_prf04hr_recycled_n, r20_prf04hr_recycled_2n, r20_prf04hr_recycled_4n,
        r20_prf04hr_wave_n, r20_prf04hr_wave_2n, r20_prf04hr_wave_4n,
        r20_prf04hr_wave_release_n, r20_prf04hr_wave_release_2n, r20_prf04hr_wave_release_4n);
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
