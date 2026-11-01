# g1_wild_judge: seed 1, scale 1, 2 round(s) of ABBA per scenario, one process per run

ns = min over runs of ns per op (op = one alloc, free or realloc); RSS = median peak RSS. Ratio = sefer / mimalloc (< 1: sefer better).

| Scenario | ops | sefer ns/op | mi ns/op | time ratio | sefer peak MiB | mi peak MiB | RSS ratio |
|---|---:|---:|---:|---:|---:|---:|---:|
| grow64 | 33554432 | 44.38 | 24.46 | 1.815 | 1303 | 1159 | 1.124 |
| grow1k | 2097152 | 277.41 | 180.09 | 1.540 | 1183 | 1038 | 1.139 |
| growmix | 2920386 | 251.78 | 177.76 | 1.416 | 1354 | 1160 | 1.168 |
| regrow | 2191530 | 204.77 | 109.82 | 1.865 | 685 | 588 | 1.165 |
| large | 10374 | 7868.32 | 1146.26 | 6.864 | 14 | 28 | 0.490 |
| wild | 8573461 | 25.71 | 13.95 | 1.843 | 45 | 53 | 0.844 |
| phase | 36626747 | 19.53 | 14.55 | 1.343 | 196 | 204 | 0.959 |
