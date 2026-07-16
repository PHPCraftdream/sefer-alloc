# R7 Workstream C2 -- Pool-cap sweep and documented presets

**Date:** 2026-07-17
**Base revision:** `e53f1f7` (HEAD of `main`, post-R7 plan)
**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release profile
**Feature set:** `production` (default, no `alloc-lazy-commit`)
**Harnesses:**
- `cargo bench --features production --bench global_alloc -- working_set_cycle`
  (criterion wall-clock, `sample_size(10)`, fast profile)
- `cargo bench --features production --bench global_alloc -- pool_cap_sweep`
  (deterministic decommit-count judge, existing harness)
- `cargo bench --features production --bench global_alloc -- segment_decommit_cycle`
  (segment lifecycle latency)
- `cargo run --release --features production --example first_alloc_process`
  (process-per-sample commit/RSS)
- `cargo test --release --features production --test small_segment_pool`
  (pool correctness: 10 tests)
- `cargo test --release --features production --test regression_c3_unbounded_recycle`
  (bounded-retention + eventual-drain proof)

---

## 1. What the pool cap controls

`SmallSegmentPoolConfig::pool_segments` (file:
`src/alloc_core/small_segment_pool_config.rs`) caps the number of empty small
segments retained in the hysteresis pool. When a small segment empties
(`live_count == 0`), it is either RETAINED in the pool (pages stay committed,
free lists intact, no OS round-trip) or RELEASED immediately (decommit + OS
release + slot recycle). The pool's purpose: avoid the
decommit-recommit-reserve churn when the working set oscillates across segment
boundaries.

`pool_byte_cap` is a secondary ceiling: the effective cap is
`min(pool_segments, pool_byte_cap / SEGMENT)`. Each small segment is exactly
`SEGMENT` = 4 MiB, so a matched byte cap is `pool_segments * 4 MiB`.

**Default:** `pool_segments = 4`, `pool_byte_cap = 16 MiB` (= 4 x 4 MiB).

## 2. Sweep methodology

For each `pool_cap` in {0, 1, 4, 8, 16}, `pool_byte_cap` was set to
`pool_cap * 4 MiB` (matched, so only the segment-count knob constrains).
The default constants in `small_segment_pool_config.rs` were temporarily
changed, the crate rebuilt, all harnesses re-run sequentially (one cargo/bench
process at a time), and the defaults restored after all measurements. Final
`git diff src/` is empty -- no sweep scaffolding remains.

## 3. Decommit-count sweep (deterministic judge)

The `pool_cap_sweep` harness spreads allocations across 40 distinct small
segments, empties every one in a single ring-drain, and counts the
`decommit_calls` delta. Deterministic (run-to-run identical), block-size
independent (the pool mechanism is size-agnostic).

| pool_cap | decommit_calls | pooled | segments released |
|---------:|---------------:|-------:|------------------:|
| 0        | 39             | 0      | 39                |
| 1        | 38             | 1      | 38                |
| 4        | 35             | 4      | 35                |
| 8        | 31             | 8      | 31                |
| 16       | 23             | 16     | 23                |

Formula: `decommit = 40 - 1 - pool_cap` (40 segments spread, 1 is always the
current carve target and is never decommitted). Perfect monotonic decrease.
Every cap value is honoured exactly.

## 4. Working-set cycle (production oscillation pattern)

The `working_set_cycle` bench: 64 independent working sets, each oscillated
(free-then-realloc every block) -- the exact pattern the pool exists to smooth.
Criterion `sample_size(10)`, fast profile. The `decommit_calls` and
`segments_released_total` deltas are process-wide cumulative snapshots.

### 4.1 Decommit calls during working_set_cycle

| pool_cap | 16 B | 64 B | 256 B | 1024 B |
|---------:|-----:|-----:|------:|-------:|
| 0        | 0    | 118  | 217   | 471    |
| 1        | 0    | 48   | 217   | 379    |
| 4        | 0    | 0    | 173   | 379    |
| 8        | 0    | 0    | 56    | 379    |
| 16       | 0    | 0    | 0     | 201    |

**Reading the table:**
- 16 B: all blocks fit within the primordial segment; no segment ever empties.
  Pool cap is irrelevant.
