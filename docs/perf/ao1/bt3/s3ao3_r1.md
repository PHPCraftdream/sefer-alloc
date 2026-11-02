# bench:table (paired): 3 process(es) × 200 rounds; commit 42a0439b, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.6) | 4.4 (4.4–4.6) | 42.7 (41.9–43.5) | 1.25× slower |
| 64B | 5.3 (5.2–5.5) | 6.1 (6.0–6.2) | 44.5 (43.3–44.8) | **1.16× faster** |
| 256B | 5.9 (5.7–6.0) | 7.5 (7.5–7.8) | 53.6 (53.2–55.6) | **1.28× faster** |
| 1024B | 8.0 (7.9–8.2) | 15.1 (15.1–15.8) | 45.4 (45.4–47.4) | **1.89× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.8) | 5.4 (5.4–5.5) | 38.6 (38.5–41.0) | **1.14× faster** |
| 64B | 4.6 (4.6–4.7) | 5.9 (5.9–6.1) | 41.7 (40.6–41.9) | **1.27× faster** |
| 256B | 4.7 (4.7–5.5) | 8.4 (8.2–10.2) | 35.9 (35.1–37.8) | **1.78× faster** |
| 1024B | 4.9 (4.9–5.0) | 17.4 (15.6–19.2) | 41.5 (41.2–45.6) | **3.53× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.6) | 5.4 (5.4–5.5) | 38.8 (38.7–40.0) | **1.09× faster** |
| 64B | 5.0 (4.9–5.1) | 5.6 (5.5–5.6) | 40.8 (39.7–41.6) | **1.12× faster** |
| 256B | 5.0 (4.9–5.4) | 8.6 (7.2–8.8) | 38.4 (38.3–40.6) | **1.71× faster** |
| 1024B | 5.5 (5.3–6.8) | 18.7 (18.5–19.0) | 66.5 (60.9–67.2) | **3.38× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 1024B 1.465 ns of 5.518 (26.5 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.002 ns of 8.447 (23.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 4.394 ns of 41.504 (10.6 %)
