# r6-u26 — отчёт: R6-T41, R6-T43, R6-T44, R6-T45 (волна 13)

База: ветка `r6-u26`, коммит `dab9686f`. Все правки — чистые переносы/переименования; поведение, ABI, битовые раскладки и тексты `fatal(b"...")` не менялись. Сборки/тесты/бенчи не запускались (запрещены правилами раунда) — всё тяжёлое идёт в CI. Разрешённые проверки перечислены в конце.

## Итог по задачам

| Задача | Статус | Причина/объём |
|---|---|---|
| R6-T44 (`AbandonGuard` → `LeaseReleaseGuard`) | НЕ СДЕЛАНО | детерминированно ломает 2 source-pin теста и iai-точку входа; алиас их не спасает; тесты менять запрещено (§1) |
| R6-T43 (мёртвый код) | СДЕЛАНО | зеркало в `loom_shim.rs` удалено, `cfg(not(std))`/конъюнкты `std` убраны, все 7 предупреждений B3 устранены поэлементно, clippy-строка добавлена синхронно (§2) |
| R6-T45 (константы/имена) | ЧАСТИЧНО | (a),(b),(c),(d) сделаны; (e) типизированные состояния и (f) `slot_index` — НЕ сделаны, обе блокировки немодифицируемыми потребителями (§3) |
| R6-T41 (конвенции) | ЧАСТИЧНО | CQ-1 сделана; CQ-2 — только 5 безопасных сужений видимости, остальное осознанно не тронуто; CQ-3 — 25 лишних однототовых `#[path]` удалены, межкаталожные/глобы/алиасы не тронуты (§4) |

## 1. R6-T44 — НЕ СДЕЛАНО (блокировка, нужно решение владельца)

Переименование типа в `src/global/tls_heap.rs` детерминированно роняет CI, а тесты править запрещено:

- `tests/r11_ph4b_teardown_ordering_pinned.rs:40` — `.find("impl Drop for AbandonGuard")` по сырому тексту `src/global/tls_heap.rs` (`include_str!`), без совпадения — паника. Строка :44 берёт тело до `\n}\n` от найденного места.
- `tests/r11_ph4b_local_no_drop_pinned.rs:75` — `.find("struct AbandonGuard")`, :82 — текст ассерта с именем.

Предложенный в постановке алиас `#[doc(hidden)] pub type AbandonGuard = LeaseReleaseGuard;` не решает проблему: (а) тесты ищут литеральные подстроки `struct AbandonGuard` / `impl Drop for AbandonGuard` в исходнике — алиас их не создаёт; (б) это новый `pub`-элемент на doc-hidden-поверхности (расширение internals-свой API без согласия владельца).

Дополнительно сломались бы: `benches/r20_prf04_heap_reuse_l2_iai.rs:34` (`EntryPoint::Custom` с манглированным символом `...tls_heap::AbandonGuard`), `scripts/r20_prf04_heap_reuse_support.mjs:35` (тот же символ), `docs/evidence/source_scope.csv:4` (evidence-строка R4b-11 пинит `src/global/tls_heap.rs::impl Drop for AbandonGuard`).

Уточнение к постановке: `README.md`, `crates/`, `fuzz/` НЕ содержат `AbandonGuard` (rg — 0 совпадений); «таблица unsafe» имени не упоминает. В `src/` имя живёт в типе + комментариях (`tls_heap.rs:8,76,178,215,223,234,248,431,497,600`, `global/mod.rs:8`, `registry/mod.rs:9`, `heap_slot.rs:20`, `heap_core/state/ownership.rs:11`, `sefer_alloc/diag.rs:231`); комментарии оставлены как есть — править их без переименования значило бы ссылаться на несуществующий тип. `tests/no_stale_doc_references.rs:235` упоминает имя только в прозе док-комментария (ничего не ассертит).

**Вопрос владельцу:** разрешить правку якорей в двух пин-тестах + `EntryPoint`/`mjs`-символов + evidence-CSV (тогда R6-T44 делается одним механическим коммитом), либо закрыть задачу как отклонённую. Точный список правок готов приложить.

## 2. R6-T43 — СДЕЛАНО

### 2а. Мёртвое зеркало tagged-стека удалено

`src/registry/bootstrap/loom_shim.rs`: 463 → 236 строк. Удалены `StackHead`/`StackStorage`/`StackOps` с blanket-impl, комментарий divergence-заметок 1–7, `use core::sync::atomic::AtomicU64;`, `use tagged_index_stack::{...};` и 4 строки модульной доки о зеркале. `OncePtrCell`-шим, `#![allow(unsafe_code)]` и оба `unsafe impl` сохранены — README-инвентарь unsafe (пересчитывается тестами из grep) и `tests/unsafe_allow_placement.rs` не затронуты.

