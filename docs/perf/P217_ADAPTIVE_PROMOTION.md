# P217 — where Sefer still loses to mimalloc, and `large-cache-adaptive` promoted into `production`

**Kind:** `perf(runtime)` — the `production` feature composition changes (it gains `large-cache-adaptive`,
which pulls in `large-cache-extended`). Sections 1–3 are measurement only; section 4 is the evidence for
the promotion. Task #217, 2026-11-25.

## 0. Summary

1. On main `4e1f0b6e` the wild judge (`examples/g1_wild_judge.rs`, Windows) still shows mimalloc ahead in
   `regrow` (1.247), `growmix` (1.159), `wild` (1.250) and `large` (1.401); `grow64`, `grow1k` and `phase`
   are at parity (section 1).
2. The time decomposition (section 3) places the losses in two places: the scattered free of small blocks
   at teardown (`growmix` 1040.8 ms of the 1166.6 ms difference, kernel time only +31.1 ms, so user-mode
   free work; `regrow` teardown +541.7 ms) and the Large free path (`large`: Large free +6.5 ms,
   teardown +2.1 ms of +9.0 ms).
3. The existing opt-in `large-cache-adaptive` (J14) removes the `large` loss: x/base 0.655 (0.602–0.712),
   x/mi 0.952 (0.803–1.128), parity with mimalloc; `growmix`, `regrow` and `wild` do not move (section 4).
   Its cost is retention: peak RSS in `large` 13 → 23 MiB (mimalloc 28), in `wild` 42 → 56 MiB (mimalloc 49).
4. Decision (owner, delegated, 2026-11-25): accept J14 — including campaign 2's one unreadable Windows A/A
   cell, `docs/perf/J14_ADAPTIVE_LARGE_CACHE_C2.md` — and promote it into `production`. An explicit
   `budget_bytes` stays a hard cap.
5. Not addressed here: the small-block scattered free (`growmix`, `regrow`, the small part of `wild`).
   It stays open as the next lever in `docs/perf/OPEN_ITEMS.md` item 146.

## 1. Loss map on main

Every number in sections 1–4 is printed by `node scripts/p217_report.mjs` from the raw files in
`docs/perf/p217/`; the summary CSV is `docs/perf/P217_ADAPTIVE_PROMOTION_summary.csv`
(`node scripts/p217_report.mjs --csv docs/perf/P217_ADAPTIVE_PROMOTION_summary.csv`).

Estimator (LM-1): per block the minimum of 3 repeats of `ns_per_op` per arm, the per-block log ratio,
exp of the mean with a 95 % t interval over 12 blocks. Class: LOSS if the interval is above 1, WIN if
below, otherwise PARITY. Both arms run in the same binary: `sefer` is a `static SeferAlloc` called through
its `GlobalAlloc` implementation (the face a `#[global_allocator]` uses), `mi` is `mimalloc::MiMalloc`, so
the layer measured is the one `production` ships.

| scenario | blocks | sefer/mi (95 % CI) | class | peak RSS MiB sefer/mi | end RSS MiB sefer/mi |
|---|---:|---|---|---|---|
| grow64 | 12 | 0.975 (0.845–1.125) | PARITY | 1235/1159 | 28/182 |
| grow1k | 12 | 1.031 (0.916–1.161) | PARITY | 1114/1038 | 28/1030 |
| regrow | 12 | 1.247 (1.057–1.472) | LOSS | 646/588 | 68/572 |
| growmix | 12 | 1.159 (1.076–1.248) | LOSS | 1277/1160 | 1254/1137 |
| wild | 12 | 1.250 (1.081–1.444) | LOSS | 42/50 | 26/49 |
| phase | 12 | 1.001 (0.865–1.159) | PARITY | 182/168 | 165/124 |
| large | 12 | 1.401 (1.124–1.745) | LOSS | 13/28 | 7/28 |

## 2. Multi-thread

`examples/malloc_macro.rs`, 6 runs, median throughput; time ratio = mi Mops / sefer Mops.

| cell | sefer Mops | mi Mops | sefer/mi time |
|---|---:|---:|---:|
| larson T=1 | 16.10 | 14.79 | 0.919 |
| larson T=2 | 25.63 | 21.84 | 0.852 |
| larson T=4 | 2.63 | 1.96 | 0.744 |
| mstress T=1 | 16.58 | 8.26 | 0.498 |
| mstress T=2 | 2.77 | 23.88 | 8.605 |
| mstress T=4 | 4.18 | 4.48 | 1.073 |

