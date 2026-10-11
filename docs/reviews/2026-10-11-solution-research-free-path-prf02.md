# Исследование решения: самый дешёвый корректный own-thread free на production (R17-PRF-02 + цена `payload_start` из 36ba3758)

Базовый SHA: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`; содержит R18-исправления `ff3d60bb..HEAD`, включая `36ba3758`).
Режим: только чтение. Компиляция, тесты, бенчмарки, скрипты НЕ запускались. Ir-числа ниже либо взяты из закоммиченных логов (файл:строка), либо помечены «оценка по коду, не измерение».

## §0 Чек-лист хода

- [x] Шаг 0: база = 31db7f17
- [x] Чтение карточек OPEN_ITEMS / отчётов R17 (oxx performance, xxs P3, RESOLVED 180/181)
- [x] Чтение кода: routing.rs, dealloc_own_base.rs, canonical_block_of, canonical_base_of_mut, BlockKind, Layout
- [x] `git show 36ba3758`: до/после
- [x] Сверка Ir по логам (46,3→70,7; 72,0→96,0) + дополнительная пара n9−n8 (одиночный дешёвый push)
- [x] Подсчёт инструкций до/после (оценка по коду) + статическая атрибуция окна
- [x] Инварианты повторного разрешения (провенанс, кто от него зависит)
- [x] Варианты + рекомендация
- [x] План реализации, гейт, тесты, коммиты
- [x] §6/§7

## §1 Постановка

### 1.1 Что не исправлено (текущий код на 31db7f17)

Горячий путь own-thread free малого блока под `production`
(`production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit"]`, `Cargo.toml:419`; ни `hardened`, ни `medium-classes`):

1. `HeapCore::dealloc` — `src/registry/heap_core/free/dealloc.rs:219–224`: проверка null → `dealloc_routing`.
2. `HeapCore::dealloc_routing` — `src/registry/heap_core_xthread/routing.rs:10–17`: **первое** разрешение `self.core.canonical_block_of(ptr)` (`:11`) → `dealloc_own_thread_with_base(block, layout, base)` (`:13`).
3. `AllocCore::canonical_block_of` — `src/alloc_core/alloc_core/mem/mem_impl.rs:34–42`: null-check, `segment_base_of_ptr`, `table.canonical_base_of_mut(candidate)`, `block = Node::deref(base, ptr.addr() - base.addr())`. Блок получает провенанс **сохранённого корня**, от `ptr` берётся только адрес.
4. `SegmentTable::canonical_base_of_mut` — `src/alloc_core/segment/segment_table/segment_table_impl.rs:743–758`: индекс кэша, сравнение `own_cache_keys[i]`, загрузка `own_cache[i]`, null-test; под `bench-internals` — `CONTAINS_BASE_TIER1_HITS.fetch_add` (`:746–748`).
5. `HeapCore::dealloc_own_thread_with_base` — `src/registry/heap_core/free/dealloc_own_base.rs:344–355`: **второе** разрешение `self.core.canonical_block_of(ptr)` (`:350`) + `if root.addr() != base.addr() { return }` (`:353–355`). Далее `class_for` (`:372`), `count` (`:373`), `small_free_guard(root, ptr, c, layout)` (`:387`), `FREE_PARK_CAP[c]` (`:411–412`), `mark_magazine` + запись слота + `count` (`:422–424`).
6. `small_free_guard` — `dealloc_own_base.rs:189–304`, ветка production:
   - `let kind = BlockKind::of(base, Some(c))` (`:198`) — загрузка байта `kind` заголовка (`segment_header_views.rs:11–46`);
   - Ph3b pre-route `if matches!(&kind, BlockKind::Large) { RouteToLargeFree }` (`:215–229`);
   - `off = ptr - base` (`:283`);
   - **добавлено 36ba3758:** `payload_start = Primordial ? primordial_meta_end() : small_meta_end()` и `if off < payload_start { RejectNoOp }` (`:285–292`);
   - `is_in_magazine(off)` (`:293`);
   - `off >= bump_of()` (`:296`) — **36ba3758 снял `#[cfg(feature = "alloc-decommit")]`**, но для `production` это no-op (фича входит в `production`);
   - `alloc_bitmap().is_free(off)` (`:299`).

Обе границы — константы времени компиляции: `small_meta_end()` (`segment_header_layout.rs:70–72`), `primordial_meta_end()` (`:126–128`), `primordial_meta_end() > small_meta_end()` (реестр первичного сегмента начинается в `small_meta_end()`, `:78–80`).

### 1.2 Что известно и НЕ переоткрывается

- R17-PRF-02: P3, PARTIALLY VALID/NARROWED (`docs/reviews/2026-10-08-src-review-xxs-round-17-P3.md:18–23`). Атрибуция «≈11 Ir из +24,4» — ГИПОТЕЗА (`…-oxx-round-17-performance.md:221–225`).
- R17-SEC-01 (item 180) и R17-CQ-02 (item 181) — CLOSED (`docs/correctness-open-items/RESOLVED.md:61–92`). Защита нижней и верхней границы обязательна; вопрос только в её цене. Отказ от проверки не рассматривается.
- Perf item 1 (`docs/perf/OPEN_ITEMS.md:122–130`): Tier-1 hit ≈ 8,2 Ir, Tier-2 ≈ 12,0 Ir (`:128`). Регион overflow/`flush_class` признан исчерпанным — не трогается.
- Perf item 56 CLOSED (`OPEN_ITEMS.md:2312–2326`): +3 Ir на каждый вызов `contains_base` — артефакт счётчиков `bench-internals`, в чистом production их нет.
- G1 (`OPEN_ITEMS.md:1434–1453`) и F1b/item 45: слияние двух битмапов отвергнуто (семантика / измеренная цена). Вариант D ниже (сентинельные биты) — **другой** механизм, но он тоже меняет семантику битмапа магазина, поэтому сюда относится то же требование аудита.
- `b707d196` (round 16 perf 84) уже убрал разрешение через таблицу в overflow-цикле (`dealloc_own_base.rs:486–498`, `debug_assert` вместо lookup). Overflow-ветка здесь не рассматривается.

