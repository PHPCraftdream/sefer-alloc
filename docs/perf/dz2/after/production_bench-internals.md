# bench:table (paired): 3 process(es) × 200 rounds; commit c4b1ceeb+dirty, features `production bench-internals`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.3–7.3) | 4.6 (4.6–4.7) | 44.2 (43.6–45.2) | 1.58× slower |
| 64B | 7.2 (7.1–7.5) | 6.3 (6.2–6.3) | 46.0 (44.9–46.1) | 1.13× slower |
| 256B | 7.8 (7.8–7.9) | 7.8 (7.8–7.9) | 40.7 (40.6–41.1) | 1.00× (tie) |
| 1024B | 10.3 (10.2–10.3) | 16.9 (16.7–17.1) | 258.7 (254.5–271.7) | **1.65× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.8) | 5.8 (5.8–5.8) | 40.8 (40.2–41.7) | 1.00× (tie) |
| 64B | 5.8 (5.7–5.8) | 6.3 (6.2–6.3) | 44.3 (43.5–45.5) | **1.09× faster** |
| 256B | 5.8 (5.7–5.8) | 8.9 (8.7–9.1) | 38.1 (38.0–39.1) | **1.54× faster** |
| 1024B | 6.2 (5.9–6.2) | 17.2 (17.0–20.6) | 42.1 (41.2–44.5) | **2.79× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.7–6.0) | 5.7 (5.7–5.8) | 40.8 (40.7–41.1) | 1.03× slower |
| 64B | 6.0 (5.9–6.2) | 5.9 (5.9–6.0) | 43.9 (42.5–45.0) | 1.02× slower |
| 256B | 6.4 (6.3–6.8) | 8.3 (8.3–9.2) | 41.0 (41.0–42.5) | **1.31× faster** |
| 1024B | 6.9 (6.9–7.0) | 20.1 (20.1–21.0) | 70.5 (65.4–74.9) | **2.89× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 256B 0.488 ns of 6.396 (7.6 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 3.564 ns of 17.188 (20.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 9.424 ns of 70.459 (13.4 %)
