# Повторное read-only ревью качества и архитектуры

Дата: 2026-07-13  
Объект: текущее рабочее дерево workspace `sefer-alloc`  
Контекст: `docs/agent_reviews_round3/code_quality_review.md`

## Метод и ограничения

Ревью выполнено повторным статическим чтением текущих Rust-исходников, workspace manifest-файлов, документации и предыдущих отчётов. Git, сборка, тесты, бенчмарки, скрипты и fuzzing не запускались. Существующие файлы не изменялись; единственная запись — этот новый отчёт. Поэтому сигнатуры, visibility, feature graph, явные ветви и противоречия документации подтверждены непосредственно исходниками, но фактический warning set, codegen и runtime-поведение не проверялись.

## Итог

После round3 закрыты четыре конкретных пункта: неподдерживаемые `LargeCacheMode` удалены, лишний обход registry в `stats()` компилируется только вместе с `alloc-stats`, `Default for AllocCore` удалён, unsafe-инвентарь использует точный line-anchored поиск без ручного счётчика. Исправления `HeapOverflow::drain`, unsafe-сигнатур abandoned-stack API и основной документации `PageMap` также сохранились.

Однако два наиболее опасных замечания round3 не устранены. Публичные `AllocCore::dealloc/realloc` и doc-hidden прокси `HeapCore` остаются safe, хотя текущие комментарии реализации прямо показывают случаи, в которых production-сборка принимает interior pointer или неверный `Layout` и повреждает allocator. Публичная doc-hidden тестовая поверхность также по-прежнему позволяет safe-коду инициировать непроверяемые raw reads/writes/releases; в ней дополнительно обнаружен конкретный release-only дефект счётчика `refill_class`.

Ниже актуальные замечания отсортированы по риску.

## Актуальные замечания

### 1. Safe `dealloc/realloc` по-прежнему допускают нарушение memory-safety из safe Rust

- Статус относительно round3: открыт; принятое в remediation plan обозначение «design decision» не подтверждается текущей реализацией.
- Приоритет: критический.
- Файл/строки: `src/alloc_core/alloc_core.rs:17-30`, `src/alloc_core/alloc_core.rs:770-789`, `src/alloc_core/alloc_core.rs:1341-1395`; `src/alloc_core/alloc_core_small.rs:2244-2267`; `src/registry/heap_core.rs:1085-1105`, `src/registry/heap_core.rs:1146-1210`, `src/registry/heap_core.rs:1452-1468`; feature-состав — `Cargo.toml:162-203`; публичные re-export — `src/lib.rs:243-260`, `src/lib.rs:282-286`.
- Обоснование: оба слоя всё ещё объявляют `dealloc/realloc` как safe `pub fn`, но корректность зависит от непроверяемых условий «точное начало живого блока» и «точный исходный `Layout`». Это не только теоретическая претензия. Комментарии `HeapCore::dealloc_own_thread_with_base` прямо фиксируют, что 16-байтно выровненный interior pointer попадает в другой bitmap bit, выглядит allocated и затем выдаётся повторно как mid-block address (`heap_core.rs:1186-1198`). Защита стоит под `#[cfg(feature = "hardened")]` (`:1204-1210`), тогда как `production` не включает `hardened`. Та же защита substrate-деаллокации выключена по умолчанию (`alloc_core_small.rs:2248-2267`). В fastbin-пути проверка, что small `Layout` не применён к Large-сегменту, тоже только hardened (`heap_core.rs:1161-1184`). Для `realloc` `safe_payload_read_span` ограничивает копирование физическим committed span сегмента, а не размером исходного allocation (`alloc_core.rs:1371-1395`), поэтому завышенный `old_layout` всё ещё разрешает чтение за границей выделенного блока, пока оно остаётся внутри сегмента.
- Предлагаемое улучшение: сделать публичные raw-pointer `dealloc/realloc` обоих уровней `unsafe fn` с полным `# Safety` и оставить safe-интерфейс только через allocation token/handle, содержащий проверяемую принадлежность, block start и размер. Если safe API принципиален, обязательные проверки block start/kind/layout должны работать во всех сборках, а размер живого allocation должен храниться/восстанавливаться независимо от caller-provided `Layout`; feature `hardened` не может быть границей soundness safe API.
- Риск: allocator corruption, повторная выдача пересекающихся блоков, out-of-bounds/uninitialised read и UB достижимы внешним safe-кодом. Формулировка README «cannot express UB at the type level» дополнительно создаёт ложное доверие к этой границе.
- Уверенность: высокая.

