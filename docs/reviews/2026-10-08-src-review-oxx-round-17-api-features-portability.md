# Ревью `src/` — раунд 17 (oxx) — публичный API и семвер-поверхность, матрица features/cfg, портируемость платформ

## 1. Охват, инвентарь, метод, база, вердикт

**База:** `main` @ `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`). Push-CI на этом SHA зелёный: run `37785720332` (только чтение через `gh`).

**Ревьюер и режим:** oxx (Claude Opus 5.5, effort=max), один из восьми тематических ревьюеров раунда 17, изолированный git worktree, без суб-агентов. **Ревью проведено без компиляции и исполнения кода** (приказ владельца: режим только чтение). Не запускались `cargo` (включая `check`/`metadata`), `rustc`, тесты, witness, Miri/Loom/Kani, бенчмарки и кросс-цели. Исполненные свидетельства в отчёте — это только логи уже завершённых прогонов GitHub Actions, прочитанные через `gh` (приложение C). Каждое такое свидетельство повторно сверено с исходником на базовом SHA. Исходники не правились; создан один файл — этот отчёт.

**Тема:** (a) публичная поверхность и семвер; (b) матрица features/cfg; (c) портируемость; (d) граница с `crates/`.

**Инвентарь** (подсчитан командами приложения A, не скопирован):
- `src/`: 168 файлов, 43 325 физических строк.
- Крейт-корень реэкспортирует 22 элемента: `Handle`, `Region`, `SyncRegion` (из `sefer-region`); `EpochHandle`, `EpochRegion`, `LockFreeHandle`, `LockFreeRegion`, `ShardedHandle`, `ShardedRegion` (`experimental`); `PinnedRunner` (`pinning`); `LargeCacheConfig`, `LargeCacheMode`, `LargeCachePolicy`, `Profile`, `SmallPoolPolicy`, `SmallSegmentPoolConfig` (`alloc-core+alloc-decommit`); `AllocCore`, `SegmentLayout`, `InvalidAlign` (`alloc-core`; последний — из `size-classes`); `AllocStats`, `MaintenanceStartError`, `SeferAlloc` (`alloc-global`). Плюс три пути модулей `alloc_core`/`global`/`registry` — `#[doc(hidden)] pub` только при `internals` (`src/lib.rs:433–483`).
- По всему `src/`: 478 строк `pub fn`/`pub unsafe fn`/`pub const fn`; 430 строк `pub(crate|super|in …) fn`; 118 строк `pub struct/enum/trait/type/const/static`; 95 строк `pub use`; 455 атрибутов `#[doc(hidden)]`.
- Хуки: 246 строк `pub (unsafe) fn dbg_*` видны сканеру `scripts/verify-dbg-hook-safety.mjs`, из них 30 — `pub unsafe fn`. Невидимы сканеру: 2 `pub const fn dbg_*` и 47 уникальных имён публичных функций `*_for_test(s)` без префикса `dbg_`.
- Features: 30 объявлено (`Cargo.toml:120–876`; граф импликаций — приложение B); в `src/` упоминаются 26 имён, все объявлены.
- cfg: 1 281 `#[cfg(`/`#![cfg(`, 61 `cfg_attr`, 20 `cfg!`, 188 предикатов `not(feature = …)`. Целевые cfg в `src/` — только `target_pointer_width` (`src/lib.rs:391`), плюс `miri`/`loom`/`kani`. Нет ни одного `unix`/`windows`/`target_os`: ОС-специфика целиком в `crates/aligned-vmem` и `crates/numa-shim`.
- Тавтологии: в `src/registry` 118 атрибутных строк, в `src/global` 24 — с фичами, которые в этих модулях всегда включены (`alloc-global`/`alloc-xthread`/`alloc-core`/`std`). Невозможные ветки — 2 (`src/global/fallback.rs:322,481`).

**Метод.**
1. **Обязательное чтение индексов:** `docs/CORRECTNESS_OPEN_ITEMS.md` (целиком, с lookup-таблицей); `docs/correctness-open-items/ACTIVE.md`, `TRACKED_platform_contracts.md`, `TRACKED_misc.md` — целиком; `TRACKED_ci_gate_coverage.md` — карточки 19–82 и 88/95/107/140 целиком, остальное по заголовкам; `TRACKED_hook_safety.md` — шапка, начало п. 5, п. 9 целиком; остальные `TRACKED_*` — по заголовкам и Status. `docs/perf/OPEN_ITEMS.md` — шапка (1–245), все Status-строки, пункты 64–83 и Recently resolved целиком; длинные исторические нарративы `[D]/[L]` — по current-state карточкам, как в R16. Также прочитаны отчёт R16 и манифест `SRC_REVIEW_R16_MANIFEST.md`.
2. **Полное построчное чтение** (код и комментарии, ≈4 600 строк): `src/lib.rs`, `src/alloc_core/mod.rs`, `src/global/mod.rs`, `src/global/sefer_alloc/{mod,core,diag,global_alloc,maintenance,batch}.rs`, `src/global/{alloc_stats,maintenance_start_error,tls_heap}.rs`, `src/alloc_core/segment/segment_layout.rs`, `src/alloc_core/platform/numa.rs`, `src/alloc_core/small/alloc_core_small/find_segment.rs`, `src/alloc_core/small/mod.rs`, `src/concurrent/mod.rs`, `src/registry/segment_route/{shard_lock,pin}.rs`.
3. **Выборочно, по диапазонам:**
   - `src/alloc_core/large/alloc_core_large.rs` (1–40, 150–760), `src/alloc_core/large/alloc_core_large_cache.rs` (360–425);
   - `src/alloc_core/alloc_core/mem/{realloc_fastpath.rs` (1–30, 286–561)`, mem_impl.rs` (1–262)`}`;
   - `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` (1–370), `src/alloc_core/small/alloc_core_small_magazine.rs` (1–160, 405–470);
   - `src/alloc_core/alloc_core/{bootstrap.rs` (40–135)`, lifecycle.rs` (88–167)`, alloc_core_core_diag/header_diag.rs` (1–100)`}`;
   - `src/alloc_core/config/{large_cache_config.rs` (170–245, 370–410)`, profile.rs` (100–200)`}`, `src/alloc_core/platform/size_classes.rs` (225–355);
   - `src/alloc_core/segment/bitmap/magazine_bitmap.rs` (115–149), `src/alloc_core/segment/segment_header/segment_header_views.rs` (5–30);
   - `src/registry/bootstrap/registry.rs` (190–285), `src/registry/heap_slot.rs` (90–180), `src/registry/segment_route/small_sidecar.rs` (70–165);
   - `src/global/fallback.rs` (300–330, 400–500), `src/global/maintenance_service.rs` (1–60, 160–235);
   - `src/concurrent/sharded/sharded_region.rs` (366–530, 700–790), `src/concurrent/epoch/epoch_region.rs` (630–670), `src/concurrent/lock_free/lock_free_region.rs` (500–545), `src/concurrent/pinning.rs` (1–175).
4. **Скрининг всех 168 файлов** grep-запросами с чтением каждого попадания (приложение A):
   - все cfg-предикаты; gated `use` против мест использования; `not(feature)`-ветки;
   - публичная поверхность: `pub fn`, `#[doc(hidden)]`, `*_for_test(s)`, `pub const fn dbg_`; параметры с cfg в сигнатурах; cfg-варианты публичных enum; `unsafe impl Send/Sync`;
   - `std::process::abort()`; счётчики спиннинга;
   - `AtomicU64`/`target_has_atomic`; endianness (`to/from_*_bytes`, `transmute`, байтовые чтения полей); `align(64)`;
   - `PAGE`/`page_size()`; TLS (`thread_local!`, `Instant::now`, `thread::current`); новые API std для оценки MSRV.
5. **Сверка с манифестами и CI** (только чтение):
   - `Cargo.toml` — `[package]`, `[features]`, `[dependencies]` целиком;
   - `scripts/check-matrix.mjs` — `PER_PR_ROWS`; `scripts/verify-dbg-hook-safety.mjs` — логика целиком, таблицы по запросам;
   - `.github/workflows/ci.yml` — jobs msrv, msrv-runtime-windows, no_std, docs, feature-powerset, test-feature-isolation, multi-arch, test-macos и grep всех вызовов `cargo`;
   - `README.md` — «Target support», «Features matrix»; `docs/INTEGRATION.md` §1; стражи `tests/no_stale_doc_references.rs` и `tests/no_panic_doc_accuracy.rs` — по запросам.
