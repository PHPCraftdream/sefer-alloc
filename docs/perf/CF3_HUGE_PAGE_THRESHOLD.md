# CF-3 — the transparent-huge-page threshold becomes a setting; sweep and default (`bench`, measurement-only, default unchanged)

**Class:** the setting itself (`LargeCacheConfig::huge_page_after_segments`, commit `f76b0914f40a2767402b0ddc9eff104f681c6f45`; tree `58771673f7fa11977c6239896e3087db53f17b54`, unchanged by the later reword of its subject) is
`perf(opt-in)`: default unchanged (16), one relaxed load added to the cold small-segment reservation path. This report is `bench`:
it only measures the setting (judge arm `sefer_pool` takes `WJ_THP=<n>` at build time). Tables and both summary CSVs are generated
by `scripts/cf3_report.mjs` from `docs/perf/cf3/`; the prose cites them. Platform: Linux (WSL2) only — the hint is a no-op elsewhere.

**Entry point under test:** a static `SeferAlloc` instance reached through `GlobalAlloc` shims (the `__rust_alloc` boundary of a `#[global_allocator]` user), with the system allocator as the process's own global allocator: `SeferAlloc::with_config(LargeCacheConfig::DEFAULT.pool(...).huge_page_after_segments(n))`
inside the process judge `examples/g1_wild_judge.rs` (arm `sefer_pool`). It is not the process's registered `#[global_allocator]`; it is the same `GlobalAlloc` methods on the same allocator type, which is the layer the setting ships at.

## 0. Summary

1. **The setting.** `.huge_page_after_segments(n)` on `LargeCacheConfig`: from the n-th small segment (4 MiB each, counted over
   the whole process) on, a newly reserved small segment is advised `MADV_HUGEPAGE` (Linux). `0` never hints; default 16 (64 MiB of
   small segments, the TH-1 value). Process-wide, first config wins; out-of-range values (above `u32::MAX`) mean "never"; no value can
   index anything.
2. **What the hint buys (page faults, deterministic).** Minor page faults of the process in the 1 GiB growth shape `grow64`:
   threshold 0 (never) 331,728; 1: 53,882; 4: 56,937; 16 (default): 69,166; 64: 118,078; mimalloc 33,432. The other growth shapes
   show the same order (tables below). The hinted-segment counter follows the threshold exactly (grow64: 273 / 270 / 258 / 210
   segments for thresholds 1 / 4 / 16 / 64).
3. **What it costs (memory).** In the small shapes the threshold decides how much memory is backed by huge pages: `wild` peak RSS 41
   MiB at 0 and 64 and above, 62 at 16, 100 at 4 and 110 at 1; `phase` 185 MiB at 0, 295 at 16, 307 at 4, 310 at 1. The big growth
   shapes are not affected (grow64 peak RSS 1296–1300 MiB at every threshold: they fill whole segments).
4. **Time.** Medians are in the direction of the faults — never hinting is slower in the three growth shapes (ratio to the default
   1.203, 1.726, 1.560 in grow64, growmix, regrow) — but the per-repetition ranges are wide on the shared machine (the control
   `sefer`, the same configuration in the same binary, spans 0.60–1.32 in grow64), so the time rows are supporting evidence and the
   page-fault counts are the measurement. Hinting earlier than 16 (thresholds 4 and 1) removes 18 % and 22 % of the default's faults in grow64 (numerator: 69,166 minus 56,937 and
   53,882; denominator: the default's 69,166), without a time difference above the noise and with 38 and 48 MiB more peak RSS in `wild`.
5. **Verdict:** the default stays 16. In grow64 it has 2.1× the page faults of mimalloc (69,166 / 33,432), against 9.9× with the hint
   off (331,728 / 33,432) and 3.5× at threshold 64 (118,078 / 33,432), while the small shapes keep their 4 KiB-page RSS (`wild` 62 MiB
   against 110 MiB at threshold 1). The README section "Transparent huge pages (Linux)" documents the setting and the numbers.

