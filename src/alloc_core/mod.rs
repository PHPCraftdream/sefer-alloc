//! Phase 8 — the segment substrate + self-hosted metadata (the Membrane
//! Inversion), behind the `alloc-core` feature.
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//! Tier-1 `#![allow(unsafe_code)]` seams in this tree (oxx R2-07) are
//! `platform::{os,node,sidecar}`, `large::large_cache_extended`,
//! `segment::segment_table::route_slots`, and
//! `segment::remote_bitmap::sidecar_bitmap::leaf_classes`.
//! Otherwise-safe files carry documented tier-2 `#[allow(unsafe_code)]`
//! items, including owner-side sidecar dereference. Do not
//! hand-count these — per `CLAUDE.md`'s "Active rules" unsafe-inventory
//! convention, the self-verifying, comment-proof command
//! `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/alloc_core` enumerates
//! every tier-1 (`#!...`) and tier-2 (`#...`) site under this tree; that
//! command's output, not a number quoted here, is the source of truth.
//! (`numa` was a third tier-1 seam until task #1306 removed its test-only
//! `bind_segment` unsafe fn — it is now pure safe delegation to `numa-shim`.)

// The file `alloc_core.rs` carries the same name as this module per the
// crate's one-export-per-file convention; silence clippy's module_inception.
#[allow(clippy::module_inception)]
mod alloc_core;
/// Group module: the public const-buildable configuration types (Profile, LargeCacheConfig, LargeCacheMode, SmallSegmentPoolConfig).
mod config;
/// Group module: the large/huge allocation path — `alloc_large` + slow path + reclaim, the per-shard large-cache decay/eviction cluster, the experimental `large-cache-extended` sidecar, and the per-reservation Large phase-transition state.
mod large;
/// Group module: OS & platform shims (os, numa, size_classes) plus the confined raw-memory unsafe seams (node, sidecar).
mod platform;
/// Group module: the segment substrate — the per-segment metadata header
/// family (`segment_header/`), the self-hosted `SegmentTable` registry, the
/// per-class `segment_directory` + its `directory_stats` counters, the
/// route slots (`segment_table/route_slots.rs`), the `segment_layout`
/// geometry, and the per-segment bitmap family (`bitmap/`). Production
/// foreign-free ingress uses the registry's pinned `segment_route` sidecars.
mod segment;
/// Group module: the small/medium allocation path — the small
/// alloc/dealloc/carve hot cluster, directory-accelerated segment lookup,
/// small-segment reserve, the magazine (tcache) batch ops, cross-thread
/// reclaim, small-path diagnostics, the `alloc-decommit` empty-segment pool
/// + decommit machinery, and the measurement-only `ReservedSmallSegment`
/// handle.
mod small;
// R6-T20: crate-wide single source of truth for the medium-promotion
// reachability predicate (`alloc_core/config/medium_promotion.rs`). The
// positive `#[cfg]` generator is invoked in this file below and across both
// the `alloc_core` and `registry` trees, so it is ungated; the negated
// generator and the `cfg!` const have only `alloc-global`(-`internals`)
// consumers, and an ungated `pub(crate) use` of those would be an
// unused-import error under `-D warnings` in `alloc-core`-only builds —
// hence their gates.
pub(crate) use config::medium_promotion::medium_promotion_reachable;
#[cfg(feature = "alloc-global")]
pub(crate) use config::medium_promotion::medium_promotion_unreachable;
#[cfg(all(feature = "alloc-global", feature = "internals"))]
pub(crate) use config::medium_promotion::MEDIUM_PROMOTION_REACHABLE;
#[cfg(feature = "alloc-global")]
pub(crate) use large::reservation_state;
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[doc(hidden)]
pub use large::reservation_state::LargeReservationState;
#[cfg(feature = "alloc-global")]
pub(crate) use segment::remote_bitmap;
// The former flat segment-substrate child modules now live in the `segment/`
// group module and are re-exported here (at their original
// visibility/cfg/doc-hidden parity) so every one stays reachable at its
// existing `alloc_core::<name>` module path.
pub(crate) use segment::bitmap::state_byte;
#[cfg_attr(
    not(any(feature = "alloc-stats", feature = "internals")),
    allow(unused_imports)
)]
pub(crate) use segment::directory_stats;
#[cfg(feature = "alloc-segment-directory")]
pub(crate) use segment::segment_directory;
#[doc(hidden)]
pub use segment::segment_header;
// `allow(unused_imports)`: same discipline — these three siblings' ITEMS are
// consumed via `alloc_core::segment_header::...` paths, never via these
// module names; the re-exports exist purely to keep the old
// `alloc_core::segment_header_layout` / `_meta_fields` / `_views` module
// paths valid (module-path parity with the pre-reorg `mod` declarations).
pub(crate) use segment::segment_layout;
pub(crate) use segment::segment_table;
#[allow(unused_imports)]
use segment::{segment_header_layout, segment_header_meta_fields, segment_header_views};

