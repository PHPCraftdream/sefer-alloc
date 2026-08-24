# Read-only pre-publication audit — `numa-shim`

**Автор:** Сол-кодекс (`Sol-codex`)  
**Время:** 2026-08-24 11:31:55 Europe/Berlin  
**Проверенная ревизия:** `58d94da80bf829f0675290546021f5916402fb8c`  
**База:** `origin/main` = `a84840824f6e3b5a0a802fe27358489d3d7a68e3`; локальный HEAD опережает её двумя документационными коммитами.  
**Режим:** только чтение, один агент, без под-агентов. Тесты, сборки, `cargo check`, Clippy, Miri, bench и `cargo publish` не запускались. GitHub Actions проверен только read-only запросом метаданных; для точного HEAD CI-run не найден (`gh run list ...` вернул `[]`).

## Итог

**К немедленной публикации не готов.**

По статическому анализу подтверждённого UB, double-free, UAF или ошибки FFI-layout не найдено. Но есть один новый функциональный блокер Linux (`bind_range` почти наверняка не работает для показанного в README обычного `Vec`), две обязательные release-предпосылки и несколько важных остаточных ограничений.

После исправления контракта `bind_range`, подготовки версии `0.2.0`, финального Phase-1 gate на том же SHA и зелёного CI вердикт может быть **условным GO**. Фазы 2/4 NUMA-gate остаются сознательно принятым owner waiver, а не доказательством готовности.

## Findings

### P1 — Linux `bind_range` silently no-ops для обычного heap-адреса

**Доказательство:** `crates/numa-shim/src/lib.rs:449-529` передаёт `base` напрямую в `mbind(2)`. Safety-контракт требует только live mapped range (`:498-512`), но не page-aligned address. README показывает `Vec<u8>` (`crates/numa-shim/README.md:48-53`), чей адрес обычно не выровнен на системную страницу.

