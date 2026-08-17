# aligned-vmem — независимое read-only исследование перед релизом (R7)

Дата: 2026-08-16  
Ревизия: `2ad2607bd958dba75ac86239fc716e9b47b2fc23`  
Пакет: `aligned-vmem` `0.2.0`  

## Область и ограничения

Это новый статический проход после R6 по текущему `HEAD`: public API, lazy/huge
semantics, Windows/Unix/Miri backends, unsafe/FFI boundaries, feature/cfg
матрица, тестовые оракулы, CI и release-документация.

Режим был только read-only. В этом проходе не запускались тесты, сборка,
`cargo check`, `clippy`, `cargo doc`, publish/semver-проверки и бенчмарки.
Единственное изменение — этот отчёт. Независимое незакоммиченное изменение
`tests/remote_ring_shadow_head.rs` не относится к `aligned-vmem` и не учитывалось.

## Итог

Нового доказанного memory-safety/UB-дефекта в production unsafe/FFI-путях
статический проход не выявил. Но для публикации с заявленным `lazy-commit`
рекомендую условный **NO-GO до решения двух пунктов**:

1. Исправить rustdoc `Reservation`: сейчас он обещает zero-fill после любого
   Unix-decommit, что противоречит lazy, Darwin/BSD и huge-page поведению (R7-1).
2. До фиксации API явно принять или изменить state-blind контракт lazy handle:
   `Reservation` не сообщает, какая часть Windows-региона committed (R7-2).

Для обычного eager backend на Linux/Windows это не выглядит новым блокирующим
дефектом. Остальные пункты — низкоприоритетные оптимизации, диагностика,
release-gate parity и документационная гигиена.

## Что изменилось после R6

По сравнению с R6 исправлены и не переоткрываются:

- lazy Windows validity scope теперь описан в `as_ptr`/`Reservation`;
- lazy `size` и `initial_commit` проверяются относительно runtime `page_size()`;
- Android/Windows huge cfg и ошибочный huge-example исправлены;
- `ReservationFullParts` и `from_raw_parts` получили более точный контракт,
  включая Miri provenance;
- safe decommit для `is_huge()` теперь не делает заведомо бесполезный backend
  syscall;
- CI получил doc, package и semver gates.

Остались следующие наблюдения.

## Findings

### R7-1 — MEDIUM: validity docs обещают zero-fill шире фактического контракта

`crates/vmem/src/lib.rs:731` и `:819` говорят: после decommit на Unix ядро
вернёт свежие zero pages. Это неверно как общее утверждение:

- `decommit_lazy` на Linux прямо документирован как не гарантирующий zero-fill,
  пока ядро не reclaim-нул страницу (`lib.rs:2161`);
- eager `MADV_DONTNEED` на Darwin/BSD может оставить старые данные
  (`lib.rs:2115` и далее);
- для huge reservations safe methods теперь делают ранний no-op
  (`lib.rs:1195-1210`), поэтому старое содержимое остаётся.

Отдельные rustdoc `decommit`, `decommit_lazy` и capability-query описывают эти
исключения честнее, но пользователь, читающий `Reservation`/`as_ptr`, получает
противоречивое обещание. Это может привести к использованию старого значения
как будто оно было обнулено — особенно в allocator metadata.

Рекомендация: заменить общее “на Unix свежие zero pages” на матрицу
операция × платформа: zero-fill гарантирован только для обычного eager path
на Linux/Windows при успешной OS-операции; lazy не даёт zero-fill guarantee,
Darwin/BSD eager advisory-only, huge — no-op. Для остальных случаев говорить
“адресное пространство остаётся доступным/зарезервированным, содержимое не
гарантируется”.

### R7-2 — MEDIUM/LOW: lazy `Reservation` остаётся state-blind

`Reservation` хранит `base`, `len`, reservation metadata и `granted_huge`, но не
committed length/state (`lib.rs:750-798`). `Reservation::commit_range` лишь
проверяет границы и page multiples (`lib.rs:1319`), не обновляя и не раскрывая
состояние. На Windows чтение/запись в reserved-only tail до успешного commit
может закончиться access violation; текущая документация это уже предупреждает,
но API не может диагностировать ошибку.

Это остаток открытого item 66, а не новая divergence в docs. До публикации
нужно выбрать владельческое решение: отдельный `LazyReservation`, observable
`committed_len`/range state или явное принятие caller-tracked contract. Если
`lazy-commit` не входит в поддерживаемый release profile, риск можно понизить
до backlog, но это решение должно быть зафиксировано.

### R7-3 — LOW, performance: Windows huge fast path всё ещё может платить лишние FFI-вызовы

