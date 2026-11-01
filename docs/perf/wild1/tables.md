### wild (windows, 12 blocks, one whole run per block and arm: the repeat with the smallest wall time)

| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst_alloc | 97.3 | 85.6 | 11.7 | 6.7..16.8 | 17 % (11.7 / 68.9) | 4181475 | 4181475 |
| burst_free | 73.8 | 31.3 | 42.5 | 40.2..44.9 | 62 % (42.5 / 68.9) | 4181475 | 4181475 |
| large_alloc | 11.3 | 8.8 | 2.5 | 2.1..2.9 | 4 % (2.5 / 68.9) | 4785 | 4785 |
| large_free | 5.1 | 0.9 | 4.1 | 3.8..4.4 | 6 % (4.1 / 68.9) | 4543 | 4543 |
| large_realloc | 3.0 | 2.9 | 0.1 | -0.2..0.4 | 0 % (0.1 / 68.9) | 145 | 145 |
| small_realloc | 0.6 | 0.4 | 0.1 | 0.1..0.2 | 0 % (0.1 / 68.9) | 1554 | 1554 |
| teardown | 3.8 | 0.2 | 3.6 | 2.7..4.5 | 5 % (3.6 / 68.9) | 5457 | 5457 |
| rest | 28.6 | 24.4 | 4.2 | 2.4..5.9 | 6 % (4.2 / 68.9) |  |  |
| wall | 223.4 | 154.5 | 68.9 | 59.7..78.1 | 100 % (68.9 / 68.9) |  |  |

Realloc moved (new pointer) in the last block: sefer large 144 of 145, small 1425 of 1554; mi large 143 of 145, small 1374 of 1554.

### phase (windows, 12 blocks, one whole run per block and arm: the repeat with the smallest wall time)

| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst_alloc | 418.6 | 382.0 | 36.6 | 20.4..52.8 | 46 % (36.6 / 78.9) | 17911350 | 17911350 |
| burst_free | 167.0 | 138.1 | 28.9 | 20.8..37.0 | 37 % (28.9 / 78.9) | 17911350 | 17911350 |
| large_alloc | 8.7 | 5.2 | 3.6 | 2.9..4.3 | 5 % (3.6 / 78.9) | 2545 | 2545 |
| large_free | 7.4 | 0.7 | 6.7 | 5.8..7.7 | 9 % (6.7 / 78.9) | 2537 | 2537 |
| large_realloc | 3.4 | 1.0 | 2.3 | 1.0..3.6 | 3 % (2.3 / 78.9) | 288 | 288 |
| small_realloc | 20.4 | 17.4 | 3.0 | 2.0..4.1 | 4 % (3.0 / 78.9) | 38921 | 38921 |
| teardown | 2.8 | 0.9 | 2.0 | 1.4..2.5 | 3 % (2.0 / 78.9) | 32372 | 32372 |
| rest | 134.8 | 139.0 | -4.2 | -13.6..5.2 | -5 % (-4.2 / 78.9) |  |  |
| wall | 763.2 | 684.3 | 78.9 | 45.6..112.2 | 100 % (78.9 / 78.9) |  |  |

Realloc moved (new pointer) in the last block: sefer large 288 of 288, small 33577 of 38921; mi large 286 of 288, small 30985 of 38921.

### large (windows, 12 blocks, one whole run per block and arm: the repeat with the smallest wall time)

| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst_alloc | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / 4.0) | 0 | 0 |
| burst_free | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / 4.0) | 0 | 0 |
| large_alloc | 9.0 | 11.1 | -2.1 | -3.1..-1.0 | -52 % (-2.1 / 4.0) | 4029 | 4029 |
| large_free | 4.9 | 0.8 | 4.2 | 3.8..4.6 | 105 % (4.2 / 4.0) | 3889 | 3889 |
| large_realloc | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / 4.0) | 0 | 0 |
| small_realloc | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / 4.0) | 0 | 0 |
| teardown | 1.5 | 0.0 | 1.5 | 1.2..1.7 | 37 % (1.5 / 4.0) | 187 | 187 |
| rest | 1.6 | 1.2 | 0.4 | 0.2..0.5 | 10 % (0.4 / 4.0) |  |  |
| wall | 17.0 | 13.1 | 4.0 | 2.4..5.5 | 100 % (4.0 / 4.0) |  |  |

