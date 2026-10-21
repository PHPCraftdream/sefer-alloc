// mt_xp4_tls_probe.rs — decisive R30-8 measurement: hit/miss of the EXISTING
// R1 TLS route pin cache (RoutePin::dbg_route_pin_cache_totals) under real
// malloc-bench-rs larson/mstress workloads at T=2 and T=4, in a
// bench-internals counters build (Ir is never taken from this build).
// Usage: mt_xp4_tls_probe <sefer|mimalloc> <larson|mstress> <steps> <threads>
use malloc_bench_rs::{run, Config, Workload};

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn pin_totals() -> (u64, u64, u64, u64, u64, u64, u64) {
    sefer_alloc::registry::segment_route::RoutePin::dbg_route_pin_cache_totals()
}
#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn pin_totals() -> (u64, u64, u64, u64, u64, u64, u64) {
    (0, 0, 0, 0, 0, 0, 0)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let alloc = a.get(1).map(String::as_str).unwrap_or("sefer");
    let wl = match a.get(2).map(String::as_str).unwrap_or("larson") {
        "mstress" => Workload::Mstress,
        _ => Workload::Larson,
    };
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let threads: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(2);
    let cfg = Config {
        threads,
        steps_per_thread: steps,
        working_set: 768,
        mstress_blocks: 512,
    };
    let r = match alloc {
        "mimalloc" => run(wl, &cfg, || mimalloc::MiMalloc),
        _ => run(wl, &cfg, sefer_alloc::SeferAlloc::new),
    };
    let t = pin_totals();
    let hits = t.0;
    let misses = t.1;
    let denom = hits + misses;
    let frac = if denom > 0 {
        hits as f64 / denom as f64
    } else {
        1.0
    };
    println!(
        "{alloc} {r:.0} T={threads} PINS hits={hits} misses={misses} inserts={} evictions={} unlink_fb={} releases={} sweeps={} TLS_HIT_FRAC {frac:.4}",
        t.2, t.3, t.4, t.5, t.6
    );
}
