# Read-only review новых волн: Round 23

**Дата:** 2026-07-27  
**Диапазон:** `bc4aacf..6e0dbad`  
**HEAD:** `6e0dbad3a256ecf8586acec68124d9a474551e33`  
**Режим:** только чтение истории Git, diff и файлов. Сборка, тесты,
бенчмарки и скрипты не запускались. Единственная запись в рабочее дерево —
этот отчёт. Существующий untracked-каталог `.claude/` не затрагивался.

## Краткий вердикт

**Нет, Round 23 сам по себе production-код заметно не ускорил.** Это прежде
всего волна исправления методологии, диагностических hooks, CI/cfg-долга и
документации. В `Cargo.toml` не менялся production feature bundle, а изменения
в `src/` состоят из:

- измерительных `dbg_*` hooks;
- счётчика шагов `hash_remove`, активного на операции только под
  `alloc-stats`;
- уточнения `#[cfg]` и `allow` для ранее некомпилируемых feature-комбинаций.

Это полезная волна: она исправила неверную сравнительную картину. После
алгебраического удаления разных bootstrap-констант горячий 16-byte churn
SeferAlloc имеет **69 Ir на alloc+free pair против 77 Ir у mimalloc**, то есть
примерно на **10,4% меньше инструкций**, а не в 1,326 раза больше. На холодном
полном `alloc N → free N` раунде SeferAlloc всё ещё примерно в **2,00–2,08
раза дороже**: 203,86/196,32 против 101,81/94,27 Ir на дополнительную пару.

Но главный headline R23-3 — «own-thread body = 80,8% hot free» — **нельзя
использовать как атрибуцию обычного hot churn**. Его workload освобождает 64
разных блока подряд. При `TCACHE_CAP = 16` он многократно попадает в magazine
overflow и включает:

1. восемь отдельных `segment_base_of_ptr + clear_magazine`;
2. `flush_class` на восемь блоков;
3. копирование оставшихся восьми указателей вниз;
4. только затем обычный push.

Следовательно, 74,70 Ir/free — это стоимость **batch-free тела с
амортизированными half-flush/compaction**, а не только «M2 oracles +
непереполняющий push». Самый перспективный следующий объект исследования —
именно overflow/flush, а не немедленная переделка safety-oracles.

## Что вошло в волну

Диапазон содержит 10 коммитов:

| Коммит | Суть | Влияние на production speed |
|---|---|---|
| `897aa26` | актуализация perf open-items | нет |
| `6b4ac50` | doc/test follow-ups предыдущего review | нет |
| `7f2a9ef` | исправление изоляции `contains_base` | только измерение |
| `3cf2d66` | корректный warm N/2N mimalloc gate | только измерение |
| `315aa8a` | атрибуция hot paths и новые hooks | только измерение/API surface |
| `de7213d` | исправление remap-вердикта | design-only |
| `4a4500a` | cfg/clippy fixes | практически нет |
| `37393fe` | детерминированный scan-step oracle | только `alloc-stats` |
| `eb6c392` | решение по batch API без потребителя | нет |
| `6e0dbad` | changelog/checkpoints/reviews | нет |

Суммарный diff велик — 8 687 добавленных строк в 46 файлах, — но львиная
доля приходится на отчёты, raw logs и bench arms. Runtime-оптимизации,
меняющей алгоритм production allocation/deallocation, в диапазоне нет.

## Findings

### P0 — R23-3 смешивает обычный free и magazine overflow

`dealloc_free_only_16b` и `dealloc_own_thread_body_only_16b` сначала
аллоцируют массив из `CHURN_OPS = 64` разных указателей, затем освобождают их
одним последовательным проходом
(`benches/perf_gate_iai.rs:275-294,587-613`). Это не форма
`small_churn_16b`, где один блок немедленно освобождается после allocation.

Production free-path при `cnt < TCACHE_CAP` делает bitmap mark, запись slot и
увеличение count (`src/registry/heap_core_free.rs:712-742`). Но каждые
несколько frees batch-shaped workload достигает `cnt == TCACHE_CAP` и
выполняет существенно более дорогую ветку
(`src/registry/heap_core_free.rs:744-802`):

- цикл по `slots[0..FLUSH_N]`;
- для каждого pointer заново вычисляет base/off и очищает magazine bitmap;
- передаёт восемь блоков в substrate через `flush_class`;
- сдвигает восемь оставшихся указателей;
- обновляет mask и добавляет текущий pointer.

