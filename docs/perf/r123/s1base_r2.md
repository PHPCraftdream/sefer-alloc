# bench:table (paired): 3 process(es) × 200 rounds; commit 5fd6b277, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.4 (6.3–6.6) | 4.7 (4.6–4.8) | 43.6 (42.6–48.0) | 1.35× slower |
| 64B | 6.1 (6.1–6.1) | 6.4 (6.1–6.5) | 45.4 (44.1–47.5) | **1.05× faster** |
| 256B | 6.7 (6.5–7.1) | 7.9 (7.6–8.1) | 40.6 (39.3–41.1) | **1.17× faster** |
| 1024B | 9.2 (8.6–9.3) | 16.6 (15.9–16.7) | 260.9 (252.3–263.8) | **1.81× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.3–5.5) | 5.7 (5.5–6.0) | 39.9 (39.1–43.2) | **1.04× faster** |
| 64B | 5.4 (5.2–5.5) | 6.4 (5.9–6.6) | 43.6 (42.0–45.3) | **1.19× faster** |
| 256B | 5.4 (5.2–5.6) | 9.4 (9.0–9.6) | 38.4 (36.8–41.4) | **1.73× faster** |
| 1024B | 5.6 (5.4–5.9) | 17.4 (17.0–18.1) | 41.5 (40.6–44.3) | **3.10× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.5–6.0) | 5.8 (5.5–6.0) | 40.9 (39.1–43.3) | **1.02× faster** |
| 64B | 6.3 (5.6–6.7) | 6.0 (5.6–6.1) | 43.1 (42.0–45.6) | 1.06× slower |
| 256B | 6.0 (5.6–6.3) | 8.7 (8.2–9.0) | 41.9 (40.1–42.4) | **1.46× faster** |
| 1024B | 6.2 (5.9–6.7) | 19.5 (19.5–20.0) | 67.0 (50.2–69.0) | **3.17× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 4/12 cells within 0.3 ns; widest relative churn_write 64B 1.074 ns of 6.299 (17.1 %)
- mimalloc: 1/12 cells within 0.3 ns; widest relative churn 64B 0.684 ns of 6.396 (10.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 18.750 ns of 67.041 (28.0 %)
