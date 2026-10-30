# bench:table (paired): 3 process(es) × 200 rounds; commit f17c9f0e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.1–7.2) | 4.7 (4.6–4.7) | 44.5 (44.2–47.4) | 1.51× slower |
| 64B | 6.8 (6.8–7.0) | 6.4 (6.3–6.4) | 46.0 (45.9–46.1) | 1.07× slower |
| 256B | 7.7 (7.7–7.8) | 8.0 (7.9–8.0) | 40.2 (39.9–40.6) | **1.03× faster** |
| 1024B | 10.0 (10.0–10.2) | 16.7 (16.6–16.7) | 263.4 (261.7–267.5) | **1.67× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 5.7 (5.7–5.8) | 41.2 (40.7–41.6) | **1.01× faster** |
| 64B | 5.6 (5.6–5.7) | 6.3 (6.2–6.4) | 43.9 (42.2–45.7) | **1.13× faster** |
| 256B | 5.7 (5.6–5.8) | 8.6 (8.2–8.9) | 37.1 (37.1–37.7) | **1.53× faster** |
| 1024B | 5.9 (5.8–5.9) | 16.8 (16.0–19.8) | 44.4 (41.5–44.5) | **2.87× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–6.0) | 5.8 (5.6–5.8) | 41.3 (41.1–41.6) | 1.00× (tie) |
| 64B | 5.9 (5.9–5.9) | 5.9 (5.8–6.0) | 43.7 (42.8–44.7) | **1.01× faster** |
| 256B | 6.0 (6.0–6.7) | 8.2 (8.2–8.3) | 41.0 (40.6–43.7) | **1.37× faster** |
| 1024B | 6.3 (6.3–7.3) | 19.5 (19.3–20.2) | 67.9 (65.4–68.5) | **3.12× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn_write 1024B 1.074 ns of 6.250 (17.2 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 3.759 ns of 16.797 (22.4 %)
- system: 1/12 cells within 0.3 ns; widest relative churn 64B 3.418 ns of 43.945 (7.8 %)
