### Judge (Windows, g1_wild_judge, scale 1), sefer / mimalloc

| scenario | blocks | sefer / mi (exp mean d) | 95 % CI | class | min sefer ms | min mi ms | peak RSS sefer MiB | peak RSS mi MiB | end RSS sefer MiB | end RSS mi MiB |
|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|
| grow64 | 12 | 1.730 | 1.678–1.784 | LOSS | 1096 | 629 | 1300 | 1159 | 29 | 1031 |
| grow1k | 12 | 1.490 | 1.412–1.573 | LOSS | 467 | 314 | 1180 | 1038 | 29 | 1030 |
| regrow | 12 | 1.412 | 1.326–1.504 | LOSS | 345 | 237 | 683 | 588 | 71 | 572 |
| growmix | 12 | 1.300 | 1.234–1.368 | LOSS | 537 | 407 | 1351 | 1160 | 1328 | 1137 |
| wild | 12 | 1.520 | 1.439–1.605 | LOSS | 187 | 119 | 45 | 53 | 29 | 53 |
| phase | 12 | 1.143 | 1.099–1.188 | LOSS | 644 | 574 | 196 | 205 | 178 | 205 |
| large | 12 | 1.278 | 1.182–1.381 | LOSS | 15 | 11 | 14 | 28 | 8 | 28 |

RSS columns are medians over all runs of the cell (peak_rss / end_rss as printed by the judge).

### Release attribution, sefer_pool arm (Windows)

| scenario | blocks | sefer / mi | sefer_pool / mi | sefer_pool / sefer | share of the sefer-mi gap that disappears without release | seg_released sefer (median) | seg_released sefer_pool (max over runs) | config_conflicts (max over runs) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| wild | 12 | 1.461 (1.380–1.547) | 1.483 (1.411–1.559) | 1.015 (0.951–1.084) | -4 % (= (ln 1.461 - ln 1.483) / ln 1.461) | 595 | 595 | 0 |
| phase | 12 | 1.149 (1.079–1.223) | 1.130 (1.051–1.214) | 0.983 (0.936–1.032) | 12 % (= (ln 1.149 - ln 1.130) / ln 1.149) | 1022 | 1004 | 0 |
| large | 12 | 1.404 (1.257–1.569) | 1.434 (1.386–1.483) | 1.021 (0.921–1.132) | -6 % (= (ln 1.404 - ln 1.434) / ln 1.404) | 589 | 589 | 0 |

The share is computed from the means of the per-block log ratios; it is meaningful only where sefer / mi is above 1. seg_released of the sefer_pool arm is the path-activation oracle (0 = no segment was released in free).

### bench:table paired set s1 (sefer vs mimalloc, one build)

