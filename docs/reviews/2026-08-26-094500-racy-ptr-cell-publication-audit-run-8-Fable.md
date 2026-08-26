# `racy-ptr-cell` — аудит готовности к публикации, прогон 8

- Время: 2026-08-26 09:45:00 +02:00
- Ревьюер: Fable (независимый цикл; продолжает сквозную нумерацию серии
  Sol-codex run 1–7, item 134)
- Ревизия: `8c13ae0525971fefc694b57142eb8139e2ce7b8c`
- Предыдущий отчёт: `docs/reviews/2026-08-26-082611-racy-ptr-cell-publication-audit-run-7-Sol-codex.md`
- Режим: **строго read-only статический аудит**. Никакие cargo-команды
  (test/check/clippy/bench/miri/loom/publish --dry-run) не запускались; никакие
  под-агенты не привлекались. Git использовался только read-only
  (`log`/`show`/`ls-files`). Все утверждения о зелёных прогонах тестов, miri,
  loom и измеренных числах — это записи авторов в commit messages и docs,
  рассмотренные как заявленная история, а не собственные проверки этого
  прогона.

## Вердикт

**GO.** Блокирующих находок (P0/P1/P2) нет. Реализация, unsafe-инвентарь,
атомарный протокол, layout-контракт, публикационные метаданные, тесты и CI
согласованы; крейт можно публиковать на crates.io в текущем виде.

Найдены четыре новые P3-находки (три — документационный дрейф/CI-полнота, одна —
API-полнота) и четыре nano-замечания (P4). Ни одна не затрагивает корректность,
soundness или честность публичных обещаний; все дешёвые. Рекомендуется закрыть
F1–F4 до публикации (или F4 — первым же патч-релизом: добавление `impl Debug`
semver-минорно), но публикацию они не блокируют.

## Что исследовано (полный свежий проход, не только дифф)

- `Cargo.toml` (метаданные, keywords/categories, `[lints] workspace`,
  `cfg(loom)`-зависимость, `[[bench]]`-ключи), `README.md`, `CHANGELOG.md`, обе
  лицензии, полный `git ls-files` крейта (состав будущего tarball).
- `src/lib.rs` (facade: crate-doc, `compile_error!`-гейт, cfg-split) и
  `src/imp.rs` (вся реализация: state machine, все atomic-ordering'и,
  `RollbackGuard`, `RollbackProbe`, оба `dbg_*`-probe, все unsafe-сайты).
- `tests/cell_unit.rs` и `tests/loom_racy_ptr_cell.rs` — построчно, с проверкой,
  что каждый тест доказывает заявленное в имени/doc (включая контрфактуалы).
- `benches/racy_ptr_cell_bench.rs` + корневой `bench-iters.txt` (все 7 строк
  манифеста сверены с текущими ID строк бенчмарка — устаревших имён нет).
- `.github/workflows/ci.yml`: `racy-ptr-cell-gates`, `racy-ptr-cell-miri`,
  строки в workspace-test job (native debug/release, thumbv7em build, clippy
  `--all-targets`, rustdoc `-D warnings`), `loom-alloc-global` (tee+grep
  sentinel), `msrv` job.
