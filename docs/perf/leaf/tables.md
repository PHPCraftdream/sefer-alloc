### Judge, windows

| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | min base ms | min x ms |
|---|---:|---:|---|---:|---|---:|---:|
| grow64 | 12 | 0.971 | 0.947–0.995 | 1.005 | 0.987–1.024 | 1077 | 1060 |
| grow1k | 12 | 0.939 | 0.909–0.969 | 0.972 | 0.941–1.005 | 471 | 443 |
| regrow | 12 | 0.967 | 0.932–1.003 | 0.985 | 0.941–1.031 | 332 | 332 |
| growmix | 12 | 1.005 | 0.966–1.045 | 1.010 | 0.987–1.033 | 513 | 500 |
| wild | 12 | 0.996 | 0.934–1.061 | 0.972 | 0.925–1.021 | 179 | 181 |
| phase | 12 | 0.994 | 0.941–1.050 | 1.001 | 0.950–1.054 | 613 | 608 |
| large | 12 | 0.966 | 0.903–1.034 | 1.017 | 0.942–1.097 | 14 | 14 |

### Judge, linux (WSL), scale 0.5

| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | min base ms | min x ms |
|---|---:|---:|---|---:|---|---:|---:|
| grow64 | 12 | 0.967 | 0.927–1.010 | 0.980 | 0.942–1.020 | 413 | 413 |
| grow1k | 12 | 1.005 | 0.939–1.077 | 0.988 | 0.935–1.044 | 122 | 130 |
| regrow | 12 | 0.952 | 0.902–1.006 | 1.009 | 0.936–1.088 | 119 | 118 |
| growmix | 12 | 0.945 | 0.890–1.003 | 0.969 | 0.927–1.013 | 182 | 183 |
| wild | 12 | 1.002 | 0.968–1.037 | 1.008 | 0.964–1.055 | 42 | 43 |
| phase | 12 | 0.976 | 0.954–0.999 | 0.993 | 0.964–1.022 | 273 | 271 |
| large | 12 | 0.994 | 0.942–1.049 | 0.996 | 0.966–1.028 | 6 | 6 |

### bench:table paired set s1 (order base, leaf, leaf, base; aligned code)

| Cell (leaf) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -0.5 % / -0.2 % | -0.2 % | +1.7 % / +0.9 % | +0.9 % | 1.268 → 1.263 |
| direct 64 B | -8.3 % / -7.8 % | -7.8 % | -4.5 % / -6.3 % | -4.5 % | 0.906 → 0.833 |
| direct 256 B | +0.6 % / -2.1 % | +0.0 % | +2.6 % / -0.8 % | +0.0 % | 0.763 → 0.757 |
| direct 1024 B | +2.4 % / -1.1 % | +0.0 % | +2.4 % / -2.9 % | +0.0 % | 0.528 → 0.531 |
| churn 16 B | -0.5 % / +1.0 % | +0.0 % | +3.1 % / +1.0 % | +1.0 % | 0.865 → 0.867 |
| churn 64 B | -2.7 % / +2.3 % | +0.0 % | +2.1 % / -1.0 % | +0.0 % | 0.788 → 0.786 |
| churn 256 B | -4.8 % / -20.9 % | -4.8 % | +2.1 % / -1.0 % | +0.0 % | 0.570 → 0.497 |
| churn 1024 B | +16.5 % / -1.7 % | +0.0 % | +2.0 % / -1.0 % | +0.0 % | 0.262 → 0.281 |
| churn_write 16 B | -2.7 % / -1.8 % | -1.8 % | +0.0 % / +0.0 % | +0.0 % | 0.918 → 0.898 |
| churn_write 64 B | -4.3 % / -2.9 % | -2.9 % | -1.0 % / -2.9 % | -1.0 % | 0.899 → 0.866 |
| churn_write 256 B | -13.4 % / +1.0 % | +0.0 % | +2.0 % / +1.0 % | +1.0 % | 0.622 → 0.579 |
| churn_write 1024 B | -2.0 % / -3.2 % | -2.0 % | +1.9 % / -0.9 % | +0.0 % | 0.288 → 0.280 |

Sefer time (A/B of the two builds): geometric mean -0.0 %; agreed wins 5.5 pp, agreed losses 2.9 pp; largest agreed loss +1.0 %.
Ratio to mimalloc: geometric mean over 12 cells: -2.6 %; agreed wins 19.5 pp, agreed losses 0.0 pp (ratio ∞); largest agreed loss +0.0 %.

Magnitude rule: gm -0.0 % (<= -0.5 %), wins/losses 5.5/2.9 pp (>= 2x), largest loss +1.0 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**

### bench:table paired set s2 (order base, leaf, leaf, base; aligned code)

