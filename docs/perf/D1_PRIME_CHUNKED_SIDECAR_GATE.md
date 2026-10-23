# D1′ chunked sidecar — comparative performance gate

**Verdict: NO-GO / NOT ACCEPTED.** Safety CI receipt applies to the earlier safety-tested source; this report evaluates the measured candidate identified below. No merge to `main` is authorized or performed.

## Source identity and environment

- Baseline commit/tree: `f80c681b799ef4ef7fa08d79c06896fdf1104fe9` / `b5d22c5a770e503750bb549152245325d915f350`; measurement tree `346a155750dc1fda2d5eb1be0edc3b3e6f1bcfa3`.
- Candidate commit/tree: `734395b7bb12bdd21f605c07de5426b2e4e717ef` / `89bfd1c99ff3f528b1d9d1e93c468029c3e82496`; retained-pin WSL measurement tree `001e6ad026a2e1a960ccc580d79a7ff9cf10f781`.
- Canonical wall-clock, IAI and first-allocation use the cited commit trees. MT source identity is each base commit plus the pre-measurement harness patch SHA-256 `743fef67ba3f7fb50e39710197d9d66ef5a7b8322d4542a3cb5c069b896b87f1`; the later registration measurement trees must not be attributed retroactively to the MT run. Registration trees: `988f78d97035d2870a71561c34cc9f21b8b21b76` / `c3a52a9ce02f6ff47602b3c6a7d0ca19939b439f`. Windows retained-pin trees: `60fcd80364ced3bef26587e405ff32b17b132690` / `8e7952f37a99a05e46db33075318d46e06a87aaa`.
- Ограничения: native Linux не измерялся; WSL PSS/private-dirty сняты только для component retained-pin lifecycle, не для first-alloc и нескольких уровней live segments. Эти ADR-оси не закрыты. NO-GO исключает promotion данного кандидата, но не означает полного покрытия всей матрицы. Предыдущая фраза «Linux-ось закрыта» была слишком широкой.
- Windows host: 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz; 16 logical processors; Windows 10 Pro x86_64; Rust 1.97.0 (2d8144b78 2026-07-07), LLVM 22.1.6, Cargo 1.97.0 (c980f4866 2026-06-30).
- Linux memory oracle: WSL2 Ubuntu 24.04 userland; memory telemetry via /proc/self/smaps_rollup; Linux PC 6.18.33.2-microsoft-standard-WSL2 #1 SMP PREEMPT_DYNAMIC Thu Jun 18 21:54:43 UTC 2026 x86_64; Rust 1.98.1 (48a229cea 2026-09-01), Cargo 1.98.1 (797e8a9bc 2026-08-05). This is WSL evidence, not native Linux.
- Wall-clock canonical rows use clean, isolated Criterion targets and four balanced source runs (before, after, after, before); IAI totals and marginal Ir/op are reported on separate axes.
- Installed-allocator comparisons use fresh processes and A/B/B/A blocks; all claims name the measured entry point below.

## Canonical wall-clock — `bench:table`

Entry point: Criterion invokes each allocator through `GlobalAlloc` directly in one process; this is not an installed `#[global_allocator]` run. Each delta is candidate minus baseline; each candidate/baseline ratio prints its numerator and denominator. Raw OLS slopes are diagnostic; Criterion estimates preserve the canonical `bench:table` axis. Four clean runs reduce but do not eliminate host noise; this table is not a per-cell confidence test.

