# Повторное read-only ревью качества и архитектуры

Дата: 2026-07-14  
Объект: текущее рабочее дерево workspace `sefer-alloc`  
Контекст: `docs/agent_reviews_round4/code_quality_review.md` и `docs/reviews/2026-07-13-round4-remediation-plan.md`

## Метод и ограничения

Ревью выполнено статическим чтением актуальных Rust-исходников, manifest-файлов, документации и предыдущих отчётов. Git, сборка, тесты, бенчмарки, проектные скрипты и fuzzing не запускались. Существующие файлы не изменялись; единственная запись — этот новый отчёт. Поэтому сигнатуры, visibility, cfg-граф, явные состояния и противоречия комментариев подтверждены непосредственно исходниками, но фактический warning set, codegen и runtime-поведение не проверялись.

## Итог

Последний цикл исправлений дал заметный результат. Закрыт публичный mutation-доступ к state machine registry; большинство raw test hooks получили честную `unsafe fn`-границу; исправлён возврат `refill_class`; добавлен teardown trim; удалён недостижимый abandon/adopt substrate; дедуплицированы два bitmap-механизма; документация `sefer-region` синхронизирована с реализацией; невалидный тест `Reservation::is_empty` удалён, а сам бессмысленный метод deprecated; крупные small/heap реализации частично тематически разделены.

Тем не менее главная soundness-проблема round4 остаётся открытой, причём текущие исходники сами содержат дополнительные доказательства: recommended `production` оставляет выключенными проверки interior pointer и generation, известный cross-thread double-free сценарий закреплён как красный `#[ignore]`-тест, а foreign path после проверки только `base != null` читает `magic` по произвольному ненулевому адресу. Кроме того, новый `debug_assert!` конфликта конфигурации срабатывает уже после захвата слота и при unwind оставляет его `LIVE`. Ниже замечания отсортированы по риску.

## Актуальные замечания

### 1. Safe raw-pointer `dealloc/realloc` всё ещё не обеспечивают заявленную safe-границу

- Статус относительно round4: открыт; текущий код дополнительно фиксирует известный красный production-сценарий и неполный foreign-pointer fix.
- Приоритет: критический.
- Файл/строки: публичные safe entry points и обещания — `src/alloc_core/alloc_core.rs:744-817`, `src/alloc_core/alloc_core.rs:1361-1432`, `src/registry/heap_core.rs:976-997`, `src/registry/heap_core.rs:1348-1364`; отключённые production-проверки — `src/registry/heap_core.rs:1057-1105`, `src/alloc_core/alloc_core_small.rs:1022-1045`, `Cargo.toml:164-205`; foreign raw read — `src/registry/heap_core_xthread.rs:222-266`, `src/registry/heap_core.rs:1468-1523`; известный residual — `src/registry/heap_core.rs:1159-1199`, `tests/regression_xthread_double_free_residual.rs:45-66`, `tests/regression_xthread_double_free_residual.rs:105-107`; физический, а не allocation-sized realloc bound — `src/alloc_core/alloc_core.rs:1407-1455`.
- Обоснование: обе API-поверхности принимают raw pointer и caller-provided `Layout` как safe `pub fn`, хотя необходимые preconditions не проверяются во всех сборках. Конкретные остатки:
  1. Если `contains_base` текущего heap не находит сегмент, `dealloc_foreign_slow` и foreign-leg `realloc` проверяют только `base.is_null()`, после чего вызывают `SegmentHeader::magic_at(base)`. Произвольный ненулевой адрес, выровненный маскированием на границу сегмента, остаётся raw read из невалидной/неотображённой памяти; исправлен только частный пример `1 as *mut u8`, маскирующийся в null, а не класс дефекта.
  2. Комментарии own-thread fastbin/substrate прямо описывают, что 16-байтно выровненный interior pointer без `hardened` попадает в другой bitmap bit и может быть повторно выдан как mid-block address. `production` не включает `hardened`.
  3. Re-issue-before-drain для cross-thread double free остаётся красным и `#[ignore]` во всех non-hardened профилях; generation-защита работает только с `hardened`. Это противоречит safe-документации о double-free-before-reuse как no-op.
  4. `safe_payload_read_span` ограничивает `realloc` физически committed остатком сегмента, но не размером конкретного живого allocation. Завышенный `old_layout`, остающийся внутри того же сегмента/span, всё ещё может прочитать соседние или неинициализированные байты.

  Аргумент о невозможности распознать stale pointer после повторного использования объясняет ограничение `free(ptr)`-подобного ABI, но не делает safe Rust API sound: у `GlobalAlloc` эти preconditions находятся на `unsafe` trait boundary, тогда как здесь внешний safe-код может нарушить их без `unsafe` блока.
