# Повторное performance-review (round 5)

Дата статического ревью: 2026-07-14.

## 1. Объём и методика

Это повторное **read-only** ревью текущего дерева после исправлений, описанных в
`docs/agent_reviews_round4/performance_review.md`. Исходники, тесты, benchmark
harnesses, скрипты, Cargo-профили, feature flags и сохранённые отчёты были
перепроверены по текущим файлам. Ничего не запускалось: ни Git, ни компиляция,
ни тесты, ни benchmark, ни profiler, ни скрипты, ни fuzzing. Поэтому ниже
отдельно разделены:

- факты, непосредственно доказуемые кодом и сохранёнными результатами;
- причинные гипотезы, которые требуют новых измерений.

Содержимое `target/criterion` не использовано как текущий baseline: каталог
`target` игнорируется (`.gitignore:1`), найденные локальные данные имеют старые
имена benchmark (`SeferMalloc*`) и не привязаны к текущему исходному дереву,
toolchain и feature set. Версионированные таблицы в `docs/` пригодны как
история, но не как парное измерение текущего бинарника.

## 2. Итог

Я не вижу статического свидетельства, что последние волны изменений должны
были ускорить те три серии, которые выводит `scripts/bench-table.mjs`.
Большинство изменений улучшает lifecycle/correctness, RSS либо редкие
cross-thread/tail-сценарии; часть performance-идей была отклонена и оставлена
в feature-off состоянии. Основной выводимый benchmark, напротив, измеряет
прогретый single-thread bulk/churn с минимальной выборкой.

Главные причины отсутствия видимого выигрыша:

1. Изменённые пути почти не попадают в каноническую таблицу.
2. `global_alloc` сохраняет один TLS heap и его историю между группами, а
   сценарий, подписанный как cold, после warmup уже не cold.
3. Скрипт таблицы берёт только point estimate, игнорирует Criterion `change:` и
   не печатает четыре из семи запускаемых групп.
4. `sample_size(10)` и короткие окна недостаточны для уверенного сравнения
   небольших изменений на шумном Windows-хосте; сохранённые результаты сами
   декларируют вариативность ±15–20%.
5. Hot path по-прежнему содержит bitmap/hash/cache metadata work, а workload
   `Vec::push` и bootstrap IAI сильно разбавляют его стоимость.
6. Специализированный `heap_xthread` harness некорректен: 64-байтные блоки
   публикуются как class 0 (16 байт), а один сценарий повторно публикует уже
   освобождённые указатели. Его числа нельзя считать оценкой production
   cross-thread free.

Следовательно, наблюдаемая стагнация не доказывает, что исправления бесполезны;
но и сохранённые данные не доказывают ускорение. Для нескольких текущих чисел
есть сигнал возможной просадки, однако существующая методика не позволяет
отделить регрессию codegen/hot path от межзапускового шума.

## 3. Что изменилось после round 4 и чего ожидать от benchmark

| Изменение | Текущее состояние | Где проявляется | Ожидаемый эффект в канонической таблице |
|---|---|---|---|
| N1: очистка heap перед recycle | Реализовано: `src/global/tls_heap.rs:239-262`, `src/registry/heap_core.rs:1833-1910` | Завершение потока/recycle, освобождение tcache, small pool и large cache | Почти нулевой: обычные итерации `global_alloc` поток не завершают |
| N2: конфликт конфигурации recycled slot | Реализована проверка/диагностика, но не reconfiguration: `src/registry/heap_registry.rs:169-254`, `src/global/sefer_alloc.rs:242-254` | Повторное получение слота с несовместимой конфигурацией | Не является throughput-оптимизацией |
| N3: tombstone rebuild | Заменён backward-shift delete: `src/alloc_core/segment_table.rs:143-149,294-303,356-369,713-826` | Удаление сегмента и длинные probe clusters | Убирает редкий rebuild/spike, но обычный churn почти не удаляет сегменты |
| R2: retry storm | Лимит уменьшен до 8192: `src/registry/heap_core.rs:204-248`, `src/registry/heap_core_xthread.rs:382-433` | Насыщенная remote ring при живом owner | Single-thread таблица путь не вызывает |
| R1: поиск сегмента | Убран временный precollect, но сохранён линейный проход: `src/alloc_core/alloc_core_small.rs:239-410` | Heap с большим числом сегментов/дыр | Текущие IAI-сценарии имеют максимум три сегмента |
| R3: magazine bitmap | Per-hit update сохранён: `src/registry/heap_core.rs:821-905,1216-1259,1582-1650` | Каждый hot alloc/free через magazine | Это всё ещё стоимость production hot path |