- Корневые `Cargo.toml` (path-dep, `[workspace.lints]`, loom workspace-dep) и
  `README.md` (инвентарь seam'ов, строка про `racy-ptr-cell` — присутствует).
- История: все коммиты крейта после прогона 7 (`82ea415`) и сам HEAD
  (`8c13ae0` — только файлинг item 134, крейт не менялся).

## Изменения после прогона 7

Один содержательный коммит — `82ea415` (task #1407, закрывает run-7 F1):
doc-only правка бенчмарка. Верхняя фраза «Every row measures the CELL PROTOCOL —
and nothing else» удалена; разность `contention/one_cell −
baseline/scaffolding_only` теперь последовательно названа «DIFFERENTIAL ESTIMATE
under this exact harness», с явным объяснением, что timed-величина — это round
makespan перекрывающихся потоков (включая benchmark-авторский `init_body` с 64
`spin_loop`), а не алгебраическая изоляция intrinsic-стоимости протокола; для
regression-сигнала рекомендовано сравнивать `contention/one_cell` саму с собой
при фиксированных параметрах. Проверено против текущего текста файла (module
doc + комментарии у трёх contention-строк): формулировки соответствуют
методу измерения, overclaim'а не осталось. **Run-7 F1 закрыта корректно.**
Control flow бенчмарка не менялся — контроль-строки по-прежнему совпадают с
рабочей по source shape (worker'ы идут через общий `take_round_cell`,
bench-поток в обеих строках держит `cell` из untimed setup).

## Находки

### F1 — P3: список «Real-type properties proved» в loom-suite не содержит property 7, которую объявляет её собственный баннер

**Где:** `crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:15-33` (module doc,
нумерованный список свойств 1–6) против `:392` (баннер «Real-type property 7:
`get()` never reports a cell held at INITIALIZING») и `:495` (баннер
«Real-type property 6»).

Module doc перечисляет шесть доказанных свойств (1–5 + 6 про probe-clobber).
Тест `real_get_returns_none_while_a_winner_holds_the_sentinel` (добавлен task
#1399) помечен в теле файла баннером «property 7», но в module-doc-списке
свойства 7 нет вовсе; вдобавок секции идут в порядке 1-4, 5, **7, 6**. Дрейф
безопасного направления (недосказанность, не overclaim), но это ровно тот класс
рассинхронизированных числительных, с которым боролись review-8/9 (задачи
#1382/#1383): аудитор, читающий «что доказано» по module doc, недосчитается
одного реально существующего доказательства.

**Фикс:** добавить пункт 7 в module-doc-список (формулировка уже есть в баннере
и doc самого теста) и либо переставить секции в порядок 6→7, либо снять
зависимость от порядковых номеров в баннерах.

### F2 — P3: loom-suite цитирует несуществующую секцию README «Running the loom suite»

**Где:** `crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:71-72`: «The README
says the same thing under "Running the loom suite"; this command must not drift
from it».

В `README.md` секции с таким названием нет — команда запуска loom-suite живёт
под заголовком «The two rules people get wrong» (`README.md:213-215`). Сами
команды при этом совпадают дословно (сверено) — дрейфанул только заголовок,
причём в комментарии, чья единственная работа — предотвращать дрейф. В
опубликованном tarball README и тест уезжают вместе, так что несуществующая
ссылка станет видна любому потребителю, читающему loom-suite.

**Фикс (одно из двух):** поправить имя секции в комментарии теста, либо (лучше)
завести в README настоящий заголовок «Running the loom suite» — команда и
предупреждение про глобальность `--cfg loom` уже фактически образуют такую
подсекцию.

### F3 — P3: msrv-job не имеет ни одной строки для racy-ptr-cell — `rust-version = "1.88"` проверяется только транзитивно и только для библиотеки

**Где:** `.github/workflows/ci.yml:1931-2001` (job `msrv`) против
`crates/racy-ptr-cell/Cargo.toml:5`.

Библиотека крейта компилируется на pinned 1.88 лишь косвенно: корневой шаг
`cargo check --all-features` включает `alloc-core` → path-dep `racy-ptr-cell`.
Но собственный тестовый/дев-граф крейта (`tests/cell_unit.rs`,
`tests/loom_racy_ptr_cell.rs` в native-выключенном виде, dev-dependency
`bench-scale-tool = "0.1"`) на 1.88 не компилируется нигде — ровно тот gap,
который этот же job уже явно закрыл персональными строками для трёх других
публикуемых крейтов (`sefer-region`, `aligned-vmem`, `numa-shim`, см.
комментарии task #823/#1173/#1282 в самом job). Для крейта, публикуемого с
заявленным MSRV, паритет напрашивается; сегодняшнее покрытие не отловит ни
пост-1.88 конструкцию в тестах, ни уползание MSRV у `bench-scale-tool`.

**Фикс:** добавить в `msrv` job две строки —
`cargo check -p racy-ptr-cell` и `cargo test -p racy-ptr-cell --no-run`
(вторая компилирует тесты + дев-граф; bench при `test = false` ею не
собирается — если хочется покрыть и его, третья строка
`cargo check -p racy-ptr-cell --all-targets`). Стоимость — секунды: дев-граф
крейта состоит из одного маленького `bench-scale-tool`.

### F4 — P3: у `RacyPtrCell<T>` нет `impl Debug` — единственный публичный тип серии без него

**Где:** `crates/racy-ptr-cell/src/imp.rs:94` (объявление типа; derive/impl
`Debug` отсутствует — проверено по всему файлу).

`RollbackProbe` — `derive(Debug, ...)`; `std::sync::OnceLock` и оборачиваемый
`AtomicPtr<T>` оба реализуют `Debug` (API guidelines C-DEBUG). Сейчас
downstream-структура, содержащая `RacyPtrCell`, не может `#[derive(Debug)]` —
ровно тот allocator-metadata-случай («meant to sit in allocator metadata»,
собственная формулировка layout-секции), где cell оказывается полем большой
структуры. Ручной impl тривиален, не требует `T: Debug` (классифицировать
Relaxed/Acquire-load в `Uninit`/`Initializing`/`Ready(addr)` без dereference) и
ничего не ослабляет. За 15 прогонов вопрос ни разу не поднимался (проверено
grep'ом по всем отчётам серии — «Debug» упоминался только в смысле
`dbg_*`-probe). Не блокер: добавление trait-impl задним числом semver-минорно;
но дешевле всего сделать это сейчас, до заморозки 0.1.0, тем же движением,
каким #1389/#1390 полировали API «до заморозки».

**Фикс:** ручной `impl<T> core::fmt::Debug for RacyPtrCell<T>` (без bound на
`T`), печатающий трёхсостоянийную классификацию; одно-строчный тест в
`cell_unit.rs`. Если отказ от `Debug` намеренный (не печатать адреса?) —
зафиксировать отказ явно, как это уже сделано для `#[doc(hidden)]`-позиции.

## Nano-замечания (P4, не требуют действий до публикации)

- **N1.** `Cargo.toml:7` (description): «The cell's own operations use no std
  sync and no heap» — без квалификатора «non-panicking», который lib.rs/README
  везде тщательно несут (panic-путь release-active `assert!` в std-окружении
  МОЖЕТ аллоцировать, о чём подробно говорит собственная panic-таблица крейта).
  Description и так отсылает «(see docs)», а до «operations» стоит
  «non-panicking» в обоих полных документах, так что это сжатие, не ложь —
  но одно слово (`non-panicking operations`) сняло бы и этот зазор.
- **N2.** `Cargo.toml:16-17`: комментарий у `[lints] workspace = true` называет
  «harmlessly unused here» только `kani`, тогда как разделяемая check-cfg
  таблица (`Cargo.toml` корня, строка 108) объявляет ещё
  `aligned_vmem_page_size_override` и `numa_shim_mock` — равно неиспользуемые
  здесь (и они уедут в опубликованный, развёрнутый `[lints]` манифеста).
- **N3.** `CHANGELOG.md:7`: «0.1.0 - Unreleased» — при фактической публикации
  заменить на дату релиза (механика релиза, не дефект).
- **N4.** `benches/racy_ptr_cell_bench.rs:161`: в `Contention::round` режим
  `MODE_CONTEND` матчится wildcard-веткой `_ =>` — будущий новый режим,
  добавленный без правки match, молча станет contention. Явная ветка
  `MODE_CONTEND =>` + `_ => unreachable!()` в бенч-коде дешевле будущей
  отладки.

## Полный аудит production-кода (свежее чтение)

### State machine, liveness, повторно верифицированные инварианты

Три состояния однозначно разделены адресом одного `AtomicPtr<T>`: `null` /
адрес `1` / любой другой non-null. `new` (обе cfg-версии) release-активно
требует `align_of::<T>() >= 2`, так что выровненный указатель на `T` не может
совпасть с sentinel; синтезированный safe-кодом `NonNull` с адресом 1
отклоняется release-active `assert!` перед публикацией, и unwind этого
`assert!` корректно проходит через уже-взведённый `RollbackGuard` (guard
создаётся до `init()`, defuse — только после публикации/явного rollback).
Loser-спин — строго `while == INITIALIZING`; `null` выводит на re-race,
реальный указатель возвращается. Anti-livelock-правило соблюдено во всех трёх
местах чтения (fast path, re-checked top-of-loop, спин). `FnOnce` соответствует
фактическому at-most-once на вызов. Re-entrancy/transitive-deadlock/`init`
-обязательства задокументированы и на уровне метода, и на уровне крейта.

### Atomic ordering

Load-bearing пара — winner `store(Release)` ↔ все читатели `load(Acquire)` —
присутствует во всех путях (включая `get` и оба `dbg_*`). Rollback (оба:
явный OOM и guard's Drop) — `store(null, Release)`; claim-CAS — success
`Acquire` / failure `Relaxed` с перечитыванием. Возможная избыточность
(Acquire на claim-success и на каждой итерации спина) честно помечена в коде
как открытый вопрос с правильной дисциплиной решения (loom-контрфактуал +
измерение на weakly-ordered target до любого ослабления). Согласен с этой
позицией; см. «Возможности ускорения» ниже.

### Unsafe и provenance

Полный unsafe-инвентарь: `unsafe impl Send`/`Sync` (обоснование —
дословно модель `AtomicPtr<T>`, корректно: крейт никогда не dereference'ит `T`
и не выдаёт `&T`, только raw capability) плюс четыре
`NonNull::new_unchecked`, каждый строго доминируем проверкой
non-null/non-sentinel в той же функции. Sentinel — `without_provenance_mut`,
только сравнение адресов, никогда не dereference. `#![allow(unsafe_code)]`
на facade с одним задокументированным основанием; ни одного unsafe в lib.rs.
Тесты избегают int→ptr round-trip'ов (провенанс-чистые схемы возврата через
`cell.get()` / канал-сигнал — сверено в обоих файлах).

### Layout, portability, зависимости

`#[repr(transparent)]` + ZST `PhantomData<*mut T>` — «one word» является
контрактом; закреплено тестом `layout_matches_a_single_atomic_ptr` (который
честно оговаривает, что именно он ловит). `compile_error!` на
`not(target_has_atomic = "ptr")` + позитивный cfg на `mod imp` дают ровно один
диагноз на неподдерживаемых таргетах. Обычная сборка — ноль non-std
зависимостей; `loom` — только под `cfg(loom)` (паттерн, официально
рекомендованный loom); `bench-scale-tool` — dev-only, реальный crates.io-крейт
(сверено по `Cargo.lock`: registry + checksum).

### Публикационные метаданные

`license = "MIT OR Apache-2.0"` + оба текста в tarball; keywords — ровно 5
валидных; categories — 3 валидных slug'а (включая `no-std::no-alloc`);
`repository`/`homepage`/`documentation` согласованы. Состав tarball
(`git ls-files`) — 10 файлов, ничего лишнего и ничего недостающего;
`[[bench]] harness = false, test = false` — явно, с верным комментарием.
CHANGELOG сверен с фактическим API построчно: FnOnce/`#[must_use]`,
`repr(transparent)`, двухвариантный `RollbackProbe` с честным
NotApplicable-контрактом, loom-посылки — всё соответствует коду после правок
#1401/#1405.

### Тесты

`cell_unit.rs`: 8 тестов; оракулы не вакуумные (два `#[should_panic]` guard'ят
release-active-проверки, чьи counterfactual-верификации описаны в комментариях;
timeout-схема на панике-rollback переводит потенциальный livelock в
проваленный, а не повисший прогон; unwind-тест после #1404 заявляет ровно ту
(более слабую, но реальную) дизъюнкцию интерливингов, которую доказывает).
`loom_racy_ptr_cell.rs`: свойства реального типа (exactly-once ×2 threads и
×3, fast-path re-entry, OOM-rollback-liveness, get()-во-время-sentinel с
двусторонним handshake, probe-vs-real-winner clobber-тест с построчно
задокументированным плохим интерливингом) + два `#[should_panic]`
контрфактуала анти-вакуумности, у каждого честно указано, чем он отличается от
реального типа и почему детектор — causality checker loom'а, а не value-assert.
Единственный дефект — нумерационный дрейф module doc (F1).

### Бенчмарк

После #1391→#1398→#1403→#1407 методология в порядке: ни одна timed-область не
аллоцирует; cold-строка меряет полный протокол на свежей cell из untimed
setup; contention-строка и два её baseline'а согласованы по source shape;
формулировки про makespan/differential estimate теперь соответствуют тому, что
метод действительно даёт. `bench-iters.txt` (корень) содержит все 7 актуальных
ID и ни одного устаревшего. Остаточное замечание — только N4.

### CI

Покрытие крейта: native debug + release (`--no-fail-fast`), bare-metal
`thumbv7em-none-eabi` build, clippy `--all-targets -D warnings` (покрывает
bench и тесты), rustdoc `-D warnings --no-deps`, miri обычный +
`-Zmiri-strict-provenance`, real-type loom с tee+grep-sentinel на именованный
probe-тест, `cargo publish --dry-run` package-gate с верным обоснованием
отсутствия semver-checks (нет baseline до первой публикации). Названия тестов,
цитируемые комментариями ci.yml, сверены с файлами — все существуют; счётных
числительных не осталось (#1406). Единственный gap — msrv (F3).

## Возможности ускорения

Новых недоказанных возможностей не вижу; подтверждаю существующую позицию кода:

1. **Fast path** (`get`/`get_or_try_init` по READY): один `Acquire`-load +
   две integer-сравнения + branch; `#[cold] #[inline(never)]` slow-path
   вынесен. На x86-64 это уже минимум; ничего не добавить.
2. **Claim-CAS success `Acquire` → `Relaxed`** и **спин-итерации `Acquire` →
   `Relaxed`-спин с `Acquire`-перечтением на выходе** — оба кандидата уже
   названы в самом коде (комментарий у CAS в `init_slow`) с верной дисциплиной:
   без loom-контрфактуала слабой формы и измерения на AArch64 не трогать; на
   x86-64 выигрыш недоказуем в принципе (Acquire-load — обычный load).
   Поддерживаю: over-strong, never under-strong.
3. Двойной `Acquire`-load при первом UNINIT-заходе (fast path + top-of-loop в
   `init_slow`) — once-per-cell cold-path, устранение усложнило бы re-race
   control flow ради нуля наблюдаемого выигрыша. Не делать.
4. Аллокаций, лишних атомиков, `std`-примитивов в production-коде нет.

## Границы уверенности

- Аудит статический, read-only: код не компилировался и не исполнялся;
  зелёность CI, результаты loom/miri и все измеренные числа (panic-allocation
  counts, bench-калибровки) приняты как заявленные артефакты репозитория.
- Аргументы про atomic-ordering и loom-свойства проверены рассуждением по
  исходнику, не повторным model-checking'ом.
- Утверждение «`OnceLock::get_or_try_init` is still unstable
  (`once_cell_try`)» соответствует моим сведениям на дату аудита, но статус
  std-feature стоит механически перепроверить на актуальном toolchain
  непосредственно перед публикацией (однострочный `cargo doc`-взгляд) — если
  фича стабилизировалась, три места (Cargo.toml description косвенно, lib.rs,
  README) потребуют смягчения формулировки.

## Сводка

| # | Severity | Находка | Блокер? |
|---|----------|---------|---------|
| F1 | P3 | loom module doc: списка свойств 1–6, баннеры 1-4/5/**7**/6 — property 7 не в списке | нет |
| F2 | P3 | loom-suite цитирует несуществующую README-секцию «Running the loom suite» | нет |
| F3 | P3 | msrv-job: ни одной строки `-p racy-ptr-cell`; MSRV 1.88 проверяется лишь транзитивно, только lib | нет |
| F4 | P3 | нет `impl Debug` для `RacyPtrCell<T>` (C-DEBUG; `RollbackProbe`/`OnceLock`/`AtomicPtr` его имеют) | нет |
| N1–N4 | P4 | description без «non-panicking»; `[lints]`-комментарий про один cfg из трёх; дата в CHANGELOG при релизе; wildcard-ветка MODE_CONTEND | нет |

**Итог: GO.** P0/P1/P2 — нет. Крейт готов к `cargo publish`; F1–F4 —
рекомендованная полировка до (или сразу после) первого релиза, F4 удобнее
всего закрыть до заморозки API.
