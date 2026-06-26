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
//! ## Scope (honest)
//!
//! This is an *application-level* store, not a drop-in global allocator. The
//! global-allocator descent (`ByteRegion` + `GlobalAlloc`) is a later,
//! research-flagged phase; see `docs/PLAN.md`. For a process-wide allocator,
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

// Structural confinement of `unsafe` (compiler-checked, not prose):
//  - With NO features (or only `std`): `#![forbid(unsafe_code)]` — no `unsafe`
//    is possible anywhere in the crate.
//  - With `experimental` (3b-II `crossbeam-epoch` tier) and/or `byte`
//    (Phase 4 `ByteRegion` + `GlobalAlloc`, optionally + `byte-sharded` for
//    the Phase 7d parallel `ShardedByteArena`) and/or `alloc-core`
//    (Phase 8 self-hosted segment substrate) and/or `alloc-global`
//    (Phase 11 `SeferMalloc` `GlobalAlloc` face): the crate is
//    `#![deny(unsafe_code)]` (any `unsafe` outside an allowed module is a hard
//    error), and the confined modules lift this with `#![allow(unsafe_code)]`:
//      * `concurrent::hand` (under `experimental`), and
//      * `byte::byte_region` / `byte::byte_allocator`
//        / `byte::sharded_byte_arena` (under `byte`; the last only with
//        `byte-sharded`), and
//      * `alloc_core::os` (the OS segment aperture) and `alloc_core::node`
//        (the intrusive free-list node seam — the generalized `hand`
//        discipline) (both under `alloc-core`), and
//      * `global::sefer_malloc` (the `unsafe impl GlobalAlloc` malloc-face
//        seam — the trait obligation + pointer handoff to the Heap),
//        `global::tls_heap` (the Phase 12.3 raw-pointer TLS binding +
//        `AbandonGuard` seam — the `*mut HeapCore` handoff under the
//        single-writer invariant, and the `unsafe fn recycle`/
//        `abandon_segments` calls in the guard's drop), and
//        `global::fallback` (the Phase 12.3 primordial fallback heap seam —
//        the `static mut MaybeUninit<HeapCore>` + atomic-init state-machine
//        + spinlock-guarded `&mut` handout) (all under `alloc-global`), and
//      * `registry::heap_slot` + `registry::heap_registry` (the Phase 12.2
//        global heap slot-table seam — the `Sync`/`Send` impls on `HeapSlot`
//        under the atomic single-writer protocol, and the `*mut HeapCore`
//        pointer handoff out of a slot's `UnsafeCell`) (under `alloc-global`).
//    So "the `unsafe` lives in named modules" is enforced by the compiler in
//    EVERY configuration. See `src/concurrent/hand.rs`, `src/byte/*`,
//    `src/alloc_core/{os,node}.rs`, `src/global/sefer_malloc.rs`, and
//    `src/registry/{heap_slot,heap_registry}.rs`.
#![cfg_attr(
    not(any(
        feature = "experimental",
        feature = "byte",
        feature = "alloc-core"
    )),
    forbid(unsafe_code)
)]
#![deny(unsafe_code)]
// The single-threaded core (`Region<T>` / `Handle<T>`) needs only `alloc` and
// builds under `no_std`. Disable the default `std` feature to drop `SyncRegion`
// and the concurrent/byte tiers and get the bare `no_std` + `alloc` core.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

mod handle;
mod region;

#[cfg(feature = "std")]
mod sync_region;

#[cfg(feature = "experimental")]
mod concurrent;

#[cfg(feature = "byte")]
mod byte;

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

pub use handle::Handle;
pub use region::Region;

#[cfg(feature = "std")]
pub use sync_region::SyncRegion;

#[cfg(feature = "experimental")]
pub use concurrent::{
    EpochHandle, EpochRegion, LockFreeHandle, LockFreeRegion, ShardedHandle, ShardedRegion,
};

#[cfg(feature = "pinning")]
pub use concurrent::PinnedRunner;

#[cfg(feature = "byte")]
pub use byte::{ByteAllocator, ByteRegion};

#[cfg(feature = "alloc-core")]
pub use alloc_core::{AllocCore, SegmentLayout};

#[cfg(feature = "alloc")]
pub use heap::{with_heap, Heap};

#[cfg(feature = "alloc-global")]
pub use global::SeferMalloc;

#[cfg(feature = "byte-sharded")]
pub use byte::ShardedByteArena;
