# bench:table (paired): 3 process(es) × 200 rounds; commit 97c18b36, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.4–6.5) | 4.6 (4.6–4.7) | 44.1 (43.8–44.9) | 1.40× slower |
| 64B | 6.0 (5.9–6.2) | 6.3 (6.3–6.4) | 45.1 (45.1–45.9) | **1.07× faster** |
| 256B | 6.9 (6.8–7.0) | 7.9 (7.8–8.1) | 40.3 (40.0–41.8) | **1.15× faster** |
| 1024B | 9.4 (9.3–9.6) | 16.6 (16.6–17.1) | 248.6 (243.2–265.6) | **1.76× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.6) | 5.8 (5.8–5.8) | 41.0 (40.6–41.0) | **1.06× faster** |
| 64B | 5.4 (5.4–5.4) | 6.3 (6.2–6.5) | 43.7 (43.1–43.8) | **1.17× faster** |
| 256B | 5.4 (5.4–5.5) | 8.8 (8.7–9.0) | 37.8 (36.9–40.3) | **1.63× faster** |
| 1024B | 5.5 (5.5–5.6) | 18.7 (15.9–19.0) | 42.3 (41.0–43.2) | **3.39× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 5.7 (5.6–5.8) | 41.8 (40.1–42.3) | 1.00× (tie) |
| 64B | 5.8 (5.7–6.0) | 5.9 (5.7–6.0) | 42.4 (42.2–43.3) | **1.01× faster** |
| 256B | 6.4 (5.7–6.6) | 8.3 (8.0–8.5) | 40.8 (40.6–43.3) | **1.28× faster** |
| 1024B | 6.2 (6.1–6.3) | 20.0 (19.4–20.2) | 63.9 (47.3–66.2) | **3.25× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn_write 256B 0.879 ns of 6.445 (13.6 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 3.125 ns of 18.701 (16.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 18.848 ns of 63.867 (29.5 %)