# bench:table (paired): 3 process(es) × 200 rounds; commit c495edc9, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.5 (4.4–4.5) | 42.2 (42.0–42.6) | 1.25× slower |
| 64B | 5.7 (5.6–8.0) | 6.0 (6.0–6.2) | 43.2 (43.2–43.8) | **1.05× faster** |
| 256B | 6.0 (5.9–6.0) | 7.5 (7.5–7.7) | 54.0 (43.8–55.8) | **1.26× faster** |
| 1024B | 8.3 (8.3–8.3) | 15.6 (15.3–15.7) | 48.2 (46.7–85.2) | **1.87× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.8) | 5.3 (5.3–5.5) | 38.1 (37.4–38.2) | **1.14× faster** |
| 64B | 4.6 (4.6–4.7) | 5.9 (5.8–6.0) | 40.5 (40.0–42.1) | **1.27× faster** |
| 256B | 4.7 (4.7–5.3) | 8.6 (8.2–10.8) | 36.0 (35.3–36.7) | **1.84× faster** |
| 1024B | 4.9 (4.8–5.0) | 18.4 (16.8–19.3) | 40.7 (40.5–42.9) | **3.72× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.0) | 5.4 (5.3–5.4) | 38.4 (37.1–38.7) | **1.11× faster** |
| 64B | 4.9 (4.8–5.0) | 5.5 (5.4–5.6) | 40.4 (40.1–42.2) | **1.12× faster** |
| 256B | 5.0 (4.9–5.3) | 8.6 (8.4–8.8) | 38.9 (37.7–39.5) | **1.74× faster** |
| 1024B | 5.3 (5.2–5.4) | 18.7 (18.5–19.0) | 64.6 (64.0–67.7) | **3.54× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 2.393 ns of 5.713 (41.9 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.686 ns of 8.643 (31.1 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 38.525 ns of 48.193 (79.9 %)

### bench:table paired set s2 (sefer vs mimalloc, one build)

# bench:table (paired): 3 process(es) × 200 rounds; commit c495edc9, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 4.5 (4.4–4.5) | 42.2 (42.1–42.5) | 1.26× slower |
| 64B | 5.4 (5.2–8.0) | 6.0 (6.0–6.1) | 43.4 (43.4–43.8) | **1.12× faster** |
| 256B | 5.9 (5.8–5.9) | 7.6 (7.3–7.6) | 53.5 (44.7–53.9) | **1.29× faster** |
| 1024B | 8.2 (8.1–8.3) | 15.4 (15.3–15.4) | 45.0 (44.1–82.9) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–9.9) | 5.4 (5.4–5.4) | 38.0 (37.7–38.8) | **1.10× faster** |
| 64B | 4.7 (4.6–4.7) | 5.9 (5.8–5.9) | 41.2 (40.2–41.4) | **1.25× faster** |
| 256B | 4.7 (4.6–4.7) | 8.1 (8.0–10.9) | 35.7 (35.4–35.7) | **1.71× faster** |
| 1024B | 4.9 (4.9–4.9) | 18.8 (16.2–18.8) | 40.3 (39.1–42.8) | **3.84× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (4.9–5.7) | 5.3 (5.3–5.3) | 38.5 (38.4–39.1) | 1.04× slower |
| 64B | 4.9 (4.9–5.0) | 5.5 (5.4–5.5) | 40.6 (39.9–42.1) | **1.11× faster** |
| 256B | 5.0 (5.0–5.0) | 8.4 (8.4–8.7) | 38.1 (37.7–38.7) | **1.70× faster** |
| 1024B | 5.4 (5.3–5.4) | 18.2 (18.0–18.7) | 67.0 (61.3–67.9) | **3.38× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 16B 5.078 ns of 4.883 (104.0 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 2.930 ns of 8.105 (36.2 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 38.769 ns of 44.971 (86.2 %)

### bench:table cells, sefer / mi time ratio in both sets

| cell | set s1 | set s2 | class |
|---|---:|---:|---|
| Warm repeated bulk burst 16 B | 1.250 | 1.260 | LOSS |
| Warm repeated bulk burst 64 B | 0.952 | 0.893 | WIN |
| Warm repeated bulk burst 256 B | 0.794 | 0.775 | WIN |
| Warm repeated bulk burst 1024 B | 0.535 | 0.532 | WIN |
| Churn, non-writing 16 B | 0.877 | 0.909 | WIN |
| Churn, non-writing 64 B | 0.787 | 0.800 | WIN |
| Churn, non-writing 256 B | 0.543 | 0.585 | WIN |
| Churn, non-writing 1024 B | 0.269 | 0.260 | WIN |
| Churn + write 16 B | 0.901 | 1.040 | MIXED |
| Churn + write 64 B | 0.893 | 0.901 | WIN |
| Churn + write 256 B | 0.575 | 0.588 | WIN |
| Churn + write 1024 B | 0.282 | 0.296 | WIN |

### MT (malloc_macro, 6 runs, unpinned), aggregate Mops/s, median over runs

| cell | sefer | mimalloc | System | sefer / mi time | class |
|---|---:|---:|---:|---:|---|
| larson T=1 | 49.05 | 53.86 | 12.71 | 1.098 | LOSS |
| larson T=2 | 51.99 | 46.75 | 13.54 | 0.899 | WIN |
| larson T=4 | 90.05 | 74.80 | 26.24 | 0.831 | WIN |
| mstress T=1 | 60.73 | 75.26 | 5.87 | 1.239 | LOSS |
| mstress T=2 | 78.75 | 79.36 | 11.27 | 1.008 | PARITY |
| mstress T=4 | 131.81 | 149.97 | 17.45 | 1.138 | LOSS |

### Loss list (class LOSS or MIXED), sorted by sefer / mi time ratio

| source | cell | sefer / mi | class |
|---|---|---:|---|
| judge-win | grow64 | 1.730 | LOSS |
| judge-win | wild | 1.520 | LOSS |
| judge-win | grow1k | 1.490 | LOSS |
| judge-win | regrow | 1.412 | LOSS |
| judge-win | growmix | 1.300 | LOSS |
| judge-win | large | 1.278 | LOSS |
| bench:table | Warm repeated bulk burst 16 B | 1.255 | LOSS |
| mt | mstress T=1 | 1.239 | LOSS |
| judge-win | phase | 1.143 | LOSS |
| mt | mstress T=4 | 1.138 | LOSS |
| mt | larson T=1 | 1.098 | LOSS |
| bench:table | Churn + write 16 B | 0.970 | MIXED |

Counts: 25 cells, 11 LOSS, 1 MIXED, 1 PARITY, 12 WIN.