| Cell (leaf) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | +3.7 % / +0.6 % | +0.6 % | +2.6 % / +1.7 % | +1.7 % | 1.250 → 1.277 |
| direct 64 B | +0.1 % / +0.3 % | +0.1 % | +0.9 % / +2.8 % | +0.9 % | 0.893 → 0.895 |
| direct 256 B | -0.3 % / -2.4 % | -0.3 % | +1.7 % / +0.9 % | +0.9 % | 0.776 → 0.765 |
| direct 1024 B | -3.2 % / +0.3 % | +0.0 % | -3.5 % / +0.0 % | +0.0 % | 0.537 → 0.529 |
| churn 16 B | +0.0 % / +1.2 % | +0.0 % | +0.0 % / +2.1 % | +0.0 % | 0.869 → 0.874 |
| churn 64 B | -1.2 % / +0.4 % | +0.0 % | -2.0 % / +2.1 % | +0.0 % | 0.790 → 0.787 |
| churn 256 B | -21.0 % / +3.8 % | +0.0 % | -1.0 % / +2.1 % | +0.0 % | 0.548 → 0.500 |
| churn 1024 B | -6.5 % / -12.0 % | -6.5 % | -6.5 % / -10.1 % | -6.5 % | 0.286 → 0.259 |
| churn_write 16 B | +0.8 % / +1.2 % | +0.8 % | -1.0 % / +3.0 % | +0.0 % | 0.914 → 0.923 |
| churn_write 64 B | +2.8 % / -0.8 % | +0.0 % | +1.0 % / +1.0 % | +1.0 % | 0.874 → 0.882 |
| churn_write 256 B | +0.6 % / -0.8 % | +0.0 % | +0.0 % / +2.0 % | +0.0 % | 0.573 → 0.572 |
| churn_write 1024 B | -1.7 % / -7.4 % | -1.7 % | -0.9 % / -5.2 % | -0.9 % | 0.296 → 0.283 |

Sefer time (A/B of the two builds): geometric mean -0.3 %; agreed wins 7.4 pp, agreed losses 4.5 pp; largest agreed loss +1.7 %.
Ratio to mimalloc: geometric mean over 12 cells: -1.9 %; agreed wins 8.4 pp, agreed losses 1.6 pp (ratio 5.37); largest agreed loss +0.8 %.

Magnitude rule: gm -0.3 % (<= -0.5 %), wins/losses 7.4/4.5 pp (>= 2x), largest loss +1.7 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**

### MT control (malloc_macro, aggregate Mops/s, 6 samples per arm, ABBA x 3)

| cell | base median | leaf median | leaf / base (medians) | base samples | leaf samples |
|---|---:|---:|---:|---|---|
| larson T=1 | 55.2 | 55.2 | 0.999 | 45.58 37.24 56.74 55.58 59.44 54.91 | 54.76 55.63 49.34 52.27 56.17 58.32 |
| larson T=2 | 57.7 | 59.2 | 1.025 | 57.01 48.5 58.69 62.41 58.46 54.77 | 60.06 55.68 58.57 58.88 59.53 65.9 |
| larson T=4 | 86.8 | 93.5 | 1.076 | 95.72 83.54 91.89 72.55 90.15 79.34 | 96.44 103.96 82.94 95.74 86.26 91.2 |
| mstress T=1 | 68.0 | 71.6 | 1.053 | 68.69 57.17 66.1 73.22 67.36 69.94 | 71.63 54.07 66.04 71.63 74.22 72.05 |
| mstress T=2 | 79.4 | 72.1 | 0.908 | 85.06 67.94 72.31 78.89 106.81 79.97 | 79.07 68.45 76 63.89 69 75.29 |
| mstress T=4 | 139.6 | 141.3 | 1.013 | 106.14 135.93 143.46 144.68 140.75 138.43 | 142.7 129.87 140.28 135.43 142.4 150.4 |

### Confirmation on the final commit (windows; arms: base, base2 = the binary measured above, x = the final-commit binary)

| scenario | measured leaf / base | 95 % CI | final leaf / base | 95 % CI | final / measured | 95 % CI |
|---|---:|---|---:|---|---:|---|
| grow64 | 0.960 | 0.948–0.973 | 0.985 | 0.970–1.001 | 1.026 | 1.007–1.045 |
| grow1k | 0.927 | 0.903–0.952 | 0.936 | 0.908–0.965 | 1.010 | 0.987–1.033 |
| regrow | 0.966 | 0.945–0.988 | 0.980 | 0.947–1.014 | 1.014 | 0.988–1.042 |
| growmix | 0.965 | 0.945–0.986 | 0.967 | 0.936–0.999 | 1.002 | 0.974–1.030 |

### Motivation: time inside the per-leaf System requests (uncommitted timers, docs/perf/leaf/share_ident.txt), windows, 5 runs per cell, medians

| scenario | arm | total ms | leaf allocs | leaf alloc ms (raw) | timer pair ns | leaf alloc ms net = raw - allocs x timer | net / total | leaf frees | leaf free ms | free / total |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| grow64 | sefer | 940 | 262144 | 50 | 52 | 36.4 | 3.9 % (36.4 / 940 ms) | 256744 | 28 | 3.0 % (28 / 940 ms) |
| grow64 | sefer_pool | 803 | 262144 | 50 | 54 | 35.4 | 4.4 % (35.4 / 803 ms) | 0 | 0 | 0.0 % (0 / 803 ms) |
| grow1k | sefer | 474 | 262144 | 42 | 54 | 28.1 | 5.9 % (28.1 / 474 ms) | 256744 | 26 | 5.5 % (26 / 474 ms) |
| grow1k | sefer_pool | 361 | 262144 | 44 | 49 | 31.4 | 8.7 % (31.4 / 361 ms) | 0 | 0 | 0.0 % (0 / 361 ms) |
| regrow | sefer | 462 | 148619 | 26 | 51 | 17.4 | 3.8 % (17.4 / 462 ms) | 133839 | 13 | 2.8 % (13 / 462 ms) |
| regrow | sefer_pool | 365 | 148619 | 25 | 54 | 16.1 | 4.4 % (16.1 / 365 ms) | 0 | 0 | 0.0 % (0 / 365 ms) |
| growmix | sefer | 724 | 296788 | 51 | 54 | 35.0 | 4.8 % (35.0 / 724 ms) | 0 | 0 | 0.0 % (0 / 724 ms) |
| growmix | sefer_pool | 707 | 296788 | 51 | 54 | 34.1 | 4.8 % (34.1 / 707 ms) | 0 | 0 | 0.0 % (0 / 707 ms) |
