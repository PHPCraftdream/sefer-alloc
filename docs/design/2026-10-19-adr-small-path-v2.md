# ADR v2 (раунд 5, финальный): путь мелких блоков своего потока — У1 (усиление) + У2 (опция); этап B отклонён

Статус: ПРОЕКТ, раунд 5. Дата: 2026-10-19. База: `origin/main` `f93c1b52` (ветка `b4-adr-v2`).
Ревью раунда 4 (`adr_v2_review4_rush.md` — P2-1, P2-2; `adr_v2_review4_ox.md` — P2-1 + P3-1…10) —
оба FAIL без P1; все пункты устранены (§0). Измерения: `../b4-math/docs/perf/B4_MATH_HACKS.md`
(все 7 арифметических кандидатов NO-GO; база w_alloc 63 / w_free 115, larson 319 / mstress 494);
бюджетные поправки — `brainstorm_ox.md` §1.1/§1.2.

**Скоуп (сужен решением владельца, раунд 3):**
- **Этап 1 — У1** (`ClassLeaves` как авторитет начала/класса блока на free) — **УСИЛЕНИЕ**:
  production начинает отвергать aligned wrong-class и interior в невыданные гранулы (scalar+batch),
  невыровненные free — **в batch**; закрывается item 247(b). Прогноз **по режимам** (§2.1):
  *проба (граница `GlobalAlloc`, runtime-Layout) −1…−3 Ir/free; iai (константный Layout) ≈ +1…+5,
  центр ≈ +3* — цена усиления во встраиваемом режиме вынесена владельцу явно (§10.2).
- **Этап 2 — У2** (drain одним проходом, production) — **опция** после GO этапа 1 и решения
  владельца по §10.5; alloc ≈59–60 (снимаемая часть drain — ОЦЕНКА 3–4 Ir/op; видимый drain 7.7
  ИЗМЕРЕНО).
- **Этап B (M2, free-bin-cap) — исключён** (§4.4); Miri-спайк удалён вместе с ним.
- Этап D — закрыт как пустой (ИЗМЕРЕНО NO-GO, §4.6).

Числовая база — **ОДИН харнесс/база**: `_raw_b4_math_math-base.log` (alloc 63 / free 115). Налог
кадра — **15–17 Ir НА СТОРОНУ** (`brainstorm_ox.md` §1.1). iai и larson/mstress T=1 **инлайнят
аллокатор и кадр не видят** (`brainstorm_ox.md` §1.2). **Режимы метрик различаются** (ox P2-1
раунда 4): iai вызывает free с **константным** Layout ⇒ `class_for`/`DIVISIBILITY[c]` сворачиваются
в константы (для 16 B `divides` вырождается в `off & 15`); проба вызывает через `GlobalAlloc` со
**runtime-Layout** ⇒ кредит `divides` −7 действителен только там. Прогнозы и решающие пороги —
раздельно по режимам (§2.1, §9). Все цены — ИЗМЕРЕНО (со ссылкой) или ОЦЕНКА (±30–50 %). Межпоточный
протокол, сегменты, Large-путь не меняются, кроме одного изменения, вынесенного вопросом владельцу
(снятие Release-store повторной выдачи в У2 — §10.5).

## 0. Что изменено в раунде 5

