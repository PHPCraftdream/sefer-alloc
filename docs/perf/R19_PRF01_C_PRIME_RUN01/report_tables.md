## Frozen criteria (derived from the judge input, asserted equal to the judge's saved rows)

| criterion | operands | value | frozen limit | pass |
|---|---|---|---|---|
| multiseg_cold_256k B/A | 454636/4026117 | 0.112922 | <= 0.25 | true |
| seg_cycle_decommit_256k B/A | 1118770/12368927 | 0.090450 | <= 0.25 | true |
| growth16 slope B1 (Ir4n-Ir2n)/(Ir2n-Irn) | 35782898/16687226 | 2.144329 | <= 2.5 | true |
| growth16 slope B2 (Ir4n-Ir2n)/(Ir2n-Irn) | 35782898/16687226 | 2.144329 | <= 2.5 | true |
| hot control small_churn_16b |B-A| | 569 vs T=10 (4 cross pairs) | 569 | <= T | false |
| hot control churn_256b |B-A| | 569 vs T=10 (4 cross pairs) | 569 | <= T | false |
| hot control churn_write_256b |B-A| | 569 vs T=10 (4 cross pairs) | 569 | <= T | false |
| hot control aligned_churn_640b_a128 |B-A| | 569 vs T=10 (4 cross pairs) | 569 | <= T | false |
| mimalloc cells delta (13 cells x 4 pairs) | 52/52 rows with delta 0 | 0 | == 0 | true |
| producer (IrB-IrA)/4096 remote frees | 36992/4096 | 9.03125 | <= 6 | false |
| size_of SmallSidecar B-A (bytes) | 42496-41984 | 512 | == 512 | true |

## Per-cell Ir (A1=A2=C0 and B1=B2 are bit-identical)

| cell | Ir A | Ir B | delta |
|---|---|---|---|
| aligned_churn_640b_a128 | 56702 | 57271 | 569 |
| churn_256b | 56696 | 57265 | 569 |
| churn_write_256b | 56900 | 57469 | 569 |
| growth16_2n | 55694709 | 32619322 | -23075387 |
| growth16_4n | 161673847 | 68402220 | -93271627 |
| growth16_n | 21579507 | 15932096 | -5647411 |
| mimalloc_bootstrap_proxy | 13050 | 13050 | 0 |
| mimalloc_churn_256b | 16130 | 16130 | 0 |
| mimalloc_cold_alloc_free_256x16b | 32325 | 32325 | 0 |
| mimalloc_cold_alloc_free_256x16b_2n | 58389 | 58389 | 0 |
| mimalloc_cold_alloc_free_256x16b_4n | 106653 | 106653 | 0 |
| mimalloc_cold_alloc_free_256x64b | 33329 | 33329 | 0 |
| mimalloc_cold_alloc_only_256x16b | 24584 | 24584 | 0 |
| mimalloc_cold_alloc_only_256x16b_2n | 42968 | 42968 | 0 |
| mimalloc_cold_alloc_only_256x16b_4n | 75872 | 75872 | 0 |
| mimalloc_recycle_alloc_free_256x16b | 53027 | 53027 | 0 |
| mimalloc_recycle_alloc_free_256x64b | 54031 | 54031 | 0 |
| mimalloc_small_churn_16b | 16629 | 16629 | 0 |
| mimalloc_small_churn_16b_2n | 21557 | 21557 | 0 |
| multiseg_cold_256k | 4026117 | 454636 | -3571481 |
| r19_prf01_joined_producer_same_word | 1515559 | 1552551 | 36992 |
| r19_prf01_publication | 4334263 | 352424 | -3981839 |
| seg_cycle_decommit_256k | 12368927 | 1118770 | -11250157 |
| small_churn_16b | 56708 | 57277 | 569 |

## Native axis (descriptive only; the judge stopped at the first p50Ns == 0: 13 of 108 rows have p50Ns == 0)

| n | arm | metric | processes | median | min | max | zero values |
|---|---|---|---|---|---|---|---|
| 1 | A | p50Ns | 9 | 103 | 0 | 104 | 4 |
| 1 | A | p99Ns | 9 | 104 | 104 | 208 | 0 |
| 1 | A | nsPerOp | 9 | 97.85107421875 | 78.784912109375 | 138.822265625 | 0 |
| 1 | AA | p50Ns | 9 | 0 | 0 | 103 | 7 |
| 1 | AA | p99Ns | 9 | 104 | 104 | 105 | 0 |
| 1 | AA | nsPerOp | 9 | 90.55859375 | 82.000732421875 | 96.892578125 | 0 |
| 1 | B | p50Ns | 9 | 103 | 0 | 104 | 2 |
| 1 | B | p99Ns | 9 | 104 | 104 | 208 | 0 |
| 1 | B | nsPerOp | 9 | 124.453125 | 88.786376953125 | 147.709716796875 | 0 |
| 2 | A | p50Ns | 9 | 415 | 208 | 622 | 0 |
| 2 | A | p99Ns | 9 | 520 | 312 | 4873 | 0 |
| 2 | A | nsPerOp | 9 | 246.4853515625 | 149.779296875 | 357.077392578125 | 0 |
| 2 | AA | p50Ns | 9 | 416 | 208 | 519 | 0 |
| 2 | AA | p99Ns | 9 | 520 | 313 | 1142 | 0 |
| 2 | AA | nsPerOp | 9 | 244.733642578125 | 142.555908203125 | 324.171630859375 | 0 |
| 2 | B | p50Ns | 9 | 415 | 208 | 519 | 0 |
| 2 | B | p99Ns | 9 | 519 | 312 | 934 | 0 |
| 2 | B | nsPerOp | 9 | 222.556640625 | 142.73388671875 | 279.114013671875 | 0 |
| 4 | A | p50Ns | 9 | 937 | 415 | 1246 | 0 |
| 4 | A | p99Ns | 9 | 5711 | 2700 | 7268 | 0 |
| 4 | A | nsPerOp | 9 | 365.016845703125 | 206.355224609375 | 496.55859375 | 0 |
| 4 | AA | p50Ns | 9 | 1143 | 830 | 1350 | 0 |
| 4 | AA | p99Ns | 9 | 5522 | 4159 | 7159 | 0 |
| 4 | AA | nsPerOp | 9 | 425.04296875 | 328.25 | 474.363037109375 | 0 |
| 4 | B | p50Ns | 9 | 935 | 519 | 1664 | 0 |
| 4 | B | p99Ns | 9 | 5191 | 2492 | 9045 | 0 |
| 4 | B | nsPerOp | 9 | 367.339599609375 | 220.4931640625 | 606.775146484375 | 0 |
| 8 | A | p50Ns | 9 | 1973 | 415 | 3531 | 0 |
| 8 | A | p99Ns | 9 | 26173 | 6543 | 43722 | 0 |
| 8 | A | nsPerOp | 9 | 618.604736328125 | 280.290283203125 | 1019.32861328125 | 0 |
| 8 | AA | p50Ns | 9 | 2596 | 623 | 3220 | 0 |
| 8 | AA | p99Ns | 9 | 22532 | 5920 | 31986 | 0 |
| 8 | AA | nsPerOp | 9 | 697.696044921875 | 309.482421875 | 793.447998046875 | 0 |
| 8 | B | p50Ns | 9 | 3011 | 1350 | 3947 | 0 |
| 8 | B | p99Ns | 9 | 24403 | 11624 | 43306 | 0 |
| 8 | B | nsPerOp | 9 | 711.38818359375 | 406.658935546875 | 1014.293701171875 | 0 |
