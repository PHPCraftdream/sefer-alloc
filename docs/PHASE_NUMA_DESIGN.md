# NUMA-aware путь в sefer-alloc — дизайн-спека

Исследовательский документ (пишется до реализации). Закрывает пробел #7
«нет осознания NUMA-узлов» в MALLOC_BENCH. Под фичфлагом `numa-aware`
(default off — поведение по умолчанию не меняется).

---

## §0. Что уже есть — NUMA-релевантные точки

### OS-seam (`src/alloc_core/os.rs`)

Файл содержит единственный confined-`unsafe` блок резервирования памяти
через `mmap`/`VirtualAlloc`. Сейчас НИГДЕ не используются NUMA-специфичные
флаги или вызовы:

- **Linux**: `mmap` вызывается с `MAP_PRIVATE | MAP_ANON`, без `mbind(2)` и
  без `set_mempolicy(2)`. Страницы распределяются по политике процесса по
  умолчанию (обычно «local» на NUMA-системе, но не гарантировано).
- **Windows**: `VirtualAlloc(..., MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE)` —
  нет `VirtualAllocExNuma`, узел не указывается.
- **macOS**: публичного NUMA API нет (Apple Silicon — SoC без классической
  NUMA-топологии, публичных syscall для `mbind`/`set_mempolicy` нет).
  **Статус: `unsupported`.** Фича компилируется как no-op на Darwin.

`decommit_pages`/`recommit_pages` уже существуют и корректно обёрнуты.
NUMA-вызовы должны встроиться рядом — в отдельный файл
`src/alloc_core/numa.rs`, аналогичным образом `cfg`-гейтованный.

### Сегментный заголовок (`src/alloc_core/segment_header.rs`)

`SegmentHeader` — `#[repr(C)]`, копия, чисто безопасный код. Сейчас
содержит: `magic`, `kind`, `segment_id`, `bump`, `large_size/align`,
`reservation`, `reservation_len`, `owner_thread_free`, `owner_state`,
`next_abandoned`, `live_count`, `decommitted`.

**Ключевое ограничение (из compile-time assert):**
```
const _: () = assert!(size_of::<SegmentHeader>() <= PAGE);
const _: () = assert!(Layout::page_map_off() == PAGE);
```
Заголовок должен вписаться в одну страницу (4 KiB). Сейчас он ~96 байт;
поле `node_id: u32` добавит 4 байта — ≪ PAGE. Безопасно.

### AllocCore (`src/alloc_core/alloc_core.rs`)

Точки резервирования сегментов:
- `reserve_small_segment` (строки 997–1034): вызывает `Segment::reserve(SEGMENT)`,
  затем инициализирует метаданные и выбирает сегмент как `small_cur`.
- `alloc_large` (строки 957–993): вызывает `Segment::reserve(needed)` для
  каждой крупной аллокации.

**Оба места — точки встройки NUMA-политики**. После `Segment::reserve` и до
записи заголовка нужно вызвать `numa::bind_segment(base, len, node_id)`.

Выбор «текущего» сегмента на пути `alloc_small`:
1. `pop_free` из `small_cur`
2. `find_segment_with_free` — сканирует все сегменты
3. `carve_block_with_refill` → `carve_block` (bump из `small_cur`)
4. `reserve_small_segment` (новый сегмент)

NUMA-предпочтение — на шаге 2 и 4: сначала ищем сегмент с `node_id == my_node`,
потом зарезервируем новый на `my_node`.

### Heap (`src/heap/heap.rs`)

`Heap` — per-thread, создаётся через TLS-ленивую инициализацию (см. `heap_tls`
и `SeferMalloc`). Привязка к потоку — через TLS, а не к CPU или NUMA-узлу.

Если поток мигрирует между узлами — `Heap` не знает об этом. Сегменты
остаются «принадлежащими» тому узлу, с которого создавались. Это MVP-допущение
(стратегия «игнорируем» — см. §4).