| Finding (rush4 / ox4) | Где устранён / почему не согласен |
|---|---|
| rush P2-1 / ox P3-5 (фича кандидата в пробе/плечах) | §9: проба — `prof_ao_f*.sh`/`prof_f*.sh` с аргументом FEATURES (`prof_ao_f_win.sh:6` принимает FEAT), A=`production`, B=`production small-class-authority`; iai — per-side features: фича входит в `production` кандидата, на базе — заглушка через `baseSha` (механизм фичи оракула); обязанности записаны; `--oracle`-стадия — в списке работ этапа 1; свидетельство активации пробы — строки `leaf_classes.rs` в построчной разметке `w_free` (R30-8) |
| rush P2-2 / ox P3-6 (alloc-метрика) | §9: hit — пара (`alloc_magazine_prefill_only_16b` `:456`, `alloc_magazine_hit_only_16b` `:495`), `hit = (Ir(hit) − Ir(prefill))/16`; drain/refill — cold/recycle-пары с alloc-only-плечами (добавляются в объём этапа 2); этап 1 — \|Δ\| ≤ 0.5 Ir/op **на каждой** named alloc-производной; этап 2 — порог −3 на drain/refill-метрику; расширение runner вычисляет производные |
| ox P2-1 (режимы метрики / прогноз) | §2.1 — два прогноза по режимам; решающий режим строки 2 — **ячейки с `black_box(layout)`** (`dealloc_free_only_16b_rtl` + `dealloc_prealloc_only_16b_rtl`, + пара 1088; добавляются в `perf_gate_iai.rs` в объёме этапа 1); константные ячейки — сопутствующий гейт ≤ +5 Ir/free; обоснование «cost and benefit in the same regime»; §10.2 — цена усиления по обоим режимам; в iai линия `DIVISIBILITY` не читается (статьи «−1 линия» там нет) |
| ox P3-1 (невыровненные = усиление только batch) | Шапка, §4.5 категория 3, §10.2: усиление = {aligned wrong-class (scalar+batch), interior в нулевой лист (scalar+batch), невыровненный (только batch)}; scalar невыровненный — категории 1/2 по правилу первого старого предиката (item 209) |
| ox P3-2 (T8 без `start+8`) | §8 T8: кейс `start+8` для batch + мутант «выравнивание только в scalar-ветке» |
| ox P3-3 (мутант T6 — тождество) | §8 T6: мутант переформулирован — «классификатор относит отказ к SEC-1 или класс-байту раньше, чем к H1/magazine/bump/is_free» |
| ox P3-4 (знаменатель активации) | §9: по каждой **free**-ячейке (prefix-ячейки без free исключены — иначе 0/0); числитель = free, дошедшие до шага (2) §4.5 по У1-пути (счётчик cached-handle); ≥ 95 % |
| ox P3-7 (транспорт handle) | §4.1: (а)/(в) — handle заново выводится в guard по `cache_index(root)` (heap-private); (б) — читается из заголовка; **запрет протаскивать handle через `OwnedBlock`/witness** (H5A ИЗМЕРЕНО +5) |
| ox P3-8 (`is_routed()` gate) | §4.2: cfg-комбинация fatal-хука записана точно; поведение при выключенных фичах — хук недостижим для маршрутизированного ядра (HeapCore-ядро создаётся только с owner) |
| ox P3-6 (гейты) | §9: batch-ячейки **исключены** из %-hot-гейта; batch-порог — `ΔIr/N ≤ +6 Ir/блок` с проверкой неизменности alloc-части (пар prefix нет — сравнение полных ячеек); batch-строка добавлена в §2.1; «alloc ±0.5» → двусторонний \|Δ\| ≤ 0.5 |
| ox P3-8/§10.6 (атрибуция) | §10.6: −20…−30 — экономия **SEC-1** (G7h 33 → 6), уже в этапе 1; H1 — несколько Ir (в iai сворачивается в `off & 15`) |
| ox P3-9 (вариант (б) поверхность) | §4.1: изменение поверхности при порче памяти (hardened SEC-1 начинает читать указатель из сегментной памяти) + кейс T7 «испорченный handle»; критерий выбора варианта — EstCycles **+ T7** |
| ox P3-10 (мелочи) | §2.1 подпись std/core (четвёртое 3.0 = `uint_macros`); «ox3»; «Снятые» → §11; T1: 49 production + 55/58 под `medium-classes`/`-wide`; делители по каждой паре; этап 2 интервал refill/recycle −2…+1 % — решение не требуется, фиксируется в отчёте |

## 1. Реальная матрица гарантий (этапы 1–2)

Цепочки: free `GlobalAlloc::dealloc` → `dealloc_routing` (`routing.rs:10-20`) → `owned_block_of`
(`mem_impl.rs:42-70`) → `small_free_guard` (`dealloc_own_base.rs:189-323`) → `divides`/push/overflow.
Alloc: `hot.rs:106-149` → hit `:226-302` → miss → refill (`:652-674`) → drain
(`alloc_core_small_impl.rs:530-656`) / carve.

