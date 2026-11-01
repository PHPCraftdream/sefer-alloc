# bench:table (paired): 3 process(es) × 200 rounds; commit f05ffb67, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.9) | 4.5 (4.5–4.6) | 43.1 (43.1–43.1) | 1.26× slower |
| 64B | 5.7 (5.5–6.1) | 6.1 (6.1–6.1) | 43.9 (43.6–44.0) | **1.08× faster** |
| 256B | 6.2 (6.1–6.3) | 7.7 (7.5–7.7) | 55.6 (38.8–56.8) | **1.25× faster** |
| 1024B | 8.5 (8.3–8.6) | 15.9 (15.8–16.0) | 278.3 (255.0–294.9) | **1.86× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.7–4.8) | 5.4 (5.4–5.5) | 38.6 (37.9–38.7) | **1.12× faster** |
| 64B | 4.7 (4.7–4.8) | 5.9 (5.9–6.2) | 40.4 (40.1–41.2) | **1.25× faster** |
| 256B | 4.8 (4.7–4.8) | 8.1 (8.1–8.4) | 42.3 (39.6–42.5) | **1.69× faster** |
| 1024B | 5.0 (4.9–5.0) | 15.8 (15.2–18.9) | 40.1 (39.3–40.2) | **3.17× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (5.0–5.0) | 5.5 (5.4–5.5) | 38.4 (38.4–39.1) | **1.10× faster** |
| 64B | 5.0 (5.0–5.1) | 5.6 (5.6–6.1) | 40.9 (40.9–41.9) | **1.12× faster** |
| 256B | 5.6 (5.1–5.7) | 8.0 (7.9–8.8) | 39.8 (39.6–42.8) | **1.42× faster** |
| 1024B | 5.5 (5.3–5.6) | 18.4 (18.4–18.8) | 44.9 (44.8–45.0) | **3.37× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 256B 0.586 ns of 5.615 (10.4 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn 1024B 3.710 ns of 15.771 (23.5 %)
- system: 2/12 cells within 0.3 ns; widest relative direct 256B 18.066 ns of 55.615 (32.5 %)
