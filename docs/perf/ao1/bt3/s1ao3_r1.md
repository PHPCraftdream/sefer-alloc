# bench:table (paired): 3 process(es) × 200 rounds; commit 42a0439b, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.5 (4.4–4.5) | 42.7 (41.7–42.8) | 1.24× slower |
| 64B | 5.1 (5.1–5.2) | 6.1 (5.9–6.1) | 43.4 (43.4–44.1) | **1.19× faster** |
| 256B | 6.0 (5.9–6.0) | 7.6 (7.3–7.6) | 52.5 (43.7–55.5) | **1.28× faster** |
| 1024B | 8.1 (8.0–8.2) | 15.3 (15.2–15.4) | 45.3 (43.9–79.6) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.7) | 5.4 (5.4–5.4) | 38.6 (37.9–38.8) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 5.8 (5.7–5.9) | 41.1 (39.6–41.6) | **1.25× faster** |
| 256B | 4.7 (4.6–4.7) | 8.5 (8.2–8.6) | 37.6 (35.4–38.4) | **1.82× faster** |
| 1024B | 4.9 (4.8–5.5) | 16.8 (16.4–17.1) | 40.4 (40.1–41.6) | **3.45× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.1) | 5.4 (5.4–5.4) | 38.8 (36.9–38.9) | **1.11× faster** |
| 64B | 4.9 (4.8–5.1) | 5.5 (5.5–5.5) | 39.8 (39.4–41.5) | **1.12× faster** |
| 256B | 5.0 (4.9–5.0) | 8.5 (8.2–8.7) | 39.8 (38.0–40.6) | **1.71× faster** |
| 1024B | 5.3 (5.3–5.4) | 18.3 (18.0–18.4) | 61.4 (61.3–65.3) | **3.43× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn 1024B 0.684 ns of 4.883 (14.0 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn_write 256B 0.537 ns of 8.496 (6.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 35.694 ns of 45.312 (78.8 %)