| # | Гарантия | Сегодня (file:line) и реальная сила | В дизайне | Момент | Оракул | Цена сегодня → в дизайне |
|---|---|---|---|---|---|---|
| G1 | Принадлежность без разыменования | маска (`os.rs:160-162`) + own_cache (`segment_table_impl.rs:789-804,851-853`) + span-check | без изменений; + handle карты класса (§4.1, варианты (а)/(б)/(в)) | вход free | `stats_foreign_or_unroutable_frees`, `dealloc_only_no_bind*` | 10 → 10 |
| G2 | Kind-роутинг | `BlockKind::of` `:205`; Large `:228-235`; Unknown `:284-286` | без изменений (H5A ИЗМЕРЕНО +5 — witness не расширяется) | до M2-оракулов | `regression_hardened_large_kind_own_free.rs:103` | 3 → 3 |
| G3 | Free в метаданные | `payload_start` `:304-311` | этап 1: заменяется класс-байтом старт-гранулы; flushed-пути — без изменений | вход free | `regression_dealloc_metadata_region_guard.rs:77` | 3 → 0 отдельно |
| G4 | Double free в magazine (oracle 1) | `is_in_magazine` `:312-314`; clear hit 12.19 (R29-10), mark push `:461,:570`, refill `hot.rs:81-87` | без изменений — отдельный ранний `return` (§4.5) | вход free | `heap_core_tcache_m2.rs:68,286`, `regression_magazine_oracles.rs:63,242` + T5 | 5 → 5 |
| G5 | Double free после flush (oracle 2) | `alloc_bitmap.is_free` `:318-320` | без изменений | вход free | `heap_core_tcache_m2.rs:149,227`, `double_free_guard` | 2 → 2 |
| G6 | Stale/uncarved (oracle 1.5) | `off >= bump_of()` `:315-317` | без изменений | вход free | `regression_magazine_bump_guard.rs:77` | 2 → 2 |
| G7 | Interior/wrong-class/невыровненный | production: `divides` `:421-427` — aligned wrong-class **проходит** (`:413-416`); невыровненный scalar **отвергается** (item 209); в batch фильтров нет (`dealloc_batch.rs:311-312`) | У1: `off & (MIN_BLOCK−1)==0 ∧ encoded(off>>4)==c+1` (§4.2) — scalar и batch; «⊆» только для полной формулы | после kind, до push | T1, T8 + мутанты | scalar: `divides` ≈7 → класс-байт ≈5 + выравнивание 1–2; batch: +4…6 Ir/блок (§2.1) |
| G7h | hardened: точный класс | `small_class_matches` `:417-420` → `route_slots.rs:216-237`, fail-open; ≈33 Ir | та же проверка через handle; batch закрывает item 247(b) | до push | `regression_hardened_interior_ptr.rs:65,145`, `r11_4_dealloc_batch_hardened_guards` | ≈33 → ≈6 |
| H1 (hardened) | Interior по `off % block_size` | `:289-296` — несколько Ir (в iai с константным `c` сворачивается в `off & 15`) | **остаётся как сегодня**; съём законен только вместе с конъюнктом `byte==c+1 ∧ aligned ⇒ off % bs == 0` — отдельное решение владельца (§10.6) | hardened-вход | `regression_hardened_interior_ptr.rs` | без изменений |
| G8 | Producer interior/never-issued | `sidecar_bitmap.rs:118-127` (item 246) | без изменений | публикация | `r6_u21_foreign_free_interior_granule.rs:97`, loom | 0 own |
| G9 | C6 | drain probe `:559-606` + commit `:609-633`; carve `:888-894` | этап 2: production drain одним проходом; hardened двухпроходным; carve/pop — без изменений | каждая выдача | `r11_ph3a_issue_transaction_*`; клетка `:193-237` — «Снятые» (§11) | per block ≈3–4 (ОЦЕНКА) → 0 |
| G10 | Exactly-once credit | fatal `segment_header_meta_fields.rs:50-58,72-79` | без изменений | границы run | `r11_ph3a_*:61-80` | ≈0.5/run |
| G11 | Терминальная публикация | `routing.rs:26-79`; reclaim `reclaim.rs:23-72` | без изменений (проверка класса в reclaim не добавляется) | публикация / cut | `r6_sidecar_bitmap.rs:187`, Miri path-b | без изменений |
| G12 | Э6: own free не пишет тело | push `:452`; body-write — flush `:662`, reclaim `:67`, dealloc_small `:119`; R1-01 `:494-507` — root-запись подтверждена, SB/TB INFERRED (item 164) | без изменений | — | 4 expected-red клетки `ci.yml:3048-3155` | 0 |
| G13 | R1-01 free-park budget | `FREE_PARK_CAP` (`tcache.rs:184-194`) | без изменений (H4 NO-GO) | push | `regression_r1_01_magazine_free_budget.rs` | 2 → 2 |
| G14 | Residual | `:180-186`, `tcache.rs:47-53`; item 246 | без изменений | — | нет | 0 |
| G15 | Poisoned production `next` | `continuation_bounds.rs:34-36`; `alloc_core_small_impl.rs:563-625` — items 201/247(a) | этап 2 не меняет силу; + `contains_aligned(head)` (§4.3) | pop/drain | `regression_freelist_next_validation` | 0 |

## 2. Бюджеты — по одной базе (`_raw_b4_math_math-base.log`, alloc 63 / free 115)

Правило учёта (`brainstorm_ox.md` §1.1): `файл | строка` и ` | файл:функция` — два представления
одной стоимости; не суммируются. Налог кадра — 15–17 Ir на сторону (ИЗМЕРЕНО).

### 2.1 free: разложение базы (ИЗМЕРЕНО, base.log) и дельта этапа 1 по режимам

| Статья базы | Ir | Источник/примечание |
|---|---:|---|
| Кадр обёртки `w_free` (function-split: строка `fn w_free` 15 + хвост ≈2) | 17 | base.log:34-35 |
| TLS/dispatch | 3 | base.log:53 |
| `global_alloc` dispatch | 4 | base.log:46 (function-split уже включает построчные 2.0+2.0) |
| `size_classes`: `class_for` ≈5 + `divides` ≈7 | 12 | base.log:36 (runtime-Layout; в iai сворачивается — см. прогноз (б)) |
| own_cache lookup | 10 | base.log (cache_index 4.1 + загрузка записи 4.0) |
| `segment_bitmap` — битовая арифметика G4/G5/mark | 8.8 | base.log:38 |
| kind decode | 3 | base.log |
| guard-ветки: payload 3 + magazine 5 + bump 2 + is_free 2 | 12 | base.log |
| push: count 5 + slot 2.9 + count store 2 + `cap` 2 | 12 | base.log |
| `flush_class_internal` | 4.7 | base.log:42 — собственная стоимость файла, без вызываемых |
| std/core (`ptr` 4.2 + `mut_ptr` 3.3 + `cmp` 3.0 + `uint_macros` 3.0; thread_local — в строке TLS) | 13.5 | base.log |
| прочее / не идентифицировано (остаток; вкл. хвост `dealloc_own_base` ≈6.4) | 15.0 | выведено |
| **Сумма** | **115.0** ✓ | |

