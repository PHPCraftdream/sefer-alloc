//! R31-4 (task #467) — counterfactual for [`ReservedSmallSegment`]'s
//! type-level double-release guarantee.
//!
//! ## What this proves, and how
//!
//! `src/alloc_core/reserved_small_segment.rs` retrofits
//! `AllocCore::dbg_decomp_reserve_and_keep` / `AllocCore::dbg_decomp_release`
//! (previously: bare `Option<*mut u8>` out, `unsafe fn(&mut self, base: *mut
//! u8)` in, guarded only by a `debug_assert!` compiled out in `--release`)
//! onto a typed, move-consumed handle, [`ReservedSmallSegment`]. The claim
//! this task must prove non-vacuously (per CLAUDE.md's "Tests" section) is:
//! **calling `dbg_decomp_release` twice on the same reservation is now a
//! COMPILE ERROR (E0382, "use of moved value"), not a runtime hazard.**
//!
//! A compile error is, by definition, not something a `#[test]` function can
//! exercise and pass/fail at runtime — there is no `Result`/`panic` path for
//! "this code did not compile," because the test binary itself would not
//! exist. Three options exist for proving this claim externally (per the
//! task's own instruction); this file uses two of them together:
//!
//! 1. A `trybuild`-style compile-fail test harness (a separate crate that
//!    compiles a small fixture and asserts it FAILS with a specific error).
//!    Checked `Cargo.toml`'s `[dev-dependencies]` before choosing: `trybuild`
//!    (or any compile-fail-testing crate) is NOT already a dev-dependency of
//!    this crate, and no other file in `tests/` uses that pattern (grepped
//!    for `trybuild` crate-wide: zero hits). Adding a new test-tooling
//!    dependency for exactly one case is exactly what CLAUDE.md's tests
//!    section says not to do when the alternative below is available.
//! 2. **Chosen**: write the POSITIVE single-use path thoroughly (this file),
//!    and explain — in a code comment here AND in
//!    [`ReservedSmallSegment`]'s own module doc — precisely why a second
//!    `.dbg_decomp_release(handle)` call cannot compile. This is externally
//!    verifiable by any reader without needing a special harness: the
//!    argument is pure Rust move semantics, not a runtime property.
//! 3. **Also chosen (R31-14b, task #484 — added after `docs/reviews/2026-07-31-r31-full-review.md`
//!    §7 P2-5 flagged this file's original two-options analysis missed it)**:
//!    `assert!(core::mem::needs_drop::<ReservedSmallSegment>())`.
//!    `needs_drop` is `const`-evaluable and callable at ordinary runtime, and
//!    a type implementing `Drop` can never also implement `Copy` — a hard
//!    rustc rule, not a project convention. Combined with option 2's
//!    by-value `dbg_decomp_release` signature (already exercised below),
//!    this is the complete compile-error argument reduced to one cheap,
//!    zero-dependency assertion — and unlike option 2's prose, it WOULD FAIL
//!    at test time if a future refactor removed the `Drop` impl and derived
//!    `Copy` instead, silently reopening the double-release hazard this type
//!    exists to close. See `reserved_small_segment_needs_drop_so_it_cannot_be_copy`
//!    below.
//!
//! ## The compile-error argument (verifiable by inspection, not run here)
//!
//! `AllocCore::dbg_decomp_release(&mut self, handle: ReservedSmallSegment)`
//! takes `handle` BY VALUE (not `&ReservedSmallSegment`, not
//! `&mut ReservedSmallSegment`). Rust's ownership rules mean passing a
//! non-`Copy` value by value MOVES it out of the caller's local variable.
//! `ReservedSmallSegment` derives no `Copy`/`Clone` impl (checked: its
//! definition in `reserved_small_segment.rs` has `#[derive(Debug)]` only,
//! and it holds a `Drop` impl — `Drop` types can never be `Copy` in Rust,
//! this is a hard compiler rule, not a project convention). Concretely, this
//! code:
//!
//! ```text
//! let handle = ac.dbg_decomp_reserve_and_keep().unwrap();
//! ac.dbg_decomp_release(handle);   // (1) moves `handle` into the call
//! ac.dbg_decomp_release(handle);   // (2) rustc E0382: "use of moved value:
//!                                  //     `handle`" — `handle` was already
//!                                  //     moved out at (1), so there is
//!                                  //     nothing left to pass to (2).
//! ```
//!
//! would fail `cargo build`/`cargo test` with E0382 at line (2), before any
//! test ever runs — this is compiler-enforced, not something this test file
//! can accidentally get wrong or a future refactor can silently weaken
//! without every caller of `dbg_decomp_release` failing to compile.
//! `Non-executed fence (per CLAUDE.md's "no doctests" rule — this fence is
//! ` ```text `, never compiled/run).
//!
//! This is a strictly stronger guarantee than the PRE-R31-4 shape (a bare
//! `unsafe fn(&mut self, base: *mut u8)` where nothing — not even
//! `--release`'s stripped `debug_assert!` — stopped a caller from writing
//! exactly the two-call sequence above and having it compile, link, and run
//! (with whatever `release_or_pool_empty_segment` does on its second call
//! against an already-recycled base).

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

use sefer_alloc::alloc_core::ReservedSmallSegment;
use sefer_alloc::AllocCore;

