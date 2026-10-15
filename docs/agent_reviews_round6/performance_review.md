# Performance-review `src/` (round 6): горячий путь, атомики, сканы и места для ускорения

Дата: 2026-10-15.

Исходное дерево: `132ab85bd2fa443cfa5c6e0fbdea46231c3beaaf` (все ссылки `файл:строка` даны по
этому коммиту, пути — относительно корня репозитория).

## 1. Метод и ограничения

- Только статическое чтение `src/` (плюс сверка с `benches/perf_gate_iai.rs`, `Cargo.toml`,
  `docs/perf/OPEN_ITEMS.md`, `docs/perf/OPEN_ITEMS_ARCHIVE.md`,
  `docs/perf/round-manifests/R20_MANIFEST.md`, `docs/agent_reviews_round5/performance_review.md`,
  `docs/reviews/2026-10-08-src-review-oxx-round-17-performance.md`).
- Ничего не собиралось и не запускалось: ни `cargo`, ни бенчмарки, ни `cargo asm`, ни проектные
  скрипты. Поэтому **каждое число выигрыша ниже — оценка** (помечено «оценка»), а не измерение.
  Подтверждённым чтением считается только то, что прямо видно в коде: какие функции вызываются,
  какие поля читаются/пишутся, какие атомарные операции стоят на пути.
- Состав `production` взят из `Cargo.toml:423`:
  `alloc-global, alloc-xthread, alloc-decommit, fastbin, alloc-segment-directory,
  primordial-lazy-commit`. `alloc-stats`, `hardened`, `virgin-zero-skip`, `bench-internals`
  в production не входят, поэтому их стоимость в оценках не учитывается.
- Перед составлением списка сверены закрытые/отвергнутые решения (раздел 6). Ничего из
  отвергнутого (C′/C''/C‴, TCACHE_CAP↑, FLUSH_N, отложенная очистка magazine-бита, DualBitmap,
  hint-ы `find_segment_with_free`, clz `class_for`, удержанный Large-маршрут и т.п.) повторно не
  предлагается.
- Предпочтение владельца — эффективные дефолты без новых feature-флагов. Ни одна находка ниже
  флага не требует.

## 2. Итог

1. **Запись владельца в заголовок сегмента (`owner_state`) в production — мёртвая работа.**
   Её не читает ни один production-путь: маршрутизация чужого free идёт через
   `Entry.owner` каталога маршрутов. Единственный читатель — тестовый наблюдатель
   `dbg_owner_id_for`. При этом `stamp_segment_owner` выполняется на каждом refill-промахе
   magazine (отдельный проход по блокам плюс табличный lookup на каждый новый сегмент) и на
   каждом Large alloc. Удаление безопасно, флага не требует и поглощает открытый item 72
   (двойной проход в `finish_magazine_refill`). → **R6-PF-1**.
2. **Самый дорогой путь в production — обнаружение кросс-поточных free.** Для routed-ядра
   (то есть для любого heap в production) отрицательному ответу директории доверять нельзя.
   Каждый refill-промах без свободного блока проходит все Small-сегменты и на каждом делает
   `swap(0, AcqRel)` по каждому слову pending-битмапа, от начала payload до `bump`: до
   ~4 000 locked RMW на полностью нарезанный сегмент. Все три отвергнутых варианта
   (C′/C''/C‴) переносили «сводку» в producer и проиграли по цене producer. Предлагается
   вариант **без дополнительных RMW producer**: эпоха pin-ов упаковывается в уже
   существующий счётчик `Entry.refs`. Владелец пропускает drain сегмента, если с прошлого
   полного скана не было ни одного pin. → **R6-PF-2** (наибольший потенциальный выигрыш и
   наибольший риск: протокол, нужен Loom).
3. Помимо этого найдены:
   - неограниченный O(L) Large-sweep на каждом Large alloc с лишним чтением заголовка
     каждого Large-сегмента (R6-PF-3);
   - две раздельные строки кэша и лишняя проверка в Tier-1 `own_cache` на каждом free
     (R6-PF-4);
   - per-record пересчёт инвариантов сегмента в owner-drain (R6-PF-5);
   - вероятные bounds-check-и на индексе magazine-слота в самом горячем пути — требует
     проверки asm (R6-PF-6);
   - деление на каждом refill-промахе при уже существующей таблице (R6-PF-7);
   - мелочи в `alloc_zeroed`, в worker-е обслуживания и на Large own-free (R6-PF-8…10).
4. Ранее поданные, но никем не взятые идеи R17 (OPT-01, OPT-03, OPT-04, OPT-05, OPT-07,
   OPT-08, OPT-11) по текущему коду **всё ещё актуальны** (раздел 5). OPT-01 и OPT-05 стоит
   взять в первую очередь после PF-1.

Находок по приоритетам: **High — 2, Medium — 4, Low — 4** (всего 10).

## 3. Сводная таблица

