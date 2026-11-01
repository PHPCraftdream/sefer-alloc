### Judge, Windows, campaign 1 (preregistered design: 12 blocks, minimum of 3 repeats)

| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | rule | verdict | min base ms | min x ms |
|---|---:|---:|---:|---:|---:|---|---|---:|---:|
| grow64 | 12 (x3) | 0.989 | 0.931–1.051 | 1.023 | 0.958–1.092 | C1 | HOLD | 1282 | 1292 |
| grow1k | 12 (x3) | 1.066 | 0.973–1.168 | 1.092 | 1.029–1.159 | C1 | UNREADABLE (A/A excludes 1) | 512 | 528 |
| regrow | 12 (x3) | 0.911 | 0.838–0.989 | 0.955 | 0.884–1.032 | C1 | ok | 413 | 393 |
| growmix | 12 (x3) | 1.003 | 0.943–1.066 | 1.009 | 0.952–1.069 | C1 | ok | 657 | 676 |
| wild | 12 (x3) | 0.938 | 0.856–1.028 | 0.939 | 0.889–0.992 | T1 | UNREADABLE (A/A excludes 1) | 230 | 207 |
| phase | 12 (x3) | 1.083 | 0.970–1.210 | 1.046 | 0.964–1.135 | C1 | KILL | 776 | 791 |
| large | 12 (x3) | 0.987 | 0.910–1.070 | 0.966 | 0.831–1.123 | C1 | HOLD | 16 | 16 |
| burst | 12 (x3) | 1.030 | 0.961–1.104 | 0.975 | 0.926–1.025 | C1 | KILL | 90 | 93 |

### Judge, Linux (WSL2), campaign 1 (wild/phase/large/burst at scale 1, GiB scenarios at scale 0.5)

| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | rule | verdict | min base ms | min x ms |
|---|---:|---:|---:|---:|---:|---|---|---:|---:|
| grow64 | 12 (x3) | 1.012 | 0.956–1.070 | 0.988 | 0.923–1.057 | C2 | ok | 509 | 533 |
| grow1k | 12 (x3) | 0.929 | 0.829–1.042 | 0.958 | 0.882–1.042 | C2 | HOLD | 151 | 147 |
| regrow | 12 (x3) | 0.937 | 0.880–0.997 | 0.947 | 0.860–1.043 | C2 | ok | 134 | 136 |
| growmix | 12 (x3) | 1.122 | 1.053–1.196 | 1.075 | 1.007–1.148 | C2 | UNREADABLE (A/A excludes 1) | 224 | 225 |
| wild | 12 (x3) | 0.972 | 0.941–1.004 | 0.988 | 0.953–1.024 | T2 | FAIL | 200 | 194 |
| phase | 12 (x3) | 1.003 | 0.980–1.026 | 0.989 | 0.965–1.014 | C2 | ok | 655 | 663 |
| large | 12 (x3) | 1.027 | 0.999–1.055 | 1.108 | 0.910–1.348 | C2 | ok | 13 | 14 |
| burst | 12 (x3) | 1.022 | 0.987–1.057 | 0.995 | 0.967–1.023 | C2 | ok | 73 | 74 |

### bench:table paired set s1 (order base, x, x, base; aligned code)

| Cell (x) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -0.5 % / -0.2 % | -0.2 % | -1.6 % / +0.8 % | +0.0 % | 1.313 → 1.308 |
| direct 64 B | +17.3 % / -29.6 % | +0.0 % | +9.0 % / -29.6 % | +0.0 % | 1.091 → 0.969 |
| direct 256 B | +1.8 % / -8.3 % | +0.0 % | +0.0 % / -8.8 % | +0.0 % | 0.817 → 0.788 |
| direct 1024 B | -0.9 % / +2.6 % | +0.0 % | +0.0 % / +0.5 % | +0.0 % | 0.531 → 0.535 |
| churn 16 B | -0.9 % / -0.8 % | -0.8 % | -0.9 % / +0.0 % | +0.0 % | 0.869 → 0.862 |
| churn 64 B | +2.9 % / +2.4 % | +2.4 % | +2.9 % / +1.0 % | +1.0 % | 0.750 → 0.770 |
| churn 256 B | -13.2 % / +2.5 % | +0.0 % | -1.9 % / -4.6 % | -1.9 % | 0.482 → 0.451 |
| churn 1024 B | +0.7 % / -2.1 % | +0.0 % | +0.0 % / -2.6 % | +0.0 % | 0.272 → 0.271 |
| churn_write 16 B | +2.7 % / -0.9 % | +0.0 % | +1.9 % / -0.9 % | +0.0 % | 0.912 → 0.920 |
| churn_write 64 B | -2.2 % / +1.8 % | +0.0 % | -4.5 % / +1.8 % | +0.0 % | 0.884 → 0.882 |
| churn_write 256 B | +2.9 % / +0.5 % | +0.5 % | +1.8 % / -5.8 % | +0.0 % | 0.576 → 0.586 |
| churn_write 1024 B | -3.2 % / -9.0 % | -3.2 % | -5.7 % / -10.3 % | -5.7 % | 0.314 → 0.294 |

Sefer time (A/B of the two builds): geometric mean -2.7 %; agreed wins 7.7 pp, agreed losses 1.0 pp; largest agreed loss +1.0 %.
Ratio to mimalloc: geometric mean over 12 cells: -1.8 %; agreed wins 4.3 pp, agreed losses 2.9 pp (ratio 1.48); largest agreed loss +2.4 %.

