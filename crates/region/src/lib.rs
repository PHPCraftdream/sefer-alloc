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
//! ## Invariants upheld (I1–I6)
//!
//! - **I1 — resolution:** a fresh handle resolves via [`Region::get`] to the
//!   inserted value until it is [`Region::remove`]d.
//! - **I2 — tombstone:** after `remove(h)`, `get(h)` returns `None` for
//!   roughly `2^31` reuse cycles of that slot (a stale handle that has
//!   survived that many insert/remove cycles may wrap and spuriously
//!   resolve to a later value). A second `remove(h)` is a no-op `None`.
//! - **I3 — no ABA:** a stale handle — one whose slot has since been reused —
//!   does not resolve to a live value for roughly `2^31` reuse cycles of
//!   that slot. slotmap's `DefaultKey` carries a generation counter bumped on
//!   removal, so the old handle fails the version check. After ~2^31 cycles the
//!   generation wraps and a very old handle may alias a later value.
//! - **I4 — accounting:** [`Region::len`] equals the number of live entries and
//!   [`Region::is_empty`] agrees.
//! - **I5 — drop-once:** every live value is dropped exactly once — on `remove`
//!   (returned to the caller) or on `Region` drop — never twice, never leaked.
//! - **I6 — instance isolation:** a `Handle<T>` resolves only through the
//!   [`Region<T>`] instance that minted it. Every accessor checks the
//!   handle's `region_id` against the region's own before touching the
//!   backing slotmap; a mismatch is treated exactly like a stale handle. Two
//!   `Region<T>`s can never alias each other's values through a shared
//!   `DefaultKey`, even when that key collides (as it commonly does — the
//!   first insert into any fresh `Region` tends to produce the same key).
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
pub use region::{Iter, IterMut, Region, RegionIdExhaustedError};

// Test-only forwarder (see its own doc comment in `region.rs`): exposes the
// `region_id`-minting helper to integration tests in `tests/`, which can
// only reach items re-exported from the crate root. `#[doc(hidden)]` keeps
// it off docs.rs; this is not part of the public API.
#[doc(hidden)]
pub use region::dbg_try_mint_region_id;

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use sync_region::SyncRegion;
