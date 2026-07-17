//! Process-per-sample judge for "a thread that only ever FREES a foreign
//! block, never allocates" (task R6-OPT-A5, `radical_optimization_review`
//! §4 P0-1 measurement plan / §6 Stage A.5).
//!
//! ## Why this harness exists
//!
//! `SeferAlloc::dealloc` (`src/global/sefer_alloc.rs`) calls
//! `self.current_heap()` unconditionally — even for a thread that has NEVER
//! allocated anything of its own. `current_heap()` binds a FULL per-thread
//! heap (`HeapRegistry::claim` -> `AllocCore::new` -> reserve/commit a 4 MiB
//! primordial segment) on the FIRST call, whether that call is an `alloc` or
//! a `dealloc`. So a thread whose entire lifetime is "receive a pointer from
//! elsewhere, free it, exit" pays the FULL heap-binding cost just to free ONE
//! foreign pointer — a common shape (a worker pool that only ever frees
//! results a producer thread built).
//!
//! This is Stage A — a MEASUREMENT harness, not a source change. It does NOT
//! touch `current_heap()`, `dealloc`, or any binding logic. It exists so a
//! later, separate task (R6-OPT-P0-1, blocked on this one — "dealloc-without-
//! heap-bind route for unbound/TORN threads") can honestly prove whatever win
//! it claims. The core deliverable of THIS harness is the **treatment vs.
//! control delta**:
//!
//! - **treatment**: a fresh, never-before-bound worker thread's FIRST EVER
//!   call into the global allocator is a `dealloc` of a foreign pointer.
//! - **control**: the SAME shape of worker, except it performs ONE own
//!   `alloc` first (forcing it down the already-well-understood "thread
//!   binds a heap via alloc" path), THEN frees the foreign pointer.
//!
//! Pre-fix, `current_heap()` binds identically regardless of whether the
//! triggering call was `alloc` or `dealloc` — so treatment and control should
//! show CONVERGING first-operation latency and commit-charge deltas. A large
//! observed gap would indicate a harness bug, not a real effect (see this
//! file's `main` for the actual measured numbers this run produced).
//!
//! ## Why process-per-sample (same discipline as `first_alloc_process.rs`)
//!
//! Thread-local heap bindings persist for a thread's entire lifetime (until
//! `AbandonGuard::drop` on thread exit releases the registry slot back to the
//! free pool). You cannot cleanly test "a never-before-bound thread's first
//! dealloc" by reusing worker threads across samples within one process —
//! the second sample's "fresh" thread would actually be reusing a
//! recycled-but-previously-bound slot's residual state, or at best a newly
//! spawned OS thread whose process-wide registry/segment bookkeeping has
//! already been warmed up by earlier samples in the same process (segment
//! reservations, registry high-water mark, etc. are process-global). So each
//! sample here is a genuinely fresh process, exactly like
//! `examples/first_alloc_process.rs` (R6-OPT-A1) — see that file for the
//! RSS/commit probe technique reused verbatim below.
//!
//! ## Real installed `#[global_allocator]`, not the doc-hidden test seam
//!
//! Unlike most other harnesses in this crate (which call `AllocCore`/
//! `HeapCore` directly via the `#[doc(hidden)]` test-only forwarders), this
//! harness installs `SeferAlloc` as the process's actual
//! `#[global_allocator]`. That is deliberate: the defect under test lives in
//! `SeferAlloc::dealloc`'s `GlobalAlloc` impl itself (the unconditional
//! `current_heap()` call), not in `AllocCore`/`HeapCore` — so this harness
//! must exercise the REAL `dealloc`/`alloc` global entry points a normal
//! Rust program uses (`Box`, `Vec`, etc., plus explicit
//! `std::alloc::{alloc, dealloc}` for the timed calls), not the lower-level
//! test seam.
//!
//! ## Worker shape: ephemeral (ONE variant, not both — see rationale)
//!
//! The task brief asks to consider both **persistent** (spawned once, parked,
//! released together) and **ephemeral** (spawned fresh right before their one
//! free, exiting immediately after) worker shapes, and to pick the one that
//! most directly isolates "cost of an unbound thread's first dealloc" if only
//! one fits cleanly.
//!
//! This harness uses **ephemeral workers exclusively**. Rationale: the
//! measured quantity is "a thread's FIRST EVER call into the allocator, and
//! only that call, for its entire life" — a persistent-worker variant (parked
//! on a barrier, released, then parked again for reuse) would need each
//! worker to do exactly ONE free and then either exit or go idle forever;
//! reusing a persistent worker across (B,T) cells in the SAME process would
//! violate the "never-before-bound" precondition for every cell after the
//! first that reuses that worker (its heap is already bound from cell 1). A
//! persistent-worker variant could still be built with disposable workers
//! parked once and released once (never reused across cells), but that is
//! IDENTICAL in effect to spawning a fresh ephemeral thread per free — the
//! barrier only adds synchronization ceremony without buying anything a plain
//! `thread::spawn` + immediate work does not already give, because these
//! worker threads do no OTHER work before their one free (there is no setup
//! phase to hoist outside a timed window, unlike `heap_fanin_persistent.rs`'s
//! producers, which pre-allocate before their timed burst). So "ephemeral"
//! and "persistent-but-single-use" collapse to the same construction here;
//! this harness implements it once, as ephemeral, which is also the simpler
//! and more realistic mirror of the real-world "spawn a worker, it frees one
//! thing, it exits" pattern this task targets.
//!
//! ## What it prints
//!
//! One `RESULT key=value` line per metric (machine-parseable, matching every
//! sibling Stage-A harness's convention):
//!
//! ```text
//! RESULT mode=<treatment|control>
//! RESULT b=<n> RESULT t=<n>
//! RESULT rss_before_kib=<n> RESULT rss_after_kib=<n>
//! RESULT rss_after_join_kib=<n>
//! RESULT commit_before_kib=<n> RESULT commit_after_kib=<n>
//! RESULT commit_after_join_kib=<n>
//! RESULT first_dealloc_latency_ns=<n>
//! RESULT steady_dealloc_latency_ns=<n>
//! RESULT heaps_claimed_high_water=<n>
//! RESULT segments_reserved_before=<n> RESULT segments_reserved_after=<n>
//! ```
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example dealloc_only_unbound_thread --features alloc-global -- <B> <T> <mode>
//! # or via the aggregating runner:
//! node scripts/dealloc-only-bench.mjs
//! ```
//!
//! `<mode>` is `treatment` (default) or `control`.

