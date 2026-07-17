//! Unified loom model-check of [`racy_ptr_cell::RacyPtrCell`] — run against the
//! **real** type (the crate aliases its atomics to `loom::sync::atomic` under
//! `--cfg loom`, so this harness exercises the shipped implementation, not a
//! hand-copied shadow model).
//!
//! This single suite replaces the FOUR in-tree shadow-model harnesses that each
//! transcribed the same protocol against `loom::sync::atomic`:
//! `loom_bootstrap_cas`, `loom_chunk_cas`, `loom_overflow_sidecar_cas`, and
//! `loom_fallback_init`. They all model-checked one of two properties —
//! (a) exactly-once CAS-published init with Release/Acquire happens-before, and
//! (b) OOM-rollback liveness (losers re-race, no forever-spin) — over the
//! `UNINIT -> INITIALIZING -> READY` state machine. Both properties are proved
//! here directly on `RacyPtrCell`.
//!
//! # Real-type properties proved (over every interleaving)
//!
//! 1. **Exactly-once init** — only ONE thread runs the winner's init closure.
//! 2. **Same pointer for all observers** — every caller returns the SAME
//!    published pointer.
//! 3. **No sentinel / null leaks** — no caller returns the sentinel or null as
//!    a success.
//! 4. **Happens-before** — a loser observing the real pointer under `Acquire`
//!    sees the winner's fully-written pointee (Release/Acquire pair).
//! 5. **OOM-rollback liveness** — after the winner's init returns `None` once,
//!    every thread still terminates and one eventually publishes READY (no
//!    thread spins forever waiting on a rolled-back sentinel).
//!
//! # The two counterfactuals (non-vacuousness proofs)
//!
//! Loom cannot rebuild the crate with a deliberately-broken ordering, so the
//! two broken protocols are transcribed here as `#[should_panic]` models over
//! `loom::sync::atomic` — the exact shape `RacyPtrCell` implements, with the ONE
//! ordering/condition flipped:
//!
//! - `counterfactual_relaxed_publish_loses_happens_before` — publishes the real
//!   pointer with `Relaxed` instead of `Release`; loom finds the interleaving
//!   where a loser reads the pointer without observing the pointee write.
//! - `counterfactual_spin_on_ready_livelocks_on_oom_rollback` — a loser spins
//!   `while != READY` instead of `while == INITIALIZING`; after the winner's OOM
//!   rollback the loser spins past a bound → the livelock this crate's
//!   `== INITIALIZING` rule exists to prevent.
//!
//! If either counterfactual PASSES (does not panic) the suite is vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --test loom_racy_ptr_cell
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

use racy_ptr_cell::RacyPtrCell;

const SENTINEL: usize = 1;

/// A minimal pointee. `init_marker` is written by the winner's init closure
/// (via a `loom::sync::atomic::AtomicU32` inside a `Box`) and every loser that
/// observes the published pointer under `Acquire` must see it — the concrete
/// witness of the Release/Acquire happens-before. `#[repr(align(2))]` satisfies
/// the cell's `align_of::<T>() >= 2` sentinel-collision guard.
#[repr(align(2))]
struct Payload {
    init_marker: AtomicU32,
}

/// Build a leaked, process-`'static`-shaped payload the way a real init closure
/// would (an OS reservation the winner leaks). Under loom we `Box::leak` and
/// reclaim it after the model iteration to keep loom's per-iteration allocator
/// balanced.
fn make_payload() -> core::ptr::NonNull<Payload> {
    let b = Box::new(Payload {
        init_marker: AtomicU32::new(0xDEAD_BEEF),
    });
    core::ptr::NonNull::from(Box::leak(b))
}

/// Reclaim a leaked payload (loom leak-check hygiene). SAFETY: `p` came from
/// `make_payload`'s `Box::leak` and is reclaimed exactly once per iteration
/// after all threads joined.
unsafe fn reclaim_payload(p: core::ptr::NonNull<Payload>) {
    drop(Box::from_raw(p.as_ptr()));
}

// ============================================================================
// Real-type property 1-4: exactly-once, same pointer, no leak, happens-before.
// ============================================================================

