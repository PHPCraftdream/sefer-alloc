# bench:table (paired): 3 process(es) × 200 rounds; commit 5fd6b277, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.4 (6.3–6.4) | 4.6 (4.6–4.7) | 44.0 (42.8–46.9) | 1.39× slower |
| 64B | 6.0 (6.0–6.3) | 6.2 (6.1–6.3) | 45.2 (44.0–46.5) | **1.03× faster** |
| 256B | 6.8 (6.7–6.9) | 7.8 (7.5–7.8) | 41.5 (38.5–41.7) | **1.14× faster** |
| 1024B | 9.0 (8.9–9.1) | 16.3 (16.0–16.7) | 261.4 (258.0–261.7) | **1.81× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (5.4–5.4) | 5.5 (5.4–5.5) | 40.1 (39.6–40.3) | **1.02× faster** |
| 64B | 5.3 (5.3–5.4) | 6.0 (5.9–6.1) | 43.0 (41.2–44.8) | **1.14× faster** |
| 256B | 5.4 (5.3–5.5) | 9.0 (8.9–9.8) | 36.2 (36.0–38.3) | **1.68× faster** |
| 1024B | 5.6 (5.5–5.6) | 16.4 (16.0–19.4) | 42.0 (41.2–44.1) | **2.94× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.5) | 5.6 (5.6–5.7) | 39.9 (39.0–40.0) | **1.03× faster** |
| 64B | 5.6 (5.4–5.8) | 5.7 (5.7–5.9) | 41.8 (41.3–46.9) | **1.03× faster** |
| 256B | 5.7 (5.6–5.7) | 8.6 (8.5–9.0) | 40.9 (39.9–41.5) | **1.53× faster** |
| 1024B | 5.9 (5.9–6.0) | 19.8 (18.9–20.1) | 66.6 (63.4–66.8) | **3.36× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn_write 64B 0.391 ns of 5.566 (7.0 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 3.467 ns of 16.357 (21.2 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 64B 5.615 ns of 41.797 (13.4 %)
