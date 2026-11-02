# bench:table (paired): 3 process(es) × 200 rounds; commit 82626f9c, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.5) | 4.4 (4.4–4.4) | 42.8 (42.0–42.9) | 1.24× slower |
| 64B | 5.7 (5.0–6.4) | 6.4 (6.0–6.4) | 43.3 (42.8–44.3) | **1.12× faster** |
| 256B | 5.7 (5.7–5.8) | 7.8 (7.5–7.8) | 43.2 (41.7–55.2) | **1.38× faster** |
| 1024B | 8.3 (8.2–8.3) | 15.5 (15.4–15.5) | 277.2 (274.2–297.4) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.4 (5.3–5.4) | 37.5 (37.5–38.1) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 6.0 (5.9–6.0) | 40.6 (40.4–41.0) | **1.28× faster** |
| 256B | 4.7 (4.6–5.1) | 8.3 (7.9–8.6) | 42.8 (42.5–43.8) | **1.77× faster** |
| 1024B | 4.8 (4.8–4.8) | 15.9 (15.7–16.4) | 38.6 (38.5–38.7) | **3.29× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.4 (5.3–5.4) | 37.7 (37.5–37.9) | **1.12× faster** |
| 64B | 4.9 (4.9–4.9) | 5.6 (5.5–5.6) | 40.1 (39.6–40.7) | **1.14× faster** |
| 256B | 5.0 (4.9–5.2) | 8.4 (8.3–8.7) | 42.5 (41.2–44.1) | **1.70× faster** |
| 1024B | 5.3 (5.2–5.7) | 18.2 (17.3–19.0) | 42.5 (42.5–44.5) | **3.45× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 1.465 ns of 5.713 (25.6 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 0.782 ns of 8.301 (9.4 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 13.477 ns of 43.213 (31.2 %)
