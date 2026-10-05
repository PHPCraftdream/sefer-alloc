//! Real terminal-sidecar fan-in latency matrix with persistent producer threads.
//! Owners run active, slow, paused, or surrender their lease before publication.
//! Per-free p50/p99/max and full burst wall time remain distinct measurements.
//! Only paused cells report time-to-reclaim: one real owner descriptor sweep,
//! with an exact retired-publication count. Exited cells report no guessed
//! reclaim time because this harness no longer owns their heap.
//! No retired ring/retry/spill counters are reported.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
#![allow(clippy::cast_possible_truncation, clippy::needless_pass_by_value)]

use std::alloc::Layout;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// A small-class size well under `SMALL_MAX`, so every block routes through
/// the ring (never the Large/A1 path). Matches `tests/remote_fanin.rs`'s and
/// `heap_fanin_production.rs`'s `BLOCK_SIZE`.
const BLOCK_SIZE: usize = 64;

/// Artificial per-op throttle for the `slow` owner state — small enough that
/// the owner still completes many drain cycles over a multi-millisecond
/// burst, large enough to sit clearly between `active` (no sleep) and
/// `paused` (owner never runs until the burst ends) on the pressure axis.
const SLOW_OWNER_SLEEP: Duration = Duration::from_micros(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerState {
    Active,
    Slow,
    Paused,
    Exited,
}

impl OwnerState {
    fn label(self) -> &'static str {
        match self {
            OwnerState::Active => "active",
            OwnerState::Slow => "slow",
            OwnerState::Paused => "paused",
            OwnerState::Exited => "exited",
        }
    }
}

/// Percentile helper: `sorted` must already be sorted ascending. `p` in
/// `0.0..=1.0`.
fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// The result of one matrix cell.
struct CellResult {
    threads: usize,
    burst: usize,
    owner: OwnerState,
    p50: Duration,
    p99: Duration,
    max: Duration,
    n_ops: usize,
    wall_total: Duration,
    time_to_reclaim: Option<Duration>,
}

impl CellResult {
    fn report(&self) {
        let ttr = self
            .time_to_reclaim
            .map(|d| format!("{:.3}ms", d.as_secs_f64() * 1e3))
            .unwrap_or_else(|| "n/a".to_string());
        eprintln!(
            "heap_fanin_persistent: T={:<3} burst={:<8} owner={:<7} n_ops={:<8} \
             p50={:>9.1}ns p99={:>9.1}ns max={:>10.1}ns wall_total={:>8.3}ms \
             time_to_reclaim={}",
            self.threads,
            self.burst,
            self.owner.label(),
            self.n_ops,
            self.p50.as_secs_f64() * 1e9,
            self.p99.as_secs_f64() * 1e9,
            self.max.as_secs_f64() * 1e9,
            self.wall_total.as_secs_f64() * 1e3,
            ttr,
        );
    }
}

