# Deep audit 07 — Оптимизации: возможности (не микро-регрессии)

**Дата:** 2026-07-17 · **Ревизия:** `main` @ `ffd3215` (+ незакоммиченный
mechanical split `alloc_core_small*.rs` / `alloc_core_large*.rs` в рабочем
дереве) · **Метод:** только статический анализ кода + зафиксированные числа из
`docs/perf/` (IAI_BASELINE, R7_*); бенчи заново НЕ гонялись.

**Система координат (текущая cost-модель, из зафиксированных данных):**

- Горячий churn — «выигранный фронт»: 16B churn ~18.3 ns/op, 1.08–10.15×
  быстрее mimalloc (`R7_BENCH_RESULTS.md` §2); iai-марджинал ~73 Ir/op после
  RAD-5. Kill-gate ±10 raw Ir на churn — любой кандидат обязан не трогать
  magazine-hit путь.
- Слабое место — **cold-direct 16–64B: 27–28 ns/op, 2.0–2.7× медленнее
  mimalloc** (`R7_BENCH_RESULTS.md` §2, §3) — это главный незакрытый разрыв.
- Refill-miss скан: закрыт directory (R7-A GO, 29–254×), но фича **не в
  `production`** (Cargo.toml:199 vs 269) и fallback-скан остаётся O(S).
- Commit-модель: R7-B + B6 дали first-heap 4.52 MiB → ~0.887 MiB, но тоже
  **не в `production`** (Cargo.toml:310).

---

## P0 — крупные возможности

### P0-1. Batch/scoped alloc API — амортизация TLS + классификации + bitmap-RMW

| Поле | Значение |
|---|---|
| **file:line** | внутренние примитивы уже есть: `src/alloc_core/alloc_core_small_magazine.rs:129` (`refill_class_bump_checked` — батчевый bump-carve прямо в caller-буфер), `:370` (`flush_class` — батчевый free), `src/alloc_core/alloc_core_small.rs:865` (`drain_freelist_batch`), `:1109` (`carve_batch`). Скалярная обвязка, которую надо амортизировать: `src/global/tls_heap.rs:394-419` (TLS-резолв на каждый op), `src/registry/heap_core_alloc.rs:63-79` (классификация), `:217-222` (RAD-5 `clear_magazine` RMW на каждый hit), `src/registry/heap_core_free.rs:297-341` (2 bitmap-оракула + mark на каждый free). |
| **Ожидаемый выигрыш** | Cold-direct 16–64B: сейчас ~27 ns/op против 10–14 у mimalloc. Пер-op скалярный налог, который батч амортизирует: TLS-load+branch, `class_for` LUT-load, `segment_base_of_ptr`+bitmap RMW×(1 на alloc-hit + 2–3 на free), stamp-check. Оценка порядка: **1.5–2× на direct-шейпах** (27 → ~14–18 ns/op) при батче ≥8; `R7_BENCH_RESULTS.md` §3 прямо называет batch/scoped API путём закрытия этого разрыва. |
| **Сложность/риск** | Средняя-высокая: НОВЫЙ публичный API (semver-поверхность), нужен дизайн scope-хендла, который пинит `&mut HeapCore` один раз (эквивалент `CurrentHeap::Own`, полученного единожды) + `alloc_batch(layout, &mut [*mut u8]) -> usize` / `free_batch`. Опасности: (а) хендл не должен переживать поток (TORN-протокол tls_heap.rs); (б) fallback-ветка требует spinlock-дисциплины; (в) magazine-байпас в батче должен сохранять D1/M2-инварианты — но `refill_class_bump_checked`/`flush_class` уже несут эти доказательства. |
| **Где эффект / где ноль** | Эффект: bulk-alloc шейпы (парсеры, десериализация, Vec-of-boxes, arena-подобные фазы), cold-direct. **Ноль на churn** (там уже magazine-hit, и GlobalAlloc-путь не меняется), ноль на Large. |
| **Новый API/гарантии** | Да — новая публичная поверхность (не GlobalAlloc). Гарантии GlobalAlloc не меняются. |

### P0-2. Промоушен `alloc-segment-directory` в `production`

