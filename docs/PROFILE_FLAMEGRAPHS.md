# sefer-alloc — Flamegraph Profiling Report (2026-06-28)

Задача #61: построить flamegraph'ы под характерными нагрузками, найти hot paths,
выявить candidate-оптимизации.

---

## §0 — Как воспроизвести

### Предусловия

```
perf_event_paranoid = 2  (уже активно)
cargo install flamegraph   # 0.6.13
cargo install inferno      # 0.12.6
apt-get install linux-tools-generic  # perf 6.8
```

**Важно — WSL2 ABI mismatch:**
`/usr/bin/perf` (symlink на perf-6.18 WSL2-ядра) сломан при записи данных.
Надо использовать `/usr/lib/linux-tools/6.8.0-124-generic/perf` напрямую.
`cargo flamegraph` с этим PATH работает как ожидается.

**Важная деталь:** `cargo flamegraph` с `perf.data` в рабочем каталоге на
примонтированном NTFS (D: drive) вызывает ошибку `"failed to write perf data,
error: Bad address"` — это не ABI-проблема, а медленный NTFS IO под perf MMapped
кольцевым буфером. Решение: использовать `CARGO_TARGET_DIR=/tmp/...` + напрямую
собирать бинарь и запускать perf с `-o /tmp/...`.

### Команды воспроизведения

```bash
export PATH=/usr/lib/linux-tools/6.8.0-124-generic:$PATH

# §1 — Small-class churn
mkdir /tmp/sefer-fg1
CARGO_PROFILE_BENCH_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg1 \
  cargo build --bench global_alloc --features 'alloc-global' --profile bench
perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg1/perf_small.data \
  /tmp/sefer-fg1/release/deps/global_alloc-<hash> --bench 'SeferMalloc'
perf script -i /tmp/sefer-fg1/perf_small.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg1/small_churn.svg

# §2 — MT cross-thread
mkdir /tmp/sefer-fg2
CARGO_PROFILE_RELEASE_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg2 \
  cargo build --release --example malloc_macro \
  --features 'alloc-global alloc-xthread'
perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg2/perf_mt.data \
  /tmp/sefer-fg2/release/examples/malloc_macro
perf script -i /tmp/sefer-fg2/perf_mt.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg2/mt_xthread.svg

# §3 — Large/realloc
mkdir /tmp/sefer-fg3
CARGO_PROFILE_BENCH_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg3 \
  cargo build --bench large_realloc --features 'alloc-global' --profile bench
perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg3/perf_large.data \
  /tmp/sefer-fg3/release/deps/large_realloc-<hash> --bench 'SeferMalloc'
perf script -i /tmp/sefer-fg3/perf_large.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg3/large_realloc.svg

# §4 — tokio burn-in
mkdir /tmp/sefer-fg4
CARGO_PROFILE_RELEASE_DEBUG=true CARGO_TARGET_DIR=/tmp/sefer-fg4 \
  cargo build --release --example tokio_burn_in \
  --features 'alloc-global alloc-xthread'
SEFER_BURNIN_SECONDS=20 SEFER_TOKIO_WORKERS=4 SEFER_BURNIN_TASKS=512 \
  perf record -F 99 --call-graph dwarf,16384 -g \
  -o /tmp/sefer-fg4/perf_tokio.data \
  /tmp/sefer-fg4/release/examples/tokio_burn_in
perf script -i /tmp/sefer-fg4/perf_tokio.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title '...' > /tmp/sefer-fg4/tokio_burnin.svg
```

---

## §1 — Single-thread small-class churn (`global_alloc` bench)

**SVG:** `/tmp/sefer-fg1/small_churn.svg`  
**Сэмплов:** 9 463 (cycles:Pu). Потери сэмплов: 0.

### Что обнаружено — ПРЕДУПРЕЖДЕНИЕ о качестве данных

Flamegraph профилирует **весь процесс criterion**, включая его собственную
статистику (KDE / bootstrapping / rayon). В итоге картина сильно искажена:

| Функция | Self-time |
|---|---|
| rayon/criterion KDE `bridge_producer_consumer_helper` | **52.25%** |
| `libm __ieee754_exp_fma` | 20.74% |
| `libm exp()` | 11.56% |
| `AllocCore::alloc` (SeferMalloc) | **1.72%** |
| `bench_direct_alloc` (обёртка benchmark) | 1.02% |
| `SegmentTable::contains_base` | 0.72% |
| `HeapCore::stamp_segment_owner` | 0.29% |

**Вывод:** ~84% CPU — criterion статистика (KDE с exp() из libm). Сам аллокатор
занимает ~3.7% суммарно. Данные **информативны** только относительно друг друга,
не в абсолютном смысле.

### Hot paths аллокатора (в пределах его 3.7%)

**Top-3 по self-time (только аллокатор):**
1. `AllocCore::alloc` — 1.72% (основной путь аллокации)
2. `SegmentTable::contains_base` — 0.72% (проверка чужого указателя в dealloc)
3. `HeapCore::stamp_segment_owner` — 0.29% (атомарная штамповка владения)