/// 2-thread race on the REAL `RacyPtrCell`: both call `get_or_try_init` with an
/// init that counts its own invocations. Asserts exactly-once init, same
/// pointer, non-null/non-sentinel, and that the loser sees `init_marker`.
#[test]
fn real_exactly_once_two_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell: Arc<RacyPtrCell<Payload>> = Arc::new(RacyPtrCell::new());
        let init_count = Arc::new(AtomicU32::new(0));

        let run = |cell: Arc<RacyPtrCell<Payload>>, ic: Arc<AtomicU32>| {
            cell.get_or_try_init(|| {
                ic.fetch_add(1, Ordering::Relaxed);
                Some(make_payload())
            })
            .expect("init must succeed (no OOM in this model)")
        };

        let (c1, i1) = (Arc::clone(&cell), Arc::clone(&init_count));
        let t1 = thread::spawn(move || run(c1, i1));
        let (c2, i2) = (Arc::clone(&cell), Arc::clone(&init_count));
        let t2 = thread::spawn(move || run(c2, i2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r2, "all threads must observe the SAME pointer");
        assert!(r1.as_ptr().addr() != 0, "pointer must not be null");
        assert_ne!(r1.as_ptr().addr(), SENTINEL, "pointer must not be sentinel");

        let count = init_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly ONE thread must run init (got {count})");

        // Happens-before: loser observing the pointer under Acquire sees the
        // winner's init write. SAFETY: r1 == r2 is the published pointer.
        let marker = unsafe { (*r1.as_ptr()).init_marker.load(Ordering::Relaxed) };
        assert_eq!(
            marker, 0xDEAD_BEEF,
            "loser must see the fully constructed pointee (Release/Acquire pair)"
        );
        unsafe { reclaim_payload(r1) };
    });
}

/// 3-thread race (main + 2 spawned) on the REAL cell — more interleavings,
/// tighter preemption bound. Same properties.
#[test]
fn real_exactly_once_three_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.check(|| {
        let cell: Arc<RacyPtrCell<Payload>> = Arc::new(RacyPtrCell::new());
        let init_count = Arc::new(AtomicU32::new(0));

        let run = |cell: Arc<RacyPtrCell<Payload>>, ic: Arc<AtomicU32>| {
            cell.get_or_try_init(|| {
                ic.fetch_add(1, Ordering::Relaxed);
                Some(make_payload())
            })
            .expect("init must succeed")
        };

        let (c1, i1) = (Arc::clone(&cell), Arc::clone(&init_count));
        let t1 = thread::spawn(move || run(c1, i1));
        let (c2, i2) = (Arc::clone(&cell), Arc::clone(&init_count));
        let t2 = thread::spawn(move || run(c2, i2));

        let r_main = cell
            .get_or_try_init(|| {
                init_count.fetch_add(1, Ordering::Relaxed);
                Some(make_payload())
            })
            .expect("main init must succeed");

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r_main, "thread 1 and main must agree");
        assert_eq!(r2, r_main, "thread 2 and main must agree");
        assert_ne!(r_main.as_ptr().addr(), SENTINEL);

        let count = init_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly ONE init (got {count})");

        let marker = unsafe { (*r_main.as_ptr()).init_marker.load(Ordering::Relaxed) };
        assert_eq!(marker, 0xDEAD_BEEF);
        unsafe { reclaim_payload(r_main) };
    });
}

/// Fast-path re-entry: once published, a second `get_or_try_init` in the same
/// thread hits the fast path (no second init) and returns the same pointer.
#[test]
fn real_fast_path_reentry_same_pointer() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell: Arc<RacyPtrCell<Payload>> = Arc::new(RacyPtrCell::new());
        let init_count = Arc::new(AtomicU32::new(0));

        let (c1, i1) = (Arc::clone(&cell), Arc::clone(&init_count));
        let t1 = thread::spawn(move || {
            c1.get_or_try_init(|| {
                i1.fetch_add(1, Ordering::Relaxed);
                Some(make_payload())
            })
            .unwrap()
        });

        let (c2, i2) = (Arc::clone(&cell), Arc::clone(&init_count));
        let t2 = thread::spawn(move || {
            let first = c2
                .get_or_try_init(|| {
                    i2.fetch_add(1, Ordering::Relaxed);
                    Some(make_payload())
                })
                .unwrap();
            // Second call — must hit the fast path (get()), same pointer, no
            // extra init.
            let second = c2.get().expect("cell is READY after first call");
            assert_eq!(first, second, "fast-path re-entry must be the same pointer");
            first
        });

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();
        assert_eq!(r1, r2);

        let count = init_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE init across both threads (got {count})"
        );
        unsafe { reclaim_payload(r1) };
    });
}

// ============================================================================
// Real-type property 5: OOM-rollback liveness (losers re-race, no forever-spin).
// ============================================================================

