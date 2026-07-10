# Performance review — churn/reuse-path size-dependent degradation (2026-07-10)

**Scope:** why SeferAlloc's churn (free-then-realloc-same-slot) benchmark
degrades far more sharply with block size than mimalloc's. **Method:** fxx
(Fable-5, effort=max) research agent, read-only investigation plus an
isolated reproduction in a scratch project (`D:\tmp\sefer-probe`, outside
the repo — no repo files modified) and a criterion feature-bisect
(`--features production` vs `--features fastbin`) run live during
investigation. No repo files were modified; no findings below have been
implemented.

## Trigger

Fresh wall-clock benchmarks (this session) showed a size-dependent
asymmetry between cold-direct and churn:

| Size | Cold-direct Sefer | Cold-direct mimalloc | Churn Sefer | Churn mimalloc |
|---|---:|---:|---:|---:|
| 16B | 36.3ns | 18.0ns | 37.0ns | 25.8ns |
| 1024B | 47.5ns | 54.0ns | 224.3ns | 86.8ns |

At 1024B, SeferAlloc's cold-direct number actually **beats** mimalloc
(47.5ns vs 54.0ns), but under churn it balloons to 224.3ns (4.7x its own
cold-direct number) while mimalloc only grows 1.6x. Something in the
reuse path specifically, and specifically at larger block sizes, is
expensive.

## Root cause (confirmed, not hypothesized)

**The blow-up is not in the magazine/freelist reuse path.** The actual
free→realloc-same-slot steady-state loop measures **29-30 ns/op at
1024B** (flat across all sizes measured, faster than mimalloc). The
entire 224ns anomaly is the **zero-hysteresis empty-small-segment
decommit→release→re-reserve lifecycle** (`alloc-decommit`, part of the
`production` feature bundle) interacting with the benchmark's timed
working-set teardown.

### Evidence chain

1. **Feature bisect** (criterion, same bench): `--features production` →
   1024B churn ≈ 162ns/op (noisy 132-181); `--features fastbin` (identical
   minus `alloc-decommit`) → **44.5ns/op, flat across 16B-1024B**. The
   blow-up vanishes exactly when `alloc-decommit` is removed.
2. **Isolated probe** (`D:\tmp\sefer-probe`, production features): one
   persistent working set → 12-22ns/op at all sizes, **0 decommits**. The
   reuse path itself is size-independent.
3. **Criterion-shape probe** (N=64 pre-allocated working sets per batch,
   mimicking `iter_batched(SmallInput)`): reproduces the anomaly exactly —
   1024B: 208-246ns/op with 20 decommit + 20 release + 20 re-reserve
   cycles; 16/64/256B: 29-49ns/op with **0** decommits (their footprint
   stays inside the primordial segment, which is decommit-exempt).
4. **Phase split at 1024B:** churn loop 29.5ns/op; **teardown 715ns per
   free** (vs 27ns at 16B, 55-62ns at 256B). 256 teardown frees × 715ns ≈
   183µs/iteration — the entire reported inflation.
5. **iai cross-check:** `recycle_alloc_free_256x16b` Ir/op ≈ `cold` Ir/op
   (205.3 vs 204.5, near-identical) while wall-clock diverges 4.7x at
   1024B — the deterministic instruction-count judge is **structurally
   blind** to this cost class (it doesn't count page faults or syscalls).

## Ranked findings

### 1. Zero-hysteresis decommit+release of empty small segments (root cause) — HIGH confidence, MEDIUM risk to fix

- **File:** `src/alloc_core/alloc_core.rs:1070` (`dec_live_and_maybe_decommit`)
  and `:1121` (batch variant) → `decommit_empty_segment_for_release`
  (`:1282`) → `SegmentTable::recycle` (`segment_table.rs:381`) →
  `os::release_segment` (`MEM_RELEASE`/`munmap` of the whole 4MiB
  reservation), also reached via `:3447-3449` (`dealloc_small`) and
  `:2836-2841` (ring-drain in `find_segment_with_free_impl`).
- The moment `live_count` hits 0 on a non-current Small segment, the entire
  reservation goes back to the OS. Any workload whose live set oscillates
  across a segment boundary pays, per oscillation: release syscall + re-
  reserve + metadata re-init + a full set of demand-zero page faults on
  next use. Larger blocks → more pages per block → cost scales with block
  size; 16B never leaves the primordial segment so never triggers it.
- **Fix direction:** Mechanism-2 hysteresis pool (already anticipated as
  PERF-4's deferred next step — see
  `docs/checkpoints/2026-07-09-perf4-decommit-mechanism1-fix.md`): keep the
  last N empty *committed* small segments, mirroring the existing
  `large_cache` (8 slots + byte budget + lazy decay,
  `alloc_core.rs:73`, `maybe_decay_large_cache` `:3772`). **Critical:** the
  pool must keep pages committed, not decommit-then-cache — decommitted
  pages re-fault on next touch, which is most of the measured cost.
  mimalloc's equivalent is its ~10ms purge delay.
- **Projected effect:** the `fastbin`-only run is the "infinite hysteresis"
  upper bound: 1024B churn → ~45ns/op, i.e. ~2x **faster** than mimalloc's
  86.8ns, versus 2.6x slower today.

### 2. First-touch page fault taken on the FREE path via `flush_run`'s `write_next` — HIGH confidence, MEDIUM risk

- **File:** `src/alloc_core/alloc_core.rs:2390` (`Node::write_next` inside
  `flush_run`), reached from magazine overflow flush
  (`heap_core.rs:1092`, `flush_class`, batches of `FLUSH_N = 8`).
- A freed block's body is written (freelist link) even when the block
  lives in a never-touched virgin page. The allocator faults in pages
  purely to store metadata that — when the flush empties the segment — is
  immediately discarded by the decommit reset. Dead work amplified by
  finding 1's recurrence.
- **Fix direction:** finding 1's fix removes the recurrence (pages stay
  warm) — the pragmatic near-term answer. Longer-term, this reopens (with
  new evidence) the case for PERF-3.5 (run-encoded freelist), whose prior
  NO-GO verdict was rendered by an Ir-only judge on primordial-resident
  benches where avoiding block-body writes has no fault/RSS payoff. Do
  **not** re-litigate PERF-3 until finding 1 lands and a fault-aware judge
  exists (see finding 3).

