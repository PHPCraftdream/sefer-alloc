| cell | kind | A1 | A2 | B1 | B2 | C0 | T | B-A (4 cross pairs) | pass |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| `small_churn_16b` | control | 56696 | 56696 | 56582 | 56582 | 56696 | 10 | -114 | **NO** |
| `alloc_magazine_prefill_only_16b` | control | 55973 | 55973 | 55863 | 55863 | 55973 | 10 | -110 | **NO** |
| `alloc_magazine_hit_only_16b` | control | 56331 | 56331 | 56219 | 56219 | 56331 | 10 | -112 | **NO** |
| `large_alloc_free_cycle` | affected | 49907 | 49907 | 49581 | 49581 | 49907 | 10 | -326 | yes |
| `large_cache_hit_only_4mib` | R1 | 1253 | 1253 | 760 | 760 | 1253 | 10 | -493 | yes |
| `large_cache_eviction_hit_only_41x4mib` | R2 | 109373 | 109373 | 108192 | 108192 | 109373 | 10 | -1181 | yes |
| `large_cache_payload_hit_only_4mib` | R3 | 1281 | 1281 | 1400 | 1400 | 1281 | 10 | 119 | **NO** |
| `large_cache_free_slot_search_prefill_only` | affected | 63304 | 63304 | 63220 | 63220 | 63304 | 10 | -84 | yes |
| `large_cache_free_slot_search_cycle_only` | affected | 74416 | 74416 | 69894 | 69894 | 74416 | 10 | -4522 | yes |
| `realloc_grow` | affected | 582915 | 582915 | 582406 | 582406 | 582915 | 10 | -509 | yes |
| `cold_alloc_only_256x16b` | control | 115384 | 115384 | 115277 | 115277 | 115384 | 10 | -107 | **NO** |
| `cold_alloc_only_256x16b_2n` | control | 187506 | 187506 | 187399 | 187399 | 187506 | 10 | -107 | **NO** |
| `cold_alloc_only_256x16b_4n` | control | 330190 | 330190 | 330083 | 330083 | 330190 | 10 | -107 | **NO** |
| `churn_256b` | control | 56696 | 56696 | 56582 | 56582 | 56696 | 10 | -114 | **NO** |
| `mimalloc_small_churn_16b` | mimalloc | 16629 | 16629 | 16629 | 16629 | 16629 | 10 | 0 | yes |
| `mimalloc_small_churn_16b_2n` | mimalloc | 21557 | 21557 | 21557 | 21557 | 21557 | 10 | 0 | yes |
| `mimalloc_churn_256b` | mimalloc | 16130 | 16130 | 16130 | 16130 | 16130 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x16b` | mimalloc | 32325 | 32325 | 32325 | 32325 | 32325 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x16b_2n` | mimalloc | 58389 | 58389 | 58389 | 58389 | 58389 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x16b_4n` | mimalloc | 106653 | 106653 | 106653 | 106653 | 106653 | 10 | 0 | yes |
| `mimalloc_cold_alloc_only_256x16b` | mimalloc | 24584 | 24584 | 24584 | 24584 | 24584 | 10 | 0 | yes |
| `mimalloc_cold_alloc_only_256x16b_2n` | mimalloc | 42968 | 42968 | 42968 | 42968 | 42968 | 10 | 0 | yes |
| `mimalloc_cold_alloc_only_256x16b_4n` | mimalloc | 75872 | 75872 | 75872 | 75872 | 75872 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x64b` | mimalloc | 33329 | 33329 | 33329 | 33329 | 33329 | 10 | 0 | yes |
| `mimalloc_recycle_alloc_free_256x16b` | mimalloc | 53020 | 53020 | 53020 | 53020 | 53020 | 10 | 0 | yes |
| `mimalloc_recycle_alloc_free_256x64b` | mimalloc | 54024 | 54024 | 54024 | 54024 | 54024 | 10 | 0 | yes |
| `mimalloc_bootstrap_proxy` | mimalloc | 13050 | 13050 | 13050 | 13050 | 13050 | 10 | 0 | yes |

| n | metric | median A | median AA | median B | A/A noise (num/den) | limit | B vs A (num/den) | B<=A(1+noise) |
|---:|---|---:|---:|---:|---:|---:|---|---|
| 1 | nsPerOp | 169.8 | 202.7 | 166.4 | 19.38 % (32.9/169.8) | 5 % | -3.4/169.8 = -2.0 % | yes |
| 1 | p99Ns | 207 | 310 | 207 | 49.76 % (103/207) | 5 % | 0/207 = 0.0 % | yes |
| 4 | nsPerOp | 128.7 | 116.2 | 85.0 | 9.72 % (12.5/128.7) | 5 % | -43.7/128.7 = -33.9 % | yes |
| 4 | p99Ns | 517 | 517 | 207 | 0.00 % (0/517) | 5 % | -310/517 = -60.0 % | yes |
| 8 | nsPerOp | 143.0 | 146.7 | 81.0 | 2.60 % (3.7/143.0) | 5 % | -62.0/143.0 = -43.4 % | yes |
| 8 | p99Ns | 1447 | 1551 | 207 | 7.19 % (104/1447) | 5 % | -1240/1447 = -85.7 % | yes |

iai cells failing their frozen criterion: 8/27 = 29.63 %; native axes with A/A noise above 5 %: 4/6 = 66.67 %.