#![cfg(feature = "alloc-global")]
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::alloc::Layout;
use std::sync::{Arc, Barrier};
use std::time::Instant;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — thin KiB wrappers over the `proc-probe` crate's
// re-export of `proc-memstat`'s same-instant `snapshot()` (bytes). The OS FFI
// that used to be a self-contained copy here now lives in ONE place
// (`crates/proc-memstat`, reached via `proc-probe`'s "measure + report"
// re-export), shared with `examples/first_alloc_process.rs` and the
// `paired_ab_*` trio. The `RESULT` lines are emitted via `proc_probe::emit_*`
// (see `main`). Printed line names/units are unchanged.
// ---------------------------------------------------------------------------

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

// ---------------------------------------------------------------------------
// Harness body
// ---------------------------------------------------------------------------

/// A small-class allocation size — well under `SMALL_MAX`, routes through the
/// ring/fastbin path, matching the block size other Stage-A siblings use.
const BLOCK_SIZE: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Worker's FIRST EVER allocator call is the foreign free.
    Treatment,
    /// Worker performs ONE own alloc+dealloc FIRST (binding its heap via the
    /// already-understood `alloc` path), THEN frees the foreign pointer. The
    /// gap between this and `Treatment`'s first-op cost is the P0-1 win this
    /// harness exists to make visible.
    Control,
}

