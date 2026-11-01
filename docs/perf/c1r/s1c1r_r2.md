# bench:table (paired): 3 process(es) × 200 rounds; commit d9b05663, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 4.5 (4.5–4.6) | 42.8 (42.2–43.3) | 1.25× slower |
| 64B | 5.1 (5.1–5.5) | 6.2 (6.1–6.2) | 44.7 (44.3–44.9) | **1.20× faster** |
| 256B | 5.9 (5.8–5.9) | 7.7 (7.6–7.8) | 39.8 (39.3–39.9) | **1.30× faster** |
| 1024B | 8.3 (8.1–8.6) | 15.9 (15.8–16.1) | 252.9 (241.7–256.1) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–4.9) | 5.7 (5.6–5.7) | 38.3 (38.3–38.9) | **1.16× faster** |
| 64B | 4.8 (4.7–4.8) | 5.9 (5.8–6.0) | 41.6 (40.4–42.2) | **1.23× faster** |
| 256B | 4.8 (4.8–5.4) | 8.5 (8.4–8.5) | 39.5 (39.2–39.6) | **1.78× faster** |
| 1024B | 5.3 (4.9–5.6) | 15.4 (15.1–19.4) | 39.6 (39.0–40.7) | **2.89× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (5.0–5.5) | 5.7 (5.6–5.8) | 39.2 (38.8–40.0) | **1.08× faster** |
| 64B | 5.0 (4.9–5.1) | 5.6 (5.6–5.7) | 41.2 (40.8–41.5) | **1.13× faster** |
| 256B | 5.0 (5.0–5.1) | 8.3 (8.1–8.8) | 39.7 (39.3–40.8) | **1.66× faster** |
| 1024B | 5.3 (5.3–5.8) | 18.6 (17.8–18.6) | 45.2 (44.8–47.6) | **3.50× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 1024B 0.732 ns of 5.322 (13.8 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 4.297 ns of 15.381 (27.9 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 2.735 ns of 45.166 (6.1 %)