**Дельта этапа 1 (У1; ОЦЕНКА ±30–50 %) — по режимам:**
- **(а) проба, граница `GlobalAlloc`, runtime-Layout** (кредит `divides` −7 из base.log действует):
  − `divides` ≈7 − `payload_start` 3 + класс-байт ≈5 + handle 1–2 + выравнивание 1–2 ⇒
  **−1…−3 Ir/free**.
- **(б) iai, константный Layout** (статьи `class_for`/`DIVISIBILITY[c]` — константы; для 16 B
  `divides` вырождается в `off & 15` — тот же предикат, что добавляет У1; линия `DIVISIBILITY`
  в iai **не читается** — статьи «−1 линия» там нет): − payload 3 − divides 1…2 + класс-байт 5 +
  handle 1…2 + выравнивание 1…2 ⇒ **≈ +1…+5 Ir/free, центр ≈ +3**. Опора (ИЗМЕРЕНО): PR #57 добавил
  `divides` в тех же ячейках за **+2 Ir/free** (`docs/perf/R6_T05_T06_T33_FREE_VALIDATION_GATE.md:44-46`).
- **batch** (в этапе 1 нет `divides` — снимать нечего): **+4…6 Ir/блок (ОЦЕНКА)** = +5 класс-байт +
  1…2 handle + 1…2 выравнивание − 3 payload; ячейки batch исключены из %-hot-гейта (§9).

Главная ценность этапа 1 — усиление (G7/G7h) и hardened −20…−30 Ir/free (ОЦЕНКА; измерено в iai —
`R6_T05_T06_T33_FREE_VALIDATION_GATE.md:67`). Цена усиления в обоих режимах вынесена владельцу
(§10.2).

### 2.2 alloc: дельта этапа 2 (У2)

Видимый drain в w_alloc ≈ **7.7 Ir/op** (ИЗМЕРЕНО, base.log:9,16,17,30,31). Снимаемая часть —
**ОЦЕНКА 3–4 Ir/op** ⇒ **alloc 63 → ≈59–60 (ОЦЕНКА)**.

### 2.3 Сверка с раундом 4

Прогноз «−1…−3» без указания режима отозван (ox P2-1 раунда 4): он был валиден только для пробы;
для iai предрешал NO-GO главному обещанию. Честные прогнозы — раздельно по режимам (§2.1).

## 3. Разрешение противоречий консультаций (по коду)

Без изменений с раунда 4: (3.1) `ClassLeaves`, не per-class страницы; (3.2) закрытые семейства —
R32-6, B3/PB, N/off-body, этап 2 r4, R3, s2, H1-слияние — не повторяются; (3.3) off-body и У3
отклонены.

## 4. Дизайн

### 4.1 Проектирование от данных

| Структура | Сегодня | В дизайне | Линии на free |
|---|---|---|---|
| own_cache | AoS `(key, root)` ×16; hit = одна линия | **Варианты (выбор по EstCycles + T7, этап 1)**: (а) AoS + массив `leaves` ⇒ 2 линии, +128 B; (б) указатель на `ClassLeaves` в линии заголовка ⇒ 0 доп. линий; (в) own_cache `(key, root, leaves)` выровненный до 32 B ⇒ **1 линия на hit**, +256 B, heap-private | (а) +1; (б) 0; (в) +0 |
| Класс-байт гранулы | producer-only чтение | 2 линии (`mixed[]` + лист) − снимаемая линия `DIVISIBILITY` (только runtime-Layout; в iai её нет) | +2, −1 (проба) |
| `OwnedBlock` | `(root, block)` 16 B | **не расширяется**; **транспорт handle**: (а)/(в) — handle **заново выводится в guard** по `cache_index(root)` (heap-private); (б) — читается из заголовка сегмента. **Запрещено протаскивать handle через `OwnedBlock`/witness** (H5A ИЗМЕРЕНО +5) | 0 |

**Вариант (б) — жизненный цикл и поверхность**: поле пишется в `register_payload`
(`segment_table_impl.rs:452-492`) и attach primordial (`lifecycle.rs:276-277`); слот — инертный
`owner_state` (`segment_header_impl.rs:282-297`, пин 144 B); инвариант «поле валидно ⇔ сегмент
зарегистрирован ⇔ route жив». **Изменение поверхности при порче памяти**: hardened SEC-1 начинает
читать указатель из сегментной памяти (сегодня — только heap-private `RouteSlots`) — не ослабление по
контракту целостности метаданных, но фиксируется; кейс T7 «испорченный handle» с видом отказа
обязателен при выборе (б). **Критерий выбора варианта — EstCycles + результат T7**, не только
EstCycles.

