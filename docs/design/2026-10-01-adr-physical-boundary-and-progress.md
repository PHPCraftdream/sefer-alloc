# ADR: физическая граница освобождения, ownerless-прогресс и perf-floor (Ph2 плана 2026-10-01-113842)

Статус: **ПРИНЯТ** владельцем проекта по его поручению «спроси у @ox» (все четыре вопроса делегированы консультанту `ox`; рекомендации консультанта приняты без изменений по существу). Условие, которое консультант оставил за владельцем (класс MODEL-LIMIT, §Решение п.3), принято на тех же основаниях и подтверждено экспериментом PG-1 ниже. Дата: 2026-10-01. Перед первым runtime-коммитом фазы Ph3 привязать к неизменяемому SHA.

Источники: консультация `ox` (read-only, без запуска кода), приёмочные записки Ph0, Ph1a, Ph1b, W+1/W+2 и PG-1 в `docs/reviews/`, план `docs/design/2026-10-01-113842-src-foundational-refactoring-plan-xs-sol.md`.

## Решение

1. Production-backend остаётся slab'ом. Учёт свободных блоков для ВСЕХ размеров и kind'ов (Small, Primordial, Large) переносится вне тела блока. Ни own-thread free, ни flush, refill, owner cut/trim, maintenance, fallback или teardown TLS не читают и не пишут тело блока между его выдачей пользователю и повторной выдачей. `Node::write_next`/`Node::read_next` уходят с этих путей.
2. Per-object `System` (W+1, Ph1b) и owner-side free по переданному cap (W+2) в production не принимаются. Ветка `proto/exact-object-ph1b` (коммит `cabc785b`) — исследовательский эталон, не сливается.
3. Класс **MODEL-LIMIT**: повторная выдача, zero/copy и физическое освобождение байтов общей reservation, пока ещё жив кадр, освободивший их через by-value `Box`. Это свойство любого allocator'а на общей reservation под строгими SB/TB, а не дефект конкретного пути. Класс задаётся исполняемыми expected-red клетками с проверкой причины и публикуется как известная граница модели; production его не закрывает. **Подтверждено PG-1** (`docs/reviews/2026-10-01-adr-pg1-w0-receipt.md`, коммит `4382e23b`): на `production internals bench-internals` свидетель `w0` (reuse внутри живого by-value кадра) красный в Miri SB и TB (UB при `Box::new` на alloc-пути, до записи через новый Box); контроли `w0-after` и `w0-same-frame` зелёные; reuse реально происходит (40 из 40 попыток по адресу).
4. Безусловный автономный ownerless reclaim не обещается. Публикуется условная гарантия G1–G4; фоновый поток по умолчанию не запускается.
5. Пределы perf/RSS (§Гейты) пред-регистрированы; их нарушение означает NO-GO.
6. Стабильный публичный API не ломается, версия не поднимается, приватные протокольные типы не становятся `pub unsafe`.

## Контекст

- P1 (`docs/correctness-open-items/ACTIVE.md`): установленный `SeferAlloc`, настоящий `Box`, producer остановлен после терминального RMW внутри `Box::drop`; owner trim пишет `next` в тело через root reservation; SB и TB дают exit 1. Нативного падения и нормативного вердикта языка нет. Paused-witness красный на обоих наборах: `alloc-global internals bench-internals` и `production internals bench-internals` (PG-1 закрыл пробел: раньше гонялся только первый).
- W+1/W+2/W+3 (изолированный `System`) зелёные в SB/TB. W−1 (root write 1/8 байт), `reissue-live-root` и dealloc через root при паузе красные. Следствие: внутри живого кадра законны только отдельная Rust-аллокация или потомок cap.
- Ph1b закрывает P1 только для 1..16 B ценой `System` на каждый объект и поиска в таблице на каждом `dealloc`; блоки от 17 B, Large и Primordial остаются в том же классе. В `--all-features` прототип ломает существующий slab-свидетель `r8_global_box_provenance`.
- На не-fastbin пути (`alloc-global` без `production`) `dealloc_small` пишет `write_next` прямо в `Box::drop` (PG-1, набор B: красны все сценарии, включая контроли).
- Горячий путь уже вне тела (`PerClass.slots`, `src/registry/heap_core/state/tcache.rs`). Тело трогают refill, flush и remote reclaim: `write_next` в `alloc_core_small_reclaim.rs`, `alloc_core_small/dealloc.rs`, `alloc_core_small_magazine.rs`; `read_next` в `alloc_core_small_impl.rs`.
- Own-thread free заменяет входящий cap на root-derived блок (`mem_impl.rs`, `dealloc_own_base.rs`).
- Нынешний прогресс: G2, повторный claim слота, worker после успешного `start_maintenance()` (опрос раз в 10 мс). Политика fresh-first в `pick_with_saturation` откладывает claim старых FREE-heap до исчерпания слотов.

