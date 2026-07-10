//! # sefer-alloc
//!
//! A safe, *handle-addressed* region store. Instead of handing out raw
//! pointers, a [`Region<T>`] hands out small generational [`Handle<T>`]
//! values; the bytes live in a dense, cache-friendly backing store that the
//! region is free to move. A stale handle never resolves to a live value — it
//! returns `None`, never undefined behaviour.
//!
//! ## The engine
//!
//! The single-threaded core is a thin **typed membrane** over
//! [`slotmap`](https://crates.io/crates/slotmap): [`Region<T>`] wraps a
//! `slotmap::SlotMap<DefaultKey, T>` and [`Handle<T>`] is a newtype over a
//! `DefaultKey` plus `PhantomData<fn() -> T>`, so handles stay generic-over-`T`
//! and typed (which raw slotmap keys are not). `slotmap`'s audited `unsafe`
//! owns the dense generational layout — the free list, generation bump on
//! remove, and version-saturation retirement; this crate adds the typed
//! boundary and stays `#![forbid(unsafe_code)]`.
//!
//! [`Region`], [`Handle`], and [`SyncRegion`] now live in the `sefer-region`
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
//! hit rates, cross-thread reclaim/overflow counts, and cumulative
//! segment/heap totals (`segments_reserved_total - segments_released_total`
//! is the live segment count — the field to alert on for a segment leak).
//! `stats()` is a handful of relaxed atomic loads (no locks, no allocation),
//! safe to poll on a metrics-scrape timer:
//!
//! ```no_run
//! # #[cfg(feature = "alloc-global")]
//! # {
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
//! # }
//! ```
//!
//! **Multi-thread footgun:** `alloc-global` without `alloc-xthread` has no
//! sound cross-thread free path — a block freed on a different thread than
//! it was allocated on leaks (safely, but permanently) instead of racing.
//! See `SeferAlloc`'s "Multi-thread safety" doc
//! section for the full explanation. Use `["alloc-global", "alloc-xthread"]`
//! (or the `production` bundle) for any real multi-threaded deployment.
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
//! ```
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

