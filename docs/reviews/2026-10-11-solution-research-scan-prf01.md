# Исследование решения: стоимость owner-скана pending-bitmap (R17-PRF-01, perf item 82)

База: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`).
Режим: только чтение; ничего не компилировалось и не запускалось. Все числа —
из закоммиченных логов/CSV с указанием файла:строки, либо помечены
«оценка по коду, не измерение».

## §0 Чек-лист хода

- [x] Шаг 0: база проверена (`31db7f17`)
- [x] Первоисточники: R17 performance (oxx) §PRF-01, xxs P2 + R18 follow-up, perf item 82, R35 gate + CSV
- [x] Текущий код скана и вызывающие (`bitmap_scan.rs`, `sidecar_bitmap.rs`, `find_segment.rs`, `alloc_core_small_magazine.rs`)
- [x] Историческое окно: причина = `a4245965` (O(1) guard кольца → фиксированный скан до high-water)
- [x] Сверка «слов × Ir/слово» с логами (R35 oracle counts × 12 Ir/слово из PG3R)
- [x] Почему R35 не показал выигрыша (Ir слеп к замене swap→load; +7 Ir/слово инструментовки)
- [x] Протокол: контракт публикации, удалённый dirty hint, Loom-модель (§2.4)
- [x] Варианты и корректность (§3)
- [x] Рекомендация, план, эксперимент (§4–§7)

## §1 Постановка

### 1.1 Что не исправлено (перечитано на `31db7f17`)

- `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:11` — doc «One cut is taken per word, including empty words».
- `bitmap_scan.rs:38–73` `BitmapScan::next_cut`: в production-сборке (без cfg `r18_sidecar_scan_bench`) — безусловный `pending.swap(0, Ordering::AcqRel)` на `:57`. Ветка-кандидат prefilter (`load(Acquire)==0 → skip`) — `:48–55`, только под `all(feature="bench-internals", r18_sidecar_scan_bench)`.
- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:121–145` `scan/scan_from`: `end_word = high_water.div_ceil(MIN_BLOCK*64)` (`:135`), т.е. 1 слово = 1 KiB payload; `WORDS = 4096` на 4 MiB сегмент (`:22–28`).
- Продюсер: `sidecar_bitmap.rs:110–119` `publish` — `classes.encoded(granule)` (Acquire) до RMW, затем единственный терминальный `fetch_or(.., AcqRel)` (`:117`); «No sidecar access is permitted after this RMW» (`:106`).
- Discovery: `src/alloc_core/small/alloc_core_small/find_segment.rs:136–174` `drain_segment_sidecar` — `high_water = bump_of()` (`:141`), диапазон `[payload_start_word, ceil(bump/1KiB))` (`:145–146`), цикл `next_cut`/`pop` (`:155–161`).
- `find_segment.rs:400–403` — `trust_negative = !self.table.is_routed()`; routed-ядро всегда идёт в полный линейный обход.
- `find_segment.rs:465–505` — обход всех активных Small/Primordial; drain кандидата (`:491`) ДО проверки `bt.head(class_idx)` (`:506–508`). То же в `validate_directory_candidate` (`:714` до `:731`).
- Частота: `src/alloc_core/small/alloc_core_small_magazine.rs:204–261` — `free_exhausted` латчится только на один refill; `find_segment_with_free` (`:246`) вызывается раз на refill magazine, если freelist текущего сегмента пуст.

### 1.2 Что уже известно и НЕ переоткрывается

- perf item 80 (CLOSED, `18d76472`, патч S): скан начинается с первого payload-слова. Остаток «пропуск пустых payload-слов» — не реализован (`docs/perf/OPEN_ITEMS_ARCHIVE.md` § 80, по xxs P2 `:19`).
- perf item 82 (OPEN, `docs/perf/OPEN_ITEMS.md:3044–3049`): доверие отрицательному ответу директории при routed-таблице — НЕ СЕЙЧАС; R18 измерил prefilter, не продвинул.
- R35 (`docs/perf/R35_SIDE_CAR_SCAN_PREFILTER_GATE.md`): кандидат prefilter, 3 сценария, 99/275/92 скана, +32 Ir, −4% Callgrind-cycles; вердикт не выносился.
- perf item 70: продюсерская ось (fan-in/contention терминального sidecar) — отдельна и OPEN.
- xxs-синтез: PRF-01 = P2 «структурная цена», исторические ×132–178 — не текущее измерение.

## §2 Механизм (НАБЛЮДЕНО vs ВЫВЕДЕНО)

### 2.1 Что вызвало историческое удорожание

НАБЛЮДЕНО (логи):

