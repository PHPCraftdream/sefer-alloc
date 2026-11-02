# bench:table (paired): 3 process(es) × 200 rounds; commit 42a0439b, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.4 (4.4–4.4) | 41.7 (41.7–43.3) | 1.26× slower |
| 64B | 5.0 (4.9–6.2) | 6.0 (6.0–6.0) | 43.5 (43.2–43.7) | **1.21× faster** |
| 256B | 5.8 (5.8–5.8) | 7.6 (7.5–7.7) | 54.6 (54.5–56.0) | **1.30× faster** |
| 1024B | 8.0 (7.9–8.1) | 15.4 (15.1–15.5) | 45.1 (44.0–46.2) | **1.93× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.6 (4.6–5.2) | 5.3 (5.3–5.3) | 37.6 (37.0–38.0) | **1.15× faster** |
| 64B | 4.6 (4.6–4.7) | 5.9 (5.9–5.9) | 40.2 (39.8–41.0) | **1.29× faster** |
| 256B | 4.7 (4.6–4.7) | 8.2 (7.9–10.1) | 37.7 (34.3–38.5) | **1.74× faster** |
| 1024B | 4.9 (4.8–4.9) | 18.4 (15.9–18.5) | 39.8 (37.5–44.4) | **3.77× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.0) | 5.3 (5.3–5.4) | 37.8 (37.7–38.1) | **1.10× faster** |
| 64B | 5.0 (4.8–5.2) | 5.5 (5.5–5.5) | 40.3 (38.8–40.5) | **1.11× faster** |
| 256B | 5.6 (4.9–5.7) | 8.1 (7.8–8.6) | 39.9 (37.6–40.4) | **1.43× faster** |
| 1024B | 5.3 (5.3–5.3) | 18.4 (18.3–18.6) | 67.7 (64.9–68.0) | **3.48× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 1.318 ns of 4.980 (26.5 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.197 ns of 8.154 (26.9 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 6.836 ns of 39.795 (17.2 %)