**unsafe-handle**: seam-аксессор в tier-1 `route_slots.rs`; порядок teardown как сегодня
(`routes.remove` → `hash_remove` → `own_cache_clear`, `:563-579`/`:713-729`); окно owner-only;
инвариант «entry валиден, пока route жив» + debug_assert; инвариант own_cache (`:212-225`)
переписывается.

### 4.2 У1 — формула с выравниванием; три случая нулевого листа / handle

Формула (scalar И batch — общий `small_free_guard`):
```
off & (MIN_BLOCK−1) == 0   ∧   encoded(off >> 4) == c + 1
```
Обоснование конъюнкта (сценарий `free(start+8)`: `is_in_magazine` читает бит `start` по `off>>4` —
`segment_bitmap.rs:113-117` — и промахивается; итог — невыровненная выдача и перекрытие соседа),
доказательство `byte==c+1 ∧ aligned ⇒ divides(c, off)` (`size_classes.rs:59-63`, `leaf_classes.rs:1-5`),
«⊆» только для полной формулы, судьба H1 (§10.6) — без изменений с раунда 4.

Три случая (правило — только стартовая гранула; внутренние листья блоков крупнее 4 КиБ нулевые,
`SMALL_MAX` ≈ 253 КиБ — не затрагиваются): (а) unrouted — прежний путь целиком с SEC-1
(`route_slots.rs:223-235`), на пути HeapCore недостижим (`:452-457`); (б) нулевой лист ⇒ REJECT
(`small_sidecar.rs:128-131`); (в) отсутствующий handle при живом route ⇒ debug_assert + восстановление
не слабее SEC-1.

**Инвалидация / retain (раунд 4, с уточнением gate)**: retain достижим только через хук
`dbg_force_decommit_retain_for` (`decommit.rs:106-121`); оба тестовых потребителя — на
немаршрутизированном `AllocCore::new()`; ядро HeapCore крейт-приватно (`core.rs:111`), создаётся
`new_with_owner` (`lifecycle.rs:120-129`). **Механизм: retain ⇒ fatal для маршрутизированного ядра.**
Точная cfg-комбинация: `is_routed()` объявлен под `alloc-global ∧ alloc-xthread`
(`segment_table_impl.rs:305-309`) ⇒ проверка в хуке — `#[cfg(all(feature = "small-class-authority",
feature = "alloc-global", feature = "alloc-xthread"))] fatal if routes.is_some()`. **При выключенных
фичах** (`alloc-global`/`alloc-xthread` отсутствуют): маршрутизированного ядра не существует —
HeapCore-ядро создаётся только с owner (`new_with_owner` под `alloc-global`), `AllocCore::new()`
безroute — хук легален и ведёт себя как сегодня; пин-тест T9 — под той же комбинацией фич.
Остальное: класс magazine-ключа из `class_for` (вариант физического класса — §10.4).

### 4.3 У2 — без изменений с раунда 4 (один проход + `contains_aligned(head)`; hardened
двухпроходным; run-level сторожа; C6 вакуумно; Release-store — §10.5; лемма — routed-геометрия,
случаи (а)–(в)).

### 4.4 Отклонено: этап B (M2, free-bin-cap) — пять ослаблений, u32-ссылки, caller-cap (`orig`),
спайк; переоткрытие — только новым решением владельца (без изменений с раунда 3).

### 4.5 Проверки free — последовательные ранние `return`; диагностика — три категории

Порядок: (1) выравнивание; (2) класс-байт; (3) magazine; (4) bump; (5) is_free. Слияние — NO-GO (H1).

Категории диагностики (production `divides` `:421-427` и hardened SEC-1 `:417-420` сегодня
молчаливы; H1 увеличивает `LOCAL_REJECT_FREES` до SEC-1 — `:289-295` против `:417-438`):
1. **«сегодня считает `LOCAL_REJECT_FREES`»** — RejectNoOp, H1: как сегодня;
2. **«сегодня молча»** — production `divides`/SEC-1: по умолчанию остаются молчаливыми;
3. **«усиления У1»** — `LOCAL_REJECT_STRONGER_FREES`: **aligned wrong-class (scalar+batch)**,
   **interior в нулевой лист (scalar+batch)**, **невыровненный free (только batch)** — scalar
   невыровненный отвергается уже сегодня (`divides`, item 209) и относится к категории 1/2 по
   правилу первого старого предиката. Классификация — инлайн в существующую reject-руку; R6-T55
   сохраняются.

### 4.6 Арифметические приёмы — все ИЗМЕРЕНО NO-GO (`B4_MATH_HACKS.md`); этап D закрыт (без
изменений с раунда 4).