| Точка | multiseg_cold_256k Ir | seg_cycle_decommit_256k Ir | Источник |
|---|---:|---:|---|
| до cutover (2026-09-22) | 30 394 | 72 256 | `docs/perf/_raw_task1999_alloc_zeroed_stub_gap_after_1982.log:504,511` |
| после cutover, без патча S (Ph3a before) | 4 200 108 | 12 858 834 | `docs/perf/_raw_ph3a_iai_before.log:490,497` |
| после патча S (PG3R base) | 4 028 057 | 12 375 007 | `docs/perf/_raw_pg3r_iai_base.log:488,495` |
| Ph6b cand (`fd954856`, после S) | 4 028 010 | 12 374 664 | `docs/perf/_raw_ph6b_iai_cand_run1.log:579,586` |

НАБЛЮДЕНО (код/история): `git log -S drain_segment_sidecar -- src` даёт первый коммит `a4245965` (2026-09-30, «complete terminal sidecar reclamation and owner maintenance cutover»). В `a4245965^` discovery дренировал remote-ring с O(1) pre-drain empty-guard (PERF-PASS-4 G9/C2: сравнение кэшированного `ring_drain_head` с хвостом кольца, `git show a4245965^:src/alloc_core/small/alloc_core_small/find_segment.rs` около `:247–259`). Cutover заменил O(1)-проверку «есть ли что-то новое» на фиксированный скан O(high_water/1 KiB) атомарных swap на кандидата.

НАБЛЮДЕНО (R35, число слов): oracle-прогоны R35 на тех же геометриях (258 752 B × 34, 2 и 6 раундов — `benches/support/r18_sidecar_scan.rs:42–62` повторяет `benches/perf_gate_iai.rs:2869–2955`) дали visited = 319 063 / 1 004 503 слов при 99 / 275 routed-сканах, из них непустых — 0 (`R35_SIDE_CAR_SCAN_PREFILTER_GATE.md:28–29`). То есть 100% обмениваемых слов пустые; ≈3 223 / ≈3 653 слова на скан.

НАБЛЮДЕНО (цена слова): `docs/perf/PG3R_SCAN_PATCH_IAI.md:279–295` — одно лишнее слово = ровно 12 Ir (1024 слова = 12 288 Ir, совпадение 99.7–100%).

ВЫВЕДЕНО (сверка, оценка по логам): 319 063 × 12 = 3 828 756 Ir; + 30 394 (до cutover) = 3 859 150 против наблюдённых 4 028 010 → модель объясняет 95.8% всего Ir и 95.8% дельты; остаток 168 860 Ir ≈ 1 706 Ir/скан (построение scan/route lookup/прочие правки окна). seg_cycle: 1 004 503 × 12 + 72 256 = 12 126 292 против 12 374 664 → 98.0%, остаток ≈ 903 Ir/скан. Геометрия скана между `fd954856` и базой не менялась (`git diff fd954856 31db7f17` по `find_segment.rs`, `remote_bitmap/`, `alloc_core_small_magazine.rs` — только cfg-инструментовка R18, doc и фильтр `encoded()==0` в `publish`).

ВЫВОД: причина — не размер слова и не рост числа сегментов, а **замена O(1)-индикатора «новое есть?» на O(high-water) обход** в `a4245965`. Это причина, а не симптом: лечится восстановлением дешёвого индикатора непустоты (или его эквивалента), а не удешевлением одного слова.

### 2.2 Масштабирование (ВЫВЕДЕНО, оценка по коду)

- Число слов на один discovery = Σ по активным Small/Primordial кандидатам до первого хита `ceil(bump/1KiB) − payload_start_word`.
- Discovery вызывается раз на refill-промах (`alloc_core_small_magazine.rs:246`). При чистом росте один класс: каждый refill добавляет `refill_len × block_size` к bump текущего сегмента и сканирует все нарезанные слова всех сегментов → Σ ≈ H²/(2·refill_bytes·1 KiB): квадратично по нарезанному объёму H (совпадает с моделью R17 `docs/reviews/2026-10-08-src-review-oxx-round-17-performance.md:163–172`; она — экстраполяция).
- Пустые слова: в R35 все 1.3 M обменянных слов пустые (0 непустых в Growth/Cycles; 34 из 355 593 в Publication — `R35...md:28–30`).

### 2.3 Почему R35 не показал выигрыша prefilter

НАБЛЮДЕНО: candidate visited = baseline visited (319 063 / 1 004 503 / 355 593), exchanges 0/0/34; Ir +32 на сценарий; estimated cycles −4.0/−4.2/−4.2% (`R35...md:22–24,28–45`).

