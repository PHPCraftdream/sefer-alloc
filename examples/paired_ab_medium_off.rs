//! R10-2 process-level A/B/B/A judge binary, **medium-classes OFF arm**
//! (`production` feature set, no `medium-classes`).
//!
//! This is the BASELINE arm of the R10-2 medium-classes wall-clock gate
//! (`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`). It installs `SeferAlloc`
//! as the real `#[global_allocator]` (built with `--features production` —
//! NO `medium-classes`), runs the shared phased workload
//! (`examples/_shared/paired_ab_medium_workload.rs`, `include!`d verbatim —
//! byte-identical to the code compiled into `paired_ab_medium_on.rs`), and
//! emits one `RESULT` line per phase so `scripts/paired-ab-runner.mjs
//! --config` can pair each phase independently across A/B/B/A process
//! launches:
//!
//! - `RESULT elapsed_ns=<n>` — total of all three phases.
//! - `RESULT alloc_ns=<n>` — alloc-phase wall-clock (allocate `WS_LEN`
//!   simultaneously-live medium objects).
//! - `RESULT free_ns=<n>` — free-phase wall-clock (free the held objects).
//! - `RESULT realloc_ns=<n>` — realloc-phase wall-clock (realloc-grow
//!   through the medium range).
//! - `RESULT segments_reserved_total=<n>` — SeferAlloc's own diagnostic
//!   counter, the installed-allocator sanity gate. MUST be > 0 in BOTH arms
//!   (both genuinely install SeferAlloc and exercise it).
//! - `RESULT rss_after_kib` / `RESULT commit_after_kib` — memory snapshot.
//!
//! The ONLY difference between this binary and `paired_ab_medium_on.rs` is
//! the Cargo feature set at build time. The source is byte-for-byte identical
//! (modulo the `arm` label string below) — both `include!` the same shared
//! workload file. The `medium-classes` feature changes how the compiled
//! `SeferAlloc` routes sizes in the 256 KiB–1 MiB range (Large path vs small
//! path), not the workload source.
//!
//! **Build:** `cargo build --release --example paired_ab_medium_off --features production`

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

    proc_probe::emit("arm", "medium_off");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("alloc_ns", alloc_ns);
    proc_probe::emit_ns("free_ns", free_ns);
    proc_probe::emit_ns("realloc_ns", realloc_ns);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("rss_after_kib", rss_kib());
    proc_probe::emit_u64("commit_after_kib", commit_kib());
}
