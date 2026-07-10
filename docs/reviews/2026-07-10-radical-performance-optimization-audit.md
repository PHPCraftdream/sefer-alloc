# Аудит радикальных возможностей ускорения — 2026-07-10

## Контекст и границы

Аудит выполнен в режиме read-only относительно исходного кода на ревизии
`e6b9b3a`. Код allocator'а, тесты и конфигурация не изменялись. Единственный
результат работы — этот документ.

Исследованы:

- публичные лица `SeferAlloc`, `Region<T>` и concurrent tiers;
- hot paths small alloc/free, refill/flush, cross-thread free;
- lifecycle small/large segments, decommit и caches;
- registry bootstrap и layout `HeapSlot`/`HeapCore`;
- существующие perf-reviews, IAI/Callgrind baselines и Criterion benches;
- уже выполненная серия `PERF-PASS-1..5`, чтобы не предлагать повторно
  реализованные или измеренно отклонённые идеи.

Главный вывод: простых универсальных микрооптимизаций почти не осталось.
Steady-state churn уже очень силён. Следующие большие выигрыши требуют либо
нового представления состояния, либо batch API, либо изменения memory-retention
policy. Основные ещё не закрытые области — cold/bulk tiny allocations,
первоначальная инициализация registry, широкие oscillating working sets и
устойчивый cross-thread fan-in.

---

## 1. Свежий wall-clock срез

Команды:

```text
cargo bench --bench global_alloc --features production -- \
  "global_alloc_churn/SeferAlloc|working_set_cycle/SeferAlloc"

cargo bench --bench global_alloc --features production -- \
  "global_alloc_churn/(mimalloc|System)"

cargo bench --bench global_alloc --features production -- \
  "global_alloc/(SeferAlloc|mimalloc)/"
```

Criterion использует `sample_size(10)`, 150 ms warm-up и 600 ms measurement.
Хост шумный, поэтому числа ниже — ориентиры; важны устойчивые различия порядка
величин. Время пересчитано из µs за 1024 пары в ns на одну пару `alloc+free`.

| Размер | Steady-state Sefer | Steady-state mimalloc | Cold/bulk Sefer | Cold/bulk mimalloc |
|---:|---:|---:|---:|---:|
| 16 B | ~21.9 ns | ~23.1 ns | ~39.5 ns | ~17.1 ns |
| 64 B | ~20.6 ns | ~27.9 ns | ~40.4 ns | ~20.8 ns |
| 256 B | ~21.6 ns | ~41.2 ns | ~41.4 ns | ~33.5 ns |
| 1024 B | ~23.4 ns | ~267 ns | ~46.2 ns | ~57.4 ns |

У mimalloc в 256/1024 B наблюдалась высокая дисперсия, поэтому эти две
steady-state оценки нельзя трактовать как точный коэффициент. Тем не менее
срез подтверждает главное:

- steady-state churn не является текущей проблемой Sefer;
- cold/bulk 16–64 B остаётся главным scalar-path проигрышем;
- при 1024 B Sefer уже лидирует даже на bulk-паттерне;
- оптимизации, добавляющие работу на magazine hit ради редкого cold win,
  особенно опасны: они могут ухудшить уже выигранный фронт.

Актуальный детерминированный baseline после PERF-PASS-5 находится в
`docs/perf/IAI_BASELINE.md`. Bootstrap-adjusted стоимость составляет примерно:

- `small_churn_16b`: 124.2 Ir/op;
- `cold_alloc_free_256x16b`: 192.6 Ir/op;
- `recycle_alloc_free_256x16b`: 194.3 Ir/op.

Разница примерно в 68–70 инструкций на пару между churn и cold/recycle —
реальный оптимизационный бюджет для tiny bulk path.

---

## 2. Что уже сделано и не должно предлагаться повторно

Серия PERF-PASS-1..5 уже реализовала или исследовала:

1. `thin LTO` + `codegen-units = 1`: примерно −1–3.5% Ir.
2. Исправление Criterion teardown и новый `working_set_cycle` judge.
3. Windows reserve-then-commit-exact и Unix exact-mmap-first.
4. Пропуск 32 KiB `AllocBitmap` zero-init на virgin OS pages:
   большой выигрыш на fresh-segment workloads.
