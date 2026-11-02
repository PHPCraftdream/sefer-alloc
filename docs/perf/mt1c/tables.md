### callgrind Ir, T=1, 300 000 steps (Linux)

| workload | sefer (base) | sefer (WILD-2) | mimalloc | sefer / mi | WILD-2 - base (Ir) |
|---|---:|---:|---:|---:|---:|
| mstress | 70417758 | 70417628 | 63674269 | 1.106 (70417758 / 63674269) | -130 |
| larson | 68719216 | 68719358 | 64158764 | 1.071 (68719216 / 64158764) | 142 |

### Inclusive Ir per allocator call (callgrind call tree, shim frame included)

| workload | entry | sefer Ir | calls | Ir / call | mi Ir | calls | Ir / call | sefer - mi per call |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| mstress | alloc_shim | 16000351 | 418135 | 38.3 | 22522505 | 418135 | 53.9 | -15.6 |
| mstress | dealloc_shim | 22217288 | 418135 | 53.1 | 10383917 | 418135 | 24.8 | 28.3 |
| larson | alloc_shim | 11047917 | 300768 | 36.7 | 17243986 | 300768 | 57.3 | -20.6 |
| larson | dealloc_shim | 15350442 | 300768 | 51.0 | 7018627 | 300768 | 23.3 | 27.7 |

### Cold events (callgrind call tree, mstress, 300 000 steps)

| event | Ir (inclusive) | calls |
|---|---:|---:|
| sefer free_magazine_full_fast (includes park) | 1832343 | 20757 (park calls) |
| sefer refill_magazine_miss | 1545062 | 20788 |
| mimalloc _mi_malloc_generic | 2077851 | 20038 |
| mimalloc _mi_page_retire | 735452 | not used |

Cold-event Ir: sefer 3377405 (3377405 / 70417758 = 0.048 of its run), mimalloc 2813303 (2813303 / 63674269 = 0.044); difference 564102 of the whole-run gap 6743489 (564102 / 6743489 = 0.084).

### perf stat, user mode, 3 000 000 steps, medians of 7 interleaved runs (Linux)

| workload | cycles sefer (M) | cycles mi (M) | sefer / mi | instructions sefer (M) | instructions mi (M) | sefer / mi | IPC sefer | IPC mi | L1D misses sefer / mi | min-to-max cycles sefer (M) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| mstress | 242.2 | 224.7 | 1.078 | 697.5 | 629.6 | 1.108 | 2.879 | 2.802 | 1.027 | 241.0-243.9 |
| larson | 220.2 | 188.8 | 1.166 | 679.2 | 630.0 | 1.078 | 3.085 | 3.337 | 0.900 | 211.4-228.8 |

### Other counters, medians of 5 interleaved runs (3 000 000 steps)

| workload | branches sefer / mi (M) | branch misses sefer / mi (M) | L1I misses sefer / mi (k) | dTLB load misses sefer / mi | L1D loads sefer / mi (M) |
|---|---|---|---|---|---|
| mstress | 109.1 / 132.4 | 2.397 / 2.362 | 55.6 / 58.1 | 1140 / 975 | 151.7 / 123.9 |
| larson | 103.4 / 121.0 | 0.274 / 0.373 | 38.0 / 45.9 | 1193 / 1166 | 147.1 / 126.6 |

### larson: cycles inside std mpmc try_recv (perf record cycles:u share x perf stat cycles)

sefer 49.99 % of 220.2 M = 110.1 M cycles; mimalloc 53.59 % of 188.8 M = 101.2 M cycles; difference 8.9 M of the cycle gap 31.4 M (8.9 / 31.4 = 0.284). Sampling shares carry skid; this row is an attribution hint, not a gate.
