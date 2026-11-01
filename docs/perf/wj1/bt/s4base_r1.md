# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.7–6.4) | 4.6 (4.5–4.6) | 43.3 (43.3–43.5) | 1.28× slower |
| 64B | 5.8 (5.7–5.9) | 6.3 (6.0–6.3) | 44.6 (44.3–45.9) | **1.08× faster** |
| 256B | 6.3 (6.3–6.4) | 7.7 (7.6–7.7) | 40.1 (39.2–62.4) | **1.22× faster** |
| 1024B | 8.6 (8.4–8.8) | 16.0 (15.6–16.6) | 249.8 (247.4–316.9) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–5.7) | 5.6 (5.5–5.7) | 40.0 (39.6–40.3) | **1.14× faster** |
| 64B | 4.8 (4.7–5.4) | 6.2 (6.1–6.3) | 42.0 (41.3–45.0) | **1.27× faster** |
| 256B | 4.8 (4.8–4.8) | 9.3 (8.8–9.5) | 41.1 (40.1–44.1) | **1.92× faster** |
| 1024B | 5.1 (5.0–5.2) | 18.3 (17.5–19.0) | 41.9 (40.9–45.4) | **3.61× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (5.0–5.7) | 5.6 (5.5–5.7) | 40.8 (39.7–41.2) | **1.06× faster** |
| 64B | 5.1 (5.0–5.1) | 5.8 (5.6–5.8) | 42.4 (41.4–44.1) | **1.13× faster** |
| 256B | 5.1 (5.1–5.1) | 8.9 (8.5–9.1) | 40.9 (40.1–43.3) | **1.75× faster** |
| 1024B | 5.4 (5.4–5.9) | 19.2 (18.6–19.5) | 46.2 (46.0–51.6) | **3.55× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 16B 0.879 ns of 4.932 (17.8 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 1.514 ns of 18.311 (8.3 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 23.145 ns of 40.137 (57.7 %)
