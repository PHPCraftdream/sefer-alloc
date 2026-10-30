# bench:table (paired): 3 process(es) × 200 rounds; commit 10b53942, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.5–6.6) | 4.7 (4.6–4.8) | 46.0 (45.9–46.1) | 1.40× slower |
| 64B | 6.3 (5.9–6.3) | 6.4 (6.3–6.4) | 46.5 (45.9–47.1) | **1.03× faster** |
| 256B | 6.9 (6.9–7.1) | 8.1 (7.9–8.1) | 41.1 (40.6–41.5) | **1.16× faster** |
| 1024B | 9.4 (9.1–9.6) | 16.9 (16.9–16.9) | 247.2 (247.0–255.0) | **1.79× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.7) | 5.9 (5.8–6.0) | 41.5 (41.0–41.6) | **1.06× faster** |
| 64B | 5.5 (5.4–5.6) | 6.4 (6.2–6.5) | 44.1 (43.9–44.8) | **1.18× faster** |
| 256B | 5.7 (5.6–5.8) | 8.9 (8.7–9.1) | 37.8 (37.7–38.2) | **1.57× faster** |
| 1024B | 5.7 (5.7–5.8) | 17.7 (17.1–20.5) | 44.5 (42.1–45.5) | **3.09× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.6–6.0) | 5.7 (5.7–5.9) | 41.6 (41.3–41.6) | 1.03× slower |
| 64B | 5.8 (5.7–6.5) | 6.0 (5.8–6.0) | 43.0 (42.9–44.0) | **1.03× faster** |
| 256B | 6.4 (6.2–6.4) | 8.3 (8.1–8.5) | 41.0 (40.4–41.5) | **1.31× faster** |
| 1024B | 6.2 (6.0–6.3) | 19.8 (19.0–20.5) | 66.5 (64.4–72.8) | **3.21× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 64B 0.781 ns of 5.811 (13.4 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 3.369 ns of 17.676 (19.1 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 8.399 ns of 66.504 (12.6 %)