| ID | Приор. | Место (`файл:строка`) | Суть | Ожидаемый эффект (оценка) | Риск | Флаг |
|---|---|---|---|---|---|---|
| R6-PF-1 | High | `src/registry/heap_core/state/ownership.rs:40-86`; вызовы `src/registry/heap_core/alloc/hot.rs:76-86,374,603,646`, `src/registry/heap_core/free/realloc.rs:597`, `src/registry/heap_core/alloc/batch.rs:158,234,270`; сброс `src/alloc_core/large/alloc_core_large.rs:438-447` | `owner_state` пишется, но в production не читается; убрать stamp и `last_stamped_segment` | refill: −(3·n + ~20) Ir на промах; Large alloc: −~15–25 Ir; hot hit/free: 0 | Низкий (только тесты-наблюдатели) | нет |
| R6-PF-2 | High | `src/registry/segment_route/directory.rs:32,142-157,215-229`; `src/alloc_core/small/alloc_core_small/find_segment.rs:136-191,410-440,480-520`; `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:55` | эпоха pin-ов в `Entry.refs` → владелец пропускает drain сегмента без новых pin-ов | multiseg/growth/seg_cycle: с O(S·W) locked RMW до O(S) (оценка ×10 и более на этих ячейках); producer: ≤ 0 Ir | Высокий (протокол публикации) | нет |
| R6-PF-3 | Medium | `src/registry/heap_core/alloc/hot.rs:195-200,639-642`; `src/alloc_core/alloc_core/sidecar_drain.rs:297-322` (чтение `kind_at` в `:312`), `:327-378` (`:361`) | полный Large-sweep на каждом Large alloc + чтение заголовка каждого Large-сегмента ради abort-проверки | при L живых Large: с ~3–4·L промахов кэша до ≤4 проб на cache-hit; −1 промах/TLB на пробу | Средний (удержание RSS) | нет |
| R6-PF-4 | Medium | `src/alloc_core/segment/segment_table/segment_table_impl.rs:203-204,258-259,733-748,791-798,856-863` | ключи и корни `own_cache` в двух массивах + проверка `!is_null`; AoS + sentinel-ключ | −2 Ir и −1 строка L1 на каждый free/realloc/lookup | Низкий | нет |
| R6-PF-5 | Medium | `src/alloc_core/small/alloc_core_small_reclaim.rs:21-68`; циклы `find_segment.rs:159-174`, `sidecar_drain.rs:137-142,229-251` | per-record `kind_at`/`payload_start`/`block_size`/`bump_of`/`dec_live` в owner-drain; поднять на уровень скана сегмента | −~10–20 Ir на reclaimed record (оценка); +`%` → OPT-08 | Низкий–средний | нет |
| R6-PF-6 | Medium | `src/registry/heap_core/alloc/hot.rs:237,286`; `src/registry/heap_core/free/dealloc_own_base.rs:404,416` | вероятный bounds-check `slots[i]` (`count: u8` ⇒ компилятор не знает `< 16`) на каждом hit/push | −1..−2 Ir на hit и на free, если check есть (проверить asm) | Низкий | нет |
| R6-PF-7 | Low | `src/registry/heap_core/alloc/hot.rs:494-496,681-683` → `src/registry/heap_core/state/tcache.rs:127-139` | `REFILL_BYTE_BUDGET / block_size` — runtime `div` на каждом refill-промахе при готовой таблице `FREE_PARK_CAP` | −1 `div` (~20–40 тактов) на промах | Очень низкий | нет |
| R6-PF-8 | Low | `src/registry/heap_core/alloc/hot.rs:631-634`; `src/alloc_core/platform/node.rs:159-169` | `write_bytes` переменной длины → вызов `memset` в мелком `alloc_zeroed` | −~10–20 тактов на calloc-образный small alloc с неконстантным размером | Низкий | нет |
| R6-PF-9 | Low | `src/registry/heap_registry/claim.rs:126-151`; `src/registry/heap_slot.rs:150-244`; `src/global/maintenance_service.rs:29-30,144,167` | worker делает CAS FREE→MAINTENANCE без предварительной загрузки по OWNED-слотам; `state`/`generation` делят строку с началом `HeapCore` | убрать RFO-инвалидацию строки владельца каждые 10 мс | Очень низкий | нет |
| R6-PF-10 | Low | `src/registry/heap_core/free/dealloc_own_base.rs:384-396,549-551` → `src/alloc_core/alloc_core/mem/mem_impl.rs:233-250` | Large own-free повторно резолвит корень, который уже есть в `OwnedBlock` | −1 Tier-1 lookup (~8 Ir) на Large free | Очень низкий | нет |

## 4. Подробности

### R6-PF-1 (High) — мёртвый owner-stamp на refill и Large alloc

**Подтверждено чтением.**
- `stamp_segment_owner` (`src/registry/heap_core/state/ownership.rs:40-86`) делает
  `canonical_block_of(ptr)` (маска плюс Tier-1/Tier-2 lookup), сравнение с
  `last_stamped_segment` и Relaxed-загрузку `owner_state`. На промахе кэша stamp добавляет
  Acquire-загрузку и условную Release-запись.
- Читатели `owner_state` / `unpack_owner_id` по всему `src/`: сам `stamp_segment_owner`
  (`:58-60`, `:75-76`) и тестовый `dbg_owner_id_for`
  (`src/registry/heap_core/diag/queries.rs:92-98`). Других нет.
