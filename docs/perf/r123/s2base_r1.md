# bench:table (paired): 3 process(es) × 200 rounds; commit 5fd6b277, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.6 (6.5–6.8) | 4.9 (4.7–4.9) | 45.3 (44.9–46.3) | 1.36× slower |
| 64B | 6.2 (6.0–9.7) | 6.5 (6.4–6.6) | 46.4 (46.1–46.8) | **1.06× faster** |
| 256B | 7.3 (7.1–7.3) | 8.1 (8.1–8.2) | 42.2 (41.5–43.5) | **1.11× faster** |
| 1024B | 10.0 (9.7–10.6) | 17.1 (17.1–17.5) | 282.5 (273.5–287.5) | **1.72× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.6–5.7) | 6.0 (6.0–6.3) | 42.4 (41.8–42.9) | **1.05× faster** |
| 64B | 5.6 (5.5–5.6) | 6.9 (6.2–7.8) | 45.5 (44.8–45.7) | **1.23× faster** |
| 256B | 5.7 (5.7–5.8) | 8.8 (8.5–12.4) | 44.6 (39.0–45.3) | **1.55× faster** |
| 1024B | 6.0 (5.9–6.2) | 20.6 (16.8–21.3) | 43.9 (41.8–49.7) | **3.46× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (5.7–6.2) | 5.9 (5.8–6.1) | 43.0 (42.2–43.2) | 1.03× slower |
| 64B | 6.0 (5.8–6.3) | 6.2 (5.9–6.4) | 45.5 (43.9–45.8) | **1.02× faster** |
| 256B | 6.6 (5.9–6.7) | 9.5 (8.1–9.6) | 44.2 (41.5–46.4) | **1.43× faster** |
| 1024B | 6.6 (6.3–7.8) | 20.9 (19.6–21.0) | 55.9 (50.9–66.7) | **3.14× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 6/12 cells within 0.3 ns; widest relative direct 64B 3.711 ns of 6.152 (60.3 %)
- mimalloc: 3/12 cells within 0.3 ns; widest relative churn 256B 3.858 ns of 8.838 (43.7 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 15.722 ns of 55.859 (28.1 %)
