# bench:table (paired): 3 process(es) × 200 rounds; commit eb95709d, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.4 (4.4–4.4) | 42.9 (42.3–43.1) | 1.26× slower |
| 64B | 5.0 (4.8–6.2) | 6.0 (6.0–6.1) | 43.6 (43.2–43.7) | **1.21× faster** |
| 256B | 5.6 (5.6–5.7) | 7.6 (7.5–7.7) | 54.6 (54.0–55.0) | **1.35× faster** |
| 1024B | 8.0 (8.0–8.1) | 15.3 (15.2–15.6) | 44.8 (44.5–45.3) | **1.91× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.3 (5.3–5.5) | 38.6 (38.6–38.9) | **1.14× faster** |
| 64B | 4.6 (4.6–4.7) | 5.8 (5.8–5.9) | 40.6 (40.6–40.9) | **1.24× faster** |
| 256B | 4.8 (4.6–5.6) | 8.2 (8.1–8.4) | 34.7 (34.5–35.0) | **1.70× faster** |
| 1024B | 4.8 (4.8–5.0) | 18.4 (16.3–18.7) | 40.0 (39.2–43.3) | **3.80× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.4) | 5.4 (5.4–5.5) | 38.9 (38.4–39.6) | **1.08× faster** |
| 64B | 4.8 (4.8–4.9) | 5.5 (5.5–5.6) | 40.2 (39.5–40.7) | **1.14× faster** |
| 256B | 5.0 (4.9–5.0) | 8.6 (8.3–8.6) | 38.9 (38.2–39.4) | **1.74× faster** |
| 1024B | 5.3 (5.2–5.3) | 18.7 (17.4–18.8) | 65.5 (60.4–69.0) | **3.54× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 1.318 ns of 4.980 (26.5 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 2.343 ns of 18.359 (12.8 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 8.545 ns of 65.527 (13.0 %)