/// Run one (T, burst, owner) matrix cell. Threads are spawned fresh for this
/// cell (see the module doc's "why a custom loop, not Criterion" section —
/// this function IS the "persistent, once-per-cell spawn, N-op timed burst"
/// shape; within a cell every producer thread performs its ENTIRE burst
/// slice across a single barrier release, i.e. threads are not re-spawned
/// per-op, only once per cell, which is the actual "persistent thread" claim
/// this harness makes relative to `heap_fanin_production.rs`'s per-ITERATION
/// respawn).
///
/// **Never recycle within this process (bug fix, post-review).** An earlier
/// version of this function called `HeapRegistry::recycle` on every claimed
/// heap at the end of each cell. `HeapRegistry`'s free-slot pool
/// (`Registry::free_slots`) is a LIFO Treiber stack — `recycle` pushes,
/// `claim` pops — so the very next cell's `claim()` calls were highly likely
/// to pop the EXACT SAME slot(s) just recycled by the previous cell,
/// inheriting that slot's `HeapCore` and segments WHOLE (Phase 12.5's
/// documented whole-slot-reuse design). Neither `HeapRegistry::recycle` nor
/// `HeapCore::trim_for_recycle` (the production teardown hook, wired only
/// through `AbandonGuard::drop` on real thread exit — this bench never goes
/// through that path) drains a segment's `RemoteFreeRing` or a heap's
/// `HeapOverflow` ring; both are drained lazily/opportunistically by the
/// OWNER's own `alloc()` calls, which is not guaranteed to have fully
/// emptied every ring by the time a cell's owner thread joins (the owner's
/// alloc-loop visits whichever segment `find_segment_with_free` happens to
/// scan; under `fastbin`, `drain_heap_overflow` runs only on a
/// magazine-MISS, so it is not even reliably reachable by a bounded number
/// of plain `alloc()` calls at cell-end). The result, caught by the
/// coordinator's zero-trust re-run: a later cell claiming a slot whose ring
/// was left non-empty by an earlier cell started with LESS ring headroom
/// than a truly fresh segment, so even an `active`-owner cell measured
/// later in a run degraded toward `paused`-like retry-storm numbers —
/// cross-cell state leakage, not a real difference in owner behavior.
///
/// The fix: this function (and its callers) never call `HeapRegistry::recycle`
/// except for the ONE load-bearing case the `exited` owner state itself
/// requires (see that state's construction below) — every OTHER claimed heap
/// (every producer, every owner in every other state, the `exited` state's
/// post-burst "fresh claimant") is deliberately leaked for the remainder of
/// this process's lifetime, so no cell can ever inherit another cell's
/// segments. `MAX_HEAPS = 4096` gives comfortable headroom: even the full
/// `--full-matrix` 100-cell cross product claims at most ~2,240 heaps
/// worst-case (every T+1 threads per cell, summed across all 100 cells),
/// well under the registry's capacity. This is a short-lived measurement
/// binary, not a long-running process — leaking registry slots for the
/// duration of one `cargo bench` invocation has no consequence beyond that
/// invocation's own address space, reclaimed whole by the OS on process
/// exit.
fn run_cell(threads: usize, burst: usize, owner: OwnerState) -> CellResult {
    // ---- Owner setup (claim + pre-allocate blocks the producers will free) ----
    // Ph4c: migrated to the safe `dbg_claim_lease` API. `HeapLease` is
    // `!Send`, so the lease authority now lives exactly where the ownership
    // lives — semantically MORE precise than the legacy raw-pointer handoff:
    //
    // - `active`/`slow`: the owner THREAD claims its own lease (it IS the
    //   slot's owner), pre-allocates the `burst` blocks BEFORE waiting on
    //   `start_barrier` (it joins the barrier as an extra party — all of
    //   this is outside the timed window), and hands the address list plus
    //   its slot index to the coordinator over an mpsc channel so the
    //   producer slices can be built from it. The timed loop then uses only
    //   `lease.core()` (`&mut`, same thread). The lease is DROPPED at thread
    //   end (LIVE → FREE); the coordinator re-claims the slot afterwards
    //   (untimed) for the residual sidecar-ingress drain — the LIFO reuse
    //   hint set by the Drop returns the same slot, asserted via
    //   `slot_index()`, and the drained slot is `mem::forget`-ed to keep the
    //   "never recycle within this process" discipline intact.
    // - `paused`/`exited`: no owner thread (as before); the coordinator
    //   holds the lease itself and pre-allocates through `lease.core()`.
    //   `exited` drops the lease at exactly the old `HeapRegistry::recycle`
    //   point — before any producer touches the blocks, so every free in
    //   the timed burst targets a segment whose owning slot is genuinely
    //   FREE for the whole burst. `paused` keeps the lease alive until the
    //   untimed time-to-reclaim measurement below, then forgets it (the
    //   slot stays LIVE for the process, exactly like the legacy leaked
    //   raw pointer).
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    const OWNER_YIELD_EVERY: u32 = 64;
    let owner_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (owner_addr_tx, owner_addr_rx) = mpsc::channel::<(u32, Vec<usize>)>();

    let mut owner_thread: Option<thread::JoinHandle<()>> = None;
    // Lease held by the coordinator across the burst (Paused only; `exited`
    // is dropped at the recycle point below, the other states never hold it
    // on this thread).
    let mut owner_lease: Option<sefer_alloc::registry::heap_registry::HeapLease> = None;
    // Slot index of the owner's heap (sent over the channel for active/slow,
    // read off the lease for paused) — used by the untimed post-burst drain
    // to assert the fresh re-claim got the SAME slot back.
    let owner_slot_index: u32;
    let addrs: Vec<usize>;

    // Barrier geometry must be fixed BEFORE the owner thread is spawned (it
    // is a `start_barrier` party itself for active/slow), but the producer
    // slice count is derivable purely from (threads, burst) — the exact
    // same two-stage ceiling-division arithmetic as `slices.len()` below.
    let chunk = burst.div_ceil(threads);
    let effective_threads = burst.div_ceil(chunk).max(1);
    let owner_is_thread = matches!(owner, OwnerState::Active | OwnerState::Slow);
    let start_barrier = Arc::new(Barrier::new(
        effective_threads + 1 + usize::from(owner_is_thread),
    ));
    let done_barrier = Arc::new(Barrier::new(effective_threads + 1));
    match owner {
        OwnerState::Active | OwnerState::Slow => {
            // `thread::yield_now()` every `OWNER_YIELD_EVERY` iterations
            // (`active` only — `slow` already yields the scheduler via its
            // own sleep): on a machine with fewer logical CPUs than
            // `threads + 1` (every T > num_cpus cell in this matrix — T up
            // to 64 vs this harness's own 16-core dev box), a completely
            // uncooperative owner spin loop measurably starves the producer
            // threads it is supposed to be racing against (measured:
            // without this yield, T=32/64 `active` cells inflated
            // wall_total into the hundreds-of-ms/seconds range purely from
            // scheduler contention, not genuine ring-retry cost — an
            // artifact of THIS harness's owner loop, not a production
            // signal). A bare `spin_loop()` hint is not enough here (it is
            // a CPU pause hint, not a scheduler yield); this periodic real
            // yield keeps the owner's "always draining" semantics (still
            // the tightest-draining state on the pressure axis relative to
            // `slow`/`paused`/`exited`) while letting an oversubscribed run
            // actually make forward progress within this project's
            // fast-bench-profile budget.
            let owner_done = Arc::clone(&owner_done);
            let sleep_between = matches!(owner, OwnerState::Slow);
            let start_barrier = Arc::clone(&start_barrier);
            owner_thread = Some(thread::spawn(move || {
                let _ = bootstrap::ensure();
                let mut lease =
                    HeapRegistry::dbg_claim_lease().expect("owner HeapRegistry::claim failed");
                let this_slot_index = lease.slot_index();
                // Pre-allocate the whole burst BEFORE the barrier: the timed
                // window must not contain any of this setup (same work the
                // coordinator did pre-Ph4c, just moved into the thread that
                // actually owns the slot).
                let mut owner_ptrs: Vec<*mut u8> = Vec::with_capacity(burst);
                {
                    let heap = lease.core();
                    for _ in 0..burst {
                        let p = heap.alloc(layout);
                        assert!(!p.is_null(), "owner pre-alloc returned null");
                        owner_ptrs.push(p);
                    }
                }
                let thread_addrs: Vec<usize> = owner_ptrs.iter().map(|&p| p as usize).collect();
                owner_addr_tx
                    .send((this_slot_index, thread_addrs))
                    .expect("coordinator must be receiving the owner address list");

                // Parked here until the main thread releases every party
                // (producers + this owner) simultaneously — OUTSIDE the
                // timed region.
                start_barrier.wait();

                let heap = lease.core();
                let mut batch: Vec<*mut u8> = Vec::new();
                let mut iter: u32 = 0;
                while !owner_done.load(Ordering::Relaxed) {
                    let p = heap.alloc(layout);
                    if !p.is_null() {
                        batch.push(p);
                    }
                    if sleep_between {
                        thread::sleep(SLOW_OWNER_SLEEP);
                    } else {
                        iter = iter.wrapping_add(1);
                        if iter.is_multiple_of(OWNER_YIELD_EVERY) {
                            thread::yield_now();
                        }
                    }
                    // Cap unbounded growth: an active owner that never
                    // self-frees would otherwise grow `batch` for as long as
                    // the burst runs. Free the batch periodically off the
                    // ring path (own-thread free), keeping `small_cur`'s
                    // free list from refilling faster than
                    // `find_segment_with_free` gets exercised (same
                    // reasoning as heap_fanin_production.rs::run_active).
                    if batch.len() >= 4096 {
                        for p in batch.drain(..) {
                            // SAFETY: `p` was returned by `heap.alloc(layout)`
                            // above with the same layout, is still live,
                            // freed once, own-thread via the owner's lease.
                            unsafe { heap.dealloc(p, layout) };
                        }
                    }
                }
                for p in batch {
                    // SAFETY: `p` was returned by `heap.alloc(layout)` above
                    // with the same layout, is still live, freed once,
                    // own-thread via the owner's lease.
                    unsafe { heap.dealloc(p, layout) };
                }
                // Ph4c: recycle via lease Drop (LIVE → FREE Release) — the
                // coordinator re-claims this slot (untimed, LIFO hint) for
                // the residual ingress drain below. The reclaimed-and-
                // drained slot is then forgotten, preserving the "never
                // recycle within this process" leak discipline.
                drop(lease);
            }));
            let (idx, thread_addrs) = owner_addr_rx
                .recv()
                .expect("owner thread must send its pre-allocated address list");
            owner_slot_index = idx;
            addrs = thread_addrs;
        }
        OwnerState::Paused | OwnerState::Exited => {
            let mut lease =
                HeapRegistry::dbg_claim_lease().expect("owner HeapRegistry::claim failed");
            owner_slot_index = lease.slot_index();
            let mut owner_ptrs: Vec<*mut u8> = Vec::with_capacity(burst);
            {
                let heap = lease.core();
                for _ in 0..burst {
                    let p = heap.alloc(layout);
                    assert!(!p.is_null(), "owner pre-alloc returned null");
                    owner_ptrs.push(p);
                }
            }
            addrs = owner_ptrs.iter().map(|&p| p as usize).collect();
            if owner == OwnerState::Exited {
                // Recycle NOW (lease Drop = LIVE → FREE Release), before any
                // producer touches the blocks — every free in the timed burst
                // below targets a segment whose owning slot is genuinely FREE
                // for the whole burst (see module doc). Same point in the
                // sequence as the legacy explicit `HeapRegistry::recycle`.
                drop(lease);
            } else {
                // Paused: the lease (and the slot) stays with the coordinator
                // across the burst for the untimed time-to-reclaim sweep.
                owner_lease = Some(lease);
            }
        }
    }

    // ---- Producer setup: each claims its own heap (a real remote thread —
    // never the owner's), splits the address list into disjoint slices.
    // Threads are spawned ONCE here, parked on `start_barrier`, and perform
    // their entire burst slice after release — this IS the "threads created
    // once, outside the timer" fix over heap_fanin_production.rs. ----
    let slices: Vec<Vec<usize>> = addrs.chunks(chunk).map(<[usize]>::to_vec).collect();
    // Some (T, burst) combinations produce fewer slices than the REQUESTED
    // `threads` — `chunks(chunk)` never emits empty slices, and two-stage
    // ceiling division (chunk_size = ceil(burst/threads), then slice_count =
    // ceil(burst/chunk_size)) can round down by exactly one when `burst`
    // is not an exact multiple of `chunk_size`. Concretely, at this
    // harness's own T=64/burst=1_000 matrix point: chunk_size =
    // ceil(1000/64) = 16, and ceil(1000/16) = 63, not 64 — the 63rd slice
    // absorbs the remainder (8 items) instead of a 64th thread being spawned
    // for a near-empty slice. This is expected two-stage-chunking arithmetic
    // (every one of the `burst` items is still covered exactly once, just by
    // one fewer thread than nominally requested at this specific burst/T
    // ratio), not an off-by-one bug — `report()`'s printed `T=` column
    // always reflects this ACTUAL slice count (`effective_threads`), not the
    // caller's nominal `threads` argument, specifically so a reader is never
    // shown a `T=64` label next to a run that genuinely used 63 producer
    // threads.
    debug_assert_eq!(slices.len(), effective_threads);

    let mut handles = Vec::with_capacity(effective_threads);
    for slice in slices {
        let start_barrier = Arc::clone(&start_barrier);
        let done_barrier = Arc::clone(&done_barrier);
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            // Ph4c: safe lease API. This heap is deliberately NEVER recycled
            // (see run_cell's doc comment on the "never recycle within this
            // process" fix), so the lease is explicitly forgotten below —
            // forgetting keeps the slot LIVE exactly like the old
            // dropped-on-the-floor raw pointer.
            let mut lease =
                HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");
            let remote_heap = lease.core();

            // Parked here until the main thread releases every producer
            // simultaneously — this rendezvous is OUTSIDE the timed region
            // from the main thread's point of view (the main thread starts
            // its Instant AFTER this barrier releases, not before).
            start_barrier.wait();

            let mut latencies: Vec<Duration> = Vec::with_capacity(slice.len());
            for addr in slice {
                let p = addr as *mut u8;
                let t0 = Instant::now();
                // SAFETY: `p` was returned by the owner's `alloc(layout)`
                // with the same layout, is still live, freed exactly once
                // here -- the deliberate cross-thread free path measured.
                unsafe { remote_heap.dealloc(p, layout) };
                latencies.push(t0.elapsed());
            }

            done_barrier.wait();
            // Deliberately NOT recycled — see run_cell's doc comment on the
            // "never recycle within this process" fix (the coordinator's
            // zero-trust review caught cross-cell state leakage from LIFO
            // slot reuse; this is the other half of that fix, alongside the
            // owner heap below). `mem::forget` keeps the slot LIVE exactly
            // like the old leaked raw pointer.
            core::mem::forget(lease);
            latencies
        }));
    }

    // Give every producer thread time to reach the start barrier (claim +
    // bootstrap can take a little wall-clock on first touch) — this wait is
    // itself outside the timed region; it only ensures the barrier release
    // below is the actual synchronized start rather than a race with a
    // still-initializing producer.
    thread::sleep(Duration::from_millis(1));

    // ---- TIMED SECTION: release start barrier -> producers free -> wait
    // for completion -> stop the clock. This is the ENTIRE timed window. ----
    let wall_start = Instant::now();
    start_barrier.wait();
    done_barrier.wait();
    let wall_total = wall_start.elapsed();

    owner_done.store(true, Ordering::Relaxed);
    if let Some(h) = owner_thread {
        h.join().expect("owner thread must not panic");
    }

    let mut all_latencies: Vec<Duration> = Vec::with_capacity(burst);
    for h in handles {
        let latencies = h.join().expect("producer thread must not panic");
        all_latencies.extend(latencies);
    }
    all_latencies.sort_unstable();
    if matches!(owner, OwnerState::Active | OwnerState::Slow) {
        // Ph4c: the owner thread dropped its lease on exit (LIVE → FREE), so
        // the legacy raw dereference of the leaked pointer is replaced by a
        // fresh safe re-claim. This untimed sweep completes residual
        // descriptor obligations. The lease's Drop published the LIFO reuse
        // hint for its own slot, so the re-claim deterministically returns
        // the SAME slot (asserted) — every other thread has joined, so no
        // one else can have claimed it. After the drain the lease is
        // FORGOTTEN, not dropped: the slot stays LIVE-for-the-process,
        // exactly like the legacy leaked raw pointer (the "never recycle
        // within this process" discipline of run_cell's doc comment).
        let mut lease = HeapRegistry::dbg_claim_lease().expect("post-burst owner re-claim failed");
        assert_eq!(
            lease.slot_index(),
            owner_slot_index,
            "LIFO reuse hint must return the owner's slot for the untimed drain"
        );
        let _drained = lease.core().dbg_drain_sidecar_ingress();
        core::mem::forget(lease);
    }

    let n_ops = all_latencies.len();
    let p50 = percentile(&all_latencies, 0.50);
    let p99 = percentile(&all_latencies, 0.99);
    let max = all_latencies.last().copied().unwrap_or(Duration::ZERO);

    // Only the paused cell retains this core's exclusive owner lease. Measure
    // the actual descriptor sweep, not synthetic alloc/dealloc churn. Exited
    // cells have surrendered that lease and do not report a guessed reclaim time.
    let time_to_reclaim = match owner {
        OwnerState::Paused => {
            // Ph4c: same sweep through the coordinator-held lease (`core(&mut)`
            // is the sole core-access seam; all other threads have joined).
            let mut lease = owner_lease
                .take()
                .expect("paused cell must still hold the owner lease");
            let heap = lease.core();
            let t0 = Instant::now();
            let reclaimed = heap.dbg_drain_sidecar_ingress();
            let elapsed = t0.elapsed();
            assert_eq!(reclaimed, burst, "every paused publication must be retired");
            // Keep the slot LIVE for the process (deliberately never
            // recycled — run_cell's doc comment), exactly like the legacy
            // leaked raw pointer.
            core::mem::forget(lease);
            Some(elapsed)
        }
        OwnerState::Active | OwnerState::Slow | OwnerState::Exited => None,
    };

    CellResult {
        threads: effective_threads,
        burst,
        owner,
        p50,
        p99,
        max,
        n_ops,
        wall_total,
        time_to_reclaim,
    }
}

