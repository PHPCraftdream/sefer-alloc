# R19-PRF02-R REGATE — perf(runtime)-кандидат B: GO

Identity SHA256: 76b4f9d0ed144b1803409f6ecb4f01c52bb3346b51a5a336cd5a91f4052b849c
Preregistration SHA256: d1e951abb5a65b97e332dba38a5db124177979a8a18e2314e2f0cefde04f67ba
Environment: RUSTC_WRAPPER=;CARGO_BUILD_JOBS=3;CARGO_TARGET_DIR=<repo>/target; activation R19_PRF02R_ARM explicitly A/B.
Features: {"pure":"production internals","instrumented":"production internals bench-internals","activation":"production internals bench-internals medium-classes"}

Large: переписана после наблюдения, доказательством не является.
Thresholds remain exactly as preregistered; no percentages.

## Activation

| Arm | Repeat | Status | Observed counts / denominator or failure |
|---|---|---|---|
| A | 1 | PASS | small=32/16; Large=3/1; inplace=1/1; alloc=0/1; move=2/1; promotion=3/1; tail=1/1 |
| A | 2 | PASS | small=32/16; Large=3/1; inplace=1/1; alloc=0/1; move=2/1; promotion=3/1; tail=1/1 |
| B | 1 | PASS | small=16/16; Large=2/1; inplace=1/1; alloc=0/1; move=1/1; promotion=2/1; tail=0/1 |
| B | 2 | PASS | small=16/16; Large=2/1; inplace=1/1; alloc=0/1; move=1/1; promotion=2/1; tail=0/1 |

## Arithmetic and controls

Assertions enforced when data is available: every numerator is a safe integer, denominator a positive safe integer, and value * denominator equals numerator. Both repeats must match every raw count, including diagnostics. Gate checks: pure push <= -6; instrumented push <= -8; instrumented churn numerator <= -8*64; raw churn N/2N <= +10 on both axes; mimalloc N/2N delta = 0; magazine numerator delta within +/-10; Large one-sided delta <= +10. Free-average and body diagnostic have no added threshold.

All arithmetic, repeat and gate assertions passed.

See CSV for raw counts, every derived numerator/denominator, B-A deltas and receipt provenance. Partial data on failure is not a GO.

