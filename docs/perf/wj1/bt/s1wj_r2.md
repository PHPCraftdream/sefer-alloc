# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–6.0) | 4.6 (4.5–4.6) | 43.0 (42.6–45.0) | 1.29× slower |
| 64B | 6.0 (6.0–6.1) | 6.1 (6.1–6.3) | 44.1 (44.1–44.9) | **1.02× faster** |
| 256B | 6.3 (6.3–6.6) | 7.8 (7.6–7.9) | 42.7 (39.6–58.4) | **1.24× faster** |
| 1024B | 8.7 (8.6–8.8) | 16.0 (15.7–16.2) | 293.7 (252.4–300.7) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.7–4.9) | 5.5 (5.5–5.6) | 40.7 (39.7–40.8) | **1.15× faster** |
| 64B | 4.7 (4.7–4.8) | 6.1 (6.0–6.1) | 41.9 (41.6–42.5) | **1.28× faster** |
| 256B | 4.8 (4.7–4.8) | 8.6 (8.6–8.9) | 42.7 (41.3–43.4) | **1.81× faster** |
| 1024B | 5.0 (5.0–5.0) | 16.7 (16.2–17.7) | 42.4 (40.2–43.8) | **3.36× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (5.0–5.1) | 5.6 (5.5–5.6) | 40.4 (39.7–40.7) | **1.10× faster** |
| 64B | 5.0 (4.9–5.0) | 5.7 (5.6–5.7) | 41.7 (41.6–43.8) | **1.14× faster** |
| 256B | 5.1 (5.0–5.3) | 8.3 (8.0–8.8) | 41.4 (40.8–41.7) | **1.63× faster** |
| 1024B | 5.4 (5.3–5.4) | 19.0 (18.8–19.7) | 50.3 (44.8–51.5) | **3.51× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn_write 256B 0.342 ns of 5.127 (6.7 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 0.781 ns of 8.350 (9.4 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 18.847 ns of 42.676 (44.2 %)
