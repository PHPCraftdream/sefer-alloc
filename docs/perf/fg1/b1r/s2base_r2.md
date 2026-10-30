# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.0–7.1) | 4.7 (4.7–4.8) | 45.1 (44.4–45.9) | 1.49× slower |
| 64B | 6.9 (6.8–7.1) | 6.3 (6.2–6.4) | 45.9 (45.8–46.3) | 1.10× slower |
| 256B | 7.7 (7.6–7.7) | 7.9 (7.7–8.0) | 40.3 (40.1–41.0) | **1.03× faster** |
| 1024B | 9.9 (9.8–10.0) | 16.7 (16.6–16.7) | 253.6 (244.7–258.7) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 5.8 (5.7–5.9) | 41.2 (41.2–42.7) | **1.01× faster** |
| 64B | 5.7 (5.6–6.3) | 6.2 (6.2–6.4) | 43.8 (42.3–44.1) | **1.08× faster** |
| 256B | 5.7 (5.6–5.8) | 9.3 (8.9–9.3) | 42.0 (41.8–42.2) | **1.65× faster** |
| 1024B | 5.9 (5.9–6.2) | 17.0 (16.2–18.4) | 40.8 (40.8–41.6) | **2.88× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–6.5) | 5.8 (5.6–6.0) | 41.4 (41.2–42.7) | 1.01× slower |
| 64B | 5.8 (5.8–5.9) | 5.9 (5.8–6.0) | 42.9 (42.9–43.0) | **1.01× faster** |
| 256B | 6.0 (5.9–6.9) | 8.4 (8.0–9.0) | 42.8 (42.4–43.2) | **1.41× faster** |
| 1024B | 6.3 (6.3–6.3) | 19.3 (18.8–19.5) | 52.3 (49.5–67.9) | **3.09× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 256B 0.977 ns of 6.006 (16.3 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 2.148 ns of 16.992 (12.6 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 18.408 ns of 52.295 (35.2 %)
