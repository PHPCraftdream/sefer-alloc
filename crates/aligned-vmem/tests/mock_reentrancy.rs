//! A REAL regression test for task #945/M-1 (`mock::record`'s `RECORDING`
//! reentrancy guard), written for task #954 after independent review finding
//! 6.5 confirmed `tests/mock.rs`'s `reentrancy_guard_silently_drops_nested_calls`
//! is vacuous: it only performs ordinary, non-reentrant reserve/decommit/
//! recommit/drop sequences, so every one of its assertions passes identically
//! whether or not the `RECORDING` guard exists. Removing the guard (see the
//! module doc below for the exact counterfactual performed to confirm this)
//! does not fail that test.
//!
//! # Why a separate integration-test binary
//!
//! Genuine reentrancy into [`aligned_vmem::mock::record`] requires the
//! consumer's allocator itself to call back into `aligned-vmem` from inside
//! an allocation triggered by `record`'s own `Vec::push`. That means we need
//! a `#[global_allocator]` — which is process-global and can only be
//! installed once per binary — so this scenario cannot share a test binary
//! with `tests/mock.rs`'s ordinary (non-reentrant) tests. Cargo already
//! compiles every file under `tests/` as its own separate binary, so a
//! dedicated file is the natural (and only) place for this.
//!
//! # How the reentrancy is constructed
//!
//! [`mock::record`] (`crates/aligned-vmem/src/mock.rs`) does, in order:
//! 1. `RECORDING.set(true)`
//! 2. `CALLS.try_with(|c| c.borrow_mut().push(call))`
//! 3. `RECORDING.set(false)`
//!
//! Step 2 is the reentrancy window: if `Vec::push` needs to grow `CALLS`'s
//! backing buffer, it calls out to the process's `#[global_allocator]`. If
//! that allocator's `alloc()` itself calls back into `aligned-vmem` (e.g.
//! `reserve_aligned`, which itself calls `mock::record`), the nested
//! `record()` call runs while `RECORDING` is still `true` from the outer
//! call — genuine reentrancy, not a simulation.
//!
//! [`ReentrantProbeAlloc`] below is installed as `#[global_allocator]`. Its
//! `alloc()` checks a thread-local `TRIGGER_ON_NEXT_ALLOC` flag; when armed,
//! it disarms itself (so it fires exactly once, not recursively forever) and
//! calls `aligned_vmem::reserve_aligned` (mock cfg on) before delegating
//! to the real system allocator to service the original request. We arm the
//! trigger, then call `reserve_aligned` once on a FRESH thread (so `CALLS`
//! starts at `Vec::new()`, capacity 0 — its very first `push` is
//! GUARANTEED to grow the buffer and allocate, since a zero-capacity `Vec`
//! cannot service a `push` without hitting the allocator first). That first
//! `push`'s allocation triggers `ReentrantProbeAlloc::alloc`, which — because
//! armed — immediately calls `reserve_aligned` again, landing a second,
//! genuinely reentrant `mock::record(Call::Reserve { .. })` call while the
//! outer call's `RECORDING` flag is still `true`.
//!
//! # Confirming this is not vacuous (counterfactual, performed manually)
//!
//! Per this repo's zero-trust rule, the fix was temporarily reverted locally
//! (the `RECORDING` guard's early-return branch in `mock::record` commented
//! out, restoring the bare pre-M-1 `CALLS.with(|c| c.borrow_mut().push(call))`)
//! and this test was re-run:
//! [`reentrancy_is_silently_dropped_with_outer_record_intact`] panicked with
//! `BorrowMutError` (propagated out of `reserve_aligned`'s outer call, from
//! inside the nested `push`), proving the test WOULD fail without the fix.
//! The fix was then restored and the test passes again. See the final task
//! report for the exact commands run.

#![cfg(aligned_vmem_mock)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aligned_vmem::mock::{self, Call};
use aligned_vmem::reserve_aligned;

const MIB: usize = 1024 * 1024;

thread_local! {
    /// When `true`, the NEXT call to [`ReentrantProbeAlloc::alloc`] on this
    /// thread will (a) disarm itself and (b) call `reserve_aligned` before
    /// delegating to the system allocator — the reentrancy trigger.
    static TRIGGER_ON_NEXT_ALLOC: Cell<bool> = const { Cell::new(false) };
    /// Set to `true` inside the triggered nested `alloc()` call, confirming
    /// the trigger actually fired (as opposed to the growth allocation never
    /// happening at all, which would make this test vacuous the other way).
    static TRIGGER_FIRED: Cell<bool> = const { Cell::new(false) };
}

/// A `GlobalAlloc` wrapper around [`System`] that can be armed to call back
/// into `aligned-vmem` from inside `alloc()`, on demand, on the current
/// thread only. See the module doc above for the full scenario this builds.
struct ReentrantProbeAlloc;

