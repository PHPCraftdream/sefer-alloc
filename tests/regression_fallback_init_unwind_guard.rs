//! Regression (R34-17/task #536, release-stabilization finding F-8 [low]):
//! the fallback heap's primordial init (`fallback::heap_ptr`) must roll
//! `INIT_STATE` back to `UNINIT` if anything between the UNINIT→INITIALIZING
//! CAS and the READY publish **unwinds** — so loser threads stop spinning and
//! re-race the CAS, instead of livelocking forever on a stuck `INITIALIZING`.
//!
//! ## The defect this covers
//!
//! Before R34-17, `heap_ptr` CAS'd `INIT_STATE` UNINIT→INITIALIZING, then ran
//! `HeapCore::new(u32::MAX)` / the in-place `write` / `bind_thread_free`, then
//! published `READY` (or rolled back to `UNINIT` on primordial OOM) — with NO
//! guard. If any of those steps unwound, `INIT_STATE` stayed `INITIALIZING`
//! FOREVER, and every other thread that reached `heap_ptr` spun unbounded in
//! the `while INIT_STATE.load(Acquire) == STATE_INITIALIZING { spin_loop() }`
//! loser loop — a process-wide livelock. This is the EXACT failure mode
//! `LockGuard` already eliminated one function down (`with_heap`'s spinlock);
//! same RAII form, one level up. No current reachable production path triggers
//! the unwind (`HeapCore::new` is panic-hardened), so this is hardening, not a
//! live-bug fix.
//!
//! ## The fix under test
//!
//! `InitStateGuard` — armed right after the CAS is won; its `Drop` stores
//! `UNINIT` (Release) IF still armed when it goes out of scope (the unwind
//! path). Both normal exit paths (READY published / OOM-rolled-back) call
//! `disarm()` so the `Drop` is a no-op.
//!
//! ## How the unwind is forced
//!
//! A test-only `AtomicBool` (`DBG_INJECT_FALLBACK_INIT_PANIC`, `internals`-gated)
//! makes `heap_ptr` `panic!()` immediately after winning the CAS and BEFORE
//! `HeapCore::new` — a plain flag read that does NOT touch allocator metadata
//! through a raw pointer, so it is a safe injection point (not an `unsafe fn`),
//! following R34-15's `DBG_INJECT_CHUNK_OOM` model. The hook
//! `dbg_panic_in_fallback_init_rolls_back` arms the flag, `catch_unwind`s a
//! `heap_ptr` call (panics), clears the flag, then calls `heap_ptr` AGAIN —
//! returning `true` iff the second call completed (re-init succeeded).
//!
//! ## Non-vacuousness (counterfactual)
//!
//! The precondition assertion (`dbg_init_state() == STATE_UNINIT` on entry)
//! guarantees the injection is reachable (otherwise `heap_ptr`'s fast path
//! returns before the CAS). The watchdog join timeout is the counterfactual:
//! without `InitStateGuard`, the panicking first call leaves `INIT_STATE` stuck
//! at `INITIALIZING`, the second `heap_ptr` spins forever, the watchdog join
//! times out, and this test fails as a TIMEOUT. Verified by temporarily
//! neutering the guard's `Drop` (commenting out its `INIT_STATE.store`),
//! confirming the timeout, then restoring — `git diff` showed zero residual
//! changes.

#![cfg(all(all(feature = "alloc-global", feature = "std"), feature = "internals"))]

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use sefer_alloc::global::{
    dbg_init_state, dbg_panic_in_fallback_init_rolls_back, STATE_INITIALIZING, STATE_READY,
    STATE_UNINIT,
};

/// A panic out of the fallback init region must roll `INIT_STATE` back to
/// `UNINIT`, so a subsequent `heap_ptr` re-races the CAS and succeeds instead of
/// spinning forever on a stuck `INITIALIZING` (R34-17/task #536, F-8).
#[test]
fn fallback_init_panic_rolls_back_state_not_wedged() {
    // Precondition: the fallback must NOT already be initialised, or the
    // injection is unreachable (the fast path returns before the CAS). In a
    // dedicated test binary that does not install `SeferAlloc` as the global
    // allocator, the fallback is never hit by the harness, so this holds.
    assert_eq!(
        dbg_init_state(),
        STATE_UNINIT,
        "precondition: fallback must be UNINIT so the panic injection is \
         reachable (if already READY, this test binary initialised the fallback \
         elsewhere and the test is vacuous)"
    );

    // Run the hook on a dedicated thread with a bounded join: a wedged state
    // (regression) never sends → we time out. The hook itself installs
    // `catch_unwind`, so the panicking first `heap_ptr` does not abort the
    // thread.
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let ok = dbg_panic_in_fallback_init_rolls_back();
        let _ = tx.send(ok);
    });

    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(true) => {
            handle.join().expect("watchdog thread panicked");
        }
        Ok(false) => panic!(
            "fallback init panic-safety hook reported failure — the first \
             `heap_ptr` did not panic as expected, or the second returned null \
             (test hook broken)"
        ),
        Err(_) => panic!(
            "FALLBACK INIT STATE WEDGED: a panic inside `heap_ptr`'s guarded \
             region left `INIT_STATE == INITIALIZING` forever, so the second \
             `heap_ptr` spun in the loser loop indefinitely (InitStateGuard \
             missing / not rolling back on unwind)"
        ),
    }

    // Post-condition: after the successful re-init, the fallback is READY.
    assert_eq!(
        dbg_init_state(),
        STATE_READY,
        "post-condition: the second heap_ptr must have published READY"
    );
    // Sanity: the state never passed through a wedged INITIALIZING that the
    // test observed (it would have timed out above); this just confirms the
    // final published value is the terminal READY, not the panic-stuck
    // INITIALIZING.
    assert_ne!(
        dbg_init_state(),
        STATE_INITIALIZING,
        "state must not be wedged at INITIALIZING"
    );
}
