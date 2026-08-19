# Новое статическое исследование `aligned-vmem` перед публикацией

Автор: **Сол-кодекс**

Время отчёта: **2026-08-19 21:48:52 +02:00 (Europe/Berlin)**

Ревизия: `000c0767416b96df11aa7bbb8b80efb4c09cb754`

Версия крейта: `0.2.0` (unreleased)

Режим: только чтение исходников и конфигурации; без под-агентов; без запуска тестов, сборки, `cargo check`, Clippy, Miri, benchmark и `cargo publish --dry-run`.

## Итог

**Вердикт: NO-GO для публикации `aligned-vmem 0.2.0` в текущем виде.**

Критических или явно memory-unsafe ошибок в обычном safe-пути создания и уничтожения `Reservation` статически не найдено. Проверенные пути аккуратно ограничивают арифметику, сохраняют provenance, захватывают OS error до cleanup, линейно передают владение и fail-closed обрабатывают неизвестный runtime page size.

Публикацию блокируют четыре проблемы публичного контракта уровня Medium. Они особенно дороги после первого релиза, потому что исправление затрагивает `Reservation::from_raw_parts`, модель huge mapping и семантику результата `try_decommit`. Все четыре уже перечислены в `docs/CORRECTNESS_OPEN_ITEMS.md`, item 90, но на исследованной ревизии остаются открытыми.

| Уровень | Количество | Итог |
|---|---:|---|
| Critical | 0 | Не найдено |
| High | 0 | Не найдено |
| Medium | 4 | Блокируют первый публичный релиз |
| Low | 4 | Желательно исправить до релиза |
| Performance | 3 | Одна существенная экономия HugeTLB pool, две задачи на измерение/рефакторинг |
| Coverage | 2 | Нет прямого postcondition для reclaim/zero и release HugeTLB |

Это статический аудит, а не доказательство отсутствия дефектов. Фактическое прохождение существующих CI gates в рамках исследования не проверялось по прямому запрету пользователя.

## Что изменилось после предыдущего исследования

Предыдущий аудит был зафиксирован коммитом `967b821` и исследовал `cecdeec`. Между `cecdeec` и текущим `HEAD` изменены пять относящихся к теме файлов (`+161/-56`): CI, README, changelog, `tests/decommit_capability.rs` и индекс открытых вопросов. **Файлы `crates/aligned-vmem/src/**` не изменялись.**

Полезные улучшения:

- real-HugeTLB CI теперь логирует kernel version и `Hugepagesize`;
- path-activation sentinels отделены от `--nocapture`, чтобы вывод параллельного libtest не разрывал строки, проверяемые `grep`;
- oracle требует `unix_madvise_successes() == unix_madvise_attempts()` после доказанного ненулевого числа попыток, то есть ловит частичный отказ будущего multi-call dispatch;
- README/changelog точнее отделяют «ядро приняло `madvise`» от фактического reclaim/zero-fill;
- открытые проблемы прошлого аудита занесены в общий индекс с владельческими рекомендациями.

Эти изменения усиливают доказательную базу, но не исправляют runtime/API-проблемы ниже.

Две поправки к формулировкам предыдущего отчёта:

1. `Drop` не полностью «теряет» ошибку release: в сборке с `bench-internals` неуспешный Unix `munmap` учитывается в `UNIX_MUNMAP_FAILURES`, а Windows release — в соответствующем счётчике. В обычном публичном API результат всё равно не наблюдаем.
2. Crate-level rustdoc говорит о парных формах для **reservation/commit entry points**, поэтому отсутствие `try_release`/`try_decommit_lazy` само по себе не противоречит этой фразе. Реальная неточность уже: утверждение, что infallible-формы вызывают `try_*`, неверно для семейства decommit.

## Блокирующие находки

### M1 — `from_raw_parts` не описывает гранулярность принятого HugeTLB mapping

Затронуто:

- `crates/aligned-vmem/src/reservation.rs:1021-1168,1217-1249`;
- `crates/aligned-vmem/src/os/unix.rs:50-89,450-519,1084-1109`;
- `ReservationFullParts` и RAII `Drop`.

Публичный unsafe-конструктор хранит huge-состояние одним `granted_huge: bool`. Для mapping, созданного самим крейтом, это достаточно: Linux/Android path явно запрашивает `MAP_HUGE_2MB`. Для произвольного принятого mapping bool не кодирует фактический huge-page size и release/decommit granularity.

