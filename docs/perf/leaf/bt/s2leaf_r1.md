# bench:table (paired): 3 process(es) × 200 rounds; commit ade284e0, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.8) | 4.4 (4.4–4.5) | 41.8 (41.6–42.2) | 1.30× slower |
| 64B | 5.5 (5.4–7.8) | 6.0 (6.0–6.3) | 43.3 (43.0–43.9) | **1.09× faster** |
| 256B | 5.9 (5.6–5.9) | 7.6 (7.5–7.7) | 54.3 (54.1–63.6) | **1.29× faster** |
| 1024B | 8.2 (8.0–8.3) | 15.4 (15.2–15.5) | 44.8 (44.0–44.9) | **1.89× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.4 (5.3–5.4) | 38.3 (37.4–38.3) | **1.15× faster** |
| 64B | 4.6 (4.6–4.7) | 5.9 (5.9–6.5) | 40.8 (40.6–41.1) | **1.27× faster** |
| 256B | 4.7 (4.7–4.8) | 10.6 (9.2–10.8) | 35.6 (35.4–36.6) | **2.27× faster** |
| 1024B | 4.9 (4.9–5.3) | 18.7 (18.3–18.9) | 42.0 (39.6–42.1) | **3.78× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.1) | 5.3 (5.3–5.4) | 38.2 (38.2–38.6) | **1.07× faster** |
| 64B | 4.9 (4.9–4.9) | 5.5 (5.5–6.1) | 40.9 (40.4–41.1) | **1.13× faster** |
| 256B | 5.0 (5.0–5.0) | 8.7 (8.6–8.7) | 38.9 (37.6–39.1) | **1.75× faster** |
| 1024B | 5.3 (5.3–5.5) | 18.7 (18.6–18.9) | 67.2 (65.0–67.6) | **3.51× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 2.441 ns of 5.518 (44.2 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 1.611 ns of 10.645 (15.1 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 9.472 ns of 54.297 (17.4 %)
