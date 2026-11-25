//! # sefer-alloc
//!
//! A safe, *handle-addressed* region store. Instead of handing out raw
//! pointers, a [`Region<T>`] hands out small generational [`Handle<T>`]
//! values; the bytes live in `slotmap`'s contiguous slot array, resolved by a
//! single indirection (see `docs/BENCHMARKS.md`). A stale handle does not
//! resolve to a live value for roughly `2^31` reuse cycles of that slot
//! (after ~2^31 cycles the generation wraps and a very old handle may
//! alias a later value). It returns `None`, never undefined behaviour.
//!
//! ## The engine
//!
//! The single-threaded core is a thin **typed membrane** over
//! [`slotmap`](https://crates.io/crates/slotmap): [`Region<T>`] wraps a
//! `slotmap::SlotMap<DefaultKey, T>` and [`Handle<T>`] is a `#[repr(C)]`
//! struct of three fields — a `slotmap::DefaultKey`, a `region_id:
//! NonZeroUsize` that identifies which `Region` instance minted the handle
//! (see I7 below), and a `PhantomData<fn() -> T>` — so handles stay
//! generic-over-`T` and typed (which raw slotmap keys are not) while also
//! rejecting cross-`Region` aliasing. `slotmap`'s `unsafe` owns the
//! generational layout — the free list, generation bump on
//! remove, and 32-bit generation wrap after ~2^31 cycles; this crate adds the typed
//! boundary and instance isolation while staying `#![forbid(unsafe_code)]`.
//!
//! [`Region`], [`Handle`], and `SyncRegion` (std-only) now live in the `sefer-region`
//! crate alongside `aligned-vmem` / `numa-shim` / `malloc-bench-rs`. They are
//! re-exported here for backward compatibility.
//!
//! ## Scope (honest)
//!
//! This is an *application-level* store, not a drop-in global allocator. For a
//! process-wide allocator, use `SeferAlloc` (opt-in `production` feature) or
//! reach for `mimalloc`.
//!
//! ## Monitoring `SeferAlloc` in production (`stats()`)
//!
//! `SeferAlloc` exposes a cheap, process-wide diagnostic snapshot via
//! `SeferAlloc::stats` → `AllocStats`: cache
//! hit rates, cross-thread reclaim diagnostics, and cumulative
//! segment/heap totals (`segments_reserved_total - segments_released_total`
//! is the live segment count — the field to alert on for a segment leak;
//! `foreign_or_unroutable_frees` counts address-only route lookup failures
//! or rejected terminal publications, without requiring `alloc-stats`).
//! It is not an exhaustive invalid-free or layout-mismatch detector;
//! callers must still provide a current allocation and exact layout.
//! `stats()` is lock-free and allocation-free, but its cost is
//! feature-dependent: without `alloc-stats` it is a handful of relaxed atomic
//! loads (O(1)); with `alloc-stats` on, the two hit counters are summed by an
//! O(minted-slot-count) walk over registry slot metadata (see
//! `SeferAlloc::stats`'s own doc for the precise contract). Safe to poll on a
//! metrics-scrape timer either way (requires the `alloc-global` feature;
//! runnable form in `tests/sefer_alloc_examples.rs`):
//!
//! ```text
//! use sefer_alloc::SeferAlloc;
//!
//! #[global_allocator]
//! static GLOBAL: SeferAlloc = SeferAlloc::new();
//!
//! let stats = GLOBAL.stats();
//! let segments_live = stats
//!     .segments_reserved_total
//!     .saturating_sub(stats.segments_released_total);
//! println!("segments_live={segments_live} tcache_hits={}", stats.tcache_hits);
//! ```
//!
//! **Multi-thread safety:** `alloc-global` unconditionally implies
//! `alloc-xthread` (R5-01) — without it, a block freed on a different thread
//! than it was allocated on had no sound routing path (leaked, safely but
//! permanently) and a cross-thread `realloc` returned null (an abort for a
//! `Vec` grown after being moved to another thread). `Cargo.toml`'s feature
//! unification plus a `compile_error!` guard in `src/lib.rs` make that
//! configuration unbuildable, so any `--features alloc-global` build is sound
//! for real multi-threaded deployment. See `SeferAlloc`'s "Multi-thread
//! safety" doc section for the full explanation.
//!
//! `SeferAlloc` (and the whole allocator stack) is **`std`-only** — it needs
//! thread-local storage and `std::time::Instant`. `Region<T>` / `Handle<T>`
//! (this crate's other face) are `no_std` + `alloc`-only and unaffected.
//!
//! See `docs/INVARIANTS.md` for the safety invariants this crate upholds and
//! `docs/ARCHITECTURE.md` for the architecture.
//!
//! ## Example
//!
//! ```text
//! use sefer_alloc::Region;
//!
//! let mut region = Region::new();
//! let a = region.insert("alpha");
//! let b = region.insert("beta");
//!
//! assert_eq!(region.get(a), Some(&"alpha"));
//!
//! region.remove(a);
//! assert_eq!(region.get(a), None); // stale handle → None, never UB
//! assert_eq!(region.get(b), Some(&"beta")); // others stay valid
//! ```
//!
//! Runnable form: `tests/region_invariants.rs`.

