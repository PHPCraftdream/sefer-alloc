# Перф-ревью: оптимальность hot-path и честность методологии измерений

**Дата:** 2026-07-09. **Исполнитель:** независимый ревьюер №7 (угол —
ОПТИМАЛЬНОСТЬ), один из 7 параллельных ревьюеров. **Режим:** только чтение и
анализ; ни один бенчмарк не перезапускался (все числа ниже — из кода и из
существующих ledger-документов проекта; где вывод требует новых измерений,
это сказано явно).

**Вердикт (кратко):** production hot-path (magazine hit / own-thread free)
в отличной форме — на нём ноль атомарных RMW, O(1) резолюция класса,
кэшированные membership/stamp-проверки; комментарии «fast path»/«O(1)»
соответствуют коду. Методология измерений — одна из самых честных, что
встречаются (ledger honest-reject'ов с числами, детерминированный
Ir-судья, канонизация единиц в `bench:table`). Главные проблемы: (1) ОТКРЫТАЯ
регрессия PERF-4 (0.3.0 медленнее 0.2.1 на segment-churn) имеет конкретный,
видимый в коде механизм — decommit-путь выполняет мёртвую работу
непосредственно перед полным release сегмента, и hysteresis отсутствует; (2)
iai-судья считает проценты от сумм, в которых ~58–90 % — bootstrap-константа
процесса, из-за чего номинальные процентные пороги GO/NO-GO имеют
непостоянную (2–10×) фактическую жёсткость per-op; (3) бенч `Vec_push`
не измеряет то, что заявляет его заголовок.

---

## Scope

- Hot path alloc/dealloc/realloc: `src/global/sefer_alloc.rs`,
  `src/global/tls_heap.rs`, `src/registry/heap_core.rs` (актуальное имя
  per-thread magazine/tcache fast path — подтверждено), `src/registry/tcache.rs`,
  `src/alloc_core/alloc_core.rs`, `size_classes.rs`, `segment_table.rs`,
  `segment_header.rs`, `alloc_bitmap.rs`, `remote_free_ring.rs`,
  `src/registry/heap_slot.rs`.
- Методология: `benches/global_alloc.rs`, `benches/perf_gate_iai.rs`,
  `benches/large_realloc.rs`, `scripts/bench-table.mjs`, `docs/BENCHMARKS.md`,
  `docs/HEAP_BENCH.md`, `docs/ALLOC_BENCH.md`, `docs/perf/IAI_BASELINE.md`.
- Незакрытые perf-расследования: `docs/checkpoints/2026-07-08-perf4-decommit-churn-investigation.md`,
  `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`,
  `docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`, `docs/perf/FAULT_PROBE.md`,
  `docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`.
- Места, где комментарий сам признаёт цену операции (`hardened`-деление и т.п.).

## Методология ревью

Чтение полного пути одного alloc/dealloc/realloc в конфигурации `production`
(`alloc-global + alloc-xthread + alloc-decommit + fastbin`) от
`SeferAlloc::alloc` до записи в сегментные метаданные; инвентаризация
атомиков/ветвлений/зависимых загрузок на пути одного потока; сверка
заявлений комментариев («fast path», «O(1)», «hoisted», «byte-identical»)
с фактическим кодом; арифметическая проверка чисел в perf-ledger'ах против
формы бенчей (op-count'ы, скейлы, bootstrap-константы). Количественные
оценки даны только там, где они выводятся из кода или из уже опубликованных
детерминированных таблиц; остальное помечено «требует профилирования».

---

## Находки

### F1 — [HIGH] Decommit-путь пустого small-сегмента: мёртвая работа перед немедленным release; hysteresis отсутствует. Это и есть наиболее вероятный механизм открытой регрессии PERF-4

Открытые задачи #216/#217 (checkpoint
`docs/checkpoints/2026-07-08-perf4-decommit-churn-investigation.md:70-74,97-102`)
зафиксировали внешний сигнал: 0.3.0 на shamir-db sweep («много коротких
small-сегментов, быстро циклирующих») на ~15–18 % медленнее 0.2.1, гипотеза —
`alloc-decommit`, добавленный в `production` бандл в 0.3.0. Расследование
поставлено в очередь, но не начато. Чтение кода даёт гипотезе конкретный
механизм — даже два:

1. **Мёртвая работа на каждом опустевшем сегменте.** Все три production-точки,
   где `dec_live_and_maybe_decommit`/`dec_live_batch_and_maybe_decommit`
   возвращают `true`, немедленно вызывают `table.recycle(base)`:
   - `dealloc_small` — `src/alloc_core/alloc_core.rs:3329-3332`;
   - ring-drain в `find_segment_with_free_impl` — `alloc_core.rs:2734-2739`;
   - `flush_run` — `alloc_core.rs:2432-2438`.

   А `SegmentTable::recycle` (`src/alloc_core/segment_table.rs:381-421`,
   release на :415) отдаёт ОС **весь** резерв (`os::release_segment`).
   При этом за мгновение до этого `decommit_empty_segment`
   (`alloc_core.rs:1201-1262`) успевает выполнить:
   - syscall `os::decommit_pages` на ~4 MiB payload (:1206) — избыточен перед
     `MEM_RELEASE`/`munmap` всего резерва;
   - обнуление 49 голов BinTable (:1216-1219);
   - ~1 KiB записей page-map (`PAGES_PER_SEGMENT = 1024`, :1223-1227);
   - повторное зануление alloc-bitmap — **32 768 байт** побайтовым циклом
     (`AllocBitmap::FOOTPRINT`, `src/alloc_core/alloc_bitmap.rs:64,80-86`).

   Из всего этого для оставшихся stale-записей текущего ring-drain
   load-bearing только `meta.set_bump(payload_start)` (:1214) — guard
   `off >= bump` отсекает записи ДО обращения к bitmap/BinTable. Остальное —
   работа, результат которой уничтожается release'ом через микросекунды.

2. **Отсутствие hysteresis.** Workload, осциллирующий вокруг границы
   заполнения сегмента, на каждом цикле платит: reserve (syscall) + init
   метаданных (header + 1 KiB page-map + BinTable + 32 KiB bitmap-заливка
   в `reserve_small_segment`, `alloc_core.rs:3928-4025`) + first-touch
   page faults, затем decommit + reset + release (см. выше). Large-путь от
   этого класса проблем защищён кэшем (`large_cache`, 8 слотов + budget +
   decay); small-сегменты аналога не имеют. mimalloc в этом месте держит
   purge delay (по умолчанию десятки мс) — у sefer порог срабатывания
   нулевой.

**Побочное следствие:** ветка recommit-on-reuse в `carve_block`/`carve_batch`
(`alloc_core.rs:3132-3136, 3215-3219`) в production-потоке недостижима —
decommit'нутый Small-сегмент никогда не доживает до следующего carve (его
всегда release'ат в той же операции). Проверка `is_decommitted()` на carve —
дешёвая, но мёртвая ветка; весь recommit-механизм жив только в тестах.

