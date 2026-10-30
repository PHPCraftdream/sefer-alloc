# bench:table (paired): 3 process(es) × 200 rounds; commit 20395f5e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.2–7.5) | 4.8 (4.7–4.8) | 44.2 (43.7–44.4) | 1.51× slower |
| 64B | 7.1 (6.8–9.6) | 6.4 (6.3–6.9) | 46.0 (45.3–47.0) | 1.10× slower |
| 256B | 7.9 (7.8–8.1) | 8.0 (7.8–8.3) | 41.1 (40.7–42.3) | **1.01× faster** |
| 1024B | 10.2 (10.0–10.3) | 16.9 (16.7–16.9) | 261.3 (252.0–263.5) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 5.8 (5.6–5.9) | 41.1 (41.0–41.5) | **1.02× faster** |
| 64B | 5.7 (5.6–5.7) | 6.5 (6.4–6.6) | 44.0 (43.5–44.8) | **1.14× faster** |
| 256B | 5.8 (5.7–5.9) | 11.6 (9.1–12.3) | 38.8 (38.4–39.3) | **2.01× faster** |
| 1024B | 6.3 (6.0–6.3) | 19.9 (15.9–19.9) | 47.8 (40.2–49.4) | **3.18× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–6.2) | 5.8 (5.8–5.9) | 40.9 (40.6–41.5) | 1.00× (tie) |
| 64B | 6.0 (5.9–6.0) | 6.0 (6.0–6.1) | 43.8 (43.8–44.2) | 1.00× (tie) |
| 256B | 6.7 (6.0–6.8) | 9.2 (9.0–9.6) | 41.1 (40.8–45.8) | **1.38× faster** |
| 1024B | 6.5 (6.2–6.5) | 19.8 (19.7–20.6) | 72.2 (67.5–74.0) | **3.05× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 2.783 ns of 7.080 (39.3 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 3.223 ns of 11.572 (27.9 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 9.179 ns of 47.754 (19.2 %)