- Предлагаемое улучшение: сделать raw-pointer `AllocCore::{dealloc,realloc}` и `HeapCore::{dealloc,realloc}` `unsafe fn` с полным `# Safety`, оставив safe API только через owning allocation token/handle. Если safe API принципиален, до любого header read нужна process-global membership-проверка без разыменования caller address; block start, kind, generation и фактический allocation size должны валидироваться без `Layout` как источника истины во всех профилях. `hardened` может усиливать диагностику, но не быть границей soundness safe API.
- Риск: чтение произвольной/неотображённой памяти, чтение за границей allocation, allocator corruption, повторная выдача пересекающихся блоков и UB из внешнего safe Rust.
- Уверенность: высокая; опасные ветви и известный красный counterexample прямо документированы в текущих исходниках.

### 2. Doc-hidden test surface всё ещё содержит safe операции, намеренно ломающие инварианты allocator

- Статус относительно round4: частично исправлен; registry control plane закрыт и многие raw hooks стали `unsafe`, но единая граница проведена не полностью.
- Приоритет: критический по soundness, высокий по сопровождению.
- Файл/строки: публичные doc-hidden root modules — `src/lib.rs:244-267`; безопасные header-corruption hooks — `src/alloc_core/alloc_core.rs:1150-1181`, `src/alloc_core/alloc_core.rs:1183-1222`; для сравнения исправленные unsafe hooks — `src/alloc_core/alloc_core.rs:1264-1310`, `src/alloc_core/alloc_core_small_diag.rs:62-129`, `src/alloc_core/alloc_core_small_diag.rs:132-217`, `src/alloc_core/remote_free_ring.rs:487-540`, `src/alloc_core/segment_header.rs:1573-1673`.
- Обоснование: `#[doc(hidden)]` не ограничивает downstream-доступ. `dbg_stamp_segment_id` и особенно `dbg_stamp_kind_byte` остаются safe `pub fn`, хотя намеренно записывают произвольные значения в load-bearing allocator metadata. Membership `assert!` подтверждает лишь принадлежность сегмента, но не сохраняет инвариант: после safe подмены `kind` или `segment_id` последующий safe `dealloc`, `alloc` либо автоматический `Drop` может неверно маршрутизировать/release сегмент или оставить stale table state. Это ровно тот класс «мутирует состояние/пишет raw metadata», который remediation #101 требовал пометить `unsafe fn`. Комментарий `dbg_corrupt_freelist_head_next` даже называет safe `dbg_stamp_*` установленным corruption-паттерном, показывая расхождение классификации.
- Предлагаемое улучшение: как минимум сделать все corruption/preset hooks `unsafe fn` с конкретными postconditions и обязанностями восстановления. Архитектурно лучше убрать white-box hooks из publish surface: перенести их в непубликуемый test-support crate/внутренние unit tests либо генерировать узкий harness, который недоступен обычной dependency build. Оставить public safe только read-only диагностику с release-surviving membership/bounds checks.
- Риск: внешний safe-код способен привести внутренний allocator в состояние, для которого последующие safe методы вызывают raw-memory операции с нарушенными предпосылками; одновременно doc-hidden ABI фиксирует внутренний layout и усложняет рефакторинг.
- Уверенность: высокая.

### 3. Config-conflict `debug_assert!` захватывает слот до panic и оставляет его `LIVE`; сама instance-модель остаётся нечестной

