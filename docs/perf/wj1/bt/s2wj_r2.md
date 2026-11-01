# bench:table (paired): 3 process(es) × 200 rounds; commit ff112904, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (6.1–6.3) | 4.6 (4.5–4.7) | 44.1 (43.4–45.5) | 1.34× slower |
| 64B | 6.2 (5.6–6.2) | 6.3 (6.3–6.4) | 45.4 (44.4–46.1) | **1.02× faster** |
| 256B | 6.6 (6.5–6.7) | 7.8 (7.8–8.0) | 58.6 (40.2–60.7) | **1.19× faster** |
| 1024B | 8.8 (8.7–9.0) | 16.7 (15.9–16.8) | 305.0 (250.2–318.4) | **1.90× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 4.9 (4.8–5.0) | 5.6 (5.6–5.7) | 40.6 (39.5–40.7) | **1.14× faster** |
| 64B | 4.9 (4.8–5.0) | 6.2 (6.1–6.5) | 42.3 (42.1–43.1) | **1.26× faster** |
| 256B | 4.9 (4.8–5.0) | 8.6 (8.4–9.0) | 43.3 (42.5–45.3) | **1.74× faster** |
| 1024B | 5.2 (5.1–5.6) | 17.4 (15.8–19.5) | 43.4 (39.6–43.8) | **3.37× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.1 (5.0–5.7) | 5.6 (5.5–5.7) | 40.8 (39.7–41.5) | **1.11× faster** |
| 64B | 5.1 (5.0–5.2) | 5.8 (5.7–6.2) | 42.5 (41.0–43.0) | **1.12× faster** |
| 256B | 5.2 (5.1–5.2) | 8.0 (7.7–8.0) | 41.0 (40.3–42.5) | **1.55× faster** |
| 1024B | 5.5 (5.5–5.6) | 19.3 (18.8–19.7) | 45.7 (45.6–51.9) | **3.50× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 9/12 cells within 0.3 ns; widest relative churn_write 16B 0.733 ns of 5.078 (14.4 %)
- mimalloc: 6/12 cells within 0.3 ns; widest relative churn 1024B 3.711 ns of 17.432 (21.3 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 20.507 ns of 58.594 (35.0 %)
