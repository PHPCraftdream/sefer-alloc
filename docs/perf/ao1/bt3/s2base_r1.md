# bench:table (paired): 3 process(es) × 200 rounds; commit eb95709d, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.4–5.9) | 4.4 (4.4–4.5) | 42.0 (41.6–43.1) | 1.25× slower |
| 64B | 5.4 (5.1–6.2) | 6.1 (5.9–6.1) | 43.5 (43.3–43.7) | **1.13× faster** |
| 256B | 5.8 (5.7–5.9) | 7.6 (7.5–7.6) | 54.0 (52.7–54.1) | **1.31× faster** |
| 1024B | 8.1 (8.0–8.1) | 15.1 (15.1–15.3) | 44.5 (43.9–44.9) | **1.87× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.7–5.3) | 5.3 (5.3–5.3) | 38.1 (37.4–38.2) | **1.09× faster** |
| 64B | 4.6 (4.6–4.6) | 5.9 (5.8–6.0) | 40.8 (40.1–40.8) | **1.27× faster** |
| 256B | 4.7 (4.7–4.7) | 7.9 (7.8–8.4) | 35.0 (34.4–35.3) | **1.69× faster** |
| 1024B | 4.9 (4.8–4.9) | 18.3 (16.0–18.7) | 41.3 (39.1–42.0) | **3.75× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–5.5) | 5.3 (5.3–5.4) | 37.8 (37.8–38.6) | **1.09× faster** |
| 64B | 4.8 (4.8–4.8) | 5.6 (5.5–5.6) | 39.7 (39.2–40.8) | **1.15× faster** |
| 256B | 5.2 (5.0–5.3) | 8.3 (8.3–8.7) | 37.7 (37.0–38.4) | **1.60× faster** |
| 1024B | 5.5 (5.3–5.6) | 17.5 (17.1–18.5) | 66.1 (59.9–66.9) | **3.17× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative direct 64B 1.074 ns of 5.371 (20.0 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 2.685 ns of 18.311 (14.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 6.983 ns of 66.113 (10.6 %)