// SAFETY: delegates every operation to `System`, which is itself a sound
// `GlobalAlloc` implementation; the only addition is a thread-local trigger
// check that performs a self-contained, non-reentering-into-`alloc`-again
// (the trigger disarms itself before recursing) call into `aligned-vmem`.
unsafe impl GlobalAlloc for ReentrantProbeAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let should_trigger = TRIGGER_ON_NEXT_ALLOC.with(|t| {
            if t.get() {
                // Disarm immediately: the nested `reserve_aligned` call below
                // will itself allocate (e.g. inside `mmap`'s bookkeeping or
                // the `Reservation` struct's own drop path), and we must NOT
                // trigger again for that, or every allocation from here on
                // would recurse forever.
                t.set(false);
                true
            } else {
                false
            }
        });
        if should_trigger {
            TRIGGER_FIRED.with(|f| f.set(true));
            // This call's own `try_reserve_aligned` -> `mock::record(...)`
            // runs here, reentering `record` while the OUTER `record` call
            // (the one whose `Vec::push` growth allocation led to this
            // `alloc()` call in the first place) still has `RECORDING` set.
            // Drop the reservation immediately -- we only need the mock-record
            // side effect, not a live reservation.
            let _ = reserve_aligned(64 * 1024, 64 * 1024);
        }
        // SAFETY: `layout` is the same layout the caller passed to us; we
        // forward it unchanged to `System`, which is the real allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged, `ptr`/`layout` came from a prior
        // `alloc`/`realloc` call on this same allocator (the `GlobalAlloc`
        // contract), which always delegates to `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged, same reasoning as `dealloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: ReentrantProbeAlloc = ReentrantProbeAlloc;

/// The positive-path regression test for M-1: a genuinely reentrant
/// `mock::record` call (constructed via the allocator-callback scenario
/// documented in the module header) must NOT panic, and the OUTER
/// (non-reentrant) call's recording must survive intact. The INNER
/// (reentrant) call's recording is documented (and asserted here) to be
/// silently dropped.
///
/// Counterfactual: with the `RECORDING` guard removed, this test panics with
/// `BorrowMutError` (verified manually -- see the module doc's "Confirming
/// this is not vacuous" section and the final task report).
#[test]
fn reentrancy_is_silently_dropped_with_outer_record_intact() {
    // Run on a FRESH thread so `CALLS` starts at `Vec::new()` (capacity 0)
    // on that thread -- guaranteeing the very first `push` inside `record`
    // must grow the buffer and therefore must call the global allocator,
    // which is the reentrancy trigger point.
    let handle = std::thread::spawn(|| {
        mock::reset();
        TRIGGER_FIRED.with(|f| f.set(false));
        TRIGGER_ON_NEXT_ALLOC.with(|t| t.set(true));

        // This is the OUTER call. Its `try_reserve_aligned` ->
        // `mock::record(Call::Reserve { .. })` sets `RECORDING = true`, then
        // `CALLS.borrow_mut().push(..)` on an empty `Vec` triggers a growth
        // allocation, which reaches `ReentrantProbeAlloc::alloc`, which (armed)
        // calls `reserve_aligned` again -- the INNER, genuinely reentrant call.
        // Without the M-1 fix this panics with `BorrowMutError` from the
        // double mutable borrow of `CALLS` on this thread.
        let outer = reserve_aligned(2 * MIB, 2 * MIB).expect("outer reserve must succeed");

        assert!(
            TRIGGER_FIRED.with(Cell::get),
            "the reentrancy trigger never fired -- the growth allocation \
             this test depends on did not happen, making the test vacuous \
             the OTHER way (never actually testing reentrancy at all)"
        );

        // Drop explicitly so the Release call lands before we drain, and so
        // we control exactly when it's recorded relative to the assertions
        // below.
        drop(outer);

        let calls = mock::drain();
        // We expect EXACTLY the outer call's own two calls (Reserve +
        // Release) -- the inner, reentrant Reserve call must have been
        // silently dropped by the RECORDING guard, not merged in. If the
        // guard were absent and no panic occurred (impossible with a plain
        // `RefCell`, but this assertion also documents the intended
        // contract independently of the panic), a length of 3 would mean
        // the reentrant call leaked into the log despite the guard.
        assert_eq!(
            calls.len(),
            2,
            "expected exactly [Reserve, Release] for the OUTER call only \
             -- the inner reentrant Reserve must be silently dropped, not \
             merged into the log: {calls:?}"
        );
        assert!(
            matches!(calls[0], Call::Reserve { size, align, .. } if size == 2 * MIB && align == 2 * MIB),
            "calls[0] must be the OUTER Reserve call: {:?}",
            calls[0]
        );
        assert!(
            matches!(calls[1], Call::Release { .. }),
            "calls[1] must be the OUTER Release call (from Drop): {:?}",
            calls[1]
        );
    });
    handle
        .join()
        .expect("reentrancy scenario thread must not panic");
}

/// Companion negative-shape assertion: confirms that, absent an armed
/// trigger, the SAME allocator wrapper is fully transparent (records exactly
/// the calls a plain, non-reentrant sequence would). This isolates the
/// wrapper itself as a source of behavioral difference, so a reader of
/// [`reentrancy_is_silently_dropped_with_outer_record_intact`] can trust that
/// test's failure mode (if the guard is removed) is really about reentrancy,
/// not an artifact of running under a custom global allocator.
#[test]
fn probe_allocator_is_transparent_when_not_armed() {
    let handle = std::thread::spawn(|| {
        mock::reset();
        TRIGGER_ON_NEXT_ALLOC.with(|t| t.set(false));
        let r = reserve_aligned(MIB, MIB).expect("reserve");
        drop(r);
        let calls = mock::drain();
        assert_eq!(
            calls.len(),
            2,
            "unarmed probe allocator must record exactly Reserve + Release, \
             same as the real system allocator would: {calls:?}"
        );
    });
    handle.join().expect("thread must not panic");
}