| Workload | Unit | Before Sefer OLS | Candidate Sefer OLS | Delta | Criterion candidate / baseline | Criterion before / candidate | mimalloc before / candidate | Sefer/mimalloc before / candidate |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| global_alloc/{allocator}/16B | ns/pair | 18.15 | 20.29 | 2.14 | 20.91 / 18.43 = 1.135 | 18.43 / 20.91 | 5.96 / 6.89 | 2.864 / 3.191 |
| global_alloc_churn/{allocator}/16B | ns/pair | 8.70 | 13.33 | 4.64 | 12.65 / 8.82 = 1.434 | 8.82 / 12.65 | 7.77 / 9.26 | 1.111 / 1.427 |
| global_alloc_churn_write/{allocator}/16B | ns/pair | 10.42 | 11.89 | 1.47 | 11.33 / 10.94 = 1.035 | 10.94 / 11.33 | 7.70 / 9.54 | 1.393 / 1.205 |
| global_alloc_churn_with_teardown/{allocator}/64B | ns/pair | 12.18 | 18.55 | 6.37 | 17.58 / 14.39 = 1.222 | 14.39 / 17.58 | 10.08 / 10.92 | 1.471 / 1.467 |
| segment_decommit_cycle/{allocator}/253KiB | ns/batch | 3372.27 | 12218.27 | 8846.00 | 12136.71 / 3287.20 = 3.692 | 3287.20 / 12136.71 | 3065.92 / 3873.33 | 1.067 / 3.375 |

## IAI — raw totals and marginal cost are distinct

Entry point: the normal churn/alloc-free workloads use `SeferAlloc` through its `GlobalAlloc` facade directly; isolated free-only probes enter through `HeapCore` diagnostics. Neither is a process-installed `#[global_allocator]` timing result. The installed-global process comparison is reported separately below.

| Workload | Before total Ir | Candidate total Ir | Candidate / baseline total Ir | Before Est. Cycles | Candidate Est. Cycles |
| --- | --- | --- | --- | --- | --- |
| small_churn_16b | 52439 | 11548 | 11548 / 52439 = 0.220 | 136286 | 33334 |
| cold_alloc_free_256x16b | 97971 | 63449 | 63449 / 97971 = 0.648 | 199228 | 105737 |
| cold_alloc_free_256x64b | 99471 | 67487 | 67487 / 99471 = 0.678 | 208037 | 118017 |
| recycle_alloc_free_256x16b | 150470 | 112640 | 112640 / 150470 = 0.749 | 267219 | 169588 |
| large_alloc_free_cycle | 49095 | 8109 | 8109 / 49095 = 0.165 | 132948 | 29579 |

Marginal Ir/op is the separate normalized axis; the comparison to mimalloc is shown as SeferAlloc/mimalloc for each source.

| Workload | Before Sefer Ir/op | Candidate Sefer Ir/op | Before mimalloc Ir/op | Candidate mimalloc Ir/op | Before Sefer/mimalloc | Candidate Sefer/mimalloc |
| --- | --- | --- | --- | --- | --- | --- |
| small_churn_16b | 52.3 | 53.7 | 55.9 | 55.9 | 0.936 | 0.961 |
| cold_alloc_free_256x16b | 190.9 | 216.2 | 75.3 | 75.3 | 2.535 | 2.871 |
| cold_alloc_free_256x64b | 196.8 | 231.9 | 79.2 | 79.2 | 2.485 | 2.928 |
| recycle_alloc_free_256x16b | 198.0 | 204.2 | 78.1 | 78.1 | 2.535 | 2.615 |

## Real installed global allocator and mimalloc

Entry point: the process installs each allocator as `#[global_allocator]` and runs the shared workload. The internal window and full process wall are both reported; a CI including zero is INCONCLUSIVE, not PASS. A/A controls are retained in `installed_paired_ab_summary.csv`.

| Comparison (A/B) | Internal A ns | Internal B ns | A/B internal ratio | Internal paired 95% CI Δ ns | Process A ns | Process B ns | A/B process ratio | Process paired 95% CI Δ ns |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| after-before | 631108 | 656438 | 0.961 | -86558…35898 | 7438430 | 6931940 | 1.073 | -325385…1338365 |
| before-mimalloc | 575163 | 1322488 | 0.435 | -844270…-650380 | 6311205 | 8257150 | 0.764 | -2559389…-1332501 |
| after-mimalloc | 605503 | 1287410 | 0.470 | -761395…-602420 | 6092685 | 8051705 | 0.757 | -2230860…-1687180 |