Проверка неиспользования: `rg "StackHead|StackStorage|StackOps"` по `src/ tests/ benches/ examples/ crates/ scripts/` — все потребители импортируют настоящий крейт (`tests/loom_registry_free_slots.rs:23` — `use tagged_index_stack::{...}`), ссылок на `loom_shim::{Stack*}` нет; комментарий самого файла прямо называл зеркало «unused by current source callers».

Сопутствующие абзацы: `src/lib.rs` (бюллетень seam `loom_shim` — убраны два предложения о зеркале), `src/registry/bootstrap/mod.rs` (то же в списке детей), `README.md` строка таблицы unsafe для `loom_shim.rs` (убрано «/ the tagged free-list head»).

### 2б. Недостижимые cfg-ветви `std`

Основание: `alloc-core = ["std", ...]` (Cargo.toml:168), модуль `global` собирается только под `alloc-global` (`src/lib.rs`), значит в `src/global/**` `std` включён всегда.

- `src/global/fallback.rs`: пары `#[cfg(feature = "std")]` / `#[cfg(not(feature = "std"))]` в лузер-цикле `heap_ptr_impl` и в `LockGuard::acquire` — оставлена только std-ветка (`std::thread::yield_now()`); 2 док-предложения о not(std)-ветке удалены; 5× `all(feature = "std", feature = "internals", feature = "bench-internals")` → `all(feature = "internals", feature = "bench-internals")`; 2× `all(feature = "std", feature = "internals")` → `feature = "internals"`.
- `src/global/mod.rs`: те же 2 конъюнкта сняты у re-экспортов R12-04/L4-хуков.

### 2в. B3 — 7 предупреждений при `--features "alloc-global alloc-xthread"`

Эмпирика: до правок эта комбинация давала ровно 7 предупреждений (список совпал с постановкой), `production` и `production internals` — 0 уже на базе. После правок — 0/0/0. Каждому элементу сужен cfg до фактического набора потребителей (grep по `src/ tests/ benches/ examples/ crates/`), blanket-allow не заводился:

| # | Элемент | Файл | Фактические потребители (cfg) | Правка |
|---|---|---|---|---|
| 1 | re-export `mem::OwnedBlock` | `src/alloc_core/alloc_core/mod.rs` | внешняя цепочка `alloc_core::OwnedBlock` (outer: `all(alloc-global, fastbin)`) → fastbin-пути free/realloc | cfg → `all(feature = "alloc-global", feature = "fastbin")` |
| 2 | `canonical_root_for` | `src/alloc_core/alloc_core/mem/mem_impl.rs` | fastbin-дебаг-ассерты (`hot.rs`, `dealloc_own_base.rs` ×2, `state/tcache_flush.rs` — все `all(alloc-global, fastbin)`) + `all(alloc-global, internals)` (`heap_core/diag/queries.rs`) | cfg → `any(feature = "fastbin", all(feature = "alloc-global", feature = "internals"))` |
| 3 | `release_initializing` | `src/alloc_core/large/reservation_state.rs` | единственный production-вызов — `alloc-decommit`-ветка Large-кэша в `alloc_large` (`alloc_core_large.rs`, cfg на ветке :293); kani/тесты — свои cfg | allow → `not(any(feature = "alloc-decommit", all(feature = "internals", feature = "alloc-global")))` |
| 4 | `SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL` | `segment_table_impl.rs` | писатели — `recycle()` (`alloc-decommit`); читатель — internals-диаг (`table_diag.rs`) | из allow-условия убран лишний `alloc-xthread` |
| 5 | `CONFIG_CONFLICTS` | `heap_registry/counters.rs` | increment — `claim_lease_with_config` (`alloc-decommit`, `claim.rs`); чтение — `config_conflicts_total` | allow → `not(any(feature = "alloc-decommit", feature = "internals"))` |
| 6 | `config_conflicts_total` | `heap_registry/counters.rs` | `stats()` (`diag.rs`, ветка `alloc-decommit`) + внешние примеры (internals) | тот же allow |
| 7 | re-export `heap_registry::config_conflicts_total` | `src/registry/mod.rs` | тот же набор | `cfg_attr(not(any(alloc-decommit, internals)), allow(unused_imports))` — по образцу соседних re-экспортов |

### 2г. clippy-строка для `alloc-global alloc-xthread` — добавлена синхронно

