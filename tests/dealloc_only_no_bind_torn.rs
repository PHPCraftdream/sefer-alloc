//! R6-OPT-P0-1 — the TORN half of the "dealloc without binding a heap"
//! deliverable: a thread whose `AbandonGuard` already recycled its registry
//! slot (see `global::tls_heap`'s "TLS teardown and the TORN sentinel"
//! module doc) must resolve a subsequent foreign dealloc via
//! `CurrentHeapForDealloc::ForeignNoBind`, WITHOUT taking the fallback
//! spinlock — not via `fallback::with_heap` (the pre-fix behavior).
//!
//! ## Why the deterministic hook, not real thread teardown
//!
//! As `tls_heap_teardown_torn_sentinel.rs` explains, forcing a specific
//! thread to actually exit while asserting on its TLS state from the outside
//! is not expressible in safe test code (the thread is gone by the time you
//! could observe it). `tls_heap` exposes
//! `dbg_teardown_then_resolve_is_foreign_no_bind`, the `current_for_dealloc`
//! analogue of the existing `dbg_teardown_then_resolve_is_fallback` hook: it
//! calls the EXACT SAME `mark_local_torn` function `AbandonGuard::drop`
//! calls (not a reimplementation), then resolves `current_for_dealloc()` and
//! reports whether it took the `ForeignNoBind` arm.
//!
//! ## Non-vacuous / counterfactual
//!
//! If `current_for_dealloc`'s TORN arm were wrong (e.g. mapped to `Own(TORN)`
//! instead of `ForeignNoBind`), `dbg_teardown_then_resolve_is_foreign_no_bind`
//! would return `false` and the first assertion below would fail. Verified by
//! hand during development: temporarily changing the `Ok(_) =>
//! CurrentHeapForDealloc::ForeignNoBind` arm in `current_for_dealloc` to
//! `CurrentHeapForDealloc::Own(p)` makes this test fail (and would also
//! reintroduce the exact UAF `current_for_alloc`'s TORN handling guards
//! against — see the module doc).
//!
//! ## End-to-end correctness + the "fallback lock not taken" proof
//!
//! Beyond the deterministic resolver-mapping proof above, this file also
//! drives a REAL TORN-thread dealloc end to end: a worker thread binds a
//! heap (one alloc), exits (its `AbandonGuard::drop` recycles the slot and
//! stamps `LOCAL` to `TORN`) — then, from the CURRENT (test) thread, we poke
//! the same `mark_local_torn` hook to reproduce the TORN state locally and
//! free a genuinely foreign pointer (allocated by a THIRD, still-live
//! heap) through the real `SeferAlloc::dealloc` entry point, asserting:
//! (a) the free completes without corrupting the allocator (a follow-up
//! alloc/dealloc round-trip on the live heap still works), and
//! (b) `dbg_fallback_lock_acquisitions()` is unchanged across the call —
//! i.e. the fallback spinlock was never taken, confirming the P0-1 shortcut
//! (route straight through `HeapCore::dealloc_foreign_routing`) fired
//! instead of the pre-fix `fallback::with_heap` path.
//!
//! Per project convention: tests live in `tests/`, not inline.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::global::{dbg_fallback_lock_acquisitions, tls_heap};
use sefer_alloc::SeferAlloc;

