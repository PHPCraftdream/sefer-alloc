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
//! process-wide allocator, use `SeferMalloc` (opt-in `production` feature) or
//! reach for `mimalloc`.
//!
//! See `docs/INVARIANTS.md` for the safety invariants this crate upholds and
//! `docs/DESIGN.md` for the architecture.
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
//     Confined to alloc_block / free_block / drain_mailbox helpers only;
//     every unsafe call carries a // SAFETY: comment. Bench harness, not runtime.
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
//  (Phase 11 `SeferMalloc` `GlobalAlloc` face): the crate is
//  `#![deny(unsafe_code)]` (any `unsafe` outside an allowed module is a hard
//  error), and the confined modules lift this with `#![allow(unsafe_code)]`:
//
//    Production path (`production` = alloc-global + alloc-xthread + alloc-decommit):
//      * `alloc_core::os`   — thin interop wrapper around aligned-vmem; any
//                             additional unsafe blocks carry `// SAFETY:` proof.
//                             (under `alloc-core`)
//      * `alloc_core::node` — intrusive free-list node r/w through raw pointers;
//                             the generalized `hand` discipline. (under `alloc-core`)
//      * `global::sefer_malloc` — the `unsafe impl GlobalAlloc` malloc-face seam
//                             (trait obligation + pointer handoff). (under `alloc-global`)
//      * `global::tls_heap`     — raw-pointer TLS binding + `AbandonGuard` seam.
//                             (under `alloc-global`)
//      * `global::fallback`     — primordial fallback heap seam —
//                             `static mut MaybeUninit<HeapCore>` + atomic-init
//                             state-machine + spinlock-guarded `&mut` handout.
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

#[cfg(feature = "alloc")]
mod heap;

#[cfg(feature = "alloc-global")]
mod global;

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

#[cfg(feature = "alloc")]
pub use heap::{with_heap, Heap};

#[cfg(feature = "alloc-global")]
pub use global::SeferMalloc;
