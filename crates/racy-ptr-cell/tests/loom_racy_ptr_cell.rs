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
//! 6. **`dbg_rollback_reenterable` does not clobber a concurrent real winner**
//!    — a `dbg_rollback_reenterable` probe racing against a real
//!    `get_or_try_init` caller on the SAME cell must never cause a second
//!    `init` execution or two different published pointers, even though the
//!    probe's entry CAS only proves `UNINIT` at one instant, not for its
//!    whole duration.
//!
//! # The two counterfactuals (non-vacuousness proofs)
//!
//! Loom cannot rebuild the crate with a deliberately-broken ordering, so the
//! two broken protocols are transcribed here as `#[should_panic]` models over
//! `loom::sync::atomic`, each with the ONE ordering/condition under test
//! flipped:
//!
//! - `counterfactual_relaxed_publish_loses_happens_before` — models the SAME
//!   `AtomicPtr`-with-sentinel shape `RacyPtrCell` implements, but publishes
//!   the real pointer with `Relaxed` instead of `Release`; loom finds the
//!   interleaving where a loser reads the pointer without observing the
//!   pointee write.
//! - `counterfactual_spin_on_ready_livelocks_on_oom_rollback` — models the
//!   SAME three-state `UNINIT -> INITIALIZING -> READY` protocol, but over a
//!   simpler `AtomicU8` 3-state encoding rather than the packed
//!   `AtomicPtr`-with-sentinel `RacyPtrCell` actually uses (this
//!   simplification is deliberate — the livelock property under test depends
//!   only on the state machine's transitions, not on how a state is encoded
//!   into bits — but it means this ONE counterfactual is not literally
//!   `RacyPtrCell`'s exact bit-level shape, unlike the other one above). A
//!   loser spins `while != READY` instead of `while == INITIALIZING`; after
//!   the winner's OOM rollback the loser spins past a bound → the livelock
//!   this crate's `== INITIALIZING` rule exists to prevent.
//!
//! If either counterfactual PASSES (does not panic) the suite is vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test -p racy-ptr-cell --release \
//!     --test loom_racy_ptr_cell
//! ```
//!
//! Keep the `-p racy-ptr-cell`: `--cfg loom` is a global `RUSTFLAGS` cfg that
//! reaches every crate in the build, and under it `RacyPtrCell::new` is not
//! `const` — so an unscoped run can break any `static CELL: RacyPtrCell<T> =
//! RacyPtrCell::new();` elsewhere in the workspace. The README says the same
//! thing under "Running the loom suite"; this command must not drift from it.

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

