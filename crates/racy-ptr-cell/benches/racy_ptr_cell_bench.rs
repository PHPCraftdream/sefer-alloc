//! `bench-scale-tool` fixed-iteration benches for `RacyPtrCell<T>`.
//!
//! This crate is the hottest primitive in the sefer-alloc allocator's init
//! paths (lazy CAS-published pointer cell, used inside `#[global_allocator]`
//! bootstrapping), so its hot-path latency is worth tracking.
//!
//! **What these numbers are, and are not.** Every row measures the CELL
//! PROTOCOL — CAS, `Release` publish, `Acquire` load, loser spin — and
//! nothing else. In particular no row allocates inside its timed region:
//! the payload every `init` closure publishes is leaked ONCE, before any
//! measurement, and the same `NonNull` is republished into each fresh cell
//! (legal precisely because the cell does not own its pointee — it only
//! stores and hands back the pointer). An earlier version of this file
//! called `Box::new` inside the timed closure, which made the "cold" number
//! mostly a measurement of the system allocator plus monotonic heap growth,
//! not of this crate.
//!
//! The `baseline/*` rows measure the harness scaffolding WITHOUT the cell,
//! so the cell's own cost is the difference. Read them before quoting any
//! absolute figure.
//!
//! Run:
//! ```text
//! cargo bench -p racy-ptr-cell --bench racy_ptr_cell_bench -- --calibrate 1
//! cargo bench -p racy-ptr-cell --bench racy_ptr_cell_bench
//! ```

use std::hint::black_box;
use std::ptr::NonNull;
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
/// every contention row and its baseline stay in lockstep.
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

/// One round of the contention workload: a fresh cell is placed in `slot`,
/// all `CONTENDERS` threads meet at `start`, every one of them calls
/// `get_or_try_init` on it (exactly one wins and runs `init`; the rest take
/// the loser spin), and they meet again at `done`.
struct Contention {
    slot: Mutex<Option<Arc<RacyPtrCell<Payload>>>>,
    start: Barrier,
    done: Barrier,
}

impl Contention {
    /// The per-round work a contender performs: meet at `start`, race for
    /// the round's cell, meet at `done`.
    fn round(&self, payload: NonNull<Payload>) {
        self.start.wait();
        let cell = self
            .slot
            .lock()
            .expect("contention slot mutex poisoned")
            .clone()
            .expect("the benchmark thread publishes a cell before each round");
        black_box(cell.get_or_try_init(|| init_body(payload)));
        self.done.wait();
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

    // ── contention/one_cell and its baseline ────────────────────────────────
    // The crate's most expensive documented scenario — the loser spin-wait —
    // and, until now, the only one with no baseline at all.
    //
    // Honest description of what the number contains: one timed round is a
    // full `start` barrier + every contender's `get_or_try_init` on the same
    // fresh cell + a `done` barrier. Two barrier crossings are therefore
    // INSIDE the measurement; `baseline/barriers_only` runs the identical
    // barrier pair with no cell at all, so the protocol's own contended cost
    // is the DIFFERENCE between the two rows, not the contention row's
    // absolute value. Do not quote the absolute number as "the cost of a
    // contended init".
    //
    // The contender threads are spawned once, live for the whole run, and
    // are never joined. Once the harness moves past the contention rows they
    // simply block forever on a `start` barrier that will never again reach
    // its participant count, and the process exit reaps them. That is
    // deliberate: the harness offers no post-run hook to join them, and a
    // shutdown flag would be unreachable for exactly the same reason the
    // barrier is — the benchmark thread has already stopped participating.
    {
        let ctl = Arc::new(Contention {
            slot: Mutex::new(None),
            start: Barrier::new(CONTENDERS),
            done: Barrier::new(CONTENDERS),
        });
        for _ in 1..CONTENDERS {
            let c = Arc::clone(&ctl);
            std::thread::spawn(move || {
                // `NonNull<Payload>` is `!Send`, so each contender leaks its
                // OWN process-lifetime payload here rather than receiving
                // the benchmark thread's. Which one ends up published
                // depends on who wins the round, and does not matter: the
                // cell only stores the pointer, and all of them are equally
                // valid, equally leaked, and identical in shape.
                let mine = shared_payload();
                loop {
                    c.round(mine);
                }
            });
        }

        let bench_ctl = Arc::clone(&ctl);
        h.bench_batched(
            "contention/one_cell",
            move || {
                // UNTIMED: publish the fresh cell the whole cohort will race
                // on this round.
                let cell = Arc::new(RacyPtrCell::<Payload>::new());
                *bench_ctl
                    .slot
                    .lock()
                    .expect("contention slot mutex poisoned") = Some(Arc::clone(&cell));
                (Arc::clone(&bench_ctl), cell)
            },
            move |(ctl, cell)| {
                ctl.start.wait();
                black_box(cell.get_or_try_init(|| init_body(payload)));
                ctl.done.wait();
            },
        );

        // Baseline: the same two barrier crossings, no cell.
        let base_ctl = Arc::clone(&ctl);
        h.bench_batched(
            "baseline/barriers_only",
            move || Arc::clone(&base_ctl),
            |ctl| {
                ctl.start.wait();
                ctl.done.wait();
            },
        );
    }

    h.run();
}
