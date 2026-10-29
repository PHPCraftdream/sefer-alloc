# Half-flush-only split: prereg до первого B-run

База: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Предыдущий общий overflow-helper NO-GO из эксперимента own-free-cost НЕ включён. Ни D1′, ни scalar issue не входят в дерево.

## Механизм и границы

Оставить cap<TCACHE_CAP eviction в исходном inline body буквально прежним, включая guard/flush→replace→state→mask→counter order. Только full-magazine half-flush/compaction вынести в private cold/inline-never helper. Передать table-derived OwnedBlock, уже вычисленный class и проверенный guard offset; не читать count/cap повторно в helper. Существующий if cap<TCACHE_CAP уже разделяет эти случаи, новый общий dispatch не добавляется.

Гипотеза: убрать live ranges и compaction calls редкой full ветви, не добавив вызов к частому cap-eviction. Inline cap-ветвь всё ещё может удержать общий кадр: это ограничение гипотезы, а не гарантированный результат. При неизменном/неудачном codegen — остановить эксперимент, не переписывать protocol ради спасения цифр.

Порядок мутаций НЕ переставлять: guard-before-mutation, old evicted-vs-current distinction, exact rejection counters, LIVE/MAG/FREE и credit/virgin transitions остаются прежними. Подготовку текущего блока до flush в этом кандидате не переносить: это было бы отдельным behavior/proof изменением. Public API/features/config/refill/cap/версии не меняются.

## Измерительный протокол

Повторно использовать уже проверенные own_free_profile и own_free_wall byte-for-byte; baseline и candidate получают идентичные harness/Cargo declarations, отдельные target directories. Source identity (base+exact patch hash+new examples hashes), features, toolchain и binary hashes фиксируются до соответствующего запуска.

- Первый gate: WSL production-only marginal Ir/free, N=50000→100000, rt/const16/const64, два прохода/arm. Runtime цель минимум −4Ir/free к принятой базе; const не хуже базы. Repeat и неизменённый mimalloc controls ≤0.01Ir/free. Ir не считать wall-clock.
- Обязательные controls: bulk64 и bulk16k (alloc64→free64), 100→200 rounds, два прохода каждого source/allocator arm; оба не хуже base по Ir/free. Нельзя компенсировать проигрыш cap-eviction выигрышем normal free.
- Actual callgraph: full helper ожидается только на bulk64, шесть раз/round; на bulk16k helper обязан отсутствовать, при этом реальные flush calls остаются 60/round. Это проверяется по raw callgraph, не предполагается по label.
- Native Windows: 20 balanced fresh-process blocks/mode, A/miA/B/miB/miB/B/miA/A. rt/const —1000000steps, bulk —256rounds. AA runtime:10blocks×4processes. Per-block means, Student-t paired CI df19; AA df9. Workload window включает setup/churn/teardown; full process отдельно. Все пять строк и Mi/Sefer до/после публикуются, даже если отрицательные. CI nominal per-row, не global significance.
- Короткое описательное native comparison можно завершить по запросу пользователя при раннем NO-GO; широкую promotion-матрицу при этом не запускать. Если ранние gates GO, это ещё не полная приёмка: canonical/IAI/Larson/mstress/MT/memory и exact-head CI остаются непроверенными, пока не выполнены. Старые nonreg budgets не ослабляются.

## Проверка и полномочия

Отдельные tests/: full16 и smaller cap, exact residency/state/credit/reuse. Они должны ловить пропуск flush/неверную compaction/парковку не того блока; возврат baseline equivalent code не обязан ломать correctness test. Rustfmt/check/Clippy и cargo-free scripts разрешены. Test execution/Miri/Loom/CI не заявлять без execution receipts; локальные тесты и push/merge/agents текущим запросом не разрешены. Сравнительные benchmark builds/profile/native runs разрешены явно.

Все прежние perf/correctness statuses/triggers оставить без изменений. Прочитанные ранее в этой сессии оба OPEN_ITEMS indexes и тематические карточки остаются применимым round context; новый итог внести в durable perf index. Цель — проверяемое улучшение, допустим честный NULL/NO-GO/INCONCLUSIVE.
