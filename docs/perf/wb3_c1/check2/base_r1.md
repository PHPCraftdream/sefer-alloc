# bench:table (paired): 3 process(es) × 200 rounds; commit 9d2823e5, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.1–7.2) | 4.8 (4.7–4.8) | 44.8 (44.7–46.3) | 1.50× slower |
| 64B | 6.9 (6.7–7.2) | 6.4 (6.3–6.4) | 46.6 (45.6–46.7) | 1.08× slower |
| 256B | 7.9 (7.7–8.0) | 8.0 (8.0–8.0) | 41.4 (40.3–42.8) | **1.01× faster** |
| 1024B | 10.3 (10.2–10.4) | 16.9 (16.8–17.0) | 257.0 (254.0–261.8) | **1.65× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–6.2) | 6.0 (5.9–6.1) | 41.9 (40.3–43.1) | **1.03× faster** |
| 64B | 5.7 (5.7–5.8) | 6.6 (6.6–6.7) | 45.1 (44.1–45.7) | **1.17× faster** |
| 256B | 5.8 (5.7–5.8) | 8.9 (8.7–9.9) | 38.1 (37.8–43.6) | **1.53× faster** |
| 1024B | 5.9 (5.7–7.2) | 17.8 (17.5–19.8) | 45.0 (41.6–45.8) | **3.03× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.9–6.1) | 5.9 (5.7–5.9) | 42.1 (40.4–43.7) | 1.03× slower |
| 64B | 6.0 (5.9–6.0) | 6.0 (5.8–6.2) | 42.7 (42.3–44.1) | **1.01× faster** |
| 256B | 6.0 (6.0–7.1) | 8.2 (8.1–9.5) | 43.4 (41.8–44.1) | **1.36× faster** |
| 1024B | 6.5 (6.3–8.1) | 20.0 (19.4–20.5) | 70.9 (49.7–73.6) | **3.08× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 1024B 1.806 ns of 6.494 (27.8 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 1.416 ns of 8.154 (17.4 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 23.975 ns of 70.947 (33.8 %)