### 2. Doc-hidden test surface остаётся публичной raw-memory API; `refill_class` имеет release-only ошибку результата

- Статус относительно round3: открыт; локальные guards не устранили архитектурную причину.
- Приоритет: критический по soundness, высокий по сопровождению.
- Файл/строки: публичные модули — `src/lib.rs:237-260`; raw hooks — `src/alloc_core/remote_free_ring.rs:487-532`, `src/alloc_core/numa.rs:38-64`, `src/alloc_core/alloc_core.rs:1242-1274`, `src/alloc_core/alloc_core_small.rs:470-565`; batch API — `src/alloc_core/alloc_core_small.rs:605-621`.
- Обоснование: `#[doc(hidden)]` скрывает документацию, но не ограничивает downstream-доступ. Safe `RemoteFreeRing::over_test_buffer/init_test_buffer` сами признают непроверяемый контракт `FOOTPRINT` writable/lifetime. Safe `bind_segment` вызывает unsafe shim, полагаясь на внутреннее владение reservation, хотя функция доступна снаружи через public module. `dbg_recycle` имеет явный safety-контракт «после вызова адрес освобождён», но остаётся safe. Bitmap dump hooks вычисляют base из произвольного указателя, не проверяют membership и защищают размер лишь `debug_assert!`; в release они могут читать дальше bitmap/segment. `dbg_corrupt_freelist_head_next` и `dbg_drain_freelist_batch` также обращаются к вычисленному base без общей membership-проверки. Отдельный конкретный API-дефект: `refill_class` лишь debug-assert-ит `out.len() >= want`, в release итерирует `take(want)` по более короткому slice, но затем возвращает `want`; вызывающий код получает число инициализированных элементов больше фактического.
- Предлагаемое улучшение: вынести white-box поддержку в непубликуемый test-support crate или в feature, отсутствующую в publish/release surface; сократить `pub mod` до реальных поддерживаемых API. До миграции все hooks с непроверяемым raw-memory/lifetime/ownership контрактом сделать `unsafe fn`, а проверяемые — пропускать через единый release-surviving membership/bounds validator. Для `refill_class` убрать отдельный `want` в пользу `out.len()` либо возвращать фактически заполненный count и проверять class index.
- Риск: внешний safe-код может читать, писать или освобождать чужую/короткоживущую память. Большая тестовая ABI-поверхность фиксирует внутренний layout и мешает безопасному рефакторингу; неверный count способен привести downstream-код к чтению неинициализированных slots.
- Уверенность: высокая.

### 3. `SeferAlloc::with_config` выглядит instance-local, но конфигурация навсегда привязывается к process-global slot/TLS

