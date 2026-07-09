# Ревью чистоты: механическое соответствие конвенциям CLAUDE.md

- **Дата:** 2026-07-09
- **Угол ревью:** чистота / формальное соответствие правилам `CLAUDE.md` (1 из 7 параллельных ревью)
- **Режим:** только чтение и анализ; grep-driven, каждая находка верифицирована чтением исходника

---

## Scope

| Зона | Объём |
|---|---|
| `src/**/*.rs` | 42 файла (включая `lib.rs`, `kani_proofs.rs`, 5 `mod.rs`) |
| `crates/*/src/**/*.rs` | 7 файлов (region: 4, vmem: 1, numa: 1, malloc-bench: 1) |
| `mod.rs` во всём дереве | 5 файлов: `src/{global,registry,concurrent}/mod.rs`, `src/alloc_core/mod.rs`, `src/alloc_core/deferred_large/mod.rs` |
| Документация сеамов | `README.md` («Where unsafe lives»), `docs/ARCHITECTURE.md` §«Confined unsafe seams», шапка `src/lib.rs`, `CLAUDE.md` «Active rules» |
| Вне scope | `tests/`, `benches/`, `examples/`, `fuzz/`, `docs/` (кроме сверки инвентаря сеамов) |

## Методология (использованные проверки)

1. `rg -n '^pub'` по `src/` и `crates/*/src/` → полная карта верхнеуровневых `pub`/`pub(crate)` items (колонка 0 = верхний уровень модуля; репозиторий проходит `cargo fmt --check`, поэтому отступы каноничны).
2. `rg -n 'macro_export'` → 0 совпадений (экспортируемых макросов нет).
3. `rg -n 'cfg\(test\)|mod\s+tests'` по `src/` и `crates/*/src/` → инлайн-тесты.
4. `rg -n '#\[test\]'` по `src/` и `crates/*/src/` → прямые тест-функции.
5. `rg -n 'allow\(unsafe_code\)'` по всему репозиторию + сверка с README/ARCHITECTURE/lib.rs.
6. `rg -n 'TODO|FIXME|XXX|unimplemented!|todo!|(?i:placeholder)'` по `src/` и `crates/*/src/`.
7. `RUSTFLAGS="-W missing_docs" cargo check --workspace --all-features --message-format=short` в **изолированный** `CARGO_TARGET_DIR` (временная папка, репозиторий не затронут) → 0 предупреждений `missing documentation`. Канал линтов верифицирован контрфактически: та же команда выдала 12 предупреждений `dead_code` для `numa-shim` (см. §f-3), т.е. предупреждения реально доходят.
8. Дополнительный построчный скан (perl, вне репозитория): для каждого верхнеуровневого `pub`/`pub(crate)` item (88 `pub` + 81 `pub(crate)` определений, без учёта `pub mod`/`pub use`) — есть ли собственный `///`-doc. Каждый NODOC-хит перепроверен чтением файла.
9. Точечные греп-проверки устаревших доков: `struct Heap\b` / `heap::Heap` / `` `Heap` `` (тип удалён), `^byte\s*=` в `**/Cargo.toml` (фичи `byte` не существует).

---

## (a) «One file — one export» — файлы с более чем одним публичным экспортом

Правило CLAUDE.md: «Each source file defines exactly one public item (type, trait, function). The file name matches the export».

Считались **полностью публичные** (`pub `) верхнеуровневые определения (struct/enum/trait/fn/const/static/type). `pub use`/`pub mod` — проводка, не определения; `pub(crate)` — не публичный экспорт (вынесен в A-2).

### A-1. Нарушители по букве правила — ≥2 `pub`-экспортов в одном файле — **14 файлов, High**

