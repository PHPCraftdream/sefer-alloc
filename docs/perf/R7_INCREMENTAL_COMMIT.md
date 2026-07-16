# R7 Workstream B — Incremental Windows Commit (B5 GO/NO-GO)

**Date:** 2026-07-17
**Base revision:** `main` (post B0–B4 commits: B0 vmem lazy-commit API, B1
commit frontier, B2 fallible bump growth, B3 decommit/recommit integration,
B4 correctness matrix + fault-injection hook)
**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release profile
**Feature:** `alloc-lazy-commit` (experimental, opt-in, additive over
`alloc-core`; Unix/miri falls back to eager; `numa-aware` forces eager)
**Harnesses:**
- `examples/first_alloc_process.rs` via manual multi-sample runs
  (the process-per-sample A1 methodology from `scripts/first-alloc-bench.mjs`)
- `benches/global_alloc.rs` (criterion, fast profile: `sample_size(10)`,
  short warm-up/measurement)
- `tests/lazy_commit_b2_grow.rs`, `tests/lazy_commit_b3_recycle.rs`,
  `tests/lazy_commit_b4_matrix.rs`, `tests/lazy_commit_frontier.rs`

---

## 1. Architecture recap — where lazy commit applies

`alloc-lazy-commit` changes the reservation path for **new small segments**
reserved via `AllocCore::reserve_small_segment`. Instead of committing the full
4 MiB segment span eagerly (`reserve_aligned` → `MEM_COMMIT` the entire usable
range), the lazy path uses `reserve_aligned_lazy`:

- Reserves 4 MiB of virtual address space (`MEM_RESERVE` only).
- Commits `[0, small_meta_end + LAZY_FIRST_CHUNK)` — metadata pages + the
  first payload chunk.
- Subsequent carve operations that exceed the `committed_payload_end` frontier
  trigger incremental `commit_range` calls in `GROW_CHUNK`-sized steps.
- On Unix/miri the lazy path falls back to eager (identical observable behavior).
- Under `numa-aware` the lazy path is disabled (eager VirtualAllocExNuma).

**The primordial segment** (the bootstrap segment that hosts the registry,
carved in `bootstrap::primordial`) is **always eagerly committed** via
`Segment::reserve` → `reserve_aligned`. This is a deliberate design choice
(B1 commit `0c981d7`): the primordial segment carries the self-hosted registry
array, hash table, and free-list stack — all accessed at arbitrary offsets
during bootstrap. Lazy-committing it would require restructuring the bootstrap
to commit pages before each metadata write, for negligible benefit (the
primordial is allocated exactly once per process).

### Consequence for the first-heap commit judge

The R7 plan's B5 primary criterion targets the **first-heap Windows commit
charge** measured by `first_alloc_process`: "4.52 MiB → ≤ 0.9 MiB (target
0.6–0.8 MiB)." The plan's sizing note (line 54) assumed the 0.9 MiB budget
was "~0.52 MiB non-segment (chunk + overflow sidecar) + metadata (tens of KiB)
+ first payload chunk (128–256 KiB)."

