# Повторное read-only ревью качества и архитектуры

Дата: 2026-07-12  
Объект: текущее рабочее дерево workspace `sefer-alloc`  
Контекст: `docs/agent_reports_round2/code_cleanup_review.md`

## Метод и ограничения

Ревью выполнено повторным статическим чтением текущих Rust-исходников, manifest-файлов, документации и существующих отчётов. Git, сборка, тесты, бенчмарки, скрипты и fuzzing не запускались. Поэтому выводы о сигнатурах, visibility, feature graph, явных ветвях и противоречиях документации имеют прямое подтверждение в исходниках; фактический warning set, поведение codegen и runtime-характеристики не проверялись.

## Итог

После round2 заметно улучшена точность документации и закрыты несколько конкретных дефектов: исправлён `HeapOverflow::drain`, актуализированы `PageMap`, `Region` и module docs, исправлена категория `sefer-region`, raw abandoned-stack API стали `unsafe`. Однако два главных cleanup-риска остаются: публичный safe raw-pointer allocator API и огромная doc-hidden тестовая поверхность. Также исправление фиктивных `LargeCacheMode` создало новую регрессию: неподдерживаемая конфигурация теперь паникует лениво на первой глобальной аллокации, хотя allocator декларирует no-panic контракт.

Ниже замечания отсортированы по риску.

## Актуальные замечания

### 1. `AllocCore::dealloc/realloc` и прокси `HeapCore` остаются ошибочно safe

- Приоритет: критический.
- Файл/строки: `src/alloc_core/alloc_core.rs:13-21`, `src/alloc_core/alloc_core.rs:720-767`, `src/alloc_core/alloc_core.rs:1297-1355`; `src/registry/heap_core.rs:1085-1105`, `src/registry/heap_core.rs:1452-1468`; публичный re-export — `src/lib.rs:234-242`, `src/lib.rs:279-280`.
- Обоснование: методы по-прежнему объявлены `pub fn`, хотя принимают произвольные `*mut u8` и полагаются на контракт «ранее возвращённый живой блок с правильным `Layout`». Проверка принадлежности сегмента не доказывает, что `ptr` указывает на начало живого блока и что layout соответствует именно этому блоку. `safe_payload_read_span` ограничивает чтение границей committed span, но не размером исходной аллокации. Для `dealloc` interior pointer или неверный class/layout способен изменить bitmap/freelist не того блока. Более того, module doc прямо говорит, что эти методы `unsafe`, тогда как сигнатуры safe.
- Предлагаемое улучшение: сделать обе пары методов `unsafe fn` с полным `# Safety`; оставить safe API только поверх непрозрачного allocation token/handle, если такой API действительно нужен. Синхронизировать module docs и marketing-claims после смены границы.
- Риск: UB или повреждение allocator достижимо из внешнего safe Rust; `#[doc(hidden)]` у `HeapCore` не ограничивает доступ. Изменение сигнатур breaking, но текущее состояние является soundness-дефектом.
- Уверенность: высокая.

### 2. Публичная doc-hidden test surface всё ещё позволяет нарушать raw-memory контракты из safe-кода

- Приоритет: критический.
- Файл/строки: публичные модули — `src/lib.rs:234-257`; `src/alloc_core/alloc_core.rs:1214-1246`; `src/alloc_core/alloc_core_small.rs:470-565`; `src/alloc_core/remote_free_ring.rs:490-532`; `src/alloc_core/numa.rs:38-63`.
- Обоснование: локальные исправления заменили некоторые `debug_assert!` на release-surviving `assert!`, но закрыли лишь легко проверяемые null/alignment/membership условия. Safe `RemoteFreeRing::over_test_buffer/init_test_buffer` не могут проверить writable footprint и lifetime. Safe `dbg_recycle` перед membership-проверкой вызывает `SegmentHeader::read_at(base)` через `SegmentTable::recycle`. `dbg_corrupt_freelist_head_next`, `dbg_drain_freelist_batch`, `dbg_alloc_bitmap_bytes_for` и `dbg_magazine_bitmap_bytes_for` работают с вычисленным base без общей release-проверки принадлежности; у bitmap hooks размер защищён только `debug_assert!`. Safe публичный `bind_segment` делегирует unsafe shim-контракт владения reservation. Все пути доступны downstream-коду через `pub mod`, несмотря на `#[doc(hidden)]`.
- Предлагаемое улучшение: убрать white-box hooks из публикуемой библиотеки: использовать отдельный непубликуемый test-support crate/feature либо пересмотреть правило, запрещающее локальные unit tests для raw-memory seams. До миграции сделать все непроверяемые pointer constructors/mutators `unsafe fn`, а проверяемые hooks централизованно валидировать одним helper, а не разрозненными asserts.
- Риск: внешний safe-код может вызвать raw read/write/release по чужому или короткоживущему адресу; одновременно сотни тестовых exports фиксируют внутреннюю архитектуру и мешают рефакторингу.
- Уверенность: высокая.