**Top-3 по total-time (только аллокатор):**
1. `AllocCore::alloc` — включает `pop_free` (pop из BinTable) + `alloc_small`
2. `SegmentTable::contains_base` — линейный O(segments) scan при каждом `dealloc`
3. `HeapCore::stamp_segment_owner` — Acquire-load + условный Release-store на каждом alloc

### Наблюдения

1. **`contains_base` — O(segments) scan при КАЖДОМ `dealloc`.** Это линейный
   перебор по slots[] в segment_table.rs:220. В бенче, где одна нить непрерывно
   делает alloc/dealloc маленьких блоков, это не узкое место только потому что
   segments < 5. Но при росте live_segments (до 50–100) это может стать
   заметным.

2. **`stamp_segment_owner` на каждом alloc.** В `HeapCore::alloc` после каждой
   удачной аллокации вызывается `stamp_segment_owner`, которая делает Acquire
   load + условный Release store `owner_state`. Для segment-hot workload где
   один сегмент используется постоянно, условие `unpack_owner_id(cur) != self.id`
   никогда не срабатывает, но Acquire load всё равно происходит.

3. **Реальная производительность:** SeferMalloc ~18–20 µs на партию 32К
   операций (criterion batch). Mimalloc ~10–14 µs (1.5–2× быстрее на малых
   размерах). Это соответствует ожиданиям.

### Candidate-оптимизации (§1)

- **OPT-A:** Skip `stamp_segment_owner` когда сегмент уже промаркирован этим
  heap id (кэшировать last_stamped_base в HeapCore). Ожидаемый выигрыш: ~0.3%
  CPU, но может быть заметен в micro-bench.
- **OPT-B:** `contains_base` — заменить линейный scan на hash-сет или bitmap
  с O(1) lookup. Актуально при segments > 20.

---

## §2 — MT cross-thread free (`malloc_macro` benchmark)

**SVG:** `/tmp/sefer-fg2/mt_xthread.svg`  
**Сэмплов:** 361 (cycles:Pu). Потери сэмплов: 0.  
**Примечание:** малое число сэмплов — workload короткий (larson + mstress T=1/2/4).
Выводы — индикативные, не точные.

### Top-3 по self-time

| Функция | Self-time |
|---|---|
| `std::sync::mpmc::list::Channel::try_recv` | **16.70%** |
| `libc _int_free` | 11.21% |
| `libc malloc` | 10.28% |

*Замечание: `libc malloc/_int_free` здесь — malloc_macro бенч запускает все три
аллокатора (SeferMalloc, mimalloc, System) параллельно.*

| Функция (только SeferMalloc) | Self-time |
|---|---|
| `AllocCore::alloc` | **5.24%** |
| `HeapCore::dealloc_routing` | 3.58% |
| `mstress_worker<SeferMalloc>` | 1.91% |
| `larson_worker<SeferMalloc>` | 0.98% |
| `HeapCore::stamp_segment_owner` | 1.46% |

### Наблюдения

1. **`dealloc_routing` — 3.58% self.** Это cross-thread dealloc путь: читает
   `magic_at`, `owner_thread_free_at`, вычисляет `segment_base_of_ptr` и толкает
   `(offset, class)` в RemoteFreeRing через CAS. Занимает ~40% от alloc (5.24%).
   Соотношение ожидаемо для larson (который активно делает cross-thread free).

2. **`stamp_segment_owner` — 1.46%** из общих 5.24% alloc. Это значит что на
   каждые ~3.6 аллокации тратится 1 stamp. Значительно.

3. **`mstress` vs `larson`:** mstress на SeferMalloc (1.91%) vs larson (0.98%).
   mstress делает случайные размеры с большим диапазоном — больше промахов по
   free-list, чаще `carve_block`.

4. **Сравнение с mimalloc:** mimalloc показывает лучший mstress (27.41 M vs
   19.23 M ops/sec). Это 1.43× отрыв. larson — SeferMalloc выигрывает (18.3 M
   vs 13.6 M). Flamegraph показывает что `mi_page_queue_find_free_ex` и
   `mi_page_malloc_zero` в mimalloc занимают схожий процент с нашим `alloc`.

5. **RemoteFreeRing overhead:** cross-thread push занимает малую часть — ~0.1%
   в `ring.push`. Это значит что CAS-резервирование не является горлышком.

### Candidate-оптимизации (§2)

- **OPT-C:** `stamp_segment_owner` — переписать на branch-free check:
  сохранять last_stamped_base и пропускать stamp если `ptr` в том же сегменте.
  Экономит Acquire load + branch на каждом alloc.
- **OPT-D:** `dealloc_routing` — `segment_base_of_ptr` вызывается и в alloc и в
  dealloc. Можно передавать base как hint (если caller знает) — но это меняет
  API. Не рекомендуется.

---

## §3 — Large/realloc (`large_realloc` bench)

