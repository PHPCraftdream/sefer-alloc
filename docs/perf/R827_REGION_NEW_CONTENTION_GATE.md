# R827: Region::new() Contention Gate

## What and why

This gate measures `Region::new()` throughput under multi-threaded contention on the shared `NEXT_REGION_ID` atomic (post-#813 `fetch_update` mechanism), replacing the defective historical harness in `region_bench.rs`. The old harness had methodological flaws:
- Sequential thread start (no barrier-aligned start)
- `Instant::elapsed()` calls inside the hot loop
- No baseline to isolate contention cost from the rest of `Region::new()`'s work

This gate measures the actual cost of contention on `NEXT_REGION_ID` by comparing three arms:
- `shared_atomic`: real load — calls `Region::<u64>::new()` repeatedly, exercising the actual shared `NEXT_REGION_ID` atomic via its `fetch_update`/CAS-retry-loop primitive
- `shared_fetch_add`: isolates cache-line contention ALONE — a SHARED atomic, but a plain `fetch_add` (not `fetch_update`), so the primitive matches `baseline_local_atomic`'s and only "shared vs local" varies
- `baseline_local_atomic`: isolates contention entirely — a thread-LOCAL atomic with `fetch_add`, so the primitive matches `shared_fetch_add`'s and only "shared vs local" varies

**Update (2026-08-11, task #832 closing review, finding F-C6):** the original two-arm design (`shared_atomic` vs `baseline_local_atomic` only) conflated two distinct costs, because #813 changed BOTH the sharing regime (adding a process-wide `NEXT_REGION_ID`) AND the RMW primitive (`fetch_add` → `fetch_update`/CAS-loop) at the same time. A baseline that only varies "shared vs local" cannot attribute a measured gap to either cause alone. The `shared_fetch_add` arm was added to close this gap — see "Decomposition" below.

## Immutable source identity

Measured on commit `a935e79cc2f589880402452a79e0186861f70bb6` (adds the `shared_fetch_add` decomposition arm; see the closing-review update above).

This commit includes ONLY the harness implementation (`crates/region/benches/region_new_contention_gate.rs`) plus doc-comment fixes elsewhere in the crate found by the same review — no measurement results. The harness code is immutable at this SHA for the numbers below. (Prior harness identities, superseded by this measurement: `59c079c` — the original two-arm harness, still a valid citation for the historical two-arm-only numbers this report previously carried; `8a6e190` — amended out before any measurement, never a valid citation.)

## Methodology

- **Barrier-aligned start:** All threads wait on a `std::sync::Barrier` before beginning timed work, ensuring aligned start.
- **Fixed work:** Each thread performs exactly `ITERS_PER_THREAD = 200,000` iterations (not fixed duration). Chosen empirically for ~100-300ms per sample on a single thread.
- **Thread counts:** N ∈ {1, 2, 4, 8}, capped by `std::thread::available_parallelism()`.
- **Samples:** `SAMPLES = 5` independent repetitions per (arm, thread_count) combination.
- **Arms:**
  - `shared_atomic`: Repeatedly constructs and drops `Region::<u64>::new()`.
  - `shared_fetch_add`: Performs (a) one `fetch_add(1)` on a SHARED `Arc<AtomicUsize>` (real cross-thread contention, but the same primitive as `baseline_local_atomic`) and (b) one `SlotMap::<DefaultKey, u64>::new()` allocation and drop.
  - `baseline_local_atomic`: Performs (a) one `fetch_add(1)` on a thread-local `AtomicUsize` (no cross-thread contention) and (b) one `SlotMap::<DefaultKey, u64>::new()` allocation and drop.
- **Output:** Raw per-sample CSV lines printed BEFORE any summary prose, with derived mean/median computed directly from those samples (no manual transcription).

## Results

| arm | threads | mean ops/sec | median ops/sec |
|-----|---------|--------------|----------------|
| shared_atomic | 1 | 6,790,796 | 6,775,573 |
| shared_atomic | 2 | 5,361,433 | 6,282,265 |
| shared_atomic | 4 | 6,986,049 | 7,022,053 |
| shared_atomic | 8 | 5,477,466 | 6,113,467 |
| shared_fetch_add | 1 | 5,719,373 | 5,990,122 |
| shared_fetch_add | 2 | 8,568,206 | 8,623,477 |
| shared_fetch_add | 4 | 11,968,411 | 12,289,051 |
| shared_fetch_add | 8 | 12,695,237 | 12,441,109 |
| baseline_local_atomic | 1 | 6,624,609 | 6,937,411 |
| baseline_local_atomic | 2 | 14,070,938 | 13,492,045 |
| baseline_local_atomic | 4 | 25,031,949 | 24,278,179 |
| baseline_local_atomic | 8 | 40,972,705 | 44,065,239 |

(Table copied verbatim from `docs/perf/R827_REGION_NEW_CONTENTION_GATE_summary.csv`, itself derived from `docs/perf/_raw_r827_region_new_contention.log` by a small script — no hand-transcription.)

## Interpretation

The overhead ratio at 8 threads is **0.134** (shared_atomic.mean / baseline_local_atomic.mean) — `Region::new()` under 8-thread contention runs at **13.4%** of the isolated baseline's throughput, an **~87% throughput penalty**. (This ratio is noisy run to run on this single dev host — an earlier run of the same two arms measured 0.153/~85%; both runs agree on the qualitative picture: the shared arm never scales past ~1 thread's throughput.)

The baseline arm shows near-linear scaling from 1→8 threads (6.6M → 41.0M ops/sec), while the `shared_atomic` arm is essentially flat across all thread counts (5.4M-7.0M ops/sec regardless of N). The contention bottleneck saturates almost immediately — aggregate `Region::new()` throughput does not meaningfully improve past 1 thread, let alone scale with thread count.

## Decomposition (added 2026-08-11, closing-review finding F-C6)

At 8 threads:
- **`contention_ratio`** = `shared_fetch_add.mean / baseline_local_atomic.mean` = 12,695,237 / 40,972,705 = **0.310** — this isolates PURE cache-line contention: same `fetch_add` primitive, only "shared vs local" differs. A shared, contended `fetch_add` alone costs ~69% of throughput versus no sharing at all.
- **`cas_primitive_ratio`** = `shared_atomic.mean / shared_fetch_add.mean` = 5,477,466 / 12,695,237 = **0.431** — this isolates the cost of `fetch_update`'s CAS-retry-loop versus a plain `fetch_add`, holding the sharing regime constant (both arms are shared/contended). The CAS loop costs an ADDITIONAL ~57% on top of the contention already measured by `contention_ratio`.
- The two ratios compose: `0.310 × 0.431 ≈ 0.134`, matching the `overhead_ratio` above (mean-based; median-based recomposition will differ slightly due to per-arm sampling noise).

**Conclusion:** the ~87% total penalty is NOT purely a "shared atomic is slow" story — F1/#813's own primitive change (fetch_add → fetch_update/CAS-loop, made necessary by the exhaustion fix) contributes roughly as much as cache-line contention does. Both cost/perf changed together in that fix, which was a correctness fix (region_id reuse was a release blocker), not a perf regression under this project's own perf-vs-correctness framing — but this decomposition means a future perf-improvement attempt on this contention point should not assume switching back to a shared `fetch_add` would recover most of the gap; roughly a third of it is cache-line contention that plain `fetch_add` would still pay.

## Artifacts

- Raw log: `docs/perf/_raw_r827_region_new_contention.log` (force-added via `git add -f`, size < 200 KiB — Tier 1 per artifact storage policy)
- Summary CSV: `docs/perf/R827_REGION_NEW_CONTENTION_GATE_summary.csv`

## Structural mitigations considered (2026-08-11 code-quality review, Q22)

The following two structural mitigations for the measured contention were raised after R827's measurement, but were not implemented and are not recommended to pursue without further measurement:

1. **Lazy minting of `region_id`.** Currently, `Region::new()` and `Region::with_capacity()` mint `region_id` eagerly via a CAS loop on the shared `NEXT_REGION_ID` atomic, even though no `region_id` is needed until the first `insert()` (all accessors only compare handles against the region's own ID). A hypothetical lazy design would store `Option<NonZeroUsize>` and mint inside `insert()` — removing the shared atomic from `Region::new()` entirely, and from the workload R827 measured. This would come with real trade-offs:
   - The exhaustion panic moves from `new()`/`with_capacity()` to `insert()` — a documented-contract change, since `insert`'s current panic list names only the slotmap-full case.
   - `Debug` would need to render an unminted region (a state the current design never exposes).
   - The branch cost per insert, though perfectly predicted, is non-zero.
   This was not implemented because it changes the published panic contract and adds runtime overhead on the hot path (insert) to remove it from a cold path (construction). If future perf work reopens this question, `benches/region_new_contention_gate.rs` can be extended with a fourth arm measuring lazy minting directly.

2. **Block reservation of `region_id` values.** A thread-local cursor claiming `N` IDs per shared RMW would amortize the contended operation `N`-fold (e.g., a thread-local `AtomicUsize` fetch-adding from a shared pool of 1000 IDs before touching the global `NEXT_REGION_ID`). This reduces contention at the cost of dividing the 32-bit id budget by `N`. Since this crate explicitly documents 32-bit exhaustion as *reachable rather than theoretical* (`region.rs:210-217`), a reservation mechanism that burns IDs faster is a net regression on the one target where contention actually matters. Additionally, the state machinery for thread-local pools (cleanup on thread exit, cross-thread handoff on migration, etc.) is nontrivial to implement soundly without re-entrancy hazards in `Drop`. This was not implemented because the id-budget trade-off is exactly wrong for the target that most needs the contention fix. If future work revisits this, it should pair a reservation scheme with a concurrent-generation extension (e.g. `u64` IDs on 64-bit targets only) rather than accepting the current id-space truncation.