# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (6.0–6.5) | 4.6 (4.5–4.7) | 44.4 (43.3–44.4) | 1.34× slower |
| 64B | 5.5 (5.5–6.0) | 6.0 (6.0–6.1) | 45.3 (44.6–45.5) | **1.10× faster** |
| 256B | 6.4 (6.3–6.5) | 7.8 (7.6–7.8) | 40.0 (39.7–59.4) | **1.21× faster** |
| 1024B | 8.8 (8.4–8.9) | 16.2 (15.9–16.5) | 244.9 (234.1–308.9) | **1.83× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.7 (5.6–5.8) | 39.8 (39.6–40.7) | **1.17× faster** |
| 64B | 4.7 (4.7–4.9) | 6.0 (6.0–6.1) | 43.2 (42.1–43.5) | **1.27× faster** |
| 256B | 4.9 (4.8–5.4) | 9.1 (8.5–9.3) | 40.8 (40.7–44.3) | **1.84× faster** |
| 1024B | 5.0 (5.0–5.2) | 16.8 (16.1–17.6) | 41.8 (40.3–42.7) | **3.35× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.2) | 5.6 (5.5–5.7) | 40.3 (39.8–40.9) | **1.11× faster** |
| 64B | 5.2 (5.0–5.3) | 5.7 (5.6–5.8) | 43.1 (41.7–43.8) | **1.10× faster** |
| 256B | 5.1 (5.1–5.3) | 8.3 (7.6–9.1) | 41.2 (41.1–42.2) | **1.61× faster** |
| 1024B | 5.4 (5.4–5.6) | 19.2 (19.1–19.4) | 48.5 (45.5–49.9) | **3.54× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn 256B 0.586 ns of 4.932 (11.9 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 1.465 ns of 8.252 (17.8 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 19.629 ns of 40.039 (49.0 %)