- Владельца чужого free определяет `pin.owner()` → `Entry.owner`
  (`src/registry/heap_core_xthread/routing.rs:35`, `src/registry/segment_route/directory.rs:165-167`).
  Комментарии самого stamp это подтверждают: «Foreign-free routing reads the independent
  descriptor, not this field» (`ownership.rs:78-79`).
- Точки вызова в production:
  - `finish_magazine_refill` (`hot.rs:76-86`): отдельный проход по `n ≤ 16` блокам с
    `segment_base_of_ptr` и сравнением, stamp на каждый новый сегмент. Затем второй проход
    ради `mark_magazine` (`:96-102`) — это item 72;
  - Large-ветка `alloc` (`hot.rs:373-375`);
  - Large-ветка `alloc_zeroed` (`hot.rs:645-647`);
  - promotion в `realloc` (`realloc.rs:597`, только `medium-classes`);
  - batch-api (`batch.rs:158,234,270`).

  На cache-hit в `alloc_large` (`alloc_core_large.rs:438-447`) `owner_state` перезаписывается
  в `OWNER_ID_NONE` только затем, чтобы последующий stamp его снова перезаписал.

**Предложение.** Убрать вызовы stamp из production-путей. Убрать поле `last_stamped_segment`
(`src/registry/heap_core/core.rs:208-209`) и сброс в `alloc_core_large.rs:438-447`. Сохранить
`owner_state` в заголовке как инертное поле, чтобы не трогать 144-байтовый layout-пин. В
`finish_magazine_refill` останется один проход (`mark_magazine`), и item 72 закрывается сам
собой. Тестовые наблюдатели `dbg_owner_id_for` / `dbg_last_stamped_segment` используются в
8 тестовых файлах (`tests/heap_core_tcache_stamp.rs`, `tests/stamp_cache.rs`,
`tests/r11_ph3b_kind_aware_entries.rs` и др.). Тесты, которые проверяют «указатель
принадлежит этому heap», перевести на `canonical_root_for`. Тесты самого stamp-кэша удалить
вместе с механизмом. Оба наблюдателя убрать из `scripts/verify-dbg-hook-safety.mjs`.

**Ожидаемый эффект (оценка).** Refill-промах: −(3·n + 15…25) Ir. Это порядка 1–2 % на
`cold_alloc_only_256x16b` (16 refill-ов на 256 alloc). Large alloc: −15…25 Ir на вызов.
Побочный эффект: stamp на Large-hit заполнял Tier-1 `own_cache` — `register_payload` его не
заполняет, — так что первый free этого Large переплатит Tier-2 probe. Чистый эффект для
Large-цикла ≈ 0…−10 Ir. Горячий hit и push magazine не меняются (stamp там уже убран P4/F7).

**Риск.** Низкий. Это удаление записи, которую никто не читает. Abort-проверку
«`canonical_block_of` вернул None» на только что выданном блоке можно оставить
`debug_assert!`-ом.

**План измерения.** iai, слоевой судья `HeapCore` / `SeferAlloc`:
- ячейки выигрыша — `cold_alloc_only_256x16b{,_2n,_4n}` (наклон N/2N/4N по правилу
  контрольной геометрии R20), `recycle_alloc_only_256x16b`, `large_alloc_free_cycle`,
  `large_cache_hit_only_4mib` минус `large_cache_prefill_only_4mib`;
- контроли ±T — `small_churn_16b{,_2n}`, `alloc_magazine_hit_only_16b`,
  `dealloc_free_only_16b`;
- mimalloc-ячейки — ровно 0 (кроме `mimalloc_bootstrap_proxy`, см. item 89).

Oracle активации пути: предсказание ΔIr ∝ числу refill-промахов (у пары prefill/hit оно
известно из кода) и ∝ числу Large alloc. Наклон по N обязан быть линейным.

### R6-PF-2 (High) — пропуск owner-drain по эпохе pin-ов, без новых RMW у producer

**Подтверждено чтением.**
- Routed-ядро не доверяет отрицательному ответу директории (`find_segment.rs:410-440`,
  `trust_negative = !self.table.is_routed()`). После промаха директории или без неё
  выполняется линейный проход по всем активным Small-сегментам (`:481-520`). Каждый
  кандидат сначала проходит `drain_segment_sidecar` (`:506`), и даже кандидаты директории
  дренируются до проверки `BinTable` (`:729`).
- `drain_segment_sidecar` (`:136-191`) идёт по словам pending от начала payload до `bump`.
  На каждое слово, включая пустые, production делает `pending.swap(0, AcqRel)`
  (`bitmap_scan.rs:55`). Для полного 4 MiB-сегмента это ~4 024 слова: 4 096 минус 72 слова
  метаданных (`small_meta_end() = 73 728 B`, вычислено из
  `src/alloc_core/segment/segment_header/segment_header_layout.rs:13-76`).
- `refill_class_bump_impl` вызывает `find_segment_with_free` один раз на refill
  (`src/alloc_core/small/alloc_core_small_magazine.rs:246`, затем latch). В growth-режиме,
  когда своих free нет, каждый refill платит O(S·W) locked RMW. Это та самая стоимость,
  которую мерили cells PRF-01 (multiseg B/A 0.079 у C‴ — по сути «насколько дорог main»).
