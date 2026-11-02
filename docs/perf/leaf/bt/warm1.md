# bench:table (paired): 3 process(es) × 200 rounds; commit b2796882, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.7) | 4.4 (4.4–4.5) | 42.3 (41.4–43.3) | 1.25× slower |
| 64B | 5.3 (5.1–7.7) | 6.0 (5.9–6.0) | 43.4 (42.9–43.8) | **1.12× faster** |
| 256B | 5.7 (5.7–5.8) | 7.5 (7.4–7.6) | 54.4 (41.2–56.6) | **1.31× faster** |
| 1024B | 8.3 (8.2–8.4) | 15.3 (15.2–15.8) | 286.0 (286.0–300.8) | **1.85× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.7) | 5.4 (5.3–5.5) | 38.0 (36.9–38.6) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 6.0 (6.0–6.0) | 40.6 (39.7–41.6) | **1.28× faster** |
| 256B | 4.7 (4.7–5.3) | 8.0 (7.9–8.0) | 41.9 (41.6–42.1) | **1.70× faster** |
| 1024B | 4.9 (4.9–4.9) | 16.4 (15.7–18.6) | 39.2 (38.6–40.0) | **3.35× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.5) | 5.4 (5.3–5.4) | 37.6 (37.5–38.4) | **1.08× faster** |
| 64B | 4.9 (4.9–5.3) | 5.6 (5.5–5.6) | 40.3 (39.9–41.2) | **1.13× faster** |
| 256B | 5.0 (5.0–5.6) | 8.6 (8.5–8.6) | 41.5 (41.4–42.2) | **1.73× faster** |
| 1024B | 6.1 (5.7–6.4) | 18.1 (17.7–19.0) | 43.9 (43.2–44.1) | **2.99× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative direct 64B 2.588 ns of 5.322 (48.6 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 2.832 ns of 16.357 (17.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 15.479 ns of 54.443 (28.4 %)