// Serialise: `dbg_fallback_lock_acquisitions` and `LOCAL` poking are
// process/thread-global state; run this file's tests one at a time.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// The deterministic resolver-mapping proof: TORN -> `ForeignNoBind`, via the
/// same-function hook (mirrors `tls_heap_teardown_torn_sentinel.rs`'s
/// `teardown_then_resolve_is_fallback` for `current_for_alloc`).
#[test]
fn torn_sentinel_resolves_to_foreign_no_bind() {
    let _serial = SerialGuard::acquire();
    for _ in 0..8 {
        assert!(
            tls_heap::dbg_teardown_then_resolve_is_foreign_no_bind(),
            "TORN sentinel did not resolve to CurrentHeapForDealloc::ForeignNoBind \
             — a post-teardown dealloc would either dereference a stale, \
             possibly-already-reclaimed registry slot pointer, or (if mapped \
             to Fallback like the alloc-side resolver) needlessly take the \
             fallback spinlock, defeating R6-OPT-P0-1's TORN-path win"
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

/// End-to-end: a real TORN-thread-shaped dealloc of a genuinely foreign
/// pointer (a) completes correctly (no corruption — verified by a follow-up
/// round-trip) and (b) does NOT take the fallback spinlock.
#[test]
fn torn_dealloc_of_foreign_pointer_is_correct_and_skips_fallback_lock() {
    let _serial = SerialGuard::acquire();

    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(64, 8).unwrap();

    // A separate, still-live heap allocates the block this "TORN" thread
    // will free — a genuinely cross-thread-owned pointer. This is NOT the
    // fallback-owned case: the pointer here is allocated by an ordinary
    // (non-fallback) registry-slot heap. The narrower "TORN AND the pointer
    // happens to be fallback-owned" case is a DELIBERATE, DOCUMENTED
    // trade-off — see the inline comment in `SeferAlloc::dealloc`
    // (`src/global/sefer_alloc.rs`, the `CurrentHeapForDealloc::ForeignNoBind`
    // match arm) for the full argument for why it is still correct, just
    // routed differently (ring-push instead of the fallback's own direct
    // `contains_base`-first free) than before this task. No EXISTING test in
    // this crate exercised that specific "TORN thread frees a
    // fallback-owned pointer, assert it takes the fast/direct path"
    // scenario prior to this task either (verified during development by
    // grepping `tests/` for "fallback" + "TORN"/"torn" — the only hits were
    // resolver-mapping tests for the ALLOC side, which never route a real
    // free through the fallback at all) — so there is nothing pre-existing
    // to update here, and this test does not attempt to re-derive that
    // narrower case from scratch.
    let owner_thread = std::thread::spawn(move || {
        let a = SeferAlloc::new();
        let layout = Layout::from_size_align(64, 8).unwrap();
        // SAFETY: valid non-zero layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "owner alloc returned null");
        p as usize
    });
    let addr = owner_thread.join().expect("owner thread panicked");
    // The owner thread has already exited by the time `join` returns; its
    // own heap slot may or may not yet be visible as recycled to this test,
    // but that is irrelevant here — the freed block is a real, live, mapped
    // sefer segment regardless of whether its ORIGINAL owner's slot has been
    // reused since. The header's `owner_thread_free` stamp still resolves to
    // whichever slot now owns that segment (whole-slot reuse — see
    // `registry::heap_registry`'s module doc), and the routing below is
    // correct either way.

    // Reproduce the TORN state on THIS (test) thread via the same
    // `mark_local_torn` function `AbandonGuard::drop` calls, saving/
    // restoring `LOCAL` exactly like `dbg_teardown_then_resolve_is_foreign_no_bind`
    // does internally — except here we keep the poisoned state across the
    // real `dealloc` call under test, then restore it manually.
    let saved = tls_heap::dbg_mark_local_torn_for_test();

    let before = dbg_fallback_lock_acquisitions();

    let ptr = addr as *mut u8;
    // SAFETY: `ptr` was allocated by the (now-exited) owner thread with
    // `layout` and handed over by value; this is the exact "TORN thread
    // frees a foreign pointer" shape under test. `a` here is a fresh
    // `SeferAlloc` value (not installed as `#[global_allocator]`), but
    // `dealloc` resolves via THIS THREAD's `LOCAL` (just poisoned to TORN
    // above), independent of which `SeferAlloc` instance issues the call —
    // matching the crate's own "process-wide TLS, not per-instance" model.
    unsafe { a.dealloc(ptr, layout) };

    let after = dbg_fallback_lock_acquisitions();

    // Restore this thread's real TLS state before doing anything else on it
    // (mirrors `dbg_teardown_then_resolve_is_foreign_no_bind`'s save/restore
    // discipline) — otherwise every subsequent allocator call on this test
    // thread (including the control alloc below and libtest's own harness
    // bookkeeping) would keep resolving as TORN.
    tls_heap::dbg_restore_local_for_test(saved);

    assert_eq!(
        after, before,
        "a TORN thread's dealloc of a genuinely foreign (non-fallback-owned) \
         pointer must NOT take the fallback spinlock (R6-OPT-P0-1): \
         lock-acquisitions before={before}, after={after}"
    );

    // Control: the allocator is still fully functional on this thread after
    // the TORN dealloc + restore — a genuine alloc/dealloc round-trip must
    // succeed, proving the TORN-routed free neither corrupted state nor
    // left this thread's TLS wedged.
    // SAFETY: valid non-zero layout.
    let p = unsafe { a.alloc(layout) };
    assert!(!p.is_null(), "allocator unusable after TORN-thread dealloc");
    // SAFETY: p was just allocated above with `layout`.
    unsafe { a.dealloc(p, layout) };
}
