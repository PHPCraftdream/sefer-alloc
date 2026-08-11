# R827: Region::new() Contention Gate

## What and why

This gate measures `Region::new()` throughput under multi-threaded contention on the shared `NEXT_REGION_ID` atomic (post-#813 `fetch_update` mechanism), replacing the defective historical harness in `region_bench.rs`. The old harness had methodological flaws:
- Sequential thread start (no barrier-aligned start)
- `Instant::elapsed()` calls inside the hot loop
- No baseline to isolate contention cost from the rest of `Region::new()`'s work

This gate measures the actual cost of contention on `NEXT_REGION_ID` by comparing two arms:
- `shared_atomic`: real load — calls `Region::<u64>::new()` repeatedly, exercising the actual shared atomic
- `baseline_local_atomic`: isolates contention — performs the same RMW pattern but on a thread-local atomic plus a `SlotMap` allocation

## Immutable source identity

Measured on commit `59c079cf480e9c9a54297019d68c3a73aca5e22b`.

This commit includes ONLY the harness implementation and `Cargo.toml` bench registration — no measurement results or documentation changes. The harness code is immutable at this SHA. (An earlier harness commit, `8a6e190`, was amended-out and replaced by this one solely to fix `cargo fmt` formatting on the harness file — no behavioral change — before this measurement was taken; `8a6e190` was never cited by any committed artifact and is not a valid citation for this report.)

## Methodology

- **Barrier-aligned start:** All threads wait on a `std::sync::Barrier` before beginning timed work, ensuring aligned start.
- **Fixed work:** Each thread performs exactly `ITERS_PER_THREAD = 200,000` iterations (not fixed duration). Chosen empirically for ~100-300ms per sample on a single thread.
- **Thread counts:** N ∈ {1, 2, 4, 8}, capped by `std::thread::available_parallelism()`.
- **Samples:** `SAMPLES = 5` independent repetitions per (arm, thread_count) combination.
- **Arms:**
  - `shared_atomic`: Repeatedly constructs and drops `Region::<u64>::new()`.
  - `baseline_local_atomic`: Performs (a) one `fetch_add(1)` on a thread-local `AtomicUsize` (same RMW pattern, no cross-thread contention) and (b) one `SlotMap::<DefaultKey, u64>::new()` allocation and drop. This approximates the non-contention work inside `Region::new()`.
- **Output:** Raw per-sample CSV lines printed BEFORE any summary prose, with derived mean/median computed directly from those samples (no manual transcription).

## Results

| arm | threads | mean ops/sec | median ops/sec |
|-----|---------|--------------|----------------|
| shared_atomic | 1 | 6,959,375 | 6,944,734 |
| shared_atomic | 2 | 6,775,563 | 6,264,791 |
| shared_atomic | 4 | 6,637,530 | 6,712,158 |
| shared_atomic | 8 | 6,645,813 | 6,676,191 |
| baseline_local_atomic | 1 | 7,581,012 | 6,969,734 |
| baseline_local_atomic | 2 | 12,460,242 | 12,415,497 |
| baseline_local_atomic | 4 | 23,658,480 | 23,632,980 |
| baseline_local_atomic | 8 | 43,355,569 | 43,212,877 |

(Table copied verbatim from `docs/perf/R827_REGION_NEW_CONTENTION_GATE_summary.csv`, itself derived from `docs/perf/_raw_r827_region_new_contention.log` by a small script — no hand-transcription.)

## Interpretation

The overhead ratio at 8 threads is **0.153** (shared_atomic.mean / baseline_local_atomic.mean).

This means `Region::new()` under 8-thread contention runs at **15.3%** of the throughput of the isolated baseline. In other words, contention on `NEXT_REGION_ID` causes an **~85% throughput penalty** at 8 threads.

The baseline arm shows near-linear scaling from 1→8 threads (7.58M → 43.4M ops/sec), while the shared_atomic arm is essentially flat across all thread counts (6.6M-7.0M ops/sec regardless of N). The contention bottleneck saturates almost immediately — aggregate `Region::new()` throughput does not meaningfully improve past 1 thread, let alone scale with thread count.

## Artifacts

- Raw log: `docs/perf/_raw_r827_region_new_contention.log` (force-added via `git add -f`, size < 200 KiB — Tier 1 per artifact storage policy)
- Summary CSV: `docs/perf/R827_REGION_NEW_CONTENTION_GATE_summary.csv`