N1 может улучшать RSS и удержание памяти, N3 — worst-case latency, R2 —
поведение при saturation. Ни одно из этих свойств нельзя надёжно вывести из
трёх строк `bench-table`.

## 4. Аудит измерительных путей

### 4.1 `benches/global_alloc.rs`

Основные параметры: размеры 16/64/256/1024, `OPS = 1024`, working set 256
(`benches/global_alloc.rs:35-43`). Быстрые группы используют
`sample_size(10)`, warmup 150 ms и measurement 600 ms
(`benches/global_alloc.rs:233-237,390-394,470-474,520-524,624-628,731-735`).
Это минимальный smoke/perf profile, не устойчивый regression experiment.

Найденные проблемы:

1. **`global_alloc` не является cold/no-reuse.** В одной iteration сначала
   выделяются 1024 блока, затем освобождаются (`benches/global_alloc.rs:66-81`),
   но Criterion многократно выполняет closure на warmup и measurement в одном
   процессе. После первого прохода allocator уже имеет прогретые freelists,
   committed pages и metadata. Заголовок `scripts/bench-table.mjs:44-51`
   «cold / no reuse» фактически неверен.

2. **Состояние Sefer сохраняется между группами.** Новые значения
   `SeferAlloc::new()` не создают независимый heap: быстрый TLS-путь возвращает
   текущий heap (`src/global/tls_heap.rs:421-445`; путь вызова
   `src/global/sefer_alloc.rs:269-286,398-445`). Поэтому high-water segment
   table, holes, tcache, pool и large cache зависят от порядка уже выполненных
   benchmark. Сравнение не изолировано по функции/размеру.

3. **Фиксированный порядок реализаций.** Внутри размера идут Sefer, mimalloc,
   System (`benches/global_alloc.rs:233-263`). Нет рандомизации или
   межпроцессного чередования, поэтому drift частоты CPU, температуры и фоновой
   нагрузки может коррелировать с allocator.

4. **Churn setup/teardown в основном вынесены из таймера.** `iter_batched`
   создаёт prefill и guard вне timed closure
   (`benches/global_alloc.rs:83-142,390-454,520-579`). Это корректно для
   изоляции hot steady state, но исключает lifecycle, refill initialization и
   финальную очистку — как раз области последних исправлений. Отдельная группа
   `churn_with_teardown` включает teardown (`benches/global_alloc.rs:460-518`),
   но канонический скрипт её не выводит.

5. **Комментарии частично устарели.** Комментарий около
   `benches/global_alloc.rs:149-154` говорит, что teardown входит в timing и
   занижает churn; текущий guard уже выносит teardown из timed closure.

6. **Часть групп без сопоставимого baseline.** `working_set_cycle` измеряет
   только Sefer (`benches/global_alloc.rs:660-780`), а pool-cap sweep является
   диагностикой, а не сопоставимой wall-clock серией
   (`benches/global_alloc.rs:783-966`).

7. **`Vec::push` разбавляет allocator.** На 1024 push приходится лишь
   логарифмическое число realloc/growth; stores, bounds/capacity logic и копии
   элементов занимают значительную долю. Изменение нескольких инструкций
   allocator не обязано быть заметно в полном времени closure.

### 4.2 `scripts/bench-table.mjs`

Скрипт запускает весь `global_alloc` с `--features production`
(`scripts/bench-table.mjs:28-40,155-160`), но парсер и таблица теряют важную
информацию:

- берётся средний элемент тройки `time`, то есть point estimate
  (`scripts/bench-table.mjs:65-103`);
- строка Criterion `change:` намеренно игнорируется
  (`scripts/bench-table.mjs:84`);
- печатаются только `global_alloc`, `global_alloc_churn`,
  `global_alloc_churn_write` и Vec (`scripts/bench-table.mjs:117-144,174-179`);
- не печатаются `churn_with_teardown`, `segment_decommit_cycle`,
  `working_set_cycle` и pool-cap diagnostic, хотя они зарегистрированы и
  запускаются (`benches/global_alloc.rs:968-989`);
- скрипт сам предупреждает, что sample 10 шумный и для gate нужен IAI
  (`scripts/bench-table.mjs:149-151`).

Итоговая markdown-таблица не сохраняет confidence interval, outliers,
bootstrap distribution и paired baseline. По ней нельзя статистически
подтвердить изменение порядка 5–15%.

### 4.3 `benches/heap_xthread.rs`: числа сейчас недостоверны

Это наиболее серьёзный дефект harness.

- Harness выделяет блоки с layout 64 байта, но вызывает
  `dbg_push_to_ring(ptr, 0)` (`benches/heap_xthread.rs:57-77,108`).
- Контракт seam требует передать реальный class allocation
  (`src/alloc_core/alloc_core_small_reclaim.rs:324-361`).
- Class 0 соответствует 16 байтам (`src/alloc_core/size_classes.rs:62,265-269`),
  а не 64.
- В `push_drain_256` набор указателей создаётся один раз до Criterion iteration,
  затем повторно публикуется и drain-ится (`benches/heap_xthread.rs:59-85`).
  После первого прохода эти блоки уже свободны; следующие проходы в основном
  проверяют поведение на duplicate/stale free, а не поток валидных remote frees.
- Seam идёт прямо в `AllocCore::dbg_push_to_ring`, обходя production
  `HeapCore::push_with_overflow_retry`. Нет реального producer thread и нет
  contention нескольких producers.

Следовательно, этот benchmark не измеряет ни эффект уменьшения retry budget,
ни production fan-in, ни корректный цикл 64-байтных allocation/free. Любая
стабильность или просадка его результата не относится к R2.

### 4.4 Другие harnesses

- `benches/heap_async_pattern.rs:4-23,170-183` — single-thread, без runtime,
  spawn и конкурирующего allocator; один pipeline предварительно прогревается.
  Название «async» не означает измерение cross-thread handoff или thread exit.
- `benches/large_realloc.rs:181-183,221-223,261-263` использует sample 10 и
  1 s warmup/2–3 s measurement. Повторения прогревают large cache; это не
  first-touch/OS-cold сценарий. Последний round-4 документ прямо отмечает, что
  large/realloc после последних изменений не перезапускался
  (`docs/ALLOC_BENCH.md:71-84`).
- `examples/malloc_macro.rs:431-445,470-506` запускает Sefer, затем mimalloc,
  затем System в фиксированном порядке. Таймер останавливается после join, так
  что worker teardown и новый `trim_for_recycle` входят во время Sefer, хотя не
  входят в число операций. Это потенциально чувствительно к N1. Кроме того,
  worker может начать после barrier до того, как main запишет `Instant`, что
  создаёт небольшую переменную недоучтённую начальную часть.
- `crates/malloc-bench/src/lib.rs:531-563` также включает worker cleanup до join,
  но дополнительный drain в main происходит после остановки таймера. Получается
  асимметричная граница измерения.
- `benches/sharded_write.rs:50-149` и `benches/pinned_write.rs:53-118` создают
  region и выполняют spawn/join внутри timed iteration. Они измеряют смесь
  создания, scheduler/affinity и inserts. Эти legacy/research region не должны
  использоваться как показатель production allocator.

### 4.5 IAI/Callgrind gate