- Producer (`routing.rs:26-74`):
  1. `lookup` берёт лок шарда;
  2. `pin_from_array` наращивает `Entry.refs` через `fetch_update` (load + CAS-цикл,
     `directory.rs:142-157`);
  3. `publish` → `fetch_or` слова pending;
  4. drop pin → `refs.fetch_sub(1, AcqRel)` (`directory.rs:215-229`).

  Каждая публикация лежит строго между инкрементом и декрементом `refs`.

**Предложение (новый механизм, не C′/C''/C‴).** Упаковать в `Entry.refs: AtomicUsize`
две величины:
- младшие биты (например, 24) — текущее число держателей, как сейчас;
- старшие (40) — монотонную «эпоху» захватов pin.

Изменения:
- Pin: `fetch_add(1 + EPOCH_ONE, AcqRel)` с abort, если младшая часть переполнилась
  (по образцу `Arc`). Это одна `lock xadd` вместо `fetch_update` (load + `lock cmpxchg` +
  возможный повтор).
- Unpin: `fetch_sub(1)` без изменений. Проверка «последний держатель» сравнивает
  младшую часть с 1.
- Владелец хранит owner-only `last_epoch[slot]` в `RouteSlots`, не в памяти producer.
- Перед **полным** drain сегмента владелец делает **RMW-снимок** `refs`
  (`fetch_add(0, AcqRel)`: RMW читает последнее значение в modification order). Если
  младшая часть равна базовой (1 — регистрация владельца), он запоминает эпоху E, иначе
  ставит INVALID.
- На следующем кандидате тот же RMW-снимок: если «держатели = база и эпоха = E», drain
  пропускается.
- Bounded/strict-пути (`drain_sidecar_ingress_bounded`, `drain_sidecar_ingress`) `last_epoch`
  не обновляют и не используют. Контракт strict trim R6-03 («No dirty hint …»,
  `sidecar_drain.rs:186-193`) не меняется. Переход strict trim на пропуск — отдельный гейт.
- Периодический принудительный полный drain (как `DIRECTORY_MISS_FULL_SCAN_PERIOD`)
  ограничивает теоретический ABA при переполнении эпохи (2^40 захватов между двумя сканами
  одного сегмента).

**Набросок доказательства (оценка, требует Loom/рецензии).** Пусть публикация P не
поглощена сканом S, начавшимся снимком R1 с эпохой E и держателями = база. Pin для P
захвачен либо до R1 в mo(`refs`), либо после.
- **До R1, и pin уже отпущен.** Release unpin синхронизируется с RMW R1, поэтому
  `fetch_or` P happens-before R1, а R1 sequenced-before swap слова. Swap видит бит, и P
  поглощена — противоречие.
- **До R1, но pin не отпущен.** Тогда держателей больше базы, `last_epoch` = INVALID, и
  пропуска не будет.
- **После R1.** Эпоха в mo(`refs`) ≥ E+1, следующий RMW-снимок это увидит, пропуска нет.

Пропуск только откладывает reclaim: опубликованная запись держит credit сегмента, и сегмент
не освободится. Ошибка протокола проявится удержанием памяти, а не UB. Но доказательство
всё равно обязательно.

**Ожидаемый эффект (оценка).**
- Без кросс-поточных free (growth, multiseg_cold) каждый пропущенный сегмент стоит
  1 locked RMW вместо ~4 000. Ячейки `multiseg_cold_256k`, `seg_cycle_decommit_256k`,
  `growth16_*` должны упасть на величину порядка «C‴ benefit» или ниже: C‴ давал
  multiseg B/A 0.079, здесь пропускается весь скан, а не только пустые слова.
- Producer: ≤ 0 Ir на free, потому что CAS-цикл заменяется одной `xadd`. Бюджет Gate P
  (+6/+10 Ir на free) выполняется по построению; проверить мерой.
- Цена владельца при активных producer-ах: +1 RMW на строку `Entry`, которую producer-ы
  и так тянут.

**Риск.** Высокий: меняется кодировка счётчика жизни дескриптора и вводится условный пропуск
в протоколе терминальной публикации (`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`).
Условие переоткрытия item 82 (`docs/perf/OPEN_ITEMS_ARCHIVE.md` § 82 «Reopen trigger»: дизайн,
убирающий работу сводки из пути producer) этот вариант, по-видимому, выполняет. Решение
за владельцем.

**План измерения.**
1. Loom на actual-source (`tests/loom_sidecar_bitmap_actual.rs`, расширить на `Entry.refs`)
   плюс counterfactual-мутант «снимок Acquire-load вместо RMW».
2. Новая преррегистрация по образцу `docs/perf/R20_PRF01_C3_*` с теми же ячейками:
   - выигрыш — `multiseg_cold_256k`, `seg_cycle_decommit_256k`, наклон `growth16_n/2n/4n`;
   - цена — `r19_prf01_joined_producer_same_word`, distinct-word, `r19_prf01_publication`
     net, construction-ячейка;
   - hot-семьи — по наклону N/2N.
3. Oracle активации: `bench-internals`-счётчики «drain пропущен / выполнен» плюс
   существующий `Prf01SidecarOracle`. В growth доля пропусков должна быть ≥ 95 %, в
   producer-ячейках пропусков 0.
