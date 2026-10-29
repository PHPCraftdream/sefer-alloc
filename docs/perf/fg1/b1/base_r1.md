# bench:table (paired): 3 process(es) × 200 rounds; commit 20395f5e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.2–7.3) | 4.7 (4.6–4.8) | 45.4 (44.4–45.6) | 1.56× slower |
| 64B | 6.8 (6.6–7.1) | 6.4 (6.2–6.5) | 45.9 (45.5–46.4) | 1.06× slower |
| 256B | 7.8 (7.5–7.9) | 8.0 (7.6–8.1) | 40.6 (39.8–40.7) | **1.03× faster** |
| 1024B | 10.2 (10.0–10.2) | 17.0 (16.8–17.3) | 274.3 (267.6–275.9) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.8) | 5.9 (5.8–5.9) | 41.9 (41.6–42.2) | **1.02× faster** |
| 64B | 5.7 (5.6–5.7) | 6.4 (6.4–6.4) | 44.5 (44.0–44.5) | **1.13× faster** |
| 256B | 5.8 (5.7–5.8) | 9.2 (8.7–9.4) | 38.7 (38.0–38.7) | **1.59× faster** |
| 1024B | 5.9 (5.9–6.6) | 17.1 (15.6–20.6) | 42.4 (42.4–44.6) | **2.92× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.9–6.0) | 5.8 (5.7–5.9) | 41.9 (41.3–42.0) | 1.03× slower |
| 64B | 5.8 (5.8–7.7) | 6.0 (6.0–6.0) | 44.0 (42.5–44.1) | **1.03× faster** |
| 256B | 6.1 (6.0–6.1) | 9.3 (8.4–9.6) | 41.3 (40.4–43.7) | **1.53× faster** |
| 1024B | 6.3 (6.1–6.7) | 20.0 (18.8–20.5) | 72.1 (66.4–73.4) | **3.15× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 64B 1.904 ns of 5.811 (32.8 %)
- mimalloc: 5/12 cells within 0.3 ns; widest relative churn 1024B 4.980 ns of 17.090 (29.1 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 7.032 ns of 72.119 (9.8 %)