ВЫВЕДЕНО:
1. **Ir структурно слеп к этой правке.** Prefilter заменяет одну инструкцию (`lock xchg`) на одну загрузку + тот же тест/ветвление; число посещённых слов не меняется. Callgrind считает инструкции, а не латентность locked RMW и не RFO/запись линии. Поэтому «+32 Ir» — ожидаемое ≈0, а не отсутствие эффекта.
2. **Инструментовка доминирует в timing-плечах.** В timing-режиме под cfg каждое слово всё равно делает `BASELINE_SCAN.with` и `SCAN_COUNTS.with` (`bitmap_scan.rs:45–67`). Сверка: (6 262 375 − 4 028 010)/319 063 = 7.00 Ir/слово; (19 406 959 − 12 374 664)/1 004 503 = 7.00 Ir/слово (R35 timing base против Ph6b iai того же бенча; разные бинарники — оценка). То есть ≈37% Ir timing-плеча — харнесс.
3. **Режим не тот.** Однопоточно, линии bitmap принадлежат ядру владельца, продюсер не пишет во время окна → нет HITM/RFO-конфликта, а это и есть ось, где prefilter выигрывает. Callgrind-cycles (Ir + штрафы промахов модели) не моделирует ни `lock`, ни когерентность.
4. **Prefilter не трогает квадратичность** — он уменьшает константу на слово, а не число слов. Даже идеальный prefilter оставляет O(H²) загрузок.

Следствие для предрегистрации: prefilter-гейт на Ir невозможен по построению; нужен нативный wall-clock (и/или `perf stat` cycles + `mem_load_retired`/HITM), а структурные варианты (уменьшение visited) видны и в Ir — их и надо гейтить через Ir + oracle visited.

### 2.4 Протокол публикации и почему «dirty hint» удалён

НАБЛЮДЕНО (документы/код):
- Продюсер Small: `RoutePin::publish_small(self, offset)` (`src/registry/segment_route/pin.rs:45–50`) → `SmallSidecar::publish` (`small_sidecar.rs:149–151`) → `SidecarBitmap::publish` (`sidecar_bitmap.rs:110–119`): единственный `pending[w].fetch_or(bit, AcqRel)`. Pin поглощается вызовом и дропается ПОСЛЕ RMW; его `Drop` трогает только независимый дескриптор/sidecar (`pin.rs:37–38`; `docs/CROSS_THREAD_STATE_MACHINES.md:364–367,408–417`).
- `SmallSidecar` — отдельная `System.alloc_zeroed`-аллокация на регистрацию (`src/registry/segment_route/directory.rs:56–65`), освобождается на последнем pin (`:112–121`); не пулится и не переиспользуется между инкарнациями. Т.е. запись продюсера в sidecar в пределах pin не может попасть в чужую/освобождённую память.
- Почему dirty hint удалён: (1) в старой схеме подсказка жила в метаданных освобождаемого сегмента/heap — запись после терминальной публикации нарушает «last access» (`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:35,41`); бриф прямо описывает гонку «pending bit → пауза → dirty bit, owner видит старый dirty и отпускает сегмент до последней записи продюсера» (`docs/REMOTE_FREE_BOUNDED_RECLAMATION_DECISION_BRIEF_2026-09-29.md:40,44`); (2) strict trim и периодический maintenance не имеют права зависеть от подсказок (`CONTRACT.md:41,65`; `REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md:119–120`; `sidecar_drain.rs:186–190`); (3) негативные контроли контракта перечисляют «dirty-only discovery» как то, что oracle обязан ловить (`CONTRACT.md:112`). При этом контракт допускает «Dirty/summary … только как подсказки в стабильно живущей sidecar» (`CONTRACT.md:41`), а бриф оценивал двухуровневый summary в ≈512 Б/сегмент (`BRIEF:36`).
- Discovery не имеет обязательства включения конкурентных публикаций: «An in-flight producer after a word's cut belongs to a later scan» (`sidecar_bitmap.rs:124–125`), «concurrent terminal publications after a candidate's cut can wait for the next discovery pass» (`alloc_core_small_magazine.rs:199–203`). Обязательство включения «опубликовано до входа» есть только у strict trim (`CONTRACT.md:7–8`; `SIDECAR_REVISION.md:39–41`).
- Финализация сегмента зависит от ledger-кредита, а не от скана: опубликованный, но не потреблённый блок держит кредит (`CONTRACT.md:25`; `sidecar_drain.rs:233–236`). Значит, пропуск слова дискавери может только отложить reclaim, но не вызвать преждевременный release.
- Loom: `tests/loom_sidecar_bitmap.rs:29–35` моделирует `cut()` как load-before-swap (форма кандидата R18), а не production-`swap` — модель названа «exact atomic operations» (`:1`), но для production-пути не точна (известно: R18 follow-up в xxs P2 `:31` — «reduced Loom shadow»). Модели summary-протокола нет.
- Miri-пауза продюсера (`src/registry/heap_core_xthread/routing.rs:66–72`) стоит ПОСЛЕ `publish_small` целиком — окно «между двумя RMW» существующими тестами не воспроизводится.

