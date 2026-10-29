# Native mixed judge — calibration preregistration

База f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Product source и production composition не меняются. Предыдущие B/leaf не являются candidate arm этого этапа.

## Workload и слой

Настоящий installed global allocator: два arm-файла включают один `paired_ab_workload.rs` и один `native_mixed_driver.rs`. Прежний synthetic churn-write16/64/256/1024 + Vec growth,8 rounds на run_workload. Benchmark-only nonnull assertions запрещают молча измерить сокращённую работу при allocation failure. Одна полная warmup run_workload, затем repeats полных run_workload с setup/teardown; post-timing stats/RSS/commit не входят в workload_ns. Полный process_ns сохраняется отдельно. Не Criterion и не реальное приложение.

Production timing не включает alloc-stats/bench-internals. segments_reserved_total после timing подтверждает installed allocator: Sefer >0, mimalloc 0. Это install oracle, не oracle refill/flush; последние отдельно на этапе attribution.

## Bounded calibration до candidate данных

- Один pilot Sefer с repeats16,1warmup. Raw sample пишется до вычисления repeats.
- repeats = clamp(ceil(100000000 / (pilot.workload_ns / 16)),1,4096). Freeze JSON сохраняется ДО validation. Никакого подбора по A/B или повторных pilot.
- Validation:10 A/A blocks ×4fresh Sefer processes; позиции outer0/3 против inner1/2. Все samples остаются. Сравнение identical exact executable, не rebuild.
- Statistic arithmetic mean по workload_ns/repeat; paired Student-t95 CI для inner−outer block means, df9.
- Целевая meaningful latency delta =0.05×baseline_mean. Judge sensitivity требует paired95 halfwidth <=0.025×baseline_mean и CI включает0. Последнее проверяет отсутствие наблюдаемого position bias, не доказывает отсутствие систематической ошибки.
- Если calibration не проходит — этот judge не получает product candidate A/B. Не повторять до PASS. Можно завершить source/Ir attribution с явным отсутствием native judge, но нельзя объявить native speedup.

Source snapshot ДО build; PE SHA ДО/ПОСЛЕ samples. Зависимости из Cargo.lock: mimalloc0.1.52, criterion0.5.1 (не runner этого этапа), proc-probe0.1.0; новых зависимостей нет. Rustc/CPU/OS/features фиксируются runner. No local tests/Miri/Loom/doc/agents/commit/push/merge. Длительный build/calibration — один background executor, notification без polling.
