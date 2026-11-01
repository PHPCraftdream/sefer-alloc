# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–5.9) | 4.5 (4.4–4.5) | 42.3 (42.3–43.4) | 1.30× slower |
| 64B | 6.0 (5.7–6.3) | 6.1 (6.0–6.2) | 44.0 (43.7–44.6) | **1.02× faster** |
| 256B | 6.2 (6.2–6.4) | 7.6 (7.6–7.8) | 42.6 (38.6–44.2) | **1.23× faster** |
| 1024B | 8.5 (8.5–8.5) | 15.8 (15.7–16.1) | 265.4 (242.4–298.3) | **1.85× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–4.9) | 5.5 (5.4–5.5) | 38.7 (37.0–39.4) | **1.11× faster** |
| 64B | 4.7 (4.7–4.7) | 5.8 (5.8–6.0) | 41.0 (40.7–41.4) | **1.23× faster** |
| 256B | 4.8 (4.7–4.9) | 8.7 (8.6–8.7) | 41.7 (38.9–42.7) | **1.82× faster** |
| 1024B | 4.9 (4.9–5.1) | 16.6 (15.8–17.0) | 40.6 (39.8–41.1) | **3.38× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.2) | 5.5 (5.4–5.6) | 38.6 (38.5–39.7) | **1.11× faster** |
| 64B | 5.0 (4.9–5.0) | 5.7 (5.6–5.7) | 41.1 (39.5–42.0) | **1.14× faster** |
| 256B | 5.1 (5.0–5.3) | 8.1 (7.8–8.2) | 41.0 (39.3–42.0) | **1.59× faster** |
| 1024B | 5.5 (5.3–5.5) | 18.8 (18.0–18.8) | 47.9 (45.2–49.5) | **3.43× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative direct 64B 0.586 ns of 5.957 (9.8 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.221 ns of 16.650 (7.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 55.908 ns of 265.430 (21.1 %)
