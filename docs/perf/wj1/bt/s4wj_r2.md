# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.1) | 4.5 (4.4–4.5) | 43.5 (42.3–43.9) | 1.29× slower |
| 64B | 5.9 (5.7–6.1) | 6.1 (6.0–6.2) | 44.2 (44.1–44.2) | **1.02× faster** |
| 256B | 6.4 (6.3–6.4) | 7.7 (7.4–7.7) | 44.4 (40.4–58.3) | **1.20× faster** |
| 1024B | 8.6 (8.4–9.0) | 16.1 (16.0–16.5) | 291.2 (238.4–300.4) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.7–5.8) | 5.5 (5.5–5.6) | 40.2 (38.6–40.7) | **1.17× faster** |
| 64B | 4.7 (4.7–4.8) | 6.0 (6.0–6.4) | 41.2 (41.0–41.5) | **1.27× faster** |
| 256B | 4.8 (4.7–4.9) | 8.9 (8.8–9.1) | 43.1 (41.2–43.6) | **1.86× faster** |
| 1024B | 5.1 (5.0–5.4) | 16.6 (15.0–16.9) | 40.6 (39.0–41.8) | **3.23× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.8) | 5.5 (5.5–5.6) | 40.5 (38.5–40.7) | **1.09× faster** |
| 64B | 5.0 (4.9–5.3) | 5.6 (5.6–5.8) | 41.0 (40.8–41.5) | **1.12× faster** |
| 256B | 5.1 (5.1–5.1) | 8.3 (8.1–8.3) | 42.2 (40.9–44.1) | **1.63× faster** |
| 1024B | 5.5 (5.4–5.7) | 18.8 (18.2–18.9) | 45.7 (43.5–49.5) | **3.41× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 16B 1.123 ns of 4.736 (23.7 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 1.904 ns of 16.553 (11.5 %)
- system: 1/12 cells within 0.3 ns; widest relative direct 256B 17.871 ns of 44.385 (40.3 %)
