//! `bench-scale-tool` fixed-iteration benches for `RacyPtrCell<T>`.
//!
//! This crate is the hottest primitive in the sefer-alloc allocator's init
//! paths (lazy CAS-published pointer cell, used inside `#[global_allocator]`
//! bootstrapping), so its hot-path latency is worth tracking.
//!
//! **What these numbers are, and are not.** This file has WORKING rows
//! (`get_or_try_init/cold`, `get/hot`, `get_or_try_init/warm_already_ready`,
//! `contention/one_cell`) and their matching harness `baseline/*` rows. The
//! working rows exercise the cell protocol — CAS, `Release` publish,
//! `Acquire` load, loser spin; the `baseline/*` rows run the identical
//! surrounding harness with the cell call itself removed, so a working row
//! and its baseline can be read side by side to see roughly how much of the
//! working row's time the cell call itself accounts for. In particular no
//! row allocates inside its timed region: the payload every `init` closure
//! publishes is leaked ONCE, before any measurement, and the same
//! `NonNull` is republished into each fresh cell (legal precisely because
//! the cell does not own its pointee — it only stores and hands back the
//! pointer). An earlier version of this file called `Box::new` inside the
//! timed closure, which made the "cold" number mostly a measurement of the
//! system allocator plus monotonic heap growth, not of this crate.
//!
//! Read every `baseline/*` row before quoting an absolute figure. There
//! are two of them for the contention scenario specifically, at two
//! different scaffolding depths: `baseline/scaffolding_only` matches
//! `contention/one_cell` on BOTH sides of the round's SOURCE CODE SHAPE —
//! the three CONTENDER threads run the identical mutex-lock-then-`Arc`-clone
//! path in both rows (`Contention::round`'s shared `take_round_cell`
//! helper), and the benchmark thread's own timed routine mirrors holding a
//! pre-fetched `cell` without re-locking, exactly as `contention/one_cell`'s
//! benchmark-thread routine does — with only the `get_or_try_init` call
//! itself removed. **What `contention/one_cell - baseline/scaffolding_only`
//! is, and is not:** each row's timed value is a ROUND MAKESPAN — the
//! wall-clock span of a `start`-to-`done` barrier round across all
//! `CONTENDERS` threads, not a sum of independent per-operation costs. The
//! contention round's makespan is shaped by four threads' CAS/closure/
//! publish/spin work OVERLAPPING in time (including the timed
//! `get_or_try_init` call's own `init_body`, 64 `spin_loop` iterations
//! that are a benchmark-authored parameter, not part of `RacyPtrCell`
//! itself); the scaffolding-only round has none of that overlap, so its
//! critical path is shaped differently, not merely shorter by a fixed
//! amount. Subtracting the two makespans is a DIFFERENTIAL ESTIMATE under
//! this exact harness on this exact machine — a reasonable answer to "how
//! much slower is a full contended round than a matched control round
//! here" — not an algebraic decomposition that isolates the protocol's
//! intrinsic per-operation cost the way subtracting two SEQUENTIAL,
//! non-overlapping measurements would. For a regression signal across
//! revisions, compare `contention/one_cell` to itself with `CONTENDERS`/
//! `INIT_SPIN_ITERS`/setup held fixed; use the baseline rows to catch
//! harness/machine drift, not as a subtrahend to report as pure protocol
//! cost. `baseline/barrier_floor` strips the mutex/clone too, down to just
//! the two-barrier-crossing shape — useful context for how much of
//! `scaffolding_only`'s own makespan is barriers versus mutex/`Arc`, and
//! even less a candidate for exact subtraction than `scaffolding_only` is.
//! An earlier version of this file had only one baseline row, shaped like
//! `barrier_floor`, whose own contender threads still unconditionally
//! called `get_or_try_init` regardless of which row was being timed, AND
//! whose doc comment claimed the resulting difference was pure protocol
//! cost — neither the contamination nor the exact-isolation claim held up.
//!
//! Run:
//! ```text
//! cargo bench -p racy-ptr-cell --bench racy_ptr_cell_bench -- --calibrate 1
//! cargo bench -p racy-ptr-cell --bench racy_ptr_cell_bench
//! ```