| Axis | Arm | Repeat | Metric | Numerator | Denominator | Ir |
|---|---|---|---|---:|---:|---:|
| pure | A | 1 | dealloc_prealloc_only_16b | 63053 | 1 | 63053 |
| pure | A | 1 | dealloc_free_only_16b_n8 | 63609 | 1 | 63609 |
| pure | A | 1 | dealloc_free_only_16b_n9 | 63678 | 1 | 63678 |
| pure | A | 1 | dealloc_free_only_16b_n16 | 64161 | 1 | 64161 |
| pure | A | 1 | small_churn_16b | 56137 | 1 | 56137 |
| pure | A | 1 | small_churn_16b_2n | 61513 | 1 | 61513 |
| pure | A | 1 | alloc_magazine_prefill_only_16b | 56634 | 1 | 56634 |
| pure | A | 1 | alloc_magazine_hit_only_16b | 56992 | 1 | 56992 |
| pure | A | 1 | large_alloc_free_cycle | 49754 | 1 | 49754 |
| pure | A | 1 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| pure | A | 1 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| pure | A | 1 | push | 69 | 1 | 69 |
| pure | A | 1 | churn/pair | 5376 | 64 | 84 |
| pure | A | 1 | free-average | 1108 | 16 | 69.25 |
| pure | A | 1 | magazine numerator | 358 | 1 | 358 |
| pure | A | 2 | dealloc_prealloc_only_16b | 63053 | 1 | 63053 |
| pure | A | 2 | dealloc_free_only_16b_n8 | 63609 | 1 | 63609 |
| pure | A | 2 | dealloc_free_only_16b_n9 | 63678 | 1 | 63678 |
| pure | A | 2 | dealloc_free_only_16b_n16 | 64161 | 1 | 64161 |
| pure | A | 2 | small_churn_16b | 56137 | 1 | 56137 |
| pure | A | 2 | small_churn_16b_2n | 61513 | 1 | 61513 |
| pure | A | 2 | alloc_magazine_prefill_only_16b | 56634 | 1 | 56634 |
| pure | A | 2 | alloc_magazine_hit_only_16b | 56992 | 1 | 56992 |
| pure | A | 2 | large_alloc_free_cycle | 49754 | 1 | 49754 |
| pure | A | 2 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| pure | A | 2 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| pure | A | 2 | push | 69 | 1 | 69 |
| pure | A | 2 | churn/pair | 5376 | 64 | 84 |
| pure | A | 2 | free-average | 1108 | 16 | 69.25 |
| pure | A | 2 | magazine numerator | 358 | 1 | 358 |
| pure | B | 1 | dealloc_prealloc_only_16b | 63053 | 1 | 63053 |
| pure | B | 1 | dealloc_free_only_16b_n8 | 63472 | 1 | 63472 |
| pure | B | 1 | dealloc_free_only_16b_n9 | 63524 | 1 | 63524 |
| pure | B | 1 | dealloc_free_only_16b_n16 | 63888 | 1 | 63888 |
| pure | B | 1 | small_churn_16b | 55241 | 1 | 55241 |
| pure | B | 1 | small_churn_16b_2n | 59721 | 1 | 59721 |
| pure | B | 1 | alloc_magazine_prefill_only_16b | 55554 | 1 | 55554 |
| pure | B | 1 | alloc_magazine_hit_only_16b | 55911 | 1 | 55911 |
| pure | B | 1 | large_alloc_free_cycle | 49736 | 1 | 49736 |
| pure | B | 1 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| pure | B | 1 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| pure | B | 1 | push | 52 | 1 | 52 |
| pure | B | 1 | churn/pair | 4480 | 64 | 70 |
| pure | B | 1 | free-average | 835 | 16 | 52.1875 |
| pure | B | 1 | magazine numerator | 357 | 1 | 357 |
| pure | B | 2 | dealloc_prealloc_only_16b | 63053 | 1 | 63053 |
| pure | B | 2 | dealloc_free_only_16b_n8 | 63472 | 1 | 63472 |
| pure | B | 2 | dealloc_free_only_16b_n9 | 63524 | 1 | 63524 |
| pure | B | 2 | dealloc_free_only_16b_n16 | 63888 | 1 | 63888 |
| pure | B | 2 | small_churn_16b | 55241 | 1 | 55241 |
| pure | B | 2 | small_churn_16b_2n | 59721 | 1 | 59721 |
| pure | B | 2 | alloc_magazine_prefill_only_16b | 55554 | 1 | 55554 |
| pure | B | 2 | alloc_magazine_hit_only_16b | 55911 | 1 | 55911 |
| pure | B | 2 | large_alloc_free_cycle | 49736 | 1 | 49736 |
| pure | B | 2 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| pure | B | 2 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| pure | B | 2 | push | 52 | 1 | 52 |
| pure | B | 2 | churn/pair | 4480 | 64 | 70 |
| pure | B | 2 | free-average | 835 | 16 | 52.1875 |
| pure | B | 2 | magazine numerator | 357 | 1 | 357 |
| pure | B-A | 1 | dealloc_prealloc_only_16b | 0 | 1 | 0 |
| pure | B-A | 1 | dealloc_free_only_16b_n8 | -137 | 1 | -137 |
| pure | B-A | 1 | dealloc_free_only_16b_n9 | -154 | 1 | -154 |
| pure | B-A | 1 | dealloc_free_only_16b_n16 | -273 | 1 | -273 |
| pure | B-A | 1 | small_churn_16b | -896 | 1 | -896 |
| pure | B-A | 1 | small_churn_16b_2n | -1792 | 1 | -1792 |
| pure | B-A | 1 | alloc_magazine_prefill_only_16b | -1080 | 1 | -1080 |
| pure | B-A | 1 | alloc_magazine_hit_only_16b | -1081 | 1 | -1081 |
| pure | B-A | 1 | large_alloc_free_cycle | -18 | 1 | -18 |
| pure | B-A | 1 | mimalloc_small_churn_16b | 0 | 1 | 0 |
| pure | B-A | 1 | mimalloc_small_churn_16b_2n | 0 | 1 | 0 |
| pure | B-A | 1 | push | -17 | 1 | -17 |
| pure | B-A | 1 | churn/pair | -896 | 64 | -14 |
| pure | B-A | 1 | free-average | -273 | 16 | -17.0625 |
| pure | B-A | 1 | magazine numerator | -1 | 1 | -1 |
| pure | B-A | 2 | dealloc_prealloc_only_16b | 0 | 1 | 0 |
| pure | B-A | 2 | dealloc_free_only_16b_n8 | -137 | 1 | -137 |
| pure | B-A | 2 | dealloc_free_only_16b_n9 | -154 | 1 | -154 |
| pure | B-A | 2 | dealloc_free_only_16b_n16 | -273 | 1 | -273 |
| pure | B-A | 2 | small_churn_16b | -896 | 1 | -896 |
| pure | B-A | 2 | small_churn_16b_2n | -1792 | 1 | -1792 |
| pure | B-A | 2 | alloc_magazine_prefill_only_16b | -1080 | 1 | -1080 |
| pure | B-A | 2 | alloc_magazine_hit_only_16b | -1081 | 1 | -1081 |
| pure | B-A | 2 | large_alloc_free_cycle | -18 | 1 | -18 |
| pure | B-A | 2 | mimalloc_small_churn_16b | 0 | 1 | 0 |
| pure | B-A | 2 | mimalloc_small_churn_16b_2n | 0 | 1 | 0 |
| pure | B-A | 2 | push | -17 | 1 | -17 |
| pure | B-A | 2 | churn/pair | -896 | 64 | -14 |
| pure | B-A | 2 | free-average | -273 | 16 | -17.0625 |
| pure | B-A | 2 | magazine numerator | -1 | 1 | -1 |
| instrumented | A | 1 | small_churn_16b | 57723 | 1 | 57723 |
| instrumented | A | 1 | small_churn_16b_2n | 64059 | 1 | 64059 |
| instrumented | A | 1 | dealloc_prealloc_only_16b | 63472 | 1 | 63472 |
| instrumented | A | 1 | dealloc_free_only_16b_n8 | 64075 | 1 | 64075 |
| instrumented | A | 1 | dealloc_free_only_16b_n9 | 64149 | 1 | 64149 |
| instrumented | A | 1 | dealloc_free_only_16b_n16 | 64667 | 1 | 64667 |
| instrumented | A | 1 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| instrumented | A | 1 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| instrumented | A | 1 | large_alloc_free_cycle | 49830 | 1 | 49830 |
| instrumented | A | 1 | alloc_magazine_hit_only_16b | 57456 | 1 | 57456 |
| instrumented | A | 1 | alloc_magazine_prefill_only_16b | 57098 | 1 | 57098 |
| instrumented | A | 1 | dealloc_own_thread_body_only_16b | 70649 | 1 | 70649 |
| instrumented | A | 1 | dealloc_segment_base_of_ptr_probe_only_16b | 64045 | 1 | 64045 |
| instrumented | A | 1 | push | 74 | 1 | 74 |
| instrumented | A | 1 | churn/pair | 6336 | 64 | 99 |
| instrumented | A | 1 | free-average | 1195 | 16 | 74.6875 |
| instrumented | A | 1 | magazine numerator | 358 | 1 | 358 |
| instrumented | A | 1 | body diagnostic | 6604 | 64 | 103.1875 |
| instrumented | A | 2 | small_churn_16b | 57723 | 1 | 57723 |
| instrumented | A | 2 | small_churn_16b_2n | 64059 | 1 | 64059 |
| instrumented | A | 2 | dealloc_prealloc_only_16b | 63472 | 1 | 63472 |
| instrumented | A | 2 | dealloc_free_only_16b_n8 | 64075 | 1 | 64075 |
| instrumented | A | 2 | dealloc_free_only_16b_n9 | 64149 | 1 | 64149 |
| instrumented | A | 2 | dealloc_free_only_16b_n16 | 64667 | 1 | 64667 |
| instrumented | A | 2 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| instrumented | A | 2 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| instrumented | A | 2 | large_alloc_free_cycle | 49830 | 1 | 49830 |
| instrumented | A | 2 | alloc_magazine_hit_only_16b | 57456 | 1 | 57456 |
| instrumented | A | 2 | alloc_magazine_prefill_only_16b | 57098 | 1 | 57098 |
| instrumented | A | 2 | dealloc_own_thread_body_only_16b | 70649 | 1 | 70649 |
| instrumented | A | 2 | dealloc_segment_base_of_ptr_probe_only_16b | 64045 | 1 | 64045 |
| instrumented | A | 2 | push | 74 | 1 | 74 |
| instrumented | A | 2 | churn/pair | 6336 | 64 | 99 |
| instrumented | A | 2 | free-average | 1195 | 16 | 74.6875 |
| instrumented | A | 2 | magazine numerator | 358 | 1 | 358 |
| instrumented | A | 2 | body diagnostic | 6604 | 64 | 103.1875 |
| instrumented | B | 1 | small_churn_16b | 56494 | 1 | 56494 |
| instrumented | B | 1 | small_churn_16b_2n | 61614 | 1 | 61614 |
| instrumented | B | 1 | dealloc_prealloc_only_16b | 63472 | 1 | 63472 |
| instrumented | B | 1 | dealloc_free_only_16b_n8 | 63909 | 1 | 63909 |
| instrumented | B | 1 | dealloc_free_only_16b_n9 | 63962 | 1 | 63962 |
| instrumented | B | 1 | dealloc_free_only_16b_n16 | 64333 | 1 | 64333 |
| instrumented | B | 1 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| instrumented | B | 1 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| instrumented | B | 1 | large_alloc_free_cycle | 49809 | 1 | 49809 |
| instrumented | B | 1 | alloc_magazine_hit_only_16b | 56127 | 1 | 56127 |
| instrumented | B | 1 | alloc_magazine_prefill_only_16b | 55769 | 1 | 55769 |
| instrumented | B | 1 | dealloc_own_thread_body_only_16b | 70649 | 1 | 70649 |
| instrumented | B | 1 | dealloc_segment_base_of_ptr_probe_only_16b | 64045 | 1 | 64045 |
| instrumented | B | 1 | push | 53 | 1 | 53 |
| instrumented | B | 1 | churn/pair | 5120 | 64 | 80 |
| instrumented | B | 1 | free-average | 861 | 16 | 53.8125 |
| instrumented | B | 1 | magazine numerator | 358 | 1 | 358 |
| instrumented | B | 1 | body diagnostic | 6604 | 64 | 103.1875 |
| instrumented | B | 2 | small_churn_16b | 56494 | 1 | 56494 |
| instrumented | B | 2 | small_churn_16b_2n | 61614 | 1 | 61614 |
| instrumented | B | 2 | dealloc_prealloc_only_16b | 63472 | 1 | 63472 |
| instrumented | B | 2 | dealloc_free_only_16b_n8 | 63909 | 1 | 63909 |
| instrumented | B | 2 | dealloc_free_only_16b_n9 | 63962 | 1 | 63962 |
| instrumented | B | 2 | dealloc_free_only_16b_n16 | 64333 | 1 | 64333 |
| instrumented | B | 2 | mimalloc_small_churn_16b | 16629 | 1 | 16629 |
| instrumented | B | 2 | mimalloc_small_churn_16b_2n | 21557 | 1 | 21557 |
| instrumented | B | 2 | large_alloc_free_cycle | 49809 | 1 | 49809 |
| instrumented | B | 2 | alloc_magazine_hit_only_16b | 56127 | 1 | 56127 |
| instrumented | B | 2 | alloc_magazine_prefill_only_16b | 55769 | 1 | 55769 |
| instrumented | B | 2 | dealloc_own_thread_body_only_16b | 70649 | 1 | 70649 |
| instrumented | B | 2 | dealloc_segment_base_of_ptr_probe_only_16b | 64045 | 1 | 64045 |
| instrumented | B | 2 | push | 53 | 1 | 53 |
| instrumented | B | 2 | churn/pair | 5120 | 64 | 80 |
| instrumented | B | 2 | free-average | 861 | 16 | 53.8125 |
| instrumented | B | 2 | magazine numerator | 358 | 1 | 358 |
| instrumented | B | 2 | body diagnostic | 6604 | 64 | 103.1875 |
| instrumented | B-A | 1 | small_churn_16b | -1229 | 1 | -1229 |
| instrumented | B-A | 1 | small_churn_16b_2n | -2445 | 1 | -2445 |
| instrumented | B-A | 1 | dealloc_prealloc_only_16b | 0 | 1 | 0 |
| instrumented | B-A | 1 | dealloc_free_only_16b_n8 | -166 | 1 | -166 |
| instrumented | B-A | 1 | dealloc_free_only_16b_n9 | -187 | 1 | -187 |
| instrumented | B-A | 1 | dealloc_free_only_16b_n16 | -334 | 1 | -334 |
| instrumented | B-A | 1 | mimalloc_small_churn_16b | 0 | 1 | 0 |
| instrumented | B-A | 1 | mimalloc_small_churn_16b_2n | 0 | 1 | 0 |
| instrumented | B-A | 1 | large_alloc_free_cycle | -21 | 1 | -21 |
| instrumented | B-A | 1 | alloc_magazine_hit_only_16b | -1329 | 1 | -1329 |
| instrumented | B-A | 1 | alloc_magazine_prefill_only_16b | -1329 | 1 | -1329 |
| instrumented | B-A | 1 | dealloc_own_thread_body_only_16b | 0 | 1 | 0 |
| instrumented | B-A | 1 | dealloc_segment_base_of_ptr_probe_only_16b | 0 | 1 | 0 |
| instrumented | B-A | 1 | push | -21 | 1 | -21 |
| instrumented | B-A | 1 | churn/pair | -1216 | 64 | -19 |
| instrumented | B-A | 1 | free-average | -334 | 16 | -20.875 |
| instrumented | B-A | 1 | magazine numerator | 0 | 1 | 0 |
| instrumented | B-A | 1 | body diagnostic | 0 | 64 | 0 |
| instrumented | B-A | 2 | small_churn_16b | -1229 | 1 | -1229 |
| instrumented | B-A | 2 | small_churn_16b_2n | -2445 | 1 | -2445 |
| instrumented | B-A | 2 | dealloc_prealloc_only_16b | 0 | 1 | 0 |
| instrumented | B-A | 2 | dealloc_free_only_16b_n8 | -166 | 1 | -166 |
| instrumented | B-A | 2 | dealloc_free_only_16b_n9 | -187 | 1 | -187 |
| instrumented | B-A | 2 | dealloc_free_only_16b_n16 | -334 | 1 | -334 |
| instrumented | B-A | 2 | mimalloc_small_churn_16b | 0 | 1 | 0 |
| instrumented | B-A | 2 | mimalloc_small_churn_16b_2n | 0 | 1 | 0 |
| instrumented | B-A | 2 | large_alloc_free_cycle | -21 | 1 | -21 |
| instrumented | B-A | 2 | alloc_magazine_hit_only_16b | -1329 | 1 | -1329 |
| instrumented | B-A | 2 | alloc_magazine_prefill_only_16b | -1329 | 1 | -1329 |
| instrumented | B-A | 2 | dealloc_own_thread_body_only_16b | 0 | 1 | 0 |
| instrumented | B-A | 2 | dealloc_segment_base_of_ptr_probe_only_16b | 0 | 1 | 0 |
| instrumented | B-A | 2 | push | -21 | 1 | -21 |
| instrumented | B-A | 2 | churn/pair | -1216 | 64 | -19 |
| instrumented | B-A | 2 | free-average | -334 | 16 | -20.875 |
| instrumented | B-A | 2 | magazine numerator | 0 | 1 | 0 |
| instrumented | B-A | 2 | body diagnostic | 0 | 64 | 0 |