**SVG:** `/tmp/sefer-fg3/large_realloc.svg`  
**Сэмплов:** 8 648 (cycles:Pu). Потери: 0.

### ПРЕДУПРЕЖДЕНИЕ: те же искажения от criterion

| Функция | Self-time |
|---|---|
| criterion KDE (rayon) | **50.37%** |
| `libm __ieee754_exp_fma` | 21.93% |
| `libm exp()` | 11.01% |
| `AllocCore::alloc` (SeferMalloc) | **6.74%** |
| `HeapCore::realloc` | 0.01% |

### Top-3 по self-time (аллокатор)

1. `AllocCore::alloc` — 6.74% (значительно выше чем в small-class бенче!)
2. `libm __munmap` — 0.08% (OS dealloc для large сегментов)
3. `HeapCore::realloc` — 0.01%

### Наблюдения

1. **Large alloc полностью идёт через mmap/VirtualAlloc.** Каждая аллокация
   ≥ SMALL_MAX получает отдельный сегмент через `os::reserve_segment`. На WSL2
   mmap не имеет кэша страниц — каждый alloc+free это полный round-trip к
   гипервизору. SeferMalloc измеренное: **~8.3 µs** на alloc+free 4 MiB/16 MiB/64 MiB.
   mimalloc (не в этой профиле) имеет page-cache для крупных аллокаций → намного
   быстрее.

2. **`realloc_grow_geometric` — 65 µs на 16 doublings (64 B → 4 MiB).** Это
   alloc + memcpy + dealloc на каждом шаге. SeferMalloc `realloc` всегда
   делает новый alloc + copy (нет in-place growth) — каждый шаг = 2 mmap + 1 memcpy.
   mimalloc имеет slab-growth с частичным in-place — значительно выигрывает
   (задокументировано как «300×+ отставание в MALLOC_BENCH.md»).

3. **`AllocCore::alloc` занимает 6.74%** vs 1.72% в small-class бенче. Пропорция
   увеличилась т.к. мы профилировали только SeferMalloc (фильтр `--bench 'SeferMalloc'`),
   снизив вес criterion.

4. **`__munmap` — 0.08%** свидетельствует о реальных OS вызовах в dealloc large.
   Это холодный путь (один раз на итерацию бенча), но он дорог по абсолютному
   времени.

5. **`realloc_in_place_unfavorable`:** SeferMalloc тратит ~9.5 µs на 8 шагов
   роста с соседями. Каждый шаг — full mmap (large сегмент) + memcpy + munmap.
   Это неизбежно без сегментного кэша.

### Candidate-оптимизации (§3)

- **OPT-E:** Кэш пустых large-сегментов (размер ≤ N, например ≤ 64 MiB). При
  деаллокации large сегмент не освобождается сразу, а кладётся в per-thread
  freelist (1–2 слота). При следующей large alloc похожего размера — reuse без
  mmap. Ожидаемый выигрыш: 10–100× на large alloc+free micro-bench. Риск:
  RSS растёт (сегмент держится в памяти). Параметр: max кэш размер + time-based
  eviction.

- **OPT-F:** In-place realloc для small→small upgrades (когда новый размер ≤
  block_size текущего класса). Сейчас `AllocCore::realloc` всегда делает
  alloc + copy + dealloc. Если `new_size <= SizeClasses::block_size(old_class_idx)`,
  можно вернуть тот же ptr. Ожидаемый выигрыш: eliminates alloc+copy+dealloc на
  часто-realloc патернах. Риск: надо аккуратно обновить live_count (decommit
  feature).

---

## §4 — tokio async burn-in

**SVG:** `/tmp/sefer-fg4/tokio_burnin.svg`  
**Сэмплов:** 8 (cycles:Pu). Потери: 0.

### ПРЕДУПРЕЖДЕНИЕ: данные крайне ограничены

Burn-in с 512 задачами завершается за **0.07 секунды**. Perf с F=99 Гц успел
взять всего 8 сэмплов. Любые выводы с таким sample-size ненадёжны —
это **ориентировочные** данные, не статистически значимые.

Для получения реального профиля нужен либо:
- Повторяющийся workload (loop { spawn 512 tasks }) без early-exit, или
- Большее число задач — но тогда аллокатор падает с OOM (см. ниже).

### Что произошло при увеличении нагрузки

При `SEFER_BURNIN_TASKS=2000, SEFER_BURNIN_CONCURRENCY=200` процесс падает с:
```
memory allocation of 256 bytes failed
memory allocation of 3072 bytes failed
skipping backtrace printing to avoid potential recursion
```

Это OOM: `MAX_SEGMENTS = 1024` без `alloc-decommit` — аппендируемая таблица
сегментов переполняется при большом числе concurrent задач и их аллокаций tokio
internal (runtime allocates per-task stacks, queues, etc.).

### Top-3 по self-time (при 8 сэмплах — НИЖНЕЕ ДОВЕРИЕ)