- Статус относительно round3: открыт; документация стала честнее, архитектура не изменилась.
- Приоритет: высокий.
- Файл/строки: `src/global/sefer_alloc.rs:161-168`, `src/global/sefer_alloc.rs:192-248`; `src/global/tls_heap.rs:394-459`; `src/registry/heap_registry.rs:146-200`.
- Обоснование: `SeferAlloc` хранит `LargeCacheConfig` в экземпляре, но registry и TLS общие. `claim_with_config` применяет config только при первой materialisation slot; reused slot сохраняет прежний `HeapCore` и игнорирует config нового экземпляра. Первый allocator, затронувший slot/thread, определяет политику независимо от экземпляра, через который идут последующие вызовы. Документация явно называет разные configs «effectively unsupported», но тип и infallible constructor позволяют их создать без диагностики.
- Предлагаемое улучшение: выбрать и выразить одну модель. Для process-wide allocator — ZST/facade плюс единственная one-time global configuration с явным `Result` при конфликте. Для instance-local модели — отдельная registry/TLS identity на экземпляр. Как минимум обнаруживать несовпадающую повторную конфигурацию вместо silent ignore.
- Риск: фактические budget/decay/pool policies зависят от порядка первых вызовов и истории recycled slots; диагностика и API выглядят per-instance, но ими не являются.
- Уверенность: высокая.

### 4. Отключённый abandon/adopt substrate уже небезопасен для заявленного будущего включения

- Статус относительно round3: открыт и подтверждён дополнительной внутренней несогласованностью.
- Приоритет: высокий по сопровождению; средний по runtime, пока путь недостижим.
- Файл/строки: `src/registry/heap_registry.rs:262-353`, `src/registry/heap_registry.rs:491-584`; `src/global/tls_heap.rs:239-272`; `src/registry/heap_core.rs:2247-2269`; rejection path — `src/alloc_core/alloc_core.rs:774-789`.
- Обоснование: production использует whole-slot reuse и не вызывает `abandon_segments`; сам код предупреждает, что future reactivation разделяет `next_abandoned` с deferred-large stack и может дать wild pointer/corruption. При этом `try_adopt` после выигранного CAS игнорирует результат `register_segment_internal(base)` и безусловно делает base текущим (`heap_registry.rs:559-568`). Комментарий утверждает, что при полной таблице frees всё равно работают через routing, но `AllocCore::dealloc` первым делом отвергает base, отсутствующий в table. `reset_stamp_cache`, который документация требует при межheap-передаче, также не вызывается. То есть retained substrate не является готовой «проверенной основой»: его заявленный будущий сценарий уже расходится с действующими invariants.
- Предлагаемое улучшение: удалить недостижимый ownership-transfer subsystem из production crate и хранить эксперимент отдельно. Если он действительно нужен, сначала определить отдельные link fields, обработать failure регистрации транзакционно (rollback ownership/CAS или гарантированная регистрация), сбросить caches и добавить один end-to-end protocol, а не поддерживать изолированные halves.
- Риск: будущий разработчик может реактивировать явно «loom-proven» код, полагая его готовым, и получить потерянный сегмент, неверный current base, silent leaks или corruption.
- Уверенность: высокая.

### 5. Публичная feature matrix сохраняет rejected и deprecated реализации

- Статус относительно round3: открыт.
- Приоритет: высокий по сопровождению, низкий-средний по непосредственному runtime.
- Файл/строки: `Cargo.toml:64-88`, `Cargo.toml:203-235`; `src/concurrent/mod.rs:5-15`; `src/alloc_core/run_stack.rs:1-27`; многочисленные ветви — `src/alloc_core/alloc_core_small.rs:992-1123`, `src/alloc_core/alloc_core_small.rs:1837-2010`, `src/alloc_core/segment_header.rs:1039-1101`.
- Обоснование: `alloc-runfreelist` имеет зафиксированный NO-GO и регрессию 23–31 %, но остаётся публичной feature, меняющей segment layout и алгоритмы. Весь `experimental` tier помечен deprecated/legacy и не развивается, однако остаётся публичным bundle; `pinning` тянет весь bundle и обе тяжёлые зависимости, хотя manifest прямо признаёт over-inclusion. Это не архив: Cargo обязан разрешать эти combinations, а изменения общих invariants должны учитывать их при каждом рефакторинге.
- Предлагаемое улучшение: удалить rejected feature из публикуемого manifest и вынести research baseline в отдельный непубликуемый crate/tag. Разделить `pinning`/`ShardedRegion` и старые RCU/epoch types на независимые features; объявить срок удаления deprecated tier.
- Риск: разрастаются CI/review/compatibility surface и число layout variants; устаревшие ветви дрейфуют относительно production invariants и усложняют аудит allocator.
- Уверенность: высокая.