/// Reclaim a leaked payload (loom leak-check hygiene).
///
/// # Safety
///
/// `p` came from `make_payload`'s `Box::leak` and is reclaimed exactly once
/// per iteration after all threads joined.
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
            let ptr = cell
                .get_or_try_init(|| {
                    ic.fetch_add(1, Ordering::Relaxed);
                    Some(make_payload())
                })
                .expect("init must succeed (no OOM in this model)");
            // Happens-before check INSIDE the thread, immediately after
            // `get_or_try_init` returns and before any `join` — `join` itself
            // establishes a happens-before relationship that would hide a lost
            // Release/Acquire pairing (task #700: the prior version of this
            // test read `init_marker` only after both threads had already
            // joined, which made this assertion vacuous — it stayed green even
            // with the publish downgraded to `Relaxed`). Mirrors
            // `ensure_relaxed_publish_broken_and_check`'s identical pattern for
            // the shadow model. SAFETY: `ptr` is the published, non-null,
            // non-sentinel pointer `get_or_try_init` just returned.
            // NOTE (task #774, finding F9): this `assert_eq!` cannot itself
            // observe a wrong VALUE — `make_payload` only ever stores
            // `0xDEAD_BEEF`, so there is no zero-then-store sequence for a
            // broken publish to expose here. What actually fails under a
            // `Relaxed` publish is loom's own causality checker
            // ("Causality violation: Concurrent load and mut accesses"),
            // triggered by this READ happening at a point with no
            // join-established happens-before to the winner's store; the
            // assertion is only the vehicle that forces the cross-thread
            // read to occur at that point. Do not "simplify" this into a
            // check that doesn't dereference `ptr` (e.g. comparing pointer
            // values) — that would silently remove the detection while
            // looking equivalent.
            let marker = unsafe { (*ptr.as_ptr()).init_marker.load(Ordering::Relaxed) };
            assert_eq!(
                marker, 0xDEAD_BEEF,
                "loser must see the fully constructed pointee (Release/Acquire pair)"
            );
            ptr
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

        // SAFETY: r1 (== r2) came from make_payload()'s Box::leak, via a
        // successful get_or_try_init; reclaimed exactly once here after all
        // threads joined.
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
            let ptr = cell
                .get_or_try_init(|| {
                    ic.fetch_add(1, Ordering::Relaxed);
                    Some(make_payload())
                })
                .expect("init must succeed");
            // Happens-before check INSIDE the thread, before any join — see
            // `real_exactly_once_two_threads`'s identical fix (task #700) for
            // why the check must not happen after `join`, and its NOTE (task
            // #774, finding F9) for why the real detector on a broken publish
            // is loom's own causality checker, not this `assert_eq!`'s value
            // comparison.
            // SAFETY: `ptr` is the published, non-null, non-sentinel pointer
            // `get_or_try_init` just returned.
            let marker = unsafe { (*ptr.as_ptr()).init_marker.load(Ordering::Relaxed) };
            assert_eq!(
                marker, 0xDEAD_BEEF,
                "loser must see the fully constructed pointee (Release/Acquire pair)"
            );
            ptr
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
        // Same check for the main "thread"'s own result, likewise BEFORE the
        // t1/t2 joins below (task #700) — see the F9 NOTE above for why
        // loom's causality checker, not this assertion's value comparison,
        // is the real detector on a broken publish.
        // SAFETY: `r_main` is the published, non-null, non-sentinel pointer
        // `get_or_try_init` just returned.
        let main_marker = unsafe { (*r_main.as_ptr()).init_marker.load(Ordering::Relaxed) };
        assert_eq!(
            main_marker, 0xDEAD_BEEF,
            "main must see the fully constructed pointee (Release/Acquire pair)"
        );

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r_main, "thread 1 and main must agree");
        assert_eq!(r2, r_main, "thread 2 and main must agree");
        assert_ne!(r_main.as_ptr().addr(), SENTINEL);

        let count = init_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly ONE init (got {count})");

        // SAFETY: r_main (== r1 == r2) came from make_payload()'s Box::leak,
        // via a successful get_or_try_init; reclaimed exactly once here
        // after all threads joined.
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
        // SAFETY: r1 (== r2) came from make_payload()'s Box::leak, via a
        // successful get_or_try_init; reclaimed exactly once here after all
        // threads joined.
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
        // SAFETY: winner came from make_payload()'s Box::leak, via the
        // single successful get_or_try_init publish; reclaimed exactly once
        // here after all threads joined.
        unsafe { reclaim_payload(winner) };
    });
}

// ============================================================================
// Real-type property 7: `get()` never reports a cell held at INITIALIZING.
// ============================================================================