**Оценка:** экономия варианта (a) ниже — 1 syscall + ~34 KiB stores на каждый
опустевший сегмент; вклад в 15–18 % регрессии shamir-db — **требует
профилирования** (это ровно предмет #216).

**Рекомендация (формы для #217, мерить через судью до выбора):**
- (a) дёшево и без смены политики: на пути «release немедленно» не звать
  `decommit_pages` и не делать bt/pm/bitmap-reset — только `set_bump`
  (доказательство достаточности — guard-порядок в `reclaim_offset*`);
- (b) закрыть саму гипотезу PERF-4: hysteresis — держать последние N пустых
  small-сегментов committed+registered (аналог `large_cache` / mimalloc
  purge delay) и переиспользовать в `reserve_small_segment`; существующий
  recommit-механизм тогда снова становится живым по назначению;
- (c) для #216 нужен НОВЫЙ iai-бенч, циклирующий именно пустение/наполнение
  small-сегментов: из текущих 11 бенчей этот путь частично видит только
  `multiseg_cold_256k` (3 сегмента), а `cold_*`/`recycle_*`/`churn_*` живут в
  primordial (kind=Primordial из decommit исключён, `alloc_core.rs:1054-1065`)
  и decommit не дергают вовсе.

### F2 — [MEDIUM] Ir-судья: bootstrap-константа процесса доминирует в суммах, процентные пороги GO/NO-GO имеют непостоянную фактическую жёсткость per-op

Каждая iai-функция строит аллокатор заново в своём процессе
(`benches/perf_gate_iai.rs:89` и далее в каждом бенче), поэтому каждое число
таблицы включает полный bootstrap (реестр + primordial-резерв + 32 KiB
bitmap-init + Tcache-zero). Собственные таблицы проекта это показывают:
`large_alloc_free_cycle` = **73 011 Ir за ОДИН alloc+free**
(`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md:63-75`) — т.е. константа
≈ 73k. Отсюда маржинальная цена op-pair:

| бенч | всего Ir | ops | маржинально, Ir/pair | доля bootstrap |
|---|---:|---:|---:|---:|
| small_churn_16b | 81 423 | 64 | ≈ 131 | ≈ 90 % |
| cold_alloc_free_256x16b | 125 354 | 256 | ≈ 204 | ≈ 58 % |

Следствия:
- Номинальный порог PERF-3 «≤ +1 % Ir на непрофильных бенчах» на churn
  фактически допускает **+12.7 Ir/pair ≈ +9.7 % per-op**, а на cold — только
  ≈ +2.4 % per-op: один и тот же «1 %» в 2–10 раз мягче/жёстче в зависимости
  от доли константы. Второй стиль порога («±10 Ir kill-threshold» на churn,
  X4-B, `docs/perf/IAI_BASELINE.md:174-184`) — наоборот, ±0.16 Ir/op,
  гипер-строг. Два стиля обрамляют сигнал с разных сторон без единой шкалы.
- Любое изменение per-claim метаданных (Tcache, RunStack, gen-table)
  сдвигает ВСЕ бенчи разом через константу: в PERF-2 CAP=32 из «+28 % churn»
  (+22 859 Ir) как минимум +18 819 — это константа (виден на
  `large_alloc_free_cycle`, который magazine вообще не трогает;
  `PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md:96,199-203`); маржинально churn
  регрессировал на ≈ +63 Ir/pair (≈ +50 % per-op) — регрессия реальна, но
  заголовочные проценты измеряют не то, чем названы.

Важно: **ни один вынесенный вердикт от этого не переворачивается** — PERF-2 и
PERF-3 подтверждены wall-clock'ом на крайних точках, X7-раздел IAI_BASELINE
(:342-371) декомпозицию делает образцово. Проблема — в том, что декомпозиция
применяется эпизодически, а пороги сформулированы на сырых суммах.

**Рекомендация:** (1) добавить в `scripts/iai.mjs`-отчёт колонку
«маржинально Ir/op» (константа — из `large_alloc_free_cycle` или отдельного
`bootstrap_only`-бенча); (2) переформулировать пороги будущих экспериментов в
маржинальных Ir/op; (3) в PERF-2 зафиксировать сноской, что CAP=32/64
отклонены по iai-only (wall-clock прогнан только для CAP=128) — вывод
устойчив (X4-A воспроизведён), но полнота метода должна быть видна.

### F3 — [MEDIUM] Бенч `Vec_push` не измеряет заявленное: рост «Vec» схлопнут в одну аллокацию 4 KiB

`benches/global_alloc.rs:216-256`: комментарий (:217-218) заявляет
«exercises realloc + many small allocs as the Vec grows», но
`new_layout = Layout::array::<i64>(new_cap.max(VEC_PUSHES))` (:231) при первом
же росте прыгает сразу к полной ёмкости (512 × i64 = 4096 B), `cap`
становится 512 (:241) и ветка роста больше не выполняется. Фактически
за итерацию: **1 alloc(4096) + 512 записей + 1 dealloc, 0 realloc'ов**. То же
в арме mimalloc (:258-294). `scripts/bench-table.mjs:36-39` истинную форму
честно документирует («the growth loop's capacity jumps straight to its
final size…»), т.е. канонический вывод не врёт — но комментарий бенча и
подписи в README/ALLOC_BENCH («Vec push/grow churn», «real-world pattern»)
описывают несуществующий сценарий.

Латентно: армы Sefer/mimalloc освобождают старый указатель фиксированным
512-элементным `layout` (:227, :237-238) — при снятии клампа `.max(VEC_PUSHES)`
это станет layout-mismatch (System-арм делает правильно, :308). Реальный
паттерн геометрического роста при этом покрыт отдельным бенчем
`realloc_grow_geometric` (`benches/large_realloc.rs:59-90`), так что дыры в
покрытии нет — есть неправильная этикетка.

**Рекомендация:** либо убрать кламп и вести old_layout честно (как в
System-арме) — тогда имя соответствует содержимому; либо переименовать в
`single_alloc_4k_write` и поправить комментарий + подписи таблиц.

### F4 — [MEDIUM] Масштабирование refill-пути по числу сегментов не измерено: `find_segment_with_free` — O(n сегментов) + drain кольца КАЖДОГО сегмента, а максимальный n в бенчах = 3

`find_segment_with_free_impl` (`src/alloc_core/alloc_core.rs:2630-2775`) на
каждом refill-miss обходит все слоты таблицы и под `alloc-xthread` дренирует
`RemoteFreeRing` каждого Small-сегмента (:2706-2740) — минимум 2 атомарные
загрузки head/tail на сегмент даже при пустом кольце, по метаданным-строкам,
которые пишут remote-фрееры. Латч `free_exhausted` (`:1990,2014-2061`)
корректно ограничивает это одним проходом на refill, но сам проход остаётся
линейным. Проект это знает: X5 honest-reject прямо пишет, что выигрыш/боль
проявляется при 100+ сегментах и «no current bench models that»
(`docs/perf/IAI_BASELINE.md:285-303`); `multiseg_cold_256k` покрывает n=3
(`benches/perf_gate_iai.rs:361-386`). Т.е. поведение production-refill на
длинноживущем сервере с сотнями сегментов на поток — слепая зона обоих судей.

**Рекомендация:** добавить iai-бенч с ≥64 зарегистрированными small-сегментами
одного класса (это же — предусловие для любого возврата к X5-семейству, как
сам ledger и требует). До этого не принимать/не отклонять решений об
O(n)-скане. Количественная оценка деградации — требует профилирования.

### F5 — [LOW] `drain_freelist_batch`: комментарий «inc_live применяется ОДИН раз на k» против цикла из k вызовов `inc_live`

Док-комментарий (`src/alloc_core/alloc_core.rs:2842-2844`) заявляет hoisted
`inc_live` «ONCE by k», код в обеих cfg-ветках делает
`for _ in 0..k { meta.inc_live(); }` (:3046-3049 и :3096-3098). Батч-примитив
существует и используется соседями: `add_live(n)`
(`src/alloc_core/segment_header.rs:1052-1055`, вызов в `carve_batch`
:3228-3229), `sub_live(k)` в `flush_run`. Сейчас LLVM почти наверняка
сворачивает цикл (обычные load/store по одному адресу), т.е. это не измеримая
деградация, а рассогласование комментария и кода + хрупкость (если
`inc_live` когда-нибудь станет несворачиваемым — atomic/volatile — цикл
материализуется). Однострочная замена на `meta.add_live(k as u32)`.

### F6 — [LOW] Незакрытый пункт плана P4(b): `alloc_zeroed` всегда memset'ит, включая virgin-блоки на свежезакоммиченных (нулевых) страницах

`HeapCore::alloc_zeroed` (`src/registry/heap_core.rs:778-791`) и
`AllocCore::alloc_zeroed` (`alloc_core.rs:484-490`) зануляют безусловно.
План P4(b)/S3 (`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md:113-116,
171-173`) требовал «NOT memset zeros over OS zeros» только при железном
virgin-флаге + poison-counterfactual — пункт не реализован И не занесён в
ledger как honest-reject: нить просто оборвана. Та же семья: 32 KiB
bitmap-init побайтово пишет нули поверх гарантированно нулевых свежих страниц
при каждом `reserve_small_segment` (`alloc_bitmap.rs:80-86`) — вклад в
bootstrap-константу F2 и в цену segment-cycle из F1. Выгода — только на
calloc-тяжёлых/segment-churn формах; **требует профилирования**; закрыть
пункт в любую сторону (реализация по правилам плана или reject с числами).

### F7 — [LOW] `bench_churn_alloc*`: делитель ns/op не совпадает с фактическим числом операций (+25 % систематического завышения, смешение фаз)

Замыкание churn-бенча (`benches/global_alloc.rs:88-120`) делает 256 prefill
allocs + 1024 churn-пары + 256 teardown frees = **1280 пар**, а
`bench-table.mjs` делит на `OPS = 1024` (`scripts/bench-table.mjs:40,48-50`).
Репортуемый churn-ns/op завышен на ≈ 25 % и содержит ~20 % «холодных» операций.
Все три аллокатора страдают одинаково, порядок сохраняется; направление
смещения ратио — консервативное ПРОТИВ sefer (sefer медленнее на cold-доле,
т.е. его churn-преимущество слегка занижено). Для скрипта, созданного ради
единства единиц, лучше делить на 1280 либо таймить только churn-петлю
(`iter_batched`).

### F8 — [LOW] Риск false sharing: `HeapCore::thread_free` пишется чужими потоками и не изолирован в своей кэш-линии

`thread_free: AtomicPtr<u8>` (`src/registry/heap_core.rs:212-213`) — remote
CAS-push при cross-thread free Large-сегментов
(`push_large_deferred_free`, :513-520) — лежит в `repr(Rust)`-структуре рядом
с owner-горячими полями (`last_stamped_segment`, `core.small_cur` и т.п.);
разделение линии не исключено. Частота — только cross-thread Large-фри,
поэтому Low; при workload'ах с миграцией больших буферов между потоками
стоит `#[repr(C)]`+padding или выравнивание поля. Остальная картина
межпоточного трафика чистая: per-slot счётчики диагностик в `HeapSlot`
(w3), кольца — в метаданных сегмента, magazine — owner-private без атомиков.
Требует профилирования прежде чем платить памятью за padding.

### F9 — [LOW] Витринные заголовки: предусловия флагманских чисел и переставшее быть «unfavorable» имя бенча

- README:455-478: 12–35× vs mimalloc на large alloc+free — честные числа для
  формы «тот же размер, немедленный повтор» — идеальной для 8-слотного кэша с
  committed-страницами (осознанный трейд RSS↔latency, задокументирован у
  конфига). У заголовочной таблицы стоит одной строкой назвать предусловия
  (reuse в окне decay, фактор размера ≤ 2×), чтобы читатель не экстраполировал
  на смешанные размеры.
- `realloc_in_place_unfavorable` (`benches/large_realloc.rs:14-17,92-104`):
  после OPT-G «соседи, запрещающие рост in-place» на sefer структурно не
  действуют (Large-блок живёт в выделенном сегменте и растёт в свой
  `span_usable`); бенч теперь измеряет header-update vs чужой copy — README
  это раскрывает (:487-490), но имя бенча обещает адверсариальность, которой
  для sefer больше нет. Переименовать/добавить парный бенч принудительного
  move (рост за пределы span), чтобы copy-путь realloc оставался под
  наблюдением.
- «Cold direct» в README (:554-556) — на criterion steady state это
  recycle-путь, не first touch; ALLOC_BENCH это честно оговаривает
  (:310-318, :355-367), поверхностная подпись отстаёт.

### F10 — [LOW] Гигиена perf-документов: стареющая шапка IAI_BASELINE; нереализованная рекомендация FAULT_PROBE

- `docs/perf/IAI_BASELINE.md:6-27`: шапка всё ещё «Commit: post-W3», «all 10
  benches»; указатель на актуальную 11-бенч таблицу добавлен (:24-27) — ретро-
  NOTE закрыт наполовину, факты в шапке остались стары.
- `docs/perf/FAULT_PROBE.md:73-79` рекомендует fault-колонку (±20 %, coarse) в
  `perf-gate.yml` на реальном Linux CI — в workflow её нет (проверено grep'ом).
  Либо реализовать, либо пометить рекомендацию как отклонённую.

### F11 — [INFO] Верифицировано чистым (заявления комментариев соответствуют коду)

Проверено чтением, расхождений «декларация vs код» на горячем пути НЕ найдено:

- **Ноль атомарных RMW на production hot-path одного потока.** Magazine hit
  (`heap_core.rs:634-722`) — TLS-load + один unsigned-compare (Э2,
  `tls_heap.rs:308-333`), одна классификация (Э9, :584-600), pop массива;
  hit-счётчик выключен из `production` (`alloc-stats`, :701-707). Own-thread
  free (`dealloc_routing` :1266-1300 → `dealloc_own_thread_with_base`
  :852-1092) — mask, direct-mapped own-cache→hash (`segment_table.rs:460-486`),
  ограниченный CAP=16 сканом M2-оракул (Э10, branchless :1009-1027), byte-load
  bitmap, два store. Всё plain-память, без lock-префиксов (Э5 повсюду, включая
  large-cache hit `alloc_core.rs:3403-3430`).
- **`class_for` действительно O(1)** для align ≤ 16 (compile-time `SIZE2CLASS`,
  `size_classes.rs:98-110,161-182`); slow-walk для align > 16 ограничен и
  редок — соответствует док-заявлению.
- **`hardened`-цена изолирована как заявлено.** Признанные в Cargo.toml
  (:167-184) издержки — «real division, ~tens of cycles» interior-guard
  (`heap_core.rs:887-894`, `alloc_core.rs:3284-3287`), gen-RMW на issue/free —
  все под `#[cfg(feature = "hardened")]`; `hardened` не входит в `production`;
  стоимость опубликована с корректной bootstrap-декомпозицией
  (`IAI_BASELINE.md:307-390`). Утечек hardened-кода в default-путь не найдено.
- **Батч-заявления Э7/Э8/E1/E3** («hoisted set_head/bump/live», guard'ы
  per-block) — соответствуют коду (`drain_freelist_batch`, `flush_run`,
  `carve_batch`), с единственной оговоркой F5.
- **Ретро-долги от 2026-07-06 закрыты:** дрейф 561 912 выровнен
  (CHANGELOG.md:464,513); README-таблицы перегнаны и датированы 2026-07-07
  через `bench:table` (README.md:523-569); MT-таблицы перегнаны post-R1/R2/R3
  (ALLOC_BENCH.md:212-265).
- **PERF-2/PERF-3 honest-reject'ы** — методологически образцовые (критерии
  зафиксированы заранее, два судьи, механизм-анализ, деревья pristine);
  PERF-3-фича `alloc-runfreelist` в `production` не течёт (neutrality gate
  переподтверждён базлайном digit-for-digit). Единственная methodological
  оговорка к ним — F2.

---

## Сводная таблица открытых вопросов perf-расследований

| Источник | Вопрос | Статус в коде на 2026-07-09 |
|---|---|---|
| PERF-4 checkpoint (2026-07-08) | decommit-churn гипотеза (#216), фикс (#217) | ОТКРЫТО, не начато; механизм найден этим ревью (F1) |
| PERF-3 (:298-357) | disposition `alloc-runfreelist` (keep vs revert) — за человеком; «PERF-3.5 true diversion» | фича off/opt-in, решение не оформлено; опция записана |
| PERF-2 / X5 (IAI_BASELINE:285-303) | ≥64-сегментный бенч как триггер пересмотра X5 | не создан (F4) |
| PERF_PLAN P4(b) | `alloc_zeroed` virgin-skip | не реализовано и не отклонено (F6) |
| FAULT_PROBE (:73-79) | fault-колонка на Linux CI | не реализовано (F10) |
| PERF-3 verdict (:373-377) | «gap структурен в refill/flush-оркестровке; смотреть туда в PERF-4» | не начато; пересекается с F1/F4 |

## Итоговый вердикт

Горячий путь спроектирован и вычищен на уровне лучших практик: per-op цена
single-thread magazine-пары — считанные десятки инструкций без единого
атомарного RMW, и все «признанные цены» (M2-bitmap, hardened-деление,
Instant-tick large-cache) либо осознанно оплачены и опубликованы, либо
изолированы фичами. Документация измерений — редкой честности; wall-clock и
Ir-судья дополняют друг друга, а ошибки прошлого (µs/batch vs ns/op)
институционально закрыты `bench:table`.

Требуют действия, в порядке приоритета: **(1)** F1 — запустить #216 с новым
segment-cycle бенчем и закрыть decommit-churn (мёртвая работа перед release +
отсутствие hysteresis — прямые кандидаты для #217); **(2)** F2 — перевести
пороги судьи на маржинальные Ir/op, чтобы следующий эксперимент не наследовал
разно-жёсткие проценты; **(3)** F3 — привести `Vec_push` в соответствие с его
заявлением; **(4)** F4 — добавить ≥64-сегментный бенч до любых решений об
O(n)-скане refill-пути. Остальное — гигиена (F5–F10), не влияющая на
сегодняшние production-числа.
