# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–6.2) | 4.5 (4.5–4.6) | 42.7 (42.4–43.0) | 1.28× slower |
| 64B | 5.9 (5.9–6.4) | 6.1 (6.1–6.1) | 44.0 (43.5–44.5) | **1.02× faster** |
| 256B | 6.2 (6.2–6.3) | 7.6 (7.5–7.7) | 45.9 (44.4–46.8) | **1.22× faster** |
| 1024B | 8.7 (8.4–8.7) | 15.9 (15.8–16.1) | 296.6 (294.7–308.3) | **1.83× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–5.0) | 5.4 (5.4–5.5) | 39.6 (38.8–40.3) | **1.14× faster** |
| 64B | 4.7 (4.6–4.8) | 6.1 (6.0–6.1) | 41.6 (41.1–41.8) | **1.29× faster** |
| 256B | 4.8 (4.7–4.8) | 8.9 (8.6–9.0) | 43.3 (42.6–45.2) | **1.87× faster** |
| 1024B | 5.0 (5.0–5.1) | 16.4 (16.3–17.6) | 44.0 (43.3–44.1) | **3.29× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.4) | 5.4 (5.4–5.5) | 39.1 (38.6–40.3) | **1.08× faster** |
| 64B | 4.9 (4.9–5.0) | 5.6 (5.6–5.6) | 40.8 (40.5–43.0) | **1.13× faster** |
| 256B | 5.0 (4.9–5.1) | 8.1 (8.1–8.2) | 43.1 (42.0–44.4) | **1.63× faster** |
| 1024B | 5.3 (5.3–5.4) | 18.9 (18.4–18.9) | 50.2 (47.9–51.6) | **3.56× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 16B 0.488 ns of 4.980 (9.8 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.318 ns of 16.406 (8.0 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 3.613 ns of 50.244 (7.2 %)