// ── Workspace: fifteen independently-publishable companion crates ────────────
//
// The workspace extracted fifteen building blocks that can also be used
// standalone. Ten are runtime dependencies, with allocator companions
// selected by features; four are development infrastructure, and
// `tagged-index-stack` is verification-only:
//
//   sefer-policy       (crates/core/sefer-policy)        — allocator policy layer: config types, Large-THP policy runtime (feature: alloc-core)
//   sefer-segment      (crates/core/sefer-segment)    — segment/route substrate: SegmentTable/route directory/sidecar bitmap (feature: alloc-core)
//   sefer-region       (crates/containers/sefer-region)             — typed handle store (this re-export; runtime, no feature gate)
//   sefer-concurrent   (crates/containers/sefer-concurrent) — legacy/research concurrent tier (feature: experimental/pinning)
//   aligned-vmem       (crates/platform/aligned-vmem)               — OS virtual-memory aperture          (feature: alloc-core)
//   sefer-platform     (crates/platform/sefer-platform)             — fatal/process-exit + SEGMENT OS aperture (feature: alloc-core)
//   numa-shim          (crates/platform/numa-shim)               — NUMA detection + binding            (feature: numa-aware)
//   once-ptr-cell      (crates/primitives/once-ptr-cell)      — lazy CAS-published pointer cell     (feature: alloc-core)
//   size-classes       (crates/primitives/size-classes)       — const-built size-class tables       (feature: alloc-core)
//   sefer-ghost        (crates/primitives/sefer-ghost) — checking-build-only ghost_assert!/ghost_event! (always; no feature gate)
//   tagged-index-stack (crates/primitives/tagged-index-stack) — ABA-tagged free-index stack (verification-only; not a runtime dependency)
//   malloc-bench-rs    (crates/bench/malloc-bench-rs)       — portable GlobalAlloc bench harness  (dev-only)
//   globalalloc-model  (crates/verify/globalalloc-model)  — differential op-stream test harness (dev-only)
//   proc-memstat       (crates/platform/proc-memstat)       — same-instant RSS / commit self-probe (dev-only)
//   proc-probe         (crates/platform/proc-probe)         — RESULT key=value stdout protocol    (dev-only)
//
// (`ring-mpsc`, a standalone bounded MPSC index ring, was removed from the
// workspace, task #655 — it had zero production consumers; the in-tree
// RemoteFreeRing/HeapOverflow swap it was extracted for was investigated and
// found NO-GO, see docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md.)
//
// ── Unsafe inventory — see README.md, not here ───────────────────────────────
//
// The complete list of `unsafe` seams is NOT maintained in this file. Its
// stable source of truth is README.md § "Where `unsafe` lives (the complete
// list)" (anchor `#where-unsafe-lives-the-complete-list`).
//
// Verification command (one command, two tiers): `#![...]` matches are the
// module-level seams (tier 1); `#[...]` matches are item-scoped `unsafe fn`
// declarations and their internal call-site `unsafe {}` blocks (tier 2,
// task #101 / R4-9). The anchored form is comment-proof: `^\s*#!?\[` requires
// the line to begin with the attribute, not a `//` prefix.
//
//   grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/
//
// The crate root denies `unsafe` code, and forbids it when neither
// `experimental` nor `alloc-core` is enabled (see the attributes below).
//
// The historical `concurrent::epoch::hand` is the epoch-tier `AtomicSlot<T>`
// membrane, owned by `sefer-concurrent`.
//
// The `compile_error!` guards below tell a mis-feature build to "use the
// `production` feature bundle": alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory + primordial-lazy-commit + large-cache-adaptive.
//
// ── The soundness boundary is WIDER than the unsafe-syntax boundary ────────────
//
//  The confinement above is real, but it enforces the localization of unsafe
//  *syntax* only — it is NOT a claim that a bug outside these modules cannot
//  cause UB. The soundness boundary is the seams PLUS every safe membrane
//  function that calls into them with a documented PROSE contract: violating
//  that contract from safe code is UB even though no `unsafe` keyword appears
//  at the violation site. Concrete membranes to audit as part of the trusted
//  computing base:
//    * the `sefer-alloc-core` crate's `alloc_core::platform::node` — its safe
//      `pub(crate)` fns (`write_usize`, `write_struct`, `offset`, `zero`, ...) —
//      `pub(crate)` fns whose whole body is a raw r/w; soundness rests on the
//      caller's bounds/exclusivity/`'static` invariants stated in prose.
//    * `os::release_segment` — a safe fn; a double call (double-release) from
//      safe code is UB (the OS reservation is freed twice).
//    * `os::{decommit_pages, recommit_pages}` — safe fns; the range-containment
//      invariant is the caller's, unchecked.
//    * `sefer_registry::registry::heap_slot::HeapSlot` — its `state`/`heap` single-writer
//      invariant (which the slot's `Sync` proof and the `claim_lease` /
//      lease-drop (LIVE→FREE) protocol depend on) is a prose contract; a safe CAS of `state` LIVE→FREE
//      from the wrong place breaks it. (Non-test fields are `pub(crate)` to keep
//      this membrane inside the crate — see that module's M7 note.)
//      Its owner-only `depot` storage (B4 DP-3) rests on the same contract.
//  In short: the membrane pattern concentrates the *unsafe blocks* into a
//  small named set of `#![allow(unsafe_code)]` files (tier 1, inventoried in
//  README.md § "Where `unsafe` lives") PLUS item-scoped
//  `#[allow(unsafe_code)]` sites (tier 2 — individual
//  `unsafe fn` boundaries with unverifiable caller-pointer contracts, and the
//  `unsafe {}` blocks at their internal call sites; task #101 / R4-9), for
//  audit — but the *soundness argument* spans those safe callers too.
//  This is a deliberate, worthwhile trade — named here so a future editor does
//  not misread "no stray unsafe" as "no UB reachable from safe code".
#![warn(missing_docs)]
#![cfg_attr(
    not(any(feature = "experimental", feature = "alloc-core")),
    forbid(unsafe_code)
)]
#![deny(unsafe_code)]
// The single-threaded core (`Region<T>` / `Handle<T>`) needs only `alloc` and
// builds under `no_std`. Disable the default `std` feature to drop `SyncRegion`
// and the concurrent/byte tiers and get the bare `no_std` + `alloc` core.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

