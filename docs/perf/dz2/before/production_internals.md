# bench:table (paired): 3 process(es) × 200 rounds; commit 3782939c, features `production internals`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.2–7.3) | 4.7 (4.7–4.7) | 44.6 (43.5–44.8) | 1.53× slower |
| 64B | 7.0 (6.9–7.1) | 6.3 (6.3–6.5) | 45.6 (45.4–45.9) | 1.10× slower |
| 256B | 7.8 (7.6–7.9) | 7.9 (7.8–8.1) | 40.1 (39.5–40.9) | **1.01× faster** |
| 1024B | 10.1 (9.9–10.2) | 16.7 (16.6–16.8) | 260.2 (251.9–267.5) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 5.8 (5.7–5.8) | 41.3 (40.4–41.3) | **1.02× faster** |
| 64B | 5.6 (5.6–5.6) | 6.3 (6.2–6.3) | 43.2 (43.1–44.6) | **1.12× faster** |
| 256B | 5.7 (5.6–5.7) | 9.2 (8.9–10.2) | 37.7 (37.5–38.3) | **1.62× faster** |
| 1024B | 6.0 (5.9–6.9) | 17.0 (16.7–17.8) | 41.9 (40.7–45.4) | **2.86× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.4) | 5.8 (5.7–5.8) | 40.9 (40.4–41.9) | 1.02× slower |
| 64B | 5.8 (5.7–6.2) | 5.9 (5.9–5.9) | 42.4 (41.7–42.7) | **1.01× faster** |
| 256B | 6.1 (6.0–6.1) | 9.1 (8.1–9.4) | 41.0 (40.9–41.3) | **1.51× faster** |
| 1024B | 6.3 (6.3–6.5) | 19.4 (19.0–19.5) | 68.6 (68.1–71.6) | **3.05× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.075 ns of 5.957 (18.0 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn_write 256B 1.270 ns of 9.131 (13.9 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 4.687 ns of 41.943 (11.2 %)