### 3. The deterministic judge (iai/callgrind) cannot see this defect class — HIGH confidence, LOW risk (tooling only)

- **File:** `benches/perf_gate_iai.rs` — all benches ≤256 blocks of ≤64B
  live inside the primordial segment; `seg_cycle_decommit_256k` exercises
  decommit but Ir doesn't price syscalls/faults.
- The PERF-4 checkpoint explicitly gates Mechanism-2 on "measure via `npm
  run iai` first" — but Ir/op for recycle vs cold is near-identical while
  wall-clock diverges 4.7x. The go/no-go for the hysteresis pool **must**
  use a wall-clock, fault-bearing harness.
- **Fix direction:** add a `working_set_cycle` criterion bench (multi-
  working-set batched-teardown shape, matching the criterion-shape probe
  above) as the canonical Mechanism-2 judge; optionally record
  `dbg_decommit_count`/`dbg_segments_released_total` deltas in the bench
  output as a regression oracle.

### 4. Bench-interpretation defect: reported "churn" number is ~85% teardown at 1024B — HIGH confidence, LOW risk

- **File:** `benches/global_alloc.rs:118-125` (`churn_teardown` runs inside
  the timed `iter_batched` routine — already documented this session as a
  known ~20% skew, task #24, but at 1024B under decommit it's not 20%,
  it's ~183µs of the ~208µs region).
- The table's "churn ns/op" for larger sizes reports mostly segment-
  lifecycle teardown cost, not reuse-path cost. Sefer's actual free-then-
  realloc-same-slot path is already faster than mimalloc's at every size
  measured (29.5ns vs mimalloc's implied per-op).
- **Fix direction:** make teardown genuinely untimed (return a drop-guard
  from the routine, per the existing doc comment's own follow-up note)
  *and* keep one bench where it is timed intentionally (that's the
  Mechanism-2 signal — finding 3). Report both.

### 5. `production` bundles `alloc-decommit` with zero hysteresis — MEDIUM confidence (product decision), LOW risk

- **File:** `Cargo.toml:163` (`production = [..., "alloc-decommit", ...]`).
- This is the same mechanism flagged by the prior shamir-db 47-target sweep
  (0.3.0 ~15-18% slower than 0.2.1, which had no decommit). Every "many
  short-lived segments cycling" workload pays it. RSS-friendliness is
  real, but currently purchased at a fault-storm price on oscillating
  workloads.
- **Fix direction:** once finding 1's pool lands, `production` keeps
  decommit + pool defaults (e.g. 4-8 segments / 16-32MiB budget / decay
  tick reusing the large-cache decay pattern). An interim escape hatch
  (documented config knob for the small-segment pool size, 0 = current
  behavior) keeps the change reviewable and off-by-default-able.

## Summary recommendation

Implement PERF-4 Mechanism-2 as a **committed-segment hysteresis pool for
empty small segments**, structurally mirroring the existing 8-slot large
cache (budget + lazy decay), wired in at the three `table.recycle(base)`-
on-decommit sites and consulted by `reserve_small_segment` before going to
the OS — keeping pages committed so no recommit and no re-fault occurs on
reuse. The reuse path itself is already 29-30ns/op at 1024B (beating
mimalloc); the entire 224ns/op churn gap is 20 decommit→release→re-
reserve→virgin-fault cycles per 320 iterations, with teardown frees at
715ns each. The `fastbin`-only bound shows ~45ns/op (~2x faster than
mimalloc at 1024B) is achievable. Judge with a wall-clock multi-working-
set bench plus `dbg_segments_released_total` deltas — not the Ir-only iai
judge, which is provably blind to this cost class.
