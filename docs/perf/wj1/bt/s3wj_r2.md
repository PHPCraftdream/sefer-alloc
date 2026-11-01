# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.2) | 4.5 (4.5–4.6) | 43.1 (43.0–44.2) | 1.30× slower |
| 64B | 6.1 (6.1–6.2) | 6.2 (5.8–6.2) | 44.5 (43.8–44.6) | **1.02× faster** |
| 256B | 6.3 (6.2–6.3) | 7.6 (7.5–7.7) | 56.1 (39.3–65.8) | **1.22× faster** |
| 1024B | 8.6 (8.4–8.8) | 15.6 (15.4–16.1) | 299.8 (239.4–307.9) | **1.81× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.7) | 5.4 (5.4–5.5) | 39.5 (39.0–39.8) | **1.14× faster** |
| 64B | 4.7 (4.6–4.7) | 6.0 (5.9–6.1) | 42.3 (40.9–42.6) | **1.28× faster** |
| 256B | 4.7 (4.7–4.8) | 9.2 (8.9–9.4) | 42.8 (40.3–46.4) | **1.95× faster** |
| 1024B | 4.9 (4.9–5.1) | 16.3 (15.8–18.3) | 40.7 (40.5–41.6) | **3.31× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–4.9) | 5.4 (5.4–5.5) | 39.3 (39.1–40.0) | **1.09× faster** |
| 64B | 4.9 (4.9–5.2) | 5.6 (5.5–5.7) | 42.4 (41.5–42.4) | **1.14× faster** |
| 256B | 5.1 (5.0–5.3) | 8.2 (8.0–8.6) | 41.9 (40.1–44.6) | **1.62× faster** |
| 1024B | 5.4 (5.2–5.4) | 18.8 (18.4–19.1) | 45.4 (44.7–45.7) | **3.48× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 256B 0.342 ns of 5.078 (6.7 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 2.442 ns of 16.309 (15.0 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 26.513 ns of 56.055 (47.3 %)