### 1.3 Логи (только закоммиченные)

| Метрика | 09-22 (1) | 10-02 B=`7d80d7e1` (2) | 10-05 C=`fd954856` (3) | 10-08 A=`265a571a` (4) |
|---|---|---|---|---|
| `dealloc_prealloc_only_16b` | 8 527 (1:42) | 74 148 (2:117) | 64 265 (3:117) | 63 472 (4:24) |
| `dealloc_free_only_16b_n16` | 9 267 (1:133) | 75 196 (2:208) | 65 389 (3:208) | 64 603 (4:52) |
| → Ir/free, (n16−pre)/16 | 46,3 | 65,5 | 70,3 | 70,7 |
| `dealloc_free_only_16b_n8` / `_n9` | 8 907 / 8 952 | 74 676 / 74 741 | 64 829 / 64 899 | 64 043 / 64 113 |
| → **один дешёвый push при cnt=8, n9−n8** | **45** | **65** | **70** | **70** |
| `small_churn_16b` / `_2n` | 9 469 / 14 077 (1:28,35) | 60 629 / 67 285 | 58 346 / 65 118 | 57 518 / 63 662 (4:10,17) |
| → Ir/пара, (2N−N)/64 | 72,0 | 104,0 | 105,8 | 96,0 |

Источники: (1) `docs/perf/_raw_task1999_alloc_zeroed_stub_gap_after_1982.log`, (2) `docs/perf/_raw_ph6b_iai_base_run1.log`, (3) `docs/perf/_raw_ph6b_iai_cand_run1.log`, (4) `docs/perf/_raw_r16_perf84_A1.log`. Пары n8/n9 найдены grep по тем же файлам; в (4) их дают `_raw_r16_perf84_A1.log`, `_A2.log` и `_B1.log` (одинаковые 64 043 / 64 113 — детерминизм iai). Фичи всех прогонов: `production bench-internals internals` (`…-oxx-round-17-performance.md:598`).

Арифметика сверена: (9 267−8 527)/16 = 46,25; (64 603−63 472)/16 = 70,69; (14 077−9 469)/64 = 72,0; (63 662−57 518)/64 = 96,0. Совпадает с oxx-отчётом.

Новое по сравнению с oxx: пара **n9−n8** изолирует один не-overflow push без усреднения по overflow. Дешёвый push: 45 → 65 (+20, окно 09-22..B) → 70 (+5, окно B..C) → 70. После `265a571a` закоммиченных iai-логов нет (`git log 265a571a..HEAD -- docs/perf/_raw_*` показывает только R35-логи сайдкар-скана). **Цена 36ba3758 не измерена нигде.**

## §2 Анализ механизма

### 2.1 Откуда второе разрешение (НАБЛЮДЕНО, git)

- `2da9a941` (2026-09-29, «keep HeapCore own-path pointers allocator-derived») добавил `canonical_block_of` в начало `dealloc_own_thread_with_base`. В тот момент `dealloc_routing` передавал **числовой** `base = os::segment_base_of_ptr(ptr)` и сырой `ptr` (`git show 2da9a941:src/registry/heap_core_xthread/routing.rs`, строки 41, 61, 65). Второе разрешение тогда было **единственным** источником провенанса корня — не дублем.
- `a4245965` (2026-09-30, cutover) переписал `routing.rs`: теперь он сам вызывает `canonical_block_of` и передаёт `(block, base)` уже из таблицы. С этого момента разрешение в теле стало повторным. Внутреннее никто не убрал.

### 2.2 Кто зависит от второго разрешения (вопрос 3)

Вызовы `dealloc_own_thread_with_base` (grep по `src/`):

| Вызов | Что передаёт | Даёт ли второе разрешение что-то новое |
|---|---|---|
| `routing.rs:13` | `(block, base)` из `canonical_block_of` на `:11`, тот же `&mut self`, между вызовами нет мутаций таблицы | **Нет.** Ключ `segment_base_of_ptr(block) == base.addr()` → Tier-1 hit → `own_cache[idx]`, тот же сохранённый указатель (слот только что заполнен/подтверждён). `root == base` по адресу и по провенансу; проверка `:353` всегда ложна. |
| `realloc.rs:404` (move-leg) | `(block, base)` из `canonical_block_of` на `:143`; между ними `self.alloc` | Формально проверяет, что корень не исчез во время `self.alloc`. Но (а) live-block аргумент в комментарии `realloc.rs:~330–392`: сегмент с живым блоком не может быть unregister; (б) **до** free на `:404` выполняется `Node::copy_nonoverlapping(block, new_ptr, copy)` (`realloc.rs:348`): если бы корень был снят/размаплен, сбой случился бы раньше. Защитной ценности нет. |
| `realloc.rs:653` (`try_promote_to_large`) | `(ptr=block, base)` от вызова на `:309`, который передаёт `(base, block)` из `:143` | То же, что строка выше; копирование на `:597` предшествует free. |
| `diag_probes.rs:258` (`dbg_dealloc_own_thread_with_base`, `unsafe fn`, `bench-internals`) | `base` от вызывающего; бенч передаёт **числовой** `dbg_segment_base_of_ptr(ptr)` (`benches/perf_gate_iai.rs:829–834`) | **Да.** Здесь второе разрешение — единственный источник провенанса корня и единственная проверка «base наш». Убирать нельзя: разрешение надо перенести внутрь хука. |

