# bench:table (paired): 3 process(es) × 200 rounds; commit 5fd6b277, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (6.2–6.3) | 4.6 (4.6–4.6) | 43.8 (43.0–45.0) | 1.35× slower |
| 64B | 6.1 (6.0–8.0) | 6.3 (6.2–6.3) | 44.2 (44.2–45.3) | **1.03× faster** |
| 256B | 6.6 (6.5–6.7) | 7.6 (7.6–7.7) | 38.8 (38.4–39.9) | **1.15× faster** |
| 1024B | 9.0 (8.8–9.3) | 16.4 (15.9–16.7) | 259.8 (256.7–261.7) | **1.82× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (5.2–5.5) | 5.6 (5.5–6.0) | 40.0 (39.8–40.2) | **1.04× faster** |
| 64B | 5.3 (5.2–5.3) | 5.9 (5.9–6.3) | 42.2 (42.2–43.3) | **1.11× faster** |
| 256B | 5.3 (5.3–5.4) | 9.8 (8.8–11.0) | 37.0 (36.3–38.9) | **1.84× faster** |
| 1024B | 5.5 (5.5–5.6) | 17.2 (17.1–19.4) | 41.4 (40.3–47.1) | **3.14× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.5–5.7) | 5.5 (5.5–5.7) | 40.3 (39.3–40.8) | 1.03× slower |
| 64B | 5.5 (5.5–5.7) | 5.6 (5.6–5.8) | 42.0 (41.9–42.4) | **1.02× faster** |
| 256B | 5.7 (5.6–5.8) | 8.6 (8.1–9.0) | 40.6 (39.6–40.7) | **1.51× faster** |
| 1024B | 5.9 (5.9–6.3) | 19.8 (19.6–19.9) | 70.5 (69.2–75.1) | **3.36× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 2.002 ns of 6.055 (33.1 %)
- mimalloc: 5/12 cells within 0.3 ns; widest relative churn 256B 2.197 ns of 9.814 (22.4 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 6.787 ns of 41.357 (16.4 %)
