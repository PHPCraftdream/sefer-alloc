//! Phase 8 — the segment substrate + self-hosted metadata (the Membrane
//! Inversion), behind the `alloc-core` feature.
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//! The confined-`unsafe` seams are `os` and `node`; every other file is pure
//! safe code that composes them. (`numa` was a third, feature-gated seam
//! until task #1306 removed its test-only `bind_segment` unsafe fn — it is
//! now pure safe delegation to `numa-shim`.)

// The file `alloc_core.rs` carries the same name as this module per the
// crate's one-export-per-file convention; silence clippy's module_inception.
#[allow(clippy::module_inception)]
mod alloc_core;
/// Group module: the public const-buildable configuration types (Profile, LargeCacheConfig, LargeCacheMode, SmallSegmentPoolConfig).
mod config;
/// Group module: the large/huge allocation path — `alloc_large` + slow path + reclaim, the per-shard large-cache decay/eviction cluster, the experimental `large-cache-extended` sidecar, and the cross-thread deferred-free Treiber stack.
mod large;
/// Group module: OS & platform shims (os, numa, size_classes) plus the confined raw-memory unsafe seams (node, sidecar, dirty_by_class).
mod platform;
/// Group module: the segment substrate — the per-segment metadata header
/// family (`segment_header/`), the self-hosted `SegmentTable` registry, the
/// per-class `segment_directory` + its `directory_stats` counters, the
/// per-segment `remote_free_ring`, the `segment_layout` geometry, and the
/// per-segment bitmap family (`bitmap/`).
mod segment;
/// Group module: the small/medium allocation path — the small
/// alloc/dealloc/carve hot cluster, directory-accelerated segment lookup,
/// small-segment reserve, the magazine (tcache) batch ops, cross-thread
/// reclaim, small-path diagnostics, the `alloc-decommit` empty-segment pool
/// + decommit machinery, and the measurement-only `ReservedSmallSegment`
/// handle.
mod small;
// The former flat segment-substrate child modules now live in the `segment/`
// group module and are re-exported here (at their original
// visibility/cfg/doc-hidden parity) so every one stays reachable at its
// existing `alloc_core::<name>` module path.
pub(crate) use segment::bitmap::{alloc_bitmap, magazine_bitmap, segment_bitmap};
pub(crate) use segment::directory_stats;
#[cfg(feature = "alloc-segment-directory")]
pub(crate) use segment::segment_directory;
#[doc(hidden)]
pub use segment::segment_header;
// `allow(unused_imports)`: a `mod` declaration (this file's pre-reorg form)
// is exempt from the unused-imports lint, but the equivalent module
// re-export is not. `segment_header_gen_table`'s only in-crate consumer is
// `segment_header`'s own `pub use super::segment_header_gen_table::...`
// forwarder, which resolves through `segment`'s module declaration, not
// through this re-export — so under `hardened` the name would otherwise
// warn. Same allow-with-explanation discipline as `platform::sidecar`.
#[cfg(feature = "hardened")]
#[allow(unused_imports)]
use segment::segment_header_gen_table;
// `allow(unused_imports)`: same discipline — these three siblings' ITEMS are
// consumed via `alloc_core::segment_header::...` paths, never via these
// module names; the re-exports exist purely to keep the old
// `alloc_core::segment_header_layout` / `_meta_fields` / `_views` module
// paths valid (module-path parity with the pre-reorg `mod` declarations).
#[doc(hidden)]
pub use segment::remote_free_ring;
use segment::segment_layout;
pub(crate) use segment::segment_table;
#[allow(unused_imports)]
use segment::{segment_header_layout, segment_header_meta_fields, segment_header_views};

