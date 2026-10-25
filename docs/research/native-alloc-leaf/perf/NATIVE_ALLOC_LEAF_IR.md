# Native alloc leaf — WSL instruction gate (не native speedup)

Production-only direct GlobalAlloc facade, baseline f80 и candidate patch из [identity](native_alloc_leaf/ir1/identity.json). Не installed allocator. 120 profiles; сырые totals: [samples.json](native_alloc_leaf/ir1/samples.json), per-profile receipts, [CSV](native_alloc_leaf/ir1/summary.csv). Marginal Ir = (total2−total1)/operation_delta. Pair — весь process с harness/setup/teardown, не сумма отдельно собранных alloc/free windows. N/2N — один и тот же ELF в каждом arm; bootstrap не объявляется per-op cost.

| Mode | Axis | Before Ir/op | After Ir/op | Delta Ir/op |
|---|---|---:|---:|---:|
| rt | alloc | 55.51408 | 51.59112 | -3.9229600000000033 |
| rt | free | 87.35896 | 87.35896 | 0 |
| rt | pair | 187.87354 | 183.95114 | -3.922399999999982 |
| 16 | alloc | 22 | 20 | -2 |
| 16 | free | 52 | 52 | 0 |
| 16 | pair | 96.00032 | 94.00088 | -1.999440000000007 |
| 64 | alloc | 22 | 20 | -2 |
| 64 | free | 52 | 52 | 0 |
| 64 | pair | 96.00032 | 94.00088 | -1.999440000000007 |
| bulk64 | alloc | 96.734375 | 93.15625 | -3.578125 |
| bulk64 | free | 124.8125 | 124.8125 | 0 |
| bulk64 | pair | 238.65625 | 235.078125 | -3.578125 |
| bulk16k | alloc | 168.546875 | 166.65625 | -1.890625 |
| bulk16k | free | 235.625 | 235.625 | 0 |
| bulk16k | pair | 421.28125 | 419.390625 | -1.890625 |

Все free marginal totals равны baseline; alloc и full pair ниже во всех пяти workloads. Строгой причинной атрибуции каждого instruction нет; compiler/platform отличаются от Windows. Незначительные mimalloc pair deltas сохранены в CSV, не скрыты. [Callgraph activation](native_alloc_leaf/ir1/activation.json) хранит частоты refill/bound continuation/flush по каждому Sefer arm; raw call-edge excerpts рядом. Отсутствие helper символа при inlining не доказывает отсутствие механизма.

Вердикт: **survivor для native gate**, не GO FOR PROMOTION. Следующий этап — balanced native before/candidate/mimalloc и A/A; canonical churn16 остаётся обязательным control при выживании. Tests/CI acceptance отсутствует.