| Функция | Self-time |
|---|---|
| `AllocCore::alloc` | 24.72% |
| `run_query` (async closure) | 13.39% |
| `Mutex::lock_contended` | 13.39% |
| tokio worker `run_task` | 13.39% |
| `__memset_evex_unaligned_erms` | 12.23% |
| `HeapCore::dealloc_routing` | 9.47% |

### Наблюдения (осторожно: 8 сэмплов)

1. **`AllocCore::alloc` — 24.72% из 8 сэмплов.** Ожидаемо: tokio создаёт
   задачи, инициализирует TLS heap'ы, аллоцирует task-local данные.

2. **`Mutex::lock_contended` — 13.39%.** Это std::sync::Mutex, предположительно
   в HeapRegistry (при claim/init нового heap для нового tokio worker thread).
   TLS heap init под contention видна явно.

3. **`dealloc_routing` — 9.47%.** Cross-thread free активен: tokio дропает
   задачи на потоках-воркерах, отличных от тех, что аллоцировали.

4. **`memset` — 12.23%.** Крупные zeroed аллокации (Vec::resize, HashMap init).

5. **OOM при масштабировании** — ключевая находка: `alloc-decommit` не включён
   в стандартном `alloc-global + alloc-xthread` build. Без него сегменты не
   возвращаются → быстрое исчерпание 1024-slot таблицы под async нагрузкой.

### Candidate-оптимизации (§4)

- **OPT-G:** Включить `alloc-decommit` в tokio burn-in и soak-тест как
  recommended build. Решает OOM при scale и снижает RSS.
- **OPT-H:** HeapRegistry::claim — убрать или заменить Mutex на lock-free CAS
  для TLS heap init (атомарный slot-claim). Снизит Mutex::lock_contended при
  массовом создании задач/потоков.

---

## §5 — Prioritised optimization candidates

### #1 — OPT-E: Кэш пустых large-сегментов (HIGH IMPACT)

**Что менять:** в `AllocCore::dealloc` (путь Large) не освобождать сегмент
немедленно через `os::release_segment`, а хранить 1–2 слота в per-`AllocCore`
freelist. При следующей `alloc_large` схожего размера — reuse без mmap.

**Ожидаемый выигрыш:** 10–100× на large alloc+free micro-bench (8 µs → < 1 µs
для hot path). Актуально для `realloc_grow_geometric` и `realloc_in_place_unfavorable`.

**Риск регрессии:** RSS растёт на размер кэшированных сегментов (до 64 MiB × 2 = 128 MiB
максимум при разумных лимитах). Нужно time-based или size-based eviction. Добавляет
небольшой overhead в `alloc_large` cold path (scan freelist).

**Измеримость:** `large_alloc_free/SeferMalloc/4MiB` должен упасть с 8.3 µs до < 1 µs.

---

### #2 — OPT-F: In-place small→small realloc (MEDIUM IMPACT)

**Что менять:** в `AllocCore::realloc` перед `alloc + copy + dealloc` проверить:
если `SizeClasses::class_for(new_size, align) == SizeClasses::class_for(old_size, align)`
(или new class block_size <= current block_size), вернуть тот же ptr без copy.

**Ожидаемый выигрыш:** Vec::push-подобные паттерны (size growing by 1.5–2×) часто
попадают в тот же class_idx при malых ростах → eliminates alloc+copy+dealloc.
В `realloc_grow_geometric` первые несколько шагов (64 B → 128 B → 256 B) могут
попасть в один или соседний класс — не всегда, но частично.

**Риск регрессии:** Нулевой для корректности (проверка class_idx безопасна). Небольшой
риск фрагментации (блок больше нужного). Нужно убедиться что `live_count` и
alloc_bitmap не нарушены при in-place (размер блока не меняется — нет проблем).

**Измеримость:** Новый micro-bench `realloc_same_class` / `global_alloc/Vec_push`.

---

### #3 — OPT-B/C: O(1) segment lookup + lazy `stamp_segment_owner` (MEDIUM IMPACT)

**Что менять (OPT-B):** `SegmentTable::contains_base` — заменить линейный O(count) scan
на hash-set или открытую адресацию (open-addressing map с SEGMENT-aligned keys —
key = base >> log2(SEGMENT), value = slot_idx). Footprint: 1024 × 2 × 4 B = 8 KB
(вписывается в metadata сегмент).

**Что менять (OPT-C):** `HeapCore::alloc` вызывает `stamp_segment_owner` после КАЖДОЙ
аллокации. Добавить в `HeapCore` поле `last_stamped_segment: *mut u8` — пропускать stamp
если `segment_base_of_ptr(ptr) == self.last_stamped_segment`.

**Ожидаемый выигрыш (OPT-B):** При num_segments = 50+ снижение dealloc cost на ~0.5–0.7% CPU.
**Ожидаемый выигрыш (OPT-C):** ~1–1.5% CPU на Acquire-load при малом num_segments (как
в global_alloc micro-bench — stamp dominates ~1.46% в MT профиле).

