# P220 — where the growmix/regrow/wild time goes: CPU time, profile and simulated cache misses (`bench`, no code change)

Task #220 (2026-11-26). This is a measurement-only record and no code changes. The tables in sections 1, 2 and 4 are
printed by `node docs/perf/p220/report.mjs` from `docs/perf/p218/ab_raw.jsonl` and
`docs/perf/p220/cg_growmix.{sefer,mi}.out.gz`. The per-table summary is
`docs/perf/P220_TEARDOWN_MISS_ATTRIBUTION_summary.csv`, and identities, hashes and commands are in
`docs/perf/p220/identity.json`. The source is `origin/main` `d5e89119` with `--features production`.

## 1. Wall time under the 10 % CPU cap is mostly throttle

During the P218 A/B the judge process ran on a CPU for only 14–17 % of its timed window. Table: median over 36 runs
per cell, CPU time = process user + kernel time.

| scenario | arm | median wall ms | median CPU ms | CPU / wall |
|---|---|---:|---:|---:|
| growmix | base | 5251 | 765 | 0.146 |
| growmix | mi | 4186 | 640 | 0.153 |
| regrow | base | 2998 | 421 | 0.140 |
| regrow | mi | 2366 | 358 | 0.151 |
| wild | base | 1782 | 265 | 0.149 |
| wild | mi | 1252 | 218 | 0.174 |

What follows from this:

- The job-object hard cap (`capc 10`) throttles even a single-threaded process. Wall time measured under it is
  CPU time stretched about 7× plus the cap's scheduling quantum, so the wall ratios are noisier than the work they
  measure.
- This is also the likely reason the two `bench:table` runs of #219 disagreed: criterion's short samples land on
  random throttle windows.
- From now on, judges run under the cap report process CPU time (user + kernel) as the primary axis.

## 2. P218 re-read on CPU time

Table: per-block minimum of 3 repeats, geometric mean over 12 blocks, 95 % t interval. Wild kernel time is one or two
15.6 ms timer ticks, so it is omitted.

| scenario | metric | v1/base | v3/base | base/mi |
|---|---|---:|---:|---:|
| growmix | cpu | 1.089 (0.986–1.204) | 1.026 (0.940–1.120) | 1.215 (1.092–1.352) |
| growmix | user | 1.164 (1.019–1.329) | 1.096 (0.977–1.231) | 1.378 (1.217–1.559) |
| growmix | kernel | 0.960 (0.876–1.052) | 0.864 (0.729–1.022) | 1.136 (0.979–1.317) |
| growmix | wall | 1.034 (0.922–1.160) | 1.001 (0.907–1.106) | 1.271 (1.158–1.395) |
| regrow | cpu | 0.942 (0.858–1.034) | 0.958 (0.873–1.051) | 1.290 (1.177–1.414) |
| regrow | user | 0.961 (0.803–1.150) | 0.959 (0.816–1.127) | 1.089 (0.885–1.341) |
| regrow | kernel | 0.934 (0.812–1.074) | 1.026 (0.932–1.128) | 1.453 (1.223–1.727) |
| regrow | wall | 0.946 (0.820–1.092) | 0.966 (0.872–1.071) | 1.334 (1.169–1.522) |
| wild | cpu | 1.088 (1.016–1.165) | 1.034 (0.960–1.112) | 1.273 (1.161–1.396) |
| wild | user | 1.109 (1.037–1.186) | 1.036 (0.966–1.112) | 1.255 (1.152–1.367) |
| wild | wall | 1.072 (0.942–1.219) | 1.156 (0.984–1.359) | 1.199 (1.042–1.379) |

The P218 NULL verdict stands: no variant is faster. V1 is slower than base on `wild` CPU and on `growmix` user time.

The gap to mimalloc has two different natures:

- **`growmix` and `wild` lose in user time** (1.38, 1.26), while kernel time is within noise.
- **`regrow` loses in kernel time** (1.45), while user time is within noise. Sefer releases emptied segments
  (`seg_released` 140) and touches them again; mimalloc keeps its pages.

## 3. Sampling profile (Windows, xperf, 0.1221 ms)

The profile was taken on the line-table build of the same source, `growmix`, Sefer then mimalloc. The output is
`docs/perf/p220/xperf_lines.txt`, written by `docs/perf/p220/xperf_lines.mjs` from the 117 MB trace, whose sha256 is
in `identity.json`.

| | Sefer | mimalloc |
|---|---:|---:|
| samples in judge + allocator code | 5834 | 5106 |
| kernel (`ntoskrnl.exe`) samples | 5578 | 5489 |

- **Kernel time is the same.** The user-mode difference is +728 samples.
- **The allocator's own code is not where the user-mode gap is.** Sefer's own functions take about 650 samples
  (for example `flush_run` 221). mimalloc's take about 1100 (`mi_page_free_list_extend` 341, `mi_free` 311, …).
- **The excess sits on memory touches.** The judge's one-byte read of each block before `free`
  (`g1_wild_judge.rs:347`) takes 3555 samples under Sefer against 2539 under mimalloc. The first write into each
  fresh block takes 493 against 193.
