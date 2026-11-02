# bench:table (paired): 3 process(es) × 200 rounds; commit 82626f9c, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.5 (4.4–4.5) | 42.1 (41.7–42.8) | 1.25× slower |
| 64B | 5.5 (5.3–8.2) | 6.0 (5.9–6.0) | 43.2 (43.2–43.9) | **1.09× faster** |
| 256B | 5.7 (5.7–5.9) | 7.5 (7.4–7.6) | 42.5 (41.8–55.8) | **1.32× faster** |
| 1024B | 8.2 (8.1–8.2) | 15.3 (15.3–15.6) | 296.7 (294.5–301.7) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.7) | 5.4 (5.4–5.4) | 38.0 (37.2–38.0) | **1.16× faster** |
| 64B | 4.6 (4.6–4.7) | 5.9 (5.9–6.0) | 40.4 (39.6–40.7) | **1.27× faster** |
| 256B | 4.7 (4.6–4.7) | 8.2 (8.1–10.8) | 42.7 (42.6–43.4) | **1.74× faster** |
| 1024B | 4.8 (4.8–4.9) | 17.9 (17.7–18.8) | 41.6 (38.7–41.9) | **3.71× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.1) | 5.3 (5.3–5.4) | 38.1 (37.8–38.1) | **1.10× faster** |
| 64B | 4.8 (4.8–5.1) | 5.6 (5.6–5.6) | 40.2 (40.1–41.1) | **1.15× faster** |
| 256B | 4.9 (4.9–5.2) | 8.6 (8.6–8.7) | 43.3 (43.0–44.2) | **1.77× faster** |
| 1024B | 5.3 (5.2–5.3) | 18.6 (18.4–18.8) | 44.2 (44.2–45.6) | **3.52× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative direct 64B 2.930 ns of 5.518 (53.1 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 256B 2.783 ns of 8.154 (34.1 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 13.916 ns of 42.480 (32.8 %)
