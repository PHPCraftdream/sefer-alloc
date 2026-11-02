# bench:table (paired): 3 process(es) × 200 rounds; commit d258bcd5, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.3 (6.1–6.4) | 4.8 (4.7–4.8) | 44.7 (44.2–45.3) | 1.31× slower |
| 64B | 5.9 (5.4–7.3) | 6.3 (6.3–7.0) | 46.0 (45.8–46.2) | **1.07× faster** |
| 256B | 6.5 (6.3–6.6) | 8.3 (8.2–8.5) | 58.7 (58.1–63.8) | **1.26× faster** |
| 1024B | 9.3 (9.1–9.4) | 16.9 (16.7–17.1) | 49.5 (47.9–49.7) | **1.82× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.2–5.3) | 6.1 (6.1–6.2) | 42.2 (41.4–42.3) | **1.16× faster** |
| 64B | 5.1 (5.0–5.1) | 6.8 (6.7–6.9) | 44.6 (43.3–44.7) | **1.34× faster** |
| 256B | 5.1 (5.1–5.4) | 11.9 (10.0–12.4) | 39.3 (38.5–40.2) | **2.34× faster** |
| 1024B | 5.5 (5.5–5.8) | 20.5 (17.1–20.7) | 50.6 (48.3–51.8) | **3.71× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (5.1–6.2) | 5.9 (5.8–5.9) | 42.1 (40.1–42.1) | **1.10× faster** |
| 64B | 5.5 (5.3–5.6) | 6.1 (6.1–6.2) | 45.6 (42.9–46.0) | **1.11× faster** |
| 256B | 5.6 (5.2–5.8) | 9.6 (8.7–9.7) | 42.9 (39.9–43.3) | **1.72× faster** |
| 1024B | 6.4 (6.1–6.6) | 20.7 (20.7–21.0) | 74.9 (72.7–79.3) | **3.24× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 7/12 cells within 0.3 ns; widest relative direct 64B 1.855 ns of 5.908 (31.4 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 2.393 ns of 11.865 (20.2 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 5.761 ns of 58.740 (9.8 %)
