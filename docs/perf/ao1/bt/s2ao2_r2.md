# bench:table (paired): 3 process(es) × 200 rounds; commit e551ff85, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.6–5.9) | 4.4 (4.4–4.5) | 42.3 (42.1–42.7) | 1.31× slower |
| 64B | 5.0 (5.0–5.2) | 6.1 (5.9–6.1) | 43.8 (43.7–44.1) | **1.20× faster** |
| 256B | 5.8 (5.7–5.9) | 7.4 (7.3–7.6) | 38.8 (38.0–44.0) | **1.27× faster** |
| 1024B | 8.1 (8.0–8.1) | 15.5 (15.3–15.6) | 233.2 (219.4–284.6) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–5.2) | 5.4 (5.3–5.4) | 38.2 (37.1–38.9) | **1.13× faster** |
| 64B | 4.7 (4.6–4.8) | 6.0 (5.9–6.0) | 40.6 (40.5–40.8) | **1.26× faster** |
| 256B | 4.7 (4.7–4.7) | 8.3 (8.1–8.3) | 39.2 (38.6–42.4) | **1.76× faster** |
| 1024B | 4.8 (4.8–4.8) | 16.3 (16.0–18.6) | 39.7 (38.0–39.7) | **3.36× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–5.6) | 5.3 (5.3–5.4) | 38.6 (37.8–39.2) | **1.09× faster** |
| 64B | 4.9 (4.8–5.2) | 5.5 (5.5–5.5) | 39.9 (39.9–40.3) | **1.13× faster** |
| 256B | 4.9 (4.9–5.0) | 8.4 (8.3–8.6) | 38.8 (38.7–40.7) | **1.71× faster** |
| 1024B | 5.3 (5.2–5.3) | 18.6 (18.5–18.6) | 45.1 (43.1–45.7) | **3.52× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 16B 0.781 ns of 4.883 (16.0 %)
- mimalloc: 10/12 cells within 0.3 ns; widest relative churn 1024B 2.637 ns of 16.260 (16.2 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 1024B 65.136 ns of 233.203 (27.9 %)