Выведено: в production-путях второе разрешение — чистый no-op. Единственный реальный потребитель — `dbg_`-хук. Неявной проверкой оно служит только ему.

Провенанс: `canonical_block_of` строит `block` от сохранённого корня (`mem_impl.rs:40`), `canonical_base_of_mut` возвращает только сохранённые указатели (`segment_table_impl.rs:749,754–757`; инвариант описан на `:792–799`). Повторный вызов на `block` вернёт тот же корень. Новой провенанс-информации он не даёт.

Побочный эффект второго вызова: лишний инкремент `CONTAINS_BASE_TIER1_HITS` под `bench-internals`. `tests/segment_table_contains_base_tier1_counters.rs:88–113` проверяет «ровно один lookup на dealloc» только для `AllocCore::dealloc`, а не для `HeapCore`. Удаление второго вызова этот тест не затронет.

### 2.3 Пошаговая стоимость дешёвого push (оценка по коду, не измерение; x86-64, release, всё inline)

| Шаг | Инструкции (оценка) | Обращения к памяти |
|---|---|---|
| `dealloc` null-check | 2 | — |
| Разрешение №1: `segment_base_of_ptr`, индекс кэша, ключ, сравнение, корень, null-test, `block = root + (ptr − root)` | 10–13 (+1 `lock xadd` под `bench-internals`) | 2 загрузки (ключ, корень) |
| **Разрешение №2 + `root != base`** | **12–16 (+1 под `bench-internals`)** | **2 загрузки** |
| `class_for(size.max(16), align)` | ≈6–10 | 1 (таблица классов) |
| `tcache.classes[c].count` | 1–2 | 1 |
| `kind` (Ph3b) + сравнение с Large | 3 | 1 (байт заголовка) |
| **`payload_start` (выбор) + `off < payload_start` — 36ba3758** | **4–6** | **0** (константы) |
| `is_in_magazine(off)` | ≈5–6 | 1 |
| `off >= bump` | 3 | 1 (слово заголовка) |
| `is_free(off)` | ≈4–5 (индекс общий) | 1 |
| `FREE_PARK_CAP[c]` + сравнение | 2–3 | 1 |
| `mark_magazine` + запись слота + `count` | ≈6–8 | 1 RMW + 2 записи |

**До/после 36ba3758 (production, оценка):** +4…6 Ir на каждый не отклонённый small free (выбор константы через `cmov`/ветку + сравнение + переход). Новых обращений к памяти нет: байт `kind` уже читался с Ph3b (`fa88c95f`), а коммит только поднял `BlockKind::of` наверх (дифф `:198`, `:227`). Проверка `bump` в `production` была и раньше — 0 Ir. Ожидаемое (не измеренное) значение дешёвого push на HEAD: ≈70 + 4…6 ≈ 74–76 Ir при фичах судьи.

**Второе разрешение:** ≈12–16 Ir + 1 атомарный RMW под `bench-internals`. Для сравнения, item 1 даёт Tier-1 ≈ 8,2 Ir с счётчиком (`OPEN_ITEMS.md:128`), измерено ДО `66c2e434`. Этот коммит добавил отдельный массив `own_cache_keys` и null-test, т.е. ещё ≈2 Ir (оценка). Плюс `root != base` и `deref` ≈ 3–4 Ir.

### 2.4 Статическая атрибуция окна +25 Ir на дешёвом push (ГИПОТЕЗА, оценка по коду)

Окно 09-22 → B (+20): по `git log 1f4ac4f3..7d80d7e1` в нём лежат `2da9a941`, `66c2e434`, `c5f59f07`, `a4245965`, `d61d6d21`.

| Коммит | Добавленная работа на дешёвом push | Оценка, Ir |
|---|---|---|
| `2da9a941` | второе разрешение + `root != base` + `deref` (+ счётчик `bench-internals`) | 11–15 |
| `66c2e434` | Tier-1 сравнивает отдельный ключ и проверяет null (в разрешении №1) | ≈2 |
| `a4245965` / `c5f59f07` | routing вычисляет `block` от корня (sub+add) | ≈2 |
| `d61d6d21` (R1-01) | `cap = FREE_PARK_CAP[c]` (загрузка вместо константы `TCACHE_CAP`) | ≈1 |
| **Сумма** | | **≈16–20 из +20** |

Окно B → C (+5): из free-путевых коммитов здесь только `fa88c95f` (Ph3b, 2026-10-02, не предок `7d80d7e1`): загрузка байта `kind` + сопоставление + сравнение с Large в production ≈ 3–5 Ir. Совпадает с +5.

Вывод (ГИПОТЕЗА, проверяется bisect по n9−n8): прирост дешёвого push с 45 до 70 почти целиком объясняется пятью-шестью коммитами. Три из них — обязательные защиты (Ph3b Large-маршрут, R1-01 бюджет, провенанс корня в routing). Единственная **устранимая** часть — второе разрешение, ≈11–15 Ir под судьёй и ≈8–12 Ir в чистом production (без счётчика). Порог oxx «free ≤ ~50 Ir» (`…-oxx-round-17-performance.md:234`) этим **недостижим**. Исторические 46,3 Ir были измерены без Ph3b, R1-01 и SEC-01. Гейт должен быть A/B относительно HEAD, а не относительно 09-22.

### 2.5 Попутная находка (НОВАЯ, P4, SOURCE-CONFIRMED чтением): `small_free_guard` принимает `BlockKind::Unknown` как Small

