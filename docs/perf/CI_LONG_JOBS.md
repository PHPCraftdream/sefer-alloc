# Длительные jobs CI — наблюдения

> Только измерения и ограниченные верхние границы; вердикта/рекомендации нет.

## Coverage

Full-selected completed attempts: 6/6; successful: 3; failed/censored attempts: 3. This is not a claim of six successful full runs. Main push comparison: 1.
Attempts: 43410623227 schedule/failure 2026-11-13T10:38:25Z; 43582569269 schedule/failure 2026-11-14T09:52:11Z; 43704208363 schedule/success 2026-11-15T10:28:04Z; 43781371392 pull_request/success 2026-11-16T00:36:29Z; 43851422289 schedule/success 2026-11-16T10:43:41Z; 43886332211 schedule/failure 2026-11-16T14:29:53Z.
No-runner cancellation excluded from runner-time: 43886332211/133365637356 numa-shim real macOS kernel (unsupported-platform contract) runner_id=0/no steps. Four allowed weekly/arm64 skips are not executed runner jobs.
Stored evidence: 6 selected full attempts from collection.run_list, status completed; saved jobs matched total_count and stored snapshot coverage. Failed job statuses and step failures retained; failed durations are censored, not success samples.

## All six full-selected attempts — job-time totals

Job elapsed — runner-time proxy, not CPU utilization. Failed job durations remain censored attempt evidence, not successful-duration samples.

| run | measured job-min | top-8 / all, min | top-8 share | measured / eligible | skipped | no-runner excluded |
|---|---:|---:|---:|---:|---:|---:|
| 43410623227 · 2026-11-13T10:38:25Z (failure) | 318.7 | 201.4 / 318.7 | 63.2% | 59/59 | 4 | 0 |
| 43582569269 · 2026-11-14T09:52:11Z (failure) | 323.9 | 207.0 / 323.9 | 63.9% | 59/59 | 4 | 0 |
| 43704208363 · 2026-11-15T10:28:04Z (success) | 353.1 | 212.7 / 353.1 | 60.2% | 59/59 | 4 | 0 |
| 43781371392 · 2026-11-16T00:36:29Z (success) | 351.1 | 215.0 / 351.1 | 61.2% | 61/61 | 4 | 0 |
| 43851422289 · 2026-11-16T10:43:41Z (success) | 347.9 | 202.6 / 347.9 | 58.2% | 61/61 | 4 | 0 |
| 43886332211 · 2026-11-16T14:29:53Z (failure) | 402.2 | 228.4 / 402.2 | 56.8% | 63/63 | 1 | 1 |

Total 2096.8 job-min; per-run job-min median/range 349.5 (318.7–402.2; n=6); pooled top-8 1267.1 / 2096.8 min (60.4%).


## Successful full subset (separate; not attempt total)

Job elapsed — runner-time proxy, not CPU utilization. Failed job durations remain censored attempt evidence, not successful-duration samples.

| run | measured job-min | top-8 / all, min | top-8 share | measured / eligible | skipped | no-runner excluded |
|---|---:|---:|---:|---:|---:|---:|
| 43704208363 · 2026-11-15T10:28:04Z (success) | 353.1 | 212.7 / 353.1 | 60.2% | 59/59 | 4 | 0 |
| 43781371392 · 2026-11-16T00:36:29Z (success) | 351.1 | 215.0 / 351.1 | 61.2% | 61/61 | 4 | 0 |
| 43851422289 · 2026-11-16T10:43:41Z (success) | 347.9 | 202.6 / 347.9 | 58.2% | 61/61 | 4 | 0 |

Total 1052.1 job-min; per-run job-min median/range 351.1 (347.9–353.1; n=3); pooled top-8 630.3 / 1052.1 min (59.9%).


## Ordinary successful main pushes (separate)

Job elapsed — runner-time proxy, not CPU utilization. Failed job durations remain censored attempt evidence, not successful-duration samples.

| run | measured job-min | top-8 / all, min | top-8 share | measured / eligible | skipped | no-runner excluded |
|---|---:|---:|---:|---:|---:|---:|
| 43831250776 · 2026-11-16T08:26:42Z (success) | 4.2 | 4.0 / 4.2 | 96.4% | 9/9 | 53 | 0 |

Total 4.2 job-min; per-run job-min median/range 4.2 (4.2–4.2; n=1); pooled top-8 4.0 / 4.2 min (96.4%).


## Long jobs: all attempts, status, medians and range

Every job >600s in any selected attempt is included. Job median/range includes all measurable observations (including below 600s and failures); no-runner cancellation excluded. API step details retained below.

