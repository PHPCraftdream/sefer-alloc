### base: main 8462e42c

| scenario | sefer min ms | sefer_pool min ms | mi min ms | sefer/mi | sefer_pool/mi | (sefer − sefer_pool) / (sefer − mi) | seg_released sefer | seg_released sefer_pool | end RSS MiB sefer / pool / mi |
|---|---|---|---|---|---|---|---|---|---|
| grow64 | 1206 | 1064 | 618 | 1.951 | 1.721 | 0.241 | 268..268 | 0..0 | 29 / 1175 / 1031 |
| grow1k | 513 | 357 | 310 | 1.654 | 1.152 | 0.767 | 268..268 | 0..0 | 29 / 1175 / 1030 |
| regrow | 345 | 266 | 242 | 1.424 | 1.097 | 0.772 | 140..140 | 0..0 | 72 / 669 / 572 |
| growmix | 531 | 496 | 401 | 1.323 | 1.237 | 0.266 | 0..0 | 0..0 | 1332 / 1332 / 1137 |

Paired median of arm/mi (same block and order, n = 12 pairs per cell), base:

| scenario | sefer/mi | sefer_pool/mi | (sefer/mi − sefer_pool/mi) / (sefer/mi − 1) |
|---|---|---|---|
| grow64 | 1.833 | 1.590 | 0.292 |
| grow1k | 1.667 | 1.193 | 0.710 |
| regrow | 1.425 | 1.160 | 0.624 |
| growmix | 1.299 | 1.309 | -0.033 |

### ao1: main 8462e42c + AO-1 v2b carve

| scenario | sefer min ms | sefer_pool min ms | mi min ms | sefer/mi | sefer_pool/mi | (sefer − sefer_pool) / (sefer − mi) | seg_released sefer | seg_released sefer_pool | end RSS MiB sefer / pool / mi |
|---|---|---|---|---|---|---|---|---|---|
| grow64 | 761 | 650 | 632 | 1.204 | 1.028 | 0.861 | 268..268 | 0..0 | 29 / 1175 / 1031 |
| grow1k | 393 | 264 | 352 | 1.118 | 0.750 | 3.121 | 268..268 | 0..0 | 29 / 1175 / 1030 |
| regrow | 416 | 294 | 278 | 1.497 | 1.061 | 0.878 | 140..140 | 0..0 | 72 / 669 / 572 |
| growmix | 570 | 550 | 462 | 1.234 | 1.191 | 0.182 | 0..0 | 0..0 | 1332 / 1332 / 1137 |

Paired median of arm/mi (same block and order, n = 12 pairs per cell), ao1:

| scenario | sefer/mi | sefer_pool/mi | (sefer/mi − sefer_pool/mi) / (sefer/mi − 1) |
|---|---|---|---|
| grow64 | 1.285 | 1.034 | 0.882 |
| grow1k | 1.182 | 0.800 | 2.094 |
| regrow | 1.464 | 1.147 | 0.682 |
| growmix | 1.294 | 1.256 | 0.130 |

Path-activation oracle: in every run of the `sefer_pool` arm seg_released == 0, and config_conflicts == 0 in all 288 runs.