В `benches/perf_gate_iai.rs` зарегистрировано 12 функций; основные циклы малы
(например, churn 64 операций и cold batch 256). `scripts/iai.mjs:81-110`
поясняет, что функция выполняется в отдельном процессе, а raw Ir включает
полный allocator bootstrap. Скрипт оценивает marginal Ir/op вычитанием proxy
`large_alloc_free_cycle` (`scripts/iai.mjs:120-143`), но это лишь приближение:
bootstrap и одна large operation не являются точной общей константой для всех
сценариев.

CI gate сравнивает raw Ir и допускает до 10%
(`.github/workflows/perf-gate.yml:27-41,107-116`). Он запускается nightly,
вручную либо по perf label, а не как безусловный required PR check
(`.github/workflows/perf-gate.yml:3-19`). Следствия:

- небольшой hot-path сдвиг разбавляется bootstrap и может не пересечь 10%;
- gate детерминированнее wall clock, но не видит реальную coherence contention,
  HITM, scheduler, syscalls, demand paging, NUMA и частоту CPU;
- cache simulation полезна диагностически, однако CI decision основан на Ir;
- комментарий `scripts/iai.mjs:64` всё ещё говорит об 11 функциях, тогда как
  текущий список содержит 12 — признак рассинхронизации документации harness.

`scripts/first-alloc-bench.mjs:30-42` правильно выделяет отдельный
fresh-process сценарий с 15 samples. Именно он ближе к оценке bootstrap,
first-touch и RSS, но его числа не входят в каноническую throughput-таблицу.

## 5. Cargo profiles, features и codegen

`production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin"]`
(`Cargo.toml:165`). Важно:

- `alloc-stats` не входит в production, поэтому per-hit diagnostic increments
  компилируются из hot path (`Cargo.toml:166-187`);
- `hardened` также не входит в production; его дополнительные проверки и
  generation/modulo cost нельзя приписывать текущей таблице
  (`Cargo.toml:188-205`);
- `alloc-runfreelist` opt-in и не входит в production
  (`Cargo.toml:206-237`). Его доказанная экспериментальная регрессия не
  присутствует в обычном production binary;
- release и bench используют `lto = "thin"`, `codegen-units = 1`
  (`Cargo.toml:482-504`). Это помогает cross-crate inlining и уменьшает
  случайность codegen units, но не фиксирует машинную частоту, `target-cpu`,
  alignment функций или равенство codegen с C-библиотекой mimalloc;
- mimalloc C core компилируется своим оптимизированным build path, поэтому
  одинаковый Cargo profile не означает идентичные inlining/LTO возможности.

Изменения даже в cold functions могут менять расположение и выравнивание hot
functions при ThinLTO. Это правдоподобный источник процентов wall-clock без
изменения Ir, но он **не доказан**, пока не сравнены exact binaries, assembly,
code size и hardware counters.

## 6. Текущие узкие места allocator

### 6.1 Small hot path

На magazine hit allocator не ограничивается чтением указателя: после pop
очищается magazine bitmap (`src/registry/heap_core.rs:821-905`, особенно
`871-886`). Own-thread free выполняет segment ownership lookup, проверяет
allocation/magazine state и снова меняет bitmap
(`src/registry/heap_core.rs:1044-1301`, особенно `1216-1259`). Refill помечает
каждый из N−1 помещаемых в tcache блоков
(`src/registry/heap_core.rs:1582-1650`).

Сохранённый анализ R3 показывает, что отложить эти updates без изменения
протокола нельзя: точный bit нужен и own free, и remote reclaim
(`docs/perf/IAI_BASELINE.md:1206-1245`). Это означает, что простое «batch
bitmap later» не является безопасной оптимизацией.

### 6.2 Segment search и locality

`find_segment_with_free_impl` больше не строит временный 8 KiB список, но всё
ещё проходит `0..count` через `base_at` (`src/alloc_core/alloc_core_small.rs:239-410`).
High-water count не уменьшается; holes пропускаются
(`src/alloc_core/segment_table.rs:20-24,552-575`). Own-cache содержит всего
четыре base (`src/alloc_core/segment_table.rs:90-98,443-480`). При большом
heap это создаёт O(S) scan и conflict misses, но текущие benchmark почти не
создают такой heap.

