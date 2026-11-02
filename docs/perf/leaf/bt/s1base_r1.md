# bench:table (paired): 3 process(es) × 200 rounds; commit b2796882, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.5–5.7) | 4.4 (4.4–4.5) | 41.7 (41.6–42.6) | 1.27× slower |
| 64B | 5.4 (4.9–5.7) | 5.9 (5.9–6.1) | 43.2 (43.1–45.0) | **1.09× faster** |
| 256B | 5.6 (5.6–5.7) | 7.4 (7.3–7.5) | 38.6 (38.2–42.1) | **1.32× faster** |
| 1024B | 8.1 (8.0–8.2) | 15.4 (15.4–15.4) | 240.0 (230.1–280.1) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.6 (4.6–4.7) | 5.4 (5.3–5.5) | 37.8 (37.7–38.8) | **1.16× faster** |
| 64B | 4.6 (4.6–4.6) | 5.9 (5.8–6.0) | 40.4 (39.8–42.5) | **1.27× faster** |
| 256B | 4.7 (4.6–4.9) | 8.2 (8.1–8.2) | 39.6 (38.1–41.6) | **1.74× faster** |
| 1024B | 4.8 (4.8–5.2) | 18.5 (18.4–18.7) | 39.3 (38.5–39.6) | **3.82× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–5.6) | 5.4 (5.3–5.5) | 37.6 (37.4–39.6) | **1.09× faster** |
| 64B | 5.0 (4.9–5.0) | 5.5 (5.5–5.7) | 39.9 (39.8–43.3) | **1.11× faster** |
| 256B | 5.0 (4.9–5.0) | 7.4 (7.4–8.3) | 39.8 (38.0–42.0) | **1.49× faster** |
| 1024B | 5.3 (5.2–5.3) | 18.4 (17.9–18.6) | 44.5 (44.0–45.6) | **3.49× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 16B 0.732 ns of 4.932 (14.8 %)
- mimalloc: 10/12 cells within 0.3 ns; widest relative churn_write 256B 0.879 ns of 7.422 (11.8 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 49.951 ns of 239.990 (20.8 %)
