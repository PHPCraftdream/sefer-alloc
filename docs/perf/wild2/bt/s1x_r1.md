# bench:table (paired): 3 process(es) × 200 rounds; commit d258bcd5, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (6.1–6.3) | 4.7 (4.6–4.8) | 44.9 (43.7–45.6) | 1.31× slower |
| 64B | 6.5 (5.6–6.6) | 6.4 (6.4–6.4) | 46.3 (45.6–46.8) | 1.01× slower |
| 256B | 6.3 (6.3–6.4) | 8.1 (8.1–8.2) | 58.4 (46.7–60.7) | **1.28× faster** |
| 1024B | 8.9 (8.9–9.2) | 17.1 (16.7–17.2) | 48.0 (47.2–85.1) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (5.0–5.2) | 6.0 (5.8–6.0) | 41.5 (40.9–42.1) | **1.16× faster** |
| 64B | 5.2 (5.0–6.3) | 6.5 (6.3–6.6) | 45.4 (43.9–46.2) | **1.26× faster** |
| 256B | 5.0 (5.0–5.1) | 10.6 (9.2–11.9) | 38.7 (37.6–38.9) | **2.11× faster** |
| 1024B | 5.4 (5.2–5.7) | 20.0 (17.4–20.0) | 44.3 (42.3–51.0) | **3.68× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (5.3–5.4) | 5.8 (5.7–5.9) | 41.8 (41.1–41.9) | **1.07× faster** |
| 64B | 5.1 (5.1–5.4) | 6.0 (5.9–6.1) | 43.6 (43.4–44.6) | **1.16× faster** |
| 256B | 5.5 (5.3–5.7) | 9.3 (9.1–9.5) | 42.9 (41.0–42.9) | **1.70× faster** |
| 1024B | 5.6 (5.6–5.9) | 20.1 (20.1–20.3) | 69.2 (64.2–69.9) | **3.58× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative churn 64B 1.368 ns of 5.176 (26.4 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 256B 2.685 ns of 10.596 (25.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 37.842 ns of 48.047 (78.8 %)
