# bench:table (paired): 3 process(es) × 200 rounds; commit dfb8c143, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.2 (7.1–7.3) | 4.8 (4.8–4.8) | 44.7 (44.4–45.7) | 1.50× slower |
| 64B | 7.0 (6.8–8.6) | 6.4 (6.4–6.5) | 46.1 (45.1–46.8) | 1.09× slower |
| 256B | 7.9 (7.9–8.0) | 8.1 (8.0–8.1) | 41.6 (40.8–41.6) | **1.02× faster** |
| 1024B | 10.2 (10.1–10.3) | 16.7 (16.7–16.9) | 262.0 (260.7–273.4) | **1.65× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.6–5.8) | 5.9 (5.8–6.0) | 40.8 (40.6–41.4) | **1.03× faster** |
| 64B | 5.8 (5.7–6.3) | 6.5 (6.4–6.5) | 44.1 (43.5–44.5) | **1.12× faster** |
| 256B | 5.8 (5.7–5.8) | 9.5 (9.1–11.9) | 39.0 (37.7–39.2) | **1.65× faster** |
| 1024B | 6.0 (6.0–6.0) | 20.1 (17.1–20.5) | 42.8 (42.0–46.9) | **3.35× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–6.0) | 5.9 (5.7–5.9) | 41.3 (40.6–41.4) | 1.00× (tie) |
| 64B | 5.8 (5.8–5.9) | 6.1 (6.0–6.1) | 43.7 (42.2–45.1) | **1.04× faster** |
| 256B | 6.1 (6.0–6.1) | 9.1 (8.7–9.6) | 42.9 (42.5–42.9) | **1.50× faster** |
| 1024B | 6.8 (6.3–6.9) | 20.4 (20.3–20.4) | 67.8 (65.3–69.0) | **2.98× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 1.807 ns of 6.982 (25.9 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 256B 2.783 ns of 9.521 (29.2 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 1024B 4.883 ns of 42.773 (11.4 %)