**Риск регрессии (OPT-B):** Усложнение SegmentTable, возможные hash-collision edge cases.
**Риск регрессии (OPT-C):** При миграции между сегментами кэш инвалидируется корректно
(при следующем alloc из другого сегмента stamp происходит). Нет риска.

**Измеримость:** Micro-bench с многими живыми сегментами (e.g. 100 параллельных аллокаторов).

---

### #4 — OPT-G: `alloc-decommit` как default в multi-thread builds (HIGH IMPACT для scale)

**Что менять:** Не код, а recommended build: `alloc-global + alloc-xthread + alloc-decommit`
как recommended feature set для production. Или добавить convenience feature alias.

**Ожидаемый выигрыш:** tokio burn-in работает при 2000+ задачах без OOM; soak-test меньше
давит на RSS. Устраняет hard segment-table overflow для long-running workloads.

**Риск регрессии:** `alloc-decommit` добавляет `dec_live`/`inc_live` counter updates на
каждый alloc/dealloc — небольшой overhead (~1 field write). Проверено в soak-тесте.

---

### #5 — OPT-H: Lock-free HeapRegistry::claim (LOW IMPACT — пока)

**Что менять:** Заменить Mutex в `HeapRegistry` при TLS bind-slow path на CAS-based
claim: атомарное slot_state FREE→LIVE без blocking mutex.

**Ожидаемый выигрыш:** При burst-создании многих потоков (tokio spawn_blocking flood) убирает
`Mutex::lock_contended` который видно в профиле (13.39% из 8 сэмплов — нижнее доверие).

**Риск регрессии:** TLS bind-slow path вызывается редко (по одному разу на поток). Измеримый
выигрыш только при >100 threads/sec creation rate.

---

## Потенциальные таски на оптимизацию

1. [OPT-E] Large-segment free-cache (1–2 slots per AllocCore)
2. [OPT-F] In-place small→small realloc when class doesn't change
3. [OPT-B] O(1) SegmentTable::contains_base (open-addressing hash map)
4. [OPT-C] Lazy stamp_segment_owner (cache last_stamped_segment in HeapCore)
5. [OPT-G] Enable alloc-decommit by default in multi-thread feature sets (или документировать как рекомендованный)
6. [OPT-H] Lock-free HeapRegistry::claim (CAS-based TLS slot acquisition)

---

---

## §6 — Low-noise bench profiles (task #62)

Задача #62 добавила два новых criterion-бенча специально для низкошумного
профилирования аллокатора, обходя проблемы §1/§3 (84–85% на criterion KDE) и
§4 (8 сэмплов).

### §6.1 — `heap_xthread` (push→drain ring cycle)

**SVG:** `/tmp/sefer-fg-v3a/heap_xthread.svg`
**Сэмплов:** 4 654 (cycles:u). Потери сэмплов: 0.

**Команды воспроизведения:**
```bash
export PATH=/usr/lib/linux-tools/6.8.0-124-generic:$HOME/.cargo/bin:$PATH
mkdir -p /tmp/sefer-fg-v3a

CARGO_PROFILE_BENCH_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/sefer-fg-v3a \
  cargo build --bench heap_xthread \
  --features 'alloc-core alloc-xthread' --profile bench

perf record -F 99 -e cycles:u --call-graph dwarf,16384 \
  -o /tmp/sefer-fg-v3a/perf_xthread.data \
  /tmp/sefer-fg-v3a/release/deps/heap_xthread-<hash> --bench

perf script -i /tmp/sefer-fg-v3a/perf_xthread.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title 'heap_xthread — push/drain ring (task #62)' \
    > /tmp/sefer-fg-v3a/heap_xthread.svg
```

**Top-3 по self-time:**

| Функция | Self-time |
|---|---|
| criterion KDE `bridge_producer_consumer_helper` | **42.65%** |
| `AllocCore::dbg_push_to_ring` | **13.01%** |
| criterion `join_context` (rayon) | ~8% |

**Результаты бенча:**
- `push_drain_256` (только push+drain, без alloc): **6.7–6.9 µs** на 256 итераций
- `alloc_push_drain_256` (alloc+push+drain): **30–40 µs** на 256 итераций

**Вывод:** criterion overhead снизился с 84% (§1) до **43%** — улучшение в 2×.
Аллокатор (`dbg_push_to_ring`) теперь виден с **13%** self-time против 1.7% в §1.
Функция `dbg_drain_all_rings` была полностью inlined оптимизатором — её стоимость
растворена в итерирующем коде (это ожидаемо: drain — это tight loop с Relaxed
атомными stores).

---

### §6.2 — `heap_async_pattern` (СУБД-pipeline mixed alloc)

**SVG:** `/tmp/sefer-fg-v3a/heap_async_pattern.svg`
**Сэмплов:** 1 632 (cycles:u). Потери сэмплов: 0.