ВЫВЕДЕНО: причина удаления dirty hint — не принципиальная невозможность индикатора непустоты, а (а) его размещение в освобождаемой памяти, (б) зависимость strict trim от него, (в) недоказанный порядок set/clear. Все три снимаются, если индикатор (а) лежит в `SmallSidecar` под pin продюсера, (б) читается только discovery (strict trim/maintenance — прежний полный скан), (в) ставится ПОСЛЕ терминального `pending.fetch_or`, а владелец обнуляет его `swap` ДО обмена слов. Текст `CONTRACT.md:112` («dirty-only discovery» как негативный контроль) при этом формально противоречит — нужна правка контракта владельцем (§6).

## §3 Варианты решения

Обозначения: W — слово pending (1 KiB payload), S — summary-слово (64 бита = 64 слова W = 64 KiB payload), «оценка» — статическая оценка по коду, не измерение.

| # | Вариант | Механизм | Аргумент корректности | Выигрыш / цена (оценка) | Риск | Что доказать / измерить |
|---|---|---|---|---|---|---|
| A | Ничего / только документация | Оставить фиксированный скан; задокументировать квадратичность; сделать `multiseg_cold_256k`/`seg_cycle_decommit_256k` гейтируемыми; вынуть `pool_cap_sweep` из `bench:table` | Протокол не меняется | 0 выигрыша; цена — O(H²) RMW на росте кучи остаётся (R35: 1.0 M пустых swap на 6 раундов, `R35...md:29`) | P2 структурная цена остаётся в production | — |
| B | OPT-01 prefilter (owner `load(Acquire)==0 → skip`, иначе `swap`) | Меняется только `bitmap_scan.rs:57` | Для публикации, которая happens-before скана, load читает её по write-read coherence → не пропускается (strict trim сохраняет включение). Конкурентная — может быть пропущена, что разрешено (`CONTRACT.md:7`). Пропуск не может вызвать финализацию — блок держит кредит. Остаток: «eventual consumption» опирается на «stores become visible in finite time» (в C++/Rust — рекомендация, не гарантия) — ровно то, что item 82 требует доказать | Число посещённых слов НЕ меняется; Ir ≈ 0 (R35: +32 Ir). Нативно — убирает locked RMW и запись/грязнение линии на каждом пустом слове (величина не измерена) | Низкий; продюсер не меняется | Нативная латентность owner в matched режиме; Loom уже моделирует эту форму (`loom_sidecar_bitmap.rs:29–35`), но не на actual type |
| C | OPT-02 summary в `SmallSidecar`, только для discovery | `SmallSidecar` + `summary: [AtomicU64; 64]` (512 Б). Продюсер: `pending[w].fetch_or(AcqRel)` (терминальная публикация, LP для strict trim не меняется), затем `summary[w/64].fetch_or(1<<(w%64), Release)`. Discovery: для каждого S в диапазоне `S.swap(0, AcqRel)` (опц. с load-prefilter), затем `pending[w].swap(0, AcqRel)` только для взведённых бит в `[start_word, end_word)`. Strict trim и bounded maintenance — прежний полный скан | (1) Нет потерь: если `pending.fetch_or` P идёт в mo(W) после `W.swap` владельца, то P читает из release-последовательности владельца (AcqRel) → `S.swap` владельца hb `S.fetch_or` P → бит S взведён после очистки → следующий discovery посетит W. Если до — бит потреблён сейчас; поздний `S.fetch_or` даёт лишь ложный бит (одно пустое слово). (2) Публикация, завершённая целиком (P вернулся) до discovery, включается: Release S / Acquire S → hb → `W.swap` видит бит. (3) Окно «между двумя RMW» — блок держит кредит, финализации нет; sidecar жив под pin (`pin.rs:45–50`, `directory.rs:112–121`). (4) Strict trim от summary не зависит → его контракт неизменен. (5) Биты вне `[start,end)` невалидны для valid use (issue только ниже bump, владелец не выдаёт во время discovery) — сброс равен текущей семантике скана | Discovery: вместо ≈3 223 W/скан (R35 growth) — ≤64 S на полностью нарезанный сегмент + только грязные W. Оценка: growth 99 сканов × ≤192 S × ~6 Ir ≈ 0.1 M Ir против ≈3.83 M Ir скана сейчас (12 Ir × 319 063). Квадратичность по H остаётся только с константой /64 (через число сегментов). Цена: +1 RMW на каждый удалённый free (ось item 70), +512 Б на sidecar (≈+1.2% к 41 984 Б, `SIDECAR_REVISION.md:51–52`) | Средний: меняется контракт «последнего доступа» продюсера и текст `CONTRACT.md:112`; контеншн summary-линии (8 S на 64-байтной линии = 512 KiB payload) | Loom summary-модель + негативные контроли; Miri на actual type; iai Ir multiseg/seg_cycle/growth; продюсерская латентность и HITM в отдельном режиме |
| C′ | C + условная установка summary продюсером | `if S.load(Relaxed) & b == 0 { S.fetch_or(b, Release) }` | Безопасно по той же цепочке: если `W.fetch_or` P после `W.swap` владельца, то `S.swap` владельца hb load P → P видит 0 (или новый бит другого продюсера, покрывающий то же W) → ставит. Требует отдельной Loom-модели | Продюсер платит RMW лишь при чистом бите (≈1 раз на W за эпоху discovery); при fan-in в одно W — только load | Средний+ (тоньше доказательство) | Отдельная Loom-модель; продюсерский A/B C vs C′ |
| D | Счётчик эпохи публикаций на sidecar (O(1)-guard как до `a4245965`) | Продюсер после `pending.fetch_or` делает `epoch.fetch_add(1, Release)`; owner: `e=epoch.load(Acquire)`; `e==last_seen` → пропустить сегмент; иначе полный скан и `last_seen=e` (значение, прочитанное ДО скана) | Тот же аргумент «запись после терминального RMW, чтение до скана»; переполнение u64 практически исключено | Идеален для роста без удалённых free (все сегменты O(1)); сегмент с одной публикацией — снова полный O(high-water) скан | Один горячий счётчик на сегмент — все продюсеры сегмента бьют одну линию (хуже C) | Как C; плюс fan-in HITM |
| E | OPT-03: BinTable-first / пропуск при `live_count==0` | Discovery сначала смотрит `bt.head(class)`; drain только если пусто; пропуск drain при нулевом кредите | Без изменения протокола; BinTable-first меняет порядок и сдерживание враждебных дубликатов (xxs P5 `:53`) | Не помогает росту (там все бины пусты и кредит > 0) — главный дорогой режим не лечится | Изменение reclaim/hardening-поведения | Discovery scaling + invalid-duplicate controls |
| F | Амортизация: ограничить бесплодные discovery по объёму нарезки | После discovery без находки следующий запуск — только когда нарезано ≥ k × (просканированных слов × 1 KiB); сброс при находке | Owner-only, протокол не меняется; discovery и так без обязательства включения; strict trim/maintenance — без изменений | Квадратичность → амортизированно линейна (оценка); ноль цены для продюсера | Задерживает переиспользование удалённо освобождённой памяти → рост RSS в producer→consumer; меняет документированный порядок «free-before-bump» (`alloc_core_small_magazine.rs:206–216`) | RSS/латентность в одном режиме (правило cost/benefit) |
| G | Ограничить скан «реальным high-water» | — | — | Уже сделано: high-water = `bump_of()` (`find_segment.rs:141`), патч S убрал метаданные. Дальнейшее сужение по классу невозможно (bitmap не делится по классам) | — | Отклонено |