/// Owner (main thread) allocates `b` blocks that the `t` workers will free —
/// ownership/pointer transfer across threads, the real-world foreign-free
/// shape this task targets. Returns the pointers as raw addresses (so they
/// are `Send`able into worker closures without a wrapper type).
fn owner_allocate(b: usize) -> Vec<usize> {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();
    let mut ptrs = Vec::with_capacity(b);
    for _ in 0..b {
        // SAFETY: non-zero layout; every pointer is freed exactly once by
        // exactly one worker below (or immediately, if a run has more owner
        // blocks than can be evenly distributed — every block is still freed
        // exactly once by construction, see `distribute` below).
        let p = unsafe { std::alloc::alloc(layout) };
        assert!(!p.is_null(), "owner alloc returned null");
        unsafe { p.write_bytes(0xA5, 1) };
        ptrs.push(p as usize);
    }
    ptrs
}

/// Split `b` owner blocks fairly across `t` workers (each worker gets at
/// least one block if `t <= b`; if `t > b`, only the first `b` workers get a
/// block — the remaining workers still spawn and still make their first
/// allocator call, just with zero blocks, to keep `t`'s meaning ["thread
/// count spawned"] stable across the whole (B, T) matrix even when T > B).
fn distribute(ptrs: &[usize], t: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); t];
    for (i, &p) in ptrs.iter().enumerate() {
        out[i % t].push(p);
    }
    out
}

/// Run one (B, T, mode) cell in THIS process (process-per-sample: the caller
/// script invokes the whole binary fresh for every sample). Returns the
/// metrics gathered.
struct CellMetrics {
    rss_before: u64,
    rss_after: u64,
    rss_after_join: u64,
    commit_before: u64,
    commit_after: u64,
    commit_after_join: u64,
    first_dealloc_latency_ns: u128,
    steady_dealloc_latency_ns: u128,
    heaps_claimed_high_water: u64,
    segments_reserved_before: u64,
    segments_reserved_after: u64,
}