/// Setup-isolation proof (verification step 2 of the task's mandate): run
/// the pre-burst setup phase (claim + pre-allocate, WITHOUT ever releasing
/// the start barrier — i.e. an empty/trivial "burst") across all four owner
/// states and confirm the measured setup wall-clock is flat. If setup cost
/// depended on which owner state would LATER run, the setup/timed-section
/// boundary in `run_cell` above would be leaking owner-state-dependent work
/// into the untimed region — this check is the counterfactual that would
/// catch that regression.
///
/// **This function's own `HeapRegistry::recycle` calls are safe** (unlike
/// `run_cell`'s pre-fix version — see that function's doc comment for the
/// cross-cell ring-leakage bug found in review): no thread here ever calls
/// `dealloc`, so no `RemoteFreeRing`/`HeapOverflow` entry is ever produced
/// on any heap this function claims, and recycling a heap whose rings were
/// never touched cannot leak stale ring occupancy into a later claimant.
fn verify_setup_isolation() {
    const T: usize = 8;
    const BURST: usize = 1_000;
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let mut setup_times: Vec<(OwnerState, Duration)> = Vec::new();

    for &owner in &[
        OwnerState::Active,
        OwnerState::Slow,
        OwnerState::Paused,
        OwnerState::Exited,
    ] {
        let t0 = Instant::now();

        // Ph4c: safe lease API — owner heap claimed, used and recycled on
        // this thread (lease `Drop` = the old explicit `recycle`).
        // `Option` so the conditional `exited` Drop can move the lease out
        // without tripping the conditional-move check.
        let mut owner_lease =
            Some(HeapRegistry::dbg_claim_lease().expect("owner HeapRegistry::claim failed"));
        let owner_heap = owner_lease.as_mut().expect("lease just set").core();
        let mut owner_ptrs: Vec<*mut u8> = Vec::with_capacity(BURST);
        for _ in 0..BURST {
            let p = owner_heap.alloc(layout);
            assert!(!p.is_null());
            owner_ptrs.push(p);
        }
        let addrs: Vec<usize> = owner_ptrs.iter().map(|&p| p as usize).collect();
        if owner == OwnerState::Exited {
            drop(owner_lease.take()); // recycle: LIVE → FREE (the old explicit `recycle`)
        }
        let chunk = BURST.div_ceil(T);
        let slices: Vec<Vec<usize>> = addrs.chunks(chunk).map(<[usize]>::to_vec).collect();
        let effective_threads = slices.len().max(1);
        let start_barrier = Arc::new(Barrier::new(effective_threads + 1));
        let mut handles = Vec::with_capacity(effective_threads);
        for slice in slices {
            let start_barrier = Arc::clone(&start_barrier);
            handles.push(thread::spawn(move || {
                let _ = bootstrap::ensure();
                let mut lease =
                    HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");
                let _remote_heap = lease.core();
                start_barrier.wait();
                std::hint::black_box(&slice);
                drop(lease); // recycle: LIVE → FREE (the old explicit `recycle`)
            }));
        }
        thread::sleep(Duration::from_millis(1));

        let setup_elapsed = t0.elapsed();
        setup_times.push((owner, setup_elapsed));

        // Release + join without ever touching the burst payload — this run
        // exists purely to measure setup cost, not dealloc cost.
        start_barrier.wait();
        for h in handles {
            h.join().expect("producer must not panic");
        }
        if owner != OwnerState::Exited {
            drop(owner_lease.take()); // recycle: LIVE → FREE (the old explicit `recycle`)
        }
    }

    eprintln!("heap_fanin_persistent: setup-isolation proof (T={T}, burst={BURST}):");
    for (owner, d) in &setup_times {
        eprintln!(
            "  setup_wall_time[{:<7}] = {:.3}ms",
            owner.label(),
            d.as_secs_f64() * 1e3
        );
    }

    let times_ms: Vec<f64> = setup_times
        .iter()
        .map(|(_, d)| d.as_secs_f64() * 1e3)
        .collect();
    let min_t = times_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let max_t = times_ms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // Generous tolerance: this is a wall-clock OS-scheduling-sensitive
    // measurement (thread spawn/claim under whatever else is running on this
    // machine), not a tight micro-benchmark — the property under test is
    // "does NOT scale with owner state", not "is bit-identical every run".
    // A 5x spread would indicate a genuine leak; anything tighter is normal
    // scheduler jitter.
    let ratio = if min_t > 0.0 { max_t / min_t } else { 1.0 };
    eprintln!(
        "  spread: min={min_t:.3}ms max={max_t:.3}ms ratio={ratio:.2}x \
         (expected roughly flat across owner states — setup never touches \
         owner behavior; ratio computed for eyeball confirmation, not a hard gate)"
    );
}