### 3. Исправление `LargeCacheMode` нарушает no-panic контракт глобального allocator

- Приоритет: высокий; новая регрессия после round2.
- Файл/строки: `src/alloc_core/large_cache_config.rs:233-250`; `src/alloc_core/large_cache_mode.rs:11-39`; `src/alloc_core/alloc_core.rs:507-558`; lazy bind — `src/global/sefer_alloc.rs:211-245`, `src/global/sefer_alloc.rs:258-277`, `src/global/tls_heap.rs:394-418`, `src/global/tls_heap.rs:450-459`; заявленный контракт — `src/global/sefer_alloc.rs:43-47`, `src/global/sefer_alloc.rs:359-370`, `Cargo.toml:113-120`; тесты, закрепляющие панику, — `tests/large_cache_mode.rs:40-65`.
- Обоснование: прежний silent no-op устранён, но `Background/Both` всё ещё принимаются infallible `const` builder-ом и вызывают `panic!` только в `AllocCore::new_with_config`. Для `SeferAlloc` это происходит не при создании static, а на первой аллокации потока внутри `GlobalAlloc::alloc`. Паника из global allocator может привести к abort/reentrant allocation и прямо противоречит многократно заявленному «every entry point ... NEVER panics». Документация `new_with_config` также говорит, что `None` — единственный исход кроме успеха, не упоминая panic в основном контракте.
- Предлагаемое улучшение: не представлять неподдерживаемые состояния в публичном enum до реализации (оставить только `Lazy`, расширяемость уже обеспечена `#[non_exhaustive]`). Если совместимость требует variants, валидировать конфигурацию fallible API до установки allocator и никогда не паниковать из lazy bind/global allocation path.
- Риск: предсказуемый process abort при первой аллокации с формально принятой конфигурацией; тесты сейчас утверждают регрессию как желаемое поведение.
- Уверенность: высокая.

### 4. Конфигурация `SeferAlloc` остаётся process/slot-global, хотя API выглядит instance-local

- Приоритет: высокий.
- Файл/строки: `src/global/sefer_alloc.rs:161-168`, `src/global/sefer_alloc.rs:211-244`; `src/global/tls_heap.rs:394-418`; `src/registry/heap_registry.rs:146-190`; `src/global/sefer_alloc.rs:289-292`.
- Обоснование: прежняя проблема теперь честно документирована как «first to bind wins», но поведение не изменилось. `SeferAlloc` хранит config в экземпляре, тогда как TLS и registry общие; recycled slot сохраняет конфигурацию первого materialisation навсегда. При нескольких экземплярах конфигурация другого allocator молча игнорируется. API одновременно говорит, что несколько экземпляров не запрещены, а `stats()` у них общие.
- Предлагаемое улучшение: выбрать одну модель и выразить её типами/API: либо ZST `SeferAlloc` плюс process-wide one-time config с явным конфликтом, либо instance-owned registry/TLS identity. Если поддерживается только один global static, убрать видимость instance-local независимости и формально запретить конфликтующие конфигурации.
- Риск: порядок первого вызова на каждом slot/process определяет фактическую политику памяти; диагностика и конфигурация выглядят per-instance, но такими не являются.
- Уверенность: высокая.

### 5. В production tree одновременно сохранены отвергнутый эксперимент, legacy tier и опасный future-only substrate

