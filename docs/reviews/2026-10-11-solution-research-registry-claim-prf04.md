# Исследование решения: fresh-first claim реестра (R17-PRF-04, OPT-13)

Базовый SHA: `31db7f17698459e9ce26c3f8701c314304b9c00b` (проверено `git rev-parse HEAD`).
Код селектора в R18 не менялся: `git diff ff3d60bb HEAD -- src/registry/heap_registry/stack.rs src/registry/heap_registry/claim.rs` пуст.
Режим: только чтение; ничего не компилировалось и не запускалось. Все величины ниже —
либо цитата закоммиченного отчёта с файлом и строкой, либо явно «оценка по коду, не измерение».

## §0 Чек-лист хода

- [x] Шаг 0: базовый SHA подтверждён
- [x] Прочитаны карточки open-items и отчёты раунда 17 (§1)
- [x] Прочитан код claim/pick_slot/stack/bootstrap/tls_heap/maintenance (§1, §2)
- [x] Модель материализации heap: ресурсы, цифры из отчётов (§2.4)
- [x] Интерливинг волн K потоков (§2.2–§2.3)
- [x] Варианты решения (§3)
- [x] Рекомендация (§4) и план (§5)
- [x] Эксперимент (§6), §7

## §1 Постановка

### 1.1 Что не исправлено (перечитано на базовом SHA)

- `src/registry/heap_registry/stack.rs:108–140`, `pick_with_saturation`:
  - `:116` — `hint.swap(max)`: единственная подсказка поглощается одним claim;
  - `:118–119` — кандидат возвращается, если `< count`;
  - `:121–122` — ранний `None` только при `count == max` и насыщении;
  - `:124–125` — **`bump()` до любого скана**: свежий индекс, если `count < 4096`;
  - `:130–138` — `scan()` (`scan_claimable_slot`, `stack.rs:12–35`) выполняется только когда `bump` отказал, то есть у предела.
- `src/registry/heap_registry/claim.rs`:
  - `:435` — `MaintenanceLease::drop` пишет `reuse_hint = index`;
  - `:539` — `HeapLease::drop` (LIVE→FREE при выходе потока) пишет `reuse_hint = index`;
  - `:599` — `push_back_after_oom` тоже пишет hint;
  - `:214` — индекс, чей chunk не материализовался, тоже пишет hint;
  - все записи — `store(Relaxed)`, то есть **перезапись**: предыдущий FREE-индекс из подсказки пропадает до скана у предела.
- `src/registry/bootstrap/registry.rs:45` — `MAX_HEAPS = 4096`; `:79–81` — комментарий поля прямо признаёт: «Older FREE entries may wait until a capacity or cold-recovery scan».
- Тест, закрепляющий нынешнее поведение: `tests/r11_p3_registry_claim_selector.rs:71–99` (`overwritten_free_hint_is_deferred_until_cap_not_lost`) — при FREE-индексе 0 claim получает свежий 3 через `bump`. Любое изменение политики обязано осознанно переписать этот тест.
- Тест-ограничение, которое нельзя потерять: там же `:50–69` (`live_first_touch_is_linear_in_selector_probes`) — при N одновременно живых first-touch claim (N = 16/32/64) число проб `scan` до `bump` равно **0**.

### 1.2 Происхождение политики

- До `fe65746d` порядок был hint → scan → bump. Скан перед каждым fresh-claim давал треугольную стоимость N(N−1)/2 проб на волну одновременно живых first-touch потоков. Совет R11 P3-3 (`docs/reviews/2026-10-01-src-r11-production-cost-advice-xxs-sol.md:37–45`) рекомендовал hint → bump → скан у предела. Он прямо назвал это «deliberate policy trade» с отложенной (не потерянной) переиспользуемостью и потребовал: «On real runs measure claim latency *and* peak materialized chunks/commit under the same churn regime before accepting the fresh-first tradeoff» (`:45`).
- Там же (`:41`) перечислены допустимые следующие шаги, если отложенность окажется дорогой:
  - «exact reusable-slot set (fixed-capacity, nonallocating free-index queue/bitmap with duplicate suppression and CAS validation)»;
  - «bounded rotating probe before fresh bump».
  - И оговорка: «merely adding a summary bit is not a free improvement».
- Ограничение более раннего контракта: `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:59`. Прежний tagged-стек `free_slots` удалён из авторитетного протокола (`81fa61a5`). Bitmap или стек допустимы «только hint; full scan и CAS остаются источником истины». Для intrusive-стека требуется отдельное доказательство отсутствия дублей и циклов.

### 1.3 Что уже известно и что НЕ переоткрывать