Отчёт называет весь результат «M2 oracle checks + magazine push (fused)» и
делает его рекомендацией следующей remediation
(`docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md:27-49,272-313,361-389`).
Это неполное имя механизма: измерены также overflow, batch flush и
compaction.

Есть сильная внутренняя проверка несопоставимости workload:

- R23-2: весь steady hot `alloc + free` = **69,0 Ir/pair**;
- R23-3: isolated magazine allocation hit = **22,38 Ir/alloc**;
- R23-3: заявленный real free = **92,50 Ir/free**.

`22,38 + 92,50 = 114,88`, что больше 69,0. Это не ошибка счётчиков само по
себе: числа получены на разных состояниях магазина. Но фраза отчёта, что
оставшиеся примерно две трети hot-pair «consistent with the free-path table»,
не выдерживает эту cross-check. Таблица описывает batch-free, а не free-half
горячей пары.

**Что исправить в документации:** переименовать headline в
«64-block batch-free own-thread body including amortized magazine overflow»
и не заявлять 80,8% долей обычного hot free.

**Что измерить до изменения кода:**

1. non-overflow push при заранее контролируемом `count < TCACHE_CAP`;
2. ровно один overflow event при заранее заполненном магазине;
3. отдельную стоимость `clear bitmap batch`, `flush_class` и compaction;
4. полные free-only серии размеров 1, 8, 16, 17, 32 и 64;
5. сравнить их с настоящим interleaved `small_churn_16b`.

### P1 — самый сильный новый кандидат: удешевить overflow/flush

После правильного split наиболее вероятное сильное локальное ускорение:

1. **Группировать bitmap clear.** Сейчас для каждого из восьми pointers
   отдельно выполняются base derivation, metadata lookup и bitmap RMW.
   Если flushed slots принадлежат одному segment/bitmap word, собрать word
   mask и очистить несколько bits одним RMW. Для нескольких segments —
   сначала дешёвая группировка по base/word.
2. **Переиспользовать batch-deallocation substrate.** В проекте уже есть
   batch-механизм, который создавался именно для отказа от scalar
   `FLUSH_N`-dribble. Его внутренний алгоритм можно применить к magazine
   overflow без публикации нового API.
3. **Убрать pointer compaction.** Circular head/ring или переключение двух
   половин fixed array позволит не копировать `TCACHE_CAP - FLUSH_N`
   указателей при каждом overflow. Это требует аккуратной адаптации
   `virgin_mask` и всех diagnostic readers, но не меняет ownership model.
4. **Только после этого повторить cap/flush sweep.** Старый sweep размеров
   магазина измерял другую cost model. Поднимать `TCACHE_CAP` до исправления
   overflow преждевременно: это увеличивает `HeapCore`, RSS и объём bitmap
   работы.

Потенциал здесь не «1000× на весь allocator», но на workloads вида
`allocate-many → free-many` он может убрать заметную долю текущего примерно
двукратного отставания от mimalloc.

### P1 — cold headline всё ещё не локализует причину 2×

`cold_alloc_free_256x16b` — не «cold carve». Это полный раунд:

```text
allocate 256 distinct blocks
free the same 256 blocks
```

Его marginal 203,86/196,32 Ir включает refill, magazine, BinTable и
многократные free overflows. Сам R23-3 показывает, что bare `carve_batch`
стоит лишь 23,05 Ir/op. Следовательно, нельзя приписывать двукратное
отставание собственно carve.

Аналогично, разность второго и первого round в
`recycle_alloc_free_256x16b` действительно изолирует второй **полный
alloc+free round**, но не чистый `freelist_pop`. Название 188,20 Ir/op как
«freelist-pop» вводит в заблуждение; в собственном §6.4 отчёт уже признаёт,
что это full production pair.

**Следующий корректный split:**

- cold alloc-only с одинаковым setup;
- cold free-only;
- refill из virgin carve;
- refill из BinTable;
- magazine overflow отдельно.

Только после него выбирать между carve/refill, BinTable и free overflow.
Текущие данные скорее указывают, что bare bump-carve не является главным
врагом.

### P1 — Linux sub-region `mremap`: единственный оставшийся асимптотический lever