## Multithreaded Larson and foreign free

The MT Larson harness installs `SeferAlloc` as the real process `#[global_allocator]`; exact operation counts and route activation passed. Throughput ratios are candidate/baseline; latency CIs are paired. The foreign-free example also exercises the installed `#[global_allocator]` deallocation path. Its latency CIs include zero at all three batch sizes; RSS and commit telemetry are separate metrics.

| Workload / threads | Before block-mean median latency ns | Candidate block-mean median latency ns | Candidate/baseline | Paired latency 95% CI Δ ns | Before Mops | Candidate Mops | Candidate/baseline throughput | Latency CI excludes zero |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| larson / T=1 | 10246000 | 9820700 | 0.958 | -1640691…1288911 | 81.7 | 82.9 | 1.015 | no |
| larson / T=2 | 16075250 | 17279850 | 1.075 | -448648…5792578 | 100.3 | 92.8 | 0.925 | no |
| larson / T=4 | 16852325 | 21592950 | 1.281 | 195731…13231929 | 190.9 | 151.9 | 0.796 | yes |
| larson / T=8 | 19626200 | 23376850 | 1.191 | 492266…7598964 | 332.5 | 276.3 | 0.831 | yes |
| mstress / T=1 | 5888525 | 5220225 | 0.887 | -1160233…1645773 | 69.0 | 77.2 | 1.119 | no |
| mstress / T=2 | 8917950 | 8757550 | 0.982 | -4265517…10029417 | 89.7 | 91.2 | 1.017 | no |
| mstress / T=4 | 9952050 | 9441225 | 0.949 | -824291…1181491 | 160.8 | 169.4 | 1.054 | no |
| mstress / T=8 | 11108300 | 11088475 | 0.998 | -2714151…2281011 | 289.4 | 292.7 | 1.012 | no |

| Foreign-free batch | Before first-dealloc median ns | Candidate median ns | Paired 95% CI Δ ns | RSS after join before/candidate KiB | Commit after join before/candidate KiB |
| --- | --- | --- | --- | --- | --- |
| 30 | 300 | 350 | -191…301 | 3240 / 3212 | 1852 / 1872 |
| 60 | 350 | 350 | -321…271 | 3248 / 3216 | 1852 / 1874 |
| 120 | 400 | 400 | -227…347 | 3256 / 3220 | 1856 / 1874 |

## Registration churn, dense fill, and activation oracles

The registration entry point is direct `SeferAlloc::alloc/dealloc` component timing, not an installed `#[global_allocator]` result. Oracle builds separately verify route registrations/unlinks and candidate constructed chunks; timing builds omit diagnostic RMW. Every row below has a corresponding passing activation oracle.

| Registration cycles | Baseline registrations | Candidate registrations | Baseline unlinks | Candidate unlinks | Before median ns/cycle | Candidate median ns/cycle | Candidate/baseline median | Paired 95% CI Δ ns | Candidate constructed chunks | Candidate backing bytes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 30 | 31 | 31 | 30 | 30 | 107240 | 146458 | 1.366 | 14317…51404 | 720 | 3936000 |
| 60 | 61 | 61 | 60 | 60 | 107718 | 144808 | 1.344 | 23086…42891 | 1440 | 7872000 |
| 120 | 121 | 121 | 120 | 120 | 106199 | 147210 | 1.386 | 36515…53138 | 2880 | 15744000 |

| Dense16 workload | Before mean ns | Candidate mean ns | Candidate / baseline = numerator / denominator | Paired 95% CI Δ ns | Baseline activation | Candidate activation | Candidate constructed chunks | Candidate backing bytes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 262,144 × 16-byte blocks | 21817490 | 54095745 | 54095745 / 21817490 = 2.479 | 26449228…38107282 | true | true | 513 | 131200 |