## Рассмотренные альтернативы

- A1. Per-object `System` для узких размеров. Отклонена: частичное покрытие; заметная цена на малых размерах (README, до cutover — порядок величины); налог на поиск при каждом `dealloc`; ломает slab-свидетель под `--all-features`.
- A2. Owner-side free по cap. Отклонена: нужен mailbox `*mut u8`, идентичность владельца и политика осиротевших cap (Ph1b §10); дороже A1 без выигрыша в покрытии.
- A3. Per-object allocation для всех размеров. Единственная зелёная в SB/TB для класса MODEL-LIMIT; отклонена для production по цене; остаётся эталоном.
- A4. Worker по умолчанию ради безусловного прогресса. Отклонена: фоновый поток по умолчанию, отказ spawn, повторный вход std, fork, ~100 пробуждений/с.
- A5. Хранить cap для own-thread reuse точного размера (W+3). Отложена как опциональная доработка после GO. Ловушка: cap с диапазоном меньше блока, выданный под больший запрос, ухудшает SB.

## Последствия

Плюс: весь класс доступа к телу при retire устранён для всех размеров; remote merge становится пословным OR без касания пользовательских страниц; исчезает poisoning freelist; producer остаётся с одним RMW; возможное сжатие метаданных класса через однородные по классу leaves.
Минус: крупный рефакторинг refill/flush/cut; риск по Ir на cold/recycle; возможна фрагментация; класс MODEL-LIMIT остаётся и публикуется; цели шага 5 плана переформулируются: reuse ДО логического retire — мутант (обязан краснеть); reuse ПОСЛЕ retire при живом кадре — MODEL-LIMIT.

## Гарантии (README и rustdoc)

- **G1 (всегда):** каждый `dealloc` — терминальная публикация без доступа к телу и без аллокации; free не теряется и не удваивается.
- **G2 (владелец жив):** reclaim в slow path refill (в пределах бюджета), `trim_current_thread()`, выход потока.
- **G3 (владелец ушёл, worker нет):** при повторном claim слота. G3+ (если принято в Ph4b): ограниченная помощь перед ростом — резервированием сегмента или минтингом слота.
- **G4 (после `Ok` от `start_maintenance()`):** логический reclaim FREE-heap без дальнейших вызовов allocator'а; SLA нет; `Err` — гарантия не активна; гибель worker'а — abort.

Без G4 процесс без дальнейших вызовов allocator'а может удерживать такую память бессрочно. Физический возврат ОС не гарантируется.

## Гейты