Исправление R23-4 по коду обоснованно: medium block начинается и заканчивается
на page boundary, а monotonic bump не размещает соседа внутри его живого
диапазона. Поэтому старый neighbor-liveness blocker не существует.

Whole-segment remap остаётся NO-GO из-за стабильности segment base. Windows
остаётся NO-GO из-за несовместимой anonymous-`VirtualAlloc` backing model.
Но Linux-only sub-region `mremap` можно прототипировать:

- remap ровно page-aligned medium span;
- зарегистрировать destination как самостоятельный Large/extent;
- не проводить старый range через обычный `dealloc`, иначе он попадёт в
  BinTable и будет повторно выдан после физического переноса страниц;
- учитывать vacated range как retired до teardown всего segment;
- при любой ошибке использовать нынешний memcpy fallback.

Это потенциально заменяет `O(n)` copy на работу с page tables и потому
является действительно радикальным ускорением realloc больших medium
objects.

Но механизм имеет смысл только при наличии реального Linux-потребителя:
`medium-classes` не входит в production и ранее показывал тяжёлую realloc
регрессию. Сначала нужен victim/workload и Stage-1 счётчик доли realloc,
которые реально пересекают promotion threshold. Без потребителя это дорогая
correctness-sensitive архитектурная работа ради feature-gated сценария.

### P2 — hot churn уже близок к локальному максимуму

Корректный R23-2 результат важен: на счётчике инструкций SeferAlloc уже
обходит mimalloc в повторном 16-byte alloc/free churn примерно на 10,4%.
Поэтому дальнейшая сложная переделка routing или `contains_base` ради этого
workload имеет низкий ожидаемый ROI.

`contains_base` скорректирован с 18,6% до 8,8%. При этом composite и base-only
arms имеют разное число wrapper calls, поэтому остаточный call-boundary
эффект всё ещё нельзя строго отделить от тела `contains_base`. Аналогично
`segment_base_of_ptr`, являющийся простым pointer mask, в hook-изоляции
получил 9,8%; это повод подозревать измерительную обвязку, а не оптимизировать
сам mask.

Рекомендация: не менять ownership/liveness ordering ради single-digit
процентов без wall-clock victim. Сохранять safety-first модель и направить
работу на batch/cold/cross-thread сценарии.

### P2 — measurement hooks увеличивают production API/unsafe surface

`dbg_dealloc_own_thread_with_base` — `pub unsafe fn`, gated только
production-фичами `alloc-global + fastbin`
(`src/registry/heap_core_diag.rs:448-461`). То есть measurement-only unsafe
entry point компилируется и экспортируется в обычной production
конфигурации. `dbg_hash_contains_only` также живёт в library surface.

Это не добавляет исполнения в hot path, но формулировка «zero production
change» слишком сильна: меняются symbol/code size, API и unsafe audit
surface.

Рекомендация проекта: ввести отдельную feature вроде `bench-internals` или
`diagnostic-api`, включать её в required-features benchmark targets и не
включать в production. Все `dbg_*`, которые нужны только integration
bench/test, постепенно собрать за этим gate. При сравнительных замерах
следить, чтобы эта feature не меняла измеряемый production algorithm.

### P2 — детерминированный `hash_remove` oracle полезен, но его bound не «exact»

Замена wall-clock flake счётчиком шагов — правильное улучшение. Локальный
counter и один `fetch_max` появляются только под `alloc-stats`, поэтому
обычный production-path не дорожает.

Однако assertion `max_steps <= 4 * W` при `W = 600` — детерминированная
граница конкретной тестовой волны, а не математически точное доказательство
`O(cluster)` для любой конфигурации open-addressing table. Она отлично ловит
возврат к полному проходу `HASH_CAPACITY = 8192`, но может пропустить
патологический cluster в несколько тысяч probes.

Нужно назвать её «deterministic regression threshold for this wave», а не
«exact bound». Если проект хочет доказать алгоритмический bound, нужен
отдельный property/model test распределений и backshift invariants.

### P2 — correctness open item остаётся открытым

`docs/CORRECTNESS_OPEN_ITEMS.md:72-94` теперь честно отслеживает, что
`canary_survives_promotion_and_free_leaves_no_leak` доказывает отсутствие
double-release, но не отсутствие leak:

```text
released_delta <= reserved_delta
```

пропускает `reserved_delta = 1, released_delta = 0`.