| Поле | Значение |
|---|---|
| **file:line** | Cargo.toml:199 (`production = [...]` без directory), :269 (`alloc-segment-directory = ["alloc-core"]`); реализация `src/alloc_core/alloc_core_small.rs:358-483` (lookup), `:1636` (материализация при ≥32 сегментах). |
| **Ожидаемый выигрыш** | Измерено (R7_DIRECTORY_GONOGO.md, GO по всем 9 гейтам): refill-miss **9–12× при S=64, 29–39× при S=256, 166–254× при S=1023**; паритет при S≤3 (sidecar не материализуется до 32 сегментов — код идентичен). Для production-потребителя с сотнями сегментов (долгоживущий сервер) это единственный способ получить R7-выигрыш — сейчас он opt-in и по умолчанию никто его не получает. |
| **Сложность/риск** | Низкая механика (строчка в Cargo.toml + перепин iai) но: (а) задокументированный trade-off — при remote-dirty ≥10% drain-first путь бывает 0.7–1.0× (GONOGO §3, S=64 dirty=100%: 0.7×); (б) +6.1 KiB sidecar на кучу + 128 B/slot; (в) весь A1–A7 код попадает в производственную поверхность (A5-матрица корректности уже прогнана). Разумный гейт перед промоушеном: MT-бенч с высокой remote-плотностью. |
| **Где эффект / где ноль** | Эффект: многосегментные кучи (S≥32), особенно refill-miss/carve-нагрузка. Ноль: S<32 (не материализован), чистый magazine-hit churn, Large-путь. |
| **Новый API/гарантии** | Нет (перестановка feature-набора). |

### P0-3. Промоушен `alloc-lazy-commit` (Windows) в `production` / default

| Поле | Значение |
|---|---|
| **file:line** | Cargo.toml:310; `src/alloc_core/bootstrap.rs` (B6, `Segment::reserve_lazy` для primordial), `src/alloc_core/alloc_core_small.rs:31-84` (константы chunk), `:1022-1043` (grow-on-carve), `:1156-1175` (batch-commit). |
| **Ожидаемый выигрыш** | Измерено: first-heap commit **4.52 MiB → ~0.887 MiB (~5.2×)** после B6 (коммит `8977e88`); K2–K7 все PASS (first-alloc +6.2%, churn в шуме). Для thread-heavy процессов (per-thread кучи, MAX_HEAPS=4096) — порядок сотен MiB commit-charge экономии на пиках; сокращение RSS/commit — заявленная цель Workstream B. |
| **Сложность/риск** | Средняя: Windows-only ветка (Unix/miri — eager fallback, прозрачный, K7 PASS); cold-path цена ~100–150 µs на полный lifecycle сегмента (B5); коммит-fail-recovery покрыт 31 тестом + fault-injection. Риск главным образом в расширении производственной поверхности платформо-специфичным кодом. Открытая задача #191 (тестовые ассерты на недостижимой ноге) — привести в порядок до промоушена. |
| **Где эффект / где ноль** | Эффект: commit-charge/RSS на Windows, многопоточные процессы, короткоживущие кучи. Ноль: Unix (eager fallback), стационарный throughput (не про скорость). |
| **Новый API/гарантии** | Нет; поведенческая гарантия «commit по мере роста» уже задокументирована в фиче. |

---

## P1 — среднего масштаба

### P1-1. Остаточный O(S): fallback-скан выполняется даже при материализованном directory (carve-storm miss)

| Поле | Значение |
|---|---|
| **file:line** | `src/alloc_core/alloc_core_small.rs:478-483` («Directory miss: … fall through to the guarded linear-scan fallback») → полный скан `:509-713`. Вызов на каждый refill-miss: `alloc_core_small_magazine.rs:214-232` (`find_segment_with_free_checked` перед латчем `free_exhausted`). |
| **Суть** | Directory ускоряет только СЛУЧАЙ ПОПАДАНИЯ. Когда свободных блоков класса нет НИГДЕ (чистый cold-storm: каждый refill заканчивается carve), битовые слова нулевые → directory-скан дешёв, но затем **безусловно** выполняется полный линейный fallback O(S) с per-segment ring-guard-чтением (`tail_relaxed` на каждый сегмент). При S=1023 это те же ~67 µs на промах, что и без directory — R7-выигрыш на carve-пути не собирается. A6-гейты мерили hit-кейс (holes=0% = блоки ЕСТЬ). |
| **Ожидаемый выигрыш** | На cold-storm при больших S — устранение O(S) на каждый miss: порядок тот же, что headline R7 (десятки×–100× на miss-путь при S≥256). При S≤3 — ноль. |
| **Сложность/риск** | Средняя. Нужно доказать одностороннюю точность инварианта «бит снят ⟹ BinTable пуст» после `drain_dirty_segments` (A2 централизовал все 7 set_head-сайтов; A7 чинил единственный известный десинк). Ошибка даёт не UB, а преждевременный reserve сегмента (RSS-рост) — мягкий режим отказа; можно оставить fallback под `debug_assertions`/alloc-stats-счётчиком расхождений и снять его в release только после соака. |
| **Где эффект / где ноль** | Эффект: многосегментный cold-alloc/carve-storm (bulk-загрузка, старт сервиса). Ноль: churn, S<32, случаи, где скан находит блок (уже быстрые). |
| **Новый API/гарантии** | Нет. |