Влияние на продюсера: A/B/E/F — нет; C/C′/D — +1 RMW (C′ — условный) в `SidecarBitmap::publish`, это ось perf item 70 (не измерена ни до, ни после cutover).

Моделирование в Loom: B — уже есть форма (`loom_sidecar_bitmap.rs:29–35`); C/C′/D — нужна новая reduced-geometry модель (2 W + 1 S, 2 продюсера, владелец с двумя discovery и strict-проходом). E/F — protocol-free, Loom не нужен (нужны native-тесты RSS/латентности).

## §4 Рекомендация

**Выбрано: C — summary-слово в `SmallSidecar`, читаемое только allocation-discovery (`drain_segment_sidecar`); продюсер ставит summary безусловным `fetch_or(Release)` ПОСЛЕ терминального `pending.fetch_or(AcqRel)`; владелец делает `summary.swap(0, AcqRel)` ДО обмена слов; strict trim и bounded maintenance остаются полным фиксированным сканом.**

Почему:
- Лечит причину (§2.1): восстанавливает дешёвый индикатор «есть ли новые публикации», утраченный в `a4245965`, не трогая strict-trim-контракт и не размещая ничего в освобождаемой памяти.
- Уменьшает число посещённых слов — эффект виден в детерминированном Ir и в oracle «visited», т.е. гейтируется тем же судьёй, что зафиксировал регрессию (`multiseg_cold_256k`, `seg_cycle_decommit_256k`), в отличие от B (§2.3).
- Корректность сводится к короткой цепочке happens-before (§3, строка C), моделируемой в Loom на reduced geometry с негативными контролями, которые реально различаются (summary-до-pending, clear-после-scan).