`scripts/check-matrix.mjs` (`PER_PR_ROWS`, после `clippy-numa-aware-internals`) и `.github/workflows/ci.yml` (шаг после numa-aware): `features: 'alloc-global alloc-xthread'`, `libOnly: true`, шаг `run: cargo clippy --lib --features "alloc-global alloc-xthread" -- -D warnings`.

- Почему `--lib`: `cargo check --all-targets --features "alloc-global alloc-xthread"` НЕ собирается по design — `examples/sol_f1_dbg_carve_batch_negative_probe.rs` (required-features = `["alloc-core"]`, намеренный compile-fail без `internals`, E0599 на `dbg_carve_batch`). Тот же мотив, что у существующих `--lib`-строк.
- Контракт `tests/ci_clippy_matrix_consistency.rs` выполнен: 8 clippy-строк манифеста ↔ 8 строк `run: cargo clippy` в job `clippy`, тот же порядок; ожидаемая форма `cargo clippy --lib --features "<multi-token>" -- -D warnings` (кавычки обязательны, нормализатор их сохраняет). Заодно исправлен устаревший счётчик в шапке job (было «6 clippy rows» при семи шагах) — теперь 8 и полный список.
- Counterfactual: на коде до правок та же команда возвращает 7 предупреждений ⇒ с `-D warnings` строка красная.

## 3. R6-T45 — ЧАСТИЧНО

- **(a) DONE** — `pub(crate) const SIDECAR_WORD_BYTES: usize = MIN_BLOCK * u64::BITS as usize;` в `src/alloc_core/segment/segment_layout.rs`; заменил 9 вхождений «слова sidecar» (7 из ревью + док-комментарии): `sidecar_drain.rs` ×2, `sidecar_bitmap.rs` (+док), `find_segment.rs` ×2, `heap_core/diag/queries.rs` ×3, `segment_layout.rs` ×2 (+док). Для досягаемости из `registry::heap_core` re-export в `src/alloc_core/mod.rs` («use segment::segment_layout;») поднят до `pub(crate) use`.
- **(b) DONE** — `pub(crate) fn word_bit(index: usize) -> (usize, usize)` (рядом, `#[inline(always)]`); заменил 11 пар `x / 64` + `x % 64`: `segment_directory_impl.rs` ×5, `directory_diag.rs` ×2, `active_kind_index.rs` ×4. Обратные формы `word * 64 + bit` и summary-сдвиг `u64::MAX << (word + 1)` сознательно не тронуты.
- **(c) DONE** — enum-и вместо сырых индексов: `ClaimSource` (`#[repr(usize)]`, дискриминанты = индексы `CLAIM_SOURCES`, `counters.rs`; в `claim.rs` — `Cell<ClaimSource>`, все `set(N)` переведены; форма наблюдателя `dbg_registry_claim_sources() -> [u64; 4]` сохранена); `WorkerFault` (`#[repr(u8)]` — НЕ `repr(usize)`, потому что значение хранится в `AtomicU8`; `maintenance_service.rs`; коды 1/2 и 0=«работать» сохранены); `ScanCount` (`#[repr(usize)]`, `bitmap_scan.rs`, только внутренняя индексация `[u64; 5]`; форма возврата `measure_scans` и позиции, запиненные `tests/r6_sidecar_bitmap.rs`, не менялись).
- **(d) DONE** — `MAX_HEAPS` перенесён в `segment_header_impl.rs` (к `OWNER_ID_FALLBACK`); в `registry/bootstrap/registry.rs` — re-export `pub use crate::alloc_core::segment_header::MAX_HEAPS;` (пути `registry::{bootstrap::}MAX_HEAPS` сохранены); ассерт стал `OWNER_ID_FALLBACK as usize >= MAX_HEAPS`.
- **(e) НЕ СДЕЛАНО** — типизированные `FallbackInitState`/`SlotState`. Значения 0/1/2 пришлось бы сохранить побитово, а коллизию имён это устраняет только вместе с удалением `STATE_LIVE` — но алиас пинится тестом: `tests/r11_ph4b_teardown_ordering_pinned.rs:111` скрейпит из `claim.rs` литеральную форму `cas_state(..., STATE_LIVE, STATE_FREE, Ordering::Release, ...)`; константы `STATE_*` импортируются тестами через `global::` (`global/mod.rs`). Отклонено как небезопасное.
- **(f) НЕ СДЕЛАНО** — унификация `slot_index()`: оба направления ломают немодифицируемых потребителей. `MaintenanceLease::slot_index() usize→u32` ломает `tests/r11_ph4b_spawn_failure_retry_releases_leases.rs` (результат передаётся в `Registry::dbg_slot_state(idx: usize)`, `registry.rs:331`); `HeapLease::slot_index() u32→usize` ломает `benches/heap_fanin_production.rs` и `heap_fanin_persistent.rs` (`assert_eq!` против `u32` из канала `(u32, Vec<usize>)`). В `src/` у `MaintenanceLease::slot_index` вызовов нет вовсе.