### P1-2. Инфраструктура-судья: ≥64-сегментный iai-бенч + нативный Windows wall-clock судья

| Поле | Значение |
|---|---|
| **file:line** | `benches/perf_gate_iai.rs` (12 функций, максимум 3 сегмента — барьер зафиксирован 4 раза: X5, T10, R1, IAI_BASELINE.md:1316-1328); `docs/perf/IAI_BASELINE.md:1388-1405` (R5-R2b: реальный wall-clock-сдвиг на Windows невидим для Ir; нативного профилировщика нет); готовые кирпичи: crates paired-ab (CRATE-P9), proc-memstat (P8). |
| **Ожидаемый выигрыш** | Сам по себе — ноль ns; но это ПРЕДУСЛОВИЕ для всего класса multi-segment оптимизаций (P1-1, повторная оценка X5/T10/R1-шейпов) и для диагностики Windows-специфичных costов (page fault/VirtualAlloc/TLB), которые Ir-судья принципиально не видит. Без него любой multi-segment кандидат снова упрётся в «бенч-сьют моделирует n≤3». |
| **Сложность/риск** | Низкая-средняя (бенч-код + перепин базлайна; ETW/нативный харнес — отдельная задача, paired-ab уже даёт process-level A/B/B/A протокол). Риск: нулевой для продукта. |
| **Где эффект / где ноль** | Мета-эффект на процесс принятия решений; на рантайм — ноль. |
| **Новый API/гарантии** | Нет. |

### P1-3. `sync_directory_for_segment`: O(49) RMW-свип после каждого ring-drain → инкрементальные publish по классу

| Поле | Значение |
|---|---|
| **file:line** | `src/alloc_core/alloc_core_small.rs:1735-1747` (полный свип 49 классов: чтение головы + set/clear бит на каждый), вызовы: `:437`, `:617-619`, `:1845-1848`. |
| **Суть** | Ring-entry несёт `(off, class)` — класс известен в `reclaim_offset`. Вместо пост-drain свипа всех 49 классов (49 чтений BinTable + до 49 бит-RMW в sidecar) можно публиковать empty→nonempty инкрементально только для реально затронутых классов (обычно 1–2 на drain). |
| **Ожидаемый выигрыш** | Малый-средний: срез ~45+ лишних RMW на каждый drain «грязного» сегмента; заметно на MT-нагрузке с высокой remote-плотностью — ровно там, где GONOGO §3 зафиксировал 0.7–1.0× (снижает документированный минус directory). Порядок: проценты на dirty-heavy, не разы. |
| **Сложность/риск** | Средняя: `reclaim_offset*` — статические функции без `&mut self` (нет доступа к sidecar), понадобится проброс контекста или сбор touched-классов bitmask'ом (u64) в drain-замыкании с одним пост-drain проходом по установленным битам. Инварианты directory (A5-матрица) переиграть. |
| **Где эффект / где ноль** | Эффект: xthread+directory, remote-dirty нагрузка. Ноль: ST, non-directory, churn. |
| **Новый API/гарантии** | Нет. |

---

## P2 — дешёвые/точечные

### P2-1. Пакетирование MagazineBitmap-RMW на refill/flush (пер-run hoisting + словные маски)

