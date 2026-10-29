# raw samples: target/bt1f/prod_b16.p{1..3}.jsonl, manifest target/bt1f/prod_b16.manifest.json
# bench:table (paired): 3 process(es) × 200 rounds; cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.7 (6.7–6.8) | 4.6 (4.6–4.7) | 50.7 (50.1–53.2) | 1.46× slower |
| 64B | 6.4 (6.3–6.5) | 6.8 (6.5–6.8) | 50.0 (49.7–51.2) | **1.05× faster** |
| 256B | 6.6 (6.5–6.6) | 8.6 (8.6–8.8) | 41.7 (41.3–41.9) | **1.31× faster** |
| 1024B | 14.7 (14.5–15.0) | 16.1 (15.6–16.3) | 51.4 (51.2–51.7) | **1.09× faster** |

### Churn, non-writing (working set 256)

32768 pairs per sample, 16 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.8–5.9) | 5.5 (5.5–5.6) | 38.4 (38.0–42.0) | 1.07× slower |
| 64B | 6.1 (6.0–6.4) | 8.8 (8.8–8.9) | 38.7 (38.5–39.4) | **1.46× faster** |
| 256B | 6.3 (6.2–6.4) | 8.1 (8.0–8.2) | 40.4 (39.8–40.7) | **1.28× faster** |
| 1024B | 7.4 (7.3–7.6) | 70.6 (69.8–71.3) | 48.5 (47.3–49.7) | **9.56× faster** |

### Churn + write (16 B written after each alloc)

32768 pairs per sample, 16 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (6.1–6.2) | 5.8 (5.7–5.8) | 38.8 (38.8–43.4) | 1.05× slower |
| 64B | 6.1 (6.1–6.5) | 9.0 (8.9–9.2) | 38.6 (38.5–39.5) | **1.47× faster** |
| 256B | 6.9 (6.8–6.9) | 8.5 (8.4–8.5) | 39.7 (39.2–40.1) | **1.23× faster** |
| 1024B | 7.8 (7.7–8.2) | 70.0 (68.4–70.4) | 45.7 (45.2–46.6) | **8.96× faster** |