- Статус относительно round4: новый регрессионный дефект поверх частичного исправления прежнего замечания.
- Приоритет: высокий.
- Файл/строки: claim transaction — `src/registry/heap_registry.rs:188-256`; recycle выполняется только после успешного возврата — `src/registry/heap_registry.rs:268-317`; тест намеренно ловит panic, но не может recycle невозвращённый pointer — `tests/regression_r4_3_config_conflict.rs:64-100`; first-materialisation/TLS semantics — `src/global/sefer_alloc.rs:211-254`, `src/global/tls_heap.rs:421-445`; публичная статистика — `src/global/alloc_stats.rs:175-195`.
- Обоснование: mismatch обнаруживается после успешного `FREE → LIVE` CAS. Затем counter увеличивается и `debug_assert!` паникует до возврата `heap_ptr`. При unwind вызывающий код не получил pointer и не способен вызвать `recycle`; слот навсегда остаётся `LIVE` и выпадает из free stack. Regression-тест подтверждает именно эту последовательность: `catch_unwind` очищает слот только в `Ok`-ветке. В реальном `GlobalAlloc` panic на bind path противоречит декларируемому no-panic контракту и обычно приводит к abort, а повторяемый direct/test use способен приблизить `MAX_HEAPS` exhaustion.

  При этом архитектурная проблема round4 #3 лишь сигнализируется, но не устранена: config хранится в экземпляре `SeferAlloc`, а heap/TLS process-global; slot config остаётся first-materialisation-wins. Счётчик видит только повторный `claim_with_config` уже materialised slot и не видит другой config, если поток уже имеет cached TLS heap. Следовательно, он не является полным индикатором несовместимых instances.
- Предлагаемое улучшение: не паниковать внутри claim transaction. Минимальный fix — rollback guard, который при любом раннем выходе/панике возвращает захваченный слот в `FREE`, плюс non-panicking diagnostic. Предпочтительная модель — одна process-wide one-time configuration с `Result` при конфликте и ZST/facade `SeferAlloc`; если нужна instance-local конфигурация, registry/TLS identity должны принадлежать экземпляру. Тест должен проверять, что после конфликтного пути тот же слот снова claimable.
- Риск: утечка registry slots, debug-build abort из allocator path, недетерминированное игнорирование пользовательской конфигурации и ложное доверие к неполному счётчику конфликтов.
- Уверенность: высокая для slot leak/panic; высокая для first-wins semantics, средне-высокая для практической значимости multiple-instance сценария.

### 4. Public feature matrix продолжает обслуживать заведомо rejected и deprecated ветви

- Статус относительно round4: открыт; рекомендация remediation #97 для feature cleanup не выполнена.
- Приоритет: высокий по сопровождению, низкий-средний по непосредственному runtime default/production.
- Файл/строки: legacy bundle и over-inclusive `pinning` — `Cargo.toml:64-88`, `src/concurrent/mod.rs:5-15`, `src/concurrent/mod.rs:17-42`; rejected runfreelist — `Cargo.toml:206-237`, `src/alloc_core/run_stack.rs:1-24`, `src/alloc_core/mod.rs:65-81`; ветви в production-adjacent коде — `src/alloc_core/alloc_core_small.rs:605-624`, `src/alloc_core/alloc_core_small.rs:780-782`, `src/alloc_core/alloc_core_small_magazine.rs:434-565`, `src/alloc_core/alloc_core_small_pool.rs:699-701`, `src/alloc_core/segment_header.rs:1028-1090`.
- Обоснование: `alloc-runfreelist` объявлен NO-GO, не развивается и не входит в production, но остаётся публичной feature-комбинацией с отдельным metadata layout, hot/cold branches, raw test surface и сотнями строк специализированных тестов. Любое изменение small-segment lifecycle обязано продолжать учитывать заведомо проигравшую реализацию. Аналогично `experimental` реэкспортирует шесть deprecated типов без срока удаления, а полезный `PinnedRunner` всё ещё тянет весь legacy `arc-swap + crossbeam-epoch` bundle только ради `ShardedRegion`; manifest прямо фиксирует accepted over-inclusion вместо архитектурного разделения.
- Предлагаемое улучшение: удалить `alloc-runfreelist` feature/source/tests из активной матрицы, оставив результаты эксперимента в design/perf docs и истории; для legacy concurrent tier объявить версию/срок удаления либо вынести его в отдельный research crate. Выделить lean `sharded` subfeature, от которого зависит `pinning`, без epoch/RCU типов.
- Риск: разрастание CI/ревью-матрицы, расхождение редко собираемых ветвей, feature-specific layout regressions и лишние зависимости для пользователей `pinning`.
- Уверенность: высокая.

