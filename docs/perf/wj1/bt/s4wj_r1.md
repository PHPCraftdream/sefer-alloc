# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (5.9–6.3) | 4.6 (4.5–4.7) | 42.9 (42.8–44.8) | 1.34× slower |
| 64B | 6.2 (5.8–6.3) | 6.2 (6.2–6.3) | 44.7 (43.8–46.5) | 1.00× (tie) |
| 256B | 6.5 (6.5–6.8) | 7.8 (7.6–7.9) | 41.0 (39.9–43.7) | **1.20× faster** |
| 1024B | 8.8 (8.5–9.4) | 16.2 (16.0–17.0) | 260.0 (244.4–318.1) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.2) | 5.6 (5.6–6.0) | 40.6 (40.0–42.2) | **1.13× faster** |
| 64B | 4.8 (4.7–6.6) | 6.1 (6.1–6.4) | 42.5 (41.5–45.4) | **1.28× faster** |
| 256B | 5.5 (5.1–5.7) | 9.2 (8.9–9.5) | 42.7 (41.1–43.3) | **1.66× faster** |
| 1024B | 5.1 (5.0–5.1) | 16.6 (16.3–19.6) | 42.1 (40.7–46.1) | **3.23× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.9–5.3) | 5.5 (5.5–5.8) | 40.6 (39.3–41.6) | **1.11× faster** |
| 64B | 5.3 (5.0–5.4) | 5.7 (5.7–5.9) | 41.9 (41.7–42.7) | **1.06× faster** |
| 256B | 5.1 (5.1–5.7) | 8.2 (8.1–8.8) | 42.5 (40.5–44.5) | **1.61× faster** |
| 1024B | 5.4 (5.4–5.7) | 19.2 (18.2–20.0) | 50.6 (47.7–51.7) | **3.54× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 1/12 cells within 0.3 ns; widest relative churn 64B 1.856 ns of 4.785 (38.8 %)
- mimalloc: 4/12 cells within 0.3 ns; widest relative churn 1024B 3.369 ns of 16.553 (20.4 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 73.632 ns of 260.010 (28.3 %)
