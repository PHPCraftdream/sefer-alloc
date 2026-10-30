# bench:table (paired): 3 process(es) × 200 rounds; commit a092dc61, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.3–7.3) | 4.8 (4.7–4.8) | 44.8 (44.0–45.4) | 1.53× slower |
| 64B | 6.9 (6.7–7.0) | 6.4 (6.3–6.5) | 46.5 (45.7–46.7) | 1.08× slower |
| 256B | 7.8 (7.6–8.1) | 8.0 (7.9–8.2) | 42.2 (41.6–43.6) | **1.03× faster** |
| 1024B | 10.2 (10.0–10.5) | 17.1 (16.9–17.2) | 270.6 (257.8–271.2) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.0 (6.9–7.2) | 5.9 (5.8–5.9) | 41.9 (41.2–42.4) | 1.20× slower |
| 64B | 6.9 (6.7–7.4) | 6.4 (6.3–6.4) | 43.8 (43.0–44.5) | 1.08× slower |
| 256B | 7.0 (6.9–7.7) | 9.3 (9.0–9.4) | 39.4 (38.7–45.3) | **1.33× faster** |
| 1024B | 7.2 (7.2–7.3) | 16.5 (16.1–19.5) | 43.1 (41.1–43.5) | **2.30× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.3–7.5) | 5.8 (5.7–5.9) | 42.0 (40.9–42.1) | 1.27× slower |
| 64B | 7.1 (6.9–7.3) | 5.9 (5.9–6.0) | 43.9 (42.9–45.3) | 1.21× slower |
| 256B | 7.4 (7.2–7.9) | 8.6 (8.4–9.4) | 42.7 (42.3–45.4) | **1.16× faster** |
| 1024B | 7.8 (7.7–8.2) | 19.3 (19.0–19.7) | 69.7 (50.1–75.5) | **2.48× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 5/12 cells within 0.3 ns; widest relative churn 256B 0.781 ns of 6.982 (11.2 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 3.418 ns of 16.504 (20.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 25.391 ns of 69.727 (36.4 %)