### 5. После удаления abandon/adopt сохранены исполняемые future-only заготовки и расходящиеся «spec constructors»

- Статус относительно round4: новый конкретизированный остаток прежнего dead-code замечания.
- Приоритет: средний.
- Файл/строки: неиспользуемый stamp reset — `src/registry/heap_core.rs:1807-1831`; недостижимый full decommit variant — `src/alloc_core/alloc_core_small_pool.rs:578-590`; неиспользуемые constructors — `src/registry/heap_slot.rs:206-219`, `src/registry/heap_slot.rs:360-388`, `src/registry/heap_overflow.rs:220-233`; фактический zeroed bootstrap и сознательное расхождение — `src/registry/bootstrap.rs:288-315`.
- Обоснование: эти функции имеют `#[allow(dead_code)]` и сохраняются не для текущего caller, а на случай гипотетического возврата adoption/decommit-without-release либо как «исполняемая спецификация». `HeapSlot::new_uninit` уже намеренно отличается от реального bootstrap (`next_free = u32::MAX` против zeroed `0`), поэтому код не является надёжным single source of truth. Future-only implementation неизбежно дрейфует вместе с layout/ordering и создаёт иллюзию готовой безопасной основы — именно проблема, из-за которой abandon/adopt был удалён.
- Предлагаемое улучшение: удалить невызываемые методы/constructors; актуальные initial-state и возможные будущие протоколы описать в ADR и compile-time layout assertions. При появлении реального caller восстановить реализацию под его фактические invariants и тесты, а не реактивировать устаревшую заготовку. Если constructor нужен как проверяемая спецификация, фактический bootstrap обязан вызывать его или иметь const-equivalence assertion без сознательного расхождения.
- Риск: неверная реактивация дрейфовавшего кода, лишняя audit surface и дублирование источников истины о начальном состоянии.
- Уверенность: высокая по отсутствию caller и расхождению; средняя по приоритету удаления.

### 6. Документация после удаления abandon/adopt всё ещё одновременно описывает старую и новую архитектуру

- Статус относительно round4: частично исправлен, но остаются прямые противоречия.
- Приоритет: средний.
- Файл/строки: manifest утверждает, что substrate остался — `Cargo.toml:104-112`; `SeferAlloc` всё ещё описывает abandon/leak-until-adoption — `src/global/sefer_alloc.rs:17-24`, `src/global/sefer_alloc.rs:112-124`; registry module обещает `claim/recycle/abandon` — `src/registry/mod.rs:23-35`; README/architecture inventory ссылается на удалённый `abandon_segments` — `README.md:356-367`, `docs/ARCHITECTURE.md:158-169`; header docs сохраняют старую семантику поля — `src/alloc_core/segment_header.rs:239-270`, `src/alloc_core/segment_header.rs:432-440`, `src/alloc_core/segment_header.rs:1488-1522`; вводящее в заблуждение package description — `Cargo.toml:5-15`, при фактической cfg-политике `src/lib.rs:131-208`.
- Обоснование: actual TLS path прямо говорит, что abandon/adopt удалён (`src/global/tls_heap.rs:263-287`), но top-level docs сообщают обратное, включая несуществующую функцию и старый bounded-leak lifecycle. В `SegmentHeader` поле `next_abandoned` реально переиспользуется deferred-large stack, однако основная документация всё ещё определяет его как link глобального abandoned stack. Package description заявляет `#![forbid(unsafe_code)] at the top`, хотя allocator features переключают root на `deny` и открывают перечисленные unsafe seams. Это не косметика: maintainer строит неверную модель ownership, teardown и trusted computing base.
- Предлагаемое улучшение: одним проходом обновить manifest, crate/module docs, README и ARCHITECTURE по текущему whole-slot reuse + teardown trim + deferred-large протоколу; переименовать internal `next_abandoned`/связанные комментарии в нейтральное `deferred_next` либо чётко описать единственное текущее назначение. В Cargo description заменить абсолютное `forbid` на точное «unsafe confined to audited seams». Историю фаз и удалённых вариантов перенести в changelog/ADR.
- Риск: ошибочные изменения lifecycle/ordering, неверная security-коммуникация пользователям и повторное появление удалённых протоколов из-за stale design text.
- Уверенность: высокая.

