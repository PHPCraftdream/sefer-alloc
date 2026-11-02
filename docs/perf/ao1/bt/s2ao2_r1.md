# bench:table (paired): 3 process(es) × 200 rounds; commit e551ff85, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.4 (4.4–4.5) | 42.0 (42.0–42.9) | 1.26× slower |
| 64B | 5.2 (4.8–5.3) | 6.0 (5.9–6.3) | 43.6 (43.3–43.6) | **1.15× faster** |
| 256B | 5.8 (5.6–5.9) | 7.5 (7.5–7.7) | 54.3 (41.9–56.4) | **1.31× faster** |
| 1024B | 8.1 (8.1–8.2) | 15.5 (15.5–15.6) | 277.3 (275.2–294.3) | **1.91× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–5.4) | 5.4 (5.4–5.4) | 37.7 (36.8–38.0) | **1.13× faster** |
| 64B | 4.6 (4.6–4.6) | 5.9 (5.9–6.0) | 40.4 (39.7–41.0) | **1.27× faster** |
| 256B | 4.7 (4.6–4.7) | 8.2 (7.9–8.4) | 41.9 (41.6–42.1) | **1.75× faster** |
| 1024B | 5.0 (4.8–5.1) | 18.5 (15.9–18.5) | 38.8 (38.7–39.6) | **3.71× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–10.5) | 5.3 (5.3–5.4) | 37.7 (37.5–37.9) | **1.09× faster** |
| 64B | 4.8 (4.8–5.0) | 5.5 (5.5–5.6) | 40.4 (39.5–40.5) | **1.14× faster** |
| 256B | 5.0 (4.9–5.6) | 8.6 (8.2–8.7) | 42.3 (38.8–43.1) | **1.74× faster** |
| 1024B | 5.3 (5.2–5.3) | 18.0 (17.8–18.4) | 43.4 (43.2–43.5) | **3.42× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 16B 5.664 ns of 4.883 (116.0 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 2.539 ns of 18.457 (13.8 %)
- system: 2/12 cells within 0.3 ns; widest relative direct 256B 14.453 ns of 54.346 (26.6 %)
