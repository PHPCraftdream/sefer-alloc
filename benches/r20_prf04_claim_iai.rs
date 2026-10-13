//! R20 PRF-04 claim-path Ir cells (own iai target; the shared `perf_gate_iai`
//! target is over the file-length budget and must not grow).
//!
//! Ten cells: the `start` construction cell and the `cold`, `recycled` and
//! `wave` first-claim families at N / 2N / 4N. The bodies live in
//! `support/r20_prf04.rs`; see
//! `docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md` for the design.

#[cfg(target_os = "linux")]
#[path = "support/r20_prf04.rs"]
mod prf04;

#[cfg(target_os = "linux")]
mod gate {
    use super::prf04;
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};

    #[library_benchmark]
    fn r20_prf04_start_first_claim() {
        prf04::start_first_claim();
    }
    #[library_benchmark]
    fn r20_prf04_cold_first_claim_n() {
        prf04::cold_first_claim(prf04::PRF04_N);
    }
    #[library_benchmark]
    fn r20_prf04_cold_first_claim_2n() {
        prf04::cold_first_claim(prf04::PRF04_2N);
    }
    #[library_benchmark]
    fn r20_prf04_cold_first_claim_4n() {
        prf04::cold_first_claim(prf04::PRF04_4N);
    }
    #[library_benchmark]
    fn r20_prf04_recycled_first_claim_n() {
        prf04::recycled_first_claim(prf04::PRF04_N);
    }
    #[library_benchmark]
    fn r20_prf04_recycled_first_claim_2n() {
        prf04::recycled_first_claim(prf04::PRF04_2N);
    }
    #[library_benchmark]
    fn r20_prf04_recycled_first_claim_4n() {
        prf04::recycled_first_claim(prf04::PRF04_4N);
    }

    #[library_benchmark]
    fn r20_prf04_wave_first_claim_n() {
        prf04::wave_first_claim(prf04::PRF04_N);
    }
    #[library_benchmark]
    fn r20_prf04_wave_first_claim_2n() {
        prf04::wave_first_claim(prf04::PRF04_2N);
    }
    #[library_benchmark]
    fn r20_prf04_wave_first_claim_4n() {
        prf04::wave_first_claim(prf04::PRF04_4N);
    }

    library_benchmark_group!(name = claim_path; benchmarks =
        r20_prf04_start_first_claim, r20_prf04_cold_first_claim_n, r20_prf04_cold_first_claim_2n,
        r20_prf04_cold_first_claim_4n, r20_prf04_recycled_first_claim_n,
        r20_prf04_recycled_first_claim_2n, r20_prf04_recycled_first_claim_4n,
        r20_prf04_wave_first_claim_n, r20_prf04_wave_first_claim_2n,
        r20_prf04_wave_first_claim_4n);
    main!(library_benchmark_groups = claim_path);
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