### check-matrix (manifest-driven, non-clippy rows) — 1728.0 (1650.0–1843.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 1706.0 | failure | Run ./.github/actions/ci-check-matrix (failure) |
| 43582569269 | 1650.0 | failure | Run ./.github/actions/ci-check-matrix (failure) |
| 43704208363 | 1830.0 | success | none |
| 43781371392 | 1843.0 | success | none |
| 43851422289 | 1661.0 | success | none |
| 43886332211 | 1750.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 0.0 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 3.5 (3.0–5.0; n=6) | none |
| Run ./.github/actions/ci-check-matrix | 1721.0 (1641.0–1836.0; n=6) | 43410623227: failure; 43582569269: failure |
| Post Run ./.github/actions/ci-check-matrix | 0.0 (0.0–0.0; n=6) | none |
| Post Run actions/checkout@v5 | 1.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### fuzz targets run (weekly, 10 min/target) — 2457.0 (2457.0–2457.0; n=1) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | н/д | skipped | none |
| 43582569269 | н/д | skipped | none |
| 43704208363 | н/д | skipped | none |
| 43781371392 | н/д | skipped | none |
| 43851422289 | н/д | skipped | none |
| 43886332211 | 2457.0 | failure | Run ./.github/actions/ci-fuzz-run (failure) |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (1.0–1.0; n=1) | none |
| Run actions/checkout@v5 | 3.0 (3.0–3.0; n=1) | none |
| Run ./.github/actions/ci-fuzz-run | 2448.0 (2448.0–2448.0; n=1) | 43886332211: failure |
| Post Run actions/checkout@v5 | 1.0 (1.0–1.0; n=1) | none |
| Complete job | 0.0 (0.0–0.0; n=1) | none |
### kani proofs — 1201.5 (1043.0–1291.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 1238.0 | success | none |
| 43582569269 | 1291.0 | success | none |
| 43704208363 | 1261.0 | success | none |
| 43781371392 | 1165.0 | success | none |
| 43851422289 | 1043.0 | success | none |
| 43886332211 | 1145.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (0.0–4.0; n=6) | none |
| Run actions/checkout@v5 | 4.0 (2.0–5.0; n=6) | none |
| Run ./.github/actions/ci-kani | 1194.0 (1030.0–1286.0; n=6) | none |
| Post Run actions/checkout@v5 | 0.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–1.0; n=6) | none |
### miri (core) — 1980.5 (1331.0–2172.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 1907.0 | success | none |
| 43582569269 | 2172.0 | success | none |
| 43704208363 | 1331.0 | success | none |
| 43781371392 | 1861.0 | success | none |
| 43851422289 | 2054.0 | success | none |
| 43886332211 | 2083.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 0.0 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 2.0 (2.0–3.0; n=6) | none |
| Run ./.github/actions/ci-miri-core | 1974.5 (1326.0–2167.0; n=6) | none |
| Post Run actions/checkout@v5 | 0.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### miri (core, pin cache TreeBorrows) — 1872.0 (1289.0–2282.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 1803.0 | success | none |
| 43582569269 | 1730.0 | success | none |
| 43704208363 | 2155.0 | success | none |
| 43781371392 | 1941.0 | success | none |
| 43851422289 | 2282.0 | success | none |
| 43886332211 | 1289.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (0.0–2.0; n=6) | none |
| Run actions/checkout@v5 | 4.0 (2.0–7.0; n=6) | none |
| Run ./.github/actions/ci-miri-core-tb | 1866.0 (1281.0–2273.0; n=6) | none |
| Post Run actions/checkout@v5 | 0.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### miri (fastbin) — 1958.5 (1603.0–2071.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 1974.0 | success | none |
| 43582569269 | 1943.0 | success | none |
| 43704208363 | 2045.0 | success | none |
| 43781371392 | 1848.0 | success | none |
| 43851422289 | 1603.0 | success | none |
| 43886332211 | 2071.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 0.5 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 3.5 (2.0–4.0; n=6) | none |
| Run ./.github/actions/ci-miri-fastbin | 1951.0 (1595.0–2066.0; n=6) | none |
| Post Run actions/checkout@v5 | 0.0 (0.0–0.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### miri-plain — 1370.5 (961.0–1701.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 1634.0 | success | none |
| 43582569269 | 1701.0 | success | none |
| 43704208363 | 1265.0 | success | none |
| 43781371392 | 1270.0 | success | none |
| 43851422289 | 961.0 | success | none |
| 43886332211 | 1471.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 3.0 (2.0–4.0; n=6) | none |
| Run ./.github/actions/ci-miri-plain | 1363.0 (954.0–1696.0; n=6) | none |
| Post Run actions/checkout@v5 | 0.0 (0.0–0.0; n=6) | none |
| Complete job | 0.0 (0.0–1.0; n=6) | none |
### test (aarch64-unknown-linux-gnu) — 1436.0 (552.0–1985.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 552.0 | failure | Run cross-thread tests (cross) (failure) |
| 43582569269 | 568.0 | failure | Run cross-thread tests (cross) (failure) |
| 43704208363 | 1742.0 | success | none |
| 43781371392 | 1985.0 | success | none |
| 43851422289 | 1433.0 | success | none |
| 43886332211 | 1439.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (1.0–2.0; n=6) | none |
| Run actions/checkout@v5 | 4.0 (3.0–7.0; n=6) | none |
| Run dtolnay/rust-toolchain@stable | 13.5 (11.0–14.0; n=6) | none |
| Install cross | 3.0 (1.0–4.0; n=6) | none |
| Run tests (native) | н/д | none |
| Run cross-thread tests (native) | н/д | none |
| Run cross-thread + decommit tests (native) | н/д | none |
| Run tests (cross) | 122.5 (91.0–135.0; n=6) | none |
| Run cross-thread tests (cross) | 431.5 (394.0–599.0; n=6) | 43410623227: failure; 43582569269: failure |
| Run cross-thread + decommit tests (cross) | 495.0 (443.0–624.0; n=4) | none |
| Run production tests (cross) | 487.0 (441.0–613.0; n=4) | none |
| Post Run actions/checkout@v5 | 0.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### test (feature isolation) — 558.5 (483.0–617.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 489.0 | failure | Run ./.github/actions/ci-test-feature-isolation (failure) |
| 43582569269 | 617.0 | failure | Run ./.github/actions/ci-test-feature-isolation (failure) |
| 43704208363 | 483.0 | success | none |
| 43781371392 | 555.0 | success | none |
| 43851422289 | 609.0 | success | none |
| 43886332211 | 562.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 0.0 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 3.0 (2.0–5.0; n=6) | none |
| Run ./.github/actions/ci-test-feature-isolation | 552.5 (476.0–609.0; n=6) | 43410623227: failure; 43582569269: failure |
| Post Run actions/checkout@v5 | 0.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### test (gated bodies + all-features) — 852.5 (573.0–1051.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 573.0 | failure | Run ./.github/actions/ci-test-gated-bodies (failure) |
| 43582569269 | 669.0 | failure | Run ./.github/actions/ci-test-gated-bodies (failure) |
| 43704208363 | 842.0 | success | none |
| 43781371392 | 957.0 | success | none |
| 43851422289 | 863.0 | success | none |
| 43886332211 | 1051.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (0.0–2.0; n=6) | none |
| Run actions/checkout@v5 | 4.0 (3.0–9.0; n=6) | none |
| Run ./.github/actions/ci-test-gated-bodies | 844.5 (565.0–1036.0; n=6) | 43410623227: failure; 43582569269: failure |
| Post Run actions/checkout@v5 | 1.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### test (hardened tier) — 940.5 (837.0–1042.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 837.0 | failure | Run ./.github/actions/ci-test-hardened (failure) |
| 43582569269 | 937.0 | failure | Run ./.github/actions/ci-test-hardened (failure) |
| 43704208363 | 1042.0 | success | none |
| 43781371392 | 949.0 | success | none |
| 43851422289 | 923.0 | success | none |
| 43886332211 | 944.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 0.5 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 3.0 (2.0–4.0; n=6) | none |
| Run ./.github/actions/ci-test-hardened | 933.5 (831.0–1036.0; n=6) | 43410623227: failure; 43582569269: failure |
| Post Run actions/checkout@v5 | 0.0 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### test (xthread progression) — 992.0 (921.0–1131.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 986.0 | failure | Run ./.github/actions/ci-test-xthread (failure) |
| 43582569269 | 998.0 | failure | Run ./.github/actions/ci-test-xthread (failure) |
| 43704208363 | 1131.0 | success | none |
| 43781371392 | 986.0 | success | none |
| 43851422289 | 921.0 | success | none |
| 43886332211 | 1098.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 1.0 (0.0–3.0; n=6) | none |
| Run actions/checkout@v5 | 3.0 (3.0–5.0; n=6) | none |
| Run ./.github/actions/ci-test-xthread | 985.5 (908.0–1123.0; n=6) | 43410623227: failure; 43582569269: failure |
| Post Run actions/checkout@v5 | 0.5 (0.0–1.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |
### test windows (production) — 846.5 (408.0–1121.0; n=6) s

| run | seconds | conclusion | failed steps |
|---|---:|---|---|
| 43410623227 | 470.0 | failure | Run ./.github/actions/ci-test-windows (failure) |
| 43582569269 | 408.0 | failure | Run ./.github/actions/ci-test-windows (failure) |
| 43704208363 | 873.0 | success | none |
| 43781371392 | 820.0 | success | none |
| 43851422289 | 1119.0 | success | none |
| 43886332211 | 1121.0 | success | none |

| API step | elapsed median (range; n), s | failures |
|---|---:|---|
| Set up job | 0.0 (0.0–1.0; n=6) | none |
| Run actions/checkout@v5 | 9.5 (7.0–10.0; n=6) | none |
| Run ./.github/actions/ci-test-windows | 833.5 (393.0–1107.0; n=6) | 43410623227: failure; 43582569269: failure |
| Post Run ./.github/actions/ci-test-windows | 1.0 (0.0–2.0; n=6) | none |
| Post Run actions/checkout@v5 | 2.0 (2.0–2.0; n=6) | none |
| Complete job | 0.0 (0.0–0.0; n=6) | none |

## Log-derived phase measurements

Schema ci-long-jobs-log-phases-v1, parserVersion 3; 26/26 stored log records are from the selected six attempts. Denominator is each measured log's own API job interval (not the six-run sum).

| job | run/job | conclusion | duration s | setup | toolchain | cache restore/save | build | execution | other | unknown | build/log | execution/log |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| check-matrix (manifest-driven, non-clippy rows) | 43410623227/131835907176 | failure | 1706.0 | 3.0 | 11.3 | 0.0 / 0.0 | 298.0 | 1381.6 | 10.3 | 1.9 | 298.0/1706.0 (17.5%) | 1381.6/1706.0 (81.0%) |
| check-matrix (manifest-driven, non-clippy rows) | 43582569269/132410260146 | failure | 1650.0 | 4.2 | 8.2 | 0.0 / 0.0 | 251.9 | 1373.7 | 10.1 | 1.9 | 251.9/1650.0 (15.3%) | 1373.7/1650.0 (83.3%) |
| check-matrix (manifest-driven, non-clippy rows) | 43704208363/132795418938 | success | 1830.0 | 4.8 | 10.3 | 0.0 / 0.0 | 314.0 | 1476.5 | 22.0 | 2.4 | 314.0/1830.0 (17.2%) | 1476.5/1830.0 (80.7%) |
| kani proofs | 43410623227/131835907570 | success | 1238.0 | 6.8 | 16.9 | 0.0 / 0.0 | 3.5 | 1200.8 | 4.8 | 5.2 | 3.5/1238.0 (0.3%) | 1200.8/1238.0 (97.0%) |
| miri (core) | 43410623227/131835907208 | success | 1907.0 | 2.5 | 31.7 | 0.0 / 0.0 | 44.3 | 1818.0 | 3.0 | 7.5 | 44.3/1907.0 (2.3%) | 1818.0/1907.0 (95.3%) |
| miri (core) | 43582569269/132410260038 | success | 2172.0 | 2.4 | 26.9 | 0.0 / 0.0 | 39.1 | 2094.4 | 2.8 | 6.5 | 39.1/2172.0 (1.8%) | 2094.4/2172.0 (96.4%) |
| miri (core) | 43704208363/132795418733 | success | 1331.0 | 3.5 | 19.5 | 0.0 / 0.0 | 28.7 | 1271.0 | 2.3 | 6.0 | 28.7/1331.0 (2.2%) | 1271.0/1331.0 (95.5%) |
| miri (core, pin cache TreeBorrows) | 43410623227/131835907233 | success | 1803.0 | 2.7 | 27.5 | 0.0 / 0.0 | 16.5 | 1753.6 | 1.3 | 1.4 | 16.5/1803.0 (0.9%) | 1753.6/1803.0 (97.3%) |
| miri (core, pin cache TreeBorrows) | 43582569269/132410260048 | success | 1730.0 | 6.9 | 22.4 | 0.0 / 0.0 | 24.1 | 1670.3 | 2.3 | 3.9 | 24.1/1730.0 (1.4%) | 1670.3/1730.0 (96.6%) |
| miri (core, pin cache TreeBorrows) | 43704208363/132795418960 | success | 2155.0 | 4.2 | 29.1 | 0.0 / 0.0 | 21.9 | 2097.2 | 1.0 | 1.6 | 21.9/2155.0 (1.0%) | 2097.2/2155.0 (97.3%) |
| miri (fastbin) | 43410623227/131835907334 | success | 1974.0 | 4.3 | 31.9 | 0.0 / 0.0 | 22.4 | 1907.2 | 6.6 | 1.7 | 22.4/1974.0 (1.1%) | 1907.2/1974.0 (96.6%) |
| miri (fastbin) | 43582569269/132410260141 | success | 1943.0 | 3.1 | 27.1 | 0.0 / 0.0 | 22.3 | 1882.4 | 6.5 | 1.6 | 22.3/1943.0 (1.1%) | 1882.4/1943.0 (96.9%) |
| miri (fastbin) | 43704208363/132795418911 | success | 2045.0 | 3.9 | 27.0 | 0.0 / 0.0 | 27.2 | 1979.3 | 5.8 | 1.8 | 27.2/2045.0 (1.3%) | 1979.3/2045.0 (96.8%) |
| miri-plain | 43410623227/131835907640 | success | 1634.0 | 2.5 | 33.8 | 0.0 / 0.0 | 22.4 | 1571.4 | 1.5 | 2.4 | 22.4/1634.0 (1.4%) | 1571.4/1634.0 (96.2%) |
| miri-plain | 43582569269/132410260402 | success | 1701.0 | 2.8 | 27.8 | 0.0 / 0.0 | 22.9 | 1643.4 | 2.4 | 1.7 | 22.9/1701.0 (1.3%) | 1643.4/1701.0 (96.6%) |
| miri-plain | 43704208363/132795418762 | success | 1265.0 | 3.8 | 23.1 | 0.0 / 0.0 | 19.8 | 1214.6 | 1.9 | 1.8 | 19.8/1265.0 (1.6%) | 1214.6/1265.0 (96.0%) |
| test (aarch64-unknown-linux-gnu) | 43704208363/132795418958 | success | 1742.0 | 3.7 | 39.3 | 0.0 / 0.0 | 241.4 | 1444.5 | 10.5 | 2.6 | 241.4/1742.0 (13.9%) | 1444.5/1742.0 (82.9%) |
| test (feature isolation) | 43582569269/132410260356 | failure | 617.0 | 3.1 | 10.8 | 0.0 / 0.0 | 132.5 | 463.8 | 3.3 | 3.6 | 132.5/617.0 (21.5%) | 463.8/617.0 (75.2%) |
| test (gated bodies + all-features) | 43582569269/132410260379 | failure | 669.0 | 4.0 | 8.5 | 0.0 / 0.0 | 154.6 | 493.8 | 4.9 | 3.3 | 154.6/669.0 (23.1%) | 493.8/669.0 (73.8%) |
| test (hardened tier) | 43410623227/131835907493 | failure | 837.0 | 2.3 | 8.2 | 0.0 / 0.0 | 165.3 | 650.6 | 8.3 | 2.3 | 165.3/837.0 (19.8%) | 650.6/837.0 (77.7%) |
| test (hardened tier) | 43582569269/132410260031 | failure | 937.0 | 2.8 | 10.1 | 0.0 / 0.0 | 192.6 | 721.7 | 6.8 | 3.0 | 192.6/937.0 (20.6%) | 721.7/937.0 (77.0%) |
| test (xthread progression) | 43410623227/131835906890 | failure | 986.0 | 3.6 | 9.0 | 0.0 / 0.0 | 217.3 | 742.7 | 9.6 | 3.8 | 217.3/986.0 (22.0%) | 742.7/986.0 (75.3%) |
| test (xthread progression) | 43582569269/132410260009 | failure | 998.0 | 3.4 | 8.2 | 0.0 / 0.0 | 228.3 | 743.6 | 10.6 | 3.9 | 228.3/998.0 (22.9%) | 743.6/998.0 (74.5%) |
| test windows (production) | 43704208363/132795419078 | success | 873.0 | 7.9 | 17.9 | 0.0 / 0.0 | 302.1 | 520.6 | 17.4 | 7.2 | 302.1/873.0 (34.6%) | 520.6/873.0 (59.6%) |

### Per-job phase medians and shares

| job | measured logs n | setup | toolchain | restore | save | build | execution | other | unknown | build% median/range | execution% median/range | pooled build/log denominator | pooled execution/log denominator |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| check-matrix (manifest-driven, non-clippy rows) | 3 | 4.2 (3.0–4.8; n=3) | 10.3 (8.2–11.3; n=3) | 0.0 (0.0–0.0; n=3) | 0.0 (0.0–0.0; n=3) | 298.0 (251.9–314.0; n=3) | 1381.6 (1373.7–1476.5; n=3) | 10.3 (10.1–22.0; n=3) | 1.9 (1.9–2.4; n=3) | 17.2 (15.3–17.5; n=3)% | 81.0 (80.7–83.3; n=3)% | 863.9/5186.0 (16.7%) | 4231.8/5186.0 (81.6%) |
| fuzz targets run (weekly, 10 min/target) | 0 | н/д | н/д | н/д | н/д | н/д | н/д | н/д | н/д | н/д% | н/д% | 0.0/0.0 (н/д%) | 0.0/0.0 (н/д%) |
| kani proofs | 1 | 6.8 (6.8–6.8; n=1) | 16.9 (16.9–16.9; n=1) | 0.0 (0.0–0.0; n=1) | 0.0 (0.0–0.0; n=1) | 3.5 (3.5–3.5; n=1) | 1200.8 (1200.8–1200.8; n=1) | 4.8 (4.8–4.8; n=1) | 5.2 (5.2–5.2; n=1) | 0.3 (0.3–0.3; n=1)% | 97.0 (97.0–97.0; n=1)% | 3.5/1238.0 (0.3%) | 1200.8/1238.0 (97.0%) |
| miri (core) | 3 | 2.5 (2.4–3.5; n=3) | 26.9 (19.5–31.7; n=3) | 0.0 (0.0–0.0; n=3) | 0.0 (0.0–0.0; n=3) | 39.1 (28.7–44.3; n=3) | 1818.0 (1271.0–2094.4; n=3) | 2.8 (2.3–3.0; n=3) | 6.5 (6.0–7.5; n=3) | 2.2 (1.8–2.3; n=3)% | 95.5 (95.3–96.4; n=3)% | 112.1/5410.0 (2.1%) | 5183.4/5410.0 (95.8%) |
| miri (core, pin cache TreeBorrows) | 3 | 4.2 (2.7–6.9; n=3) | 27.5 (22.4–29.1; n=3) | 0.0 (0.0–0.0; n=3) | 0.0 (0.0–0.0; n=3) | 21.9 (16.5–24.1; n=3) | 1753.6 (1670.3–2097.2; n=3) | 1.3 (1.0–2.3; n=3) | 1.6 (1.4–3.9; n=3) | 1.0 (0.9–1.4; n=3)% | 97.3 (96.6–97.3; n=3)% | 62.5/5688.0 (1.1%) | 5521.1/5688.0 (97.1%) |
| miri (fastbin) | 3 | 3.9 (3.1–4.3; n=3) | 27.1 (27.0–31.9; n=3) | 0.0 (0.0–0.0; n=3) | 0.0 (0.0–0.0; n=3) | 22.4 (22.3–27.2; n=3) | 1907.2 (1882.4–1979.3; n=3) | 6.5 (5.8–6.6; n=3) | 1.7 (1.6–1.8; n=3) | 1.1 (1.1–1.3; n=3)% | 96.8 (96.6–96.9; n=3)% | 71.9/5962.0 (1.2%) | 5768.9/5962.0 (96.8%) |
| miri-plain | 3 | 2.8 (2.5–3.8; n=3) | 27.8 (23.1–33.8; n=3) | 0.0 (0.0–0.0; n=3) | 0.0 (0.0–0.0; n=3) | 22.4 (19.8–22.9; n=3) | 1571.4 (1214.6–1643.4; n=3) | 1.9 (1.5–2.4; n=3) | 1.8 (1.7–2.4; n=3) | 1.4 (1.3–1.6; n=3)% | 96.2 (96.0–96.6; n=3)% | 65.1/4600.0 (1.4%) | 4429.4/4600.0 (96.3%) |
| test (aarch64-unknown-linux-gnu) | 1 | 3.7 (3.7–3.7; n=1) | 39.3 (39.3–39.3; n=1) | 0.0 (0.0–0.0; n=1) | 0.0 (0.0–0.0; n=1) | 241.4 (241.4–241.4; n=1) | 1444.5 (1444.5–1444.5; n=1) | 10.5 (10.5–10.5; n=1) | 2.6 (2.6–2.6; n=1) | 13.9 (13.9–13.9; n=1)% | 82.9 (82.9–82.9; n=1)% | 241.4/1742.0 (13.9%) | 1444.5/1742.0 (82.9%) |
| test (feature isolation) | 1 | 3.1 (3.1–3.1; n=1) | 10.8 (10.8–10.8; n=1) | 0.0 (0.0–0.0; n=1) | 0.0 (0.0–0.0; n=1) | 132.5 (132.5–132.5; n=1) | 463.8 (463.8–463.8; n=1) | 3.3 (3.3–3.3; n=1) | 3.6 (3.6–3.6; n=1) | 21.5 (21.5–21.5; n=1)% | 75.2 (75.2–75.2; n=1)% | 132.5/617.0 (21.5%) | 463.8/617.0 (75.2%) |
| test (gated bodies + all-features) | 1 | 4.0 (4.0–4.0; n=1) | 8.5 (8.5–8.5; n=1) | 0.0 (0.0–0.0; n=1) | 0.0 (0.0–0.0; n=1) | 154.6 (154.6–154.6; n=1) | 493.8 (493.8–493.8; n=1) | 4.9 (4.9–4.9; n=1) | 3.3 (3.3–3.3; n=1) | 23.1 (23.1–23.1; n=1)% | 73.8 (73.8–73.8; n=1)% | 154.6/669.0 (23.1%) | 493.8/669.0 (73.8%) |
| test (hardened tier) | 2 | 2.5 (2.3–2.8; n=2) | 9.2 (8.2–10.1; n=2) | 0.0 (0.0–0.0; n=2) | 0.0 (0.0–0.0; n=2) | 178.9 (165.3–192.6; n=2) | 686.1 (650.6–721.7; n=2) | 7.5 (6.8–8.3; n=2) | 2.7 (2.3–3.0; n=2) | 20.2 (19.8–20.6; n=2)% | 77.4 (77.0–77.7; n=2)% | 357.9/1774.0 (20.2%) | 1372.2/1774.0 (77.4%) |
| test (xthread progression) | 2 | 3.5 (3.4–3.6; n=2) | 8.6 (8.2–9.0; n=2) | 0.0 (0.0–0.0; n=2) | 0.0 (0.0–0.0; n=2) | 222.8 (217.3–228.3; n=2) | 743.2 (742.7–743.6; n=2) | 10.1 (9.6–10.6; n=2) | 3.8 (3.8–3.9; n=2) | 22.5 (22.0–22.9; n=2)% | 74.9 (74.5–75.3; n=2)% | 445.6/1984.0 (22.5%) | 1486.3/1984.0 (74.9%) |
| test windows (production) | 1 | 7.9 (7.9–7.9; n=1) | 17.9 (17.9–17.9; n=1) | 0.0 (0.0–0.0; n=1) | 0.0 (0.0–0.0; n=1) | 302.1 (302.1–302.1; n=1) | 520.6 (520.6–520.6; n=1) | 17.4 (17.4–17.4; n=1) | 7.2 (7.2–7.2; n=1) | 34.6 (34.6–34.6; n=1)% | 59.6 (59.6–59.6; n=1)% | 302.1/873.0 (34.6%) | 520.6/873.0 (59.6%) |

### Slow parser actions and single-target execution

Action arrays provide phase-attributed action elapsed (not `##[endgroup]` marker time). Target entries are emitted only for parser-proven single explicit targets; top 5 per log.

| job/run | action / target label | occurrences | seconds | build / execution / unknown | source lines |
|---|---|---:|---:|---:|---|
| test (xthread progression)/43410623227 | test (--features production) | 1 | 228.5 | 45.9 / 181.7 / 0.5 | 12357–16575 |
| test (xthread progression)/43410623227 | test (--features "alloc-global alloc-xthread") | 1 | 225.9 | 40.2 / 185.0 / 0.4 | 317–4388 |
| test (xthread progression)/43410623227 | test (--features "alloc-global alloc-xthread alloc-decommit") | 1 | 205.2 | 26.1 / 178.5 / 0.3 | 4389–8434 |
| test (xthread progression)/43410623227 | test (--features "alloc-global alloc-xthread fastbin") | 1 | 201.7 | 23.6 / 177.1 / 0.7 | 8435–12346 |
| test (xthread progression)/43410623227 | target nl3_safe_xthread_churn_integrity (execution) | 2 spans | 0.7 | proven single target | 16952–17337 |
| test (xthread progression)/43410623227 | target nl3_xthread_remote_free_reuse_small_classes (execution) | 2 spans | 0.5 | proven single target | 16764–17149 |
| test (xthread progression)/43410623227 | target nl3_xthread_pingpong_reuse (execution) | 2 spans | 0.4 | proven single target | 16858–17243 |
| test (xthread progression)/43410623227 | target nl7_safe_large_realloc_integrity (execution) | 2 spans | 0.4 | proven single target | 17046–17432 |
| test (xthread progression)/43410623227 | target nl1_misuse_checks_state_byte_switch (execution) | 3 spans | 0.2 | proven single target | 17518–17812 |
| check-matrix (manifest-driven, non-clippy rows)/43410623227 | Run node scripts/run-check-matrix.mjs --keep-going --free-between-rows --kind check --kind test | 1 | 1685.3 | 298.0 / 1381.6 / 0.0 | 397–28191 |
| check-matrix (manifest-driven, non-clippy rows)/43410623227 | Run dtolnay/rust-toolchain@stable | 1 | 11.4 | 0.0 / 0.0 / 0.0 | 116–317 |
| check-matrix (manifest-driven, non-clippy rows)/43410623227 | rustup toolchain install stable | 1 | 11.1 | 0.0 / 0.0 / 0.0 | 172–214 |
| check-matrix (manifest-driven, non-clippy rows)/43410623227 | Run node scripts/verify-dbg-hook-safety.mjs | 1 | 0.5 | 0.0 / 0.0 / 0.0 | 340–350 |
| miri (core)/43410623227 | b4-r1 pin cache narrow provenance (miss, same-base hit, delivery, TLS release) | 1 | 1088.8 | 0.0 / 1088.6 / 0.1 | 2721–2871 |
| miri (core)/43410623227 | decommit_miri_cycle | 1 | 121.6 | 1.4 / 120.0 / 0.2 | 4020–4118 |
| miri (core)/43410623227 | installed Box depot park/reload/recycle witness under TreeBorrows | 1 | 76.4 | 0.0 / 76.3 / 0.2 | 3168–3262 |
| miri (core)/43410623227 | regression_r2_05_diag_provenance_miri | 1 | 70.5 | 1.1 / 69.2 / 0.2 | 4643–4747 |
| miri (core)/43410623227 | target b4_r1_pin_cache_miri (execution) | 1 spans | 1088.6 | proven single target | 2809–2871 |
| miri (core)/43410623227 | target miri_global_box_acceptance (execution) | 6 spans | 146.4 | proven single target | 2620–3910 |
| miri (core)/43410623227 | target miri_depot_cycle (execution) | 2 spans | 131.2 | proven single target | 3161–3262 |
| miri (core)/43410623227 | target decommit_miri_cycle (execution) | 1 spans | 120.0 | proven single target | 4107–4116 |
| miri (core)/43410623227 | target regression_r2_05_diag_provenance_miri (execution) | 1 spans | 69.2 | proven single target | 4729–4745 |
| miri (core, pin cache TreeBorrows)/43410623227 | b4-r1 pin cache narrow provenance under TreeBorrows | 1 | 1789.7 | 16.5 / 1753.6 / 0.0 | 341–682 |
| miri (core, pin cache TreeBorrows)/43410623227 | Run dtolnay/rust-toolchain@nightly | 1 | 8.1 | 0.0 / 0.0 / 0.0 | 119–340 |
| miri (core, pin cache TreeBorrows)/43410623227 | rustup toolchain install nightly | 1 | 7.9 | 0.0 / 0.0 / 0.0 | 184–223 |
| miri (core, pin cache TreeBorrows)/43410623227 | Create cachekey | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 241–254 |
| miri (core, pin cache TreeBorrows)/43410623227 | target b4_r1_pin_cache_miri (execution) | 1 spans | 1753.6 | proven single target | 620–682 |
| miri (fastbin)/43410623227 | Run cargo miri test --features "alloc-global alloc-xthread alloc-decommit fastbin internals" --test regression_magazine_oracles --test regression_bump_direct_re | 1 | 1956.5 | 0.0 / 0.0 / 0.0 | 327–635 |
| miri (fastbin)/43410623227 | Run dtolnay/rust-toolchain@nightly | 1 | 8.6 | 0.0 / 0.0 / 0.0 | 117–326 |
| miri (fastbin)/43410623227 | rustup toolchain install nightly | 1 | 8.3 | 0.0 / 0.0 / 0.0 | 178–216 |
| miri (fastbin)/43410623227 | Create cachekey | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 233–245 |
| test (hardened tier)/43410623227 | test (--features "production hardened") | 1 | 206.2 | 43.3 / 162.3 / 0.2 | 4473–8739 |
| test (hardened tier)/43410623227 | test (--features "hardened medium-classes") | 1 | 205.0 | 41.3 / 163.1 / 0.2 | 12975–17049 |
| test (hardened tier)/43410623227 | test (--features "hardened batch-api") | 1 | 203.7 | 41.1 / 162.2 / 0.1 | 8845–12964 |
| test (hardened tier)/43410623227 | test (--features hardened) | 1 | 201.5 | 37.9 / 163.0 / 0.4 | 317–4462 |
| test (hardened tier)/43410623227 | target r11_p3_small_sidecar_preflight (execution) | 1 spans | 0.0 | proven single target | 8824–8832 |
| kani proofs/43410623227 | cargo kani (all harnesses) | 1 | 1207.7 | 3.5 / 1200.8 / 1.0 | 378–35151 |
| kani proofs/43410623227 | cargo kani setup | 1 | 10.3 | 0.0 / 0.0 / 2.4 | 357–377 |
| kani proofs/43410623227 | Run dtolnay/rust-toolchain@stable | 1 | 9.2 | 0.0 / 0.0 / 0.0 | 115–316 |
| kani proofs/43410623227 | rustup toolchain install stable | 1 | 8.9 | 0.0 / 0.0 / 0.0 | 171–213 |
| miri-plain/43410623227 | Run cargo miri test --features "alloc-global alloc-xthread alloc-decommit internals bench-internals" --test regression_xthread_large_free_no_leak --test regress | 1 | 1559.0 | 0.0 / 0.0 / 0.0 | 327–727 |
| miri-plain/43410623227 | regression_r2_06_header_race_miri | 1 | 40.5 | 0.0 / 40.3 / 0.1 | 728–849 |
| miri-plain/43410623227 | tiny terminal owner invariant | 1 | 19.3 | 1.6 / 17.5 / 0.1 | 850–946 |
| miri-plain/43410623227 | Run dtolnay/rust-toolchain@nightly | 1 | 9.8 | 0.0 / 0.0 / 0.0 | 117–326 |
| miri-plain/43410623227 | target regression_r2_06_header_race_miri (execution) | 1 spans | 40.3 | proven single target | 812–845 |
| test (xthread progression)/43582569269 | test (--features production) | 1 | 231.8 | 47.4 / 183.8 / 0.3 | 12357–16575 |
| test (xthread progression)/43582569269 | test (--features "alloc-global alloc-xthread") | 1 | 229.8 | 46.1 / 182.9 / 0.4 | 317–4388 |
| test (xthread progression)/43582569269 | test (--features "alloc-global alloc-xthread alloc-decommit") | 1 | 206.3 | 26.8 / 179.0 / 0.3 | 4389–8434 |
| test (xthread progression)/43582569269 | test (--features "alloc-global alloc-xthread fastbin") | 1 | 203.5 | 24.0 / 178.9 / 0.3 | 8435–12346 |
| test (xthread progression)/43582569269 | target nl3_safe_xthread_churn_integrity (execution) | 2 spans | 0.7 | proven single target | 16952–17337 |
| test (xthread progression)/43582569269 | target nl3_xthread_remote_free_reuse_small_classes (execution) | 2 spans | 0.5 | proven single target | 16764–17149 |
| test (xthread progression)/43582569269 | target nl3_xthread_pingpong_reuse (execution) | 2 spans | 0.4 | proven single target | 16858–17243 |
| test (xthread progression)/43582569269 | target nl7_safe_large_realloc_integrity (execution) | 2 spans | 0.4 | proven single target | 17046–17432 |
| test (xthread progression)/43582569269 | target nl1_misuse_checks_state_byte_switch (execution) | 3 spans | 0.1 | proven single target | 17518–17812 |
| test (hardened tier)/43582569269 | test (--features "production hardened") | 1 | 234.3 | 51.9 / 181.8 / 0.3 | 4473–8739 |
| test (hardened tier)/43582569269 | test (--features "hardened batch-api") | 1 | 227.7 | 47.4 / 179.9 / 0.1 | 8845–12964 |
| test (hardened tier)/43582569269 | test (--features "hardened medium-classes") | 1 | 226.4 | 45.6 / 180.3 / 0.3 | 12975–17049 |
| test (hardened tier)/43582569269 | test (--features hardened) | 1 | 225.9 | 45.6 / 179.6 / 0.3 | 317–4462 |
| test (hardened tier)/43582569269 | target r11_p3_small_sidecar_preflight (execution) | 1 spans | 0.0 | proven single target | 8824–8832 |
| miri (core)/43582569269 | b4-r1 pin cache narrow provenance (miss, same-base hit, delivery, TLS release) | 1 | 1345.7 | 0.0 / 1345.6 / 0.1 | 2721–2871 |
| miri (core)/43582569269 | decommit_miri_cycle | 1 | 131.3 | 1.2 / 129.9 / 0.1 | 4020–4118 |
| miri (core)/43582569269 | installed Box depot park/reload/recycle witness under TreeBorrows | 1 | 81.5 | 0.0 / 81.4 / 0.1 | 3168–3262 |
| miri (core)/43582569269 | regression_r2_05_diag_provenance_miri | 1 | 67.7 | 1.0 / 66.5 / 0.1 | 4643–4747 |
| miri (core)/43582569269 | target b4_r1_pin_cache_miri (execution) | 1 spans | 1345.6 | proven single target | 2809–2871 |
| miri (core)/43582569269 | target miri_global_box_acceptance (execution) | 6 spans | 157.2 | proven single target | 2620–3910 |
| miri (core)/43582569269 | target miri_depot_cycle (execution) | 2 spans | 140.4 | proven single target | 3161–3262 |
| miri (core)/43582569269 | target decommit_miri_cycle (execution) | 1 spans | 129.9 | proven single target | 4107–4116 |
| miri (core)/43582569269 | target regression_r2_05_diag_provenance_miri (execution) | 1 spans | 66.5 | proven single target | 4729–4745 |
| miri (core, pin cache TreeBorrows)/43582569269 | b4-r1 pin cache narrow provenance under TreeBorrows | 1 | 1708.4 | 24.1 / 1670.3 / 0.0 | 341–672 |
| miri (core, pin cache TreeBorrows)/43582569269 | Run dtolnay/rust-toolchain@nightly | 1 | 8.6 | 0.0 / 0.0 / 0.0 | 119–340 |
| miri (core, pin cache TreeBorrows)/43582569269 | rustup toolchain install nightly | 1 | 8.5 | 0.0 / 0.0 / 0.0 | 184–223 |
| miri (core, pin cache TreeBorrows)/43582569269 | Create cachekey | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 241–254 |
| miri (core, pin cache TreeBorrows)/43582569269 | target b4_r1_pin_cache_miri (execution) | 1 spans | 1670.3 | proven single target | 620–672 |
| miri (fastbin)/43582569269 | Run cargo miri test --features "alloc-global alloc-xthread alloc-decommit fastbin internals" --test regression_magazine_oracles --test regression_bump_direct_re | 1 | 1926.9 | 0.0 / 0.0 / 0.0 | 327–635 |
| miri (fastbin)/43582569269 | Run dtolnay/rust-toolchain@nightly | 1 | 8.9 | 0.0 / 0.0 / 0.0 | 117–326 |
| miri (fastbin)/43582569269 | rustup toolchain install nightly | 1 | 8.6 | 0.0 / 0.0 / 0.0 | 178–216 |
| miri (fastbin)/43582569269 | Enable Cargo sparse registry | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 271–291 |
| check-matrix (manifest-driven, non-clippy rows)/43582569269 | Run node scripts/run-check-matrix.mjs --keep-going --free-between-rows --kind check --kind test | 1 | 1631.1 | 251.9 / 1373.7 / 0.0 | 397–28191 |
| check-matrix (manifest-driven, non-clippy rows)/43582569269 | Run dtolnay/rust-toolchain@stable | 1 | 8.3 | 0.0 / 0.0 / 0.0 | 116–317 |
| check-matrix (manifest-driven, non-clippy rows)/43582569269 | rustup toolchain install stable | 1 | 8.0 | 0.0 / 0.0 / 0.0 | 172–214 |
| check-matrix (manifest-driven, non-clippy rows)/43582569269 | Run actions/setup-node@v5 | 2 | 0.5 | 0.0 / 0.0 / 0.0 | 318–28212 |
| test (feature isolation)/43582569269 | test (--features "production virgin-zero-skip alloc-stats bench-internals") | 1 | 316.4 | 50.1 / 265.5 / 0.3 | 4635–9142 |
| test (feature isolation)/43582569269 | test (--features "production exact-span-large") | 1 | 263.9 | 64.9 / 198.2 / 0.4 | 317–4634 |
| test (feature isolation)/43582569269 | Run dtolnay/rust-toolchain@stable | 1 | 10.9 | 0.0 / 0.0 / 0.0 | 115–316 |
| test (feature isolation)/43582569269 | rustup toolchain install stable | 1 | 10.6 | 0.0 / 0.0 / 0.0 | 171–213 |
| test (feature isolation)/43582569269 | target r6_t09_batch_exact_narrow (execution) | 1 spans | 0.0 | proven single target | 9969–9974 |
| test (feature isolation)/43582569269 | target r19_prf02r_realloc_tail (execution) | 1 spans | 0.0 | proven single target | 9530–9538 |
| test (gated bodies + all-features)/43582569269 | test (--features "production alloc-stats bench-internals") | 1 | 308.9 | 56.2 / 252.0 / 0.3 | 317–4984 |
| test (gated bodies + all-features)/43582569269 | test (--all-features) | 1 | 270.3 | 49.6 / 220.0 / 0.3 | 8490–13216 |
| test (gated bodies + all-features)/43582569269 | b4-30 first-alloc stack budget (--all-features, linux) | 1 | 45.2 | 29.8 / 14.7 / 0.2 | 13217–13867 |
| test (gated bodies + all-features)/43582569269 | test (--features pinning) | 1 | 26.4 | 19.0 / 7.2 / 0.0 | 4985–8489 |
| miri-plain/43582569269 | Run cargo miri test --features "alloc-global alloc-xthread alloc-decommit internals bench-internals" --test regression_xthread_large_free_no_leak --test regress | 1 | 1622.5 | 0.0 / 0.0 / 0.0 | 327–727 |
| miri-plain/43582569269 | regression_r2_06_header_race_miri | 1 | 41.7 | 0.0 / 41.4 / 0.1 | 728–849 |
| miri-plain/43582569269 | tiny terminal owner invariant | 1 | 20.5 | 1.3 / 19.0 / 0.1 | 850–946 |
| miri-plain/43582569269 | Run dtolnay/rust-toolchain@nightly | 1 | 10.2 | 0.0 / 0.0 / 0.0 | 117–326 |
| miri-plain/43582569269 | target regression_r2_06_header_race_miri (execution) | 1 spans | 41.4 | proven single target | 812–845 |
| miri (core)/43704208363 | b4-r1 pin cache narrow provenance (miss, same-base hit, delivery, TLS release) | 1 | 807.4 | 0.0 / 807.3 / 0.1 | 2721–2871 |
| miri (core)/43704208363 | decommit_miri_cycle | 1 | 82.3 | 0.8 / 81.3 / 0.1 | 4020–4118 |
| miri (core)/43704208363 | installed Box depot park/reload/recycle witness under TreeBorrows | 1 | 46.7 | 0.0 / 46.6 / 0.1 | 3168–3262 |
| miri (core)/43704208363 | regression_r2_05_diag_provenance_miri | 1 | 42.8 | 0.7 / 42.0 / 0.1 | 4643–4747 |
| miri (core)/43704208363 | target b4_r1_pin_cache_miri (execution) | 1 spans | 807.3 | proven single target | 2809–2871 |
| miri (core)/43704208363 | target miri_global_box_acceptance (execution) | 6 spans | 98.5 | proven single target | 2620–3910 |
| miri (core)/43704208363 | target miri_depot_cycle (execution) | 2 spans | 85.9 | proven single target | 3161–3262 |
| miri (core)/43704208363 | target decommit_miri_cycle (execution) | 1 spans | 81.3 | proven single target | 4107–4116 |
| miri (core)/43704208363 | target regression_r2_05_diag_provenance_miri (execution) | 1 spans | 42.0 | proven single target | 4729–4745 |
| miri-plain/43704208363 | Run cargo miri test --features "alloc-global alloc-xthread alloc-decommit internals bench-internals" --test regression_xthread_large_free_no_leak --test regress | 1 | 1194.0 | 0.0 / 0.0 / 0.0 | 327–727 |
| miri-plain/43704208363 | regression_r2_06_header_race_miri | 1 | 38.4 | 0.0 / 38.1 / 0.1 | 728–849 |
| miri-plain/43704208363 | tiny terminal owner invariant | 1 | 18.1 | 1.1 / 16.7 / 0.1 | 850–946 |
| miri-plain/43704208363 | Run dtolnay/rust-toolchain@nightly | 1 | 7.9 | 0.0 / 0.0 / 0.0 | 117–326 |
| miri-plain/43704208363 | target regression_r2_06_header_race_miri (execution) | 1 spans | 38.1 | proven single target | 812–845 |
| miri (fastbin)/43704208363 | Run cargo miri test --features "alloc-global alloc-xthread alloc-decommit fastbin internals" --test regression_magazine_oracles --test regression_bump_direct_re | 1 | 2028.9 | 0.0 / 0.0 / 0.0 | 327–635 |
| miri (fastbin)/43704208363 | Run dtolnay/rust-toolchain@nightly | 1 | 8.9 | 0.0 / 0.0 / 0.0 | 117–326 |
| miri (fastbin)/43704208363 | rustup toolchain install nightly | 1 | 8.6 | 0.0 / 0.0 / 0.0 | 178–216 |
| miri (fastbin)/43704208363 | Create cachekey | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 233–245 |
| check-matrix (manifest-driven, non-clippy rows)/43704208363 | Run node scripts/run-check-matrix.mjs --keep-going --free-between-rows --kind check --kind test | 1 | 1803.0 | 314.0 / 1476.5 / 0.0 | 397–28798 |
| check-matrix (manifest-driven, non-clippy rows)/43704208363 | Run dtolnay/rust-toolchain@stable | 1 | 10.4 | 0.0 / 0.0 / 0.0 | 116–317 |
| check-matrix (manifest-driven, non-clippy rows)/43704208363 | rustup toolchain install stable | 1 | 10.0 | 0.0 / 0.0 / 0.0 | 172–214 |
| check-matrix (manifest-driven, non-clippy rows)/43704208363 | Run node scripts/verify-gate-report.mjs | 1 | 1.9 | 0.0 / 0.0 / 0.0 | 28814–30524 |
| test (aarch64-unknown-linux-gnu)/43704208363 | rustup toolchain install stable | 1 | 11.4 | 0.0 / 0.0 / 0.0 | 166–208 |
| test (aarch64-unknown-linux-gnu)/43704208363 | Run bail() { | 1 | 3.9 | 0.0 / 0.0 / 0.0 | 321–377 |
| test (aarch64-unknown-linux-gnu)/43704208363 | Create cachekey | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 224–235 |
| test (aarch64-unknown-linux-gnu)/43704208363 | Enable Cargo sparse registry | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 259–278 |
| miri (core, pin cache TreeBorrows)/43704208363 | b4-r1 pin cache narrow provenance under TreeBorrows | 1 | 2137.8 | 21.9 / 2097.2 / 0.0 | 341–682 |
| miri (core, pin cache TreeBorrows)/43704208363 | Run dtolnay/rust-toolchain@nightly | 1 | 10.5 | 0.0 / 0.0 / 0.0 | 119–340 |
| miri (core, pin cache TreeBorrows)/43704208363 | rustup toolchain install nightly | 1 | 10.3 | 0.0 / 0.0 / 0.0 | 184–223 |
| miri (core, pin cache TreeBorrows)/43704208363 | Enable Cargo sparse registry | 1 | 0.0 | 0.0 / 0.0 / 0.0 | 282–303 |
| miri (core, pin cache TreeBorrows)/43704208363 | target b4_r1_pin_cache_miri (execution) | 1 spans | 2097.2 | proven single target | 620–682 |
| test windows (production)/43704208363 | Run cargo test --features "production internals" --no-fail-fast | 1 | 257.9 | 87.0 / 167.9 / 1.8 | 360–4728 |
| test windows (production)/43704208363 | Run cargo test --features "production exact-span-large large-reserved-capacity internals" --no-fail-fast | 1 | 233.4 | 0.0 / 0.0 / 0.0 | 10410–14628 |
| test windows (production)/43704208363 | Run cargo test --features "alloc-global alloc-xthread fastbin internals" --no-fail-fast | 1 | 231.4 | 60.9 / 168.6 / 0.7 | 6422–10409 |
| test windows (production)/43704208363 | b4-30 first-alloc stack budget (production internals, windows) | 1 | 41.3 | 34.3 / 6.0 / 0.2 | 4729–5354 |
| test windows (production)/43704208363 | target tagged_index_stack_ab_runner_scratch_guard (execution) | 1 spans | 7.2 | proven single target | 16037–16056 |
| test windows (production)/43704208363 | target tagged_index_stack_compile_fail (execution) | 1 spans | 0.3 | proven single target | 15931–15946 |
| test windows (production)/43704208363 | target lazy_initial_commit_forced_page (execution) | 1 spans | 0.1 | proven single target | 6124–6131 |
| test windows (production)/43704208363 | target decomp_hooks_forced_page (execution) | 1 spans | 0.0 | proven single target | 6221–6229 |
| test windows (production)/43704208363 | target segment_state_reconciliation_oracle (execution) | 1 spans | 0.0 | proven single target | 6412–6418 |

COVERAGE GAP: 24/26 requested job logs succeeded/available; two Kani requests timed out (132410259960, 132795419134). Three long jobs in the Nov 15 run were unrequested (132795418746 xthread; 132795418785 hardened; 132795418889 gated/all-features); those plus timeout jobs leave 38 >600s attempt/job rows without log phases. Minimum of all long jobs across 3 selected runs was unmet; no extra GH requests were made.


## Cache sources and code-scoped levers

| long-job reachable cache source | observed restore/save |
|---|---|
| none in reachable long-job composites | 24 logs classify cache restore/save as 0s; missing timing is not implied zero cache work |

No Cargo cache action is reachable from the sampled long-job composites; `.github/actions/ci-licenses/action.yml` uses `actions/cache` for a license/pass marker, not Cargo build artifacts. Log parser classified restore/save as 0 in available logs.

| lever / job | build measured median (range; n), s | execution median (range; n), s | scope / cost bound |
|---|---:|---:|---|
| check-matrix (manifest-driven, non-clippy rows) | 298.0 (251.9–314.0; n=3) | 1381.6 (1373.7–1476.5; n=3) | scripts/run-check-matrix.mjs frees test executables/incremental between rows (relink/disk tradeoff); measured build is an upper bound, not total 20+ min job. The row-group sharding design is documented in [CI_CHECK_MATRIX_SHARDING.md](CI_CHECK_MATRIX_SHARDING.md); these measurements and display names are from the unsharded job. |
| kani proofs | 3.5 (3.5–3.5; n=1) | 1200.8 (1200.8–1200.8; n=1) | Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain. |
| miri (core) | 39.1 (28.7–44.3; n=3) | 1818.0 (1271.0–2094.4; n=3) | Sharding moves verification work; compile/setup replication costs remain; one-target pass cannot split by targets. |
| miri (core, pin cache TreeBorrows) | 21.9 (16.5–24.1; n=3) | 1753.6 (1670.3–2097.2; n=3) | Sharding moves verification work; compile/setup replication costs remain; one-target pass cannot split by targets. |
| miri (fastbin) | 22.4 (22.3–27.2; n=3) | 1907.2 (1882.4–1979.3; n=3) | Sharding moves verification work; compile/setup replication costs remain; one-target pass cannot split by targets. |
| miri-plain | 22.4 (19.8–22.9; n=3) | 1571.4 (1214.6–1643.4; n=3) | Sharding moves verification work; compile/setup replication costs remain; one-target pass cannot split by targets. |
| test (aarch64-unknown-linux-gnu) | 241.4 (241.4–241.4; n=1) | 1444.5 (1444.5–1444.5; n=1) | These job/display names and measurements are from the pre-sharding single aarch64 job; the current `multi-arch` workflow runs four parallel aarch64 legs by feature set (experimental / cross-thread / cross-thread-decommit / production) plus an unchanged native x86_64 leg, so per-job names no longer match the current workflow layout. |
| test (feature isolation) | 132.5 (132.5–132.5; n=1) | 463.8 (463.8–463.8; n=1) | Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain. |
| test (gated bodies + all-features) | 154.6 (154.6–154.6; n=1) | 493.8 (493.8–493.8; n=1) | Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain. |
| test (hardened tier) | 178.9 (165.3–192.6; n=2) | 686.1 (650.6–721.7; n=2) | Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain. |
| test (xthread progression) | 222.8 (217.3–228.3; n=2) | 743.2 (742.7–743.6; n=2) | Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain. |
| test windows (production) | 302.1 (302.1–302.1; n=1) | 520.6 (520.6–520.6; n=1) | Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain. |

Tier context: ordinary main push suppresses heavy seam/OS work; weekly-only jobs already run weekly, so moving cadence shifts PR latency rather than deleting CPU. Any per-job median is a measured wall-time bound, not a recommendation.

## Static compile-duplication candidates

Static exact command rows only; no multiline/YAML inference. Key retains cargo/cross, verb, package, features, explicit target scope, profile, toolchain, runner OS. This conservative inventory can miss continuations; a match is not proof of artifact/cache reuse.

- -p sefer-alloc-core --all-targets -- -D warnings (4 occurrences)
  - sefer-alloc-core-gates .github/actions/ci-sefer-alloc-core-gates/action.yml:9 [ubuntu-latest]
  - sefer-alloc-core-gates .github/actions/ci-sefer-alloc-core-gates/action.yml:12 [ubuntu-latest]
  - sefer-alloc-core-gates .github/actions/ci-sefer-alloc-core-gates/action.yml:15 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:440 [ubuntu-latest]
- -p sefer-registry --all-targets -- -D warnings (4 occurrences)
  - sefer-registry-gates .github/actions/ci-sefer-registry-gates/action.yml:9 [ubuntu-latest]
  - sefer-registry-gates .github/actions/ci-sefer-registry-gates/action.yml:12 [ubuntu-latest]
  - sefer-registry-gates .github/actions/ci-sefer-registry-gates/action.yml:15 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:457 [ubuntu-latest]
- -p sefer-registry --all-features --all-targets (2 occurrences)
  - sefer-registry-gates .github/actions/ci-sefer-registry-gates/action.yml:18 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:444 [ubuntu-latest]
- -p sefer-region --no-default-features (2 occurrences)
  - sefer-region-gates .github/actions/ci-sefer-region-gates/action.yml:20 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:55 [ubuntu-latest]
- -p aligned-vmem --all-features (4 occurrences)
  - aligned-vmem-gates .github/actions/ci-aligned-vmem-gates/action.yml:64 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:9 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:68 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:81 [ubuntu-latest]
- -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" 2>&1 | tee "$RUNNER_TEMP/vmem-mock-debug.log" (2 occurrences)
  - aligned-vmem-gates .github/actions/ci-aligned-vmem-gates/action.yml:108 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:82 [ubuntu-latest]
- -p aligned-vmem --all-features (2 occurrences)
  - aligned-vmem-gates .github/actions/ci-aligned-vmem-gates/action.yml:210 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:78 [ubuntu-latest]
- 2>&1 | tee "$RUNNER_TEMP/tagged-index-stack-packaged-repository-cfg.log" (3 occurrences)
  - tagged-index-stack-gates .github/actions/ci-tagged-index-stack-gates/action.yml:48 [ubuntu-latest]
  - test-gated-bodies .github/actions/ci-test-gated-bodies/action.yml:67 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:10 [ubuntu-latest]
- --message-format=json >"$loom_check_json" 2>"$loom_check_stderr" (2 occurrences)
  - tagged-index-stack-gates .github/actions/ci-tagged-index-stack-gates/action.yml:71 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:7 [ubuntu-latest]
- -p tagged-index-stack --no-fail-fast (2 occurrences)
  - test-workspace .github/actions/ci-test-workspace/action.yml:18 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:171 [ubuntu-latest]
- -p size-classes --all-features --no-fail-fast (3 occurrences)
  - test-workspace .github/actions/ci-test-workspace/action.yml:138 [ubuntu-latest]
  - test-workspace .github/actions/ci-test-workspace/action.yml:170 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:134 [ubuntu-latest]
- -p sefer-ghost --features internals --no-fail-fast (2 occurrences)
  - test-workspace .github/actions/ci-test-workspace/action.yml:201 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:159 [ubuntu-latest]
- -p once-ptr-cell --no-fail-fast (2 occurrences)
  - test-workspace .github/actions/ci-test-workspace/action.yml:222 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:118 [ubuntu-latest]
- -p globalalloc-model --all-features --no-fail-fast (2 occurrences)
  - test-workspace .github/actions/ci-test-workspace/action.yml:278 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:191 [ubuntu-latest]
- -p numa-shim --no-run --all-features (4 occurrences)
  - msrv .github/actions/ci-msrv/action.yml:93 [ubuntu-latest]
  - msrv .github/actions/ci-msrv/action.yml:99 [ubuntu-latest]
  - numa-shim-mock .github/actions/ci-numa-shim-mock/action.yml:23 [ubuntu-latest]
  - numa-shim-mock .github/actions/ci-numa-shim-mock/action.yml:34 [ubuntu-latest]
- --features "alloc-global internals bench-internals" --test b4_r1_pin_cache_miri -- narrow 2>&1 | tee "$RUNNER_TEMP/miri-b4-r1-pin-cache.log" (2 occurrences)
  - miri-core-pin-cache .github/actions/ci-miri-core-pin-cache/action.yml:22 [ubuntu-latest]
  - miri-core-tb .github/actions/ci-miri-core-tb/action.yml:16 [ubuntu-latest]

Miri compile sharing is only hypothetical: execution passes with different MIRIFLAGS (including SB/TB) remain distinct verification; Windows/macOS/Linux rows are not cross-OS duplicates.

## Miri target attribution and bounded sharding

One target means zero target-splitting ceiling. For multiple targets, ideal 2-way wall-time saving cannot exceed execution/2 and is constrained by the largest measured explicit target. This is a theoretical bound on measured log execution, not CPU or whole-job time; build/setup replication and load imbalance remain extra costs.

| job/run | explicit targets / action commands | log execution s | largest known target s | 2-way theoretical saving upper s |
|---|---:|---:|---:|---:|
| 43410623227/131835906515 once-ptr-cell under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43410623227/131835907193 globalalloc-model under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43410623227/131835907208 miri (core) | 28 / 38 | 1818.0 | 1088.6 | 0–729.4 |
| 43410623227/131835907233 miri (core, pin cache TreeBorrows) | 1 / 1 | 1753.6 | 1753.6 | 0–0.0 |
| 43410623227/131835907334 miri (fastbin) | 4 / 3 | 1907.2 | 0.0 | 0–953.6 |
| 43410623227/131835907372 miri (alloc-core) | 2 / 3 | no log | no log | unknown |
| 43410623227/131835907579 numa-shim macOS under miri (mod platform cfg regression guard) | default/unknown / 0 | no log | no log | unknown |
| 43410623227/131835907616 aligned-vmem under miri (interpreter, not just --cfg miri check) | default/unknown / 0 | no log | no log | unknown |
| 43410623227/131835907640 miri-plain | 6 / 4 | 1571.4 | 40.3 | 0–785.7 |
| 43582569269/132410259772 aligned-vmem under miri (interpreter, not just --cfg miri check) | default/unknown / 0 | no log | no log | unknown |
| 43582569269/132410259958 miri (alloc-core) | 2 / 3 | no log | no log | unknown |
| 43582569269/132410260038 miri (core) | 28 / 38 | 2094.4 | 1345.6 | 0–748.8 |
| 43582569269/132410260048 miri (core, pin cache TreeBorrows) | 1 / 1 | 1670.3 | 1670.3 | 0–0.0 |
| 43582569269/132410260061 globalalloc-model under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43582569269/132410260082 once-ptr-cell under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43582569269/132410260141 miri (fastbin) | 4 / 3 | 1882.4 | 0.0 | 0–941.2 |
| 43582569269/132410260402 miri-plain | 6 / 4 | 1643.4 | 41.4 | 0–821.7 |
| 43582569269/132410260407 numa-shim macOS under miri (mod platform cfg regression guard) | default/unknown / 0 | no log | no log | unknown |
| 43704208363/132795418436 once-ptr-cell under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43704208363/132795418475 aligned-vmem under miri (interpreter, not just --cfg miri check) | default/unknown / 0 | no log | no log | unknown |
| 43704208363/132795418733 miri (core) | 28 / 38 | 1271.0 | 807.3 | 0–463.7 |
| 43704208363/132795418762 miri-plain | 6 / 4 | 1214.6 | 38.1 | 0–607.3 |
| 43704208363/132795418778 miri (alloc-core) | 2 / 3 | no log | no log | unknown |
| 43704208363/132795418894 globalalloc-model under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43704208363/132795418911 miri (fastbin) | 4 / 3 | 1979.3 | 0.0 | 0–989.7 |
| 43704208363/132795418960 miri (core, pin cache TreeBorrows) | 1 / 1 | 2097.2 | 2097.2 | 0–0.0 |
| 43704208363/132795418986 numa-shim macOS under miri (mod platform cfg regression guard) | default/unknown / 0 | no log | no log | unknown |
| 43781371392/133019282353 aligned-vmem under miri (interpreter, not just --cfg miri check) | default/unknown / 0 | no log | no log | unknown |
| 43781371392/133019282384 globalalloc-model under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43781371392/133019282620 once-ptr-cell under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43781371392/133019282675 miri (fastbin) | 4 / 3 | no log | no log | unknown |
| 43781371392/133019282678 numa-shim macOS under miri (mod platform cfg regression guard) | default/unknown / 0 | no log | no log | unknown |
| 43781371392/133019282717 miri (core, pin cache TreeBorrows) | 1 / 1 | no log | no log | unknown |
| 43781371392/133019282727 miri (core) | 28 / 38 | no log | no log | unknown |
| 43781371392/133019282732 miri (alloc-core) | 2 / 3 | no log | no log | unknown |
| 43781371392/133019282758 miri-plain | 6 / 4 | no log | no log | unknown |
| 43851422289/133244747145 aligned-vmem under miri (interpreter, not just --cfg miri check) | default/unknown / 0 | no log | no log | unknown |
| 43851422289/133244747168 globalalloc-model under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43851422289/133244747299 miri (core) | 28 / 38 | no log | no log | unknown |
| 43851422289/133244747615 miri (core, pin cache TreeBorrows) | 1 / 1 | no log | no log | unknown |
| 43851422289/133244747625 once-ptr-cell under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43851422289/133244747693 numa-shim macOS under miri (mod platform cfg regression guard) | default/unknown / 0 | no log | no log | unknown |
| 43851422289/133244747713 miri-plain | 6 / 4 | no log | no log | unknown |
| 43851422289/133244747871 miri (fastbin) | 4 / 3 | no log | no log | unknown |
| 43851422289/133244747905 miri (alloc-core) | 2 / 3 | no log | no log | unknown |
| 43886332211/133365634550 aligned-vmem under miri (interpreter, not just --cfg miri check) | default/unknown / 0 | no log | no log | unknown |
| 43886332211/133365635726 once-ptr-cell under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43886332211/133365636514 globalalloc-model under miri (interpreter, strict-provenance included) | default/unknown / 0 | no log | no log | unknown |
| 43886332211/133365636628 miri (fastbin) | 4 / 3 | no log | no log | unknown |
| 43886332211/133365636722 miri (alloc-core) | 2 / 3 | no log | no log | unknown |
| 43886332211/133365636764 miri (core) | 28 / 38 | no log | no log | unknown |
| 43886332211/133365636928 miri-plain | 6 / 4 | no log | no log | unknown |
| 43886332211/133365636939 miri (core, pin cache TreeBorrows) | 1 / 1 | no log | no log | unknown |
| 43886332211/133365637161 numa-shim macOS under miri (mod platform cfg regression guard) | default/unknown / 0 | no log | no log | unknown |

Miri SB/TB execution remains semantically distinct even where compile artifacts might be shared; a single-target pass cannot be sharded by target. Kani setup/toolchain (~10s plus ~2.7s installer in the logs) is distinct from its proof execution.

### Sharding plan (landed 2026-11-16; rebalanced 2026-11-17 from measured CI wall times)

The three divisible miri jobs are sharded by test target; `miri-core-tb` is a single target and stays whole. The union of each shard set equals the old job's command set exactly (mechanical proof: `node scripts/verify-miri-sharding.mjs`; a normalized record = target/package/features/MIRIFLAGS/post-`--` args/provenance model/borrows). Shards were rebalanced against per-target durations measured in run 43984907030; expected savings are ESTIMATES derived from the attribution table above, minus shard setup/toolchain replication and balance overhead.

| old job | shards | content | measured basis / expected old-execution split (estimate) |
|---|---|---|---|
| miri (core) | `miri-core-pin-cache` | the single dominant target `b4_r1_pin_cache_miri` (narrow, strict/SB) | 807.3–1345.6 s of 1271.0–2094.4 s execution — one target, so it alone bounds a shard; old execution is split ≈1346 s vs ≈690 s (unbalanced by construction, the floor is the target itself) |
| miri (core) | `miri-core-rest` | the other 39 commands (J13b guards/advice, Box witnesses incl. 4 EXPECTED-RED pins, leases, merged regressions) | ≈690 s of the measured execution, dominated by `miri_global_box_acceptance` 146.4–157.2 s and `miri_depot_cycle` 131.2–140.4 s |
| miri (fastbin) | `miri-fastbin-oracles` | `regression_merged -- regression_magazine_oracles::` | no per-target logs exist; split by the two action commands |
| miri (fastbin) | `miri-fastbin-integrity-nl7` | NL-7 safe large-realloc integrity suite only (`nl7_safe_large_realloc_integrity`, 2 #[test] fns) | measured 1329.55 s target (run 43984907030) — single target above 900 s gets its own shard; not further divisible without test-name filters (per-test durations unknown) |
| miri (fastbin) | `miri-fastbin-integrity-rest` | bump-direct-refill + NL-3 integrity targets | measured 294.03 s (`nl3_safe_xthread_churn_integrity`) + 22.15 s (`regression_bump_direct_refill`), run 43984907030 |
| miri-plain | `miri-plain-heavy` | `b4_r1_pin_cache_miri` only | measured ≈1041 s (00:20:45 → 00:38:06 marker span, run 43984907030) — single heaviest plain target, own shard; a single target cannot be split further |
| miri-plain | `miri-plain-rest` | large free/no-leak + thread-free alias + small-ring regressions, header race, tiny terminal owner invariant | measured per-target, run 43984907030: alias 47.37 s + small ring 30.58 s + header race 27.50 s + terminal 11.31 s + large free 8.00 s |

### Post-merge measurement of the shard wall times (runner job elapsed, s)

Run 43977203668 (first landing of the sharding): integrity 1764, heavy 1285, plain-rest 204, oracles failed (9 s). Run 43984907030 (after the first rebalance): integrity 1771, heavy 1153, plain-rest 163, oracles 73, core pin cache 828/866, core rest 882/858, TreeBorrows 1379/1444. These wall times motivated the 2026-11-17 rebalance of the fastbin-integrity and miri-plain shards recorded above.

## Workflow/tier context

Ordinary `push` to main is not a full run; full selection is the stored run_list, not current README prose. Static command inventory covers reachable effective workflow/local actions and is deliberately conservative. `scripts/run-check-matrix.mjs` explicitly frees executables/incremental between rows and supports reusable dependency rlibs; xthread suite also performs `rm -rf target` between groups (disk savings versus rebuild). Nightly toolchain/cache invalidation can force setup/rebuild; moving weekly-only jobs changes cadence/PR latency, not total work.

- Inputs: `docs/perf/ci_long_jobs_inputs/*.json.gz`, `collection.json`; phase source: `log-phases.json.gz` (`ci-long-jobs-log-phases-v1`, parser v3; dictionary-indexed compact records).
- `node scripts/ci-long-jobs.mjs --report` regenerates; `node scripts/ci-long-jobs.mjs --check` verifies without writing.
- Log API intervals partition by setup/toolchain/cacheRestore/cacheSave/build/execution/other/unknown. Log percentages use measured log duration only.