Почему не остальные:
- A — оставляет P2-цену в production; допустимо только как временное состояние.
- B — не уменьшает visited, в Ir не наблюдаем, лечит константу; полезен позже как дополнение для strict trim/maintenance (где summary запрещён контрактом), но не как решение PRF-01.
- C′ — тоньше доказательство; включать только если продюсерский замер C покажет недопустимую цену (предрегистрировать вторым плечом).
- D — один горячий счётчик на сегмент хуже C по контеншну и не помогает сегменту с единичными публикациями.
- E — не лечит основной режим (рост), меняет hardening-поведение.
- F — переносит цену в RSS/задержку переиспользования и нарушает документированный порядок free-before-bump; запасной owner-only вариант, если владелец отвергнет изменение продюсера.
- G — уже реализовано патчем S.

Что остаётся НЕдоказанным в C:
1. Корректность на actual type (Loom — reduced shadow; Miri-паузы МЕЖДУ двумя RMW в коде нет — нужна новая точка паузы).
2. Цена продюсера (+1 locked RMW на каждый удалённый Small free, контеншн summary-линии) — не измерена; ось perf item 70.
3. Фактическое снижение Ir/visited — только оценка (≤ ~0.1 M Ir скана вместо ≈3.83 M на growth); остаток 0.9–1.7 k Ir на скан (§2.1) не атрибутирован и может доминировать после правки.
4. Нативный wall-clock на установленном `#[global_allocator]` и RSS (+512 Б/sidecar логически) — не измерены.
5. Совместимость с текстом контракта (`CONTRACT.md:41,112`) — решение владельца.

## §5 План реализации

### 5.1 Файлы и эскиз

- `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs`
  - `const SUMMARY_WORDS: usize = WORDS / 64;` + `assert!(SUMMARY_WORDS == 64)` в const-блоке (`:24–30`).
  - `SidecarBitmap { pending, summary: &'a [AtomicU64], classes }`; `from_initialized`/`from_leaves` проверяют `summary.len() == SUMMARY_WORDS`.
  - `publish` (`:110–119`):
    ```text
    let w = granule / 64;
    self.pending[w].fetch_or(1 << (granule % 64), AcqRel); // terminal publication (strict-trim LP, unchanged)
    self.summary[w / 64].fetch_or(1 << (w % 64), Release);  // discovery hint; sidecar pinned by caller's RoutePin
    ```
  - новый `scan_summarized_from(high_water, start_word) -> Option<BitmapScan<'a>>` — тот же расчёт `end_word`, режим Summary.
- `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs` — режим обхода внутри `BitmapScan` (без нового pub-экспорта): поле `summary: Option<(&'a [AtomicU64], next_s, mask)>`; в Summary-режиме `next_cut`:
    ```text
    loop {
        if mask == 0 {
            if next_s > last_s { return None }
            mask = summary[next_s].swap(0, AcqRel) & range_mask(next_s, start_word, end_word); // clear BEFORE word cuts
            cur_s = next_s; next_s += 1; continue;
        }
        let w = cur_s * 64 + mask.trailing_zeros(); mask &= mask - 1;
        return Some(BitmapCut { classes, word: w, bits: pending[w].swap(0, AcqRel) });
    }
    ```
  Fixed-режим (`scan`/`scan_from`) не меняется → strict trim и bounded maintenance байт-в-байт прежние.
- `src/registry/segment_route/small_sidecar.rs` — поле `summary: [AtomicU64; SidecarBitmap::SUMMARY_WORDS]` в `#[repr(C)]` (`:13–17`), явная инициализация в `initialize_at` (`:25–37`) как у `pending`; `bitmap()` передаёт summary; `scan_summarized_from`.
- `src/alloc_core/segment/segment_table/route_slots.rs` + `segment_table_impl.rs:346–356` — `scan_small_summarized_from` / `scan_small_route_summarized_from` (те же проверки root/index, что `scan_small_from`).
- `src/alloc_core/small/alloc_core_small/find_segment.rs:147–150` — `drain_segment_sidecar` вызывает summarized-вариант; doc `:131–133` и `bitmap_scan.rs:11` обновить.
- `src/alloc_core/alloc_core/sidecar_drain.rs` — код без изменений; комментарий «strict/bounded passes intentionally ignore the discovery summary».
- Диагностика (только `bench-internals`, отдельная oracle-сборка — урок R32-11 и §2.3 п.2): счётчики `summary_words_visited`, `pending_words_visited`, `pending_words_exchanged`, `nonempty_cuts`, `discovery_records_reclaimed`; read-only observer → `PURE_OBSERVERS` в `scripts/verify-dbg-hook-safety.mjs`.
- Документы (после решения владельца): `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md` §3 (последний доступ продюсера = summary RMW в pinned sidecar; LP strict trim = pending RMW) и §12 (негативный контроль: «summary до pending / очистка после word cut / strict-maintenance через summary»); `docs/REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md`; `docs/CROSS_THREAD_STATE_MACHINES.md` §9.3.

### 5.2 Тесты (только `tests/`, без doctest)