| # | Файл | Кол-во | Экспорты (строка) |
|---|---|---|---|
| 1 | `src/alloc_core/remote_free_ring.rs` | **11** | `DBG_RING_OVERFLOW` (138), `RING_SLOT_EMPTY` (144), `RING_CAP` (156), `FOOTPRINT` (178), `ENTRY_OFF16_BITS` (262), `ENTRY_CLASS_BITS` (266), `ENTRY_GEN_BITS` (270), `pack_entry_hardened` (352), `unpack_entry_hardened` (386), **`RemoteFreeRing`** (426), **`PushOverflow`** (434) — два pub-типа в одном файле |
| 2 | `src/registry/bootstrap.rs` | **9** | `MAX_HEAPS` (161), `pack_abandoned_head` (204), `unpack_abandoned_head` (225), `ABANDONED_HEAD_EMPTY` (244), `abandoned_head_is_empty` (248), **`Registry`** (260), `ensure` (338), `dbg_rollback_sentinel_reenterable` (650), `count_for_test` (697). Плюс несоответствие имени: файл `bootstrap.rs`, главный экспорт `Registry` |
| 3 | `src/registry/tagged_ptr.rs` | **7** | `dbg_pack` (169), `dbg_unpack` (175), `dbg_empty` (181), `dbg_is_empty` (187), `DBG_INDEX_BITS` (193), `DBG_TAG_BITS` (198), `DBG_INDEX_MASK` (202). Сам `TaggedPtr` — `pub(crate)` (114); публичны только тест-форвардеры |
| 4 | `crates/vmem/src/lib.rs` | **7** | `PAGE` (61), `page_size` (69), **`Reservation`** (88), `reserve_aligned` (249), `release` (273), `decommit` (295), `recommit` (315) — однофайловый крейт |
| 5 | `src/global/tls_heap.rs` | **5** | `current` (254), **`CurrentHeap`** (287), `current_for_alloc` (308), `current_for_alloc_with_config` (349), `dbg_teardown_then_resolve_is_fallback` (467). Имя файла не соответствует ни одному экспорту |
| 6 | `crates/numa/src/lib.rs` | **5** | `NO_NODE` (55), `pub mod mock` (65 — **инлайн-модуль с логикой** внутри lib.rs, а не отдельный файл), `current_node` (133), `bind_range` (174), `reserve_on_node` (214); плюс 5 инлайн `mod platform` с полной реализацией — однофайловый крейт |
| 7 | `src/registry/heap_slot.rs` | **4** | `STATE_FREE` (65), `STATE_LIVE` (66), `NEXT_FREE_TAIL` (71), **`HeapSlot`** (77) |
| 8 | `src/registry/heap_registry.rs` | **4** | **`HeapRegistry`** (56), `heaps_claimed_high_water` (790), `tcache_hits_total` (861), `large_cache_hits_total` (916) |
| 9 | `src/alloc_core/run_stack.rs` | **4** | **`RunDesc`** (106), **`RunStack`** (139), `RUNSTACK_CAPACITY` (154), `FOOTPRINT` (161) — два pub-типа в одном файле |
| 10 | `src/alloc_core/segment_header.rs` | **4** | `GEN_TABLE_FOOTPRINT` (153), `gen_at` (1259), `bump_gen` (1290), `init_gen_table_in_place` (1330). Одноимённый `SegmentHeader` — лишь `pub(crate)` (210); публичны только gen-table хелперы |
| 11 | `src/alloc_core/numa.rs` | **4** | `NO_NODE` (27), `current_node` (34), `bind_segment` (54), `reserve_aligned_on_node` (78) |
| 12 | `crates/malloc-bench/src/lib.rs` | **4** | **`Workload`** (331), **`Config`** (346), `run` (408), `sweep` (511) — однофайловый крейт, два pub-типа |
| 13 | `src/alloc_core/alloc_core.rs` | **2** | **`LargeCacheMode`** (94), **`AllocCore`** (237). Самый показательный случай: **оба типа реэкспортированы в корень крейта** (`src/lib.rs:245,247`) как стабильное публичное API — `LargeCacheMode` по правилу обязан жить в собственном файле (симметрично уже выделенному `large_cache_config.rs`) |
| 14 | `src/global/fallback.rs` | **2** | `heap_ptr` (112), `with_heap` (187). Модуль объявлен приватно (`mod fallback;`, `src/global/mod.rs:23`) → наружу не виден, но по букве правила в файле два pub-item; имя файла не соответствует экспортам |