Это не новая runtime-регрессия Round 23, но важный незакрытый долг. Нужен
per-allocation/per-base oracle: после promoted free конкретный old/new base
должен находиться в допустимом cache/decommit/released состоянии и не
оставаться навсегда зарегистрированным как live.

### P3 — append-only документация уже мешает видеть актуальную истину

`docs/perf/OPEN_ITEMS.md` начинает active item заголовком «contains_base
18.6%», затем через десятки строк исправляет его до 8,8%, затем дописывает
R23-3. Закрытый пункт остаётся в Active; R23-4 одновременно содержит старый
pending verdict и последующий DONE update. Пользователь, читающий начало
пункта, получает устаревшую картину.

Round 23 добавляет тысячи строк при нулевом runtime speedup. Полное сохранение
истории полезно для аудита, но append-only corrections не должны быть
основным интерфейсом текущего состояния.

Рекомендация:

- в `OPEN_ITEMS.md` оставлять короткую текущую карточку: status, current
  number/verdict, next trigger, ссылка на evidence;
- исторический narrative переносить в `Recently resolved` или отдельный
  immutable report;
- закрытые items физически убирать из `[A] Active`;
- начинать каждый corrected report с маленького current-verdict box, а не
  заставлять читать позднюю correction section;
- в changelog отделять «runtime improvements» от «measurement/correctness/
  tooling», чтобы размер волны не воспринимался как размер ускорения.

## Что действительно улучшилось

Несмотря на отсутствие нового production speedup, качество проекта стало
выше:

1. Исправлено некорректное сравнение с mimalloc.
2. Подтверждено, что steady 16-byte churn уже конкурентоспособен.
3. Исправлен завышенный вдвое headline `contains_base`.
4. Linux remap получил более точный, узкий CONDITIONAL-GO.
5. Закрыты clippy cfg gaps для `hardened medium-classes`, и эта комбинация
   добавлена в CI.
6. Flaky latency gate заменён детерминированной диагностикой.
7. Batch API разумно не публикуется без downstream consumer.

Это не ускорение кода, а ускорение принятия следующих решений и снижение
риска оптимизировать неверный механизм.

## Приоритет следующего этапа

### R24-1 — исправить интерпретацию R23-3

- переименовать batch-free numbers;
- убрать утверждение, что 80,8% относится к обычному hot free;
- занести correction в current summary/open-items.

### R24-2 — разложить free по состояниям магазина

Сделать shared-prefix judges для:

- non-overflow push;
- single overflow;
- batch free N = 1/8/16/17/32/64;
- bitmap clear, substrate flush и compaction;
- interleaved churn.

Это обязательный gate перед remediation.

### R24-3 — прототипировать overflow batching

Если R24-2 подтвердит доминирование overflow:

- grouped bitmap clears;
- reuse внутреннего batch flush;
- circular/no-copy magazine;
- A/B по Ir, wall-clock и размеру `HeapCore`;
- counterfactual tests для M2, `virgin_mask`, decommit/live_count.

### R24-4 — разделить cold alloc и cold free

Повторить N/2N/4N отдельно для allocation и deallocation. Сравнивать с
mimalloc только одинаковые фазы и одинаковые fixed prefixes. Не называть
полный round «carve» или «freelist pop».

### R24-5 — Linux remap только после consumer gate

Сначала измерить долю medium realloc, которая пересекает promotion threshold,
и реальный объём копируемых bytes. При существенном victim — correctness
prototype retired-range + `mremap`; иначе оставить deferred design.

### R24-6 — улучшить диагностическую архитектуру

- отдельная `bench-internals` feature;
- убрать measurement-only unsafe methods из production surface;
- усилить promoted-free leak oracle;
- привести perf/correctness open-items к current-state-first формату.

## Ответы на вопросы

**Мы действительно ускорили код этой волной?**  
Нет. По просмотренному diff Round 23 не содержит production runtime
оптимизации. Он исправляет измерения и инфраструктуру. Благоприятное число
69 против 77 Ir описывает уже существовавший код, который раньше сравнили
неправильно.

**Есть ли ещё место для сильного ускорения?**  
Да, но не доказано в headline R23-3. Самый конкретный кандидат — magazine
overflow/flush на batch-free workloads и связанная с ним cold-free половина.
Самый радикальный асимптотический кандидат — Linux-only sub-region `mremap`
для востребованного medium→Large realloc. Обычный steady hot churn уже
выглядит близким к локальному максимуму.