Loom, `tests/loom_sidecar_summary.rs` (`#![cfg(loom)]`, 2 W + 1 S, orderings как в production):
- `loom_summary_two_producers_no_loss` — 2 продюсера в разные W одного S; владелец: discovery → join → discovery; объединение = обе публикации, пересечение пусто, класс виден.
- `loom_summary_publication_between_rmws_not_lost` — продюсер выполняет только pending (пауза), discovery пропускает, strict-проход (полный скан) находит.
- `loom_summary_spurious_bit_benign` — поздний summary после потребления → следующий discovery видит пустое W, без двойного reclaim.
- Негативные (`#[should_panic(expected = "lost publication")]`): `loom_summary_negative_summary_before_pending`, `loom_summary_negative_clear_after_scan` (владелец: load S → swap W → `S.store(0)`). Оба обязаны быть красными. Контроль «Relaxed summary» Loom может не поймать (RMW в Loom читает последнее по исполнению) — его зелень не считать доказательством.
- Для C′ (если пойдёт): `loom_summary_conditional_set_no_loss`.

Native, `tests/r19_sidecar_summary_discovery.rs` (features `production internals bench-internals`):
- `joined_foreign_free_reused_by_next_discovery` — 34 × 258 752 Б на 3 сегмента, присоединённый поток освобождает один блок, следующий alloc owner получает тот же адрес через discovery. Мутант «нет summary RMW» → красный.
- `growth_discovery_exchanges_no_pending_words` — чистый рост: `pending_words_visited == 0` у discovery. Мутант «fixed-скан в discovery» → красный.
- `strict_trim_includes_pending_without_summary` — новый `unsafe fn dbg_publish_pending_only_for_test` (`bench-internals`+`internals`, `# Safety`: уникально владеемый выданный блок — правило dbg-hook п.1) ставит только pending; `trim_current_thread` обязан потребить. Мутант «strict trim через summary» → красный.
- `summary_bits_outside_high_water_are_dropped` — бит S выше `end_word` не вызывает доступа/abort и ничего не потребляет.
- Регрессия: `tests/r6_terminal_owner_drain.rs`, `tests/r6_terminal_owner_drain_model.rs`, `tests/loom_terminal_owner_drain.rs`, `tests/loom_sidecar_bitmap.rs`, `tests/r6_sidecar_bitmap.rs`, `tests/r9_bounded_background_maintenance.rs`, `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`, `tests/r8_owner_sidecar_miss.rs`; Miri (SB и TB) на paused-producer тестах + новая пауза между RMW.

### 5.3 Эскиз предрегистрации гейта (до измерений)

- **Entry point:** `SeferAlloc` `GlobalAlloc::alloc/dealloc` прямыми вызовами → `HeapCore` (routed, как любой production heap; тот же слой, что multiseg/seg_cycle и R35). Wall-clock — дополнительно установленный `#[global_allocator]` (слой поставки).
- **Идентичность:** A и B — временные measurement-commit SHA (R29-6 п.1), один toolchain; Ir — `production bench-internals internals`; timing-сборка без счётчиков, oracle — отдельной сборкой.
- **Режим 1 (польза владельца, Ir):** `multiseg_cold_256k`, `seg_cycle_decommit_256k` + новые `growth16_{n,2n,4n}` (16 Б через `GlobalAlloc`, N = 32 768 объектов = 512 KiB; база по модели ≈ H²/2^19 слов: 0.5/2.1/8.4 M слов ≈ 6/25/100 M Ir — оценка по коду). Пороги (оценка, утверждает владелец): B/A ≤ 0.25 по Ir на multiseg и seg_cycle; наклон `(Ir4n−Ir2n)/(Ir2n−Irn)` ≤ 2.5 у B (у A по модели ≈4).
- **Oracle активации (R30-8):** `routed_scans > 0` на каждом плече; рост A: `pending_words_visited` ≈ 319 063 / 1 004 503 (как R35), B: `pending_words_visited == 0` и `summary_words_visited > 0`; publication-сценарий: `discovery_records_reclaimed` и остаток после strict trim одинаковы в A и B.
- **Контроли:** hot-churn (`small_churn_16b`, `churn_256b`, `churn_write_256b`, `aligned_churn_640b_a128`) |ΔIr| ≤ T, T = max(10, |A1−A2|, |C0−A1|) по схеме R16 perf83; mimalloc ΔIr = 0; strict-trim бенч ΔIr ≤ T.
- **Режим 2 (цена продюсера — отдельно, не сводить с режимом 1 в одну Pareto-фразу):** iai «присоединённый продюсер освобождает N блоков» (ожидание оценкой: +1 locked-инструкция и несколько Ir на free); нативный fan-in 1/2/4/8 продюсеров в один сегмент: p50/p99/p999 dealloc, `perf c2c` HITM. Порог — решение владельца ДО замера (предложение: p50 ≤ +5%, p99 ≤ +10%).
- **Отчётность:** per-sample JSON → один скрипт → таблицы + `_summary.csv` с assert'ами отношений; raw-логи по правилу хранения; манифест раунда.

