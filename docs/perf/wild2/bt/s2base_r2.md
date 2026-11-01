# bench:table (paired): 3 process(es) × 200 rounds; commit c0ee7dfb, features `production`, churn regime `steady`, 2 bodies/sample for direct and steady churn; 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz x16; Windows 10 Pro 10.0.19045 x64
# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.2 (6.2–6.4) | 4.8 (4.8–4.9) | 45.0 (44.3–46.3) | 1.29× slower |
| 64B | 5.7 (5.4–8.6) | 6.5 (6.3–7.2) | 46.6 (46.4–48.3) | **1.14× faster** |
| 256B | 6.9 (6.5–7.0) | 8.3 (8.2–8.6) | 67.0 (58.5–79.0) | **1.20× faster** |
| 1024B | 9.4 (9.1–11.2) | 17.5 (17.0–18.7) | 55.4 (54.6–71.2) | **1.85× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.5 (5.2–5.9) | 6.1 (6.0–6.4) | 42.8 (42.4–43.4) | **1.11× faster** |
| 64B | 5.1 (4.9–5.3) | 7.1 (6.7–7.3) | 46.4 (45.8–48.4) | **1.39× faster** |
| 256B | 5.5 (5.2–5.6) | 13.2 (12.1–13.7) | 40.9 (40.8–52.1) | **2.39× faster** |
| 1024B | 5.9 (5.7–6.8) | 21.1 (20.5–21.2) | 51.5 (50.1–52.1) | **3.61× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, churn regime `steady`

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.3 (5.3–12.4) | 6.0 (6.0–6.3) | 43.9 (43.0–44.6) | **1.12× faster** |
| 64B | 5.5 (5.4–5.6) | 6.2 (6.2–6.3) | 45.9 (45.8–46.5) | **1.12× faster** |
| 256B | 5.7 (5.3–5.9) | 9.8 (9.4–10.3) | 43.8 (43.7–54.4) | **1.72× faster** |
| 1024B | 6.4 (5.6–6.9) | 21.4 (20.6–22.4) | 76.6 (69.1–81.9) | **3.33× faster** |

Spread of the per-process minimums (max − min over processes), per arm:

- sefer: 2/12 cells within 0.3 ns; widest relative churn_write 16B 7.129 ns of 5.322 (134.0 %)
- mimalloc: 3/12 cells within 0.3 ns; widest relative direct 64B 0.830 ns of 6.494 (12.8 %)
- system: 0/12 cells within 0.3 ns; widest relative direct 256B 20.410 ns of 67.041 (30.4 %)