4. Нативная ось (fan-in latency, `perf c2c` HITM) — на тихом хосте, как и было отложено
   для PRF-01.
5. С OPT-01 (раздел 5) механизм композируется: если пропуск не сработал, пустые слова
   читаются load-ом, а не swap-ом.

### R6-PF-3 (Medium) — неограниченный Large-sweep на каждом Large alloc

**Подтверждено чтением.**
- `alloc_with_class` при `class.is_none()` вызывает полный `drain_large_sidecar_ingress()`
  (`hot.rs:195-200`), `alloc_zeroed` — тоже (`hot.rs:639-642`).
- Полный sweep (`sidecar_drain.rs:297-322`) проходит **все** активные Large-слоты. На
  каждом он делает:
  - `base_at`;
  - `SegmentHeader::kind_at(base)` — чтение заголовка Large-резервации, то есть отдельной
    страницы, только ради `abort` (`:312`);
  - `claim_large_route` → разыменование слота `RouteSlots`, сравнение `root` и
    Acquire-загрузку слова `LargeState` в отдельной System-аллокации (`route_slots.rs:201-213`,
    `src/alloc_core/large/reservation_state.rs:58-73`).

  Итого ~3–4 зависимых промаха на каждый живой Large-объект, на каждом Large alloc. Это
  касается и cache-hit, где сама выдача стоит ~сотни Ir (R32-7).
- Hot-bounded вариант на refill-промахе (`:327-378`, `LARGE_HOT_BUDGET = 4`) тоже читает
  заголовок (`:361`).

**Предложение.**
- (а) Заменить `kind_at`-проверку в обоих sweep-ах на `debug_assert!`. Инвариант уже
  обеспечен `ActiveKindIndex` (слот в Large-наборе) и сравнением `route.root() == base`.
- (б) На Large alloc перед поиском в кэше делать ограниченную пробу тем же курсором
  `large_hot_cursor` / `LARGE_HOT_BUDGET`. Полный sweep — только на промахе кэша, перед
  OS-резервированием (с повтором поиска в кэше, если sweep что-то вернул), и в A1-пути
  `realloc`.

  Свойство «не резервировать свежую память, пока есть непоглощённый Large-free»
  сохраняется. Ограничение item 78(c) («lossy hint не заменяет sweep») не нарушается:
  sweep не заменяется, а переносится на промах.

**Ожидаемый эффект (оценка).** Для приложения с L живыми Large-буферами Large cache-hit
дешевеет с O(L) промахов до ≤ 4 проб: при L = 100 это порядка 30–40 мкс → < 1 мкс на
вызов. (а) в одиночку убирает 1 промах кэша и TLB на каждую пробу, в том числе на refill-промахе
малых классов, пока у heap есть живые Large.

**Риск.** (а) — очень низкий. (б) — средний: удалённо освобождённый Large-сегмент может
дольше ждать поглощения, пока идут cache-hit-ы (удержание RSS), до ближайшего промаха,
refill-пробы, maintenance или trim.

**План измерения.** Новая iai-ячейка: K = 0/4/16/64 живых Large плюс цикл Large alloc/free с
cache-hit, ΔIr на вызов против K (ожидаемый наклон ≈ 0 после (б)). Oracle —
существующий `LARGE_SIDECAR_SLOT_INSPECTIONS` (`sidecar_drain.rs:8-9`): было K на alloc,
станет ≤ 4. Ось цены — в том же режиме (правило R31-1): кросс-поточный Large-free с
subprocess-изоляцией, `segments_reserved_total`, RSS, число непоглощённых Large-публикаций.
Сигнал R20 «в бенчах репозитория 0 Large-запросов» означает, что без новой ячейки эффект не
виден ни в одном судье.

### R6-PF-4 (Medium) — раскладка Tier-1 `own_cache` на каждом free

**Подтверждено чтением.**
- `own_cache: [*mut u8; 16]` и `own_cache_keys: [usize; 16]` — два отдельных массива по
  128 B (`segment_table_impl.rs:203-204`).
- Hit-проверка (`:733-748`, `:791-798`) читает ключ из одного массива и корень из другого
  (разные строки L1) и дополнительно проверяет `!own_cache[index].is_null()`.
- Путь вызывается на каждом free (`mem_impl.rs:36-51` ← `routing.rs:11`), в `realloc` и
  в flush.
- Инвариант `ключ ≠ 0 ⟺ корень ≠ null` поддерживается заполнением (`:745-746`) и
  очисткой (`:856-863`). Проверка на null нужна только для запроса с ключом 0 (указатель
  ниже 4 MiB).

**Предложение.** Массив пар `[(usize, *mut u8); 16]` (16 B на запись, четыре на строку).
Пустой ключ = `usize::MAX`: это не SEGMENT-выровненный адрес, поэтому совпадения быть не
может. Тогда hit = одно сравнение ключа, корень лежит в той же строке, ветка `is_null`
исчезает. `own_cache_clear` пишет `usize::MAX`.

