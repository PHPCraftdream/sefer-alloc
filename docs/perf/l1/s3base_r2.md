# bench:table (paired): 3 process(es) × 200 rounds; commit 10b53942, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.3–6.6) | 4.8 (4.8–4.8) | 44.0 (43.8–45.0) | 1.36× slower |
| 64B | 6.2 (6.1–9.7) | 6.4 (6.3–6.5) | 46.1 (44.9–46.5) | **1.04× faster** |
| 256B | 7.0 (6.8–7.0) | 7.9 (7.9–8.1) | 42.0 (39.4–42.7) | **1.13× faster** |
| 1024B | 9.3 (9.2–9.5) | 17.1 (16.6–17.3) | 267.3 (258.9–268.4) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.4–5.7) | 5.8 (5.8–5.9) | 41.1 (40.5–42.0) | **1.05× faster** |
| 64B | 5.5 (5.4–5.6) | 6.3 (6.2–6.4) | 44.1 (43.8–45.3) | **1.16× faster** |
| 256B | 5.5 (5.5–5.6) | 9.5 (8.9–9.8) | 38.4 (36.9–40.5) | **1.74× faster** |
| 1024B | 5.7 (5.6–5.8) | 18.4 (17.7–20.3) | 42.5 (42.2–47.0) | **3.25× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.9) | 5.8 (5.7–6.0) | 41.2 (39.9–42.7) | **1.02× faster** |
| 64B | 5.7 (5.6–6.5) | 6.0 (5.9–6.0) | 43.5 (42.9–43.9) | **1.06× faster** |
| 256B | 6.0 (5.7–6.3) | 8.9 (8.3–9.1) | 43.3 (39.7–43.6) | **1.50× faster** |
| 1024B | 6.2 (6.2–7.9) | 19.7 (19.1–20.2) | 66.3 (65.0–75.4) | **3.18× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative direct 64B 3.613 ns of 6.201 (58.3 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 2.588 ns of 18.408 (14.1 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 10.352 ns of 66.309 (15.6 %)
