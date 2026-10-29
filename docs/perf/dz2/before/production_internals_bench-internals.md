# bench:table (paired): 3 process(es) × 200 rounds; commit 3782939c, features `production internals bench-internals`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 10.3 (10.2–12.1) | 4.9 (4.8–6.7) | 45.6 (44.8–60.6) | 2.09× slower |
| 64B | 12.1 (10.1–14.0) | 6.7 (6.6–8.2) | 46.9 (46.2–57.7) | 1.81× slower |
| 256B | 11.2 (10.7–15.2) | 8.3 (8.1–10.0) | 43.2 (41.7–49.5) | 1.35× slower |
| 1024B | 15.0 (14.1–26.2) | 18.2 (17.3–29.1) | 281.3 (263.4–450.4) | **1.21× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.9 (7.7–12.1) | 6.4 (6.0–12.0) | 42.0 (40.6–65.2) | 1.24× slower |
| 64B | 8.0 (7.6–11.3) | 7.6 (6.5–14.4) | 45.9 (44.8–71.9) | 1.05× slower |
| 256B | 8.0 (7.6–13.1) | 13.4 (12.2–21.7) | 47.1 (42.5–72.9) | **1.68× faster** |
| 1024B | 9.3 (7.8–14.5) | 19.7 (18.2–31.7) | 45.7 (43.3–81.5) | **2.12× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 8.1 (7.7–10.3) | 6.0 (5.9–11.3) | 41.5 (39.9–61.9) | 1.35× slower |
| 64B | 8.7 (7.9–13.3) | 6.3 (6.1–12.1) | 45.9 (42.6–71.6) | 1.38× slower |
| 256B | 8.8 (8.2–13.2) | 10.2 (9.6–18.0) | 45.6 (42.3–72.3) | **1.15× faster** |
| 1024B | 9.5 (8.7–19.1) | 20.7 (20.0–32.8) | 54.9 (48.4–103.0) | **2.17× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 10.401 ns of 9.521 (109.2 %)
- mimalloc: 0/12 cells within 0.3 ns; widest relative churn 64B 7.910 ns of 7.568 (104.5 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 54.590 ns of 54.932 (99.4 %)
