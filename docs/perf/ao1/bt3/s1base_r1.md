# bench:table (paired): 3 process(es) × 200 rounds; commit eb95709d, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.9) | 4.4 (4.4–4.4) | 42.5 (41.8–42.7) | 1.29× slower |
| 64B | 5.3 (5.0–5.5) | 6.0 (6.0–6.1) | 43.5 (43.2–43.6) | **1.14× faster** |
| 256B | 5.8 (5.6–5.9) | 7.6 (7.5–7.6) | 53.7 (44.4–56.3) | **1.31× faster** |
| 1024B | 8.1 (7.9–8.1) | 15.4 (15.2–15.5) | 46.1 (45.5–80.2) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (4.7–5.4) | 5.4 (5.4–5.5) | 38.3 (38.0–40.2) | **1.01× faster** |
| 64B | 4.7 (4.6–5.6) | 5.9 (5.9–6.0) | 41.5 (40.8–41.8) | **1.26× faster** |
| 256B | 4.7 (4.7–4.9) | 8.3 (8.0–8.8) | 35.9 (35.4–36.0) | **1.75× faster** |
| 1024B | 4.9 (4.9–5.0) | 16.8 (16.3–16.9) | 41.1 (39.5–41.8) | **3.44× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.5) | 5.4 (5.3–5.4) | 38.8 (38.2–40.1) | **1.09× faster** |
| 64B | 4.9 (4.9–5.2) | 5.5 (5.5–5.6) | 40.7 (39.5–40.9) | **1.13× faster** |
| 256B | 5.0 (4.9–5.0) | 8.7 (8.6–8.8) | 38.0 (38.0–38.6) | **1.75× faster** |
| 1024B | 5.3 (5.3–5.3) | 18.8 (17.8–19.1) | 63.8 (61.9–66.0) | **3.56× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn 64B 0.927 ns of 4.688 (19.8 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 0.830 ns of 8.301 (10.0 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 34.717 ns of 46.094 (75.3 %)