Linux man-pages указывает `EINVAL`, если `addr` не кратен размеру страницы: [mbind(2), раздел ERRORS](https://man7.org/linux/man-pages/man2/mbind.2.html#ERRORS). Код игнорирует результат syscall (`src/lib.rs:1090-1132`), поэтому пользователь получает обычный успешный возврат Rust-функции, но NUMA policy не установлена. Тест `tests/smoke.rs:25-51` проверяет только отсутствие падения и доступность буфера, а не binding.

**Исправление:** выбрать и зафиксировать один контракт до публикации:

- либо требовать page-aligned `base` для Linux и заменить README-пример на page-aligned reservation;
- либо принимать page envelope, явно требуя от вызывающего mapped соседние страницы;
- либо добавить возвращаемую ошибку/статус syscall.

Наиболее безопасный малый вариант — первый; без него headline API создаёт ложное ощущение работающего binding.

### P1 — узлы `>= 64` теряются, а fallback маскируется под node 0

`bind_range_impl_linux` использует один `u64` и сразу возвращается для `node >= 64` (`src/lib.rs:1099-1112`). Топология также сканирует только `0..64` (`:941-971`). Поэтому на системе с 64+ NUMA nodes `current_node()` может вернуть `Some(0)` для CPU, фактически находящегося на другом узле, а `reserve_on_node(..., node >= 64)` silently не применит binding.

Ограничение честно задокументировано (`src/lib.rs:466-471`, `README.md:158-161`), поэтому это не скрытая регрессия. Но для заявленного Linux NUMA API это реальный correctness gap, уже отмеченный как оставшийся binding-side residual F4/N8.

**Исправление:** динамический nodemask и расширенный topology scan; альтернативно — явно объявить поддержку только node IDs `0..63` и возвращать диагностируемую ошибку/`None`, а не silently succeed. `FellBackToZero` желательно разделить на «topology unavailable» и «CPU not mapped».

### Release blocker — crate всё ещё имеет версию `0.1.0`

`crates/numa-shim/Cargo.toml:3` остаётся `0.1.0`, `CHANGELOG.md:7-13` остаётся `Unreleased`, а root pin — `Cargo.toml:934` (`version = "0.1"`). CHANGELOG сам фиксирует breaking changes (`CHANGELOG.md:148-195`), включая удаление Cargo-feature `mock`, поэтому следующий выпуск должен быть `0.2.0`, а не `0.1.1`.

До публикации также нужно обновить три `0.1`-ссылки в README, dated CHANGELOG section, SHA в waiver и выполнить финальный Phase-1 rerun последним изменением. Release workflow намеренно остановит текущий tree на changelog/version guard.

### Release blocker — нет CI-результата для точного HEAD

Текущий HEAD `58d94da...` опережает `origin/main` и не имеет CI-run. `.github/workflows/release.yml` требует завершённый успешный CI именно для `github.sha`, а не просто зелёный run старого `a848408`.

Нужно запушить финальный SHA, дождаться CI и проверить job-level result перед tag/publish. В этом аудите push не выполнялся.

### P1/P2 — NUMA gate всё ещё не даёт полной доказательной базы

Waiver `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md:14-22` разрешает выпуск `0.2.0` с незапущенными Phase 2 (real Linux/QEMU) и Phase 4 (real multi-socket). Phase 3 остаётся частичной; Phase 1 требует финального rerun уже после version bump. Это принятое владельцем исключение, но не техническое закрытие риска.

Для потребителей нужно сохранить caveat: реальное распределение страниц между NUMA nodes не подтверждено на multi-node topology. Перед публикацией заполнить release SHA и записать финальный Phase-1 report/raw log, как требует сам waiver.

### P2 — Windows release-build проверяет commit pointer только через `debug_assert!`

`src/lib.rs:1434-1464` предполагает, что `VirtualAllocExNuma(MEM_COMMIT)` возвращает ровно `base`, и проверяет это только через `debug_assert_eq!`. В release-сборке mismatch будет принят `Reservation::from_raw_parts`, после чего handle может описывать не тот committed span.

На корректной Windows API это ожидаемое поведение, поэтому подтверждённой текущей ошибки нет. Но это load-bearing FFI assumption и release hardening должен быть unconditional: при mismatch освободить reservation и вернуть `None`.

### P2 — cold path занимает около 64 KiB стека и делает до 64 syscall triples

`Topology` хранит `[u8; 1024]` для каждого из 64 узлов (`src/lib.rs:861-949`), а инициализируется локальным значением внутри `OnceLock`. Первый `current_node()` может выполнить до 64 `open/read/close` троек и создать крупный stack frame; это особенно неприятно на small-stack threads и на allocator cold path.

После инициализации lookup остаётся O(nodes × cpumap bytes): `cpu_to_numa_node_checked` перебирает до 64 карт, а `parse_contains_cpu` повторно сканирует строку (`:952-983`, `:667-691`).

**Улучшение:** построить allocation-free reverse index `CPU -> node` либо хотя бы один раз разобрать слова cpumap без двойного token scan; уменьшить фиксированный buffer/stack footprint или хранить topology в заранее выделенной статической области с аккуратной синхронизацией. Изменение стоит делать только с измерением cold/hot lookup.

### P2 — tuple-вариант `NodeResolution::Resolved(u32)` семвер-хрупок

`NodeResolution` имеет `#[non_exhaustive]`, но `Resolved(u32)` намеренно не имеет field-level `#[non_exhaustive]` (`src/lib.rs:277-298`). Добавление в будущем CPU id, source или confidence потребует изменения tuple shape и сломает downstream destructuring. Это не текущий баг и решение задокументировано, но до первой публикации API ещё можно сделать вариант non-exhaustive или заменить его на отдельную struct.

### P2 — `#[doc(hidden)] pub` не является настоящей приватностью

`cpumap` и Linux forwarder остаются публичными (`src/lib.rs:608-618`, `:758-769`). Документация объявляет их semver-exempt, что соответствует принятому owner decision, но Rust не запрещает downstream-коду их использовать. Это допустимая зафиксированная политика, не фактическая граница API; при дальнейшей эволюции лучше заменить на отдельный test cfg/support crate.

### P3 — неточность mock-документации

`mock::set_current_node` документирован как установка значения, которое вернёт «следующий» вызов (`src/lib.rs:227-230`), но `CURRENT_NODE_SLOT` не сбрасывается и значение возвращается всеми последующими вызовами до следующего `set_current_node`. Стоит заменить `next call` на `subsequent calls until changed`.

## Что выглядит хорошо

- Linux/Windows/macOS/miri cfg-ветки взаимно разделены; custom mock cfg больше не является Cargo-feature и не активируется через `--all-features`.
- Unsafe FFI-сайты имеют локальные `SAFETY`-комментарии; raw Windows layout и ownership chain просмотрены, подтверждённого UB/double-release не найдено.
- Windows overflow cleanup после `checked_add` присутствует (`src/lib.rs:1371-1383`), commit-failure освобождает reservation (`:1412-1431`).
- Linux topology cache теперь не использует `Vec` во время `OnceLock`-инициализации, что устраняет прежний риск reentrant global allocator.
- Feature/docs.rs wiring согласован: `vmem-integration` явно включён в docs.rs, mock cfg не попадает туда случайно.

## Рекомендуемый порядок перед публикацией

1. Исправить или сузить Linux `bind_range` contract; отдельно принять решение по node IDs `>=64`.
2. Поднять `numa-shim` до `0.2.0`, обновить root pin, README и dated CHANGELOG; заполнить waiver SHA.
3. Последним изменением выполнить и записать Phase-1 rerun; затем push финального SHA и дождаться CI именно для него.
4. Сделать `cargo publish --dry-run -p numa-shim` на post-bump tree и проверить packaged crate с registry `aligned-vmem`.
5. Производительность topology/cache и Windows syscall count измерять отдельным pass; без measurements не заявлять speedup.

**Финальный verdict:** сейчас **NO-GO**. После release housekeeping без решения `bind_range` остаётся максимум **CONDITIONAL GO**, а не unconditional GO.
