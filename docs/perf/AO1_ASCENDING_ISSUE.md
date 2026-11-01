# AO-1: a magazine refill's carve run is issued in ascending address order — `perf(runtime)` candidate, judge gate HOLD

`try_carve_batch` writes the blocks of a **magazine refill** (issue state MAG)
into the output slice back to front, so the magazine's top-first pops issue a
fresh carve run in **ascending address order**. Before, each 16-block refill came
out descending: a sawtooth that defeated the hardware prefetcher for any program
that touches its blocks in allocation order (a cold read of each block before
its free is 94 % of the judge's own time on grow64, perf annotate, Linux;
mimalloc issues ascending). Free-list blocks keep their order, the virgin
refill is covered (its bits are per run), batch callers (LIVE) still read
`out[]` ascending. Code: `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs`
(`try_carve_batch`); test `tests/ao1_ascending_issue_order.rs`.

Entry point under test: `SeferAlloc`'s `GlobalAlloc` methods on a static
instance (`examples/g1_wild_judge.rs`, the `__rust_alloc` boundary) — the layer
a `#[global_allocator]` user reaches; the change sits below it, in the carve of
the real allocator.

**Verdict: the preregistered judge gate reads HOLD, not GO.** Targets met on
both platforms, no cell met the kill criterion, controls pass the preregistered
exception; the non-inferiority intervals of the noisy cells (wild, growmix,
regrow, Linux grow1k) stay wider than the 3 % margin. By the preregistration the
decision goes to the owner with both campaigns' data (no third campaign).

## 1. Versions measured

| Name | Commit / tag | What |
|---|---|---|
| v1 | `archive/ao1-v1-measured` (`f8fb5638`) | reversal of the filled magazine prefix in `HeapCore::refill_magazine_slow` (extra pass, also reordered free-list blocks); campaign 1 |
| v2 | `archive/ao1-v2-measured` (`f6695702`) | the carve writes a MAG run top-down (`out[n-1-i]`); campaign 2, `bench:table` sets |
| v2b | `0e4867c5` (this branch) | v2 with the loop split by direction (`iter_mut().rev()`): same stores in the same order, no per-block select |

v2b is v2 without the +3.0 Ir per carved block (§4); v2's timing evidence
carries over because the stores, their order and the ascending commit order are
unchanged. Base for all campaigns: `main` `82626f9c`.

## 2. Preregistration and gate

`AO1_ASCENDING_ISSUE_PREREG.md` (committed before campaign 1) and its Amendment 1
(committed before campaign 2): A/A control, grow64 target ≤ 0.90 with the upper
bound below 1.00, non-inferiority upper bound ≤ 1.03 in every other cell, kill
above it, `bench:table` and MT as non-regression controls.

## 3. Judge campaigns (`scripts/ao1_report.mjs`, derived from `docs/perf/ao1/*_raw.jsonl`)

x / base = exp(mean over blocks of ln(x ns/op ÷ base ns/op)); A/A = the same with
a byte-identical second copy of the base binary; 95 % interval, t, n−1 degrees of freedom.

### Campaign 1 (v1; 8 blocks, one run per arm; Windows scale 1, Linux scale 0.5)

| Platform | Scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | cell verdict |
|---|---|---:|---:|---|---:|---|---|
| win | growmix | 8 | 0.960 | 0.848–1.087 | 1.014 | 0.787–1.305 | HOLD (CI crosses +3 %) |
| win | grow1k | 8 | 0.834 | 0.745–0.933 | 1.014 | 0.969–1.062 | non-inferior |
| win | regrow | 8 | 1.009 | 0.875–1.164 | 1.126 | 0.857–1.480 | HOLD (CI crosses +3 %) |
| win | wild | 8 | 0.978 | 0.896–1.067 | 1.151 | 0.920–1.440 | HOLD (CI crosses +3 %) |
| win | large | 8 | 0.981 | 0.805–1.196 | 0.994 | 0.853–1.157 | HOLD (CI crosses +3 %) |
| win | grow64 | 8 | 0.797 | 0.732–0.867 | 1.090 | 0.901–1.320 | target met |
| win | phase | 8 | 1.040 | 0.859–1.260 | 1.016 | 0.790–1.308 | HOLD (CI crosses +3 %) |
| linux | grow1k | 8 | 0.869 | 0.645–1.169 | 0.940 | 0.814–1.085 | HOLD (CI crosses +3 %) |
| linux | wild | 8 | 0.980 | 0.957–1.004 | 0.958 | 0.915–1.003 | non-inferior |
| linux | grow64 | 8 | 0.794 | 0.731–0.863 | 0.951 | 0.858–1.055 | non-inferior |
| linux | growmix | 8 | 0.790 | 0.373–1.673 | 0.696 | 0.343–1.412 | HOLD (CI crosses +3 %) |
| linux | regrow | 8 | 1.035 | 0.909–1.177 | 1.009 | 0.920–1.107 | HOLD (CI crosses +3 %) |

Judge gate: HOLD

### Campaign 2 (v2; 12 blocks, minimum of 3 back-to-back repeats per arm, Amendment 1)

