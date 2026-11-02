# bench:table (paired): 3 process(es) × 200 rounds; commit c0ee7dfb, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.3 (6.1–6.3) | 4.8 (4.8–4.8) | 45.8 (44.7–46.5) | 1.32× slower |
| 64B | 6.0 (5.5–6.3) | 6.9 (6.5–6.9) | 46.6 (46.2–46.6) | **1.16× faster** |
| 256B | 6.3 (6.2–6.8) | 8.3 (8.2–8.5) | 60.2 (57.2–62.1) | **1.30× faster** |
| 1024B | 8.9 (8.8–9.4) | 16.9 (16.7–17.1) | 48.0 (47.6–49.2) | **1.91× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.0–6.0) | 6.0 (5.9–6.1) | 40.9 (40.3–42.6) | **1.15× faster** |
| 64B | 5.0 (5.0–5.2) | 6.5 (6.3–6.6) | 44.5 (43.8–46.3) | **1.30× faster** |
| 256B | 5.1 (5.0–5.2) | 9.4 (8.5–11.7) | 39.0 (38.6–41.3) | **1.83× faster** |
| 1024B | 5.4 (5.3–5.4) | 20.1 (17.8–20.6) | 42.6 (41.5–51.1) | **3.71× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (5.1–5.4) | 5.8 (5.8–5.9) | 42.1 (40.7–42.5) | **1.10× faster** |
| 64B | 5.4 (5.1–5.4) | 6.1 (6.0–6.2) | 43.7 (43.4–43.8) | **1.14× faster** |
| 256B | 5.4 (5.3–5.4) | 9.4 (8.2–9.6) | 42.6 (40.9–43.4) | **1.75× faster** |
| 1024B | 6.0 (5.8–6.2) | 20.7 (20.0–20.7) | 65.3 (65.0–74.0) | **3.47× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative churn 16B 0.977 ns of 5.176 (18.9 %)
- mimalloc: 5/12 cells within 0.3 ns; widest relative churn 256B 3.223 ns of 9.375 (34.4 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 9.668 ns of 42.578 (22.7 %)