5. Outlining холодного foreign-dealloc tail.
6. Committed small-segment hysteresis pool: −16.3% для 64 B и −13.8%
   для 1024 B в зафиксированном working-set замере; частичный выигрыш при
   footprint больше четырёх сегментов.
7. Large-cache best-fit.
8. Remote-ring empty guard и false-sharing partitions.
9. `SegmentHeader`/`Tcache` cache-line reorder: −11.7% bootstrap Ir, но почти
   нулевой marginal hot-path delta.

Измеренно отклонены и не должны возвращаться без новой формы эксперимента:

- `TCACHE_CAP=32` и bloom-gated magazine scan;
- run-encoded freelist в текущем представлении;
- clz-based `class_for` вместо `SIZE2CLASS`;
- per-class segment index в варианте, платящем постоянную цену уже при трёх
  сегментах;
- `alloc_zeroed` virgin-payload skip;
- увеличение `REFILL_BATCH` выше 31.

Важно: попытка объединить magazine-residency с текущим `AllocBitmap` была
отклонена не как фундаментально невозможная, а потому что простая смена смысла
бита не моделировала переходы bump → magazine → user → magazine → freelist и
затрагивала cross-thread reclaim.

---

## 3. Приоритет P0: точное O(1) membership для magazine

### Текущее состояние

Каждый own-thread small `free`:

1. просматривает `tcache.classes[c].slots[0..count]`;
2. проверяет per-segment `AllocBitmap`;
3. только затем помещает блок в magazine.

Magazine ограничен 16 элементами, поэтому асимптотически это bounded O(1), но
практически выполняется 0–16 сравнений указателей на каждый `free`. В bulk/free
storm magazine регулярно близок к заполнению. Это крупнейшая оставшаяся
структурная работа tiny free path.

### Почему нельзя просто переименовать существующий бит

Сегодня `AllocBitmap` кодирует membership во freelist. Bump-carved блоки
изначально имеют состояние «allocated», а `drain_freelist_batch` делает
`mark_alloc` при передаче блока вызывающему коду. При refill назначением блока
может быть magazine, а не пользователь. Простая семантика «1 = не у
пользователя» требует менять все `mark_alloc`/`mark_free` call sites и протокол
ring drain. Именно это было недооценено ранней гипотезой.

### Предпочтительный новый эксперимент: отдельный `MagazineBitmap`

Оставить `AllocBitmap` неизменным и добавить второй точный bitmap:

- push в magazine: `mark_magazine(off)`;
- pop пользователю: `clear_magazine(off)`;
- free: одна проверка magazine-bit вместо pointer scan;
- flush: `clear_magazine(off)` + существующий `mark_free(off)`;
- refill из freelist в magazine: существующий `mark_alloc` +
  `mark_magazine`;
- direct substrate allocation: magazine-bit не ставится;
- remote drain продолжает использовать существующий ring protocol, но
  `is_in_magazine` становится O(1).

Memory cost: ещё 32 KiB на 4 MiB small segment, около 0.78%. На virgin OS pages
его init можно пропустить тем же доказанным способом, что и текущий bitmap.

### Риски и критерий GO

Риск: на magazine alloc hit появится metadata store, которого сегодня нет.
Он может компенсировать экономию сканирования. Поэтому обязательны два судьи:

- IAI marginal Ir/op на churn/cold/recycle;
- Criterion на 16/64/256 B с отдельными churn и bulk patterns.

GO: существенное улучшение cold/recycle 16–64 B без статистически значимой
регрессии steady-state churn. Любая churn-регрессия больше принятого в проекте
kill threshold должна отклонять реализацию.

### Более дешёвая policy-альтернатива

Добавить явно названный `trusted-fast`/`unchecked-fast` feature, отключающий
M2 magazine scan на `GlobalAlloc` path. По контракту `GlobalAlloc::dealloc`
двойной free уже является нарушением unsafe precondition вызывающей стороны.
Это не меняет поведение для корректной Rust-программы, но ослабляет нынешнюю
defence-in-depth гарантию проекта. Такой режим должен существовать отдельно от
default `production`, а не незаметно менять его.

