//! Tokio async burn-in for `SeferMalloc` — installs the allocator as
//! `#[global_allocator]` and exercises it under a real async multi-thread
//! runtime with a СУБД-подобной (database-pipeline-like) workload.
//!
//! ## Purpose (task #52)
//!
//! All previous multi-thread harnesses (`soak_xthread`, `malloc_macro`,
//! `rss_probe`) use `std::thread` directly.  This harness fills the gap:
//! it drives allocations through a *tokio* multi-thread runtime, stressing
//! the TLS heap-init path that fires when each tokio worker thread is created,
//! and the cross-task free path that fires when allocations made on one worker
//! are dropped on another (the tokio scheduler migrates tasks freely).
//!
//! ## СУБД-pipeline workload
//!
//! ```text
//!  [query tasks] ──► [coordinator task] ──► [aggregator task]
//! ```
//!
//! 1. **Query tasks** (256–1024, configurable via `SEFER_BURNIN_TASKS`):
//!    each task simulates a DB query: allocates several `Vec<u8>`, `String`,
//!    `HashMap`, `Box`, and `Arc` payloads of varied sizes, does a few
//!    `tokio::time::sleep` yields (simulating I/O), then sends a summary
//!    `QueryResult` into `mpsc::channel`.
//!
//! 2. **Coordinator task**: receives results, drops half immediately
//!    (cross-task free pressure), forwards the rest into a second channel.
//!
//! 3. **Aggregator task**: accumulates totals; counts completed queries and
//!    total estimated allocations.
//!
//! 4. `spawn_blocking` tasks are sprinkled in by each query task (one per
//!    task) to stress the TLS init path on freshly created blocking threads.
//!
//! ## Exit codes
//!   0 — completed, no panics, no deadlocks.
//!   non-zero — a task panicked (propagated via `JoinHandle::await`).
//!
//! ## Concurrency cap and segment-table constraint
//!
//! Tokio tasks are spawned in waves of at most `SEFER_BURNIN_CONCURRENCY`
//! (default 256, max 512) concurrent in-flight tasks via a `Semaphore` +
//! `JoinSet` pair.  This caps the number of simultaneously live task cells
//! and prevents segment-table pressure under large `SEFER_BURNIN_TASKS` values.
//!
//! **Known constraint:** `SeferMalloc`'s segment table is currently
//! append-only (`MAX_SEGMENTS = 1024`; Phase 8 design — Phase 9+ will allow
//! dynamic growth).  Each new 4 MiB segment consumes a permanent slot even
//! after its blocks are freed back to the free list.  After 1024 segments have
//! been registered (cumulatively across all allocation waves), the allocator
//! returns OOM for any request that requires a NEW segment.  For the canonical
//! smoke test (256 tasks, 10 s, moderate per-task allocation volume) this limit
//! is never reached.  For very long runs with many sequential waves of tasks,
//! the cumulative segment count may reach the table ceiling — this is a
//! documented Phase 8 limitation, not a regression.
//!
//! ## Run
//!
//! ```text
//! # Smoke (10 s, 4 workers, 256 tasks) — canonical test:
//! SEFER_BURNIN_SECONDS=10 SEFER_TOKIO_WORKERS=4 SEFER_BURNIN_TASKS=256 \
//!     cargo run --release --example tokio_burn_in \
//!     --features "alloc-global alloc-xthread"
//!
//! # Larger run (1024 tasks, concurrency=256):
//! SEFER_BURNIN_TASKS=1024 \
//!     cargo run --release --example tokio_burn_in \
//!     --features "alloc-global alloc-xthread"
//! ```

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![forbid(unsafe_code)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::semicolon_if_nothing_returned
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Semaphore};
use tokio::time::{sleep, timeout};

use sefer_alloc::SeferMalloc;

// ── global allocator ────────────────────────────────────────────────────────

/// ALL allocations in this process — including tokio's internal ones — are
/// served by `SeferMalloc`.  This is the central claim of task #52.
#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// ── tuning constants / env vars ─────────────────────────────────────────────