- R17-PRF-04: `docs/reviews/2026-10-08-src-review-oxx-round-17-performance.md:280–320`. OPT-13: там же `:514–520`.
- Проверенная диспозиция: `docs/reviews/2026-10-08-src-review-xxs-round-17-P3.md:33–38`. **PARTIALLY VALID/NARROWED, P3**. Формула «K−1 за волну» — не универсальная, а условная: нет новых hint во время claim, нет maintenance.
- OPT-13 в синтезе: `docs/reviews/2026-10-08-src-review-xxs-round-17-synthesis.md:78` — «Отсутствует; не назначена».
- Correctness 78 подпункт 18 (`docs/correctness-open-items/TRACKED_process_record.md:333`). Это закрытая запись о **префиксе коммита** `fe65746` (`perf:` → должен был быть `perf(runtime):`), а не владелец замера. Переоткрывать её не нужно.
- Ни в `docs/perf/OPEN_ITEMS.md`, ни в `docs/CORRECTNESS_OPEN_ITEMS.md`/`docs/correctness-open-items/*` нет пункта о fresh-first. `grep fresh|reuse_hint|heaps_claimed` находит только perf 71 (`OPEN_ITEMS.md:3007–3010`, другое: `dealloc_batch` без bind). Пробел процесса подтверждён.
- Не переоткрываются:
  - корректность протокола lease (CAS-авторитет, `initialised`, Loom-модель `tests/loom_r11_registry_claim.rs`);
  - saturation-протокол (`src/registry/bootstrap/saturation.rs`);
  - cold OOM recovery (`scan_claim_recovery_free`, `stack.rs:62–85`).
- Решение ниже не меняет ни одного из этих механизмов.

## §2 Механизм

### 2.1 Модель (НАБЛЮДЕНО по коду)

Состояния слота (`src/registry/heap_slot.rs:88–99`): `EMPTY(0) → INITIALIZING(2) → LIVE(1) → FREE(3)`, `FREE → MAINTENANCE(4) → FREE`, `FREE → LIVE`. `initialised` монотонен (`heap_slot.rs:205–240`).

- **Bind потока.** Первый alloc потока: `current_for_alloc[_with_config]` (`src/global/tls_heap.rs:254–278,350`) при `LOCAL == null` → `bind_slow_tagged[_with_config]` (`:411–423`) → `HeapRegistry::claim_lease[_with_config]` (`claim.rs:113–157`) → `claim_impl` (`:198–346`).
  - `pick_slot` (`:350–360`) = `pick_with_saturation(hint, count, saturation, 4096, scan_claimable_slot, bump_count)`.
  - EMPTY → CAS EMPTY→INITIALIZING → `materialise` = `HeapCore::new_with_config` (`src/registry/heap_core/core.rs:333`) → `AllocCore::new_with_config_for_owner`: резервирование primordial-сегмента и регистрация маршрута → `bind_slot_counters` → `initialised = true` → INITIALIZING→LIVE.
  - FREE → CAS FREE→LIVE → `trim_for_recycle()` под новым владельцем (`claim.rs:317`) и N2-проверка конфликта конфигурации (`:318`).
  - Любой проигранный CAS или чужое состояние → `continue` → **новый `pick_slot`**.
- **Выход потока.** `AbandonGuard::drop` (`tls_heap.rs:195–223`): `mark_local_torn` → `trim_for_recycle` → `drop(lease)` → `HeapLease::drop` (`claim.rs:521–541`): CAS LIVE→FREE (Release), `reuse_hint.store(idx)`, `saturation.publish_claimable()`.
- **Что делает trim при выходе** (`src/registry/heap_core/state/ownership.rs:106–143`):
  - drain sidecar ingress;
  - flush всех tcache-классов;
  - `drain_small_pool`;
  - `release_empty_current_small_for_trim`;
  - `evict_all` Large-кэша;
  - `release_directory_for_cold_trim`.
  - Primordial-сегмент **не** decommit'ится: `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:70–72,110`.
  - Итого FREE heap удерживает: primordial (тронутые страницы), его маршрут/sidecar, слот реестра и сегменты с ещё живыми (чужими) блоками.
- **Maintenance.** `maintenance_pass` (`src/registry/heap_registry/maintenance.rs:17–40`) посещает ≤ `SLOT_BUDGET = 32` индексов за проход с периодом 10 ms (`src/global/maintenance_service.rs:29–30`). Сервис запускается только явно (модульный doc `maintenance_service.rs:1–13`). Каждый успешный визит FREE-слота при `drop` перезаписывает hint (`claim.rs:435`).
- **Ниже предела** FREE-слот достижим **только** через hint: скан вызывается лишь после отказа `bump`. Единственное другое место, где ищут FREE — cold recovery после OOM chunk (`claim.rs:206–217`).

### 2.2 Интерливинг: барьерные волны (ВЫВЕДЕНО, детерминированно по коду)

Шаблон: `for _ in 0..W { std::thread::scope(|s| for _ in 0..K { s.spawn(work) }) }`. Все K потоков волны завершаются (join) до старта следующей; maintenance выключен (как в `production` по умолчанию).

| Шаг | Событие | `reuse_hint` | `count` | FREE |
|---|---|---|---|---|
| 0 | старт процесса | MAX | 0 | ∅ |
| волна 1, claim 1..K | hint=MAX → `bump` | MAX | K | ∅ |
| выход волны 1 | K × (LIVE→FREE, `store(idx)`) | `h1` (последний вышедший) | K | {0..K−1} |
| волна 2, claim 1 | `swap` → `h1` < count → FREE→LIVE, trim | MAX | K | K−1 слотов |
| волна 2, claim 2..K | hint=MAX → `bump` ×(K−1) → materialise | MAX | 2K−1 | K−1 (невидимы) |
| выход волны 2 | K × store | `h2` | 2K−1 | 2K−1 |
| волна w | 1 reuse + (K−1) fresh | … | 1 + w(K−1) | … |

