# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.8) | 4.5 (4.5–4.6) | 42.4 (41.8–43.8) | 1.27× slower |
| 64B | 5.9 (5.6–6.1) | 6.2 (6.0–6.2) | 43.8 (43.5–44.0) | **1.04× faster** |
| 256B | 6.2 (6.1–6.3) | 7.7 (7.5–7.7) | 42.8 (42.6–57.0) | **1.24× faster** |
| 1024B | 8.5 (8.3–8.6) | 15.8 (15.5–15.9) | 301.4 (279.1–311.8) | **1.85× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.6–4.8) | 5.5 (5.5–5.5) | 38.5 (37.5–39.6) | **1.15× faster** |
| 64B | 4.7 (4.7–4.7) | 5.8 (5.8–6.0) | 40.9 (40.3–41.5) | **1.23× faster** |
| 256B | 4.8 (4.7–4.8) | 8.6 (8.1–8.6) | 43.3 (42.4–43.9) | **1.80× faster** |
| 1024B | 5.0 (4.8–5.9) | 16.6 (15.7–18.2) | 40.9 (38.6–41.7) | **3.29× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–5.6) | 5.5 (5.5–5.7) | 38.3 (38.2–39.2) | **1.11× faster** |
| 64B | 4.9 (4.9–5.0) | 5.6 (5.6–5.7) | 40.8 (39.4–42.4) | **1.14× faster** |
| 256B | 5.1 (5.0–5.1) | 8.0 (7.4–8.0) | 41.8 (41.3–42.5) | **1.55× faster** |
| 1024B | 5.3 (5.2–5.5) | 18.6 (18.2–18.7) | 46.8 (43.4–47.6) | **3.49× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 1024B 1.074 ns of 5.029 (21.4 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 2.490 ns of 16.553 (15.0 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 14.453 ns of 42.773 (33.8 %)
