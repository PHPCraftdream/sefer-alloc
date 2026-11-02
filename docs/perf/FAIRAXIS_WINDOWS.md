# WG-1 fair axis — sefer / mimalloc with the whole process counted, CPU split and page faults (Windows; `bench`, measurement-only, no code change in `src/`)

Measurement-only: no `perf(runtime)` or `perf(opt-in)` change. Only the benchmark judge `examples/g1_wild_judge.rs` gained
three output fields (`user_ms`, `kernel_ms`, `faults`, taken at the end of a run). The question comes from the independent
consultation after `WG1_WINDOWS_MAP_AFTER_AO1.md`: the judge window includes the final `free_all`, where sefer returns
segments to the OS and mimalloc keeps them until the process exits, so the window may overstate sefer's gap in the growth
and regrow shapes; and the cause of the `growmix` gap (memory size and page faults against CPU) was not attributed.

**Entry point under test:** a static `SeferAlloc` instance reached through `#[inline(never)]` `GlobalAlloc` shims at the
`__rust_alloc` boundary (header of `examples/g1_wild_judge.rs`), arms `sefer` and `mi` in one binary, one process per run, the
process's own global allocator stays `System`. It is not the process's registered `#[global_allocator]`; it is the same
`GlobalAlloc` methods on the same allocator type, which is the layer the allocator's tuning ships at.

## Design

* Windows 10, i7-11800H, shared machine. Source: `main` `4479f40f` plus the judge change (the file hash in the ident file is of the committed judge) (`docs/perf/fairaxis/ident.txt`).
* Seven scenarios, scale 1, seed 1. Per repetition the two arms run back to back, the order alternating by repetition
  (`docs/perf/fairaxis/run.mjs`). Run 1: 9 repetitions; run 2: 15 repetitions, same binary.
* Per run record: **window** = the judge's `ns_per_op` times `ops`; **process** = wall time from spawn to exit measured by the
  runner; **user**, **kernel** = `GetProcessTimes` at the end of the run (resolution about 15 ms); **faults** =
  `PageFaultCount` at the end; **RSS left** = `end_rss`.
* Statistics: window and process — median over repetitions of the per-repetition ratio `sefer / mi`, with the range;
  user, kernel, faults — ratio of the two arms' medians (the per-repetition ratio is undefined when a CPU field reads 0 at
  this resolution). There is no A/A arm in this run (one sefer binary); the two runs are the replication.

## Result (`node scripts/fairaxis_report.mjs`, derived from `docs/perf/fairaxis/win_run{1,2}.jsonl.gz`)

### Run 1

| Scenario | reps | window | window range | process | process range | user (ratio of medians) | kernel (ratio of medians) | faults (ratio of medians) | RSS left, MiB sefer / mi |
|---|---:|---:|---|---:|---|---:|---:|---:|---|
| growmix | 9 | 1.296 | 0.946–1.658 | 1.279 | 0.965–1.635 | 1.422 | 0.956 | 1.161 | 1328 / 1137 |
| regrow | 9 | 0.998 | 0.431–2.149 | 0.910 | 0.423–1.151 | 1.132 | 1.744 | 1.157 | 71 / 572 |
| wild | 9 | 1.311 | 0.845–2.401 | 1.259 | 0.691–2.260 | 1.368 | 1.000 | 1.222 | 29 / 53 |
| large | 9 | 1.240 | 0.100–1.429 | 0.679 | 0.102–0.800 | n/a | 1.000 | 1.088 | 8 / 28 |
| grow64 | 9 | 1.247 | 0.702–1.750 | 1.138 | 0.674–1.477 | 1.334 | 1.046 | 1.121 | 29 / 1031 |
| phase | 9 | 1.147 | 0.945–1.229 | 1.129 | 0.943–1.207 | 1.096 | 1.000 | 1.049 | 177 / 205 |
| grow1k | 9 | 1.167 | 0.438–1.494 | 0.994 | 0.404–1.247 | 1.000 | 1.388 | 1.136 | 29 / 1030 |

### Run 2

