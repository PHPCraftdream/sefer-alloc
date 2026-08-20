# Предрелизный аудит `aligned-vmem 0.2.0`

- Автор: **Сол-кодекс**
- Время отчёта: **2026-08-20T07:39:08+02:00** (`Europe/Berlin`)
- Ревизия: `dc2ecdde1e400f6d0064840948843df399648abd` (`main`)
- Режим: статическое исследование только для чтения, без под-агентов
- Проверки исполнения: **не запускались** — без тестов, сборки, `cargo check`, Clippy, Miri, benchmark и `cargo publish --dry-run`

## Вердикт

**NO-GO для публикации в текущем состоянии.**

В обычном safe API библиотеки при статическом просмотре новой явной ошибки memory safety не найдено. Основной блокер находится в Windows-only тесте: он заведомо нарушает unsafe-контракт, выполняет out-of-bounds pointer arithmetic и при неудачном расположении адресного пространства способен decommit'нуть чужую живую область процесса. Кроме того, новый публичный `DecommitOutcome` имеет ложную OS-семантику под mock/miri, а README и rustdoc расходятся с фактическим API и уже добавленным HugeTLB-оракулом.

Сводка:

| Класс | Количество | Блокирует релиз |
|---|---:|---|
| Critical | 0 | — |
| High | 1 | да |
| Medium | 3 | да |
| Low | 3 | нет по отдельности |
| Performance | 3 | сначала измерить |
| Coverage | 3 | один связан с High |

## High

### H1. Тест `Refused` содержит UB и может повредить чужую память процесса

Файлы:

- `crates/aligned-vmem/tests/decommit_outcome.rs:273-331`
- `crates/aligned-vmem/src/os/windows.rs:347-374`

`refused_variant_is_produced_by_a_genuine_os_refusal` резервирует 2 MiB, затем вызывает free `try_decommit` с `far_start = 64 MiB`. Комментарий в самом тесте прямо признаёт нарушение `# Safety`: диапазон не находится внутри usable span.

Это опасно на двух уровнях:

1. Windows backend вычисляет адрес через `unsafe { base.add(start) }`. Для 2-MiB reservation вызов `base.add(64 MiB)` выходит далеко за объект; отсутствие последующего чтения не делает такую pointer arithmetic допустимой.
2. Если вычисленный адрес попадает в другую `VirtualAlloc` reservation процесса, `VirtualFree(..., MEM_DECOMMIT)` может успешно decommit'нуть её страницы. Сам тест уже документирует аналогичный состоявшийся инцидент на Linux: `madvise(MADV_DONTNEED)` нашёл чужое отображение и отбросил его содержимое. Windows gate устранил Linux-проявление, но не сам дефект.

Это test-only дефект, а не найденная дыра normal safe API, но такой тест нельзя оставлять в публикуемом репозитории и CI.

Рекомендация: удалить арифметику «достаточно далеко от allocation» и сделать детерминированный отказ через seam непосредственно перед `decommit_pages_impl`/OS wrapper. Mock должен уметь сценарно вернуть OS-side refusal, либо нужен отдельный test-only fault-injection cfg для decommit. Вариант «reserve → release → вызов по освобождённому адресу» лучше текущего, но всё ещё несёт race с повторным отображением адреса и не является таким же надёжным, как контролируемый backend result.

## Medium

### M1. Публикационная документация описывает старый API и старую силу доказательств

Основные места:

- `crates/aligned-vmem/README.md:54-56` — `try_decommit` всё ещё указан как `Result<(), VmemError>` и `Ok(())`;
- `crates/aligned-vmem/CHANGELOG.md:53-74` — старое additive-описание и старые `Ok(())`/«OS refusal не наблюдается» противоречат актуальному breaking-entry на строках 111-139;
- `crates/aligned-vmem/README.md:213-258` — утверждается, что HugeTLB-память после decommit не перечитывается;
- `crates/aligned-vmem/src/api/decommit.rs:104-138`, `src/api/reserve_aligned_huge.rs:109-138`, `src/decommit_outcome.rs:55-62` — тот же устаревший claim и ссылка на task #1174 как на ещё открытый gap;
- `crates/aligned-vmem/src/reservation.rs:303-311,742-769,1220-1229` — остаются безусловные формулировки «huge decommit silently fails» и «task #1174 ещё закрывает gap»;
- `crates/aligned-vmem/CHANGELOG.md:292-355` — старый `Ok(())` и отсутствие content-readback описаны уже после раздела с новым API.

Но `crates/aligned-vmem/tests/decommit_capability.rs:1034-1105` уже пишет `0xAB`, делает eligible HugeTLB decommit и проверяет каждый байт на ноль; `.github/workflows/ci.yml:513-531,553-590` жёстко включает этот test и marker в real-HugeTLB job. Не доказан только физический возврат страниц в pool/RSS: `HugePages_Free` намеренно остаётся наблюдением, а не gate.

README является front page на crates.io, поэтому это не косметика: пользователь увидит неверную сигнатуру и неверную модель поведения памяти.

