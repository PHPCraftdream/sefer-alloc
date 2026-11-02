# bench:table (paired): 3 process(es) × 200 rounds; commit ade284e0, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.9) | 4.5 (4.4–4.5) | 42.3 (42.3–42.6) | 1.27× slower |
| 64B | 7.5 (6.4–7.5) | 6.1 (6.0–6.2) | 43.8 (43.7–44.4) | 1.24× slower |
| 256B | 5.7 (5.7–5.8) | 7.7 (7.6–7.7) | 55.1 (54.4–56.8) | **1.35× faster** |
| 1024B | 8.2 (8.2–8.3) | 15.7 (15.6–15.7) | 47.3 (45.7–48.0) | **1.91× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.5 (5.4–5.5) | 38.5 (38.4–39.3) | **1.14× faster** |
| 64B | 4.8 (4.6–4.8) | 5.9 (5.9–6.0) | 41.8 (41.6–41.8) | **1.23× faster** |
| 256B | 4.7 (4.6–4.8) | 9.3 (9.0–10.8) | 35.6 (35.3–35.8) | **1.97× faster** |
| 1024B | 5.0 (5.0–5.1) | 18.4 (16.5–18.9) | 43.3 (38.9–43.8) | **3.70× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.0) | 5.5 (5.3–5.5) | 38.9 (38.4–39.3) | **1.11× faster** |
| 64B | 5.0 (5.0–5.3) | 5.6 (5.6–5.7) | 41.5 (41.0–42.7) | **1.13× faster** |
| 256B | 5.1 (5.0–5.4) | 8.7 (8.7–8.9) | 38.3 (37.9–39.1) | **1.71× faster** |
| 1024B | 5.4 (5.4–5.5) | 19.1 (18.4–19.2) | 68.0 (63.8–68.5) | **3.53× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative direct 64B 1.075 ns of 7.520 (14.3 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 256B 1.758 ns of 9.326 (18.9 %)
- system: 2/12 cells within 0.3 ns; widest relative churn 1024B 4.932 ns of 43.262 (11.4 %)