## 4. R6-T41 — ЧАСТИЧНО

- **CQ-1 DONE** — `config/` стоял на лимите 7 записей, поэтому создан подкаталог `config/policy/` (итог: `config/` = 7 записей, `policy/` = 4). Бывший `profile.rs` (467 строк) разделён: `policy/small_pool_policy.rs` (42), `policy/large_cache_policy.rs` (198), `policy/profile.rs` (241: модульная дока-ратиционал + `Profile`), `policy/mod.rs` (только decls + re-exports). Реэкспорты сохранены: `alloc_core/mod.rs` `pub use config::policy;` + `pub use policy::{LargeCachePolicy, Profile, SmallPoolPolicy};`; crate-root `lib.rs` не менялся; в `config/large_cache_config.rs` обновлены 5 путей/интра-док-ссылок на `config::profile::…` (4 в доках :380-384 + живой сигнатурный путь :405). Старый `src/alloc_core/config/profile.rs` удалён. Примечание: в `policy/large_cache_policy.rs` заведён `#[allow(unused_imports)]` на импорт `LargeCacheConfig` — он нужен только для разрешения интра-док-ссылок (`[LargeCacheConfig::budget_bytes]`, иначе `-D warnings` в docs-CI), с объясняющим комментарием (по образцу `alloc_core/mod.rs`).
- **CQ-2 МИНИМУМ** — сделаны только сужения с доказанно пустой внешней поверхностью: `tls_heap.rs`: `CurrentHeapForDealloc`, `current_for_dealloc`, `current_for_alloc_with_config` → `pub(crate)` (потребители только `sefer_alloc/diag.rs`, `core.rs`); `fallback.rs`: обе cfg-копии `heap_ptr` и `with_heap` → `pub(crate)` (в коде тестов/примеров не именуются — только в строках сообщений). НЕ тронуто (по правилу «что используют тесты — не трогаем»): `claim.rs` `HeapRegistry`/`MaintenanceLease`/`HeapLease` (возвращаемые типы doc-hidden pub-хуков, дергаемых тестами; сужение = E0446); `fallback.rs` `STATE_*` (импортируются тестами через `global::`); `counters.rs` fn-ы (pub use-цепочки + examples); `terminal_words.rs` — тезис ревью устарел: кроме `pub(crate) use terminal_words::*` есть `#[cfg(internals)] pub use terminal_words::{LargePhase, pack_large_state, ...}` (`segment_header/mod.rs`), и тесты импортируют их напрямую (`tests/r6_large_credit_state.rs:5`, `tests/r6_route_large_adapter.rs:3`).
- **CQ-3 ЧАСТИЧНО** — удалены 25 лишних однототовых `#[path]` (`alloc_core/alloc_core/mod.rs` ×8, `segment_header/mod.rs` ×5, `segment_table/mod.rs` ×7, `alloc_core_small/mod.rs` ×5); декларации, доки и cfg сохранены. НЕ тронуто (по постановке записано в отчёт): межкаталожные `#[path]` (`segment/mod.rs:24,26,28,44` — их 4), 7 glob-реэкспортов `*_impl::*`, 9 алиасов `#[allow(unused_imports)]` в `alloc_core/mod.rs` (их снятие требует миграции внутренних путей — сдвиг структуры).

## Изменённые файлы (38 изменённых + 4 новых, 1 удалён)