- Итог после W волн: `count = 1 + W·(K−1)` при идеале K. Это совпадает с оракулом R17 (`…-performance.md:319`).
- Предел 4096 достигается при `W ≥ 4095/(K−1)` (арифметика): K=2 → 4095 волн; K=4 → 1365; K=8 → 585; K=32 → 133.
- После предела `bump` отказывает, и `scan_claimable_slot` с вращающимся курсором находит FREE за единицы проб, потому что почти все слоты FREE. Устойчивое состояние — **4096 материализованных heap**, из которых живы K.
- То же самое без `scope`: любой пул потоков, который пересоздаётся на каждую задачу или фазу (join-all → spawn-all).

### 2.3 Интерливинги, где потери нет или она меньше (ВЫВЕДЕНО)

1. **Чередующийся churn** (thread-per-request при постоянной конкуренции): exit(a) → store a → spawn → swap получает a. Если между двумя публикациями не больше одного claim, свежих индексов нет. Потеря возникает ровно тогда, когда ≥ 2 claim приходятся на один интервал между публикациями hint, или когда ≥ 2 публикации идут подряд без claim (перезапись).
2. **Maintenance включён**: каждый проход публикует hint на последний посещённый FREE (≤ 32 визита / 10 ms). Это частичное смягчение для волн длиннее 10 ms; барьерную волну из K одновременных claim оно не спасает.
3. **Обратный эффект maintenance (ВЫВЕДЕНО, мелкий).** Claim взял hint на слот, который в этот момент в MAINTENANCE → `cas FREE→LIVE` проигран (`claim.rs:239–243`) → `continue` → hint уже MAX → `bump`. Получается лишний свежий heap, хотя FREE был. Maintenance потом опубликует hint заново, но уже поздно для этого claim.
4. **Устаревший hint** (запись hint на слот, который тем временем снова LIVE) — то же: проигранный CAS → `bump`. Безопасно, но тоже мимо переиспользования.

### 2.4 Цена лишнего heap (цитаты и оценки)

| Ресурс | Величина | Источник/статус |
|---|---|---|
| VA primordial | 4 MiB на heap | `heap_slot.rs:30–34` (doc), SEGMENT — по коду |
| Commit первого heap (исторически) | 904 KiB в среднем (5 проб: 904/908/904/900/904) при `production,primordial-lazy-commit` | `docs/perf/R12_9_PRIMORDIAL_LAZY_COMMIT.md:114–117`. **Раунд 12, другое дерево; включает bootstrap реестра — не цена N-го heap на базовом SHA** |
| System-память маршрута | 41 KiB `SmallSidecar` + mixed-листы | R17-PRF-04 (`…-performance.md:292,305`), мной не перепроверялось |
| CPU материализации | `large_alloc_free_cycle` 49 922 / 49 951 Ir (материализация + первый Large-цикл вместе) | `…-performance.md:298–300`; доля материализации не изолирована (P3-диспозиция `…-P3.md:37`) |
| Слот реестра | `size_of::<HeapCore>() ≤ 9216` B (`core.rs:283`) + заголовок; новый chunk на каждые 64 индекса (`bootstrap/chunk.rs:65`) | оценка по коду; страницы chunk коммитятся лениво ОС |
| Удержание после выхода | primordial-страницы, тронутые прежним владельцем, + sidecar + слот | **не измерено нигде** (grep по `docs/perf`) |

- Оценка по коду, не измерение: при K=8 и 585+ волнах процесс приходит к 4096 heap. VA = 16 GiB. Commit ≥ 4096 × (метаданные primordial + sidecar). Даже при нижней границе ~50 KiB на heap (41 KiB sidecar по R17 + ≤ 9 KiB слот, без страниц primordial) это порядка 200 MiB против K × то же при идеале. Порядок величины не проверен — потому §6.
- **Выгода fresh-first, которую нельзя потерять (НАБЛЮДЕНО):**
  1. 0 проб до `bump` в all-live first-touch волне (тест `:50–69`).
  2. Свежий heap не платит `trim_for_recycle` под новым владельцем (`claim.rs:317`).
  3. При `numa-aware` (не в `production`) свежий primordial первым трогает новый поток на своём узле. Переиспользованный heap несёт страницы чужого узла; `invalidate_numa_node_cache` (`claim.rs:332–335`) сбрасывает только кэш номера узла, а не размещение страниц.

## §3 Варианты решения

Общий инвариант для всех вариантов (не меняется):
- единственный источник права — CAS по `HeapSlot::state`;
- `initialised` (Release/Acquire) охраняет разыменование;
- насыщение — только по скану, начатому на пределе (`stack.rs:127–138`);
- cold recovery и скан у предела остаются страховочной сетью для EMPTY-индексов после OOM chunk.

Все оценки цены ниже — **оценка по коду, не измерение**.