/// The FAST default matrix — every axis touched at least once, but with
/// every burst size capped at 1_000 (the largest size that reliably keeps
/// this project's fast-bench-profile convention, CLAUDE.md: "whole suite in
/// a few seconds to a couple of minutes"). At burst >= 100_000, a
/// production ring genuinely saturated under sustained pressure spends real
/// CPU time in `RING_PUSH_RETRY_SPINS`-bounded retry loops (up to 8,192
/// spin+CAS attempts PER overflowing push) — this is authentic allocator
/// cost, not harness overhead (see the module doc's "burst-size-axis sweep"
/// section), but it means a single burst=100_000 cell under sustained
/// pressure measured tens of SECONDS of genuine wall-clock on this
/// project's own dev hardware. That cost is real and worth measuring, but
/// not as this binary's fast default — see [`run_reduced_matrix`] (opt-in
/// via `--reduced`) for the large-burst / high-pressure-corner cells.
fn run_quick_matrix() {
    let _ = bootstrap::ensure();

    eprintln!("heap_fanin_persistent: === reference cell ===");
    run_cell(8, 1_000, OwnerState::Active).report();

    eprintln!("heap_fanin_persistent: === T-axis sweep (burst=1_000, owner=active) ===");
    for &t in &[1usize, 2, 8, 32, 64] {
        run_cell(t, 1_000, OwnerState::Active).report();
    }

    eprintln!("heap_fanin_persistent: === burst-axis sweep, capped (T=8, owner=active) ===");
    for &b in &[256usize, 400, 1_000] {
        run_cell(8, b, OwnerState::Active).report();
    }

    eprintln!("heap_fanin_persistent: === owner-state-axis sweep (T=8, burst=1_000) ===");
    for &o in &[
        OwnerState::Active,
        OwnerState::Slow,
        OwnerState::Paused,
        OwnerState::Exited,
    ] {
        run_cell(8, 1_000, o).report();
    }

    eprintln!(
        "heap_fanin_persistent: === interaction spot-check, capped (T=32, burst=1_000, \
         paused) ==="
    );
    run_cell(32, 1_000, OwnerState::Paused).report();

    eprintln!(
        "heap_fanin_persistent: (run with --reduced for the large-burst / \
         high-pressure-corner cells [100_000/1_000_000], or --full-matrix for the \
         complete 5x5x4 cross product — both are slower; see this binary's module doc)"
    );
}