**Ожидаемый эффект (оценка).** −2 Ir (test + branch) и −1 строка L1 на каждый lookup.
По R22-17 Tier-1 hit стоит ~8,2 Ir, то есть это порядка 20–25 % стоимости `contains_base`.
Строит HeapCore немного дороже: заполнение 16 ключей не нулём — +~16 записей на
конструирование (оценка).

**Риск.** Низкий: локальное изменение одного типа, инвариант проще прежнего.

**План измерения.** iai `dealloc_contains_base_probe_only_16b`,
`dealloc_free_only_16b{,_n8,_n16}`, `small_churn_16b{,_2n}` (ожидаемо −1…−2 Ir на
операцию); EstCycles/L1 для оси раскладки; construction-ячейка — односторонний допуск;
mimalloc-ячейки — 0. Oracle: Tier-1 hit-rate из `CONTAINS_BASE_TIER1_HITS/_MISSES`
(`bench-internals`) не должен измениться.

### R6-PF-5 (Medium) — per-record инварианты в owner-drain

**Подтверждено чтением.** `reclaim_sidecar_record` (`alloc_core_small_reclaim.rs:21-68`)
на каждой записи заново выполняет:
- `kind_at(base)` и проверку kind;
- вычисление `payload_start`;
- `block_size(class)` — проверенный индекс таблицы;
- `off.is_multiple_of(block_size)` — деление на переменный делитель, это OPT-08;
- `meta.bump_of()`;
- `dec_live()`.

Все вызывающие циклы перебирают записи **одного** сегмента (`find_segment.rs:159-174`,
`sidecar_drain.rs:137-142`, `:229-251`). Сравните `flush_run`
(`src/alloc_core/small/alloc_core_small_magazine.rs:567-669`): там те же инварианты
подняты на уровень run-а (Э8), а credit-ы списываются одним `sub_live`.

**Предложение.** Разделить reclaim на «контекст сегмента» (kind, `payload_start`, `bump`,
`SegmentMeta`, вычисленные один раз на скан) и «запись». Накапливать число принятых записей
и вызывать `sub_live(k)` один раз в конце скана, как в `flush_run`. Это корректно: между
записями одного скана bump и kind не меняются, потому что скан — owner-only операция без
carve. Делимость — по OPT-08 (таблица магических констант на класс).

**Ожидаемый эффект (оценка).** −10…20 Ir на поглощённую запись, плюс устранение одного
`div` (~20–40 тактов нативно) на запись. Влияет на пропускную способность producer→consumer.

**Риск.** Низкий–средний: abort-проверки остаются, меняется только место вычисления.
Поведение `sub_live` при частичном отказе (abort) прежнее.

**План измерения.** iai `r19_prf01_publication` (N публикаций → drain), наклон по числу
записей; native — `benches/heap_fanin_production.rs` и producer/consumer-харнесс R32-11.
Oracle: `SIDECAR_INGRESS_RECORDS_CONSUMED` (`sidecar_drain.rs:56-62`) равен числу
публикаций.

### R6-PF-6 (Medium, условно) — bounds-check индекса magazine-слота

**Подтверждено чтением.**
- `count` имеет тип `u8` (`src/registry/heap_core/state/tcache.rs:235`).
- В hit-арме `self.tcache.classes[c].slots[new_cnt]` (`hot.rs:286`) при `new_cnt = cnt - 1`
  компилятору неизвестно, что `new_cnt < 16`.
- В push-арме `slots[cnt]` (`dealloc_own_base.rs:416`) условие `cnt < cap`, где
  `cap = FREE_PARK_CAP[c] as usize` (`:404`) — тоже `u8`.

Без asm нельзя утверждать, что LLVM не вывел границу. Это **гипотеза**.

**Предложение.** Сначала `cargo asm` для `HeapCore::alloc` (hit-арм) и
`dealloc_own_thread_with_base` (push-арм) в `--release --features production`. Если там есть
`cmp $0xf` / `jae` → `panic_bounds_check`, индексировать через `idx & (TCACHE_CAP - 1)`.
Это эквивалентно по инварианту `count ≤ TCACHE_CAP` и не требует `unsafe`. Плюс
`const_assert!(TCACHE_CAP.is_power_of_two())`. Индекс `classes[c]` (49 классов, не степень
двойки) не трогать: расширение до 64 ломает стековый пин `HeapCore ≤ 9216 B`
(`core.rs:283`).

**Ожидаемый эффект (оценка).** −1…2 Ir на hit (~22,4 Ir) и на push. Если check-а нет —
NULL, закрыть.

**Риск.** Низкий.

**План измерения.** asm-дифф; iai `alloc_magazine_hit_only_16b` минус prefill,
`dealloc_free_only_16b_n8/_n9/_n16`, `small_churn_16b{,_2n}` (наклон), mimalloc 0.

### R6-PF-7 (Low) — деление на каждом refill-промахе

**Подтверждено чтением.**
- `refill_magazine_slow` и `refill_magazine_slow_virgin` вычисляют
  `refill_n_for_class(SizeClasses::block_size(c))` (`hot.rs:681-683`, `:494-496`), то есть
  runtime-деление `REFILL_BYTE_BUDGET / block_size` с clamp (`tcache.rs:127-139`).