**Смягчающее обстоятельство (важно для честности вердикта):** для большинства «лишних» экспортов в репозитории существует локально задокументированный паттерн — «`pub` только потому, что модуль `#[doc(hidden)]`; это test-only export pattern» (см. комментарии в `src/alloc_core/mod.rs:20-22,43-46,56-62,66-71`, `src/registry/mod.rs:44-47`, `src/registry/tagged_ptr.rs:155-166`). То есть проект осознанно ввёл конвенцию-поверх-конвенции, которая в `CLAUDE.md` **не кодифицирована**. Исключения: №13 (`alloc_core.rs` — два элемента стабильного API) и №1/№9/№12 (по два содержательных pub-типа в файле) не покрываются даже этим паттерном.

**Рекомендация:** либо (i) разнести по файлам как минимум №13 (`LargeCacheMode`), №1 (`PushOverflow`), №9 (`RunDesc`), №12 (`Workload`/`Config`), либо (ii) явно дописать в `CLAUDE.md` санкционированные исключения: doc-hidden тест-форвардеры, кластеры протокольных констант при основном типе, однофайловые seam-крейты в `crates/`.

### A-2. Low: множественные `pub(crate)`-items / несоответствие имени файла экспорту

Буква правила говорит про «public item», поэтому ниже — Low (дух правила):

- `src/alloc_core/segment_header.rs` — **крупнейший мульти-item файл**: 7 `pub(crate)`-типов в одном файле (`SegmentKind` 159, `PageClass` 178, `SegmentHeader` 210, `PageMap` 615, `BinTable` 686, `Layout` 747, `SegmentMeta` 942) + ~10 `pub(crate)` констант + `align_up` (602), >1300 строк.
- `src/alloc_core/size_classes.rs` — два `pub(crate)`-типа (`SizeClasses` 126, `AllocKind` 379) + 8 констант.
- `src/concurrent/hand.rs` — два `pub(crate)`-типа (`EvictOutcome` 64, `AtomicSlot` 103); имя файла не соответствует ни одному.
- `src/registry/tcache.rs` — `Tcache` (105) + 3 константы + `refill_n_for_class` (81), все `pub(crate)`.
- `src/alloc_core/segment_table.rs` — `SegmentTable` (129) + 8 `pub(crate)` констант.
- Несоответствие имени файла экспорту (одноэкспортные файлы): `src/alloc_core/os.rs` → `Segment` (118); `src/alloc_core/bootstrap.rs` → `Primordial`/`primordial()` (28/41); `src/concurrent/pinning.rs` → `PinnedRunner` (104); `src/alloc_core/deferred_large/drain.rs` → pub-элемент `DBG_LARGE_XTHREAD_RECLAIMED` (24) при том, что имени файла соответствует `pub(crate) drain_large_deferred_free` (48).

Эталонные файлы (правило соблюдено идеально): весь `crates/region/src/` (`handle.rs`/`region.rs`/`sync_region.rs` — ровно по одному экспорту, `lib.rs` — только реэкспорты), `src/global/{alloc_stats,sefer_alloc}.rs`, `src/registry/heap_core.rs`, `src/alloc_core/{large_cache_config,segment_layout}.rs`, все 6 файлов `src/concurrent/*_{region,handle}.rs`, `src/alloc_core/deferred_large/{push,layout_consistent,tail}.rs`.

---

## (b) «mod.rs — reexports only, no code» — **0 нарушений**

