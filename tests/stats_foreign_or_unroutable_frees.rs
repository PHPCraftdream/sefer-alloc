//! Review finding 2.3 — `SeferAlloc::stats().foreign_or_unroutable_frees`
//! makes the "foreign / unroutable free was silently dropped" no-op OBSERVABLE.
//!
//! ## The footgun this covers
//!
//! In a build WITHOUT `alloc-xthread` there is no cross-thread free routing: a
//! block freed on a heap that does not own its segment resolves to a base that
//! is not in that heap's segment table, falls into `AllocCore::dealloc`'s
//! foreign-pointer no-op, and is **leaked permanently**. `alloc-global` without
//! `alloc-xthread` is a legitimate single-threaded trade-off (so there is no
//! `compile_error!`), but a program built that way by mistake would leak with
//! no other observable signal. This counter is that signal.
//!
//! ## How the scenario is constructed (non-vacuous)
//!
//! We drive `SeferAlloc` directly via the `GlobalAlloc` trait (NOT installed as
//! this binary's `#[global_allocator]`, matching `tests/stats_reflects_activity.rs`
//! / `tests/global_alloc.rs`), snapshot `stats()`, then `dealloc` a pointer that
//! is GUARANTEED not to belong to any of this allocator's segments — a pointer
//! into a stack array. Its computed segment base is never registered in the
//! heap's `SegmentTable`, so `dealloc` takes the foreign/unroutable no-op branch
//! and bumps the counter. We assert the delta is strictly positive.
//!
//! A stub that never incremented (the pre-fix behaviour — a silent drop) would
//! leave the delta at 0 and fail this assertion: that is the counterfactual that
//! makes the test meaningful, not just "it doesn't panic".
//!
//! The per-event increment is gated behind `alloc-stats` (default OFF, not in
//! `production`), so the delta assertion itself is `alloc-stats`-gated — under a
//! build without `alloc-stats` the field reads 0 by design. The
//! **no-panic / safe-no-op** behaviour of freeing a foreign pointer is asserted
//! unconditionally.

// Scoped to the `alloc-global`-WITHOUT-`alloc-xthread` configuration — the
// exact footgun this counter observes. Under `alloc-xthread`, `dealloc` routes
// through `HeapCore::dealloc_routing`, whose foreign-pointer handling reads the
// candidate segment's header (`magic_at(base)`) to decide routing; passing a
// pointer into a stack array (whose "segment base" is arbitrary, possibly
// unmapped memory) would fault there — so a synthetic foreign-pointer free is
// only a SOUND probe in the `!alloc-xthread` build, where `AllocCore::dealloc`'s
// table-only `contains_base` guard rejects the pointer without dereferencing it.
// That `!alloc-xthread` build is also precisely the one where a foreign free is
// a permanent leak, i.e. the configuration the counter exists for.
#![cfg(all(feature = "alloc-global", not(feature = "alloc-xthread")))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

/// Free a pointer that provably does not belong to any `SeferAlloc` segment
/// (a pointer into a stack array). This exercises the foreign/unroutable
/// no-op branch of `dealloc`. It must be a safe no-op in every feature
/// configuration — never a panic, never a write to the foreign memory.
#[test]
fn foreign_pointer_free_is_a_safe_no_op_and_is_counted() {
    let a = SeferAlloc::new();

    // A real allocation first, so the allocator is warm and has at least one
    // registered segment — this makes the "foreign base is NOT in the table"
    // distinction meaningful (an empty table would trivially reject anything).
    let live_layout = Layout::from_size_align(64, 8).unwrap();
    // SAFETY: valid non-zero layout.
    let live = unsafe { a.alloc(live_layout) };
    assert!(!live.is_null());

    // A pointer that is provably NOT one of our segments: the address of a
    // stack local. Its segment base can never be in the heap's SegmentTable.
    let mut foreign_buf = [0u8; 64];
    let foreign_ptr: *mut u8 = foreign_buf.as_mut_ptr();

    let before = a.stats().foreign_or_unroutable_frees;

    // Free the foreign pointer. This must NOT panic and must NOT touch the
    // foreign memory — it is a safe no-op (and, under `alloc-stats`, counted).
    // SAFETY: `foreign_ptr` is a valid, aligned, non-null pointer to 64 bytes;
    // `dealloc`'s foreign-pointer guard rejects it before touching it. This is
    // the exact "block freed on a heap that does not own it" shape the counter
    // exists to observe; the guard makes it sound to pass here.
    unsafe { a.dealloc(foreign_ptr, live_layout) };

    // Prove we did not corrupt the foreign buffer (the no-op really is a no-op).
    assert_eq!(
        foreign_buf, [0u8; 64],
        "foreign free must not write the block"
    );

    let after = a.stats().foreign_or_unroutable_frees;

    // The per-event increment is `alloc-stats`-gated (default OFF, not in
    // `production`) — matching `tcache_hits` / `large_cache_hits`. Under
    // `alloc-stats` the counter must advance by at least the one foreign free
    // we provably performed on this thread; other tests in this binary racing
    // on the process-wide counter can only INCREASE the delta, never decrease
    // it, so `>` is the race-robust oracle.
    #[cfg(feature = "alloc-stats")]
    assert!(
        after > before,
        "foreign_or_unroutable_frees did not increase across a foreign free: \
         before={before}, after={after}"
    );

    // Without `alloc-stats` the field is a compiled-out no-op counter: it must
    // read a stable 0-delta (never garbage), the "stable shape across feature
    // combinations" guarantee.
    #[cfg(not(feature = "alloc-stats"))]
    assert_eq!(
        after, before,
        "without alloc-stats the counter must not move (increment compiled out)"
    );

    // Clean up the real allocation.
    // SAFETY: `live` was allocated above with `live_layout` and is still live.
    unsafe { a.dealloc(live, live_layout) };
}
