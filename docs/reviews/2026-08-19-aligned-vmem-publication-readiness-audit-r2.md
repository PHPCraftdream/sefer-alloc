# Новое статическое исследование `aligned-vmem` перед публикацией

Дата: 2026-08-19  
Ревизия: `cecdeec9593282de55f88e5d53f79f7668da4526`  
Предыдущая база сравнения: `1ed79e96`  
Режим: только чтение исходников и конфигурации; без под-агентов; без запуска тестов, сборки, `cargo check`, Clippy, Miri, benchmark и `cargo publish --dry-run`.

## Итог

**Вердикт: пока NO-GO для публикации `0.2.0`.**

Исправления после предыдущего исследования существенные и закрывают оба его главных функциональных дефекта: ошибка запроса системного размера страницы теперь кэшируется как poison и валидаторы fail-closed; eager-decommit HugeTLB на Linux/Android теперь может доходить до `MADV_DONTNEED` для допустимого 2-MiB диапазона. Release-profile CI с реальным backend тоже добавлен.

Однако до фиксации публичного API желательно не публиковать версию: у принятой через `Reservation::from_raw_parts` HugeTLB-карты недостаточно метаданных для честного управления её жизненным циклом, поведение такой карты зависит от Cargo feature потребителя, `# Safety` смешивает требования безопасности с функциональными обещаниями, а `try_decommit() -> Result<(), VmemError>` не сообщает результат decommit, хотя документация местами предлагает использовать его именно так. После публикации исправление этих мест может потребовать несовместимого изменения API.

В проверенном safe-пути резервирования/освобождения не найдено подтверждённой memory corruption, double-free или арифметического переполнения. Это результат статического исследования, а не доказательство их отсутствия.

Сводка новых находок:

| Уровень | Количество | Значение |
|---|---:|---|
| Critical | 0 | Не найдено |
| High | 0 | Не найдено |
| Medium | 4 | Блокируют публикацию API `0.2.0` |
| Low | 3 | Желательно исправить до релиза |
| Performance / coverage | 3 | Измерить или усилить после контрактных исправлений |

## Что изменилось относительно прошлого исследования

| Предыдущая находка | Новый статус |
|---|---|
| H1: `page_size()` fail-open при ошибке OS-query | **Закрыта функционально.** Введены poison-состояние, `try_page_size()` и fail-closed проверки диапазонов. Осталась документационная неточность про debug panic — L1 ниже. |
| M1: HugeTLB decommit всегда пропускался | **Закрыта для созданных самим крейтом 2-MiB HugeTLB mappings.** Для adopted mapping остались M1/M2. |
| L1: неоднозначная таксономия `VmemError` | **Закрыта не полностью.** См. L2. |
| L2: CI проверял release-семантику только через mock | **Закрыта.** В `.github/workflows/ci.yml` есть release/all-features запуск с real backend. |

Между базой и текущим `HEAD` изменены 28 относящихся к исследованию файлов (`+2444/-285`), поэтому исправленные участки перечитаны как новая реализация, а не приняты по changelog.

## M1 — `from_raw_parts` не может полностью описать произвольный HugeTLB mapping

Затронуто:

- `crates/aligned-vmem/src/reservation.rs`: `Reservation::from_raw_parts`, `Drop`;
- `crates/aligned-vmem/src/os/unix.rs`: `release_reservation`, `LINUX_HUGE_PAGE_SIZE`, `MAP_HUGE_2MB`;
- `crates/aligned-vmem/src/reservation_full_parts.rs`.

Сейчас huge-состояние хранится одним `granted_huge: bool`. Для mappings, созданных самим крейтом, этого достаточно: крейт явно запрашивает `MAP_HUGE_2MB`, и размер известен как 2 MiB. Для произвольного mapping, принятого через публичный `unsafe fn from_raw_parts`, одного bool недостаточно: он не кодирует фактический huge-page size и требования к освобождению.

Linux требует для HugeTLB, чтобы и адрес, и длина `munmap()` были кратны размеру underlying huge page. Обычный `munmap()` допускает некратную page-size длину, поэтому существующее общее rustdoc-утверждение, что немного укороченный `reservation_len` безвреден из-за округления ядром, нельзя переносить на HugeTLB без отдельной оговорки. `Drop` безусловно передаёт сохранённый `reservation_len` в backend и игнорирует результат `munmap`; ошибочная huge-гранулярность превращается в тихую утечку mapping.