### Фичи (`Cargo.toml`)

Образец для нового флага — `alloc-decommit`:
```toml
alloc-decommit = ["alloc-core"]
```
Аналогично:
```toml
# NUMA-aware segment reservation: при выделении нового сегмента запрашивает
# страницы у NUMA-узла, на котором работает вызывающий поток
# (Linux: mbind(2), Windows: VirtualAllocExNuma).
# macOS: no-op (нет публичного NUMA API). Default OFF.
# Requires `alloc-core`.
numa-aware = ["alloc-core"]
```

---

## §1. API цели — NUMA OS-интерфейс

### Linux

```c
// Привязать уже замапленные страницы к узлу:
int mbind(void *addr, unsigned long len,
          int mode,                      // MPOL_BIND = 2
          const unsigned long *nodemask,
          unsigned long maxnode,
          unsigned int flags);           // 0 для жёсткой привязки
```

- `mode = MPOL_BIND` — страницы обязаны приходить с конкретного узла.
- `mode = MPOL_PREFERRED` — предпочесть узел, допустить другой при нехватке.
- `nodemask` — битовая маска узлов; для одного узла `n`: `1UL << n`.
- Вызвать ПОСЛЕ `mmap`, ДО первого обращения к страницам (тогда страницы
  «цепляются» к нужному узлу при page-fault'е).

Опционально — `set_mempolicy(2)` на поток (меняет политику для всех
последующих `mmap` в потоке), но это глобально для потока и опасно.
Лучше per-mapping `mbind`.

Источник информации о топологии (без внешних зависимостей):
```
/sys/devices/system/node/node<N>/cpumap   — маска CPU, принадлежащих узлу N
/sys/devices/system/node/online           — список active-узлов
/proc/self/status → Cpus_allowed          — допустимые CPU для процесса
/proc/self/task/<tid>/stat → поле[38]     — текущий CPU потока
```
Для MVP достаточно: прочитать текущий CPU (`sched_getcpu(3)` — оболочка над
`getcpu(2)`), потом найти узел по `/sys/devices/system/node/node*/cpumap`.

### Windows

```c
LPVOID VirtualAllocExNuma(
    HANDLE hProcess,     // GetCurrentProcess()
    LPVOID lpAddress,    // NULL — ОС выбирает адрес
    SIZE_T dwSize,
    DWORD  flAllocationType,  // MEM_RESERVE | MEM_COMMIT
    DWORD  flProtect,         // PAGE_READWRITE
    DWORD  nndPreferred       // номер узла
);
```

Обнаружение узлов:
```c
UCHAR  node_count;
GetNumaHighestNodeNumber(&node_count);  // число NUMA-узлов - 1

PROCESSOR_NUMBER proc;
GetCurrentProcessorNumberEx(&proc);     // текущий логический процессор

USHORT node_number;
GetNumaProcessorNodeEx(&proc, &node_number);  // узел этого процессора
```

**Внимание**: на Windows нет эквивалента `mbind` для уже зарезервированной
памяти. Поэтому на Windows нет разделения резервирования и привязки — нужно
вызывать `VirtualAllocExNuma` вместо `VirtualAlloc`. Это меняет точку
встройки: нужно передать `node_id` в `reserve_aligned`.

### macOS

Публичного NUMA API нет. Apple Silicon — архитектура UMA (Unified Memory
Architecture) без физической NUMA-асимметрии в публичной модели. Фича
`numa-aware` на Darwin компилируется как no-op: детекция возвращает узел 0,
`bind_segment` — no-op.

---

## §2. Точки встройки в наш код

### Новый файл `src/alloc_core/numa.rs`

По образцу `src/alloc_core/os.rs`:

```rust
//! NUMA-seam: определение текущего NUMA-узла и привязка сегмента к узлу.
//! Confined-`unsafe` модуль (единственное место для NUMA-syscalls).
//! Gated под `#[cfg(all(feature = "numa-aware", not(miri)))]`.
#![allow(unsafe_code)]  // аналог os.rs

/// Нет узла / фича выключена. Sentinel.
pub const NO_NODE: u32 = u32::MAX;

/// Определить NUMA-узел текущего потока.
/// Возвращает `NO_NODE`, если API недоступен или фича выключена.
pub fn current_node() -> u32 { ... }

/// Привязать `[base, base+len)` к NUMA-узлу `node`.
/// Вызывать ПОСЛЕ mmap/VirtualAlloc, до первого обращения к страницам.
/// На Windows: не применимо (привязка происходит при резервировании);
/// no-op при `node == NO_NODE` или на macOS.
pub fn bind_segment(base: *mut u8, len: usize, node: u32) { ... }

/// Версия `reserve_aligned` с NUMA-предпочтением (для Windows,
/// где нужен `VirtualAllocExNuma` вместо `VirtualAlloc`).
/// На не-Windows: резервирует обычным способом, затем вызывает `bind_segment`.
pub fn reserve_aligned_on_node(usable: usize, node: u32)
    -> Option<(NonNull<u8>, NonNull<u8>, usize)> { ... }
```

Все `unsafe`-блоки — с `// SAFETY:` комментарием. Полная аналогия `os.rs`.

### `src/alloc_core/segment_header.rs` — новое поле

```rust
#[repr(C)]
pub(crate) struct SegmentHeader {
    // ... существующие поля ...
    pub live_count: u32,
    pub decommitted: u32,
    /// NUMA-узел, на котором были выделены страницы этого сегмента.
    /// `NO_NODE` (u32::MAX) означает «неизвестен / не используется».
    /// Присутствует в КАЖДОЙ сборке (layout стабилен); читается/используется
    /// только под `#[cfg(feature = "numa-aware")]`.
    pub node_id: u32,
}
```

Аналогично `live_count`/`decommitted` — поле присутствует всегда, чтобы
layout заголовка был стабилен вне зависимости от набора фичей. Доступ — через
`offset_of!` по той же дисциплине, что `bump_of`/`set_bump`.

**Проверка размера**: добавление `u32` не нарушит assert `size_of::<SegmentHeader>() <= PAGE` — текущий размер ~96 байт, останется ~100 байт ≪ 4096.

### `src/alloc_core/alloc_core.rs` — изменение `reserve_small_segment`

```rust
fn reserve_small_segment(&mut self) -> Option<*mut u8> {
    // Новый код под numa-aware:
    #[cfg(feature = "numa-aware")]
    let my_node = numa::current_node();

    // Сначала проверить: есть ли уже пустой (decommitted) сегмент
    // на нужном узле? (переиспользование вместо нового резервирования)

    #[cfg(feature = "numa-aware")]
    let segment = numa::reserve_aligned_on_node(SEGMENT, my_node)?;
    #[cfg(not(feature = "numa-aware"))]
    let segment = Segment::reserve(SEGMENT)?;

    // ... остальной код без изменений ...

    // Записать node_id в заголовок:
    #[cfg(feature = "numa-aware")]
    {
        let off = core::mem::offset_of!(SegmentHeader, node_id);
        Node::write_u32(Node::offset(base, off) as *mut u32, my_node);
    }
    // ...
}
```

### NUMA-предпочтение в `find_segment_with_free`

```rust
pub(crate) fn find_segment_with_free(&self, class_idx: usize) -> Option<*mut u8> {
    #[cfg(feature = "numa-aware")]
    let my_node = numa::current_node();

    let mut fallback = None;
    for base in self.table.bases() {
        if !matches!(SegmentHeader::kind_at(base), ...) { continue; }
        // ...ring drain...

        #[cfg(feature = "numa-aware")]
        {
            let seg_node = segment_node_id(base);
            if seg_node != my_node && seg_node != numa::NO_NODE {
                // Не наш узел — запомним как fallback, продолжим поиск
                if fallback.is_none() { /* store */ }
                continue;
            }
        }
        let bt = SegmentMeta::new(base).bin_table();
        if bt.head(class_idx) != FREE_LIST_NULL {
            return Some(base);  // локальный узел — берём сразу
        }
    }
    // fallback: сегмент другого узла, если локального нет
    fallback
}
```

---

## §3. Per-node сегментные пулы

**Нужны ли отдельные BinTable / free-list по узлам?** — Нет, для MVP.

Обоснование:
- `Heap` уже per-thread. В типичной рабочей нагрузке СУБД поток живёт на
  одном ядре и одном узле (особенно с `pinning`-фичей, которая уже есть).
- Сегменты уже per-heap (не разделяются между потоками в steady-state, только
  через cross-thread free).
- Разделение BinTable по узлам означало бы удвоение/утроение метаданных
  внутри каждого сегмента и усложнение `find_segment_with_free` — без выгоды
  при `pin`.

**Достаточно**: тег `node_id` в заголовке + предпочтение локальных сегментов
в `find_segment_with_free` + выделение новых сегментов на `my_node`.

Если нагрузка покажет, что межузловые сегменты преобладают (например, при
heap-balancing или adoption), — тогда рассмотреть разделение, НО это фаза N+1.

---

## §4. Миграция потока

**Проблема**: если ОС переселила поток на другой NUMA-узел, `current_node()`
вернёт новый узел, а все существующие сегменты этого heap'а будут со старым
`node_id`. Аллокации будут по-прежнему выдаваться из «дальних» сегментов.

**Стратегии:**

| Вариант | Описание | Сложность | Выбор |
|---------|----------|-----------|-------|
| (a) Игнорируем | Поток работает со старыми сегментами; `current_node()` влияет только на НОВЫЕ резервирования | Минимальная | **MVP** |
| (b) Периодическая переподписка | На каждый `alloc` проверять `current_node()` vs `segment node_id`; при расхождении — мигрировать | Высокая, без выигрыша | Нет |
| (c) Pinning пользователем | Пин потока через фичу `pinning` (`core_affinity`) — тогда миграции нет | Минимальная (уже есть) | **Рекомендация** |

**Решение для MVP**: стратегия (a) + рекомендация в документации использовать
`pinning`-фичу. Это честно: NUMA-выигрыш проявляется именно там, где поток
пинован к ядру узла. Без пиннинга NUMA — best-effort оптимизация, не гарантия.

**Синергия с `pinning`**: фича `pinning` уже подключает `core_affinity`
(безопасная обёртка над `sched_setaffinity`/`SetThreadAffinityMask`).
Документировать: `numa-aware + pinning` — рекомендуемая комбинация.
Только `numa-aware` без `pinning` даёт best-effort (помогает при низкой
миграции, не помогает при высокой).

---

## §5. Тестирование без реального железа

### QEMU fake-NUMA (Linux)

Запустить Linux VM с поддельной NUMA-топологией:

```sh
qemu-system-x86_64 \
  -m 2G \
  -smp 4,sockets=2,cores=2,threads=1 \
  -numa node,nodeid=0,cpus=0-1,mem=1G \
  -numa node,nodeid=1,cpus=2-3,mem=1G \
  -numa dist,src=0,dst=1,val=20 \
  ...
