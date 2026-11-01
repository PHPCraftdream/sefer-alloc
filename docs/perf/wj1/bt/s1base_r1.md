# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.1) | 4.6 (4.6–4.6) | 42.8 (42.4–43.0) | 1.28× slower |
| 64B | 5.9 (5.6–6.3) | 6.2 (6.2–6.4) | 44.2 (43.9–44.4) | **1.06× faster** |
| 256B | 6.4 (6.2–6.5) | 7.7 (7.7–8.0) | 42.8 (38.4–57.1) | **1.20× faster** |
| 1024B | 8.6 (8.4–8.8) | 16.0 (15.9–16.1) | 292.7 (241.8–307.9) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.8) | 5.5 (5.5–5.6) | 39.2 (38.5–40.4) | **1.15× faster** |
| 64B | 4.7 (4.7–4.9) | 6.1 (6.0–6.3) | 42.4 (41.1–66.7) | **1.28× faster** |
| 256B | 4.8 (4.8–4.9) | 8.9 (8.7–11.1) | 41.9 (38.8–43.1) | **1.87× faster** |
| 1024B | 5.4 (5.3–6.0) | 16.7 (16.4–17.0) | 40.6 (40.2–43.0) | **3.12× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (4.9–6.0) | 5.5 (5.4–5.6) | 39.1 (38.7–39.6) | **1.06× faster** |
| 64B | 5.0 (4.9–5.1) | 5.6 (5.5–5.7) | 43.1 (41.4–67.2) | **1.12× faster** |
| 256B | 5.2 (5.1–5.4) | 8.1 (8.0–8.3) | 41.8 (39.8–42.0) | **1.57× faster** |
| 1024B | 5.5 (5.3–5.5) | 18.8 (18.6–19.0) | 47.2 (45.9–48.8) | **3.45× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn_write 16B 1.074 ns of 5.225 (20.6 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 2.344 ns of 8.936 (26.2 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 64B 25.586 ns of 42.383 (60.4 %)
