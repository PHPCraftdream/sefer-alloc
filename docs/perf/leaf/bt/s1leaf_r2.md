# bench:table (paired): 3 process(es) × 200 rounds; commit ade284e0, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.9) | 4.5 (4.4–4.5) | 42.9 (42.5–45.1) | 1.26× slower |
| 64B | 5.1 (4.9–5.6) | 6.2 (6.0–6.2) | 44.2 (44.0–44.8) | **1.21× faster** |
| 256B | 5.8 (5.8–5.9) | 7.7 (7.7–7.7) | 55.4 (55.3–58.1) | **1.33× faster** |
| 1024B | 8.2 (8.2–8.3) | 15.6 (15.5–16.0) | 45.9 (45.4–50.0) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.8) | 5.5 (5.5–5.5) | 38.5 (38.2–40.7) | **1.14× faster** |
| 64B | 4.7 (4.7–4.8) | 5.9 (5.8–6.2) | 41.8 (41.2–42.5) | **1.24× faster** |
| 256B | 4.8 (4.8–4.8) | 10.7 (8.1–11.1) | 35.6 (35.5–36.3) | **2.23× faster** |
| 1024B | 4.9 (4.8–5.0) | 19.2 (16.3–19.2) | 42.5 (38.1–43.8) | **3.89× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.4) | 5.5 (5.5–5.5) | 38.7 (38.6–39.5) | **1.11× faster** |
| 64B | 4.9 (4.9–5.0) | 5.6 (5.6–5.8) | 41.6 (39.7–41.6) | **1.15× faster** |
| 256B | 5.1 (5.1–5.1) | 8.8 (8.6–8.8) | 38.3 (38.1–39.5) | **1.73× faster** |
| 1024B | 5.3 (5.3–6.2) | 19.0 (18.6–19.1) | 67.6 (67.5–69.0) | **3.57× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 1024B 0.879 ns of 5.322 (16.5 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.979 ns of 10.693 (27.9 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 5.713 ns of 42.480 (13.4 %)
