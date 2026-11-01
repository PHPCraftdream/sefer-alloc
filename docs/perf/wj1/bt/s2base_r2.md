# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (5.9–6.1) | 4.6 (4.6–4.7) | 44.1 (43.1–44.8) | 1.32× slower |
| 64B | 5.6 (5.6–6.1) | 6.2 (6.0–6.3) | 44.4 (44.2–46.3) | **1.10× faster** |
| 256B | 6.5 (6.4–6.9) | 8.0 (7.9–8.2) | 45.0 (44.5–59.1) | **1.23× faster** |
| 1024B | 9.0 (8.9–9.3) | 16.5 (16.2–17.4) | 319.9 (301.2–329.1) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.4) | 5.7 (5.6–5.9) | 40.8 (39.4–41.2) | **1.15× faster** |
| 64B | 4.8 (4.7–5.0) | 6.3 (6.3–6.5) | 43.3 (43.2–44.7) | **1.31× faster** |
| 256B | 4.9 (4.9–5.1) | 8.9 (8.9–9.0) | 44.4 (43.8–46.2) | **1.80× faster** |
| 1024B | 5.2 (5.1–5.4) | 16.3 (15.5–17.1) | 43.6 (40.7–43.8) | **3.11× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (4.9–5.5) | 5.7 (5.6–5.8) | 40.7 (40.2–42.0) | **1.08× faster** |
| 64B | 5.2 (5.0–5.2) | 5.8 (5.7–5.9) | 42.6 (42.6–43.2) | **1.11× faster** |
| 256B | 5.2 (5.1–5.4) | 8.1 (8.1–8.3) | 41.8 (41.3–42.4) | **1.57× faster** |
| 1024B | 5.6 (5.5–5.9) | 19.2 (18.6–19.6) | 47.6 (45.3–50.7) | **3.43× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 16B 0.537 ns of 4.980 (10.8 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.612 ns of 16.260 (9.9 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 14.649 ns of 44.971 (32.6 %)
