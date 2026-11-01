# bench:table (paired): 3 process(es) × 200 rounds; commit 82626f9c, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 4.4 (4.4–4.5) | 42.5 (42.0–44.1) | 1.25× slower |
| 64B | 5.0 (4.8–5.3) | 5.9 (5.9–6.0) | 43.2 (43.2–43.6) | **1.19× faster** |
| 256B | 5.6 (5.6–5.6) | 7.5 (7.4–7.6) | 55.6 (42.5–55.7) | **1.33× faster** |
| 1024B | 8.2 (8.1–8.3) | 15.4 (15.2–15.6) | 286.7 (284.5–300.0) | **1.88× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.7) | 5.4 (5.4–5.5) | 38.0 (36.9–38.6) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 6.0 (6.0–6.1) | 40.1 (39.9–40.2) | **1.28× faster** |
| 256B | 4.6 (4.6–5.3) | 8.4 (7.9–8.9) | 42.5 (42.3–43.0) | **1.82× faster** |
| 1024B | 4.9 (4.8–4.9) | 16.4 (16.4–16.5) | 40.3 (38.5–40.3) | **3.33× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.8–5.1) | 5.4 (5.3–5.4) | 38.4 (38.2–38.9) | **1.11× faster** |
| 64B | 4.8 (4.8–4.9) | 5.5 (5.5–5.6) | 39.6 (39.3–40.9) | **1.14× faster** |
| 256B | 5.0 (4.9–5.0) | 8.6 (8.6–8.6) | 40.2 (39.6–41.6) | **1.73× faster** |
| 1024B | 5.2 (5.2–5.2) | 18.5 (18.0–18.8) | 44.1 (43.6–44.7) | **3.58× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn 256B 0.683 ns of 4.639 (14.7 %)
- mimalloc: 9/12 cells within 0.3 ns; widest relative churn 256B 0.977 ns of 8.447 (11.6 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 13.135 ns of 55.566 (23.6 %)