- Таблица `FREE_PARK_CAP[c]` (`tcache.rs:176-186`) по своему doc равна ровно этому значению.

**Предложение.** `let want = FREE_PARK_CAP[c] as usize;`.

**Эффект (оценка).** −1 `div` (~20–40 тактов нативно; в Callgrind −3…5 Ir) на промах.

**Риск.** Очень низкий; равенство уже задокументировано.

**Измерение.** Наклон `cold_alloc_only_256x16b{,_2n,_4n}`; контроль hot = 0.

### R6-PF-8 (Low) — `memset` в мелком `alloc_zeroed`

**Подтверждено чтением.**
- В production (`virgin-zero-skip` выключен) Small-арм `alloc_zeroed` зовёт
  `Node::zero(ptr, size)` (`hot.rs:631-634`), то есть `write_bytes` переменной длины
  (`node.rs:159-169`).
- iai-ячейка `alloc_zeroed_magazine_hit_only_16b` использует константный layout. По
  `docs/perf/TASK1999_ALLOC_ZEROED_STUB_GAP_summary.csv`: (9098 − 8707) / 16 = 24,4 Ir на
  hit против 22,4 у plain-hit. Это означает, что `memset` там свёрнут в константу, и ячейка
  **не видит** вызов, который случится за ABI-границей `__rust_alloc_zeroed`.

**Предложение.** Для `size ≤ 128` обнулять `align_up(size, 16)` байт фиксированным циклом
16-байтовых записей. Это не выходит за `block_size(c)`: все классы кратны 16, а блок
выровнен на 16. Иначе — `write_bytes`.

**Эффект (оценка).** −10…20 тактов на вызов с неконстантным размером. Ir-ось может
показать ±0.

**Риск.** Низкий.

**Измерение.** Нужна ячейка с `black_box(size)` через реальный `#[global_allocator]` (правило
слоя), wall-clock и Ir; oracle — счётчик путей «inline-zero vs memset».

### R6-PF-9 (Low) — worker обслуживания и раскладка `HeapSlot`

**Подтверждено чтением.**
- `try_maintenance_at` (`claim.rs:126-151`) после загрузки `initialised` сразу делает
  `cas_state(FREE → MAINTENANCE)` и на OWNED-слотах этот CAS всегда проигрывает. На x86
  неудачный `lock cmpxchg` всё равно забирает строку в Exclusive.
- `claim` уже использует TTAS-форму (`:278-298`).
- `HeapSlot` (`heap_slot.rs:150-244`): `state` и `generation` стоят в начале, `heap`
  (`HeapCore`, `repr(Rust)`) — со смещения 16. Значит, первые ~48 B `HeapCore` делят строку
  со `state`. Какие поля `HeapCore` туда попадают, статически неизвестно (переупорядочивание
  `repr(Rust)`).
- Worker посещает 32 слота каждые 10 мс (`maintenance_service.rs:29-30,144,167`). Сам
  worker opt-in (`start_maintenance`).

**Предложение.** Загружать `state` (Relaxed) перед CAS и пропускать слот, если он не FREE.
Опционально выровнять `heap` на 64 (`#[repr(align(64))]`-обёртка), чтобы
`state`/`generation`/`initialised` не делили строки с владельцем.

**Эффект (оценка).** Убирает до 3 200 RFO-инвалидаций строк владельцев в секунду
(32 слота / 10 мс). Влияние мало, но изменение бесплатное.

**Риск.** Очень низкий.

**Измерение.** Только нативно: `perf c2c` / HITM на многопоточном churn с запущенным и
выключенным worker; Ir нейтрален.

### R6-PF-10 (Low) — повторный lookup на Large own-free

**Подтверждено чтением.** `dealloc_own_thread_with_base` получает `OwnedBlock {root, block}`
(`dealloc_own_base.rs:347-348`). Для Large (`RouteToLargeFree`, `:384-396`, и хвост
`:549-551`) он вызывает `self.core.dealloc(block, layout)`, а тот снова делает
`canonical_block_of` (`mem_impl.rs:239`). R19 сознательно оставил «Large own free 3 → 2»
lookup-а.

**Предложение.** Сделать `pub(crate) unsafe fn dealloc_at_base` доступным HeapCore и
передавать `owned.root()`.

**Эффект (оценка).** −1 Tier-1 lookup (~8 Ir) на Large free.

**Риск.** Очень низкий.

**Измерение.** `large_alloc_free_cycle`, `large_cache_hit_only_4mib`; контроль small = 0.

## 5. Ранее поданные непроверенные идеи — актуальность по текущему коду

Ни одна не измерялась, все остаются без владельца (item 88 / item 70 / item 25). Ниже —
подтверждение, что механизм по-прежнему в коде, и рекомендуемый приоритет.