Only the T=1 cells are readable: under the 10 % CPU cap (section 5) several threads share a throttled
quota, which collapses absolute throughput (larson T=4 at 2–3 Mops) and makes the T>1 cells scheduler
artefacts. No conclusion is drawn from T>1.

## 3. Where the time goes

`WJ_PROF=1` (the WILD-1 method, `docs/perf/WILD1_GAP_DECOMPOSITION.md`): per block and arm the repeat with
the smallest wall time, per bucket the mean difference sefer − mi with a 95 % t interval over 12 blocks.
`teardown` is the final `free_all` of every live block (shuffled order in `growmix`). The buckets and the
rest partition the wall time (asserted by the script). Buckets with no time in either arm are omitted.

**large** (kernel-time difference 2.6 ms)

| bucket | sefer ms | mi ms | diff ms (95 % CI) |
|---|---:|---:|---|
| large_alloc | 13.0 | 13.6 | -0.4 (-2.5..1.6) |
| large_free | 7.3 | 1.0 | 6.5 (5.4..7.6) |
| teardown | 2.0 | 0.1 | 2.1 (1.6..2.5) |
| rest | 2.2 | 1.6 | 0.8 (0.3..1.3) |
| wall | 24.2 | 16.2 | 9.0 (5.2..12.7) |

**regrow** (kernel-time difference 80.7 ms)

| bucket | sefer ms | mi ms | diff ms (95 % CI) |
|---|---:|---:|---|
| teardown | 1122.3 | 554.1 | 541.7 (337.6..745.9) |
| rest | 2313.9 | 2330.5 | -279.6 (-579.1..19.8) |
| wall | 2976.7 | 2636.3 | 262.1 (91.4..432.7) |

**growmix** (kernel-time difference 31.1 ms)

| bucket | sefer ms | mi ms | diff ms (95 % CI) |
|---|---:|---:|---|
| teardown | 2365.9 | 1219.9 | 1040.8 (806.0..1275.7) |
| rest | 3445.8 | 3223.2 | 125.7 (-239.2..490.7) |
| wall | 6087.5 | 4678.0 | 1166.6 (630.5..1702.6) |

**wild**: unreadable under the cap — the burst, Large alloc/realloc and rest intervals span several
hundred ms around zero; only Large free (+8.7, 6.8..10.6 ms), teardown (+6.1, 3.9..8.3 ms) and small
realloc (+0.5, 0.1..0.8 ms) are resolved (full table in the script output).

Reading: `large` loses in Large free — the 8-slot Large cache releases spans to the OS that the scenario
asks for again (section 4, `seg_released`); `growmix` and `regrow` lose in the teardown
free of small blocks, almost entirely user time. The second is not touched by this change.

## 4. A/B: `large-cache-adaptive`

Same commit `4e1f0b6e`, two builds of `g1_wild_judge`:

- base: `--features "production internals"`, sha256 `c9b60022ee9c8fc2faa99c18efd203ab163e8489a209ca9b118b6e3eb618839d`;
- x: `--features "production internals large-cache-adaptive"`, sha256 `8228f50d41a4348f9015ddc7037c4d62540963cb674659c574b08b8fc1e9603e`;
- the mi arm runs from the base binary.

Runner `docs/perf/p217/run_ab.mjs`: one process per (block, scenario, arm, repeat), scenario order shuffled
per block, arm order rotated per block, 12 blocks × 3 repeats. Same estimator as section 1.

| scenario | x/base | base/mi | x/mi | seg_released base/x (median) | peak RSS MiB base/x/mi | end RSS MiB base/x/mi |
|---|---|---|---|---|---|---|
| growmix | 1.011 (0.919–1.112) | 1.270 (1.105–1.460) | 1.284 (1.143–1.443) | 0/0 | 1277/1277/1160 | 1254/1254/1137 |
| regrow | 1.038 (0.977–1.103) | 1.269 (1.169–1.378) | 1.318 (1.164–1.492) | 140/140 | 646/646/588 | 68/68/572 |
| large | 0.655 (0.602–0.712) | 1.454 (1.268–1.668) | 0.952 (0.803–1.128) | 589/165 | 13/23/28 | 7/12/28 |
| wild | 1.019 (0.886–1.172) | 1.220 (1.019–1.461) | 1.243 (1.019–1.515) | 595/227 | 42/56/49 | 26/34/49 |