```

Внутри VM:
- `numactl --hardware` показывает 2 узла.
- `numactl --cpunodebind=0 ./sefer_test` — прогнать тест на узле 0.
- Проверить, что наш код запрашивает узел 0 для потока 0, узел 1 для потока 1.
- `/proc/<pid>/maps` + `numastat -m` — проверить, откуда физически пришли
  страницы.

Альтернатива без QEMU — загрузочный параметр ядра `numa=fake=4` (4 виртуальных
NUMA-узла на одном физическом сокете). Не требует VM.

### Тест `tests/numa_seam.rs`

Юнит-тест для `src/alloc_core/numa.rs`:

```rust
#[test]
#[cfg(feature = "numa-aware")]
fn current_node_returns_valid_value() {
    let node = numa::current_node();
    // Либо NO_NODE (unsupported), либо < 64 (разумная граница)
    assert!(node == numa::NO_NODE || node < 64);
}

#[test]
#[cfg(all(feature = "numa-aware", target_os = "linux"))]
fn bind_segment_does_not_panic() {
    // Резервировать сегмент, привязать к узлу 0, освободить.
    // Проверяет, что mbind не падает (EINVAL etc.)
    ...
}
```

### Тест `tests/numa_alloc.rs`

Интеграционный тест под `alloc-global + alloc-xthread + numa-aware`:

```rust
#[test]
#[cfg(all(feature = "numa-aware", feature = "alloc-global"))]
fn alloc_from_local_node() {
    // Запустить 2 потока, пинованных к разным NUMA-узлам.
    // Каждый выделяет N блоков.
    // Проверить: segment.node_id == thread_numa_node для большинства сегментов.
    ...
}
```

### ВАЖНО — честность об ограничениях

QEMU / `numa=fake` проверяют КОРРЕКТНОСТЬ привязки (верный `mbind`/
`VirtualAllocExNuma` вызван, `node_id` записан правильно). Они НЕ верифицируют
latency-выигрыш: на одном физическом сокете все «узлы» имеют одинаковую
задержку доступа.

**Цифра прироста производительности требует реального 2-сокетного железа:**
- AWS c5n.metal, i3.metal (Xeon, 2 сокета)
- AWS r6g.metal (Graviton 2, несколько NUMA-доменов)
- Dual-socket dev box

Это ограничение MVP — зафиксировать в MALLOC_BENCH и в рамках этапа E
реализации.

---

## §6. Риск и область

### Safety

Новый confined-`unsafe` блок `src/alloc_core/numa.rs`:
- `mbind` syscall: не изменяет данных сегмента, только политику выделения
  физических страниц. Основной риск: передать неверный `addr`/`len` или узел.
  Защита: вызывать ТОЛЬКО на live-сегменте сразу после `mmap`, до любого
  использования; `len` берётся из `Segment::len()` (кратно `SEGMENT`);
  `node` — из `current_node()`, который ограничен системным `node_count`.
- `VirtualAllocExNuma`: та же семантика, что `VirtualAlloc`, + параметр узла.
  Если узел недоступен — возвращает NULL (OOM path, штатно обрабатывается).
- `// SAFETY:` на каждый `unsafe`-блок — обязательно.

