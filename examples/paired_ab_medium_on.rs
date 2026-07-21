//! R10-2 process-level A/B/B/A judge binary, **medium-classes ON arm**
//! (`production,medium-classes` feature set).
//!
//! This is the TREATMENT arm of the R10-2 medium-classes wall-clock gate
//! (`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`). It installs `SeferAlloc`
//! as the real `#[global_allocator]` (built with `--features
//! "production,medium-classes"`), runs the shared phased workload, and emits
//! the same `RESULT` lines as `paired_ab_medium_off.rs`.
//!
//! The ONLY difference between this binary and `paired_ab_medium_off.rs` is
//! the Cargo feature set at build time. The source is byte-for-byte identical
//! (modulo the `arm` label string below) — both `include!` the same shared
//! workload file. Under `medium-classes`, sizes in the 256 KiB–1 MiB range
//! route through the small path (six new exact classes) instead of the
//! dedicated 4 MiB Large path; under the baseline they route Large. The
//! workload source is identical — the compiled `SeferAlloc` differs.
//!
//! **Build:** `cargo build --release --example paired_ab_medium_on --features "production,medium-classes"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared phased workload body — see `examples/_shared/paired_ab_medium_workload.rs`'s
// module doc for why `include!` (not a shared crate module) is used.
// Provides `run_phased_workload()` + the `rss_kib`/`commit_kib` probes.
include!("_shared/paired_ab_medium_workload.rs");

fn main() {
    let (elapsed_ns, alloc_ns, free_ns, realloc_ns) = run_phased_workload();

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "medium_on");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("alloc_ns", alloc_ns);
    proc_probe::emit_ns("free_ns", free_ns);
    proc_probe::emit_ns("realloc_ns", realloc_ns);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("rss_after_kib", rss_kib());
    proc_probe::emit_u64("commit_after_kib", commit_kib());
}
