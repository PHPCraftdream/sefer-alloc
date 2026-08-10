//! `bench-scale-tool` fixed-iteration benches for `RacyPtrCell<T>` (task #760).
//!
//! This crate previously had zero benches of its own — it is the hottest
//! primitive in the sefer-alloc allocator's init paths (lazy CAS-published
//! pointer cell, used inside `#[global_allocator]` bootstrapping), but no
//! benchmark coverage existed to track hot-path latency.
//!
//! Run:
//! ```text
//! cargo bench -p racy-ptr-cell --bench racy_ptr_cell_bench -- --calibrate 1
//! cargo bench -p racy-ptr-cell --bench racy_ptr_cell_bench
//! ```

use std::hint::black_box;
use std::ptr::NonNull;

use bench_scale_tool::Harness;
use racy_ptr_cell::RacyPtrCell;

// Use the same Payload type as the tests - align(4) is sufficient and
// realistic for the allocator use case.
#[repr(align(4))]
struct Payload {
    #[allow(dead_code)]
    marker: u32,
}

fn leak() -> NonNull<Payload> {
    NonNull::from(Box::leak(Box::new(Payload { marker: 42 })))
}

fn main() {
    let mut h = Harness::new("racy_ptr_cell_bench", env!("CARGO_MANIFEST_DIR"));

    // ── get_or_try_init/cold ─────────────────────────────────────────────────
    // Cold path: every iteration creates a fresh UNINIT cell and runs the full
    // init sequence (CAS -> init closure -> Release publish).
    //
    // Intentionally leaks the payload on each iteration, matching the crate's
    // documented usage pattern ("leaked for the process lifetime"). Calibrates
    // to a few million iterations (~4.4M measured on this host) -- well below
    // any host-RAM-exhaustion risk even accounting for real allocator overhead
    // per small allocation, which is acceptable for a short-lived benchmark.
    h.bench_batched(
        "get_or_try_init/cold",
        RacyPtrCell::<Payload>::new,
        |cell| {
            black_box(cell.get_or_try_init(|| Some(leak())));
        },
    );

    // ── get/hot ──────────────────────────────────────────────────────────────
    // Hot path: cell is already READY; measures the single Acquire load.
    {
        let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
        // Initialize once before benchmarking.
        cell.get_or_try_init(|| Some(leak())).unwrap();
        h.bench("get/hot", move || {
            black_box(cell.get());
        });
    }

    // ── get_or_try_init/warm_already_ready ───────────────────────────────────
    // Warm path through get_or_try_init: cell is already READY, so the fast
    // path (first Acquire load check) returns early without running init.
    {
        let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
        cell.get_or_try_init(|| Some(leak())).unwrap();
        h.bench("get_or_try_init/warm_already_ready", move || {
            black_box(cell.get_or_try_init(|| Some(leak())));
        });
    }

    // ── CAS-retry/loser_spin workload ────────────────────────────────────────
    // NOT IMPLEMENTED — single-threaded harness cannot honestly model the
    // multi-threaded loser spin-wait scenario. The loser path spins with Acquire
    // loads while observing the INITIALIZING sentinel — this requires real
    // concurrent threads contending on the same cell to exercise correctly.
    //
    // No bench-internals debug hook exists to force the cell into INITIALIZING
    // state from a single thread (only dbg_is_ready() and dbg_rollback_reenterable()
    // are exposed, neither of which can simulate an ongoing init race). Any
    // artificial single-threaded simulation would measure something different
    // than the actual loser path and would be misleading.

    h.run();
}