## 5. Интеграционные накладные

Налог кадра 15–17 Ir/сторону (ИЗМЕРЕНО, `brainstorm_ox.md` §1.1). iai и larson/mstress инлайнят
аллокатор — кадр не видят (`brainstorm_ox.md` §1.2) ⇒ **режимы метрик различаются** (§2.1): iai —
константный Layout (добавляемая стоимость видна, снимаемая частично сворачивается), проба —
runtime-Layout (видит и то, и другое, включая кадр). Инфраструктура ≈47 из 115 free.

## 6. Стоимость выбранной конструкции (не «нижняя граница всех допустимых»)

**По режимам**: проба (runtime-Layout) — production free ≈**112–114** (−1…−3); iai (константный
Layout) — **≈ +1…+5 Ir/free, центр ≈ +3** — это **цена гарантии усиления во встраиваемом режиме**,
решаемая владельцем (§10.2), а не автоматический NO-GO. Как ускорение этап 1 в пробе не окупается;
ценность — усиление и hardened −20…−30 (ИЗМЕРЕНО в iai-плечо, `R6_T05...md:67`). Статьи отличия от
mimalloc: гарантийные — съём = ослабление/смена ключа (§10.4); реализационные (кадр, TLS) —
снимаются реализацией. Универсальная нижняя граница не утверждается.

## 7. Цена по памяти и C6/OOM-атомарность

Без изменений с раунда 4: +0 байт на сегмент; (а) +128 B; (б) +8 B в линии заголовка; (в) +256 B;
маска снята. C6: этап 1 free-путь не трогает; этап 2 — production drain неотказен; клетка
`r11_ph3a_:193-237` — «Снятые» (§11).

## 8. Тест-доказательства

| # | Тест | Эталон / мутанты |
|---|---|---|
| T1 | Включение `old_prod_reject ⇒ new_reject` + сохранение acceptance; кейсы усиления = {aligned wrong-class, interior в нулевой лист, невыровненный — batch}; **эталон — полная hardened-цепочка** (H1 `:289-296` + payload `:304-311` + SEC-1 `:417-420`); лемма невыровненных `off`; классы: **49 production** (`size_classes.rs:21-22`), расширенные **55/58 — только под не-production фичами `medium-classes` / `medium-classes-wide`** (`size_classes.rs:32-40`) — перебор в соответствующих конфигурациях; состояния геометрии. **Мутанты**: (м1) «снят конъюнкт выравнивания» — краснеет на `start+8`; (м2) «сравнение с `c` вместо `c+1`»; (м3) «только payload, без класс-байта»; (м4) «ошибочный/устаревший cached handle принудительно подан в проверку» | полный перебор |
| T4 | Инвариант «узел бина ⇒ `encoded==c+1`» (routed-геометрия, случаи (а)–(в)) | counterfactual: «узел бина с невыданным/нулевым байтом» или «wrong-class free без У1» краснеет |
| T5 | G4-оракул (free X → refill в magazine → повторный free → нет двойной выдачи) | мутант «убрать `is_in_magazine`» краснеет в этапе 1 |
| T6 | Три категории диагностики (§4.5) | мутант «усиление засчитано в `LOCAL_REJECT_FREES`» краснеет; мутант **«классификатор относит отказ к SEC-1 или класс-байту раньше, чем к H1/magazine/bump/is_free»** краснеет (правило первого старого предиката; невыровненный scalar-free — категория 1/2) |
| T7 | **Порча метаданных**: испорченный `head`/`next` — вид отказа прежний; **+ кейс «испорченный handle»** (обязателен при варианте (б)): вид отказа — reject по инварианту/debug_assert/fatal, без дикого чтения пользовательских данных | мутант «без `contains_aligned(head)`» / «без инварианта handle» краснеет |
| T8 | **Batch 247(b)** под `production batch-api`: aligned wrong-class, interior в нулевой лист, **`free(start+8)`** ⇒ no-op | мутанты: «У1 только в scalar-ветке»; **«выравнивание только в scalar-ветке»** (ловит частичный перенос: класс-байт в общем guard, выравнивание — нет) |
| T9 | Retain-пин: маршрутизированное ядро + хук ⇒ fatal (под `alloc-global ∧ alloc-xthread ∧ small-class-authority`) | мутант «fatal снят» краснеет; retain-тесты на `AllocCore::new()` остаются зелёными |

## 9. План прототипов

### Этап 1 — `small-class-authority` (У1)

**Файлы**: `dealloc_own_base.rs` (формула §4.2); `dealloc_batch.rs:311`; `mem_impl.rs:42-70`
(кэш-путь; **handle НЕ через `OwnedBlock`** — §4.1); `segment_table_impl.rs` (вариант (а): fill/clear
`:789-804,:871-877`; (б): `register_payload` `:452-492`, `lifecycle.rs:276-277`, слот `owner_state`);
seam `route_slots.rs`; `small_sidecar.rs`; fatal-хук retain (cfg §4.2); счётчики T6; cfg-фича оракула.