// ── Workspace: four independently-publishable companion crates ────────────────
//
// The workspace extracted four building blocks that can also be used standalone:
//
//   sefer-region    (crates/region)       — typed handle store (this re-export)
//   aligned-vmem    (crates/vmem)         — OS virtual-memory aperture
//   numa-shim       (crates/numa)         — NUMA detection + binding
//   malloc-bench-rs (crates/malloc-bench) — portable GlobalAlloc bench harness
//
// ── Unsafe inventory — the complete, verifiable picture ───────────────────────
//
// Source of truth: `grep -rln 'allow(unsafe_code)' src/ crates/`
//
// EXTERNAL publishable crates (each independently auditable):
//
//   aligned-vmem  (crates/vmem/src/lib.rs)         — #![allow(unsafe_code)]
//     Sole purpose: SEGMENT-aligned mmap/VirtualAlloc + page decommit/recommit.
//     Entire crate = OS aperture. Small, single-responsibility. Audit in isolation.
//
//   numa-shim     (crates/numa/src/lib.rs)         — #![allow(unsafe_code)]
//     Sole purpose: Linux mbind(2) via syscall(2), Windows VirtualAllocExNuma.
//     No libnuma dep. Small, single-responsibility. Audit in isolation.
//
//   malloc-bench-rs (crates/malloc-bench/src/lib.rs) — #![allow(unsafe_code)]
//     Confined to alloc_block / free_block / drain_mailbox helpers plus one
//     `unsafe impl Send for Block` (the cross-thread ownership-transfer token);
//     every unsafe call carries a // SAFETY: comment. Bench harness, not runtime.
//     Callers must supply a stateless-facade `A` (see `run`'s contract doc).
//
//   sefer-region  (crates/region/src/lib.rs)       — #![forbid(unsafe_code)]
//     Handle<T> / Region<T> / SyncRegion<T>. Zero own unsafe; slotmap's
//     audited unsafe owns the generational layout.
//
// INTERNAL sefer-alloc seams (compiler-enforced — a stray `unsafe` outside
// these named modules is a hard compile error in every configuration):
//
//  With NO features (or only `std`): `#![forbid(unsafe_code)]` — no `unsafe`
//  is possible anywhere in the crate.
//
//  With `experimental` (3b-II `crossbeam-epoch` tier) and/or `alloc-core`
//  (Phase 8 self-hosted segment substrate) and/or `alloc-global`
//  (Phase 11 `SeferAlloc` `GlobalAlloc` face): the crate is
//  `#![deny(unsafe_code)]` (any `unsafe` outside an allowed module is a hard
//  error), and the confined modules lift this with `#![allow(unsafe_code)]`:
//
//    Production path (`production` = alloc-global + alloc-xthread + alloc-decommit + fastbin):
//      * `alloc_core::os`   — thin interop wrapper around aligned-vmem; any
//                             additional unsafe blocks carry `// SAFETY:` proof.
//                             (under `alloc-core`)
//      * `alloc_core::node` — intrusive free-list node r/w through raw pointers;
//                             the generalized `hand` discipline. (under `alloc-core`)
//      * `global::sefer_alloc` — the `unsafe impl GlobalAlloc` alloc-face seam
//                             (trait obligation + pointer handoff). (under `alloc-global`)
//      * `global::tls_heap`     — raw-pointer TLS binding + `AbandonGuard` seam.
//                             (under `alloc-global`)
//      * `global::fallback`     — primordial fallback heap seam —
//                             `static mut MaybeUninit<HeapCore>` + atomic-init
//                             state-machine + spinlock-guarded `&mut` handout.
//                             (under `alloc-global`)
//      * `registry::bootstrap`     — primordial-segment carve / SegmentTable
//                             bootstrap seam — raw-pointer footprint carving
//                             of the metadata region under the atomic
//                             single-writer bootstrap protocol.
//                             (under `alloc-global`)
//      * `registry::heap_slot`     — `Sync`/`Send` impls + `UnsafeCell` hand-off.
//                             (under `alloc-global`)
//      * `registry::heap_registry` — `*mut HeapCore` pointer handoff out of a slot.
//                             (under `alloc-global`)
//
//    Optional `numa-aware` path:
//      * `alloc_core::numa` — thin interop wrapper around numa-shim; any
//                             additional unsafe blocks carry `// SAFETY:` proof.
//                             (under `numa-aware`)
//
//    Research / older tiers (not in production build):
//      * `concurrent::hand`         — epoch-tier AtomicSlot<T>. (under `experimental`, legacy/research-tier)
//
//  So "the `unsafe` lives in named modules" is enforced by the compiler in
//  EVERY configuration. Verifiable: `grep -rln 'allow(unsafe_code)' src/ crates/`
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
//    * `alloc_core::node::{write_usize, write_struct, offset, zero, ...}` — safe
//      `pub(crate)` fns whose whole body is a raw r/w; soundness rests on the
//      caller's bounds/exclusivity/`'static` invariants stated in prose.
//    * `os::release_segment` — a safe fn; a double call (double-release) from
//      safe code is UB (the OS reservation is freed twice).
//    * `os::{decommit_pages, recommit_pages}` — safe fns; the range-containment
//      invariant is the caller's, unchecked.
//    * `registry::heap_slot::HeapSlot` — its `state`/`heap` single-writer
//      invariant (which the slot's `Sync` proof and the `claim`/`recycle`
//      protocol depend on) is a prose contract; a safe CAS of `state` LIVE→FREE
//      from the wrong place breaks it. (Non-test fields are `pub(crate)` to keep
//      this membrane inside the crate — see that module's M7 note.)
//  In short: the membrane pattern concentrates the *unsafe blocks* into 14
//  files for audit, but the *soundness argument* spans those safe callers too.
//  This is a deliberate, worthwhile trade — named here so a future editor does
//  not misread "no stray unsafe" as "no UB reachable from safe code".
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
// ownership-checked routing path (`dealloc_routing`'s owner-identity stamp
// and the per-segment `RemoteFreeRing` both live behind `alloc-xthread`), so
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

// Phase 1: typed handle store, extracted to `sefer-region`. Re-exported here
// for backward compatibility — existing users of `sefer_alloc::{Region, Handle,
// SyncRegion}` continue to work unchanged. New consumers who want ONLY the
// handle store (no allocator stack) should depend on `sefer-region` directly.

#[cfg(feature = "experimental")]
mod concurrent;

// `alloc_core` is the Phase 8+ segment substrate. Its public surface is
// `AllocCore` / `SegmentLayout` (re-exported below). The module itself is also
// `#[doc(hidden)] pub` so the isolated ring test (`tests/remote_ring_unit.rs`)
// can reach `alloc_core::remote_free_ring::RemoteFreeRing`'s `#[doc(hidden)]`
// test surface — this is the established test-only export pattern (see
// `registry` below). Nothing in `alloc_core` is stable public API.
#[cfg(feature = "alloc-core")]
#[doc(hidden)]
pub mod alloc_core;

// `#[doc(hidden)] pub` (not private `mod`) so the task #129 teardown-ordering
// tests (`tests/tls_heap_teardown_torn_sentinel.rs`,
// `tests/tls_heap_teardown_ordering_stress.rs`) can reach `global::tls_heap`'s
// `#[doc(hidden)]` test hook `dbg_teardown_then_resolve_is_fallback` — the
// same established test-only export pattern as `alloc_core`/`registry`
// above. Nothing in `global` beyond `SeferAlloc`/`AllocStats` (re-exported
// below) is stable public API.
#[cfg(feature = "alloc-global")]
#[doc(hidden)]
pub mod global;

#[cfg(feature = "alloc-global")]
#[doc(hidden)]
pub mod registry;

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
#[cfg(feature = "alloc-core")]
pub use alloc_core::{AllocCore, SegmentLayout};

#[cfg(feature = "alloc-global")]
pub use global::{AllocStats, SeferAlloc};

#[cfg(kani)]
mod kani_proofs;