| # | Вариант | Механизм | Корректность | Выигрыш / цена | Риск | Что доказать/измерить |
|---|---|---|---|---|---|---|
| V0 | Ничего не менять: документация + perf-карточка | Политика как есть. В `docs/perf/OPEN_ITEMS.md` — карточка «retention при fresh-first не измерена». В README/ARCHITECTURE — оговорка про барьерные волны | Без изменений | 0 / 0. Барьерные волны по-прежнему растят `count` до 4096 (§2.2: следует из кода неизбежно) | В этом режиме удержание остаётся в 4096/K раз выше идеала | Только замер §6 — чтобы решить, нужен ли V4 |
| V1 | Счётчик FREE + ограниченный вращающийся probe перед `bump` | `free_count`: +1 до CAS →FREE, −1 после выигранного CAS FREE→LIVE. Если `free_count > 0` — пробовать ≤ P материализованных слотов от `scan_cursor` (только `FREE && initialised`), затем `bump` | +1 предшествует Release-CAS, −1 следует за Acquire-CAS. По когерентности RMW счётчик не уходит в минус. Счётчик и probe — только подсказки: ошибка = лишний probe или лишний `bump` | При `free_count == 0` — 0 проб (тест R11 проходит). Иначе ≤ P загрузок `state` чужих слотов. **Неточно:** последний claimer волны (1 FREE на весь `count`) промахивается окном P и делает `bump`. `count` растёт медленнее, но растёт | Probe читает линию `state` LIVE-слота. Если с ней делят кэш-линию поля `HeapCore` (тип не `repr(C)`, раскладка не проверена — ГИПОТЕЗА), владелец получает coherence-промах | Распределение промахов окна, сходимость `count`, false sharing |
| V2 | Ограниченный probe без счётчика | То же, но всегда ≤ P проб перед `bump` | Как V1 | В all-live first-touch волне — N·P проб вместо 0. **Нарушает приёмку R11** (`tests/r11_p3_registry_claim_selector.rs:65`, `probes == 0`) | Как V1, плюс регресс холодного bind | — (отвергается приёмкой R11) |
| V3 | Массив из H подсказок | Публикация: CAS в пустую ячейку, иначе перезапись по курсору. Claim: `swap` по H ячейкам | Как сейчас: ячейка — только подсказка | Точно при K ≤ H; при K > H — та же отложенность. H RMW на каждый промах | Тот же класс потерь при больших K | Выбор H без данных |
| V4 | **Точное неинтрузивное множество FREE: двухуровневый битмап (64 слова × 64 бита + summary-слово), второй уровень после hint** | Порядок: hint → `free_set.take()` → проверка насыщения → `bump` → скан у предела. **Публикация** — во всех местах, где сейчас `reuse_hint.store` для FREE-слота: `words[w].fetch_or(bit)`, затем `summary.fetch_or(1<<w)`. **`take`:** загрузить summary → младший бит → загрузить слово → `fetch_and(!bit)`; бит «наш», только если в старом значении он был. Опустевшее слово → clear-then-recheck для summary. **`forget(idx)`** (`fetch_and`) — после выигранного FREE→LIVE из любого источника | Битмап — только подсказка, владение даёт только CAS. **Нет ABA и циклов:** нет ссылок и связей, бит идемпотентен, дублей нет. **Нет переполнения:** 4096 бит фиксированы. **Проигранный CAS на MAINTENANCE:** бит потреблён, но `MaintenanceLease::drop` после MAINT→FREE публикует его заново (`claim.rs:422–436`). **Запоздалая публикация** прежнего владельца после нашего `forget` оставляет устаревший бит на LIVE-слоте — цена: один проигранный CAS. **Summary:** сеттер — `fetch_or(word)`, затем `fetch_or(summary)` (Release); очиститель — `fetch_and(summary)` (AcqRel), `load(word)` (Acquire), при ≠ 0 — `fetch_or(summary)`. Если `fetch_or` сеттера раньше в порядке модификации summary — RMW очистителя синхронизируется с ним и видит бит слова. Если позже — бит summary выставлен уже после очистки. Навсегда бит не теряется | Барьерные волны: `count` ≈ пику живых потоков (цель OPT-13). Цена: +2 RMW на выход потока. На bind при непустом множестве — 1–2 загрузки, 1–3 RMW и `forget`; при пустом — 1 загрузка summary и **0 проб слотов** (приёмка R11 сохраняется). Память: 65 × 8 = 520 B в `static REGISTRY` | Контеншн на линии summary в барьерной волне. Выбор смещается с «самый свежий» на «младший индекс» (hint сохраняет LIFO для одиночного churn). Shadow-модель Loom может разойтись с реальным типом. При `numa-aware` реюз отдаёт страницы чужого узла | Loom: в покое нет «невидимого» FREE; повторный реюз после MAINTENANCE; запоздалая публикация; гонка summary. Замер §6: удержание, латентность all-live first-touch, steady churn |
| V5 | Вернуть scan-first (до `fe65746d`) | hint → полный скан → `bump` | Протокол был проверен | Реюз точный, но N(N−1)/2 проб на all-live волну | Возвращает регресс, ради устранения которого делали R11 P3-3 | — (отвергается приёмкой R11) |
| V6 | Оставить fresh-first, но снизить удержание FREE heap (decommit payload primordial при выходе) | Снижает удержание на один heap, но не число heap | Primordial целиком decommit'ить нельзя: его метаданные хостят реестр (`alloc_core_small_pool_impl.rs:70–72`). Нужна частичная политика только для payload | Не затрагивает VA, sidecar, слоты и рост `count`. Цена — цикл decommit/recommit на каждом реюзе primordial (класс регрессии R8-10, `docs/perf/R12_9_PRIMORDIAL_LAZY_COMMIT.md:95–101`) | Высокий; ортогонален OPT-13 | Отдельный гейт; PRF-04 не решает |

## §4 Рекомендация

**Выбрано: сначала замер (§6) на базовой политике и кандидате, затем V4 — hint → точный неинтрузивный битмап FREE → `bump` → скан у предела. Продвижение в `production` — только по предрегистрированному гейту.** Если гейт покажет несущественное удержание (порог ниже), политика остаётся fresh-first, а пункт закрывается как V0 с цифрами.