### 6. Standalone `sefer-region` документирует свойства, которых его типы не обеспечивают

- Статус относительно round3: новый/ранее пропущенный остаток; прежний вывод о полном закрытии документации `Region` верен для source docs, но не для crate README.
- Приоритет: средний.
- Файл/строки: `crates/region/README.md:9-21`; фактический storage — `crates/region/src/region.rs:7-15`, `crates/region/src/region.rs:100-119`; branding — `crates/region/src/handle.rs:9-20`; blanket complexity claim — `crates/region/src/region.rs:14-15`, при наличии `reserve/iter/clear` в `:80-89`, `:122-139`.
- Обоснование: README всё ещё называет обычный `SlotMap` «dense, cache-friendly, always-compact», хотя актуальные source docs правильно описывают tombstone holes и неденсную итерацию. README также утверждает, что `Handle<T>` исключает cross-region confusion, но `PhantomData<fn() -> T>` брендирует только тип значения: handle из одного `Region<T>` свободно принимается другим `Region<T>` и может совпасть с живым `DefaultKey`. Наконец, source doc заявляет «All operations are O(1)», хотя iteration/clear линейны, а reserve может аллоцировать.
- Предлагаемое улучшение: синхронизировать README с `SlotMap`, сузить обещание до cross-**type** защиты и явно документировать same-`T` cross-instance limitation. Если нужна настоящая region identity, добавить runtime store id в handle либо отдельный brand parameter. Заменить blanket O(1) на per-method complexity.
- Риск: пользователь выбирает crate из-за неверной locality/compactness модели или считает handle instance-branded и получает тихий доступ/удаление логически чужого значения. UB нет, но целостность прикладных данных нарушается.
- Уверенность: высокая по документации и type shape; средняя по необходимости instance branding как изменения API.

### 7. `AllocBitmap` и `MagazineBitmap` продолжают дублировать один низкоуровневый механизм

- Статус относительно round3: открыт без структурных изменений.
- Приоритет: средний.
- Файл/строки: `src/alloc_core/alloc_bitmap.rs:48-138`; `src/alloc_core/magazine_bitmap.rs:64-145`.
- Обоснование: обе структуры повторяют `bits`, одинаковый `FOOTPRINT`, constructor, byte-wise initialization, `locate`, bit test, set и clear. Даже cfg/dead-code annotation и raw access discipline продублированы. Различается только доменная семантика имён и состояния.
- Предлагаемое улучшение: выделить приватный механический `SegmentBitmap` с `init/test/set/clear/locate`, сохранив `AllocBitmap` и `MagazineBitmap` как semantic newtypes, чтобы состояния нельзя было смешать.
- Риск: bounds/init/access fix легко применяется к одной копии и пропускает вторую; прямое объединение без newtypes, напротив, ухудшит type safety.
- Уверенность: высокая.

### 8. Тест `Reservation::is_empty` по-прежнему строит заведомо невалидный объект

- Статус относительно round3: открыт без изменений.
- Приоритет: средний-низкий.
- Файл/строки: `crates/vmem/src/lib.rs:117-129`, `crates/vmem/src/lib.rs:180-228`, `crates/vmem/src/lib.rs:247-267`; `crates/vmem/tests/smoke.rs:133-162`.
- Обоснование: safe constructor и unsafe contract требуют non-zero `len`, поэтому `is_empty()` константно false на валидном state space. Тест вызывает `from_raw_parts(..., len = 0, ...)` и прямо признаёт отклонение от `# Safety`; `forget` предотвращает Drop, но не делает нарушение precondition валидным способом тестирования API. Таким тестом нельзя доказывать поддерживаемую семантику типа.
- Предлагаемое улучшение: удалить/deprecate `is_empty` как бессмысленный для non-empty RAII handle либо сделать zero-length реальным валидным состоянием с определёнными ownership/drop rules. Удалить тест, нарушающий unsafe-контракт.
- Риск: тестовая база нормализует вызов unsafe API вне контракта и скрывает неясную модель state space; непосредственный production-риск низкий.
- Уверенность: высокая.

