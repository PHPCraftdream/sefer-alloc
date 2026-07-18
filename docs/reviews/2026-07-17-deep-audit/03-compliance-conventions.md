# Deep audit 2026-07-17 — AUDIT-3: Комплаенс с конвенциями проекта

- **Аудитор:** read-only агент (@fxx), статический анализ + grep + чтение; без cargo/git-записей.
- **База:** рабочее дерево на коммите `ffd3215` (2026-07-17 18:41 +0200), ветка `main`, дерево чистое (только untracked `ci_watch*.log` и checkpoints).
- **Источник правил:** `CLAUDE.md` (корень репозитория), прочитан полностью.

---

## Сводная таблица нарушений

| # | Severity | Confidence | Место | Правило | Кратко |
|---|----------|------------|-------|---------|--------|
| F1 | **High** | High | `README.md:388-400` | Зеркало unsafe-инвентаря (README ↔ факт) | Таблица tier-2 устарела: заявлено 6 файлов / 21 сайт, фактически **14 файлов / 33 сайта**; 3 из 6 имён файлов уже не содержат сайтов (код переехал при сплитах); фраза «That's the full list» ложна |
| F2 | **Medium** | High | `README.md:344-350` | Зеркало unsafe-инвентаря (README ↔ факт) | Таблица «External publishable crates» перечисляет 3 крейта с `#![allow(unsafe_code)]`, фактически их **7** — отсутствуют `racy-ptr-cell`, `ring-mpsc`, `globalalloc-model`, `proc-memstat` |
| F3 | **Medium** | High | `src/lib.rs:108-127` | Зеркало unsafe-инвентаря (lib.rs ↔ факт) | Блок «EXTERNAL publishable crates» в заголовке lib.rs — те же 4 отсутствующих крейта, что и в F2 |
| F4 | **Low** | High | `src/lib.rs:89-96` | Точность зеркала / актуальность доков | «four independently-publishable companion crates» — в workspace фактически **11** крейтов (`Cargo.toml:84`) |
| F5 | **Low** | Medium | `crates/vmem/src/mock.rs:31-134`, `crates/vmem/src/fault_injection.rs:56-67` + текст исключения №3 в `CLAUDE.md` | One file — one export (исключение №3 «single-file seam crates») | `aligned-vmem` больше не однофайловый (lib.rs + error.rs + mock.rs + fault_injection.rs), исключение №3 в написанном виде на него не распространяется; `mock.rs` несёт 5 top-level `pub`-элементов, `fault_injection.rs` — 2 |
| F6 | Info | High | корень репо | Гигиена рабочего дерева | Untracked `ci_watch.log`, `ci_watch2.log`, `ci_watch_ffd3215.log` в корне — мусор от наблюдения за CI, не в `.gitignore` |
| F7 | Info | Medium | `README.md:305-315` | Актуальность доков | Диаграмма workspace в §«Two faces» показывает 4 крейта из 11 (возможно намеренно — только runtime-дерево + bench; бейджи на :320-325 корректно перечисляют только 4 опубликованных) |

Нарушений уровня Critical нет. Компиляторно-принудительная часть конвенций (confinement `unsafe`, отсутствие inline-тестов, отсутствие runnable-доктестов, чистые `mod.rs`) — **полностью зелёная**. Все находки — дрейф документации-зеркала после сплитов R6 и крейт-экстракции CRATE-P3/P4/P6/P8.

---

## Правило 1: One file — one export

**Метод:** подсчёт top-level `^pub (fn|struct|enum|trait|const|static|type|use|mod)` по каждому файлу `src/**` и `crates/*/src/**` (кроме `lib.rs`/`mod.rs`), затем ручная сверка каждого файла с >1 экспортом против санкционированных исключений.

**Файлы с >1 top-level pub и их вердикты:**