Сохранённый R1 experiment дал лишь −4.3 и −6.6 Ir/op в двух multi-segment
сценариях и практически ноль в cold/recycle; suite достигала максимум трёх
сегментов (`docs/perf/IAI_BASELINE.md:1264-1334`). Это прямое объяснение, почему
изменение поиска не дало headline-роста: workload не пересекает область, где
асимптотика важна.

### 6.3 Tcache layout

`TCACHE_CAP = 16`, byte refill budget 64 KiB
(`src/registry/tcache.rs:39-98`). `PerClass` хранит count рядом с началом массива
из 16 pointers (`src/registry/tcache.rs:100-153`). На 64-bit обычный stride
порядка 136 байт до внешнего padding; при полном cache count и верхний pointer
не гарантированно попадают в одну cache line. Массив по всем size classes также
давит на L1.

Попытка увеличить CAP уже дала последовательные регрессии: CAP32 примерно
+17–28%, CAP64 +39–69%, CAP128 +84–153% по Ir, а CAP128 до +120–224% wall-clock
(`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`). Механизм согласуется с
ростом footprint, zero-init/refill work и L1 spill. Текущий CAP16 верен как
baseline; «больше cache» здесь не бесплатное ускорение.

### 6.4 Cross-thread atomics и overflow

Remote ring имеет capacity 256 (`src/alloc_core/remote_free_ring.rs:156-166`).
Push читает head/tail и выполняет CAS; при full production может сделать до
8192 попыток, после чего уйти в HeapOverflow
(`src/registry/heap_core_xthread.rs:382-433`). Каждая неудачная попытка означает
повторные atomic/coherence операции. Это не видно single-thread churn.

`RemoteFreeRing` разносит head и tail по cache lines
(`src/alloc_core/remote_free_ring.rs:423-451`), что полезно при producer/consumer
contention. Однако `HeapOverflow` хранит head/tail рядом и два отдельных
payload-потока (`bases` и packed metadata); push делает несколько atomic loads,
CAS и stores (`src/registry/heap_overflow.rs:193-215,280-309`). При fan-in это
кандидат на false sharing и плохую locality.

Capacity HeapOverflow равна 2048 в обычной сборке
(`src/registry/heap_overflow.rs:109-175`). Inline SoA занимает около 24 KiB на
heap slot, то есть около 96 MiB виртуального адресного пространства для 4096
слотов до учёта остального `Registry`; lazy pages уменьшают RSS, но first-touch
и TLB footprint остаются отдельной осью. Bootstrap-код сам документирует
крупный production registry (`src/registry/bootstrap.rs:21-44,280-297`).

### 6.5 Locks, copies и large path

- Обычный owner small path не использует mutex; contention сосредоточен в
  remote atomics/overflow. Глобальный spinlock fallback включается при
  недоступном TLS/исчерпании registry (`src/global/tls_heap.rs:373-381`), то
  есть это редкий, но очень дорогой путь.
- Large-cache lookup линейно просматривает небольшой фиксированный набор
  entries (`src/alloc_core/alloc_core_large.rs:77-98`); прогретые large benches
  в первую очередь измеряют этот hit path, не системный allocator.
- Fallback realloc выделяет новый блок и копирует payload
  (`src/alloc_core/alloc_core.rs:1404-1435`). Workloads с ростом Vec измеряют и
  copy bandwidth, поэтому allocator bookkeeping там не доминирует.
- Эксперимент `alloc-runfreelist` добавлял sort/scan/descriptor work поверх
  существующей chain и получил +23–31% Ir и +40.5–68.9% wall-clock
  (`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`). Он оставлен feature-off, так
  что текущий production не платит эту цену, но эксперимент хорошо показывает
  риск дополнительных проходов по metadata.

## 7. Анализ сохранённых результатов

