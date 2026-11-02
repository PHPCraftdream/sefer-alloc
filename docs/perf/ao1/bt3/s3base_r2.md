# bench:table (paired): 3 process(es) × 200 rounds; commit eb95709d, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.6) | 4.4 (4.4–4.5) | 42.3 (41.8–42.8) | 1.24× slower |
| 64B | 5.1 (5.0–6.2) | 6.0 (6.0–6.1) | 43.5 (43.2–44.0) | **1.17× faster** |
| 256B | 5.6 (5.6–5.8) | 7.4 (7.4–7.5) | 53.7 (43.2–54.4) | **1.32× faster** |
| 1024B | 8.0 (7.9–8.1) | 15.1 (15.0–15.1) | 46.5 (45.8–81.7) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.6 (4.6–4.7) | 5.3 (5.3–5.4) | 38.5 (38.2–38.6) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 5.8 (5.8–5.8) | 40.9 (40.6–41.4) | **1.25× faster** |
| 256B | 4.7 (4.7–4.8) | 7.9 (7.8–8.8) | 35.3 (35.0–35.6) | **1.67× faster** |
| 1024B | 4.8 (4.8–4.8) | 16.7 (15.7–18.5) | 40.3 (40.2–41.2) | **3.45× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.4 (5.3–5.4) | 38.8 (38.2–40.3) | **1.11× faster** |
| 64B | 4.9 (4.8–5.0) | 5.5 (5.5–5.6) | 40.0 (39.6–40.5) | **1.13× faster** |
| 256B | 5.0 (4.9–5.4) | 8.2 (7.6–8.3) | 38.3 (37.7–38.6) | **1.64× faster** |
| 1024B | 5.4 (5.2–5.6) | 18.5 (18.5–18.7) | 64.4 (62.5–65.7) | **3.45× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 1.123 ns of 5.127 (21.9 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 2.734 ns of 16.699 (16.4 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 35.888 ns of 46.533 (77.1 %)