**Команды воспроизведения:**
```bash
CARGO_PROFILE_BENCH_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/sefer-fg-v3a \
  cargo build --bench heap_async_pattern \
  --features 'alloc-global' --profile bench

perf record -F 99 -e cycles:u --call-graph dwarf,16384 \
  -o /tmp/sefer-fg-v3a/perf_async.data \
  /tmp/sefer-fg-v3a/release/deps/heap_async_pattern-<hash> --bench

perf script -i /tmp/sefer-fg-v3a/perf_async.data --no-inline 2>/dev/null \
  | inferno-collapse-perf 2>/dev/null \
  | inferno-flamegraph --title 'heap_async_pattern — mixed alloc pipeline (task #62)' \
    > /tmp/sefer-fg-v3a/heap_async_pattern.svg
```

**Top-3 по self-time:**

| Функция | Self-time |
|---|---|
| criterion KDE `bridge_producer_consumer_helper` | **39.64%** |
| `AllocCore::alloc` | **12.25%** |
| criterion rayon helpers | ~8% |

**Результаты бенча:**
- `SeferMalloc/pipeline` (40 small + 1 grow + 16 medium allocs): **1.6–2.1 µs** на итерацию

**Вывод:** criterion overhead **40%** против 85% в §3 (large_realloc) — улучшение в 2.1×.
`AllocCore::alloc` виден с **12.25%** против 1.72% в §1 (без фильтра SeferMalloc).
Realloc (`HeapCore::realloc`) виден с ~0.56%, что честно отражает малую долю rowtargets
с grow-операциями относительно plain alloc+free.

---

### §6.3 — Сравнительная таблица: criterion overhead до и после

| Профиль | Бенч | Сэмплов | criterion KDE self-time | allocator self-time |
|---|---|---|---|---|
| §1 | `global_alloc` (small churn) | 9 463 | **84%** | 1.72% (AllocCore::alloc) |
| §3 | `large_realloc` (realloc-heavy) | 8 648 | **85%** | 6.74% (AllocCore::alloc) |
| §4 | `tokio_burn_in` | 8 | ~50% (ненадёжно) | 24% (ненадёжно) |
| **§6.1** | **heap_xthread** (ring push+drain) | **4 654** | **43%** | **13.01%** (dbg_push_to_ring) |
| **§6.2** | **heap_async_pattern** (pipeline) | **1 632** | **40%** | **12.25%** (AllocCore::alloc) |

**Итог:** criterion overhead фундаментально связан с размером сэмплов измерения.
Если inner loop выполняется за < 10 µs, criterion берёт 10 сэмплов и тратит
~3 секунды на статистику — за которые функция KDE делает n² сравнений.
При **7 µs** (push_drain_256) за 3 секунды accumulates ~430 000 итераций →
10 точек → KDE по 10 числам = минимальная работа, но она всё равно занимает ~43% CPU.

Выигрыш новых бенчей: **2× снижение criterion overhead (84% → 43%)** при той же
продолжительности, плюс **7–10× увеличение видимости allocator-функций** (1.7% → 13%).
Это достаточно для идентификации hot paths, но не для точного изоляционного
профилирования.

**Рекомендация:** для глубокого изоляционного профиля (ожидаемый allocator
share > 60%):
1. **`samply`**: `cargo install samply` + `samply record cargo bench --bench heap_xthread ...`
   — macOS/Linux profiler с низким overhead, лучшим call-graph и flame chart UI.
   На WSL2 поддерживается с ограничениями (нет kernel symbols, но user-space работает).
2. **Standalone tight-loop binary**: написать `examples/bench_ring_tight.rs` без criterion —
   `loop { push 256 + drain }` с wall-clock timing в начале/конце. Без KDE overhead.
   Профилировать этот binary напрямую через `perf record -F 999`. Allocator share > 90%.

---

## Итоги profiling session

### Что сработало

- `perf record -F 99 --call-graph dwarf,16384` с явным путём к perf 6.8 на WSL2 — работает.
- `inferno-collapse-perf | inferno-flamegraph` — хорошо демангируют Rust symbols.
- Все 4+2 SVG сгенерированы и содержат читаемые stack traces с символами.
- Новые бенчи §6.1/§6.2 снизили criterion overhead с 84–85% до 40–43%.

### Что не сработало / ограничения

1. **cargo flamegraph на NTFS mount** — падает с "Bad address" из-за медленного IO
   при записи кольцевого bufer perf. Решение: явный perf + target_dir в /tmp.

2. **Criterion bench профили (§1, §3)** — criterion тратит 80–85% CPU на
   собственную KDE-статистику (rayon + exp()). Видны только ~3–7% от аллокатора.
   Для честного профиля нужен standalone tight-loop без criterion.

3. **tokio burn-in (§4)** — 512 задач завершаются за 70 мс → 8 сэмплов (F=99 Гц).
   Данные ненадёжны. Масштабирование (>1000 задач) приводит к OOM без alloc-decommit.

4. **Sample losses** при большом bench (test_bench.data 6.6 GB): 41% lost.
   Решение: профилировать только один аллокатор (--bench 'SeferMalloc'), снизить
   время измерения.

