### A. Pin cache hit rate = hits / frees (counting build; requested = resolved slots asserted per run)

| m | order | base (main) | 4 slots | 8 slots | 16 slots | 32 slots | 64 slots |
|---:|---|---:|---:|---:|---:|---:|---:|
| 4 | seq | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 4 | cyclic | 1.00 | 0.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 4 | random | 1.00 | 0.50 | 1.00 | 1.00 | 1.00 | 1.00 |
| 5 | seq | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 5 | cyclic | 1.00 | 0.00 | 0.60 | 1.00 | 1.00 | 1.00 |
| 5 | random | 1.00 | 0.40 | 0.80 | 1.00 | 1.00 | 1.00 |
| 8 | seq | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 8 | cyclic | 1.00 | 0.00 | 0.00 | 1.00 | 1.00 | 1.00 |
| 8 | random | 1.00 | 0.25 | 0.50 | 1.00 | 1.00 | 1.00 |
| 16 | seq | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 16 | cyclic | 1.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| 16 | random | 1.00 | 0.13 | 0.25 | 0.50 | 1.00 | 1.00 |
| 32 | seq | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 32 | cyclic | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 |
| 32 | random | 0.50 | 0.06 | 0.12 | 0.25 | 0.50 | 1.00 |
| 64 | seq | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| 64 | cyclic | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| 64 | random | 0.25 | 0.03 | 0.06 | 0.13 | 0.25 | 0.50 |

### B. Time per foreign free in ns (medians of 9 interleaved processes) and the ratio to base and to mimalloc

| m | order | base | 4 slots | 8 slots | 16 slots | 32 slots | 64 slots | mimalloc |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 4 | seq | 17.5 | 18.7 (1.07) | 16.6 (0.95) | 16.5 (0.94) | 17.0 (0.97) | 16.5 (0.95) | 50.0 |
| 4 | cyclic | 31.1 | 76.6 (2.46) | 28.3 (0.91) | 28.8 (0.93) | 28.9 (0.93) | 29.8 (0.96) | 45.0 |
| 4 | random | 35.9 | 86.8 (2.42) | 35.1 (0.98) | 36.2 (1.01) | 37.4 (1.04) | 37.3 (1.04) | 37.8 |
| 8 | seq | 19.6 | 19.3 (0.99) | 19.4 (0.99) | 18.2 (0.93) | 18.7 (0.96) | 18.4 (0.94) | 54.6 |
| 8 | cyclic | 28.8 | 78.3 (2.72) | 87.9 (3.05) | 28.1 (0.98) | 30.3 (1.05) | 27.4 (0.95) | 46.2 |
| 8 | random | 37.9 | 109.6 (2.89) | 93.8 (2.48) | 37.4 (0.99) | 40.3 (1.06) | 38.3 (1.01) | 41.8 |
| 16 | seq | 20.3 | 18.3 (0.90) | 19.1 (0.94) | 17.6 (0.87) | 17.8 (0.88) | 17.7 (0.87) | 67.3 |
| 16 | cyclic | 30.0 | 80.3 (2.68) | 85.2 (2.84) | 86.5 (2.88) | 29.0 (0.97) | 30.9 (1.03) | 49.3 |
| 16 | random | 42.0 | 133.2 (3.17) | 131.3 (3.13) | 97.7 (2.32) | 41.0 (0.98) | 42.7 (1.02) | 44.8 |
| 32 | seq | 23.4 | 18.4 (0.79) | 20.4 (0.87) | 18.9 (0.81) | 20.2 (0.86) | 19.3 (0.83) | 68.1 |
| 32 | cyclic | 95.9 | 97.5 (1.02) | 99.6 (1.04) | 97.3 (1.01) | 103.6 (1.08) | 36.9 (0.38) | 68.2 |
| 32 | random | 115.1 | 161.4 (1.40) | 165.1 (1.43) | 153.7 (1.34) | 124.2 (1.08) | 44.9 (0.39) | 53.4 |
| 64 | seq | 20.1 | 19.4 (0.96) | 19.7 (0.98) | 19.0 (0.94) | 19.9 (0.99) | 19.2 (0.95) | 68.9 |
| 64 | cyclic | 108.7 | 103.0 (0.95) | 115.8 (1.07) | 107.8 (0.99) | 108.6 (1.00) | 112.6 (1.04) | 48.3 |
| 64 | random | 170.1 | 190.2 (1.12) | 194.5 (1.14) | 194.0 (1.14) | 182.6 (1.07) | 140.1 (0.82) | 51.5 |

Values in parentheses: ns per free of the arm / ns per free of base (main, 32 slots, no mask).

### C. Instructions (callgrind program totals; the set-up is identical across arms, so the difference is the cost of the frees)

| m | order | base total | 4 slots: extra Ir per free vs base | 8 slots: extra Ir per free vs base | 16 slots: extra Ir per free vs base | 32 slots: extra Ir per free vs base | 64 slots: extra Ir per free vs base |
|---:|---|---:|---:|---:|---:|---:|---:|
| 4 | seq | 9772783 | -0.98 | -0.93 | 4.79 | -0.62 | -0.21 |
| 4 | cyclic | 9838211 | 555.75 | -0.93 | -0.82 | -0.61 | -0.20 |
| 16 | seq | 30759528 | -1.32 | -1.25 | -1.21 | -1.10 | -0.83 |
| 16 | cyclic | 31018442 | 554.01 | 696.00 | 696.04 | -2.50 | -2.23 |
| 32 | seq | 60363996 | 0.23 | -0.41 | -0.15 | -1.03 | -0.88 |
| 32 | cyclic | 92649851 | 73.05 | 52.47 | 87.31 | 52.49 | -487.28 |
| 64 | seq | 117653455 | -0.62 | 0.12 | 0.02 | -1.70 | -0.69 |
| 64 | cyclic | 174492443 | 166.00 | 165.11 | 156.55 | 32.24 | -93.06 |

### E. Design iteration 1 (tree e5e1e9b0, superseded): the sweep wrapped its cursor with a variable modulus

In the cells where every free misses, the 32-slot arm of iteration 1 (same hit rate as base) was slower than base. Instructions (callgrind) and time (medians of 9 processes; fixed arm order and rotated arm order):

| m | order | extra Ir per free, 32 slots vs base | sweep_dead_slots Ir (m = 32 cyclic), base / 32 slots | ns per free 32 slots / base, fixed order | rotated order |
|---:|---|---:|---|---:|---:|
| 32 | cyclic | 86.2 | 13261346 / 18646786 | 1.36 | 1.24 |
| 64 | cyclic | 52.5 | - | 1.46 | 1.32 |

Cause: `self.sweep_cursor % slots` with a run-time `slots` compiles to a division and a bounds check inside the loop of `sweep_dead_slots`, which runs twice per miss. Fix: wrap with the mask and mask the index with the array capacity (section C and B are measured on the fixed tree).

### D. Worst-case retention of dead route descriptors per idle producer thread (slots x ~0.29 MiB, the DZ-3 per-pin figure for a segment with every class in use)

| slots | worst case per thread | worst case at 32 producer threads |
|---:|---:|---:|
| 4 | 1.2 MiB | 37.1 MiB |
| 8 | 2.3 MiB | 74.2 MiB |
| 16 | 4.6 MiB | 148.5 MiB |
| 32 | 9.3 MiB | 297.0 MiB |
| 64 | 18.6 MiB | 593.9 MiB |
