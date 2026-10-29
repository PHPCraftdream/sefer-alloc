## Вердикт: **FIX-REQUIRED**

**OBSERVED** ниже — факты из прочитанных файлов; **INFERRED** — последствия предлагаемого дизайна, без исполнения. Основная идея совместима с механизмом K5, но §3–§4 пока оставляют опасные неопределённости.

### 1. Soundness и инварианты

- **INFERRED — обязательное исправление: `parked: u16` недостаточно.** При E = 64 МиБ класс 16 Б может удерживать `64 МиБ / 256 = 262144` записей плюс 8 магазинов пола. Нужно **`u32`**, либо существенно меньший явно обоснованный предел E. Это касается также per-class счётчиков и low-water mark. P0 проверяет только K = 1024 одного класса и не обосновывает ширину для K7: `worktrees/g1-p0/src/registry/heap_core/state/depot.rs:32`.
- **OBSERVED:** действующий free-гейт запрещает вызов парковки после заполнения пола: `src/registry/heap_core/free/dealloc_own_base.rs:572`. В дизайне явно заменить проверку `parked < DEPOT_CAP[c]` на допуск всех depot-классов с проверкой бюджета внутри холодного пути. Иначе overflow недостижим.
- **INFERRED:** закрепить инвариант **`parked = floor_count + overflow_count`**, отсутствие дублей и порядок **overflow-first reload, затем пол**. Тогда существующие reload-сайты сохраняются: `src/registry/heap_core/alloc/hot.rs:502`, `src/registry/heap_core/alloc/hot.rs:681`. При отказе парковки указатели, счётчики и текущий магазин должны оставаться неизменными.
- **OBSERVED:** MAG→LIVE при выдаче уже есть, sidecar reclaim отвергает MAG: `src/registry/heap_core/alloc/hot.rs:93`, `src/alloc_core/small/alloc_core_small_reclaim.rs:55`. **INFERRED:** перенос указателей с сохранением MAG и кредитов сам по себе не требует изменения remote-протокола или double-free oracle. Но формулировку И3 нужно расширить до «активный магазин, пол или overflow»: сейчас она буквально говорит только о tcache magazine — `src/alloc_core/segment/bitmap/state_byte.rs:44`.
- **Обязательный R1-01 контракт decay:** сбрасывать только **ранее припаркованные** записи; никогда текущий `block` и не активный магазин после его добавления. Безопасная точка — до добавления текущего освобождаемого блока. Существующая причина запрета записи тела прямо описана в `src/registry/heap_core/free/dealloc_own_base.rs:594`.
- **OBSERVED:** teardown уже дренирует магазины до освобождения retention; thread-exit trim предшествует передаче слота: `src/registry/heap_core/state/tcache_flush.rs:93`, `src/registry/heap_core/state/ownership.rs:11`. **INFERRED:** сохранить этот порядок; отключить epoch/decay внутри teardown-reload, чтобы teardown не запускал вложенное обслуживание.

**Unsafe seams:** адресация/инициализация записей, создание ограниченных slices, владение reservation и commit/decommit должны быть локализованы в `depot_overflow.rs`; существующий `claim.rs` остаётся единственным seam привязки slot depot. Внешний `Depot` должен содержать zero-valid raw pointer, не обязательный reference/`NonNull`/неинициализированный owning token: `src/registry/heap_registry/claim.rs:795`. Обновить P12, Loom-модель и ghost-правила не только на разрешённые вызовы, но и на **drain-before-decommit, reset-before-reuse, no-decay-in-teardown**. Нынешний owner-rule ловит конкретно `depot.as_deref_mut`, а не произвольные raw-доступы: `docs/verification/ghost-rules.json:2015`.

### 2. OS-backed storage

**OBSERVED:** примитив подходящий: Windows делает reserve с начальным commit и затем MEM_COMMIT; Unix использует обычное eager-accessible mapping, `commit_range` — no-op: `crates/aligned-vmem/src/os/windows.rs:430`, `crates/aligned-vmem/src/os/unix.rs:562`. Этот путь не требует `GlobalAlloc`; lazy-commit dependency уже подключена через `alloc-core`: `Cargo.toml:170`.

**Исправить описание реализации:**

1. Указать layout: **header + records + links + alignment padding**, размер округляется вверх до runtime page size; начальный commit ненулевой и покрывает header. API требует именно это: `crates/aligned-vmem/src/api/reserve_aligned_lazy.rs:19`.
2. **E/256 достаточно как верхняя граница числа записей**, поскольку каждый магазин стоит ≥256 Б. Но 528 КиБ — только массивы при E=1 МиБ, без header/округления.
3. Раздельный массив links неудобен для одного commit-watermark: запись первого link далеко за массивом указателей требует отдельного committed-range либо коммита промежутка. **Проще:** массив interleaved records `{ pointers[16], next: u32 }`, с `size_of`/`align_of` вместо предположения о размере; единый последовательный watermark.
4. **Критическая неопределённость:** header находится внутри reservation, а §4 обещает decommit всех committed pages. Нельзя после этого разыменовывать header на Windows. Выбрать явно: оставить header-page committed либо держать небольшой управляющий объект/token вне mapping и recommit header перед любым доступом.
5. Резервирование возвращает owning `LazyReservation`, который освобождает mapping при drop: `crates/aligned-vmem/src/api/reserve_aligned_lazy.rs:28`. Описать передачу владения, сохранение reservation geometry и отсутствие dangling/self-referential references.
6. При commit failure — не публиковать запись и не менять watermark; существующие записи остаются доступными для reload/trim. Уточнить, запрещены ли только новые growth-attempts или вся парковка, включая свободные committed records.
7. После trim явно сбросить heads/free-list/high-water/low-water и переинициализировать используемые записи. **Не полагаться на zero-fill:** Darwin/BSD его не гарантируют — `crates/aligned-vmem/src/api/decommit.rs:22`. Счётчик вызовов decommit не доказывает возврат страниц ОС.