Все 5 `mod.rs` прочитаны построчно; содержат исключительно `mod`/`pub mod`/`pub use` + doc-комментарии и атрибуты (`#[cfg]`, `#[doc(hidden)]`, `#[allow]`) на этих объявлениях. Ни одного `fn`/`struct`/`impl`/`#[test]`:

| Файл | Вердикт |
|---|---|
| `src/alloc_core/mod.rs` | чисто (правило даже процитировано в шапке: «Re-exports only — no logic lives here») |
| `src/registry/mod.rs` | чисто |
| `src/global/mod.rs` | чисто |
| `src/concurrent/mod.rs` | чисто |
| `src/alloc_core/deferred_large/mod.rs` | чисто |

Примечание (не нарушение): `src/lib.rs` — корень крейта, формально не `mod.rs`; кроме проводки содержит один `compile_error!`-guard (`lib.rs:187-193`) — задокументированная защита `fastbin`⇒`alloc-xthread` (санкционирована комментарием в `Cargo.toml:144-152`). `crates/region/src/lib.rs` — только реэкспорты (образцовый корень).

---

## (c) Инлайн-тесты в src — **0 нарушений**

- `cfg(test)` / `mod tests` в `src/**/*.rs`: **0 совпадений**.
- `cfg(test)` / `mod tests` в `crates/*/src/**/*.rs`: **0 совпадений**.
- `#[test]` в обоих деревьях: **0 совпадений**.
- Тесты фактически живут отдельно: 117 файлов в корневом `tests/` + `crates/{region,vmem,numa,malloc-bench}/tests/`.

**Low-заметка (осознанное исключение, не нарушение):** `src/kani_proofs.rs` — 11 proof-харнессов `#[kani::proof]` в двух инлайн-модулях, скомпилированных **только** под `cfg(kani)` (`src/lib.rs:252-253`). Это тестоподобный код в `src/`, но вынести его в `tests/` нельзя: харнессы обращаются к `pub(crate)`-internals (`crate::alloc_core::node::Node`, `crate::concurrent::hand::AtomicSlot`), недоступным интеграционным тестам. Правило CLAUDE.md говорит про `#[cfg(test)] mod tests` — формально не задето; при желании абсолютной чистоты стоит кодифицировать это исключение в CLAUDE.md одной строкой.

---

## (d) TODO / FIXME / XXX / unimplemented! / todo! / placeholder — **3 находки, все Low/benign**

`FIXME`, `XXX`, `unimplemented!()`, `todo!()` — **0** в `src/` и `crates/*/src/`.

| Файл:строка | Текст | Классификация |
|---|---|---|
| `src/registry/heap_core.rs:1635` | `/// TODO: call from try_adopt / reclaim_abandoned if those paths are ever wired…` (на `reset_stamp_cache`) | **Low** — осознанная roadmap-заметка «хук для будущих фаз», но фактически это мёртвый код, удерживаемый `#[allow(dead_code)]` (1638): метод «not called from any production path». Ровно тот класс «half-wired features», который ZERO-TRUST-секция CLAUDE.md велит отлавливать; сейчас он честно задокументирован — держать на радаре |
| `src/registry/heap_core.rs:292` | `…See the TODO in reset_stamp_cache` | Low — перекрёстная ссылка на предыдущий пункт, не самостоятельный долг |
| `src/concurrent/lock_free_region.rs:417` | `// Slot 0 — placeholder Vacant, overwritten below…` | benign — слово «placeholder» описывает алгоритм (слот перезаписывается ниже), не незаконченный код |

---

## (e) Инвентарь `allow(unsafe_code)` vs документация — **код ↔ README сходятся 13/13; расхождение в самом CLAUDE.md (High)**

`grep -rln 'allow(unsafe_code)'` по коду даёт ровно **13 файлов**:

| Файл (строка атрибута) | Задокументирован? |
|---|---|
| `src/alloc_core/os.rs:28` | ✅ README:345, ARCHITECTURE:136, lib.rs:130 |
| `src/alloc_core/node.rs:37` | ✅ README:346, ARCHITECTURE:137, lib.rs:133 |
| `src/alloc_core/numa.rs:21` | ✅ README:347, ARCHITECTURE:144, lib.rs:154 |
| `src/global/sefer_alloc.rs:66` | ✅ README:348, ARCHITECTURE:138, lib.rs:135 |
| `src/global/tls_heap.rs:96` | ✅ README:349, ARCHITECTURE:139, lib.rs:137 |
| `src/global/fallback.rs:67` | ✅ README:350, ARCHITECTURE:140, lib.rs:139 |
| `src/registry/bootstrap.rs:146` | ✅ README:351, ARCHITECTURE:141, lib.rs:143 |
| `src/registry/heap_slot.rs:53` | ✅ README:352, ARCHITECTURE:142, lib.rs:148 |
| `src/registry/heap_registry.rs:35` | ✅ README:353, ARCHITECTURE:143, lib.rs:150 |
| `src/concurrent/hand.rs:43` | ✅ README:354, ARCHITECTURE:145, lib.rs:159 |
| `crates/vmem/src/lib.rs:52` | ✅ README:335, ARCHITECTURE:115, lib.rs:101 |
| `crates/numa/src/lib.rs:46` | ✅ README:336, ARCHITECTURE:116, lib.rs:105 |
| `crates/malloc-bench/src/lib.rs:56` | ✅ README:337, ARCHITECTURE:117, lib.rs:109 |

Итого: **расхождений между кодом и README нет** — «Source of truth: grep -rln…» (README:329) честен; счётные утверждения README («eight named confined seams» под `production` + `numa` + деприкейченный `hand` = 10 внутренних, README:358-367) арифметически сходятся с grep. `docs/ARCHITECTURE.md:120` («10 src modules total») и шапка `src/lib.rs:95-162` — согласованы. `crates/region` корректно противопоставлен как `#![forbid(unsafe_code)]` (`crates/region/src/lib.rs:43`).

**High — устарел сам CLAUDE.md.** `CLAUDE.md:77-78` («Active rules»): «`unsafe` is allowed only in **one documented module `hand`** (phases 3b/4) behind a feature flag» — это противоречит фактическим 13 файлам (10 внутренних сеамов + 3 seam-крейта), которые остальная документация (README/ARCHITECTURE/lib.rs/Cargo.toml) описывает как санкционированную эволюцию фаз 8–12. Управляющий документ ревью сам не соответствует реальности, которую обязан нормировать: любой формальный аудит «по букве CLAUDE.md» обязан объявить 12 из 13 файлов нарушителями. Требуется обновить формулировку Active rules (например: «unsafe разрешён только в named seam-модулях, инвентаризованных в README §Where unsafe lives / lib.rs; полный список верифицируется `grep -rln 'allow(unsafe_code)' src/ crates/`»).

---

## (f) Doc-комментарии: отсутствующие и устаревшие

### f-1. Отсутствующие доки на публичных items

- **Компиляторная проверка:** `RUSTFLAGS="-W missing_docs" cargo check --workspace --all-features` → **0 предупреждений** `missing documentation` (канал верифицирован — см. Методологию §7). Оговорка: линт `missing_docs` по дизайну не проверяет `#[doc(hidden)]`-поддеревья, а почти вся «лишняя» pub-поверхность здесь именно doc-hidden. Дополнительно: `crates/{vmem,numa,malloc-bench}` сами несут `#![deny(missing_docs)]`.
- **Строгий построчный скан** (каждый верхнеуровневый item обязан иметь собственный `///`): из 88 `pub` + 81 `pub(crate)` определений без собственного doc-комментария — **9** (все — `#[doc(hidden)]`-форвардеры либо `pub(crate)`-внутренности, накрытые общим соседним доком; все Low):