- Приоритет: высокий по сопровождению, средний по непосредственному runtime-риску.
- Файл/строки: `Cargo.toml:64-88`, `Cargo.toml:204-235`; `src/alloc_core/run_stack.rs:1-25`; `src/concurrent/mod.rs:5-15`; `src/registry/heap_registry.rs:262-320`, `src/registry/heap_registry.rs:328-350`, `src/registry/heap_registry.rs:500-585`; `src/alloc_core/alloc_core_small_pool.rs:577-590`; `src/registry/heap_core.rs:2247-2269`; `src/alloc_core/size_classes.rs:112-118`, `src/alloc_core/size_classes.rs:212-218`.
- Обоснование: `alloc-runfreelist` имеет зафиксированный NO-GO и регрессию 23–31%, но остаётся публичной feature с layout/algorithm branches и тестами. `experimental` целиком legacy/deprecated, однако `pinning` тянет его зависимости. Abandon/adopt subsystem не используется production path, но остаётся callable и содержит документированный reactivation hazard: два стека делят `next_abandoned`, совместное включение может дать wild pointer/corruption. Рядом остаются full-reset decommit, `reset_stamp_cache` и Huge classifier, удерживаемые только для гипотетических будущих политик.
- Предлагаемое улучшение: удалить rejected feature из публикуемого manifest; вынести research/legacy и abandon/adopt experiments в отдельный непубликуемый crate/ветку. Разделить `pinning` и legacy concurrent features. Future hooks возвращать только вместе с реальным вызывающим поведением и инвариантами.
- Риск: feature-matrix и review surface растут вокруг вариантов, которые не рекомендуются и не развиваются; future-код дрейфует и может быть реактивирован вопреки уже документированной несовместимости.
- Уверенность: высокая.

### 6. `AllocBitmap` и `MagazineBitmap` всё ещё дублируют один механизм

- Приоритет: средний.
- Файл/строки: `src/alloc_core/alloc_bitmap.rs:48-138`; `src/alloc_core/magazine_bitmap.rs:64-145`.
- Обоснование: структуры повторяют поле `bits`, геометрию `FOOTPRINT`, `new`, byte-wise init, `locate`, read-modify-write set/clear. Различается только доменная семантика имён. Даже cfg/dead-code аннотация на `init_in_place` скопирована буквально.
- Предлагаемое улучшение: выделить приватный механический `SegmentBitmap` (`init/test/set/clear/locate`), сохранив два semantic newtype, чтобы типы состояний нельзя было смешать.
- Риск: исправление bounds, инициализации или memory-access discipline легко попадёт только в одну копию; прямое объединение без newtypes, напротив, ухудшит типовую безопасность.
- Уверенность: высокая.

### 7. Публичный контракт стоимости `SeferAlloc::stats()` не соответствует реализации

- Приоритет: средний.
- Файл/строки: обещание — `src/global/sefer_alloc.rs:280-287`, `src/global/alloc_stats.rs:17-21`; реализация — `src/registry/heap_registry.rs:1015-1042`, `src/registry/heap_registry.rs:1070-1090`; вызовы — `src/global/sefer_alloc.rs:315-333`.
- Обоснование: docs обещают «fixed handful of relaxed atomic loads», «no segment or heap walk» и пригодность для metrics-scrape hot path. Фактически при соответствующих features один snapshot выполняет два прохода `0..count.min(MAX_HEAPS)`; на каждом slot есть как минимум Acquire gate и Relaxed counter load. Стоимость O(high-water heap slots), до 4096, и никогда не уменьшается после завершения потоков.
- Предлагаемое улучшение: немедленно исправить complexity/cost docs. Если нужен заявленный O(1), поддерживать агрегаты или snapshot-cache вне scrape path, не возвращая глобальный contended increment на allocator hot path.
- Риск: неожиданный линейный latency metrics endpoint и ложные архитектурные предположения клиентов.
- Уверенность: высокая.

### 8. `Reservation::is_empty` формально «исправлен» тестом, который нарушает unsafe-контракт конструктора

- Приоритет: средний.
- Файл/строки: `crates/vmem/src/lib.rs:117-129`, `crates/vmem/src/lib.rs:180-228`, `crates/vmem/src/lib.rs:247-267`; `crates/vmem/tests/smoke.rs:133-162`.
- Обоснование: все допустимые состояния требуют `len != 0`, поэтому `is_empty()` остаётся константно ложным на валидном state space. Новый тест создаёт `Reservation::from_raw_parts(..., len = 0, ...)`, хотя `# Safety` требует non-zero multiple of `PAGE`; комментарий теста прямо признаёт нарушение контракта. Нарушение unsafe precondition нельзя использовать как доказательство публичной семантики, даже если объект затем `forget`-ится.
- Предлагаемое улучшение: удалить/deprecate бессмысленный метод либо сделать zero-length reservation реальным валидным состоянием и полностью определить его ownership/drop semantics. Удалить тест, основанный на заведомо невалидном unsafe-вызове.
- Риск: тестовая база нормализует нарушение unsafe-контрактов и создаёт ложное ощущение закрытого API-дефекта; runtime-риск самого метода низкий.
- Уверенность: высокая.

