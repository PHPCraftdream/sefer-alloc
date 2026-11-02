### A. Owner re-allocation after a wave of foreign frees (Linux, 3 freeing threads, medians of 7 processes per arm)

| shape | metric | sefer | mimalloc | sefer / mi |
|---|---|---:|---:|---:|
| 4 seg x 64 B (235929 blocks) | wave of foreign frees (ms) | 1.00 | 2.00 | 0.50 |
| 4 seg x 64 B (235929 blocks) | owner phase, sum of all alloc calls (ms) | 4.40 | 2.70 | 1.63 |
| 4 seg x 64 B (235929 blocks) | mean per alloc (ns) | 19 | 11 | 1.63 |
| 4 seg x 64 B (235929 blocks) | p99 (ns) | 53 | 66 | 0.80 |
| 4 seg x 64 B (235929 blocks) | p99.9 (ns) | 247 | 346 | 0.71 |
| 4 seg x 64 B (235929 blocks) | longest single alloc (ns) | 1582756 | 27292 | 57.99 |
| 4 seg x 64 B (235929 blocks) | time in calls over 10 us (ms) | 1.66 | 0.09 | 18.44 |
| 4 seg x 64 B | wave + owner phase, sum of medians (ms) | 5.40 | 4.70 | 1.15 |
| 16 seg x 64 B (943718 blocks) | wave of foreign frees (ms) | 2.80 | 7.90 | 0.35 |
| 16 seg x 64 B (943718 blocks) | owner phase, sum of all alloc calls (ms) | 24.80 | 12.80 | 1.94 |
| 16 seg x 64 B (943718 blocks) | mean per alloc (ns) | 26 | 14 | 1.93 |
| 16 seg x 64 B (943718 blocks) | p99 (ns) | 121 | 67 | 1.81 |
| 16 seg x 64 B (943718 blocks) | p99.9 (ns) | 310 | 506 | 0.61 |
| 16 seg x 64 B (943718 blocks) | longest single alloc (ns) | 9085822 | 836487 | 10.86 |
| 16 seg x 64 B (943718 blocks) | time in calls over 10 us (ms) | 11.24 | 2.00 | 5.62 |
| 16 seg x 64 B | wave + owner phase, sum of medians (ms) | 27.60 | 20.70 | 1.33 |
| 64 seg x 64 B (3774873 blocks) | wave of foreign frees (ms) | 10.40 | 32.30 | 0.32 |
| 64 seg x 64 B (3774873 blocks) | owner phase, sum of all alloc calls (ms) | 95.60 | 54.70 | 1.75 |
| 64 seg x 64 B (3774873 blocks) | mean per alloc (ns) | 25 | 15 | 1.74 |
| 64 seg x 64 B (3774873 blocks) | p99 (ns) | 102 | 69 | 1.48 |
| 64 seg x 64 B (3774873 blocks) | p99.9 (ns) | 306 | 505 | 0.61 |
| 64 seg x 64 B (3774873 blocks) | longest single alloc (ns) | 33012511 | 977243 | 33.78 |
| 64 seg x 64 B (3774873 blocks) | time in calls over 10 us (ms) | 42.63 | 10.35 | 4.12 |
| 64 seg x 64 B | wave + owner phase, sum of medians (ms) | 106.00 | 87.00 | 1.22 |
| 16 seg x 1024 B (58982 blocks) | wave of foreign frees (ms) | 0.80 | 1.80 | 0.44 |
| 16 seg x 1024 B (58982 blocks) | owner phase, sum of all alloc calls (ms) | 7.90 | 2.00 | 3.95 |
| 16 seg x 1024 B (58982 blocks) | mean per alloc (ns) | 135 | 34 | 3.94 |
| 16 seg x 1024 B (58982 blocks) | p99 (ns) | 536 | 313 | 1.71 |
| 16 seg x 1024 B (58982 blocks) | p99.9 (ns) | 1151 | 622 | 1.85 |
| 16 seg x 1024 B (58982 blocks) | longest single alloc (ns) | 4652415 | 136345 | 34.12 |
| 16 seg x 1024 B (58982 blocks) | time in calls over 10 us (ms) | 6.33 | 0.32 | 19.78 |
| 16 seg x 1024 B | wave + owner phase, sum of medians (ms) | 8.70 | 3.80 | 2.29 |

### B. Where the owner gets its blocks from (refill-path counters of a scratch-patched build, owner phase only)

| shape | refills | blocks wanted | from current segment freelist | from class cursor | from discovery hit | from bump-carve | other | discovery calls / hits | segments reserved | drained: pooled / released / kept | ingress calls / slots |
|---|---:|---:|---:|---:|---:|---:|---:|---|---:|---|---|
| 4 seg x 64 B | 14746 | 235936 | 53849 / 235936 = 0.228 | 182050 / 235936 = 0.772 | 30 / 235936 = 0.000 | 7 / 235936 = 0.000 | 0 | 4 / 3 | 0 | 2 / 0 / 2 | 1 / 4 |
| 16 seg x 64 B | 58982 | 943712 | 25894 / 943712 = 0.027 | 304648 / 943712 = 0.323 | 56 / 943712 = 0.000 | 613014 / 943712 = 0.650 | 100 | 38325 / 5 | 10 | 4 / 10 / 2 | 1 / 16 |
| 16 seg x 64 B, pool 8 | 58982 | 943712 | 25894 / 943712 = 0.027 | 549856 / 943712 = 0.583 | 96 / 943712 = 0.000 | 367806 / 943712 = 0.390 | 60 | 23001 / 9 | 6 | 8 / 6 / 2 | 1 / 16 |
| 16 seg x 1024 B | 3686 | 58976 | 1618 / 58976 = 0.027 | 19004 / 58976 = 0.322 | 40 / 58976 = 0.001 | 38214 / 58976 = 0.648 | 100 | 2400 / 5 | 10 | 4 / 10 / 2 | 1 / 16 |

