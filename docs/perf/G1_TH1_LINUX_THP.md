# G1 TH-1: transparent huge pages for the small segments of a growing heap (Linux) — `perf(runtime)`

`SeferAlloc` now asks the kernel for transparent huge pages
(`madvise(MADV_HUGEPAGE)`) on every small segment from the 16th small-segment
reservation of the process on (64 MiB of small segments). On Linux a heap that
keeps growing in small blocks then fills with 2 MiB pages instead of 4 KiB
ones; smaller heaps keep 4 KiB pages and their RSS. Large spans never get the
hint. On other targets the call is a no-op.

- `crates/aligned-vmem`: `Reservation::advise_huge_pages(&self) -> bool`
  (`api/reserve.rs`; Linux/Android `madvise`, `false` elsewhere, under miri
  and under the mock backend).
- `src/alloc_core/small/alloc_core_small/reserve.rs`: the policy
  (`THP_AFTER_SMALL_SEGMENTS = 16`, process-wide count of small-segment
  reservations), issued right after the reservation of a small segment.

Entry point under test: SeferAlloc's `GlobalAlloc` methods on a static
instance (`examples/g1_wild_judge.rs`, the `__rust_alloc` boundary); the
change sits below it, in the small-segment reservation of the real
`#[global_allocator]`.

## 1. Mechanism

The wild judge (`G1_WJ1_WINDOWS_SPAN_RESERVE.md`) left growth scenarios at
1.5–2× mimalloc's time on Linux. `perf stat` on grow64 (scale 0.5, 512 MiB):
166 K page faults for SeferAlloc against 17 K for mimalloc, user time 660 ms
against 356 ms (`/proc/vmstat`-level THP counters read 0 on this WSL kernel;
the fault counts and `AnonHugePages` are the evidence). With
`transparent_hugepage/enabled = madvise` (the WSL and common distribution
default) only memory marked `MADV_HUGEPAGE` gets 2 MiB pages; SeferAlloc
marked none.

## 2. Policy, from four probe stages

Scratch builds of the same tree, the hint switched off by an environment
variable in the same binary (`docs/perf/th1/probe/*.txt`, script
`docs/perf/th1/probe/th1.sh`; 4 runs per sefer arm, minimum ns/op):

| Probe stage | Scenario | off min ns | on min ns | on / off (min) | peak RSS MiB off / on |
|---|---|---:|---:|---:|---|
| th1 | grow64 | 31.9 | 28.8 | 0.900 | 649 / 652 |
| th1 | grow1k | 244.4 | 134.2 | 0.549 | 589 / 592 |
| th1 | growmix | 205.0 | 129.7 | 0.633 | 675 / 678 |
| th1 | regrow | 189.5 | 100.7 | 0.531 | 342 / 344 |
| th1 | large | 1257.4 | 43584.6 | 34.663 | 8 / 511 |
| th1 | wild | 24.5 | 66.9 | 2.736 | 19 / 467 |
| th1 | phase | 17.5 | 18.6 | 1.063 | 136 / 553 |
| th1b | grow64 | 35.7 | 27.0 | 0.757 | 649 / 652 |
| th1b | grow1k | 254.7 | 186.9 | 0.734 | 589 / 592 |
| th1b | growmix | 247.0 | 169.7 | 0.687 | 675 / 678 |
| th1b | regrow | 207.2 | 127.9 | 0.617 | 342 / 344 |
| th1b | large | 1246.1 | 1512.5 | 1.214 | 8 / 15 |
| th1b | wild | 21.5 | 24.4 | 1.134 | 19 / 60 |
| th1b | phase | 16.8 | 16.4 | 0.977 | 136 / 217 |
| th1c | grow64 | 31.5 | 25.8 | 0.818 | 649 / 651 |
| th1c | grow1k | 251.5 | 142.5 | 0.567 | 589 / 591 |
| th1c | growmix | 223.9 | 153.3 | 0.684 | 675 / 678 |
| th1c | regrow | 197.2 | 116.4 | 0.590 | 342 / 344 |
| th1c | large | 1370.9 | 1399.6 | 1.021 | 8 / 15 |
| th1c | wild | 19.9 | 21.4 | 1.077 | 19 / 57 |
| th1c | phase | 16.7 | 16.3 | 0.973 | 136 / 201 |
| th1d | grow64 | 33.2 | 26.9 | 0.812 | 649 / 651 |
| th1d | grow1k | 255.8 | 130.8 | 0.512 | 589 / 591 |
| th1d | growmix | 221.3 | 136.6 | 0.617 | 675 / 678 |
| th1d | regrow | 203.5 | 130.0 | 0.639 | 342 / 344 |
| th1d | large | 1351.2 | 1330.5 | 0.985 | 8 / 8 |
| th1d | wild | 21.2 | 21.7 | 1.022 | 19 / 19 |
| th1d | phase | 17.5 | 17.3 | 0.986 | 136 / 200 |