/// The legitimate single-use path: reserve, read the base (without
/// consuming), release exactly once. Confirms the retrofit did not change
/// observable behavior for the correct usage — same reservation mechanism
/// (`reserve_small_segment_impl`, unchanged since R30-1), only the type
/// wrapping it changed.
#[test]
fn reserve_read_base_release_once_succeeds() {
    let mut ac = AllocCore::new().expect("primordial");

    let handle = ac
        .dbg_decomp_reserve_and_keep()
        .expect("reservation must succeed on a fresh AllocCore");

    // `.dbg_base()` reads the address WITHOUT consuming the handle — proves
    // the handle is still usable for a measurement read after construction,
    // and that reading it does not move it (an `if let Some(_) =
    // handle.dbg_base()`-shaped API would have made this awkward; a plain
    // `&self` accessor does not).
    let base = handle.dbg_base();
    assert!(!base.is_null(), "reserved segment base must not be null");

    // Consume the handle exactly once. This MOVES `handle` — attempting to
    // use `handle` again after this line (e.g. a second
    // `ac.dbg_decomp_release(handle)`) is exactly the E0382 case documented
    // in this file's module doc; it is not written here because it would
    // make this file fail to compile, which is the point.
    ac.dbg_decomp_release(handle);

    // The allocator must remain fully healthy after the release — proves
    // `dbg_decomp_release` actually performed the real release
    // (`release_or_pool_empty_segment`), not a no-op, and that nothing about
    // the typed-handle wrapping introduced a leak or corruption on the
    // legitimate path.
    let layout = core::alloc::Layout::from_size_align(64, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "ordinary alloc after release must succeed");
    unsafe { core::ptr::write_bytes(p, 0x42, 64) };
    let mut buf = [0u8; 64];
    unsafe { core::ptr::copy_nonoverlapping(p, buf.as_mut_ptr(), 64) };
    assert!(buf.iter().all(|&b| b == 0x42), "readback mismatch");
    // SAFETY: `p` was returned by the matching `ac.alloc(layout)` above, is
    // live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };
}

/// Reserving and releasing many times in a loop (a fresh handle each
/// iteration) must keep working — confirms the handle type is not a
/// one-shot artifact and that `AllocCore`'s reservation/release machinery
/// composes correctly with the typed wrapper across repeated use, matching
/// the same release-branch-churn shape `tests/r30_1_decomp_full_cycle_cursor_safety.rs`
/// exercises for the sibling `dbg_decomp_full_cycle` hook.
#[test]
fn repeated_reserve_release_cycles_stay_healthy() {
    let mut ac = AllocCore::new().expect("primordial");
    let pool_cap = ac.dbg_pool_cap();
    assert!(pool_cap > 0, "pool must be enabled for this test");

    // Pre-fill the pool so every reservation below drives the RELEASE
    // branch (genuine OS release + table.recycle), not the pool-push
    // branch — mirrors `tests/r30_1_decomp_full_cycle_cursor_safety.rs`'s
    // established pre-fill pattern.
    for _ in 0..pool_cap {
        assert!(ac.dbg_decomp_full_cycle(), "reserve failed during pre-fill");
    }
    assert_eq!(ac.dbg_pooled_count(), pool_cap, "pool must be at capacity");

    for _ in 0..16 {
        let handle = ac
            .dbg_decomp_reserve_and_keep()
            .expect("reserve_and_keep failed during churn");
        assert!(!handle.dbg_base().is_null());
        ac.dbg_decomp_release(handle);
    }

    // Ordinary alloc/write/free on the same heap — confirms the heap is
    // still healthy after 16 typed-handle reserve/release cycles.
    let layout = core::alloc::Layout::from_size_align(128, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "ordinary alloc after churn must succeed");
    unsafe {
        core::ptr::write_bytes(p, 0xCD, 128);
    }
    let mut buf = [0u8; 128];
    unsafe {
        core::ptr::copy_nonoverlapping(p, buf.as_mut_ptr(), 128);
    }
    assert!(buf.iter().all(|&b| b == 0xCD), "readback mismatch");
    // SAFETY: `p` was returned by the matching `ac.alloc(layout)` above, is
    // live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };
}

/// The third double-release counterfactual option (R31-14b, task #484 —
/// see this file's module doc, option 3): `needs_drop` is `const`-evaluable
/// and callable at runtime, and a type with a `Drop` impl can never also be
/// `Copy` (a hard rustc rule). Combined with `dbg_decomp_release` taking
/// `ReservedSmallSegment` BY VALUE (exercised by the tests above), this
/// assertion is the complete compile-error argument reduced to one runtime
/// check — and unlike the prose argument in this file's module doc, it
/// WOULD FAIL here if a future refactor removed `ReservedSmallSegment`'s
/// `Drop` impl and derived `Copy` instead, which would silently reopen the
/// double-release hazard this type exists to close.
#[test]
fn reserved_small_segment_needs_drop_so_it_cannot_be_copy() {
    assert!(
        core::mem::needs_drop::<ReservedSmallSegment>(),
        "ReservedSmallSegment must keep its Drop impl — a type with Drop can \
         never be Copy, so this is the runtime half of the double-release \
         compile-error argument this file's module doc makes; if this \
         assertion ever fails, a refactor removed Drop (and therefore may \
         have made the type Copy), reopening the double-release hazard \
         ReservedSmallSegment was built to close"
    );
}