### 7. Тематическое разбиение начато успешно, но три главных implementation-файла и manifest остаются историческими монолитами

- Статус относительно round4: частично закрыт; существенное улучшение есть, завершения нет.
- Приоритет: средний.
- Файл/строки: оставшиеся монолиты — `src/alloc_core/alloc_core.rs:1-1832`, `src/registry/heap_core.rs:1-1924`, `src/alloc_core/segment_header.rs:1-1729`, `Cargo.toml:1-504`; удачный начатый split — `src/alloc_core/mod.rs:11-23`, `src/registry/mod.rs:42-69`; новые тематические файлы — `src/alloc_core/alloc_core_small.rs:1-1306`, `src/alloc_core/alloc_core_small_magazine.rs:1-710`, `src/alloc_core/alloc_core_small_pool.rs:1-712`, `src/alloc_core/alloc_core_small_reclaim.rs:1-431`, `src/alloc_core/alloc_core_small_diag.rs:1-219`, `src/registry/heap_core_xthread.rs:1-517`, `src/registry/heap_core_diag.rs:1-141`; сохраняемое правило — `src/alloc_core/mod.rs:4-13`, `src/registry/mod.rs:23-35`.
- Обоснование: вынос small magazine/pool/reclaim/diag и heap xthread/diag подтвердил, что несколько `impl Type` по тематическим файлам работают и заметно уменьшают исходные монолиты. Но `alloc_core.rs`, `heap_core.rs` и `segment_header.rs` всё ещё смешивают layout/state, hot paths, lifecycle, diagnostics и длинные task/phase retrospectives. Правило «one export per file» продолжает порождать module inception и используется как организационный принцип, хотя Rust не требует держать все impl одного типа рядом. Большие исторические комментарии часто повторяют уже вынесенные design docs и затрудняют поиск действующих invariants.
- Предлагаемое улучшение: продолжить тот же проверенный split по стабильным подсистемам (`alloc_core` lifecycle/config/diagnostics, `heap_core` alloc/free/tcache/ownership, `segment_header` layout/views/field accessors). В исходнике оставить краткий текущий контракт, ordering и safety proof; task IDs, rejected alternatives и измерения вынести в ADR/perf docs. Отказаться от blanket one-export-per-file в пользу тематической cohesion.
- Риск: высокая когнитивная нагрузка, конфликтные изменения и пропуск feature-specific invariant при локальном ревью; непосредственный runtime-риск низкий.
- Уверенность: высокая.

## Сверка всех замечаний round4