- `th1`, every reservation: growth gains, but Large spans fill whole huge
  pages (large 35×, peak RSS 8 → 511 MiB) and wild loses 2.7×.
- `th1b`, small segments only: Large is clean; wild and phase still carry huge
  pages on small heaps (wild peak RSS 19 → 60 MiB).
- `th1c`, from 16 live segments of any kind: Large spans count, so wild still
  crosses it.
- `th1d`, from the 16th small-segment reservation: growth gains kept, large
  and wild unchanged in time and RSS. This is the shipped policy.

## 3. Implementation A/B (Linux)

Base `89c1fa58` (branch `g1-wild`, PR #150) against `70e5943f` (this change),
both built with the same judge source (`examples/g1_wild_judge.rs`, sha256
prefix `fc56944a8e421585`, adds `anon_huge_peak`/`anon_huge_end` from
`/proc/self/smaps_rollup` as the path-activation evidence). WSL2 Ubuntu on the
host below, scale 0.5, order base / TH-1 / mimalloc / TH-1 / base per round;
run1: all seven scenarios, 4 runs per sefer arm; run2/run3: the noisiest
scenarios again with 8 runs per sefer arm. Tables by
`node scripts/th1_report.mjs --csv docs/perf` (`G1_TH1_LINUX_THP_summary.csv`):

| Run | Scenario | runs/arm | base min / med ns | TH-1 min / med ns | mimalloc min ns | TH-1 / base (min) | TH-1 / base (median) | base / mi (min) | TH-1 / mi (min) | page faults base / TH-1 / mi | THP peak MiB base / TH-1 | peak RSS MiB base / TH-1 / mi |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| run1 | grow64 | 4 | 49.1 / 65.5 | 43.1 / 65.5 | 30.4 | 0.879 | 1.000 | 1.615 | 1.419 | 165950 / 42803 / 16766 | 0 / 4 | 649 / 651 / 583 |
| run1 | grow1k | 4 | 366.8 / 433.8 | 224.6 / 354.3 | 180.9 | 0.612 | 0.817 | 2.028 | 1.242 | 150559 / 27411 / 1407 | 0 / 4 | 589 / 591 / 523 |
| run1 | growmix | 4 | 354.5 / 392.1 | 257.1 / 620.7 | 175.1 | 0.725 | 1.583 | 2.024 | 1.468 | 173224 / 31304 / 3296 | 0 / 558 | 675 / 678 / 582 |
| run1 | regrow | 4 | 321.6 / 405.2 | 249.9 / 292.1 | 158.5 | 0.777 | 0.721 | 2.029 | 1.577 | 88156 / 25020 / 3153 | 0 / 248 | 342 / 344 / 297 |
| run1 | large | 4 | 1710.2 / 3282.6 | 2396.0 / 3352.8 | 43734.9 | 1.401 | 1.021 | 0.039 | 0.055 | 3219 / 3218 / 490 | 0 / 0 | 8 / 8 / 701 |
| run1 | wild | 4 | 32.8 / 41.4 | 29.0 / 38.5 | 38.6 | 0.884 | 0.930 | 0.849 | 0.751 | 5147 / 5145 / 640 | 0 / 0 | 19 / 19 / 242 |
| run1 | phase | 4 | 19.3 / 24.5 | 20.1 / 25.1 | 22.2 | 1.038 | 1.024 | 0.871 | 0.904 | 36333 / 16100 / 678 | 0 / 140 | 136 / 200 / 314 |
| run2 | grow64 | 8 | 43.5 / 54.5 | 37.2 / 50.0 | 27.8 | 0.856 | 0.917 | 1.564 | 1.339 | 165949 / 42803 / 16765 | 0 / 484 | 649 / 651 / 583 |
| run2 | grow1k | 8 | 425.1 / 570.9 | 263.4 / 369.5 | 250.2 | 0.620 | 0.647 | 1.699 | 1.053 | 150558 / 27412 / 1405 | 0 / 484 | 589 / 591 / 523 |
| run2 | large | 8 | 1651.5 / 2252.2 | 1779.9 / 2257.8 | 35605.9 | 1.078 | 1.002 | 0.046 | 0.050 | 3219 / 3218 / 491 | 0 / 0 | 8 / 8 / 701 |
| run2 | phase | 8 | 19.5 / 24.5 | 20.4 / 24.3 | 21.2 | 1.049 | 0.988 | 0.921 | 0.966 | 36329 / 16100 / 676 | 0 / 140 | 136 / 200 / 314 |
| run3 | growmix | 8 | 305.2 / 443.0 | 294.4 / 365.5 | 164.7 | 0.965 | 0.825 | 1.853 | 1.788 | 173222 / 31306 / 3297 | 0 / 558 | 675 / 678 / 582 |
| run3 | regrow | 8 | 236.5 / 327.4 | 187.2 / 302.4 | 158.5 | 0.791 | 0.924 | 1.492 | 1.181 | 88150 / 25015 / 3152 | 0 / 248 | 342 / 344 / 297 |
| run3 | wild | 8 | 24.6 / 37.5 | 24.3 / 30.7 | 35.7 | 0.991 | 0.818 | 0.687 | 0.681 | 5145 / 5146 / 637 | 0 / 0 | 19 / 19 / 242 |

MT control (`g1_mt_one`, T = 1, 2 M ops, wall ms):

| Run | g1_mt_one T=1 | runs/arm | base median ms | TH-1 median ms | TH-1 / base |
|---|---|---:|---:|---:|---:|
| run1 | mstress | 8 | 48 | 46 | 0.958 |
| run1 | larson | 8 | 41 | 40 | 0.976 |
| run2 | mstress | 8 | 47 | 49 | 1.043 |
| run2 | larson | 8 | 41 | 43 | 1.049 |
| run3 | mstress | 8 | 45 | 45 | 1.000 |
| run3 | larson | 8 | 40 | 40 | 1.000 |

Reading:

- **Growth:** grow1k −38…−39 % (min), grow64 −12…−14 %, regrow −21…−22 %,
  growmix −4…−27 %. Page faults fall 3.5–6× (grow64 166 K → 43 K), and the
  heap holds 484–558 MiB of huge pages at the live peak, against 0 at base.
- **large, wild:** the hint never fires (huge pages 0 at both arms, page
  faults equal); their deltas are the noise of the host (large has 5 190 ops;
  run2's 8 runs per arm put it at +0.2 % by median).
- **phase:** 140 MiB of huge pages, peak RSS 136 → 200 MiB (mimalloc 314),
  time within ±5 % (median −1.2 % in run2).
- **MT:** within ±5 % on both runs, either sign.
- The host was loaded during run1 (every arm, mimalloc included, spread up to
  2×); the minimum per arm is the statistic, the median is printed alongside.

## 4. Cost and limits

- RSS: a small segment marked after the threshold commits memory in 2 MiB
  steps, so a heap past 64 MiB can hold up to 2 MiB more per partly filled
  segment (phase: +64 MiB peak). Smaller heaps are unchanged by construction.
- Decommit of a partly used segment (`MADV_DONTNEED` on 4 KiB ranges) splits
  the huge page back into 4 KiB pages; correctness is unchanged.
- Windows and macOS: no effect (no unprivileged THP equivalent).
- `transparent_hugepage/enabled = never`: the call is accepted and ignored.

## 5. Tests

- `crates/aligned-vmem/tests/advise_huge_pages.rs`: the hint is accepted
  exactly on Linux/Android, keeps every page's contents and leaves the span
  writable; repeated calls agree.
- `tests/th1_thp_hint_policy.rs` (`bench-internals internals`): no hint below
  16 small segments; once 20 are reserved, exactly `reserved − 15` hints were
  issued (`AllocCore::dbg_small_segments_reserved`,
  `AllocCore::dbg_thp_hinted_small_segments`). Without the threshold the count
  would be `reserved`; without the hint, 0.

## 6. Identity

- Base `89c1fa58fe8e05b02c514dea4b847b76576a777f` (branch `g1-wild`);
  measured change `70e5943f4a588c694205cf0d56caeb84f6e7596a` (branch `g1-th1`); binary hashes in
  `docs/perf/th1/impl/run*_bin.txt`; driver `docs/perf/th1/impl/th1m.sh`.
- Host: Windows 10 x86_64, i7-11800H, 48 GiB; Linux numbers from WSL2 Ubuntu
  (kernel 6.18, THP `madvise`, defrag `madvise`).