- `BlockKind::Unknown` документирован так: «No free path may act on a block whose physical kind it cannot trust» (`src/alloc_core/segment/segment_header/block_kind.rs:39–42`). `tests/kind_at_strict_decode.rs:20–26` описывает тот же контракт: Unknown уходит в «not this kind» no-op на каждом вызове.
- `AllocCore::dealloc_at_base` это соблюдает: `SegmentKind::Unknown => {}` (`mem_impl.rs:540`), `BlockKind::Large | BlockKind::Unknown => {}` (`:525`).
- `small_free_guard` проверяет только `Large` (`:227`, `:236`, `:262`) и `Primordial` (`:285`). При `Unknown` (повреждённый байт `kind` ≥ 3) он берёт `small_meta_end()`, проходит M2-проверки и может вернуть `Accept`, т.е. блок попадает в магазин. Касается всех сборок с `fastbin`, включая `production` и `dealloc_batch_small`.
- Почему P4: нужна предшествующая порча заголовка. Чтения остаются внутри собственного (канонического) сегмента. Конкретный сбой не установлен; это расхождение с документированным контрактом защиты.
- Исправление встраивается в рекомендацию (§4, коммит 2) без цены на горячем пути: отказ для Unknown уходит в холодную ветку той же проверки `kind >= 2`.

## §3 Варианты решения

### 3.1 (a) Повторное разрешение корня

| Вариант | Механизм | Аргумент корректности | Выигрыш / цена (оценка) | Риск | Что доказать/измерить |
|---|---|---|---|---|---|
| **A0. Ничего не менять / только документация** | Описать дубль как цену защиты | Корректно сейчас | 0 / ≈11–15 Ir на каждый own free под судьёй | Нет | — |
| **A1. Сырые параметры `(block, root)`, сделать `dealloc_own_thread_with_base` `unsafe fn`** | Убрать `:350–355`, переименовать `base`→`root`. Контракт `# Safety`: «`root`/`block` — результат `canonical_block_of` в этом же `&mut self`-окне». `debug_assert_eq!(self.core.canonical_root_for(block), Some(root))` | Провенанс даёт сам вызывающий (все три prod-вызова берут пару из `canonical_block_of`). `canonical_root_for` — `&self`, без заполнения кэша, поэтому debug и release не расходятся по состоянию кэша | −11…−15 Ir под судьёй, −8…−12 в production / новые `unsafe`-блоки в 3 местах (routing уже `allow(unsafe_code)`, realloc — 2 новых tier-2 сайта) | Растёт инвентарь `unsafe` (README §«Where unsafe lives»). Сигнатура остаётся «сырой указатель → метаданные» | Провенанс-тесты зелёные, Miri на own-free |
| **A2. Типизированный свидетель `OwnedBlock { root, block }`** (приватные поля; создаётся только `AllocCore::owned_block_of(&mut self, ptr) -> Option<OwnedBlock>`, тонкая обёртка над `canonical_block_of`) | `dealloc_own_thread_with_base(&mut self, owned: OwnedBlock, layout)` остаётся безопасной `fn`; `dbg_`-хук сам вызывает `owned_block_of` и сравнивает адрес с переданным `base` (семантика хука сохраняется) | Инвариант «корень из таблицы, блок от корня» держится на типе: внутри крейта его нельзя подделать без конструктора. Тот же live-block аргумент для realloc, что и сейчас (свидетель — доказательство на момент создания; как и кортеж сегодня, он не привязан к времени жизни, иначе нельзя пережить `self.alloc`) | То же −11…−15 Ir; два указателя в регистрах — 0 Ir / +1 файл (одно-экспортный), правка 4 вызовов + сигнатуры `try_promote_to_large` | Низкий: изменение типов, поведение то же | Те же тесты; новый счётчик-оракул (§5) |
| **A3. Передавать вниз только `root`, `block` пересчитывать** | `block = root + (ptr − root)` в теле | Как A2, но с повтором арифметики | ≈−10…−13 Ir | Как A1 (сырой указатель) | — |

### 3.2 (b) Цена защиты нижней/верхней границы (36ba3758)

