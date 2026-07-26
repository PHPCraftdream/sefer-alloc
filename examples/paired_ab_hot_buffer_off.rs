//! R21-1 process-level A/B/B/A judge binary, **single-hot-buffer, medium-classes
//! OFF arm** (`production` feature set, no `medium-classes`).
//!
//! This is the BASELINE arm of the R21-1 single-hot-buffer harness
//! (`docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §6.1 point 2 / §8 step
//! 2). Unlike `paired_ab_medium_off.rs` (R10-2's deliberately adversarial
//! 16-simultaneously-live-object harness), THIS harness allocates exactly
//! ONE buffer and repeatedly grows it through the medium-class ladder, with
//! nothing else allocated concurrently in the timed region — the direct
//! realization of R10-2 §5's "buffer construction (grow to target, then
//! operate)" workload profile, which R20-3 §5.2/§5.3 argues the existing
//! N=16 harness cannot represent.
//!
//! It installs `SeferAlloc` as the real `#[global_allocator]` (built with
//! `--features production` — NO `medium-classes`), runs the shared
//! single-hot-buffer workload (`examples/_shared/paired_ab_hot_buffer_workload.rs`,
//! `include!`d verbatim — byte-identical to the code compiled into
//! `paired_ab_hot_buffer_on.rs`), and emits `RESULT` lines in the SAME shape
//! `paired_ab_medium_{off,on}.rs` use, so `scripts/paired-ab-runner.mjs`
//! (built-in stats engine, unmodified) can pair the `realloc_ns` metric
//! across A/B/B/A process launches:
//!
//! - `RESULT elapsed_ns=<n>` — total of the timed region (identical to
//!   `realloc_ns` in this harness — there is only one timed phase; see the
//!   shared workload's module doc for why).
//! - `RESULT alloc_ns=<n>` — always `0` here (no separate alloc phase is
//!   timed in this harness — kept as a `RESULT` line purely for shape
//!   parity with `paired_ab_medium_{off,on}.rs`, so any tooling that expects
//!   all four phase keys to be present keeps working unmodified).
//! - `RESULT free_ns=<n>` — always `0`, same reason as `alloc_ns` above.
//! - `RESULT realloc_ns=<n>` — the ONLY metric this harness actually
//!   measures: wall-clock of growing the single live buffer through the
//!   medium-class ladder, `ROUNDS` times.
//! - `RESULT segments_reserved_total=<n>` — SeferAlloc's own diagnostic
//!   counter, the installed-allocator sanity gate. MUST be > 0 in BOTH arms.
//! - `RESULT rss_after_kib` / `RESULT commit_after_kib` — memory snapshot.
//!
//! The ONLY difference between this binary and `paired_ab_hot_buffer_on.rs`
//! is the Cargo feature set at build time. The source is byte-for-byte
//! identical (modulo the `arm` label string below) — both `include!` the
//! same shared workload file. Under this (baseline) build, the single 256
//! KiB–1 MiB buffer routes through the dedicated Large path immediately (no
//! `medium-classes`, so it is well above the tiny-object small-class
//! ceiling); under the `on` arm it routes through the small path (six exact
//! medium classes) until it crosses `MEDIUM_REALLOC_PROMOTION_THRESHOLD`.
//!
//! **Build:** `cargo build --release --example paired_ab_hot_buffer_off --features production`

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

    proc_probe::emit("arm", "hot_buffer_off");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("alloc_ns", 0);
    proc_probe::emit_ns("free_ns", 0);
    proc_probe::emit_ns("realloc_ns", realloc_ns);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("rss_after_kib", rss_kib());
    proc_probe::emit_u64("commit_after_kib", commit_kib());
}
