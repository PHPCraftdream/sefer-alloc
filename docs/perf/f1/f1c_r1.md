# bench:table (paired): 3 process(es) × 200 rounds; commit 2b225d83, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.4 (7.1–7.5) | 4.8 (4.7–4.8) | 45.7 (43.8–47.4) | 1.54× slower |
| 64B | 6.8 (6.6–6.9) | 6.4 (6.4–6.4) | 46.4 (45.3–47.2) | 1.07× slower |
| 256B | 7.9 (7.9–8.1) | 8.0 (7.8–8.2) | 41.4 (40.0–42.4) | **1.01× faster** |
| 1024B | 10.6 (10.5–11.0) | 17.0 (16.9–17.6) | 263.5 (256.7–269.0) | **1.60× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–6.1) | 6.2 (6.0–6.3) | 41.5 (40.8–43.1) | **1.05× faster** |
| 64B | 5.8 (5.8–5.8) | 7.0 (6.9–7.5) | 45.3 (45.1–45.6) | **1.20× faster** |
| 256B | 6.0 (5.9–6.0) | 9.4 (8.9–9.6) | 38.6 (37.8–39.2) | **1.57× faster** |
| 1024B | 6.0 (6.0–6.1) | 17.2 (17.0–18.0) | 42.7 (41.2–43.2) | **2.89× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–6.1) | 5.9 (5.9–6.0) | 42.1 (40.5–43.1) | 1.00× (tie) |
| 64B | 6.0 (6.0–6.1) | 6.1 (6.1–6.1) | 44.5 (43.3–45.3) | **1.02× faster** |
| 256B | 6.1 (6.1–6.8) | 9.8 (9.8–9.8) | 44.2 (42.3–44.3) | **1.60× faster** |
| 1024B | 6.8 (6.5–6.9) | 20.2 (19.8–20.6) | 69.3 (64.6–71.5) | **2.96× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 256B 0.781 ns of 6.104 (12.8 %)
- mimalloc: 5/12 cells within 0.3 ns; widest relative churn 64B 0.586 ns of 6.982 (8.4 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 6.933 ns of 69.336 (10.0 %)