**Что улучшить в коде?**  
Сначала разделить non-overflow и overflow cost; затем группировать bitmap
operations, переиспользовать batch flush и убрать compaction. Measurement
hooks изолировать отдельной feature. Не ослаблять safety-oracles на основании
нынешних 80,8%.

**Что улучшить в проекте?**  
Сделать reports current-state-first, отделить measurement wins от runtime
wins, закрытые пункты убирать из Active, а performance claims проверять
арифметическими cross-checks между workload shapes до переноса в headline.

---

## Дополнение: как именно ускорять код

**Дата дополнения:** 2026-07-27  
**Режим исследования:** только чтение текущих реализаций. Никаких новых
замеров не выполнялось, поэтому ниже явно разделены доказанные лишние
операции и гипотезы об их wall-clock/Ir эффекте.

### 1. Наиболее практичный первый прототип: magazine-aware `flush_class`

Сейчас scalar magazine overflow проходит одни и те же восемь pointers дважды:

```text
heap_core_free:
    for 8 blocks:
        derive base/off
        construct SegmentMeta
        clear magazine bit

    flush_class(8 blocks):
        derive base снова
        найти same-segment runs
        construct SegmentMeta снова на run
        для каждого block:
            derive off снова
            проверить alloc bitmap
            записать intrusive next
            mark alloc bitmap free
```

Первый проход виден в `src/registry/heap_core_free.rs:762-768`, второй —
в `src/alloc_core/alloc_core_small_magazine.rs:532-568,586-665`.
Аналогичный отдельный clear-pass есть в teardown
`src/registry/heap_core_tcache.rs:78-103`.

Нужен отдельный внутренний primitive, например:

```text
flush_magazine_class(class, blocks)
```

Он должен группировать same-segment runs как нынешний `flush_class`, но внутри
каждого run:

1. получить `SegmentMeta` один раз;
2. очистить magazine-residency bits;
3. выполнить нынешний alloc-bitmap/BinTable flush;
4. только после всех metadata touches разрешить decommit/release.

Обычный `flush_class`, используемый `dealloc_batch` для блоков, которые не
попадали в magazine, должен сохраниться или стать const-generic/internal-mode
вариантом без magazine clear.

Это не меняет policy и не ослабляет M2: bit по-прежнему очищается до
возможного освобождения segment. Оно лишь сливает два прохода и убирает
повторные base/off/meta вычисления.

**Ожидаемый профиль эффекта:**

- steady interleaved churn: почти ноль, overflow там не срабатывает;
- `allocate-many → free-many`: средний/высокий эффект на free half;
- teardown/trim: умеренный эффект;
- RSS и размер `HeapCore`: без изменений.

Это самый безопасный первый patch: алгоритм и порядок жизненных переходов
остаются прежними.

### 2. Следующий слой: bulk bitmap masks вместо N byte-RMW

`SegmentBitmap::set/clear` сейчас на каждый offset отдельно делает:

```text
locate → byte load → OR/AND → byte store
```

(`src/alloc_core/segment_bitmap.rs:89-116`). Такой же per-block loop уже
отмечен в коде `alloc_batch` как natural follow-up
(`src/registry/heap_core_alloc.rs:1019-1028`), но primitive ещё не создан.

Для `FLUSH_N = 8` соседних блоков размером 16 bytes все восемь residency bits
лежат **в одном bitmap byte**:

```text
8 blocks × 1 bit = 1 byte
```

То есть common contiguous run можно перевести с восьми
load-modify-store циклов на один:

```text
combined_mask = bit(off0) | ... | bit(off7)
bitmap_byte &= !combined_mask
```

Для 64-byte blocks восемь blocks занимают четыре bitmap bytes, поэтому
теоретическое сокращение — с 8 RMW до 4. Для 256-byte и более крупных
классов blocks обычно попадают в разные bytes, и выигрыша от слияния почти
нет. Значит gate должен быть size-aware.

Рекомендуемый API не должен отдавать наружу сырой bitmap pointer. Лучше
оставить arithmetic/raw-memory seam внутри `SegmentBitmap`:

```text
clear_many(offsets)
set_many(offsets)
```