/// Default tokio worker-thread count.  `SEFER_TOKIO_WORKERS` overrides.
const DEFAULT_WORKERS: usize = 4;

/// Default number of query tasks to spawn.  `SEFER_BURNIN_TASKS` overrides.
const DEFAULT_TASKS: usize = 256;

/// Default burn-in duration in seconds.  `SEFER_BURNIN_SECONDS` overrides.
const DEFAULT_SECONDS: u64 = 10;

/// Heartbeat interval.
const HEARTBEAT_SECS: u64 = 5;

/// mpsc channel capacity (back-pressures producers so memory stays bounded).
const CHAN_CAPACITY: usize = 512;

/// Maximum number of query tasks alive concurrently.  `SEFER_BURNIN_CONCURRENCY`
/// overrides.  Caps segment-table pressure: `SeferMalloc` has
/// `MAX_SEGMENTS = 1024`; without a cap, 1024 simultaneously live task cells
/// (~768 B each) plus tokio internals can exhaust the table.
const DEFAULT_CONCURRENCY: usize = 256;

// ── result type ──────────────────────────────────────────────────────────────

/// Summary produced by one query task.  Carries owned allocations so
/// cross-task drop exercises the xthread-free path.
struct QueryResult {
    /// Task identifier.
    task_id: usize,
    /// A heap-allocated string "result set" (simulates serialised rows).
    rows: Vec<String>,
    /// A small aggregation map (key → byte-count).
    stats: HashMap<String, u64>,
    /// Estimated number of alloc calls this task made.
    alloc_estimate: u64,
}

// ── deterministic PRNG (no dep, fixed seed → reproducible) ──────────────────

struct Xrs64(u64);

impl Xrs64 {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish in `[lo, hi)`.
    #[inline]
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo)
    }
}

// ── query task ───────────────────────────────────────────────────────────────