### 9. Крупнейшие типы остаются монолитами, а правило «one export per file» мешает тематическому разбиению

- Статус относительно round3: открыт; размеры практически не изменились.
- Приоритет: средний.
- Файл/строки: `src/alloc_core/alloc_core_small.rs:1-2519`; `src/registry/heap_core.rs:1-2429`; `src/alloc_core/alloc_core.rs:1-1820`; `src/alloc_core/segment_header.rs:1-1712`; `src/registry/heap_registry.rs:1-1210`.
- Обоснование: один файл одновременно содержит hot paths, lifecycle, cross-thread protocol, decommit, diagnostics, test hooks и длинные task/phase retrospectives. Несколько тематических `impl Type` в разных файлах дали бы меньшую review surface, но локальное правило «one-export-per-file» используется как аргумент сохранять всё поведение рядом. Рост комментариев не компенсирует смешение ответственностей: load-bearing invariants теряются среди истории экспериментов.
- Предлагаемое улучшение: разделить implementation blocks по подсистемам (`small carve/refill`, `free/reclaim`, `tcache`, `ownership/routing`, `diagnostics/test-support`, `layout accessors`) при сохранении visibility и inlining. В исходниках оставить текущий контракт и safety proof; историю task IDs и rejected alternatives перенести в ADR/design docs.
- Риск: высокая когнитивная нагрузка, конфликтные изменения и сложность локального ревью feature-specific поведения.
- Уверенность: высокая.

### 10. Manifest и верхнеуровневые module docs одновременно перегружены историей и содержат устаревшую архитектуру

- Статус относительно round3: открыт; обнаружены дополнительные stale claims.
- Приоритет: средний.
- Файл/строки: `Cargo.toml:35-49`, `Cargo.toml:57-240`, весь manifest `Cargo.toml:1-502`; устаревший `alloc-xthread` contract — `Cargo.toml:104-112`; `src/registry/mod.rs:1-19`; `src/global/tls_heap.rs:4-10`, `src/global/tls_heap.rs:23-33`, актуальная реализация — `src/global/tls_heap.rs:239-272`.
- Обоснование: manifest остаётся 502-строчным design/changelog ledger. При этом его `alloc-xthread` block всё ещё описывает `ThreadFreeStack`, abandonment-leak и будущую adoption, тогда как текущая small-free архитектура использует per-segment `RemoteFreeRing`, а thread exit recycle-ит whole slot. `registry` module doc и начало `tls_heap` также говорят, что guard abandons segments, прямо противореча текущему `AbandonGuard::drop`, который специально не вызывает abandon. Исторические narrative-блоки уже мешают различать действующий контракт и снятую фазу.
- Предлагаемое улучшение: оставить рядом с feature/модулем только актуальный контракт и load-bearing dependency; phase history перенести в ADR/changelog. Добавить компактную проверяемую feature matrix и обновить registry/TLS overview под whole-slot reuse + RemoteFreeRing/deferred-large split.
- Риск: maintainer или аудитор строит неверную модель ownership/lifetime и меняет не тот протокол; реальные feature dependencies хуже видны среди устаревшей истории.
- Уверенность: высокая.

## Сверка всех замечаний round3

