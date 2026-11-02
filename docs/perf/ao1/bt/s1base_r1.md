# bench:table (paired): 3 process(es) × 200 rounds; commit 82626f9c, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.4 (4.4–4.5) | 42.2 (41.8–42.6) | 1.26× slower |
| 64B | 5.1 (4.8–8.1) | 6.0 (6.0–6.1) | 43.4 (43.4–43.7) | **1.17× faster** |
| 256B | 5.7 (5.6–5.8) | 7.6 (7.5–7.6) | 42.9 (38.1–43.6) | **1.34× faster** |
| 1024B | 8.3 (8.2–8.3) | 15.5 (15.5–15.5) | 291.9 (226.1–309.6) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.4 (5.4–5.4) | 38.2 (37.7–38.5) | **1.16× faster** |
| 64B | 4.6 (4.6–4.7) | 6.0 (5.9–6.0) | 40.5 (40.2–40.6) | **1.28× faster** |
| 256B | 4.7 (4.7–4.7) | 8.8 (8.8–11.0) | 41.3 (38.6–42.0) | **1.86× faster** |
| 1024B | 4.9 (4.9–5.1) | 18.6 (15.6–18.7) | 39.2 (38.6–41.8) | **3.81× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–6.8) | 5.4 (5.3–5.4) | 37.9 (37.7–37.9) | **1.08× faster** |
| 64B | 4.9 (4.9–4.9) | 5.6 (5.6–5.6) | 39.9 (39.5–40.5) | **1.14× faster** |
| 256B | 5.1 (4.9–5.3) | 8.7 (8.6–8.7) | 39.4 (39.1–41.5) | **1.71× faster** |
| 1024B | 5.4 (5.2–5.7) | 18.3 (17.6–18.6) | 44.6 (43.0–45.1) | **3.41× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 3.271 ns of 5.127 (63.8 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 256B 2.197 ns of 8.789 (25.0 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 83.496 ns of 291.895 (28.6 %)
