# bench:table (paired): 3 process(es) × 200 rounds; commit d258bcd5, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (6.0–6.3) | 4.7 (4.6–5.0) | 44.9 (44.8–46.1) | 1.29× slower |
| 64B | 8.0 (6.7–11.8) | 6.4 (6.3–6.5) | 45.5 (45.5–46.3) | 1.24× slower |
| 256B | 6.4 (6.3–6.7) | 8.2 (8.1–8.2) | 57.7 (46.2–61.9) | **1.27× faster** |
| 1024B | 9.6 (8.8–9.7) | 16.7 (16.3–18.6) | 60.6 (46.8–92.5) | **1.75× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.1–5.4) | 6.1 (5.8–6.3) | 42.6 (42.1–43.3) | **1.18× faster** |
| 64B | 5.1 (4.9–5.1) | 7.1 (6.4–7.4) | 44.1 (44.0–44.1) | **1.39× faster** |
| 256B | 5.1 (4.9–5.6) | 11.4 (9.2–12.2) | 40.0 (38.8–40.1) | **2.24× faster** |
| 1024B | 6.0 (5.3–6.1) | 19.4 (18.8–20.7) | 49.3 (47.1–50.3) | **3.26× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (5.4–5.8) | 5.9 (5.7–6.1) | 41.6 (41.4–42.3) | **1.09× faster** |
| 64B | 5.3 (5.2–5.4) | 6.3 (6.0–6.3) | 44.9 (43.9–46.6) | **1.17× faster** |
| 256B | 5.6 (5.3–6.3) | 9.6 (9.2–9.8) | 43.7 (41.5–45.7) | **1.72× faster** |
| 1024B | 6.0 (5.8–6.6) | 20.9 (20.7–21.1) | 73.6 (71.7–75.8) | **3.47× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 2/12 cells within 0.3 ns; widest relative direct 64B 5.127 ns of 8.008 (64.0 %)
- mimalloc: 2/12 cells within 0.3 ns; widest relative churn 256B 2.978 ns of 11.377 (26.2 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 45.703 ns of 60.596 (75.4 %)
