# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.1–7.1) | 4.6 (4.6–4.7) | 45.5 (45.3–46.0) | 1.54× slower |
| 64B | 6.8 (6.6–7.2) | 6.4 (6.3–6.4) | 45.9 (45.3–46.7) | 1.06× slower |
| 256B | 7.8 (7.6–7.8) | 8.0 (7.9–8.0) | 40.4 (40.1–40.4) | **1.03× faster** |
| 1024B | 10.1 (9.7–10.1) | 16.6 (16.5–16.7) | 268.5 (234.4–271.2) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 5.9 (5.6–6.1) | 41.7 (41.5–42.7) | **1.03× faster** |
| 64B | 5.6 (5.6–5.7) | 6.3 (6.3–6.5) | 44.3 (44.0–44.5) | **1.12× faster** |
| 256B | 5.7 (5.7–5.8) | 9.6 (8.5–10.6) | 38.9 (36.9–39.4) | **1.68× faster** |
| 1024B | 6.0 (5.7–6.0) | 17.7 (17.3–19.9) | 41.9 (41.4–43.6) | **2.98× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–5.9) | 5.8 (5.8–5.9) | 42.4 (41.7–42.7) | 1.01× slower |
| 64B | 5.8 (5.7–6.9) | 6.1 (5.9–6.1) | 43.9 (42.3–45.4) | **1.05× faster** |
| 256B | 6.5 (6.2–6.5) | 9.3 (9.2–9.4) | 41.2 (40.6–42.3) | **1.43× faster** |
| 1024B | 6.5 (6.3–7.4) | 19.8 (19.1–20.1) | 70.9 (67.8–71.0) | **3.05× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 64B 1.221 ns of 5.762 (21.2 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 2.100 ns of 9.570 (21.9 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 36.767 ns of 268.457 (13.7 %)