Почему так:
- **Сам механизм дефекта не требует замера.** В барьерных волнах `count = 1 + W(K−1)` до 4096 — это логика кода (§2.2), а не гипотеза. Неизвестна только цена одного FREE heap в байтах: ни один закоммиченный отчёт её не измерял (§2.4). Без этой цифры нельзя честно утверждать, что усложнение стоит выгоды. Требование R11 (`…-advice-xxs-sol.md:45`) и правила CLAUDE.md о path-activation и cost/benefit в одном режиме тоже требуют сначала числа.
- **V4 — единственный вариант, который одновременно:**
  1. точен (не теряет FREE при любом K);
  2. сохраняет приёмку R11 — 0 проб слотов в all-live first-touch;
  3. не трогает строки `state` чужих LIVE-слотов;
  4. соответствует контракту `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:59` («bitmap — только hint; full scan и CAS — источник истины»). Обязательство доказать отсутствие дублей и циклов у битмапа выполняется по построению: связей нет, бит идемпотентен.
- **Отвергнуты:**
  - V1 — неточен (последний claimer волны промахивается) и читает линии чужих живых слотов;
  - V2 и V5 — нарушают приёмку R11;
  - V3 — та же потеря при K > H;
  - V6 — ортогонален и рискованнее (класс R8-10), рост `count`/VA/sidecar не трогает;
  - V0 — допустимый исход, но только как результат замера, а не по умолчанию: в режиме барьерных волн дефект детерминирован.

**Остаётся недоказанным:**
1. Величина удержания на один FREE heap и, значит, существенность выигрыша.
2. Что контеншн на summary-слове в барьерной волне K = 32…64 не ухудшает first-alloc latency сверх шума.
3. Что предпочтение младшего индекса не хуже LIFO по прогреву. Hint сохраняет LIFO для одиночного churn, но волна получит «холодные» heap — их trim уже выполнен при выходе, поэтому ожидается слабое влияние: ГИПОТЕЗА.
4. Что shadow-модель Loom соответствует реальному типу (смягчается unit-тестами реального `FreeSlotHint`).
5. Поведение при `numa-aware` — решение владельца (§6.3).

## §5 План реализации

### 5.1 Файлы и функции (эскиз; base SHA `31db7f17`)

1. **Новый файл `src/registry/bootstrap/free_slot_hint.rs`** — одна публичная сущность `#[doc(hidden)] pub struct FreeSlotHint` (рядом с `saturation.rs`, по тому же образцу). Без `unsafe`; atomics из `core` (как и остальные поля `Registry` под loom — `registry.rs:8,18–25`).

   ```text
   const WORDS: usize = MAX_HEAPS / 64;          // 64
   const TAKE_ATTEMPTS: usize = 2 * WORDS;       // граница латентности take()
   pub struct FreeSlotHint { summary: AtomicU64, words: [AtomicU64; WORDS] }
   impl FreeSlotHint {
     pub const fn new() -> Self                  // [const { AtomicU64::new(0) }; WORDS]
     pub fn publish(&self, idx: usize) {         // ТОЛЬКО после CAS → FREE
       let (w, b) = (idx / 64, idx % 64);
       self.words[w].fetch_or(1 << b, Release);
       self.summary.fetch_or(1 << w, Release);
     }
     pub fn take(&self) -> Option<usize> {       // кандидат, НЕ владение
       for _ in 0..TAKE_ATTEMPTS {
         let s = self.summary.load(Acquire);
         if s == 0 { return None; }               // all-live first-touch: 1 load
         let w = s.trailing_zeros() as usize;
         let word = self.words[w].load(Acquire);
         if word == 0 { self.clear_summary_recheck(w); continue; }
         let bit = 1u64 << word.trailing_zeros();
         let old = self.words[w].fetch_and(!bit, AcqRel);
         if old & bit == 0 { continue; }          // бит взял другой claimer
         if old == bit { self.clear_summary_recheck(w); }
         return Some(w * 64 + bit.trailing_zeros() as usize);
       }
       None                                       // → bump; скан у предела страхует
     }
     pub fn forget(&self, idx: usize)             // после выигранного FREE→LIVE
     fn clear_summary_recheck(&self, w: usize) {
       self.summary.fetch_and(!(1 << w), AcqRel);
       if self.words[w].load(Acquire) != 0 { self.summary.fetch_or(1 << w, Release); }
     }
   }
   ```

   `forget` не трогает summary: лишний бит summary самоочищается в следующем `take`.

2. **`src/registry/bootstrap/mod.rs`** — только `pub(crate) mod free_slot_hint;` и re-export (правило «mod.rs — только реэкспорт»).

3. **`src/registry/bootstrap/registry.rs`**:
   - поле `pub(crate) free_set: FreeSlotHint`, инициализация в `const fn new()` (`:100–108`);
   - обновить doc полей `count` (`:73–75`) и `reuse_hint` (`:79–81`): «older FREE entries are found by the exact free-set».

4. **`src/registry/heap_registry/stack.rs:108–140`** — у `pick_with_saturation` появляется параметр `reuse: impl FnOnce() -> Option<usize>`. Он вызывается после hint, **до** проверки насыщения и `bump`:

   ```text
   if candidate < before { return Some(candidate); }
   + if let Some(i) = reuse() { if i < before { return Some(i); } }
   if before == max && saturation.is_saturated() { return None; }
   ```

   Проверка `i < before` стоит там же, где для hint. Бит выставляется только для индекса, уже прошедшего `bump`, так что проверка — защита в глубину.