| Файл | pub items | Вердикт |
|---|---|---|
| `src/alloc_core/remote_free_ring.rs` (11) | `RemoteFreeRing`, `PushOverflow`, `RING_CAP`, `FOOTPRINT`, `ENTRY_*`, `pack/unpack_entry_hardened`, `DBG_RING_OVERFLOW` | **OK** — эталонный пример исключения №2 (protocol-constant cluster), прямо назван в CLAUDE.md |
| `src/registry/heap_slot.rs` (4) | `HeapSlot` + `STATE_FREE`/`STATE_LIVE`/`NEXT_FREE_TAIL` | **OK** — исключение №2 (протокол-константы состояния слота при одном типе) |
| `src/registry/heap_core.rs` (3) | `HeapCore` + 2 `DBG_*` статика | **OK** — исключение №1/№2 (doc-hidden диагностические счётчики при одном типе) |
| `src/global/tls_heap.rs` (9) | 2 enum-результата + 3 resolve-fn + 4 `dbg_*` хука | **OK по духу** — одна ответственность (TLS-binding resolution); `dbg_*` — исключение №1 (test-only forwarders, модуль `#[doc(hidden)]` в `lib.rs:262`) |
| `src/global/fallback.rs` (4) | `heap_ptr`, `with_heap` + 2 `dbg_*` | **OK по духу** — одна ответственность (fallback-куча), `dbg_*` — исключение №1 |
| `src/registry/bootstrap.rs` (6) | `Registry`, `ensure`, `MAX_HEAPS` + 3 test-хука | **OK по духу** — одна ответственность (bootstrap реестра), хуки — исключение №1 |
| `src/registry/heap_registry.rs` (7) | `HeapRegistry` + 4 stats-аксессора + 2 `dbg_*` | **OK по духу** — одна ответственность; аксессоры реэкспортированы doc-hidden в `registry/mod.rs:89-98` |
| `src/alloc_core/numa.rs` (4) | `NO_NODE`, `current_node`, `bind_segment`, `reserve_aligned_on_node` | **OK по духу** — один seam «NUMA interop»; `pub` только из-за `#[doc(hidden)]`-модуля (`src/alloc_core/mod.rs:50-57`) |
| `src/alloc_core/segment_header.rs` (2) | `GEN_TABLE_FOOTPRINT` + back-compat `pub use` gen-table fn (`:1110`) | **OK** — реэкспорт-форвардер после сплита R6-CQ-7c |
| `src/alloc_core/segment_header_gen_table.rs` (3) | `gen_at`/`bump_gen`/`init_gen_table_in_place` | **OK** — одна ответственность (байтовые аксессоры gen-таблицы), tier-2 документирован |
| `crates/vmem/src/mock.rs` (5) | `Call`, `drain`, `reset`, `fail_next_reserve`, `fail_next_commit` | **см. F5** |
| `crates/vmem/src/fault_injection.rs` (2) | `arm_fail_next`, `arm_fail_at` | **см. F5** |

### F5 — Low / Medium confidence: `aligned-vmem` вырос из исключения «single-file seam crate»

- **file:line:** `crates/vmem/src/mock.rs:31,112,118,127,134`; `crates/vmem/src/fault_injection.rs:56,67`; текст исключения — `CLAUDE.md` §«File and module structure», п.3 (называет `crates/vmem/src/lib.rs` примером однофайлового крейта).
- **Правило:** One file — one export; исключение №3 покрывает крейты, у которых «the whole crate is one file».
- **Факт:** после CRATE-P2 (#172, sanctioned) `aligned-vmem` состоит из 4 файлов (`lib.rs`, `error.rs`, `mock.rs`, `fault_injection.rs`). `error.rs` — 1 экспорт (соответствует правилу), но `mock.rs` несёт 5 top-level pub, `fault_injection.rs` — 2, и исключение №3 в написанном виде на многофайловый крейт уже не распространяется.
- **Влияние:** по духу оба файла чисты (одна ответственность: mock-backend с его протоколом записи вызовов; arming fault-инъекции) — но буква санкционированного исключения разошлась с реальностью, и будущий формальный аудит не имеет текстового основания их пропустить.
- **Фикс (выбрать одно):** (a) обновить п.3 в `CLAUDE.md`: «крейты в `crates/` — одна сфокусированная библиотека; многофайловые крейты следуют правилу one-responsibility-per-file, а их test-infra модули (mock/fault-injection) — санкционированные протокол-кластеры»; (b) либо свернуть `mock.rs`/`fault_injection.rs` обратно в `lib.rs` (возврат к однофайловости). Вариант (a) дешевле и честнее.

**Прочие многофайловые крейты** (`region`: handle/region/sync_region; `globalalloc-model`: arbitrary_stream/strategy; `vmem`: error) — каждый файл несёт ≤1 top-level pub, чисто.

---

## Правило 2: mod.rs — только реэкспорты

**Проверены все 5 `mod.rs`:** `src/alloc_core/mod.rs`, `src/registry/mod.rs`, `src/global/mod.rs`, `src/concurrent/mod.rs`, `src/alloc_core/deferred_large/mod.rs`.

**Вердикт: PASS, нарушений нет.** Все содержат исключительно `mod`/`pub mod`/`pub(crate) mod`/`pub use` + doc-комментарии + cfg/allow-атрибуты. Ни логики, ни типов, ни функций, ни тестов.

---

## Правило 3: тесты в tests/, не inline в src/

**Метод:** `grep -rn '#[cfg(test)]' src/ crates/*/src/` и `grep -rn '#[test]\|mod tests' src/ crates/*/src/`.

**Вердикт: PASS — ноль inline-тестов.** Единственное совпадение `#[test]` — `crates/globalalloc-model/src/lib.rs:45`, и это строка внутри ` ```text `-фенса doc-комментария (проверено: фенс открыт на `:39` как ` ```text `, закрыт на `:55`), не исполняемый код. `#[cfg(kani)] src/kani_proofs.rs` — санкционированное исключение №4.

