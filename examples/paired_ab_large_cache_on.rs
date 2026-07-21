//! R10-5 process-level A/B/B/A judge binary, **small-path recycle arm**
//! (`production,medium-classes-wide,alloc-stats` feature set).
//!
//! This is the TREATMENT arm of the R10-5 warm-Large-cache-hit gate
//! (`docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`). It installs `SeferAlloc` as
//! the real `#[global_allocator]` (built with `--features "production
//! medium-classes-wide alloc-stats"`), runs the shared warm-recycle workload,
//! and emits the same `RESULT` lines as `paired_ab_large_cache_off.rs`.
//!
//! The ONLY difference between this binary and `paired_ab_large_cache_off.rs`
//! is the Cargo feature set at build time. The source is byte-for-byte
//! identical (modulo the `arm` label string below) — both `include!` the same
//! shared workload file. Under `medium-classes-wide`, `SMALL_MAX` rises to
//! 1.75 MiB, so 1.5/1.75 MiB requests route through the small path's freelist
//! (the ~60 ns push/pop recycle R9-4 §2.4's consolation-prize claim is about)
//! instead of the dedicated Large path. `alloc-stats` is added so the
//! `large_cache_hits` counter is live — it MUST read 0 in this arm (the small
//! path never touches the Large cache), proving the two arms exercise
//! genuinely different code paths.
//!
//! `medium-classes-wide` implies `medium-classes` (which implies `alloc-core`);
//! it is NOT part of `production`. The `production` feature list in
//! `Cargo.toml` is untouched.
//!
//! **argv[1]** = allocation size in KiB (`1536` = 1.5 MiB, `1792` = 1.75 MiB).
//!
//! **Build:** `cargo build --release --example paired_ab_large_cache_on --features "production medium-classes-wide alloc-stats"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared warm-recycle workload body — see
// `examples/_shared/paired_ab_large_cache_workload.rs`'s module doc for why
// `include!` (not a shared crate module) is used.
include!("_shared/paired_ab_large_cache_workload.rs");

fn main() {
    // argv[1] = allocation size in KiB (1536 / 1792). One size per launch.
    let size_kib: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("usage: paired_ab_large_cache_on <size_kib> (e.g. 1536 or 1792)");
    let size_bytes = size_kib.checked_mul(KIB).expect("size_kib overflow");

    let (recycle_ns, _) = run_warm_recycle_workload(size_bytes);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_on");
    proc_probe::emit_ns("recycle_ns", recycle_ns);
    proc_probe::emit_u64("size_kib", size_kib as u64);
    proc_probe::emit_u64("large_cache_hits", stats.large_cache_hits);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("segments_released_total", stats.segments_released_total);
    proc_probe::emit_u64("decommit_calls", stats.decommit_calls);
    proc_probe::emit_u64("rss_after_kib", rss_kib());
    proc_probe::emit_u64("commit_after_kib", commit_kib());
}
