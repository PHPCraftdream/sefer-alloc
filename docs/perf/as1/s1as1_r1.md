# bench:table (paired): 3 process(es) × 200 rounds; commit 97c18b36, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.5–6.6) | 4.8 (4.7–4.8) | 44.7 (44.3–44.9) | 1.37× slower |
| 64B | 6.0 (6.0–6.4) | 6.3 (6.3–6.5) | 45.7 (45.1–45.8) | **1.06× faster** |
| 256B | 6.9 (6.7–7.0) | 7.9 (7.8–8.0) | 40.5 (40.4–42.1) | **1.14× faster** |
| 1024B | 9.4 (9.0–9.4) | 16.6 (16.3–16.7) | 259.8 (71.6–259.9) | **1.77× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.6) | 5.8 (5.6–5.9) | 41.6 (40.5–41.9) | **1.05× faster** |
| 64B | 5.5 (5.4–5.5) | 6.2 (5.9–6.3) | 43.6 (43.2–45.2) | **1.13× faster** |
| 256B | 5.5 (5.5–5.6) | 9.0 (8.8–9.1) | 40.4 (36.3–41.6) | **1.63× faster** |
| 1024B | 5.7 (5.6–6.0) | 17.1 (15.6–20.2) | 43.0 (41.2–47.8) | **3.02× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 5.7 (5.7–5.7) | 41.1 (40.9–41.4) | 1.00× (tie) |
| 64B | 5.7 (5.6–5.7) | 5.8 (5.8–5.9) | 43.5 (42.1–43.6) | **1.02× faster** |
| 256B | 6.0 (5.8–6.4) | 8.2 (8.2–8.5) | 42.4 (40.2–43.7) | **1.38× faster** |
| 1024B | 6.3 (6.0–6.6) | 19.1 (18.7–20.0) | 65.4 (49.2–67.4) | **3.04× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 1024B 0.684 ns of 6.299 (10.9 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 4.541 ns of 17.090 (26.6 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 188.281 ns of 259.814 (72.5 %)
