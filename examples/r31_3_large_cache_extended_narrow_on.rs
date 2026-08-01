//! R31-3 (task #466, step 3) — N=1/2/4 narrow-working-set-after-burst TIMING
//! regression judge, **treatment arm** (`large-cache-extended` ON —
//! 8+32=40-slot cache).
//!
//! Companion to `r31_3_large_cache_extended_narrow_off.rs` — see that file's
//! module doc for the full rationale and workload shape. The ONLY difference
//! between the two binaries is this one is built WITH `large-cache-extended`;
//! both `include!` the identical workload source
//! (`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`).
//!
//! **ALLOCATOR LAYER UNDER TEST** (CLAUDE.md's R30-8 rule): the real
//! `#[global_allocator]` — via plain `std::alloc::{alloc, dealloc}`.
//!
//! **CORRECTED 2026-08-01 (task #487):** this binary used to install
//! `SeferAlloc::new()` (`LargeCacheConfig::DEFAULT`, which under
//! `large-cache-extended` resolves `budget_bytes: None` to
//! `Some(DEFAULT_EXTENDED_BUDGET_BYTES)` = 256 MiB, R17-9). That default is
//! exactly right for the SIBLING turnover A/B
//! (`paired_ab_large_cache_extended_on.rs`, unaffected by this fix — see the
//! shared workload's module doc's 2026-08-01 note for why THIS gate is
//! different), but wrong here: the corrected (real, 4 MiB)
//! `SegmentLayout::SEGMENT`-derived materialisation-burst sizes
//! (`materialize_sizes(9)` in the shared workload) individually exceed 256
//! MiB for the last three of the nine sizes, and the per-deposit budget
//! check (`large_cache_deposit_budget_infeasible`) rejects any deposit whose
//! OWN size exceeds the budget — so under the default config the sidecar
//! would never even see 9 successful deposits, never overflow the base 8,
//! and never materialise. This binary now installs
//! `SeferAlloc::with_config(LargeCacheConfig::new().budget_bytes(usize::MAX))`
//! (the documented "recover unbounded" idiom,
//! `src/alloc_core/large_cache_config.rs:130-136`) so the materialisation
//! burst can actually succeed — the shared workload's own in-run oracle
//! (`run_narrow_ab_workload`) hard-asserts this resolved to an unbounded
//! budget and that the sidecar genuinely materialised, rather than assuming
//! it.
//!
//! **Build:** `cargo build --release --example r31_3_large_cache_extended_narrow_on --features "production alloc-stats bench-internals large-cache-extended"`

use sefer_alloc::{LargeCacheConfig, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc =
    SeferAlloc::with_config(LargeCacheConfig::new().budget_bytes(usize::MAX));

// Shared narrow-AB workload body — identical include to
// `r31_3_large_cache_extended_narrow_off.rs`; see that file / the shared
// workload's own module doc.
include!("_shared/r31_3_large_cache_extended_narrow_ab_workload.rs");

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    assert!(
        matches!(n, 1 | 2 | 4),
        "narrow working-set size must be 1, 2, or 4 (got {n})"
    );

    let (elapsed_ns, hits, total_deallocs, rss_after_kib, commit_after_kib) =
        run_narrow_ab_workload(&GLOBAL, n);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_extended_narrow_on");
    proc_probe::emit_u64("narrow_n", n as u64);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("large_cache_hits", hits);
    proc_probe::emit_u64("total_deallocs", total_deallocs);
    proc_probe::emit_u64("rss_after_kib", rss_after_kib);
    proc_probe::emit_u64("commit_after_kib", commit_after_kib);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    #[cfg(feature = "bench-internals")]
    {
        proc_probe::emit_u64(
            "oracle_total_slots",
            GLOBAL.dbg_current_large_cache_total_slots() as u64,
        );
        proc_probe::emit_u64(
            "oracle_materialised",
            u64::from(GLOBAL.dbg_current_large_cache_extension_materialised()),
        );
    }
}