### Регресс

- Feature default OFF (`numa-aware` без `= default`).
- Без флага: byte-for-byte старое поведение. `Segment::reserve` не изменяется.
- Новое поле `SegmentHeader::node_id` инициализируется в `NO_NODE` в
  конструкторах `small()` и `large()` — layout стабилен.
- Compile-time assert `size_of::<SegmentHeader>() <= PAGE` по-прежнему
  выполняется.

### Совместимость с `alloc-decommit`

`decommit_empty_segment` сбрасывает `live_count`, `decommitted`, `bump`.
Поле `node_id` НЕ сбрасывается — оно отражает физическую привязку сегмента,
которая не меняется при decommit/recommit. После recommit сегмент возвращается
к тому же узлу.

### Объём

| Артефакт | Ориентировочно |
|----------|---------------|
| `src/alloc_core/numa.rs` | 250–400 строк |
| `src/alloc_core/segment_header.rs` | +8 строк (поле + конструкторы) |
| `src/alloc_core/alloc_core.rs` | +30–50 строк (`#[cfg(feature)]`-блоки) |
| `src/alloc_core/mod.rs` | +1 строка (`pub(crate) mod numa;`) |
| `tests/numa_seam.rs` | ~60 строк |
| `tests/numa_alloc.rs` | ~120 строк |
| `Cargo.toml` | +6 строк |