Верхняя текущая таблица `docs/ALLOC_BENCH.md:22-84` помечена 2026-07-14 и сама
задаёт шум ±15–20%. В сравнении с сохранённой таблицей 2026-07-10
(`docs/ALLOC_BENCH.md:89-106`) получаются следующие непарные изменения Sefer:

| Сценарий | 16 B | 64 B | 256 B | 1024 B |
|---|---:|---:|---:|---:|
| churn + write | +8.6% | +12.8% | +1.3% | +17.4% |
| churn без write | +14.4% | +21.0% | +15.5% | +12.6% |
| bulk, подписанный cold | −12.7% | +7.4% | −18.0% | +7.2% |

Vec улучшился примерно на 16% (1237 → 1040 ns), несмотря на просадку большинства
churn rows. Mimalloc и System между таблицами также заметно двигаются. Это
говорит о существенной межзапусковой/host компоненте и о разной смеси работы в
Vec; это **не** доказывает ни регрессию Sefer, ни её отсутствие.

Сигнал, требующий проверки: все четыре строки non-write churn в более новом
снимке хуже, одна на 21%. Однонаправленность заслуживает парного повторения, но
граница равна заявленному шуму, а сохранены только point estimates. Без raw
samples, CI и exact-baseline binary причинная атрибуция невозможна.

Сохранённые IAI-решения дают более ясную картину:

- R1 candidate почти ничего не менял на малом числе сегментов и был отклонён;
- R3 batching был отклонён по correctness;
- увеличение tcache capacity доказанно ухудшало locality/work;
- run-freelist доказанно регрессировал и остался вне production;
- round-4 документ сообщает нулевое изменение Ir для принятых gate-neutral
  изменений (`docs/ALLOC_BENCH.md:71-84`).

Иными словами, несколько «волн изменений» включали исследования с revert/no-go,
feature-off код и correctness/lifecycle fixes. Ожидать накопительного прироста
production benchmark от суммы таких волн методологически неверно.

## 8. Почему benchmark стагнирует или проседает

### 8.1 Доказанные причины/факты

1. **Mismatch workload и изменений.** N1 работает на thread exit, R2 — при
   saturated remote ring, N3 — при segment deletion; headline table —
   single-thread steady-state alloc/free.
2. **Изменения с отрицательным verdict не входят в production.** Run-freelist
   feature-off, CAP возвращён к 16, R1/R3 candidates не приняты. Они физически
   не могут дать cumulative speedup текущей production сборке.
3. **«Cold» прогревается.** Warmup и повторение в одном процессе превращают его
   в steady bulk reuse.
4. **Heap state не изолирован.** Один TLS heap сохраняется между группами и
   `SeferAlloc::new()`; результат зависит от истории benchmark process.
5. **Каноническая таблица теряет статистику и релевантные группы.** Парсер
   отбрасывает `change:` и не выводит teardown/segment/working-set результаты.
6. **Низкая статистическая мощность.** Sample 10, 150/600 ms и заявленный шум
   ±15–20% не различают ожидаемые малые изменения.
7. **IAI raw metric разбавлен bootstrap.** Gate сравнивает полный Ir и допускает
   10%, а не чистые инструкции hot loop.
8. **Cross-thread harness не измеряет production path.** Неверный class,
   duplicate frees, test seam вместо retry/overflow и отсутствие contention.
9. **Оставшаяся hot-path цена не была устранена.** Magazine bitmap updates,
   segment lookup и refill metadata work всё ещё присутствуют на каждом
   соответствующем событии.
10. **Сохранённые wall-clock снимки непарные.** Между ними меняются также
    mimalloc/System, а raw samples и exact binary identity не сохранены.

Эти пункты доказаны структурой harness/code или сохранённой методикой. Они
доказывают, почему текущий набор плохо способен показать выигрыш; они не
доказывают точный вклад каждого фактора в проценты.

### 8.2 Гипотезы о реальной просадке