5. **`src/registry/heap_registry/claim.rs`**:
   - `pick_slot` (`:350–360`): передать `|| reg.free_set.take()`;
   - ветка `STATE_FREE` после успешного CAS (`:239–244`): `reg.free_set.forget(idx)`;
   - `MaintenanceLease::drop` (`:434–436`), `HeapLease::drop` (`:538–540`), `push_back_after_oom` (`:599–600`): `reg.free_set.publish(idx)` **после** state-CAS и **до** `saturation.publish_claimable()`;
   - путь chunk-OOM (`:213–217`) **не** публикует бит: слот не материализован, его по-прежнему подбирают cold-скан и скан у предела;
   - doc `claim_impl` (`:192–197`) и `pick_slot` (`:348–349`): новый порядок.

6. **Диагностика источника claim (только `bench-internals`, правило 2 о `dbg_*`-хуках).**
   - В `pick_slot` локальный `Cell<Source>` выставляют замыкания reuse/bump/scan; по умолчанию — Hint.
   - `claim_impl` по выигранному CAS делает `Relaxed fetch_add` в один из статических счётчиков `{hint, free_set, bump_fresh, cap_scan, recovery}` в `src/registry/heap_registry/counters.rs`.
   - Читатель `dbg_registry_claim_sources()` — pure observer, его нужно занести в таблицу `PURE_OBSERVERS` в `scripts/verify-dbg-hook-safety.mjs`.
   - Без `bench-internals` код не компилируется: нулевая цена для `production`.

7. **Документация в коммите изменения:**
   - `CHANGELOG.md` (новая запись; историческая строка `:367` не трогается);
   - `docs/ARCHITECTURE.md` — порядок выбора слота;
   - карточка в `docs/perf/OPEN_ITEMS.md` (заводится в коммите C1, закрывается в C3);
   - манифест раунда `docs/perf/round-manifests/…`.

### 5.2 Тесты (только `tests/`, без doctest)

| Файл | Features | Что проверяет | Красный до / зелёный после | Мутанты, которые должны покраснеть |
|---|---|---|---|---|
| `tests/r19_prf04_free_slot_hint_unit.rs` (новый) | `alloc-global internals` | Реальный `FreeSlotHint`: `publish`→`take` выдаёт младший индекс; повторный `publish` идемпотентен — `take` выдаёт индекс ровно один раз, затем `None`; `forget` снимает только свой бит; публикация всех 4096 → ровно 4096 разных `take`, затем `None`; `take` на пустом → `None` | До: файла и типа нет (новый API). Поведенческий «красный до» — в тесте ниже | `take` игнорирует старое значение `fetch_and` → дубль; `forget` очищает слово целиком; summary не выставляется в `publish` |
| `tests/r11_p3_registry_claim_selector.rs` (правка) | как сейчас | `select` получает `reuse`. Старые сценарии идут с `\|\| None` и фиксируют прежнюю семантику селектора без множества. Новый `free_set_reuses_overwritten_free_before_bump`: FREE 0 в множестве, hint на 1 → два claim дают 1 и 0, `count` не растёт. `live_first_touch_is_linear_in_selector_probes` — с реальным пустым `FreeSlotHint`: `probes == 0`, `count == n` | Новый сценарий на базовом коде: второй claim → `bump` (3), а не 0 | reuse после `bump`; reuse без проверки `< before` |
| `tests/r7_p3_claim_scan.rs` (правка) | как сейчас | 5 вызовов `pick_with_saturation` (`:21,43,103,133,218`) получают `\|\| None`: поведение у предела не меняется | — | — |
| `tests/r19_prf04_thread_wave_reuse.rs` (новый, отдельный бинарник, один `#[test]`) | `production internals` | Реальный `#[global_allocator]`-путь (`SeferAlloc`) — по образцу `examples/r31_10_trim_rss_gate.rs:39–44`. Разогревочная волна, затем W=6 волн по K=4 потока через `std::thread::spawn` + `join` (не `scope`, см. §7). Каждый поток делает alloc/free. Assert: Δ`stats().heaps_claimed_high_water` за 6 волн ≤ K; Δ`config_conflicts` == 0 | До: Δ = 6·(4−1) = 18 (по §2.2). После: ≤ 4 | Убрать `publish` из `HeapLease::drop`; вызывать reuse после `bump`; не публиковать после MAINTENANCE (покрывает Loom) |
| `tests/loom_r19_registry_free_set.rs` (новый shadow-модуль Loom, по образцу `tests/loom_r11_registry_claim.rs`) | `--cfg loom`, `alloc-global internals` | (a) 2 слота × {выход владельца: LIVE→FREE + publish; claimer: hint → take → CAS; maintenance: FREE→MAINT→FREE + publish}. В покое: каждый FREE-слот либо имеет бит, либо стоит в hint; двух владельцев нет. (b) Гонка `publish` и `clear_summary_recheck` на одном слове: в покое `word != 0 ⇒ summary bit` | — (модель новая) | Без recheck в `clear_summary_recheck`; `publish` до state-CAS; `take`, считающий бит своим без проверки `old & bit` |
| Существующие, перезапустить | как в их заголовках | `tests/r11_p3_registry_claim_{chunk_oom,first_bind,uninit_oom}.rs`, `tests/registry_basic.rs`, `tests/regression_claim_oom_initialised_gate.rs`, `tests/loom_r11_registry_claim.rs`, `tests/loom_r8_maintenance_lease.rs`, `tests/dealloc_only_no_bind.rs` | — | — |

