# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (5.9–6.1) | 4.6 (4.6–4.7) | 42.7 (42.6–43.8) | 1.32× slower |
| 64B | 6.0 (5.6–6.3) | 6.3 (6.0–6.4) | 44.6 (44.1–45.5) | **1.04× faster** |
| 256B | 6.5 (6.5–6.6) | 7.8 (7.7–7.9) | 42.7 (40.8–43.8) | **1.19× faster** |
| 1024B | 8.8 (8.7–9.0) | 16.3 (16.2–16.5) | 310.0 (260.0–314.0) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.7 (5.7–5.8) | 39.9 (39.9–40.7) | **1.17× faster** |
| 64B | 4.9 (4.9–5.1) | 6.2 (6.1–6.3) | 42.5 (41.6–43.2) | **1.26× faster** |
| 256B | 4.9 (4.8–5.0) | 8.7 (8.6–9.1) | 42.6 (40.9–45.7) | **1.79× faster** |
| 1024B | 5.1 (5.0–5.2) | 16.5 (15.8–16.7) | 43.3 (42.3–44.1) | **3.22× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (5.0–5.1) | 5.5 (5.5–5.6) | 40.6 (39.5–40.7) | **1.09× faster** |
| 64B | 5.1 (5.0–5.1) | 5.7 (5.6–5.8) | 41.5 (39.7–42.6) | **1.13× faster** |
| 256B | 5.1 (5.1–5.1) | 8.9 (8.8–9.0) | 40.8 (40.8–41.4) | **1.76× faster** |
| 1024B | 5.4 (5.4–5.6) | 19.4 (19.3–19.5) | 48.2 (46.9–49.5) | **3.59× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative direct 64B 0.684 ns of 6.006 (11.4 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative direct 64B 0.390 ns of 6.250 (6.2 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 54.053 ns of 309.961 (17.4 %)
