# RET-1 measurement A — what keeping every empty segment buys once the release is counted (`bench`, measurement-only, no code, no default changed)

Measurement-only: no `perf(runtime)` or `perf(opt-in)` change. The question comes from the owner's decision list (OWN-1,
item 3, "Retain") and the consultation of `@oh` and the independent reviewer: the CF-1 sweep (`CF1_POOL_SEGMENTS.md`)
showed a Windows time gain only at pool 1024, which retains the whole burst. A release that is only deferred is not saved,
so the gain has to be read with the cost of what the process does afterwards. This report adds the whole-process time to the
judge-window time for the same runs; `Retain` is not implemented.

**Entry point under test:** a static `SeferAlloc` instance reached through `GlobalAlloc` shims (the `__rust_alloc` boundary of
a `#[global_allocator]` user), with the system allocator as the process's own global allocator:
`SeferAlloc::with_config(LargeCacheConfig::DEFAULT.pool(...))` inside `examples/g1_wild_judge.rs` (arm `sefer_pool`, built
with `WJ_POOL=<N>`). It is not the process's registered `#[global_allocator]`; it is the same `GlobalAlloc` methods on the
same allocator type, which is the layer the pool ships at.

## Design

* Windows 10, i7-11800H, shared machine (other load possible). Binaries and tree: `docs/perf/ret1/ident.txt` (the CF-1
  sweep tree, `main` before AO-1 and WILD-2; retention behaviour does not depend on them).
* Pools 4 (control, the default), 16, 64, 1024; scenarios `grow64`, `grow1k`, `regrow`, `growmix`; scale 1, seed 1; 9
  repetitions; one process per run; the pool order rotates per repetition (`docs/perf/ret1/run.mjs`).
* Two clocks per run: the **window** — the judge's own `ns_per_op` over its timed region; the **process** — wall-clock from
  spawn to exit measured by the runner, which includes process start, the judge's untimed setup and teardown and the exit,
  where the operating system takes back whatever the allocator still holds.
