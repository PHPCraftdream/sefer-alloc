# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.0) | 4.5 (4.5–4.6) | 43.2 (42.5–43.6) | 1.29× slower |
| 64B | 5.9 (5.6–6.0) | 6.2 (6.2–6.5) | 44.1 (44.0–44.2) | **1.05× faster** |
| 256B | 6.4 (6.2–6.4) | 7.7 (7.7–7.9) | 55.5 (42.2–60.1) | **1.21× faster** |
| 1024B | 8.5 (8.3–8.7) | 15.9 (15.8–15.9) | 298.7 (286.4–315.1) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.8) | 5.5 (5.4–5.6) | 38.5 (38.1–38.8) | **1.14× faster** |
| 64B | 4.7 (4.7–4.7) | 5.9 (5.8–6.1) | 41.1 (40.7–41.3) | **1.25× faster** |
| 256B | 4.8 (4.7–4.8) | 8.6 (8.5–10.7) | 41.9 (41.9–42.9) | **1.81× faster** |
| 1024B | 5.0 (4.9–5.3) | 18.6 (16.9–18.9) | 39.4 (39.0–42.1) | **3.70× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.7) | 5.5 (5.4–5.5) | 38.7 (38.2–39.1) | **1.11× faster** |
| 64B | 5.0 (5.0–5.1) | 5.6 (5.6–5.8) | 40.7 (40.7–41.7) | **1.12× faster** |
| 256B | 5.1 (5.0–5.1) | 7.9 (7.8–8.8) | 40.2 (39.8–43.2) | **1.56× faster** |
| 1024B | 5.4 (5.4–5.5) | 18.8 (18.0–19.0) | 45.3 (44.1–45.6) | **3.51× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 16B 0.732 ns of 4.932 (14.8 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.246 ns of 8.643 (26.0 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 17.871 ns of 55.469 (32.2 %)
