# Исследование решения: Large-кэш пересоздаёт directory route на каждом цикле (R17-PRF-03, OPT-06)

База: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`; R18-исправления `ff3d60bb..HEAD` входят).
Режим: только чтение. Ни одной компиляции, теста, бенча, `npm`/`node` не запускалось. Все числа Ir — из закоммиченных логов (файл:строка) или помечены «оценка по коду, не измерение».

## §0 Чек-лист хода

- [x] Шаг 0: база проверена (HEAD = 31db7f17)
- [x] Первоисточники: oxx round-17 performance §R17-PRF-03 / OPT-06, xxs P3 / P5 / synthesis, perf OPEN_ITEMS 70/82
- [x] Трасса deposit (unregister) — код
- [x] Трасса hit (register) — код
- [x] RouteDirectory: шард-лок, CAS, alloc/free, pin
- [x] Логи Ir (378 -> 1303) — проверены, с оговоркой о сопоставимости
- [x] Корректность удержания маршрута (терминальные состояния, foreign free, eviction, pin, payload)
- [x] Варианты, рекомендация, план, гейт

## §1 Постановка

### 1.1 Что не исправлено (перечитано на базовом SHA)

Каждый цикл «Large-free в кэш → cache hit» снимает глобальный directory route и создаёт новый.

**Deposit (own-thread free)**
- `src/alloc_core/alloc_core/mem/mem_impl.rs:269` — `claim_live` по слову заголовка;
- `:402` — `self.table.unregister(base)`;
- `:436` — `cache_consumed`;
- `:442–453` — `large_cache_slot_set`.

**Цепочка unregister**
- `src/alloc_core/segment/segment_table/segment_table_impl.rs:489–537`: `routes.remove(slot_id)` на `:515–516`;
- `src/alloc_core/segment/segment_table/route_slots.rs:203–212`: `slot.take()` роняет `RouteRegistration`;
- `src/registry/segment_route/registration.rs:78–82`: `Drop` вызывает `RouteDirectory::remove`;
- `src/registry/segment_route/directory.rs:891–897`: шард-лок, `Shard::remove` (`:605–641`);
- затем `EntryHandle::drop` (`directory.rs:210–224`): `refs.fetch_sub`, при последней ссылке два `System.dealloc` (`LargeState` в `:126`, `Entry` в `:223`).

**Foreign-путь deposit**
- `src/alloc_core/large/alloc_core_large.rs:726–832` (`reclaim_large_segment`): `unregister` на `:738`, `cache_consumed` на `:803`.

**Hit**
- `src/alloc_core/large/alloc_core_large.rs:246–483`:
  - `begin_large_reuse` (`:249`);
  - targeted header writes (`:387–411`);
  - `self.table.register_payload(...)` (`:420–436`);
  - `finish_large_reuse` (`:452`).
- `register_payload` (`segment_table_impl.rs:385–446`) → `RouteSlots::prepare` (`route_slots.rs:74–92`) → `RouteDirectory::register` (`directory.rs:789–865`):
  - глобальный `next_incarnation.fetch_update` (`:825–829`);
  - `EntryHandle::new` → `System.alloc(LargeState)` (`:72`) + `System.alloc(Entry)` (`:85`);
  - шард-лок + `Shard::insert` (`:832–848`).

**Протокол, который сохранял бы маршрут**
- `src/registry/segment_route/registration.rs:55–75`: `cache_large_consumed`, `begin_large_reuse`, `finish_large_reuse_after_reset`, `release_cached_large`;
- обёртки `src/registry/segment_route/large_state.rs:34–50`.
- Grep по `src/`: production-вызовов этих методов нет. Их вызывает только `tests/r6_route_directory.rs:151–165` (и модельный тест).

### 1.2 Что уже известно (не переоткрывать)

- **oxx R17-PRF-03** (`docs/reviews/2026-10-08-src-review-oxx-round-17-performance.md:239–278`): SOURCE-CONFIRMED путь; разложение — ГИПОТЕЗА; рекомендация OPT-06 (`:454–461`).
- **xxs P3** (`docs/reviews/2026-10-08-src-review-xxs-round-17-P3.md:25–31`): P3 без изменения, PARTIALLY VALID/NARROWED.
  - Сужение: при живом pin физический free откладывается до последнего pin.
  - Текущая цена не измерена.
  - Владельца-карточки нет; связанная — perf **70** (стоимость sidecar/cutover; `docs/perf/OPEN_ITEMS.md:3000–3005`).
- **Synthesis** (`docs/reviews/2026-10-08-src-review-xxs-round-17-synthesis.md:71`): OPT-06 «Отсутствует; не назначена».
- **Контракт** (`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md` §2, §5):
  - stale/double free вне valid-use proof;
  - чтение *текущего* generation не распознаёт stale raw pointer после same-VA reuse — это граница hardening, а не valid-use safety.
  - Это НЕ надо переоткрывать: удержание маршрута эту границу не меняет (см. §2.4).
- **perf 82** (`docs/perf/OPEN_ITEMS.md:3044–3049`): отрицательный ответ директории не авторитетен. Решение не должно его использовать.

### 1.3 Числа (проверено по закоммиченным логам)

Метрика — разность Ir пары `large_cache_hit_only_4mib − large_cache_prefill_only_4mib`. Это ровно один hit-alloc + один deposit-free на слое `HeapCore` (`benches/perf_gate_iai.rs:2280–2339`). Форма бенча «alloc **и** free в хвосте» неизменна с момента его создания (`git log -L` по `fn large_cache_hit_only_4mib`: `eb2463a4` добавил тело с хвостовым free; `5083e4bc`/`11755191` меняли только способ claim и `unsafe`). Значит, ряд сопоставим по форме цикла.

| Лог (строки) | Дата | prefill | hit | Δ на цикл |
|---|---|---|---|---|
| `docs/perf/_raw_task1999_alloc_zeroed_stub_gap_after_1982.log:678–679` | 2026-09-22 (`1f4ac4f3`) | 6 575 | 6 953 | **378** |
| `docs/perf/_raw_ph3a_iai_before.log:664–665` | 2026-10-02 (`3962933b`) | 58 763 | 60 103 | **1 340** |
| `docs/perf/_raw_pg3_iai_base.log:674–675` | — | 59 190 | 60 570 | 1 380 |
| `docs/perf/_raw_r16_perf83_A1.log:125–126` (и A2/B1/B2) | R16 | 58 797 | 60 100 | **1 303** |
| `docs/perf/_raw_r32_7_after.log:700–701` | R32-7 | 6 770 | 7 115 | 345 |

Оговорки (НАБЛЮДЕНО):
- `RouteDirectory` появился позже лога 378: `efec9d16` и `1d881429`, оба 2026-09-29.
- Между этими точками префикс вырос примерно в 9 раз (6.6k → 58.8k Ir). Значит, менялся и общий путь, а не только маршрут.
- Рост +925…+962 Ir **совпадает по времени** с введением маршрутов, но bisect не делался. Атрибуция всей дельты маршруту — ГИПОТЕЗА.
- Ir считает `lock cmpxchg`/`lock xadd` как одну инструкцию. Цена атомарных RMW и межъядерной конкуренции за `next_incarnation` в Ir не видна, её нужно мерить нативно (§6).

## §2 Механизм

### 2.1 Что стоит в цикле (по коду, однопоточный цикл без чужих pin)

Deposit, маршрутная часть:
1. `unregister`: `segment_id_at`, `base_at`, повторный `SegmentHeader::read_at` (144 B copy), `routes.remove`.
2. `RouteDirectory::remove`:
   - `shard_index`;
   - **шард-лок CAS** (`shard_lock.rs:50–53`, `compare_exchange_weak` Acquire);
   - `locate` (бинпоиск по индексу блоков и блоку);
   - `remove_at`;
   - если блок опустел (единственный маршрут в шарде) — `index.remove` + `recycle` в `spare`, без System-free;
   - Release-store unlock.
3. `EntryHandle::drop`: **`refs.fetch_sub(AcqRel)`** (`directory.rs:216`). В цикле без pin `refs == 1` (единственная ссылка — владельца; pin создаётся только через `lookup`, `directory.rs:883`) → **два `System.dealloc` выполняются внутри цикла** (`:220`, `:223`).
4. Плюс табличная бухгалтерия `unregister` (`hash_remove`, `own_cache_clear`, `free_list_push`, `ActiveKindIndex::clear`). Она нужна и при удержании маршрута: таблица — отдельный механизм.

Hit, маршрутная часть:
1. `RouteSlots::prepare` → `ensure_capacity` (no-op в стационаре).
2. `register`:
   - под `internals` — TLS-чтение `FAIL_NEXT_REGISTRATION` (`directory.rs:797–807`; только в тестовых/бенч-сборках с `internals`, в чистом `production` его нет);
   - валидации + `aligned_vmem::page_size()`;
   - **глобальный `next_incarnation.fetch_update`** (CAS-цикл, Relaxed; одна кэш-линия на процесс);
   - **2× `System.alloc`** (8 B + 72 B, оценка размера `Entry` по полям `directory.rs:24–34`);
   - **шард-лок CAS**;
   - `insert`: при пустом шарде — блок из `spare`, без аллокации;
   - unlock.
3. `routes.put`, `hash_insert_identity`, `ActiveKindIndex::set` — табличная часть, нужна в любом варианте.

Итого маршрутная часть на цикл (НАБЛЮДЕНО по коду): **2 System alloc + 2 System free, 2 захвата шард-лока, 1 глобальный CAS-цикл, 1 `fetch_sub`**, два бинпоиска/сдвига. Нечего снимать: header-слово (4 CAS: `claim_live`, `cache_consumed`, `begin_reuse`, `finish_reuse`) и табличную бухгалтерию — они нужны физическому протоколу.

Оценка по коду, не измерение: маршрутная часть ≈ 300–700 Ir из 1 303. glibc tcache malloc/free по 20–60 Ir каждый; directory-логика 150–300 Ir.

### 2.2 Сужение синтеза уточнено

- **В обычном режиме (own-thread churn, однопоточный бенч) оба физических free выполняются внутри цикла.** НАБЛЮДЕНО по коду: `refs` = 1 до drop; pin берут только `publish_foreign` (`src/registry/heap_core_xthread/routing.rs:27`), foreign `realloc` (`src/registry/heap_core/free/realloc.rs:414`) и test-хуки.
- Отложенный free бывает только на foreign-пути. Producer держит pin через `publish_large` и роняет его после CAS. Owner, дренируя, может снять маршрут раньше. Тогда последний `fetch_sub` и два `System.dealloc` выполняются **на потоке producer** — межпоточный free в System malloc.
- Для пути own-thread сужение синтеза формально верно, но к измеряемому бенчу не применимо.

### 2.3 Два независимых слова фазы (ключ к корректности)

- **Физическое слово заголовка** (`src/alloc_core/segment/segment_header/terminal_words.rs:16–19, 107–163`):
  - цикл `LIVE(g) → CONSUMING(g) → CACHED(g) → INITIALIZING(g+1) → LIVE(g+1)`;
  - трогает только владелец (`src/alloc_core/large/reservation_state.rs:9–18`).
- **Слово маршрута** `LargeState` (System, `src/registry/segment_route/large_state.rs`):
  - всегда стартует `LIVE(1)` (`:14–18`);
  - producer: `LIVE(g)→PENDING(g)` (`publish_pending`, `g` = *текущее* поколение слова, `:25–28`);
  - owner-drain: `PENDING→CONSUMING` (`claim_pending`, вызывается из `src/alloc_core/alloc_core/sidecar_drain.rs:167/249/290/339` через `claim_large_route`);
  - в production слово никогда не доходит до `CACHED`/`INITIALIZING`: маршрут уничтожается раньше.
- Метода `claim_live` у `LargeState` нет. Для own-thread deposit с удержанием маршрута его нужно добавить или найти эквивалент (§5).

### 2.4 Корректность удержания маршрута (OPT-06) — интерливинги

Обозначения: O — owner, P — producer, S — stale/double free (нарушение контракта).

1. **Валидный foreign free, затем reuse.**
   - P: `lookup` → pin → CAS `LIVE(g)→PENDING(g)` → drop pin.
   - O: drain `claim_pending` → `CONSUMING(g)` → `reclaim_large_segment` → (новое) `cache_consumed` → `CACHED(g)`.
   - O hit: `begin_reuse` → `INITIALIZING(g+1)` → reset/table → `finish_reuse` → `LIVE(g+1)`.
   - Если P «завис» после CAS и держит pin, это лишь `refs`. По контракту pin не даёт права на reservation (`directory.rs` модульная doc: `mod.rs:10–12`). Корректно.
2. **S до reuse** (free уже закэшированного адреса).
   - Сегодня: маршрута нет → `lookup` = None → `FOREIGN_OR_UNROUTABLE_FREES++`, drop.
   - С удержанием: `lookup` найдёт запись. Без доработки `publish_pending` проваливается (фаза `CACHED`) и даёт тот же счётчик (`routing.rs:62–65`). Наблюдаемое поведение free совпадает.
   - **Но** foreign `realloc` (`realloc.rs:414–434`) после `lookup` + `contains_payload` скопирует из закэшированного блока и вернёт не-null. Сегодня он вернул бы null.
   - Если O параллельно вытеснит и освободит reservation, копия читает unmapped память.
   - Это invalid use, но регресс hardening-паритета. **Решение: `lookup` для `RouteKind::Large` должен отвечать «нет маршрута» при фазе `CACHED`/`INITIALIZING`/`RELEASED`** (одна Acquire-загрузка на холодном foreign-пути). Так восстанавливается паритет «закэшированный адрес не живой» для последовательного stale-сценария.
   - Гонка lookup↔cache остаётся той же, что и сегодня (lookup↔free живого маршрута).
3. **S после reuse** (stale free старого instance по тому же адресу, когда блок снова LIVE).
   - Сегодня: новый маршрут `LIVE(1)` → S публикует `PENDING` → O рекламирует **чужую живую** аллокацию.
   - С удержанием: `LIVE(g+1)`, `publish_pending` читает текущее `g+1` → то же самое.
   - Защита не хуже и не лучше. Контракт §5 прямо говорит, что без handle-API защиты нет. Не переоткрывать.
4. **S, зависший между `generation()` и CAS через reuse.**
   - S загрузил `g`, O прошёл `CACHED(g)→…→LIVE(g+1)`, CAS S ожидает `LIVE(g)` → провал.
   - Сегодня S держал бы pin на старой отвязанной `Entry` (фаза `CONSUMING`) → провал. Паритет.
5. **Двойной free «own + foreign» в гонке** (invalid).
   - Сегодня: own-путь не трогает слово маршрута. `PENDING` от P пропадает вместе с удалённым маршрутом — молча.
   - С удержанием own-deposit должен перевести слово маршрута в `CONSUMING`. Если там `PENDING`, нужно принять и это: `LIVE|PENDING → CONSUMING`. Иначе маршрут останется `PENDING`, и последующий `begin_reuse` (ожидает `CACHED`) провалится.
   - Рекомендация: при own-deposit `claim_live`, при провале `claim_pending`. Если провалились оба — `abort`, как в соседних ветках `mem_impl.rs:269–271`.
6. **Eviction/decay/trim/drop закэшированного.** Порядок обязателен:
   - `release_cached` слова маршрута (необязательно, запись уйдёт);
   - **drop регистрации (unlink) ДО `os::release_segment`**.
   - Иначе после release другой heap может получить тот же VA, и его `register` вернёт `RouteError::Duplicate` (`directory.rs:843–846`) → ложный OOM.
   - UB это не даёт: `remove` трогает только System-память. Но это функциональный дефект.
   - Места (все грепнуты):
     - `src/alloc_core/large/alloc_core_large_cache_eviction.rs:41–53` (`evict_at_least`) и `:235–244` (`evict_one_oldest`);
     - `src/alloc_core/alloc_core/lifecycle.rs:60–71` (`release_cached_on_drop`; registry-heaps сюда не доходят, `:398–404`);
     - `src/alloc_core/large/alloc_core_large.rs:249–262` (исчерпание поколения заголовка);
     - `:427–434` (провал `register_payload`).
7. **Смена payload при reuse — обязательное условие.**
   - `hdr_aligned = align_up(size_of::<SegmentHeader>(), max(align, page_size))` (`alloc_core_large.rs:141–151`). При другом `align` payload, а с ним и `Entry.payload`, другой.
   - `Entry.payload` неизменяем (`directory.rs:27`), `lookup` требует `addr == payload` для Large (`:876–881`).
   - Удержание при другом payload сделало бы валидный foreign free нового блока «немаршрутизируемым» — **тихая утечка**.
   - **Решение:** переиспользовать маршрут только при равенстве payload, иначе drop + обычный `register`. `end` (= root + `reserved_capacity`), `root`, `owner`, `kind` у той же reservation неизменны: `register_payload` получает `slot.reserved_capacity` (`alloc_core_large.rs:420–425`); `owner` = id heap-слота (`route_slots.rs:20–24`), постоянный для `AllocCore` в слоте.
8. **Смена владельца heap (lease).**
   - `AllocCore` и его кэш живут в слоте реестра всё время процесса (`lifecycle.rs:398–404`), owner id маршрута = id слота.
   - Удерживаемый маршрут переживает смену потока-арендатора так же, как живые маршруты таблицы. Новых вопросов нет.
9. **Провенанс.**
   - `RouteRegistration::root()` — allocator-origin root, сохранённый при регистрации (`directory.rs:28, 171–173`).
   - На hit код использует `slot.base` из `CachedLarge`, т.е. тот же root.
   - Producer root не получает (`pin.rs:4–5`). Удержание ничего не меняет.
10. **Census/диагностика.** `live_route_census_for_test` начнёт считать закэшированные Large. Тесты, сравнивающие глобальный census при параллельном Large-churn в том же бинарнике, могут стать нестабильными — проверить при реализации (`tests/r11_ph3a_*`, `tests/r11_p3_small_sidecar_issue_oom/support.rs`).

Вывод (ВЫВЕДЕНО, не доказано исполнением): OPT-06 корректен при четырёх условиях:
- (a) `lookup` фильтрует не-живые фазы Large;
- (b) payload-равенство или перерегистрация;
- (c) unlink до OS release на всех пяти путях;
- (d) own-deposit поглощает `PENDING`.

## §3 Варианты

| # | Механизм | Аргумент корректности | Выигрыш / цена (оценка по коду) | Риск | Что доказать/измерить |
|---|---|---|---|---|---|
| A | Ничего не менять; честно задокументировать цену в perf OPEN_ITEMS (новая карточка, ссылка на perf 70) | Поведение не меняется | 0 / 0 | Цена остаётся: 4 System-вызова, 2 лока, глобальный CAS на цикл; конкуренция за `next_incarnation` при MT Large churn | Ничего; только запись |
| B | **OPT-06: удерживать регистрацию в `CachedLarge`**, вести слово маршрута через `CACHED → INITIALIZING(g+1) → LIVE`. `lookup` для Large отвечает None вне `LIVE`/`PENDING`/`CONSUMING`; payload-равенство или перерегистрация; unlink до OS release | §2.4: фазы и поколение на независимом System-слове; producer CAS только из `LIVE`; owner-only переходы под lease; провенанс root неизменен | Минус 2 alloc + 2 free + 2 лока + глобальный CAS + `fetch_sub` на цикл; ≈ 300–700 Ir (оценка). Цена: +1 Acquire-load на foreign Large lookup; ≤ 40 удерживаемых `Entry`+`LargeState` на heap (8 + 32 слота) ≈ 40 × ~128 B ≈ 5 KiB/heap (оценка с учётом заголовков malloc) против ≥ 40 × 4 MiB закэшированного VA | Средний: 5 путей release, новый переход own-deposit, новая видимая фаза в директории, изменение census | Тесты §5; iai-пара; MT wall-clock; оракул регистраций = Δ`next_incarnation` |
| C | Пул дескрипторов: переиспользовать `Entry`/`LargeState` без System-alloc/free, но по-прежнему unlink/link в директории | Безопасно, только если пул принимает дескрипторы с последней ссылкой на **любом** потоке (последний `fetch_sub` может быть у producer, §2.2) → нужен потокобезопасный пул | Минус 4 System-вызова; остаются 2 лока, глобальный CAS, бинпоиски. Оценка ≈ 40–60% выигрыша B | Средний-высокий: новый конкурентный пул, ABA в пуле | Модель пула (loom), та же iai-пара |
| D | Ленивая регистрация при первом foreign-обращении | **Невозможна**: foreign free находит владельца только через маршрут (`routing.rs:27`). Без маршрута валидный foreign free дропается → утечка (perf 82: отрицательный ответ не авторитетен) | — | Корректность | Отвергнут по коду |
| E | Пакетная регистрация / отложенный unlink | Отложенный до eviction unlink и есть B; пакетный register при hit смысла не имеет (одна регистрация на hit) | ≈ B | ≈ B | — |
| F | Только убрать глобальный CAS: шардовый/потоковый incarnation | `incarnation` используется лишь в sanity-проверке `lookup` (`directory.rs:884`); уникальность нужна в пределах ключа. Шардовый счётчик под шард-локом сохраняет её | Минус 1 межъядерный RMW; Ir почти 0 | Низкий | MT wall-clock; иначе невидим |

## §4 Рекомендация

**Вариант B (OPT-06)**, реализуемый как `perf(runtime)` только после предрегистрированного гейта (§5.4). До измерения — запись карточки варианта A в `docs/perf/OPEN_ITEMS.md`, чтобы находка не потерялась.

Почему не остальные:
- **A** — законный fallback: если гейт даст NULL/NO-GO, остаёмся на нём.
- **C** даёт меньший выигрыш и требует потокобезопасного пула. Последний free дескриптора может случиться на чужом потоке, поэтому C сложнее B при меньшей пользе.
- **D** некорректен.
- **E** сводится к B.
- **F** — дешёвое дополнение, но ничего не даёт в Ir. B и так убирает CAS с hit-пути; на fresh-пути он остаётся.

Что в B остаётся недоказанным:
- реальная Ir/ns-экономия (разложение 300–700 Ir — оценка);
- отсутствие регресса census-зависимых тестов;
- полнота перечня путей release: grep нашёл 5. Будущий новый путь release без unlink даст `Duplicate`-OOM, поэтому нужен структурный барьер (§5.1, «единая функция»);
- Loom/модельное доказательство интерливингов 1–5 на реальных типах. Есть только `tests/r6_route_directory.rs` на standalone-директории.

## §5 План реализации

### 5.1 Код (эскиз)

1. **`src/registry/segment_route/large_state.rs`**: добавить `pub fn claim_owner(&self) -> Option<u64>` — owner-only `LIVE(g)→CONSUMING(g)`, иначе `PENDING(g)→CONSUMING(g)` (интерливинг 5). Также `pub fn is_issued(&self) -> bool` (фаза ∈ {LIVE, PENDING, CONSUMING}).
2. **`src/registry/segment_route/directory.rs::lookup`** (`:876–881`): для `RouteKind::Large` после проверки payload:
   ```text
   if matches!(entry.kind, Large) && !large_state_of(entry).is_issued() { return None; }
   ```
   Это чтение System-слова под шард-локом, reservation не трогается.
3. **`src/registry/segment_route/registration.rs`**: обёртки `claim_large_owner()`, `payload_addr()`. Удаление test-only статуса существующих `cache_large_consumed`/`begin_large_reuse`/`finish_large_reuse_after_reset`/`release_cached_large` не требуется — они становятся production-вызовами.
4. **Новый файл `src/alloc_core/segment/segment_table/parked_route.rs`** (один экспорт): `pub(crate) struct ParkedRoute(#[cfg(feature="alloc-global")] Option<RouteRegistration<'static>>)`. Без `alloc-global` — ZST. Методы: `none()`, `is_some()`, `release_before_unmap(self)` (drop регистрации). `mod.rs` — только `mod`/`pub use`.
5. **`segment_table_impl.rs`**:
   - `unregister_parking(&mut self, base) -> ParkedRoute`: как `unregister`, но `routes.take(slot_id)` вместо `remove`;
   - `register_payload_parked(&mut self, base, len, kind, payload, parked: ParkedRoute) -> Option<u32>`: если `parked` есть, `root()==base` и `payload_addr()==payload.addr()` — минуя `prepare`, сразу `put`; иначе drop `parked` и обычный путь.
   - **`route_slots.rs`**: `take(index) -> Option<RouteRegistration>`.
6. **`CachedLarge`** (`src/alloc_core/alloc_core/alloc_core_impl.rs:175–202`): поле `route: ParkedRoute`.
   - Deposit `mem_impl.rs:402–453`: `let parked = self.table.unregister_parking(base);` затем `parked.claim_owner()` (abort на None) и `parked.cache_consumed(g_route)`.
   - Foreign `alloc_core_large.rs:738–820`: маршрут уже `CONSUMING` после drain → `cache_consumed`.
   - В ветке «не допущен в кэш» — обычный `unregister` (drop) **до** `release_segment` (как сейчас).
7. **Hit** `alloc_core_large.rs:246–452`:
   - `slot.route.begin_large_reuse()`; при `None` (исчерпание) — drop route, путь без маршрута;
   - header writes;
   - `register_payload_parked(..., slot.route)`;
   - `finish_large_reuse` заголовка;
   - `finish_large_reuse_after_reset(g)` маршрута — **последним**, перед возвратом (публикация LIVE после table/header reset, контракт §5).
8. **Единая функция release закэшированного**: `AllocCore::release_cached_large(cached: CachedLarge)` = drop route → `release_cached` заголовка → `os::release_segment`. Её вызывают все 5 мест (§2.4.6). Структурный барьер: `CachedLarge` не `Copy`; поле `route` приватно и извлекается только этой функцией.

Сборки без `alloc-global` (standalone `AllocCore`, `routes == None`): `ParkedRoute` = ZST, поведение байт-в-байт прежнее.

### 5.2 Тесты (в `tests/`, без doctest)

Features: `production internals` (+ `bench-internals` для оракула).

1. `tests/r19_large_route_retained_cycle.rs`:
   - цикл alloc/free 4 MiB на `HeapCore` (`claim_leaked_heap`-путь) ×N;
   - assert: Δ`next_incarnation` = 0 на hit-циклах (новый read-only `RouteDirectory::incarnations_issued_for_test()` под `internals`+`bench-internals`, observer);
   - census Large = 1, пока блок в кэше.
   - Красный до: Δ = 1 на цикл; зелёный после.
2. `..._stale_foreign_free_of_cached_is_unroutable`: alloc → own free (в кэш) → foreign поток `dealloc` того же адреса → `FOREIGN_OR_UNROUTABLE_FREES` +1, census/кэш не изменены, следующий hit выдаёт тот же адрес и он `LIVE(g+1)`. Зелёный до и после (тест паритета). Мутант: убрать фильтр фазы в `lookup` → тест foreign `realloc` (п.3) краснеет.
3. `..._stale_foreign_realloc_of_cached_returns_null`: паритет «null» (сегодня null из-за отсутствия маршрута).
4. `..._valid_foreign_free_then_reuse`: поток B освобождает Large потока A → drain → кэш → hit → поток C освобождает новый блок → reclaim ровно один раз (exactly-once через `dbg_large_cache_hits` и счётчики reclaim).
5. `..._payload_change_reregisters`: free (align 8) → hit с `align = 64 KiB` (другой `hdr_aligned`) → foreign free нового блока **маршрутизируется** (не unroutable). Мутант: убрать проверку payload → краснеет (тихая утечка, счётчик unroutable +1).
6. `..._evict_unlinks_before_release`: заполнить кэш, форсировать eviction (`trim_current_thread`/budget) → census Large = 0 после eviction; затем `RouteDirectory::register` того же VA другим heap не даёт `Duplicate`. Мутант: unlink после release → п.6 краснеет (только детерминированно через census, не через гонку).
7. `..._own_deposit_absorbs_pending` (invalid-use hardening, `internals`): `pin.publish_large()` вручную, затем own free → нет abort, нет двойного reclaim.
8. Loom-модель (если реалистична): `LargeState` + lookup-фильтр, интерливинги 1, 4, 5 из §2.4 на настоящих атомиках `LargeState`.

### 5.3 Порядок коммитов (R30-12)

1. `docs(perf)`: карточка в `docs/perf/OPEN_ITEMS.md` (вариант A как текущее состояние, ссылка на этот отчёт; поле Next trigger = гейт §5.4). Если префикс `docs(perf)` не принят — `docs`.
2. `test`: тесты 2, 3, 7 (паритет, зелёные до изменения) + оракул `incarnations_issued_for_test` (`bench`-гейт, `bench-internals`).
3. `bench`: предрегистрация гейта (док `docs/perf/R19_LARGE_ROUTE_RETAIN_GATE.md` с порогами до измерения) + скрипт таблиц.
4. `perf(runtime)`: реализация B + тесты 1, 4, 5, 6 — только при GO; иначе `bench` с NO-GO/NULL-отчётом и без изменения `src`.
5. При изменении `production`-пути — обновление `npm run bench:table`/`npm run iai` в README/IAI_BASELINE в том же PR (правило CLAUDE.md).

### 5.4 Эскиз предрегистрации гейта

- **Entry point:** `HeapCore::alloc`/`HeapCore::dealloc` через `claim_leaked_heap` — существующая пара `large_cache_prefill_only_4mib`/`large_cache_hit_only_4mib`, тот же слой, что `#[global_allocator]` (`perf_gate_iai.rs:2257–2262`). Вторая ось — native wall-clock через настоящий `SeferAlloc` `GlobalAlloc`.
- **Источник:** A = `31db7f17` (или будущий base), B = временный commit-SHA кандидата. Иммутабельная identity до измерения (правило R29-6, п.7).
- **Primary (iai, `production bench-internals internals`):** Δcycle = hit − prefill.
  - GO-порог: Δcycle(B) ≤ Δcycle(A) − 200 Ir (нижняя граница оценки §2.1 с запасом; порог фиксируется до прогона).
  - Повторяемость: A1/A2 и B1/B2 из одного бинарника, плюс C0 — второй бинарник того же источника A. T = max(10, |C0 − A1|), урок R16 perf 83.
- **Затронутые, НЕ контроли** (направление ожидается ≤ 0; предрегистрировать как «affected»): `large_alloc_free_cycle` (`perf_gate_iai.rs:2226–2237`; по коду его Large-free под `alloc-decommit` ∈ `production` идёт через admission `mem_impl.rs:363–455` и при допуске депонирует → unregister исчезает; факт допуска в этом арме проверить оракулом, не измерялось), `large_cache_free_slot_search_*`, `realloc_grow`.
  - **Предложенный oxx kill-gate «`large_alloc_free_cycle` ±10 Ir» для этого изменения неверен по форме** — арм затронут.
- **Контроли (|Δ| ≤ T):** `small_churn_16b`, `churn_256b`, `alloc_magazine_*`, `cold_alloc_only_*` (Small route регистрация не меняется), mimalloc ΔIr = 0.
- **Path-activation oracle (на арм, вне таймируемого тела):**
  - (1) `dbg_large_cache_hits()` Δ = 1 на хвостовом alloc (как R32-7);
  - (2) Δ`incarnations_issued_for_test()` на хвостовом цикле: A = 1, B = 0;
  - (3) census Large = 1 в B после хвостового free, 0 в A.
  - Без (1)–(3) результат непригоден (правило R30-8).
- **Режим cost и benefit в одном арме:**
  - (R1) hit-доминируемый: 4 MiB alloc/free, hit-rate ≈ 100%;
  - (R2) eviction-доминируемый: рабочее множество > ёмкости кэша (≥ 41 различных размеров/объектов на heap), hit-rate ≈ 0%: выигрыш ≈ 0, цена — перенос unlink к eviction, ожидаемо нейтрально;
  - (R3) payload-переменный: чередование align 8 / 64 KiB → путь перерегистрации; цена — лишняя проверка.
  - Каждый арм отчитывает hit-rate (числитель/знаменатель), Δincarnation и RSS/VmHWM в том же процессе. Объединять R1-выигрыш с R2-ценой в одну Pareto-фразу запрещено.
- **Native MT (вторичный, subprocess-per-arm):** N ∈ {1, 4, 8} потоков, каждый — цикл 4 MiB alloc/free через `GlobalAlloc`, 9 процессов на арм, медиана ns/op + p99.
  - Условие GO по этой оси: не хуже A сверх шума, определённого A/A-прогоном.
  - Отдельно записать: `perf c2c` HITM на линии `next_incarnation` (Linux), если доступно.
- **Config evidence (R26-4):** resolved `large_cache` budget/slots на арм, `config_conflicts_total()` Δ = 0.

## §6 Что нельзя решить без исполнения

1. **Реальная доля маршрута в 1 303 Ir.** Эксперимент: `node scripts/iai.mjs large_cache_prefill_only_4mib large_cache_hit_only_4mib` на A и на временной ветке, где `register`/`remove` подменены no-op-заглушкой. Подмена только для измерения: unsound, не коммитится как runtime. Плюс callgrind `--dump-instr=yes` с инклюзивной стоимостью `RouteDirectory::register`/`remove`/`EntryHandle::drop` из одного прогона A — достаточно для атрибуции без кода.
   - Решение: если инклюзивная маршрутная доля < 200 Ir, B не оправдан → вариант A.
2. **MT-эффект `next_incarnation` и шард-локов:** native-прогон §5.4 (MT). Без него утверждение «глобальная точка конкуренции» — ГИПОТЕЗА.
3. **Census-тесты под параллельным запуском:** прогон затронутых тестовых файлов после реализации.
4. **Решения владельца:**
   - (a) допустимо ли, что закэшированный Large виден в директории (фильтруется фазой) — архитектурное изменение инварианта «маршрут = живая reservation в таблице»;
   - (b) поведение own-deposit на `PENDING` (поглотить vs abort как hardening-сигнал двойного free);
   - (c) пороги гейта §5.4 (200 Ir — предложение, не данность);
   - (d) нужен ли вариант F отдельно (ниже Ir-порога, виден только natively).

## §7 Проверено и НЕ дало вывода

- Точная атрибуция дельты 378 → 1 340 (`1f4ac4f3`..`3962933b`): в окне введены и маршруты (`efec9d16`, `1d881429`), и терминальные слова, и общий рост префикса примерно в 9 раз. Разделить без bisect нельзя.
- Цена glibc malloc/free для 8 B/72 B в Ir — только оценка. Логов с инклюзивной стоимостью `System.alloc` на этом пути нет.
- Нет данных о том, как часто в реальных нагрузках Large-hit сочетается с foreign free (доля пути §2.2 с отложенным free на потоке producer).
- Новых находок уровня P0–P2 не обнаружено. Одно наблюдение для плана, не дефект текущего кода: предложенный oxx kill-gate `large_alloc_free_cycle ±10` для OPT-06 некорректен, потому что этот арм затронут изменением (§5.4).
