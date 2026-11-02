# bench:table (paired): 3 process(es) × 200 rounds; commit ade284e0, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–6.1) | 4.5 (4.4–4.5) | 42.9 (42.4–43.7) | 1.26× slower |
| 64B | 5.3 (5.0–6.4) | 6.1 (6.1–6.1) | 44.4 (43.8–44.7) | **1.15× faster** |
| 256B | 5.8 (5.7–5.9) | 7.6 (7.6–7.7) | 56.1 (54.1–58.9) | **1.32× faster** |
| 1024B | 8.3 (8.2–8.4) | 15.6 (15.4–15.7) | 45.5 (44.7–53.8) | **1.89× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.8) | 5.5 (5.4–5.6) | 38.6 (37.9–40.0) | **1.14× faster** |
| 64B | 4.7 (4.7–4.7) | 6.0 (5.9–6.2) | 41.3 (41.2–42.4) | **1.27× faster** |
| 256B | 4.8 (4.8–4.9) | 8.6 (8.5–10.9) | 36.3 (36.1–36.4) | **1.79× faster** |
| 1024B | 4.8 (4.7–5.0) | 18.8 (16.2–19.3) | 39.4 (38.3–44.8) | **3.94× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.1) | 5.5 (5.5–5.6) | 39.3 (38.6–40.4) | **1.10× faster** |
| 64B | 5.0 (4.9–5.1) | 5.7 (5.6–5.7) | 41.1 (39.0–42.7) | **1.14× faster** |
| 256B | 5.1 (5.0–5.6) | 8.9 (8.8–8.9) | 38.6 (37.4–39.4) | **1.75× faster** |
| 1024B | 5.4 (5.2–7.1) | 19.1 (18.9–19.2) | 68.8 (68.8–69.5) | **3.56× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 1024B 1.904 ns of 5.371 (35.4 %)
- mimalloc: 10/12 cells within 0.3 ns; widest relative churn 256B 2.442 ns of 8.643 (28.3 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 9.131 ns of 45.459 (20.1 %)