| Вариант | Механизм | Аргумент корректности | Выигрыш / цена (оценка) | Риск | Что доказать |
|---|---|---|---|---|---|
| **B0. Оставить как есть** | `cmov`-выбор + два сравнения | Закрыто items 180/181 | 0 / текущие 4–6 Ir | Unknown проходит (§2.5) | — |
| **B1. Разделённое сравнение + Unknown в холодной ветке** | `if off < SMALL_META_END { reject }` (сравнение с непосредственной константой). Затем `if kind_byte == PRIMORDIAL && off < PRIMORDIAL_META_END { reject }`. В production pre-route заменить `kind == Large` на `kind >= 2` с холодным разбором `Large → Route`, иначе `RejectNoOp`; в hardened/promotion-сборках — явный `Unknown → RejectNoOp` | Отклоняемое множество для Small/Primordial то же самое (`S ≤ P`). Unknown теперь отклоняется (ужесточение, совпадает с документированным контрактом) | Small-сегмент: ≈−1…−2 Ir относительно `cmov`-формы (нет загрузки двух констант и `cmov`); Unknown — 0 Ir на горячем пути (ветка `>= 2` заменяет `== 2`) | Низкий; поведение меняется только для повреждённого `kind` | Мутанты §5.3; кодоген может и сам свернуть `cmov`, тогда выигрыш ≈0 |
| **B2. Одна диапазонная проверка** `off.wrapping_sub(ps) < bump.wrapping_sub(ps)` | Объединяет `off < ps` и `off >= bump` | Требует инварианта `bump ≥ ps(kind)`. По коду он держится: bump стартует с `meta_end` (`bootstrap.rs:285`, `reserve.rs:259–265`), только растёт (`alloc_core_small_impl.rs:775,954`), сбрасывается в `small_meta_end()` и только для Small (`decommit.rs:144–148,210`) | ≈0 Ir: 2 `sub` + `cmp` + `jcc` против 2×(`cmp`+`jcc`); минус одна ветка | Новое доказательство инварианта ради ≈0 Ir. Если инвариант нарушен (bump < ps), проверка **пропускает всё**, что хуже текущей | Инвариант по всем писателям `bump` |
| **B3. `payload_start` из заголовка** (новое поле) | Одна загрузка вместо выбора | Корректно | ≈−1 Ir / меняется раскладка `SegmentHeader`, все конструкторы, layout-asserts | Средний (раскладка) | layout-тесты |
| **D. Сентинельные биты магазина для метаданных** | При инициализации/сбросе сегмента выставить в `MagazineBitmap` биты `[0, payload_start)`; тогда `is_in_magazine` сам отклоняет метаданные, явное `off < ps` убирается | Корректно, только если ВСЕ места инициализации ставят биты: bootstrap primordial, virgin reserve (сейчас полагается на нули mmap, init пропускается под `not(miri)` — `alloc_bitmap.rs:89–97`), повторное использование из пула, сброс при decommit. И если ни один потребитель не трактует бит иначе, чем «отклонить» | −4…−6 Ir (оценка) / ≈ `meta_end/128` байт записи на инициализацию сегмента (сотни байт, первая страница битмапа и так затрагивается первыми free) | **Высокий**: семантический инвариант распределён по 4+ местам; область рядом с отказами G1/F1b; ошибка = молчаливая потеря защиты SEC-01 | Аудит всех `init_in_place`/сбросов; новые тесты на каждый путь (virgin/pool/decommit/primordial) |
| **E. Убрать проверку** | — | Нарушает CLOSED 180 | — | Отклонено правилами | — |

Порядок проверок: в принимающем (горячем) пути выполняются **все** предикаты `small_free_guard`, и все отказы — no-op `RejectNoOp` без побочных эффектов. Поэтому перестановка по частоте отказов на Ir горячего пути не влияет. Она меняет только стоимость редкого отказа. Жёстко зафиксирован лишь порядок «магазин раньше `is_free`» (док `:174–177`). Это не рычаг.

## §4 РЕКОМЕНДАЦИЯ

**Выбрано: A2 (типизированный свидетель `OwnedBlock`, второе разрешение убрано из тела и перенесено в `dbg_`-хук) + B1 (разделённая нижняя граница, Unknown → отказ в холодной ветке).** Вариант D — нет; решение о нём остаётся владельцу (§6), только если после A2+B1 гейт покажет, что оставшиеся ≈4 Ir важны.

Почему так:
- A2 даёт тот же выигрыш, что A1, но не добавляет `unsafe`. Инвариант «корень — из таблицы» сейчас проверяется в рантайме, а здесь его гарантирует тип. Для крейта с `#![forbid(unsafe_code)]` в верхнем слое и инвентарём `unsafe` в README это сильнее, чем A1: функция, которая читает метаданные по сырому указателю от вызывающего, остаётся безопасной только потому, что свидетеля нельзя подделать.
- A0 отвергнут: в production повторное разрешение — доказанный no-op (§2.2), то есть цена без защиты.
- A3 отвергнут: тот же риск сырого указателя, что у A1, а выигрыш меньше.
- B1 выбран вместо B0: при цене ≈0…−2 Ir он закрывает P4-расхождение §2.5.
- B2 отвергнут: ≈0 Ir, но требует нового доказательства инварианта, а при его нарушении ослабляет защиту.
- B3 отвергнут: меняется раскладка заголовка ради ≈1 Ir.
- D отложен: ≈4–6 Ir против распределённого семантического инварианта рядом с отказами G1/F1b.

Ожидание (оценка по коду, не измерение): дешёвый push 74–76 → ≈60–64 Ir под судьёй (`production bench-internals internals`), churn-пара ≈ −12…−16 Ir. Порог oxx «≤ 50» не достигается и не должен служить критерием (§2.4).

Остаётся недоказанным без исполнения:
- (1) реальная цена 36ba3758 и реальный выигрыш A2/B1 в Ir;
- (2) свёл ли LLVM уже часть дубля (например, второй null-check);
- (3) что wall-clock следует за Ir;
- (4) что в чистом `production` без счётчиков выигрыш ≈ на 3 Ir меньше (вывод из item 56, не измерено на этом пути).

## §5 План реализации

### 5.1 Код (эскиз)

Новый файл `src/alloc_core/alloc_core/mem/owned_block.rs` (один экспорт), реэкспорт в `mem/mod.rs` (только `mod`/`pub(crate) use`):

```text
/// Allocator-derived own-segment witness: `root` is the table's stored
/// reservation root, `block` is derived from it. Constructed only by
/// `AllocCore::owned_block_of`.
#[derive(Clone, Copy)]
pub(crate) struct OwnedBlock { root: *mut u8, block: *mut u8 }
impl OwnedBlock {
    #[inline(always)] pub(crate) fn root(self) -> *mut u8 { self.root }
    #[inline(always)] pub(crate) fn block(self) -> *mut u8 { self.block }
    #[inline(always)] pub(super) fn new(root: *mut u8, block: *mut u8) -> Self { .. } // only mem_impl
}
```

`mem_impl.rs` — рядом с `canonical_block_of`:

```text
#[inline(always)]
pub(crate) fn owned_block_of(&mut self, ptr: *mut u8) -> Option<OwnedBlock> {
    self.canonical_block_of(ptr).map(|(r, b)| OwnedBlock::new(r, b))
}
```

Видимость конструктора: `pub(super)` в модуле `mem` (или `pub(in crate::alloc_core::alloc_core::mem)`), чтобы `registry` не мог его вызвать.

`routing.rs:11–13`:

```text
-        if let Some((base, block)) = self.core.canonical_block_of(ptr) {
+        if let Some(owned) = self.core.owned_block_of(ptr) {
             #[cfg(feature = "fastbin")]
-            self.dealloc_own_thread_with_base(block, layout, base);
+            self.dealloc_own_thread_with_base(owned, layout);
             #[cfg(not(feature = "fastbin"))]
-            self.dealloc_own_thread(block, layout, base);
+            self.dealloc_own_thread(owned.block(), layout, owned.root());
```

`dealloc_own_base.rs:344–355`:

```text
-    pub(crate) fn dealloc_own_thread_with_base(&mut self, ptr: *mut u8, layout: Layout, base: *mut u8) {
-        let Some((root, block)) = self.core.canonical_block_of(ptr) else { return; };
-        if root.addr() != base.addr() { return; }
+    pub(crate) fn dealloc_own_thread_with_base(&mut self, owned: OwnedBlock, layout: Layout) {
+        let (root, block) = (owned.root(), owned.block());
+        debug_assert_eq!(self.core.canonical_root_for(block), Some(root)); // read-only, no cache fill
```

Ниже `ptr` заменяется на `block` в вызове `small_free_guard(root, block, c, layout)`. Сегодня там `ptr`, но после первого разрешения `ptr == block` по адресу, а `small_free_guard` использует только адрес (`:272`, `:283`). Это делает провенанс единообразным.

`realloc.rs:143,148,309,404,545–557,653`: `own_block = self.core.owned_block_of(ptr)`; `try_promote_to_large(owned, old_layout, new_size)`; `dealloc_own_thread_with_base(owned, old_layout)`. Прочие использования `base`/`block` в realloc берут `owned.root()`/`owned.block()`.

`diag_probes.rs:249–259` (хук остаётся `unsafe fn` с `bench-internals` и прежней семантикой):

```text
let Some(owned) = self.core.owned_block_of(ptr) else { return };
if owned.root().addr() != base.addr() { return; }
self.dealloc_own_thread_with_base(owned, layout);
```

`small_free_guard` (B1), production-ветка:

```text
-    if matches!(&kind, BlockKind::Large) { return SmallFreeGuard::RouteToLargeFree; }
+    if matches!(&kind, BlockKind::Large | BlockKind::Unknown) {   // one >=2 test on the raw byte
+        return if matches!(&kind, BlockKind::Large) { RouteToLargeFree } else { RejectNoOp };
+    }
```

В F7-ветках (A)/(B) после Large-проверки — `if matches!(&kind, BlockKind::Unknown) { return RejectNoOp; }`. Это hardened/promotion-сборки, не production.

Нижняя граница:

```text
-    let payload_start = if Primordial { P } else { S };
-    if (off as usize) < payload_start { return RejectNoOp; }
+    if (off as usize) < SegmentLayout::small_meta_end() { return RejectNoOp; }
+    if matches!(&kind, BlockKind::Primordial) && (off as usize) < SegmentLayout::primordial_meta_end() {
+        return RejectNoOp;
+    }
```

Док-комментарии `:107–185` и `:331–341` (Э9-текст о `contains_base`/`segment_base_of_ptr` устарел) обновить. Комментарии лаконичные, без doctest.

Не входит в объём (упомянуто для полноты): `RouteToLargeFree` и хвост «non-small» вызывают `self.core.dealloc(block, layout)`, а он разрешает корень **третий** раз (`mem_impl.rs:230`). Это Large-путь, не горячий small. Можно открыть отдельно через `pub(crate) unsafe fn dealloc_at_base` (`mem_impl.rs:253–255`). Не смешивать с этим гейтом.

### 5.2 Тесты (только `tests/`, без doctest)

1. **Новый** `tests/heapcore_own_free_single_canonical_lookup.rs` (`#![cfg(all(feature = "internals", feature = "bench-internals", feature = "fastbin"))]`): аналог `segment_table_contains_base_tier1_counters.rs:59–114`, но через `HeapCore::dealloc` (lease `HeapRegistry::dbg_claim_lease`). Оракул: `hits_delta + misses_delta == N` на N own-free. **Красный до** (2N из-за второго разрешения), **зелёный после**. Это и path-activation-оракул гейта. Сериализовать через общий lock (счётчики глобальные).
2. **Новый** `tests/regression_magazine_free_unknown_kind.rs`: выделить 16 B в lease-heap, повредить байт `kind` значением `0x99`, `heap.dealloc(p)` → `dbg_tcache_count(0) == 0`; восстановить байт, освободить корректно. Нужен `HeapCore`-форвардер `dbg_stamp_kind_byte`. Он трогает метаданные по сырому указателю, поэтому по правилу CLAUDE.md: `pub unsafe fn`, `# Safety`, гейт `bench-internals` (+`internals`), запись в `UNSAFE_HOOKS`/`BENCH_UNSAFE_HOOKS` `scripts/verify-dbg-hook-safety.mjs`. Альтернатива без нового хука — если у lease есть доступ к `AllocCore` (не найден при чтении; проверить). **Красный до** (Accept → count 1), зелёный после.
3. Должны остаться зелёными **без изменений**:
   - `tests/regression_magazine_free_bounds.rs` (оба теста; 16 B — Primordial или Small в зависимости от сегмента anchor, `SMALL_MAX` — Small);
   - `tests/regression_dealloc_metadata_region_guard.rs` (слой `AllocCore`, этим изменением не затронут; контроль, что sibling-guard цел);
   - `tests/r6_heapcore_provenance.rs` (из `2da9a941`);
   - `tests/r6_canonical_root_cache.rs`, `tests/segment_table_contains_base_tier1_counters.rs`;
   - `tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs`, `tests/regression_dealloc_batch_promoted_large_free.rs`, `tests/r11_4_dealloc_batch_hardened_guards.rs`, `tests/regression_hardened_large_kind_own_free.rs`, `tests/regression_hardened_interior_ptr.rs`;
   - `tests/kind_at_strict_decode.rs`, `tests/r11_ph3b_kind_aware_entries.rs`, `tests/regression_r1_01_magazine_free_budget.rs`.
