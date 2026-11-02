# bench:table (paired): 3 process(es) × 200 rounds; commit b2796882, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.5 (4.4–4.5) | 42.0 (41.7–42.6) | 1.25× slower |
| 64B | 5.2 (4.9–5.8) | 6.0 (5.9–6.1) | 44.8 (43.7–44.9) | **1.15× faster** |
| 256B | 5.7 (5.6–5.8) | 7.4 (7.3–7.5) | 38.9 (38.8–42.4) | **1.29× faster** |
| 1024B | 8.3 (8.2–8.4) | 15.7 (15.4–15.9) | 244.1 (231.2–312.0) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.9) | 5.4 (5.4–5.5) | 38.7 (38.0–39.0) | **1.16× faster** |
| 64B | 4.6 (4.6–4.6) | 5.9 (5.8–6.0) | 42.1 (41.2–42.7) | **1.27× faster** |
| 256B | 4.7 (4.7–4.8) | 8.8 (7.8–11.0) | 39.3 (39.2–42.6) | **1.86× faster** |
| 1024B | 5.3 (5.1–5.5) | 18.5 (15.9–18.9) | 38.9 (38.8–49.2) | **3.47× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.2) | 5.4 (5.3–5.4) | 38.0 (37.5–39.1) | **1.11× faster** |
| 64B | 4.9 (4.8–4.9) | 5.6 (5.5–5.7) | 40.7 (40.6–41.8) | **1.13× faster** |
| 256B | 5.0 (4.9–5.4) | 8.6 (8.3–8.8) | 39.4 (38.6–39.4) | **1.74× faster** |
| 1024B | 5.7 (5.3–5.8) | 18.7 (18.5–18.8) | 44.7 (44.5–50.6) | **3.30× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative direct 64B 0.879 ns of 5.176 (17.0 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 3.174 ns of 8.789 (36.1 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 80.762 ns of 244.092 (33.1 %)
