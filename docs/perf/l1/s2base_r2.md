# bench:table (paired): 3 process(es) × 200 rounds; commit 10b53942, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.6 (6.5–6.7) | 4.7 (4.7–4.7) | 44.1 (43.9–44.9) | 1.40× slower |
| 64B | 6.1 (6.1–6.2) | 6.4 (6.2–6.4) | 46.0 (45.1–46.5) | **1.05× faster** |
| 256B | 6.8 (6.6–6.9) | 7.8 (7.8–8.0) | 41.0 (40.1–41.8) | **1.14× faster** |
| 1024B | 9.3 (9.2–9.3) | 16.7 (16.6–16.9) | 257.7 (249.9–259.4) | **1.80× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.5–6.7) | 5.8 (5.7–5.8) | 40.9 (40.5–41.9) | **1.04× faster** |
| 64B | 5.4 (5.4–5.5) | 6.3 (6.3–6.4) | 43.8 (43.5–44.1) | **1.17× faster** |
| 256B | 5.6 (5.5–5.9) | 8.8 (8.5–9.1) | 42.1 (39.1–43.5) | **1.59× faster** |
| 1024B | 5.6 (5.6–6.6) | 17.2 (16.6–20.2) | 41.7 (40.9–43.1) | **3.06× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.9) | 5.7 (5.6–5.8) | 40.8 (40.0–42.0) | **1.01× faster** |
| 64B | 5.7 (5.7–5.9) | 5.8 (5.8–5.9) | 42.4 (42.3–43.5) | **1.02× faster** |
| 256B | 5.8 (5.7–5.9) | 8.3 (8.0–8.4) | 42.0 (41.1–42.8) | **1.43× faster** |
| 1024B | 6.2 (6.2–6.2) | 20.0 (18.9–20.1) | 51.9 (48.7–64.0) | **3.23× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn 16B 1.269 ns of 5.518 (23.0 %)
- mimalloc: 8/12 cells within 0.3 ns; widest relative churn 1024B 3.662 ns of 17.188 (21.3 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 15.283 ns of 51.904 (29.4 %)