Realloc moved (new pointer) in the last block: sefer large 0 of 0, small 0 of 0; mi large 0 of 0, small 0 of 0.

### wild (linux (WSL2), 12 blocks, one whole run per block and arm: the repeat with the smallest wall time)

| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst_alloc | 79.6 | 69.3 | 10.3 | 8.9..11.6 | 24 % (10.3 / 42.1) | 4181475 | 4181475 |
| burst_free | 61.3 | 20.1 | 41.2 | 39.8..42.7 | 98 % (41.2 / 42.1) | 4181475 | 4181475 |
| large_alloc | 10.3 | 33.0 | -22.7 | -24.4..-21.0 | -54 % (-22.7 / 42.1) | 4785 | 4785 |
| large_free | 4.6 | 1.0 | 3.6 | 3.2..3.9 | 8 % (3.6 / 42.1) | 4543 | 4543 |
| large_realloc | 3.1 | 0.5 | 2.6 | 2.5..2.8 | 6 % (2.6 / 42.1) | 145 | 145 |
| small_realloc | 0.5 | 0.3 | 0.2 | 0.2..0.2 | 0 % (0.2 / 42.1) | 1554 | 1554 |
| teardown | 2.0 | 0.2 | 1.8 | 1.6..2.0 | 4 % (1.8 / 42.1) | 5457 | 5457 |
| rest | 30.0 | 24.9 | 5.1 | 3.0..7.3 | 12 % (5.1 / 42.1) |  |  |
| wall | 191.4 | 149.3 | 42.1 | 37.3..47.0 | 100 % (42.1 / 42.1) |  |  |

Realloc moved (new pointer) in the last block: sefer large 144 of 145, small 1425 of 1554; mi large 143 of 145, small 1374 of 1554.

### phase (linux (WSL2), 12 blocks, one whole run per block and arm: the repeat with the smallest wall time)

| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst_alloc | 340.4 | 297.7 | 42.6 | 33.9..51.4 | 45 % (42.6 / 95.3) | 17911350 | 17911350 |
| burst_free | 120.7 | 90.6 | 30.2 | 27.0..33.3 | 32 % (30.2 / 95.3) | 17911350 | 17911350 |
| large_alloc | 8.0 | 26.0 | -18.0 | -19.9..-16.2 | -19 % (-18.0 / 95.3) | 2545 | 2545 |
| large_free | 7.8 | 0.8 | 7.0 | 6.5..7.5 | 7 % (7.0 / 95.3) | 2537 | 2537 |
| large_realloc | 2.8 | 2.0 | 0.8 | 0.6..1.0 | 1 % (0.8 / 95.3) | 288 | 288 |
| small_realloc | 16.8 | 12.0 | 4.7 | 3.7..5.7 | 5 % (4.7 / 95.3) | 38921 | 38921 |
| teardown | 1.7 | 0.9 | 0.8 | 0.4..1.2 | 1 % (0.8 / 95.3) | 32372 | 32372 |
| rest | 138.3 | 111.1 | 27.2 | 15.7..38.7 | 29 % (27.2 / 95.3) |  |  |
| wall | 636.5 | 541.2 | 95.3 | 73.8..116.9 | 100 % (95.3 / 95.3) |  |  |

Realloc moved (new pointer) in the last block: sefer large 288 of 288, small 33577 of 38921; mi large 286 of 288, small 30985 of 38921.

### large (linux (WSL2), 12 blocks, one whole run per block and arm: the repeat with the smallest wall time)

| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst_alloc | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / -103.7) | 0 | 0 |
| burst_free | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / -103.7) | 0 | 0 |
| large_alloc | 7.7 | 113.2 | -105.5 | -112.3..-98.7 | 102 % (-105.5 / -103.7) | 4029 | 4029 |
| large_free | 3.7 | 1.8 | 1.9 | 1.8..2.1 | -2 % (1.9 / -103.7) | 3889 | 3889 |
| large_realloc | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / -103.7) | 0 | 0 |
| small_realloc | 0.0 | 0.0 | 0.0 | 0.0..0.0 | 0 % (0.0 / -103.7) | 0 | 0 |
| teardown | 0.8 | 0.1 | 0.7 | 0.7..0.8 | -1 % (0.7 / -103.7) | 187 | 187 |
| rest | 2.5 | 3.4 | -0.8 | -1.1..-0.5 | 1 % (-0.8 / -103.7) |  |  |
| wall | 14.7 | 118.4 | -103.7 | -110.6..-96.7 | 100 % (-103.7 / -103.7) |  |  |