**Тесты**: T1–T9 + существующий набор.

**Судья GO — iai A/B `perf_gate_iai`, метрики и решающий режим:**
- **(а) Формулы free** (отдельно A/B, затем дельта):
  `free(n) = (Ir(dealloc_free_only_16b_n) − Ir(dealloc_prealloc_only_16b)) / n`;
  делители: `dealloc_free_only_16b` n = 64 (CHURN_OPS, `perf_gate_iai.rs:102`), `_n8` = 8, `_n16` = 16,
  `_n1024` = 1024; серия 1088 — prefix `dealloc_prealloc_only_1088_16b` (`:1587-1601`), n = 1088.
- **(а-rtl) РЕШАЮЩИЙ режим строки 2 — runtime-Layout**: новые ячейки с `black_box(layout)` по той же
  prefix-схеме — **`dealloc_free_only_16b_rtl` + `dealloc_prealloc_only_16b_rtl`** (n = 64) и
  **`dealloc_free_only_1088_16b_rtl` + `dealloc_prealloc_only_1088_16b_rtl`** (n = 1088);
  **добавляются в `perf_gate_iai.rs` в объёме этапа 1** (по образцу существующих, `:276`,`:298`,`:878`,
  `:904`). Обоснование: судья должен видеть добавляемую и снимаемую стоимость в одном режиме
  («cost and benefit in the same regime») — иначе константные ячейки предрешают NO-GO главному
  обещанию этапа 1.
- **(б) Константные ячейки** (`dealloc_free_only_16b*` без `_rtl`) — **сопутствующий гейт с заранее
  записанным допуском ≤ +5 Ir/free**, иначе NO-GO (прогноз центра ≈ +3 — внутри допуска).
- **(в) Правило агрегации**: каждая named-ячейка в пределах порога; одна регрессия = NO-GO; без
  усреднения. **EstCycles**: расширение парсера раннера (`r20_r6_ab_iai_run.mjs:101-119` — сегодня
  только `Instructions`) на Cycles — в объёме этапа.
- **(г) Hardened-плечо** — отдельный A/B-прогон, features `production hardened bench-internals
  internals`, те же формулы; порог ≤ −15 Ir/free.
- **(д) Alloc-метрики** (rush P2-2): **hit** — пара `alloc_magazine_prefill_only_16b` (`:456`) /
  `alloc_magazine_hit_only_16b` (`:495`), `hit = (Ir(hit) − Ir(prefill)) / 16`;
  **drain/refill** — cold/recycle-пары: `cold_alloc_free_256x16b` против alloc-only-плеча
  `cold_alloc_only_256x16b` (добавляется по образцу prefix-ячеек; n = 256) и аналогично
  `recycle_alloc_only_256x16b`. **Этап 1: \|Δ\| ≤ 0.5 Ir/op на каждой named alloc-производной
  (hit и drain/refill) — двусторонний допуск**; **этап 2: порог −3 — именно на drain/refill-метрику**.
  **Расширение runner вычисляет эти производные** (не только EstCycles).
- **(е) Batch-api-прогон** (`dealloc_batch_fresh_16/64/80/81_16b`, `:1870-1946`): **формула
  `ΔIr/N`** (пар prefix нет — сравнение полных ячеек A/B с проверкой, что alloc-часть не изменилась);
  **порог ≤ +6 Ir/блок; batch-ячейки исключены из %-hot-гейта** (иначе допуск мёртв: +4…6 Ir/блок ×
  64 = +2…3 % ячейки при hot ≤ +0.5 %).
- **(ж) Activation-оракул**: отдельная cfg-фича оракула (на базе — заглушка), включается только в
  `--oracle`-сборке; **подключение `--oracle`-стадии раннера (`:318-326`) — в списке работ этапа 1**.
  Числители — раздельные: «проверка по cached handle» / «случай (в) — ожидается 0» / «случай (а) —
  недостижим на HeapCore». **Знаменатель — по каждой free-ячейке** (prefix-ячейки без free
  исключены — иначе 0/0): free, дошедшие до шага (2) §4.5 по У1-пути. **Порог ≥ 95 % по каждой
  free-ячейке, суммарно по прогону.**
- **(з) Проба prof_ao — feature-capable**: скрипты `.rush/wsl/prof_ao_f*.sh` / `prof_f*.sh` с
  аргументом FEATURES (существуют; `prof_ao_f_win.sh:6` принимает FEAT); **A = `production`,
  B = `production small-class-authority`**. Свидетельство активации кандидата: строки
  `leaf_classes.rs` появляются в построчной разметке `w_free` пробы (правило R30-8). **Порог:
  `w_free ≤ база + 0`.**