с fast path, который накапливает mask, пока `byte_idx` не изменился, и затем
делает один RMW. Это не требует sorting и дёшево обрабатывает самый частый
случай — последовательные offsets одного refill/carve run. Для произвольного
порядка он просто сбрасывает accumulator и остаётся эквивалентен нынешнему
коду.

Применить primitive можно в трёх местах:

1. scalar overflow/`flush_magazine_class`;
2. `flush_all_tcache`;
3. deferred clear в `alloc_batch`.

После residency bitmap тем же способом можно исследовать batched
`AllocBitmap::mark_free` внутри `flush_run`. Здесь требуется локальный shadow
текущего byte, чтобы M2 duplicate test видел изменения предыдущих элементов
run до финального store. Это сложнее, поэтому residency clear — первая
стадия, alloc bitmap — отдельная вторая.

### 3. Не начинать с circular magazine

У нынешней overflow-ветки есть восемь pointer assignments при compaction
(`heap_core_free.rs:779-783`). Их можно убрать ring/head-index организацией,
но цена появится на **каждом** обычном magazine hit/push:

- дополнительный head/start field в `PerClass`;
- add/mask при каждом slot access;
- более сложная синхронизация `virgin_mask`;
- ухудшение самой сильной сегодня метрики — steady hot churn.

Более дешёвый эксперимент — flush верхней половины, оставляя нижнюю на месте,
но он выбрасывает самые недавно освобождённые, то есть ухудшает temporal
locality. Full flush также убирает compaction, но оставляет после overflow
один тёплый block вместо девяти.

Поэтому порядок должен быть таким:

1. сначала измерить долю compaction отдельно;
2. сначала слить clear+flush passes и bitmap RMW;
3. менять представление magazine только если восемь pointer copies всё ещё
   материальны.

На современных оптимизаторах fixed loop из восьми assignments может быть
полностью unrolled; без assembly/Ir gate архитектурная переделка здесь
неоправданна.

### 4. Более радикальный hot-path вариант: единый 2-bit block-state bitmap

Текущие два bitmaps кодируют три реальных состояния:

| `AllocBitmap` | `MagazineBitmap` | Состояние |
|---:|---:|---|
| 0 | 0 | выдан пользователю / ещё не carved |
| 1 | 0 | находится в BinTable free list |
| 0 | 1 | находится в magazine |
| 1 | 1 | недопустимое |

На обычном own-thread free код:

1. читает magazine bitmap;
2. читает alloc bitmap;
3. записывает magazine bitmap.

Теоретически их можно заменить одним bitmap с 2 bits на block:

```text
USER = 00
FREE_LIST = 01
MAGAZINE = 10
INVALID = 11
```

Тогда free делает один state load и один transition `USER → MAGAZINE`;
magazine pop — `MAGAZINE → USER`; flush — `MAGAZINE → FREE_LIST`; freelist
pop — `FREE_LIST → USER`.

Footprint остаётся тем же: два нынешних 32 KiB bitmaps превращаются в один
64 KiB state bitmap. Возможный выигрыш — одна metadata address stream и один
RMW вместо двух probes плюс RMW.

Но это **не следующий patch**, а design/prototype:

- старый эксперимент G1 уже показал, что простая смена смысла
  `AllocBitmap` ломает несколько переходов;
- нужно инвентаризировать все `mark_free`, `mark_alloc`,
  `mark_magazine`, `clear_magazine`, reset и ring-drain sites;
- `00` всё ещё объединяет live и never-carved, поэтому stale/bump guards
  остаются обязательными;
- ошибки state transition напрямую превращаются в double issue или leak.

Запускать такой проект стоит только если чистый non-overflow free после
правильного R24-2 split действительно останется большим wall-clock victim.
Потенциал выше, чем у микрооптимизации `contains_base`, но и correctness-risk
намного выше.

### 5. Улучшить уже существующий `dealloc_batch`

Код batch API уже реализует правильную крупную идею: после заполнения
magazine весь overflow staging отправляется в один `flush_class`, а не
серией half-flush по восемь блоков
(`src/registry/heap_core_dealloc_batch.rs:293-343`).

До публичного consumer у реализации есть четыре конкретных улучшения.

#### 5.1 Кэшировать ownership result для одинакового base

Сейчас каждый block выполняет `contains_base(base)`
(`heap_core_dealloc_batch.rs:215-237`). В allocation-order batch десятки или
сотни соседних blocks часто принадлежат одному segment.