/// Pins [`RacyPtrCell::get`]'s own published contract directly, rather than
/// inferring it from the exactly-once oracles: while a winner thread really
/// holds the `INITIALIZING` sentinel, a concurrent reader's `get()` must
/// return `None` — never `Some(sentinel)`, never `Some(null)` — and once the
/// winner has published, `get()` must return that exact pointer.
///
/// The exactly-once tests above catch an `is_ready` regression only
/// indirectly (a caller handed address `1` would fail on the `init_marker`
/// dereference). This one states the reader-side contract as its own
/// property: the reader calls `get()` and asserts, for every observation at
/// every interleaving, that the value is either `None` or the real published
/// pointer.
///
/// The winner deliberately does an observable store inside `init`, so the
/// sentinel is genuinely held across a scheduling point loom can interleave
/// the reader into.
#[test]
fn real_get_returns_none_while_a_winner_holds_the_sentinel() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let cell: Arc<RacyPtrCell<Payload>> = Arc::new(RacyPtrCell::new());
        let in_init = Arc::new(AtomicBool::new(false));

        // Winner: holds the sentinel across an observable step inside init.
        let (cw, iw) = (Arc::clone(&cell), Arc::clone(&in_init));
        let w = thread::spawn(move || {
            cw.get_or_try_init(|| {
                // Announce that the sentinel is now held, giving the reader
                // below a scheduling point to be interleaved into rather
                // than landing entirely before the claim CAS.
                iw.store(true, Ordering::Release);
                Some(make_payload())
            })
            .expect("init must succeed (no OOM in this model)")
        });

        // Reader: only ever calls the public `get()`, never `get_or_try_init`.
        let (cr, ir) = (Arc::clone(&cell), Arc::clone(&in_init));
        let r = thread::spawn(move || {
            // Two observations are enough — loom explores the scheduling,
            // not the iteration count.
            for _ in 0..2 {
                let _init_started = ir.load(Ordering::Acquire);
                match cr.get() {
                    None => {
                        // UNINIT or INITIALIZING — both correct for `get()`.
                    }
                    Some(p) => {
                        let addr = p.as_ptr().addr();
                        assert_ne!(addr, 0, "get() must never report null as Some");
                        assert_ne!(
                            addr, SENTINEL,
                            "get() must never report the INITIALIZING sentinel as Some \
                             — a reader would treat a mid-init cell as published"
                        );
                        // SAFETY: `get()` returned `Some`, so per its own
                        // contract the winner's `Release` publish
                        // happened-before this `Acquire` load and the pointee
                        // is fully written.
                        let marker = unsafe { (*p.as_ptr()).init_marker.load(Ordering::Relaxed) };
                        assert_eq!(
                            marker, 0xDEAD_BEEF,
                            "a pointer handed out by get() must point at a fully \
                             constructed pointee"
                        );
                    }
                }
            }
        });

        let winner = w.join().unwrap();
        r.join().unwrap();

        // After the winner joined the cell is READY, and `get()` must agree
        // with what the winner published.
        assert_eq!(
            cell.get(),
            Some(winner),
            "once the winner has published, get() must return that exact pointer"
        );

        // SAFETY: `winner` is the single published pointer from
        // `make_payload()`'s leak; both threads have joined, so it is
        // reclaimed exactly once here.
        unsafe { reclaim_payload(winner) };
    });
}

// ============================================================================
// Real-type property 6: dbg_rollback_reenterable vs. a concurrent real winner.
// ============================================================================

