# bench:table (paired): 3 process(es) × 200 rounds; commit 5988bdcd+dirty, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.3 (7.2–7.3) | 4.8 (4.8–4.8) | 45.9 (44.8–46.8) | 1.52× slower |
| 64B | 6.9 (6.7–7.0) | 6.4 (6.3–6.5) | 46.7 (45.5–46.7) | 1.08× slower |
| 256B | 7.8 (7.8–7.9) | 8.0 (7.8–8.2) | 40.6 (39.9–41.7) | **1.02× faster** |
| 1024B | 10.2 (10.1–10.2) | 17.0 (16.5–17.5) | 258.0 (250.7–259.6) | **1.68× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.7–5.9) | 5.8 (5.8–5.9) | 42.2 (41.9–43.7) | **1.01× faster** |
| 64B | 5.6 (5.5–5.7) | 6.4 (6.3–6.6) | 44.1 (43.4–45.6) | **1.14× faster** |
| 256B | 5.7 (5.7–5.8) | 9.2 (8.9–10.1) | 38.7 (38.2–38.7) | **1.61× faster** |
| 1024B | 5.8 (5.8–5.9) | 20.5 (20.1–20.6) | 41.6 (41.2–43.7) | **3.53× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.1) | 5.8 (5.8–5.8) | 42.2 (41.6–43.2) | 1.02× slower |
| 64B | 5.8 (5.8–5.8) | 6.0 (5.9–6.0) | 43.6 (42.2–44.0) | **1.03× faster** |
| 256B | 6.1 (6.0–6.3) | 9.2 (8.3–9.5) | 42.2 (41.5–42.3) | **1.52× faster** |
| 1024B | 6.3 (6.3–6.3) | 20.3 (20.1–20.7) | 72.1 (71.8–75.0) | **3.20× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn_write 16B 0.342 ns of 5.908 (5.8 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn_write 256B 1.269 ns of 9.229 (13.8 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 2.539 ns of 41.602 (6.1 %)
