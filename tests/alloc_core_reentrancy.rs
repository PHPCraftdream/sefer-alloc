//! Reentrancy audit for the Phase 8 segment substrate (`alloc-core`) — M5.
//!
//! This is the **load-bearing** invariant of the substrate (§4 M5, §2 of
//! `ALLOC_PLAN.md`): NO entry point on the alloc/dealloc path may allocate
//! through the global allocator, take a global lock that could deadlock against
//! itself, or panic. The whole point of the self-hosted substrate is that when
//! `AllocCore` (or its Phase 11 `GlobalAlloc` face) IS the global allocator, a
//! re-entrant call would recurse infinitely.
//!
//! ## How this test proves it
//!
//! We install a counting `GlobalAlloc` wrapper as the test's global allocator.
//! It delegates to the system allocator but counts every `alloc`/`dealloc`
//! call. We then run a workload through `AllocCore` — using ONLY fixed-size
//! stack arrays for the test's own bookkeeping (so the test does NOT allocate
//! through the global allocator during the measured window) — and assert that
//! the allocation counter did NOT increase: `AllocCore` served every byte from
//! its OS-backed segments without ever touching the global allocator. This is
//! the runtime proof of M5.
//!
//! ## Structural note (complementary to the runtime check)
//!
//! A grep over `src/alloc_core/` confirms: no `Vec`, `Box`, `HashSet`,
//! `format!`, `String`, or `std::alloc` appears anywhere on the alloc path
//! (`alloc_core.rs`, `bootstrap.rs`, `segment_header.rs`, `segment_table.rs`,
//! `size_classes.rs`). The only `unsafe` modules are `os` (the OS syscalls,
//! which call `mmap`/`VirtualAlloc` directly — NOT `std::alloc`) and `node`
//! (intrusive pointer r/w). The metadata self-hosts in segment memory; there is
//! no path from `alloc`/`dealloc`/`realloc`/`alloc_zeroed` to a global
//! allocation.

// Under miri, `AllocCore` uses `std::alloc` (the miri aperture in `os.rs`), so
// the M5 reentrancy invariant is inapplicable. Additionally, the `#[global_allocator]`
// Counting wrapper triggers a Stacked Borrows violation inside `std`'s Windows
// System allocator — an upstream miri limitation, not our bug. This test is
// meaningful only on the real (non-miri) path.
#![cfg(not(miri))]
#![cfg(feature = "alloc-core")]
// R11-5: skip under `numa-aware-mock`. The mock's `thread_local! Vec<MockCall>`
// (in `numa-shim`, gated on its `mock` feature — pulled in by
// `numa-aware-mock`) allocates via the global allocator on the first
// `current_node()` dispatch (the `Vec::push` in `mock::record` grows the
// heap-allocated call log). That is a TEST-INFRASTRUCTURE allocation, not a
// production M5 violation: in a real (non-mock) build the platform
// `current_node_impl` does not touch the global allocator at all. The M5
// invariant is still checked in every non-mock feature configuration
// (`production,alloc-stats`, `numa-aware`, etc.) — this skip only avoids
// the false positive that arises when the mock backend replaces the real
// platform code under `--all-features`.
#![cfg(not(feature = "numa-aware-mock"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use sefer_alloc::AllocCore;

/// A counting wrapper around the system allocator. Installed as the test's
/// global allocator so we can observe any re-entrant allocation from inside
/// `AllocCore`.
///
/// **Counter scope is thread-local, not process-wide.** Earlier versions used
/// a process-global `AtomicUsize`, which made the test flaky on CI: the Rust
/// test harness, libstd, and libc background work (e.g. glibc's lazy
/// `getenv`-style init, panic-runtime setup, thread-local bootstrap on first
/// access from a background thread) can all touch the global allocator on
/// threads OTHER than the test thread, contaminating the delta. The whole
/// point of M5 is "AllocCore's own thread never re-enters the global
/// allocator", so a per-thread counter is the correct scope — what AllocCore
/// is doing on a *different* thread is irrelevant to this invariant.
struct Counting;

std::thread_local! {
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static DEALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}