Новый Loom-файл нужно внести в `scripts/loom.mjs` и в loom-job `ci.yml` (сентинели `scripts/verify-ci-sentinels.mjs`, `scripts/verify-loom-target-wiring.mjs`) — так же, как это сделал `fe65746d`. Счётчики тестов в README/ARCHITECTURE не хардкодить заново: правило no-hardcoded-counts.

### 5.3 Эскиз предрегистрации гейта (по CLAUDE.md)

- **Entry point.** Настоящий `#[global_allocator] static GLOBAL: SeferAlloc` в `examples/r19_prf04_thread_wave_gate.rs`. Политика живёт в `HeapRegistry::claim_lease_with_config`, который вызывается только из TLS-bind (`tls_heap.rs:350,422`). Поэтому слой `AllocCore` здесь неприменим, а `HeapCore` не включает выбор слота.
- **Арки и идентичность.**
  - A = SHA коммита C1 (счётчики + харнесс, политика fresh-first).
  - B = SHA временного измерительного коммита C2 (политика V4).
  - Оба собираются в отдельных `git worktree` (протокол pinned-commit). Неизменяемая идентичность — эти два SHA, зафиксированные **до** замера (правило R29-6 п.7).
- **Сборки.**
  - Ретенция и латентность — чистый `--features production`: `bench-internals` добавляет `lock xadd` на own free (`…-oxx-round-17-performance.md:550`) и исказил бы латентность. Оракул здесь — `stats().heaps_claimed_high_water`, он доступен без `bench-internals` (`src/global/sefer_alloc/diag.rs:180`).
  - Разбивка по источникам claim — отдельный прогон `production bench-internals`, только для oracle активации; его временные числа в вердикт не входят.
- **Режимы** — cost и benefit в одном режиме, без смешивания (правило regime):
  1. **R-wave** (выгода и цена): барьерные волны K ∈ {2, 8, 32} × W ∈ {16, 128}; при K=32, W=128 у A ожидается `count = 1 + 128·31 = 3969` < 4096. Поток: N малых блоков 64 B…4 KiB суммарно ~1 MiB + один Large 2 MiB, запись по странице, free всего, выход. Волна — `spawn` + `join` всех K.
  2. **R-live** (kill-gate R11): K=64 потоков стартуют за `Barrier`, первый alloc после барьера, FREE-heap до этого нет.
  3. **R-churn** (kill-gate hint-пути): 2000 последовательных пар `spawn` → alloc/free → `join` по одному потоку.
- **Доказательства по R26-4 на каждую ячейку.**
  - REQUESTED: `LargeCacheConfig::new()` по умолчанию.
  - RESOLVED: Δ`config_conflicts` == 0 (hard-assert).
  - Изоляция: отдельный подпроцесс на каждую тройку (арк, режим, K, W, повтор) — межарочная утечка невозможна по построению.
  - Граница: maintenance-сервис **не** запускается; харнесс это проверяет.
- **Oracle активации (hard-assert до доверия к цифрам ячейки).**
  - R-wave, A: Δcount == 1 + W(K−1). Отклонение означает, что режим не барьерный; ячейка невалидна.
  - R-wave, B: Δcount ≤ K + 1; в `bench-internals`-прогоне `free_set + hint ≥ (W−1)·K − 1`, `bump_fresh ≤ K + 1`.
  - R-live, оба арка: Δcount == 64, `free_set == 0`.
  - R-churn, оба: Δcount ≤ 2.
- **Метрики.**
  - `commit` и RSS (`proc_probe::snapshot()`, как в `examples/r31_10_trim_rss_gate.rs:104`) в покое после последней волны и пик по волнам;
  - Δ`segments_reserved_total`;
  - латентность первого alloc в потоке (ns от старта потока до возврата первого alloc), p50/p99 по ячейке;
  - wall-clock прогона.
  - Первичные данные — по-сэмплово в JSON. CSV и таблицы отчёта выводятся одним проверяемым скриптом, отношения в нём проверяются `assert` (правило derived-tables).
- **Пороги** (предложение; утверждает владелец до запуска):
  - **BENEFIT** (R-wave, K=8, W=128, медиана из 5): `commit_idle(B) ≤ commit_idle(A) − max(64 MiB, 25 % · commit_idle(A))`. Если ни в одной ячейке R-wave порог не достигнут — **NULL**: политика остаётся fresh-first, пункт закрывается как V0 с цифрами.
  - **KILL-1** (R-live): A/B/B/A через `scripts/paired-ab-runner.mjs` (конфиг по образцу `docs/perf/r26_3_run.json`), 20 пар. B не медленнее A статистически значимо (paired t, p<0.05) и |Δp50| ≤ 5 %. Плюс same-vs-same контроль A/A.
  - **KILL-2** (R-churn): Δwall ≤ max(2 %, шум A/A).
  - **R-wave p99 первого alloc**: репортится обоими арками. Ухудшение B > 10 % — вопрос владельцу, а не автоматический NO-GO: реюз платит `trim_for_recycle`, свежий — материализацию.
