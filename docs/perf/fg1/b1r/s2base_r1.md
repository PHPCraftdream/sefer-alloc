# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.0–7.2) | 4.7 (4.6–4.8) | 44.8 (44.5–46.4) | 1.51× slower |
| 64B | 6.8 (6.8–6.8) | 6.4 (6.3–6.4) | 46.2 (45.8–46.4) | 1.06× slower |
| 256B | 7.7 (7.6–7.7) | 8.0 (7.9–8.0) | 41.7 (41.2–42.2) | **1.04× faster** |
| 1024B | 10.0 (9.9–10.0) | 16.9 (16.7–16.9) | 259.8 (249.6–260.6) | **1.70× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.8) | 5.6 (5.6–5.8) | 40.6 (40.2–41.6) | 1.00× (tie) |
| 64B | 5.7 (5.6–6.1) | 6.3 (6.2–6.3) | 43.3 (42.9–43.6) | **1.11× faster** |
| 256B | 5.7 (5.6–5.7) | 9.1 (8.7–11.1) | 38.3 (37.6–38.6) | **1.60× faster** |
| 1024B | 5.8 (5.8–5.9) | 16.7 (16.4–18.0) | 43.4 (41.8–45.0) | **2.88× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.8) | 5.7 (5.7–5.7) | 41.1 (40.8–41.8) | 1.02× slower |
| 64B | 5.7 (5.7–6.0) | 5.9 (5.9–5.9) | 43.1 (42.9–43.4) | **1.03× faster** |
| 256B | 6.0 (5.9–6.0) | 8.9 (8.3–9.3) | 41.9 (41.0–42.3) | **1.49× faster** |
| 1024B | 6.3 (6.3–6.4) | 19.3 (18.8–19.7) | 66.2 (65.2–71.7) | **3.05× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn 64B 0.489 ns of 5.664 (8.6 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 2.393 ns of 9.082 (26.3 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 6.543 ns of 66.162 (9.9 %)
