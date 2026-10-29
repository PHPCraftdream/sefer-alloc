# bench:table (paired): 3 process(es) × 200 rounds; commit 0ee09c13, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.4 (7.1–7.5) | 4.7 (4.6–4.7) | 44.4 (43.8–44.6) | 1.58× slower |
| 64B | 6.9 (6.7–7.1) | 6.3 (6.2–6.3) | 45.8 (45.7–46.2) | 1.10× slower |
| 256B | 7.7 (7.6–7.8) | 7.8 (7.7–8.0) | 40.8 (39.6–41.6) | **1.02× faster** |
| 1024B | 9.9 (9.9–10.0) | 16.5 (15.8–16.6) | 263.1 (256.8–268.3) | **1.66× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 5.7 (5.6–5.8) | 40.1 (38.9–41.7) | **1.02× faster** |
| 64B | 5.5 (5.4–5.6) | 6.2 (6.0–6.2) | 43.0 (42.0–43.1) | **1.11× faster** |
| 256B | 5.6 (5.5–5.6) | 8.9 (8.8–9.3) | 38.5 (37.0–38.7) | **1.60× faster** |
| 1024B | 5.8 (5.8–6.1) | 16.2 (15.9–16.3) | 43.2 (42.9–43.9) | **2.80× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.6–5.8) | 5.6 (5.5–5.6) | 41.7 (40.4–42.3) | 1.03× slower |
| 64B | 5.7 (5.7–5.7) | 5.8 (5.7–5.9) | 43.1 (42.5–43.8) | **1.02× faster** |
| 256B | 5.9 (5.8–6.0) | 8.2 (7.9–8.5) | 41.6 (39.6–42.7) | **1.39× faster** |
| 1024B | 6.2 (6.1–6.3) | 19.2 (19.1–19.5) | 66.3 (63.4–71.3) | **3.12× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 1024B 0.342 ns of 5.762 (5.9 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 0.635 ns of 8.203 (7.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 7.959 ns of 66.260 (12.0 %)
