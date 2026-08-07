//! Compile-time assertions pinning [`Handle<T>`]'s Send/Sync bounds and layout.
//!
//! This file tests claims made in [`Handle`]'s documentation and implementation:
//!
//! 1. [`Handle<T>`] is unconditionally `Send + Sync` regardless of `T`, because it
//!    only holds a `slotmap::DefaultKey` plus a `PhantomData<fn() -> T>` (a
//!    fn-pointer phantom, which is Send+Sync regardless of T).
//!
//! 2. [`Handle<T>`]'s layout: `size_of::<Handle<T>>()` should be exactly 8 bytes
//!    (a `u32` index + a `NonZeroU32` version from slotmap's DefaultKey, with no
//!    padding), and `Option<Handle<T>>` should also be exactly 8 bytes via the
//!    niche optimization (the version field is NonZeroU32).
//!
//! The negative direction (that `SyncRegion<Cell<u32>>: Sync` must NOT compile)
//! was manually verified in `docs/reviews/2026-08-07-sefer-region-safety-review.md`
//! section 4 and is not tested here (would require a `compile_fail` doctest or a
//! trybuild dependency).

use sefer_region::Handle;
use std::mem::size_of;

/// Helper: compile-time check that a type implements both Send and Sync.
const fn assert_send_sync<T: Send + Sync>() {}

/// A type that is NOT Send (reference-counted, not thread-safe).
#[derive(Clone)]
struct NonSendType {
    _rc: std::rc::Rc<()>,
}

// NonSendType is deliberately not Send: Rc<T> is !Send.
// (Not explicitly implementing !Send; the auto-trait rules make it so.)

/// A type that is NOT Sync (interior mutability without synchronization).
struct NonSyncType {
    _cell: std::cell::Cell<u32>,
}

// NonSyncType is deliberately not Sync: Cell<T> is !Sync.
// (Not explicitly implementing !Sync; the auto-trait rules make it so.)

/// Verify Handle<T> is Send+Sync even when T is !Send.
///
/// If Handle<T>'s implementation ever changes such that it's no longer
/// unconditionally Send (e.g., by changing the PhantomData to `PhantomData<T>`),
/// this will fail to compile.
const _: () = assert_send_sync::<Handle<NonSendType>>();

/// Verify Handle<T> is Send+Sync even when T is !Sync.
///
/// If Handle<T>'s implementation ever changes such that it's no longer
/// unconditionally Sync (e.g., by changing the PhantomData to `PhantomData<T>`),
/// this will fail to compile.
const _: () = assert_send_sync::<Handle<NonSyncType>>();

/// Verify Handle<T> is Send+Sync even when T is neither Send nor Sync.
const _: () = assert_send_sync::<Handle<*const u32>>();

/// Verify Handle<T> is Send+Sync even when T contains both !Send and !Sync components.
const _: () = assert_send_sync::<Handle<std::rc::Rc<std::cell::Cell<u32>>>>();

/// Runtime test: verify Handle<T> is actually Send at runtime across threads.
#[test]
fn handle_is_send_even_when_t_is_not_send() {
    let h: Handle<NonSendType> = sefer_region::Region::new().insert(NonSendType {
        _rc: std::rc::Rc::new(()),
    });

    // This must compile: if Handle<T> is !Send, this thread::spawn call fails.
    std::thread::spawn(move || {
        // Use the handle to prove we received it.
        let _ = h;
    })
    .join()
    .expect("thread should succeed");
}

/// Runtime test: verify Handle<T> is actually Sync at runtime across threads.
#[test]
fn handle_is_sync_even_when_t_is_not_sync() {
    use std::sync::Arc;

    let h: Handle<NonSyncType> = sefer_region::Region::new().insert(NonSyncType {
        _cell: std::cell::Cell::new(0),
    });

    // This must compile: if Handle<T> is !Sync, Arc::clone won't work
    // for sharing across threads.
    let h_arc = Arc::new(h);

    let h_arc_clone = Arc::clone(&h_arc);
    std::thread::spawn(move || {
        // Use the handle to prove we received it.
        let _ = *h_arc_clone;
    })
    .join()
    .expect("thread should succeed");
}

/// Compile-time layout assertion: Handle<T> is exactly 8 bytes.
///
/// slotmap::DefaultKey is 8 bytes (u32 index + NonZeroU32 version), and
/// PhantomData is zero-sized, so Handle<T> should be exactly 8 bytes with no padding.
const _: () = assert!(size_of::<Handle<u8>>() == 8);

/// Compile-time layout assertion: Option<Handle<T>> is exactly 8 bytes.
///
/// Because Handle<T> contains a NonZeroU32 field (the version part of DefaultKey),
/// Option<Handle<T>> should use the niche optimization and also be exactly 8 bytes.
const _: () = assert!(size_of::<Option<Handle<u8>>>() == 8);

#[test]
fn handle_layout_matches_expectations() {
    // Runtime check for documentation purposes; the compile-time assertions above
    // are the real guard. This just makes the numbers visible in test output.
    assert_eq!(size_of::<Handle<u8>>(), 8, "Handle<T> should be 8 bytes");
    assert_eq!(
        size_of::<Option<Handle<u8>>>(),
        8,
        "Option<Handle<T>> should also be 8 bytes (niche optimization)"
    );
}