Sparse max-small activation also passed; its route remains retained after trim in both versions, so the oracle does not require unlink there. Полное время процесса включает запуск, вывод, создание потоков и teardown; work — только внутреннее окно. В таблице ниже обе оси используют средние, а не медианы; CI относится к среднему paired delta десяти блоков.

| Scenario | Cycles | Metric | Before mean ns | Candidate mean ns | Candidate/baseline mean | Paired mean delta 95% CI ns |
| --- | --- | --- | --- | --- | --- | --- |
| registration | 30 | work_per_cycle_mean_ns | 119213 | 152074 | 1.276 | 14317…51404 |
| registration | 30 | process_wall_total_ns | 12542580 | 13291590 | 1.060 | -659881…2157901 |
| registration | 60 | work_per_cycle_mean_ns | 113318 | 146307 | 1.291 | 23086…42891 |
| registration | 60 | process_wall_total_ns | 18388885 | 20653085 | 1.123 | 832941…3695459 |
| registration | 120 | work_per_cycle_mean_ns | 109598 | 154424 | 1.409 | 36515…53138 |
| registration | 120 | process_wall_total_ns | 29484110 | 36856790 | 1.250 | 5336742…9408618 |
| dense16 | 1 | work_per_cycle_mean_ns | 21817490 | 54095745 | 2.479 | 26449228…38107282 |
| dense16 | 1 | process_wall_total_ns | 29718425 | 62615435 | 2.107 | 26774759…39019261 |
| sparse_max_small | 1 | work_per_cycle_mean_ns | 90435 | 92575 | 1.024 | -9512…13792 |
| sparse_max_small | 1 | process_wall_total_ns | 5253010 | 5480005 | 1.043 | 25797…428193 |

## First allocation and retained-pin memory

Windows working-set/RSS and commit are reported separately; System free counters prove logical sidecar release, not immediate OS page release. Linux PSS and `Private_Dirty` come from `/proc/self/smaps_rollup` in WSL2 fresh processes. The retained-pin oracle asserts no sidecar free at owner unlink and exact backing free on final pin drop in every sample.

| Memory axis | Metric/stage | Before median | Candidate median | Paired mean delta | Paired 95% CI Δ |
| --- | --- | --- | --- | --- | --- |
| Windows | sidecar_size_bytes | 41088.0 | 65600.0 | 24512.0 | 24512.0…24512.0 |
| Windows | rss_after_registration_kib | 3020.0 | 2976.0 | -45.6 | -47.5…-43.7 |
| Windows | commit_after_registration_kib | 8882.0 | 8904.0 | 22.6 | 19.6…25.6 |
| Windows | commit_after_last_pin_drop_kib | 8882.0 | 8904.0 | 22.6 | 19.6…25.6 |
| WSL Linux | pss_registration_kib | 570.0 | 538.0 | -36.5 | -44.4…-28.6 |
| WSL Linux | pss_owner_unlink_pin_held_kib | 574.0 | 542.0 | -35.7 | -43.0…-28.4 |
| WSL Linux | private_dirty_registration_kib | 176.0 | 144.0 | -35.2 | -36.5…-33.9 |
| WSL Linux | private_dirty_owner_unlink_pin_held_kib | 180.0 | 148.0 | -35.2 | -36.5…-33.9 |
| WSL Linux | private_dirty_root_free_kib | 176.0 | 144.0 | -31.7 | -32.7…-30.7 |

The first-allocation probe calls `SeferAlloc::alloc` directly (it deliberately does not install a process `#[global_allocator]`) to control when bootstrap begins. Samples are 30 fresh processes per source.