Первичный системный контракт: [Linux `mmap(2)` / `munmap(2)`](https://man7.org/linux/man-pages/man2/munmap.2.html), раздел Huge page (Huge TLB) mappings.

Почему это блокер до публикации: публичный тип уже обещает exact-once RAII release, а текущая форма adopted-метаданных не позволяет проверить или сохранить всё необходимое для произвольного HugeTLB mapping.

Рекомендация:

1. Либо запретить `granted_huge == true` для `from_raw_parts` и дать отдельный конструктор только для точно поддерживаемого 2-MiB формата.
2. Либо заменить bool на типизированные метаданные (`HugePageKind`, `Option<NonZeroUsize>` или эквивалент), которые несут фактическую гранулярность release/decommit.
3. Явно включить huge-кратность `reservation`, `reservation_len`, `base`, `len` в контракт и не заявлять обычное округление `munmap` для HugeTLB.
4. Решить, должен ли неуспешный release оставаться не наблюдаемым; как минимум debug/diagnostic path не должен терять ошибку бесследно.

## M2 — поведение принятой huge-карты меняется от feature потребителя

Затронуто:

- `crates/aligned-vmem/src/reservation.rs`: `is_huge`, `decommit`, `try_decommit`, `decommit_lazy`;
- `crates/aligned-vmem/src/os/mod.rs` и `src/os/unix.rs`: feature-gated `huge_decommit_range_is_eligible`.

`from_raw_parts(..., granted_huge: true)` и `is_huge()` доступны независимо от feature `huge-pages`. Но ветка, которая разрешает eligible HugeTLB `MADV_DONTNEED`, компилируется только с `feature = "huge-pages"`. Одна и та же реально существующая adopted-карта поэтому:

- с feature может дойти до backend на Linux/Android >= 5.18;
- без feature гарантированно пропускается, даже если mapping и диапазон допустимы.

Feature уместно использовать для включения конструктора, который **создаёт** huge mapping. Оно не должно менять корректное обслуживание уже принятого mapping. Cargo feature-unification также делает это поведение зависимым от состава dependency graph приложения.

Дополнительная ошибка рекомендации: rustdoc предлагает для отключённого huge-path использовать `decommit_lazy`, но этот метод для `is_huge()` безусловно пропускает backend, потому что `MADV_FREE` для HugeTLB не заявлен.

Рекомендация: компилировать распознавание и eager-decommit adopted HugeTLB на поддерживаемых OS независимо от feature создания либо не разрешать принимать huge mapping в сборке без соответствующей capability. `decommit_lazy` из рекомендации убрать.

## M3 — `# Safety` содержит требования, нарушение которых сами тесты считают не-UB

Затронуто:

- `crates/aligned-vmem/src/reservation.rs`: rustdoc `from_raw_parts`;
- `crates/aligned-vmem/tests/reservation_decommit_contract.rs`;
- `crates/aligned-vmem/tests/decommit_capability.rs`.

В `# Safety` перечислены не только условия владения, provenance, размера и lifetime mapping, но и точность `granted_huge`/Windows commit-state. Последние управляют функциональным выбором backend и наблюдаемым результатом, но не во всех случаях являются предусловиями memory safety. Интеграционные тесты намеренно строят synthetic `granted_huge` reservation с несоответствующим реальным mapping, отдельно объясняя, почему конкретное нарушение не вызывает UB.

Это противоречивый контракт: либо нарушение пункта `# Safety` действительно может вызвать UB и тесты не должны его нарушать, либо пункт должен находиться в разделе `Behavioral requirements`/`Correctness`, где описаны последствия — пропуск decommit, неверная capability или утечка.

Рекомендация: оставить в `# Safety` только условия, нарушение которых может сделать safe-методы или `Drop` memory-unsafe. Функциональные требования вынести отдельно и связать с типизированными huge-метаданными из M1.

## M4 — `try_decommit()` сообщает валидность аргументов, а не исход операции

Затронуто:

- `crates/aligned-vmem/src/api/decommit.rs`: `try_decommit`;
- `crates/aligned-vmem/src/reservation.rs`: `try_decommit`, `can_decommit_reclaim_and_zero`;
- `crates/aligned-vmem/src/lib.rs` и `README.md`.

Backend decommit возвращает `()`, результаты `madvise`/`VirtualFree` отбрасываются. Поэтому `Ok(())` означает лишь: page-size query доступен и диапазон прошёл проверку. Оно не различает:

- Rust-level skip для huge mapping;
- вызов, отклонённый ядром (`EINVAL`, старый kernel, и т. п.);
- принятый advisory syscall;
- фактическое возвращение RSS или последующее zero-fill.

При этом rustdoc `can_decommit_reclaim_and_zero()` предлагает судить о конкретном huge-диапазоне по результату `decommit`/`try_decommit` или счётчику. По return value это невозможно. `huge_decommit_attempts` существует только с `bench-internals` и считает Rust-level skips, а не универсальный syscall outcome.

Общий crate-level текст «каждый reservation/commit entry point имеет infallible и fallible форму» тоже неточен: `try_decommit` вызывает `decommit`, а не наоборот, не переносит OS error; у `decommit_lazy`, `release` и `leak_zeroed_pages` нет симметричных `try_*` форм.

Рекомендация до фиксации API:

- либо вернуть честный `DecommitOutcome` (`Skipped`, `AdviceAccepted`, `AdviceRejected(VmemError)`), отдельно объяснив, что accepted advice не гарантирует немедленный RSS reclaim;
- либо переименовать/задокументировать функцию как проверку контракта диапазона и не советовать её для определения результата операции;
- свести infallible/fallible пути к одному private implementation, чтобы семантика не расходилась.

## L1 — документация poison-состояния обещает no-op, но debug-сборка паникует

`page_size()` теперь корректно возвращает консервативный `PAGE` после неуспешного OS-query, а state-changing операции fail-closed. Однако `decommit()` на poison-состоянии вызывает `debug_assert!(false, ...)`: в debug это panic, в release — no-op.

`README.md`, changelog и rustdoc `page_size` описывают decommit как no-op без build-profile оговорки. `Reservation::decommit` в `# Panics` перечисляет нарушения диапазона, но не этот достижимый из safe API panic. Тест `page_size_query_failure.rs` проверяет `try_decommit`, но намеренно не вызывает infallible `decommit`, поэтому расхождение не закреплено оракулом.

Рекомендация: выбрать один контракт. Предпочтительно возвращать/распространять ошибку через fallible API, а infallible API документировать как debug panic + release no-op; добавить соответствующий `# Panics` и будущий тест.

## L2 — `VmemError` всё ещё смешивает разные причины `code == None`

`error.rs` утверждает, что любая ошибка — это raw OS code либо caller-contract sentinel. Фактически один no-code sentinel используется как минимум для:

- invalid arguments;
- OS отказа без доступного кода;
- нулевого/неподходящего OS grant;
- ошибки page-size query;
- fault-injection до системного вызова;
- mock backend failure.

Документация `os_refusal_unknown_code` обещает «FOUR sources», но не перечисляет все фактические call sites; комментарий fault-injection ошибочно относит injection к этим четырём. Имя «OS refusal» неверно для ошибки валидации, page-size query и искусственной инъекции.

Рекомендация для ещё не опубликованного API: явный enum (`InvalidArgument`, `Os { code: Option<u32> }`, `RejectedGrant`, `PageSizeUnavailable`, `InjectedFailure`) либо хотя бы нейтральный no-code kind. Это улучшит диагностику и не заставит затем ломать семвер.

## L3 — заявленный MSRV не проверяет собственный all-features профиль крейта

`Cargo.toml` заявляет `rust-version = "1.88"`. CI хорошо покрывает stable default/all-features, mock, i686 GNU/musl, docs.rs feature set, packaging и semver. Но root-scoped MSRV job не гарантирует включение `aligned-vmem/huge-pages`, а отдельного MSRV `cargo check -p aligned-vmem --all-features` нет; этот остаток прямо отмечен в CI-комментарии/`docs/CORRECTNESS_OPEN_ITEMS.md`.

Рекомендация: до релиза добавить отдельный compile-only MSRV gate для package all-features (и при возможности `--all-targets`). В рамках этого исследования он не запускался.

## Возможности ускорения и снижения расхода ресурсов

### P1 — granted HugeTLB при `align > 2 MiB` может тратить до 2× bounded pool

На 64-bit Unix общий путь отображает `size + align` и сохраняет mapping целиком. Для обычных страниц это в основном цена виртуального адресного пространства. Для `MAP_HUGETLB` весь span резервирует страницы из ограниченного pool.

Пример: `size = align = 4 MiB` при 2-MiB huge pages резервирует 8 MiB — четыре huge pages вместо двух полезных. Это может раньше исчерпать pool и заставить последующие запросы перейти на ordinary pages. Код и perf-open-item уже знают о цене, но open-item устарел в части отсутствия hugetlb CI-host: такой job теперь есть.

Возможное улучшение: для гарантированно huge-aligned mapping отрезать head/tail кратно 2 MiB либо применить exact-size strategy. Это нельзя принимать только по рассуждению: сначала измерить pool occupancy, число fallback и syscall cost на уже существующем real-hugetlb CI host.

### P2 — лишние проверки/atomic loads в `try_decommit`

Safe `Reservation::try_decommit` и free `try_decommit` повторно получают page-size/poison state, после чего `decommit` делает это ещё раз. В steady state это несколько relaxed atomic loads на одну операцию. Цена мала рядом с syscall, но заметна для empty/invalid/skipped вызовов.

После решения M4 private helper может принимать уже провалидированный page size/range и выполнять единственный dispatch. Оптимизировать только вместе с упрощением контракта, не отдельным рискованным patch.

### P3 — известные cold-path кандидаты требуют измерения, не немедленной правки

- Linux huge exact-size miss затем пробует более крупный huge mapping перед ordinary fallback. Второй вызов не строго «гарантированно обречён» из-за concurrency/transient state, поэтому удалять его без измерения нельзя.
- Windows speculative large-page path способен заплатить за дополнительные `VirtualAlloc`/`VirtualFree` перед two-call fallback. Счётчики уже есть; нужен Windows-native профиль.
- Общий 64-bit Unix over-reserve — осознанный обмен одного syscall на большее VA; generic exact-size retry не следует возвращать без workload evidence.

## Coverage gap — real HugeTLB job пока проверяет dispatch, не постусловие памяти

`.github/workflows/ci.yml` теперь создаёт настоящий hugetlb pool, требует фактический `MAP_HUGETLB` grant и через `bench-internals` доказывает, что eligible decommit дошёл до принятого `madvise`. Это сильное улучшение.

Но оракул не читает содержимое после decommit/recommit и не измеряет RSS/pool reclaim; changelog это честно признаёт. До заявления о фактическом zero-fill/reclaim полезно добавить write → decommit → recommit/read-zero проверку на real HugeTLB job и, отдельно, наблюдение pool/RSS. Нужно различать «ядро приняло advice» и «физическая память уже возвращена».

## Положительные результаты проверки

- Размеры `size + align` проверяются на overflow и ограничиваются адресуемым диапазоном.
- Проверка range использует runtime page size и fail-closed poison state.
- Unix pointer arithmetic использует strict-provenance операции, а не восстановление provenance из integer.
- `errno`/`GetLastError` снимаются до cleanup-вызовов, которые могли бы затереть ошибку.
- Владение `Reservation` линейно: `Drop` единственный, `into_*` подавляет его; тип `Send`, но не `Sync`.
- Windows reserve/commit failure paths освобождают полученный reservation.
- Для созданных крейтом Linux huge mappings явно запрашивается `MAP_HUGE_2MB`, а decommit eligibility соответствует документированным ограничениям Linux 5.18+.
- Runtime path не имеет внешних зависимостей; feature surface и docs.rs metadata явно заданы.
- Статически просмотренная CI-конфигурация содержит default/all-features/mock/release, Miri, i686, rustdoc warnings, package dry-run, semver и real-hugetlb gates.

## Рекомендуемый порядок исправлений

1. Определить поддерживаемую модель adopted HugeTLB: запретить её либо расширить метаданные и release-контракт (M1).
2. Убрать зависимость обслуживания adopted mapping от feature создания; исправить совет про `decommit_lazy` (M2).
3. Разделить memory-safety и behavioral требования `from_raw_parts` (M3).
4. Зафиксировать честную семантику результата `try_decommit` до публикации API (M4).
5. Синхронизировать rustdoc/README/changelog, включая poison debug panic и современные HugeTLB правила (L1, документационные части M2/M4).
6. Разделить категории `VmemError` и добавить MSRV all-features gate (L2/L3).
7. После этого запустить полный release matrix и усилить real-HugeTLB postcondition oracle; данный аудит этого не делал.
8. Измерить P1 на real pool и P3 на целевых OS; P2 объединить с рефакторингом M4.

## Граница исследования

Исследование выполнено одним контекстом без под-агентов. Прочитаны код крейта, platform backends, public rustdoc, README/changelog, связанные тестовые оракулы, CI и открытые correctness/performance записи. Особое внимание уделено unsafe/FFI, RAII, arithmetic, page-size state, huge pages, feature combinations, error paths и публичным обещаниям.

Не выполнялись тесты, сборка, `cargo check`, Clippy, Miri, benchmark, sanitizers, packaging и публикация. Поэтому фактическое прохождение существующих gates, поведение на Windows/macOS/BSD/Android и производительность не подтверждались этим исследованием. Единственное изменение workspace, сделанное исследованием, — этот отчёт; ранее существовавший untracked `docs/checkpoints/2026-08-19-1520.md` не изменялся.