// The ten former flat child modules now live in two group modules — the
// `platform/` shims+seams and `config/` configuration types — and are
// re-exported here (at their original visibility/cfg/doc-hidden parity) so
// every one stays reachable at its existing `alloc_core::<name>` module path.
pub(crate) use platform::node;
#[cfg(feature = "numa-aware")]
#[doc(hidden)]
pub use platform::numa;
pub(crate) use platform::os;
// `allow(unused_imports)`: a `pub(crate) mod` declaration (this file's
// pre-reorg form) is exempt from the unused-imports lint, but the equivalent
// module re-export is not — and sidecar's only consumers
// (`os.rs`'s directory-sidecar reservation, `large_cache_extended.rs`) are
// both feature-gated, so under plain `alloc-core` the name would otherwise
// warn. Same allow-with-explanation discipline as the `internals`-off
// `mod alloc_core` declaration in `lib.rs`.
#[cfg(feature = "class-aware-dirty")]
pub(crate) use platform::dirty_by_class;
#[allow(unused_imports)]
pub(crate) use platform::sidecar;
pub(crate) use platform::size_classes;
// The former flat small-path child modules now live in the `small/` group
// module and are re-exported here (at their original visibility/cfg parity)
// so every one stays reachable at its existing `alloc_core::<name>` module
// path. `allow(unused_imports)` on the private ones: a `mod` declaration
// (their pre-reorg form) is exempt from the unused-imports lint, but the
// equivalent module re-export is not — these siblings' ITEMS are consumed as
// `impl AllocCore` methods (or, for `alloc_core_small`, only under the
// lazy-commit features; for `alloc_core_small_pool`, via the
// `bench-internals`-gated snapshot-type re-export below), never via these
// module names, so the names would otherwise warn in several feature
// configs. Same allow-with-explanation discipline as `platform::sidecar`.
#[allow(unused_imports)]
use small::alloc_core_small;
#[allow(unused_imports)]
use small::alloc_core_small_diag;
#[allow(unused_imports)]
use small::alloc_core_small_magazine;
#[cfg(feature = "alloc-decommit")]
#[allow(unused_imports)]
use small::alloc_core_small_pool;
#[allow(unused_imports)]
use small::alloc_core_small_reclaim;
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub use small::reserved_small_segment;
// The four former flat large-path child modules now live in the `large/`
// group module and are re-exported here (at their original visibility/cfg/
// doc-hidden parity) so every one stays reachable at its existing
// `alloc_core::<name>` module path.
// `allow(unused_imports)`: a `mod` declaration (this file's pre-reorg form)
// is exempt from the unused-imports lint, but the equivalent module
// re-export is not — nothing in-crate references the module by name (its
// items are consumed as `impl AllocCore` methods), so under `internals`
// (where this module is not part of the crate's public surface) the name
// would otherwise warn. Same allow-with-explanation discipline as
// `small::alloc_core_small`.
#[cfg(feature = "alloc-decommit")]
pub use config::large_cache_config;
#[cfg(feature = "alloc-decommit")]
pub use config::large_cache_mode;
#[cfg(feature = "alloc-decommit")]
pub use config::profile;
#[cfg(feature = "alloc-decommit")]
pub use config::small_segment_pool_config;
#[allow(unused_imports)]
use large::alloc_core_large;
#[cfg(feature = "alloc-decommit")]
#[allow(unused_imports)]
use large::alloc_core_large_cache;
#[doc(hidden)]
pub use large::deferred_large;
#[cfg(feature = "large-cache-extended")]
pub(crate) use large::large_cache_extended;

