//! Compile-time assertions tripwiring [`Handle<T>`]'s Send/Sync bounds and layout.
//!
//! `Handle<T>` deliberately carries no `#[repr(C)]`/`#[repr(transparent)]` (see
//! its own doc comment for why — no FFI use case, and the inner
//! `slotmap::DefaultKey` isn't `#[repr(C)]` upstream either, so pinning only
//! the outer order wouldn't yield a real stable ABI). The assertions below
//! are a **tripwire against silent drift** in this workspace's own build
//! (e.g. a future `slotmap` minor bump changing `DefaultKey`'s size) — not a
//! contract this crate promises to downstream consumers, who do not compile
//! this file at all.
//!
//! This file tests claims made in [`Handle`]'s documentation and implementation:
//!
//! 1. [`Handle<T>`] is unconditionally `Send + Sync` regardless of `T`, because it
//!    only holds a `slotmap::DefaultKey` plus a `PhantomData<fn() -> T>` (a
//!    fn-pointer phantom, which is Send+Sync regardless of T).
//!
//! 2. [`Handle<T>`]'s layout is WIDTH-DEPENDENT, because `region_id` is a
//!    pointer-width `NonZeroUsize` (not a fixed-width `NonZeroU64` — chosen
//!    so the type stays buildable on no_std targets without 64-bit atomics).
//!    `size_of::<Handle<T>>()` is 16 bytes on a 64-bit host (a `u32` index +
//!    `NonZeroU32` version from slotmap's `DefaultKey` = 8 bytes align 4,
//!    plus an 8-byte `NonZeroUsize` region_id align 8 — the stricter align
//!    pads the struct to 16) and 12 bytes on a 32-bit host (both fields
//!    align 4, so they pack with no padding). Verified empirically for both
//!    `x86_64-pc-windows-msvc` (16) and `i686-pc-windows-msvc` (12) — see
//!    the sefer-region F1 task summary. `Option<Handle<T>>` matches
//!    `Handle<T>`'s own size on every width via the niche optimization
//!    (region_id is `NonZeroUsize`).
//!
//! The negative direction (that `SyncRegion<Cell<u32>>: Sync` must NOT compile)
//! was manually verified in `docs/reviews/2026-08-07-sefer-region-safety-review.md`
//! section 4 and is not tested here (would require a `compile_fail` doctest or a
//! trybuild dependency).

use core::mem::size_of;
use sefer_region::Handle;

/// Helper: compile-time check that a type implements both Send and Sync.
const fn assert_send_sync<T: Send + Sync>() {}

/// A type that is NOT Send (reference-counted, not thread-safe).
#[cfg(feature = "std")]
#[derive(Clone)]
struct NonSendType {
    _rc: std::rc::Rc<()>,
}

// NonSendType is deliberately not Send: Rc<T> is !Send.
// (Not explicitly implementing !Send; the auto-trait rules make it so.)

/// A type that is NOT Sync (interior mutability without synchronization).
///
/// `core::cell::Cell`, not `std::cell::Cell` — this assertion does not need
/// `alloc`/`std` at all, so it (and the raw-pointer assertion below) run
/// unconditionally, giving `assert_send_sync` real users even under
/// `--no-default-features` (only the `Rc`-based assertions below genuinely
/// need `alloc`, so only those stay `#[cfg(feature = "std")]`-gated).
struct NonSyncType {
    _cell: core::cell::Cell<u32>,
}

/// Verify Handle<T> is Send+Sync even when T is !Send.
///
/// If Handle<T>'s implementation ever changes such that it's no longer
/// unconditionally Send (e.g., by changing the PhantomData to `PhantomData<T>`),
/// this will fail to compile.
#[cfg(feature = "std")]
const _: () = assert_send_sync::<Handle<NonSendType>>();

/// Verify Handle<T> is Send+Sync even when T is !Sync.
///
/// If Handle<T>'s implementation ever changes such that it's no longer
/// unconditionally Sync (e.g., by changing the PhantomData to `PhantomData<T>`),
/// this will fail to compile.
///
/// Unconditional (needs neither `alloc` nor `std`): see `NonSyncType`'s doc.
const _: () = assert_send_sync::<Handle<NonSyncType>>();

/// Verify Handle<T> is Send+Sync even when T is neither Send nor Sync.
///
/// Unconditional: `*const u32` is `!Send + !Sync` in `core`, no `alloc`/`std` needed.
const _: () = assert_send_sync::<Handle<*const u32>>();

/// Verify Handle<T> is Send+Sync even when T contains both !Send and !Sync components.
#[cfg(feature = "std")]
const _: () = assert_send_sync::<Handle<std::rc::Rc<std::cell::Cell<u32>>>>();

/// Expected `size_of::<Handle<u8>>()` for the host's pointer width.
///
/// `Handle<T>` contains: `slotmap::DefaultKey` (8 bytes: `u32` index +
/// `NonZeroU32` version, align 4), a `NonZeroUsize` region_id (pointer-width:
/// 8 bytes/align 8 on a 64-bit host, 4 bytes/align 4 on a 32-bit host), and
/// `PhantomData` (zero-sized). On 64-bit, region_id's stricter align-8 pads
/// the struct to 16 bytes; on 32-bit, both fields share align 4 and pack
/// with no padding to 12 bytes. Verified empirically by building this crate
/// for `x86_64-pc-windows-msvc` (16) and `i686-pc-windows-msvc` (12) — see
/// the sefer-region F1 task summary for the raw probe output. Written as a
/// single width-driven formula (rather than two hardcoded per-width
/// literals) so it stays correct if a third pointer width is ever exercised.
#[cfg(target_pointer_width = "64")]
const EXPECTED_HANDLE_SIZE: usize = 16;
#[cfg(target_pointer_width = "32")]
const EXPECTED_HANDLE_SIZE: usize = 12;

/// Compile-time layout assertion: Handle<T> matches the width-dependent
/// expected size (see `EXPECTED_HANDLE_SIZE`'s doc comment).
const _: () = assert!(size_of::<Handle<u8>>() == EXPECTED_HANDLE_SIZE);

/// Compile-time layout assertion: Option<Handle<T>> is the same size as
/// Handle<T> on this width.
///
/// Because Handle<T> contains NonZero fields (NonZeroUsize for region_id),
/// Option<Handle<T>> should use the niche optimization and match Handle<T>'s
/// own size exactly, on every pointer width.
const _: () = assert!(size_of::<Option<Handle<u8>>>() == EXPECTED_HANDLE_SIZE);