- **Артефакты.** Сырые логи и JSON (`docs/perf/_raw_r19_prf04_*.log`) по ярусам размера (CLAUDE.md, хранение артефактов); `docs/perf/R19_PRF04_THREAD_WAVE_GATE.md` + `_summary.csv`; манифест раунда.

### 5.4 Порядок коммитов (префиксы R30-12)

1. **C1** `bench(registry): thread-wave gate and claim-source counters for R17-PRF-04`. Харнесс, `bench-internals`-счётчики, карточка в `docs/perf/OPEN_ITEMS.md`. Политика не меняется.
2. **C2** `perf(runtime): reuse FREE registry heaps through an exact free-slot bitmap before minting` — `FreeSlotHint`, селектор, публикации, тесты §5.2, CHANGELOG/ARCHITECTURE. Сначала — временный измерительный коммит. На `main` он попадает только после GO; при NULL/NO-GO не попадает вовсе (нет «висящего» хука, правило 3 о `dbg_*`: счётчики C1 переоценить в той же задаче).
3. **C3** `bench(registry): R19-PRF04 thread-wave gate report` — отчёт, CSV, сырые логи, закрытие карточки (перенос в Recently resolved), манифест.
4. При изменении состава `production` — нет (новых features нет). Но это `perf(runtime)` в «всегда включённой» области, поэтому после волны — `npm run bench:table` + `npm run iai` по правилу «After each wave».

## §6 Что нельзя решить без исполнения

### 6.1 Точный эксперимент

```text
git worktree add ../sefer-alloc-prf04-a <C1-sha>
git worktree add ../sefer-alloc-prf04-b <C2-sha>
# в каждом worktree:
cargo build --release --example r19_prf04_thread_wave_gate --features production
cargo build --release --example r19_prf04_thread_wave_gate --features "production bench-internals"   # только oracle
# ретенция/активация: подпроцесс на ячейку, 5 повторов, R-wave K∈{2,8,32} × W∈{16,128}
# латентность: node scripts/paired-ab-runner.mjs --config docs/perf/r19_prf04_run.json --arms A,B   (R-live, R-churn; + A,A контроль)
```

- **Измерить:** Δcount, commit/RSS в покое и пик, Δ`segments_reserved_total`, p50/p99 первого alloc, wall; источники claim из `bench-internals`-прогона.
- **Критерий решения:** пороги §5.3. GO = выполнены BENEFIT, KILL-1, KILL-2 и все oracle; NULL = oracle выполнены, BENEFIT нет. NO-GO = нарушен KILL-1 или KILL-2.

Минимальный вариант «измерить сначала»: прогнать только арк A (C1) в R-wave. Если `commit_idle` при K=8, W=128 отличается от прогона W=1 менее чем на 64 MiB, V4 не нужен (NULL без реализации C2).

### 6.2 Чего не видно без исполнения

- Удержание одного FREE heap в байтах на текущем дереве (§2.4).
- Действительно ли `JoinHandle::join` возвращается после TLS-деструкторов (`AbandonGuard::drop`) на Windows и Linux. От этого зависит детерминизм A-oracle и теста §5.2. ГИПОТЕЗА: да, потому что join ждёт завершения нативного потока. Для `std::thread::scope` это не предполагается.
- Реальный контеншн на summary-слове при K=32/64.

### 6.3 Решения владельца

1. Утвердить пороги §5.3 (64 MiB / 25 %, 5 %, 2 %, 10 % p99) до запуска.
2. Является ли барьерная волна (пересоздаваемые пулы, `thread::scope` в цикле) целевым режимом. Если нет — V0 с документацией законен.
3. `numa-aware`: оставить V4 и там (реюз отдаёт страницы чужого узла) или в `numa-aware`-сборках сохранить fresh-first. Вне `production`; требует отдельного замера.
4. Предпочтение младшего индекса (компактность, ниже тронутых chunk) против ротации (меньше контеншн).
5. Нужен ли отдельный гейт для V6 (decommit payload primordial у FREE heap) — ортогональный вопрос ретенции.

## §7 Проверено и НЕ дало вывода

- **Цена материализации heap.** В `docs/perf` нет отчёта, изолирующего удержание одного heap после `trim_for_recycle`. `R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md` даёт RSS процесса после trim (3.2 MiB в строке заголовка), а не на один heap. `R31_3_…` §4 — Large-кэш на heap (232–403 MiB), к FREE-heap после exit-trim неприменимо. `R12_9` — только первый heap, раунд 12.
- **Отдельный Ir материализации.** `large_alloc_free_cycle` (`…-performance.md:298–300`) смешивает материализацию с первым Large-циклом; P3-синтез это подтверждает (`…-P3.md:37`).
- **Раскладка `HeapSlot`/`HeapCore` по кэш-линиям** (делит ли `state` линию с горячими полями владельца): `HeapCore` не `repr(C)` (`core.rs:95`), без компиляции не установить. Влияет только на V1/V2 и на скан у предела, не на V4.
- **Поведение `std::thread::scope` относительно TLS-деструкторов** — не проверялось (§6.2); тест и харнесс спроектированы на `spawn` + `join`.
- **Причина удаления `free_slots` в `81fa61a5`** найдена только косвенно: контракт `docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:59` и diff коммита. Отдельного отчёта с измерениями об этом нет.
- **Новых находок не выявлено.** Мелочь из §2.3 п.3 (maintenance может вызвать лишний `bump`) — частный случай R17-PRF-04, а не новая находка.
