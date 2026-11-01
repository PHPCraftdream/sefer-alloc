# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–6.0) | 4.6 (4.6–4.6) | 44.9 (43.3–45.1) | 1.29× slower |
| 64B | 5.8 (5.7–5.8) | 6.2 (6.0–6.3) | 44.7 (43.5–45.9) | **1.08× faster** |
| 256B | 6.4 (6.3–6.5) | 7.8 (7.8–7.9) | 58.7 (44.6–63.4) | **1.21× faster** |
| 1024B | 8.6 (8.6–8.9) | 16.2 (16.1–16.7) | 310.5 (300.4–310.9) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.7–4.9) | 5.6 (5.6–5.6) | 40.6 (39.6–42.1) | **1.15× faster** |
| 64B | 4.8 (4.7–4.9) | 6.3 (6.2–6.4) | 42.0 (41.8–42.9) | **1.32× faster** |
| 256B | 4.8 (4.7–4.9) | 8.8 (8.6–10.0) | 44.8 (43.3–45.6) | **1.82× faster** |
| 1024B | 5.1 (5.0–5.1) | 17.3 (16.3–17.3) | 40.4 (40.1–43.3) | **3.41× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.1) | 5.6 (5.5–5.6) | 40.3 (39.8–41.7) | **1.11× faster** |
| 64B | 5.0 (5.0–5.1) | 5.6 (5.6–5.7) | 41.9 (41.6–42.1) | **1.13× faster** |
| 256B | 5.1 (5.1–5.5) | 8.2 (8.1–8.9) | 44.1 (42.7–44.7) | **1.60× faster** |
| 1024B | 5.4 (5.4–5.5) | 18.3 (17.7–19.5) | 46.4 (45.9–46.9) | **3.38× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn_write 256B 0.342 ns of 5.127 (6.7 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 1.318 ns of 8.789 (15.0 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 18.799 ns of 58.691 (32.0 %)
