# G1-K1 — pre-registration (fixed before any J1′ v2 run of this variant)

Base: `main` @ `37780bc7`. Measurement branch `g1-k1-meas` (harness commit
cherry-picked from `b1-flush-free` measurement branch `51eda7b1`).
Variant (patch in `docs/perf/b4_ablate/patches/`):

- `k1_drain_no_reissue` — `try_drain_freelist_batch` no longer calls
  `IssueRun::prepare_for` / `commit` per drained block, i.e. it no longer
  re-stores the block's `ClassLeaves` class code. Every block on a class-`c`
  bin was issued as class `c` before it was freed (flush, reclaim and
  `dealloc_small` accept only such blocks), its code `c + 1` was stored at that
  issue, leaves are never dropped or reset while the route lives, and bins
  are emptied on decommit — so the store wrote the value already there. The
  once-per-batch route guards of `prepare_small_issue_run` stay; a debug
  witness asserts the code is present. Carve (the first issue) is unchanged.
  Contract change: drain no longer fails on a leaf spill OOM (it promotes no
  leaf).

Callgrind (WSL, `b4_pmu_probe`, whole process Ir, ИЗМЕРЕНО before this
file; base `g1-pmu`@`dfb89621`, variant `g1-k1`@`c0c4eacf`):
bulk16k 95 891 733 → 85 698 115 (−10.6 %), mixed 256/1024 54 412 117 →
49 528 816 (−9.0 %); mixed 16/64, bulk64, pair16 unchanged (±0.02 %).
perf stat cycles/op (separate runs, not a judge): bulk16k 26.873 → 25.343,
mixed 256/1024 32.059 → 29.658. Instructions are not time; the verdict is
the time judge.

Protocol: J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
blocks, all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap.

Thresholds (same as B1, B2):

1. Win: bulk16k effect CI upper < 0.
2. No harm: pair16, pair64, larson effect ≤ +1 % and CI upper ≤ +1.5 %.
3. No harm, other kernels: not (CI lower > 0 AND effect > +2 %).

GO = 1 ∧ 2 ∧ 3. Path activation: bulk frees 16 384 blocks per pass and
reallocates them, so every bulk16k refill drains the bin by construction;
callgrind shows the drain's `route_slots.rs` self Ir going from 3 458 888 to
0 on bulk16k. Second measurement, against mimalloc: main judge J1′ v2
(`scripts/b4_interleave_run.mjs --reps 20 --blocks 2000`) on `g1-k1`.
