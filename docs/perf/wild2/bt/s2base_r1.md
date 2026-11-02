# bench:table (paired): 3 process(es) × 200 rounds; commit c0ee7dfb, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.3 (6.2–6.3) | 4.7 (4.7–4.8) | 45.4 (45.3–46.1) | 1.34× slower |
| 64B | 5.8 (5.4–6.9) | 6.5 (6.5–6.5) | 46.4 (46.2–46.9) | **1.13× faster** |
| 256B | 6.4 (6.3–6.8) | 8.4 (8.1–8.4) | 59.3 (58.6–64.3) | **1.31× faster** |
| 1024B | 9.5 (8.9–10.1) | 17.1 (17.0–17.3) | 54.1 (51.3–58.6) | **1.80× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (5.0–5.3) | 5.9 (5.8–6.3) | 42.2 (41.1–43.6) | **1.11× faster** |
| 64B | 5.2 (4.9–5.4) | 6.7 (6.7–6.7) | 47.4 (45.9–48.4) | **1.30× faster** |
| 256B | 5.3 (4.9–5.9) | 12.3 (11.5–12.3) | 38.7 (38.3–40.4) | **2.32× faster** |
| 1024B | 5.8 (5.7–6.1) | 20.6 (18.5–20.9) | 49.7 (45.2–51.7) | **3.58× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.2 (5.1–5.6) | 6.0 (5.6–6.0) | 41.6 (41.3–43.1) | **1.14× faster** |
| 64B | 5.3 (5.2–5.3) | 6.3 (6.1–6.3) | 45.9 (44.7–46.8) | **1.17× faster** |
| 256B | 5.6 (5.6–5.8) | 9.6 (9.2–9.8) | 42.6 (41.0–43.6) | **1.71× faster** |
| 1024B | 6.2 (5.9–6.5) | 20.7 (20.7–21.1) | 77.8 (69.3–78.3) | **3.33× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 3/12 cells within 0.3 ns; widest relative direct 64B 1.514 ns of 5.811 (26.1 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 2.392 ns of 20.605 (11.6 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 7.324 ns of 54.102 (13.5 %)