/// **Repeated-cell consistency check** (added post-review — the coordinator's
/// zero-trust re-run caught a cross-cell state-leakage bug, fixed by
/// `run_cell`'s "never recycle within this process" discipline; see that
/// function's doc comment for the root cause). Runs the SAME (T=8,
/// burst=1_000, `active`) cell configuration `REPEAT_COUNT` times in a row,
/// interleaved with other cells in between each repeat (mirroring how the
/// quick/reduced matrices naturally re-visit this exact cell at three
/// different points — "reference cell", the T-axis sweep's T=8 entry, and
/// the owner-axis sweep's `active` entry) to reproduce the exact shape of
/// run that exposed the bug, and asserts the measured `p50` stays within a
/// generous tolerance across all repeats. This is the automated form of the
/// manual check the coordinator asked for: "run the SAME cell configuration
/// at least twice within one process run and confirm the numbers stay
/// consistent both times".
///
/// Tolerance: `p50` must not exceed `CONSISTENCY_MAX_P50_NS` (a generous
/// absolute ceiling, not a tight statistical bound — this box's own
/// scheduler jitter under CPU oversubscription is real and expected to vary
/// run to run; the property this check exists to catch is the BUG's
/// signature specifically — a `p50` climbing into the MILLISECOND range,
/// three-plus orders of magnitude above this cell's genuine sub-microsecond
/// active-owner cost — not sub-2x jitter).
const REPEAT_COUNT: usize = 3;
const CONSISTENCY_MAX_P50_NS: f64 = 100_000.0; // 100us — generous vs. the ~500ns-2us genuine cost, but 100x+ below the ~8-15ms the bug produced.