use std::hint::black_box;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Barrier, Mutex};

use bench_scale_tool::Harness;
use racy_ptr_cell::RacyPtrCell;

// Same Payload shape as the tests — align(4) is sufficient (the cell needs
// align >= 2) and realistic for the allocator use case.
#[repr(align(4))]
struct Payload {
    #[allow(dead_code)]
    marker: u32,
}

/// Number of threads racing on ONE cell in the contention rows, INCLUDING
/// the benchmark thread itself. Tune here rather than at a call site so
/// every contention row and its baselines stay in lockstep.
const CONTENDERS: usize = 4;

/// How long the winner's `init` closure stays inside the critical section,
/// in `spin_loop` hints. This is the knob the contention rows exist to
/// explore: every loser spins for exactly as long as the winner's `init`
/// runs, so a longer `init` moves the measurement from "CAS + publish" to
/// "how expensive is the spin itself". Keep it small enough that the whole
/// suite still finishes in seconds.
const INIT_SPIN_ITERS: u32 = 64;

/// A process-lifetime payload, leaked once. Republished into every cell
/// under test — sound because `RacyPtrCell` never drops, frees, or reads
/// through its pointee, so the same pointer may back any number of cells.
/// Returned by value (`NonNull` is `Copy`).
fn shared_payload() -> NonNull<Payload> {
    NonNull::from(Box::leak(Box::new(Payload { marker: 42 })))
}

/// The winner-side work every `init` performs: a fixed, allocation-free
/// spin, then the shared pointer. Fixed cost so a contention row's number
/// reflects the protocol, not a variable payload.
fn init_body(p: NonNull<Payload>) -> Option<NonNull<Payload>> {
    for _ in 0..INIT_SPIN_ITERS {
        std::hint::spin_loop();
    }
    Some(p)
}

/// What a contender does after the `start` barrier this round. Set by the
/// benchmark thread before ITS OWN barrier crossing, so a round's actual
/// work always matches whichever row is currently being timed regardless of
/// row execution order — a contender never has to guess.
const MODE_CONTEND: u8 = 0;
/// Same scaffolding as `MODE_CONTEND` (mutex lock, `Arc` clone) with only
/// the `get_or_try_init` call itself skipped.
const MODE_SCAFFOLDING_ONLY: u8 = 1;
/// Neither the mutex nor the cell — the two barrier crossings alone.
const MODE_BARRIER_FLOOR: u8 = 2;
const MODE_SHUTDOWN: u8 = 3;

/// One round of the contention workload: a fresh cell is placed in `slot`,
/// all `CONTENDERS` threads meet at `start`, then behave according to
/// `mode`, and meet again at `done`.
struct Contention {
    slot: Mutex<Option<Arc<RacyPtrCell<Payload>>>>,
    start: Barrier,
    done: Barrier,
    mode: AtomicU8,
}

impl Contention {
    /// The per-round work a contender performs: meet at `start`, act
    /// according to `mode`, meet at `done` (skipped on shutdown). Returns
    /// `false` once shutdown is observed, so the caller's loop can end and
    /// the thread becomes joinable.
    ///
    /// Ordering: the benchmark thread's `mode.store` always happens, in
    /// real time, strictly before any contender's `start.wait()` can
    /// return — a `Barrier` only releases waiters once every participant,
    /// including the benchmark thread, has arrived, and the benchmark
    /// thread performs its store before its own arrival. `Release`/`Acquire`
    /// here make that ordering explicit in the code rather than resting on
    /// an unstated argument about `Barrier`'s internals.
    fn round(&self, payload: NonNull<Payload>) -> bool {
        self.start.wait();
        match self.mode.load(Ordering::Acquire) {
            MODE_SHUTDOWN => return false,
            MODE_BARRIER_FLOOR => {
                self.done.wait();
            }
            MODE_SCAFFOLDING_ONLY => {
                let cell = self.take_round_cell();
                black_box(&cell);
                self.done.wait();
            }
            _ => {
                let cell = self.take_round_cell();
                black_box(cell.get_or_try_init(|| init_body(payload)));
                self.done.wait();
            }
        }
        true
    }