// 0.3.0 (task A2) — defence-in-depth: `fastbin` is unsound without
// `alloc-xthread`. `Cargo.toml`'s `fastbin = ["alloc-global", "alloc-xthread"]`
// feature-unification is the primary fix (any normal `--features fastbin`
// build pulls `alloc-xthread` in automatically); this `compile_error!` is a
// belt-and-suspenders guard for the unlikely case someone builds with
// `--no-default-features --features fastbin` against a stale `Cargo.toml` /
// vendored copy, or a future edit accidentally drops the dependency again.
// Without `alloc-xthread`, a cross-thread free of a small block has no
// ownership-checked routing path (the current terminal sidecar route lives
// behind `alloc-xthread`), so
// a naive cross-thread free would write directly into another thread's
// private magazine/free-list — an unsynchronised data race, not a
// correctness nicety.
#[cfg(all(feature = "fastbin", not(feature = "alloc-xthread")))]
compile_error!(
    "sefer-alloc: `fastbin` requires `alloc-xthread` (cross-thread free \
     without it races the per-thread magazine/free-list — unsound). Enable \
     both, e.g. `--features fastbin,alloc-xthread`, or use the `production` \
     feature bundle."
);

// R5-01 (xxs round 5): without `alloc-xthread` a cross-thread free leaks its
// block and a cross-thread `realloc` returns null (an abort for a moved-then-
// grown `Vec`). `Cargo.toml` makes `alloc-global` imply it; this guard covers
// a stale/vendored manifest, like `fastbin`'s guard above.
#[cfg(all(feature = "alloc-global", not(feature = "alloc-xthread")))]
compile_error!(
    "sefer-alloc: `alloc-global` requires `alloc-xthread` (without it, a \
     cross-thread free leaks its block and pins its segment, and a \
     cross-thread realloc returns null — an abort for a moved-then-grown \
     Vec). Enable both, e.g. `--features alloc-global,alloc-xthread`, or use \
     the `production` feature bundle. Check for a stale/vendored Cargo.toml \
     if you did not request this combination directly."
);

