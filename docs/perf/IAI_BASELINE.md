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

## Pre-X-arc full baseline (2026-07-05)

Fresh full-run snapshot taken before the perf/correctness X-arc (#182–187).
Confirms **zero drift since W7a** — every `Ir` is byte-identical to the post-W7a
values above (small_churn 80,793 · cold 123,512 · recycle 175,892 · realloc_grow
1,520,714). This is the reference the X-arc diffs against; X1 (in-place Large
realloc) targets `realloc_grow`.

This run also records the FULL callgrind metrics — L1/L2/RAM hits and
`Estimated Cycles` (`= L1 + 5·L2 + 35·RAM`, callgrind's default model). These
are what X3's cache-aware judge upgrade will diff against, since `Ir` counts a
`udiv` and a cache-missing load identically (both = 1 instruction) while cycles
do not. Numbers are deterministic run-to-run (callgrind).

| bench function              |        Ir |    L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ---------: | ------: | -------: | ----------: |
| small_churn_16b             |    80,793 |    141,802 |      64 |    5,201 |     324,157 |
| aligned_churn_640b_a128     |    80,677 |    141,690 |      64 |    5,201 |     324,045 |
| large_alloc_free_cycle      |    72,341 |    131,665 |      62 |    5,206 |     314,185 |
| realloc_grow                | 1,520,714 |  3,751,251 |  45,317 |   92,240 |   7,206,236 |
| cold_alloc_free_256x16b     |   123,512 |    193,440 |     107 |    5,277 |     378,670 |
| cold_alloc_free_256x64b     |   123,019 |    192,766 |     109 |    5,459 |     384,376 |
| recycle_alloc_free_256x16b  |   175,892 |    255,667 |     109 |    5,281 |     441,047 |
| recycle_alloc_free_256x64b  |   175,414 |    254,996 |     111 |    5,475 |     447,176 |
| churn_256b                  |    80,668 |    141,676 |      64 |    5,202 |     324,066 |
| churn_write_256b            |    80,796 |    141,929 |      64 |    5,205 |     324,424 |

**`realloc_grow` is the outlier the X-arc exists for:** 92,240 RAM hits and
45,317 L2 hits vs ~5.2k / ~100 for every other bench — memcpy floors made
visible. Its Estimated Cycles (7,206,236) is ~22× any other bench, a WIDER gap
than the 19× seen in `Ir` alone: the cache-miss cost of the copies is invisible
to `Ir` but real in cycles. X1 (in-place growth within the already-mapped
`span_usable`) removes the copy entirely on the fits-in-span path, so the cycle
metric should collapse even harder than `Ir`.

## Post-X1+X2+X3 reference (2026-07-05) — the CURRENT 11-bench table

After X1 (OPT-G in-place Large realloc, `754eee5`), X2 (#164 drain-side
magazine check, `7441dcc`) and X3 (judge upgrade: cache columns in
`scripts/iai.mjs` + the new `multiseg_cold_256k` bench). **Future diffs are
taken against THIS table** — adding the 11th bench fn shifted every old
bench's Ir via pure binary layout (identical benches shifted identically:
both cold +1,160, both recycle +2,066, small_churn/churn_256b both +321;
`large`/`realloc_grow` only +2), so the 10-bench numbers above are retired
as reference points.

| bench                       |        Ir |    L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ---------: | ------: | -------: | ----------: |
| small_churn_16b             |    81,396 |    142,717 |     160 |    5,217 |     326,112 |
| aligned_churn_640b_a128     |    81,405 |    142,728 |     160 |    5,219 |     326,193 |
| large_alloc_free_cycle      |    72,984 |    132,617 |     158 |    5,220 |     316,107 |
| realloc_grow                |   561,912 |  1,173,997 |   3,973 |   74,963 |   3,817,567 |
| cold_alloc_free_256x16b     |   125,215 |    195,546 |     172 |    5,323 |     382,711 |
| cold_alloc_free_256x64b     |   125,218 |    195,369 |     174 |    5,504 |     388,879 |
| recycle_alloc_free_256x16b  |   179,018 |    260,572 |     170 |    5,329 |     447,937 |
| recycle_alloc_free_256x64b  |   179,021 |    260,383 |     172 |    5,522 |     454,513 |
| churn_256b                  |    81,396 |    142,717 |     160 |    5,217 |     326,112 |
| churn_write_256b            |    81,524 |    142,972 |     160 |    5,218 |     326,402 |
| multiseg_cold_256k          |   111,642 |    189,151 |     184 |    5,514 |     383,061 |

**The X-arc headline, in both judges:** `realloc_grow` Ir 1,520,714 →
561,912 (**−63%**, X1 in-place growth + X2's magazine-routed realloc
alloc-leg) and Estimated Cycles 7,206,236 → 3,817,567 (**−47%**; RAM hits
92,240 → 74,963 — the memcpy floors are gone on every fits-in-span step).
X2's accepted documented costs (see commit `7441dcc`): +~630 Ir one-time
bootstrap per heap claim and ~+30 Ir per refill-miss; hot magazine push/pop
untouched (churn per-op below the pre-X2 baseline once the one-time constant
is excluded). `multiseg_cold_256k` (new, X3) is the designated judge for X5
(per-class segment queues): 34 × 256 KiB allocations span 3 segments and the
second round's refills walk all three via `find_segment_with_free`.
