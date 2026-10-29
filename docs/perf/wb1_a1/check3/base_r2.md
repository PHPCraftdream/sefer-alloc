# bench:table (paired): 3 process(es) × 200 rounds; commit 0ee09c13, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.1–7.5) | 4.7 (4.6–4.9) | 44.0 (43.8–44.3) | 1.54× slower |
| 64B | 7.0 (6.8–7.0) | 6.4 (6.4–6.5) | 45.4 (45.2–45.5) | 1.08× slower |
| 256B | 7.7 (7.6–7.9) | 7.9 (7.9–8.0) | 40.4 (39.5–40.6) | **1.03× faster** |
| 1024B | 10.0 (10.0–10.2) | 16.8 (16.7–17.0) | 263.0 (248.1–270.0) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–6.3) | 5.9 (5.8–6.0) | 41.0 (39.9–42.4) | **1.05× faster** |
| 64B | 5.6 (5.5–5.6) | 6.3 (6.1–6.3) | 43.8 (42.4–44.1) | **1.13× faster** |
| 256B | 5.6 (5.6–5.7) | 8.9 (8.9–10.7) | 39.1 (37.8–39.2) | **1.59× faster** |
| 1024B | 5.9 (5.8–6.0) | 17.5 (16.5–19.5) | 41.7 (41.3–46.3) | **2.98× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.9) | 5.7 (5.7–5.9) | 40.4 (39.9–43.0) | 1.02× slower |
| 64B | 5.8 (5.8–5.9) | 5.9 (5.9–6.1) | 43.2 (42.2–44.3) | **1.03× faster** |
| 256B | 6.0 (5.9–6.1) | 8.6 (8.6–9.3) | 40.6 (40.1–43.2) | **1.44× faster** |
| 1024B | 6.3 (6.3–6.3) | 19.3 (18.8–19.5) | 69.8 (68.6–72.5) | **3.07× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn 16B 0.684 ns of 5.615 (12.2 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 1.806 ns of 8.936 (20.2 %)
- system: 1/12 cells within 0.3 ns; widest relative churn 1024B 5.078 ns of 41.699 (12.2 %)