**Таблица решений этапа 1 (одна, непересекающаяся; сначала correctness, потом performance):**
1. **Correctness**: T1–T9 зелёные/красные как задано; активация ≥ 95 % (знаменатель (ж)) — иначе **NO-GO**.
2. **Решающий performance — `free_rtl` (ячейки (а-rtl), runtime-Layout)**:
   `free_rtl < база −1` → **GO**; `−1 ≤ free_rtl ≤ +1` → **только явное решение владельца**
   («за усиление» или NO-GO-возврат); `free_rtl > +1` → **NO-GO**.
3. **Сопутствующие гейты (каждый обязательный; нарушение любого → NO-GO)**: константные ячейки
   ≤ +5 Ir/free; hardened free ≤ база −15; hot ≤ +0.5 % Ir и EstCycles ≤ +1 % — на каждой named-ячейке,
   **кроме batch-ячеек**; batch `ΔIr/N` ≤ +6 Ir/блок; проба `w_free ≤ база + 0`; alloc: \|Δ\| ≤ 0.5
   Ir/op на каждой named alloc-производной.

### Этап 2 — `drain-single-pass` (У2) — опция: после GO этапа 1 **и** решения §10.5

1. Correctness: T4/T7, активация ≥ 95 % (знаменатель = eligible drain/recycle-операции) — иначе NO-GO.
2. **`alloc_drain < база −3` → GO; `−3 ≤ alloc_drain ≤ 0` → решение владельца при принятом §10.5;
   `alloc_drain > 0` → NO-GO** (метрика — drain/refill из §9(д), `cold_alloc_only_*`/
   `recycle_alloc_only_*`-плечи добавляются в объёме этапа 2).
3. refill/recycle ≥ 2 % — обязательный сопутствующий гейт: регресс любой > 1 % = NO-GO независимо от
   alloc; **внутри интервала от −2 % до +1 % решение не требуется — результат фиксируется в отчёте**;
   hot ≤ +2 % Ir (проценты).

## 10. Вопросы владельцу (меняют форму гарантий)

1. ~~Cap-link/спайк~~ — снят с этапом B.
2. **Усиление invalid-use — безусловное предусловие этапа 1**, с явной ценой по режимам:
   **проба (runtime-Layout) ≈ −1…−3 Ir/free (нейтрально/выгода); встраиваемый режим (константный
   Layout, LTO) ≈ +1…+5 Ir/free, центр ≈ +3 (§2.1(б), опора `R6_T05_T06_T33_FREE_VALIDATION_GATE.md:44-46`)
   — цена гарантии**, которую владелец принимает или отклоняет решением «за усиление»; автоматический
   NO-GO не является единственным исходом этой цены (решающий режим строки 2 — runtime-Layout, §9).
   Состав усиления: aligned wrong-class и interior в нулевой лист (scalar+batch); невыровненный free —
   только batch (scalar отвергает уже сегодня, item 209).
3. ~~У1-инвалидация~~ — снята (fatal-хук, §4.2); retain-политика на маршрутизированных сегментах —
   отдельный ADR со сбросом ClassLeaves.
4. **Ключ magazine/bin: Layout-класс vs физический класс** (−≈7 Ir только в runtime-режиме; меняет
   посадку free с указателем валидного размещения, но Layout другого класса).
5. **У2 — снятие Release-store повторной выдачи**; HB-аргумент §4.3; решение до старта этапа 2.
6. **Съём hardened-проверок**: −20…−30 Ir/free — это экономия **SEC-1** (G7h: 33 → 6), уже входящая
   в этап 1; **H1** — несколько Ir (в iai с константным `c` сворачивается в `off & 15`); его съём
   законен только вместе с конъюнктом `byte==c+1 ∧ aligned ⇒ off % bs == 0` — отдельное решение,
   вне этого этапа.

## 11. ИЗМЕРЕНО vs ОЦЕНКА

ИЗМЕРЕНО: база 63/115/319/494 (`math-base.log`); разложение §2.1; кадр 15–17 (`brainstorm_ox.md` §1.1);
drain 7.7; **+2 Ir/free от `divides` в iai-ячейках** (`R6_T05_T06_T33_FREE_VALIDATION_GATE.md:44-46`);
hardened SEC-1 33 Ir в iai (`:67`); все 7 NO-GO B4-math; 12.19/hit (R29-10). ОЦЕНКА: прогнозы по
режимам (проба −1…−3; iai +1…+5, центр +3); снимаемая часть drain 3–4; alloc ≈59–60; batch +4…6;
hardened −20…−30; линии кэша; поведение LLVM. Не утверждается: паритет; «единственный путь»;
универсальная нижняя граница.

### Снятые вопросы и клетки (§11 — для полноты; ссылки «§0 Снятые» раунда 4 исправлены)

- Переориентация proof-клетки C6 (`r11_ph3a_:193-237`) при этапе 2 — техническое, в объёме этапа 2.
- Production-политика retain на маршрутизированных сегментах — отдельный ADR со сбросом ClassLeaves.
- Этап B/M2, u32-ссылки, caller-cap (`orig`), Miri-спайк — §4.4.