| № round3 | Текущий статус | Проверка по текущим исходникам |
|---:|---|---|
| 1 | Открыт | Safe raw-pointer `AllocCore/HeapCore::dealloc/realloc` сохранены; current comments подтверждают interior-pointer/layout corruption в non-hardened production. См. находку 1. |
| 2 | Открыт | `pub mod` и safe raw test hooks сохранены; добавлен конкретный дефект release-count в `refill_class`. См. находку 2. |
| 3 | Закрыт | В `LargeCacheMode` остался только `Lazy`; panic match и `Background/Both` отсутствуют (`large_cache_mode.rs:11-39`, `large_cache_config.rs:233-249`). |
| 4 | Документирован, но открыт | First-bind semantics описаны, однако config остаётся slot/TLS-global и silent first-wins. См. находку 3. |
| 5 | Открыт | NO-GO runfreelist, deprecated concurrent tier, over-inclusive pinning и unwired abandon/adopt substrate остаются. См. находки 4-5. |
| 6 | Открыт | Bitmap-механика всё ещё продублирована. См. находку 7. |
| 7 | Закрыт | Без `alloc-stats` оба registry walks compile-time заменены на `0`; docs честно указывают O(slots) с feature (`heap_registry.rs:1010-1063`, `:1087-1130`, `sefer_alloc.rs:280-299`). |
| 8 | Открыт | `is_empty` и тест с нарушением `from_raw_parts` contract не изменены. См. находку 8. |
| 9 | Закрыт | `impl Default for AllocCore` отсутствует; fallible `new/new_with_config` остались единственными constructors. |
| 10 | Открыт | Future-only/dead subsystems и hooks сохранены; abandon/adopt дополнительно внутренне расходится с действующими invariants. См. находки 4-5. |
| 11 | Открыт | Четыре крупнейших root-модуля всё ещё имеют 1712-2519 строк. См. находку 9. |
| 12 | Закрыт | `CLAUDE.md:111-123` и `src/lib.rs:98-102` используют anchored `^#![allow(unsafe_code)]`; ручной count удалён. |

## Отдельно подтверждённые закрытые пункты

1. Round3 #3: неподдерживаемые `LargeCacheMode::{Background, Both}` удалены на уровне типов; lazy panic из global allocation path больше не представим этой конфигурацией.
2. Round3 #7: заявленная стоимость `SeferAlloc::stats()` синхронизирована с feature graph — O(1) без `alloc-stats`, O(initialized slots) с ним.
3. Round3 #9: panicking `Default for AllocCore` удалён.
4. Round3 #12: unsafe-инвентарь стал самопроверяемым без ложных совпадений и hardcoded count.
5. Связанный correctness-пункт `HeapOverflow::drain` остаётся исправленным: возвращается фактический финальный `head` (`src/registry/heap_overflow.rs:339-397`).
6. Raw abandoned-stack API остаются `unsafe fn` с `# Safety` (`src/registry/heap_registry.rs:322-432`, `:514-518`).
7. Основная `PageMap` документация по-прежнему честно описывает first-class-wins/mixed pages и запрещает production routing по map (`src/alloc_core/segment_header.rs:188-202`, `:818-829`).
8. `RunStack` и `MagazineBitmap` module docs по-прежнему корректно различают NO-GO и production GO (`src/alloc_core/run_stack.rs:1-27`, `src/alloc_core/magazine_bitmap.rs:1-13`).
9. Категория standalone `sefer-region` остаётся корректной (`crates/region/Cargo.toml:12-13`: `no-std`). При этом его README не полностью синхронизирован с source docs — см. находку 6, поэтому прежний общий вывод о полностью закрытой документации `Region` требует этого уточнения.

## Рекомендуемый порядок дальнейших работ

1. Исправить soundness boundary safe allocator API и изолировать публичную raw test surface (находки 1-2).
2. Определить честную process-wide или instance-local модель конфигурации `SeferAlloc` (3).
3. Удалить либо заново спроектировать недостижимый abandon/adopt substrate; сократить rejected/legacy feature matrix (4-5).
4. Исправить контракт standalone `sefer-region` и невалидный `Reservation` test (6, 8).
5. Затем устранять механическое дублирование, монолиты и исторически перегруженную/stale документацию (7, 9-10).