// The concurrent tier uses AtomicU64; pointer-width atomics alone do not suffice.
#[cfg(all(feature = "experimental", not(target_has_atomic = "64")))]
compile_error!(
    "sefer-alloc: `experimental` (including `pinning`, which implies it) requires \
     64-bit atomics (target_has_atomic = \"64\"). Choose a target with this \
     capability or disable `experimental` and features that imply it."
);

// R2-17 (independent src review round 2, docs/reviews/2026-09-22-120730-src-review-xa-round-2.md) — the allocator's
// 64-bit-only support boundary, declared explicitly instead of surfacing as
// two cryptic E0080 const-assert failures. What fires here: ANY allocator
// feature build on a non-64-bit target. `alloc-core` is the substrate root,
// and every feature above it (`alloc-xthread`, `alloc-global`, `production`,
// `fastbin`, `batch-api`, `alloc-stats`, `medium-classes`, `numa-aware`,
// `alloc-decommit`, `page-map-diag`, `exact-span-large`,
// `alloc-segment-directory`, ...) implies it, so this single gate covers the
// whole allocator surface.
//
// The metadata placement and magazine layout have target-ABI bounds.
// The header is constrained to fit within one page, not one exact size.
// AtomicU64 and pointer provenance need a separate 32-bit audit; removing one
// layout assertion is not target support.
//
// What stays available: the region-only surface. The default features are
// `std` only, and with no allocator feature enabled the crate is a
// `Region<T>` handle store that keeps building on 32-bit targets, `std` or
// `no_std` (see `--no-default-features`, pinned on 32-bit by
// `tests/regression_r2_17_64bit_target_gate.rs`).
//
// This gate is deliberately POINTER-WIDTH-conditional, not
// arch-conditional: it rejects exactly the class of targets where the
// pinned layout premise (`*mut u8` / `usize` / `AtomicU64` alignment) is
// false, and says nothing about which architecture family a target belongs
// to. The full target support matrix lives in README.md "Target support".
// Review: docs/reviews/2026-09-22-120730-src-review-xa-round-2.md §R2-17.
#[cfg(all(feature = "alloc-core", not(target_pointer_width = "64")))]
compile_error!(
    "sefer-alloc: allocator features require a 64-bit target (R2-17 support \
     decision): `alloc-core` and every feature built on it (`alloc-global`, \
     `production`, ...) require metadata/magazine layout bounds, AtomicU64 and \
     pointer-provenance rules that have NOT been audited on 32-bit pointer \
     widths. Rebuild for a \
     64-bit target, or drop the allocator features (the region-only \
     `Region<T>` surface still works on 32-bit). See README.md \
     \"Target support\"."
);

// Phase 1: typed handle store, extracted to `sefer-region`. Re-exported here
// for backward compatibility — existing users of `sefer_alloc::{Region, Handle,
// SyncRegion}` continue to work unchanged. New consumers who want ONLY the
// handle store (no allocator stack) should depend on `sefer-region` directly.

#[cfg(feature = "experimental")]
mod concurrent;

// `alloc_core` is the Phase 8+ segment substrate. Its public surface is
// `AllocCore` / `SegmentLayout` (re-exported below via `pub use
// alloc_core::{...}`, which needs `mod alloc_core` in scope regardless of
// `internals`). With `internals` ON, the module is also `#[doc(hidden)] pub`
// so integration tests can reach internal `#[doc(hidden)]` test
// surfaces — this is the established test-only export pattern (see
// `registry` below). Nothing in `alloc_core` is stable public API.
//
// R34-3 (task #522, finding B1): `#[doc(hidden)]` alone hides this module
// from rustdoc but does NOT remove it from the public semver/ABI surface —
// see `SeferAlloc::alloc_batch`'s doc comment in `src/global/sefer_alloc/batch.rs`
// for the same principle already applied to two individual methods. Two
// mutually-exclusive `mod alloc_core;` declarations below (one `pub`, one
// `pub(crate)`, `cfg`-disjoint on `internals`) give the module body itself
// unconditional visibility to the REST of this file (so the crate-root
// `pub use alloc_core::{AllocCore, SegmentLayout, ...}` re-exports keep
// working with `internals` OFF, unconditionally on `alloc-core`/
// `alloc-decommit` as before) while making the EXTERNAL path
// `sefer_alloc::alloc_core::*` — everything NOT re-exported at the crate
// root, i.e. every `dbg_*` hook and internal type — reachable only when
// `internals` is enabled. See `internals`'s own doc comment in `Cargo.toml`
// for the full boundary/decision writeup.
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[doc(hidden)]
#[allow(missing_docs)]
pub mod alloc_core;
// R6-T16: no blanket `allow(dead_code, unused_imports)` here. Without
// `internals` this module's only external callers (integration tests
// reaching `dbg_*`/other non-re-exported items directly) are unreachable,
// so every item that exists solely for that white-box test surface is
// gated per-element with `#[cfg(feature = "internals")]` (or a scoped
// `cfg_attr(..., allow(...))` with a stated reason) instead of a
// module-wide suppression that would also hide genuinely dead code in the
// shipped `production` configuration.
#[cfg(all(feature = "alloc-core", not(feature = "internals")))]
pub(crate) mod alloc_core;