### 3. Config plumbing

**INFERRED:** `LargeCacheConfig` — приемлемый существующий транспорт, несмотря на название; новая ось `Profile` сейчас не нужна. Но README обязан прямо сказать:

- E — **на heap/slot**, не на процесс и не общий лимит RSS;
- без `alloc-decommit` действует фиксированный default, отключить его этим API нельзя;
- без fastbin/depot механизм неактивен;
- `AllocCore` сам по себе depot не использует.

Хранить resolved E и сравнивать его нужно в **`HeapCore::live_config_matches`**, добавив conjunction к существующему forwarding; не дублировать depot policy в `AllocCore::lifecycle`: `src/registry/heap_core/core.rs:363`, `src/alloc_core/alloc_core/lifecycle.rs:233`.

**OBSERVED:** recycled slot сохраняет прежнюю конфигурацию, конфликт только считается — это не применение новой настройки: `src/registry/heap_registry/claim.rs:216`. Дизайн должен явно раскрыть **first-materialisation-wins**, а не обещать безусловное применение настройки каждого экземпляра.

`dbg_depot_budget()` должен читать фактическое поле heap, включая до первой парковки и после recycle. Для замеров нужны requested/resolved значения **каждого worker heap**, conflict delta = 0 и process/thread identity; наличие одного accessor само по себе R26-4 не выполняет — `CLAUDE.md:702`.

### 4. Decay

**INFERRED:** политика возможна, но «correct and cheap» пока не обосновано.

- Определить точный порядок события, обновления minimum и смены эпохи; после сброса `h/2` начать следующую эпоху с **фактической оставшейся occupancy**, не с нуля.
- `floor(h/2)` оставляет **один overflow-магазин навсегда**, даже при постоянных событиях. Либо документировать этот остаток, либо обеспечить прогресс, например `ceil(h/2)`.
- «Bulk16k не затухает» не следует только из периодического опустошения: если один bulk-цикл длиннее 256 событий, целая эпоха может пройти при положительном minimum. Нужен oracle и отдельная проверка фаз относительно границы эпохи: `docs/design/2026-10-28-depot-budget-design.md:74`.
- Одно epoch завершение может сбросить большой объём, особенно при E=64 МиБ. Задать **bounded work per event** с отложенным долгом; измерять tail latency.
- Посторонние операции могут быть одними magazine hits и не двигать эпоху. Отдельно описать idle/hit-only retention.
- Общий бюджет — first-come-first-served: холодный класс способен блокировать полезный новый класс до decay. Это не soundness-дефект, но нужна явная политика fairness и mixed-class измерение; гарантировать справедливость без механизма нельзя.

### 5. Замеры и приёмка

Исправить §7 следующим образом:

- **Главный критерий:** измеренный выигрыш K7 **непосредственно против mimalloc** хотя бы на одном основном ядре и отсутствие проигрышей на остальных по заранее заданным CI/допускам. «Лучше K5 по отношению» не доказывает победу над mimalloc; текущий пункт сравнивает отсутствие регрессии с K5 — `docs/design/2026-10-28-depot-budget-design.md:126`. Пороги «75% P0» отделить как инженерную цель, не подменять ими решение владельца.
- Для каждого плеча/повтора автоматически проверять resolved config, conflicts и ожидаемые **дельты активации**, не просто печатать счётчики. Нужны occupancy/high-water, budget refusals, reserve/commit failures, decay events/flushed records, committed metadata bytes.
- Добавить режимы ниже/на/выше E, mixed classes, cold-start против steady-state, **повторный burst после decay/trim**. В каждом режиме измерять и скорость/latency, и память; отдельные несовпадающие speed/RSS сценарии не дают Pareto-вывода — `CLAUDE.md:924`.
- RSS-судье добавить baseline, после burst/free, idle, hit-only и гарантированно cold-event нагрузку, explicit trim, выход и повторный claim слота. Разделить память metadata от pinned segments: **E ограничивает байты блоков, не физическое удержание сегментов**.
- Пик измерять во время нагрузки; worker threads должны оставаться живыми до retention snapshots. Сохранить Windows PrivateUsage/WorkingSet, учитывать pool policy и одинаковый touch pattern.
- Safety gate дополнить commit/decommit failure, максимальным E на классе 16 Б, переиспользованием записей между классами, double-free припаркованного блока и handoff после drain. Kani должен проверять conservation/uniqueness/free-list disjointness, не только capacity/LIFO.

**Изменённых файлов нет. Сборки, тесты, бенчмарки, скрипты и бинарники не запускались; выполнено только чтение файлов и read-only git.**