`win_reserve_commit` расширяет single-call attempt до
`GetLargePageMinimum()` (`lib.rs:2695-2747`). При отсутствии privilege или если
`size` не кратен minimum, сначала выполняется `VirtualAlloc(...MEM_LARGE_PAGES)`;
после отказа plain retry запускается безусловно (`lib.rs:2778-2793`). При
misaligned plain result добавляются `VirtualFree` и two-call fallback.

Наблюдаемость неполная: `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` считает только
успешное логическое завершение, `WINDOWS_LARGE_PAGE_RETRY_FAILURES` — только
случай, где отказались обе попытки, а alignment-counter — только successful
misaligned result (`lib.rs:318-331`, `:490-509`, `:2791`, `:2824`). Частый
сценарий “huge отказал, обычный retry сработал” оплачивает лишний syscall, но
не виден отдельным счётчиком.

Рекомендация: кэшировать `GetLargePageMinimum()`, заранее пропускать huge
attempt при `size % minimum != 0`, а plain retry выполнять только для ошибок,
которые он способен исправить (не для memory-pressure/OOM). Перед изменением
fast path нужен Windows-covered measurement. Текущий `docs/perf/OPEN_ITEMS.md`
item 47 устарел: он всё ещё утверждает, что `GetLargePageMinimum()` нигде не
вызывается (`:2145`).

### R7-4 — LOW, performance: Linux huge exact failure повторяется

Для `huge && align == 2 MiB` сначала выполняется exact
`libc_mmap(size, true)` (`lib.rs:3299-3328`). Если он вернул `NULL`, общий путь
ниже снова делает `libc_mmap(size + align, true)` и только затем ordinary
fallback (`lib.rs:3373-3378`). На типичной машине без hugetlb pool это две
неуспешные huge attempts на один logical reserve.

Без изменения fallback semantics можно различать причины: при `NULL` сразу
идти к ordinary fallback, а повторять huge только при редком успешном, но
неожиданно misaligned exact mapping. Это отдельный небольшой optimization pass,
который стоит подтвердить syscall-count/latency измерением.

### R7-5 — INFO/LOW, performance: 64-bit Unix удерживает `size + align` VA

Generic exact-size path на 64-bit намеренно выключен (`lib.rs:29`,
`unix_reserve` около `:3258`), поэтому обычная reservation держит весь
over-reserved mapping. Это syscall-optimal trade-off для типичного 64-bit
случая, но при больших `align` и большом числе живых reservations даёт
теоретическое давление на virtual address space. Существующий Linux huge
exact exception не закрывает generic case.

Не менять без workload evidence: exact probe на miss стоит `mmap + munmap +
over-reserve mmap`, а текущий путь — один `mmap`. Для решения нужен отдельный
64-bit reservation-heavy measurement; counters exact-path не измеряют generic
64-bit ordinary path.

### R7-6 — LOW, API/performance: uniform lazy validation имеет намеренную цену

`validate_initial_commit` всегда вызывает `page_size()` и требует runtime-page
multiple для `size` и `initial_commit` (`lib.rs:1851`, вызов `:2428`). На Unix и
Miri backend игнорирует `initial_commit` и eager-коммитит весь span, поэтому
проверка там не нужна для текущей реализации. На 16 KiB host она также
отвергает ранее допустимый `PAGE`-multiple, хотя Unix path мог бы его безопасно
принять.

Текущая fail-closed portability policy разумна и уже задокументирована. До
релиза нужно лишь не потерять решение: либо сохранить единый portable contract,
либо сделать проверку platform-specific и честно принять divergent API.

### R7-7 — LOW: `VmemError` неправильно описывает range errors

`VmemError::invalid_argument()` используется не только для `size/align`, но и
для misaligned/out-of-bounds commit/recommit ranges (`lib.rs:1329`, `:2342`).
При этом `Display` всегда печатает
`invalid argument (size/align contract violation)` (`crates/vmem/src/error.rs:43`,
`:126`). Пользователь получает неверное объяснение причины.

Минимальное исправление без расширения enum — “argument contract violation”.
Лучшее API-улучшение для будущей major-версии — отдельные invalid-range
variants/diagnostic payload, чтобы не терять имя нарушенного параметра.

### R7-8 — LOW: локальный release-gate всё ещё слабее CI

`.github/workflows/ci.yml:136-242` имеет aligned-vmem default/named/all-features
clippy, tests, Miri compile, cross-target checks, rustdoc warnings, publish
dry-run и semver check. `scripts/check-all.mjs:176-208` имеет пять clippy rows и
только `cargo test -p aligned-vmem --all-features`.

Следовательно, локальный `npm run check` не исполняет default-feature runtime
tests и не повторяет doc/semver gates. CI это компенсирует, поэтому это не
самостоятельный release blocker; но перед следующей волной стоит добавить
default test и doc/semver checks. `cargo publish --dry-run` разумно оставить
только CI из-за сети/crates.io.

