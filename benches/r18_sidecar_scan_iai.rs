//! Dedicated R18 same-binary sidecar scan measurement; Linux/Callgrind only.

#[cfg(all(target_os = "linux", not(r18_sidecar_scan_bench)))]
fn main() {
    eprintln!("R18 sidecar scan benchmark requires RUSTFLAGS=\"--cfg r18_sidecar_scan_bench\"");
    std::process::exit(1);
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("R18 sidecar scan benchmark requires Linux/Callgrind and RUSTFLAGS=\"--cfg r18_sidecar_scan_bench\"");
    std::process::exit(1);
}

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
use iai_callgrind::{library_benchmark, library_benchmark_group, main};

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
#[path = "support/r18_sidecar_scan.rs"]
mod r18_sidecar_scan;

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
#[library_benchmark]
fn r18_scan_growth() {
    r18_sidecar_scan::round("r18_scan_growth", 2, false);
}

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
#[library_benchmark]
fn r18_scan_cycles() {
    r18_sidecar_scan::round("r18_scan_cycles", 6, false);
}

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
#[library_benchmark(setup = r18_sidecar_scan::publication_setup)]
fn r18_scan_publication(sefer: sefer_alloc::SeferAlloc) {
    r18_sidecar_scan::publication_owner_window(sefer);
}

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
library_benchmark_group!(
    name = r18_scan;
    benchmarks = r18_scan_growth, r18_scan_cycles, r18_scan_publication
);

#[cfg(all(target_os = "linux", r18_sidecar_scan_bench))]
main!(
    config = iai_callgrind::LibraryBenchmarkConfig::default().pass_through_envs([
        "SEFER_R18_SCAN_MODE",
        "SEFER_R18_SCAN_ORACLE",
        "SEFER_R18_SAMPLES",
    ]);
    library_benchmark_groups = r18_scan
);
