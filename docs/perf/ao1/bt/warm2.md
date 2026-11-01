# bench:table (paired): 3 process(es) × 200 rounds; commit e551ff85, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.6) | 4.5 (4.4–4.5) | 42.8 (42.0–43.9) | 1.25× slower |
| 64B | 5.1 (5.1–7.9) | 6.0 (5.9–6.1) | 43.7 (43.4–44.3) | **1.17× faster** |
| 256B | 5.7 (5.6–5.8) | 7.4 (7.3–7.4) | 42.5 (38.4–59.5) | **1.31× faster** |
| 1024B | 8.1 (8.0–8.1) | 15.4 (15.2–15.6) | 274.4 (244.5–299.1) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.4 (5.3–5.8) | 38.3 (37.9–40.2) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 5.9 (5.9–6.1) | 41.2 (39.8–41.6) | **1.27× faster** |
| 256B | 4.7 (4.7–4.7) | 8.2 (8.0–8.3) | 41.4 (39.0–41.6) | **1.74× faster** |
| 1024B | 4.8 (4.8–4.9) | 17.1 (16.3–18.4) | 39.7 (39.6–40.1) | **3.55× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–4.9) | 5.4 (5.3–5.8) | 38.8 (37.9–40.3) | **1.10× faster** |
| 64B | 4.8 (4.8–4.8) | 5.5 (5.5–5.6) | 40.7 (40.4–40.7) | **1.14× faster** |
| 256B | 4.9 (4.9–5.0) | 8.7 (8.3–8.7) | 39.2 (39.1–42.0) | **1.76× faster** |
| 1024B | 5.2 (5.2–5.3) | 18.4 (18.3–18.7) | 44.9 (44.6–45.9) | **3.51× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative direct 64B 2.783 ns of 5.078 (54.8 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 2.050 ns of 17.139 (12.0 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 21.093 ns of 42.529 (49.6 %)