| Metric | Unit | Before median | Candidate median | Candidate / baseline = numerator / denominator |
| --- | --- | --- | --- | --- |
| first_alloc_latency_ns | ns | 55100 | 44400 | 44400 / 55100 = 0.806 |
| rss_delta_1_heap_kib | KiB | 168 | 132 | 132 / 168 = 0.786 |
| rss_delta_8_heaps_kib | KiB | 1108 | 834 | 834 / 1108 = 0.753 |
| rss_delta_64_heaps_kib | KiB | 6918 | 5020 | 5020 / 6918 = 0.726 |
| commit_delta_1_heap_kib | KiB | 1188 | 1212 | 1212 / 1188 = 1.020 |
| commit_delta_8_heaps_kib | KiB | 7120 | 7340 | 7340 / 7120 = 1.031 |
| commit_delta_64_heaps_kib | KiB | 48896 | 50456 | 50456 / 48896 = 1.032 |

The Windows probe includes RSS/commit deltas at one, eight, and 64 heaps; no Linux PSS claim is made for these Windows runs. Raw samples and derived summaries are `first_alloc_*_samples.json` and `first_alloc_*_summary.json`.

The retained-pin oracle is a component-level `RouteDirectory::register` / `RoutePin` owner-unlink/final-release probe, not allocator throughput. Windows uses process RSS/VM-size and System allocation counters; Linux WSL uses `smaps_rollup` PSS and `Private_Dirty`.

Candidate backing allocation grows from the baseline sidecar size. Более низкие resident/PSS/private-dirty values — наблюдаемая разница этого probe, не доказательство её единственной причины и не гарантия снижения reserved memory или OS commit. Memory telemetry сама выделяет временные буферы; snapshots не атомарны. First-allocation arms запускались последовательно, без balanced paired CI, и являются описательными.

## Decision

The candidate is **NO-GO / NOT ACCEPTED** for the current design. The decisive evidence is not a single aggregate average: canonical wall-clock contains material regressions, the corrected MT Larson harness shows significant T=4/T=8 deterioration, dense16 and repeated registration timing regress, and the IAI marginal axis does not mirror the favorable raw-total changes. The installed-global paired workload alone is inconclusive for candidate versus baseline; beating mimalloc on that one workload cannot override the regressions. The WSL memory decrease is a distinct, non-native observation and does not change this verdict.

## Ограниченная zero-trust проверка доставки

- Product `src/`, Cargo.lock и production composition в этой доставке не изменены. Новых public hooks нет. Unsafe-сайты новых probes/test — `System.alloc/dealloc` и прямые `SeferAlloc.alloc/dealloc`; layouts валидны, указатели остаются live до освобождения, route/pin уничтожаются до System backing. Это ручная проверка границ unsafe/RAII, не новый Miri receipt.
- MT correction сохраняет receivers до завершения producers и последнего drain; exact operation assertions отвергли первый неполный прогон. Старый MT run1 исключён из verdict. Original baseline run со shared Criterion target также исключён; source-identity failures WSL произошли до сбора samples.
- Регрессионный тест проверяет zero/two constructed chunks и точный final-pin free; eager construction нарушила бы исходный zero-chunk assert, premature free — счётчик при retained pin. Тест typechecked, локально не исполнялся; delivery CI пока не подтверждён.
- No-build проверки: Windows targeted cargo check и Clippy -D warnings с production, production internals, production internals bench-internals; workspace cargo fmt --check через WSL; Linux oracle Clippy. Report generator --check, JS syntax и structure guard прошли. Полный независимый аудит всех rust-intel модулей не выполнялся; подагенты не запускались.

## Evidence and reproduction

- Machine-readable companion: `D1_PRIME_CHUNKED_SIDECAR_GATE_summary.csv` (generated by the same script as this report).
- Raw logs and per-sample structured data: `docs/perf/d1_prime_chunked_sidecar/raw/`; cited files are under 200 KiB uncompressed or gzip-compressed per repository policy.
- Derived axis inputs: `docs/perf/d1_prime_chunked_sidecar/summaries/`; measurement and derivation scripts are archived in `docs/perf/d1_prime_chunked_sidecar/reproduction/`.
- Regenerate this report and summary: `node scripts/d1_prime_chunked_sidecar_report.mjs`.
- Wall-clock raw-to-table derivation: archived `derive-wallclock.mjs`; IAI derivation: archived `derive-iai.mjs`.