| Scenario | reps | window | window range | process | process range | user (ratio of medians) | kernel (ratio of medians) | faults (ratio of medians) | RSS left, MiB sefer / mi |
|---|---:|---:|---|---:|---|---:|---:|---:|---|
| growmix | 15 | 1.299 | 0.647–3.335 | 1.336 | 0.575–2.403 | 1.476 | 1.167 | 1.161 | 1328 / 1137 |
| regrow | 15 | 1.247 | 0.942–1.571 | 1.156 | 0.860–1.342 | 1.073 | 1.603 | 1.157 | 71 / 572 |
| wild | 15 | 1.501 | 1.096–1.954 | 1.433 | 1.063–1.872 | 1.337 | 2.067 | 1.221 | 29 / 53 |
| large | 15 | 1.299 | 0.977–1.795 | 0.673 | 0.596–0.913 | n/a | 1.000 | 1.088 | 8 / 28 |
| grow64 | 15 | 1.274 | 1.081–1.600 | 1.129 | 0.969–1.459 | 1.434 | 1.095 | 1.121 | 29 / 1031 |
| phase | 15 | 1.113 | 0.520–1.187 | 1.108 | 0.523–1.169 | 1.099 | 1.000 | 1.050 | 177 / 205 |
| grow1k | 15 | 1.233 | 0.519–1.971 | 1.066 | 0.315–1.589 | 0.897 | 1.248 | 1.136 | 29 / 1030 |

## Reading

* **The noise of this design is large.** Several per-repetition ranges span a factor of two or more (run 1 `regrow` window
  0.431–2.149; run 2 `growmix` 0.647–3.335), and run 1 and run 2 disagree on `regrow` (window 0.998 against 1.247, process
  0.910 against 1.156). Only statements that hold in both runs are made below; `regrow` run 1 is not used.
* **Counting the process narrows the growth shapes.** Run 2, window → process: grow64 1.274 → 1.129, grow1k 1.233 → 1.066,
  regrow 1.247 → 1.156; run 1 gives the same direction for grow64 (1.247 → 1.138) and grow1k (1.167 → 0.994). The cause is in
  the RSS column: mimalloc ends with 1031 MiB of the grown memory still held (572 MiB for regrow), sefer with 29 (71) MiB,
  because sefer returns empty segments inside the window. This is the retention policy measured in
  `RET1_RETAIN_PROCESS_LEVEL.md`, not an allocator-path cost.
* **`growmix` and `wild` stay behind at process level** (run 2: 1.336 and 1.433; run 1: 1.279 and 1.259) and the CPU split
  puts the gap in user time (growmix 1.42 / 1.48, wild 1.37 / 1.34 of mimalloc's user time). Page faults are 16 % (growmix)
  and 22 % (wild) higher for sefer in both runs, kernel time is within the clock's resolution or noisy (0.96–2.07). Neither
  explains a gap of 30–40 %. In `growmix` sefer's RSS left (1328 MiB) is 17 % above mimalloc's (1137 MiB), consistent with
  the faults ratio; it is not the gap.
* **The hypothesis that `growmix` is a memory-size problem is not supported**: with +16 % faults and equal kernel time the
  extra faults cost at most a few percent; the user-CPU ratio (about 1.45) is the quantity that has to move.
* **`phase` stays at about 1.11 at process level** in both runs. **`large` reads 0.67 at process level** in both runs while its
  window is 1.24–1.30: the run is about 30 ms, process start and exit dominate, and mimalloc's start-up is the larger
  share; no allocator-path statement is made from it.

## Verdict

Measurement, no GO/NO-GO of a change. It (1) separates the shapes where the gap is policy-sensitive (grow64, grow1k, regrow:
the process-level gap is 1.07–1.16 and the window gap is retention timing) from those where it is CPU (growmix, wild, phase:
1.11–1.43 at process level, in user time); (2) removes memory size and page faults as the explanation of `growmix`; (3) points
the next measurement at the user-CPU of the mixed free and of the small free in a many-segment heap (`WILD1_GAP_DECOMPOSITION.md`
memo misses, `GM-1` metadata misses), not at the release policy.

## Limits

* Windows only, one machine, shared; no A/A arm; `user`/`kernel` have a resolution of about 15 ms, so short runs
  (`large`, `phase` kernel) carry no CPU-split information.
* The ratio of arm medians for CPU fields is not a paired statistic.
* The judge's untimed setup is part of the process time and is the same for both arms.

## Identity and reproduction

* `docs/perf/fairaxis/ident.txt` (source SHA, judge file hash, binary hash), raw `docs/perf/fairaxis/win_run{1,2}.jsonl.gz`,
  summary `docs/perf/FAIRAXIS_WINDOWS_summary.csv`
  (`node scripts/fairaxis_report.mjs --csv docs/perf/FAIRAXIS_WINDOWS_summary.csv`).
* Re-run: build `examples/g1_wild_judge.rs` (`--release --features production`), then
  `node docs/perf/fairaxis/run.mjs <exe> <out.jsonl> 15`.
