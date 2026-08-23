# Новое расследование `aligned-vmem` — готовность к публикации (после волны изменений task #1219–#1249)

- Автор: **GLM**
- Время: 2026-08-23 13:36 (Europe/Berlin)
- Ревизия: `54f69e9` (HEAD; содержимое крейта не менялось с `0e65bfe`/task #1249 — HEAD отличается только docs-коммитом; рабочий дерево по `crates/aligned-vmem` чистое, проверено `git status --porcelain`)
- Режим: **только чтение, один агент, без под-агентов**. Не запускались: тесты, сборка, `cargo check`, Clippy, Miri, бенчмарки, docs, `cargo publish`. Все выводы ниже — чтение исходников; исполненное покрытие см. в разделе «CI».

## Граница исследования

Полностью прочитаны: все ~40 файлов `crates/aligned-vmem/src/` (включая `api/`, `os/`, `bench_internals/`, тестовые seam-модули `page_size_override*`), `Cargo.toml`, `README.md`, `CHANGELOG.md` (814 строк), CI-покрытие крейта в `.github/workflows/ci.yml` (джобы `aligned-vmem-gates` и `aligned-vmem-hugetlb-real`), карточки items 90/91/93/94/97 в `docs/correctness-open-items/TRACKED_publish_readiness.md`, отчёт шестого аудита (`docs/reviews/2026-08-23-131201-…-Сол-кодекс.md`), соответствующие пункты `docs/perf/OPEN_ITEMS.md` (48/50/52/53/55). Не читались построчно: `tests/` (только структура и подсчёт `#[test]` — 23 файла, ~175 тестов), `benches/`, `examples/`.

## Итоговый вердикт

**Технически — GO.** Ни одного нового дефекта кода, UB, утечки или семантического расхождения в прочитанных путях я не нашёл. Все технические блокеры шести независимых аудитов (2026-08-16 … 2026-08-23) закрыты; я выборочно перепроверил ключевые из них по исходникам, а не по чужим отчётам (список ниже).

Ественный оставшийся шаг — **не кодовый**: сам акт релиза. Формальный NO-GO шестого аудита держится ровно на F1 — `## 0.2.0 - Unreleased` в `crates/aligned-vmem/CHANGELOG.md:7`, — который владелец сознательно отложил («P0 пока не обращаем внимания», item 97). Это не дефект: крейт корректно не считает себя выпущенным, и релизный workflow (`release.yml:301-309`) намеренно не пропустит публикацию мимо этой строки. Публикация = проставить дату + запустить workflow (перед этим, по правилам репо, — свежий `npm run check` и подтверждение зелёного CI на landing SHA).

## Что я перепроверил заново сам (OBSERVED, чтением)

1. **Архитектура/конвенции.** One-file-one-export соблюдён во всём `src/`; `mod.rs` — только реэкспорты; `#![allow(unsafe_code)]` + `#![deny(missing_docs)]`; у каждого FFI-вызова — `// SAFETY:`; дисциплина немедленного захвата `errno`/`GetLastError` (task #713) проведена во всех точках отказа (`libc_mmap`, `libc_madvise`, `winapi_virtual_decommit`, commit-failure в `win_reserve_commit`); strict provenance (`.addr()`/`.with_addr()`, без `as usize` round-trip) в путях reserve/release Unix и Windows.
2. **Арифметика и границы.** `checked_add` на `size + align` + отвержение `> isize::MAX` (`api/internal.rs::validate_size_align`); `align_up_addr` без переполнений с `fit`-перепроверкой; `debug_assert!` на инвертированный диапазон в обоих `decommit_pages_impl` и в `recommit_pages_impl`; «ядовитое» значение page-size (`usize::MAX`) fail-closed по построению — `is_multiple_of(usize::MAX)` истинно только для 0/`usize::MAX`, любой обычный непустой диапазон отвергается даже без явной проверки яда.
3. **Windows-бэкенд.** Локальная `SystemInfo` соответствует реальной `SYSTEM_INFO` (union-голова корректно расплющена, выравнивание/размер совпадают); fast-path `VirtualAlloc(MEM_RESERVE|MEM_COMMIT)` защищён БЕЗУСЛОВНОЙ пост-проверкой выравнивания (не `debug_assert`) и на retry-ветке тоже; `MEM_RELEASE` c `len=0` релизит весь регион независимо от `reservation_len` (under-report задокументирован); таксономия счётчиков больших страниц (retry-failures / alignment-failures / plain-fallback-successes) соответствует трём разным веткам кода; вырожденный случай `GetLargePageMinimum()==0` корректно уводит в two-call path.
4. **Unix-бэкенд.** Пер-ОС таблица `_SC_PAGESIZE` (Darwin 29, BSD 47/28, bionic 39, glibc/musl 30) с честной историей двух ошибок; `OffT` dual-width (32-bit glibc/bionic `i32`, всё остальное `i64`); `compile_error!` на неизвестный Unix и на MIPS; huge-контракт: 2-MiB-кратность `size`/`align` ДО попытки, exact-size fast-path только при `align == 2 MiB` с runtime-проверкой гарантии ядра, over-reserve хранится целиком (один `munmap` на базу); двойная doomed-попытка `MAP_HUGETLB` при промахе exact-пути — задокументирована и четыре раза разобрана (perf item 52, вердикт NULL).
5. **Модель владения.** `Reservation: Send + !Sync` с обоснованием; `Drop` — ровно один release; `into_parts`/`into_reservation_parts`/`into_full_parts` — `mem::forget(self)` + `#[must_use]`-сообщения об утечке на типах-токенах; `from_raw_parts` — полный набор проверяемых инвариантов (ненулевые, порядок `base >= reservation`, выравнивание, покрытие `reservation_len >= len + offset`, валидный `Layout`), плюс 2-MiB-assert на Linux/Android при `granted_huge=true` (пять имён, четыре независимых проверки — так и задокументировано), плюс feature-gate «`granted_huge=true` требует `huge-pages`».
6. **Исполнимость/деградация.** `page_size()` инфоллибл, деградированное состояние (не наблюдалось ни на одной поддерживаемой платформе) poison'ится на процесс: decommit → no-op, recommit/commit_range → false, `try_*` → no-code ошибка, `try_page_size` → `Err`; `LazyReservation`-watermark монотонен, `ensure_committed` идемпотентен и не двигает watermark при ошибке; `decommit_reclaims_and_zeroes()`/`can_decommit_reclaim_and_zero()`/`lazy_commit_is_honored()` — консистентная семья capability-запросов.
7. **Cargo/метаданные.** Нулевые зависимости (dev-dep `bench-scale-tool` не влияет на потребителей; `cargo publish --dry-run` — CI-ряд); двойная лицензия с файлами; `package.metadata.docs.rs` закреплён точным списком (mock не может быть включён `all-features`, `bench-internals` исключён — осознанно); `unexpected_cfgs` закрыт декларациями; примеры с `required-features`.
8. **CI (чужие прогонки, на которые ссылаюсь).** `aligned-vmem-gates`: clippy `-D warnings` в трёх наборах фич + `--all-features`, тесты `--all-features`, mock-cfg debug И release, release real-path, три самопроверяющихся seam-ряда (`page_size_override`, `page_size_query_failure`, `decommit_poison_no_panic` — каждый с grep-постусловием, чтобы потерянный cfg краснел), miri+mock check, кросс-чеки i686 glibc/musl, `cargo doc -D warnings` в ДВУХ наборах (`--all-features` И точный docs.rs-набор), `publish --dry-run`, `semver-checks`. `aligned-vmem-hugetlb-real`: реальный `nr_hugepages=64` пул + три оракула (path-activation на grant, kernel-accept на `madvise`=0, zero-fill readback). Последний полный локальный прогон зафиксирован в теле `0e65bfe` (60/60 под полным набором фич).

## Findings

### G1 — P0 (действие, не дефект): сам акт релиза
`CHANGELOG.md:7` `## 0.2.0 - Unreleased`. Совпадает с F1 шестого аудита; отложено владельцем. Закрывается проставлением даты и прогоном релизного workflow; gate `release.yml` удерживает это решение намертво, датировать заранее было бы фальсификацией даты релиза (уже один раз удаляли — task #1099/I2).

### G2 — P2 (bookkeeping): карточки аудитов 90/91/93/94 удовлетворили собственные условия закрытия, но их Status-строки это не отражают
Все четыре карточки лежат в `docs/correctness-open-items/TRACKED_publish_readiness.md` со Status: OPEN, при этом (всё OBSERVED, прочитано напрямую):

- **item 90**: Status говорит «This card closes when #1190 is answered» и «What remains is the single OPEN QUESTION» — но ответ (NO, 2026-08-20) вписан в САМУ карточку appended-блоком и внесён в `src/reservation.rs` + `CHANGELOG.md` коммитом `8972810`.
- **item 91**: «Closes with #1190» — условие выполнено.
- **item 93**: триггер «#1190 answered and #1216's audit run» — оба произошли (#1216 = пятый аудит, его F1–F8 закрыты, см. item 94), Status всё ещё «Still open: #1190 and #1216».
- **item 94**: триггер «#1190's answer landing in `src/reservation.rs`, item 90's card, `CHANGELOG.md`» — выполнен тем же `8972810` (+ ответ внутри карточки item 90).

По собственному правилу репо (R34-24: closed item не должен выглядеть активным; перенос в RESOLVED тем же коммитом) это структурный дефект текущего состояния индекса. Ничего не ломает, но следующий раунд/аудит прочтёт эти карточки как живые блокеры — ровно тот класс «true-then/false-later»-устаревания, ради которого правило писано (сам item 90 цитирует прецедент). Калибровка: держать карточки открытыми до формального снятия NO-GO — защитимая позиция владельца, но тогда Status-строки должны говорить «ждём решения о снятии NO-GO/релизе», а не «ждём #1190», который уже отвечен. Минимальное действие: один docs-коммит, обновляющий четыре Status-строки (и, по правилу, переносящий карточки в RESOLVED с однострочным указателем).

### G3 — P3 (docs): два устаревших указателя `src/lib.rs` в `src/mock.rs`
- `src/mock.rs:31` — «see the `unsafe impl Send for Reservation` and its SAFETY comment in `src/lib.rs`»: impl живёт в `src/reservation.rs:1572` с момента разбора монолита (task #1055/R7-10).
- `src/mock.rs:69` — «see those functions' implementations in `src/lib.rs`»: реализации теперь в `src/api/recommit.rs` и `src/api/commit_range.rs`.

Модуль компилируется только под `--cfg aligned_vmem_mock`, потребительского эффекта ноль; но это тот самый doc-drift-класс, который в остальном крейте ловит `vmem-doc-drift-guard.mjs`. Попутно замечу (не отдельный finding): секция «Recording contract edge cases» в том же модуле перечисляет правило «пустой диапазон не пишется в лог» только для `recommit`/`commit_range`, хотя свободные `decommit`/`decommit_lazy` ведут себя так же (early-return до `mock::record`).

### G4 — INFO (perf): все возможности ускорения уже в индексах, ни одна не доказана измерением, ни одна не блокирует публикацию
1. **64-bit Unix over-reserve** `size + align` VA (perf items 48/53) — осознанный размен (1 syscall, soundness one-munmap-дизайн task #842); для huge-grant слак `align` байт тарифицируется страницами bounded-пула (2× для `size==align==4 MiB`). Хост с пулом в CI теперь есть, но workload-харнеса с циклом резерваций и чтением счётчиков/пула по-прежнему нет — это единственное, что отделяет «задокументировано» от «измерено».
2. **Двойной doomed `MAP_HUGETLB`** при промахе exact-пути (perf item 52) — отвергнут четыре раза (R6-8/R7-4/F9/четвёртый аудит) как NULL: контрпример «разные размеры могут пройти/упасть по-разному» не опровергнут; нужно измерение syscall-cost на пуле/без пула.
3. **Windows speculative large-page window** (perf items 55/57): на непривилегированном хосте в диапазоне `64 KiB < align <= GetLargePageMinimum()` до 2 лишних `VirtualAlloc` + 1 `VirtualFree` до two-call path. Счётчики таксономии уже есть; направление из шестого аудита (preflight/кэширование SeLockMemoryPrivilege-способности) разумно, но только после профилирования — как сам отчёт и говорит.
4. **`fault-injection` в production-графе**: при включённой feature каждый реальный commit платит атомарный RMW + неконфликтный мьютекс (task #1021/R4-8). Feature выключена по умолчанию и компилируется из прод-пути полностью — стоимость нулевая, пока её никто не включил. Включать в production не нужно; менять модель — semver-решение на потом.

Ни один пункт не является доказанным ускорением; все требуют измерений на целевых ОС. Согласен с шестым аудитом: не блокеры.

### G5 — INFO: честно задокументированные остаточные ограничения (не дефекты; перечислены для полноты ответа «что улучшить»)
- **Darwin/BSD eager decommit advisory-only** (correctness item 48): без фикса (нужен re-`mmap(MAP_FIXED)`, отдельный раунд); `decommit_reclaims_and_zeroes()` честно возвращает `false`.
- **Реальный отказ ОС → `DecommitOutcome::Refused`** покрыт только симуляцией fault-injection (шестой аудит F5); остаток зафиксирован в трёх местах (item 92, док `Refused`, док `dispatch_try_decommit`).
- **Физический возврат huge-страниц пулу** не гейтится (`HugePages_Free` — только наблюдение; обоснованно: kernel-global счётчик).
- **Reasoned-from-spec цели**: BSD×4, Android, tvOS/watchOS, AArch64 Linux — константы из заголовков ОС, без эмпирической проверки на железе.
- **`try_decommit_lazy` не существует** — задокументировано как возможная аддитивная опция будущего.

## Ответы на вопросы задания

- **Готов ли к публикации?** Да — техническая готовность достигнута; осталась релизная механика (G1), удерживаемая решением владельца, плюс желательно прибраться в индексах (G2) и два док-нита (G3).
- **Есть ли что исправить?** В коде — нет: новых дефектов не найдено, все известные закрыты. Исправить книгу учёта (G2) и указатели в mock-доках (G3).
- **Улучшить?** G2/G3; API перед первым релизом менять не нужно (все one-way-door решения — docs.rs-набор, имя, лицензия, семвер-контракты, `alloc-lazy-commit`-алиас — уже приняты и зафиксированы).
- **Ускорить?** Доказанных ускорений нет; кандидаты G4.1–G4.4 требуют измерений (для G4.1/G4.2 — в первую очередь построить reservation-heavy харнес на существующем hugetlb-раннере, что уже записано как next-trigger item 48/57).

## Что я НЕ делаю в этом отчёте

Не выдаю чтение за исполнение: все пункты раздела «Перепроверено» — статические наблюдения. Исполненное покрытие крейта обеспечивают CI-ряды, перечисленные в п.8 (последний зафиксированный полный прогон — в теле коммита `0e65bfe`), и перед реальным `cargo publish` их нужно увидеть зелёными на landing SHA.