| Идея (источник) | Где сейчас | Состояние | Рекомендация |
|---|---|---|---|
| R17-OPT-01: загрузка перед swap пустого слова | `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:55` (production — безусловный `swap`; load-ветка только под `r18_sidecar_scan_bench`, `:42-53`) | актуально | **High** среди старых: только владелец, без флага. Ir-судья покажет +Ir (R18: +32 Ir на сценарий), поэтому судить нужно нативно или по EstCycles; нужен Loom для strict trim |
| R17-OPT-05: проверка маршрута раз на партию | `src/alloc_core/segment/segment_table/segment_table_impl.rs:316-333`, `route_slots.rs:137-179`, `src/registry/segment_route/small_sidecar.rs:91-102`, `issue_transaction.rs:108-122` | актуально: `prepared` вычисляется ≥ 3 раз на блок, `segment_id_at`/`base_at`/`root` — дважды. В `try_drain_freelist_batch` цепочка проходится дважды (`alloc_core_small_impl.rs:561-641`); для freelist-блока код класса уже записан, commit — чистая проверка | **Medium–High**: оценка R17 — 20–50 Ir на блок refill |
| R17-OPT-03: не дренировать при `live_count == 0` / сначала `BinTable` | `find_segment.rs:506,729` | актуально (drain всегда до проверки head) | Medium; вместе с R6-PF-2 |
| R17-OPT-04: двойная инициализация `SmallSidecar` | `src/registry/segment_route/directory.rs:56-65` (`alloc_zeroed` + `initialize_at`) | актуально | Low: ~6 тыс. записей на сегмент; первое касание 41 KiB |
| R17-OPT-07: foreign free — CAS-лок без TTAS, `fetch_update` на `refs`, второй `find` | `src/registry/segment_route/shard_lock.rs:57,80`, `directory.rs:142-157,926` | актуально | Medium (item 70); `fetch_update` → `fetch_add` входит в R6-PF-2 |
| R17-OPT-08: делимость без `div` | `alloc_core_small_reclaim.rs:39` | актуально | Low–Medium; входит в R6-PF-5 |
| R17-OPT-09: стек `PerClass` вниз | `tcache.rs:228-286` | актуально | Low (не трогать CAP/FLUSH_N) |
| R17-OPT-10: бюджет фона по непустым cut | `sidecar_drain.rs:88-184` | актуально | Low; зависит от OPT-01 |
| R17-OPT-11: второй Large-sweep в одном `realloc` | `realloc.rs:191-194` + move-leg → `hot.rs:197-199` | актуально | Low; после R6-PF-3(б) теряет смысл |
| item 72: двойной проход `finish_magazine_refill` | `hot.rs:76-102` | актуально | **поглощается R6-PF-1** |
| item 25: `virgin-zero-skip` в production | `hot.rs:584-636` | opt-in | вне темы этого ревью; владелец предпочитает дефолт без флага, решение по item 25 |
| item 64: `FREE_PARK_CAP` без гейта | `dealloc_own_base.rs:404-460` | без измерения | оставить по триггеру item 64 |

## 6. Что сознательно не предлагается (отвергнуто или закрыто)

- Сводка удалённых публикаций с RMW у producer: C′ (R19), C'' и C‴ (R20) — NO-GO по цене
  producer; item 82 закрыт, флаг-фолбэк отвергнут владельцем. R6-PF-2 отличается тем, что
  **не добавляет** producer-у ни одной RMW.
- Отложенная или пакетная очистка magazine-бита на hit (R3, item 17) — отклонено по
  корректности.
- `TCACHE_CAP` 16→32/64/128 (X4, item 18) и sweep `FLUSH_N` (R25-3, item 44) — NO-GO.
- Слияние `AllocBitmap` и `MagazineBitmap` (F1b, item 45) — NO-GO по Ir. Чисто
  локальностный F1 ждёт ≥ 64-сегментного макро-бенча (item 34).
- Hint-ы и битмапы `find_segment_with_free` (X5, T10, R1; items 20/22/23) — NO-GO до
  ≥ 64-сегментного судьи.
- clz-`class_for` и гибрид LUT (X6, item 19) — NO-GO.
- Ленивый staging-массив (R26-7) и тюнинг `flush_class` (R28-1) — область признана
  исчерпанной.
- Удержанный Large-маршрут в кэше (PRF-03, item 86) — закрыт.
- Large shrink in place (item 83) — NO-GO по контролю.
- Сайдкары `usable_size`/`seq` large-cache (F8 (1)/(3)) — не строились сознательно.
- Reservation-only overflow tier (item 16) — honest reject.
- Повышение `OWN_CACHE_SIZE` уже сделано (F2). R6-PF-4 меняет раскладку, а не размер.

## 7. Рекомендуемый порядок

1. **R6-PF-1** — сразу: дёшево, детерминированный Ir-гейт, закрывает item 72.
2. **R6-PF-7**, **R6-PF-10**, **R6-PF-3(а)** — тривиальные правки одним iai-гейтом.
3. **R6-PF-6** — сначала asm; дальше по результату.
4. **R6-PF-4** — отдельный гейт (hot-путь free, kill-порог ±10 Ir).
5. **R17-OPT-05** — пакетный issue: наибольший оставшийся выигрыш на refill.
6. **R6-PF-2** вместе с **R17-OPT-01** — после решения владельца по переоткрытию
   протокола: сначала Loom, затем новая преррегистрация по каркасу C‴.
7. **R6-PF-5**, **R6-PF-3(б)**, **R6-PF-8**, **R6-PF-9** — по триггерам кросс-поточной и
   Large-нагрузки, с нативной осью и правилом одного режима для цены и выгоды.