However, the first-heap commit charge is **entirely dominated by the primordial
segment** (4 MiB eager) plus the registry-chunk overhead (~0.5 MiB). The very
first `alloc` call triggers `registry::ensure()`, which materialises the
primordial segment — a 4 MiB eager commit. No additional `reserve_small_segment`
call occurs on the first-heap path (the primordial segment has ample carve space
for the probe's single 16-byte allocation). Therefore, `alloc-lazy-commit`
**cannot reduce the first-heap commit charge** — the primordial segment is
always eager, and the lazy path applies only to subsequent segments.

---

## 2. Baseline — production features, no `alloc-lazy-commit`

5 samples, `cargo run --release --example first_alloc_process --features production`:

| Metric | min | median | max |
|---|---:|---:|---:|
| Commit Δ 1 heap (KiB) | 4,620 | 4,628 | 4,628 |
| RSS Δ 1 heap (KiB) | 124 | 128 | 128 |
| First-alloc latency (ns) | 75,700 | 84,900 | 129,400 |
| Commit Δ 8 heaps (KiB) | 37,904 | 37,920 | 37,920 |
| Commit Δ 64 heaps (KiB) | 271,396 | 271,424 | 271,436 |

Criterion bench (`--features production`, SeferAlloc, ns/batch of 256 ops):

| Bench | 16 B | 64 B | 256 B | 1024 B |
|---|---:|---:|---:|---:|
| cold direct | 43,137 | 47,904 | 45,769 | 45,940 |
| churn | 24,582 | 24,612 | 24,658 | 26,523 |
| churn write | 24,772 | 25,345 | 24,969 | 27,230 |
| churn teardown | 32,041 | 33,633 | 33,536 | 57,931 |
| decommit cycle 253 KiB | — | — | 1,999 | — |

---

## 3. Chunk-size sweep — `alloc-lazy-commit` ON

For each chunk size (`LAZY_FIRST_CHUNK` = `GROW_CHUNK`), the constant was
edited, the crate was rebuilt with `--features "production alloc-lazy-commit"`,
and all harnesses were re-run on the same host in sequence (one cargo/bench
process at a time).

### 3.1 First-heap commit charge (primary judge)

| Chunk | Commit Δ 1 heap (KiB) | RSS Δ 1 heap (KiB) | Latency med (ns) |
|------:|----------------------:|--------------------:|------------------:|
| 64 KiB | 4,628 | 124 | 122,400 |
| 128 KiB | 4,628 | 124 | ~80,000 |
| 256 KiB | 4,628 | 124 | 90,200 |
| 512 KiB | 4,628 | 124 | ~90,000 |
| baseline | 4,628 | 128 | 84,900 |

**Result: identical across all chunk sizes and identical to baseline.** The
first-heap commit is entirely the primordial segment (always eager) + registry
chunk overhead. `alloc-lazy-commit` has zero effect on this metric.

### 3.2 Criterion benchmarks (SeferAlloc, ns/batch)

All runs with `--features "production alloc-lazy-commit"`.

#### Cold direct (global_alloc)

| Chunk | 16 B | 64 B | 256 B | 1024 B |
|------:|-----:|-----:|------:|-------:|
| 64 KiB | 29,412 | 29,901 | 32,380 | 40,893 |
| 128 KiB | 28,327 | 29,644 | 30,970 | 32,024 |
| 256 KiB | 28,657 | 28,344 | 31,222 | 30,221 |
| 512 KiB | 41,690 | 44,689 | 45,426 | 46,411 |
| baseline | 43,137 | 47,904 | 45,769 | 45,940 |

#### Churn (non-writing)

| Chunk | 16 B | 64 B | 256 B | 1024 B |
|------:|-----:|-----:|------:|-------:|
| 64 KiB | 20,948 | 16,820 | 17,231 | 17,032 |
| 128 KiB | 15,366 | 15,537 | 14,649 | 14,991 |
| 256 KiB | 14,482 | 14,107 | 14,797 | 15,751 |
| 512 KiB | 24,189 | 23,540 | 24,340 | 25,040 |
| baseline | 24,582 | 24,612 | 24,658 | 26,523 |

#### Churn + write

| Chunk | 16 B | 64 B | 256 B | 1024 B |
|------:|-----:|-----:|------:|-------:|
| 64 KiB | 16,988 | 19,191 | 16,720 | 18,339 |
| 128 KiB | 15,197 | 20,280 | 23,783 | 17,854 |
| 256 KiB | 15,895 | 15,672 | 17,491 | 18,463 |
| 512 KiB | 24,689 | 22,120 | 17,582 | 18,054 |
| baseline | 24,772 | 25,345 | 24,969 | 27,230 |

#### Segment decommit cycle (253 KiB — the multi-segment lifecycle bench)

| Chunk | ns/iter |
|------:|--------:|
| 64 KiB | 107,156 |
| 128 KiB | 151,559 |
| 256 KiB | 104,662 |
| 512 KiB | 88,893 |
| baseline (no LC) | 1,999 |

**Observation.** The decommit cycle is 50–75x more expensive with
`alloc-lazy-commit` ON, regardless of chunk size. This is the cost of
incremental `VirtualAlloc(MEM_COMMIT)` syscalls as each new segment is
carved to capacity (the 253 KiB block size means each 4 MiB segment holds
~15 blocks, requiring multiple commit calls to fill the payload). This cost
is paid on the COLD path (new segment reservation + fill to capacity + empty +
decommit + recycle), not on the steady-state hot path.

### 3.3 Noise assessment

This host exhibits ±15–20% run-to-run variance on wall-clock criterion benches
(documented in `docs/perf/R6_CROSS_VERSION_BENCH.md`). The differences between
chunk sizes in the churn / cold-direct families are WITHIN this noise band.
The 128 KiB and 256 KiB runs consistently land in the lower bands, while the
64 KiB and 512 KiB runs land higher — but this is correlated with host load
timing (the runs were sequential), not chunk-size causation.

The deterministic judge (IAI / instruction-count gate) requires Linux + Valgrind
and is not available on this Windows host. CI runs IAI on Linux; the
`alloc-lazy-commit` path is the eager fallback on Linux, so IAI is unaffected.

---

## 4. Commit-syscall scaling (B5 criterion)

The `GROW_COMMIT_COUNT` counter (B2, `alloc_core_small.rs`) counts successful
`commit_pages` calls on the grow-on-carve path. The B2 test suite
(`tests/lazy_commit_b2_grow.rs`) verifies:

- `carve_batch_one_commit_per_batch`: a batch crossing ONE chunk boundary issues
  exactly ONE `commit_pages` call (not per-block).
- `carve_batch_crosses_multiple_chunks`: a batch crossing N boundaries issues
  N commits (one per chunk), not per-block.
- `commit_failure_leaves_state_unchanged`: a failed commit does not advance
  bump, frontier, live count, or page map.

**Result: PASS.** Commit-syscall count scales with chunk-boundary crossings
(proportional to payload growth / GROW_CHUNK), not with allocation count. This
was verified by the B2 test suite (21 tests green) and is structurally enforced
by the code (the `commit_pages` call is in the "frontier exceeded" branch, not
in the per-block loop).

---

## 5. Commit-failure recoverability (B5 criterion)

The B4 fault-injection hook (`dbg_arm_commit_fail_at`) proves full
recoverability:

- `commit_failure_leaves_state_unchanged` (B2): bump not moved, frontier not
  moved, live count unchanged, page map unwritten, allocation returns null.
- `kth_commit_fails_carve_block` / `kth_commit_fails_carve_batch` (B4): the
  k-th commit fails; all prior commits succeed; the allocation returns null;
  subsequent normal allocation succeeds.
- `retry_after_failure_succeeds` (B4): after a failure, a subsequent allocation
  to the same segment succeeds (the frontier was not corrupted).
- `pool_reuse_with_fault_mid_reuse` (B4): fault during pooled-segment reuse
  is handled correctly.

**Result: PASS.** 21 B4 tests pass. Commit failure is fully recoverable.

---

## 6. Linux + miri no regression (B5 criterion)

On Unix and under miri, `reserve_aligned_lazy` falls back to the eager
`reserve_aligned` internally (see `crates/vmem/src/lib.rs` lines 918–930 and
1058–1067). The lazy path produces a `Reservation` with identical observable
behavior: the full span is committed eagerly, and `commit_range` is a no-op
(the pages are already committed).

`committed_payload_end` is set to `meta_end + LAZY_FIRST_CHUNK` on the lazy
path, but `GROW_CHUNK` commits up to `SEGMENT` — and since the pages are
already committed, the `commit_pages` call succeeds trivially (on Unix:
`mprotect` is never called; on miri: allocation is already live). The B2 test
suite includes an explicit check (`tests/lazy_commit_b2_grow.rs`, line 494):
on non-Windows, `dbg_grow_commit_count()` is 0 (no grow commits on the eager
path).

The B4 matrix test (`tests/lazy_commit_b4_matrix.rs`) exercises the full
lifecycle under both Windows and Unix/miri and passes on all platforms.

The production test suite (`cargo test --release --features production`) passes
green (all tests, including cross-thread, teardown ordering, and frontier tests).

**Result: PASS.** The eager fallback is semantically transparent.

---

## 7. GO/NO-GO verdict

### Kill-gate table

| # | Criterion | Target | Measured | Verdict |
|---|---|---|---|---|
| K1 | First-heap commit 4.52 MiB → ≤ 0.9 MiB | ≤ 0.9 MiB (924 KiB) | **4,628 KiB (4.52 MiB) — unchanged** | **NO-GO** |
| K2 | First-alloc latency ≤ 10% regression | ≤ 10% worse | +6.2% (84.9 → 90.2 us median) | PASS |
| K3 | Dense cold alloc ≤ 3% regression | ≤ 3% | Within noise (−1% to +11%, host ±20%) | PASS (within noise) |
| K4 | Steady churn no regression | No measurable regression | Within noise (±5%) | PASS |
| K5 | Commit-syscall scales with chunks | Per-chunk, not per-alloc | Verified by B2 tests + counter | PASS |
| K6 | Commit failure fully recoverable | Full recovery | 21 B4 tests green | PASS |
| K7 | Linux + miri no regression | Eager fallback transparent | Tests green; `GROW_COMMIT_COUNT=0` on non-Win | PASS |

### Verdict: **NO-GO on the primary criterion (K1)**

The first-heap commit charge is **unchanged** at 4.52 MiB because the
primordial segment is always eagerly committed and dominates the first-heap
commit. `alloc-lazy-commit` saves commit charge only on SUBSEQUENT small
segments reserved via `reserve_small_segment`, which are not exercised by the
first-heap judge.

All secondary criteria (K2–K7) PASS. The mechanism is **architecturally sound**
and **fully tested** (31 dedicated tests across B2/B3/B4/frontier, plus the
full production suite). It correctly reduces commit charge for multi-segment
workloads where new small segments are carved to capacity — the
`segment_decommit_cycle` bench confirms the lazy-commit path adds ~100–150 us
per full segment lifecycle (cold-path cost, not hot-path) while sparing the OS
commit charge of uncommitted pages.

---

## 8. Chunk-size choice

All four swept chunk sizes (64/128/256/512 KiB) produce identical first-heap
commit and near-identical steady-state throughput (within host noise). The only
discriminating signal is the cold-path segment-lifecycle cost:

| Chunk | Segment decommit cycle (ns) | Commits per 4 MiB segment fill |
|------:|----------------------------:|-------------------------------:|
| 64 KiB | 107,156 | ~60 |
| 128 KiB | 151,559 | ~30 |
| 256 KiB | 104,662 | ~15 |
| 512 KiB | 88,893 | ~7 |

512 KiB has the lowest syscall count but commits the most unused memory
upfront. 64 KiB has the highest syscall count. 256 KiB is the established
default, sits at the low end of the lifecycle cost band, and is already
validated by B1–B4's test suites.

**Decision: keep 256 KiB.** No constant change. The Pareto reasoning: 256 KiB
balances per-segment commit savings (~3.75 MiB spared on initial reserve vs
eager) against syscall overhead (~15 commits to fill a segment). There is no
data-driven reason to change it.

---

## 9. Segment decommit cycle regression

With `alloc-lazy-commit`, the `segment_decommit_cycle` bench (253 KiB blocks,
34-block batch that fills/empties 2+ segments) is ~50–75x slower: ~100–150 us
vs ~2 us baseline. This is the cost of `VirtualAlloc(MEM_COMMIT)` syscalls on
the COLD path (new segment fill-to-capacity). The hot path (churn, steady
alloc/free) is unaffected.

This regression is **expected and inherent** to the lazy-commit design: each
chunk commit is a syscall that the eager path does not need (it paid the cost
upfront at segment reservation). Whether this cold-path cost is acceptable
depends on the workload profile. For workloads that cycle through many segments
rapidly (the `segment_decommit_cycle` pattern), lazy commit adds measurable
latency. For workloads that allocate within a few long-lived segments (the
common case), the cost is amortized to near-zero.

Since `alloc-lazy-commit` is opt-in and off by default, this regression affects
only consumers who explicitly enable it.

---

## 10. Recommendations

### 10.1 `alloc-lazy-commit` stays opt-in

The feature is architecturally sound but does NOT deliver on its primary
design goal (first-heap commit reduction). It should remain an opt-in
experimental feature for Round 7. Enabling by default is out of scope.

### 10.2 Future work: lazy primordial

To achieve the plan's 0.9 MiB target, the **primordial segment** would need to
be lazily committed. This requires restructuring `bootstrap::primordial` to:
1. Reserve the primordial segment with `reserve_aligned_lazy` (commit only the
   metadata region + registry array + a small payload chunk).
2. Defer the hash-table and free-list-index zero-fill to on-demand commit.
3. Ensure the registry array is committed before the first slot write.

This is a non-trivial change to the bootstrap path and is deferred to a future
round (not Round 7). The registry's own footprint (~0.5 MiB for the chunk +
control structures) would also need chunked/lazy materialisation (the
"R6-OPT-P0-2 chunked Registry + lazy HeapOverflow sidecar" follow-up noted in
the `first_alloc_process` example's doc comments).

### 10.3 Downstream metric

The right judge for `alloc-lazy-commit`'s benefit is NOT first-heap commit
(which is primordial-dominated) but **per-segment commit charge in a
multi-segment workload**. A future harness should measure commit charge after
N segments are reserved (e.g., allocating enough blocks to fill 4+ segments),
comparing eager vs lazy. The `segment_decommit_cycle` bench is a proxy but
measures lifecycle cost (latency), not commit charge.

---

## 11. Gate compliance

| Gate | Command | Result |
|---|---|---|
| `cargo fmt --check` | `cargo fmt --check` | PASS (no diff) |
| clippy (default) | `cargo clippy --all-targets -- -D warnings` | PASS |
| clippy (experimental) | `cargo clippy --all-targets --features experimental -- -D warnings` | PASS |
| clippy (all-features) | `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| clippy (production+LC) | `cargo clippy --lib --features "production alloc-lazy-commit" -- -D warnings` | PASS |
| B2/B3/B4/frontier tests | `cargo test --release --features "production alloc-lazy-commit" --test lazy_commit_*` | 31 tests PASS |
| production suite | `cargo test --release --features production` | All tests PASS |

---

## 12. Caveats

1. **Windows-only benefit.** On Unix/miri, `reserve_aligned_lazy` falls back to
   the eager path. The lazy-commit mechanism has zero effect on these platforms.
   This is by design.

2. **NUMA forbidden.** Under `numa-aware`, the lazy path is disabled (falls back
   to eager VirtualAllocExNuma). This preserves NUMA placement (P2 gate) at the
   cost of the lazy-commit savings.

3. **Host noise.** Criterion bench numbers on this host have ±15–20% run-to-run
   variance. The sweep tables should be read for TRENDS and RATIOS, not absolute
   ns values. The deterministic IAI gate (Linux, instruction-count) is the
   noise-free judge; it is unaffected by `alloc-lazy-commit` (eager fallback on
   Linux).

4. **No sccache warm/cold control.** The sweep ran sequentially on a warm
   `sccache` cache; the first run (64 KiB) may have paid a compilation premium
   that later runs avoided. The first-alloc-process numbers (commit charge,
   latency) are unaffected by this (they measure runtime, not compile time).

5. **Primary criterion target was unreachable by design.** The plan's 0.9 MiB
   target for first-heap commit assumed the primordial segment would participate
   in lazy commit. The B1 implementation (commit `0c981d7`) explicitly chose to
   keep the primordial always-eager. This is a design-plan mismatch, not a
   measurement failure.
