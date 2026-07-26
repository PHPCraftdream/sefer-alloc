//! R21-1 process-level A/B/B/A judge binary, **single-hot-buffer, medium-classes
//! ON arm** (`production,medium-classes` feature set).
//!
//! This is the TREATMENT arm of the R21-1 single-hot-buffer harness
//! (`docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §6.1 point 2 / §8 step
//! 2). It installs `SeferAlloc` as the real `#[global_allocator]` (built
//! with `--features "production,medium-classes"`), runs the shared
//! single-hot-buffer workload, and emits the same `RESULT` lines as
//! `paired_ab_hot_buffer_off.rs`.
//!
//! The ONLY difference between this binary and `paired_ab_hot_buffer_off.rs`
//! is the Cargo feature set at build time. The source is byte-for-byte
//! identical (modulo the `arm` label string below) — both `include!` the
//! same shared workload file. Under `medium-classes`, the single buffer
//! routes through the small path (six exact medium classes, 256 KiB–1 MiB)
//! until it crosses `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
//! (`src/registry/heap_core_free.rs`, 256 KiB) on its first grow step, at
//! which point it promotes to a dedicated Large segment exactly like the
//! baseline arm's block was all along — this harness's per-round reset back
//! to `REALLOC_BASE` (256 KiB) then repeats that same first-crossing
//! promotion cost every round, which is the specific cost R20-3's proposed
//! OPT-H mechanism targets, under the single-hot-buffer condition (§5.3 of
//! that design) where OPT-H's tail-adjacency precondition is expected to
//! hold on essentially every grow.
//!
//! **Build:** `cargo build --release --example paired_ab_hot_buffer_on --features "production,medium-classes"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared single-hot-buffer workload body — see
// `examples/_shared/paired_ab_hot_buffer_workload.rs`'s module doc for why
// `include!` (not a shared crate module) is used. Provides
// `run_hot_buffer_workload()` + the `rss_kib`/`commit_kib` probes.
include!("_shared/paired_ab_hot_buffer_workload.rs");

fn main() {
    let (elapsed_ns, realloc_ns) = run_hot_buffer_workload();

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "hot_buffer_on");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("alloc_ns", 0);
    proc_probe::emit_ns("free_ns", 0);
    proc_probe::emit_ns("realloc_ns", realloc_ns);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("rss_after_kib", rss_kib());
    proc_probe::emit_u64("commit_after_kib", commit_kib());
}