### B2. Timeline of the 16 seg x 64 B case (cumulative, every 4th snapshot of 2048 refills)

| refills | blocks from freelists | blocks from bump-carve | segments reserved | discovery calls |
|---:|---:|---:|---:|---:|
| 0 | 0 | 0 | 0 | 0 |
| 8192 | 131056 | 0 | 0 | 2 |
| 16384 | 262128 | 0 | 0 | 4 |
| 24576 | 330598 | 62592 | 1 | 3918 |
| 32768 | 330598 | 193644 | 3 | 12110 |
| 40960 | 330598 | 324696 | 5 | 20302 |
| 49152 | 330598 | 455748 | 7 | 28494 |
| 57344 | 330598 | 586800 | 9 | 36686 |

### C. callgrind, owner phase = run with 943718 allocations minus run with 0 (16 seg x 64 B, harness statistics off)

| arm | Ir, 0 allocs | Ir, 943718 allocs | owner phase Ir | Ir per block |
|---|---:|---:|---:|---:|
| sefer | 296275973 | 545970780 | 249694807 | 264.6 |
| mi | 245770967 | 363247922 | 117476955 | 124.5 |

sefer / mi = 249694807 / 117476955 = 2.13.

### C2. Owner-phase share by function (sefer; inclusive Ir of the call tree of the run with 943718 allocations minus the run with 0, i.e. set-up and wave removed)

| function | owner-phase inclusive Ir | owner-phase calls | Ir per call | Ir per owner block (/ 943718) |
|---|---:|---:|---:|---:|
| refill_magazine_miss | 135693353 | 58982 | 2300.6 | 143.8 |
| find_segment_with_free_impl (inside the previous) | 83507543 | 38325 | 2178.9 | 88.5 |
| process_ingress (inside find_segment_with_free_impl) | 63351162 | 38321 | 1653.2 | 67.1 |
| drain_segment_sidecar (inside process_ingress) | 62545780 | 16 | 3909111.3 | 66.3 |
| try_carve_batch | 27130640 | 38330 | 707.8 | 28.7 |

Cross-check against the counters of section B: owner-phase refills 58982 against 58982, discovery calls 38325 against 38325. Bump-carve costs 44.3 Ir per carved block (27130640 / 613014); the sidecar drain costs 66.3 Ir per block of the 943718 blocks drained (62545780 / 943718); discovery without its drains costs 546.9 Ir per call.

### D. Experiments (each a scratch change measured against the same base, interleaved, medians of 7 processes)

| experiment | shape | metric | base | variant | mimalloc | variant / base |
|---|---|---|---:|---:|---:|---:|
| E0 pool 8 (Profile Throughput) | 4 seg x 64 B | sum_ms | 4.50 | 5.00 | 2.90 | 1.111 |
| E0 pool 8 (Profile Throughput) | 4 seg x 64 B | max_ns | 1608301.00 | 1805147.00 | 22015.00 | 1.122 |
| E0 pool 8 (Profile Throughput) | 8 seg x 64 B | sum_ms | 10.70 | 11.10 | 5.90 | 1.037 |
| E0 pool 8 (Profile Throughput) | 8 seg x 64 B | max_ns | 3495620.00 | 3628756.00 | 31429.00 | 1.038 |
| E0 pool 8 (Profile Throughput) | 16 seg x 64 B | sum_ms | 23.40 | 25.40 | 13.80 | 1.085 |
| E0 pool 8 (Profile Throughput) | 16 seg x 64 B | max_ns | 7815976.00 | 8916727.00 | 755099.00 | 1.141 |
| E1 no release/pool after a drain empties a segment | 4 seg x 64 B | sum_ms | 4.70 | 4.50 | 2.80 | 0.957 |
| E1 no release/pool after a drain empties a segment | 4 seg x 64 B | max_ns | 1696520.00 | 1745856.00 | 23240.00 | 1.029 |
| E1 no release/pool after a drain empties a segment | 16 seg x 64 B | sum_ms | 26.90 | 23.30 | 13.30 | 0.866 |
| E1 no release/pool after a drain empties a segment | 16 seg x 64 B | max_ns | 9583635.00 | 8194257.00 | 880211.00 | 0.855 |
| E1 no release/pool after a drain empties a segment | 16 seg x 1024 B | sum_ms | 7.90 | 5.30 | 3.10 | 0.671 |
| E1 no release/pool after a drain empties a segment | 16 seg x 1024 B | max_ns | 4453819.00 | 2785711.00 | 831221.00 | 0.625 |
| E2 skip discovery for 63 refills after a miss (upper bound, unsafe) | 16 seg x 64 B | sum_ms | 27.60 | 25.70 | - | 0.931 |

### D2. Experiment E2 in callgrind Ir (upper bound; same binaries as the wall-clock row above)

| scenario | base Ir | E2 Ir | E2 / base |
|---|---:|---:|---:|
| rp1 owner phase (943718 allocs minus 0) | 250324421 | 229695949 | 0.918 |
| g1_wild_judge grow64 (scale 0.3) | 1886004955 | 1672296815 | 0.887 |
| g1_wild_judge grow1k (scale 0.3) | 130387234 | 117057438 | 0.898 |
