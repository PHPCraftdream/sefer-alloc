# Installed mixed workload — WSL Ir attribution, не native verdict

Pinned f80 production; actual installed global allocator, общий churn16/64/256/1024+Vec workload. Native judge calibration FAILED; product native A/B не выполнялся. Raw4profiles/sourceidentity: [attribution1](native_mixed/attribution1/identity.json). Повторы120/241; один warmup полный workload вне timing, но внутри collected process. Marginal Ir=(total241−total120)/121fullrounds. Одна fullround=run_workload с8внутренними rounds и полным setup/teardown. Это не Ir/alloc и не ns/pair.

| Arm | Bucket (self: — exclusive; events — inclusive) | Ir120 | Ir241 | Marginal Ir/full round | Calls120 | Calls241 |
|---|---|---:|---:|---:|---:|---:|
| sefer | TOTAL | 794345187 | 1588199760 | 6560781.5950413225 | NA | NA |
| sefer | self:rust_dealloc | 389202416 | 778404240 | 3216544 | 4961007 | 9922007 |
| sefer | self:workload_container | 277866548 | 555733126 | 2296418 | 121 | 242 |
| sefer | refill | 69199606 | 138310038 | 571160.5950413223 | 59059 | 118107 |
| sefer | flush | 55174886 | 110341206 | 455920 | 118107 | 236203 |
| sefer | drain | 58755223 | 117591303 | 486248.5950413223 | 59059 | 118107 |
| sefer | discovery | 88273 | 88273 | 0 | 72 | 72 |
| mimalloc | TOTAL | 1460992354 | 2914255949 | 12010442.933884298 | NA | NA |
| mimalloc | self:workload_container | 140465754 | 280931508 | 1160874 | 121 | 242 |

Event inclusive costs могут быть вложены, их нельзя складывать. Отсутствие символа не доказывает отсутствие пути: inlining/tailcalls требуют проверки. Self-cost partition каждого profile проверен на точное равенство PROGRAM TOTALS; top function self costs в function_profiles.json.gz. Ir share не является wall-clock share. Old B4 event mix/estimates не переносятся на этот workload. Fresh source/codegen reading требуется до выбора изменения.

## Решение и границы

Full marginal instruction ratio mimalloc/Sefer=1.8306420904121945; это Ir ratio, НЕ native speed ratio. Доли buckets (каждая со своим numerator/denominator):

- self:rust_dealloc: 3216544/6560781.5950413225 = 0.490268415950789 от полного marginal Ir.
- self:workload_container: 2296418/6560781.5950413225 = 0.35002201593414517 от полного marginal Ir.
- refill: 571160.5950413223/6560781.5950413225 = 0.0870567914458559 от полного marginal Ir.
- flush: 455920/6560781.5950413225 = 0.06949172036828463 от полного marginal Ir.
- drain: 486248.5950413223/6560781.5950413225 = 0.07411443103194165 от полного marginal Ir.
- discovery: 0/6560781.5950413225 = 0 от полного marginal Ir.

Крупнейшие подтверждённые self-контейнеры — dealloc и run_workload; последний содержит как алгоритм workload, так и инлайненный allocator, и не является pure alloc cost. Одинаковый source workload не доказывает одинаковый машинный harness floor между arms. Refill/flush существенны, но вывод о доминировании дорогих переходов именно здесь не подтверждён. Discovery72calls в обоих N — bootstrap, не marginal victim. Старые S1/R3 выводы не переоткрываются.

Отдельно прочитан mi_bin exactELF: оптимизированное leaf-тело, не доказательство C -O0; CFLAGS overrides в build output отсутствуют. Значительные aligned/generic mimalloc call counts требуют отдельной layer/regime проверки перед любым обобщением, а не изменения чужого allocator для удобного сравнения.

**Вердикт: ATTRIBUTION COMPLETE / PRODUCT CANDIDATE NOT OPENED.** Пригодного native judge для meaningful5/100 baseline нет; новый causal native bottleneck и новый безопасный механизм его устранения не установлены. Native A/B, broad/canonical/MT, локальные tests/Miri/Loom не запускались. Это bounded stop, не опровержение возможности ускорения. Raw source paths нормализованы в архивных profiles; receipt сохраняет hash оригинала.