---

## 4. Приоритет P0: chunked HeapRegistry

### Проблема

Registry содержит фиксированный массив из 4096 полных `HeapSlot`. Текущий
stride слота после cache-line alignment — примерно 7040 B, то есть общий
reservation около 27.5 MiB.

При bootstrap выполняется проход по всем 4096 слотам и записывается
`next_free = u32::MAX`. Поскольку шаг между слотами больше 4 KiB, проход
затрагивает как минимум 4096 разных страниц — примерно 16 MiB physical RSS
даже если процесс использует один heap/thread. На Windows дополнительно
возникает commit charge всего reservation.

Это не steady-state hot-path дефект, но крупный first-allocation latency и RSS
дефект.

### Решение

```text
Registry
  chunks: [AtomicPtr<Chunk>; 64]
  count/free_slots/abandoned_segs

Chunk
  slots: [HeapSlot; 64]
```

Индекс слота разбивается на `chunk = idx / 64`, `local = idx % 64`. Chunk
создаётся через существующий `aligned_vmem` и CAS-публикацию только при первом
обращении. Slot addresses остаются стабильными на весь процесс, лимит 4096 и
TaggedPtr encoding сохраняются.

### Ожидаемый эффект

- первый registry chunk порядка 440 KiB вместо reservation 27.5 MiB;
- инициализация 64 slots вместо 4096 — около 64-кратного сокращения работы;
- RSS масштабируется с реальным high-water числом потоков;
- TLS-bound hot path не меняется.

### Проверка

Нужен отдельный process-per-sample benchmark:

- latency первой allocation;
- committed/private RSS после 1, 8, 64, 512 heaps;
- параллельное создание первого chunk;
- recycle/claim через границу chunk;
- registry stats и abandoned adoption через несколько chunks.

---

## 5. Приоритет P0/P1: настоящий batch API

Ядро уже имеет эффективные внутренние примитивы:

- `refill_class`/`refill_class_bump`;
- `flush_class`;
- `carve_batch` с одним bump update и одним batched live-count update;
- batched freelist head update.

Но публичный scalar `GlobalAlloc` для каждого блока повторяет TLS resolution,
classification, routing, M2 и magazine bookkeeping.

Предлагаемый API:

```text
unsafe fn alloc_batch(layout, out: &mut [*mut u8]) -> usize
unsafe fn dealloc_batch(layout, ptrs: &[*mut u8])
```

или безопасная typed-обёртка `Pool<T>` поверх этих примитивов.

Batch dealloc может проверять duplicates внутри батча один раз, выполнять
классификацию один раз и группировать блоки по segment/class перед flush. Это
не ускорит произвольный код через стандартный `GlobalAlloc`, но даст самый
высокий потолок для DBMS, ECS, network buffers и arena-like workloads.

Для bulk 16 B текущий scalar путь около 39.5 ns/op против 17.1 ns/op mimalloc;
batch API — наиболее прямой способ убрать повторяющуюся per-call работу, не
ухудшая выигранный scalar churn.

---

## 6. Приоритет P1: scalable/adaptive small-segment pool

### Текущая проблема

Конфигурация публично позволяет написать `.pool_segments(8)`, но runtime cap
жёстко ограничен `POOL_MAX_SLOTS = 4`. Значение молча clamp-ится. В итоге
широкий working set продолжает регулярно делать release/decommit/re-reserve.

Простое увеличение массива плохо масштабируется: `AllocCore` встроен в каждый
из 4096 `HeapSlot`, поэтому любое увеличение per-heap массива умножается на
максимальный registry footprint.

### Решение

Сделать pool intrusive FIFO/LRU через small-segment headers:

- `pool_prev`/`pool_next` либо отдельный owner-only link;
- в `AllocCore` только `head`, `tail`, `count`, `byte_budget`;
- O(1) admit, reuse removal и oldest eviction;
- реальный configurable cap 4/8/16/64;
- adaptive cap: повышать при быстром reuse после pool miss, снижать decay tick;
- OOM path немедленно дренирует pool — это уже добавлено в `e6b9b3a`.

