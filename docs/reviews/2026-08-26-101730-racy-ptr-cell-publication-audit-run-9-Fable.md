# `racy-ptr-cell` — аудит готовности к публикации, прогон 9

- Время: 2026-08-26 10:17:30 +02:00
- Ревьюер: Fable (третий Fable-прогон в серии; продолжает сквозную нумерацию
  Sol-codex run 1–7 + Fable run 8, items 130/133/134/135)
- Ревизия: `5c41f26b8244a343304207fc4ab2e2e546580b99`
- Предыдущий отчёт: `docs/reviews/2026-08-26-094500-racy-ptr-cell-publication-audit-run-8-Fable.md`
  (мой же прогон 8, вердикт GO, четыре P3 — все с тех пор закрыты коммитами
  `6785e82`, `70f0294`, `a8bf944`)
- Режим: **строго read-only статический аудит**. Никакие cargo-команды
  (test/check/clippy/bench/miri/loom/publish --dry-run) не запускались; никакие
  под-агенты не привлекались. Git использовался только read-only
  (`log`/`diff`/`show`/`ls-files`). Все утверждения о зелёных локальных
  прогонах и измеренных числах — записи авторов в commit messages и docs,
  рассмотренные как заявленная история, а не собственные проверки этого
  прогона.

## Вердикт

**GO.** Блокирующих находок (P0/P1/P2) нет. Все четыре P3-находки прогона 8
закрыты корректно (каждая перепроверена по исходнику, не по commit message).
Реализация, атомарный протокол, unsafe-инвентарь, layout-контракт,
публикационные метаданные, тесты, бенчмарк и CI согласованы.

Одна новая P3-находка (имя нового теста обещает три состояния, тест доказывает
два — дешёвый однострочный фикс) и шесть P4-замечаний (два новых, четыре
перенесённых из прогона 8). Ни одна не затрагивает корректность, soundness или
честность публичных обещаний. Публикация по-прежнему упирается только в
известные owner-gated решения (см. «Стоячие pre-publish условия» ниже), не в
код.

## Проверка закрытия находок прогона 8 (персонально, по исходнику)

Дифф `df0074e..HEAD` — ровно 5 файлов (ci.yml, CHANGELOG.md, imp.rs,
cell_unit.rs, loom_racy_ptr_cell.rs), ничего вне заявленных фиксов; сам крейт
больше никто не трогал (состав tarball — те же 10 файлов по `git ls-files`).

- **Run-8 F1 (loom module doc без property 7) — закрыта корректно.**
  `tests/loom_racy_ptr_cell.rs:34-40`: пункт 7 добавлен в список с точной
  формулировкой («get() never reports a cell held at INITIALIZING»), плюс
  явная оговорка «Numbered by content, not by physical position below:
  property 7's section sits between 5 and 6». Сверено с баннерами в теле:
  секции идут 1-4 (`:124`), 5 (`:322`), 7 (`:400`), 6 (`:503`) — оговорка
  соответствует действительности, снята зависимость от порядка, а не скрыта.
- **Run-8 F2 (несуществующая README-секция «Running the loom suite») —
  закрыта корректно.** `loom_racy_ptr_cell.rs:76-80` теперь цитирует «The two
  rules people get wrong»; такая секция в README реально существует
  (`README.md:191`), содержит и дословно ту же команду запуска
  (`README.md:214` против `loom_racy_ptr_cell.rs:71-72`), и предупреждение о
  глобальности `--cfg loom` со «scope the flag» (`README.md:217-226`).
  Выбран вариант «поправить цитату» (а не заводить новый README-заголовок) —
  допустимый из двух предложенных.
- **Run-8 F3 (msrv-job без строк `-p racy-ptr-cell`) — закрыта корректно,
  покрытие реальное, не дублирующее.** `.github/workflows/ci.yml:2002-2014`:
  добавлены `cargo check -p racy-ptr-cell` и `cargo test -p racy-ptr-cell
  --no-run` в job с pinned `dtolnay/rust-toolchain@1.88` (`:1956`).
  Аргумент комментария перепроверен: workspace НЕ виртуальный (root package =
  `sefer-alloc`), корневые шаги `cargo check/test --no-run --all-features`
  без `-p`/`--workspace` действуют только на корневой пакет, так что
  `tests/cell_unit.rs` и дев-граф крейта на 1.88 раньше не компилировались
  нигде — новые строки закрывают именно этот пробел, дубликата нет (все
  прочие jobs с racy-ptr-cell-тестами — stable/nightly). Замечание о
  корректности формулировки: `cargo test --no-run` собирает
  `bench-scale-tool` (dev-deps передаются `--extern` каждому тест-таргету,
  поэтому rlib собирается независимо от фактического использования), и
  комментарий аккуратно называет только `cell_unit.rs` и `bench-scale-tool`,
  НЕ утверждая, что собирается сам bench-таргет (`test = false` исключает
  его из `cargo test`) — остаточный зазор см. N1 ниже; он был явно назван
  опциональным в самой находке F3 прогона 8, так что это осознанный выбор,
  не ошибка фикса.
