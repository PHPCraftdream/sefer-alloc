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
//! wraps it in `Handle<T>` — a `PhantomData<fn() -> T>`-branded key — so the
//! compiler rejects cross-**type** handle confusion at the type level (a
//! `Handle<Foo>` cannot be used where a `Handle<Bar>` is expected). Note:
//! branding is by value type `T`, not by `Region` instance — a `Handle<T>`
//! from one `Region<T>` is accepted by a *different* `Region<T>` of the same
//! type and could silently access or remove a value keyed by the same
//! `DefaultKey` in that other instance.
//!
//! ## Invariants upheld (I1–I5)
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
#![cfg_attr(docsrs, feature(doc_cfg))]

mod handle;
mod region;

#[cfg(feature = "std")]
mod sync_region;

pub use handle::Handle;
pub use region::Region;

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use sync_region::SyncRegion;