### R7-9 — LOW: release-документация и индекс расходятся с кодом

Найдены конкретные stale-фрагменты:

- `crates/vmem/README.md:157-164` содержит лишнюю строку `item 6 for the
  incident record` внутри lazy bullet;
- `crates/vmem/src/lib.rs:29` говорит, что на 64-bit Unix exact path полностью
  выключен, не оговаривая Linux huge exact exception; аналогичные исторические
  формулировки есть в module/public docs;
- `lib.rs:300-306` описывает в two-call counter возможный “third best-effort
  retry”, но текущий two-call path не запрашивает `MEM_LARGE_PAGES` и такого
  retry не делает;
- `crates/vmem/Cargo.toml:116-128` всё ещё называет `huge_decommit_attempts`
  upper bound incompatibility rate и пишет, что feature “has no such feature of
  its own yet”, хотя текущий counter считает early exits и feature уже есть;
- `docs/CORRECTNESS_OPEN_ITEMS.md:2154` утверждает, что Windows reserve
  counters не имеют тестов, хотя проверки есть в
  `crates/vmem/tests/lazy_commit.rs` и `tests/huge_pages.rs`;
- item 58 (`:2227-2233`) говорит, что `try_reserve_aligned_exact` исключает
  Android, но текущий cfg — только 32-bit Unix и Android в него входит;
- item 68 (`:2353-2411`) рекомендует `decommit_reclaims_and_zero`, а его
  option (a) предлагает другое `decommit_reclaim_and_zero`. До публикации нужно
  выбрать одну точную spelling или закрыть item решением оставить asymmetry.

Это не runtime defect, но stale evidence повышает риск повторного исправления
уже закрытых проблем или неверного release sign-off.

### R7-10 — INFO: backend tuple и monolithic `lib.rs` остаются smell

`RawReservation` (`lib.rs:1871`) снижает риск transposition только на call site;
backend functions всё ещё возвращают unnamed tuples, что сам source comment
признаёт. Один `lib.rs` одновременно содержит public API, три backend-а,
mock/cfg-ветки и локальные FFI. Это не текущая ошибка, но усложняет аудит и
увеличивает вероятность cfg-specific regression.

Для будущей major-версии стоит разделить private `os_windows`/`os_unix`/`os_miri`
modules и возвращать named raw result из backend boundary. В 0.2.0 такой refactor
лучше не смешивать с release fix.

### R7-11 — INFO/LOW, defensive portability: успешный `mmap` по адресу zero

`libc_mmap` (`lib.rs:4041`) считает `NULL` failure вместе с обычным fallback,
проверяя только `MAP_FAILED`. На поддерживаемых конфигурациях `mmap(NULL, ...)`
обычно не выбирает address zero, но POSIX API не даёт crate-level гарантии
ненулевого результата. Если mapping по адресу zero когда-либо разрешён, он будет
ошибочно воспринят как отказ и не будет `munmap`-нут, то есть останется leak.

Защитный вариант: отдельно распознать `MAP_FAILED`, а успешный `NULL` перед
возвратом failure немедленно `munmap`-нуть. Это не подтверждено текущей CI
матрицей и не блокирует обычные Linux/Windows релизы.

## Unsafe/FFI red-tier pass

Повторно просмотрены:

- `unsafe impl Send for Reservation` (`lib.rs:1612`);
- `from_raw_parts`, `into_reservation`, `release`/`release_parts`;
- raw decommit/recommit/commit APIs и safe wrappers;
- Windows `VirtualAlloc`/`VirtualFree`/`GetSystemInfo`/`GetLargePageMinimum`;
- Unix `mmap`/`munmap`/`madvise`/`sysconf` и `OffT` cfg;
- Miri `std::alloc::alloc/dealloc` и provenance contract.

По чтению текущего кода не найдено нового доказанного transposed-pointer,
double-release, use-after-free или неверного manual-`Send` дефекта. Основные
остаточные риски — caller contracts unsafe API, silent OS release/decommit
failures, documented lazy state и непроверенные platform-specific branches;
они не превращены этим read-only проходом в утверждение, что runtime всё
протестировано.

## Покрытие и обязательные условия sign-off

До release sign-off следует отдельно убедиться, что CI прошёл:

- Windows ordinary/lazy/huge fallback и Linux ordinary/huge fallback;
- rustdoc `-D warnings`, package dry-run и semver check;
- i686 compile-only rows с принятым ограничением отсутствия 32-bit runtime;
- Miri compile-only limitation, BSD/Android/tvOS/watchOS spec-only limitation;
- отсутствие незакоммиченных изменений внутри `crates/vmem`.

Сам этот отчёт не заменяет runtime verification: по явному требованию задачи
тесты и другие проверки здесь не запускались.