fn verify_repeated_cell_consistency() {
    let _ = bootstrap::ensure();

    eprintln!(
        "heap_fanin_persistent: repeated-cell consistency check (T=8, burst=1_000, \
         active, {REPEAT_COUNT} repeats interleaved with unrelated cells):"
    );

    let mut p50s_ns: Vec<f64> = Vec::with_capacity(REPEAT_COUNT);
    for i in 0..REPEAT_COUNT {
        let cell = run_cell(8, 1_000, OwnerState::Active);
        let p50_ns = cell.p50.as_secs_f64() * 1e9;
        eprintln!(
            "  repeat[{i}]: p50={:.1}ns p99={:.1}ns",
            p50_ns,
            cell.p99.as_secs_f64() * 1e9,
        );
        p50s_ns.push(p50_ns);

        // Interleave an UNRELATED cell between repeats — this is exactly
        // the shape ("reference cell" -> T-axis sweep -> owner-axis sweep,
        // each separated by several other cells) that exposed the bug; a
        // fix that only works when the same cell runs back-to-back with
        // nothing in between would not actually prove the leak is gone.
        if i + 1 < REPEAT_COUNT {
            let _ = run_cell(2, 256, OwnerState::Paused);
        }
    }

    let max_p50 = p50s_ns.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_p50 = p50s_ns.iter().copied().fold(f64::INFINITY, f64::min);
    eprintln!(
        "  spread across {REPEAT_COUNT} repeats: min={min_p50:.1}ns max={max_p50:.1}ns \
         ratio={:.2}x",
        if min_p50 > 0.0 {
            max_p50 / min_p50
        } else {
            1.0
        }
    );

    assert!(
        max_p50 < CONSISTENCY_MAX_P50_NS,
        "REGRESSION: repeated-cell consistency check failed — p50 reached \
         {max_p50:.1}ns for the (T=8, burst=1_000, active) cell, exceeding the \
         {CONSISTENCY_MAX_P50_NS:.0}ns ceiling. This is the exact signature of the \
         cross-cell ring-state-leakage bug the coordinator's zero-trust review found \
         (a later occurrence of the SAME active-owner cell degrading toward \
         paused-like millisecond latencies) — see run_cell's doc comment for the root \
         cause (LIFO heap-slot reuse carrying an undrained RemoteFreeRing/HeapOverflow \
         forward into a later cell) and its fix (never recycle heaps within this \
         process). If this assertion fires, that fix has regressed."
    );
    eprintln!("  PASS: p50 stayed below {CONSISTENCY_MAX_P50_NS:.0}ns across all repeats.");
}

