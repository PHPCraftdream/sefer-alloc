# bench:table (paired): 3 process(es) × 200 rounds; commit 20395f5e, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.1–7.3) | 4.7 (4.7–4.8) | 44.3 (44.2–45.7) | 1.54× slower |
| 64B | 7.0 (6.7–7.1) | 6.3 (6.3–6.5) | 45.7 (45.3–47.0) | 1.10× slower |
| 256B | 7.8 (7.5–7.9) | 8.0 (7.8–8.0) | 40.6 (40.5–42.0) | **1.02× faster** |
| 1024B | 10.3 (9.9–10.5) | 16.9 (16.7–17.3) | 254.6 (242.5–270.2) | **1.65× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.9) | 5.8 (5.7–6.0) | 41.7 (41.3–42.3) | **1.01× faster** |
| 64B | 5.6 (5.6–5.8) | 6.4 (6.3–6.7) | 43.8 (43.6–44.0) | **1.15× faster** |
| 256B | 5.9 (5.8–6.8) | 11.7 (10.2–12.1) | 42.2 (38.0–43.8) | **2.00× faster** |
| 1024B | 6.2 (6.0–6.2) | 20.5 (17.1–20.6) | 43.8 (42.3–45.5) | **3.33× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.9) | 5.8 (5.8–5.9) | 42.3 (40.3–42.3) | 1.01× slower |
| 64B | 6.0 (5.7–6.0) | 6.1 (5.7–6.1) | 44.3 (42.7–44.4) | **1.02× faster** |
| 256B | 6.0 (6.0–6.1) | 9.5 (8.9–9.5) | 43.3 (41.9–43.4) | **1.59× faster** |
| 1024B | 6.6 (6.3–6.6) | 20.1 (20.0–20.1) | 50.6 (49.5–75.8) | **3.02× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn 256B 1.074 ns of 5.859 (18.3 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 3.467 ns of 20.459 (16.9 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 26.367 ns of 50.635 (52.1 %)
