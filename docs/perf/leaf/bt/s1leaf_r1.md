# bench:table (paired): 3 process(es) × 200 rounds; commit ade284e0, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.8) | 4.5 (4.4–4.5) | 43.2 (42.8–43.4) | 1.27× slower |
| 64B | 5.2 (4.9–7.7) | 6.2 (6.0–6.2) | 44.6 (44.0–45.5) | **1.19× faster** |
| 256B | 5.8 (5.7–5.8) | 7.6 (7.6–7.6) | 54.7 (44.2–56.3) | **1.31× faster** |
| 1024B | 8.3 (8.2–8.3) | 15.4 (15.4–15.6) | 45.8 (45.6–75.9) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.0) | 5.6 (5.5–5.6) | 39.6 (39.0–40.0) | **1.16× faster** |
| 64B | 4.7 (4.7–4.9) | 6.2 (6.0–6.3) | 43.6 (41.8–44.0) | **1.31× faster** |
| 256B | 4.8 (4.8–4.9) | 8.7 (7.8–10.4) | 36.4 (36.3–37.1) | **1.83× faster** |
| 1024B | 4.9 (4.8–4.9) | 16.2 (14.9–19.1) | 41.4 (38.7–43.9) | **3.28× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.0) | 5.5 (5.5–5.5) | 39.9 (39.2–40.4) | **1.12× faster** |
| 64B | 4.9 (4.9–5.0) | 5.7 (5.6–6.1) | 40.5 (39.5–42.9) | **1.16× faster** |
| 256B | 5.1 (5.0–6.0) | 8.7 (7.4–8.8) | 38.5 (38.5–39.1) | **1.72× faster** |
| 1024B | 5.4 (5.2–5.4) | 19.1 (17.8–19.3) | 63.5 (62.4–68.9) | **3.56× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative direct 64B 2.783 ns of 5.176 (53.8 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.540 ns of 8.740 (29.1 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 30.274 ns of 45.801 (66.1 %)