4. Miri: `r6_heapcore_provenance` и новый тест 1 (если гейты позволяют под Miri) — доказать, что провенанс-свидетель не ослабил Stacked/Tree Borrows.
5. Мутанты (каждый должен дать красный; нумерация M — мутантов, не путать с M2-оракулами guard):
   - M1 — убрать `off < small_meta_end` → `regression_magazine_free_bounds::small_…` красный (metadata free входит в магазин);
   - M2 — убрать Primordial-ветку → красный только если anchor первого теста лежит в Primordial. **Не проверено без исполнения.** Рекомендуется добавить в тест `assert_eq!(heap.dbg_kind_at_tag(anchor), 0)` или отдельный Primordial-вариант, иначе M2 может выжить;
   - M3 — `<` → `<=` → красный «payload boundary free rejected»;
   - M4 — убрать Unknown-отказ → красный тест 2;
   - M5 — вернуть второе разрешение → красный тест 1;
   - M6 — в `dbg_`-хуке убрать `owned_block_of` и брать `base` как корень → должен ловить `regression_r2_05_diag_provenance_miri` / Miri (проверить, покрывает ли он этот хук; если нет — отметить как непокрытый).

### 5.3 Предрегистрация гейта (по CLAUDE.md)

- **Entry point:** `HeapCore::dealloc` → `dealloc_routing` → `dealloc_own_thread_with_base` — тот же слой, что и `#[global_allocator]` для own-thread free (`SeferAlloc` проксирует в `HeapCore::dealloc`). Фикстуры `benches/perf_gate_iai.rs` вызывают `HeapCore` напрямую (`:301–309`, `:950–969`). Почему этот слой: изменение живёт в `HeapCore`, а `AllocCore::dealloc` не затронут. Вспомогательный wall-clock (необязательный) — через `#[global_allocator]` (`npm run bench:table`, ячейки `global_alloc_churn/*/16B`).
- **Фичи судьи:** `production bench-internals internals` (как в логах 1–4). Отдельно указать, что счётчик даёт +1 RMW на разрешение. Вторая ось — чистый `production` (без `bench-internals`) через wall-clock или callgrind без счётчиков, если владелец потребует чистое production-число.
- **Источник:** временный measurement-commit SHA или `git write-tree` ДО измерения (правило R29-6, п.7).
- **Плечи:** A = 31db7f17 (HEAD), B = A + коммит 1 (A2), C = B + коммит 2 (B1). Каждый — свой бинарь, iai детерминирован; A/A повтор каждого плеча дважды (ожидается Ir-идентичность, как `_raw_r16_perf84_A1/A2`).
- **Основные метрики (Ir):**
  - push = `dealloc_free_only_16b_n9 − _n8`;
  - free-avg = `(_n16 − dealloc_prealloc_only_16b)/16`;
  - churn = `(small_churn_16b_2n − small_churn_16b)/64`;
  - тело = `(dealloc_own_thread_body_only_16b − dealloc_segment_base_of_ptr_probe_only_16b)/64`. **Внимание:** после A2 хук сам делает одно разрешение, так что метрика тела остаётся сопоставимой с A (тоже одно разрешение внутри). Ожидаемая дельта ≈0 — это контроль, что тело не изменилось иначе.
- **Пороги (предрегистрация):**
  - B−A: push ≤ −8 Ir **и** churn ≤ −8 Ir/пару (нижняя граница оценки −11 минус запас);
  - C−B: push в [−3, +1] Ir (B1 не должен дорожать);
  - kill-gate: каждый churn-бенч не хуже +10 raw Ir (X4-B, `docs/perf/IAI_BASELINE.md:827`);
  - `mimalloc_small_churn_16b` / `_2n` ΔIr = 0 (A/A-контроль среды, логи: 16 629 / 21 557 во всех 1–3);
  - `large_alloc_free_cycle` ΔIr в ±10 (Large-путь не должен меняться: `RouteToLargeFree` по-прежнему делает третье разрешение);
  - `alloc_magazine_hit_only_16b − _prefill_only_16b` в ±10 на 16 hit-ов.
- **Оракул активации (path-activation):**
  - на прогоне плеча под `bench-internals` записать `dbg_contains_base_tier1_hits + misses` за таймируемый цикл n16: A = 32, B/C = 16 (ровно одно разрешение на free);
  - `dbg_tcache_count(0)` после n16 = 16 (все 16 — push, ни одного отказа/overflow);
  - для C — счётчик отказов, если он заведён (иначе `tcache_count` достаточно).
