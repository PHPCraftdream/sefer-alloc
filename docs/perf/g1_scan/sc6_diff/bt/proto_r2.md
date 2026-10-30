# bench:table (paired): 3 process(es) × 200 rounds; commit a11dc0c7, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.0 (7.0–7.3) | 4.7 (4.6–4.8) | 45.9 (43.9–46.0) | 1.48× slower |
| 64B | 6.9 (6.8–10.1) | 6.3 (6.3–6.4) | 45.2 (45.0–45.7) | 1.09× slower |
| 256B | 7.7 (7.5–7.8) | 7.8 (7.8–7.9) | 40.4 (39.7–40.9) | **1.01× faster** |
| 1024B | 10.0 (10.0–10.2) | 16.7 (16.6–16.8) | 270.9 (253.8–272.3) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.5–6.0) | 5.8 (5.6–5.9) | 41.2 (39.1–42.0) | 1.03× slower |
| 64B | 5.6 (5.5–5.7) | 6.4 (6.2–6.4) | 43.5 (42.9–43.6) | **1.15× faster** |
| 256B | 5.8 (5.5–6.5) | 8.9 (8.9–9.0) | 37.8 (37.3–40.6) | **1.55× faster** |
| 1024B | 6.0 (5.7–6.0) | 19.6 (16.9–20.4) | 42.0 (40.9–42.3) | **3.30× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.7–6.1) | 5.8 (5.7–5.8) | 40.9 (40.0–42.1) | 1.03× slower |
| 64B | 5.8 (5.8–5.9) | 6.0 (5.8–6.0) | 42.4 (42.1–44.3) | **1.03× faster** |
| 256B | 5.9 (5.7–6.2) | 9.3 (9.1–9.5) | 40.6 (39.7–43.0) | **1.57× faster** |
| 1024B | 6.4 (6.3–6.4) | 19.9 (18.8–20.5) | 67.1 (66.2–68.2) | **3.11× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative direct 64B 3.320 ns of 6.934 (47.9 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 3.466 ns of 19.629 (17.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 256B 3.320 ns of 37.842 (8.8 %)
