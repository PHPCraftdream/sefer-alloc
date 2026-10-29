# bench:table (paired): 3 process(es) × 200 rounds; commit 3782939c, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.1–7.2) | 4.7 (4.7–4.8) | 44.6 (44.6–44.8) | 1.53× slower |
| 64B | 6.9 (6.9–7.0) | 6.4 (6.3–6.5) | 46.0 (45.9–46.4) | 1.07× slower |
| 256B | 7.7 (7.6–7.8) | 8.0 (7.8–8.0) | 41.1 (40.9–41.6) | **1.03× faster** |
| 1024B | 10.0 (9.9–10.2) | 16.8 (16.6–16.9) | 257.6 (255.4–259.8) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.8) | 5.8 (5.8–5.9) | 41.2 (40.9–41.5) | **1.03× faster** |
| 64B | 5.7 (5.6–5.7) | 6.5 (6.5–6.6) | 44.1 (43.5–44.4) | **1.16× faster** |
| 256B | 5.7 (5.7–6.3) | 9.1 (8.8–9.3) | 41.8 (39.0–42.1) | **1.59× faster** |
| 1024B | 6.1 (6.0–6.1) | 16.5 (16.5–20.3) | 44.4 (41.5–45.3) | **2.73× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.8–6.1) | 5.8 (5.7–5.8) | 41.7 (40.8–42.8) | 1.03× slower |
| 64B | 5.9 (5.8–5.9) | 6.0 (6.0–6.1) | 43.1 (42.7–44.4) | **1.02× faster** |
| 256B | 6.1 (6.0–6.1) | 8.8 (8.3–9.4) | 41.9 (41.8–42.4) | **1.45× faster** |
| 1024B | 6.5 (6.4–6.5) | 19.4 (18.7–20.2) | 47.9 (47.7–71.2) | **2.96× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn 256B 0.635 ns of 5.713 (11.1 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 3.809 ns of 16.504 (23.1 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 23.535 ns of 47.949 (49.1 %)
