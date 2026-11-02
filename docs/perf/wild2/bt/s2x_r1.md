# bench:table (paired): 3 process(es) × 200 rounds; commit d258bcd5, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.3 (6.1–6.4) | 4.9 (4.8–4.9) | 46.5 (46.0–46.7) | 1.28× slower |
| 64B | 6.6 (5.9–7.6) | 6.6 (6.5–7.0) | 46.7 (46.6–46.9) | 1.00× (tie) |
| 256B | 6.5 (6.5–6.6) | 8.3 (8.3–8.5) | 63.6 (60.8–67.4) | **1.28× faster** |
| 1024B | 9.4 (9.3–10.0) | 17.3 (17.3–17.6) | 48.8 (47.4–64.9) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.2–5.5) | 6.1 (5.9–6.6) | 43.3 (42.8–44.0) | **1.16× faster** |
| 64B | 5.2 (5.0–5.4) | 7.0 (6.5–7.4) | 46.2 (45.5–46.9) | **1.35× faster** |
| 256B | 5.5 (5.0–5.7) | 12.3 (11.7–12.9) | 41.0 (39.7–41.8) | **2.23× faster** |
| 1024B | 5.6 (5.5–6.2) | 17.8 (17.7–21.4) | 51.9 (51.2–56.0) | **3.19× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.4 (5.3–5.8) | 6.0 (5.9–6.0) | 43.5 (43.4–43.5) | **1.12× faster** |
| 64B | 5.4 (5.4–5.5) | 6.1 (6.1–6.3) | 46.1 (44.9–46.5) | **1.14× faster** |
| 256B | 5.6 (5.5–6.1) | 9.9 (9.6–9.9) | 43.8 (43.2–45.9) | **1.76× faster** |
| 1024B | 6.6 (6.2–8.2) | 21.1 (20.6–21.9) | 73.5 (71.7–82.4) | **3.18× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 3/12 cells within 0.3 ns; widest relative churn_write 1024B 2.051 ns of 6.641 (30.9 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 3.711 ns of 17.773 (20.9 %)
- system: 2/12 cells within 0.3 ns; widest relative direct 1024B 17.530 ns of 48.779 (35.9 %)
