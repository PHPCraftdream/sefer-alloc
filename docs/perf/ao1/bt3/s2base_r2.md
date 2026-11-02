# bench:table (paired): 3 process(es) × 200 rounds; commit eb95709d, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.4 (4.4–4.5) | 41.7 (41.6–42.4) | 1.25× slower |
| 64B | 5.2 (5.1–5.2) | 6.0 (6.0–6.1) | 43.2 (43.0–43.9) | **1.16× faster** |
| 256B | 5.7 (5.7–6.1) | 7.6 (7.5–7.6) | 53.8 (53.3–54.9) | **1.34× faster** |
| 1024B | 8.2 (7.9–8.2) | 15.3 (14.9–15.5) | 44.8 (44.8–46.6) | **1.87× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–5.6) | 5.4 (5.3–5.4) | 37.7 (37.5–38.3) | **1.15× faster** |
| 64B | 4.6 (4.6–6.3) | 5.9 (5.9–5.9) | 40.7 (40.4–40.7) | **1.26× faster** |
| 256B | 4.7 (4.6–4.7) | 8.2 (8.2–8.3) | 37.1 (34.9–37.9) | **1.74× faster** |
| 1024B | 4.8 (4.8–5.0) | 18.1 (15.7–18.7) | 40.4 (38.4–44.9) | **3.74× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.2) | 5.4 (5.4–5.4) | 38.4 (37.9–38.9) | **1.11× faster** |
| 64B | 4.8 (4.8–4.9) | 5.6 (5.5–5.6) | 40.3 (39.2–40.7) | **1.15× faster** |
| 256B | 5.0 (4.9–5.1) | 8.5 (8.4–8.7) | 40.2 (38.3–40.2) | **1.71× faster** |
| 1024B | 5.3 (5.2–5.4) | 18.3 (18.1–18.6) | 67.4 (62.8–67.6) | **3.44× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn 64B 1.709 ns of 4.639 (36.8 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 3.027 ns of 18.066 (16.8 %)
- system: 1/12 cells within 0.3 ns; widest relative churn 1024B 6.445 ns of 40.430 (15.9 %)
