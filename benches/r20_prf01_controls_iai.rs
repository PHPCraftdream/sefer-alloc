//! R20 PRF-01 v2 control-geometry Ir cells (own iai target; the shared
//! `perf_gate_iai` target is over its file-length budget and must not grow).
//!
//! Thirteen cells: the direct construction cell `r20_prf01_start_one_alloc_free_16b`
//! and the four hot-churn families at N / 2N / 4N. The bodies live in
//! `support/r20_prf01_controls.rs` (semantic copies of the original
//! `perf_gate_iai.rs` hot-control bodies; correspondence documented there and
//! in `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md`).

#[cfg(target_os = "linux")]
#[path = "support/r20_prf01_controls.rs"]
mod prf01;

#[cfg(target_os = "linux")]
mod gate {
    use super::prf01;
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};

    #[library_benchmark]
    fn r20_prf01_start_one_alloc_free_16b() {
        prf01::start_one_alloc_free_16b();
    }
    #[library_benchmark]
    fn r20_prf01_small_churn_16b_n() {
        prf01::small_churn_16b(prf01::PRF01_N);
    }
    #[library_benchmark]
    fn r20_prf01_small_churn_16b_2n() {
        prf01::small_churn_16b(prf01::PRF01_2N);
    }
    #[library_benchmark]
    fn r20_prf01_small_churn_16b_4n() {
        prf01::small_churn_16b(prf01::PRF01_4N);
    }
    #[library_benchmark]
    fn r20_prf01_churn_256b_n() {
        prf01::churn_256b(prf01::PRF01_N);
    }
    #[library_benchmark]
    fn r20_prf01_churn_256b_2n() {
        prf01::churn_256b(prf01::PRF01_2N);
    }
    #[library_benchmark]
    fn r20_prf01_churn_256b_4n() {
        prf01::churn_256b(prf01::PRF01_4N);
    }
    #[library_benchmark]
    fn r20_prf01_churn_write_256b_n() {
        prf01::churn_write_256b(prf01::PRF01_N);
    }
    #[library_benchmark]
    fn r20_prf01_churn_write_256b_2n() {
        prf01::churn_write_256b(prf01::PRF01_2N);
    }
    #[library_benchmark]
    fn r20_prf01_churn_write_256b_4n() {
        prf01::churn_write_256b(prf01::PRF01_4N);
    }
    #[library_benchmark]
    fn r20_prf01_aligned_churn_640b_a128_n() {
        prf01::aligned_churn_640b_a128(prf01::PRF01_N);
    }
    #[library_benchmark]
    fn r20_prf01_aligned_churn_640b_a128_2n() {
        prf01::aligned_churn_640b_a128(prf01::PRF01_2N);
    }
    #[library_benchmark]
    fn r20_prf01_aligned_churn_640b_a128_4n() {
        prf01::aligned_churn_640b_a128(prf01::PRF01_4N);
    }

    library_benchmark_group!(name = prf01_controls; benchmarks =
        r20_prf01_start_one_alloc_free_16b,
        r20_prf01_small_churn_16b_n, r20_prf01_small_churn_16b_2n, r20_prf01_small_churn_16b_4n,
        r20_prf01_churn_256b_n, r20_prf01_churn_256b_2n, r20_prf01_churn_256b_4n,
        r20_prf01_churn_write_256b_n, r20_prf01_churn_write_256b_2n, r20_prf01_churn_write_256b_4n,
        r20_prf01_aligned_churn_640b_a128_n, r20_prf01_aligned_churn_640b_a128_2n,
        r20_prf01_aligned_churn_640b_a128_4n);
    main!(library_benchmark_groups = prf01_controls);
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