Рекомендация: перед релизом нормализовать README, rustdoc и единую секцию `0.2.0` CHANGELOG по финальному состоянию. В changelog убрать исторически промежуточные утверждения из `Added`/`Changed` либо явно сделать их последовательной migration history без противоречия итоговому API.

### M2. `DecommitOutcome::Advised` обещает OS acceptance там, где OS-вызова нет

Файлы:

- `crates/aligned-vmem/src/decommit_outcome.rs:10-24,51-66`
- `crates/aligned-vmem/src/lib.rs:62-75`
- `crates/aligned-vmem/src/api/decommit.rs:297-320`
- `crates/aligned-vmem/src/os/miri.rs:45-57`

Публичный контракт говорит, что `Advised` означает: backend call сделан, kernel/OS принял его. Однако:

- под `aligned_vmem_mock` syscall не выполняется, запись в call log безусловно возвращает `Advised`;
- под miri backend является no-op и возвращает `Ok(())`, которое также преобразуется в `Advised`;
- crate-level docs без оговорки утверждают, что payload показывает, «what the OS actually did».

Из-за этого тестовый consumer может интерпретировать `Advised` как доказательство реального OS acceptance, хотя это лишь принятие запроса заменяющим backend.

Рекомендация: до первой публикации выбрать контракт. Минимальный совместимый вариант — определить `Advised` как «выбранный backend принял запрос», отдельно указав, что native backend соответствует успешному syscall, а mock/miri лишь симулируют принятие. Если различие должно быть программно наблюдаемым, добавить отдельный вариант вроде `Simulated` сейчас, пока API ещё не опубликован.

### M3. Публичный контракт adoption для HugeTLB оставляет предрелизное решение открытым

Файл: `crates/aligned-vmem/src/reservation.rs:1220-1282,1393-1467`.

Новые проверки существенно улучшили `from_raw_parts`: `granted_huge=true` требует feature и на Linux/Android проверяет 2-MiB shape. Но 1-GiB HugeTLB mapping также кратен 2 MiB и проходит все эти asserts, хотя decommit eligibility продолжает рассчитываться с гранулярностью 2 MiB. Защита поэтому проверяет форму, но не может доказать реальную гранулярность backing.

Rustdoc прямо оставляет вопрос поддержки 1-GiB adoption «not yet answered by the crate owner». Для первого публичного релиза это незавершённое решение на unsafe boundary.

Рекомендация: формально закрыть решение до публикации:

- если поддерживается только crate-native `MAP_HUGE_2MB`, назвать 1-GiB adoption неподдерживаемым correctness-contract violation и убрать формулировку открытого вопроса;
- если произвольная HugeTLB granularity входит в scope, хранить её в metadata и использовать для decommit/release checks до стабилизации round-trip API.

## Low

### L1. Free `try_decommit` дважды читает page-size cache, а комментарий утверждает «ровно один раз»

`crates/aligned-vmem/src/api/decommit.rs:254-256,259-281,372-393`:

- строка 381 вызывает `page_size_or_poison()`;
- затем `decommit_range_is_well_formed` вызывает его снова;
- `dispatch_try_decommit` вообще не читает page size, вопреки собственному комментарию на строках 261-276.

Для normal production cache стабилен, поэтому это прежде всего лишний atomic load и неверная внутренняя документация. Исправление: получить `ps` один раз и передать его в predicate либо валидировать endpoints inline, как делает safe method.

### L2. `decommit` не собирает полный unsafe-контракт внутри `# Safety`

`crates/aligned-vmem/src/api/decommit.rs:26-38` упоминает «within the span» до заголовка, но сам `# Safety` говорит только о live base и ненужных данных. Backend затем использует `base.add(start)`, то есть in-bounds — обязательная предпосылка Rust pointer arithmetic, а не только функциональное требование.

Рекомендация: повторить в `# Safety` явное `end <= reservation.len()` и владение всем диапазоном. Для unsafe API небольшое дублирование лучше хрупкой ссылки на соседний абзац.

### L3. Raw ownership tokens не помечены `#[must_use]`

`ReservationParts` и `ReservationFullParts` владеют единственной информацией, позволяющей освободить mapping, но простой drop этих структур молча течёт. Методы extraction помечены `#[must_use]`, однако полезно пометить и сами типы сообщением вроде «dropping these parts leaks the reservation». Это не предотвращает намеренный leak, но ловит больше случайных потерь ownership token.

## Возможности ускорения

### P1. Повторный заведомо более дорогой `MAP_HUGETLB` после miss exact-fast-path

В `crates/aligned-vmem/src/os/unix.rs:134-168,202-235` запрос с `align == 2 MiB` сначала делает exact-size huge `mmap(size)`. После отказа generic path снова делает huge `mmap(size + align)`, и лишь затем ordinary fallback. На host без доступного hugetlb pool один логический запрос платит за два ожидаемо неуспешных HugeTLB syscall, причём второй просит больше дефицитных страниц.

Рекомендация: сохранять причину первого отказа и для устойчивых ошибок (`ENOMEM`, unsupported/permission/configuration) сразу идти в ordinary fallback; transient-классы при необходимости оставить retry. Сначала измерить syscall count/latency на профиле без pool.

