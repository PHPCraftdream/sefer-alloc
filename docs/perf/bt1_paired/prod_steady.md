# bench:table (paired): 3 process(es) × 200 rounds; commit 73241d28, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.3–7.3) | 4.7 (4.7–4.7) | 45.8 (44.5–46.1) | 1.54× slower |
| 64B | 7.0 (6.9–8.4) | 6.3 (6.3–6.3) | 45.5 (45.4–45.8) | 1.12× slower |
| 256B | 7.9 (7.7–7.9) | 7.9 (7.7–7.9) | 40.7 (40.6–40.8) | 1.00× (tie) |
| 1024B | 10.2 (10.2–10.2) | 16.6 (16.2–16.7) | 262.6 (258.3–270.5) | **1.62× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.8) | 5.8 (5.7–5.9) | 41.1 (40.6–42.4) | **1.03× faster** |
| 64B | 5.6 (5.6–5.6) | 6.3 (6.3–6.4) | 43.2 (42.7–43.4) | **1.13× faster** |
| 256B | 5.9 (5.7–7.0) | 9.2 (8.1–9.4) | 38.2 (37.5–42.2) | **1.56× faster** |
| 1024B | 6.2 (5.9–6.2) | 17.0 (16.7–20.0) | 44.1 (41.7–44.4) | **2.75× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.9) | 5.7 (5.7–5.9) | 40.9 (40.6–42.9) | 1.02× slower |
| 64B | 6.4 (5.8–6.4) | 5.9 (5.9–6.0) | 42.4 (42.2–43.6) | 1.08× slower |
| 256B | 6.2 (6.1–6.4) | 8.3 (8.2–9.4) | 41.3 (41.0–42.1) | **1.34× faster** |
| 1024B | 6.6 (6.5–6.8) | 19.6 (19.5–19.9) | 66.4 (50.3–68.1) | **2.96× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 1.513 ns of 7.031 (21.5 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 3.321 ns of 17.041 (19.5 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 17.822 ns of 66.357 (26.9 %)
