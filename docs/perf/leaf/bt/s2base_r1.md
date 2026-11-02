# bench:table (paired): 3 process(es) × 200 rounds; commit b2796882, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.5 (4.4–4.5) | 42.3 (42.3–42.8) | 1.25× slower |
| 64B | 5.5 (5.5–5.5) | 6.0 (5.9–6.1) | 43.9 (43.6–44.0) | **1.09× faster** |
| 256B | 5.8 (5.8–5.8) | 7.4 (7.3–7.5) | 38.8 (38.2–39.8) | **1.29× faster** |
| 1024B | 8.4 (8.4–8.6) | 15.4 (15.4–15.7) | 230.1 (219.3–246.1) | **1.83× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.8) | 5.4 (5.4–5.5) | 37.7 (37.3–38.8) | **1.15× faster** |
| 64B | 4.7 (4.6–4.7) | 6.0 (6.0–6.1) | 41.3 (41.1–43.4) | **1.26× faster** |
| 256B | 4.7 (4.7–4.8) | 8.5 (8.3–8.7) | 39.2 (39.1–40.8) | **1.79× faster** |
| 1024B | 5.3 (4.9–5.3) | 18.7 (16.1–18.9) | 41.4 (39.8–41.6) | **3.54× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (5.0–5.1) | 5.4 (5.3–5.5) | 38.3 (37.2–38.8) | **1.08× faster** |
| 64B | 4.8 (4.8–5.5) | 5.6 (5.6–5.7) | 41.1 (39.6–43.9) | **1.16× faster** |
| 256B | 5.0 (4.9–5.2) | 8.7 (8.6–8.8) | 39.4 (39.3–40.9) | **1.76× faster** |
| 1024B | 5.4 (5.3–5.8) | 18.6 (18.4–18.8) | 49.4 (46.4–50.2) | **3.45× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 64B 0.635 ns of 4.834 (13.1 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 1024B 2.783 ns of 18.652 (14.9 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 26.807 ns of 230.078 (11.7 %)