* Per scenario and pool: ratio to the pool-4 run of the same repetition, median over the 9 repetitions, with the range.
* Config evidence (R26-4): the requested pool is in every record (`pool_req`); the resolved value is read back by the judge
  (`pool_res`, `0` in the records means the judge's own field was not populated for this build path, see Limits) and the CF-1
  report carries the resolved-value and conflict checks for these exact binaries (`cf1/*/evidence.jsonl`). Mechanism
  evidence: `end_rss` (memory left after the run) is in every record and rises with the pool as in CF-1 (table below).
  Subprocess isolation per run.

## Result (`node scripts/ret1_report.mjs`, derived from `docs/perf/ret1/win_raw.jsonl`)

| Scenario | pool | window ratio to pool 4 (median of per-rep ratios) | window range | process ratio to pool 4 | process range | share of window gain kept at process level | RSS left after run, MiB (median) | pool-4 process ms (median) |
|---|---:|---:|---|---:|---|---:|---:|---:|
| grow64 | 16 | 0.952 | 0.735–1.038 | 0.956 | 0.738–1.045 | 91 % | 80 | 1329 |
| grow64 | 64 | 0.937 | 0.667–1.059 | 0.952 | 0.683–1.074 | 77 % | 285 | 1329 |
| grow64 | 1024 | 0.873 | 0.624–0.944 | 0.946 | 0.680–1.020 | 42 % | 1172 | 1329 |
| grow1k | 16 | 0.951 | 0.809–1.044 | 0.967 | 0.824–1.053 | 67 % | 80 | 592 |
| grow1k | 64 | 0.912 | 0.820–1.012 | 0.962 | 0.859–1.061 | 43 % | 285 | 592 |
| grow1k | 1024 | 0.779 | 0.660–0.916 | 0.966 | 0.824–1.150 | 16 % | 1172 | 592 |
| regrow | 16 | 1.002 | 0.761–1.082 | 1.014 | 0.775–1.112 | n/a (no window gain) | 123 | 454 |
| regrow | 64 | 0.911 | 0.818–0.992 | 0.968 | 0.873–1.063 | 36 % | 328 | 454 |
| regrow | 1024 | 0.840 | 0.756–1.060 | 0.969 | 0.886–1.183 | 19 % | 668 | 454 |
| growmix | 16 | 0.976 | 0.890–1.267 | 0.986 | 0.879–1.228 | 56 % | 1328 | 872 |
| growmix | 64 | 1.026 | 0.818–1.238 | 0.993 | 0.818–1.226 | n/a (no window gain) | 1328 | 872 |
| growmix | 1024 | 1.005 | 0.892–1.253 | 0.982 | 0.874–1.212 | n/a (no window gain) | 1328 | 872 |

"Share of window gain kept at process level" = (1 − process ratio) ÷ (1 − window ratio), both medians of per-repetition
ratios of the same pool against pool 4; it is printed only where the window ratio is below 1 (the script asserts the
arithmetic).

## Reading

* **The window gain of pool 1024 reproduces CF-1** (grow64 0.873 against 0.917, grow1k 0.779 against 0.757, regrow 0.840
  against 0.794; the same ordering and size, a different session).
* **At whole-process level the gain at pool 1024 is 3–5 %** (0.946, 0.966, 0.969) and its per-repetition range contains 1.0
  in every scenario. The window gain is 13–22 %; what is kept at process level is 42 %, 16 % and 19 %. The rest is spent
  outside the window, in the process's start and exit with 1172 MiB (668 MiB for regrow) still held.
* **The noise is large.** Ranges of the per-repetition ratios span more than ±20 % in several cells; a difference of 3–5 % at
  process level is not distinguishable from zero in this run. This is a statement about this measurement, not a proof that
  the process-level gain is zero.
* **Pools 16 and 64** sit between: the window ratios (0.95 and 0.91–0.94 where they gain) are within the spread of CF-1's
  control, their process ratios are 0.95–0.97; the memory price is 80 and 285 MiB (about 4 MiB per segment, as in CF-1).
* `growmix` does not depend on the pool (as in WG-M0): its end RSS is 1328 MiB for every pool.

## Verdict

NULL for implementing `Retain`: a deferred release does not turn into a saving. At the level the user sees (the process from
start to exit) the best case, pool 1024, is 3–5 % faster within noise, in return for 1172 MiB left after the run against 29 MiB
at the default. The recommendation of `@oh` and the reviewer stands: no `Retain` code; the size of the pool is documented with
its retention formula in the README (PR #169). RP-2(b) (a batch reset of a fully free segment on drain) is not decided by this
measurement; it stays the next step for a shape where the release itself is the cost, as a dev-feature prototype that starts
with the release-only case.

## Limits

* Windows only. Linux time in WSL2 has no usable signal for this question (CF-1 §3); Linux evidence stays the counters.
* Whole-process time includes the judge's untimed setup, so the ratio is diluted by a constant that is the same for every
  pool; it does not remove the deferred exit cost, which is the quantity of interest.
* One session, 9 repetitions, shared machine; the ranges above are the honest uncertainty.
* `pool_res` in the raw records is the judge's own read-back field for this build path and reads `0` in these records; the
  resolved pool of these exact binaries is established by the CF-1 `evidence.jsonl` checks, which ran the same
  `time_<N>.exe` family, and by `end_rss` rising with the pool.

## Identity and reproduction

* Tree, binary hashes, rustc and CPU: `docs/perf/ret1/ident.txt`. Raw per-run data: `docs/perf/ret1/win_raw.jsonl`
  (one JSON record per run). Summary: `docs/perf/RET1_RETAIN_PROCESS_LEVEL_summary.csv`
  (`node scripts/ret1_report.mjs --csv docs/perf/RET1_RETAIN_PROCESS_LEVEL_summary.csv`).
* Re-run: build `examples/g1_wild_judge.rs` with `WJ_POOL=<N>` for N in 4, 16, 64, 1024 (`--release --features production`),
  copy to `time_<N>.exe`, then `RET1_BIN_DIR=<dir> RET1_OUT=<jsonl> node docs/perf/ret1/run.mjs 9`.
