# bench:table (paired): 3 process(es) × 200 rounds; commit 5988bdcd+dirty, features `production bench-internals`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.5 (7.5–7.6) | 4.8 (4.8–4.8) | 45.5 (45.3–46.0) | 1.57× slower |
| 64B | 7.2 (7.1–11.5) | 6.6 (6.4–6.8) | 46.1 (46.0–46.2) | 1.09× slower |
| 256B | 8.1 (8.0–8.2) | 8.1 (8.0–8.2) | 41.2 (40.4–42.3) | **1.01× faster** |
| 1024B | 10.6 (10.6–10.6) | 17.2 (16.9–17.5) | 282.2 (269.6–286.3) | **1.62× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.8–6.0) | 5.9 (5.8–5.9) | 41.6 (41.1–42.0) | 1.01× slower |
| 64B | 5.6 (5.6–5.8) | 6.5 (6.3–6.7) | 44.6 (43.3–45.7) | **1.16× faster** |
| 256B | 5.9 (5.8–6.0) | 8.9 (8.9–12.1) | 38.9 (38.2–39.7) | **1.53× faster** |
| 1024B | 6.1 (6.1–6.4) | 20.1 (18.0–20.4) | 44.0 (42.3–47.2) | **3.29× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (6.0–6.1) | 5.9 (5.8–6.0) | 41.9 (40.6–43.1) | 1.02× slower |
| 64B | 6.1 (5.9–6.5) | 6.1 (6.0–6.1) | 43.7 (43.6–45.2) | 1.00× (tie) |
| 256B | 6.2 (6.2–6.3) | 9.4 (8.8–9.6) | 42.3 (41.1–43.3) | **1.51× faster** |
| 1024B | 7.0 (6.9–8.0) | 20.6 (20.3–20.6) | 72.9 (70.8–75.1) | **2.92× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 4.443 ns of 7.178 (61.9 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 3.125 ns of 8.936 (35.0 %)
- system: 1/12 cells within 0.3 ns; widest relative churn 1024B 4.932 ns of 43.994 (11.2 %)