Magnitude rule: gm -2.7 % (<= -0.5 %), wins/losses 7.7/1.0 pp (>= 2x), largest loss +1.0 % (<= 3 %), new LOSS cells 0 (0) => **GO**

### bench:table paired set s2 (order base, x, x, base; aligned code)

| Cell (x) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -4.5 % / +0.2 % | +0.0 % | -1.5 % / -0.8 % | -0.8 % | 1.313 → 1.284 |
| direct 64 B | +12.6 % / +41.2 % | +12.6 % | +13.4 % / +40.2 % | +13.4 % | 0.884 → 1.121 |
| direct 256 B | +2.9 % / -5.8 % | +0.0 % | +2.3 % / -6.4 % | +0.0 % | 0.798 → 0.785 |
| direct 1024 B | -1.9 % / +6.0 % | +0.0 % | -0.5 % / +1.5 % | +0.0 % | 0.547 → 0.558 |
| churn 16 B | -4.1 % / -6.1 % | -4.1 % | -0.9 % / -5.4 % | -0.9 % | 0.902 → 0.855 |
| churn 64 B | -3.5 % / -0.3 % | -0.3 % | +0.0 % / -1.0 % | +0.0 % | 0.744 → 0.729 |
| churn 256 B | +4.2 % / +6.6 % | +4.2 % | +4.6 % / -8.0 % | +0.0 % | 0.424 → 0.447 |
| churn 1024 B | +12.0 % / +10.6 % | +10.6 % | -3.4 % / +1.7 % | +0.0 % | 0.278 → 0.310 |
| churn_write 16 B | +2.0 % / +2.6 % | +2.0 % | +2.8 % / +0.9 % | +0.9 % | 0.885 → 0.905 |
| churn_write 64 B | +3.3 % / -4.3 % | +0.0 % | +0.9 % / -3.6 % | +0.0 % | 0.871 → 0.866 |
| churn_write 256 B | -2.5 % / +0.3 % | +0.0 % | +0.0 % / -1.7 % | +0.0 % | 0.582 → 0.575 |
| churn_write 1024 B | +4.9 % / -4.2 % | +0.0 % | +7.1 % / -6.8 % | +0.0 % | 0.300 → 0.301 |

Sefer time (A/B of the two builds): geometric mean +1.1 %; agreed wins 1.7 pp, agreed losses 14.4 pp; largest agreed loss +13.4 %.
Ratio to mimalloc: geometric mean over 12 cells: +2.6 %; agreed wins 4.4 pp, agreed losses 29.4 pp (ratio 0.15); largest agreed loss +12.6 %.

Magnitude rule: gm +1.1 % (<= -0.5 %), wins/losses 1.7/14.4 pp (>= 2x), largest loss +13.4 % (<= 3 %), new LOSS cells 1 (0) => **NO-GO**

### MT control (malloc_macro, SeferAlloc aggregate Mops/s, 6 samples per arm, ABBA x 3), rule C4: x worse than base by more than 3 % fails

| cell | base median | x median | x / base (medians) | C4 | base samples | x samples |
|---|---:|---:|---:|---|---|---|
| larson T=1 | 39.1 | 34.9 | 0.892 | FAIL | 53.41 36.71 39.36 38.84 48.29 34.88 | 35.65 34.12 46.42 32 34.04 37.02 |
| larson T=2 | 44.7 | 44.3 | 0.991 | ok | 47.01 42.06 42.45 50.51 55.07 41.36 | 42.1 42.34 44.56 45.93 48.77 44.13 |
| larson T=4 | 80.3 | 80.9 | 1.007 | ok | 82.49 81.31 65.53 95.1 2.07 79.33 | 80.13 77.85 85.77 81.69 86.58 79.91 |
| mstress T=1 | 55.3 | 51.8 | 0.935 | FAIL | 63.96 48.42 47.62 62.26 64.26 45.02 | 61.31 47 45.59 64.96 53.06 50.46 |
| mstress T=2 | 70.3 | 70.4 | 1.002 | ok | 92.27 3.12 62.26 74.82 72.13 68.45 | 97.97 68.32 72.48 57.57 96 66.15 |
| mstress T=4 | 129.6 | 121.8 | 0.940 | FAIL | 126.89 142.44 108.75 117.59 132.32 136.16 | 120.56 122.05 123.64 6.86 129.95 121.6 |

### Path activation (x + bench-internals build, Windows, sefer, scale 1, 5 runs, medians)

T3 compares the frees accepted by the second chance in `wild` with the memo exits of the base probe (WILD-1, median 3804795 over 5 runs, `docs/perf/wild1/probe_counts.txt`); the threshold is 40 %.

| scenario | second-chance accepts | Tier-1 hits | Tier-1 misses (hash probes) | share |
|---|---:|---:|---:|---|
| wild | 2424451 | 2903205 | 1384375 | 63.7 % (2424451 / 3804795) T3 PASS |
| phase | 435136 | 18202303 | 130675 |  |
| burst | 1 | 3021540 | 1 |  |
| growmix | 73015 | 77749 | 1382444 |  |
| regrow | 278920 | 1095297 | 468 |  |

### callgrind Ir (Linux, sefer arm), rule C5

| scenario (scale) | base Ir | x Ir | x / base - 1 | limit | C5 |
|---|---:|---:|---:|---|---|
| wild | 168176393 | 163519983 | -2.77 % (-4656410 / 168176393) | ..0 % | ok |
| phase | 415192904 | 415194942 | 0.00 % (2038 / 415192904) | ..0.5 % | ok |
| burst | 100684704 | 100686830 | 0.00 % (2126 / 100684704) | -0.3..0.3 % | ok |
