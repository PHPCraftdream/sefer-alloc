# bench:table (paired): 3 process(es) × 200 rounds; commit eb95709d, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–6.0) | 4.4 (4.4–4.5) | 42.0 (41.7–42.5) | 1.25× slower |
| 64B | 5.6 (5.1–7.7) | 6.0 (6.0–6.1) | 43.5 (43.5–43.7) | **1.07× faster** |
| 256B | 5.7 (5.7–5.8) | 7.6 (7.6–7.6) | 55.0 (54.5–57.0) | **1.32× faster** |
| 1024B | 8.1 (8.1–8.2) | 15.2 (15.2–15.3) | 46.2 (43.8–48.3) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.7) | 5.4 (5.4–5.5) | 38.5 (37.5–38.5) | **1.15× faster** |
| 64B | 4.6 (4.6–4.7) | 6.0 (5.9–6.0) | 40.7 (40.5–41.1) | **1.28× faster** |
| 256B | 4.7 (4.7–4.7) | 8.0 (8.0–9.1) | 37.9 (37.1–38.2) | **1.71× faster** |
| 1024B | 4.9 (4.9–5.3) | 18.2 (15.6–18.7) | 41.2 (40.0–43.7) | **3.72× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (4.8–5.5) | 5.4 (5.4–5.4) | 38.4 (37.3–38.7) | **1.02× faster** |
| 64B | 4.9 (4.9–5.1) | 5.5 (5.5–5.6) | 40.3 (39.4–40.9) | **1.13× faster** |
| 256B | 5.1 (4.9–5.1) | 8.6 (8.3–8.6) | 40.4 (39.8–41.0) | **1.70× faster** |
| 1024B | 5.4 (5.3–5.5) | 17.7 (17.4–18.8) | 67.4 (66.0–67.7) | **3.26× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 2.588 ns of 5.615 (46.1 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 3.125 ns of 18.164 (17.2 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 4.590 ns of 46.191 (9.9 %)
