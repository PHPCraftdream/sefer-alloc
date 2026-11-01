# bench:table (paired): 3 process(es) × 200 rounds; commit e551ff85, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.4 (4.4–4.4) | 42.4 (41.8–42.8) | 1.26× slower |
| 64B | 5.4 (5.3–5.8) | 6.0 (6.0–6.4) | 43.3 (43.0–44.1) | **1.10× faster** |
| 256B | 5.8 (5.7–5.9) | 7.4 (7.3–7.7) | 38.4 (38.0–55.9) | **1.28× faster** |
| 1024B | 8.1 (8.0–8.2) | 15.4 (15.3–15.6) | 239.2 (218.4–277.9) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–4.8) | 5.4 (5.4–5.4) | 38.3 (38.0–38.5) | **1.15× faster** |
| 64B | 4.6 (4.6–4.7) | 5.9 (5.9–6.0) | 41.1 (40.7–41.7) | **1.27× faster** |
| 256B | 4.7 (4.7–4.7) | 8.4 (7.9–8.8) | 39.1 (38.4–42.3) | **1.79× faster** |
| 1024B | 4.9 (4.9–5.0) | 15.9 (15.3–17.1) | 41.7 (38.6–42.5) | **3.25× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–4.9) | 5.4 (5.3–5.4) | 37.9 (37.9–38.7) | **1.09× faster** |
| 64B | 4.9 (4.8–4.9) | 5.6 (5.5–5.6) | 41.5 (40.1–41.6) | **1.14× faster** |
| 256B | 4.9 (4.9–5.1) | 8.7 (8.5–8.7) | 39.5 (38.5–42.3) | **1.76× faster** |
| 1024B | 5.3 (5.3–5.4) | 18.5 (18.4–18.5) | 44.7 (44.5–46.0) | **3.50× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative direct 64B 0.489 ns of 5.420 (9.0 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.758 ns of 15.869 (11.1 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 17.871 ns of 38.379 (46.6 %)
