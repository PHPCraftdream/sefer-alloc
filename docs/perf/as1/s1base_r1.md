# bench:table (paired): 3 process(es) × 200 rounds; commit f17c9f0e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.1–7.2) | 4.7 (4.6–4.7) | 45.7 (44.1–46.3) | 1.53× slower |
| 64B | 6.9 (6.8–7.2) | 6.3 (6.3–6.4) | 45.7 (45.5–46.3) | 1.09× slower |
| 256B | 7.9 (7.4–7.9) | 8.0 (8.0–8.0) | 40.5 (40.4–40.6) | **1.01× faster** |
| 1024B | 10.0 (10.0–10.3) | 16.7 (16.6–17.3) | 260.3 (258.6–270.3) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.6–5.8) | 5.9 (5.7–5.9) | 41.4 (40.5–42.5) | **1.02× faster** |
| 64B | 5.7 (5.6–5.8) | 6.4 (6.4–6.4) | 43.6 (42.3–43.7) | **1.13× faster** |
| 256B | 5.9 (5.6–6.3) | 9.0 (8.7–9.2) | 37.7 (37.4–38.5) | **1.53× faster** |
| 1024B | 5.8 (5.8–6.8) | 16.3 (15.6–18.6) | 42.9 (40.7–43.7) | **2.83× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–6.1) | 5.8 (5.7–5.8) | 41.4 (41.3–42.6) | 1.03× slower |
| 64B | 6.0 (5.9–6.1) | 5.9 (5.9–6.0) | 43.7 (42.2–43.8) | 1.01× slower |
| 256B | 6.5 (5.9–7.0) | 8.6 (8.4–9.4) | 41.8 (40.4–42.7) | **1.31× faster** |
| 1024B | 6.3 (6.3–6.3) | 19.9 (18.4–20.3) | 71.8 (65.1–72.7) | **3.19× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 1024B 1.074 ns of 5.762 (18.6 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 2.979 ns of 16.309 (18.3 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 7.568 ns of 71.777 (10.5 %)
