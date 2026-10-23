# Wall-clock comparison from raw Criterion samples

For `Linear` samples, the script derives an ordinary least-squares slope from raw `times` versus `iters` and also reports Criterion’s structured slope point estimate and 95% interval. For `Flat` samples, the raw estimator is the median of `time / iters`, alongside Criterion’s mean estimate. Any OLS/Criterion divergence stays visible; paired ratios use Criterion estimates to match the canonical `bench:table` metric. Each source arm has two clean-target runs in ABBA order. `Δ % = (after − before) / before`; ratios are paired per run as SeferAlloc / mimalloc.

| Workload | Unit | Before Sefer OLS | After Sefer OLS | Δ % (after − before) / before | Before Criterion estimate | After Criterion estimate | Criterion Δ % (after − before) / before | Before mimalloc | After mimalloc | Before System | After System | Before Sefer / mimalloc | After Sefer / mimalloc |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| global_alloc/{allocator}/16B | ns/pair | 18.15 | 20.29 | 11.79 | 18.43 | 20.91 | 13.47 | 5.96 | 6.89 | 62.05 | 69.24 | 2.86 | 3.19 |
| global_alloc/{allocator}/64B | ns/pair | 21.41 | 21.89 | 2.23 | 20.44 | 23.17 | 13.35 | 8.03 | 10.19 | 62.03 | 77.55 | 2.58 | 2.64 |
| global_alloc/{allocator}/256B | ns/pair | 24.78 | 22.00 | -11.22 | 23.60 | 24.38 | 3.30 | 10.61 | 11.56 | 88.88 | 71.61 | 2.16 | 2.08 |
| global_alloc/{allocator}/1024B | ns/pair | 21.25 | 22.94 | 8.00 | 23.78 | 24.15 | 1.56 | 18.20 | 21.31 | 83.85 | 67.00 | 1.34 | 1.15 |
| global_alloc_churn/{allocator}/16B | ns/pair | 8.70 | 13.33 | 53.33 | 8.82 | 12.65 | 43.39 | 7.77 | 9.26 | 55.31 | 46.41 | 1.11 | 1.43 |
| global_alloc_churn/{allocator}/64B | ns/pair | 10.11 | 9.52 | -5.80 | 10.71 | 10.46 | -2.36 | 10.06 | 9.53 | 59.57 | 56.34 | 1.05 | 1.00 |
| global_alloc_churn/{allocator}/256B | ns/pair | 10.61 | 12.80 | 20.64 | 10.48 | 12.02 | 14.64 | 18.76 | 19.74 | 57.95 | 70.47 | 0.54 | 0.61 |
| global_alloc_churn/{allocator}/1024B | ns/pair | 12.64 | 11.04 | -12.66 | 11.75 | 11.76 | 0.11 | 100.70 | 101.28 | 79.13 | 64.92 | 0.12 | 0.13 |
| global_alloc_churn_write/{allocator}/16B | ns/pair | 10.42 | 11.89 | 14.11 | 10.94 | 11.33 | 3.55 | 7.70 | 9.54 | 49.26 | 63.80 | 1.39 | 1.20 |
| global_alloc_churn_write/{allocator}/64B | ns/pair | 12.20 | 12.78 | 4.69 | 12.09 | 11.81 | -2.38 | 10.78 | 13.19 | 54.47 | 54.99 | 1.24 | 1.00 |
| global_alloc_churn_write/{allocator}/256B | ns/pair | 8.72 | 16.34 | 87.46 | 10.66 | 14.32 | 34.36 | 16.85 | 16.17 | 62.42 | 66.53 | 0.62 | 0.86 |
| global_alloc_churn_write/{allocator}/1024B | ns/pair | 14.76 | 11.17 | -24.34 | 13.76 | 12.76 | -7.24 | 87.58 | 94.24 | 72.61 | 71.21 | 0.15 | 0.13 |
| global_alloc_churn_with_teardown/{allocator}/16B | ns/pair | 13.94 | 14.09 | 1.07 | 14.14 | 14.61 | 3.35 | 8.45 | 11.73 | 59.23 | 59.45 | 1.55 | 1.41 |
| global_alloc_churn_with_teardown/{allocator}/64B | ns/pair | 12.18 | 18.55 | 52.27 | 14.39 | 17.58 | 22.18 | 10.08 | 10.92 | 57.80 | 59.05 | 1.47 | 1.47 |
| global_alloc_churn_with_teardown/{allocator}/256B | ns/pair | 18.41 | 14.11 | -23.36 | 16.36 | 15.14 | -7.43 | 12.38 | 13.39 | 80.56 | 69.77 | 1.27 | 1.09 |
| global_alloc_churn_with_teardown/{allocator}/1024B | ns/pair | 51.92 | 9.98 | -80.77 | 35.35 | 13.85 | -60.82 | 25.99 | 25.18 | 75.32 | 75.49 | 1.48 | 0.53 |
| global_alloc/manual_realloc_sim/{allocator} | ns/closure | 522.71 | 536.94 | 2.72 | 537.67 | 579.78 | 7.83 | 488.94 | 457.05 | 811.59 | 844.05 | 1.08 | 1.12 |
| segment_decommit_cycle/{allocator}/253KiB | ns/batch | 3372.27 | 12218.27 | 262.32 | 3287.20 | 12136.71 | 269.21 | 3065.92 | 3873.33 | 34464.68 | 42058.01 | 1.07 | 3.38 |
| working_set_cycle/{allocator}/16B | ns/batch | 165696.58 | 184531.68 | 11.37 | 160809.59 | 167046.56 | 3.88 | — | — | — | — | — | — |
| working_set_cycle/{allocator}/64B | ns/batch | 168600.30 | 209598.70 | 24.32 | 157039.71 | 186143.27 | 18.53 | — | — | — | — | — | — |
| working_set_cycle/{allocator}/256B | ns/batch | 119836.06 | 161045.15 | 34.39 | 134684.16 | 161396.58 | 19.83 | — | — | — | — | — | — |
| working_set_cycle/{allocator}/1024B | ns/batch | 198050.00 | 176300.00 | -10.98 | 215256.67 | 214760.00 | -0.23 | — | — | — | — | — | — |