Realloc moved (new pointer) in the last block: sefer large 0 of 0, small 0 of 0; mi large 0 of 0, small 0 of 0.

### Small LIFO bursts: clean heap (scenario burst) against the same bursts inside wild and phase

ns per block = bucket ms / calls; ratio = exp(mean over blocks of ln(sefer / mi)) with a 95 % t-interval; one whole run per block and arm (the repeat with the smallest wall time).

| platform | context | sefer alloc ns/block | mi alloc ns/block | alloc sefer / mi (95 % CI) | sefer free ns/block | mi free ns/block | free sefer / mi (95 % CI) |
|---|---|---:|---:|---:|---:|---:|---:|
| windows | burst | 25.3 | 22.5 | 1.128 (1.056-1.205) | 10.7 | 8.6 | 1.251 (1.141-1.373) |
| windows | wild | 23.3 | 20.5 | 1.141 (1.077-1.209) | 17.7 | 7.5 | 2.373 (2.215-2.543) |
| windows | phase | 23.4 | 21.3 | 1.094 (1.056-1.133) | 9.3 | 7.7 | 1.206 (1.154-1.260) |
| linux (WSL2) | burst | 18.3 | 16.8 | 1.091 (1.056-1.127) | 7.4 | 5.8 | 1.263 (1.217-1.310) |
| linux (WSL2) | wild | 19.0 | 16.6 | 1.148 (1.128-1.169) | 14.7 | 4.8 | 3.055 (2.978-3.134) |
| linux (WSL2) | phase | 19.0 | 16.6 | 1.143 (1.114-1.172) | 6.7 | 5.1 | 1.332 (1.301-1.364) |

### Linux (WSL2) hardware counters, perf stat, user mode, median of 5 runs

| scenario | arm | cycles (M) | instructions (M) | IPC | L1D load misses (M) |
|---|---|---:|---:|---:|---:|
| burst | sefer | 304.5 | 985.4 | 3.236 | 2.9 |
| burst | mi | 272.5 | 939.4 | 3.447 | 4.3 |
| burst | sefer / mi | 1.117 | 1.049 | | 0.676 |
| wild | sefer | 712.6 | 1691.0 | 2.373 | 17.8 |
| wild | mi | 463.0 | 1389.0 | 3.000 | 10.3 |
| wild | sefer / mi | 1.539 | 1.217 | | 1.738 |
| phase | sefer | 2403.4 | 6115.3 | 2.544 | 61.9 |
| phase | mi | 2044.6 | 5869.8 | 2.871 | 41.8 |
| phase | sefer / mi | 1.175 | 1.042 | | 1.482 |

### Why the free lane misses (probe build, Windows, sefer, median of 5 runs)

lane frees = accepted + memo_key exits + seed exits (kind, bump and state exits were 0 in every run). Chain lookups after a memo miss: own_cache hits = TIER1 hits - accepted, own_cache misses (hash probe) = TIER1 misses.

| scenario | lane frees | accepted by the memo | memo_key exits | memo exits / lane frees | own_cache hits after a memo miss | own_cache misses (hash probe) | hash probes / lane frees |
|---|---:|---:|---:|---:|---:|---:|---:|
| burst | 3021541 | 3021539 | 2 | 0.0 % (2 / 3021541) | 1 | 1 | 0.0 % (1 / 3021541) |
| wild | 4285881 | 476262 | 3804795 | 88.8 % (3804795 / 4285881) | 2554708 | 1256610 | 29.3 % (1256610 / 4285881) |
| phase | 18293769 | 17737011 | 554122 | 3.0 % (554122 / 18293769) | 465362 | 131340 | 0.7 % (131340 / 18293769) |

### own_cache size, wild, Windows, sefer, median of 5 runs (throwaway probe builds)

| OWN_CACHE_SIZE | memo_key exits | hash probes (own_cache misses) | hash probes / lane frees |
|---:|---:|---:|---:|
| 16 | 3810251 | 1353336 | 31.6 % (1353336 / 4285881) |
| 64 | 3811751 | 209167 | 4.9 % (209167 / 4285881) |
