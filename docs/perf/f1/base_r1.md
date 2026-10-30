# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.2–7.3) | 4.8 (4.8–4.9) | 45.8 (45.0–46.0) | 1.50× slower |
| 64B | 7.0 (6.9–8.9) | 6.4 (6.3–6.4) | 46.2 (45.8–46.9) | 1.09× slower |
| 256B | 8.0 (7.9–8.0) | 8.0 (7.8–8.2) | 41.1 (40.0–42.9) | 1.01× slower |
| 1024B | 10.4 (10.2–10.8) | 17.2 (17.0–17.3) | 266.4 (265.9–276.8) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.0) | 6.0 (5.9–6.1) | 42.1 (42.1–42.2) | **1.03× faster** |
| 64B | 5.8 (5.7–5.8) | 6.8 (6.6–6.9) | 44.5 (44.5–68.4) | **1.17× faster** |
| 256B | 5.9 (5.8–5.9) | 10.1 (9.1–12.1) | 38.9 (37.7–44.9) | **1.70× faster** |
| 1024B | 5.9 (5.9–6.6) | 20.4 (16.0–20.9) | 42.8 (42.7–46.2) | **3.45× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.9–6.0) | 5.9 (5.9–5.9) | 41.8 (41.6–42.1) | 1.01× slower |
| 64B | 6.0 (5.9–6.1) | 6.1 (6.1–6.2) | 44.3 (42.2–72.6) | **1.02× faster** |
| 256B | 6.1 (6.1–6.3) | 9.0 (8.8–9.3) | 42.4 (42.0–43.0) | **1.48× faster** |
| 1024B | 6.9 (6.5–6.9) | 20.5 (19.1–20.6) | 67.3 (50.3–70.3) | **2.97× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 2.002 ns of 7.031 (28.5 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 2.978 ns of 10.059 (29.6 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 64B 30.419 ns of 44.336 (68.6 %)
