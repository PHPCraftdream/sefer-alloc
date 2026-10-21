//! B4-29 (U31 trigger) Ir cells: the PRF01 `growth16` discovery-scale axis,
//! SeferAlloc vs mimalloc, on their own iai target (`perf_gate_iai` is a
//! frozen `docs/structure_limits_baseline.csv` file and must not grow).
//!
//! The Sefer arms are thin wrappers over the shared PRF01 workload body
//! `prf01_support::R19Prf01::growth16(n)` for n in {32_768, 65_536, 131_072}
//! — the same three arms as `perf_gate_iai`'s `growth16_n/2n/4n`, re-hosted
//! here so the growth axis can be profiled in isolation
//! (callgrind / callgrind_annotate) without the rest of the perf gate.
//!
//! The mimalloc arms are byte-for-byte mirrors of the same workload shape
//! (n blocks of 16 B @ align(8), all allocated first, then all freed), called
//! DIRECTLY through mimalloc's `GlobalAlloc` impl on a locally-constructed
//! `mimalloc::MiMalloc` value — NEVER installed as `#[global_allocator]` —
//! per `perf_gate_iai.rs`'s established mimalloc pattern (its module doc
//! around the `mimalloc_bootstrap_proxy` seam): same op counts, same sizes,
//! same alignment, only the allocator value differs.

#[cfg(not(target_os = "linux"))]
fn main() {}

// Shared PRF01 bodies; only `growth16` is exercised here, the producer /
// publication helpers stay for parity with the other `support/r19_prf01.rs`
// consumers (same dead-code allowance as their `#[cfg_attr(test)]`).
#[allow(dead_code)]
#[path = "support/r19_prf01.rs"]
mod prf01_support;

#[cfg(target_os = "linux")]
mod gate {
    use iai_callgrind::{library_benchmark, library_benchmark_group, main};
    use mimalloc::MiMalloc;
    use std::alloc::{GlobalAlloc, Layout};
    use std::hint::black_box;

    use crate::prf01_support;

    const GROWTH16_N: usize = 32_768;
    const GROWTH16_2N: usize = 65_536;
    const GROWTH16_4N: usize = 131_072;

    #[library_benchmark]
    fn sefer_growth16_n() {
        prf01_support::R19Prf01::growth16(GROWTH16_N);
    }

    #[library_benchmark]
    fn sefer_growth16_2n() {
        prf01_support::R19Prf01::growth16(GROWTH16_2N);
    }

    #[library_benchmark]
    fn sefer_growth16_4n() {
        prf01_support::R19Prf01::growth16(GROWTH16_4N);
    }

    /// mimalloc mirror of `R19Prf01::growth16`: n blocks of 16 B @ align(8),
    /// all allocated before ANY free (the PRF01 all-live-batch shape), then
    /// all freed exactly once. Parity with the Sefer arms is op-for-op.
    fn mimalloc_growth16_body(n: usize) {
        let mi = MiMalloc;
        let layout = Layout::from_size_align(16, 8).unwrap();
        let mut addresses: Vec<usize> = Vec::with_capacity(n);
        for _ in 0..n {
            // SAFETY: layout has non-zero size and valid (power-of-two)
            // alignment.
            let block = unsafe { mi.alloc(layout) };
            assert!(!block.is_null());
            addresses.push(block.expose_provenance());
        }
        black_box(&addresses);
        for address in addresses {
            let block = core::ptr::with_exposed_provenance_mut(address);
            // SAFETY: uniquely owned live allocation returned by `mi.alloc`
            // above with the same layout; freed exactly once.
            unsafe { mi.dealloc(block, layout) };
        }
    }

    #[library_benchmark]
    fn mimalloc_growth16_n() {
        mimalloc_growth16_body(GROWTH16_N);
    }

    #[library_benchmark]
    fn mimalloc_growth16_2n() {
        mimalloc_growth16_body(GROWTH16_2N);
    }

    #[library_benchmark]
    fn mimalloc_growth16_4n() {
        mimalloc_growth16_body(GROWTH16_4N);
    }

    library_benchmark_group!(
        name = b4_u31_trigger_growth16;
        benchmarks =
            sefer_growth16_n,
            sefer_growth16_2n,
            sefer_growth16_4n,
            mimalloc_growth16_n,
            mimalloc_growth16_2n,
            mimalloc_growth16_4n);
    main!(library_benchmark_groups = b4_u31_trigger_growth16);
    pub fn run() {
        main();
    }
}

#[cfg(target_os = "linux")]
fn main() {
    gate::run();
}