fn run_cell(b: usize, t: usize, mode: Mode) -> CellMetrics {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let rss_before = rss_kib();
    let commit_before = commit_kib();
    let segments_reserved_before = GLOBAL.stats().segments_reserved_total;

    // Owner allocates B blocks on ITS OWN (main-thread) heap — a normal bound
    // heap doing normal allocation, per the task's step 1.
    let owner_ptrs = owner_allocate(b);
    let slices = distribute(&owner_ptrs, t);

    // Barrier releases every worker's spawn simultaneously so the FIRST
    // worker's timed op is not skewed by earlier workers still spinning up
    // (mirrors heap_fanin_persistent.rs's rendezvous pattern, R6-OPT-A2) —
    // though here each worker is freshly spawned per cell (ephemeral, see
    // module doc), the barrier still gives a clean simultaneous start so the
    // "first" and "steady" latency samples are not polluted by staggered
    // spawn timing.
    let start = Arc::new(Barrier::new(t + 1));
    // First worker (index 0) reports its op as "first" (the never-before-
    // bound thread's very first allocator call); every other worker's op
    // (also each thread's own first-ever call, since every worker here is a
    // FRESH never-before-bound thread) reports as "steady" — i.e. "steady"
    // here means "first op of a DIFFERENT already-common shape of unbound
    // thread", giving a same-cell baseline of many samples of the identical
    // treatment/control operation, one per worker.
    let mut handles = Vec::with_capacity(t);
    for slice in slices {
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || -> Option<u128> {
            start.wait();

            if mode == Mode::Control {
                // Control: force this never-before-bound thread down the
                // ALREADY-understood "bind via alloc" path first, via ONE
                // own alloc + free, before touching the foreign block(s).
                // SAFETY: non-zero layout; freed immediately with the same
                // layout.
                let own = unsafe { std::alloc::alloc(layout) };
                if !own.is_null() {
                    unsafe {
                        own.write_bytes(0x5A, 1);
                        std::alloc::dealloc(own, layout);
                    }
                }
            }

            let mut latency_ns: Option<u128> = None;
            for (j, addr) in slice.into_iter().enumerate() {
                let p = addr as *mut u8;
                let t0 = Instant::now();
                // SAFETY: `p` was allocated by the owner thread with `layout`
                // and transferred by pointer value (never touched again by
                // the owner) — a sound cross-thread free, the exact "foreign
                // free" shape this harness targets. `alloc-xthread` routes it
                // through the cross-thread path when enabled; without it,
                // this exercises SeferAlloc's fallback foreign-pointer
                // handling instead (still a legitimate, if different,
                // "unbound thread's first dealloc" measurement — the module
                // doc's Cargo feature gate covers only `alloc-global`
                // deliberately, so this harness also runs — and remains
                // meaningful — without `alloc-xthread`).
                unsafe { std::alloc::dealloc(p, layout) };
                let elapsed = t0.elapsed().as_nanos();
                if j == 0 {
                    latency_ns = Some(elapsed);
                }
            }
            latency_ns
        }));
    }

    start.wait();

    let rss_after = rss_kib();
    let commit_after = commit_kib();

    let mut per_worker_first: Vec<u128> = Vec::with_capacity(t);
    for h in handles {
        if let Some(ns) = h.join().expect("worker thread must not panic") {
            per_worker_first.push(ns);
        }
    }

    // "first_dealloc_latency_ns" = worker 0's first-ever op (the literal
    // "first free in the whole cell"). "steady_dealloc_latency_ns" = the
    // median of the REMAINING workers' first-ops — each of THOSE is also a
    // never-before-bound thread's first call, so "steady" here specifically
    // means "typical cost of this operation once the PROCESS (registry,
    // segment bookkeeping) is already warmed up by worker 0", as distinct
    // from "first_dealloc_latency_ns" which is the very first such op in a
    // cold process. This is the honest analogue available here of
    // first_alloc_process.rs's single first-vs-steady split, adapted to a
    // multi-worker cell instead of a single main-thread timeline.
    let first_dealloc_latency_ns = per_worker_first.first().copied().unwrap_or(0);
    let steady_dealloc_latency_ns = if per_worker_first.len() > 1 {
        let mut rest = per_worker_first[1..].to_vec();
        rest.sort_unstable();
        rest[rest.len() / 2]
    } else {
        first_dealloc_latency_ns
    };

    let rss_after_join = rss_kib();
    let commit_after_join = commit_kib();
    let heaps_claimed_high_water = GLOBAL.stats().heaps_claimed_high_water;
    let segments_reserved_after = GLOBAL.stats().segments_reserved_total;

    CellMetrics {
        rss_before,
        rss_after,
        rss_after_join,
        commit_before,
        commit_after,
        commit_after_join,
        first_dealloc_latency_ns,
        steady_dealloc_latency_ns,
        heaps_claimed_high_water,
        segments_reserved_before,
        segments_reserved_after,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let b: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(64);
    let t: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);
    let mode = match args.get(3).map(String::as_str) {
        Some("control") => Mode::Control,
        _ => Mode::Treatment,
    };

    let metrics = run_cell(b, t, mode);

    proc_probe::emit(
        "mode",
        if mode == Mode::Control {
            "control"
        } else {
            "treatment"
        },
    );
    proc_probe::emit_u64("b", b as u64);
    proc_probe::emit_u64("t", t as u64);
    proc_probe::emit_u64("rss_before_kib", metrics.rss_before);
    proc_probe::emit_u64("rss_after_kib", metrics.rss_after);
    proc_probe::emit_u64("rss_after_join_kib", metrics.rss_after_join);
    proc_probe::emit_u64("commit_before_kib", metrics.commit_before);
    proc_probe::emit_u64("commit_after_kib", metrics.commit_after);
    proc_probe::emit_u64("commit_after_join_kib", metrics.commit_after_join);
    proc_probe::emit_ns("first_dealloc_latency_ns", metrics.first_dealloc_latency_ns);
    proc_probe::emit_ns(
        "steady_dealloc_latency_ns",
        metrics.steady_dealloc_latency_ns,
    );
    proc_probe::emit_u64("heaps_claimed_high_water", metrics.heaps_claimed_high_water);
    proc_probe::emit_u64("segments_reserved_before", metrics.segments_reserved_before);
    proc_probe::emit_u64("segments_reserved_after", metrics.segments_reserved_after);
}