- **Режим:** cost и benefit в одном режиме — однопоточный own-free в один Primordial/Small сегмент, Tier-1 hit. Регрессию на Tier-2-heavy (R32-10 thrash) это не покрывает. Контроль: `examples/r32_10_own_cache_tier1_thrash_gate.rs` — ожидание «не хуже» (одно разрешение вместо двух; при промахе Tier-1 второе было бы hit-ом, поэтому выигрыш там ≈ Tier-1, а не Tier-2).
- **Статистика wall-clock (если делается):** чередующиеся прогоны 20+20, paired t ≥ t_crit(df, α=0.05) плюс sign-тест, по образцу `OPEN_ITEMS.md:137` (t=8.114 ≫ crit 2.101, sign 19/20). Для Ir статистика не нужна (детерминизм), достаточно A/A-идентичности.
- **Артефакты:** сырые логи (< 200 KiB, force-add), `docs/perf/R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE.md` + `_summary.csv`. Таблицы и отношения выводит один скрипт с `assert` на арифметику. Каждый процент — с числителем и знаменателем.

### 5.4 Порядок коммитов (R30-12)

Отдельного коммита «только красные тесты» нет: между фазами дерево должно быть зелёным. Каждый тест входит в коммит со своим исправлением, а наблюдённый красный результат «до» (контрфакт) записывается в сообщение коммита и карточку, как в items 180/181.

1. `perf(runtime): resolve own-free segment root once via OwnedBlock witness (R17-PRF-02)` — A2 + тест 1 + отчёт гейта B−A + обновлённый `docs/perf/IAI_BASELINE.md` (обязательно: горячий путь `production` изменился) + `npm run iai`. `perf(runtime)` — потому что меняется горячий алгоритм в постоянной части `production`.
2. `fix(perf): reject Unknown segment kind and split payload lower bound in small_free_guard` — B1 + тест 2 (+ `HeapCore`-форвардер `dbg_stamp_kind_byte`, если нужен) + C−B в том же отчёте. `fix(perf)`, а не `perf(runtime)`: восстанавливается документированный инвариант (Unknown → отказ), а ускорение ≈0…2 Ir не заявляется.
3. Документы в тех же коммитах:
   - `docs/CORRECTNESS_OPEN_ITEMS.md`: новый item для §2.5 (P4), закрывается коммитом 2;
   - `docs/perf/OPEN_ITEMS.md`: карточка PRF-02 (её нет — завести в коммите 2 со Status/verdict);
   - `CHANGELOG.md`;
   - round-manifest.

## §6 Что нельзя решить без исполнения

1. **Цена 36ba3758 и выигрыш A2/B1.** Команда: `npm run iai` (или прямой `cargo bench --bench perf_gate_iai --features "production bench-internals internals"`) на A=31db7f17, на `36ba3758^` (= `54fc2a21`) и на B, C в отдельных worktree (`git worktree add ../sefer-alloc-<label> <sha>`). Измерить n8, n9, n16, prealloc, churn/2n, body/segbase, mimalloc, large_cycle. Решение по порогам §5.3.
2. **Bisect атрибуции (подтвердить §2.4).** Метрика n9−n8 по окну `1f4ac4f3..7d80d7e1` (269 коммитов, ≈8 шагов). Ожидаемые ступени: `2da9a941` (≈+11–15), `66c2e434` (≈+2), `a4245965` (≈+2), `d61d6d21` (≈+1). В окне `7d80d7e1..fd954856` — `fa88c95f` (≈+5). Если хоть одна ступень разойдётся с оценкой больше чем на 5 Ir, гипотеза §2.4 отвергнута.
3. **Кодоген B1.** Если LLVM уже превратил выбор `payload_start` в разделённое сравнение, C−B ≈ 0. Тогда в коммите 2 остаётся только Unknown-отказ (обоснован корректностью).
4. **Решения владельца:**
   - (а) Какой порог считать «закрытием» PRF-02: A/B-выигрыш от HEAD или исторические ≤ 50 Ir. Второе недостижимо без отказа от Ph3b/R1-01/SEC-01.
   - (б) Нужен ли вариант D (сентинельные биты, ≈4–6 Ir) после A2+B1. Он связан с G1/F1b и требует отдельной задачи с аудитом.
   - (в) Нужна ли ось чистого `production` без `bench-internals` (рекомендация (ii) item 56 по-прежнему не решена).
   - (г) Делать ли сразу третье разрешение на Large-пути (`dealloc_at_base`) — отдельный гейт.

## §7 Проверено и НЕ дало вывода

- Нет ни одного закоммиченного iai-лога после `265a571a` с free-фикстурами. Цена 36ba3758 по логам не восстановима.
- Не удалось статически установить, сворачивает ли LLVM второй `canonical_block_of` (ключ тот же, но между вызовами есть возможные записи через `&mut self`, и счётчик под `bench-internals` — атомарный RMW, барьер для свёртки). Измеренный +20 в окне `2da9a941` говорит, что не сворачивает, но это ГИПОТЕЗА до bisect.
- Не удалось определить по чтению, в каком сегменте (Primordial или Small) лежит первый 16-B anchor в `regression_magazine_free_bounds.rs:13–21`. От этого зависит, ловит ли тест мутант M2. Код теста обрабатывает оба случая и не фиксирует ни один.
- Не найден `HeapCore`-уровневый способ повредить байт `kind` (есть только `AllocCore::dbg_stamp_kind_byte`, `header_diag.rs:157`). Для теста 2 нужен новый форвардер или доступ к `AllocCore` из lease. Существование последнего не проверено.
- Окно B→C (+5 на push) по free-путевым файлам содержит только `fa88c95f` из кандидатов. Ph4b/Ph4c (`a0dc988d`, `5083e4bc`) меняли setup бенча (`claim_leaked_heap`); при вычитании prealloc это должно сокращаться, но по чтению не доказано.