### 9. `Default for AllocCore` скрывает fallible OS-reservation за паникой

- Приоритет: средний-низкий.
- Файл/строки: `src/alloc_core/alloc_core.rs:1678-1696`; fallible constructor — `src/alloc_core/alloc_core.rs:496-505`.
- Обоснование: `AllocCore::new()` честно возвращает `Option`, но `Default::default()` преобразует OOM в `expect` panic. Комментарий признаёт, что внутри проекта этот impl не используется и существует только ради возможных generic bounds. Для memory allocator это особенно неудачная абстракция: generic код воспринимает `Default` как обычное конструирование, тогда как оно делает OS reservation и может паниковать при дефиците памяти.
- Предлагаемое улучшение: убрать `Default`; использовать `try_new`/`new -> Option` как единственный путь. Если trait обязателен внешней совместимостью, deprecated-ить impl/API ожидание и явно вынести panicking constructor с говорящим именем.
- Риск: неожиданная паника/OOM escalation и лишняя публичная поверхность без внутренних пользователей.
- Уверенность: высокая.

### 10. Крупнейшие модули по-прежнему смешивают несколько подсистем и историю изменений

- Приоритет: средний.
- Файл/строки: `src/alloc_core/alloc_core_small.rs:1-2519`; `src/registry/heap_core.rs:1-2429`; `src/alloc_core/alloc_core.rs:1-1813`; `src/alloc_core/segment_header.rs:1-1712`; примеры исторических/future блоков — `src/registry/heap_core.rs:407-433`, `src/registry/heap_core.rs:2247-2269`, `src/registry/heap_registry.rs:262-320`.
- Обоснование: размеры не уменьшились, некоторые файлы выросли. Один export продолжает объединять routing, cache, lifecycle, diagnostics, test hooks и многосотстрочные task/phase retrospectives. Правило «one file — one export» не ограничивает ответственность implementation blocks и фактически препятствует естественному разбиению поведения.
- Предлагаемое улучшение: разделить implementation по подсистемам (`small carve/refill/reclaim`, tcache/routing, diagnostics/test support, ownership/adoption), разрешив несколько `impl Type` в тематических файлах. В коде оставить текущие инварианты и короткие safety proofs; историю task IDs и rejected alternatives держать в ADR/design docs.
- Риск: высокая когнитивная нагрузка, конфликтные изменения и невозможность локально ревьюить feature-specific поведение. Рефакторинг проводить малыми шагами с сохранением visibility/inlining.
- Уверенность: высокая.

### 11. `Cargo.toml` остаётся 502-строчным design/changelog ledger

- Приоритет: низкий.
- Файл/строки: `Cargo.toml:35-49`, `Cargo.toml:57-240`, `Cargo.toml:242-319`, `Cargo.toml:321-502`.
- Обоснование: определения features, dependencies и targets окружены фазами, task IDs, benchmark verdicts, историей отклонённых подходов и длинными инструкциями. Важные фактические связи (`fastbin -> alloc-xthread`, `pinning -> experimental`, supported bundles) приходится извлекать из больших narrative-блоков; сведения дублируют `docs/perf`, `docs/PLAN.md` и module docs.
- Предлагаемое улучшение: оставить рядом с TOML entry короткий актуальный контракт и load-bearing constraint; остальное перенести в feature-matrix/ADR. NO-GO эксперимент после удаления feature хранить только в experiment report.
- Риск: ошибки feature graph хуже видны при review, а исторические комментарии расходятся с актуальной реализацией.
- Уверенность: высокая.

### 12. Заявленный «source of truth» unsafe-инвентаря сам считает комментарии и даёт неверный итог

- Приоритет: низкий.
- Файл/строки: `CLAUDE.md:112-119`; `src/lib.rs:100`, `src/lib.rs:167`, `src/lib.rs:169-193`; `README.md:326-360`.
- Обоснование: рекомендуемый `grep -rln 'allow(unsafe_code)'` ищет строку где угодно, включая комментарии. Например, `src/registry/heap_overflow.rs` содержит фразу «There is no `#![allow(unsafe_code)]` in this file», а `src/lib.rs` содержит саму команду; оба становятся ложными совпадениями. Реальных crate/module attributes `^#![allow(unsafe_code)]` в текущих `.rs` — 13, тогда как `src/lib.rs` утверждает 14 файлов. Это ломает заявленную самопроверяемость инвентаря.
- Предлагаемое улучшение: использовать точный anchored pattern по Rust-файлам, например поиск `^#!\[allow\(unsafe_code\)\]`, и не фиксировать ручной count в prose либо генерировать/проверять его отдельным read-only lint.
- Риск: аудиторы получают ложный список seams и могут как проверять лишний файл, так и не заметить реальный новый attribute среди шумных совпадений.
- Уверенность: высокая.

