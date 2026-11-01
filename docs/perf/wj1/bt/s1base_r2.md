# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.9–6.1) | 4.6 (4.5–4.6) | 43.3 (43.0–43.7) | 1.29× slower |
| 64B | 5.9 (5.6–6.0) | 6.3 (6.2–6.4) | 44.6 (44.6–44.9) | **1.08× faster** |
| 256B | 6.5 (6.2–6.6) | 7.9 (7.7–7.9) | 44.5 (39.8–59.0) | **1.20× faster** |
| 1024B | 8.7 (8.7–8.7) | 16.7 (16.0–16.8) | 307.6 (252.3–323.9) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.4) | 5.7 (5.6–5.8) | 40.2 (39.5–40.5) | **1.14× faster** |
| 64B | 4.8 (4.8–4.9) | 6.2 (6.2–6.3) | 42.4 (41.8–43.3) | **1.30× faster** |
| 256B | 5.4 (4.9–5.6) | 8.5 (8.4–9.1) | 42.5 (40.9–44.4) | **1.58× faster** |
| 1024B | 5.1 (5.0–5.1) | 16.6 (15.5–18.1) | 41.0 (40.1–41.4) | **3.26× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (4.9–5.1) | 5.6 (5.5–5.7) | 39.8 (39.5–40.8) | **1.11× faster** |
| 64B | 5.2 (5.1–5.2) | 5.7 (5.7–5.8) | 41.6 (41.3–42.9) | **1.10× faster** |
| 256B | 5.1 (5.0–5.9) | 7.9 (7.6–8.8) | 41.1 (40.1–42.3) | **1.53× faster** |
| 1024B | 5.5 (5.4–6.2) | 19.0 (18.6–19.3) | 48.1 (45.7–50.1) | **3.44× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn_write 256B 0.830 ns of 5.127 (16.2 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 1.270 ns of 7.861 (16.2 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 19.189 ns of 44.531 (43.1 %)