- Удалён: `src/alloc_core/config/profile.rs`. Новые: `src/alloc_core/config/policy/{mod,profile,small_pool_policy,large_cache_policy}.rs`.
- src: `src/lib.rs` (дока seam), `src/alloc_core/mod.rs`, `src/alloc_core/config/{mod,large_cache_config}.rs`, `src/alloc_core/config/policy/*`, `src/alloc_core/alloc_core/mod.rs`, `src/alloc_core/alloc_core/mem/mem_impl.rs`, `src/alloc_core/alloc_core/sidecar_drain.rs`, `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs`, `src/alloc_core/large/reservation_state.rs`, `src/alloc_core/segment/{mod.rs не менялся; segment_header/mod.rs, segment_header/segment_header_impl.rs, segment_layout.rs, remote_bitmap/bitmap_scan.rs, remote_bitmap/sidecar_bitmap.rs, segment_directory/segment_directory_impl.rs, segment_table/mod.rs, segment_table/active_kind_index.rs, segment_table/segment_table_impl.rs}`, `src/alloc_core/small/alloc_core_small/{mod.rs, find_segment.rs}`, `src/global/{mod.rs, fallback.rs, tls_heap.rs, maintenance_service.rs}`, `src/registry/{mod.rs, bootstrap/loom_shim.rs, bootstrap/mod.rs, bootstrap/registry.rs, heap_core/diag/queries.rs, heap_registry/claim.rs, heap_registry/counters.rs}`.
- Прочее: `README.md` (строка таблицы unsafe loom_shim), `scripts/check-matrix.mjs` (+1 clippy-строка), `.github/workflows/ci.yml` (+1 шаг, шапка job), `tests/{r6_sidecar_bitmap,class_leaves_publication_miri,sidecar_granule_index_exhaustive,loom_sidecar_bitmap_actual}.rs` — по 4-строчному стабу `segment_layout::SIDECAR_WORD_BYTES` (те же формулы) в фейковые `alloc_core`-модули: эти тесты source-инклюдят `sidecar_bitmap.rs`, куда рефакторинг добавил импорт; без стабов E0432. Это единственные правки в `tests/`.

## Тесты

- Новых тестов нет (чистый перенос; постановка — «новые не нужны»). Counterfactual-пин для B3 — новая clippy-строка (§2г): красная на старом коде.
- CI-строки, где проверяется: новая строка `clippy (--features "alloc-global alloc-xthread")` (job `clippy`); регресс существующих поверхностей ловят существующие строки.
- Запрещено мне, прогнать на CI: `cargo test` всех фичевых строк (особенно `alloc-global internals` и `production internals`: r11_ph4b_*, regression_fallback_*, r6_*), `cargo test --doc`, loom, miri, `cargo hack`, iai/bench-гейты, `cargo doc` `-D warnings`, `npm run check`/`iai`/`bench:table`, сборку examples/benches.

## Предлагаемый коммит

Префикс: `refactor` (перенос/выделение без изменения поведения; попадание под `docs:` direction-2 исключено — правки не только комментарии).

Сообщение: `refactor: R6-T41/T43/T45 - drop dead loom_shim stack mirror, per-item cfg gates for alloc-global alloc-xthread (+clippy row), split config/policy, name constants/enums`