Rustdoc утверждает, что на native Unix `reservation_len`, недостающий меньше одной runtime page, безвреден из-за округления `munmap`. Это неверно для HugeTLB: Linux требует, чтобы и адрес, и длина `munmap` были кратны размеру underlying huge page. При `granted_huge == true` конструктор проверяет только кратность `PAGE` (4 KiB), но не 2 MiB и не фактическому huge-page size. В итоге некорректный `reservation_len` может заставить `munmap` вернуть `EINVAL` и оставить весь mapping, включая pinned huge pages. Системный контракт: [Linux mmap/munmap — Huge TLB mappings](https://man7.org/linux/man-pages/man2/munmap.2.html).

Отдельно принятый 1-GiB HugeTLB mapping нельзя честно обслужить текущей 2-MiB eligibility-проверкой: диапазон, кратный 2 MiB, может быть недопустим для его реальной гранулярности.

Почему блокер: тип обещает exact-once RAII ownership/release, а публичный способ принятия foreign mapping не несёт данных, необходимых для выполнения этого обещания. Исправление после публикации вероятно потребует semver-изменения.

Рекомендуемый минимальный вариант для `0.2.0`:

- сохранить bool, но определить `true` строго как поддерживаемый крейтом 2-MiB формат;
- на Linux/Android принимать `true` только с feature `huge-pages`;
- проверять кратность 2 MiB для `reservation`, `base`, `reservation_len`, `len` и смещения `base - reservation`;
- явно оговорить HugeTLB-исключение из обычного правила округления `munmap`;
- если нужна поддержка произвольной гранулярности, заменить bool на типизированные metadata до первого релиза.

### M2 — поведение уже принятого huge mapping зависит от Cargo feature

Затронуто: `Reservation::from_raw_parts`, `is_huge`, `decommit`, `try_decommit`, `decommit_lazy`.

`from_raw_parts(..., granted_huge: true)` и `is_huge()` существуют без `huge-pages`, но Linux/Android eligibility helper компилируется только с этой feature. Одинаковый внешний HugeTLB mapping поэтому:

- с feature может дойти до eager `MADV_DONTNEED` на допустимом диапазоне;
- без feature гарантированно возвращается через skip-path.

Feature разумно включает **создание** huge mapping, но не должна молча менять обслуживание уже принятого ресурса. Cargo feature unification дополнительно делает поведение зависимым от состава всего dependency graph.

Rustdoc на `reservation.rs:637` предлагает без feature использовать `decommit_lazy`; это неверно: `decommit_lazy` безусловно пропускает huge mapping, поскольку `MADV_FREE` для HugeTLB не заявлен.

Рекомендация: либо собирать корректное обслуживание принятого 2-MiB mapping независимо от feature, либо запретить `granted_huge == true` без feature. Второй вариант естественно сочетается с минимальным решением M1.

### M3 — `from_raw_parts # Safety` смешивает UB-предусловия с функциональной корректностью

Затронуто: `reservation.rs:1021-1160` и тесты synthetic `granted_huge`.

Раздел `# Safety` одновременно требует:

- реальную liveness/exclusive ownership/provenance и exact-once release — их нарушение действительно может сделать safe API или `Drop` memory-unsafe;
- точность `granted_huge` и форму Windows commit state — их нарушение в описанных сценариях меняет capability/dispatch, приводит к no-op или утечке, но не обязательно вызывает UB.

Тесты намеренно создают ordinary mapping с synthetic `granted_huge == true` и отдельно объясняют, почему это не UB. Значит, тестовый oracle и публичный `# Safety` дают несовместимые определения допустимости.

Рекомендация: оставить в `# Safety` только условия memory safety, а huge/commit-state требования вынести в отдельный `Correctness contract` с точными последствиями нарушения. Не объявлять тестируемое поведение UB только ради согласования текста.

### M4 — `try_decommit() -> Result<(), VmemError>` сообщает в основном о валидации, а не о decommit

Затронуто:

- `crates/aligned-vmem/src/api/decommit.rs:234-278`;
- `crates/aligned-vmem/src/reservation.rs:381-412,751-820`;
- Unix `libc_madvise` и Windows `VirtualFree(MEM_DECOMMIT)` wrappers.

`try_decommit` возвращает `Err` для poison/range-contract failure, но backend возвращает `()` и отбрасывает результат syscall. Поэтому `Ok(())` одновременно означает:

- пустой корректный диапазон;
- huge-path был пропущен;
- syscall был вызван и отклонён;
- syscall был принят.

При `bench-internals` часть различий видна глобальными счётчиками, но это диагностическая feature, не outcome конкретного публичного вызова. Rustdoc `can_decommit_reclaim_and_zero` при этом советует для конкретного huge-range «call decommit/try_decommit and judge by outcome», хотя по outcome судить нельзя. Свободный `try_decommit` одновременно честно пишет, что OS refusal намеренно не является ошибкой. Linux действительно возвращает наблюдаемый результат `madvise`; при успехе `MADV_DONTNEED` для private anonymous mappings последующий доступ получает zero-filled pages, а HugeTLB поддерживается с Linux 5.18 при нужной гранулярности: [Linux madvise(2)](https://man7.org/linux/man-pages/man2/madvise.2.html). Windows `VirtualFree` также возвращает success/failure: [Microsoft VirtualFree](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualfree).

Рекомендация до фиксации API:

- ввести `DecommitOutcome`, например `Skipped`, `Advised`, `Refused(VmemError)`;
- не называть `Advised` гарантией немедленного RSS reclaim;
- свести method/free, infallible/fallible пути к одному private dispatch, чтобы не повторять validation и не расходиться семантически;
- если API оставляется как есть, переименовать его в форму, явно сообщающую только о валидности запроса, и удалить совет «judge by outcome».

## Low и maintainability

### L1 — poison-state документирован как no-op, но debug-сборка паникует

`api/decommit.rs:166-173` выполняет `debug_assert!(false)` при неуспешном page-size query. В release это no-op, в debug — panic. `page_size` rustdoc и README говорят «decommit becomes a no-op» без build-profile оговорки, а `Reservation::decommit # Panics` перечисляет range violations, но не этот достижимый из safe API panic.

Нужно выбрать и закрепить один контракт: предпочтительно явно документировать debug panic/release no-op и добавить будущий oracle; fallible путь уже умеет вернуть ошибку.

### L2 — `VmemError` недостаточно различает no-code причины

Тип различает invalid argument и остальное, но все остальные причины с `code == None` сводятся к `os_refusal_unknown_code`: недоступный OS code, отклонённый address-zero grant, failed page-size query, fault injection и mock failure. Rustdoc заявляет ровно «FOUR sources», хотя реальные call sites шире списка; имя «OS refusal» неверно для нескольких из них.

Пока `0.2.0` не опубликован, лучше ввести явный kind/enum (`Os`, `RejectedGrant`, `PageSizeUnavailable`, `Injected`, `InvalidArgument`) либо хотя бы нейтральный различимый no-code kind.

### L3 — MSRV 1.88 не компилирует package-specific `huge-pages`

CI `msrv` запускает root-scoped `cargo check --all-features` и `cargo test --no-run --all-features`, но root manifest не форвардит `aligned-vmem/huge-pages`. Отдельного `-p aligned-vmem --all-features` на toolchain 1.88 нет. Stable CI этот код компилирует, но заявленный MSRV для него не доказан. Это уже открытый item 88.

Нужен узкий compile-only row: `cargo check -p aligned-vmem --all-features` и желательно `cargo test -p aligned-vmem --no-run --all-features` на 1.88.

### L4 — пользовательские комментарии и rustdoc всё ещё расходятся с кодом

Примеры:

- `lib.rs:62`: «infallible forms forward to try_*» — decommit family устроено наоборот или имеет отдельную validation;
- `api/decommit.rs:248-254`: huge operation названа «incompatible outright», хотя соседний rustdoc уже описывает Linux >= 5.18 eligible path;
- `reservation.rs:637`: неверный совет использовать `decommit_lazy` для huge mapping;
- `Drop`-комментарий `reservation.rs:1262` утверждает, что reservation получена через `reserve_aligned`, игнорируя публичный `from_raw_parts`;
- длинные исторические task-number narratives внутри runtime-модулей уже несколько раз устаревали и затрудняют проверку текущего инварианта.

Перед релизом полезно вынести историю решений в design/open-items docs, а рядом с unsafe-кодом оставить краткие актуальные proof comments и ссылки на системный контракт.

## Производительность

### P1 — `align > 2 MiB` может удерживать до 2× HugeTLB pool

На 64-bit Unix exact huge fast path работает только при `align == 2 MiB`. Для большего alignment общий путь отображает `size + align` и сохраняет весь mapping. Пример: `size = align = 4 MiB` резервирует 8 MiB, то есть четыре 2-MiB huge pages ради двух полезных. Для HugeTLB это не просто дешёвое виртуальное адресное пространство, а ограниченный заранее выделенный pool.

После исправления huge-кратности head/tail можно технически отрезать целыми huge pages, сократив steady-state occupancy, но первоначальный `mmap(size + align)` всё равно требует увеличенный pool и может преждевременно перейти на ordinary fallback. Для устранения admission-cost нужна exact-size/aligned стратегия, а не только trimming. Сначала измерить pool occupancy, fallback rate и цену дополнительных syscalls на real-HugeTLB runner.

### P2 — decommit family повторяет page-size loads и validation

Safe `Reservation::try_decommit` читает poison/page size и валидирует диапазон, затем free `try_decommit`, а затем infallible `decommit` повторяют часть работы. В steady state это несколько relaxed atomic loads. Рядом с syscall цена мала, но заметна для empty/invalid/skipped вызовов. Удалять точечно не стоит; объединить с M4 в единый private dispatch с одним snapshot `page_size`.

### P3 — speculative platform paths требуют измерения

- Linux huge exact miss затем пробует более крупный huge mapping до ordinary fallback. Это может быть полезно при transient/fragmentation, поэтому не удалять без счётчиков.
- Windows large-page request может оплатить failed large allocation, ordinary retry, release из-за misalignment и two-call fallback. Нужен Windows-native профиль уже существующих counters.
- Generic 64-bit Unix over-reserve осознанно меняет один syscall на больший VA span; не возвращать generic retry без workload evidence.

## Coverage gaps

### C1 — real HugeTLB доказывает acceptance, но не postcondition

Текущий CI теперь хорошо доказывает реальный `MAP_HUGETLB` grant, dispatch и `madvise == 0`. Он не выполняет write → decommit → read/recommit → read-zero на этом mapping и не наблюдает RSS/`HugePages_Free`. «Kernel accepted advice» не тождественно «ресурс уже возвращён». README теперь честно признаёт границу.

Следующее усиление: отдельные оракулы на zero-fill последующего доступа и на pool/RSS reclaim, без объединения этих двух свойств в один flaky threshold.

### C2 — release HugeTLB не проверяется прямым oracle

`UNIX_MUNMAP_FAILURES` существует, но реальные HugeTLB tests не проверяют его вокруг Drop. Имеющийся smoke-тест счётчика проверяет только reset-to-zero. Комментарий в `unix.rs` предполагает, что leak проявится как test failure/resource exhaustion, но pool содержит много страниц, а число одновременных mapping невелико; отсутствие release не обязано сделать job красным.

Нужен release-attempt/success oracle либо наблюдение `HugePages_Free` до создания и после Drop. Проверка только «failure counter остался нулём» не ловит случай, когда вызов release исчез полностью.

## Положительные результаты

- `size + align`, span bounds и `Layout` проверяются checked-арифметикой и `isize`-ограничениями.
- Pointer arithmetic использует `addr`/`with_addr`, не восстанавливает provenance из произвольного integer.
- `errno`/`GetLastError` захватываются непосредственно после failing syscall и до cleanup.
- Владение `Reservation` линейно: один `Drop`; `into_*` подавляет Drop; safe state-mutating methods требуют `&mut self`; тип `Send`, но не `Sync`.
- Windows failure paths освобождают временную reservation; commit watermark меняется только после успешного commit.
- Page-size query failure кэшируется как poison и fail-closed блокирует page-state операции.
- Созданные самим крейтом Linux huge mappings явно используют `MAP_HUGE_2MB`; допустимый eager decommit соответствует Linux >= 5.18 contract.
- Runtime dependency list пуст; feature/docs.rs metadata явны.
- Статически видна сильная CI-матрица: default/all-features, real/mock, debug/release, Miri, i686 GNU/musl, macOS/Windows, docs, packaging, semver и real HugeTLB.
- В проверенном safe-created пути не найдено подтверждённых integer overflow, double release или memory corruption.

## Рекомендуемый порядок исправлений

1. Зафиксировать поддерживаемую модель adopted huge mapping и исправить M1+M2.
2. Разделить `from_raw_parts` memory-safety и correctness contracts (M3); синхронизировать round-trip types/tests.
3. До публикации решить семантику decommit outcome и объединить dispatch (M4, затем P2).
4. Исправить poison panic docs, `VmemError`, stale rustdoc/comments и MSRV all-features gate (L1-L4).
5. Добавить прямые HugeTLB postcondition/release oracles (C1-C2).
6. Измерить P1/P3 на целевых OS, затем оптимизировать; P1 наиболее вероятно даёт практический выигрыш в ограниченном pool.
7. Только после этих правок выполнить полный release matrix, Miri, docs, packaging и publish dry-run — этот аудит их не запускал.

## Граница исследования

Аудит выполнен одним контекстом без под-агентов по явному требованию пользователя. Использован bounded-проход rust-intel по unsafe/FFI, RAII, API/lifetimes, arithmetic/types, features/dependencies, testing, semantic conformance и concurrency/state. Модули async и crypto/security не исследовались глубоко: в крейте нет async runtime, сетевого протокола, криптографии или обработки секретов, которые активировали бы эти направления.

Прочитаны исходники крейта, Unix/Windows/miri/mock backends, публичный rustdoc, manifest, README/changelog, относящиеся тестовые оракулы, CI и correctness open items. Внешние системные утверждения сверены с Linux man-pages и Microsoft Learn, ссылки приведены рядом с находками.

Единственная запись, сделанная исследованием, — этот файл. Существовавший до начала untracked-файл `docs/checkpoints/2026-08-19-2140.md` не изменялся.
