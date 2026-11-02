# bench:table (paired): 3 process(es) × 200 rounds; commit 42a0439b, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.7 (5.5–5.9) | 4.4 (4.4–4.5) | 42.3 (42.2–42.4) | 1.29× slower |
| 64B | 5.4 (5.1–6.9) | 6.0 (6.0–6.0) | 43.4 (43.3–43.5) | **1.10× faster** |
| 256B | 5.8 (5.7–5.9) | 7.6 (7.5–7.6) | 52.7 (52.7–55.0) | **1.31× faster** |
| 1024B | 8.0 (7.9–8.0) | 15.7 (15.4–15.9) | 44.6 (44.5–45.6) | **1.96× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.8) | 5.4 (5.3–6.1) | 38.4 (38.0–38.8) | **1.15× faster** |
| 64B | 4.7 (4.6–4.9) | 5.8 (5.7–6.1) | 40.5 (40.1–40.9) | **1.23× faster** |
| 256B | 4.7 (4.7–4.7) | 8.3 (7.8–8.5) | 34.9 (34.9–35.7) | **1.76× faster** |
| 1024B | 4.9 (4.8–5.0) | 17.8 (16.1–18.6) | 40.2 (38.9–40.3) | **3.64× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–4.9) | 5.4 (5.3–6.1) | 38.2 (38.1–38.6) | **1.10× faster** |
| 64B | 4.9 (4.9–5.3) | 5.5 (5.5–5.6) | 40.4 (40.1–40.7) | **1.13× faster** |
| 256B | 5.0 (4.9–5.0) | 8.4 (8.3–8.6) | 38.3 (38.2–38.5) | **1.69× faster** |
| 1024B | 5.3 (5.2–5.4) | 18.5 (17.9–19.1) | 63.8 (61.1–66.1) | **3.48× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative direct 64B 1.807 ns of 5.420 (33.3 %)
- mimalloc: 4/12 cells within 0.3 ns; widest relative churn 1024B 2.491 ns of 17.773 (14.0 %)
- system: 2/12 cells within 0.3 ns; widest relative churn_write 1024B 4.980 ns of 63.818 (7.8 %)