// The ten former flat child modules now live in two group modules — the
// `platform/` shims+seams and `config/` configuration types — and are
// re-exported here (at their original visibility/cfg/doc-hidden parity) so
// every one stays reachable at its existing `alloc_core::<name>` module path.
/// The one sanctioned process terminator (`fatal(msg)`), reachable crate-wide (R6-T15).
pub(crate) use platform::fatal;
pub(crate) use platform::node;
#[cfg(feature = "numa-aware")]
#[doc(hidden)]
pub use platform::numa;
pub(crate) use platform::os;
/// R2-12: process-wide owner-sidecar reservation/release accounting — the
/// sidecar-leak-fix acceptance oracle (`platform::sidecar_stats`). Test-only
/// surface in the same `#[doc(hidden)]` spirit as the rest of this module's
/// internal reexports: NOT stable public API.
#[cfg(feature = "internals")]
pub use platform::sidecar_stats::{dbg_sidecar_reservation_stats, SidecarReservationStats};
// `allow(unused_imports)`: a `pub(crate) mod` declaration (this file's
// pre-reorg form) is exempt from the unused-imports lint, but the equivalent
// module re-export is not — and sidecar's only consumers
// (`os.rs`'s directory-sidecar reservation, `large_cache_extended.rs`) are
// both feature-gated, so under plain `alloc-core` the name would otherwise
// warn. Same allow-with-explanation discipline as the `internals`-off
// `mod alloc_core` declaration in `lib.rs`.
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
pub use config::policy;
#[cfg(feature = "alloc-decommit")]
pub use config::small_segment_pool_config;
#[allow(unused_imports)]
use large::alloc_core_large;
#[cfg(feature = "alloc-decommit")]
#[allow(unused_imports)]
use large::alloc_core_large_cache;
#[cfg(feature = "large-cache-extended")]
pub(crate) use large::large_cache_extended;