## 1. What was measured

* **Judge and identity.** `examples/g1_wild_judge.rs` at commit `f76b0914f40a2767402b0ddc9eff104f681c6f45` (`cf3/tree.txt`), release;
  `--features production` for time, `--features "production internals bench-internals"` for the counters; binary hashes, `rustc`, CPU, kernel
  and the system's transparent-huge-page mode (`madvise`: the hint is honoured) are in `cf3/ident.txt`. Linux WSL2, i7-11800H.
* **Arms.** `t<N>` = `sefer_pool` built with `WJ_THP=<N>` for N in 0, 1, 4, 16, 64, 1024 and the default pool (`WJ_POOL=4`: pool 4 segments, byte cap 16 MiB —
  the first build of this sweep used the CF-1 default of the arm, 1024, and was discarded before any data was taken); `sefer` =
  `SeferAlloc::new()`; `mi` = mimalloc. `sefer` and `t16` run the same binary with the same effective configuration: their spread is the
  noise floor. Six scenarios (`grow64`, `grow1k`, `growmix`, `regrow`, `wild`, `phase`; `large` allocates no small segments), seed 1,
  scale 1, one process per run.
* **Rotation.** Per scenario and repetition the eight arms run in a cyclic rotation, 9 repetitions.
* **Config evidence (R26-4).** For every threshold: requested (`thp_req`), resolved (`thp_res`, read with `AllocCore::dbg_huge_page_after_segments()`
  from the allocator itself), pool requested and resolved (4 / 4), `config_conflicts` = 0; the script aborts on any mismatch
  (`cf3/evidence.jsonl`, `cf3/mech.jsonl`). Subprocess isolation per run.
* **Mechanism evidence (R30-8).** The hinted-segment counter (`THP_HINTED_SMALL_SEGMENTS`) and the huge-page-backed bytes at the live
  peak (`AnonHugePages` from `/proc`) per threshold; the script checks that threshold 0 hints nothing and that the counter never rises with the threshold.
* **Statistics.** Time: median of per-repetition ratios against `t16` of the same repetition, with the minimum and maximum of those
  ratios; faults and RSS: medians.


## 2. Tables

**Median ns/op, ratio to `t16` of the same repetition (the default threshold; median of per-repetition ratios; `sefer` is the same configuration in the same binary: its spread is the noise floor; < 1 = faster than the default)**

| scenario | sefer | t0 | t1 | t4 | t16 | t64 | t1024 | mi |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 0.957 (0.60–1.32) | 1.203 (0.83–1.42) | 1.330 (0.89–1.45) | 1.215 (0.63–1.39) | 1.000 (1.00–1.00) | 0.997 (0.64–1.19) | 1.220 (0.80–1.31) | 0.543 (0.42–0.83) |
| grow1k | 1.046 (0.34–1.20) | 1.939 (0.76–2.33) | 0.982 (0.81–2.23) | 1.007 (0.33–2.69) | 1.000 (1.00–1.00) | 1.150 (0.45–1.42) | 2.035 (0.72–2.11) | 1.754 (0.25–2.17) |
| growmix | 0.997 (0.89–1.18) | 1.726 (1.16–1.87) | 1.762 (1.05–2.10) | 1.069 (0.77–2.12) | 1.000 (1.00–1.00) | 1.173 (1.02–1.60) | 1.642 (1.14–1.82) | 0.742 (0.50–1.44) |
| regrow | 1.026 (0.60–1.62) | 1.560 (0.97–1.82) | 0.920 (0.57–1.17) | 1.032 (0.57–1.68) | 1.000 (1.00–1.00) | 1.158 (0.75–1.33) | 1.573 (0.99–2.01) | 0.681 (0.49–1.30) |
| wild | 1.019 (0.85–1.10) | 1.084 (0.81–1.22) | 1.015 (0.82–1.08) | 1.000 (0.86–1.12) | 1.000 (1.00–1.00) | 0.976 (0.80–1.18) | 0.993 (0.82–1.10) | 1.171 (0.78–1.36) |
| phase | 0.992 (0.86–1.15) | 1.009 (0.92–1.12) | 0.945 (0.83–1.11) | 0.976 (0.87–1.07) | 1.000 (1.00–1.00) | 1.038 (0.94–1.16) | 1.025 (0.90–1.09) | 1.014 (0.96–1.20) |

