//! C6 (ph5b hardening matrix, gap #5, task #2094): the primordial-fallback
//! OOM injector `HeapCore::dbg_inject_fallback_oom_for_test`
//! (`src/global/fallback.rs`) must make the fallback alloc path return
//! null/None, and DISARMING it must fully restore service — no stuck OOM, no
//! panic, and the successfully-allocated block must dealloc cleanly.
//!
//! The injector is effective only BEFORE `STATE_READY` (it forces the
//! primordial `HeapCore::new` inside `fallback::heap_ptr` to fail and roll
//! back to `STATE_UNINIT`). This file is its own test binary and installs no
//! global allocator, so the process-wide fallback static is guaranteed UNINIT
//! until our first `dbg_with_fallback_for_test` — the same precondition
//! discipline as `tests/regression_fallback_init_unwind_guard.rs` (which
//! asserts it explicitly). Routing goes through the SAME production
//! `with_heap`/`LockGuard` path the installed `SeferAlloc` fallback arms use
//! (no test-only shortcut), mirroring `tests/r1_10_fallback_hits_counted_in_stats.rs`.
//!
//! Deltas-only note: the fallback is a process-global singleton shared by
//! every test in THIS binary — this file deliberately makes no assertion
//! about "which registry slot" or absolute counts, only about the fallback
//! alloc/dealloc outcomes of its own sequence.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::Layout;

use sefer_alloc::registry::HeapCore;

const LAYOUT: Layout = match Layout::from_size_align(64, 8) {
    Ok(l) => l,
    Err(_) => panic!("bad layout"),
};

#[test]
fn fallback_oom_injection_nulls_alloc_and_recovers_when_disarmed() {
    // ── Phase 1: arm the injector BEFORE the fallback ever initialises, then
    // allocate through the production fallback path → must be null/None
    // (true-OOM shape: `with_heap` reports None because the primordial
    // reservation "failed").
    HeapCore::dbg_inject_fallback_oom_for_test(true);
    let oom = HeapCore::dbg_with_fallback_for_test(|heap| heap.alloc(LAYOUT));
    assert!(
        oom.is_none(),
        "fallback alloc with the OOM injector armed returned {oom:?} — the injection did not surface as OOM"
    );

    // ── Phase 2: disarm → the SAME path must serve allocations again. The
    // init-state rollback (UNINIT after the injected primordial OOM) is what
    // makes this retry possible at all — a stuck INITIALIZING/failed state
    // would wedge every later fallback alloc.
    HeapCore::dbg_inject_fallback_oom_for_test(false);
    let p = HeapCore::dbg_with_fallback_for_test(|heap| heap.alloc(LAYOUT)).expect(
        "fallback heap must initialise and serve once the injector is disarmed — \
                 a stuck post-OOM state leaks the failure past disarm",
    );
    assert!(!p.is_null(), "post-disarm fallback alloc returned null");

    // ── Phase 3: dealloc of the fallback-served block must not panic.
    HeapCore::dbg_with_fallback_for_test(|heap| {
        // SAFETY: `p` is a unique live fallback allocation with `LAYOUT`.
        unsafe { heap.dealloc(p, LAYOUT) };
    })
    .expect("fallback lock for dealloc");
}
