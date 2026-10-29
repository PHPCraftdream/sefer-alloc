# bench:table (paired): 3 process(es) × 200 rounds; commit 3782939c, features `production bench-internals`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 10.1 (10.1–10.2) | 4.7 (4.7–4.8) | 45.2 (45.1–46.4) | 2.12× slower |
| 64B | 9.9 (9.7–11.7) | 6.3 (6.3–6.4) | 45.7 (45.2–46.6) | 1.55× slower |
| 256B | 10.7 (10.7–10.9) | 8.0 (7.9–8.1) | 40.3 (40.0–41.0) | 1.34× slower |
| 1024B | 13.7 (13.6–13.9) | 16.7 (16.6–17.2) | 261.0 (256.3–263.2) | **1.22× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.6 (7.5–7.6) | 5.9 (5.8–5.9) | 41.8 (41.3–41.8) | 1.29× slower |
| 64B | 7.5 (7.5–7.5) | 6.3 (6.2–6.5) | 43.8 (43.4–44.8) | 1.18× slower |
| 256B | 7.5 (7.5–7.6) | 9.1 (9.0–11.9) | 38.7 (37.6–42.1) | **1.21× faster** |
| 1024B | 7.9 (7.7–7.9) | 19.5 (16.7–20.2) | 43.7 (41.2–44.8) | **2.48× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.7 (7.7–7.9) | 5.8 (5.6–5.8) | 41.6 (41.2–41.6) | 1.33× slower |
| 64B | 7.9 (7.8–8.1) | 5.9 (5.8–6.0) | 42.1 (41.8–44.1) | 1.33× slower |
| 256B | 8.3 (8.1–8.4) | 9.4 (8.0–9.6) | 41.8 (40.9–42.7) | **1.14× faster** |
| 1024B | 8.9 (8.5–9.7) | 20.6 (19.8–20.8) | 69.2 (68.3–72.0) | **2.32× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 8/12 cells within 0.3 ns; widest relative direct 64B 2.002 ns of 9.863 (20.3 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 256B 2.881 ns of 9.082 (31.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn 256B 4.492 ns of 38.672 (11.6 %)
