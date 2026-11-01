# bench:table (paired): 3 process(es) × 200 rounds; commit d9b05663, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.8) | 4.6 (4.5–4.6) | 42.5 (42.2–43.6) | 1.24× slower |
| 64B | 5.2 (5.0–5.2) | 6.2 (5.9–6.3) | 43.9 (43.8–44.0) | **1.19× faster** |
| 256B | 5.8 (5.8–5.9) | 7.7 (7.5–8.0) | 44.4 (42.6–58.5) | **1.33× faster** |
| 1024B | 8.3 (8.1–8.3) | 15.8 (15.6–15.9) | 312.4 (295.3–315.4) | **1.91× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.8) | 5.4 (5.4–5.5) | 38.6 (38.3–38.8) | **1.13× faster** |
| 64B | 4.7 (4.7–4.7) | 5.9 (5.7–5.9) | 40.8 (40.8–41.2) | **1.24× faster** |
| 256B | 4.8 (4.7–4.8) | 8.6 (8.5–8.9) | 41.9 (41.5–43.6) | **1.80× faster** |
| 1024B | 5.0 (4.9–5.2) | 16.5 (16.2–16.5) | 39.5 (39.2–40.3) | **3.31× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.0) | 5.5 (5.5–5.5) | 38.8 (38.5–39.1) | **1.11× faster** |
| 64B | 5.0 (4.9–5.0) | 5.6 (5.6–5.7) | 40.9 (40.1–41.0) | **1.12× faster** |
| 256B | 5.1 (5.1–5.2) | 7.9 (7.7–8.6) | 40.7 (40.4–41.4) | **1.54× faster** |
| 1024B | 5.4 (5.4–5.4) | 18.6 (18.4–18.7) | 45.4 (44.3–48.3) | **3.46× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 12/12 cells within 0.3 ns; widest relative churn 1024B 0.293 ns of 4.980 (5.9 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn_write 256B 0.879 ns of 7.910 (11.1 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 15.918 ns of 44.434 (35.8 %)
