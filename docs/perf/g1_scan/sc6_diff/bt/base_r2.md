# bench:table (paired): 3 process(es) × 200 rounds; commit 20395f5e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.0–7.2) | 4.7 (4.6–4.7) | 44.8 (44.3–45.9) | 1.51× slower |
| 64B | 6.8 (6.7–6.9) | 6.4 (6.4–6.5) | 45.4 (45.3–46.9) | 1.05× slower |
| 256B | 7.7 (7.7–7.8) | 8.0 (7.9–8.1) | 41.1 (40.2–41.1) | **1.04× faster** |
| 1024B | 10.0 (9.8–10.4) | 16.8 (16.6–16.9) | 254.4 (230.4–264.9) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 5.9 (5.6–6.0) | 40.1 (39.2–41.6) | **1.03× faster** |
| 64B | 5.6 (5.5–6.2) | 6.3 (6.2–6.4) | 43.2 (42.2–44.7) | **1.13× faster** |
| 256B | 5.7 (5.6–5.8) | 9.4 (9.2–11.8) | 41.6 (38.1–43.0) | **1.64× faster** |
| 1024B | 6.0 (5.9–6.9) | 16.2 (16.0–19.9) | 44.6 (41.9–46.1) | **2.69× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–6.2) | 5.8 (5.7–5.9) | 40.8 (39.8–41.9) | 1.00× (tie) |
| 64B | 5.8 (5.8–5.9) | 5.9 (5.9–6.0) | 42.2 (42.1–43.1) | **1.03× faster** |
| 256B | 6.0 (6.0–6.1) | 9.3 (8.8–9.5) | 42.2 (41.1–42.4) | **1.54× faster** |
| 1024B | 6.3 (6.3–6.4) | 20.2 (20.0–20.4) | 50.9 (48.3–72.0) | **3.23× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.026 ns of 6.006 (17.1 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.587 ns of 9.375 (27.6 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 23.633 ns of 50.879 (46.4 %)
