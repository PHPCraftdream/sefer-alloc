//! P0.2 pilot merged regression test target for the workspace restructure
//! plan (docs/plans/2026-11-15-workspace-restructure-plan.md, step P0.2).
//!
//! Each module below is a former flat integration test file
//! tests/<name>.rs moved here VERBATIM (byte-identical copy), gated by the
//! same file-level `#![cfg(...)]` predicate(s) the original carried,
//! restated as stacked `#[cfg(...)]` attributes on the mod declaration.
//! Test function names inside the modules are unchanged, so full test
//! paths become `<module>::<fn>` compared to the old flat `<fn>`.
//! This target carries no file-level cfg itself; per-module cfgs decide
//! compilation. Pilot report: docs/perf/P0_2_TEST_MERGE_PILOT.md.
//!
//! The #[path = "regression_merged/<name>.rs"] attributes are required
//! because crate-root `mod` declarations resolve relative to the parent
//! directory of the root file (tests/), not the regression_merged/ directory.
//!
//! Modules that rely on process-global state (global counters, the global
//! registry lifecycle, fresh-slot / pointer-reuse assumptions) stay as flat
//! targets because all modules of this target share one process; the
//! exclusion table is derived by scripts/p0_2_global_state_scan.mjs from
//! docs/perf/p0_2_exclusions.json and printed in
//! docs/perf/P0_2_TEST_MERGE_PILOT.md.
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[path = "regression_merged/regression_alloc_zeroed_fresh.rs"]
mod regression_alloc_zeroed_fresh;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_carve_batch.rs"]
mod regression_carve_batch;
#[cfg(all(
    feature = "alloc-core",
    feature = "internals",
    feature = "misuse-checks"
))]
#[path = "regression_merged/regression_dealloc_metadata_region_guard.rs"]
mod regression_dealloc_metadata_region_guard;
#[cfg(feature = "alloc-global")]
#[path = "regression_merged/regression_exact_256_class.rs"]
mod regression_exact_256_class;
#[cfg(all(
    all(feature = "alloc-global", feature = "fastbin"),
    feature = "internals"
))]
#[path = "regression_merged/regression_fastbin_aligned_roundtrip.rs"]
mod regression_fastbin_aligned_roundtrip;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_flush_class_unsafe_boundary.rs"]
mod regression_flush_class_unsafe_boundary;
#[cfg(all(feature = "hardened", feature = "internals"))]
#[path = "regression_merged/regression_hardened_interior_ptr.rs"]
mod regression_hardened_interior_ptr;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_inplace_large_realloc.rs"]
mod regression_inplace_large_realloc;
#[cfg(feature = "alloc-core")]
#[path = "regression_merged/regression_large_align_no_segment_exhaustion.rs"]
mod regression_large_align_no_segment_exhaustion;
#[cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "alloc-stats",
    feature = "internals"
))]
#[path = "regression_merged/regression_large_cache_multi_size_cycle.rs"]
mod regression_large_cache_multi_size_cycle;
#[cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals"
))]
#[path = "regression_merged/regression_large_cache_span_usable_stable.rs"]
mod regression_large_cache_span_usable_stable;
#[cfg(all(
    all(feature = "alloc-global", feature = "fastbin"),
    feature = "internals"
))]
#[path = "regression_merged/regression_magazine_oracles.rs"]
mod regression_magazine_oracles;
#[cfg(all(
    all(feature = "alloc-global", feature = "fastbin"),
    feature = "internals"
))]
#[path = "regression_merged/regression_magazine_scan_bounds.rs"]
mod regression_magazine_scan_bounds;
#[cfg(all(
    feature = "internals",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#[path = "regression_merged/regression_magic_at_atomic_load.rs"]
mod regression_magic_at_atomic_load;
#[cfg(feature = "alloc-core")]
#[path = "regression_merged/regression_page_aligned_no_segment_exhaustion.rs"]
mod regression_page_aligned_no_segment_exhaustion;
#[cfg(all(
    all(feature = "alloc-global", feature = "fastbin", feature = "alloc-stats"),
    feature = "internals"
))]
#[path = "regression_merged/regression_percounter_perheap_aggregation.rs"]
mod regression_percounter_perheap_aggregation;
#[cfg(feature = "internals")]
#[path = "regression_merged/regression_r2_05_diag_provenance.rs"]
mod regression_r2_05_diag_provenance;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_r2_05_diag_provenance_miri.rs"]
mod regression_r2_05_diag_provenance_miri;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_r2_3_dbg_accessors_membership_guard.rs"]
mod regression_r2_3_dbg_accessors_membership_guard;
#[cfg(all(feature = "alloc-global", not(feature = "alloc-stats")))]
#[path = "regression_merged/regression_r3a_stats_walk_compiled_out.rs"]
mod regression_r3a_stats_walk_compiled_out;
#[cfg(all(feature = "alloc-xthread", feature = "internals"))]
#[path = "regression_merged/regression_r4_2_null_base_foreign_guard.rs"]
mod regression_r4_2_null_base_foreign_guard;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_r4_2_refill_return_clamp.rs"]
mod regression_r4_2_refill_return_clamp;
#[cfg(all(
    feature = "exact-span-large",
    feature = "alloc-core",
    feature = "internals"
))]
#[path = "regression_merged/regression_r6_t22_foreign_window_key_miri.rs"]
mod regression_r6_t22_foreign_window_key_miri;
#[cfg(feature = "alloc-core")]
#[path = "regression_merged/regression_realloc_cross_class_shrink.rs"]
mod regression_realloc_cross_class_shrink;
#[cfg(feature = "alloc-core")]
#[path = "regression_merged/regression_realloc_foreign_pointer_null.rs"]
mod regression_realloc_foreign_pointer_null;
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[path = "regression_merged/regression_realloc_heap_oob.rs"]
mod regression_realloc_heap_oob;
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[path = "regression_merged/regression_realloc_inplace_global.rs"]
mod regression_realloc_inplace_global;
#[cfg(feature = "alloc-core")]
#[path = "regression_merged/regression_realloc_oob_old_layout.rs"]
mod regression_realloc_oob_old_layout;
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[path = "regression_merged/regression_resolver_branch_collapse.rs"]
mod regression_resolver_branch_collapse;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_stamp_metadata_unsafe_boundary.rs"]
mod regression_stamp_metadata_unsafe_boundary;
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[path = "regression_merged/regression_virgin_bitmap_skip.rs"]
mod regression_virgin_bitmap_skip;
#[cfg(feature = "std")]
#[path = "regression_merged/regression_w3_stats_aliasing_miri.rs"]
mod regression_w3_stats_aliasing_miri;
