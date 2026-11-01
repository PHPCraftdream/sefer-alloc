# bench:table (paired): 3 process(es) × 200 rounds; commit 82626f9c, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.5 (4.5–4.6) | 42.2 (41.7–43.4) | 1.24× slower |
| 64B | 5.2 (5.0–5.3) | 6.0 (5.9–6.4) | 43.8 (43.3–43.8) | **1.16× faster** |
| 256B | 5.7 (5.7–5.8) | 7.4 (7.4–7.7) | 43.0 (38.1–43.0) | **1.30× faster** |
| 1024B | 8.2 (8.1–8.2) | 15.6 (15.5–15.7) | 292.7 (232.5–299.1) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.5 (5.4–5.6) | 38.0 (37.6–38.4) | **1.18× faster** |
| 64B | 4.6 (4.6–4.6) | 6.0 (5.9–6.1) | 40.2 (39.9–40.3) | **1.28× faster** |
| 256B | 4.7 (4.6–4.7) | 8.2 (8.1–8.9) | 42.8 (39.5–43.1) | **1.75× faster** |
| 1024B | 4.9 (4.9–5.1) | 16.1 (15.4–18.0) | 41.0 (38.8–42.0) | **3.30× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (4.9–5.6) | 5.4 (5.3–5.5) | 38.8 (38.2–39.7) | **1.06× faster** |
| 64B | 4.9 (4.9–4.9) | 5.6 (5.5–5.7) | 39.6 (39.2–40.1) | **1.14× faster** |
| 256B | 5.0 (5.0–5.1) | 8.6 (8.4–8.7) | 40.5 (39.9–43.3) | **1.72× faster** |
| 1024B | 5.3 (5.2–5.3) | 18.4 (18.3–18.4) | 43.7 (43.3–44.5) | **3.49× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn_write 16B 0.683 ns of 5.127 (13.3 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 2.539 ns of 16.113 (15.8 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 66.552 ns of 292.725 (22.7 %)
