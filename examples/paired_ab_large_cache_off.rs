//! R10-5 process-level A/B/B/A judge binary, **WARM-Large-cache arm**
//! (`production,alloc-stats` feature set — 1.5/1.75 MiB route Large, warm cache).
//!
//! This is the BASELINE arm of the R10-5 warm-Large-cache-hit gate
//! (`docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`). It installs `SeferAlloc` as the
//! real `#[global_allocator]` (built with `--features "production alloc-stats"`
//! — NO `medium-classes`/`medium-classes-wide`), runs the shared warm-recycle
//! workload (`examples/_shared/paired_ab_large_cache_workload.rs`, `include!`d
//! verbatim — byte-identical to the code compiled into
//! `paired_ab_large_cache_on.rs`), and emits `RESULT` lines.
//!
//! The ONLY difference between this binary and `paired_ab_large_cache_on.rs`
//! is the Cargo feature set at build time. The source is byte-for-byte
//! identical (modulo the `arm` label string below) — both `include!` the same
//! shared workload file. Without `medium-classes`/`medium-classes-wide`,
//! `SMALL_MAX` is ~253 KiB, so 1.5/1.75 MiB requests route through the
//! dedicated 4 MiB Large path — and with a working set below
//! `LARGE_CACHE_SLOTS` (8), the steady-state allocs HIT the warm Large cache
//! (cheap in-process bookkeeping, NOT a `VirtualFree`/`VirtualAlloc`
//! round-trip). `alloc-stats` is added (NOT part of `production`, passed only
//! on this probe binary's build line — the `production` feature list in
//! `Cargo.toml` is untouched) so the per-hit `large_cache_hits` counter is
//! live and emitted as a RESULT line, PROVING the baseline is warm-vs-warm
//! (the methodology gap R9-4 §2.4 left open).
//!
//! **argv[1]** = allocation size in KiB (e.g. `1536` = 1.5 MiB, `1792` = 1.75
//! MiB). One size per process launch keeps each size's cache state independent
//! (no cross-size cache contamination).
//!
//! **RESULT lines:**
//! - `RESULT recycle_ns=<n>` — steady-state warm alloc+free recycle wall-clock.
//! - `RESULT size_kib=<n>` — the argv-selected size (sanity echo).
//! - `RESULT large_cache_hits=<n>` — the WARM-CACHE PROOF. MUST be large
//!   (~`WS_LEN * ROUNDS`) in this arm; `0` in the treatment arm.
//! - `RESULT segments_reserved_total=<n>` — cumulative OS reservations; stays
//!   low if recycling in-process (no OS churn).
//! - `RESULT decommit_calls=<n>` — M6 decommit counter (diagnostic).
//! - `RESULT segments_released_total=<n>` — cumulative OS releases.
//! - `RESULT rss_after_kib` / `RESULT commit_after_kib` — memory snapshot.
//!
//! **Build:** `cargo build --release --example paired_ab_large_cache_off --features "production alloc-stats"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared warm-recycle workload body — see
// `examples/_shared/paired_ab_large_cache_workload.rs`'s module doc for why
// `include!` (not a shared crate module) is used.
// Provides `run_warm_recycle_workload(size_bytes)` + the `rss_kib`/`commit_kib`
// probes.
include!("_shared/paired_ab_large_cache_workload.rs");

fn main() {
    // argv[1] = allocation size in KiB (1536 / 1792). One size per launch.
    let size_kib: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("usage: paired_ab_large_cache_off <size_kib> (e.g. 1536 or 1792)");
    let size_bytes = size_kib.checked_mul(KIB).expect("size_kib overflow");

    let (recycle_ns, _) = run_warm_recycle_workload(size_bytes);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_off");
    proc_probe::emit_ns("recycle_ns", recycle_ns);
    proc_probe::emit_u64("size_kib", size_kib as u64);
    proc_probe::emit_u64("large_cache_hits", stats.large_cache_hits);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("segments_released_total", stats.segments_released_total);
    proc_probe::emit_u64("decommit_calls", stats.decommit_calls);
    proc_probe::emit_u64("rss_after_kib", rss_kib());
    proc_probe::emit_u64("commit_after_kib", commit_kib());
}
