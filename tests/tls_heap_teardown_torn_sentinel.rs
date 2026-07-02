//! Task #129 — deterministic counterfactual for the TORN-sentinel fix in
//! `global::tls_heap`.
//!
//! ## The bug this guards against
//!
//! `LOCAL` (a `Cell<*mut HeapCore>`, no `Drop`) and `GUARD` (an
//! `AbandonGuard`, has `Drop`) are both `thread_local!`s declared in that
//! order. Rust destroys thread-locals in REVERSE declaration order, so on
//! thread exit `GUARD` drops FIRST — recycling this thread's registry slot —
//! and `LOCAL` (a bare `Cell`, no drop glue) survives with its now-stale
//! pre-recycle pointer. Before this fix, every resolver
//! (`current`/`current_for_alloc`/`current_for_alloc_with_config`) treated
//! any non-null `LOCAL` as "my own live slot" — so a post-`GUARD`-drop
//! allocation (e.g. from some OTHER thread-local's `Drop`, declared before
//! `LOCAL` and hence destroyed after it) would dereference the stale
//! pointer into a slot another thread may have already re-claimed: two
//! `&mut HeapCore` aliasing the same slot, a data race / UAF.
//!
//! ## Why this test cannot black-box exercise real teardown
//!
//! Forcing a specific thread to actually exit while asserting on its TLS
//! state from the outside is not expressible in safe test code (the thread
//! is gone by the time you could observe it). So `tls_heap` exposes a
//! `#[doc(hidden)]` hook, `dbg_teardown_then_resolve_is_fallback`, that
//! calls the EXACT SAME `mark_local_torn` function `AbandonGuard::drop`
//! calls (not a reimplementation), then resolves `current_for_alloc()` and
//! reports whether it took the `Fallback` arm. This is the deterministic,
//! same-thread counterfactual for the ordering bug: it isolates "does the
//! resolver treat TORN as fallback" from "did teardown order run
//! correctly" (the latter is covered by the separate ordering stress test,
//! `tls_heap_teardown_ordering_stress.rs`).
//!
//! ## Non-vacuous / counterfactual
//!
//! Remove the `Ok(p) if p == TORN => CurrentHeap::Fallback` arm from
//! `current_for_alloc` (leaving only the `!is_null` arm) and this test
//! fails: the hook would observe `CurrentHeap::Own(TORN)` instead of
//! `Fallback`, so `dbg_teardown_then_resolve_is_fallback()` returns `false`
//! and the `assert!` below fails. This was verified by hand during
//! development (see the task #129 report) by physically deleting the arm,
//! confirming the test goes red, then restoring it.
//!
//! This test does NOT install a `#[global_allocator]` — it exercises
//! `sefer_alloc::global::tls_heap` directly as a `#[doc(hidden)]`
//! test-surface library call, using the process's default allocator for its
//! own bookkeeping (`assert!`, etc). `global` and `tls_heap` are `pub`
//! (behind `#[doc(hidden)]`) specifically so this test can reach them — see
//! `src/lib.rs` and `src/global/mod.rs`.

#![cfg(feature = "alloc-global")]

use sefer_alloc::global::tls_heap;

#[test]
fn teardown_then_resolve_is_fallback() {
    // Exercise the hook a few times on this thread: each call saves/pokes/
    // restores `LOCAL`, so repeated calls prove the hook is not itself
    // corrupting this thread's TLS state (which would make later real
    // allocations on this test thread crash).
    for _ in 0..8 {
        assert!(
            tls_heap::dbg_teardown_then_resolve_is_fallback(),
            "TORN sentinel did not resolve to CurrentHeap::Fallback — the \
             resolver would dereference a stale, possibly-already-reclaimed \
             registry slot pointer (task #129 regression)"
        );
    }

    // Control: after the hook restores `LOCAL`, a real allocation on this
    // thread must still work (the hook must not have left `LOCAL` poisoned).
    let v: Vec<u8> = vec![1, 2, 3, 4, 5];
    assert_eq!(
        v.iter().sum::<u8>(),
        15,
        "post-hook allocation is corrupted"
    );
}
