# raw samples: target/bt1f/bi_b1.p{1..3}.jsonl, manifest target/bt1f/bi_b1.manifest.json
# bench:table (paired): 3 process(es) × 200 rounds; cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 9.6 (9.6–9.6) | 4.6 (4.5–4.7) | 49.3 (49.0–49.8) | 2.07× slower |
| 64B | 9.8 (9.4–9.8) | 6.4 (6.3–6.5) | 49.1 (48.9–50.4) | 1.53× slower |
| 256B | 10.7 (10.4–10.7) | 8.4 (8.3–8.5) | 45.4 (44.4–50.2) | 1.27× slower |
| 1024B | 12.2 (12.2–12.4) | 16.0 (15.8–16.0) | 165.6 (97.2–182.8) | **1.31× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.5 (7.4–7.6) | 14.3 (14.1–14.3) | 39.2 (39.0–39.4) | **1.90× faster** |
| 64B | 7.5 (7.4–7.5) | 14.2 (14.1–14.5) | 64.8 (63.9–66.0) | **1.90× faster** |
| 256B | 7.5 (7.4–7.6) | 8.3 (8.3–8.4) | 40.8 (36.7–53.1) | **1.12× faster** |
| 1024B | 7.5 (7.5–7.6) | 66.5 (65.5–67.0) | 39.6 (39.0–40.4) | **8.90× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 7.7 (7.7–7.8) | 14.6 (14.4–14.8) | 39.5 (39.3–39.7) | **1.89× faster** |
| 64B | 7.8 (7.7–7.9) | 14.4 (14.4–14.6) | 66.9 (65.7–67.0) | **1.86× faster** |
| 256B | 7.9 (7.8–8.2) | 8.6 (8.4–8.6) | 38.1 (36.4–52.9) | **1.09× faster** |
| 1024B | 8.0 (7.8–8.0) | 65.8 (65.6–66.3) | 39.9 (39.6–40.9) | **8.27× faster** |