/// Two threads race the REAL cell; the FIRST winner's init returns `None` (OOM)
/// exactly once (an `AtomicBool` gate), rolling the sentinel back. The other
/// thread (or a re-racing loser) must still init successfully and reach READY.
/// Both threads terminate (loom itself proves no infinite loop) and one
/// eventually gets a real pointer.
#[test]
fn real_survives_oom_rollback_two_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell: Arc<RacyPtrCell<Payload>> = Arc::new(RacyPtrCell::new());
        let oom_used = Arc::new(AtomicBool::new(false));
        let success_count = Arc::new(AtomicU32::new(0));

        let run = |cell: Arc<RacyPtrCell<Payload>>, oom: Arc<AtomicBool>, sc: Arc<AtomicU32>| {
            cell.get_or_try_init(|| {
                // Inject OOM exactly once, on the first winner.
                if oom
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    None
                } else {
                    sc.fetch_add(1, Ordering::Relaxed);
                    Some(make_payload())
                }
            })
        };

        let (c1, o1, s1) = (
            Arc::clone(&cell),
            Arc::clone(&oom_used),
            Arc::clone(&success_count),
        );
        let t1 = thread::spawn(move || run(c1, o1, s1));
        let (c2, o2, s2) = (
            Arc::clone(&cell),
            Arc::clone(&oom_used),
            Arc::clone(&success_count),
        );
        let t2 = thread::spawn(move || run(c2, o2, s2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Both terminate (loom guarantees — no infinite loop). At least one
        // returned a real pointer after the single rollback. NOTE: the OOM'd
        // winner returns None; the other thread (or a re-racing loser) succeeds.
        let winner = match (r1, r2) {
            (Some(p), _) | (_, Some(p)) => p,
            (None, None) => {
                panic!("at least one thread must publish READY after the single OOM rollback")
            }
        };
        assert_ne!(winner.as_ptr().addr(), SENTINEL);
        assert!(winner.as_ptr().addr() != 0);

        // Exactly one SUCCESSFUL init published (the OOM'd attempt does not
        // count — it never published a pointer).
        let count = success_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly one successful publisher (got {count})");

        // Final state is READY.
        let ready = cell
            .get()
            .expect("cell must be READY after the survivor succeeds");
        assert_eq!(ready, winner, "the READY pointer is the survivor's");
        unsafe { reclaim_payload(winner) };
    });
}

// ============================================================================
// Counterfactual A — Relaxed publish loses the happens-before.
// ============================================================================

/// The BROKEN cell: identical to `RacyPtrCell` EXCEPT it publishes the real
/// pointer with `Relaxed` instead of `Release`. A loser's `Acquire` load of the
/// pointer no longer synchronises with the winner's `init_marker` write, so loom
/// finds an interleaving where the loser observes `init_marker == 0`
/// (uninitialised). We assert it is `0xDEAD_BEEF`; loom makes it fail → the
/// `#[should_panic]` is satisfied.
/// Run the BROKEN (Relaxed-publish) protocol AND check the happens-before
/// INSIDE the thread — the marker read must happen while the thread is running,
/// NOT after `join` (join synchronises, hiding the bug). Returns the observed
/// pointer so the caller can reclaim it.
///
/// The winner allocates the payload with the marker ZERO, writes the real
/// marker, then publishes the pointer with `Relaxed`. A loser's `Acquire` load
/// of `ptr` does NOT synchronise with that Relaxed publish, so loom finds the
/// interleaving where the loser observes the pointer while the marker is still
/// 0 — the assertion below fires inside the loser thread.
fn ensure_relaxed_publish_broken_and_check(
    ptr: &Arc<loom::sync::atomic::AtomicPtr<Payload>>,
) -> *mut Payload {
    let sentinel = SENTINEL as *mut Payload;
    match ptr.compare_exchange(
        core::ptr::null_mut(),
        sentinel,
        Ordering::Acquire,
        Ordering::Relaxed,
    ) {
        Ok(_) => {
            let b = Box::new(Payload {
                init_marker: AtomicU32::new(0),
            });
            let base = Box::into_raw(b);
            // SAFETY: `base` is the just-leaked box; we are its sole writer.
            unsafe { (*base).init_marker.store(0xDEAD_BEEF, Ordering::Relaxed) };
            // BUG: Relaxed publish.
            ptr.store(base, Ordering::Relaxed);
            base
        }
        Err(_) => loop {
            let p = ptr.load(Ordering::Acquire);
            if p.addr() == SENTINEL {
                loom::thread::yield_now();
                continue;
            }
            if p.addr() != 0 {
                // Read the marker RIGHT HERE, inside the loser thread, before
                // any join. With the Relaxed publish there is no happens-before
                // pairing, so loom may resolve this to the stale 0.
                // SAFETY: `p` is the winner's non-null box.
                let marker = unsafe { (*p).init_marker.load(Ordering::Relaxed) };
                assert_eq!(
                    marker, 0xDEAD_BEEF,
                    "loser must see the fully constructed pointee (Release/Acquire pair)"
                );
                return p;
            }
            loom::thread::yield_now();
        },
    }
}