| Поле | Значение |
|---|---|
| **file:line** | `src/registry/heap_core_alloc.rs:423-429` (refill: цикл n−1 блоков, на каждый — `segment_base_of_ptr` + `SegmentMeta::new` + байтовый RMW `mark_magazine`); `src/registry/heap_core_free.rs:361-367` (flush: тот же паттерн ×FLUSH_N=8). |
| **Суть** | Блоки refill'а почти всегда из одного сегмента (bump-carve run) с шагом `block_size` ⇒ биты `off>>4` идут регулярной решёткой: для 16B-класса 15 соседних бит = 2 байта — можно ставить одним-двумя словными OR вместо 15 отдельных байтовых RMW; минимум — hoisting `SegmentMeta` на run (как `flush_class` уже делает run-detection Э8 для BinTable, alloc_core_small_magazine.rs:418-447). |
| **Ожидаемый выигрыш** | Порядок −5…−15 Ir/op на cold/recycle (сейчас ~182–195 Ir/op после RAD-5, т.е. единицы %). Churn — строго ноль (hit-путь не трогается). |
| **Сложность/риск** | Низкая-средняя; биты должны получиться бай-в-бит те же (маска по решётке шага). Прецедент: E1/Э7/Э8 batch-hoisting'и все прошли gate. |
| **Где эффект / где ноль** | Эффект: refill-miss/flush (cold, recycle). Ноль: churn, Large. |
| **Новый API/гарантии** | Нет. |

### P2-2. Interleaved-бит-макет: AllocBitmap + MagazineBitmap в одну кэш-линию (2 бита/слот)

| Поле | Значение |
|---|---|
| **file:line** | `src/alloc_core/alloc_bitmap.rs`, `src/alloc_core/magazine_bitmap.rs` (два независимых 32 KiB массива на разных offsets); free-путь читает/пишет ОБА: `heap_core_free.rs:297-341` (`is_in_magazine` → `is_free` → `mark_magazine`). |
| **Суть** | Каждый own-thread free сейчас трогает 2 разнесённые metadata-линии (плюс header-линию для `bump_of`). Чередование бит (bit0=free, bit1=magazine на слот) сводит оба оракула + mark к одной линии/одному RMW-слову. |
| **Ожидаемый выигрыш** | На горячем churn — около нуля (линии уже в L1: рабочий набор мал). На cold/рассеянных working set'ах (где RAM hits доминируют — см. cold-бенчи L1/RAM профили) — единицы % EstCycles. Порядок мал; главный аргумент — половина metadata-футпринта, который free-путь фолтит. |
| **Сложность/риск** | Средне-высокая для скромного выигрыша: переезд обоих bitmap-семантик через один механизм ломает `SegmentBitmap`-дедуп (R4-6), задевает M2-оракулы и RAD-5-инварианты. Делать только при появлении реального профиля (P1-2) с доказанным contention этих линий. |
| **Где эффект / где ноль** | Эффект: cold/scattered free-heavy. Ноль: горячий churn. |
| **Новый API/гарантии** | Нет. |

### P2-3. Zero-tracking для `alloc_zeroed` (пропуск memset на virgin-страницах)

