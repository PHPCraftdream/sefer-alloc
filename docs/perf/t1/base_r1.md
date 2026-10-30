# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (6.9–7.3) | 4.6 (4.6–4.7) | 44.2 (43.7–45.2) | 1.55× slower |
| 64B | 6.8 (6.7–7.0) | 6.3 (6.3–6.4) | 45.3 (45.1–46.5) | 1.09× slower |
| 256B | 7.6 (7.5–8.0) | 7.8 (7.7–8.1) | 40.9 (40.3–41.8) | **1.03× faster** |
| 1024B | 9.8 (9.5–10.0) | 16.6 (16.4–16.8) | 258.9 (247.9–265.7) | **1.69× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 5.7 (5.6–5.8) | 41.5 (40.0–42.5) | **1.02× faster** |
| 64B | 5.5 (5.4–5.7) | 6.3 (6.3–6.4) | 43.0 (43.0–43.2) | **1.13× faster** |
| 256B | 5.6 (5.4–5.8) | 9.0 (8.9–11.9) | 38.3 (37.1–40.8) | **1.60× faster** |
| 1024B | 5.9 (5.8–6.5) | 17.0 (17.0–19.7) | 43.1 (41.8–50.2) | **2.91× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.9) | 5.7 (5.6–5.8) | 41.6 (39.7–44.2) | 1.01× slower |
| 64B | 6.1 (5.7–6.3) | 5.9 (5.9–6.0) | 43.0 (41.7–44.0) | 1.02× slower |
| 256B | 5.8 (5.8–6.0) | 8.2 (8.1–9.0) | 41.1 (39.5–43.1) | **1.41× faster** |
| 1024B | 6.3 (6.3–6.5) | 19.0 (18.9–20.3) | 73.8 (50.5–74.0) | **2.99× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 1024B 0.732 ns of 5.859 (12.5 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 2.929 ns of 8.984 (32.6 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 23.438 ns of 73.779 (31.8 %)