Локальные:

```text
last_base
last_is_owned
```

позволят выполнять ownership lookup только при смене base. Для foreign/mixed
batch semantics сохраняются: при новом base выполняется настоящий lookup,
не-owned elements идут в scalar routing.

Это применимо только внутри batch API; переносить такой cache между
независимыми scalar calls нельзя без invalidation при recycle.

#### 5.2 Проверить цену 4 KiB staging initialization

`STAGE_CAP = 512`, поэтому локальный
`[*mut u8; 512] = [null; 512]` логически инициализирует 4 KiB stack storage
при каждом вызове, даже если batch мал
(`heap_core_dealloc_batch.rs:201-213`). Оптимизатор может удалить ненужное
обнуление, поскольку читается только уже записанный prefix, но это нельзя
считать доказанным по source.

Нужен assembly/Ir gate. Если memset остаётся:

- использовать меньший stage (например, 64/128) и измерить лишние flush
  calls;
- либо аккуратный `MaybeUninit`-buffer с очень узким unsafe proof;
- либо специализировать small-batch stack capacity и отдельный large path.

Не добавлять `MaybeUninit` до доказательства, что memset реально существует:
это correctness complexity ради потенциально уже устранённой работы.

#### 5.3 Исправить противоречие «first vs last warm»

Документация обещает, что **последние** `TCACHE_CAP` элементов batch остаются
в magazine (`heap_core_dealloc_batch.rs:117-135`). Реализация идёт от начала
slice, заполняет magazine первыми принятыми blocks, а последующие отправляет
в stage (`:215,293-331`). То есть при пустом magazine тёплыми остаются
**первые**, а не последние 16.

Нужно принять явное policy-решение:

- либо исправить документацию на «first accepted blocks»;
- либо изменить алгоритм так, чтобы сохранять последние frees, что лучше
  соответствует temporal locality и scalar overflow policy.

Для реализации «last warm» не обязательно делать два полных прохода:
фиксированный rolling buffer на `TCACHE_CAP` может вытеснять старейшие
элементы в staging, оставляя последние. Но это добавляет slot traffic; нужен
reallocate-after-batch judge, а не только free throughput.

#### 5.4 Не публиковать API без потребителя

R23-7 правильно оставил batch API экспериментальным: обычные `Box`/`Vec`
drop paths не вызывают allocator-specific batch free. Внутренние улучшения
имеют смысл как подготовка, но product-facing работу нужно начинать с
downstream adapter/consumer, иначе выигрыш остаётся недостижимым для
пользователя.

### 6. Что делать с холодным отставанием примерно 2×

Текущий cold judge смешивает:

- allocation refill;
- freelist drain или virgin carve;
- owner stamping;
- 256 scalar frees;
- magazine overflow и `flush_class`;
- возможные segment transitions.

Bare `carve_batch = 23,05 Ir/op` уже показывает, что сам bump arithmetic
слишком мал, чтобы объяснить полный результат 196–204 Ir/pair.

Следующая волна должна сначала получить четыре числа:

| Judge | Что изолирует |
|---|---|
| cold alloc-only N/2N/4N | refill + carve/freelist + issue |
| cold free-only N/2N/4N | routing + push + overflow/flush |
| one virgin refill | `refill_class_bump_checked` на пустом substrate |
| one recycled refill | `drain_freelist_batch` через тот же HeapCore face |

После описанных выше flush оптимизаций повторить сравнение с mimalloc.
Возможны три исхода:

1. основная половина gap исчезает — значит виноват scalar overflow;
2. остаётся на alloc-only — исследовать refill stamping/bitmap transitions;
3. обе половины близки к mimalloc, но full round нет — искать setup/segment
   transition, а не ещё один hot micro-tweak.

### 7. Если после flush остаётся дорогим refill

`refill_magazine_slow` после batch refill:

- проходит возвращённые blocks для owner stamping с дедупликацией по
  соседнему base;
- затем проходит `n - 1` blocks ещё раз для `mark_magazine`
  (`src/registry/heap_core_alloc.rs:718-748`).

Это два близких post-refill loops. Их можно слить:

```text
for each retained block:
    if base changed: stamp owner
    accumulate/set magazine bit
handle issued block's stamp separately
```

Но это корректно только при сохранении нынешней границы:

- `issued = slots[n-1]` не должен быть marked magazine;
- все retained blocks должны быть marked до возврата;
- hardened generation bump применяется только в момент issue;
- cross-thread stale-entry predicate должен видеть claimed blocks в нужное
  окно.

Лучше всего соединить это с `mark_many`: один loop и один bitmap RMW на
соседний byte. Это потенциально ускорит cold alloc/refill, не затрагивая
magazine-hit path.

### 8. Какие идеи пока не брать

1. **Убирать M2 checks.** Идентичность проекта safety-first, а текущие 80,8%
   не изолируют checks от overflow. Сначала правильный split.
2. **Header-first ownership без liveness proof.** Ради скорректированных 8,8%
   нельзя вводить потенциальный read unmapped/foreign metadata.
3. **Увеличивать `TCACHE_CAP`.** Это расширяет каждый `PerClass`, `HeapCore`
   и RSS; старые sweeps уже показывали регрессии. Сначала удешевить overflow.
4. **Page-run layer без victim.** Дизайн готов, но production workload,
   ограниченный `MAX_SEGMENTS` или reservation syscalls на 1,25–2 MiB,
   по-прежнему не показан.
5. **Linux `mremap` без medium-realloc consumer.** Это радикальный lever, но
   correctness/FFI работа оправданна только измеренным количеством больших
   copy bytes.
6. **Публичный batch API «на всякий случай».** Без адаптера реальный Box/Vec
   код его не использует.

## Конкретная рекомендуемая очередь реализации

| Порядок | Работа | Риск | Ожидаемый охват |
|---:|---|---|---|
| 1 | Исправить R23-3 interpretation и сделать state-shaped judges | низкий | устраняет выбор неверной цели |
| 2 | `flush_magazine_class`: слить clear-pass с `flush_class` | низкий/средний | batch-free, teardown |
| 3 | `MagazineBitmap::clear_many/mark_many` | средний | 16/64-byte batch free/refill |
| 4 | Cold alloc-only/free-only split | низкий | локализует оставшийся ~2× gap |
| 5 | Слить stamping + mark-magazine refill loops | средний | cold alloc/refill |
| 6 | Batch `last_base` ownership cache | низкий | batch API, same-segment batches |
| 7 | Решить first-vs-last-warm policy | средний | post-batch reuse locality |
| 8 | 2-bit state bitmap prototype, только если non-overflow victim доказан | высокий | scalar free/all bitmap transitions |
| 9 | Linux `mremap`, только после consumer/copy-bytes gate | очень высокий | asymptotic medium realloc |

### Gates для каждого performance patch

Каждый этап должен иметь:

1. Ir judge отдельно для 16/64/256 bytes;
2. wall-clock A/B/B/A для batch sizes 1/8/16/17/32/64/256;
3. сценарий `free batch → immediately realloc same class`, чтобы выигрыш
   free throughput не покупался потерей cache warmth;
4. `HeapCore` size/RSS gate;
5. counterfactual tests M2, stale free, cross-thread ring, `virgin_mask`,
   live_count/decommit/recycle;
6. mutation test, доказывающий, что новый judge краснеет при возврате
   per-block pass/RMW;
7. отдельный production-feature run без `alloc-stats` и measurement hooks.

### Реалистичная оценка потолка

Из опубликованных чисел можно вывести только гипотезу, не обещание:

- batch-shaped free R23-3: 92,50 Ir/free;
- hot pair R23-2: 69,0 Ir/pair;
- isolated hot allocation hit: 22,38 Ir/alloc;
- следовательно, неявный остаток hot free/refill =
  `69,0 - 22,38 ≈ 46,62 Ir`.

Batch-shaped free почти вдвое дороже этого остатка:

```text
92,50 / 46,62 ≈ 1,98
```

Workloads и состояния не идентичны, поэтому это **не доказательство
двукратного ускорения**. Но разница примерно 45,9 Ir/free — сильный сигнал,
что overflow/flush state, отсутствующий в interleaved churn, способен
объяснить большую часть batch-free overhead.

Разумная цель следующей волны:

- сохранить нынешние 69 Ir hot pair без регрессии;
- заметно приблизить full cold pair 196–204 Ir к mimalloc 94–102 Ir прежде
  всего через free/flush половину;
- считать радикальным успехом не процент отдельного hook, а сокращение
  end-to-end `allocate-many → free-many` при неизменных safety/RSS свойствах.