| Поле | Значение |
|---|---|
| **file:line** | `src/alloc_core/alloc_core.rs:774-780` (`alloc_zeroed` = alloc + безусловный `Node::zero`); `src/registry/heap_core_alloc.rs:296-309`. |
| **Суть** | Свежекоммиченные страницы (Windows MEM_COMMIT demand-zero / anon mmap) уже нулевые — memset по ним двойная работа (и первокасание!). mimalloc ведёт page-level `is_zero`. У sefer уже есть точный сигнал virginity: PERF-PASS-2 использует его для пропуска bitmap-init (`alloc_core_small.rs:1541-1575`), и commit-frontier (`committed_payload_end`) точно знает, что выше bump никогда не писали. Для bump-carved блока с `off >= максимум-когда-либо-достигнутого-bump` память гарантированно нулевая. |
| **Ожидаемый выигрыш** | Только calloc-heavy нагрузки (интерпретаторы, zeroed-буферы): до полного среза memset-стоимости на fresh-путях (для 256 KiB блока — микросекунды/op). Порядок: разы на calloc-fresh, ноль на переиспользуемых блоках (там честный memset обязателен). |
| **Сложность/риск** | Средняя: нужен high-water «virgin frontier» per segment (одно поле; decommit-reset должен его сбрасывать корректно — decommit возвращает demand-zero страницы на Windows, но `MADV_DONTNEED`-семантика на macOS уже кусалась: см. отклонённый P4(b), `alloc_core_small.rs:1561-1566` — переиспользованные mapping'и НЕ считать нулевыми). Ошибка = выдача ненулевой памяти из alloc_zeroed — тихая корректностная бомба; нужен жёсткий differential-тест. |
| **Где эффект / где ноль** | Эффект: calloc-heavy. Ноль: alloc/realloc, churn. |
| **Новый API/гарантии** | Нет (усиление внутреннее). |

### P2-4. Medium-classes: включение после измерения (закрыть «medium не закончен»)

| Поле | Значение |
|---|---|
| **file:line** | `src/alloc_core/size_classes.rs:84-101` (EXTRAS 256K–1M под `medium-classes`), Cargo.toml:260 (opt-in, не в production); судья готов: `benches/medium_size_sweep.rs`; A7 (десинк directory на верхнем medium-классе) уже починен (#182). |
| **Суть** | Без фичи запросы 253 KiB–4 MiB идут Large-путём: целый 4 MiB сегмент + OS/кэш-раунд-трип на каждый (large_cache смягчает, но churn+teardown 1024B-профиль показывает, что decommit-цена уже кусается). Medium-классы маршрутизируют их через small/magazine-путь (refill_n у них 1 блок — D3-бюджет уже корректен, tcache.rs:64-77). |
| **Ожидаемый выигрыш** | На 256K–1M нагрузках — порядок large-cache-miss-цены (сотни ns – µs/op) против ~десятков ns small-пути; внутренняя фрагментация 4 MiB→(1–4 блока/сегмент). Точных чисел нет — sweep-бенч есть, прогнать и решить. |
| **Сложность/риск** | Низкая механика; риск — SIZE2CLASS растёт 16 KiB→64 KiB (`size_classes.rs:140-150`, clippy-нота) — кэш-давление классификатора на реальных working set'ах (X6-ловушка: микробенчи это не увидят). |
| **Где эффект / где ноль** | Эффект: 256 KiB–1 MiB блоки. Ноль: всё остальное (таблица классов ниже не меняется). |
| **Новый API/гарантии** | Нет (feature-решение). |

### P2-5. NUMA-билды: directory сейчас write-only индекс

| Поле | Значение |
|---|---|
| **file:line** | `src/alloc_core/alloc_core_small.rs:327-336, 358` (`not(feature = "numa-aware")` отключает directory-lookup; bitmap поддерживается, но не читается — линейный двухпроходный NUMA-скан остаётся O(S)). |
| **Ожидаемый выигрыш** | Для numa-aware конфигурации — тот же класс выигрыша, что P0-2 (десятки× на refill-miss при больших S), сейчас недоступный вовсе. |
| **Сложность/риск** | Средняя: node-aware выбор бита (нужен node_id per slot рядом с bitmap или двухпроходный обход битов с проверкой `node_id_of`). Нишевость: numa-aware не в production. |
| **Где эффект / где ноль** | Только numa-aware билды с S≥32. Иначе ноль. |
| **Новый API/гарантии** | Нет. |

---

## Закрытые направления (не переоткрывать без нового судьи)

Зафиксированы честными reject'ами с числами — повторный заход без изменения
предпосылок (бенч-масштаба или cost-модели) будет тратой цикла:

| Направление | Вердикт | Где записано |
|---|---|---|
| TCACHE_CAP 32/64 (пере-свип пост-RAD-5) | NO-GO ×2: churn +13%/+39% Ir/op, first-commit +0.4/+1.2 MiB; доминирует размер `PerClass`, не M2-скан | `R7_TCACHE_SWEEP.md` §7–8 |
| Pool-cap default (4 / 16 MiB) | Остаётся; пресеты low-rss/balanced/throughput задокументированы — дальше это конфиг пользователя, не код | `R7_POOL_CAP_PRESETS.md` §10 |
| Скан-хинты/пер-сегмент бит-карты для `find_segment_with_free` (X5/T10/R1) | NO-GO ×3 при n≤3; барьер — бенч-масштаб (см. P1-2), сам directory (R7-A) этот класс закрыл | `IAI_BASELINE.md` §X5/§T10/§R1 |
| clz-классификатор vs SIZE2CLASS LUT | NO-GO (EC регресс 10/11) | `IAI_BASELINE.md` §X6 |
| Батчинг `clear_magazine` с issue-пути (R3) | NO-GO: точность бита в момент issue — load-bearing в 2 местах (утечка блоков) | `IAI_BASELINE.md` §R3 |
| G1 (фолд magazine-оракула в AllocBitmap) | Reject, замещён RAD-5 (GO): O(1) bitmap уже стоит на пути | `IAI_BASELINE.md` §G1, §RAD-5 |
| Chunk-size 64–512 KiB (B5) | 256 KiB оставлен; дискриминатор только cold-lifecycle | `R7_INCREMENTAL_COMMIT.md` §8 |
| `Instant::now` на large-путях | Уже закрыт fast-path early-exit'ом (`alloc_core_large_cache.rs:27-44`, `alloc_core_small_pool.rs:437`) | код |

Отдельно: cache-line работа по ключевым структурам в основном исчерпана —
`PerClass` count+slots в одной линии (PERF-PASS-5, tcache.rs:114-144), HeapSlot
false-sharing partition (PERF-PASS-4), `AllocCore` field-order честно измерен
как no-op под `repr(Rust)` (`alloc_core.rs:277-309`). Остаток — только P2-2.

---

## Сводная таблица приоритетов

| # | Возможность | file:line (якорь) | Выигрыш (порядок) | Сложность/риск | Нагрузка с эффектом | Ноль-эффект | Новый API |
|---|---|---|---|---|---|---|---|
| **P0-1** | Batch/scoped alloc API | alloc_core_small_magazine.rs:129,370; tls_heap.rs:394 | 1.5–2× cold-direct 16–64B (27→~15 ns/op) | Средн.-выс. / semver, lifetime хендла | bulk-alloc, direct | churn, Large | **Да** |
| **P0-2** | `alloc-segment-directory` → production | Cargo.toml:199,269; alloc_core_small.rs:358 | 9–254× refill-miss при S≥64 (измерено) | Низк. механика / dirty≥10% минус, +6 KiB | многосегментные кучи | S<32, churn | Нет |
| **P0-3** | `alloc-lazy-commit` → production (Win) | Cargo.toml:310; bootstrap.rs (B6) | first-heap commit 5.2× ↓ (измерено) | Средн. / Win-ветка, cold +100–150 µs/lifecycle | commit/RSS, many-threads | Unix, throughput | Нет |
| **P1-1** | Снять O(S)-fallback при точном directory | alloc_core_small.rs:478-483,509-713 | десятки× на carve-storm miss при S≥256 | Средн. / нужен proof инварианта; отказ = RSS, не UB | cold-storm, большие S | S≤3, churn | Нет |
| **P1-2** | ≥64-seg iai-бенч + нативный Win-судья | perf_gate_iai.rs; IAI_BASELINE.md:1388 | мета (разблокирует P1-1 и класс X5/T10/R1) | Низк.-средн. / нулевой | процесс решений | рантайм | Нет |
| **P1-3** | Инкрементальный directory-sync вместо O(49)-свипа | alloc_core_small.rs:1735-1747 | проценты на remote-dirty MT | Средн. / инварианты A5 | xthread+directory dirty | ST, churn | Нет |
| **P2-1** | Батч MagazineBitmap-RMW на refill/flush | heap_core_alloc.rs:423; heap_core_free.rs:361 | −5…−15 Ir/op cold/recycle (единицы %) | Низк.-средн. | cold/recycle | churn | Нет |
| **P2-2** | Interleaved alloc+magazine bitmap | alloc_bitmap.rs; magazine_bitmap.rs | единицы % на scattered-free | Средн.-выс. / M2-поверхность | cold/scattered | горячий churn | Нет |
| **P2-3** | Zero-tracking в `alloc_zeroed` | alloc_core.rs:774-780 | разы на calloc-fresh | Средн. / корректностная бомба при ошибке | calloc-heavy | alloc/churn | Нет |
| **P2-4** | Medium-classes: прогнать sweep, решить | size_classes.rs:84-101; Cargo.toml:260 | large-путь → small-путь для 256K–1M | Низк. / SIZE2CLASS 64 KiB давление | 256K–1M блоки | остальное | Нет |
| **P2-5** | Node-aware directory под numa | alloc_core_small.rs:327-336 | как P0-2, для numa-билдов | Средн. / нишевое | numa, S≥32 | все прочие | Нет |

**Главный вывод.** Микро-оптимизационный запас скалярного горячего пути
практически выбран (пять PERF-PASS'ов, RAD-1..5, Э-серия; churn — выигранный
фронт с kill-gate ±10 Ir). Оставшийся крупный выигрыш лежит в трёх местах:
(1) **амортизация пер-op обвязки батч/скоуп-API** — единственный названный путь
к закрытию 2–2.7× cold-direct разрыва с mimalloc; (2) **доставка уже выигранных
R7-механизмов в production-набор** (directory, lazy-commit — оба GO/цель
достигнута, оба выключены по умолчанию); (3) **достройка судейской
инфраструктуры** (≥64-seg бенч, нативный Windows-профиль), без которой
multi-segment и Windows-специфичные кандидаты недоказуемы — что уже четырежды
останавливало этот класс работ.