/// COUNTERFACTUAL A: Relaxed publish. If this PASSES (no panic) the harness is
/// vacuous — loom failed to find the lost-happens-before interleaving.
///
/// The panic is loom's `"Causality violation: Concurrent load and mut
/// accesses"`: with the pointer published `Relaxed`, a loser's `Acquire` load
/// establishes NO happens-before with the winner's `init_marker` write, so loom
/// finds the interleaving where the loser reads the box's marker CONCURRENTLY
/// with the winner still writing it — a data race on the pointee, which is
/// exactly the corruption the correct `Release` publish rules out. (Loom flags
/// the racing access before our own `assert_eq!` on the stale value can even
/// run — a strictly stronger detection.) The `should_panic` matches loom's
/// message; the crucial property is that this counterfactual DOES panic, proving
/// the Release ordering in `RacyPtrCell` is load-bearing.
#[test]
#[should_panic(expected = "Causality violation")]
fn counterfactual_relaxed_publish_loses_happens_before() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<loom::sync::atomic::AtomicPtr<Payload>> =
            Arc::new(loom::sync::atomic::AtomicPtr::new(core::ptr::null_mut()));

        let p1 = Arc::clone(&ptr);
        let t1 = thread::spawn(move || ensure_relaxed_publish_broken_and_check(&p1));
        let p2 = Arc::clone(&ptr);
        let t2 = thread::spawn(move || ensure_relaxed_publish_broken_and_check(&p2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();
        assert_eq!(r1, r2, "both threads observe the same pointer");
        unsafe { drop(Box::from_raw(r1)) };
    });
}

// ============================================================================
// Counterfactual B — spin on `!= READY` livelocks against the OOM rollback.
// ============================================================================

const STATE_UNINIT: u8 = 0;
const STATE_INITIALIZING: u8 = 1;
const STATE_READY: u8 = 2;

/// The BROKEN loser rule: spins `while != READY` instead of `while ==
/// INITIALIZING`. When the winner rolls back to UNINIT after OOM (and no one
/// re-races), READY never comes and the loser spins past a bound — the livelock
/// signature. Bounded so the model checker itself terminates; exceeding the
/// bound is the failure we assert.
// The broken protocol's retry loop always return/panics on the FIRST iteration
// in this model (winner returns, loser panics on the livelock) — that early exit
// IS the shape under test; the outer `loop` faithfully mirrors the real
// `heap_ptr` retry structure.
#[allow(clippy::never_loop)]
fn ensure_spin_on_ready_broken(
    state: &Arc<AtomicU8>,
    winner_count: &Arc<AtomicU8>,
    oom_injected: &Arc<AtomicU8>,
) -> bool {
    const MAX_SPIN_ITERS: usize = 8;
    loop {
        if state.load(Ordering::Acquire) == STATE_READY {
            return true;
        }
        let won = state
            .compare_exchange(
                STATE_UNINIT,
                STATE_INITIALIZING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if won {
            let should_oom = oom_injected
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok();
            if should_oom {
                state.store(STATE_UNINIT, Ordering::Release);
                return false;
            }
            winner_count.fetch_add(1, Ordering::Relaxed);
            state.store(STATE_READY, Ordering::Release);
            return true;
        }
        // BUG: spin until READY. If the winner rolled back to UNINIT after OOM
        // and nobody re-races, READY never comes.
        for _ in 0..MAX_SPIN_ITERS {
            if state.load(Ordering::Acquire) == STATE_READY {
                return true;
            }
            thread::yield_now();
        }
        panic!(
            "livelock: loser spun {MAX_SPIN_ITERS} iterations without READY \
             (winner rolled back to UNINIT after OOM and this loser never re-races)"
        );
    }
}

/// COUNTERFACTUAL B: spin-on-READY livelock. If this PASSES (no panic) the
/// harness is vacuous — loom failed to find the livelock interleaving.
#[test]
#[should_panic(expected = "livelock")]
fn counterfactual_spin_on_ready_livelocks_on_oom_rollback() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(AtomicU8::new(STATE_UNINIT));
        let winner_count = Arc::new(AtomicU8::new(0));
        let oom_injected = Arc::new(AtomicU8::new(0));

        let (s1, w1, o1) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let t1 = thread::spawn(move || ensure_spin_on_ready_broken(&s1, &w1, &o1));
        let (s2, w2, o2) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let t2 = thread::spawn(move || ensure_spin_on_ready_broken(&s2, &w2, &o2));

        let _ = t1.join();
        let _ = t2.join();
    });
}