## Сверка всех замечаний round2

| № round2 | Текущий статус | Проверка по текущим исходникам |
|---:|---|---|
| 1 | Открыт | `AllocCore::dealloc/realloc` и `HeapCore` всё ещё safe; см. находку 1. |
| 2 | Частично исправлен, но открыт | Несколько guards усилены до `assert!`, abandoned-stack API стали `unsafe`, однако публичные raw-memory hooks и `pub mod` остаются; см. находку 2. |
| 3 | Документирован, архитектурно не исправлен | First-bind/slot lifetime semantics теперь описаны в `SeferAlloc::with_config`, но конфигурация всё ещё не instance-local; см. находку 4. |
| 4 | Открыт | NO-GO `alloc-runfreelist` сохранён как публичная feature и разветвляет layout/algorithms; см. находку 5. |
| 5 | Открыт | `experimental` остаётся legacy bundle, `pinning` тянет его целиком. |
| 6 | Старый silent no-op закрыт, создана регрессия | Неподдерживаемые modes теперь не молчат, но паникуют на lazy materialisation/global allocation; см. находку 3. |
| 7 | Открыт | Дублирование bitmap осталось без структурных изменений; см. находку 6. |
| 8 | В основном закрыт | `PageClass` теперь честно описывает first-touch/mixed-class модель; routing prohibition явный. Осталась локальная фраза `PageMap` entry «which size class owns the page» (`segment_header.rs:821-822`), но основной опасный контракт исправлен. |
| 9 | Закрыт | `run_stack.rs` описывает полное Ф1-Ф4 wiring и NO-GO; `magazine_bitmap.rs` прямо отмечает production GO. |
| 10 | Частично исправлен | `register_segment_internal/set_small_current_internal` теперь реально используются `try_adopt`; но `reset_stamp_cache`, full-reset decommit, Huge classifier и другие future-only части остаются. |
| 11 | Открыт | Четыре основных файла всё ещё имеют 1712-2519 строк; см. находку 10. |
| 12 | Закрыт | Документация `Region` теперь прямо говорит о tombstone holes и non-dense iteration. |
| 13 | Закрыт | `crates/region/Cargo.toml:13` использует корректную категорию `no-std`, не `no-std::no-alloc`. |
| 14 | Не закрыт по существу | Реализация стала `self.len == 0`, но валидный state space всё ещё non-empty, а regression test нарушает unsafe-контракт; см. находку 8. |
| 15 | Открыт | Root manifest остаётся 502-строчным ledger; см. находку 11. |

## Полностью закрытые пункты и дополнительные подтверждённые исправления

1. Round2 #9: module docs `RunStack` и `MagazineBitmap` синхронизированы с фактическим статусом.
2. Round2 #12: публичные docs `Region` больше не обещают always-compact/dense storage.
3. Round2 #13: metadata `sefer-region` больше не заявляет `no-std::no-alloc`.
4. Связанный round2 correctness-пункт `HeapOverflow::drain`: теперь возвращает фактический финальный `head` (`src/registry/heap_overflow.rs:339-397`), а не snapshot `tail`.
5. Связанный round2 soundness-пункт abandoned stack: `push_abandoned_segment`, `pop_abandoned_segment`, `try_adopt` и `abandon_segments` имеют `unsafe` сигнатуры и `# Safety` (`src/registry/heap_registry.rs:355-432`, `src/registry/heap_registry.rs:500-518`).
6. Round2 #8 в своей load-bearing части: `PageMap` больше не документирован как authoritative class router (`src/alloc_core/segment_header.rs:188-202`, `src/alloc_core/segment_header.rs:818-829`).

## Рекомендуемый порядок дальнейших работ

1. Исправить публичную soundness-границу allocator и полностью изолировать test-only raw-pointer surface (находки 1-2).
2. Убрать panic из `LargeCacheMode`/global allocation path и выбрать честную process-wide либо instance-local модель конфигурации (3-4).
3. Сократить feature/substrate matrix: rejected runfreelist, legacy concurrent bundle и unwired abandon/adopt future path (5).
4. Исправить `stats()` contract и недействительный `Reservation::is_empty` test (7-8).
5. Затем устранять механическое дублирование и структурный долг крупных модулей/manifest (6, 9-12).