---

## §7. Вне области

- **Tuning политик** (MPOL_INTERLEAVE vs MPOL_BIND vs MPOL_PREFERRED): только
  MPOL_BIND для MVP. Interleave — для HPC-нагрузок, preferred — мягче; по
  результатам замера.
- **NUMA-aware pinning runner**: синергия с `pinning`-фичей (уже есть
  `core_affinity`); API-расширение для явной привязки потока к NUMA-узлу +
  шарду — отдельная задача.
- **Latency-asymmetry замер**: невозможен без реального 2-сокетного железа.
  Заглушка в MALLOC_BENCH «NUMA: opt-in, верифицировано под QEMU, latency
  требует hardware».
- **Per-node free-list шардирование** внутри сегмента: не нужно для MVP;
  рассмотреть, если данные покажут высокое межузловое «загрязнение» при heap
  adoption.
- **Large-block NUMA**: `alloc_large` создаёт dedicated-сегмент; привязка
  там полезна, но менее критична (большие блоки реже). Включить на том же
  этапе A (единственный `reserve_aligned_on_node`-вызов).

---

## §8. Шаги реализации

### Phase A — `src/alloc_core/numa.rs` (OS-seam)

Новый confined-`unsafe` модуль с детекцией топологии и `bind_segment` /
`reserve_aligned_on_node`. Покрыт юнит-тестами в `tests/numa_seam.rs`.

