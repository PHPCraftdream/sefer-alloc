# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.0–7.3) | 4.7 (4.6–4.7) | 44.3 (44.3–45.5) | 1.52× slower |
| 64B | 6.8 (6.7–7.0) | 6.3 (6.2–6.5) | 45.8 (45.3–46.1) | 1.07× slower |
| 256B | 7.7 (7.7–7.8) | 7.8 (7.8–8.0) | 40.7 (39.7–41.3) | **1.01× faster** |
| 1024B | 10.0 (9.9–10.1) | 16.6 (16.4–17.3) | 255.9 (248.3–259.7) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–6.0) | 5.7 (5.7–5.8) | 41.1 (40.2–41.8) | **1.01× faster** |
| 64B | 5.7 (5.5–6.4) | 6.4 (6.3–6.4) | 42.7 (42.6–45.0) | **1.13× faster** |
| 256B | 5.7 (5.6–5.7) | 8.9 (8.8–10.4) | 37.8 (37.5–42.0) | **1.56× faster** |
| 1024B | 5.9 (5.8–5.9) | 17.3 (16.6–17.9) | 42.4 (42.3–46.7) | **2.95× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.9) | 5.7 (5.7–5.8) | 41.6 (41.3–41.7) | 1.02× slower |
| 64B | 5.9 (5.9–6.2) | 5.9 (5.8–5.9) | 43.9 (42.2–44.0) | 1.00× (tie) |
| 256B | 6.0 (5.9–7.4) | 9.2 (8.2–9.3) | 41.3 (40.7–41.5) | **1.54× faster** |
| 1024B | 6.3 (6.2–6.4) | 19.8 (19.6–20.0) | 66.0 (46.6–72.3) | **3.15× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 256B 1.514 ns of 6.006 (25.2 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 1.611 ns of 8.887 (18.1 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 25.635 ns of 65.967 (38.9 %)