| Platform | Scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | cell verdict |
|---|---|---:|---:|---|---:|---|---|
| win | growmix | 12 | 0.984 | 0.896–1.081 | 1.019 | 0.918–1.131 | HOLD (CI crosses +3 %) |
| win | grow1k | 12 | 0.736 | 0.700–0.773 | 0.983 | 0.917–1.054 | target met |
| win | regrow | 12 | 1.012 | 0.968–1.058 | 0.989 | 0.949–1.031 | HOLD (CI crosses +3 %) |
| win | wild | 12 | 1.004 | 0.915–1.101 | 0.960 | 0.895–1.030 | HOLD (CI crosses +3 %) |
| win | large | 12 | 0.954 | 0.893–1.020 | 1.067 | 0.910–1.251 | non-inferior |
| win | grow64 | 12 | 0.771 | 0.709–0.840 | 1.046 | 0.936–1.169 | target met |
| win | phase | 12 | 0.883 | 0.806–0.967 | 0.947 | 0.874–1.027 | non-inferior |
| linux | grow1k | 12 | 1.012 | 0.962–1.064 | 0.979 | 0.927–1.034 | HOLD (CI crosses +3 %) |
| linux | wild | 12 | 1.027 | 0.994–1.060 | 1.010 | 0.969–1.053 | HOLD (CI crosses +3 %) |
| linux | grow64 | 12 | 0.803 | 0.764–0.845 | 0.992 | 0.951–1.035 | target met |
| linux | growmix | 12 | 1.000 | 0.923–1.082 | 0.981 | 0.907–1.061 | HOLD (CI crosses +3 %) |
| linux | regrow | 12 | 1.013 | 0.956–1.073 | 0.994 | 0.954–1.036 | HOLD (CI crosses +3 %) |

Judge gate: HOLD

## 4. Path-activation oracle and deterministic controls (`scripts/ao1_controls_report.mjs`)

| Platform | Scenario | Arm | steps of +block size | total steps | share (steps / total) |
|---|---|---|---:|---:|---:|
| win | grow64 | base | 0 | 16777215 | 0.00 % |
| win | grow64 | x | 16776942 | 16777215 | 100.00 % |
| win | grow1k | base | 0 | 1048575 | 0.00 % |
| win | grow1k | x | 1047756 | 1048575 | 99.92 % |
| linux | grow64 | base | 0 | 8388607 | 0.00 % |
| linux | grow64 | x | 8388471 | 8388607 | 100.00 % |
| linux | grow1k | base | 0 | 524287 | 0.00 % |
| linux | grow1k | x | 523879 | 524287 | 99.92 % |

| wild judge Linux, scale 0.03 | base Ir | v2 Ir | v2b Ir | v2 / base − 1 | v2b / base − 1 |
|---|---:|---:|---:|---:|---:|
| grow64 | 151302613 | 152812719 | 151347824 | 1.00 % | 0.03 % |
| grow1k | 13714778 | 13791354 | 13693024 | 0.56 % | -0.16 % |

| g1_mt_one T=1 | base Ir | v2b Ir | v2b / base − 1 |
|---|---:|---:|---:|
| mstress | 47244574 | 47243991 | -0.001 % |
| larson | 46146060 | 46145380 | -0.001 % |

| bench:table paired set | sefer-time gm (x / base − 1) | largest agreed loss | new LOSS cells vs mimalloc | magnitude rule |
|---|---:|---:|---:|---|
| s1 | -0.5 % | +0.9 % | 0 | NO-GO |
| s2 | +0.4 % | +2.6 % | 0 | NO-GO |

`bench:table` rows above are the explicit non-regression exception of the
preregistration (sefer-time gm ≤ +1.0 %, largest agreed loss ≤ +3 %, no new
LOSS cell against mimalloc): both sets pass it; by the plain magnitude rule
(`scripts/g1_magnitude_verdict.mjs`, which requires a ≥ 0.5 % improvement) both
read NO-GO, as expected for a change whose target is outside that table. The
cells with a repeatable cost are direct 16 B and 256 B (+0.9…+2.6 % of sefer
time); v2b removes the per-block instruction cost behind it (v2 +1.00 % Ir on
grow64, v2b +0.03 %), the wall-clock of v2b itself was not remeasured.

## 5. Identity

- Base `82626f9c` (`main`); v2 `f6695702`; v2b `0e4867c5`; judge source for the
  Windows and Linux campaign-2 base builds: `e551ff85:examples/g1_wild_judge.rs`.
- Binary hashes: `docs/perf/ao1/win2_ident.txt`, `linux2_ident.txt`
  (the two base copies are byte-identical in each).
- Raw data: `docs/perf/ao1/{win,linux}{,2}_raw.jsonl`, schedules
  (`linux_schedule.txt`, `linux2_schedule.txt`; Windows via
  `docs/perf/ao1/run.mjs --emit`), `docs/perf/ao1/bt/` (paired `bench:table` md,
  csv, manifests, gzip raw samples, verdicts).
- Platforms: Windows 10 (i7-11800H, shared machine), Linux = WSL on the same
  host.

## 6. Not measured

- v2b wall-clock (Ir only).
- `malloc_macro` multi-threaded wall-clock for v2 (campaign 1's run was noise);
  MT is covered by callgrind Ir of `g1_mt_one` T=1 only.
- Linux grow1k shows no gain (1.012, 0.962–1.064): the effect is
  platform-dependent and is not explained here.
- `refill_magazine_slow_virgin` and the free-list drain keep descending /
  list order (documented, out of scope).