| Файл:строка | Item | Фактическое состояние |
|---|---|---|
| `src/registry/tagged_ptr.rs:169,175,181,187` | `dbg_pack`, `dbg_unpack`, `dbg_empty`, `dbg_is_empty` | накрыты общим `//`-блоком (155-166), не `///`; соседние `DBG_*`-константы при этом имеют `///` — непоследовательно |
| `src/registry/heap_slot.rs:66` | `STATE_LIVE` | делит один `///` с `STATE_FREE` (62-64) |
| `src/alloc_core/segment_header.rs:83` | `OWNER_STATE_ABANDONED` | делит `/// Owner-state bit layout.` (79) с `OWNER_STATE_LIVE` |
| `src/registry/bootstrap.rs:182` | `ABANDON_TAG_MASK` | примыкает к доку `ABANDON_TAG_BITS` (178-180) |
| `src/registry/heap_core.rs:105` | `type TcacheHitCounter` | накрыт `//`-блоком (63-102), не `///` |
| `src/alloc_core/deferred_large/tail.rs:22` | `DEFERRED_LARGE_TAIL` | однофайловый item, документирован модульным `//!` — приемлемо |

### f-2. Явно устаревшие doc-комментарии — **главная находка секции**

**Кластер «`Heap`» (High по совокупности как систематический drift, хотя каждая строка — doc-only).** Тип `Heap` (явное alloc-face API) **удалён** в 0.3.x — это прямо зафиксировано в `src/registry/heap_core.rs:42-46» («An earlier `Heap`/`with_heap` public face existed… it was removed in 0.3.x»). Подтверждено: `struct Heap`/`impl Heap` в коде отсутствуют (grep), из пары остался только `with_heap` (`src/global/fallback.rs:187`), возвращающий `HeapCore`. При этом **14 строк документации в 8 файлах `src/` + Cargo.toml** продолжают говорить о `Heap` в настоящем времени как о «втором публичном face»:

| Файл:строка | Устаревший текст |
|---|---|
| `src/alloc_core/mod.rs:17` | «Shared by both public allocator faces (`registry::heap_core::HeapCore` and **`heap::Heap`**)» — путь `heap::Heap` не существует |
| `src/alloc_core/node.rs:420` | intra-doc ссылка ``[`Heap::dealloc_any_thread`](crate::heap::Heap::dealloc_any_thread)`` — **битая ссылка на несуществующий `crate::heap`** (не ловится `cargo doc`, т.к. модуль pub(crate) и не рендерится) |
| `src/alloc_core/node.rs:422,434,439` | «another `Heap`'s…», «a `Heap`'s leaked (on `Drop`…)», «a `Heap`'s identity `Box` is intentionally leaked» |
| `src/alloc_core/deferred_large/drain.rs:2,16,21,47` | «both public allocator faces, `HeapCore` and `Heap`»; «the new `Heap`-face regression test can assert…» |
| `src/alloc_core/deferred_large/push.rs:2` | «across both public allocator faces, `HeapCore` and `Heap`» |
| `src/alloc_core/deferred_large/tail.rs:3` | «both public allocator faces (`HeapCore` and `Heap`)» |
| `src/alloc_core/segment_header.rs:267` | «The pointer is stable because it is **`Box`-allocated inside the owning `Heap`**» — устарело вдвойне: тип `Heap` удалён, и `thread_free` теперь — обычное поле slot-resident `HeapCore` (`src/registry/heap_core.rs:213: pub(crate) thread_free: AtomicPtr<u8>`, стемпится через `addr_of!` — heap_core.rs:1607), никакого `Box`; стабильность адреса даёт слот реестра, а не Box |
| `src/alloc_core/alloc_core.rs:3271` | «explicit `Heap` face (`with_heap` → `Heap::dealloc_small` → …)» — метода `Heap::dealloc_small` не существует |
| `src/registry/heap_core.rs:509` | «`Heap`'s call site derives its own `&AtomicPtr<u8>`…» — настоящее время об удалённом типе |
| `Cargo.toml:173` (описание фичи `hardened`) | «what the explicit `Heap`/`with_heap` face … reach» |

**Кластер «`byte`-фича» (Low).** Фичи `byte` нет ни в одном `Cargo.toml` (`^byte\s*=` → 0), но:

| Файл:строка | Устаревший текст |
|---|---|
| `src/lib.rs:170` | «drop `SyncRegion` and the concurrent/**byte** tiers» |
| `Cargo.toml:60` | «…and the concurrent/**byte** tiers» |
| `CONTRIBUTING.md:118` | «`#![deny(unsafe_code)]` with `experimental` or **`byte`** features» — вдобавок неполно: по факту deny включают `experimental` **или `alloc-core`** (`src/lib.rs:163-167`) |

