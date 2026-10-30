# bench:table (paired): 3 process(es) × 200 rounds; commit 10b53942, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.4 (6.3–6.5) | 4.8 (4.7–4.9) | 44.3 (43.8–44.3) | 1.35× slower |
| 64B | 6.2 (6.1–6.3) | 6.4 (6.4–6.6) | 45.9 (45.4–45.9) | **1.04× faster** |
| 256B | 6.8 (6.7–6.9) | 8.0 (7.9–8.1) | 40.2 (40.2–40.4) | **1.17× faster** |
| 1024B | 9.3 (9.2–9.3) | 16.8 (16.8–16.9) | 253.5 (245.4–258.7) | **1.82× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.5) | 5.8 (5.8–5.8) | 40.7 (39.9–40.9) | **1.05× faster** |
| 64B | 5.5 (5.4–6.1) | 6.3 (6.3–6.3) | 44.3 (43.5–45.6) | **1.15× faster** |
| 256B | 5.6 (5.5–5.6) | 8.8 (8.8–9.3) | 38.5 (36.7–42.0) | **1.59× faster** |
| 1024B | 5.7 (5.6–6.2) | 16.8 (16.7–17.1) | 42.3 (40.1–47.0) | **2.97× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–6.0) | 5.8 (5.8–5.8) | 41.1 (40.6–41.1) | 1.00× (tie) |
| 64B | 5.7 (5.7–5.8) | 5.9 (5.9–6.0) | 42.5 (42.0–43.1) | **1.03× faster** |
| 256B | 5.8 (5.8–5.9) | 8.4 (8.1–8.9) | 41.1 (40.5–42.1) | **1.45× faster** |
| 1024B | 6.3 (6.2–6.7) | 19.8 (19.7–19.9) | 65.2 (52.2–70.1) | **3.17× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 64B 0.635 ns of 5.518 (11.5 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn_write 256B 0.830 ns of 8.398 (9.9 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 17.871 ns of 65.186 (27.4 %)
