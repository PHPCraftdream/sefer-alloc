# bench:table (paired): 3 process(es) × 200 rounds; commit 97c18b36, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.4–6.5) | 4.7 (4.6–4.7) | 46.1 (45.2–46.3) | 1.39× slower |
| 64B | 6.0 (6.0–6.3) | 6.3 (6.2–6.4) | 46.1 (45.2–46.4) | **1.07× faster** |
| 256B | 6.8 (6.8–6.9) | 8.0 (7.9–8.0) | 40.5 (40.3–41.2) | **1.17× faster** |
| 1024B | 9.2 (9.1–9.2) | 16.7 (16.2–16.9) | 262.2 (261.6–263.9) | **1.82× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–5.5) | 5.7 (5.7–5.9) | 41.0 (41.0–42.4) | **1.04× faster** |
| 64B | 5.4 (5.4–6.2) | 6.3 (6.2–6.3) | 43.2 (42.7–44.6) | **1.16× faster** |
| 256B | 5.5 (5.5–5.5) | 8.9 (8.3–11.6) | 37.6 (37.3–42.0) | **1.63× faster** |
| 1024B | 5.7 (5.7–5.7) | 19.8 (15.7–19.9) | 43.6 (42.8–45.5) | **3.50× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 5.8 (5.7–5.8) | 41.1 (41.0–45.4) | **1.01× faster** |
| 64B | 5.8 (5.7–6.1) | 5.9 (5.8–6.0) | 43.8 (43.1–44.7) | **1.02× faster** |
| 256B | 5.7 (5.7–6.4) | 8.2 (8.0–9.4) | 41.1 (40.3–42.0) | **1.44× faster** |
| 1024B | 6.2 (6.1–6.3) | 19.8 (18.6–20.0) | 65.4 (49.5–65.6) | **3.19× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn 64B 0.781 ns of 5.371 (14.5 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 3.320 ns of 8.936 (37.2 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 16.064 ns of 65.430 (24.6 %)