/// The fuller representative reduced-matrix run described in the module
/// doc's "Matrix actually run" section: every axis value touched at least
/// once INCLUDING the large burst sizes (100_000 / 1_000_000) and the
/// high-pressure interaction corners. Opt-in via `--reduced` — this is
/// where the multi-second-to-tens-of-seconds cells live (genuine
/// `RING_PUSH_RETRY_SPINS` retry cost under sustained overflow, not harness
/// overhead), so it is not this binary's fast default.
fn run_reduced_matrix() {
    let _ = bootstrap::ensure();

    eprintln!("heap_fanin_persistent: === reference cell ===");
    run_cell(8, 1_000, OwnerState::Active).report();

    eprintln!("heap_fanin_persistent: === T-axis sweep (burst=1_000, owner=active) ===");
    for &t in &[1usize, 2, 8, 32, 64] {
        run_cell(t, 1_000, OwnerState::Active).report();
    }

    eprintln!("heap_fanin_persistent: === burst-axis sweep (T=8, owner=active) ===");
    for &b in &[256usize, 400, 1_000, 100_000, 1_000_000] {
        run_cell(8, b, OwnerState::Active).report();
    }

    eprintln!("heap_fanin_persistent: === owner-state-axis sweep (T=8, burst=1_000) ===");
    for &o in &[
        OwnerState::Active,
        OwnerState::Slow,
        OwnerState::Paused,
        OwnerState::Exited,
    ] {
        run_cell(8, 1_000, o).report();
    }

    eprintln!("heap_fanin_persistent: === interaction spot-checks (high-pressure corners) ===");
    run_cell(32, 100_000, OwnerState::Paused).report();
    run_cell(64, 100_000, OwnerState::Exited).report();
    run_cell(2, 256, OwnerState::Slow).report();
}

