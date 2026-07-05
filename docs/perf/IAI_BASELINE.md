# iai-callgrind Ir baseline

Deterministic instruction-count (`Ir`) baseline for `benches/perf_gate_iai.rs`,
the reference future perf work (e.g. W4 `carve_batch`) diffs against.

- **Commit:** post-W3 (`alloc-stats` gating; the W2 tombstone-rebuild is
  Ir-neutral). Original baseline was `4e139b0`; W3 gated the per-hit stats bump
  out of `production`, moving every hit-heavy bench BELOW the original baseline
  (small_churn −59, cold_16b −236, recycle_16b −477). W4 (`carve_batch`) diffs
  against THIS table.
- **Features:** `production` (`alloc-global` + `alloc-xthread` + `alloc-decommit`
  + `fastbin`) — the same set the CI perf-gate benches with, so these numbers
  match `.github/workflows/perf-gate.yml`. (`stats` counters are OFF in
  `production`; add `alloc-stats` to restore them at ~+59/+236/+477 Ir.)
- **How to reproduce:** `npm run iai` (from repo root). Drives the Linux-only
  bench through WSL under `valgrind --tool=callgrind` (`scripts/iai.mjs`).
- **Runner:** `iai-callgrind-runner 0.14.2` in WSL (pinned `^0.14`, matching
  `iai-callgrind = "0.14"` in `Cargo.toml`); valgrind 3.22.0.
- **Determinism:** `Ir` is callgrind instructions-retired — deterministic
  run-to-run on the same binary+input. Two back-to-back full runs produced
  BYTE-IDENTICAL `Ir` for all 10 benches. That determinism is what makes this a
  judge on this Windows dev host (wall-clock is noise; `Ir` is not).

## Baseline (Ir per bench function)

| bench function              |         Ir |
| --------------------------- | ---------: |
| small_churn_16b             |     81,170 |
| aligned_churn_640b_a128     |     81,049 |
| large_alloc_free_cycle      |     72,345 |
| realloc_grow                |  1,521,067 |
| cold_alloc_free_256x16b     |    129,863 |
| cold_alloc_free_256x64b     |    129,373 |
| recycle_alloc_free_256x16b  |    182,150 |
| recycle_alloc_free_256x64b  |    181,678 |
| churn_256b                  |     81,045 |
| churn_write_256b            |     81,173 |

## W4 result (E1 `carve_batch` + E3 batched `dec_live`; E2/E4 rejected)

E1 (`AllocCore::carve_batch` — one hoisted `align_up` div / bump load-store /
`live += n` / `is_decommitted` check / per-distinct-page marking per refill
run, replacing the per-block `carve_block` loop in `refill_class_bump`; plus
removal of the post-`free_exhausted` redundant `drain_freelist_batch` re-read)
and E3 (one `sub_live(k)` + single decommit check in `flush_run`, replacing the
per-accepted-block loop). E2 (`REFILL_N` const LUT) was REJECTED — the `[u16;49]`
load REGRESSED cold +32 / recycle +62 vs the inlined `udiv`, so it was reverted.
E4 (heap_core branch-fold) was DROPPED — the two sites sit in separate cfg
regions with fall-through semantics, not a self-contained fold; churn is already
at/below baseline and the risk to the won front is not worth a speculative
−1 branch.

| bench function              |   baseline |    W4 (E1+E3) |     delta |
| --------------------------- | ---------: | ------------: | --------: |
| cold_alloc_free_256x16b     |    129,863 |       123,516 |    −6,347 |
| cold_alloc_free_256x64b     |    129,373 |       123,023 |    −6,350 |
| recycle_alloc_free_256x16b  |    182,150 |       175,896 |    −6,254 |
| recycle_alloc_free_256x64b  |    181,678 |       175,418 |    −6,260 |
| small_churn_16b             |     81,170 |        80,797 |      −373 |
| churn_256b                  |     81,045 |        80,672 |      −373 |
| churn_write_256b            |     81,173 |        80,800 |      −373 |

Cold dropped ~6.3k Ir (the target); recycle also dropped ~6.3k (the post-latch
redundant-drain removal helps it too — better than the "neutral" bar); churn is
UNREGRESSED (slightly below baseline). All correctness pins green; carve_batch
is byte-identical to the per-block carve loop (M2 untouched — carve never writes
the bitmap; D1 exact +n; page-dedication "first class wins"; SEGMENT boundary
check; decommit recommit-on-reuse), verified by `tests/regression_carve_batch.rs`.

## W7a note (generation→u64 + TaggedPtr repack)

W7a widened `HeapSlot::generation` to `AtomicU64` (byte-identical Ir — the field
is written only on cold claim/recycle and compared only `== 1`) and repacked
`TaggedPtr` to `index:16 | tag:48` (all call sites are cold: bootstrap /
claim / recycle — none on the alloc/free hot loop). The repack shifts EVERY
bench by a uniform **−4 Ir** — a *decrease*, from the one-time bootstrap
`free_slots = TaggedPtr::empty()` store whose constant shrank from
`0x0000_0000_FFFF_FFFF` to `0x0000_0000_0000_FFFF` (a cheaper immediate that
falls inside each bench's first-claim window). Not a hot-path touch; accepted.
The current post-W7a table is the W4 table above minus 4 on every row
(e.g. `cold_alloc_free_256x16b` = 123,512; `small_churn_16b` = 80,793).