### 5.4 Порядок коммитов (R30-12)

1. `test(loom): model summary-guided sidecar discovery with negative controls` — только модель.
2. `docs: amend terminal-publication contract for discovery-only summary hints` — после решения владельца.
3. `bench: preregister sidecar summary discovery gate; add growth16 and scan oracle counters`.
4. Замер на временном measurement-commit; при GO: `perf(runtime): guide allocation discovery by a sidecar summary word` (src + native тесты) и `bench: sidecar summary discovery gate report` (raw, CSV, манифест, `docs/perf/IAI_BASELINE.md`); при NO-GO — только `bench:` с вердиктом.
5. Индексы: PRF-01 / item 82 → «Recently resolved» (или обновлённая карточка); item 70 получает продюсерские числа.

## §6 Что нельзя решить без исполнения

Эксперименты (не запускались):
1. Loom: `RUSTFLAGS="--cfg loom" cargo test --release --test loom_sidecar_summary` — позитивы зелёные, оба негатива красные. Критерий: иначе модель не различает → вариант C не принимать.
2. Miri (SB/TB): новые native-тесты + пауза между RMW. Критерий: нет UB, публикация потреблена strict trim.
3. iai A/B по §5.3 режим 1 + oracle-сборка. Критерий: пороги B/A и наклон, контроли в T, oracle как указано.
4. Режим 2 (продюсер): iai Ir/free + нативный fan-in p50/p99/p999 + `perf c2c`. Критерий — порог владельца.
5. Нативный wall-clock роста 16 Б через установленный `#[global_allocator]` (1–64 MiB, N/2N/4N), отдельно от `pool_cap_sweep`.

Решения владельца:
- Разрешить ли продюсеру второй RMW после терминальной публикации (изменение «last access», `CONTRACT.md:35,41`) и переформулировать негативный контроль `CONTRACT.md:112`.
- Порог цены продюсера (item 70) до замера; C против C′.
- +512 Б логически на каждый `SmallSidecar`.
- Блокирует ли PRF-01 perf GO текущего `production` до правки; делать ли `multiseg_cold_256k`/`seg_cycle_decommit_256k` гейтирующими.
- Если изменение продюсера отвергнуто — принять ли F (амортизация discovery) с её RSS-риском, или остаться на A/B.

## §7 Проверено и НЕ дало вывода

- Остаток 168 860 Ir (multiseg) и 248 372 Ir (seg_cycle) сверх «слова × 12 + до-cutover» (§2.1) — ≈1 706 и ≈903 Ir на скан; не атрибутирован (в окне 129 src-коммитов — R17 performance `:158`; PRF-02 (+24 Ir/free, там же `:99`) даёт ≈1.6 k Ir на multiseg (68 free) и ≈4.9 k на seg_cycle (204 free) — оценка). Bisect не выполнялся.
- Механизм −4% Callgrind estimated cycles у кандидата R35 при равном Ir не установлен (модель Callgrind не учитывает `lock` и когерентность; вероятно, разница промахов модели при записи — гипотеза).
- Нативная цена `lock xchg` против `mov` на CPU замеров R35 (i7-11800H, WSL2) — в репозитории нет измерения.
- Частота discovery для класса 16 Б (размер refill magazine) через `HeapCore` — по коду раз на refill (`alloc_core_small_magazine.rs:246`), но закоммиченного счётчика для 16 Б нет; модель квадратичности для 16 Б остаётся экстраполяцией R17.
- `pool_cap_sweep` 1.2130 s → 206.05 s — разные харнессы (xxs P2 `:17`); как доказательство не используется.
- Сравнение R35 timing-Ir с Ph6b iai (7.00 Ir/слово харнесса) — разные бинарники/цели; совпадение до сотых в обоих сценариях — сильный, но не прямой довод.

Новые наблюдения (серьёзность):
- **P4 (методика измерения):** timing-плечи R35 содержат per-word TLS-инструментовку в обоих режимах (`bitmap_scan.rs:45–67`: `BASELINE_SCAN.with` + `SCAN_COUNTS.with` на каждое слово), ≈7 Ir/слово ≈ 36% Ir timing-плеча (оценка сверкой с Ph6b, §2.3 п.2). Same-binary сравнение это не ломает, но разбавляет отношения циклов; для следующего гейта выбор режима должен быть compile-time, а не TLS.
- **P4 (покрытие, уже известно):** `tests/loom_sidecar_bitmap.rs:1` заявляет «exact atomic operations», но `cut()` (`:29–35`) — форма кандидата (load-before-swap), а не production-`swap`; production-форма в Loom не моделируется. Признано в R18 follow-up как «reduced shadow»; doc-строку стоит уточнить.
