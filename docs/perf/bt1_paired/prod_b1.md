# raw samples: target/bt1f/prod_b1.p{1..3}.jsonl, manifest target/bt1f/prod_b1.manifest.json
# bench:table (paired): 3 process(es) × 200 rounds; cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)

### Warm repeated bulk burst (alloc 1024 then free 1024)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.8 (6.7–7.0) | 4.5 (4.5–4.6) | 48.9 (48.7–49.1) | 1.49× slower |
| 64B | 6.7 (6.7–6.7) | 6.5 (6.4–6.5) | 49.0 (48.5–50.5) | 1.04× slower |
| 256B | 7.4 (7.4–7.6) | 8.4 (8.3–8.4) | 46.6 (43.6–47.8) | **1.13× faster** |
| 1024B | 8.7 (8.5–8.7) | 16.0 (15.8–16.2) | 174.0 (171.1–178.7) | **1.84× faster** |

### Churn, non-writing (working set 256)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 5.9 (5.9–5.9) | 14.8 (14.6–15.0) | 39.6 (38.9–39.6) | **2.51× faster** |
| 64B | 5.9 (5.9–6.0) | 14.6 (14.6–14.9) | 61.2 (41.6–62.3) | **2.47× faster** |
| 256B | 5.9 (5.9–6.0) | 8.5 (8.3–8.6) | 35.5 (35.4–36.2) | **1.44× faster** |
| 1024B | 5.9 (5.9–6.1) | 67.1 (66.8–67.4) | 39.0 (38.5–41.8) | **11.36× faster** |

### Churn + write (16 B written after each alloc)

2048 pairs per sample, 1 working set(s) per sample

| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
| 16B | 6.1 (6.0–6.1) | 15.1 (14.9–15.1) | 39.1 (39.1–39.2) | **2.50× faster** |
| 64B | 6.1 (6.0–6.1) | 14.9 (14.8–15.1) | 61.1 (41.0–61.9) | **2.46× faster** |
| 256B | 6.1 (6.0–6.1) | 8.6 (8.5–8.6) | 36.3 (35.9–36.5) | **1.42× faster** |
| 1024B | 6.3 (6.2–6.3) | 65.6 (65.3–66.1) | 39.7 (39.1–41.6) | **10.33× faster** |