**Median ns/op over mimalloc (median ns of the arm / median ns of `mi`; < 1 = faster than mimalloc)**

| scenario | sefer | t0 | t1 | t4 | t16 | t64 | t1024 | mi |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 1.225 | 1.634 | 1.706 | 1.551 | 1.300 | 1.282 | 1.570 | 1.000 |
| grow1k | 0.566 | 1.149 | 0.591 | 0.562 | 0.569 | 0.669 | 1.112 | 1.000 |
| growmix | 1.396 | 2.261 | 2.528 | 1.461 | 1.387 | 1.476 | 2.278 | 1.000 |
| regrow | 1.456 | 2.278 | 1.377 | 1.530 | 1.490 | 1.742 | 2.323 | 1.000 |
| wild | 0.877 | 0.943 | 0.887 | 0.912 | 0.874 | 0.887 | 0.850 | 1.000 |
| phase | 0.906 | 1.008 | 0.981 | 0.956 | 0.970 | 0.979 | 0.963 | 1.000 |

**Median minor page faults of the process (3 or 9 runs per cell)**

| scenario | sefer | t0 | t1 | t4 | t16 | t64 | t1024 | mi |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 69165 | 331728 | 53882 | 56937 | 69166 | 118078 | 331725 | 33432 |
| grow1k | 38416 | 300978 | 23129 | 26187 | 38413 | 87329 | 300972 | 2713 |
| growmix | 45211 | 345214 | 29925 | 32986 | 45211 | 94119 | 345215 | 6459 |
| regrow | 33507 | 175635 | 18222 | 21278 | 33506 | 82413 | 175632 | 6161 |
| wild | 13273 | 15008 | 9762 | 10382 | 13272 | 15009 | 15007 | 744 |
| phase | 21950 | 53552 | 9444 | 11936 | 21949 | 51232 | 53551 | 808 |

**Median peak RSS, MiB**

| scenario | sefer | t0 | t1 | t4 | t16 | t64 | t1024 | mi |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 1300 | 1296 | 1300 | 1300 | 1300 | 1300 | 1296 | 1161 |
| grow1k | 1180 | 1176 | 1180 | 1180 | 1180 | 1180 | 1176 | 1041 |
| growmix | 1351 | 1347 | 1351 | 1351 | 1351 | 1350 | 1347 | 1161 |
| regrow | 683 | 681 | 683 | 683 | 683 | 683 | 681 | 587 |
| wild | 62 | 41 | 110 | 100 | 62 | 41 | 41 | 420 |
| phase | 295 | 185 | 310 | 307 | 295 | 231 | 185 | 546 |

**Median RSS after the run, MiB**

| scenario | sefer | t0 | t1 | t4 | t16 | t64 | t1024 | mi |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 101 | 99 | 101 | 101 | 101 | 101 | 99 | 1033 |
| grow1k | 100 | 99 | 100 | 100 | 100 | 100 | 99 | 1033 |
| growmix | 1328 | 1325 | 1329 | 1329 | 1328 | 1328 | 1325 | 1139 |
| regrow | 109 | 107 | 109 | 109 | 109 | 109 | 107 | 573 |
| wild | 48 | 26 | 94 | 85 | 48 | 26 | 26 | 419 |
| phase | 239 | 169 | 254 | 251 | 239 | 197 | 169 | 545 |

**Median huge-page-backed bytes at the live peak, MiB (`AnonHugePages`)**

