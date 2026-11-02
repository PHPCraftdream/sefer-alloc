# bench:table (paired): 3 process(es) × 200 rounds; commit 42a0439b, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.5) | 4.4 (4.4–4.4) | 41.8 (41.7–42.3) | 1.24× slower |
| 64B | 5.3 (4.9–8.0) | 6.1 (6.1–6.1) | 43.4 (43.1–43.9) | **1.14× faster** |
| 256B | 5.9 (5.8–5.9) | 7.5 (7.5–7.6) | 56.0 (54.8–56.1) | **1.28× faster** |
| 1024B | 7.9 (7.8–8.1) | 15.2 (15.2–15.5) | 44.1 (43.8–45.0) | **1.93× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.3 (5.3–5.4) | 38.2 (38.0–38.5) | **1.14× faster** |
| 64B | 4.6 (4.6–4.6) | 5.8 (5.7–6.0) | 40.7 (40.1–40.8) | **1.24× faster** |
| 256B | 4.7 (4.7–4.8) | 8.3 (7.8–8.9) | 36.2 (35.2–38.0) | **1.77× faster** |
| 1024B | 4.9 (4.9–4.9) | 16.4 (15.6–17.1) | 40.6 (38.7–42.6) | **3.35× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.4 (5.4–5.4) | 38.6 (38.2–38.6) | **1.11× faster** |
| 64B | 4.9 (4.8–4.9) | 5.5 (5.5–5.5) | 39.4 (39.4–40.3) | **1.13× faster** |
| 256B | 4.9 (4.9–5.0) | 8.6 (7.7–8.6) | 39.4 (38.0–39.9) | **1.74× faster** |
| 1024B | 5.2 (5.2–5.3) | 17.7 (17.0–17.8) | 63.8 (63.1–65.2) | **3.39× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative direct 64B 3.076 ns of 5.322 (57.8 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 1.075 ns of 8.301 (13.0 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 3.906 ns of 40.576 (9.6 %)