Платформы:
- `#[cfg(all(target_os = "linux", not(miri)))]`: `mbind` + `sched_getcpu`
- `#[cfg(all(windows, not(miri)))]`: `VirtualAllocExNuma` +
  `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`
- `#[cfg(target_os = "macos")]` + `#[cfg(miri)]`: no-op, возвращает `NO_NODE`

### Phase B — `SegmentHeader.node_id` (layout)

Новое поле `u32` с `NO_NODE` в обоих конструкторах. Compile-time assert, что
`size_of::<SegmentHeader>() <= PAGE` — всё ещё выполняется. Field-specific
accessor `node_id_of` / `set_node_id` через `offset_of!`.

### Phase C — NUMA-выбор в `reserve_small_segment` + `find_segment_with_free`

Встройка `current_node()` и `reserve_aligned_on_node()` в
`reserve_small_segment`. NUMA-предпочтение в `find_segment_with_free` —
сначала сегменты с `node_id == my_node`, потом остальные. Аналогично
`alloc_large` для крупных аллокаций.

### Phase D — QEMU-тест корректности

`tests/numa_alloc.rs`: запускать только при `SEFER_NUMA_TEST=1` (guard env
var), т.к. требует реальной NUMA-топологии или QEMU. Документировать в README
(`numactl --hardware` prerequisite).

### Phase E — MALLOC_BENCH обновление

Добавить раздел «NUMA» с честным описанием: opt-in под `numa-aware`,
корректность верифицирована под QEMU/fake-NUMA, latency-выигрыш измерим
только на реальном multi-socket железе. RSS-метрика не затрагивается
(NUMA-привязка не меняет количество выделяемых сегментов).

---

## Сводная таблица точек кода

| Файл | Действие |
|------|----------|
| `src/alloc_core/numa.rs` | Новый confined-`unsafe` NUMA-seam |
| `src/alloc_core/os.rs` | Без изменений (только читаем) |
| `src/alloc_core/segment_header.rs` | + поле `node_id: u32`, accessor'ы |
| `src/alloc_core/alloc_core.rs` | + `#[cfg(numa-aware)]` в `reserve_small_segment`, `alloc_large`, `find_segment_with_free` |
| `src/alloc_core/mod.rs` | + `pub(crate) mod numa;` |
| `src/heap/heap.rs` | Без изменений (NUMA-логика ниже, в AllocCore) |
| `Cargo.toml` | + фича `numa-aware = ["alloc-core"]` |
| `tests/numa_seam.rs` | Новый тест OS-seam |
| `tests/numa_alloc.rs` | Новый интеграционный тест |
| `docs/MALLOC_BENCH.md` | + раздел «NUMA» |