6. **CI-свидетельства:** список плановых прогонов и логи четырёх jobs (приложение C).

**Не запускалось** (режим только чтение): любые сборки и тесты, `cargo check` комбинаций, `cargo metadata`, `rustc --print cfg`, кросс-цели, Miri/Loom/Kani/sanitizers, бенчмарки. Утверждение «не компилируется» в этом отчёте опирается на лог CI или помечено как ГИПОТЕЗА.

**Вердикт.**
- **Одна находка P3 — R17-API-01:** `numa-aware` без `alloc-xthread` не компилируется (E0433). Это зафиксировал плановый CI 2026-10-05, но нигде не записано, и на базе дефект сохраняется.
- **Восемь находок P4** — R17-API-02…09:
  - красный еженедельный powerset останавливается на первой ошибке и скрывает половину матрицы;
  - граница `internals` протекает для не-`dbg_` членов типов крейт-корня, сканер их не видит;
  - `numa-aware` молча выключает ещё три механизма;
  - документация матрицы фич разошлась с манифестом;
  - текст no-panic/abort контракта `SeferAlloc` неточен, а в debug возможна паника переполнения в спинлоке fallback (наблюдалась в CI);
  - `experimental` использует `AtomicU64` без гейта атомиков;
  - ГИПОТЕЗА о требовании нативного TLS;
  - `core_affinity::CoreId` выставлен в публичных сигнатурах без реэкспорта.
- P0/P1/P2 **в прочитанной области и прочитанным методом** не найдено. Это не доказательство отсутствия ошибок, не release GO и не perf GO.

## 2. Находки

Шкала — как в задании раунда.
- **P3** — ограниченный дефект; сюда входит ошибка компиляции в поддерживаемой комбинации фич.
- **P4** — вводящий в заблуждение контракт, пробел покрытия или поддерживаемость без установленного runtime-сбоя.

Классы доказательства:
- **SOURCE-CONFIRMED** — путь установлен чтением кода на базе;
- **исполнено в CI** — наблюдено в логе завершённого прогона (раньше базы) и перепроверено чтением на базе;
- **ГИПОТЕЗА** — не подтверждено; указано условие, при котором она ложна.

| ID | Severity | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R17-API-01 | **P3** | исполнено в CI (лог) + SOURCE-CONFIRMED на базе | любая сборка с `numa-aware`/`numa-aware-mock` без `alloc-xthread`, т. е. standalone `AllocCore` + NUMA | E0433: импорт `SegmentMeta` загейтован `alloc-xthread`, а используется под `numa-aware` (`alloc_core_large.rs:481,693`) |
| R17-API-02 | P4 | исполнено в CI + SOURCE-CONFIRMED (`ci.yml`) | CI-гейты матрицы фич и MSRV | weekly powerset красный с 2026-10-05, не записан; без `--keep-going` 185/390 комбинаций (+14 aligned-vmem) не проверены; нет per-PR строки `numa-aware` без `production`; MSRV не компилирует `cfg(not(<фича production>))` |
| R17-API-03 | P4 | SOURCE-CONFIRMED | `production` (`AllocCore`, `SegmentLayout`), `experimental` (регионы) | `#[doc(hidden)] pub` не-`dbg_` члены типов крейт-корня без `internals`; одна safe-функция позволяет одному потоку забрать все шарды `ShardedRegion`; сканер не видит `*_for_test(s)` и `pub const fn dbg_*` |
| R17-API-04 | P4 | SOURCE-CONFIRMED; (c) — ГИПОТЕЗА о частоте | `numa-aware` вместе с `production` (Windows), `large-reserved-capacity`, standalone `AllocCore` + `alloc-segment-directory` | `numa-aware` молча отключает `primordial-lazy-commit`, `large-reserved-capacity` и OOM-rescue R9-8; README называет ловушкой только `small-segment-lazy-commit` |
| R17-API-05 | P4 | SOURCE-CONFIRMED | документация фич | `docs/INTEGRATION.md` даёт старый состав `production` из 4 фич (на него ссылается rustdoc), README-матрица ошибается в зависимостях `alloc-stats` и молчит про `internals`, 27 комментариев `Cargo.toml` ссылаются на несуществующие файлы, константы `SegmentLayout` зависят от фич без оговорки, нет `doc(cfg)` |
| R17-API-06 | P4 | SOURCE-CONFIRMED; (c) исполнено в CI | контракт `GlobalAlloc` при любой сборке с `alloc-global`; (c) — debug-сборки, путь fallback | «The one deliberate process kill» против 99 инвариантных `abort()` в 23 файлах на путях `GlobalAlloc`; `LockGuard::acquire` переполняет `u32`-счётчик в debug (лог CI: `attempt to add with overflow` внутри аллокатора) |
| R17-API-07 | P4 | SOURCE-CONFIRMED (код); ГИПОТЕЗА (конкретные цели) | `experimental`/`pinning` на std-целях без 64-битных атомиков | `std::sync::atomic::AtomicU64` без гейта `target_has_atomic = "64"`, тогда как компаньоны свои счётчики гейтуют |
| R17-API-08 | P4 | ГИПОТЕЗА | `#[global_allocator] SeferAlloc` на целях, где std реализует TLS через OS-ключи | первое обращение к `thread_local!` выделяет ячейку через глобальный аллокатор → рекурсия; требование нативного TLS не задокументировано |
| R17-API-09 | P4 | SOURCE-CONFIRMED | `pinning` (не deprecated) | `PinnedRunner` выставляет `core_affinity::CoreId` в публичных сигнатурах без реэкспорта: мажор `core_affinity` становится частью семвера `sefer-alloc` |

### R17-API-01 — `numa-aware` без `alloc-xthread` не компилируется (E0433 `SegmentMeta`); CI показал это 2026-10-05, запись нигде не сделана

**Места.**
- `src/alloc_core/large/alloc_core_large.rs:15–16`: `#[cfg(feature = "alloc-xthread")] use crate::alloc_core::segment_header::SegmentMeta;`.
- `:692–693`: `#[cfg(feature = "numa-aware")] SegmentMeta::new(base).set_node_id(my_node);` в `alloc_large_slow`.
- `:478–482`: то же в ветке large-cache hit, внутри `#[cfg(feature = "alloc-decommit")] if align < SEGMENT {` (`:215`).
- Соседние сайты `:250`, `:433`, `:689` используют полный путь `crate::alloc_core::segment_header::SegmentMeta` и не затронуты.
- Манифест: `Cargo.toml:692` `numa-aware = ["alloc-core", "dep:numa-shim"]` (doc `:688–691`: «Requires `alloc-core`»); `:170` `alloc-xthread = ["alloc-core"]`.

**Наблюдено (CI, только чтение).**
- Плановый прогон `37315754670` (SHA `1175519…`, 2026-10-05), job `111781954361` («cargo-hack feature-powerset»), комбинация 205/390 `cargo check --no-default-features --features numa-aware`:
  ```text
  error[E0433]: cannot find type `SegmentMeta` in this scope
     --> src/alloc_core/large/alloc_core_large.rs:691:9
  691 |         SegmentMeta::new(base).set_node_id(my_node);
  error: could not compile `sefer-alloc` (lib) due to 1 previous error
  ```