fn alloc_count() -> usize {
    ALLOC_COUNT.with(|c| c.get())
}
fn dealloc_count() -> usize {
    DEALLOC_COUNT.with(|c| c.get())
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `Cell::set` does not allocate; thread-local key lookup with `const`
        // init is allocation-free in Rust 1.59+.
        let _ = ALLOC_COUNT.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = DEALLOC_COUNT.try_with(|c| c.set(c.get() + 1));
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Capacity for the test's stack-array bookkeeping (NOT a Vec — we must not
/// allocate through the global allocator during the measured window).
const LIVE_CAP: usize = 256;

#[test]
fn m5_alloc_path_does_not_touch_global_allocator() {
    // Build the layouts BEFORE the measured window (this Vec allocation is
    // outside the snapshot).
    let layouts: [Layout; 5] = [
        Layout::from_size_align(16, 8).unwrap(),
        Layout::from_size_align(64, 8).unwrap(),
        Layout::from_size_align(256, 16).unwrap(),
        Layout::from_size_align(4096, 4096).unwrap(),
        Layout::from_size_align(1024 * 1024, 4096).unwrap(), // large path
    ];

    let mut a = AllocCore::new().expect("primordial bootstrap");

    // Fixed-size stack arrays for bookkeeping — NO Vec, NO global allocation
    // during the measured window.
    let mut live_ptrs: [*mut u8; LIVE_CAP] = [core::ptr::null_mut(); LIVE_CAP];
    let mut live_layouts: [Layout; LIVE_CAP] = [Layout::from_size_align(1, 1).unwrap(); LIVE_CAP];
    let mut live_n: usize = 0;

    // Snapshot the counters IMMEDIATELY before the pure-AllocCore workload.
    let alloc_before = alloc_count();
    let dealloc_before = dealloc_count();

    // A representative churn: small allocs, large allocs, zeroed, realloc.
    for cycle in 0..20 {
        for (li, layout) in layouts.iter().enumerate() {
            let ptr = if cycle % 3 == 0 {
                a.alloc_zeroed(*layout)
            } else {
                a.alloc(*layout)
            };
            assert!(!ptr.is_null(), "alloc failed for layout {li}");
            // Stash in the stack array (no Vec growth → no global alloc).
            if live_n < LIVE_CAP {
                live_ptrs[live_n] = ptr;
                live_layouts[live_n] = *layout;
                live_n += 1;
            } else {
                // Cap full: free in place to make room (still no Vec).
                // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
                unsafe { a.dealloc(ptr, *layout) };
            }
        }
        // Free the odd-indexed survivors each cycle (exercises dealloc).
        let mut i = 1;
        while i < live_n {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { a.dealloc(live_ptrs[i], live_layouts[i]) };
            // Compact: move last into slot i.
            live_n -= 1;
            live_ptrs[i] = live_ptrs[live_n];
            live_layouts[i] = live_layouts[live_n];
            i += 1;
        }
    }

    // Realloc grow + shrink on a few survivors (no Vec; reuses slot).
    for (realloc_count, i) in (0..live_n).enumerate() {
        if realloc_count >= 4 {
            break;
        }
        let layout = live_layouts[i];
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
        let grown = unsafe { a.realloc(live_ptrs[i], layout, layout.size() * 2) };
        assert!(!grown.is_null());
        let gl = Layout::from_size_align(layout.size() * 2, layout.align()).unwrap();
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
        let shrunk = unsafe { a.realloc(grown, gl, layout.size() / 2 + 1) };
        assert!(!shrunk.is_null());
        live_ptrs[i] = shrunk;
        live_layouts[i] = Layout::from_size_align(layout.size() / 2 + 1, layout.align()).unwrap();
    }

    // The DELTA must be ZERO: `AllocCore` did not allocate a single byte
    // through the global allocator across the entire workload. This is M5.
    // (We check BEFORE dropping `a` and the stack arrays, since the harness's
    // own teardown may legitimately allocate.)
    let alloc_delta = alloc_count() - alloc_before;
    let dealloc_delta = dealloc_count() - dealloc_before;
    assert_eq!(
        alloc_delta, 0,
        "AllocCore allocated {alloc_delta} times through the global allocator (M5 violation)"
    );
    assert_eq!(
        dealloc_delta, 0,
        "AllocCore deallocated {dealloc_delta} times through the global allocator (M5 violation)"
    );

    // Now drop (outside the measured window).
    drop(a);
}