- 64 B: at cap=0, 118 decommit calls (heavy oscillation churn). cap=1 absorbs
  some (48 remaining). cap>=4 absorbs all (0 decommit calls).
- 256 B: cap=4 still shows 173 decommit calls (the working set spans more
  segments than the pool can absorb). cap=8 drops to 56. cap=16 drops to 0.
- 1024 B: the working set spans many segments. cap=4 shows 379. cap=16 drops
  to 201. Full absorption would require a cap > 16 (the 1024 B oscillation
  involves ~20+ segments cycling).

### 4.2 Wall-clock latency (noisy, directional only)

| pool_cap | 16 B (us) | 64 B (us) | 256 B (us) | 1024 B (us) |
|---------:|----------:|----------:|-----------:|-----------:|
| 0        | 188       | 183       | 192        | 225         |
| 1        | 217       | 218       | 190        | 225         |
| 4        | 184       | 189       | 195        | 215         |
| 8        | 182       | 186       | 193        | 217         |
| 16       | 182       | 185       | 198        | 200         |

**Caveat:** criterion `sample_size(10)` on a noisy Windows host with +-15-20%
run-to-run variance (documented in R6_CROSS_VERSION_BENCH.md). The differences
between cap values at 16/64/256 B are WITHIN the noise band. The signal at
1024 B (cap=16: 200 us vs cap=0: 225 us, ~11% improvement) is at the edge of
detectability.

**The decommit-count table (section 4.1) is the authoritative judge, not
wall-clock.** The wall-clock numbers confirm there is no REGRESSION from raising
the pool cap, and show a directional improvement at 1024 B (fewer OS
round-trips = less latency), but the per-run noise makes precise deltas
unreliable.

## 5. Steady-state churn (hot-path independence)

The pool is a COLD-path mechanism: it is only consulted when a segment empties
(`live_count == 0`) or when a new segment is reserved. The hot alloc/free path
never touches the pool. Churn bench results at default cap=4:

| Size | SeferAlloc (us/batch of 256 ops) |
|-----:|--------------------------------:|
| 16 B | 23.6                            |
| 64 B | 23.7                            |
| 256 B | 24.2                           |
| 1024 B | 25.4                          |

**These numbers are cap-INSENSITIVE.** The churn bench keeps a fixed working
set alive; segments never empty; the pool is never touched. Verified: no
decommit-count delta at any cap. Raising the pool cap has zero effect on
steady-state hot-path throughput.

## 6. Retained commit / RSS

The pool retains `pool_cap * SEGMENT` = `pool_cap * 4 MiB` of committed pages
when the pool is full. This is the RSS cost of the pool -- committed but idle
pages kept warm for reuse.

| pool_cap | pool_byte_cap | Max retained commit |
|---------:|--------------:|--------------------:|
| 0        | 0             | 0 MiB               |
| 1        | 4 MiB         | 4 MiB               |
| 4        | 16 MiB        | 16 MiB              |
| 8        | 32 MiB        | 32 MiB              |
| 16       | 64 MiB        | 64 MiB              |

**First-heap commit is pool-cap-INSENSITIVE.** The first-alloc-process probe
at default cap=4: 4,628 KiB commit (1 heap), 128 KiB RSS (1 heap). The pool
does not affect the first-heap commit charge because no segment has emptied yet
at that point. The retained commit is a STEADY-STATE cost that appears only
after segments have been allocated, used, and emptied.

## 7. Segment decommit cycle (253 KiB block lifecycle)

The `segment_decommit_cycle` bench fills and empties a segment of 253 KiB
blocks. At default cap=4, SeferAlloc: ~1.1 us. At cap=0: also ~1.1 us. The
pool cap does not materially affect this bench because the pool is
populated/drained within the same iteration cycle (absorbed segments are not
reused before being drained for the next iteration). This confirms the pool is
not a lifecycle-latency mechanism but a REUSE-avoidance one.

## 8. OOM pool-drain correctness

The pool is a reclaimable soft reserve: under memory pressure, both
`alloc_large_slow` and `reserve_small_segment` call `drain_small_pool()` as a
fallback when OS reservation fails. This drains the entire pool (releasing
every retained segment back to the OS), freeing the committed RSS for reuse.

