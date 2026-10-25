# Own-thread free: атрибуция и минимальный frame-кандидат

2026-10-24. Принятая база: `f80c681b799ef4ef7fa08d79c06896fdf1104fe9`. D1′ и scalar-issue candidate не включены. Production src пока не менялся на этапе атрибуции.

## Наблюдение

Актуальные structured данные — `docs/perf/own_free_cost/baseline_summary.{json,csv}`. Один production-only binary с Sefer/mimalloc; runtime layouts 16..512 B и const16/const64, маргинал 50000→100000 frees, два независимых прохода. Профиль снимается только внутри w_free wrappers; это direct GlobalAlloc facade, не установленный allocator. WSL Linux не является native Windows/Linux wall-clock evidence.

Два прохода дали идентичные counts. Self-attribution суммируется ровно в PROGRAM TOTALS. Разбор бинарника подтверждает:

- root/kind не читаются повторно на основном пути: kind перенесён регистром в guard;
- ownership memo уже принимает почти каждый free: новый cache не предлагается;
- runtime wrapper сохраняет шесть callee-saved registers и выделяет stack area до проверки TLS/ownership; const wrapper также несёт общий кадр;
- внутри этого же inline body находятся cap eviction/half-flush, вызов flush_class_internal и compaction, то есть редкая ветвь требует сохранять root/block/class/count/state на время вызовов;
- все guards и основной magazine push уже inline. Выносить весь free в ещё один общий helper не предлагается.

Это достаточная механическая гипотеза для одного ограниченного codegen-эксперимента, не доказанный speedup. Source line attribution для inline-кода приблизительна по границам статей; она не доказывает, что весь wrapper self-Ir устраним.

## Кандидат

Вынести ТОЛЬКО ветку заполненного magazine (`cnt >= cap`), включая cap eviction и half-flush/compaction, в существующий по форме cold/inline-never private helper. Обычный push, physical-kind routing, exact class/state/alignment/payload/bump guards и счётчики rejection остаются в прежнем месте и порядке. В helper передавать уже table-derived OwnedBlock и class, не копию Layout и не результаты новой классификации; count/cap/metadata view разрешено заново прочитать/построить в редком helper до первой его mutation.

Механизм: убрать необходимость держать значения редкой overflow-ветви в кадре обычного free. Не переносить счётчики, не менять policy/refill/cap, не опускать validation. OwnedBlock сам по себе не означает успешный small_free_guard: helper crate-private и вызывается только после этого guard и проверки cnt>=cap.

Исторический B4_FRAME_TAX N1-free не является доказательством GO: те эксперименты на другой базе переносили более широкий free-контекст и/или счётчики и давали regressions. Этот кандидат обязан пройти gate независимо; старый NO-GO не переименовывается и не стирается.

## Порог до первого B-run

1. Baseline/candidate — тот же probe, features production, toolchain и отдельные targets. Сначала сравнить машинный код и два прохода marginal Ir/free.
2. Цель runtime-layout free: снижение не менее 4 Ir/free; const16/const64 — не хуже baseline. Mimalloc controls неизменны между source arms в пределах 0.01 Ir/free; повторяемость одного arm также <=0.01 Ir/free. Это проверка профилировщика, не разрешение runtime regression.
3. Alloc-hit/overflow-heavy control не должен подорожать; для полной приёмки действуют прежние micro-pack budgets: ao/aoc non-target <= +0 Ir, batch <= +1 Ir/block, larson/mstress <= +0.2% от своей базы. Непроверенные строки остаются непроверенными, не PASS.
4. Если кадр/Ir не улучшился либо выросла цена обычного free — остановить кандидат (NULL/NO-GO), не запускать широкую матрицу ради спасения результата.
5. Выжившему кандидату — короткое balanced native wall сравнение Sefer A/B и mimalloc в обеих сборках, отдельно весь процесс и timed workload; при шумном A/A/CI — INCONCLUSIVE. Финальный результат обязан явно дать mimalloc/Sefer до и после, B/A; Ir ratios не называть отношением ns.

### Уточнение control/native этапа до его первого B-run

После успешного free microgate, но до control/native сборок, зафиксирован второй harness `own_free_wall`: прежние runtime/const loops плюс `bulk64` и `bulk16k` (64 allocations, затем 64 frees в round). Для переполнения — 100→200 rounds, два Callgrind прохода каждого source/allocator arm; нормировка на 64 frees/round. Цель контрольных строк — не подорожать; провал означает NO-GO по текущему prereg, независимо от выигрыша normal free. Actual callgraph должен подтвердить flush/helper activation; предсказание по policy само по себе не oracle.

Native Windows: 20 fresh-process balanced blocks на mode, последовательность A/miA/B/miB/miB/B/miA/A. Runtime/const — 1000000 churn steps; bulk — 256 rounds. Дополнительно 10 A/A blocks runtime, four-process outer/inner contrast. Выводить средние per-block arms, paired B−A 95% Student-t CI (df19), A/A CI (df9), workload_ns и process_ns отдельно. Workload window включает setup/churn/teardown; не называть его чистой free latency. Native efficacy остаётся INCONCLUSIVE, если CI эффекта включает ноль или контроль обнаруживает направленный сдвиг. Эти короткие описательные user-requested native comparisons допустимо завершить и при провале overflow control; это не запуск полной promotion-матрицы и не отмена NO-GO.

## Граница безопасности и приёмки

Сохранить table-derived root/provenance, все Small/Large/Unknown paths, точные rejection counters, evicted-vs-current block distinction (Box-frame item164), guard-before-mutation, count/cap, virgin_mask, flush/compaction order и deallocation layout. Никаких новых public hooks, raw-pointer safe API, feature/dependency/version изменений.

Нужны отдельные regression tests normal push и настоящего overflow/cap eviction, проверяющие identity/state/credit и отсутствие aliasing. Тесты, Miri/Loom и CI не запускались и не разрешены локально текущим запросом; сравнительные benchmark builds/profiles разрешены. Cargo check/Clippy не заменяют execution. Commit/push/merge и подагенты не разрешены.

Оба OPEN_ITEMS indexes и открытые тематические карточки прочитаны ранее в этой сессии; эта работа сохраняет их statuses/triggers. Perf103/104/102 — контекст, не закрываемые карточки. Новый измеренный follow-up должен попасть в perf index вместе с отчётом.