(если оркестратор дробит: T43 = mirror+cfg+B3+matrix; T45 = constants/enums; T41 = policy split + #[path] + visibility.)

## Вопросы владельцу и риски

1. **R6-T44** — нужно решение (§1): разрешить правку 2 пин-тестов + bench-символов + evidence-CSV, либо закрыть задачу.
2. Путь модуля `alloc_core::profile` стал `alloc_core::policy` (`alloc_core/mod.rs`); crate-root API (`lib.rs:512-518`) не менялся. Исторические доки/обзоры упоминают старый путь — не правились (docs/ вне scope).
3. Не запускались loom/miri/iai/doc/test — регрессии там оценивает CI. Особые точки: `tests/r6_sidecar_bitmap.rs` (позиции `measure_scans` — не менялись), счётчики r18/r19-oracle (переименованы только имена индексов), evidence-пины не задеты (T44 не делался).
4. `#[allow(unused_imports)]` в `policy/large_cache_policy.rs` — осознанный, с комментарием; альтернатива — явные пути в док-ссылках (могу переделать по решению владельца).
5. Служебный файл `old_profile.tmp`, оставленный суб-агентом в корне worktree, удалён; в коммит не попадал.

## Разрешённые проверки (команда → результат)

Все с `CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/.rush/cargo-target`:

1. `cargo check --features "alloc-global alloc-xthread" --message-format short` → 0 warnings, 0 errors (exit 0); до правок — ровно 7 предупреждений B3.
2. `cargo check --features production --message-format short` → 0/0 (exit 0).
3. `cargo check --features "production internals" --message-format short` → 0/0 (exit 0).
4. `cargo check --tests --benches --features "production internals" --message-format short` | grep `^error` (без sol_f1) → пусто (компилируемость всех тестов/бенчей под internals — проверка CQ-2/переименований).
5. `cargo check --all-targets --features "alloc-global alloc-xthread"` → единственная ошибка E0599 в `sol_f1_dbg_carve_batch_negative_probe` — намеренный compile-fail (обосновывает `libOnly` новой clippy-строки).
6. `cargo clippy --lib --features "alloc-global alloc-xthread" -- -D warnings` → exit 0.
7. `rustfmt --edition 2021 --check` по всем изменённым `.rs` (включая 4 новых `policy/*`; удалённый `profile.rs` исключён) → exit 0.
8. `node scripts/verify-structure-limits.mjs` → exit 0 («none new, none grown»; `config/` = 7 записей, `policy/` = 4).
9. `node scripts/verify-dbg-hook-safety.mjs` → PASS (249 safe / 34 unsafe / 123 bench-gated), exit 0 — новых dbg-хуков нет, реестр валиден.

## U26b (волна 14) — R6-T44 + остаток R6-T45: СДЕЛАНО, пинящие тесты обновлены

База: ветка `r6-u26b` (HEAD `6b166a7b`). По решению владельца тесты, пинящие старое, обновлены под новое. Итог: 52 изменённых файла. Этот раздел дополняет и частично отменяет строки таблицы «Итог по задачам» выше (R6-T44: НЕ СДЕЛАНО → СДЕЛАНО; R6-T45 (e): НЕ СДЕЛАНО → СДЕЛАНО; R6-T45 (d) уточнён ниже). Сборки/тесты не запускались (правила раунда); компиляция проверена разрешёнными `cargo check`/`clippy` (см. конец раздела).

### R6-T44 — переименование `AbandonGuard` → `LeaseReleaseGuard`

- `src/global/tls_heap.rs` — 13 мест: модульная дока (`:8`), TORN-дока (`:76`, `:178`), GUARD-дока (`:190`, `:213`), `static GUARD` (`:204`), `struct` (`:215`), `impl` (`:223`), `mark_local_torn`-дока (`:234`), `impl Drop` (`:248`), резолверы (`:433`), `finish_bind` (`:499`), dbg-хук (`:602`); проза «abandon guard» → «lease-release guard».
- Комментарии: `src/global/mod.rs:8`, `src/registry/mod.rs:9`, `src/registry/heap_slot.rs:20`, `src/registry/heap_core/state/ownership.rs:11`, `src/global/sefer_alloc/diag.rs:231`, `Cargo.toml:1761` и `:1765` (только строки комментариев bench-блока).
- Пин-тесты обновлены под новые литералы: `tests/r11_ph4b_teardown_ordering_pinned.rs` (`find("impl Drop for LeaseReleaseGuard")`, хелпер переименован в `lease_release_guard_drop_body`), `tests/r11_ph4b_local_no_drop_pinned.rs` (`find("struct LeaseReleaseGuard")` + сообщения).
- Символ TLS-деструктора обновлён синхронно в трёх местах: `benches/r20_prf04_heap_reuse_l2_iai.rs:34` (`EntryPoint::Custom`), `scripts/r20_prf04_heap_reuse_support.mjs:35` (`expectedSymbols`), `scripts/r20_prf04_heap_reuse_run.mjs:604` (regex-альтернатива).
- `tests/no_stale_doc_references.rs:232-239` и коммент в тесте (`~:246-249`): больше не называют `AbandonGuard` живым типом (R6-T44 переименовал guard); файл возвращён к baseline 2923 строк (лимит структуры).
- `docs/evidence/source_scope.csv:4` — якорь R4b-11 → `src/global/tls_heap.rs::impl Drop for LeaseReleaseGuard` (после коммита резолвится ровно 1 раз — проверено `verify-evidence-registry` против рабочего дерева). `registry.csv` не правил (по постановке).
- Комментарии в 8 тестах: `dealloc_only_no_bind_torn`, `global_alloc_installed`, `global_alloc_mt`, `r17_8_deterministic_trim_releases_cached_large_span`, `race_repro`, `regression_r2_08_globalalloc_no_unwind`, `regression_r4_3_teardown_trim`, `tls_heap_teardown_torn_sentinel`.
- `git grep AbandonGuard` после правки: только история — `CHANGELOG.md` (1) и `docs/**` (41: `reviews/`, `checkpoints/`, `design/`, `perf/` (включая PREREG), `_raw_*.log`, `agent_reviews_*`, датированные планы `ALLOC_PLAN_PHASE12-13`/`FASTBIN_DESIGN`/`RACE_DRAIN_RECLAIM`/`REMOTE_FREE_SIDECAR_*` — трактованы как история и не тронуты).

### R6-T45(d) — типизированные состояния

- `src/registry/heap_slot.rs`: шесть `STATE_*`-констант → `#[repr(u8)] pub enum SlotState { Empty=0, Live=1, Initializing=2, Free=3, Maintenance=4 }` + `TryFrom<u8>` (гварды сравнивают с вариантным `as u8` — перенумерация не рассинхронизирует impl). Значения в памяти/атомиках не менялись: `HeapSlot::cas_state` принимает `SlotState` и кастует внутри (`expected as u8, new as u8`), внешняя форма `Result<u8, u8>` сохранена. Алиас `STATE_LIVE`/`STATE_OWNED` свёрнут в единственный канон `Live`: у `STATE_OWNED` не было потребителей вне файла, а вся живая проза, пин-тесты и evidence-CSV формулируют переход как «LIVE → FREE». Коллизия имён `STATE_INITIALIZING` (fallback=1, slot=2) снята типами.
- `src/global/fallback.rs`: `#[repr(u8)] pub enum FallbackInitState { Uninit=0, Initializing=1, Ready=2 }` + `TryFrom<u8>`; `INIT_STATE` остался `AtomicU8`, все 9 использований — через `FallbackInitState::X as u8`; `dbg_init_state()` по-прежнему возвращает `u8`.
- Production-чтение состояния идёт через `TryFrom`: `src/registry/heap_registry/claim.rs` — `let observed = SlotState::try_from(slot.state.load(..))`, ветки `Ok(SlotState::Empty)` / `Ok(SlotState::Free)` / `_`, сравнение `observed == Ok(SlotState::Maintenance)`; CAS-аргументы типизированы во всех 8 местах `claim.rs` + `counters.rs` (`dbg_claim_then_simulate_oom`); сравнения в `stack.rs` — `== SlotState::Free as u8` и т.п.
- `src/global/mod.rs:68-71` — реэкспорт трёх констант заменён на `FallbackInitState` (тот же `#[cfg(feature = "internals")] #[doc(hidden)]`); `src/registry/bootstrap/ensure.rs:89` — комментарий у zeroed-чанка.
- Потребители в тестах обновлены механически (`STATE_X` → `SlotState::X as u8` / `FallbackInitState::X as u8`): loom-модели `loom_r11_ph4a_heap_lease`, `loom_r11_ph4b_publish_recycle_drain`, `loom_r8_maintenance_lease`; `r11_p3_registry_claim_{chunk_oom,first_bind,uninit_oom}`; `r11_ph4a_lease_miri`; `r11_ph4a_lease_not_send_sync`; `r11_ph4b_lease_miri`; `r11_ph4b_spawn_failure_retry_releases_leases`; `r6_terminal_lease_registry`; `r7_p1_chunk_oom`; `r8_maintenance_registry`; `registry_basic`; `regression_claim_oom_initialised_gate`; `regression_counter_wrap`; `con01_exit_stranded_lock`; `r12_04_fallback_init_losers_yield`; `regression_fallback_init_unwind_guard`; док-комментарии `r11_ph5b_c6_fallback_oom`.
- Пин-скрейпы обновлены с сохранением смысла: `tests/r11_ph4b_teardown_ordering_pinned.rs:111` и — вопреки постановке «только :111» — также `tests/r11_ph4a_drop_ordering_pinned.rs:37-40`, который пинил те же литералы: `args[0].ends_with("SlotState::Live")`, `args[1] == "SlotState::Free"`, `args[2] == "Ordering::Release"`.
- Не тронуто (собственные локальные протоколы): `tests/loom_registry_free_slots.rs` (свои u32-константы), `crates/once-ptr-cell/tests/loom_once_ptr_cell.rs`, `OWNER_STATE_LIVE` (owner-stamp сегментных заголовков — другой протокол).

### R6-T45(e) — один тип `slot_index()`

`MaintenanceLease::slot_index()`: `usize` → `u32` — канон выбран по типу слот-id крейта (`HeapLease.index: u32`, `reuse_hint: u32`, owner-id в сегментных заголовках `u32`); `HeapLease::slot_index()` уже был `u32` и не менялся. Поле `MaintenanceLease.index: usize → u32` с комментарием о безубыточности каста (bounds-check `< MAX_HEAPS` выше по функции); публикация хинта в `Drop` — `reuse_hint.swap(self.index, ..)` без каста. Потребители: `tests/r11_ph4b_spawn_failure_retry_releases_leases.rs` (`dbg_slot_state(index as usize)` ×2), `tests/r6_terminal_lease_registry.rs:23` (убран устаревший `as usize`), `tests/r8_maintenance_registry.rs:33` (`index as u32`), `tests/r11_p3_registry_claim_chunk_oom.rs:71` (убран identity-каст `as u32`), `tests/registry_basic.rs:257` (`id_a as u32`). Бенчи `heap_fanin_{production,persistent}` не менялись (путь `HeapLease` не затронут).

### Тесты / counterfactual

Новых тестов нет (постановка — чистое переименование/типизация с обновлением пинов). Роль «красного на старом коде» несут обновлённые source-pin тесты: `r11_ph4b_teardown_ordering_pinned` / `r11_ph4b_local_no_drop_pinned` паникуют на старом тексте `tls_heap.rs` (литералы `impl Drop for AbandonGuard` / `struct AbandonGuard` больше не находятся); `r11_ph4a_drop_ordering_pinned` / `r11_ph4b_teardown_ordering_pinned:111` красные, если CAS в `HeapLease::drop` перестанет быть `SlotState::Live → SlotState::Free` с `Ordering::Release`. CI-строки — существующие (cfg-гейты тестов не менялись): `alloc-global internals`/`production internals` (пины и registry/fallback-тесты), loom-джоба (`--cfg loom`, фичи `alloc-global alloc-xthread internals tagged-index-stack/loom`), miri-джобы (`r11_ph4a_lease_miri`, `r11_ph4b_lease_miri`), iai-джоба prf04 (символ деструктора), con01/r12_04/unwind_guard — строка с `bench-internals`.

### Разрешённые проверки (команда → результат)

Все `cargo check`/`clippy` — с `CARGO_TARGET_DIR` в общем каталоге вне worktree (пути этой машины в репозиторий не пишутся):

1. `cargo check --lib --features "alloc-global alloc-xthread" --message-format short` → exit 0.
2. `cargo check --tests --benches --features "alloc-global alloc-xthread internals bench-internals" --message-format short` → exit 0 (все не-miri/не-loom тесты + `heap_fanin_*`, `heap_lifecycle_teardown`).
3. `cargo check --features production --message-format short` → exit 0.
4. `cargo check --benches --features "production internals" --message-format short` → exit 0 (`r20_prf04_heap_reuse_l2_iai` с новым символом).
5. `cargo clippy --lib --features "alloc-global alloc-xthread" -- -D warnings` → exit 0.
6. `cargo clippy --lib --features "production internals" -- -D warnings` → exit 0.
7. `RUSTFLAGS="--cfg loom" cargo check --tests --features "alloc-global alloc-xthread internals tagged-index-stack/loom"` → exit 0 (loom-модели типечеканы; запуск моделей — только CI).
8. `rustfmt --edition 2021 --check` по всем 41 изменённым `.rs` → чисто.
9. `node scripts/verify-structure-limits.mjs` → OK («none new, none grown»; `no_stale_doc_references.rs` возвращён к baseline 2923).
10. `node scripts/verify-dbg-hook-safety.mjs` → PASS (249/34/123 — без изменений).
11. `node scripts/verify-evidence-registry.mjs` → RED, ожидаемо на незакоммиченном дереве: якорь R4b-11 скрипт ищет в `git show HEAD:` (там ещё старое имя), плюс legacy evidence-stale-строки. После коммита якорь резолвится ровно 1 раз в рабочем дереве (проверено отдельно); перепинивание строк — reverify на CI.
12. `node --check` обоих изменённых `scripts/r20_prf04_heap_reuse_*.mjs` → синтаксис OK.

Нужно прогнать на CI (запрещено локально): `cargo test` всех строк (особенно `alloc-global internals`, `production internals`, `bench-internals`-строки con01/r12_04/unwind_guard), loom-раннер `scripts/loom.mjs`, miri (`r11_ph4a_lease_miri`, `r11_ph4b_lease_miri` — их тела под `cfg(miri)` локально не типечекались), iai-гейт prf04 (реальный манглированный символ деструктора), `cargo doc`/`npm run check`/`iai`/`bench:table`, reverify evidence.

### Вопросы владельцу и риски

1. `docs/evidence/registry.csv` строка R4b-11 устарела (ожидаемо по постановке); якорь в `source_scope.csv` обновлён синхронно с кодом.
2. Канон состояния выбран `Live` (не `Owned`): у `STATE_OWNED` не было ни одного потребителя, а вся проза/пины/CSV говорят «LIVE → FREE»; альтернатива означала бы массовую правку прозы без выигрыша.
3. Тип `slot_index()` унифицирован до `u32` (не `usize`): хинты/owner-id/`HeapLease` уже u32; потребители в тестах обновлены (5 мест), бенчи не тронуты.
4. Miri- и loom-исполнение, реальные iai-символы — только CI (см. выше); локальная компиляция loom-моделей проверена.