1. **N1 увеличивает стоимость короткоживущих потоков.** `trim_for_recycle`
   flush-ит весь tcache, drain-ит pool и evict-ит large cache. В macro benchmark
   это выполняется до join/stop timer. Поэтому thread-turnover throughput может
   стать хуже в обмен на меньшее удержание памяти. Нужен отдельный lifecycle
   benchmark с и без учёта teardown.
2. **Backward-shift deletion переносит цену в каждый delete.** Редкий полный
   tombstone rebuild устранён, но удаление теперь сдвигает probe cluster.
   Среднее время отдельного delete может вырасти при длинном cluster, хотя tail
   станет предсказуемее.
3. **Code layout/ThinLTO drift.** Добавленный cold code или изменившийся inline
   decision мог ухудшить alignment/I-cache hot functions без роста Ir.
4. **Bitmap metadata доминирует мелкий churn.** Для 16–64 B полезной работы мало,
   поэтому hash/cache lookup и bitmap RMW могут скрывать wins ниже слоя.
5. **Host/order bias.** Фиксированный порядок Sefer→mimalloc→System,
   частота/температура Windows и один процесс могут систематически сдвигать
   Sefer rows. Направление нельзя установить статически.
6. **Overflow false sharing.** Соседние head/tail и раздельные payload arrays
   могут ухудшать fan-in; текущий benchmark этого не видит.
7. **Split cache lines RemoteFreeRing.** Это должно помочь истинному SPSC/MPSC
   contention, но может слегка ухудшить locality single-thread seam. Требуется
   сравнение producer counts и hardware counters.
8. **High-water holes.** Долгая история одного TLS heap может увеличить
   O(S) scan даже после деcommit/recycle; текущий порядок групп способен создать
   это состояние, но фактический S в сохранённом запуске неизвестен.

## 9. Дополнительная regression-risk вне allocator headline

`ShardedRegion` остаётся legacy/research-tier, но его benchmark может вводить в
заблуждение. TLS binding `MY_SHARD` процесс-глобален для всех instances
(`src/concurrent/sharded_region.rs:31-37,275-329`), поэтому cached shard id не
проверяет identity region. В `bind_current_thread_to_shard` повторный bind может
выиграть новый occupied CAS, но при уже существующем guard новый shard не
записывается в guard (`src/concurrent/sharded_region.rs:452-487`). Комментарий
утверждает, что новый token освободит prior guard при exit, но `ErasedGuard`
хранит только прежний `shard`; следовательно, новый token выглядит
неосвобождаемым. Это correctness/lifecycle risk, способный со временем менять
contention и benchmark state. Он не является объяснением production allocator,
но такие region numbers нельзя смешивать с ним.

## 10. Рекомендуемый план измерений (не выполнялся)

### Этап A — сделать harness валидным

1. В `heap_xthread` вычислять реальный class для 64 B через тот же layout/class
   mapping, что production. Не публиковать pointer повторно после drain без
   нового allocation.
2. Добавить production-path сценарий: owner heap + реальные producer threads,
   `HeapCore` remote free, ring full, retry и HeapOverflow. Матрица producers:
   1/2/4/8/16/32; owner active, paused и exiting.
3. Разделить macro timing на operations-only, explicit final drain и TLS
   destructor/trim. Отдельно сообщать ops/s и teardown latency/RSS reclaimed.
4. Переименовать текущий `global_alloc` в warm bulk reuse либо делать настоящий
   cold benchmark в fresh subprocess. Не смешивать эти два режима.
5. Выводить все семь Criterion groups, estimates, CI, outliers и `change:`;
   raw Criterion artifacts сохранять вместе с manifest запуска.

### Этап B — парное воспроизводимое сравнение

1. Зафиксировать CPU, OS build, Rust toolchain, target triple, `target-cpu`,
   features, profile и exact source revision для baseline/candidate.
2. Строить оба binary заранее; запускать в случайном чередовании A/B/B/A в
   отдельных процессах, по возможности pin на один physical core, отключив
   конкурирующую нагрузку. Не менять power policy между сериями.
3. Не менее 20–30 process-level repetitions для быстрых сценариев. Решение
   принимать по paired distribution/CI и effect size, не по одному median.
