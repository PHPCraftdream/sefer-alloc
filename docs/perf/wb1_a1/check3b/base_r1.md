# bench:table (paired): 3 process(es) × 200 rounds; commit 0ee09c13, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.1–7.3) | 4.7 (4.7–4.8) | 45.6 (44.1–45.9) | 1.49× slower |
| 64B | 6.9 (6.8–7.0) | 6.3 (6.3–6.4) | 45.4 (45.3–47.0) | 1.09× slower |
| 256B | 7.6 (7.6–7.7) | 7.9 (7.9–8.0) | 40.2 (39.7–41.8) | **1.03× faster** |
| 1024B | 9.8 (9.5–10.0) | 16.5 (16.4–16.7) | 248.4 (234.7–269.0) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.9) | 5.8 (5.8–5.8) | 41.2 (40.2–42.4) | **1.03× faster** |
| 64B | 5.5 (5.4–5.6) | 6.3 (6.2–6.4) | 43.9 (42.2–44.2) | **1.15× faster** |
| 256B | 5.6 (5.6–5.7) | 11.3 (9.3–11.7) | 38.3 (37.9–40.5) | **2.02× faster** |
| 1024B | 6.2 (5.9–6.4) | 19.9 (17.7–20.0) | 43.1 (41.3–47.0) | **3.20× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–6.1) | 5.7 (5.6–5.8) | 41.9 (39.6–42.0) | 1.01× slower |
| 64B | 5.8 (5.8–5.8) | 6.0 (5.9–6.0) | 42.3 (41.9–43.2) | **1.03× faster** |
| 256B | 5.9 (5.8–6.5) | 9.1 (8.2–9.3) | 40.7 (40.3–41.0) | **1.54× faster** |
| 1024B | 6.3 (6.3–6.3) | 19.7 (19.5–20.0) | 64.1 (47.5–69.7) | **3.13× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 256B 0.683 ns of 5.908 (11.6 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.344 ns of 11.328 (20.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 22.266 ns of 64.062 (34.8 %)
