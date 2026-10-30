# bench:table (paired): 3 process(es) × 200 rounds; commit 97c18b36, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.5 (6.4–6.5) | 4.8 (4.7–4.8) | 44.3 (44.0–44.6) | 1.36× slower |
| 64B | 6.1 (5.9–6.2) | 6.4 (6.4–6.5) | 45.7 (45.7–45.9) | **1.06× faster** |
| 256B | 7.0 (6.9–7.1) | 8.0 (7.9–8.0) | 40.5 (40.0–40.7) | **1.15× faster** |
| 1024B | 9.4 (9.3–9.5) | 16.7 (16.7–17.0) | 255.1 (250.8–263.9) | **1.79× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.7) | 5.8 (5.7–5.9) | 40.2 (40.1–40.7) | **1.03× faster** |
| 64B | 5.5 (5.5–5.5) | 6.3 (6.2–6.3) | 43.5 (42.3–44.1) | **1.14× faster** |
| 256B | 5.5 (5.4–5.6) | 9.0 (8.6–9.0) | 38.1 (37.3–38.1) | **1.63× faster** |
| 1024B | 5.7 (5.6–6.0) | 16.8 (16.4–17.4) | 41.8 (41.8–42.7) | **2.97× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 5.8 (5.7–5.8) | 41.0 (40.2–41.2) | **1.02× faster** |
| 64B | 5.7 (5.6–5.7) | 6.0 (5.8–6.0) | 43.1 (42.3–43.3) | **1.04× faster** |
| 256B | 5.8 (5.7–5.9) | 8.7 (8.3–9.1) | 41.0 (40.4–41.7) | **1.50× faster** |
| 1024B | 6.2 (6.2–6.2) | 19.5 (19.3–20.1) | 67.2 (66.4–70.0) | **3.14× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 11/12 cells within 0.3 ns; widest relative churn 1024B 0.391 ns of 5.664 (6.9 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 0.830 ns of 8.740 (9.5 %)
- system: 1/12 cells within 0.3 ns; widest relative churn_write 1024B 3.663 ns of 67.236 (5.4 %)
