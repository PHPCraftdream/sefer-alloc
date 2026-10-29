# bench:table (paired): 3 process(es) × 200 rounds; commit 73241d28, features `production internals bench-internals`, churn regime `steady`, 2 calls/sample; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 10.1 (9.9–10.1) | 4.7 (4.6–4.8) | 44.4 (43.0–45.5) | 2.12× slower |
| 64B | 9.7 (9.6–9.8) | 6.5 (6.3–6.5) | 45.7 (44.9–46.0) | 1.49× slower |
| 256B | 10.4 (10.2–10.6) | 7.9 (7.9–8.1) | 39.7 (39.6–40.8) | 1.31× slower |
| 1024B | 13.6 (13.3–13.7) | 16.8 (15.9–16.9) | 259.6 (256.0–260.3) | **1.24× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.5 (7.3–7.6) | 5.6 (5.6–5.8) | 40.3 (40.0–41.2) | 1.34× slower |
| 64B | 7.5 (7.3–7.5) | 6.1 (6.0–6.2) | 43.1 (42.5–43.4) | 1.22× slower |
| 256B | 7.6 (7.4–7.6) | 8.6 (8.4–8.9) | 37.8 (37.5–41.6) | **1.14× faster** |
| 1024B | 7.7 (7.6–7.8) | 17.1 (16.7–17.1) | 41.0 (40.9–41.8) | **2.22× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.6 (7.4–7.7) | 5.7 (5.6–5.7) | 40.5 (40.1–41.8) | 1.34× slower |
| 64B | 7.7 (7.6–7.8) | 5.8 (5.8–5.9) | 43.6 (41.7–43.7) | 1.32× slower |
| 256B | 8.0 (7.9–8.1) | 8.4 (8.3–8.5) | 41.3 (40.8–41.4) | **1.05× faster** |
| 1024B | 8.1 (8.0–8.2) | 19.2 (19.2–19.2) | 69.7 (50.0–71.6) | **2.37× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 16B 0.342 ns of 7.520 (4.5 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative direct 1024B 1.025 ns of 16.797 (6.1 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 21.680 ns of 69.727 (31.1 %)