**Корректность** (SB со strict-provenance и TB; строки `production internals bench-internals` и `alloc-global internals bench-internals`; Linux плюс Windows host):
- C1 retire при паузе: `Box<[u8;N]>`, N ∈ {1,7,16,17,32,256,1024}, плюс Large — PASS; мутант с возвращённым `write_next`/`read_next` — FAIL в этом месте.
- C2 reuse/zero/copy/realloc/release после join, включая новую инкарнацию на совпавшем VA — PASS.
- C3 own-thread churn и by-value функции с reuse после возврата кадра — PASS.
- C4 MODEL-LIMIT (ожидаемый FAIL с проверенной причиной): reissue при паузе; release последнего блока сегмента при паузе; release/кэш Large при паузе; W−0 (уже подтверждён PG-1). Диагностика обязана называть protected tag и место reissue/release; FAIL в любом другом месте — дефект.
- C5 протокольные мутанты (reuse до retire, двойной retire, ранний free дескриптора при pin, потерянная публикация) ловятся нативным учётным оракулом и/или Loom.
- C6 инъекция OOM на каждой границе prepare/commit: null или частичный результат, старый объект жив при OOM в `realloc`, `dealloc` без аллокаций.
- C7 native: Linux x86_64/aarch64, Windows x86_64, macOS arm64; runtime MSRV.

**Прогресс:** P1 worker reclaim за ≤ K проходов без вызовов allocator'а; P2 без worker — pending остаётся (отрицательный контроль); P3 отказ spawn и гибель worker'а; P4 G3+ (если принято).

**Perf/RSS** (база B — `main` на неизменяемом SHA, кандидат C — неизменяемый SHA; одинаковые toolchain, `production`, `Profile::default()`; пределы — отношение C/B в одном режиме; контроль A/A; ≥ 5 чередующихся прогонов в свежих процессах, медиана; таблицы и CSV строит скрипт):

| Ось | Метрика | Жёсткий предел |
|---|---|---|
| Горячий tcache (iai `small_churn_16b`, `churn_256b`, `churn_write_256b`, `aligned_churn_640b_a128`) | marginal Ir/op и EstCycles | ≤ 1.02 |
| Refill/flush (iai `cold_alloc_free_256x{16,64}b`, `recycle_alloc_free_256x{16,64}b`, `multiseg_cold_256k`) | EstCycles/op; Ir/op | EstCycles ≤ 1.05; Ir ≤ 1.10 |
| Remote merge (новое iai-плечо) | Ir на retired блок | ≤ 1.00 |
| Zeroed / realloc | marginal Ir | ≤ 1.05 |
| `bench:table` | отношение vs-mimalloc, C/B | geomean ≤ 1.05, худшая ≤ 1.10; warm bulk ≤ 1.15 |
| Реальный `#[global_allocator]`, многопоточность (larson/mstress, T = 1, 2, 4, 8) | Mops | geomean ≥ 0.95, каждая T ≥ 0.90 |
| Remote lag | p99 от публикации до retire | ≤ 1.10 |
| RSS (peak; burst→trim→idle) | RSS и commit | ≤ 1.05 (или +1 MiB) |
| Метаданные на Small route | байты у `System` | однородный fixture ≤ B; adversarial ≤ 304,128 |
| Фрагментация (смешанный по классам churn) | RSS / живые байты | ≤ 1.10 |

Оракулы на каждое плечо: tcache hit/miss; источник refill (≥ 95% из free-структуры в recycle-бенчах); число слов remote merge; число reserve/release сегментов; проходы worker'а; обратное чтение resolved config, `config_conflicts_total` delta = 0, процесс на плечо. NO-GO: нарушение жёсткого предела сверх шума A/A, активация < 95%, расхождение config, нет неизменяемой идентичности. INCONCLUSIVE блокирует GO. Обменивать регрессию горячего пути на выигрыш по RSS без нового решения владельца нельзя.

**До начала Ph3 (proof-gates):**
- PG-1: W−0/W−0′ и paused-witness на `production` — **ВЫПОЛНЕН** (`4382e23b`, MODEL-LIMIT подтверждён).
- PG-2: одноразовый спайк внетельного учёта вместо 4 `write_next` и 3 `read_next`; `miri_global_box_acceptance paused` на `production` и `alloc-global` зелёный; мутант с возвращённым `write_next` краснеет на `node.rs`; клетки (i) последний живой блок в освобождаемом сегменте при паузе, (ii) reissue при паузе, (iii) Large при паузе — ожидаемо красные с диагностикой «protected» в месте reissue/release, не в retire.
- PG-3: iai-плечо спайка на cold/recycle (информативно; EstCycles хуже ≥ 25% без пути улучшения — пересмотр геометрии).
- PG-4: решение владельца под классом MODEL-LIMIT — принято (см. статус).

