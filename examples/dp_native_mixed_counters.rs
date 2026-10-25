//! dp_native_mixed_counters.rs — B4 D0 depot-provenance chunk 2 (task
//! b4-provenance): the SAME installed churn+Vec workload as
//! `dp_native_mixed_sefer` (native-mixed-judge), but instead of the wall-clock
//! driver it prints the D0 DEPOT_* counters around the run. The workload body
//! is `include!`d verbatim from `examples/_shared/paired_ab_workload.rs`
//! (byte-identical file as the judge's — see that file's module doc for why
//! `include!`); only `fn main` differs (counters instead of timing).
//! Entry point / allocator is the installed-justice facade:
//! `#[global_allocator] static GLOBAL: sefer_alloc::SeferAlloc =
//! sefer_alloc::SeferAlloc::new()`.
//! Usage: dp_native_mixed_counters <repeats>
#![allow(unsafe_code)]

const ARM: &str = "sefer";

#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

include!("_shared/paired_ab_workload.rs");

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn depot_counters() -> [u64; 17] {
    use sefer_alloc::registry::HeapCore;
    let (ov_e, ov_b) = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let (sw_e, sw_b) = HeapCore::dbg_depot_flush_swap_top();
    let (ba_e, ba_b) = HeapCore::dbg_depot_bin_ingress_own_batch();
    let (rm_e, rm_b) = HeapCore::dbg_depot_bin_ingress_remote();
    let (td_e, td_b) = HeapCore::dbg_depot_flush_teardown();
    let (rf_e, rf_tot, rf_dr, rf_cv, rf_ce) = HeapCore::dbg_depot_refill_stats();
    let pushes = HeapCore::dbg_depot_bin_ingress_substrate_pushes();
    [
        ov_e, ov_b, sw_e, sw_b, ba_e, ba_b, rm_e, rm_b, td_e, td_b, rf_e, rf_tot, rf_dr, rf_cv,
        rf_ce, pushes, 0,
    ]
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn depot_counters() -> [u64; 17] {
    [0; 17]
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn shadow_aggregates() -> (
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    u64,
) {
    use sefer_alloc::registry::HeapCore;
    HeapCore::dbg_depot_shadow_aggregates()
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn shadow_aggregates() -> (
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    u64,
) {
    ([0; 4], [0; 4], [0; 4], [0; 4], [0; 4], [0; 4], 0)
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn unmapped() -> u64 {
    use sefer_alloc::registry::HeapCore;
    HeapCore::dbg_depot_shadow_unmapped()
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn unmapped() -> u64 {
    0
}

fn main() {
    let repeats: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "120".to_string())
        .parse()
        .expect("целое число повторений");
    assert!((1..=4096).contains(&repeats));
    let before = depot_counters();
    let sh_before = shadow_aggregates();
    run_workload(); // warmup round, same as native_mixed_driver.rs
    for _ in 0..repeats {
        run_workload();
    }
    let c = depot_counters();
    let sh_after = shadow_aggregates();
    let ds = |a: [u64; 4], b: [u64; 4]| [b[0] - a[0], b[1] - a[1], b[2] - a[2], b[3] - a[3]];
    let abs_k = ds(sh_before.0, sh_after.0);
    let real_k = ds(sh_before.1, sh_after.1);
    let fully_k = ds(sh_before.2, sh_after.2);
    let rfd_k = ds(sh_before.3, sh_after.3);
    let rdr_k = ds(sh_before.4, sh_after.4);
    let rfl_k = ds(sh_before.5, sh_after.5);
    let inv = sh_after.6 - sh_before.6;
    let d = |x: u64, y: u64| y - x;
    // B4 (R4): provenance-completeness oracle — OTHER (unattributed Small
    // bin ingress) must be ZERO on this workload.
    let attributed = d(before[1], c[1])
        + d(before[3], c[3])
        + d(before[5], c[5])
        + d(before[7], c[7])
        + d(before[9], c[9]);
    let other = d(before[15], c[15]).saturating_sub(attributed);
    assert_eq!(
        other, 0,
        "depot_bin_ingress_other_blocks != 0: an unattributed Small \
         bin-ingress path exists"
    );
    println!(
        "{{\"arm\":\"{ARM}\",\"repeats\":{repeats},\"warmup_rounds\":1,\"depot_bin_ingress_own_overflow_events\":{},\"depot_bin_ingress_own_overflow_blocks\":{},\"depot_flush_swap_top_events\":{},\"depot_flush_swap_top_blocks\":{},\"depot_bin_ingress_own_batch_events\":{},\"depot_bin_ingress_own_batch_blocks\":{},\"depot_bin_ingress_remote_events\":{},\"depot_bin_ingress_remote_blocks\":{},\"depot_flush_teardown_events\":{},\"depot_flush_teardown_blocks\":{},\"depot_refill_events\":{},\"depot_refill_blocks_total\":{},\"depot_refill_blocks_drained\":{},\"depot_refill_blocks_carved\":{},\"depot_refill_carve_events\":{},\"depot_bin_ingress_pushes\":{},\"depot_bin_ingress_other_blocks\":{}}}",
        d(before[0], c[0]),
        d(before[1], c[1]),
        d(before[2], c[2]),
        d(before[3], c[3]),
        d(before[4], c[4]),
        d(before[5], c[5]),
        d(before[6], c[6]),
        d(before[7], c[7]),
        d(before[8], c[8]),
        d(before[9], c[9]),
        d(before[10], c[10]),
        d(before[11], c[11]),
        d(before[12], c[12]),
        d(before[13], c[13]),
        d(before[14], c[14]),
        d(before[15], c[15]),
        d(before[16], c[16]),
    );
    println!(
        "{{\"absorbed_k\":[{}, {}, {}, {}],\"real_k\":[{}, {}, {}, {}],\"fully_absorbed_ev_k\":[{}, {}, {}, {}],\"refill_from_depot_k\":[{}, {}, {}, {}],\"refill_drain_k\":[{}, {}, {}, {}],\"refill_fully_k\":[{}, {}, {}, {}],\"invalidations\":{},\"unmapped\":{}}}",
        abs_k[0], abs_k[1], abs_k[2], abs_k[3],
        real_k[0], real_k[1], real_k[2], real_k[3],
        fully_k[0], fully_k[1], fully_k[2], fully_k[3],
        rfd_k[0], rfd_k[1], rfd_k[2], rfd_k[3],
        rdr_k[0], rdr_k[1], rdr_k[2], rdr_k[3],
        rfl_k[0], rfl_k[1], rfl_k[2], rfl_k[3],
        inv,
        unmapped(),
    );
}