// `#[doc(hidden)] pub` (when `internals` is on; `pub(crate)` otherwise — not
// private `mod`) so the task #129 teardown-ordering tests
// (`tests/tls_heap_teardown_torn_sentinel.rs`,
// `tests/tls_heap_teardown_ordering_stress.rs`) can reach `global::tls_heap`'s
// `#[doc(hidden)]` test hook `dbg_teardown_then_resolve_is_fallback` — the
// same established test-only export pattern as `alloc_core`/`registry`
// above. Nothing in `global` beyond `SeferAlloc`/`AllocStats` (re-exported
// below, unconditionally on `alloc-global`, independent of `internals`) is
// stable public API.
//
// R34-3 (task #522, finding B1): additive `internals` gate on the EXTERNAL
// path, same rationale and same two-declaration shape as `alloc_core` above
// — see `internals`'s doc comment in `Cargo.toml`.
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[doc(hidden)]
#[allow(missing_docs)]
pub mod global;
// No blanket allow: same `internals`-off white-box-surface discipline as
// `alloc_core` above (R6-T16) — test-only hooks are `internals`-gated
// per-element, not silenced module-wide.
#[cfg(all(feature = "alloc-global", not(feature = "internals")))]
pub(crate) mod global;

// R34-3 (task #522, finding B1): additive `internals` gate on the EXTERNAL
// path, same two-declaration shape as `alloc_core` above — see
// `internals`'s doc comment in `Cargo.toml`.
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[doc(hidden)]
#[allow(missing_docs)]
pub mod registry;
// No blanket allow: same `internals`-off white-box-surface discipline as
// `alloc_core` above (R6-T16) — test-only hooks are `internals`-gated
// per-element, not silenced module-wide.
#[cfg(all(feature = "alloc-global", not(feature = "internals")))]
pub(crate) mod registry;

pub use sefer_region::{Handle, Region};

#[cfg(feature = "std")]
pub use sefer_region::SyncRegion;

#[cfg(feature = "experimental")]
#[allow(deprecated)]
pub use concurrent::{
    EpochHandle, EpochRegion, LockFreeHandle, LockFreeRegion, ShardedHandle, ShardedRegion,
};

#[cfg(feature = "pinning")]
pub use concurrent::PinnedRunner;

#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::LargeCacheConfig;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::LargeCacheMode;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::LargeCachePolicy;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::Profile;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::SmallPoolPolicy;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::SmallSegmentPoolConfig;
#[cfg(feature = "alloc-core")]
pub use alloc_core::{AllocCore, SegmentLayout};
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::{
    LargeThpCurve, LargeThpEstimator, LargeThpGradient, LargeThpMode, LargeThpRequestGroup,
    LargeThpShare,
};
// Re-exported so a caller of `SegmentLayout::try_class_for` (size-classes
// round-4 prepublish review P2-1) can name/match the error type without
// adding `size-classes` as their own direct dependency -- the first
// `size-classes`-crate type to appear in this crate's own public API.
#[cfg(feature = "alloc-core")]
pub use size_classes::InvalidAlign;

#[cfg(feature = "alloc-global")]
pub use global::{AllocStats, MaintenanceStartError, SeferAlloc};

// Ghost checks (docs/GHOST_CHECKS_PLAN.md §3): `ghost_assert!` / `ghost_event!`
// exist only in checking builds (loom, miri, kani, `--cfg sefer_ghost`) and expand
// to nothing in production. The macros and the shadow table live in the
// `sefer-ghost` crate (crates/primitives/sefer-ghost, plan step P1.1); this
// re-export keeps every `crate::ghost::…` call site resolving unchanged.
// `pub` + hidden only for the `internals` self-test.
#[cfg(all(feature = "internals", any(loom, miri, sefer_ghost)))]
#[doc(hidden)]
pub use sefer_ghost::ghost;
#[cfg(not(all(feature = "internals", any(loom, miri, sefer_ghost))))]
#[allow(unused_imports)] // the import is what `crate::ghost::…` call sites resolve through
use sefer_ghost::ghost;