- **Run-8 F4 (нет `impl Debug`) — закрыта корректно; сам impl аудирован
  отдельно, см. следующую секцию.** `src/imp.rs:698-716` + тест в
  `cell_unit.rs:349-362` + запись в CHANGELOG (`:71-74`). Одна остаточная
  неточность в ИМЕНИ теста — новая находка F1 этого прогона.

## Аудит нового `impl Debug` (главный новый код с прогона 8)

`impl<T> core::fmt::Debug for RacyPtrCell<T>` (`src/imp.rs:698-716`):

- **Никакого лишнего/неверного atomic:** ровно один `load(Ordering::Relaxed)`.
  `Relaxed` здесь корректен, и не только по заявленному в doc основанию
  («указатель не выдаётся на dereference»): даже для читателя, узнавшего о
  публикации через другой happens-before-канал (свой `Acquire`-`get` или
  сообщение из другого потока), write-read coherence гарантирует, что
  Relaxed-load на том же атомике не прочитает значение старее уже
  установленного HB-стора — то есть Debug не может напечатать «Uninit» после
  того, как этот поток уже доказуемо видел «Ready». Слабее делать некуда,
  сильнее — незачем; сравнение с Acquire-`dbg_is_ready` в doc объяснено.
- **Ничего не течёт:** `T` не dereference'ится и не появляется в выводе;
  без bound `T: Debug` (это и даёт downstream `#[derive(Debug)]` на
  конкретных metadata-структурах, как заявляет CHANGELOG). Печатается только
  адрес (`{:p}`) в `Ready`-ветви — обычная практика Debug для
  указательных типов (`NonNull`, `AtomicPtr` делают то же); из строки адрес
  в рабочий указатель без exposed provenance не превращается.
- **Классификация полна и однозначна:** `0` / `SENTINEL_INITIALIZING` (match
  по `const usize` — валидный паттерн) / остальное; третьего значения у
  протокола нет. Реальный указатель с адресом 1 недостижим (align >= 2 +
  release-active `assert!`), так что ветви не пересекаются.
- **Cfg-совместимость:** компилируется в обоих вариантах (`loom` /
  `core::sync::atomic` — оба алиаса дают `load(Relaxed)` и `*mut T` с
  `.addr()`); `#![deny(missing_docs)]` на trait-impl-методы не
  распространяется, но doc всё равно написан; `no_std`-чистота сохранена
  (`core::fmt`, без аллокаций в самом impl).
- **Инвентарь unsafe не изменился:** комментарий-инвентарь в `lib.rs`
  (Send/Sync + четыре `NonNull::new_unchecked`) остаётся точным — Debug не
  добавил ни одного unsafe-сайта.
- **Тест** доказывает `Uninit`- и `Ready`-выводы и (компиляцией) отсутствие
  `T: Debug`-bound; сверка `{p:p}` NonNull-против-`*mut` корректна (оба —
  Pointer-форматирование одного адреса). Пробел — в имени, см. F1.

## Находки

### F1 — P3: тест `debug_reports_the_three_states_without_a_t_debug_bound` доказывает два состояния из трёх, заявленных в его имени

**Где:** `crates/racy-ptr-cell/tests/cell_unit.rs:348-362` против
`src/imp.rs:709-713`.

Тест ассертит `RacyPtrCell(Uninit)` и `RacyPtrCell(Ready({p:p}))`;
`RacyPtrCell(Initializing)` не проверяется нигде (grep по обоим тестовым
файлам: строка «Initializing)» не встречается). Commit message `a8bf944` сам
честно говорит «proving both the Uninit and Ready output» — то есть дрейф
именно в имени теста, а не в понимании автора. Между тем третье состояние
дешево и детерминированно достижимо в один поток: `fmt` берёт `&self`, так
что init-замыкание может отформатировать ту же cell, пока само держит
sentinel:

```rust
let p = cell.get_or_try_init(|| {
    assert_eq!(format!("{cell:?}"), "RacyPtrCell(Initializing)");
    Some(leak(0xBEEF))
}).unwrap();
```

Это ровно тот класс «имя теста обещает больше, чем тест доказывает», который
эта же серия ревью дважды исправляла (#1399, #1404), причём Initializing-ветвь
impl'а — единственная из трёх, которую сейчас не покрывает вообще ни один
тест (удаление её `SENTINEL_INITIALIZING`-arm'а, схлопывающее вывод в
`Ready(0x1)`, оставит весь suite зелёным — оракул этой ветви вакуумный).

**Фикс (предпочтительный):** добавить in-closure-ассерт выше — тест станет
доказывать все три состояния, имя станет точным, ветвь перестанет быть
непокрытой. Альтернатива-минимум: переименовать в
`debug_reports_uninit_and_ready_without_a_t_debug_bound`.

### P4-замечания (не требуют действий до публикации)

- **N1 (новое, остаток F3):** msrv-строки не компилируют сам bench-таргет на
  1.88 (`test = false` исключает его из `cargo test --no-run`; собирается
  только dev-граф). MSRV-обещание `rust-version` относится к библиотеке,
  которую только и собирает потребитель, так что это не дефект; если
  захочется паритета и для bench — третья строка
  `cargo check -p racy-ptr-cell --all-targets`, как и было названо в F3
  прогона 8.
- **N2 (новое):** `benches/racy_ptr_cell_bench.rs:7-9` — «WORKING rows … and
  their matching harness `baseline/*` rows» читается как 1:1-пары, но у
  `get/hot` и `get_or_try_init/warm_already_ready` baseline-строк нет (и по
  делу — там нечего вычитать); слово «matching» стоит смягчить до «and the
  `baseline/*` rows for the cold/contention scenarios».
- **N3 (перенос run-8 N1):** `Cargo.toml` description — «The cell's own
  operations use no std sync and no heap» без квалификатора «non-panicking»,
  который lib.rs/README несут везде.
- **N4 (перенос run-8 N2):** комментарий у `[lints] workspace = true`
  называет «harmlessly unused here» только `kani` из трёх разделяемых cfg.
- **N5 (перенос run-8 N3):** `CHANGELOG.md:7` «0.1.0 - Unreleased» — заменить
  на дату при фактическом релизе (гейтится release.yml; механика релиза, не
  дефект).
- **N6 (перенос run-8 N4):** `MODE_CONTEND` в `Contention::round` матчится
  wildcard-веткой `_ =>`; будущий новый режим без правки match молча станет
  contention.

## Полный свежий проход (подтверждение без изменений)

Вне продиффованных 5 файлов крейт не менялся с прогона 8; тем не менее
перечитаны целиком `lib.rs`, `imp.rs`, оба тестовых файла, бенчмарк, README,
CHANGELOG, Cargo.toml и все racy-ptr-cell-строки ci.yml. Подтверждаю выводы
прогона 8 по неизменённым частям, в сжатом виде:

- **State machine / liveness:** три состояния разделены адресом одного
  `AtomicPtr<T>`; loser-спин строго `while == INITIALIZING` во всех трёх
  местах чтения; OOM-rollback и unwind-rollback (`RollbackGuard`,
  defuse-дисциплина: до публикации взведён, defuse только после
  publish/явного rollback) корректны; release-active `assert!` на
  sentinel-адрес из safe-`init` на месте, его unwind проходит через guard.
- **Ordering:** load-bearing пара `store(Release)` ↔ `load(Acquire)` во всех
  путях; rollback-сторы `Release`; claim-CAS success `Acquire`/failure
  `Relaxed` с дисциплинированно задокументированным открытым вопросом об
  ослаблении (loom-контрфактуал + измерение на weakly-ordered target до
  любого изменения — позицию поддерживаю).
- **Unsafe/provenance:** инвентарь неизменен (2 `unsafe impl` + 4
  `new_unchecked`, каждый доминируем non-null/non-sentinel-проверкой);
  sentinel — `without_provenance_mut`, только сравнение; тесты
  провенанс-чистые.
- **Метаданные:** license MIT OR Apache-2.0 + оба текста; 5 валидных
  keywords; 3 валидных categories; `[[bench]] harness = false, test = false`
  явно; tarball — 10 файлов, лишнего нет; CHANGELOG соответствует API
  (включая новую Debug-запись — сверена с impl дословно).
- **Тесты:** cell_unit — теперь 9; оракулы невакуумные (два
  `#[should_panic]` c контрфактуальными верификациями в комментариях,
  timeout-схемы переводят потенциальный livelock в проваленный прогон);
  loom-suite — 6 real-type-тестов + 2 контрфактуала, module-doc-список
  теперь полон (7 свойств). Doc-комментарий у
  `layout_matches_a_single_atomic_ptr` (`cell_unit.rs:364-374`) стоит
  непосредственно над своей функцией и документирует именно её — вставка
  Debug-теста выше его не сместила (проверено построчно).
- **Бенчмарк:** методология после #1391→#1398→#1403→#1407 в порядке; timed
  области не аллоцируют; формулировки makespan/differential estimate
  соответствуют методу. Остатки — только N2/N6.
- **CI:** native debug+release, thumbv7em build, clippy `--all-targets -D
  warnings`, rustdoc `-D warnings`, miri (обычный + strict-provenance),
  real-type loom с tee+grep-sentinel, package-gate `cargo publish --dry-run`,
  и теперь msrv-строки. Имена тестов в комментариях ci.yml существуют в
  файлах; счётных числительных нет.

## Стоячие owner-gated pre-publish условия (не находки кода)

Неизменны с items 133–135; публикацию гейтят они, а не код:

1. Явный go-ahead владельца на `cargo publish` (не дан).
2. Item 28 (`docs/correctness-open-items/TRACKED_publish_readiness.md`) —
   one-way-door решение об имени крейта («racy» читается наоборот) и длинном
   description, плюс повторная проверка доступности имени на crates.io
   непосредственно перед публикацией.
3. Item 29 — осознанное решение `deny(missing_docs)` vs `warn` + CI-гейт до
   первого publish.
4. Дата-штамп в CHANGELOG при релизе (release.yml-гейт; N5).

## Возможности ускорения

Новых нет; Debug добавил ровно один Relaxed-load на диагностическом пути —
дешевле не бывает. Подтверждаю позицию кода по кандидатам (claim-CAS success
`Acquire` → `Relaxed`, спин-итерации → `Relaxed`+`Acquire`-перечтение на
выходе): на x86-64 выигрыш недоказуем в принципе, без loom-контрфактуала и
AArch64-измерения не трогать. Fast path (один `Acquire`-load + два integer
compare + branch; `#[cold] #[inline(never)]` slow path) — уже минимум.

## Границы уверенности

- Аудит статический, read-only: код не компилировался и не исполнялся;
  зелёность CI/miri/loom и локальные верификации коммитов — заявленные
  артефакты, не мои прогоны.
- Утверждение о том, что `cargo test --no-run` собирает `bench-scale-tool`
  (dev-deps как `--extern` у тест-таргетов), — рассуждение о поведении cargo,
  согласующееся с записью в `70f0294` о локальной проверке на 1.88, но не
  перепроверенное исполнением в этом прогоне.
- Рассуждение о корректности `Relaxed` в Debug (coherence) — вывод из модели
  памяти по исходнику, не model-checking.
- Статус `once_cell_try` (нестабильность `OnceLock::get_or_try_init`) —
  как в прогоне 8: механически перепроверить на актуальном toolchain
  непосредственно перед публикацией.

## Сводка

| # | Severity | Находка | Блокер? |
|---|----------|---------|---------|
| F1 | P3 | имя нового Debug-теста обещает «three states», ассертятся два; `Initializing`-ветвь impl'а не покрыта ни одним тестом (достижима детерминированно из init-замыкания) | нет |
| N1 | P4 | msrv-строки не собирают bench-таргет на 1.88 (осознанный остаток F3; опциональная `--all-targets`-строка) | нет |
| N2 | P4 | bench module doc: «their matching baseline/* rows» подразумевает 1:1-пары, которых нет у hot/warm-строк | нет |
| N3–N6 | P4 | переносы из прогона 8: description без «non-panicking»; `[lints]`-комментарий про один cfg из трёх; дата в CHANGELOG при релизе; wildcard-ветка `MODE_CONTEND` | нет |

**Итог: GO** — четвёртый подряд безусловный GO серии. P0/P1/P2 нет; F1 —
рекомендованная однострочная полировка до (или сразу после) первого релиза;
публикацию гейтят только четыре известных owner-gated условия выше.
