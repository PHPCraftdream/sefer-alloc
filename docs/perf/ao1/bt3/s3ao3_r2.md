# bench:table (paired): 3 process(es) × 200 rounds; commit 42a0439b, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.6–5.7) | 4.5 (4.4–4.5) | 42.8 (42.4–43.0) | 1.25× slower |
| 64B | 5.1 (4.9–5.6) | 5.9 (5.9–6.0) | 43.8 (43.0–44.7) | **1.15× faster** |
| 256B | 5.8 (5.7–5.9) | 7.4 (7.4–7.5) | 54.0 (46.5–55.0) | **1.29× faster** |
| 1024B | 8.0 (8.0–8.1) | 15.3 (15.1–15.5) | 45.2 (45.0–81.1) | **1.92× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.7 (4.6–4.8) | 5.4 (5.4–5.4) | 38.4 (38.2–39.2) | **1.15× faster** |
| 64B | 4.6 (4.6–4.6) | 5.9 (5.9–6.2) | 40.8 (39.8–40.9) | **1.27× faster** |
| 256B | 4.7 (4.6–4.8) | 8.4 (8.3–8.9) | 35.4 (34.9–35.8) | **1.80× faster** |
| 1024B | 4.9 (4.8–4.9) | 16.3 (15.7–18.5) | 40.7 (40.5–42.5) | **3.34× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.0 (4.8–5.2) | 5.4 (5.4–5.4) | 38.4 (37.9–39.5) | **1.08× faster** |
| 64B | 4.8 (4.8–4.8) | 5.6 (5.5–6.0) | 40.1 (40.0–40.4) | **1.16× faster** |
| 256B | 4.9 (4.9–4.9) | 8.6 (8.1–8.6) | 38.7 (38.2–40.2) | **1.74× faster** |
| 1024B | 5.3 (5.2–5.5) | 17.3 (16.9–18.6) | 59.1 (58.7–62.9) | **3.29× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative direct 64B 0.683 ns of 5.127 (13.3 %)
- mimalloc: 5/12 cells within 0.3 ns; widest relative churn 1024B 2.832 ns of 16.309 (17.4 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 1024B 36.133 ns of 45.215 (79.9 %)
