# bench:table (paired): 3 process(es) × 200 rounds; commit f17c9f0e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.1–7.1) | 4.7 (4.6–4.7) | 45.0 (44.7–45.2) | 1.51× slower |
| 64B | 6.9 (6.5–7.1) | 6.3 (6.3–6.4) | 45.8 (45.7–46.9) | 1.08× slower |
| 256B | 7.8 (7.7–7.8) | 7.9 (7.9–8.0) | 40.8 (39.8–41.8) | **1.02× faster** |
| 1024B | 10.0 (9.9–10.2) | 16.5 (16.5–16.7) | 260.5 (255.2–263.7) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.7) | 5.7 (5.7–5.8) | 41.5 (40.6–42.0) | **1.01× faster** |
| 64B | 5.6 (5.6–5.6) | 6.3 (6.3–6.3) | 43.8 (43.6–44.1) | **1.13× faster** |
| 256B | 5.7 (5.6–6.3) | 8.8 (8.6–9.1) | 38.7 (36.9–39.1) | **1.56× faster** |
| 1024B | 5.9 (5.6–6.0) | 15.8 (15.6–15.9) | 42.6 (41.6–45.9) | **2.67× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.8) | 5.7 (5.7–5.8) | 41.3 (40.7–41.6) | 1.01× slower |
| 64B | 5.8 (5.8–5.8) | 5.9 (5.8–5.9) | 42.7 (42.1–44.1) | **1.03× faster** |
| 256B | 6.0 (5.8–6.5) | 8.3 (8.2–8.4) | 40.9 (39.7–43.1) | **1.39× faster** |
| 1024B | 6.3 (6.2–6.8) | 19.7 (18.4–19.9) | 66.9 (64.7–73.3) | **3.13× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 256B 0.732 ns of 5.957 (12.3 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn_write 1024B 1.465 ns of 19.727 (7.4 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 8.594 ns of 66.943 (12.8 %)
