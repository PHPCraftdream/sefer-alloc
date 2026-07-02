//! Regression test for task #132 (Defect 2) — the public `with_heap` API
//! used to PANIC on a reentrant borrow (and on TLS teardown), instead of
//! returning `None` as its `Option<R>` signature already allowed.
//!
//! ## What this guards against
//!
//! Pre-fix, `with_heap` was `HEAP.with(|cell| { let mut borrow =
//! cell.borrow_mut(); ... })`. `RefCell::borrow_mut` PANICS if the `RefCell`
//! is already mutably borrowed — which happens on a reentrant call: `f`
//! (the closure passed to the outer `with_heap`) calling `with_heap` again
//! (directly, or indirectly through a `Drop` impl that allocates while the
//! outer borrow is still held). For a PUBLIC allocator API this is a
//! footgun: a `Drop` impl that allocates during thread teardown, or any
//! accidental reentrant call, would abort the calling thread instead of
//! degrading gracefully to `None` (which the signature already supports —
//! `with_heap` has always returned `Option<R>`, legal on primordial OOM).
//!
//! Post-fix, `with_heap` uses the SAME `try_with`/`try_borrow_mut` no-panic
//! mechanics as the crate-internal `with_heap_try` (now literally the same
//! implementation function, `with_heap_impl`, in `src/heap/tls.rs`):
//! `try_borrow_mut` returns `Err` (converted to `None`, not a panic) on a
//! reentrant borrow.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Temporarily reverting `with_heap`'s body to the old `HEAP.with(|cell| {
//! let mut borrow = cell.borrow_mut(); ... })` makes THIS test panic instead
//! of observing `None` from the inner call — i.e. the test (which asserts
//! `catch_unwind` reports no panic AND the inner call is `None`) fails
//! loudly. Verified by hand during development (see the task report).
//!
//! ## Scope note
//!
//! The teardown branch (`AccessError` from `try_with`, i.e. calling
//! `with_heap` after the thread's TLS destructor has already run) is not
//! practically reachable from a deterministic integration test — Rust does
//! not expose a supported way to invoke code strictly AFTER a specific
//! `thread_local!`'s destructor without also tearing down the whole thread
//! (there's no way to keep the thread alive-but-torn-down from outside
//! `std`). This test therefore covers the REENTRANT branch only, which is
//! deterministic and exercises the identical `try_borrow_mut().ok()?`
//! no-panic mechanism the teardown branch shares (`try_with(...).ok()?`).

#![cfg(feature = "alloc")]

use sefer_alloc::with_heap;

#[test]
fn with_heap_reentrant_returns_none_not_panic() {
    // The OUTER call holds a `RefCell` borrow (via `try_borrow_mut`) for the
    // duration of its closure. The INNER call, made from WITHIN that
    // closure, must observe the borrow is already held and return `None` —
    // never panic.
    let outcome = std::panic::catch_unwind(|| {
        with_heap(|_outer_heap| {
            // Reentrant: this MUST return None (not panic), because the
            // outer `try_borrow_mut()` is still held.
            let inner = with_heap(|_inner_heap| 42_i32);
            assert_eq!(
                inner, None,
                "reentrant with_heap call did not return None while the \
                 outer borrow was held — the no-panic contract is broken \
                 (or, under the OLD panicking impl, we should never even \
                 reach this assert: `borrow_mut` would have panicked \
                 first)."
            );
            // Outer call still succeeds and returns its own value.
            7_i32
        })
    });

    // The whole thing must complete WITHOUT panicking (the outer
    // catch_unwind boundary), and the outer call must have succeeded
    // (Some(7)) since the outer heap access itself was never contended.
    match outcome {
        Ok(Some(7)) => {}
        Ok(other) => panic!("unexpected outer with_heap result: {other:?}"),
        Err(_) => panic!(
            "with_heap PANICKED on a reentrant call instead of returning \
             None (task #132 regression — the old RefCell::borrow_mut-based \
             implementation panics here)."
        ),
    }
}

/// Sister check: a NON-reentrant `with_heap` call still works normally
/// (returns `Some`, allocates/frees correctly) — the no-panic rewrite must
/// not have broken the ordinary, uncontended path.
#[test]
fn with_heap_normal_path_still_works() {
    let layout = std::alloc::Layout::from_size_align(64, 8).unwrap();
    let result = with_heap(|heap| {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        unsafe {
            std::ptr::write_bytes(p, 0xAB, 64);
            assert_eq!(p.read(), 0xAB);
        }
        heap.dealloc(p, layout);
        99_i32
    });
    assert_eq!(result, Some(99));
}