| № round4 | Текущий статус | Проверка по текущим исходникам |
|---:|---|---|
| 1 | Открыт | Safe `AllocCore/HeapCore::{dealloc,realloc}` сохранены; arbitrary foreign header read, production interior-pointer gap, physical-span realloc bound и ignored residual подтверждены. См. замечание 1. |
| 2 | Частично закрыт | `refill_class` исправлен; raw ring/gen/freelist hooks в основном стали `unsafe`, registry fields закрыты. Но safe metadata-corruption hooks и весь doc-hidden public module surface остаются. См. замечание 2. |
| 3 | Частично закрыт, с регрессией | Добавлены counter и документация first-materialisation-wins, но model остаётся process-global; mismatch `debug_assert!` после CAS теряет слот при unwind. См. замечание 3. |
| 4 | Закрыт по коду | `push/pop_abandoned_segment`, `try_adopt`, `abandon_segments` и registry head удалены. Осталась stale документация и legacy naming — замечание 6. |
| 5 | Открыт | NO-GO `alloc-runfreelist`, deprecated `experimental` tier и over-inclusive `pinning` сохранены. См. замечание 4. |
| 6 | Закрыт | `crates/region/README.md:9-31` теперь честно описывает tombstone storage, cross-type (не instance) branding и границы; source docs дают per-method complexity (`crates/region/src/region.rs:5-41`). |
| 7 | Закрыт | Общий private `SegmentBitmap` выделен, `AllocBitmap`/`MagazineBitmap` остались semantic newtypes (`src/alloc_core/segment_bitmap.rs:1-117`, `alloc_bitmap.rs:59-124`, `magazine_bitmap.rs:74-139`). |
| 8 | Закрыт с semver-deprecation | Невалидный zero-length тест удалён; `Reservation::is_empty` помечен deprecated и документирован как всегда false для valid state (`crates/vmem/src/lib.rs:117-142`); в `crates/vmem/tests/smoke.rs` прежнего теста нет. |
| 9 | Частично закрыт | Small/heap реализации существенно разделены, но три файла по 1729-1924 строки и правило one-export-per-file остаются. См. замечание 7. |
| 10 | Частично закрыт | `tls_heap`/часть registry docs обновлены под whole-slot reuse, но Cargo, `SeferAlloc`, README, ARCHITECTURE и header docs всё ещё содержат старый abandon/adopt контракт. См. замечание 6. |

## Отдельно подтверждённые закрытые пункты

1. Registry control plane закрыт от downstream safe mutation: `Registry::{slots,count,free_slots}` и `HeapSlot::{state,generation,heap,next_free,initialised}` стали `pub(crate)`; тестовые записи generation проходят через `unsafe fn` (`src/registry/bootstrap.rs:188-275`, `src/registry/heap_slot.rs:238-324`).
2. `refill_class` теперь вычисляет `take = min(want, out.len())` и возвращает фактический count во всех профилях (`src/alloc_core/alloc_core_small_magazine.rs:29-64`).
3. Raw hooks `RemoteFreeRing::{over_test_buffer,init_test_buffer}`, RunStack accessors, gen-table accessors, NUMA binding и freelist/bitmap diagnostics получили `unsafe fn`/`# Safety` там, где validity/lifetime невозможно проверить типами (`src/alloc_core/remote_free_ring.rs:487-540`, `src/alloc_core/run_stack.rs:213-394`, `src/alloc_core/segment_header.rs:1573-1673`, `src/alloc_core/numa.rs:58-64`, `src/alloc_core/alloc_core_small_diag.rs:62-217`). Исключения отмечены в замечании 2.
4. Teardown trim реализован до recycle: flush tcache, drain small pool, evict large cache (`src/global/tls_heap.rs:239-262`, `src/registry/heap_core.rs:1833-1910`).
5. Abandon/adopt production substrate действительно удалён: `HeapRegistry` содержит claim/recycle без abandoned stack, TLS drop сохраняет whole-slot reuse (`src/registry/heap_registry.rs:67-317`, `src/global/tls_heap.rs:263-287`).
6. Исправления bitmap и `Reservation` подтверждены в таблице выше.
7. Прежние закрытые round3-пункты не регрессировали: unsupported `LargeCacheMode::{Background,Both}` отсутствуют (`src/alloc_core/large_cache_mode.rs:11-38`); stats walks compile-time выключены без `alloc-stats` (`src/registry/heap_registry.rs:630-684`, `src/registry/heap_registry.rs:707-750`); `Default for AllocCore` отсутствует; `HeapOverflow::drain` возвращает фактический final `head` (`src/registry/heap_overflow.rs:350-397`).

## Рекомендуемый порядок дальнейших работ

1. Закрыть soundness boundary raw allocator API и arbitrary foreign read; решить известный non-hardened residual, а не держать его ignored (замечание 1).
2. Убрать оставшиеся safe corruption hooks из public surface или сделать их `unsafe fn` (2).
3. Исправить транзакционность config-conflict path и выбрать честную global/instance model (3).
4. Сократить feature matrix и удалить future-only executable scaffolding (4-5).
5. Синхронизировать документацию, затем продолжить уже удачно начатое тематическое разбиение (6-7).