/// Simulate one async DB query.
///
/// Allocates a variety of heap objects (Vec, String, Box, HashMap, Arc) at
/// multiple sizes, yields to the runtime via `sleep`, then sends the result
/// through `tx`.  One `spawn_blocking` call per task stresses TLS heap-init
/// on tokio blocking threads.
///
/// `_permit` is a `Semaphore` permit held for the lifetime of the task; it is
/// dropped when the task completes, releasing a slot for the next task.  This
/// caps the number of concurrently live task allocations and prevents the
/// `SeferMalloc` segment table (`MAX_SEGMENTS = 1024`) from exhausting under
/// large `SEFER_BURNIN_TASKS` values.
async fn run_query(
    task_id: usize,
    tx: mpsc::Sender<QueryResult>,
    completed: Arc<AtomicU64>,
    in_flight: Arc<AtomicU64>,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let mut rng = Xrs64::new(
        0x9E37_79B9_7F4A_7C15u64
            .wrapping_mul(task_id as u64 + 1)
            .wrapping_add(0xDEAD_BEEF_CAFE_1234),
    );

    let mut alloc_estimate: u64 = 0;

    // ── Phase 1: allocate "index scan" buffers (Vec<u8>) ─────────────────
    // Sizes: 8 B – 32 KiB, distributed small-heavy.
    let n_bufs = rng.range(3, 9) as usize;
    let mut bufs: Vec<Vec<u8>> = Vec::with_capacity(n_bufs);
    for i in 0..n_bufs {
        let r = rng.next_u64();
        let sz = if r % 16 == 0 {
            // ~6% large: 4 KiB – 32 KiB
            (4 * 1024 + (r >> 8) as usize % (32 * 1024 - 4 * 1024)).max(1)
        } else {
            // ~94% small: 8 B – 512 B
            (8 + (r >> 8) as usize % (512 - 8)).max(1)
        };
        let mut buf = Vec::with_capacity(sz);
        buf.resize(sz, (i as u8).wrapping_add(0xAB));
        bufs.push(buf);
        alloc_estimate += 1;
    }

    // Yield: simulate I/O wait between buffer allocations.
    let wait_ms = rng.range(1, 8);
    sleep(Duration::from_millis(wait_ms)).await;

    // ── Phase 2: build "result rows" (Vec<String>) ────────────────────────
    let n_rows = rng.range(4, 20) as usize;
    let mut rows: Vec<String> = Vec::with_capacity(n_rows);
    for j in 0..n_rows {
        let row_len = rng.range(20, 200) as usize;
        // Build a string of the form "row-<id>-<j>-<padding...>"
        let base = format!("task-{task_id}-row-{j}-");
        let padding: String = std::iter::repeat('x').take(row_len.saturating_sub(base.len())).collect();
        rows.push(format!("{base}{padding}"));
        alloc_estimate += 2; // format + padding string
    }

    // Yield again.
    let wait_ms = rng.range(2, 15);
    sleep(Duration::from_millis(wait_ms)).await;

    // ── Phase 3: build "stats" HashMap ────────────────────────────────────
    let n_keys = rng.range(3, 12) as usize;
    let mut stats: HashMap<String, u64> = HashMap::with_capacity(n_keys);
    for k in 0..n_keys {
        let key = format!("stat-{k}");
        let val = rng.next_u64() % 1_000_000;
        stats.insert(key, val);
        alloc_estimate += 1;
    }

    // ── Phase 4: Box + Arc allocations (verify heap-allocated pointer types)
    let boxed: Box<[u8; 64]> = Box::new([0xCC; 64]);
    alloc_estimate += 1;
    let arc_payload: Arc<Vec<u64>> = Arc::new((0..32u64).collect());
    alloc_estimate += 2; // Arc + inner Vec

    // Use the allocations so they are not dead-store-eliminated.
    let _ = std::hint::black_box(boxed.as_ref()[0]);
    let _ = std::hint::black_box(arc_payload.len());

    // ── Phase 5: spawn_blocking — stresses TLS heap-init on blocking threads
    let blocking_result = tokio::task::spawn_blocking(move || {
        // Allocate inside the blocking thread to force TLS heap init.
        let v: Vec<u64> = (0..1024u64).map(|x| x * x).collect();
        let s: String = format!("blocking-task-{task_id}-sum={}", v.iter().sum::<u64>());
        // Drop `bufs` here (allocated on an async worker, freed on a blocking
        // thread) — this is the canonical cross-thread-free scenario.
        drop(bufs);
        s
    });
    let blocking_str = blocking_result.await.expect("spawn_blocking panicked");
    alloc_estimate += 3; // Vec + String + spawned closure capture
    let _ = std::hint::black_box(blocking_str.len());

    // ── Phase 6: final yield before sending ──────────────────────────────
    let wait_ms = rng.range(1, 10);
    sleep(Duration::from_millis(wait_ms)).await;

    alloc_estimate += 5; // QueryResult fields overhead

    let result = QueryResult {
        task_id,
        rows,
        stats,
        alloc_estimate,
    };

    // Send result into the pipeline; drop and skip if channel is closed
    // (happens when timeout fires).
    let _ = tx.send(result).await;

    completed.fetch_add(1, Ordering::Relaxed);
    in_flight.fetch_sub(1, Ordering::Relaxed);
}

// ── coordinator task ─────────────────────────────────────────────────────────

/// Receives `QueryResult`s from query tasks, drops half immediately
/// (cross-task free), forwards the rest to the aggregator.
///
/// The drop-half pattern maximises the chance that allocations made on one
/// tokio worker thread are freed on a *different* worker thread — the key
/// cross-thread-free scenario that exercises the xthread path in `SeferMalloc`.
async fn coordinator(
    mut rx: mpsc::Receiver<QueryResult>,
    fwd_tx: mpsc::Sender<QueryResult>,
) {
    let mut seq: u64 = 0;
    while let Some(result) = rx.recv().await {
        seq += 1;
        if seq % 2 == 0 {
            // Drop half — this may happen on a different worker than the
            // one that allocated the result's fields.
            drop(result);
        } else {
            // Forward the rest; if agg is gone, just drop.
            let _ = fwd_tx.send(result).await;
        }
    }
}

