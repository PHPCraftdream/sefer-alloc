# bench:table (paired): 3 process(es) × 200 rounds; commit c0ee7dfb, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (6.1–6.7) | 4.7 (4.7–4.8) | 45.6 (44.8–47.3) | 1.31× slower |
| 64B | 8.4 (6.0–9.1) | 6.3 (6.3–6.7) | 47.2 (45.8–47.9) | 1.32× slower |
| 256B | 7.2 (6.3–7.2) | 8.3 (8.2–8.6) | 70.9 (59.9–100.6) | **1.16× faster** |
| 1024B | 9.3 (8.8–10.8) | 17.3 (17.2–17.3) | 56.0 (49.3–72.0) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.1–5.3) | 6.0 (5.8–6.6) | 42.4 (41.6–42.9) | **1.15× faster** |
| 64B | 5.0 (4.9–5.1) | 6.9 (6.5–7.8) | 48.1 (44.5–49.6) | **1.37× faster** |
| 256B | 5.3 (5.1–5.4) | 12.7 (8.7–13.8) | 41.3 (39.6–48.0) | **2.39× faster** |
| 1024B | 5.7 (5.2–7.3) | 20.6 (20.0–20.9) | 58.6 (43.4–58.7) | **3.63× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (5.1–5.4) | 5.9 (5.7–6.0) | 43.0 (41.7–43.4) | **1.09× faster** |
| 64B | 5.4 (5.2–5.5) | 6.1 (5.9–6.7) | 46.1 (44.0–48.1) | **1.13× faster** |
| 256B | 5.9 (5.4–6.0) | 10.2 (8.5–10.4) | 48.6 (42.8–51.3) | **1.73× faster** |
| 1024B | 7.1 (5.7–7.2) | 21.0 (20.3–21.6) | 80.6 (72.3–86.9) | **2.95× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 4/12 cells within 0.3 ns; widest relative direct 64B 3.174 ns of 8.398 (37.8 %)
- mimalloc: 3/12 cells within 0.3 ns; widest relative churn 256B 5.030 ns of 12.744 (39.5 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 40.723 ns of 70.898 (57.4 %)
