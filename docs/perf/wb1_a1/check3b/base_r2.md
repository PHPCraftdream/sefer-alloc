# bench:table (paired): 3 process(es) × 200 rounds; commit 0ee09c13, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.1–7.3) | 4.7 (4.7–4.7) | 44.9 (44.2–45.2) | 1.52× slower |
| 64B | 6.9 (6.8–7.0) | 6.5 (6.4–6.5) | 46.2 (45.1–46.2) | 1.07× slower |
| 256B | 7.8 (7.7–7.8) | 8.0 (8.0–8.1) | 40.4 (39.9–41.2) | **1.03× faster** |
| 1024B | 10.0 (10.0–10.1) | 16.8 (16.6–16.8) | 256.7 (254.7–270.1) | **1.69× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.7) | 5.8 (5.8–5.8) | 41.3 (40.7–42.1) | **1.03× faster** |
| 64B | 5.7 (5.6–5.7) | 6.3 (6.3–6.4) | 42.9 (42.7–43.0) | **1.11× faster** |
| 256B | 5.7 (5.7–6.4) | 9.0 (8.8–9.0) | 37.4 (37.1–37.8) | **1.57× faster** |
| 1024B | 5.9 (5.9–6.1) | 16.3 (15.8–16.9) | 41.8 (41.6–41.8) | **2.75× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.8–6.1) | 5.7 (5.7–5.8) | 41.7 (41.4–41.9) | 1.04× slower |
| 64B | 5.9 (5.8–5.9) | 5.9 (5.9–5.9) | 42.5 (42.1–43.1) | **1.01× faster** |
| 256B | 6.0 (5.9–6.0) | 8.3 (8.3–8.4) | 40.3 (40.3–40.9) | **1.40× faster** |
| 1024B | 6.3 (6.3–6.4) | 19.7 (18.5–20.2) | 68.7 (67.8–69.3) | **3.12× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn 256B 0.781 ns of 5.713 (13.7 %)
- mimalloc: 10/12 cells within 0.3 ns; widest relative churn_write 1024B 1.709 ns of 19.678 (8.7 %)
- system: 2/12 cells within 0.3 ns; widest relative direct 1024B 15.380 ns of 256.738 (6.0 %)
