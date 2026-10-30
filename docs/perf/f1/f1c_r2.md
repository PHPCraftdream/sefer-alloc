# bench:table (paired): 3 process(es) × 200 rounds; commit 2b225d83, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.1–7.3) | 4.7 (4.7–4.8) | 45.1 (44.8–45.2) | 1.54× slower |
| 64B | 6.7 (6.5–6.8) | 6.3 (6.3–6.4) | 46.1 (46.1–46.2) | 1.05× slower |
| 256B | 7.8 (7.7–7.8) | 8.0 (8.0–8.0) | 40.7 (40.5–40.7) | **1.02× faster** |
| 1024B | 10.5 (10.4–10.6) | 17.0 (17.0–17.1) | 255.6 (252.9–268.8) | **1.62× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.8) | 6.0 (5.9–6.0) | 41.5 (41.4–41.6) | **1.03× faster** |
| 64B | 5.8 (5.7–5.8) | 6.6 (6.4–7.0) | 44.8 (44.2–45.3) | **1.14× faster** |
| 256B | 5.8 (5.8–5.9) | 9.1 (9.0–9.6) | 38.7 (38.6–41.8) | **1.57× faster** |
| 1024B | 5.9 (5.9–6.1) | 18.6 (16.5–19.6) | 43.8 (43.8–46.5) | **3.15× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.9–6.0) | 5.8 (5.8–5.9) | 41.6 (41.5–41.7) | 1.03× slower |
| 64B | 5.9 (5.9–6.1) | 6.1 (6.0–6.3) | 42.6 (42.5–43.0) | **1.02× faster** |
| 256B | 6.2 (6.0–6.3) | 8.3 (8.1–9.5) | 41.4 (40.6–44.2) | **1.36× faster** |
| 1024B | 7.0 (6.6–7.1) | 20.2 (20.0–20.4) | 71.5 (68.4–77.8) | **2.89× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 1024B 0.439 ns of 6.982 (6.3 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 1.464 ns of 8.350 (17.5 %)
- system: 4/12 cells within 0.3 ns; widest relative churn_write 1024B 9.424 ns of 71.484 (13.2 %)