## Матрица поддержки и совместимость

- Feature-наборы: `production` — строка поставки; Miri SB+TB на `production internals bench-internals` и на `alloc-global internals bench-internals`; `hardened`, `virgin-zero-skip`, `medium-classes`, `numa-aware` — native-тесты и сборка (Miri-клетки честно `NOT_RUN`); `batch-api` — experimental.
- ОС: Linux x86_64 и aarch64, Windows x86_64, macOS arm64; только 64 бита.
- Entry points: `GlobalAlloc::{alloc, alloc_zeroed, realloc, dealloc}` через установленный `#[global_allocator]` (Box, Vec, String; narrow-reborrow 1..7; align выше 16 и Large/high-align); `trim_current_thread`; `start_maintenance`/`maintenance_running`; fallback при teardown TLS и насыщении registry.
- OOM: ошибка подготовки ресурса даёт null или частичный batch, никогда abort или panic в `GlobalAlloc`; старый объект при OOM в `realloc` остаётся живым; `dealloc` не аллоцирует.
- Совместимость: стабильная поверхность (`SeferAlloc`, `AllocStats`, `MaintenanceStartError`, `Profile` и `*Config`, публичные `AllocCore`/`SegmentLayout` под `alloc-core`) не ломается; поверхность за `#[doc(hidden)]` и `internals` менять можно; любой слом стабильного API — отдельное решение и отдельная просьба о версии.
- Не становятся `pub unsafe` (только `pub(crate)`; тестам — безопасные числовые наблюдатели под `bench-internals`): `IssueTransaction`/`IssueTicket`; tickets терминальной публикации, retire и pending-claim; domain/heap lease и maintenance claim; descriptor caps (`AtomicPtr`), токены release ОС; `BitmapCut`/`RouteCut`, handles внетельной free-структуры, seam `Node`.

## Стоп-линии

- Не вводить per-object `System` в production без нового ADR.
- Не расходиться с native поведением только под Miri; не отключать protectors и валидацию; не выносить паузу за пределы кадра.
- Не переводить FAIL в MODEL-LIMIT без совпадения пред-регистрированной причины и зелёного нативного учётного оракула.
- Не продвигать в `production` при нарушенном жёстком пределе или активации < 95%.
- Не делать протокольные типы `pub unsafe`; не делать безопасный `dbg_*`, принимающий raw pointer.
- Не писать «autonomous» без успешного `start_maintenance()`.
- Не возвращать красный путь интрузивного slab как «быстрый fallback».
- Не поднимать версию и не пушить без отдельной просьбы.

## Допущения и непроверенное (из консультации)

- Оценки производительности внетельной структуры (refill через `tzcnt`, remote merge как пословный OR) — рассуждение, не измерение.
- Числа per-object `System` из README получены до cutover: годятся только как порядок величины.
- Поведение mimalloc/jemalloc/tcmalloc приведено по общеизвестным сведениям, в репозитории не проверялось.
- Трактовка класса MODEL-LIMIT как предела модели (а не языка) — гипотеза; нормативного вердикта нет.
- G3+ (помощь при росте) требует отдельной проработки lock-order и реентерабельности в Ph4b.

## Дополнения

- 2026-10-02: решения по фазам Ph4a/Ph4b/Ph4c (lease, G3+ не принят, порядок слотов, сужение legacy `pub` API, накопительный perf-якорь B0) — `docs/design/2026-10-02-adr-addendum-ph4-decisions.md`.
- 2026-10-02: геометрия off-body структуры для Ph3c (после PG-3) — `docs/design/2026-10-02-ph3c-offbody-geometry-design.md`.