pub use alloc_core::AllocCore;
/// R9-1 test seam (task #221 follow-up): the process-wide Large-path explicit
/// zero-pass counter, re-exported crate-wide so `HeapCore::alloc_zeroed`
/// (registry) can bump the SAME counter `AllocCore::alloc_zeroed` bumps. Read
/// via `AllocCore::dbg_large_zero_pass_count`. Not public API. Gated on
/// `alloc-stats`: the only consumer is the registry-side increment site, which
/// is itself `alloc-stats`-gated; the static itself stays always-compiled (the
/// `AllocCore::dbg_large_zero_pass_count` accessor reads it via the direct
/// module path and must remain available regardless of feature set).
#[cfg(feature = "alloc-stats")]
pub(crate) use alloc_core::LARGE_ZERO_PASS_CALLS;
/// R12-10 (task #261, `virgin-zero-skip`) test seam: the process-wide
/// Small-path explicit zero-pass counter, re-exported crate-wide so
/// `HeapCore::alloc_zeroed` (registry) can bump the SAME counter
/// `AllocCore::alloc_zeroed` bumps. Read via
/// `AllocCore::dbg_small_zero_pass_count`. Not public API. Mirrors
/// [`LARGE_ZERO_PASS_CALLS`]'s identical re-export discipline, gated on
/// BOTH `alloc-stats` AND `virgin-zero-skip` — unlike the Large counter, the
/// only consumer (the registry-side increment site in
/// `HeapCore::alloc_zeroed`) is nested inside a `virgin-zero-skip`-gated
/// block first and `alloc-stats`-gated second, so `alloc-stats` alone
/// (without `virgin-zero-skip`) has no consumer of this re-export — gating
/// on both avoids an "unused import" warning in that combination. The
/// static itself stays always-compiled.
#[cfg(all(feature = "alloc-stats", feature = "virgin-zero-skip"))]
pub(crate) use alloc_core::SMALL_ZERO_PASS_CALLS;
/// R29-5 (task #436) re-export seam: the medium→Large promotion-frequency /
/// copied-byte-distribution diagnostic statics, re-exported crate-wide so
/// `HeapCore::try_promote_to_large` (registry, `heap_core_free.rs`) can bump
/// the SAME counters `AllocCore::dbg_promotion_*` reads — mirrors
/// [`LARGE_ZERO_PASS_CALLS`]'s identical re-export discipline. Not public
/// API. Gated on `bench-internals` **AND** `try_promote_to_large`'s own
/// reachability predicate (registry's `medium_promotion_reachable!` macro,
/// reproduced here verbatim — a `#[cfg]` cannot take a macro invocation as
/// its argument): the only consumer of this re-export is the registry-side
/// increment site inside that function, so under `--all-features` (where
/// `exact-span-large` + `large-reserved-capacity` + `numa-aware` are
/// simultaneously on and the macro's reachability `any(...)` term evaluates
/// false, excluding `try_promote_to_large` entirely) a `bench-internals`-only
/// gate left this re-export genuinely unused — caught by
/// `cargo clippy --all-features -- -D warnings`, a real CI matrix row
/// (R29-13/task #444 zero-trust review). The statics themselves stay
/// always-compiled in `alloc_core.rs` so the always-available
/// `dbg_promotion_*` accessors have a stable definition regardless of
/// feature set; only this re-export (and `promotion_byte_bucket`'s
/// definition, gated identically) need the fuller predicate.
#[cfg(all(
    feature = "bench-internals",
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
pub(crate) use alloc_core::{
    promotion_byte_bucket, PROMOTION_BYTES_HIST, PROMOTION_BYTES_MAX, PROMOTION_BYTES_MIN,
    PROMOTION_BYTES_SUM, PROMOTION_COUNT,
};
/// R29-4 (task #435) MEASUREMENT-ONLY: the segment-state reconciliation
/// snapshot types returned by `AllocCore::dbg_segment_state_reconciliation`.
/// Re-exported from the private `alloc_core_small_pool` submodule so
/// `examples/` and `tests/` can name the return type. Not stable public API.
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub use alloc_core_small_pool::{SegmentStateAccount, SegmentStateReconciliation};
#[cfg(feature = "alloc-decommit")]
pub use large_cache_config::LargeCacheConfig;
#[cfg(feature = "alloc-decommit")]
pub use large_cache_mode::LargeCacheMode;
#[cfg(feature = "alloc-decommit")]
pub use profile::{LargeCachePolicy, Profile, SmallPoolPolicy};
/// R31-4 (task #467) MEASUREMENT-ONLY: re-exported so `examples/`/`tests/`
/// can name the handle type returned by
/// `AllocCore::dbg_decomp_reserve_and_keep` / consumed by
/// `AllocCore::dbg_decomp_release`. Not stable public API.
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub use reserved_small_segment::ReservedSmallSegment;
pub use segment_layout::SegmentLayout;
/// R4-8/N3 test-only harness for direct exercise of `SegmentTable`'s
/// open-addressing hash (backward-shift deletion). `pub` (not `pub(crate)`)
/// only because `alloc_core` itself is `#[doc(hidden)]`: the public surface is
/// test-only (the `#[doc(hidden)]` property test in `tests/`), reachable by
/// `sefer_alloc::alloc_core::SegmentHashHarness`. Nothing here is stable
/// public API.
#[doc(hidden)]
pub use segment_table::SegmentHashHarness;
#[cfg(feature = "alloc-decommit")]
pub use small_segment_pool_config::SmallSegmentPoolConfig;
