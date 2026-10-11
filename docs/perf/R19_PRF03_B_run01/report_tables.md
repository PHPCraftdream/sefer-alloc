| cell | kind | A1 | A2 | B1 | B2 | C0 | T | B-A (4 cross pairs) | pass |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| `small_churn_16b` | control | 56640 | 56640 | 56696 | 56696 | 56640 | 10 | 56 | **NO** |
| `alloc_magazine_prefill_only_16b` | control | 55916 | 55916 | 56029 | 56029 | 55916 | 10 | 113 | **NO** |
| `alloc_magazine_hit_only_16b` | control | 56274 | 56274 | 56374 | 56374 | 56274 | 10 | 100 | **NO** |
| `large_alloc_free_cycle` | affected | 49920 | 49920 | 49782 | 49782 | 49920 | 10 | -138 | yes |
| `large_cache_hit_only_4mib` | R1 | 1259 | 1259 | 824 | 824 | 1259 | 10 | -435 | yes |
| `large_cache_eviction_hit_only_41x4mib` | R2 | 109619 | 109619 | 109055 | 109055 | 109631 | 12 | -564 | yes |
| `large_cache_payload_hit_only_4mib` | R3 | 1287 | 1287 | 1438 | 1438 | 1299 | 12 | 151 | **NO** |
| `large_cache_free_slot_search_prefill_only` | affected | 63358 | 63358 | 63572 | 63572 | 63358 | 10 | 214 | yes |
| `large_cache_free_slot_search_cycle_only` | affected | 74518 | 74518 | 70737 | 70737 | 74518 | 10 | -3781 | yes |
| `realloc_grow` | affected | 582505 | 582505 | 582232 | 582232 | 582505 | 10 | -273 | yes |
| `cold_alloc_only_256x16b` | control | 114383 | 114383 | 114438 | 114438 | 114383 | 10 | 55 | **NO** |
| `cold_alloc_only_256x16b_2n` | control | 185497 | 185497 | 185552 | 185552 | 185497 | 10 | 55 | **NO** |
| `cold_alloc_only_256x16b_4n` | control | 326165 | 326165 | 326220 | 326220 | 326165 | 10 | 55 | **NO** |
| `churn_256b` | control | 56640 | 56640 | 56708 | 56708 | 56640 | 10 | 68 | **NO** |
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
| `mimalloc_recycle_alloc_free_256x16b` | mimalloc | 53020 | 53020 | 53027 | 53027 | 53020 | 10 | 7 | **NO** |
| `mimalloc_recycle_alloc_free_256x64b` | mimalloc | 54024 | 54024 | 54031 | 54031 | 54024 | 10 | 7 | **NO** |
| `mimalloc_bootstrap_proxy` | mimalloc | 13050 | 13050 | 13050 | 13050 | 13050 | 10 | 0 | yes |

| n | metric | median A | median AA | median B | A/A noise (num/den) | limit | B vs A (num/den) | B<=A(1+noise) |
|---:|---|---:|---:|---:|---:|---:|---|---|
| 1 | nsPerOp | 177.3 | 174.3 | 144.0 | 1.68 % (3.0/177.3) | 5 % | -33.3/177.3 = -18.8 % | yes |
| 1 | p99Ns | 310 | 309 | 207 | 0.32 % (1/310) | 5 % | -103/310 = -33.2 % | yes |
| 4 | nsPerOp | 155.7 | 138.2 | 115.0 | 11.25 % (17.5/155.7) | 5 % | -40.7/155.7 = -26.2 % | yes |
| 4 | p99Ns | 721 | 720 | 309 | 0.14 % (1/721) | 5 % | -412/721 = -57.1 % | yes |
| 8 | nsPerOp | 128.2 | 139.2 | 104.5 | 8.60 % (11.0/128.2) | 5 % | -23.7/128.2 = -18.5 % | yes |
| 8 | p99Ns | 1133 | 1236 | 309 | 9.09 % (103/1133) | 5 % | -824/1133 = -72.7 % | yes |

iai cells failing their frozen criterion: 10/27 = 37.04 %; native axes with A/A noise above 5 %: 3/6 = 50.00 %.
