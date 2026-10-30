# bench:table (paired): 3 process(es) × 200 rounds; commit 10b53942, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.7 (6.3–6.9) | 4.9 (4.7–5.0) | 46.6 (44.2–47.8) | 1.37× slower |
| 64B | 8.0 (6.3–8.6) | 6.5 (6.4–6.6) | 47.9 (46.0–48.2) | 1.23× slower |
| 256B | 7.1 (6.9–7.4) | 8.1 (8.1–8.4) | 40.7 (39.8–42.5) | **1.14× faster** |
| 1024B | 10.1 (9.4–10.2) | 17.8 (16.9–18.5) | 272.9 (257.6–278.5) | **1.76× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–6.0) | 6.3 (5.9–6.7) | 41.9 (40.5–42.7) | **1.11× faster** |
| 64B | 5.6 (5.3–6.0) | 7.0 (6.3–7.2) | 45.4 (43.7–47.8) | **1.25× faster** |
| 256B | 5.7 (5.6–6.1) | 11.8 (9.3–12.8) | 39.1 (38.2–41.7) | **2.09× faster** |
| 1024B | 6.0 (5.9–6.3) | 20.8 (20.5–21.6) | 45.0 (42.3–53.4) | **3.49× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.7–6.3) | 6.0 (5.8–6.6) | 43.1 (41.8–43.8) | **1.02× faster** |
| 64B | 5.9 (5.6–6.2) | 6.3 (5.8–6.3) | 43.8 (43.6–46.9) | **1.08× faster** |
| 256B | 6.1 (6.0–6.1) | 9.8 (9.4–10.1) | 42.0 (41.1–44.8) | **1.62× faster** |
| 1024B | 6.8 (6.5–7.0) | 20.9 (20.2–21.7) | 76.7 (68.2–87.7) | **3.05× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 1/12 cells within 0.3 ns; widest relative direct 64B 2.295 ns of 8.008 (28.7 %)
- mimalloc: 2/12 cells within 0.3 ns; widest relative churn 256B 3.467 ns of 11.816 (29.3 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 19.531 ns of 76.709 (25.5 %)