Mechanism evidence (path activation): `seg_released` — segments returned to the OS — falls from 589 to
165 in `large` and from 595 to 227 in `wild`, i.e. the x arm keeps spans the base arm releases; it is
unchanged in `growmix`/`regrow`, where the Large cache is not the loss. `config_conflicts` is 0 in every
run (asserted). The `wild` time does not move although its releases drop: its loss is the small-block part.

## 5. Conditions and identity

- Host: 11th Gen Intel Core i7-11800H x16, Windows 10 Pro 10.0.19045 x64; rustc 1.97.0 (2d8144b78
  2026-07-07), x86_64-pc-windows-msvc, release profile.
- Every run under a 10 % hard CPU cap (owner rule; `capc 10`, a job-object CPU rate limit). Absolute
  ns/op and ms are inflated and not comparable to other reports; only ratios inside one run are used.
- Source: main `4e1f0b6e46b3752f59263fb4756c6ac3e76b3c44`, clean tree; the x build differs only by the
  feature list. Binaries identified by sha256 above.
- Identity of the measured source, binaries, toolchain and host: `docs/perf/p217/identity.json` (also the
  `#` header of the summary CSV).
- Raw files (`docs/perf/p217/`, all under 200 KiB): `lm1_judge_raw.jsonl` (section 1, runner
  `docs/perf/lm1/run.mjs`), `mt_raw.txt` (section 2), `prof_raw.jsonl` (section 3, runner
  `docs/perf/wild1/run.mjs` with `WJ_PROF=1`, scenarios `regrow,growmix,wild,large`), `ab_raw.jsonl`
  (section 4, runner `docs/perf/p217/run_ab.mjs --blocks 12 --repeats 3 --scenarios large,wild,regrow,growmix`).

## 6. What the promotion changes

- `production` = `alloc-global`, `alloc-xthread`, `alloc-decommit`, `fastbin`, `alloc-segment-directory`,
  `primordial-lazy-commit`, `large-cache-adaptive`.
- The Large cache starts as before (8 slots, no byte budget); after the base slots overflow it learns, from
  spans it released and was asked for again, a retention target up to the heap's recent peak of live Large
  bytes; quiet decay intervals unlearn it. An explicit `budget_bytes` remains a hard cap.
- Layout: two adaptive-gated booleans live in the inline `LargeCacheDecayConfig`
  (`crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_impl.rs`); the rest of the state is in
  the extension sidecar, which exists only after the first overflow. The `size_of::<HeapCore>() <= 9216`
  compile-time pin (`crates/core/sefer-registry/src/registry/heap_core/core.rs`) is unchanged and is checked
  by every CI build of the new composition.
- The old fixed 8-slot cache: build with the previous feature list (without `large-cache-adaptive` and
  `large-cache-extended`); there is no runtime switch.
- Tests: `tests/p217_production_adaptive_default.rs` fails if `production` stops implying
  `large-cache-adaptive`, and (with `alloc-stats`) checks learning beyond 8 slots, the hard budget and decay
  to the headroom floor; `tests/j14_large_cache_adaptive.rs` keeps its J14 assertions and now also needs
  `alloc-stats`, because it reads the Large-cache hit counter, which only that feature increments.

## 7. Canonical tables

`bench:table:paired` on the promoted tree (commit `8390fee6`, plain `production`, regimes `steady` and
`fresh:16`, 3 processes × 200 rounds, under the same cap): SeferAlloc is ahead of mimalloc in all 24 cells
(1.08–4.31× steady, 1.18–15.29× fresh). The README paired tables are rewritten from these runs; samples,
manifests and CSVs are in `docs/perf/p217/bench_paired_*` (samples gzip-compressed, each over 200 KiB raw).
The promotion touches the Large path only, so these small-block cells are a no-regression control, not
evidence for the change.
The criterion `bench:table` and `npm run iai` were not refreshed in this PR — see the PR description; the
CI perf gate compares Callgrind `Ir` of the new composition against main.