- **What the profile cannot show:** inlining puts the allocator's loads next to these lines, so the profile alone
  cannot say whose miss it is. Hardware counter sampling (`-pmcprofile LLCMisses`) is unavailable on this host;
  xperf reports "Failed to configure counters".

## 4. Simulated caches (callgrind `--cache-sim=yes`, WSL, `growmix` scale 0.25)

Uncapped on this host the scenario runs at Sefer 365.1 ns/op against mimalloc 194.5 ns/op
(`cg_run.growmix.*.json`).

| arm | category | Ir | D1mr | D1mw | DLmr | DLmw |
|---|---|---:|---:|---:|---:|---:|
| sefer | judge | 57,342,931 | 7,288 | 61 | 10 | 5 |
| sefer | sefer | 93,132,992 | 359,365 | 23,174 | 269,391 | 274 |
| sefer | mimalloc | 129,448 | 233 | 22 | 191 | 11 |
| sefer | std (inlined) | 53,895,965 | 1,961,785 | 1,108,074 | 1,337,735 | 906,100 |
| sefer | other | 27,476,403 | 117,200 | 53,097 | 6,537 | 43,320 |
| sefer | **total** | 231,977,739 | 2,445,871 | 1,184,428 | 1,613,864 | 949,710 |
| mi | judge | 56,653,571 | 24 | 12 | 9 | 7 |
| mi | sefer | 1,404 | 0 | 0 | 0 | 0 |
| mi | mimalloc | 82,416,731 | 779,804 | 353,979 | 3,885 | 334,979 |
| mi | std (inlined) | 23,084,291 | 896,148 | 364,823 | 285,152 | 328,746 |
| mi | other | 22,286,175 | 11,379 | 12,361 | 2,139 | 11,760 |
| mi | **total** | 184,442,172 | 1,687,355 | 731,175 | 291,185 | 675,492 |

LL read misses sefer/mi = 1,613,864 / 291,185 = 5.54. Pointer reads and writes inlined from `core` (`std (inlined)`)
are attributed below to the function they were inlined into:

| arm | enclosing function | DLmr | DLmw | D1mr |
|---|---|---:|---:|---:|
| sefer | `AllocCore::flush_class_internal` | 617,224 | 2,947 | 663,548 |
| sefer | `Run::alloc` | 2 | 556,639 | 6,913 |
| sefer | `Run::free` | 286,921 | 0 | 365,401 |
| sefer | `AllocCore::find_segment_with_free_impl` | 268,678 | 0 | 309,070 |
| sefer | `dealloc_fast::lane_miss` | 262,772 | 0 | 302,072 |
| sefer | `ClassLeaves::promote` | 46 | 253,963 | 682 |
| sefer | `g1_wild_judge::main` | 66,550 | 91,389 | 439,696 |
| sefer | `dealloc_fast::lane_second_chance` | 43,970 | 0 | 52,325 |
| mi | `mi_page_free_list_extend` | 0 | 333,880 | 0 |
| mi | `Run::free` | 243,485 | 1 | 365,447 |
| mi | `Run::alloc` | 0 | 237,361 | 4 |
| mi | `g1_wild_judge::main` | 30,434 | 91,377 | 439,267 |
| mi | `g1_wild_judge::free_all` | 11,175 | 2 | 91,340 |
| mi | `mi_arenas_page_alloc_fresh` | 0 | 8,499 | 1,037 |
| mi | `__memcpy_avx_unaligned_erms` | 4 | 2,051 | 2,057 |
| mi | `mi_free` | 1,979 | 0 | 572,348 |

**Reading.** The judge's own read before `free` misses about equally under both allocators (287 K against 243 K), so
the Windows sample excess on that line is not the read itself. The excess LL misses are Sefer's own metadata
accesses on the free side:

- the magazine flush `flush_class_internal` (617 K) reads the bin-table head, live count and state bytes of each
  run's segment;
- `lane_miss` (263 K);
- the Small discovery scan `find_segment_with_free_impl` (269 K);
- on the write side, the first write into a fresh block (557 K against 237 K) and leaf promotion (254 K).

`mi_free` misses L1 572 K times but LL only about 2 K times, so mimalloc's page metadata stays cached. Sefer's
per-segment metadata of the ~311 live segments does not stay cached.

The leading candidate is cache-set aliasing. Every segment's header sits at a 4 MiB-aligned base, so the hot
header lines of all segments share their low 22 address bits and compete for the same cache sets. The simulated
LL indexes by low bits, as the L1 and L2 caches do; the real LLC hashes slices, so the simulation is the upper bound
for that level. The next step (task #221) is a probe that spreads the hot per-segment metadata across cache sets,
measured on this cache simulation and on the wild judge by CPU time.

## 5. Disposition

- No code change. `docs/perf/OPEN_ITEMS.md` item 146 records the two natures of the gap (user-mode metadata
  misses for `growmix`/`wild`, kernel segment release for `regrow`) and the probe as the next trigger.
- Judges under the CPU cap now report CPU time as the primary axis.
