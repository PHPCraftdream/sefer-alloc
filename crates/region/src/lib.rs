//! # sefer-region — typed handle-addressed store
//!
//! A thin typed membrane over [`slotmap`](https://docs.rs/slotmap): values live
//! in `slotmap::SlotMap` — a contiguous slot array resolved by a single
//! indirection (the lookup/churn axis it was benchmarked to win; see
//! <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/BENCHMARKS.md>). `SlotMap` keeps tombstone holes after removals, so it
//! is NOT always-compact; `DenseSlotMap` is the dense-iteration alternative.
//! Every operation exposes only typed [`Handle<T>`] values — raw `DefaultKey`s
//! never escape as usable values through the API (Debug output renders the
//! underlying key for diagnostics only — it cannot be turned back into a
//! functioning handle through this crate's public surface).
//!
//! **Runtime relationship to `sefer-alloc`:** This crate exists as a public
//! surface for the `sefer-alloc` workspace root crate (which re-exports
//! `Region`, `Handle`, and `SyncRegion`), but it is **not used by the
//! `sefer-alloc` allocator runtime itself**. Empirically verified by searching
//! the allocator source code (`src/`) in the main workspace: no direct calls
//! into `Region`/`Handle`/`SyncRegion` on any hot path. Further API evolution
//! is intentionally deferred until a confirmed external consumer requests it.
//!
//! ## What makes this different from using slotmap directly?
//!
//! Slotmap's `DefaultKey` is untyped: a `DefaultKey` from one map can be passed
//! to another of a different value type without a compile error. `sefer-region`
//! wraps it in `Handle<T>` — a `PhantomData<fn() -> T>`-branded key plus a
//! `region_id` — so the compiler rejects cross-**type** handle confusion at the
//! type level (a `Handle<Foo>` cannot be used where a `Handle<Bar>` is
//! expected), and the runtime `region_id` check rejects cross-**instance**
//! handle confusion at the value level: a `Handle<T>` minted by one
//! `Region<T>` is rejected (treated exactly like a stale handle — `None`/
//! `false`, no panic) by every *other* `Region<T>` of the same type, even one
//! whose slotmap key happens to collide with the handle's own key.
//!
//! ## Invariants upheld (I1–I7)
//!
#![doc = include_str!("invariants.md")]
//!
//! ## Pure Rust / zero own unsafe
//!
//! `#![forbid(unsafe_code)]` at the top of this crate. The internal `unsafe`
//! lives upstream, in the mature, widely-used `slotmap` dependency, not in
//! this crate. This crate adds no C / C++ libraries and contributes zero
//! `unsafe` blocks of its own.
//!
//! ## `no_std` support
//!
//! With `default-features = false` (disabling `std`) the crate compiles under
//! `no_std + alloc`, providing [`Region<T>`] and [`Handle<T>`]. The `std`
//! feature (on by default) additionally enables [`SyncRegion<T>`], which wraps
//! `Region<T>` in `std::sync::RwLock`.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(target_has_atomic = "ptr"))]
compile_error!(
    "sefer-region requires a target with pointer-width atomic read-modify-write \
     support (target_has_atomic = \"ptr\") for its process-wide region_id counter \
     (Region::new/with_capacity use AtomicUsize::fetch_update). This target does not \
     provide it — e.g. riscv32imc (no `A` extension) is NOT supported despite any \
     earlier documentation suggesting otherwise."
);

mod handle;
mod region;

#[cfg(feature = "std")]
mod sync_region;

pub use handle::Handle;
pub use region::{Iter, IterMut, Region, RegionIdExhaustedError, TryReserveError};

// Test-only forwarder (see its own doc comment in `region.rs`): exposes the
// `region_id`-minting helper to integration tests in `tests/`, which can
// only reach items re-exported from the crate root. `#[doc(hidden)]` keeps
// it off docs.rs; this is not part of the public API.
#[doc(hidden)]
pub use region::dbg_try_mint_region_id;

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use sync_region::SyncRegion;