    /// Lock `slot`, clone the round's published cell out of it. Shared by
    /// `MODE_CONTEND` and `MODE_SCAFFOLDING_ONLY` so the two modes pay
    /// IDENTICAL mutex/`Arc` cost — only the call after this differs.
    fn take_round_cell(&self) -> Arc<RacyPtrCell<Payload>> {
        self.slot
            .lock()
            .expect("contention slot mutex poisoned")
            .clone()
            .expect("the benchmark thread publishes a cell before each round")
    }
}

fn main() {
    let payload = shared_payload();
    let mut h = Harness::new("racy_ptr_cell_bench", env!("CARGO_MANIFEST_DIR"));

    // ── get_or_try_init/cold ────────────────────────────────────────────────
    // Cold path: every iteration gets a fresh UNINIT cell (built in the
    // UNTIMED setup) and runs the full init sequence — claim CAS, init
    // closure, `Release` publish. No allocation anywhere in the timed
    // region; `init` returns the pre-leaked shared payload.
    h.bench_batched(
        "get_or_try_init/cold",
        RacyPtrCell::<Payload>::new,
        move |cell| {
            black_box(cell.get_or_try_init(|| Some(payload)));
        },
    );

    // ── baseline/cold_setup_only ────────────────────────────────────────────
    // Same shape as the row above with the cell call removed, so the
    // scaffolding cost (batched-call overhead, black_box) can be subtracted
    // from it rather than guessed at.
    h.bench_batched(
        "baseline/cold_setup_only",
        RacyPtrCell::<Payload>::new,
        move |cell| {
            black_box(&cell);
            black_box(payload);
        },
    );

    // ── get/hot ─────────────────────────────────────────────────────────────
    // Hot path: cell is already READY; measures the single `Acquire` load.
    {
        let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
        cell.get_or_try_init(|| Some(payload)).unwrap();
        h.bench("get/hot", move || {
            black_box(cell.get());
        });
    }

    // ── get_or_try_init/warm_already_ready ──────────────────────────────────
    // Warm path through `get_or_try_init`: the cell is already READY, so the
    // fast path's first `Acquire` load returns early without running init.
    {
        let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
        cell.get_or_try_init(|| Some(payload)).unwrap();
        h.bench("get_or_try_init/warm_already_ready", move || {
            black_box(cell.get_or_try_init(|| Some(payload)));
        });
    }

    // ── contention/one_cell and its two baselines ───────────────────────────
    // The crate's most expensive documented scenario — the loser spin-wait.
    //
    // `baseline/scaffolding_only` matches `contention/one_cell` in SOURCE
    // SHAPE: same untimed setup (a fresh cell published to `slot`); the
    // three CONTENDER threads run the identical mutex-lock-then-clone path
    // in both modes via `Contention::round`'s shared `take_round_cell`
    // helper; the benchmark thread's own timed routine holds the
    // pre-fetched `cell` without re-locking, exactly as `contention/one_cell`'s
    // benchmark-thread routine does. Only the `get_or_try_init` call itself
    // is skipped. Read `contention/one_cell - baseline/scaffolding_only` as
    // a DIFFERENTIAL ESTIMATE, not an exact isolation of the protocol's
    // intrinsic cost: each row's timed value is a round MAKESPAN across
    // `CONTENDERS` overlapping threads, and the two rows' critical paths are
    // shaped differently (the contention round overlaps CAS/closure/publish/
    // spin across threads; the scaffolding round has none of that), so the
    // subtraction does not algebraically decompose into "the cost of just
    // the missing call" the way subtracting two sequential, non-overlapping
    // measurements would. See the module doc's opening section for the full
    // argument and what to use instead for a cross-revision regression
    // signal.
    //
    // `baseline/barrier_floor` additionally strips the mutex/clone, down to
    // just the two barrier crossings — context for how much of
    // `scaffolding_only`'s own makespan is barriers versus mutex/`Arc`, and
    // an even less exact subtrahend than `scaffolding_only` is (an earlier
    // version of this file did exactly that under the name
    // `baseline/barriers_only`, and its own doc comment overclaimed BOTH
    // the isolation itself — before its contender threads even matched the
    // contention row's source shape — and the resulting difference as pure
    // protocol cost).
    //
    // The contender threads are spawned once and live until explicitly shut
    // down after `h.run()` returns (see below) — not left to be reaped at
    // process exit.
    let mut worker_handles = Vec::with_capacity(CONTENDERS - 1);
    {
        let ctl = Arc::new(Contention {
            slot: Mutex::new(None),
            start: Barrier::new(CONTENDERS),
            done: Barrier::new(CONTENDERS),
            mode: AtomicU8::new(MODE_CONTEND),
        });
        for _ in 1..CONTENDERS {
            let c = Arc::clone(&ctl);
            worker_handles.push(std::thread::spawn(move || {
                // `NonNull<Payload>` is `!Send`, so each contender leaks its
                // OWN process-lifetime payload here rather than receiving
                // the benchmark thread's. Which one ends up published
                // depends on who wins the round, and does not matter: the
                // cell only stores the pointer, and all of them are equally
                // valid, equally leaked, and identical in shape.
                let mine = shared_payload();
                while c.round(mine) {}
            }));
        }

        // UNTIMED setup shared by both `contention/one_cell` and
        // `baseline/scaffolding_only`: publish a fresh cell to `slot` for
        // the round. Identical in both rows on purpose — only the routine
        // (and the `mode` it sets first) differs.
        let publish_round_cell = |ctl: &Arc<Contention>| -> Arc<RacyPtrCell<Payload>> {
            let cell = Arc::new(RacyPtrCell::<Payload>::new());
            *ctl.slot.lock().expect("contention slot mutex poisoned") = Some(Arc::clone(&cell));
            cell
        };

        let bench_ctl = Arc::clone(&ctl);
        h.bench_batched(
            "contention/one_cell",
            move || {
                bench_ctl.mode.store(MODE_CONTEND, Ordering::Release);
                (Arc::clone(&bench_ctl), publish_round_cell(&bench_ctl))
            },
            move |(ctl, cell)| {
                ctl.start.wait();
                black_box(cell.get_or_try_init(|| init_body(payload)));
                ctl.done.wait();
            },
        );

        let scaffold_ctl = Arc::clone(&ctl);
        h.bench_batched(
            "baseline/scaffolding_only",
            move || {
                scaffold_ctl
                    .mode
                    .store(MODE_SCAFFOLDING_ONLY, Ordering::Release);
                (Arc::clone(&scaffold_ctl), publish_round_cell(&scaffold_ctl))
            },
            |(ctl, cell)| {
                ctl.start.wait();
                // Mirrors `contention/one_cell`'s bench-thread routine
                // EXACTLY, down to not re-locking the mutex here either --
                // that row's bench thread already holds `cell` from its own
                // setup and never re-fetches it, so this one must not
                // either, or the two rows' bench-thread source shapes would
                // no longer match (see the module doc for why matching
                // SHAPE, not a claim of exact cost subtraction, is what this
                // row is actually for). The contender THREADS still run the
                // identical mutex/clone path on both rows via
                // `Contention::round`'s shared `take_round_cell` -- that is
                // where the scaffolding needs to match, and does.
                black_box(&cell);
                ctl.done.wait();
            },
        );

        let floor_ctl = Arc::clone(&ctl);
        h.bench_batched(
            "baseline/barrier_floor",
            move || {
                floor_ctl.mode.store(MODE_BARRIER_FLOOR, Ordering::Release);
                Arc::clone(&floor_ctl)
            },
            |ctl| {
                ctl.start.wait();
                ctl.done.wait();
            },
        );

        h.run();

        // Shutdown: one more `start` crossing with MODE_SHUTDOWN releases
        // every contender out of its loop (each returns `false` from
        // `round` without touching `done`), then they are joined for real —
        // no thread is left to be reaped at process exit.
        ctl.mode.store(MODE_SHUTDOWN, Ordering::Release);
        ctl.start.wait();
    }
    for handle in worker_handles {
        handle.join().expect("contender thread must not panic");
    }
}
