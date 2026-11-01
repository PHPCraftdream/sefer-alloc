# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–6.1) | 4.5 (4.5–4.5) | 42.7 (42.5–44.2) | 1.27× slower |
| 64B | 5.9 (5.6–6.0) | 6.2 (6.0–6.2) | 44.3 (43.9–46.0) | **1.04× faster** |
| 256B | 6.3 (6.2–6.6) | 7.6 (7.6–7.7) | 42.7 (39.0–43.2) | **1.22× faster** |
| 1024B | 8.5 (8.5–8.6) | 15.9 (15.8–15.9) | 306.9 (224.8–312.3) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.8) | 5.5 (5.4–6.0) | 39.2 (39.0–40.8) | **1.15× faster** |
| 64B | 4.7 (4.7–4.8) | 6.0 (5.9–6.1) | 42.0 (41.0–43.8) | **1.26× faster** |
| 256B | 4.8 (4.8–4.9) | 8.5 (8.2–9.7) | 43.1 (39.2–46.1) | **1.79× faster** |
| 1024B | 5.0 (5.0–5.0) | 16.3 (15.3–16.6) | 39.6 (39.1–42.2) | **3.27× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–9.9) | 5.5 (5.4–6.0) | 39.4 (39.0–40.3) | **1.11× faster** |
| 64B | 5.0 (4.9–5.0) | 5.6 (5.6–5.7) | 42.2 (40.8–43.9) | **1.13× faster** |
| 256B | 5.0 (5.0–5.1) | 7.8 (7.6–8.3) | 42.4 (39.3–45.3) | **1.54× faster** |
| 1024B | 5.4 (5.3–5.4) | 18.8 (18.3–18.8) | 45.6 (45.3–46.4) | **3.50× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative churn_write 16B 4.931 ns of 4.932 (100.0 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 1.465 ns of 8.545 (17.1 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 87.451 ns of 306.934 (28.5 %)
