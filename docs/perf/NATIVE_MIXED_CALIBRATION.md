# Native installed judge calibration — measurement only

Вердикт чувствительности: **FAIL / DO NOT JUDGE PRODUCT CANDIDATE**. Это не speedup allocator. Pinned source и raw41 processes: [calibration1](native_mixed/calibration1/identity.json). Один pilot, затем frozen repeats=241,10A/Ablocks×4processes. Installed Sefer production-only, никакого candidate A/B или mimalloc timing на этом этапе. Механизм установки подтверждён segments_reserved_total>0; refill/flush этим не доказаны.

Arithmetic means; Student-t95CI по inner−outer block means. Значимость meaningful effect=5/100 baseline full-workload mean; максимум halfwidth=2.5/100 того же mean. Все samples сохранены, retuning нет.

| Metric | Baseline ns/full workload round | Delta inner−outer | Paired95 CI | Halfwidth/baseline | Sensitivity | Position bias not detected |
|---|---:|---:|---|---:|---|---|
| workload_ns | 434128.2157676349 | -25744.10788381742 | -49261.30864205127 … -2226.9071255835734 | 0.054171094861112 | false | false |
| process_ns | 470188.8900414937 | -28447.863070539413 | -52248.85675942646 … -4646.86938165237 | 0.05062006821723633 | false | false |

workload_ns включает setup/teardown всех workload rounds, но не одну warmup run_workload. process_ns включает startup/warmup/output/exit и post-timing RSS probes; это другой denominator по содержанию. Проба не Criterion; Criterion mean отсутствует. Изменённый judge не исправляет задним числом предыдущий leaf INCONCLUSIVE. Tests/CI acceptance не выполнялась.
