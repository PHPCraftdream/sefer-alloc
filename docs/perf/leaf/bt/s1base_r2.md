# bench:table (paired): 3 process(es) × 200 rounds; commit b2796882, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 4.5 (4.5–4.6) | 42.8 (42.3–43.1) | 1.26× slower |
| 64B | 5.4 (5.0–7.6) | 6.1 (6.0–6.2) | 43.9 (43.6–44.5) | **1.12× faster** |
| 256B | 5.9 (5.8–6.0) | 7.6 (7.6–7.7) | 43.9 (43.1–56.5) | **1.30× faster** |
| 1024B | 8.4 (8.4–8.5) | 15.9 (15.7–16.0) | 283.5 (277.6–289.0) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.8) | 5.5 (5.5–5.6) | 38.6 (38.4–38.6) | **1.15× faster** |
| 64B | 4.8 (4.7–4.9) | 6.1 (6.1–6.1) | 41.3 (40.2–41.6) | **1.27× faster** |
| 256B | 4.8 (4.8–5.5) | 8.5 (7.7–10.2) | 43.1 (41.6–44.1) | **1.77× faster** |
| 1024B | 5.0 (4.9–5.0) | 19.0 (16.3–19.0) | 39.2 (38.8–39.4) | **3.82× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.0) | 5.4 (5.4–5.5) | 38.4 (38.0–39.1) | **1.09× faster** |
| 64B | 5.0 (4.9–5.2) | 5.6 (5.5–5.7) | 40.0 (38.9–41.0) | **1.12× faster** |
| 256B | 5.0 (5.0–5.1) | 8.8 (8.2–8.9) | 43.1 (40.3–44.1) | **1.75× faster** |
| 1024B | 5.4 (5.3–5.8) | 18.6 (18.0–18.9) | 45.8 (43.8–46.0) | **3.45× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 2.637 ns of 5.420 (48.7 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.490 ns of 8.545 (29.1 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 13.428 ns of 43.945 (30.6 %)
