# bench:table (paired): 3 process(es) × 200 rounds; commit d9b05663, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.4 (4.4–4.5) | 42.6 (41.8–43.4) | 1.26× slower |
| 64B | 5.0 (4.9–5.1) | 6.1 (6.0–6.1) | 43.4 (43.3–44.1) | **1.22× faster** |
| 256B | 5.8 (5.7–5.8) | 7.6 (7.6–7.7) | 43.6 (41.7–61.7) | **1.32× faster** |
| 1024B | 8.2 (8.1–8.2) | 15.8 (15.6–15.9) | 305.1 (298.6–307.2) | **1.93× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (4.7–5.5) | 5.4 (5.3–5.5) | 38.7 (36.9–41.3) | **1.05× faster** |
| 64B | 4.7 (4.6–4.7) | 5.9 (5.8–5.9) | 40.9 (40.7–41.6) | **1.26× faster** |
| 256B | 4.7 (4.7–4.7) | 8.2 (7.7–8.4) | 41.9 (41.8–43.9) | **1.74× faster** |
| 1024B | 4.9 (4.9–5.0) | 15.6 (15.0–15.7) | 41.2 (38.8–41.9) | **3.17× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.9) | 5.4 (5.4–5.4) | 38.6 (36.9–39.3) | **1.11× faster** |
| 64B | 5.0 (4.8–5.1) | 5.5 (5.5–5.6) | 40.5 (39.6–40.5) | **1.10× faster** |
| 256B | 5.5 (5.4–5.7) | 7.6 (7.4–7.7) | 40.7 (38.3–43.4) | **1.38× faster** |
| 1024B | 5.3 (5.3–6.7) | 18.0 (17.8–18.5) | 44.5 (43.3–46.0) | **3.42× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 1024B 1.416 ns of 5.273 (26.9 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 0.683 ns of 8.154 (8.4 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 19.971 ns of 43.555 (45.9 %)
