# bench:table (paired): 3 process(es) × 200 rounds; commit f17c9f0e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.1–7.2) | 4.7 (4.7–4.7) | 44.3 (43.2–44.3) | 1.51× slower |
| 64B | 6.8 (6.6–6.9) | 6.3 (6.3–6.4) | 45.1 (44.7–45.7) | 1.09× slower |
| 256B | 7.5 (7.5–7.6) | 7.9 (7.8–7.9) | 41.1 (40.0–41.2) | **1.05× faster** |
| 1024B | 9.7 (9.6–10.0) | 16.6 (16.5–16.7) | 261.5 (237.0–262.0) | **1.71× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.6) | 5.7 (5.7–5.9) | 40.4 (40.3–41.3) | **1.02× faster** |
| 64B | 5.5 (5.5–5.6) | 6.1 (6.1–6.4) | 42.1 (41.6–42.2) | **1.10× faster** |
| 256B | 5.7 (5.7–5.8) | 9.0 (9.0–9.1) | 37.9 (37.4–38.5) | **1.59× faster** |
| 1024B | 5.8 (5.8–5.9) | 16.7 (16.2–17.9) | 42.6 (42.6–42.9) | **2.87× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.9) | 5.7 (5.6–5.8) | 40.4 (40.3–40.8) | 1.00× (tie) |
| 64B | 5.8 (5.8–6.0) | 5.8 (5.8–5.9) | 42.4 (42.1–43.7) | 1.00× (tie) |
| 256B | 5.9 (5.9–6.1) | 8.2 (7.9–9.2) | 41.6 (40.4–41.7) | **1.39× faster** |
| 1024B | 6.4 (6.2–6.5) | 19.7 (19.0–20.1) | 71.0 (70.2–72.3) | **3.08× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn_write 1024B 0.293 ns of 6.396 (4.6 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn_write 256B 1.319 ns of 8.203 (16.1 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 25.000 ns of 261.523 (9.6 %)