---

## Правило 4: No doctests

**Метод:** перечислены ВСЕ doc-фенсы `^\s*//[/!]?\s*``` ` в `src/**` и `crates/*/src/**`, каждая пара open/close сверена вручную; отдельный grep на ` ```rust `/` ```no_run `/` ```compile_fail `/` ```ignore `/` ```should_panic ` и на `#[doc =`-атрибуты.

**Вердикт: PASS — ноль runnable-доктестов.** Все 27 фенс-пар открываются как ` ```text ` (файлы: `src/alloc_core/alloc_core.rs:348,578`; `src/alloc_core/large_cache_config.rs:65,144`; `src/alloc_core/small_segment_pool_config.rs:78`; `src/concurrent/pinning.rs:32,82`; `src/global/sefer_alloc.rs:89,100,200,319`; `crates/malloc-bench/src/lib.rs:60,454,491,633,662`; `crates/globalalloc-model/src/lib.rs:39`; и остальные крейты). «Голые» ` ``` `-строки, находимые grep'ом, — все до единой закрывающие фенсы ` ```text `-блоков.

**Трекнутый долг:** миграция pre-existing доктестов (план `docs/reviews/2026-07-12-round2-remediation-plan.md`) **фактически завершена** — pre-existing runnable-доктестов не осталось; каждый ` ```text `-пример снабжён указателем на runnable-форму в `tests/` (например `src/global/sefer_alloc.rs:336` → `tests/sefer_alloc_examples.rs`). Новых доктестов не добавлено.

---

## Правило 5: unsafe-инвентарь — точная сверка «факт vs README vs lib.rs»

**Источник истины** (команда из CLAUDE.md, выполнена на текущем дереве):

```text
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/
```

**Итог grep: 50 совпадений** = tier-1 src 10 + tier-1 crates 7 + tier-2 33.

### Факт, tier 1 (module-level `#![allow(unsafe_code)]`)

**src/ — 10 модулей:** `alloc_core/node.rs:37`, `alloc_core/numa.rs:21`, `alloc_core/os.rs:28`, `concurrent/hand.rs:43`, `global/fallback.rs:71`, `global/sefer_alloc.rs:67`, `global/tls_heap.rs:96`, `registry/bootstrap.rs:188`, `registry/heap_registry.rs:50`, `registry/heap_slot.rs:74`.

**crates/ — 7 крейтов:** `globalalloc-model:63`, `malloc-bench:76`, `numa:48`, `proc-memstat:52`, `racy-ptr-cell:87`, `ring-mpsc:96`, `vmem:75`. (Для контраста, `#![forbid(unsafe_code)]`: `region`, `size-classes`, `tagged-index-stack`, `proc-probe`.)

### Факт, tier 2 (item-level `#[allow(unsafe_code)]`) — 33 сайта в 14 файлах

| Файл | Сайтов | Строки |
|---|---|---|
| `src/alloc_core/alloc_core.rs` | 2 | 861 (`dealloc`), 1161 (`realloc`) — R6-MS-1/2 |
| `src/alloc_core/alloc_core_core_diag.rs` | 4 | 184, 268 (R6-CQ-2 `dbg_stamp_*`), 337, 363 (`dbg_unregister`/`dbg_recycle`) |
| `src/alloc_core/alloc_core_small.rs` | 2 | 807, 1605 (call-site блоки) |
| `src/alloc_core/alloc_core_small_diag.rs` | 5 | 97, 130, 164, 188, 303 (`dbg_*` декларации) |
| `src/alloc_core/alloc_core_small_magazine.rs` | 1 | 369 (`flush_class`, R6-MS-3) |
| `src/alloc_core/alloc_core_small_reclaim.rs` | 3 | 173, 393 (`dbg_push_to_ring`, R6-MS-4), 411 |
| `src/alloc_core/bootstrap.rs` | 1 | 187 (call-site `init_gen_table_in_place`) |
| `src/alloc_core/remote_free_ring.rs` | 2 | 582, 610 (`over_test_buffer`/`init_test_buffer`) |
| `src/alloc_core/segment_header_gen_table.rs` | 3 | 54, 97, 150 (`gen_at`/`bump_gen`/`init_gen_table_in_place`) |
| `src/registry/heap_core_alloc.rs` | 2 | 234, 444 (call-site блоки) |
| `src/registry/heap_core_diag.rs` | 1 | 106 (`dbg_push_to_ring`, делегация) |
| `src/registry/heap_core_free.rs` | 5 | 58 (`dealloc`), 99, 373, 400, 484 (`realloc`) |
| `src/registry/heap_core_tcache.rs` | 1 | 98 (call-site `flush_class`) |
| `src/registry/heap_core_xthread.rs` | 1 | 710 (call-site `gen_at`) |
| **Итого** | **33** | |

### Числовая сверка трёх источников

| Категория | ФАКТ (grep) | README §Where unsafe lives | src/lib.rs header | Совпадение |
|---|---|---|---|---|
| Tier-1 src-модули | **10** | 10 (`README.md:356-368`) | 10 (`src/lib.rs:140-170`) | ✅ / ✅ |
| «Eight active production seams» | 8 (10 − numa − hand) | 8 (`README.md:371`) | 8 (implied) | ✅ |
| Tier-1 crates с `#![allow]` | **7** | **3** (`README.md:347-349`) | **3** (`src/lib.rs:110-122`) | ❌ / ❌ — по 4 отсутствуют |
| Tier-2 файлов | **14** | **6** (`README.md:392-398`) | — (в lib.rs только проза, списка нет) | ❌ / n/a |
| Tier-2 сайтов | **33** | **21** (2+3+10+2+1+3) | — | ❌ / n/a |
| Unsafe-токены вне tier-1/tier-2 | **0** (проверено ниже) | заявлено 0 | заявлено 0 | ✅ / ✅ |

### F1 — High: таблица tier-2 в README устарела на два ремонт-раунда

- **file:line:** `README.md:388-400` (таблица + фраза «That's the full list (both tiers)» на `:400`).
- **Правило:** CLAUDE.md — «The seams are inventoried in README §"Where unsafe lives — the complete list" and mirrored in the src/lib.rs header»; «Any formal audit compares against this command's output».
- **Расхождения построчно:**
  - `README.md:394` заявляет `segment_header.rs` / 3 сайта — фактически 0: сайты переехали в `segment_header_gen_table.rs` при сплите R6-CQ-7c (`src/alloc_core/mod.rs:94-100` это документирует, README — нет);
  - `README.md:395` заявляет `alloc_core_small.rs` / 10 сайтов — фактически 2: пять `dbg_*`-деклараций теперь в `alloc_core_small_diag.rs` (5), а call-site блоки разошлись по `alloc_core_small_reclaim.rs` (3) и самому `alloc_core_small.rs` (2);
  - `README.md:396` заявляет `alloc_core.rs` / 2 = `dbg_unregister`/`dbg_recycle` — эти двое теперь в `alloc_core_core_diag.rs:337,363`; фактические 2 сайта `alloc_core.rs` — это `dealloc`/`realloc` (R6-MS-1/2), в таблице вообще не упомянутые;
  - `README.md:398` заявляет `heap_core.rs` / 3 call-site — фактически 0: сайты в `heap_core_alloc.rs` (2) и `heap_core_xthread.rs` (1) после сплита heap_core;
  - полностью отсутствуют в таблице: `alloc_core_core_diag.rs` (4), `alloc_core_small_diag.rs` (5), `alloc_core_small_magazine.rs` (1), `alloc_core_small_reclaim.rs` (3), `heap_core_diag.rs` (1), `heap_core_free.rs` (5), `heap_core_tcache.rs` (1) — т.е. вся волна R6-MS-1..4 / R6-CQ-2.
- **Влияние:** README — заявленный аудит-артефакт для внешнего ревьюера; сейчас он занижает item-scoped unsafe-поверхность в полтора раза (21 против 33) и посылает аудитора в 3 файла, где сайтов нет. Утверждение «That's the full list» фактически ложно. Сам confinement НЕ нарушен (grep — источник истины, и он чист), нарушено только зеркало.
- **Фикс:** перегенерировать таблицу `README.md:390-398` из вывода grep-команды (таблица из раздела «Факт, tier 2» выше готова к вставке); рассмотреть скрипт в `scripts/`, сверяющий README-таблицу с grep-выводом как шаг `npm run check`, чтобы зеркало не дрейфовало молча.

### F2 — Medium: README «External publishable crates» — 4 крейта отсутствуют

- **file:line:** `README.md:344-350`.
- **Факт:** grep находит `#![allow(unsafe_code)]` ещё в 4 крейтах, добавленных задачами CRATE-P3/P4/P6/P8: `crates/racy-ptr-cell/src/lib.rs:87`, `crates/ring-mpsc/src/lib.rs:96`, `crates/globalalloc-model/src/lib.rs:63`, `crates/proc-memstat/src/lib.rs:52`. В таблице их нет; также нет forbid-крейтов `size-classes`, `tagged-index-stack`, `proc-probe` (строка контраста есть только для `sefer-region`).
- **Влияние:** аудитор, идущий по README, не узнает о 4 дополнительных unsafe-несущих крейтах workspace; заявление «the complete list» неполно на уровне крейтов. Отмечу: корневой `Cargo.toml:37-80` (workspace-комментарий) эти крейты и их unsafe-статус документирует корректно — стал более актуальным, чем README.
- **Фикс:** добавить 4 строки в таблицу `README.md:347-349` (для `globalalloc-model` и `proc-memstat` пометить «dev-only, не в runtime-дереве»; для `ring-mpsc` — «не в runtime-дереве, swap = NO-GO, см. d062798»); при желании — вторую строку контраста для трёх forbid-крейтов.

### F3 — Medium: src/lib.rs header — те же 4 крейта отсутствуют

- **file:line:** `src/lib.rs:108-127` (блок «EXTERNAL publishable crates (each independently auditable)» перечисляет только aligned-vmem, numa-shim, malloc-bench-rs, sefer-region).
- **Правило:** то же зеркало (CLAUDE.md: «mirrored in the src/lib.rs header»).
- **Фикс:** дополнить блок теми же 4 записями, что и в F2. Внутренний tier-1 список (`src/lib.rs:140-170`) сверен — **точен** (все 10 модулей, фичи указаны верно).

### F4 — Low: «four companion crates» в lib.rs

- **file:line:** `src/lib.rs:89-96` («The workspace extracted four building blocks…»).
- **Факт:** `Cargo.toml:84` — 11 членов workspace.
- **Фикс:** переписать абзац: «четыре runtime/publish-крейта + семь извлечённых building-block-крейтов» либо просто сослаться на workspace-комментарий в `Cargo.toml`.

### Unsafe-токены вне двух tier'ов — проверка

**Метод:** список всех файлов, содержащих токен `unsafe` (58 файлов), минус файлы с `allow(unsafe_code)`; в остатке каждый файл проверен построчно с отсечением комментариев.

**Вердикт: PASS — 0 нарушений.** Во всех файлах вне инвентаря токен `unsafe` встречается только в комментариях/доках либо в атрибутах `#![forbid(unsafe_code)]` (`crates/proc-probe/src/lib.rs:45`, `crates/region/src/lib.rs:46`, `crates/size-classes/src/lib.rs:43`, `crates/tagged-index-stack/src/lib.rs:108`) и `src/lib.rs:206,208` (`forbid`/`deny`). Все `unsafe impl` в кодовой базе лежат внутри tier-1 модулей (`concurrent/hand.rs:461,471`, `global/sefer_alloc.rs:446`, `registry/bootstrap.rs:243-245` — loom-шим `RacyPtrCell` с per-impl `// SAFETY:`, `registry/heap_slot.rs`).

---

## Правило 6: каждый tier-2 сайт — `# Safety` doc и per-site `// SAFETY:`

**Метод:** для всех 33 сайтов проверено окно до 60 строк выше: декларации `unsafe fn` — наличие `/// # Safety`-секции; call-site `unsafe {}`-блоки — наличие `// SAFETY:`-комментария непосредственно при блоке.

**Вердикт: PASS — 33/33.** Все 17 деклараций `unsafe fn` несут `# Safety`-секцию (у `dealloc`/`realloc` в `alloc_core.rs:861,1161` и `heap_core_free.rs:58,484` секции длинные, >12 строк — подтверждены расширенным окном; `alloc_core_core_diag.rs:363` использует заголовок «# Safety contract mirrors …» — допустимо, но для единообразия rustdoc-секций лучше каноническое `# Safety`). Все 16 call-site блоков несут `// SAFETY:` при блоке (образцы: `alloc_core_small.rs:807`, `bootstrap.rs:187`, `heap_core_free.rs:99,400`, `heap_core_xthread.rs:710`; у `heap_core_free.rs:373` и `heap_core_tcache.rs:98` обоснование лежит в SAFETY-комментарии несколькими строками выше блока — присутствует, найдено в 60-строчном окне). Каждый `#[allow(unsafe_code)]` несёт инлайн-причину (`R6-MS-*` / `R6-CQ-2` / `task #101 / R4-MS-3`) — требование «single documented reason per item-scoped site» выполнено.

---

## Правило 7: версии без санкции

**Метод:** сверка всех `version` в `Cargo.toml` workspace + история изменений корневого `Cargo.toml`.

**Вердикт: PASS — несанкционированных бампов не найдено.**
- `sefer-alloc 0.3.0` — санкционировано (арка 0.3.0, задачи X-VER-*);
- `aligned-vmem 0.2.0` — санкционировано явно (задача CRATE-P2 #172: «aligned-vmem 0.2»);
- все остальные 10 крейтов — `0.1.0` (первичные версии новых крейтов, не бампы);
- MSRV `rust-version = "1.88"` — не менялся.

---

## Гигиена (Info)

- **F6:** untracked `ci_watch.log`, `ci_watch2.log`, `ci_watch_ffd3215.log` в корне репозитория — остатки наблюдения за CI-ранами; добавить `ci_watch*.log` в `.gitignore` или удалить.
- **F7:** `README.md:305-315` — диаграмма workspace показывает 4 крейта из 11. Если намеренно (только публикуемые/runtime) — добавить оговорку «см. полный список в Cargo.toml»; бейдж-таблица `:320-325` для 4 опубликованных крейтов корректна как есть.

---

## Итоговая оценка

Кодовая база **строго соответствует** всем компиляторно- и структурно-принудительным конвенциям: чистые `mod.rs` (5/5), ноль inline-тестов, ноль runnable-доктестов (долг миграции фактически погашен), ноль unsafe-токенов вне двух tier'ов, 33/33 tier-2 сайтов с полной Safety-документацией, ноль несанкционированных бампов версий. Единственный системный дефект — **дрейф документации-зеркала** unsafe-инвентаря (F1-F4): README и заголовок lib.rs не обновлялись вслед за R6-сплитами файлов и добавлением 4 unsafe-несущих крейтов, из-за чего «полный список» занижает tier-2 поверхность с 33 до 21 сайта и скрывает 4 крейта. Фикс механический (таблицы в этом отчёте готовы к вставке); рекомендуется автоматическая сверка README-таблицы с выводом grep-команды в составе `npm run check`.