/// Reproduces the clobber this test exists to rule out: `dbg_rollback_reenterable`
/// (thread **P**, the probe) races against a real `get_or_try_init` caller
/// (thread **A**) on the SAME `UNINIT` cell.
///
/// The bad interleaving the pre-fix probe allowed:
/// 1. P's entry CAS wins (cell = sentinel).
/// 2. A's CAS fails (cell is P's sentinel); A enters the loser spin.
/// 3. P's step-2 rollback store (cell = null) fires; A observes null, falls
///    out of its spin, loops to the top, and its OWN CAS succeeds (cell = A's
///    sentinel). A starts running its `init` closure.
/// 4. P's step-3 postcondition CAS fails (cell is A's sentinel, not null).
/// 5. Pre-fix: P's step-4 store(null) fires UNCONDITIONALLY — clobbering A's
///    sentinel while A is still inside `init`. A second `get_or_try_init`
///    caller (or a re-race) can now win the CAS and run `init` a SECOND time,
///    breaking exactly-once init and publishing a second, different pointer.
///
/// Post-fix, P's step 4 is gated on P's own step-3 CAS having re-won the
/// cell; since step 3 failed here, P performs NO further store and returns
/// `None` ("not applicable"). A's sentinel survives untouched, A completes
/// its init and publishes exactly once.
///
/// This test asserts the two invariants the bug broke: exactly-once init,
/// and every successful caller observes the SAME published pointer. On the
/// pre-fix source (step 4 unconditional) loom finds the interleaving above
/// and this test fails; on the fixed source it passes.
#[test]
fn real_probe_rollback_does_not_clobber_concurrent_winner() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.check(|| {
        let cell: Arc<RacyPtrCell<Payload>> = Arc::new(RacyPtrCell::new());
        let init_count = Arc::new(AtomicU32::new(0));
        let published: Arc<loom::sync::Mutex<Vec<usize>>> =
            Arc::new(loom::sync::Mutex::new(Vec::new()));

        // Thread P: the probe under test.
        let cp = Arc::clone(&cell);
        let probe = thread::spawn(move || {
            let _ = cp.dbg_rollback_reenterable();
        });

        // Thread A: a real caller racing the probe on the same cell.
        let (ca, ia, pa) = (
            Arc::clone(&cell),
            Arc::clone(&init_count),
            Arc::clone(&published),
        );
        let a = thread::spawn(move || {
            let r = ca.get_or_try_init(|| {
                ia.fetch_add(1, Ordering::Relaxed);
                Some(make_payload())
            });
            if let Some(p) = r {
                // `NonNull<Payload>`/`*mut Payload` is `!Send`, so the
                // pointer itself cannot cross this thread boundary through
                // the shared `Mutex<Vec<_>>`. Only the ADDRESS is recorded,
                // purely so the assertions below can compare what each
                // caller observed; it is never turned back into a pointer.
                // Reclaim goes through `cell.get()` after the joins instead,
                // which keeps the whole test strict-provenance-clean with no
                // expose/with_exposed round trip at all.
                pa.lock().unwrap().push(p.as_ptr().addr());
            }
        });

        // Thread B: a second real caller, so a clobbered sentinel has a
        // concrete second winner available to race in and double-init.
        let (cb, ib, pb) = (
            Arc::clone(&cell),
            Arc::clone(&init_count),
            Arc::clone(&published),
        );
        let b = thread::spawn(move || {
            let r = cb.get_or_try_init(|| {
                ib.fetch_add(1, Ordering::Relaxed);
                Some(make_payload())
            });
            if let Some(p) = r {
                // Same rationale as thread A above: address only, for
                // comparison.
                pb.lock().unwrap().push(p.as_ptr().addr());
            }
        });

        probe.join().unwrap();
        a.join().unwrap();
        b.join().unwrap();

        // Exactly-once init: the probe's rollback must never let a second
        // `get_or_try_init` winner emerge alongside an already-running one.
        let count = init_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE real caller must run init despite the concurrent probe (got {count})"
        );

        // Same pointer for all successful observers.
        let seen = published.lock().unwrap();
        for addr in seen.iter() {
            assert_ne!(*addr, 0, "must never publish null as success");
            assert_ne!(
                *addr, SENTINEL,
                "must never publish the sentinel as success"
            );
        }
        // Unconditional, not `if seen.len() == 2` (task #774, finding F12):
        // in this model both real callers' closures always return `Some`,
        // so `seen.len()` is always 2 by construction — the old conditional
        // form could never actually skip, but made that fact something a
        // reader had to verify rather than something the assertion itself
        // states. Asserting the length first turns a hypothetical future
        // change (a caller able to return `None`) into a loud failure here
        // instead of a silently-skipped comparison below.
        assert_eq!(seen.len(), 2, "both real callers must have published");
        assert_eq!(
            seen[0], seen[1],
            "both real callers must observe the SAME published pointer"
        );

        let published_addr = seen[0];
        drop(seen);

        // Reclaim the ONE published payload through the cell itself. All
        // threads have joined, so the cell is quiescent and `get()` hands
        // back the published pointer with its provenance intact -- no
        // expose/with_exposed round trip, and no dedup needed, because there
        // is exactly one published pointer by the exactly-once assertion
        // above.
        let published_ptr = cell
            .get()
            .expect("the cell must be READY once both real callers succeeded");
        assert_eq!(
            published_ptr.as_ptr().addr(),
            published_addr,
            "cell.get() must hand back the same pointer the callers observed"
        );
        // SAFETY: `published_ptr` is the pointer a successful
        // `get_or_try_init` published (a leaked `make_payload()` box), read
        // straight out of the cell with its original provenance. Every
        // thread has joined, so nothing else can touch it, and it is
        // reclaimed exactly once here.
        unsafe { reclaim_payload(published_ptr) };
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
        // SAFETY: r1 (== r2) came from `Box::into_raw` inside
        // `ensure_relaxed_publish_broken_and_check`'s winner branch;
        // reclaimed exactly once here after both threads joined.
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
// IS the shape under test; the outer `loop` faithfully mirrors
// `RacyPtrCell::get_or_try_init`'s own retry loop structure (task #710: this
// comment previously named `heap_ptr`, a stale identifier from the parent
// repo this crate was extracted from — this crate has no `heap_ptr`).
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