/// Documents (but does not run by default) the full 5x5x4 = 100-cell cross
/// product. Opt in with `--full-matrix` on the command line. NOT part of the
/// default run: a back-of-envelope estimate from the reduced matrix's own
/// measured per-cell cost (dominated by the `1_000_000`-burst cells' setup
/// pre-allocation, which alone measured on the order of several hundred
/// milliseconds to a few seconds per cell at T=8 in this harness's own
/// reduced-matrix run) puts the full matrix — which would repeat that
/// largest burst size across every T x owner-state combination instead of
/// once — at a wall-clock cost well outside this project's "couple of
/// minutes" fast-bench-profile convention (CLAUDE.md). Anyone who needs the
/// full cross product for a deeper investigation can call this function
/// directly (or extend the CLI below); it is intentionally not the default
/// so `cargo bench --bench heap_fanin_persistent` stays fast.
fn run_full_matrix() {
    let _ = bootstrap::ensure();
    const THREADS: &[usize] = &[1, 2, 8, 32, 64];
    const BURSTS: &[usize] = &[256, 400, 1_000, 100_000, 1_000_000];
    const OWNERS: &[OwnerState] = &[
        OwnerState::Active,
        OwnerState::Slow,
        OwnerState::Paused,
        OwnerState::Exited,
    ];
    eprintln!(
        "heap_fanin_persistent: === FULL {}x{}x{} = {}-cell matrix (this will take a while) ===",
        THREADS.len(),
        BURSTS.len(),
        OWNERS.len(),
        THREADS.len() * BURSTS.len() * OWNERS.len()
    );
    for &t in THREADS {
        for &b in BURSTS {
            for &o in OWNERS {
                run_cell(t, b, o).report();
            }
        }
    }
}

/// Number of repeated samples for the headline cross-check (see module doc).
/// Small — this is a spot-check against `heap_fanin_production.rs`'s own
/// `sample_size(10)`, not a new statistically-rigorous benchmark.
const HEADLINE_REPEATS: usize = 10;

/// Headline wall-clock cross-check: repeats the (T=8, burst=400) cell
/// `HEADLINE_REPEATS` times for both `active` and `paused` owner states
/// (the two owner-behavior endpoints `heap_fanin_production.rs` itself
/// sweeps, at the same T=8/`N=400` point that bench's own module doc
/// settled on for its fast-profile budget), and reports mean/stddev/min/max
/// of `wall_total` — a repeated-sample spread comparable by eye against that
/// bench's own criterion summary at `producers=8`. See module doc for why
/// this does not pull in `criterion` itself.
fn run_headline_repeats() {
    for &owner in &[OwnerState::Active, OwnerState::Paused] {
        let mut totals: Vec<f64> = Vec::with_capacity(HEADLINE_REPEATS);
        for _ in 0..HEADLINE_REPEATS {
            let cell = run_cell(8, 400, owner);
            totals.push(cell.wall_total.as_secs_f64() * 1e3);
        }
        let mean = totals.iter().sum::<f64>() / totals.len() as f64;
        let variance = totals.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / totals.len() as f64;
        let stddev = variance.sqrt();
        let min = totals.iter().copied().fold(f64::INFINITY, f64::min);
        let max = totals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        eprintln!(
            "heap_fanin_persistent: headline T=8 burst=400 owner={:<7} \
             (n={HEADLINE_REPEATS}) wall_total: mean={mean:.3}ms stddev={stddev:.3}ms \
             min={min:.3}ms max={max:.3}ms",
            owner.label(),
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let full_matrix = args.iter().any(|a| a == "--full-matrix");
    let reduced = args.iter().any(|a| a == "--reduced");
    let headline = args.iter().any(|a| a == "--headline")
        || std::env::var("SEFER_FANIN_PERSISTENT_HEADLINE").is_ok();

    verify_setup_isolation();
    verify_repeated_cell_consistency();

    if full_matrix {
        run_full_matrix();
    } else if reduced {
        run_reduced_matrix();
    } else {
        run_quick_matrix();
    }

    if headline {
        run_headline_repeats();
    }
}
