# bench:table (paired): 3 process(es) × 200 rounds; commit d9b05663, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.7–5.8) | 4.5 (4.5–4.6) | 43.8 (43.4–44.1) | 1.26× slower |
| 64B | 5.3 (4.8–5.3) | 6.1 (6.0–6.1) | 44.9 (43.9–45.3) | **1.15× faster** |
| 256B | 5.8 (5.8–5.9) | 7.6 (7.4–7.6) | 43.1 (38.6–60.4) | **1.30× faster** |
| 1024B | 8.2 (8.1–8.3) | 15.7 (15.4–16.0) | 279.8 (241.6–292.3) | **1.91× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.8 (4.7–4.8) | 5.5 (5.4–5.6) | 39.3 (38.6–39.6) | **1.14× faster** |
| 64B | 4.9 (4.7–5.4) | 5.9 (5.9–6.1) | 41.4 (40.3–42.9) | **1.20× faster** |
| 256B | 4.8 (4.7–4.9) | 8.2 (7.8–9.3) | 42.2 (39.6–43.0) | **1.70× faster** |
| 1024B | 5.0 (4.9–5.0) | 16.2 (15.1–16.3) | 41.9 (41.0–42.5) | **3.26× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.9–5.1) | 5.5 (5.5–5.5) | 40.0 (38.8–40.2) | **1.11× faster** |
| 64B | 5.0 (4.9–5.0) | 5.6 (5.6–5.7) | 41.2 (40.9–43.8) | **1.12× faster** |
| 256B | 5.0 (5.0–5.2) | 7.6 (7.3–8.8) | 43.1 (39.0–43.5) | **1.50× faster** |
| 1024B | 5.3 (5.3–5.4) | 18.8 (18.4–18.8) | 48.6 (44.1–49.0) | **3.52× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative churn 64B 0.732 ns of 4.883 (15.0 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative churn_write 256B 1.465 ns of 7.568 (19.4 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 21.729 ns of 43.066 (50.5 %)