- На `1175519` код этих строк идентичен базе. `git diff 1175519..453439c1` по файлу сдвигает сайт на две строки: `:691` → `:693`.
- Предыдущий плановый прогон `36423048188` (`90e7855`, 2026-09-28) прошёл все 390 комбинаций, в том числе `numa-aware` (#206).

**Происхождение.** `git log -L15,16:src/alloc_core/large/alloc_core_large.rs` указывает на `4ac8376e` (2026-09-29, «fix: enforce Large phase credit through reclaim and cache»). Коммит сузил гейт импорта с `any(numa-aware, alloc-decommit, alloc-xthread)` до `alloc-xthread`, а два `numa-aware`-сайта не тронул.

**SOURCE-CONFIRMED на базе.** Импорт по-прежнему загейтован `alloc-xthread`, а `numa-aware` эту фичу не подразумевает. Под `numa-aware + alloc-decommit` без `alloc-xthread` к ошибке на `:693` добавится такая же на `:481`. Другие отсутствующие при `numa-aware` идентификаторы я проверил чтением: `numa`, `Segment`, `LargeReservationState`, константы `large-reserved-capacity` (`:7–28`, `:50`, `:97`, `:546–653`). Их гейты согласованы.

**Достижимость.**
- Сломана любая сборка с `numa-aware` (или `numa-aware-mock`) без `alloc-xthread`/`alloc-global`: `features = ["numa-aware"]`, `["alloc-decommit", "numa-aware"]`, `["numa-aware", "internals"]`. Последняя — ровно комбинация `[T]`-пункта 107. Манифест объявляет эту конфигурацию валидной («standalone `AllocCore` + NUMA»), а `AllocCore` — стабильный тип крейт-корня (`src/lib.rs:511–512`).
- `production + numa-aware` не затронута: `alloc-global` подразумевает `alloc-xthread`. Это и строка `ci.yml:2041–2043`, и форма из `docs/INTEGRATION.md:36`.
- Ни одна per-PR строка не собирает `numa-aware` без `production`: в `scripts/check-matrix.mjs` `PER_PR_ROWS` NUMA есть только в `production hardened numa-aware internals bench-internals` (`:237`). Поэтому push-CI на базе зелёный (`37785720332`), и дефект видит только еженедельный job.

**Новое к correctness item 107.** Шести ошибок из карточки 107 в исходнике больше нет:
- импорты `src/alloc_core/small/alloc_core_small_reclaim.rs:3–14` загейтованы `all(alloc-global, alloc-xthread)`;
- `mark_magazine`/`clear_magazine` загейтованы `fastbin` (`src/alloc_core/segment/bitmap/magazine_bitmap.rs:133,144`).

Но та же комбинация `numa-aware internals` теперь даёт жёсткую ошибку компиляции. Current-number карточки устарел.

**Рекомендация.**
- Загейтовать импорт `#[cfg(any(feature = "alloc-xthread", feature = "numa-aware"))]` или перевести `:481` и `:693` на полный путь, как сделано на `:250/:433/:689`.
- Добавить в `PER_PR_ROWS` дешёвую строку `check` (только lib): `--no-default-features --features "alloc-decommit numa-aware internals"`.

**Oracle.**
- Красные до исправления, зелёные после: `cargo check --no-default-features --features numa-aware` (E0433 на `:693`) и `--features "alloc-decommit numa-aware"` (E0433 на `:481` и `:693`).
- Ручной `workflow_dispatch` job `feature-powerset` проходит позицию 205.

### R17-API-02 — еженедельный powerset красный с 2026-10-05 и после первой ошибки не проверяет 185 из 390 комбинаций; структурные пробелы матрицы и MSRV

**Места.**
- `.github/workflows/ci.yml:4084–4095`: `cargo hack check --feature-powerset --depth 2 --no-dev-deps`; только `ubuntu-latest`; `if: schedule || workflow_dispatch`; без `--keep-going`; без `-D warnings`.
- `:15`: cron `0 6 * * 1`.
- `:2587–2618`: `msrv` — 1.93, `cargo check --all-features` и `cargo test --no-run --all-features`.
- `:2772–2786`: `msrv-runtime-windows` — 1.93, `--features production`.

**Наблюдено (CI).**
- Лог job `111781954361` обрывается на ошибке позиции 205/390, затем `##[error]Process completed with exit code 1`. Позиции 206–390 и весь шаг aligned-vmem (14 комбинаций) не выполнены.
- Порядок cargo-hack сверен с зелёным логом `108930215742` (там ещё была фича `class-aware-dirty`, поэтому `numa-aware` стоит на 206; последняя позиция — `production,virgin-zero-skip`). После `numa-aware` идут комбинации, старшая по алфавиту фича которых — одна из `numa-aware`, `numa-aware-mock`, `page-map-diag`, `pinning`, `primordial-lazy-commit`, `production`, `small-segment-lazy-commit`, `std`, `virgin-zero-skip`. Все они не проверены, кроме пары `small-segment-lazy-commit,virgin-zero-skip`: cargo-hack ставит её второй.
- Последний полный проход — `90e7855` (2026-09-28). С тех пор `src/` менял 101 коммит; 17 из них — уже после частичного прохода `1175519`, в том числе новые `alloc-stats`/`numa-aware`-гейты `5ee118d2` в `find_segment.rs:214–218,460–461,484–486`.
- Красный итог не попал ни в индексы, ни в CHANGELOG, ни в манифесты: поиск `37315754670|111781954361|E0433` по `docs/` и `CHANGELOG.md` даёт только упоминания других E0433. Это аналог класса R22-3. Правило CLAUDE.md «confirm CI went green» привязано к push и плановые jobs не покрывает. Следующий плановый запуск — 2026-10-12.

**Структурные пробелы (SOURCE-CONFIRMED).**
1. **Fail-fast.** Одна ошибка скрывает остаток недели.
2. **Нет `-D warnings`.** В зелёном прогоне `108930215742` — 180 строк предупреждений (`unused_mut`, unused import, dead method), в красном до позиции 205 — те же классы. Это пункт 161; новые подробности — в приложении D.
3. **Глубина 2 и один Linux-хост.** Тройки и Windows/macOS-специфичные сочетания не видны.
4. **MSRV.** `msrv` компилирует только `--all-features`, `msrv-runtime-windows` — только `production`. Ветки `cfg(not(feature = X))` для X из состава `production` на 1.93 не компилирует никто. Среди них — документированная форма `["alloc-global"]` (`docs/INTEGRATION.md:34`). Нарушений MSRV в таких ветках чтением не найдено (§5), так что это пробел покрытия, не дефект. Пункт 19 — другая ось (check против test) и здесь не повторяется.

**Рекомендация.**
- Запускать `cargo hack … --keep-going`.
- При красном плановом job заводить карточку в `docs/CORRECTNESS_OPEN_ITEMS.md` в тот же день, либо дать job-у явный канал уведомления.
- Добавить per-PR строку из R17-API-01.
- По желанию — точечная строка `msrv`: `--no-default-features --features alloc-global`.

**Oracle.** `workflow_dispatch` до исправления R17-API-01: с `--keep-going` лог содержит 390/390 попыток и ровно перечисленные ошибки; без флага — останов на 205.

### R17-API-03 — граница `internals` (R34-3) протекает для не-`dbg_` членов типов крейт-корня; сканер хуков их не видит

**Нарушаемый принцип.** `Cargo.toml:417–460` и `src/lib.rs:419–432`: «`#[doc(hidden)]` alone is NOT a real API boundary». `production` должен давать «intended stable surface only». R34-3 закрывает `internals`-ом пути модулей и `dbg_*`-методы реэкспортированных типов, но не прочие `#[doc(hidden)] pub` члены этих же типов.

**Места.** Все перечисленные элементы не имеют гейта `internals` и достижимы в `production` или `experimental`.
- **`AllocCore`** (крейт-корень, `src/lib.rs:511–512`; модуль `small/mod.rs:22` и impl-блок `alloc_core_small_magazine.rs:19` без гейта):
  - `refill_class` (`src/alloc_core/small/alloc_core_small_magazine.rs:37–39`) — внутри крейта вызывающих нет, только тесты;
  - `refill_class_bump` (`:112–114`);
  - `refill_class_bump_virgin` (`:128–136`, под `alloc-xthread+fastbin+virgin-zero-skip`);
  - `pub unsafe fn flush_class` (`:440–443`).
- **`SegmentLayout`** (крейт-корень):
  - `small_decommit_start`/`primordial_decommit_start`/`small_lazy_initial_commit`/`primordial_lazy_initial_commit` (`src/alloc_core/segment/segment_layout.rs:183–236`). Их собственный doc: «Test-only public surface (`#[doc(hidden)]` convention — see `lib.rs`); not stable public API». Вызовов через `SegmentLayout::` в `src/` нет.
  - `PRIMORDIAL_PAYLOAD_START_WORD`/`SMALL_PAYLOAD_START_WORD` (`:23–34`) даже не скрыты. Это документированные стабильные константы внутреннего протокола скана sidecar-битмапа (добавлены `18d76472`, 2026-10-05). Единственный потребитель — `tests/r9_bounded_background_maintenance.rs`, и он сам требует `production+internals+bench-internals` (`:1–5`).
- **`ShardedRegion`** (крейт-корень под `experimental`), `_reset_my_shard_binding_for_tests()` (`src/concurrent/sharded/sharded_region.rs:710–721`) — безопасный мутатор без гейта:
  - он чистит TLS-кэш привязок для всех регионов, но, по собственному doc (`:713–716`), эксклюзивный токен не освобождает;
  - следующий `insert` снова попадает в `claim_or_get_shard` (`:474–526`) и CAS-ом забирает ещё один свободный шард (`:497–510`);
  - повторяя «reset + insert», один поток держит все эксклюзивные токены региона до своего выхода, а остальные потоки деградируют в modulo-sharing (`:513–520`);
  - это ломает документированный протокол 7b «один поток — один шард», на котором стоит не устаревший `PinnedRunner` (`src/concurrent/pinning.rs:1–30`).

  Для того же класса — нарушение инварианта из safe-кода — xxs R5-02 уже загейтовал `EpochRegion::_set_slot_generation_for_tests` на `internals` (`src/concurrent/epoch/epoch_region.rs:635–641`). Без гейта остаются и наблюдатели `_remote_free_queue_buffer_identity_for_tests` (`epoch_region.rs:655–656`, `sharded_region.rs:727–728`); они выставляют адреса внутренних буферов.

**Слепые зоны сканера (SOURCE-CONFIRMED).** Регулярное выражение токенизатора — `scripts/verify-dbg-hook-safety.mjs:375`: `\bpub\s+(unsafe\s+)?fn\s+(dbg_\w+)\b`.
1. Оно не видит 47 уникальных публичных функций `*_for_test(s)` (подсчёт — приложение A), включая перечисленные выше.
2. Оно не видит `pub const fn dbg_*`: `HeapCore::dbg_promotion_compiled` (`src/registry/heap_core/diag/diag_probes.rs:51`) и `dbg_num_chunks` (`src/registry/bootstrap/ensure.rs:212`). Сегодня оба — чистые наблюдатели. Но с Rust 1.83 `const fn` может мутировать через `&mut`, поэтому слепая зона не сводится к чистым функциям.

Класс уже поднимался: TRACKED item 9, P2-12 «name-prefix match cannot see». Тогда его закрыли переименованием одного метода, а сканер не расширили. Новое здесь — текущий объём и конкретные незагейтованные члены типов крейт-корня.

**Выведено.** Дефекта memory-safety нет:
- выход `class_idx` за границы в `refill_class*` паникует в bounds-checked `SizeClasses::block_size` (`src/alloc_core/platform/size_classes.rs:327–335`) раньше сырого доступа (`alloc_small` вызывает его первым, `alloc_core_small_impl.rs:163–164`);
- `flush_class` — `unsafe fn` с `# Safety`.

P4: семвер-поверхность `production` шире заявленной, а одна safe-функция `experimental` позволяет нарушить документированный протокол.

**Рекомендация.**
- Методы, которые нужны `HeapCore` (`refill_class_bump`, `refill_class_bump_virgin`, `flush_class`; их используют `src/registry/heap_core/{alloc/hot.rs:586,770; alloc/batch.rs:162; free/*; state/tcache_flush.rs:105}`), сделать `pub(crate)`, а для тестов добавить `#[cfg(feature = "internals")] pub fn dbg_*`-форвардеры — их увидит сканер.
- `refill_class`, тестовые функции и две константы `SegmentLayout` перевести под `#[cfg(feature = "internals")]`.
- `_reset_my_shard_binding_for_tests` и оба `_remote_free_queue_buffer_identity_for_tests` — под `internals`. Затронутые тесты (`tests/sharded.rs`, `tests/r11_p4_3_sharded_per_region_binding.rs`, `tests/r13_sharded_dead_region_retention.rs`, `tests/regression_r2_21_epoch_drain_buffer_reuse.rs`) добавят `internals`.
- Сканер: распознавать `pub const fn` и сканировать `#[doc(hidden)] pub fn` любых имён, либо ввести единый префикс для тестовых хуков.

**Oracle.**
- Компиляционный негатив по образцу `tests/r14_sidecar_owner_capability_negative.rs`: внешний крейт под `features = ["production"]` и под `["experimental"]` вызывает `AllocCore::refill_class`, `SegmentLayout::small_decommit_start`, `ShardedRegion::_reset_my_shard_binding_for_tests`. Сегодня он компилируется (тест красный), после исправления даёт E0599 (тест зелёный).
- Самотест сканера: фикстуры `pub const fn dbg_x(&mut self)` и `#[doc(hidden)] pub fn reset_for_tests()` должны получать «unreviewed».

### R17-API-04 — `numa-aware` молча выключает ещё три механизма; README предупреждает только об одном

**Места и трассы (SOURCE-CONFIRMED).**
- **(a) `primordial-lazy-commit`** — член `production` (`Cargo.toml:415`) — под `numa-aware` исключён из сборки: `src/alloc_core/alloc_core/bootstrap.rs:101,126–127` (`#[cfg(all(feature = "primordial-lazy-commit", not(feature = "numa-aware")))]`).
  - NUMA-причины для этого нет: первичный сегмент по узлам не распределяется (`:78–80`).
  - Обоснование в `:81–91` — симметрия с общим doc `alloc-lazy-commit` (`Cargo.toml:721–727`) и сохранение живого вызова `Segment::reserve` под `--all-features`, чтобы не сработал lint `dead_code`.
  - Итог: документированная форма `["production", "numa-aware"]` (`docs/INTEGRATION.md:36`) теряет выигрыш R12-9. По собственному утверждению манифеста это «~5.1x smaller first-heap commit» (`Cargo.toml:768–771`); эффект есть только на Windows (`bootstrap.rs:51–53`).
- **(b) `large-reserved-capacity`.**
  - Под `numa-aware` `reserved_capacity = usable` (`src/alloc_core/large/alloc_core_large.rs:652–653`), а константы роста загейтованы `not(numa-aware)` (`:50`, `:97`).
  - Поэтому `try_grow_large_reserved_capacity` всегда возвращает `false` (`src/alloc_core/alloc_core/mem/realloc_fastpath.rs:512–518`), и фича превращается в no-op.
  - Doc фичи (`Cargo.toml:343–387`) об этом не говорит. Тест `tests/large_reserved_capacity.rs` сам себя исключает `not(numa-aware)` (пункт 76) — авторы это знают, пользователь нет.
- **(c) OOM-rescue R9-8.**
  - `find_segment_with_free_forced` (`src/alloc_core/small/alloc_core_small/find_segment.rs:111–114`; вызовы — `alloc_core_small_impl.rs:246,344`) исключён под `numa-aware` с `5a4ba626`. Тогдашнее обоснование: «under numa-aware the directory is never trusted for lookups».
  - С R11-6 (`89865ae4`) directory-поиск под NUMA активен (`find_segment.rs:269`), и его промах доверяется для неroute-ядер: `trust_negative = !self.table.is_routed()`, а без `alloc-global+alloc-xthread` — константа `true` (`:398–401`).
  - Исключение rescue закреплено тестом `tests/segment_directory_numa.rs:324` с пометкой «same as the pre-R11-6 status quo».
  - Комментарий `find_segment.rs:208–211` до сих пор утверждает, что directory-блок под NUMA «compiled out entirely». Это неверно.

README (`README.md:1432–1451`) называет ловушкой только `small-segment-lazy-commit`.

**Выведено.** (a) и (b) — детерминированная смена семантики при включении фичи: ожидания пользователя расходятся с поведением, сбоя нет. **ГИПОТЕЗА (c):** вред возможен только для standalone `AllocCore` (неroute) с `alloc-segment-directory` + `numa-aware`, и только если устаревший отрицательный бит директории совпадёт с отказом ОС в резерве — тогда вместо свободного блока вернётся null. Гипотеза ложна, если удастся показать, что под NUMA отрицательный бит устареть не может.

**Рекомендация.**
- (a) Снять `not(numa-aware)` с гейта первичного lazy-commit, а `dead_code` у `Segment::reserve` подавить точечным `cfg_attr`. Иначе — явно задокументировать потерю в INTEGRATION и README.
- (b) Описать no-op в doc фичи и в README-ловушке.
- (c) Либо включить rescue под NUMA (он и так сканирует все бакеты), либо переписать обоснование и комментарий `:208–211`.

**Oracle.**
- (a) Windows, форма `tests/lazy_initial_commit_forced_page.rs` под `production numa-aware`: committed-байты первичного сегмента сейчас = `SEGMENT`, после исправления = `Layout::lazy_initial_commit(...)`.
- (b) Пин README и doc фичи на упоминание обеих NUMA-ловушек: сейчас красный.
- (c) Инъекция «стёртый бит + отказ резерва» под `alloc-core alloc-segment-directory numa-aware internals bench-internals`: сейчас null, после — блок.

### R17-API-05 — дрейф документации поверхности фич

- **(a) Старый состав `production`.** `docs/INTEGRATION.md:23–26`: «`production` is an alias for `alloc-global + alloc-xthread + alloc-decommit + fastbin`» — без `alloc-segment-directory` и `primordial-lazy-commit` (`Cargo.toml:415`).
  - Rustdoc `SeferAlloc` отправляет читателя именно сюда за «full feature matrix» (`src/global/sefer_alloc/core.rs:83–86`).
  - Страж `tests/no_stale_doc_references.rs:852–911` проверяет только `src/lib.rs`, `src/global/sefer_alloc/core.rs` и `README.md` (`:877`).
  - Класс уже фиксировался (аудит 2026-08-04, F-10). Новое — остаточный сайт вне стража, на который ссылается публичный rustdoc.
- **(b) README-матрица.** `README.md:1415–1430`:
  - строка `alloc-stats` даёт «Pulls in —», хотя манифест — `alloc-stats = ["alloc-core"]` (`Cargo.toml:524`), и doc фичи объясняет зачем;
  - матрица перечисляет 13 из 30 фич;
  - `internals` в README не упомянут вовсе (0 вхождений вне `bench-internals`). А это переключатель, который открывает `sefer_alloc::{alloc_core, global, registry}::*` и 30 `pub unsafe fn dbg_*`.
- **(c) `Cargo.toml`.** 27 строк комментариев (14 из них в `[features]`) ссылаются на несуществующие файлы:
  - `src/global/sefer_alloc.rs` как место «four release-surviving invariant tripwires» (`:180`);
  - `src/registry/bootstrap.rs` (`:185`);
  - `src/alloc_core/remote_free_ring.rs` как место const-assert `SMALL_CLASS_COUNT <= 62` (`:652`) — `RemoteFreeRing` в `src/` уже нет;
  - `alloc_core_small_pool.rs`, `heap_core_diag.rs`, `alloc_core_core_diag.rs`, `src/alloc_core/size_classes.rs`.

  Кроме того, `:304` говорит о «the 1024 cap», а `MAX_SEGMENTS = 4096` (`src/alloc_core/segment/segment_table/segment_table_impl.rs:78`). Это долг класса item 154, но в манифесте — главном источнике сведений о фичах для пользователя — и вне `src/`-области пункта 154.
- **(d) Значения констант `SegmentLayout` зависят от фич, и это не оговорено.**
  - `SMALL_MAX`, `SIZE_CLASS_TABLE`, `SIZE2CLASS` (`segment_layout.rs:62–92`) меняются от `medium-classes(-wide)` (`src/alloc_core/platform/size_classes.rs:21–39`).
  - `SMALL_META_END` и `*_PAYLOAD_START_WORD` меняются от `hardened` (`src/alloc_core/segment/segment_header/segment_header_layout.rs:81–86`).
  - При унификации фич Cargo одна зависимость, включившая `medium-classes`, меняет эти `const` для всех крейтов графа. Например, у потребителя сломается `const _: () = assert!(SegmentLayout::SMALL_MAX < …)`.
- **(e) docs.rs и `doc(cfg)`.** docs.rs рендерит только `production` (`Cargo.toml:37–38`), а аннотаций `doc(cfg)` в крейте нет (поиск `docsrs|doc(cfg` по `src/` пуст). В итоге:
  - публичный слой `experimental`/`pinning`/`batch-api` — включая не устаревший `PinnedRunner` — на docs.rs отсутствует;
  - у отрендеренных элементов нет пометок о требуемых фичах; например, `SeferAlloc::with_config` требует `alloc-decommit` (`core.rs:275`).

**Рекомендация.**
- Добавить `docs/INTEGRATION.md` в список стража и исправить файл.
- В README поправить строку `alloc-stats` и добавить строку `internals`.
- Переписать комментарии `Cargo.toml` на ссылки по символам.
- Оговорить в doc констант `SegmentLayout`, что значения зависят от фич.
- Добавить `doc(cfg)` (через `--cfg docsrs`).

**Oracle.**
- Страж с `docs/INTEGRATION.md` в списке: сейчас красный, после исправления — зелёный.
- Тест «колонка Pulls in в README против `Cargo.toml`»: сейчас красный на строке `alloc-stats`.

### R17-API-06 — контракт no-panic/abort `SeferAlloc` расходится с кодом; в debug спинлок fallback может паниковать внутри `GlobalAlloc`

**(a, b) Текст против кода.** `src/global/sefer_alloc/mod.rs:188–191`: «The one deliberate process kill on the alloc path is a direct `std::process::abort()` (registry chunk-materialisation OOM, `registry/bootstrap/registry.rs`)». Расхождения:
- `src/registry/bootstrap/registry.rs:245–248` теперь сам называет этот `abort()` (`:271`) «an invariant tripwire»: claim и free идут через fallible `try_ensure_chunk`.
- В 23 файлах, чей код лежит на путях `GlobalAlloc`, — 99 не-комментарных строк `std::process::abort()`. Крупнейшие: `segment_table/route_slots.rs` 22, `segment_route/directory.rs` 15, `heap_registry/claim.rs` 7, `segment_table_impl.rs` 7, `alloc_core/sidecar_drain.rs` 7, `active_kind_index.rs` 6, `alloc_core_large.rs` 5. Подсчёт выполнен по файлам, без посайтового анализа достижимости (приложение A); `maintenance_service.rs` и прототип `exact_object` не учтены.
- `9df9f6b8` (R16-01) ненадолго перечислил второй класс («a missing magazine root…»), а `b707d196` (perf 84) вернул формулировку «The one». Контракт описывает только паники. Читатель не узнает, что нарушение внутренних инвариантов на путях terminal-sidecar/route заканчивается `abort()`, а не panic.

**(c) Debug-only переполнение.** `LockGuard::acquire` (`src/global/fallback.rs:469–486`) делает `spins += 1` для `u32` без насыщения (`:475`).
- Путь: `GlobalAlloc::{alloc, alloc_zeroed, realloc}` → `SeferAlloc::with_fallback_heap` (`core.rs:354–366`) → `fallback::with_heap(_config)` (`fallback.rs:336–351`, `:362–370`) → `acquire`.
- После 2³² неудачных CAS debug-сборка паникует с «attempt to add with overflow» внутри `GlobalAlloc`. Это противоречит контракту «no path … debug or release … panics» (`mod.rs:179–183`).

**Исполненное свидетельство (c).**
- Run `37315754670`, job `111781952753` (`1175519`): тесты `r7_p3_fallback_config_default` и `r7_p3_fallback_config_zero` зависли («has been running for over 60 seconds»), затем:
  ```text
  panicked at src/global/fallback.rs:455:13:
  attempt to add with overflow
  thread panicked while processing panic. aborting.
  ```
  `:455` на `1175519` — это `spins += 1;`.
- Тот же job был красным и на push-прогоне того же SHA (`37281933773`). На базе push-CI зелёный, то есть причина зависания на базе не воспроизводится, а путь паники остался.
- Соседние циклы счётчик насыщают: ожидание инициализации `fallback.rs:310–313` (исправлено R12-04, `c4725071`) и `ShardLock::lock` (`src/registry/segment_route/shard_lock.rs:55–57`).

**Рекомендация.**
- (a, b) Переписать абзац: «invariant tripwires on terminal-sidecar/route paths are `std::process::abort()`; inventory by grep». В `tests/no_panic_doc_accuracy.rs` (сейчас он пинит только паники) добавить пофайловую перепись abort-сайтов.
- (c) Вынести общий `backoff(&mut u32)` с насыщением для всех трёх циклов. По желанию — debug-детектор самоблокировки (поток-владелец) с явным `abort` и понятным сообщением вместо безымянного переполнения.

**Oracle.**
- (c) Юнит-тест `backoff` при `spins = u32::MAX` в debug: текущая встроенная форма, перенесённая дословно, паникует; после исправления паники нет.
- (a) Пин текста и переписи.

### R17-API-07 — `experimental` использует `AtomicU64` без гейта `target_has_atomic = "64"`

**Места.**
- `std::sync::atomic::AtomicU64` используется безусловно: `src/concurrent/epoch/epoch_region.rs:68,87`, `src/concurrent/lock_free/lock_free_region.rs:18,36`, `src/concurrent/sharded/sharded_region.rs:137,181,303`.
- Гейт R2-17 (`src/lib.rs:391–401`) действует только при `alloc-core`.
- Таблица целей README (`README.md:419–424`) описывает «Region-only» и «Allocator», а `experimental` не описывает.
- Компаньоны свои счётчики гейтуют: `crates/sefer-region/src/lib.rs:58–64`, `crates/once-ptr-cell/src/lib.rs:310–316`.

**Наблюдено и выведено.** По документации std тип `AtomicU64` доступен только при `target_has_atomic = "64"`. **ГИПОТЕЗА** (кросс-сборка не выполнялась): на std-целях без 64-битных атомиков — например, 32-битные PowerPC/MIPS/RISC-V Linux — `--features experimental` упадёт с E0432 без внятного сообщения. Затронут и `pinning`, потому что он подразумевает `experimental`.

**Рекомендация.** Добавить `#[cfg(all(feature = "experimental", not(target_has_atomic = "64")))] compile_error!(…)` с объяснением; другой вариант — `AtomicUsize` с задокументированным горизонтом переполнения. Добавить строку `experimental` в таблицу целей.

**Oracle.** `cargo check --target <32-битная std-цель без AtomicU64> --features experimental`: до исправления E0432, после — сообщение `compile_error!`.

### R17-API-08 — ГИПОТЕЗА: `#[global_allocator] SeferAlloc` требует нативного TLS, и это не задокументировано

**Места.**
- `src/global/tls_heap.rs:127–160`: `LOCAL` и `GUARD` — `thread_local!` с `const`-инициализацией.
- `:256–281`: `current_for_alloc` — первое обращение потока к `LOCAL`.
- Модульный doc `:51–71` и `:460–462` рассуждает об «os-keyed platforms», но только про teardown.
- aligned-vmem пропускает Unix-цели `linux`, `android`, `macos`, `ios`, `tvos`, `watchos`, `freebsd`, `netbsd`, `openbsd`, `dragonfly` (`crates/aligned-vmem/src/os/unix.rs:704–718`).

**ГИПОТЕЗА.**
- Там, где std реализует `thread_local!` через OS-ключи (цели без `#[thread_local]`), ячейка каждого `thread_local!` на поток выделяется `Box`-ом через глобальный аллокатор при первом обращении.
- Тогда первое `LOCAL.try_with` в новом потоке войдёт в `SeferAlloc::alloc` → снова `LOCAL.try_with` (ключ ещё пуст) → снова `Box` → неограниченная рекурсия и переполнение стека.

**Условие ложности.** Гипотеза ложна, если для каждой 64-битной цели из списка aligned-vmem `rustc --print cfg --target <t>` содержит `target_thread_local`, либо если std выделяет такие ячейки через `System`. Не проверено: в этом режиме rustc не вызывается, BSD-хостов нет. README сам не заявляет поддержку этих ОС («other 64-bit … not CI-tested», `README.md:423`), поэтому до подтверждения — P4.

**Рекомендация.** Явно записать требование «нужен нативный TLS» в README «Target support» и в rustdoc `SeferAlloc`. При подтверждении — `compile_error!` для `alloc-global` по allowlist `target_os`: `cfg(target_thread_local)` нестабилен.

**Oracle.** CI-шаг, который читает `rustc --print cfg` по списку целей (только конфигурация компилятора), или запуск `tests/global_alloc_installed.rs` на такой цели.

### R17-API-09 — `PinnedRunner` выставляет `core_affinity::CoreId` без реэкспорта

**Места.**
- `src/concurrent/pinning.rs:67` (`use core_affinity::CoreId;`), `:160` `pub fn pin_current_thread_to_core(core_id: CoreId) -> bool`, `:170` `pub fn available_cores() -> Option<Vec<CoreId>>`.
- `Cargo.toml:905`: `core_affinity = { version = "0.8", optional = true }`.
- `src/lib.rs:496–497` реэкспортирует только `PinnedRunner`, README `CoreId` не упоминает.
- `pinning` не устаревший (`src/concurrent/mod.rs:13–15`).

**Выведено.** Пользователь вынужден сам зависеть ровно от `core_affinity 0.8`, а смена мажора `core_affinity` становится ломающим изменением для `sefer-alloc`. Для контраста: `InvalidAlign` из `size-classes` реэкспортирован осознанно (`src/lib.rs:513–518`).

**Рекомендация.** `pub use core_affinity::CoreId;` под `pinning` с упоминанием в README — или собственный newtype.

**Oracle.** Downstream-фикстура, которая использует `sefer_alloc::CoreId` без прямой зависимости `core_affinity`: до исправления E0432, после — компилируется.

### Гипотезы оптимизации в теме (не измерены)

Ни одна из этих гипотез не измерена. Это не speedup и не RSS-результат; любой судья обязан соблюдать правила R26-4/R30-8/entry-point/regime из CLAUDE.md.

- **O-1. Линия 128 байт.** `HeapSlot` и `HeapSlotRemote` объявлены `#[repr(C, align(64))]` (`src/registry/heap_slot.rs:103,150`). Изоляция PERF-PASS-4 «remote counters на своей линии» (`:135–149`) рассчитана на 64-байтную линию. На 128-байтных линиях (Apple M, POWER; на x86 смежный prefetcher спаривает линии) она не выполняется. Под `alloc-stats`, где счётчики пишутся на hot-path, возможен false sharing. Oracle: HITM/`perf c2c` на aarch64-darwin, A/B `align(128)`, путь активирован при включённом `alloc-stats`.
- **O-2. Lazy-commit первичного сегмента под NUMA** — см. R17-API-04 (a). Эффект только для RSS на Windows; oracle — committed-байты первичного сегмента при одинаковой нагрузке.

## 3. Платформа/комбинация → риск → покрытие CI

| Платформа / комбинация | Риск (в теме) | Чем покрыта в CI |
|---|---|---|
| x86_64-linux-gnu, `production` и per-PR строки | низкий | clippy ×6, test-core/xthread/hardened/gated-bodies/feature-isolation, miri/loom/tsan/asan; powerset — еженедельно, сейчас красный |
| x86_64-windows-msvc, `production` | низкий | test-windows (включая симуляцию страниц 16/64 KiB), msrv-runtime-windows на 1.93 |
| aarch64-apple-darwin (страница 16 KiB, линия 128 B) | низкий для `production`; O-1 | test-macos: `production internals` и две узкие строки |
| aarch64-linux-gnu (cross/qemu, страница 4 KiB) | средний: ядро с 64 KiB-страницами не исполняется | multi-arch: `experimental`, `alloc-global(+decommit)`, `production` |
| Linux/arm64 со страницами 64 KiB | средний (пункты 73/74) | только симуляция forced-page на Windows |
| `numa-aware` без `alloc-xthread` | **сломано (R17-API-01)** | только еженедельный powerset — красный и не записан |
| `production + numa-aware` | семантика (R17-API-04) | per-PR `check` без тестов; numa-real-kernel — еженедельно |
| `alloc-global` без `fastbin`/`alloc-decommit` (форма из INTEGRATION) | предупреждения (пункт 161); MSRV-ветки не проверены | строки test-xthread, multi-arch; MSRV — нет |
| `exact-object-proto`, `large-cache-extended`, `medium-classes-wide` по отдельности | семантика; компиляция по одной — только powerset | явных строк нет; только `--all-features` и powerset |
| `--no-default-features` (Region, no_std) | низкий | job no_std, thumbv7em |
| 32-битные цели, аллокатор | отклонён `compile_error!` (`lib.rs:391`) | `tests/regression_r2_17_64bit_target_gate.rs` пропускается без 32-битного std |
| 32-битные std-цели без `AtomicU64`, `experimental` | E0432 (R17-API-07) | нет |
| 64-битные BSD (FreeBSD/NetBSD/OpenBSD/DragonFly) | гипотеза TLS (R17-API-08); константы (пункты 43/60) | для корневого крейта — нет |
| Android, iOS/tvOS/watchOS | не установлено | нет |
| big-endian 64-бит (s390x, ppc64, sparc64) | чтение не нашло endian-зависимостей (§5) | нет |
| wasm32/wasm64 | аллокатор отклонён (R2-17 или `compile_error!` в aligned-vmem `unsupported.rs`) | нет |
| MSRV 1.93: `--all-features`, `production` (Windows) | низкий | msrv, msrv-runtime-windows |
| MSRV 1.93: ветки `cfg(not(<фича production>))` | не установлено | нет — R17-API-02, пункт 4 |
| docs.rs (`production`) и `--all-features` rustdoc | низкий | job docs с `-D warnings` для обоих наборов |

## 4. Вне темы

- `fuzz targets run` (run `37315754670`, job `111781954504`, `1175519`): цель `heap_core_ops`, LeakSanitizer «128 byte(s) leaked in 1 allocation(s)», crash-файл `crash-da39a3ee…` (SHA-1 пустого входа). В индексах не записано, не исследовалось.
- В том же плановом и в push-прогоне `37281933773` на `1175519` красным был `test (gated bodies + all-features)`: `r11_ph4c_ingress_consume_exactly_once_oracle`, `r6_fix_p3_numa_unknown_bucket` (смежно с пунктом 158) и зависания `r7_p3_*`. На базе push-CI зелёный. В R17-API-06 (c) использован только факт переполнения.
- `src/global/fallback.rs:322,481` — невозможные ветки `cfg(not(feature = "std"))` (`global` требует `std`); doc `:465` («keeps this fn buildable without std») неверен. Плюс 118 + 24 тавтологических атрибутных строк в `src/registry`/`src/global`. Класс пункта 154.
- `MaintenanceStartError` (`src/global/maintenance_start_error.rs:16–33`) печатает в `Display` вложенную ошибку и одновременно возвращает её из `source()` — дублирование в цепочках ошибок (рекомендация API guidelines). Мелочь.

## 5. Что проверено и не дало находок

- **Публичные enum.** Нет вариантов, зависящих от фич. `LargeCacheMode`, `SmallPoolPolicy`, `LargeCachePolicy`, `MaintenanceStartError` — `#[non_exhaustive]`. `AllocStats` — `#[non_exhaustive]` с одинаковым набором полей при любых фичах (`src/global/alloc_stats.rs:29–36,54–56`).
- **Сигнатуры.** Ни у одной публичной функции сигнатура не меняется от фич. Параметры с cfg есть только у приватных функций: `find_segment.rs:197,627–629`, `alloc_core_small_magazine.rs:167`, `fallback.rs:145`.
- **Send/Sync.** Для публичных типов нет `unsafe impl Send/Sync`. `AtomicSlot` — с условием `T: Send+Sync` (`src/concurrent/epoch/hand.rs:572,582`). `SeferAlloc` — ZST или `Copy`-конфиг. `AllocCore` документированно `!Send/!Sync`.
- **`GlobalAlloc` impl** (`src/global/sefer_alloc/global_alloc.rs`): null-`dealloc` — no-op, null-`realloc` → null, ветка `exact-object-proto` и единая политика `inline(always)` согласованы с контрактом. Safe-API `SeferAlloc` (`stats`, `trim_current_thread`, `start_maintenance`, `maintenance_running`, `fallback_config_conflicts`) не позволяет нарушить инварианты из safe-кода в прочитанных путях.
- **Safe-API `AllocCore`.** `alloc`/`alloc_zeroed` округляют ZST до `MIN_BLOCK`; огромный `align` уходит в null через `checked_add`. `dealloc`/`realloc`/`flush_class` — `unsafe fn` с `# Safety`.
- **Endianness.** Нет `to/from_*_bytes` и `transmute`. `SegmentKind` — `#[repr(u8)]` (`segment_header_impl.rs:166–167`), поэтому байтовое чтение `kind` нейтрально к порядку байт. Остальные поля читаются по `offset_of!` той же ширины.
- **Сдвиги.** `1u64 << class` защищён const-assert `SMALL_CLASS_COUNT <= 64` (`src/alloc_core/platform/size_classes.rs:213–217`, исправление R16-03). `(1u32 << n)` (`alloc_core_small_magazine.rs:265,289`) выполняется только на virgin-пути, где `n ≤ out.len() ≤ 16` защищено assert (`:142–146`). Касты смещений сегмента (4 MiB) в `u32` безопасны.
- **MSRV 1.93.** Из новых API найдены `is_multiple_of` (1.87), `is_none_or` (1.82), `map_addr` (1.84), `div_ceil` (1.73), inline `const` (1.79), `offset_of!` (1.77) — все не новее 1.93. Edition 2021, let-chains нет.
- **Имена фич.** 26 имён в `src/` — все объявлены. `production`, `numa-aware-mock`, `alloc-lazy-commit`, `default` в `src/` не упоминаются, как и задумано (алиасы и бандл).
- **Взаимоисключения.** Есть только два страхующих `compile_error!` (`src/lib.rs:342–362`); через Cargo они недостижимы. Unsafe-гейт `forbid`/`deny` (`:318–322`) покрывает все фичи, приносящие unsafe.
- **Размер страницы.** `SegmentLayout::PAGE` задокументирован как гранулярность PageMap. Сайты commit/decommit используют runtime `page_size()` или `MAX_REALISTIC_PAGE_SIZE`. Остаток — пункты 73/74, без изменений.
- **ОС-специфика** в `src/` отсутствует; `Instant::now()` на пути аллокации на целевых ОС не аллоцирует; `std::thread::current()` на пути аллокации не используется.
- **docs.rs.** Собирается только цель по умолчанию; job docs строг и для `--all-features`, и для набора docs.rs. Требование CLAUDE.md «doc-lint в точном наборе docs.rs» выполнено.
- **API-guideline, не заводилось:** у `SeferAlloc`, `AllocCore`, `EpochRegion`, `LockFreeRegion`, `ShardedRegion` нет `Debug`; добавить его позже — не ломающее изменение.
- **Реэкспорт `InvalidAlign`** — осознанное решение (`src/lib.rs:513–518`). Связка `numa-shim` ↔ aligned-vmem закрыта пунктом 46.

## Приложение A. Команды (всё только чтение)

```text
git rev-parse HEAD                                   -> 453439c123f71a98890408dd0170bd909ad9ecdf
git ls-files src | wc -l ; git ls-files src | xargs cat | wc -l   -> 168 ; 43325
grep -rhoE '#!?\[cfg\(' src | wc -l                  -> 1281
grep -rhoE '#!?\[cfg_attr\(' src | wc -l             -> 61
grep -rhoE '\bcfg!\(' src | wc -l                    -> 20
grep -rhoE 'not\(feature *=' src | wc -l             -> 188
grep -rhoE '#\[doc\(hidden\)\]' src | wc -l          -> 455
grep -rhE '^\s*pub (const )?(unsafe )?fn ' src | wc -l   -> 478
grep -rnE '\bpub\s+(unsafe\s+)?fn\s+dbg_' src | wc -l    -> 246
grep -rnE 'pub const (unsafe )?fn dbg_' src | wc -l      -> 2
grep -rhoE 'pub (const )?(unsafe )?fn [A-Za-z_0-9]+' src | sed -E 's/.* fn //' \
  | grep -E '_for_tests?$|_for_tests?_' | grep -vE '^dbg_' | sort -u | wc -l   -> 47
grep -rnE 'process::abort\(\)' src | grep -vE '^[^:]+:[0-9]+:\s*//' \
  | awk -F: '{print $1}' | sort | uniq -c            -> по файлам (сумма GlobalAlloc-файлов: 99 в 23)
awk '/^\[features\]/{f=1;next} /^\[/{f=0} f && /^[a-z0-9-]+ *=/{c++} END{print c}' Cargo.toml   -> 30
grep -cE 'src/global/sefer_alloc\.rs|src/registry/bootstrap\.rs|remote_free_ring\.rs|heap_core_diag\.rs|alloc_core_small_pool\.rs|src/alloc_core/size_classes\.rs|src/alloc_core/large_cache_extended\.rs|alloc_core_core_diag\.rs|alloc_core_small\.rs|heap_core_free\.rs|alloc_core\.rs`' Cargo.toml   -> 27
  (каждый из этих путей проверен `test -e`: файлов нет)
git log -L15,16:src/alloc_core/large/alloc_core_large.rs ; git show 4ac8376e -- src/alloc_core/large/alloc_core_large.rs
git log --oneline 90e7855..HEAD -- src | wc -l       -> 101
git log --oneline 1175519..HEAD -- src | wc -l       -> 17
git show b707d196 -- src/global/sefer_alloc/mod.rs ; git show 9df9f6b8 -- src/global/sefer_alloc/mod.rs
gh run list --workflow ci.yml --event schedule --limit 8 --json databaseId,headSha,conclusion,createdAt
gh run view 37315754670 --json jobs --jq '.jobs[] | select(.conclusion != "success")'
gh run view --job 111781954361 --log | sed 's/\x1b\[[0-9;]*m//g' | grep ...    (E0433, позиция 205/390)
gh run view --job 108930215742 --log | grep -oE 'on sefer-alloc \([0-9]+/[0-9]+\)' | tail -1   -> (390/390)
gh run view --job 111781952753 --log | grep -nE 'panicked|FAILED'   (overflow fallback.rs:455)
gh run view --job 111781954504 --log | grep -nE 'LeakSanitizer|crash-'
gh run list --workflow ci.yml --limit 4 --json databaseId,headSha,conclusion,event   (push 37785720332 success на базе)
```

Инструмент Grep (ripgrep) использовался для перечня gated-`use`, публичных `*_for_test(s)`, `doc(hidden)`, `AtomicU64`/`target_has_atomic`, `align(64)`, `PAGE`, `thread_local`/`Instant::now`/`thread::current`, а также недавних API std.

## Приложение B. Фичи и импликации (30, `Cargo.toml:120–876`)

```text
default = [std]                std = [sefer-region/std]
experimental = [std, dep:arc-swap, dep:crossbeam-epoch]      pinning = [experimental, dep:core_affinity]
alloc-core = [std, dep:aligned-vmem, aligned-vmem/lazy-commit, dep:once-ptr-cell, dep:size-classes]
alloc-xthread = [alloc-core]   alloc-global = [alloc-core, alloc-xthread]   exact-object-proto = [alloc-global, std]
batch-api = [experimental, alloc-core]   page-map-diag = [alloc-core]   alloc-decommit = [alloc-core]
exact-span-large = [alloc-core]   large-reserved-capacity = [exact-span-large, aligned-vmem/lazy-commit]
large-cache-extended = [alloc-decommit]   fastbin = [alloc-global, alloc-xthread]
production = [alloc-global, alloc-xthread, alloc-decommit, fastbin, alloc-segment-directory, primordial-lazy-commit]
internals = []   alloc-stats = [alloc-core]   bench-internals = [aligned-vmem?/bench-internals, aligned-vmem?/lazy-commit]
medium-classes = [alloc-core]   medium-classes-wide = [medium-classes]   alloc-segment-directory = [alloc-core]
hardened = [fastbin]   numa-aware = [alloc-core, dep:numa-shim]   numa-aware-mock = [numa-aware]
alloc-lazy-commit = [primordial-lazy-commit, small-segment-lazy-commit]
primordial-lazy-commit = [alloc-core, aligned-vmem/lazy-commit]   small-segment-lazy-commit = [alloc-core, aligned-vmem/lazy-commit]
lazy-commit-fault-injection = [alloc-lazy-commit, aligned-vmem/fault-injection]   virgin-zero-skip = [alloc-decommit]
```

Не подразумевают `alloc-xthread`, но открывают код `alloc_core`: `alloc-core`, `alloc-decommit`, `alloc-stats`, `numa-aware(-mock)`, `page-map-diag`, `medium-classes(-wide)`, `exact-span-large`, `large-reserved-capacity`, `large-cache-extended`, `virgin-zero-skip`, `alloc-segment-directory`, `*-lazy-commit`, `batch-api`.

Для большинства из них сборку без `alloc-xthread` проверяет только еженедельный powerset. Per-PR покрыты лишь:
- `alloc-core internals` (`ci.yml:1726`);
- `alloc-core alloc-decommit internals` (`check-matrix.mjs:196`);
- `alloc-core page-map-diag` (`ci.yml:1950`);
- `alloc-core internals lazy-commit-fault-injection` (`check-matrix.mjs:249`);
- miri-строки `alloc-core`, `alloc-core alloc-decommit internals`, `alloc-segment-directory internals` (`ci.yml:2977,2987,3016`).

## Приложение C. CI-свидетельства (`gh`, только чтение)

| Run / job | SHA | Событие | Итог | Использовано |
|---|---|---|---|---|
| `36423048188` / `108930215742` | `90e7855` | schedule 2026-09-28 | success | powerset 390/390, порядок комбинаций, 180 строк предупреждений |
| `37315754670` / `111781954361` | `1175519` | schedule 2026-10-05 | failure | powerset остановился на 205/390: E0433 `alloc_core_large.rs:691` (R17-API-01/02) |
| `37315754670` / `111781952753` | `1175519` | schedule | failure | `fallback.rs:455` «attempt to add with overflow» (R17-API-06 c) |
| `37315754670` / `111781954504` | `1175519` | schedule | failure | fuzz `heap_core_ops` LeakSanitizer (§4) |
| `37281933773` | `1175519` | push | failure | gated-bodies красный и на push |
| `37785720332` | `453439c1` (база) | push | success | per-PR CI зелёный при E0433 в `numa-aware`-конфигурациях |

## Приложение D. Связь с открытыми пунктами индексов

- **Correctness 107** — новое доказательство. Шесть перечисленных ошибок исчезли: `alloc_core_small_reclaim.rs:3–14` загейтован, `magazine_bitmap.rs:133,144` — под `fastbin`. Но комбинация `numa-aware internals` теперь — жёсткая E0433 (R17-API-01). Current-number устарел.
- **Correctness 161** — новое доказательство из логов powerset (без `-D warnings`): `unused_mut` повторяется в десятках комбинаций.
  - Сайты из карточки, на базе: `src/alloc_core/large/alloc_core_large.rs:584` и `src/alloc_core/small/alloc_core_small/reserve.rs:182`.
  - **Третий сайт**, которого в карточке нет: `alloc_core_large.rs:603` (`exact-span-large` без `alloc-decommit`, без `#[allow(unused_mut)]`).
  - По логу `1175519` (номера строк того SHA): unused import в `alloc_core/mod.rs` и неиспользуемые методы в `small/alloc_core_small/directory.rs`, `segment_directory_impl.rs`, `exact_shard.rs`.
- **Hook safety 9 (P2-12)** — класс «name-prefix» жив: 47 имён `*_for_test(s)` и 2 `pub const fn dbg_*` вне сканера (R17-API-03).
- **Misc 154** — смежные примеры вне `src/`: 27 устаревших путей в `Cargo.toml` и невозможные ветки `fallback.rs`. Статус карточки не меняется.
- **CI 19, 95; platform 43/60, 73/74; misc 159** — сверены, нового нет. R17-API-02 (4) — другая ось MSRV, не пункт 19.
- **Предлагаемые новые пункты:** R17-API-01…09; номера и тиры назначит оркестратор. Остальные пункты обоих индексов вне темы этого ревью — LEAVE.