**Verification:**
- `cargo test --release --features production --test small_segment_pool`:
  10 tests PASS, including `pool_fills_to_cap_and_no_more` (bounded retention)
  and `disabled_pool_never_retains` (cap=0 correctness).
- `cargo test --release --features production --test regression_c3_unbounded_recycle`:
  1 test PASS (bounded-retention + eventual-drain proof: at most `pool_cap`
  slots retained, every retained slot is reusable or drainable, drain releases
  all pooled segments).
- The `dbg_drain_small_pool` seam is exercised by both tests and by the
  `pool_cap_sweep` bench harness (cleanup after each cap's measurement).

**The pool remains a reclaimable soft reserve at every cap value.**

## 9. Documented presets

Per R7_PLAN.md P6, these are DOCUMENTED RECIPES over the existing
`SmallSegmentPoolConfig` API -- not new constructors or a preset enum.

### 9.1 low-rss -- minimal retained commit

```text
use sefer_alloc::SmallSegmentPoolConfig;

const POOL: SmallSegmentPoolConfig = SmallSegmentPoolConfig::new()
    .pool_segments(0)
    .pool_byte_cap(0);
```

| Knob | Value |
|---|---|
| pool_segments | 0 |
| pool_byte_cap | 0 |
| Max retained commit | 0 MiB |
| Decommit calls (40-seg drain) | 39 |
| Working-set 64B decommit | 118 |
| Working-set 1024B decommit | 471 |

**Trade-off:** Zero retained RSS. Every empty segment is released immediately.
Full OS round-trip (decommit + release + re-reserve + recommit + page fault)
on every segment oscillation. Maximum decommit/reserve churn.

**Use when:** Memory-constrained environments where every MiB of retained
commit matters more than allocation throughput under oscillation. Embedded
systems, containers with strict memory limits, serverless functions where
billed RSS is the dominant cost.

Alternatively, `pool_segments(1)` / `pool_byte_cap(4 MiB)` retains only 4 MiB
and still absorbs some 64B oscillation churn (48 decommit calls vs 118).

### 9.2 balanced -- the current production default

```text
use sefer_alloc::SmallSegmentPoolConfig;

const POOL: SmallSegmentPoolConfig = SmallSegmentPoolConfig::new();
// Equivalent to: .pool_segments(4).pool_byte_cap(16 * 1024 * 1024)
```

| Knob | Value |
|---|---|
| pool_segments | 4 (default) |
| pool_byte_cap | 16 MiB (default) |
| Max retained commit | 16 MiB |
| Decommit calls (40-seg drain) | 35 |
| Working-set 64B decommit | 0 |
| Working-set 256B decommit | 173 |
| Working-set 1024B decommit | 379 |

**Trade-off:** 16 MiB retained RSS at full pool. Completely absorbs the
oscillation churn at small block sizes (16 B, 64 B). Partially absorbs
256 B (173 decommit calls vs 217 at cap=0) and 1024 B (379 vs 471). A
practical middle ground for general-purpose server workloads.

**Use when:** General-purpose applications. Web servers, CLI tools, desktop
apps -- the default is designed for "no tuning needed." 16 MiB is a small
fraction of any modern server's RAM and eliminates the most common oscillation
pattern (sub-256-byte allocations cycling across segment boundaries).

### 9.3 throughput -- minimal decommit churn

```text
use sefer_alloc::SmallSegmentPoolConfig;

const POOL: SmallSegmentPoolConfig = SmallSegmentPoolConfig::new()
    .pool_segments(16)
    .pool_byte_cap(64 * 1024 * 1024);
```

| Knob | Value |
|---|---|
| pool_segments | 16 |
| pool_byte_cap | 64 MiB |
| Max retained commit | 64 MiB |
| Decommit calls (40-seg drain) | 23 |
| Working-set 64B decommit | 0 |
| Working-set 256B decommit | 0 |
| Working-set 1024B decommit | 201 |

**Trade-off:** 64 MiB retained RSS at full pool. Completely absorbs
oscillation churn up to 256 B block sizes. Halves the 1024 B churn
(201 vs 379 at default). Directional improvement in working-set-cycle
latency at 1024 B (~200 us vs ~215 us at default, within noise but
consistent with fewer OS round-trips).

**Use when:** Throughput-sensitive workloads that allocate/free/reallocate
across segment boundaries frequently (media processing pipelines, game engines,
scientific computing with oscillating working sets), on machines with ample
RAM where 64 MiB of retained commit is negligible. NOT suitable for
memory-constrained environments.

For extreme throughput (>16 segments oscillating), even larger caps are
possible -- the pool's intrusive list has no compile-time upper bound -- but
the data shows diminishing returns: 1024 B oscillation still shows 201
decommit calls at cap=16 because the working set spans ~20+ segments.

## 10. Default change verdict

**The default stays at pool_cap=4 / pool_byte_cap=16 MiB. No change.**

Justification:
1. The default already eliminates oscillation churn at the most common small
   block sizes (16 B, 64 B) -- zero decommit calls in the working-set-cycle
   bench.
2. 16 MiB is a modest retained RSS that is acceptable for the vast majority of
   production deployments.
3. Raising to cap=8 or cap=16 would reduce 256 B / 1024 B churn but costs
   32-64 MiB retained commit -- a 2-4x increase in the pool's RSS footprint
   with diminishing latency returns (wall-clock improvements are within noise
   at these sizes).
4. Lowering to cap=0 or cap=1 would save 12-16 MiB of retained commit but
   re-introduces decommit churn at 64 B (118 calls at cap=0 vs 0 at cap=4).
5. The three documented presets give users explicit, data-backed recipes to
   tune for their specific trade-off without changing the default.

## 11. Gate compliance

| Gate | Command | Result |
|---|---|---|
| `cargo fmt --check` | `cargo fmt --check` | PASS (no diff) |
| clippy (default) | `cargo clippy --all-targets -- -D warnings` | PASS |
| clippy (experimental) | `cargo clippy --all-targets --features experimental -- -D warnings` | PASS |
| clippy (all-features) | `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| clippy (production) | `cargo clippy --lib --features production -- -D warnings` | PASS |
| Production code change? | None (defaults restored to 4 / 16 MiB) | Zero diff to `src/` |
| Pool tests | `cargo test --release --features production --test small_segment_pool` | 10 tests PASS |
| Unbounded recycle | `cargo test --release --features production --test regression_c3_unbounded_recycle` | 1 test PASS |

No test files added, no production code changed, no `docs/ARCHITECTURE.md`
bump needed.

## 12. Caveats

1. **Host noise.** Criterion wall-clock numbers have +-15-20% run-to-run
   variance on this Windows host (documented in R6_CROSS_VERSION_BENCH.md).
   The decommit-count table (section 3, 4.1) is the deterministic,
   noise-free judge; wall-clock numbers (section 4.2) are directional only.

2. **IAI not applicable.** The pool cap is a runtime config knob, not a
   compile-time constant that changes instruction sequences. IAI
   (instruction-count via callgrind on Linux) measures the steady-state hot
   path, which is pool-cap-insensitive (section 5). The pool's cost savings
   are OS syscalls (decommit/recommit/reserve/release), not CPU instructions.

3. **Working-set-cycle geometry.** The bench's 64-working-set, 256-block-per-set
   oscillation pattern is a specific shape. Real workloads may have different
   segment-cycling patterns (more or fewer segments, different oscillation
   frequencies). The presets' relative ordering (low-rss < balanced < throughput)
   is geometry-independent; the exact decommit counts depend on the workload.

4. **Retained commit is a MAX.** The pool retains `pool_cap * 4 MiB` only
   when the pool is FULL (i.e., `pool_cap` segments have emptied and not been
   reused). In typical workloads the pool rarely saturates; the actual retained
   commit is usually lower.

5. **Decay drains the pool.** The `maybe_decay_small_pool` tick evicts the
   coldest pooled segment on each decay interval (shared with large-cache
   decay). An idle workload eventually drains the pool to zero, releasing all
   retained commit. The max-retained-commit numbers in section 6 are
   INSTANTANEOUS maximums, not steady-state costs for idle processes.