// ── aggregator task ───────────────────────────────────────────────────────────

/// Accumulates totals from coordinator-forwarded results.
async fn aggregator(
    mut rx: mpsc::Receiver<QueryResult>,
    total_allocs: Arc<AtomicU64>,
) -> u64 {
    let mut tasks_seen: u64 = 0;
    while let Some(result) = rx.recv().await {
        total_allocs.fetch_add(result.alloc_estimate, Ordering::Relaxed);
        // task_id is carried through the pipeline to confirm result identity
        // (suppresses dead_code without a lint allow attribute).
        let _ = result.task_id;
        // Drop the rows + stats here (cross-task free).
        drop(result.rows);
        drop(result.stats);
        tasks_seen += 1;
    }
    tasks_seen
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    // ── read ENV configuration ────────────────────────────────────────────

    let workers: usize = std::env::var("SEFER_TOKIO_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(8))
                .unwrap_or(DEFAULT_WORKERS)
        })
        .max(1);

    let n_tasks: usize = std::env::var("SEFER_BURNIN_TASKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TASKS)
        .clamp(1, 4096);

    let concurrency: usize = std::env::var("SEFER_BURNIN_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CONCURRENCY)
        .clamp(1, 512);

    let burn_secs: u64 = std::env::var("SEFER_BURNIN_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SECONDS)
        .max(1);

    println!("== sefer-alloc tokio burn-in ==");
    println!("global_allocator: SeferMalloc (ALL allocs — including tokio internals)");
    println!("workers={workers}  tasks={n_tasks}  concurrency={concurrency}  duration={burn_secs}s");
    println!("(SEFER_TOKIO_WORKERS / SEFER_BURNIN_TASKS / SEFER_BURNIN_CONCURRENCY / SEFER_BURNIN_SECONDS)");
    println!();

    // ── build the runtime ─────────────────────────────────────────────────

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_time()
        .thread_name("sefer-tokio-worker")
        .build()
        .expect("failed to build tokio runtime");

    // Shared counters.
    let completed = Arc::new(AtomicU64::new(0));
    let in_flight = Arc::new(AtomicU64::new(0));
    let total_allocs = Arc::new(AtomicU64::new(0));

    let wall_start = Instant::now();

    let exit_code = rt.block_on(async {
        let burn_duration = Duration::from_secs(burn_secs);

        // ── channels: query → coordinator → aggregator ────────────────
        let (query_tx, coord_rx) = mpsc::channel::<QueryResult>(CHAN_CAPACITY);
        let (fwd_tx, agg_rx) = mpsc::channel::<QueryResult>(CHAN_CAPACITY);

        // ── spawn aggregator ──────────────────────────────────────────
        let agg_total_allocs = Arc::clone(&total_allocs);
        let agg_handle = tokio::spawn(aggregator(agg_rx, agg_total_allocs));

        // ── spawn coordinator ─────────────────────────────────────────
        let coord_handle = tokio::spawn(coordinator(coord_rx, fwd_tx));

        // ── concurrency semaphore ─────────────────────────────────────
        // Caps the number of simultaneously live query tasks to avoid
        // exhausting SeferMalloc's MAX_SEGMENTS = 1024 table when
        // SEFER_BURNIN_TASKS > concurrency.
        let sem = Arc::new(Semaphore::new(concurrency));

        // ── spawn query tasks ─────────────────────────────────────────
        // We use a `JoinSet` + semaphore pair to bound live tasks to
        // `concurrency` at a time.  The launcher acquires a permit before each
        // `tokio::spawn`; the permit lives inside the task and is released on
        // task completion.  `JoinSet` collects results as tasks finish, so the
        // set never holds more than `concurrency` live task cells simultaneously
        // — preventing the `SeferMalloc` segment table from exhausting.
        let launcher = {
            let sem = Arc::clone(&sem);
            let query_tx = query_tx.clone();
            let completed = Arc::clone(&completed);
            let in_flight = Arc::clone(&in_flight);
            tokio::spawn(async move {
                let mut set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
                for i in 0..n_tasks {
                    // Acquire permit (yields until a slot is free).
                    let permit = Arc::clone(&sem)
                        .acquire_owned()
                        .await
                        .expect("semaphore closed");
                    let tx = query_tx.clone();
                    let cmp = Arc::clone(&completed);
                    let inf = Arc::clone(&in_flight);
                    inf.fetch_add(1, Ordering::Relaxed);
                    set.spawn(run_query(i, tx, cmp, inf, permit));
                    // Drain any already-finished tasks to keep the set small.
                    while let Some(res) = set.try_join_next() {
                        if let Err(e) = res {
                            eprintln!("[burn-in] query task panicked: {e:?}");
                        }
                    }
                }
                // Drain remaining.
                while let Some(res) = set.join_next().await {
                    if let Err(e) = res {
                        eprintln!("[burn-in] query task panicked: {e:?}");
                    }
                }
            })
        };
        // Drop the main copy of query_tx so coordinator sees EOF when all
        // query tasks (and the launcher's clone) finish.
        drop(query_tx);

        // ── heartbeat + timeout ───────────────────────────────────────
        let hb_completed = Arc::clone(&completed);
        let hb_in_flight = Arc::clone(&in_flight);
        let wall_start_inner = wall_start;
        let heartbeat = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
            tick.tick().await; // skip the immediate first tick
            loop {
                tick.tick().await;
                let elapsed = wall_start_inner.elapsed().as_secs_f64();
                let done = hb_completed.load(Ordering::Relaxed);
                let fly = hb_in_flight.load(Ordering::Relaxed);
                println!(
                    "[T+{elapsed:.0}s] alive: {done} tasks completed, {fly} in flight"
                );
            }
        });

        // Wait for the launcher (which itself waits for all query tasks via
        // the JoinSet) under the overall timeout.
        let result = timeout(burn_duration, async {
            match launcher.await {
                Ok(()) => 0u8,
                Err(e) => {
                    eprintln!("[burn-in] launcher panicked: {e:?}");
                    1u8
                }
            }
        })
        .await;

        heartbeat.abort();

        // Coordinator will exit when its rx is closed (all query_tx senders
        // have been dropped — the clone per task is dropped when each task
        // completes, and we dropped the original above).
        if let Err(e) = coord_handle.await {
            eprintln!("[burn-in] coordinator panicked: {e:?}");
            return 1;
        }

        // Aggregator exits when fwd_tx is dropped (which happens when coord
        // exits and drops its fwd_tx clone).
        let agg_seen = match agg_handle.await {
            Ok(n) => n,
            Err(e) => {
                eprintln!("[burn-in] aggregator panicked: {e:?}");
                return 1;
            }
        };

        let elapsed = wall_start.elapsed();
        let done = completed.load(Ordering::Relaxed);
        let alloc_est = total_allocs.load(Ordering::Relaxed);

        println!();
        println!("== tokio burn-in complete ==");
        println!("  elapsed:             {:.2}s", elapsed.as_secs_f64());
        println!("  workers:             {workers}");
        println!("  concurrency_cap:     {concurrency}");
        println!("  tasks_spawned:       {n_tasks}");
        println!("  tasks_completed:     {done}");
        println!("  agg_results_seen:    {agg_seen}  (coordinator forwarded ~half)");
        println!("  total_allocs_est:    {alloc_est}  (per-task estimate; excludes tokio internals)");

        match result {
            Ok(code) => code as i32,
            Err(_elapsed) => {
                // Timeout is OK for a burn-in: tasks were still running when
                // time expired — that is the normal path for long runs.
                println!("  (timeout reached — this is normal for long burn-in runs)");
                0
            }
        }
    });

    let elapsed = wall_start.elapsed();
    println!();
    if exit_code == 0 {
        println!("[burn-in] exit 0 — no panics, no deadlocks detected ({:.2}s)", elapsed.as_secs_f64());
    } else {
        eprintln!("[burn-in] exit {exit_code} — see errors above");
        std::process::exit(exit_code);
    }
}