### P2. `align > 2 MiB` удерживает `size + align` дефицитного HugeTLB pool

Generic Unix path сохраняет всю over-reserved mapping. Для `size == align == 4 MiB` полезные 4 MiB занимают 8 MiB pool. Новый harness (`decommit_capability.rs:1390+`) хорошо фиксирует существующую форму и syscall count, но это по-прежнему реальная capacity/performance цена.

Оптимизация требует осторожности: post-map head/tail trim должен соблюдать huge-page granularity и не возвращать старую multi-`munmap` leak-проблему. Рассматривать exact-address strategy или гранулярный trim только после pool-pressure benchmark.

### P3. Windows unprivileged large-page path имеет каскад неуспешных попыток

Для 64 KiB < `align` <= `GetLargePageMinimum()` без `SeLockMemoryPrivilege` путь может выполнить large-page attempt, ordinary retry, обнаружить недостаточную alignment, освободить её и перейти в two-call reserve+commit. Профилирующий example существует (`examples/v1189_windows_large_page_native_profile.rs`), но CI не запускает привилегированный granted-large-page режим.

Возможные направления после измерения: дешёвый privilege/capability precheck с cache, более узкий fast-path threshold для unprivileged режима или сохранение текущей best-effort стратегии, если реальная цена мала.

## Coverage gaps

### C1. Нет безопасного детерминированного оракула `Refused`

Текущий единственный real-OS oracle и есть H1. На Unix `Refused` не покрыт; mock/miri по конструкции всегда дают `Advised`. Нужен scripted refusal seam, который проверит mapping `Err(e) -> DecommitOutcome::Refused(e)` без выхода за owned range.

### C2. HugeTLB zero-fill покрыт, физический reclaim — нет

Read-zero postcondition теперь проверяется жёстко. `HugePages_Free`/RSS остаются внешними глобальными метриками и логируются без assert, поэтому фактический возврат физических huge pages конкретным decommit не доказан. Это честно допустимый gap, если release notes перестанут утверждать, что не проверяется также zero-fill.

### C3. Windows `MEM_LARGE_PAGES` granted path не является hard CI path

Репозиторий покрывает unprivileged fallback и симулирует branch metadata, но не имеет CI host с включённым `SeLockMemoryPrivilege`, который hard-assert'ит `is_huge() == true`, decommit refusal и release именно реальной large-page mapping. До появления такого runner этот путь остаётся reasoned/profiled, а не регулярно execution-verified.

## Что исправления предыдущей волны действительно закрыли

По сравнению с ревизией предыдущего отчёта `000c0767416b96df11aa7bbb8b80efb4c09cb754`:

- `from_raw_parts` разделил memory-safety и correctness contracts;
- adoption с `granted_huge=true` больше не меняет поведение молча при выключенном `huge-pages`;
- Linux/Android adoption получил ранние 2-MiB shape asserts;
- `try_decommit` теперь возвращает `DecommitOutcome`, а Unix/Windows wrappers сохраняют syscall refusal;
- page-size poison больше не превращается в debug-only panic;
- MSRV gate теперь явно охватывает `aligned-vmem --all-features` и test compile (`--no-run`);
- real-HugeTLB CI добавил write → decommit → read-zero и отдельный release-attempt oracle;
- release paths получили attempt/failure counters.

То есть предыдущие четыре Medium-находки не следует механически переносить: три закрыты по существу, а вопрос huge granularity сузился до явного предрелизного design decision.

## Рекомендуемый release gate

1. Удалить H1 и заменить его детерминированным refusal seam.
2. Свести README, rustdoc и CHANGELOG к фактическому `DecommitOutcome` и реальному уровню HugeTLB coverage.
3. Уточнить семантику `Advised` под native/mock/miri или добавить отдельный вариант.
4. Записать окончательное решение по не-2-MiB adopted HugeTLB mappings.
5. После исправлений выполнить новый независимый аудит; уже затем — обычные package/CI gates, которые в рамках этого read-only исследования намеренно не запускались.

После пунктов 1-4 crate по статической структуре выглядит близким к публикации. Performance-направления P1-P3 разумно не включать в обязательный correctness gate, если их текущая цена явно отражена в документации.

## Граница исследования

Статически просмотрены `Cargo.toml`, публичные exports и rustdoc, reservation/lazy ownership, raw-parts round trips, decommit/recommit/commit/release API, Unix/Windows/miri/mock backends, error model, arithmetic/align checks, feature/cfg matrix, README, CHANGELOG, тестовые оракулы, package/CI/MSRV/docs.rs gates и дельта от предыдущего аудита. Модули async/crypto/network не активировались: crate их не содержит. Во время работы HEAD продвинулся с `22a91cc` до `dc2ecdd`; дополнительный commit меняет только комментарии/guards вокруг теста и не устраняет H1.

Никакие исполняемые проверки не проводились. Выводы — результат чтения исходников, а не подтверждение runtime-поведением на текущей ревизии.
