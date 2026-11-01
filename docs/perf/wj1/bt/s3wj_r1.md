# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.0 (5.9–6.2) | 4.5 (4.5–4.6) | 43.0 (42.9–43.1) | 1.31× slower |
| 64B | 6.0 (5.6–8.8) | 6.2 (6.1–6.2) | 44.6 (44.2–44.6) | **1.02× faster** |
| 256B | 6.6 (6.3–6.6) | 7.7 (7.6–7.7) | 45.7 (40.1–58.1) | **1.17× faster** |
| 1024B | 8.7 (8.6–9.0) | 16.2 (16.2–16.4) | 303.4 (240.9–304.5) | **1.87× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–4.8) | 5.5 (5.5–5.6) | 39.3 (39.1–39.4) | **1.14× faster** |
| 64B | 4.7 (4.7–4.8) | 6.1 (6.0–6.2) | 41.8 (41.8–41.9) | **1.28× faster** |
| 256B | 4.8 (4.8–4.8) | 9.1 (8.6–10.9) | 43.5 (40.0–44.2) | **1.90× faster** |
| 1024B | 5.0 (5.0–5.6) | 16.1 (16.1–19.1) | 40.2 (40.1–40.9) | **3.20× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–4.9) | 5.5 (5.5–5.5) | 39.9 (39.7–40.4) | **1.11× faster** |
| 64B | 5.0 (4.9–5.0) | 5.6 (5.6–5.7) | 41.8 (41.7–42.2) | **1.12× faster** |
| 256B | 5.1 (5.1–5.1) | 8.1 (7.7–8.9) | 42.9 (39.9–44.0) | **1.60× faster** |
| 1024B | 5.4 (5.4–5.6) | 19.3 (18.9–19.4) | 45.8 (45.1–49.7) | **3.56× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 3.174 ns of 6.006 (52.8 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 256B 2.344 ns of 9.082 (25.8 %)
- system: 3/12 cells within 0.3 ns; widest relative direct 256B 17.969 ns of 45.703 (39.3 %)
