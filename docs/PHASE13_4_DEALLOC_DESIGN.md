# Phase 13.4 — dealloc набело: O(1) double-free guard + two-list

Дизайн-спека (пишется до реализации; реализацию ведёт под-агент по этому
документу). Закрывает регрессию O(N²) (бывш. #41) и поставляет two-list (13.4).

## 0. Проблема (подтверждена counterfactual'ом)

`AllocCore::dealloc_small` (src/alloc_core/alloc_core.rs) на каждом own-thread
free зовёт `free_list_contains` — **O(длины free-list)** walk (M2 double-free
guard). На фазе освобождения бенча (1024 блока одного класса в один сегмент)
free-list растёт 0→1024 → **O(N²)** ≈ 524k разыменований с cache-miss'ами =
~1.9 ms (vs mimalloc ~11 µs). Доказано: `free_list_contains → return false` ⇒
16B churn **1.9 ms → 16.5 µs** (~115×). Тот же инлайн-walk сидит в
`AllocCore::reclaim_offset` (cross-thread reclaim).

`free_list_contains` — это Phase-8 placeholder (его комментарий: «Phase 9 заменит
дешёвым cookie-guard'ом»). Phase 12.1 завёл malloc-лицо на `dealloc_small` с этим
guard'ом → реинтродукция O(N²).

## 1. Решение: O(1) точный double-free guard через per-segment alloc-bitmap

**Почему bitmap, а не canary/энкодинг:** M2 требует ТОЧНОГО «double-free = no-op,
никогда не коррупция». Canary в блоке даёт ложные срабатывания (данные юзера ==
canary) → не точно. Незащищённый double-free создаёт self-loop в free-list →
двойная выдача блока → коррупция. Bitmap — точный, O(1), стандартный (TLSF и др.).

### 1.1 Структура `AllocBitmap`

Новая metadata-область в КАЖДОМ small/primordial сегменте: 1 бит на
MIN_BLOCK-слот сегмента.

- `FOOTPRINT = SEGMENT / MIN_BLOCK / 8` байт. Для 4 МиБ/16 = 32768 байт = 32 КиБ
  = 8 страниц. **Вычислять из констант**, не хардкодить.
- Семантика бита: `1` = блок СВОБОДЕН (лежит в каком-то free-list этого
  сегмента: `free` ИЛИ `local_free`); `0` = занят / не-старт-блока.
- Индекс: `bit_index = (ptr - base) >> MIN_BLOCK_SHIFT`. Старты блоков всегда
  MIN_BLOCK-выровнены (carve выравнивает bump к block_size ≥ MIN_BLOCK, а
  block_size кратен MIN_BLOCK) → бит уникален на блок. Покрываем ВЕСЬ сегмент
  (включая метаданные) — биты метаданных просто никогда не трогаются (там нет
  стартов блоков); это убирает арифметику вычитания payload-начала.
- Инициализация: все нули (всё «занято/не-блок»). `init_in_place` через
  `Node::write_u8` (как PageMap/BinTable).
- API (всё O(1), через `node` seam, БЕЗ atomics — single-writer: сегмент пишет
  только его owner; cross-thread free идёт через ring, дренится owner'ом):
  - `is_free(off: u32) -> bool` — тест бита.
  - `mark_free(off: u32)` — установить бит (вызывается при пуше в free-list).
  - `mark_alloc(off: u32)` — снять бит (вызывается при выдаче блока).
- Файл: `src/alloc_core/alloc_bitmap.rs` (один экспорт `AllocBitmap`), как
  PageMap/BinTable. `mod.rs` — только реэкспорт.

### 1.2 Раскладка (`segment_header::Layout`)

Вставить bitmap в metadata-цепочку. ВНИМАНИЕ к порядку (ring и registry-offset
зависят от предыдущих). Предлагаемый порядок: header → page_map → bin_table →
**alloc_bitmap** → remote_ring → (primordial: registry). Обновить:
- `Layout::alloc_bitmap_off()` (новый) = `align_up_const(bin_table_off() +
  BinTable::FOOTPRINT, 8)` (или 2× если two-list расширит BinTable — см. §2;
  учесть это СРАЗУ, чтобы не сдвигать раскладку дважды).
- `Layout::remote_ring_off()` = после bitmap.
- `SegmentMeta::alloc_bitmap()` view.
- Compile-time asserts (`small_meta_end + PAGE <= SEGMENT`,
  `primordial_meta_end + PAGE <= SEGMENT`) — должны держаться (+8 страниц из
  1024). Bootstrap (`bootstrap.rs`) — carve+init bitmap, обновить `meta_pages`.

### 1.3 Интеграция в alloc/dealloc

- `dealloc_small(base, ptr, class)`: заменить `free_list_contains` на
  `bitmap.is_free(off)` → если true, no-op return (double-free, M2). Иначе
  `bitmap.mark_free(off)` + пуш в free-list.
- `pop_free` / выдача блока: `bitmap.mark_alloc(off)` перед возвратом.
- `carve_block` (свежий блок): бит уже 0 (init) — выдаём как занятый; на всякий
  случай НЕ трогаем (он 0). refill-блоки пушатся через `dealloc_small` →
  `mark_free` корректно.
- `reclaim_offset` (cross-thread reclaim): тот же guard — заменить инлайн-walk на
  `is_free`/`mark_free`. Owner — единственный писатель bitmap'а (reclaim бежит на
  owner'е), atomics не нужны.
- **Удалить** `free_list_contains` (и инлайн-копию walk в `reclaim_offset`).

## 2. two-list (`free` + `local_free`) — слой локальности (13.4)

mimalloc: own-thread free пушит в `local_free`; alloc попит из `free`; при
опустошении `free` переносит `local_free`→`free` (collect). Снижает ветвления и
отделяет own/remote очереди. cross-thread (ring) — третья очередь, уже есть.

- BinTable: второй массив u32-голов `local_free` (FOOTPRINT × 2 = 320 Б). Учесть
  в раскладке §1.2 СРАЗУ.
- own-thread `dealloc_small`: `mark_free` + пуш в `local_free` (НЕ в `free`).
- `pop_free`: если `free` пуст — `free_head = local_free_head; local_free_head =
  NULL` (O(1) transplant, порядок не важен), затем поп из `free`.
- double-free guard (bitmap) одинаково покрывает оба списка — `is_free` истинно,
  если блок в любом из них. Поэтому two-list не усложняет guard.

**Честность (план §3.4):** two-list принять, ТОЛЬКО если бенч покажет выигрыш.
Поэтому реализовать в ДВА коммита:
- **13.4a (bitmap guard)** — сам по себе убивает регрессию (ожидаем ~16 µs),
  M2-точный. Замерить.
- **13.4b (two-list)** — поверх; замерить дельту; оставить, если помогает.

Раскладку bitmap считать сразу с учётом удвоенного BinTable (чтобы 13.4b не
сдвигал метаданные повторно).

## 3. Регресс-гейт (обязателен)

Тест `tests/dealloc_sublinear.rs` (или в существующем): освободить N и 2N блоков
одного класса, измерить работу (счётчик node-reads ИЛИ грубое время) — рост
должен быть ~линейным, не квадратичным. Counterfactual: КРАСНЕЕТ на старом O(N)
walk. Без гейта O(N²) вернётся молча.

Дополнительно: убедиться, что есть unit на M2 double-free (блок, освобождённый
дважды, не выдаётся двум вызывающим / free-list не зацикливается). Если нет —
добавить; он должен пройти и со старым, и с новым guard'ом (инвариант сохранён).

## 4. Верификация (zero-trust, руками)

- Вся сюита зелёная: `alloc-core`, `alloc-global`, `alloc-global alloc-xthread`.
- race_repro / race_norecycle / global_alloc_mt ×5 — без флака.
- `clippy --all-targets` (те же фичи) — 0 новых.
- Бенч `global_alloc` 16/64/256/1024B: 16B возвращается к ~16 µs (от 1.9 ms).
- miri на bitmap-инварианте (маленький bounded), если дёшево.
- Коммит на границе фазы (13.4a отдельно, 13.4b отдельно).

## 5. Вне области (отдельные задачи)

- **Per-class bump cursors** (истинная page-dedication, как mimalloc): убрал бы
  §13 в корне (page_map стал бы надёжен → carry-class в ring не нужен, #40
  растворяется). Больше и рискованнее — отдельная будущая задача, НЕ здесь.
- #40 (латентный §13 на drain) — после/в свете 13.4 (тот же dealloc/drain-код).
