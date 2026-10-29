# bench:table (paired): 3 process(es) × 200 rounds; commit 73241d28, features `production`, churn regime `fresh:16`, 2 bodies/sample for direct; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.8 (6.7–6.9) | 4.6 (4.6–4.6) | 49.9 (49.7–51.0) | 1.47× slower |
| 64B | 6.6 (6.5–6.6) | 6.4 (6.3–6.7) | 50.8 (50.8–52.3) | 1.02× slower |
| 256B | 6.6 (6.5–6.6) | 8.3 (8.0–8.6) | 40.1 (39.7–40.3) | **1.27× faster** |
| 1024B | 15.0 (14.8–16.6) | 15.4 (15.2–15.7) | 60.3 (56.8–62.4) | **1.02× faster** |

### Churn, non-writing (working set 256)

16384 pairs per sample, churn regime `fresh:16`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.6 (5.5–5.6) | 5.5 (5.5–5.5) | 39.3 (38.5–40.4) | 1.01× slower |
| 64B | 5.7 (5.5–5.8) | 7.3 (7.2–7.4) | 38.5 (36.2–40.0) | **1.29× faster** |
| 256B | 6.2 (6.1–6.2) | 9.1 (8.8–9.1) | 39.1 (38.8–39.7) | **1.47× faster** |
| 1024B | 7.0 (6.9–7.3) | 69.3 (68.6–69.3) | 45.5 (45.1–46.3) | **9.87× faster** |

### Churn + write (16 B written after each alloc)

16384 pairs per sample, churn regime `fresh:16`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.8 (5.8–6.0) | 5.7 (5.7–5.7) | 38.8 (38.5–39.4) | 1.02× slower |
| 64B | 6.0 (5.9–6.1) | 7.6 (7.4–7.6) | 38.5 (37.7–40.4) | **1.26× faster** |
| 256B | 6.7 (6.6–6.8) | 9.2 (9.1–9.3) | 38.8 (38.8–39.3) | **1.38× faster** |
| 1024B | 7.4 (7.4–7.5) | 69.2 (68.8–69.3) | 48.2 (44.5–49.4) | **9.31× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 10/12 cells within 0.3 ns; widest relative direct 1024B 1.758 ns of 15.039 (11.7 %)
- mimalloc: 7/12 cells within 0.3 ns; widest relative direct 256B 0.635 ns of 8.350 (7.6 %)
- system: 0/12 cells within 0.3 ns; widest relative churn_write 1024B 4.888 ns of 48.224 (10.1 %)
