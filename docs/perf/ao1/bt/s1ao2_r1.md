# bench:table (paired): 3 process(es) × 200 rounds; commit e551ff85, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.4 (4.4–4.4) | 41.6 (41.5–42.2) | 1.26× slower |
| 64B | 5.1 (4.8–5.8) | 6.0 (5.8–6.1) | 43.3 (43.1–44.6) | **1.16× faster** |
| 256B | 5.8 (5.7–5.8) | 7.5 (7.4–7.6) | 41.7 (41.7–59.2) | **1.30× faster** |
| 1024B | 8.1 (8.1–8.3) | 15.6 (15.3–15.7) | 288.2 (283.3–299.5) | **1.93× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.4 (5.4–5.4) | 37.6 (37.1–38.2) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 6.0 (5.9–6.0) | 41.2 (39.9–41.7) | **1.28× faster** |
| 256B | 4.7 (4.6–4.7) | 8.6 (8.1–8.9) | 42.0 (39.2–42.8) | **1.83× faster** |
| 1024B | 4.9 (4.8–5.1) | 15.4 (14.8–18.0) | 39.3 (38.5–41.2) | **3.16× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.3 (5.3–5.4) | 38.1 (37.6–38.2) | **1.10× faster** |
| 64B | 4.9 (4.9–4.9) | 5.5 (5.5–5.5) | 41.4 (40.5–41.6) | **1.13× faster** |
| 256B | 4.9 (4.9–4.9) | 8.6 (7.6–8.7) | 39.7 (39.5–42.2) | **1.75× faster** |
| 1024B | 5.2 (5.1–5.2) | 18.0 (17.8–18.5) | 43.9 (42.8–45.2) | **3.44× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative direct 64B 1.026 ns of 5.127 (20.0 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 3.125 ns of 15.430 (20.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 17.481 ns of 41.748 (41.9 %)