4. Использовать Criterion baseline (`--save-baseline`/paired `change:`), но
   дополнительно хранить process-level samples: Criterion внутри одного
   процесса не устраняет history bias.
5. Считать относительный результат к одновременно измеренным mimalloc/System
   как диагностический normalization, но не заменять им абсолютные cycles/op.

### Этап C — разложить workload по механизму

Отдельные серии:

- bootstrap/first alloc/first touch/RSS в fresh process;
- чистый tcache hit 16/64/256/1024 B;
- refill/flush boundary, отдельно с write и без write;
- сегменты S = 1/3/16/64/256, затем holes 0/25/50/75%;
- segment delete с различной длиной probe cluster, median и p95/p99/max;
- thread lifecycle: 1 allocation, 1k и 100k операций на поток;
- remote fan-in: ring below/full, retry exhausted, overflow drain;
- large cache warm hit, miss, eviction, decommit и OS-cold;
- realloc отдельно: bookkeeping и bytes copied;
- macro workload с фиксированным total work и отдельно fixed work/thread.

Для каждого сценария заранее указать, какой change должен его затронуть. Это
предотвратит поиск R2-эффекта в single-thread churn и N1-эффекта в steady state.

### Этап D — counters, layout и codegen

На Linux/native (не только Callgrind) собрать без изменения workload:

- cycles, instructions, IPC;
- branches/branch-misses;
- L1D/LLC loads и misses, dTLB misses;
- cache-to-cache/HITM для remote fan-in;
- page faults, syscalls, context switches;
- peak/steady RSS и committed/decommitted bytes;
- latency histogram p50/p95/p99/max для delete, refill, overflow и teardown.

Для exact baseline/candidate сохранить disassembly и сравнить size/alignment,
inlining и instruction sequence функций:

- `HeapCore::alloc` / own-thread dealloc;
- tcache hit/refill/flush;
- `find_segment_with_free_impl`;
- `push_with_overflow_retry` и `RemoteFreeRing::push`;
- `SegmentTable` insert/remove;
- TLS heap resolution и recycle trim.

Если Ir совпадает, а cycles ухудшаются, первыми проверять alignment, branch
misses, L1D misses и coherence. Если растут и Ir, и cycles — локализовать новые
ветви/metadata loops. Если ухудшается только thread-turnover — проверять N1 и
границу таймера.

## 11. Приоритет действий

1. **P0 (measurement correctness):** исправить/заменить `heap_xthread` harness;
   до этого не использовать его для выводов о R2.
2. **P0 (evidence):** перестать называть warm bulk benchmark cold и сохранять
   raw paired results, а не только point estimates.
3. **P1:** изолировать TLS heap state по process/benchmark и рандомизировать
   allocator order.
4. **P1:** добавить целевые lifecycle/fan-in/many-segment серии, соответствующие
   N1/R2/N3/R1.
5. **P1:** подтвердить или опровергнуть однонаправленную non-write churn
   просадку парным A/B измерением exact binaries.
6. **P2:** исследовать bitmap hot-path и tcache layout только через безопасный
   protocol redesign; уже отклонённые batching/CAP идеи не повторять без нового
   механизма.
7. **P2:** вынести HeapOverflow footprint/false sharing в отдельный design и
   fan-in experiment.
8. **P2:** исправить lifecycle/token defect legacy `ShardedRegion` либо явно
   исключить его benchmark из allocator performance claims.

## 12. Финальный вердикт

На текущих данных корректный verdict — **неподтверждённая wall-clock
стагнация с возможной churn-регрессией, но без достаточного причинного
измерения**. Принятые изменения в основном не нацелены на headline hot loop;
rejected experiments не входят в production; текущая каноническая таблица
скрывает релевантные группы и статистику. Самые сильные следующие шаги — не
ещё одна общая optimization wave, а исправление cross-thread harness,
process-isolated paired A/B и механистические сценарии для lifecycle,
many-segment и fan-in.