pub use alloc_core::AllocCore;
// R6-T16: consumed by the fastbin-gated own-free/realloc paths (`fastbin`
// implies `alloc-global`) and by the medium promotion path in `realloc.rs`,
// which `medium-classes` compiles without `fastbin`.
#[cfg(all(
    feature = "alloc-global",
    any(feature = "fastbin", feature = "medium-classes")
))]
#[cfg_attr(not(feature = "fastbin"), allow(unused_imports))]
pub(crate) use alloc_core::OwnedBlock;
/// The dropped-free counter, re-exported crate-wide so
/// `HeapCore::publish_foreign` can bump it (like
/// [`LARGE_ZERO_PASS_CALLS`]). Not public API; read via
/// `AllocCore::foreign_or_unroutable_frees`.
#[cfg(any(feature = "alloc-xthread", feature = "alloc-stats"))]
pub(crate) use alloc_core::FOREIGN_OR_UNROUTABLE_FREES;
#[cfg(feature = "alloc-global")]
pub(crate) use alloc_core::LARGE_REMOTE_RETIREMENTS;
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
/// R6-T55: the local-reject counter, re-exported crate-wide so the
/// `dealloc_large_at_base` reject branch and the `small_free_guard` consumers
/// can bump it (same pattern as `FOREIGN_OR_UNROUTABLE_FREES` above). Read via
/// `AllocCore::dbg_local_reject_frees`. Not public API; the increment sites
/// are `internals`/`bench-internals`-gated, so the re-export carries the same
/// predicate.
#[cfg(any(feature = "internals", feature = "bench-internals"))]
pub(crate) use alloc_core::LOCAL_REJECT_FREES;
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
/// B4-M (P1-2): X-null tail-guard counter, re-exported crate-wide for its
/// increment site in `registry` (the S1 counters stay internal to
/// `alloc_core`, reached via `alloc_core::alloc_core::`).
#[cfg(feature = "bench-internals")]
pub(crate) use alloc_core::XNULL_NULL_TAIL_GUARD;
/// B4 collapse diagnosis (task B4_MT_COLLAPSE_DIAGNOSIS): slow-path observer
/// counters, re-exported crate-wide for their increment sites and
/// `SeferAlloc::dbg_b4_collapse_counters`.
#[cfg(feature = "bench-internals")]
pub(crate) use alloc_core::{
    B4_CURSOR_HITS, B4_CURSOR_MISSES, B4_DIRECTORY_LOOKUPS, B4_FIND_SEGMENT_CALLS,
    B4_FOREIGN_PUBLISH_OK, B4_SCAN_CALLS, B4_SCAN_WORDS, B4_SHARD_LOCK_SPINS, B4_SHARD_LOCK_YIELDS,
    B4_U31_DEFERRED_SCANS, B4_U31_DISCOVERY_SCANS, B4_U31_SCAN_HOLDERS_BASE,
    B4_U31_SCAN_NO_PENDING, B4_U31_SCAN_SEGMENTS,
};
/// SC-6 counting oracle statics (`--cfg g1_ingress_oracle`).
#[cfg(all(g1_ingress_oracle, feature = "bench-internals"))]
pub(crate) use alloc_core::{G1_NOTIFIES, G1_OWNER_EPOCH, G1_PUBLISHES};
medium_promotion_reachable! {
    /// R29-5 (task #436) re-export seam: the medium→Large promotion-frequency /
    /// copied-byte-distribution diagnostic statics, re-exported crate-wide so
    /// `HeapCore::try_promote_to_large` (registry, `free/realloc.rs`) can bump
    /// the SAME counters `AllocCore::dbg_promotion_*` reads — mirrors
    /// [`LARGE_ZERO_PASS_CALLS`]'s identical re-export discipline. Not public
    /// API. Gated on `bench-internals` **AND** `try_promote_to_large`'s own
    /// reachability predicate (R6-T20: generated by the
    /// `medium_promotion_reachable!` macro; the extra `bench-internals` term
    /// rides as an ordinary `#[cfg]` attribute inside the invocation — multiple
    /// cfg attributes AND): the only consumer of this re-export is the
    /// registry-side increment site inside that function, so under
    /// `--all-features` (where `exact-span-large` + `large-reserved-capacity` +
    /// `numa-aware` are simultaneously on and the predicate's `any(...)` term
    /// evaluates false, excluding `try_promote_to_large` entirely) a
    /// `bench-internals`-only gate left this re-export genuinely unused —
    /// caught by `cargo clippy --all-features -- -D warnings`, a real CI matrix
    /// row (R29-13/task #444 zero-trust review). The statics themselves stay
    /// always-compiled in `counters.rs` so the always-available
    /// `dbg_promotion_*` accessors have a stable definition regardless of
    /// feature set; only this re-export (and `promotion_byte_bucket`'s
    /// definition, gated identically) need the fuller predicate.
    #[cfg(feature = "bench-internals")]
    pub(crate) use alloc_core::{
        promotion_byte_bucket, PROMOTION_BYTES_HIST, PROMOTION_BYTES_MAX, PROMOTION_BYTES_MIN,
        PROMOTION_BYTES_SUM, PROMOTION_COUNT,
    };
}
/// M-C oracle (Ph4c, receipt open question M-C) numeric observers of the
/// strict trim-path sidecar ingress drain — see
/// `alloc_core/sidecar_drain.rs` and `SeferAlloc::dbg_sidecar_ingress_stats`.
/// Not public API; never read by production logic.
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "bench-internals"
))]
pub(crate) use alloc_core::{
    BACKGROUND_INGRESS_STEP_CALLS, SIDECAR_INGRESS_DRAIN_CALLS, SIDECAR_INGRESS_RECORDS_CONSUMED,
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
pub use policy::{LargeCachePolicy, Profile, SmallPoolPolicy};
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
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use segment_table::SegmentHashHarness;
#[cfg(feature = "alloc-decommit")]
pub use small_segment_pool_config::SmallSegmentPoolConfig;