| scenario | sefer | t0 | t1 | t4 | t16 | t64 | t1024 | mi |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 1032 | 0 | 1092 | 1080 | 1032 | 840 | 0 | 1030 |
| grow1k | 1032 | 0 | 1092 | 1080 | 1032 | 840 | 0 | 1030 |
| growmix | 1178 | 0 | 1238 | 1226 | 1178 | 986 | 0 | 1136 |
| regrow | 560 | 0 | 620 | 608 | 560 | 368 | 0 | 570 |
| wild | 28 | 0 | 88 | 76 | 28 | 0 | 0 | 416 |
| phase | 226 | 0 | 286 | 274 | 226 | 54 | 0 | 542 |

**Small segments the hint was issued for (counter `THP_HINTED_SMALL_SEGMENTS`, median of 3 runs)**

| scenario | threshold 0 | threshold 1 | threshold 4 | threshold 16 | threshold 64 | threshold 1024 |
|---|---:|---:|---:|---:|---:|---:|
| grow64 | 0 | 273 | 270 | 258 | 210 | 0 |
| grow1k | 0 | 273 | 270 | 258 | 210 | 0 |
| growmix | 0 | 310 | 307 | 295 | 247 | 0 |
| regrow | 0 | 155 | 152 | 140 | 92 | 0 |
| wild | 0 | 22 | 19 | 7 | 0 | 0 |
| phase | 0 | 78 | 75 | 63 | 15 | 0 |

## 3. Reading the tables

* **Fault ratios in the summary.** Never-hint over default in grow64: 331,728 / 69,166 = 4.8; default over mimalloc: 69,166 / 33,432 = 2.1; never over
  mimalloc: 331,728 / 33,432 = 9.9; threshold 64 over mimalloc: 118,078 / 33,432 = 3.5 (all from the page-fault table; denominators named).
* **Thresholds 1 and 4 against the default.** 53,882 and 56,937 against 69,166 faults in grow64 (22 % and 18 % fewer; numerator the difference,
  denominator the default); the time column does not separate them from the default, and `wild` peak RSS is 110 and 100 MiB against 62.
* **Threshold 1024 behaves like 0** in every scenario here (fewer than 1024 small segments exist), which is also the "never" documented value.
* **Why the big shapes do not move in RSS.** They allocate whole segments, so every hinted segment is fully touched either way.

## 4. Limits

* Linux only, one thread per scenario, six fill-then-free or phase shapes; WSL2 kernel with the `madvise` mode. A system with the mode `never`
  ignores the hint, one with `always` backs segments by huge pages regardless of the setting.
* The page-fault counts are the reproducible measurement; wall-clock medians carry the noise of a shared machine.
* No default changed, so no `bench:table` / `iai` refresh is owed; the cold path gained one relaxed atomic load per small-segment reservation
  (a reservation is one per 4 MiB of small-block growth).

## 5. Reproduction

* Judge sources: tree in `cf3/tree.txt`. Scripts `cf3/build.sh`, `cf3/sweep.sh 9 1`, `cf3/mech.sh 3` (they read the directory in `CF3_DIR`; the binaries'
  hashes are in `cf3/ident.txt`).
* Tables and CSVs: `node scripts/cf3_report.mjs cf3/raw.jsonl cf3/evidence.jsonl cf3/mech.jsonl docs/perf`, then
  `node scripts/cf3_report.mjs --assemble docs/perf` rebuilds this file from `cf3/doc_head.md`, `cf3/cf3_tables.md` and `cf3/doc_tail.md`.
* Summary files: `docs/perf/CF3_HUGE_PAGE_THRESHOLD_summary.csv` (time, faults, RSS, huge-page bytes) and
  `docs/perf/CF3_HUGE_PAGE_THRESHOLD_mech_summary.csv` (hinted-segment counter per threshold). Source identity: commit
  `f76b0914f40a2767402b0ddc9eff104f681c6f45`.
