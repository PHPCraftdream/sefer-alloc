# bench:table (paired): 3 process(es) × 200 rounds; commit 20395f5e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.2–7.3) | 4.8 (4.7–4.9) | 45.3 (44.6–45.6) | 1.53× slower |
| 64B | 9.9 (6.8–11.4) | 6.4 (6.3–6.6) | 46.5 (45.8–46.6) | 1.54× slower |
| 256B | 8.0 (7.9–8.1) | 8.0 (7.7–8.2) | 41.2 (40.4–42.4) | 1.00× (tie) |
| 1024B | 10.4 (10.3–10.7) | 17.6 (17.5–18.1) | 284.8 (282.5–285.4) | **1.69× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.9) | 5.9 (5.8–6.0) | 41.9 (41.2–42.0) | **1.02× faster** |
| 64B | 5.7 (5.7–5.8) | 6.7 (6.4–6.9) | 44.8 (43.9–45.9) | **1.18× faster** |
| 256B | 5.8 (5.7–6.0) | 11.9 (8.9–12.4) | 40.0 (38.5–41.4) | **2.05× faster** |
| 1024B | 6.3 (5.9–6.3) | 19.5 (16.7–20.3) | 48.6 (42.8–51.0) | **3.12× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–6.1) | 5.8 (5.8–5.9) | 42.2 (41.2–42.2) | 1.01× slower |
| 64B | 5.9 (5.9–6.3) | 6.0 (6.0–6.1) | 43.6 (43.5–45.6) | **1.03× faster** |
| 256B | 6.0 (5.9–6.1) | 8.9 (8.3–9.4) | 43.5 (41.3–43.7) | **1.48× faster** |
| 1024B | 6.6 (6.3–6.9) | 20.5 (19.9–21.5) | 72.0 (69.9–74.3) | **3.08× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative direct 64B 4.639 ns of 9.912 (46.8 %)
- mimalloc: 5/12 cells within 0.3 ns; widest relative churn 256B 3.467 ns of 11.914 (29.1 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 8.252 ns of 48.633 (17.0 %)