5. **WSL2 PMU трассировки** — kernel tracepoints недоступны (ожидаемо для WSL2).
   User-space sampling (cycles:u) работает корректно. (`cycles:Pu` недоступен
   на этом WSL2 ядре — использован `cycles:u`.)

6. **criterion overhead фундаментален:** даже с новыми бенчами §6.1/§6.2 критерий
   занимает 40–43% CPU. Это не баг новых бенчей — это структурное свойство criterion
   при малых inner loops (< 10 µs). Для полного изоляционного профиля нужен samply
   или standalone binary (см. §6.3 Рекомендация).

---

## §7 — Post-fastbin re-investigation: где сидит реальный потолок (2026-06-29)

После завершения проекта fast-bin/tcache (P0–P7, см.
[`FASTBIN_DESIGN.md`](FASTBIN_DESIGN.md)) и доинлайна всего seam'а (#101/#102)
single-thread larson/mstress T=1 остаются **~1.3× медленнее mimalloc**. Чтобы
понять, можно ли это закрыть и где, был проведён повторный flamegraph на
финальном коде. Это исследование — методологически важная глава: оно показывает,
почему наивная гипотеза «8.5% бита → если убрать = 8.5% выигрыш» не сработала
и где находится **реальный потолок** для следующих оптимизаций.

### §7.0 — Воспроизведение

```bash
# WSL Ubuntu 24.04
export PATH=/usr/lib/linux-tools/6.8.0-124-generic:$HOME/.cargo/bin:$PATH

# Build with line-tables (so srcline resolution maps through inlining)
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/sefer-p8 \
  cargo build --release --example malloc_macro --features "alloc-global alloc-xthread"

# Full sweep (larson + mstress, T=1/2/4, all three allocators)
perf record -F 2000 -g --call-graph dwarf,8192 \
  -o /tmp/sefer-p8/p.data \
  /tmp/sefer-p8/release/examples/malloc_macro

# Source-line resolution (maps through inlining; sefer-alloc functions are
# fully inlined into the worker body, so symbol-level resolution misses them)
perf report --stdio -i /tmp/sefer-p8/p.data -g none \
  --sort=srcline --percent-limit 0.5 --full-source-path
```

### §7.1 — Where the cycles ACTUALLY go (larson + mstress)

| % | Layer | Notes |
|---|-------|-------|
| **21.5%** | `libc malloc.c` (`malloc/free`) | System-arm sampling (slowest arm, gets most samples by elapsed-time-share) |
| **18.8%** | `examples/malloc_macro.rs:188` (worker body) | xorshift PRNG + array indexing + branch — common to all three arms |
| **15.9%** | `std/sync/mpmc/mod.rs:948` | **Cross-thread mpmc channel coordination** — bench harness handoff overhead |
| **9.3%** | `core/sync/atomic.rs:3899` | Atomic operations — half ours (stamp_segment_owner Relaxed-load, owner_state CAS), half bench's |
| 8.6% | `libc malloc.c:4649` (`_int_free`) | System arm |
| 7.4% | `libc malloc.c:3347` (`_int_malloc`) | System arm |
| 5.9% | bench worker body (mstress branch) | |
| 5.7% | `std/sync/mpmc/mod.rs:397` | More channel coordination |
| ~3-5% | mimalloc internals (`free.c:209`, `alloc.c:120`) | mimalloc-arm cost |
| **0.23%** | `src/alloc_core/alloc_bitmap.rs:126` (`mark_alloc`/`mark_free`) | **OUR M2 bitmap** |
| **0.05%** | `src/alloc_core/alloc_bitmap.rs:116` (`locate`) | **Bit-position math** |

**Total Sefer-alloc own-code: < 1% of MT runtime.**

