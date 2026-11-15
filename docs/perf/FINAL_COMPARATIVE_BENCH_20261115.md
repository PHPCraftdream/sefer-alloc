# Final comparative measurements under hard limits (2026-11-15)

Status: measurement-only. No `perf(runtime)` / `perf(opt-in)` change; no product GO/veto is derived here; relative numbers, not canonical ones.

## Identity

- Source: `d093edd78f33edc5b0517e677c320c21b45c0220` (main after PR #234); the measured tree equals it (the worktree differs only by documents).
- Limiter: `wnice` built by GitHub CI from win-nice master `0acb5f7` (sha256 prefix `0f22f28aed55e1c56371aab9`), installed system-wide on 2026-11-15.
- Host: Windows 10 19045, 11th Gen Intel Core i7-11800H (16 logical CPUs), WSL2 Ubuntu-24.04 (kernel 6.18, THP `madvise`, 4096-byte pages, 2 MiB huge pages, 24.2 GB available).
- Toolchains: Windows bench table rustc 1.97.0; WSL J13 study rustc 1.98.1 (recorded in the build receipt).

## Constraints (owner request)

Everything ran under `wnice -p high -c 10` — a hard cap of 10% of all logical CPUs (1.6 CPUs) with HIGH priority.
- Windows: `wnice -p high -c 10 -- node scripts\bench-table.mjs`. The cap is a Job Object hard cap (flags 0x5, rate 1000); HIGH reaches every process of the tree through wnice's unelevated booster (job completion port + SetPriorityClass).
- WSL2: `wnice --wsl -c 10 -p high -- bash j13-run.sh <phase>` — the limits are applied inside the guest (systemd scope: CPUQuota=160%, Nice=-20, CPUWeight=10000), because a Windows Job Object does not reach the VM.
- Job-free launch: the agent shell is itself inside a Job Object (nested CPU rates multiply: 10% became 0.9% in the W1 spike), so every measurement was started through WMI `Win32_Process.Create` (`scripts/run-job-free.ps1`; probe `in_job=False` before wnice, hard cap exactly 10% under it).
- Limits of the method: a single-threaded benchmark uses at most 1 of the 1.6 allowed CPUs, so it is not throttled; multi-threaded parts and compilation are. The WSL guest cap does not stop Windows-side processes from displacing the VM threads (the host-side vmmem process cannot be opened without elevation). The numbers are suitable for relative comparison only.

## 1. bench:table — SeferAlloc vs mimalloc vs System (ns per pair unless stated)

`bench-table` self-check: PASS, 159 bench ids parsed, all 51 expected ids present. Raw log: `docs/perf/_raw_final_comparative_bench_20261115.log`; summary: `docs/perf/FINAL_COMPARATIVE_BENCH_20261115_summary.csv` (derived by `bench-table-to-csv.mjs`).

### Warm repeated bulk burst (1 alloc + 1 free per pair)

| Size | SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.2 | 5.1 | 49.3 | 1.19x faster |
| 64B | 4.1 | 6.6 | 53.6 | 1.61x faster |
| 256B | 4.7 | 9.0 | 52.0 | 1.90x faster |
| 1024B | 4.2 | 18.6 | 63.8 | 4.44x faster |

### Churn, non-writing

| Size | SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 | 6.7 | 37.7 | 1.24x faster |
| 64B | 4.5 | 11.8 | 41.5 | 2.60x faster |
| 256B | 4.5 | 17.6 | 46.4 | 3.92x faster |
| 1024B | 4.8 | 95.9 | 52.3 | 20.06x faster |

### Churn + write

| Size | SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 | 7.1 | 48.5 | 1.41x faster |
| 64B | 5.1 | 8.3 | 40.9 | 1.64x faster |
| 256B | 5.7 | 17.1 | 46.3 | 2.97x faster |
| 1024B | 8.9 | 68.5 | 52.6 | 7.70x faster |

### Churn + teardown (diagnostic composite, not a clean per-pair figure)

| Size | SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 | 6.7 | 55.5 | 1.23x faster |
| 64B | 5.6 | 7.4 | 61.0 | 1.33x faster |
| 256B | 10.9 | 13.0 | 59.5 | 1.20x faster |
| 1024B | 32.2 | 22.9 | 59.8 | 1.40x slower |

### manual_realloc_sim (ns per closure: manual grow chain, not GlobalAlloc::realloc)

| SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---:|---:|---:|---:|
| 388.7 | 342.1 | 767.6 | 1.14x slower |

### segment_decommit_cycle (ns per batch of 34 alloc + 34 free)

| SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---:|---:|---:|---:|
| 1259.3 | 2193.6 | 33317.0 | 1.74x faster |

Reading: SeferAlloc is faster than mimalloc in every sized cell except churn+teardown at 1024B, and slower in the manual realloc chain. Each ratio is mimalloc/SeferAlloc of the printed point estimates (the CSV script asserts it within rounding).

## 2. J13 registered study — Always vs Off (WSL2, tail trim included)

Registered protocol unchanged: 10 cells, 12 ABBA blocks plus 12 A/A blocks each (960 children), min of 2 launches per block, exp(mean of 12 paired log ratios), two-sided 95% paired t interval (df=11, t*=2.201). The CI blocks `G1 limited-study single CI build and provenance receipt` and `Manual J13 measurement` were extracted verbatim from the repository (`extract-j13-steps.mjs`) and run unmodified in the guest. A discarded 16-child smoke came first (all host checks passed, reducer refused the one-block input as intended). The whole study took 2 min 40 s (children are milliseconds long).

Always/Off is the ratio of the selected metric (cold_ns, or cache_ns for the CACHE cell); below 1 means Always is faster. Every A/A interval contains 1 (payload readability only, never speed evidence). Evidence: `docs/perf/j13_final_20261115/` (files above 200 KiB gzip-compressed); summary: `docs/perf/J13_FINAL_COMPARATIVE_20261115_summary.csv` (derived by `j13-summary.mjs` from the per-cell reducer reports).

| Cell | Regime | Always/Off (95% CI) | A/A payload ratio (95% CI) | Median Off -> Always (ns) | Reading |
|---|---|---:|---:|---:|---|
| cycle-1MiB | COLD | 0.9826 (0.9349-1.0329) | 0.9868 (0.9429-1.0328) | 299469 -> 290795 | no difference detected |
| cycle-8MiB-minus-1 | COLD | 0.9808 (0.9568-1.0053) | 0.9897 (0.9677-1.0123) | 2441554.5 -> 2363862.5 | no difference detected |
| cycle-8MiB | COLD | 0.4602 (0.3272-0.6473) | 1.0086 (0.9772-1.0411) | 2353116 -> 1077589.5 | Always 2.17x faster |
| cycle-16MiB | COLD | 0.3888 (0.3181-0.4753) | 1.0031 (0.9632-1.0446) | 4515851 -> 2301659.5 | Always 2.57x faster |
| cycle-128MiB | COLD | 0.4328 (0.3428-0.5464) | 1.0067 (0.9802-1.0339) | 35196293 -> 21514023 | Always 2.31x faster |
| cycle-1024MiB | COLD | 0.3677 (0.3266-0.4139) | 0.9843 (0.9502-1.0196) | 295789207 -> 115023712 | Always 2.72x faster |
| grow-1GiB | COLD | 0.4579 (0.4282-0.4896) | 0.9833 (0.9388-1.0299) | 747597960 -> 383005920 | Always 2.18x faster |
| many-128x16MiB | COLD | 0.3986 (0.3563-0.4460) | 0.9450 (0.8696-1.0270) | 607529691.5 -> 300209102 | Always 2.51x faster |
| edge-128MiB | COLD | 11.8877 (11.1342-12.6921) | 1.0313 (0.9859-1.0788) | 13547 -> 158206 | Always 11.89x slower |
| cache-16MiB | CACHE | 0.3043 (0.2592-0.3574) | 0.9185 (0.8169-1.0328) | 115456.5 -> 27898 | Always 3.29x faster |

Reading, with numerators and denominators: for cold allocation+touch+free at 8 MiB and above (cycle-8MiB, cycle-16MiB, cycle-128MiB, cycle-1024MiB, grow-1GiB, many-128x16MiB), Always/Off is 0.37-0.46 (geometric mean of the 12 per-block paired ratios of the cell's metric under Always over Off), i.e. Always is 2.2-2.7x faster; at cycle-1MiB and cycle-8MiB-minus-1 (below the 8 MiB threshold, advice not applied) no difference is detected (both 95% intervals contain 1). The CACHE cell (cache-16MiB, second life of a cached 16 MiB block) reads the opposite way on this host: Always/Off is 0.3043 (0.2592-0.3574), Always 3.29x faster (median 115456.5 ns Off -> 27898 ns Always); the A/A payload interval (0.9185, 0.8169-1.0328) still contains 1, so payload readability holds. The earlier GitHub-runner study measured the same cell at 1.0231 (1.0093-1.0371): the direction differs between hosts; the cause is not investigated.

**Regression to report to the owner (not new, not a veto):** edge-128MiB (128 MiB request, 1 repetition, untouched payload, cache off) is 11.9x slower with Always: median 13547 ns (Off) -> 158206 ns (Always), roughly 145 microseconds of madvise cost over 128 MiB when no page is touched. The earlier registered study measured the same cell at 10.04 (8.71-11.58). Advice has a fixed per-MiB cost that only pays off when the memory is touched; no default change is inferred.

Comparability caveats: the earlier study ran on a GitHub runner (AMD EPYC, bare-metal-like) at rustc 1.99.1 before tail trim; this one runs in a WSL2 VM on a laptop CPU under a CPU cap, so absolute magnitudes differ (for example cycle-8MiB 0.4602 here vs 0.1336 there) and only the direction and the per-cell readability carry over.

## Reproduction

Scripts are kept in `docs/perf/final_comparative_20261115_scripts/`: `run-job-free.ps1`, `jobprobe.ps1`, `bench-table-to-csv.mjs`, `extract-j13-steps.mjs`, `wsl-setup-j13.sh`, `j13-run.sh`, `j13-export.sh`, `j13-summary.mjs`, `final-report.mjs`. Order: setup guest (`wsl-setup-j13.sh <sha>`), extract the two CI blocks, then `run-job-free.ps1 ... wnice --wsl -c 10 -p high -- bash j13-run.sh build|smoke|full`; export, derive the CSVs, build this report.
