//! # sefer-region — typed handle-addressed store
//!
//! A thin typed membrane over [`slotmap`](https://docs.rs/slotmap): values live
//! in `slotmap::SlotMap` — a contiguous slot array resolved by a single
//! indirection (the lookup/churn axis it was benchmarked to win; see
//! `docs/BENCHMARKS.md`). `SlotMap` keeps tombstone holes after removals, so it
//! is NOT always-compact; `DenseSlotMap` is the dense-iteration alternative.
//! Every operation exposes only typed [`Handle<T>`] values — raw `DefaultKey`s
//! never escape the crate boundary.
//!
//! ## What makes this different from using slotmap directly?
//!
//! Slotmap's `DefaultKey` is untyped: a `DefaultKey` from one map can be passed
//! to another of a different value type without a compile error. `sefer-region`
//! wraps it in `Handle<T>` — a `PhantomData<fn() -> T>`-branded key — so the
//! compiler rejects cross-region handle confusion at the type level.
//!
//! ## Invariants upheld (I1–I5)
//!
//! - **I1 — resolution:** a fresh handle resolves via [`Region::get`] to the
//!   inserted value until it is [`Region::remove`]d.
//! - **I2 — tombstone:** after `remove(h)`, `get(h)` is `None` forever; a
//!   second `remove(h)` is a no-op `None`.
//! - **I3 — no ABA:** a stale handle — one whose slot has since been reused —
//!   never resolves to a live value. slotmap's `DefaultKey` carries a generation
//!   that is bumped on removal, so the old handle fails the version check.
//! - **I4 — accounting:** [`Region::len`] equals the number of live entries and
//!   [`Region::is_empty`] agrees.
//! - **I5 — drop-once:** every live value is dropped exactly once — on `remove`
//!   (returned to the caller) or on `Region` drop — never twice, never leaked.
//!
//! ## Pure Rust / zero own unsafe
//!
//! `#![forbid(unsafe_code)]` at the top of this crate. The internal `unsafe` in
//! the `slotmap` dependency is its own, audited and battle-tested. This crate
//! adds no C / C++ libraries and contributes zero `unsafe` blocks of its own.
//!
//! ## `no_std` support
//!
//! With `default-features = false` (disabling `std`) the crate compiles under
//! `no_std + alloc`, providing [`Region<T>`] and [`Handle<T>`]. The `std`
//! feature (on by default) additionally enables [`SyncRegion<T>`], which wraps
//! `Region<T>` in `std::sync::RwLock`.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod handle;
mod region;

#[cfg(feature = "std")]
mod sync_region;

pub use handle::Handle;
pub use region::Region;

#[cfg(feature = "std")]
pub use sync_region::SyncRegion;