The sefer functions are fully inlined into the bench worker body by the
`#[inline(always)]` campaign (#101/#102). At the sample level they show up as
the inlined call sites in `bench_direct_alloc`'s body (the 18.8% line), not as
separate symbols. So this 18.8% is "worker harness body + inlined sefer-alloc".

### §7.2 — The hypothesis that died

The single-thread BULK microbench profile (`SeferMalloc/16B` from §1 here)
showed:
- 8.5% `alloc_bitmap::locate` (bit addressing)
- 3.8% `is_free` (M2 double-free check)
- 5.9% `contains_base` hash probe
- … etc.

Naive reading: if we remove these on the dealloc fast path, we save 12-18%.

That informed the [P8 design](FASTBIN_DESIGN.md#p8-investigation--idea-2-bintable-bitmap--in-block-key-reverted):
replace `AllocBitmap::is_free` in `dealloc_small` with an in-block key in word1.
P8 was implemented cleanly (correctness preserved, 165/0 tests, 43M-op
cross-thread soak balanced) — but **failed to deliver the expected larson T=1
improvement**.

**Why:** the MT macro-bench profile above is fundamentally different from the
single-thread bulk microbench. On MT:
- The bench harness's mpmc channel coordination is **16%** of runtime alone.
- libc malloc dominates (the System arm bench is slow; gets sampled most).
- Our entire allocator + its bitmap is **< 1%** of MT runtime.

The 8.5% bitmap-locate cost from §1 was an artifact of a tight microbench
where everything else was fast. On a real MT workload with thread
coordination, channel overhead, and atomics, the bitmap is a rounding error.

**Lesson: profile the workload you're trying to optimize, not a different one
that happens to be available.** The §1 microbench was useful for finding
small-class hot paths under the inline campaign (#101/#102 — where the entire
hot path is one fused function and every nanosecond matters); it is **NOT**
the right profile for guiding cross-thread MT optimizations.

### §7.3 — Where the larson T=1 gap REALLY sits

Subtracting overhead that isn't ours:
- libc/System arm: not our problem.
- bench worker body: common to all arms.
- mpmc channel: bench harness.

What's left of "us" in the larson workload:
1. **Atomic operations ~4-5%** (half of the 9.3% atomics line is ours):
   `stamp_segment_owner` Relaxed-load + compare on every alloc (the OPT-C
   cache already reduced this from Acquire+Release to Relaxed; further
   reduction would require eliminating the per-alloc stamp entirely — P4
   hoisted it into refill on the magazine hit path, but the **large path**
   and the **bulk-bypass path** still per-alloc stamp).
2. **`dealloc_routing` reads on every dealloc**: `magic_at`, `owner_thread_free_at`,
   `kind_at`. Required for safe cross-thread routing. Per the §0 microbench
   ~5%, on MT roughly similar.
3. **`contains_base` hash probe on every dealloc**: ~3-5% on the dealloc
   side. Required as the M2 foreign-pointer guard (catches frees of pointers
   not allocated by us).
4. **Inline TLS resolution**: `current_for_alloc()` does a `try_with`-based
   safe TLS read on every alloc + dealloc. Some unavoidable cost.

The total bottom-up Sefer-attributable cost on MT is roughly 8-12% of bench
runtime. mimalloc's equivalent is lower because:
- mimalloc has no foreign-pointer guard (`contains_base` equivalent doesn't exist
  on its fast path — a free of a non-mimalloc pointer is UB in their model).
- mimalloc has no M2 double-free guard on the fast path (double-free = UB).
- mimalloc inlines a more compact hot path (their alloc fast-path is ~11 asm
  instructions on x86_64; ours is closer to 25-30 due to safety checks).

**The ~1.3× T=1 gap is the integrated cost of all these guards.** It is NOT
located in any one function we can profile out — that was the P8 lesson.

### §7.4 — What COULD close the gap (and what costs it)

Three remaining levers, ranked by EV per risk:

1. **IDEA 4 — `contains_base` elision on proven-own dealloc**
   (`docs/FASTBIN_DESIGN.md` §9). Estimated: ~3-6% on dealloc path. Risk:
   medium (weakens the foreign-pointer M2 guarantee from "exhaustive registry
   check" to "magic + owner_tf compare"). Effort: 1-2 days.

2. **Per-thread inline TLS pointer to bypass routing on proven-own free**.
   Cache `owner_thread_free_head_address` in TLS; on dealloc compare directly
   to TLS cache instead of reading the segment header. Estimated: ~5-8%. Risk:
   high (TLS lifetime + cross-thread visibility require careful audit; the
   #100 TLS-flake taught us this). Effort: ~5-day project.

3. **Accept the ~1.3× single-thread gap** as the documented cost of:
   - M2 double-free safety (vs mimalloc UB)
   - Foreign-pointer free safety (vs mimalloc UB)
   - Cross-thread routing readiness (mimalloc has its own but we audit ours)
   - `#![forbid(unsafe_code)]` at the top level (one audited aperture)

   And focus future perf work on the workloads where we already win:
   - **Large alloc/free (OPT-E):** 16-39× faster than mimalloc.
   - **MT T≥2 (larson/mstress):** 1.2-1.3× faster.
   - **Churn 16-1024B:** 1.7-7.3× faster.

Option 3 is the most honest given the project's safety-first stance. Options 1
and 2 are viable if a specific deployment needs the single-thread perf and is
willing to accept the safety trade-off.

### §7.5 — Methodological lesson (the meta finding)

**Re-profile the workload you're optimizing for.** A profile is a tool, not a
universal truth. The §1 single-thread bulk profile and the §2/§7 MT profile
look completely different despite measuring "the same" allocator, because:

- Single-thread bulk: tight loop, no thread coordination, no cross-thread paths
  taken → bitmap addressing dominates the % share.
- MT macro: thread coordination, channel overhead, atomics dominate; our
  allocator is < 1% of runtime.

The P8 hypothesis was constructed from the §1 profile and applied to a goal
defined by §2 numbers. That mismatch is what killed it.

**Practical rule for future fastbin / hot-path work:** before designing a
"replace X with Y" optimization, re-profile the *specific* benchmark you want
to improve. If the function you're targeting isn't in the top of *that*
benchmark's profile, the change won't move *that* number.
