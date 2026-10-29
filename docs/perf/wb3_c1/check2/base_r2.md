# bench:table (paired): 3 process(es) × 200 rounds; commit 9d2823e5, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.4 (7.4–7.5) | 4.9 (4.9–4.9) | 45.6 (45.3–46.7) | 1.51× slower |
| 64B | 8.4 (7.1–9.4) | 6.6 (6.6–6.6) | 46.7 (46.7–47.6) | 1.28× slower |
| 256B | 8.2 (8.1–8.3) | 8.2 (8.1–8.3) | 41.6 (41.0–43.0) | 1.01× slower |
| 1024B | 10.8 (10.5–10.9) | 17.3 (17.2–18.1) | 282.5 (274.7–299.5) | **1.59× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.0) | 6.2 (6.1–6.5) | 42.1 (41.6–43.4) | **1.06× faster** |
| 64B | 5.9 (5.8–6.0) | 7.6 (6.8–7.9) | 44.9 (44.3–45.0) | **1.29× faster** |
| 256B | 5.9 (5.8–6.2) | 9.3 (9.1–12.4) | 42.6 (39.0–45.8) | **1.58× faster** |
| 1024B | 6.1 (6.1–7.3) | 20.6 (20.6–21.1) | 43.5 (42.5–43.5) | **3.38× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (6.0–6.1) | 6.0 (6.0–6.3) | 41.8 (41.3–44.1) | 1.01× slower |
| 64B | 6.1 (6.0–6.5) | 6.4 (6.2–6.5) | 46.0 (43.0–46.0) | **1.06× faster** |
| 256B | 6.3 (6.3–7.2) | 9.8 (8.3–10.0) | 44.1 (41.2–45.7) | **1.55× faster** |
| 1024B | 7.6 (7.0–7.9) | 21.1 (20.9–21.3) | 74.7 (51.9–79.8) | **2.79× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 5/12 cells within 0.3 ns; widest relative direct 64B 2.295 ns of 8.447 (27.2 %)
- mimalloc: 4/12 cells within 0.3 ns; widest relative churn 256B 3.320 ns of 9.326 (35.6 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 27.881 ns of 74.658 (37.3 %)