Default 4/16 MiB можно сохранить, но пользовательская конфигурация должна быть
честной. Silent clamp необходимо либо удалить, либо превратить в явную ошибку.

### Trade-off

Pool сохраняет страницы committed. Это сознательный RSS ↔ latency обмен.
Нужны presets:

- low-rss: cap 0–1;
- balanced: 4 / 16 MiB;
- throughput: adaptive 8–64 MiB или workload-defined budget.

---

## 7. Приоритет P1: overflow-safe cross-thread free

`RemoteFreeRing` имеет `RING_CAP = 256`. При переполнении `push` возвращает
`Err(PushOverflow)`, но вызывающая сторона игнорирует результат. Блок остаётся
mapped, но больше никогда не возвращается allocator'у. Повторяющиеся overflow
events поэтому дают неограниченный логический leak и удерживают segment
`live_count` выше нуля, мешая decommit.

Это не видно в обычном larson/mstress, где доля remote frees умеренная, но
опасно для producer→consumer/fan-in workloads.

Варианты:

1. heap-level overflow MPSC stack;
2. fixed overflow log после перехода registry на chunks;
3. dirty-segment queue: remote producer при empty→non-empty один раз ставит
   queued flag и публикует base; owner дренирует только dirty segments;
4. bounded retry/yield как временное уменьшение burst overflow, но не полное
   решение.

Предпочтительная архитектура — dirty-segment queue плюс non-dropping overflow
fallback. Она одновременно:

- исключает потерю освобождённых блоков;
- убирает polling rings всех сегментов;
- уменьшает O(n segments) работу refill miss при cross-thread трафике.

Протокол высокорисковый: нужны loom tests для empty→queued, concurrent push
при очистке флага, slot recycle/adoption и delayed publication.

---

## 8. Приоритет P2: hybrid per-class segment index

`find_segment_with_free_impl` остаётся O(number_of_registered_segments) и на
miss проходит `slots[0..count)`, читает kind, проверяет ring и BinTable.

Ранний per-class index был правильно отклонён при трёх сегментах: постоянное
обслуживание структуры стоило дороже scan. Но это не отменяет пользу при
64–1024 сегментах.

Гибридный вариант:

- при `count < 64` оставить текущий scan;
- после порога построить `class_nonempty[49][1024 bits]`;
- поиск через `trailing_zeros` по максимум 16 `u64` words;
- обновлять бит только на переходах BinTable `empty ↔ non-empty`;
- dirty remote segments сначала дренировать и затем обновлять index.

Размер около 49 × 1024 / 8 = 6.1 KiB на активный heap. Это превращает поиск в
O(n/64), но добавляет bookkeeping на free/pop transitions. До реализации
обязателен benchmark с 64/128/512 активными small segments; текущие benches
работают в основном с ≤3 и не способны честно судить эту идею.

---

## 9. Приоритет P2: разделить caching и decommit policy

Large cache сейчас функционально находится под feature `alloc-decommit`.
Поэтому нельзя выбрать естественный latency-профиль:

- не decommit-ить small segments;
- сохранить large cache;
- избежать small live-count/decommit lifecycle work.

Предлагаемое разбиение:

- `alloc-large-cache`;
- `alloc-small-decommit`;
- `alloc-small-pool`;
- существующие `fastbin`, `alloc-xthread`;
- presets `throughput`, `balanced`, `low-rss`, `hardened`.

Это позволит small-only throughput workload не платить за aggressive RSS
return, а mixed workload — сохранить сильнейшее преимущество large-cache.

---

## 10. Приоритет P3: O(1) over-aligned classification

Для `align <= 16` уже используется `SIZE2CLASS` O(1). Для большего alignment
`class_for` идёт вперёд по таблице из 49 классов, обычно 0–3 шага.

Поскольку `Layout::align()` — степень двойки, walk можно заменить небольшой
compile-time таблицей:

```text
ALIGNED_CLASS[align_log2][seed_class] -> class | NONE
```

Размер порядка 15 × 49 bytes. Это убирает walk и modulo с over-aligned path.
Однако текущий IAI `aligned_churn_640b_a128` почти равен обычному churn
(124.4 против 124.2 Ir/op), поэтому ожидаемый эффект мал. Реализовывать только
после workload-профиля с большим количеством align 32/64/128/4096.

---

## 11. `Region<T>` и concurrent containers

`Region<T>` — тонкая оболочка над `slotmap::SlotMap`; все основные операции уже
O(1), wrapper overhead минимален.

Для iteration-heavy workload существующие измерения показывают:

- `Region/SlotMap` iteration: ~14.1 µs на 10k;
- `DenseSlotMap`: ~10.8 µs, около 30% быстрее;
- lookup у `DenseSlotMap` примерно на 16% медленнее;
- churn примерно втрое медленнее.

Поэтому менять default нельзя, но полезен отдельный `DenseRegion<T>` для full
sweep-heavy workloads.

Для `SyncRegion` основной practical win не требует изменения реализации:
несколько операций должны выполняться под одним `read()`/`write()` guard, а не
через тысячи one-shot методов с повторным lock/unlock.

Experimental concurrent regions уже помечены legacy/research-tier; вкладывать
в их микрооптимизацию до появления production use case нецелесообразно.

---

## 12. Code-quality изменения, помогающие будущей оптимизации

Они не дадут непосредственный runtime win, но уменьшат риск perf-работ:

1. Разделить почти 4900-строчный `alloc_core.rs` на small path, small pool,
   large path и large cache.
2. Удалить silent config clamp или сделать resolved cap публично наблюдаемым.
3. Исправить stale `rss_probe` prose: глобальный overflow counter уже
   существует, но комментарии всё ещё говорят, что его надо добавить.
4. Публиковать рядом два churn числа:
   - чистый reuse-path;
   - lifecycle-inclusive working-set cycle.
5. Добавить first-process/startup benchmark: Criterion внутри одного процесса
   не измеряет реальную цену registry bootstrap.

---

## 13. Рекомендуемая последовательность работ

### Этап A — измерительные инструменты

1. `first_alloc_process`: latency + RSS/commit после первого alloc.
2. `pool_cap_sweep`: 0/1/4/8/16 segments.
3. `multiseg_refill`: 3/16/64/128/512 segments.
4. `remote_fanin`: ring occupancy, overflow, reclaimed/attempted, RSS.
5. `batch_alloc_free`: scalar против batch для 16/64/256/1024 B.

### Этап B — независимые крупные выигрыши

1. Chunked registry.
2. Experimental `MagazineBitmap` с жёстким GO/NO-GO.
3. Batch API.

### Этап C — lifecycle и MT

1. Scalable/adaptive small pool.
2. Overflow fallback + dirty-segment queue.
3. Feature/preset split cache/decommit.

### Этап D — только по показаниям benchmarks

1. Hybrid per-class segment bitmaps при ≥64 segments.
2. Over-aligned classifier LUT.
3. `DenseRegion<T>` как отдельный backend.

---

## Итоговый вердикт

Проект не страдает от плохой базовой алгоритмики на обычном hot path:

- size classification для типичного alignment O(1);
- ownership lookup O(1);
- magazine hit без locks и atomic RMW;
- cached large/realloc paths чрезвычайно сильны;
- MT scaling уже конкурентоспособен.

Самые большие незакрытые возможности:

1. убрать pointer scan magazine точным bitmap/state representation;
2. перестать materialize-ить registry на 4096 heaps при первом потоке;
3. дать пользователю настоящий batch API;
4. позволить small pool масштабироваться выше четырёх сегментов;
5. перестать терять blocks при RemoteFreeRing overflow;
6. индексировать свободные сегменты только после порога, где O(n) scan реально
   становится дорогим.

Именно эти направления способны дать двузначные проценты или кратные выигрыши.
Остальные найденные возможности — локальные проценты либо workload-specific
настройка, а не радикальное ускорение.