### f-3. Информационно (обнаружено контрфактической проверкой линт-канала)

`cargo check --workspace --all-features` не warning-clean: **12 `dead_code`-предупреждений в `crates/numa/src/lib.rs`** (583, 607, 612, 640, 691, 698-700, 710, 721, 723, 725) — фича `mock` замыкает публичные функции на мок и оставляет платформенные реализации неиспользуемыми. Обычные CI-конфигурации (`npm run check` гоняет clippy по корневому пакету, где `mock` не включается юнификацией) этого не видят; workspace-wide `--all-features` прогон упадёт под `-D warnings`. Low, смежно со scope.

---

## Итоговые счётчики нарушений по правилам

| Правило CLAUDE.md | High | Low | Комментарий |
|---|---|---|---|
| (a) One file — one export | **14 файлов** (11 в `src/`, 3 крейт-корня в `crates/`) | ~9 файлов (мульти-`pub(crate)`/имя файла ≠ экспорт) | худшие: `remote_free_ring.rs` (11 pub), `registry/bootstrap.rs` (9), `tagged_ptr.rs` (7); самый чистый кандидат на немедленное исправление — `LargeCacheMode` из `alloc_core.rs` (стабильное API) |
| (b) mod.rs — reexports only | **0** | 0 | 5/5 mod.rs чистые |
| (c) Тесты не инлайн в src | **0** | 1 заметка (`kani_proofs.rs`, обоснованное исключение) | 117 файлов в `tests/` + crates/*/tests |
| (d) TODO/placeholder/незаконченный код | **0** | 2 TODO (roadmap-хук `reset_stamp_cache` + перекрёстная ссылка) + 1 benign «placeholder» | `unimplemented!`/`todo!`/`FIXME`/`XXX` — 0 |
| (e) Инвентарь unsafe-сеамов | **1** (сам `CLAUDE.md:77-78` устарел: «only … one documented module `hand`» vs фактические 13 файлов) | 0 | код ↔ README ↔ ARCHITECTURE ↔ lib.rs: 13/13, расхождений нет |
| (f) Doc-комментарии | 0 (rustc `missing_docs` = 0) | **9** items без собственного `///` + **17 строк** устаревших доков (14×`Heap` включая 1 битую intra-doc ссылку, 3×`byte`) + 1 информационная (numa-shim dead_code) | кластер `Heap` рекомендуется вычистить одним проходом sed-класса |

**Сводный вердикт:** жёсткие структурные правила (b), (c) выполнены безупречно; (d) чисто; инвентарь unsafe (e) образцово синхронизирован между кодом и тремя документами, но **сам CLAUDE.md отстал от него на несколько фаз**. Систематический долг — правило (a): оно нарушено в 14 файлах по букве, при этом проект уже живёт по неписаной уточнённой конвенции (doc-hidden тест-форвардеры и seam-крейты), которую следует либо кодифицировать в CLAUDE.md, либо привести файлы к букве. Документационный drift вокруг удалённого типа `Heap` — самый массовый конкретный дефект чистоты (14 строк в 8 файлах + Cargo.toml).
