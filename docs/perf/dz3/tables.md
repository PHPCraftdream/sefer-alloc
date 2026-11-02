### A. Foreign free of m x 2048 blocks of 1 KiB by one other thread (Linux; time: medians of 9 processes; hit rate: counting build; Ir: callgrind)

| m | order | pin hit rate base / new | extra Ir per free vs seq, base / new | ns per free base | new | mimalloc | new / base | new / mimalloc |
|---:|---|---|---|---:|---:|---:|---:|---:|
| 4 | seq | 1.00 / 1.00 | - | 17.7 | 18.7 | 47.8 | 1.06 | 0.39 |
| 4 | cyclic | 1.00 / 1.00 | 8 / 8 | 28.1 | 29.2 | 40.0 | 1.04 | 0.73 |
| 4 | random | 1.00 / 1.00 | 26 / 26 | 38.3 | 35.3 | 37.9 | 0.92 | 0.93 |
| 5 | seq | 1.00 / 1.00 | - | 18.4 | 18.8 | 51.6 | 1.02 | 0.36 |
| 5 | cyclic | 0.00 / 1.00 | 446 / 8 | 74.0 | 29.4 | 40.6 | 0.40 | 0.72 |
| 5 | random | 0.79 / 1.00 | 118 / 26 | 57.4 | 35.7 | 35.7 | 0.62 | 1.00 |
| 8 | seq | 1.00 / 1.00 | - | 19.6 | 19.6 | 58.7 | 1.00 | 0.33 |
| 8 | cyclic | 0.00 / 1.00 | 444 / 7 | 75.0 | 28.5 | 41.9 | 0.38 | 0.68 |
| 8 | random | 0.50 / 1.00 | 244 / 26 | 78.2 | 38.9 | 36.8 | 0.50 | 1.06 |
| 16 | seq | 1.00 / 1.00 | - | 18.8 | 19.9 | 61.9 | 1.06 | 0.32 |
| 16 | cyclic | 0.00 / 1.00 | 442 / 9 | 83.5 | 30.8 | 48.6 | 0.37 | 0.63 |
| 16 | random | 0.25 / 1.00 | 353 / 26 | 104.8 | 37.3 | 44.9 | 0.36 | 0.83 |
| 64 | seq | 1.00 / 1.00 | - | 19.5 | 20.2 | 64.2 | 1.04 | 0.31 |
| 64 | cyclic | 0.00 / 0.00 | 461 / 434 | 89.4 | 101.8 | 43.0 | 1.14 | 2.37 |
| 64 | random | 0.06 / 0.25 | 450 / 312 | 169.9 | 163.7 | 57.8 | 0.96 | 2.83 |

### B. Design iterations (scratch prototypes; ns per free of the variant / ns per free of the 4-slot base measured in the same run, 9 processes each)

| variant | m=5 cyclic | m=16 cyclic | m=16 random | m=16 seq | m=64 cyclic | m=64 random |
|---|---:|---:|---:|---:|---:|---:|
| 16 slots, linear search, round-robin | 0.37 | 0.43 | 0.48 | 1.16 | 1.30 | 1.10 |
| 64 slots, linear search, round-robin | 0.38 | 0.46 | 0.51 | 1.15 | 0.71 | 0.73 |
| 16 slots, linear search + last-hit memo, incremental sweep | 0.45 | 0.48 | 0.54 | 0.93 | 1.12 | 0.97 |
| direct-mapped 16, incremental sweep | 0.39 | 1.03 | 0.80 | 0.94 | 1.09 | 0.89 |
| direct-mapped 64, incremental sweep | 0.37 | 0.40 | 0.40 | 0.91 | 1.10 | 0.61 |
| direct-mapped 32, incremental sweep (prototype of the change) | 0.37 | 0.39 | 0.38 | 0.92 | 1.03 | 0.86 |
| change as measured (section A), same ratio | 0.40 | 0.37 | 0.36 | 1.06 | 1.14 | 0.96 |

### C. Sweep budget: occupied slots examined per sweep call, 8 against 4 (time: medians of 9 processes in one interleaved run; extra Ir per free vs seq from callgrind program totals)

| m | order | ns per free base | budget 8 | budget 4 | budget 8 / base | budget 4 / base | extra Ir per free, budget 8 / budget 4 |
|---:|---|---:|---:|---:|---:|---:|---|
| 16 | seq | 19.3 | 18.4 | 18.8 | 0.95 | 0.97 | - |
| 16 | cyclic | 84.0 | 28.4 | 29.2 | 0.34 | 0.35 | 6 / 9 |
| 16 | random | 100.4 | 37.3 | 36.9 | 0.37 | 0.37 | 30 / 26 |
| 64 | seq | 19.7 | 20.0 | 19.8 | 1.02 | 1.00 | - |
| 64 | cyclic | 96.3 | 116.6 | 93.0 | 1.21 | 0.97 | 593 / 437 |
| 64 | random | 168.9 | 185.8 | 164.2 | 1.10 | 0.97 | 419 / 313 |

### D. Multi-thread controls (g1_mt_one, 1000000 steps per thread, aggregate Mops/s, medians of 7 interleaved runs)

| workload | threads | base (min-max) | change (min-max) | mimalloc | change / base | reading |
|---|---:|---|---|---:|---:|---|
| larson | 2 | 66.05 (61.9-67.2) | 67.42 (64.4-70.6) | 56.20 | 1.021 | ranges overlap: no difference readable |
| larson | 4 | 103.17 (93.3-122.5) | 120.18 (110.4-126.8) | 89.85 | 1.165 | ranges overlap: no difference readable |
| mstress | 2 | 109.87 (97.3-115.0) | 109.28 (89.8-113.5) | 98.58 | 0.995 | ranges overlap: no difference readable |
| mstress | 4 | 201.78 (165.7-209.5) | 200.34 (163.7-209.2) | 170.55 | 0.993 | ranges overlap: no difference readable |
