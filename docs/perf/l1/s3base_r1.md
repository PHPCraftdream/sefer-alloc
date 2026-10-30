# bench:table (paired): 3 process(es) × 200 rounds; commit 10b53942, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.4–6.6) | 4.7 (4.7–4.7) | 44.7 (43.8–46.9) | 1.38× slower |
| 64B | 6.2 (6.1–6.2) | 6.4 (6.4–6.5) | 46.3 (45.5–46.3) | **1.04× faster** |
| 256B | 7.0 (6.9–7.1) | 8.0 (7.9–8.1) | 41.2 (39.6–43.1) | **1.15× faster** |
| 1024B | 9.4 (9.3–9.4) | 16.8 (16.2–16.9) | 268.7 (263.2–269.1) | **1.79× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.7) | 5.8 (5.6–5.9) | 42.5 (41.0–43.2) | **1.04× faster** |
| 64B | 5.5 (5.4–5.5) | 6.3 (6.1–6.3) | 44.3 (42.7–44.4) | **1.15× faster** |
| 256B | 5.5 (5.5–6.0) | 9.0 (8.9–9.0) | 41.0 (37.7–41.9) | **1.63× faster** |
| 1024B | 5.7 (5.7–5.8) | 16.6 (16.6–16.8) | 43.1 (42.0–43.3) | **2.91× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.8) | 5.8 (5.6–5.8) | 41.5 (41.5–42.3) | 1.01× slower |
| 64B | 5.8 (5.7–6.3) | 5.9 (5.9–5.9) | 42.7 (42.1–44.9) | **1.02× faster** |
| 256B | 5.9 (5.8–6.4) | 8.2 (8.0–9.4) | 42.1 (41.3–43.9) | **1.40× faster** |
| 1024B | 6.3 (6.2–6.7) | 19.6 (18.9–20.5) | 69.3 (47.8–70.2) | **3.08× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 64B 0.635 ns of 5.811 (10.9 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn_write 256B 1.416 ns of 8.203 (17.3 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 22.412 ns of 69.287 (32.3 %)
