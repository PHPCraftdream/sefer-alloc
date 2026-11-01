# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.4 (6.2–6.4) | 4.7 (4.7–4.8) | 45.1 (44.8–45.6) | 1.37× slower |
| 64B | 6.0 (5.9–6.3) | 6.3 (6.3–6.9) | 46.1 (46.0–47.0) | **1.06× faster** |
| 256B | 6.8 (6.8–7.0) | 8.2 (7.6–8.3) | 41.6 (40.0–66.3) | **1.19× faster** |
| 1024B | 9.2 (9.2–9.6) | 17.1 (16.8–17.5) | 248.5 (246.7–318.1) | **1.85× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (4.9–5.2) | 5.9 (5.7–6.2) | 41.4 (39.0–44.0) | **1.15× faster** |
| 64B | 4.9 (4.8–5.0) | 6.3 (6.2–6.8) | 43.3 (41.1–43.9) | **1.30× faster** |
| 256B | 5.1 (4.8–5.1) | 8.6 (8.2–9.1) | 43.4 (40.5–47.1) | **1.70× faster** |
| 1024B | 5.1 (5.0–5.3) | 17.5 (15.9–20.3) | 43.6 (41.0–45.4) | **3.45× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.0–5.5) | 5.9 (5.5–5.9) | 41.4 (40.1–44.2) | **1.13× faster** |
| 64B | 5.2 (5.0–5.3) | 5.9 (5.7–6.0) | 44.3 (41.1–44.7) | **1.14× faster** |
| 256B | 5.3 (5.1–5.3) | 8.4 (7.3–8.6) | 41.0 (39.6–43.8) | **1.60× faster** |
| 1024B | 5.6 (5.5–5.9) | 19.3 (18.5–19.5) | 50.5 (47.8–56.8) | **3.43× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn_write 16B 0.489 ns of 5.176 (9.4 %)
- mimalloc: 2/12 cells within 0.3 ns; widest relative churn 1024B 4.443 ns of 17.529 (25.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 26.221 ns of 41.553 (63.1 %)
