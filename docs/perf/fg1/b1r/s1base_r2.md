# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.1 (7.0–7.1) | 4.7 (4.7–4.8) | 44.2 (43.9–44.3) | 1.51× slower |
| 64B | 6.9 (6.7–7.0) | 6.4 (6.3–6.5) | 45.8 (45.0–46.0) | 1.08× slower |
| 256B | 7.7 (7.6–7.7) | 8.0 (7.9–8.1) | 40.5 (40.0–40.9) | **1.04× faster** |
| 1024B | 10.0 (9.9–10.0) | 16.7 (16.3–16.8) | 258.9 (253.8–266.9) | **1.67× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.7) | 5.8 (5.7–5.8) | 40.5 (38.6–40.7) | **1.04× faster** |
| 64B | 5.5 (5.5–5.6) | 6.3 (6.3–6.4) | 42.7 (41.3–43.1) | **1.15× faster** |
| 256B | 5.6 (5.5–6.7) | 9.0 (8.9–9.2) | 41.0 (37.4–41.2) | **1.60× faster** |
| 1024B | 5.8 (5.7–5.9) | 17.3 (16.0–18.4) | 43.1 (40.4–47.2) | **2.98× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–5.9) | 5.7 (5.6–5.7) | 40.7 (40.5–41.2) | 1.03× slower |
| 64B | 5.7 (5.7–5.8) | 5.9 (5.8–5.9) | 42.2 (42.0–42.5) | **1.03× faster** |
| 256B | 6.0 (5.9–6.3) | 8.3 (8.1–9.0) | 41.4 (39.2–41.5) | **1.39× faster** |
| 1024B | 6.3 (6.3–6.8) | 19.5 (19.2–19.9) | 49.2 (45.2–72.8) | **3.07× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 256B 1.171 ns of 5.615 (20.9 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 2.392 ns of 17.334 (13.8 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 27.588 ns of 49.170 (56.1 %)
