# bench:table (paired): 3 process(es) × 200 rounds; commit a11dc0c7, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.1–7.1) | 4.7 (4.6–4.8) | 45.6 (43.9–45.9) | 1.49× slower |
| 64B | 6.9 (6.7–6.9) | 6.3 (6.3–6.4) | 46.3 (45.5–46.5) | 1.09× slower |
| 256B | 7.7 (7.6–7.8) | 7.9 (7.8–8.1) | 41.1 (40.1–41.6) | **1.02× faster** |
| 1024B | 10.1 (10.0–10.1) | 16.9 (16.6–17.0) | 262.3 (259.9–272.8) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.7) | 5.8 (5.7–5.9) | 40.6 (39.8–42.6) | **1.02× faster** |
| 64B | 5.6 (5.6–5.6) | 6.3 (6.1–6.4) | 44.3 (42.6–45.5) | **1.11× faster** |
| 256B | 5.6 (5.6–5.7) | 8.9 (8.7–9.3) | 37.8 (37.5–41.5) | **1.58× faster** |
| 1024B | 5.9 (5.8–5.9) | 18.1 (17.0–19.7) | 42.0 (40.9–43.0) | **3.07× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.7–6.0) | 5.8 (5.6–5.8) | 40.8 (40.8–42.4) | 1.03× slower |
| 64B | 5.9 (5.7–5.9) | 5.9 (5.9–6.0) | 43.5 (43.4–43.8) | 1.00× (tie) |
| 256B | 5.9 (5.9–6.0) | 8.3 (8.1–8.5) | 41.4 (41.0–42.3) | **1.40× faster** |
| 1024B | 6.4 (6.3–6.5) | 19.2 (18.8–20.1) | 66.3 (52.1–72.7) | **3.01× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 12/12 cells within 0.3 ns; widest relative churn_write 16B 0.293 ns of 5.908 (5.0 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 2.686 ns of 18.115 (14.8 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 20.508 ns of 66.309 (30.9 %)
