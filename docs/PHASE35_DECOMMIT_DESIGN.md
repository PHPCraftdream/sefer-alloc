# Phase 35 — M6 decommit (return empty segments to the OS), M11-free

Дизайн-спека (пишется до реализации). Закрывает единственный честный пробел в
MALLOC_BENCH: RSS неограничен (пустые сегменты не возвращаются ОС). Под фичфлагом
`alloc-decommit` (default off — поведение по умолчанию не меняется).

## 0. Что уже есть
- OS-seam ГОТОВ: `os::decommit_pages(base, start, end)` (Win `VirtualFree
  MEM_DECOMMIT` / unix `madvise(MADV_DONTNEED)` / miri no-op) и
  `os::recommit_pages` (Win `VirtualAlloc MEM_COMMIT` / unix implicit / miri
  no-op). Контракт: вызывать на live-сегменте БЕЗ live-блоков в диапазоне.
- НЕ хватает: учёта живых блоков (`live_count`) и политики/проводки.

## 1. M11 НЕ ТРЕБУЕТ epoch (ключевой вывод — обосновать в коде)
План §2.5 проектировал M11 через crossbeam-epoch, потому что СТАРАЯ модель
интрузивного cross-thread free писала `next` ВНУТРЬ блока — freer мог писать в
декоммиченную страницу. **Variant-2 (Phase 12.6) это растворил:** cross-thread
freer НЕ дереференсит блок — он пушит `(offset|class)` в `RemoteFreeRing`,
лежащий в МЕТАДАННЫХ сегмента (страницы метаданных НИКОГДА не декоммитятся).
Доказательство безопасности decommit БЕЗ epoch:
1. Декоммитим payload сегмента ТОЛЬКО при `live_count == 0` → в декоммиченном
   диапазоне нет ни одного живого блока.
2. Поздний валидный cross-thread free невозможен при live_count==0: все блоки
   уже свободны; повторный free свободного блока = double-free.
3. `reclaim_offset` (owner-side) на стейл-записи кольца: вычисляет адрес
   `Node::deref(base,off)` (БЕЗ доступа к памяти), читает magic/kind/**bitmap
   is_free** — ВСЁ в метаданных — и для свободного блока (а при live==0 все
   свободны) делает no-op ДО любой записи в блок. Декоммиченную страницу не
   трогает.
4. `reclaim` и `decommit` оба owner-side → сериализованы на потоке-владельце; нет
   конкуренции reclaim-vs-decommit на одном сегменте.
⇒ UAF/записи в декоммиченную память не возникает. epoch/crossbeam НЕ нужен.
(Записать это рассуждение в код/доку как обоснование «почему нет M11-барьера».)

## 2. live_count (owner-only, без atomics)
Все мутации live_count происходят на владельце: own-thread alloc/free + owner-side
reclaim. Cross-thread freer НЕ трогает live_count (он пушит в кольцо; владелец
декрементит при reclaim). ⇒ обычное `u32`-поле, не atomic.
- Поле в `SegmentHeader` (новое, owner-only). Доступ field-specific (offset_of!),
  как bump (owner-only).
- Семантика: число ВЫДАННЫХ (carved-and-not-free) блоков.
  - `pop_free` (выдача): `live += 1`.
  - `carve_block` — блок, возвращаемый ВЫЗЫВАЮЩЕМУ: `live += 1`. Refill-блоки
    (идут сразу в free list): live НЕ меняется.
  - `dealloc_small` (own-thread free): `live -= 1`.
  - `reclaim_offset` (cross-thread reclaim): `live -= 1`.
  - Согласованность с bitmap: live == (всего carved) − (free). Можно
    debug_assert'ом сверять с подсчётом, но в релизе — счётчик.

## 3. Политика decommit (консервативная для старта)
В `dealloc_small`/`reclaim_offset` ПОСЛЕ декремента: если `live_count == 0` И
сегмент НЕ является текущей целью карва (`base != small_cur`; в HeapCore — не
current ни для какого класса) →
1. Декоммитить payload `[small_meta_end, SEGMENT)` (метаданные — header/page_map/
   bin_table/**alloc_bitmap**/ring — оставить committed: их читают cross-thread).
2. СБРОСИТЬ сегмент в чистый-пустой: `bump = small_meta_end`, все BinTable heads =
   FREE_LIST_NULL, page_map payload-страницы = Free, **bitmap = 0**. (Безопасно:
   live==0, никаких живых блоков.) Это делает опустевший сегмент бланком для
   переиспользования.
3. Поставить флаг `decommitted` в заголовке (новое поле/бит).
Под фичфлагом `alloc-decommit`; без него — текущее поведение (сегмент остаётся
committed, переиспользуется через free list — но free list пуст после reset...
NB: без decommit reset НЕ делаем — оставляем как сейчас).

## 4. Recommit при переиспользовании
Когда декоммиченный сегмент снова выбирается как `small_cur` / карвится:
- `carve_block`: если флаг `decommitted` стоит и собираемся писать в payload →
  `os::recommit_pages(base, small_meta_end, SEGMENT)` (Win явный commit; unix
  implicit), снять флаг. Простейшее: recommit всего payload при первом
  переиспользовании (не пер-страничный ленивый — проще и корректно).
- Альтернатива: реже переиспользуем декоммиченные — пусть alloc резервирует
  свежий сегмент, а декоммиченные остаются как RSS-возврат до явного revisit.
  Выбрать проще-корректное; recommit-on-reselect достаточно.

## 5. Тесты (обязательно; safety-sensitive)
- `tests/decommit_soak.rs` (фичфлаг alloc-decommit): устойчивый churn,
  опустошающий сегменты; ассертить, что decommit ВЫЗЫВАЕТСЯ при live→0 (счётчик
  вызовов через тест-seam) и что после переиспользования данные корректны
  (write/readback по recommit'нутым страницам). Под miri (decommit no-op) —
  ассертить БУХГАЛТЕРИЮ (live_count→0→decommit-hook вызван, reset корректен),
  не RSS.
- **miri** на decommit/recommit-цикле (bounded): нет UAF, нет доступа к «чужой»
  памяти.
- Регресс: вся сюита + race ×5 зелёные С фичфлагом и без. Особо: cross-thread
  reclaim стейл-записи в декоммиченный сегмент → bitmap no-op (добавить тест:
  декоммитить сегмент, пушнуть стейл-offset в его кольцо, drain → no-op, без
  паники/доступа).
- (Heavy gate #32) TSan под WSL на decommit-пути.

## 6. Объём/риск
Safety-sensitive (UAF на декоммиченных страницах — худший отказ). Доказательство
§1 (epoch не нужен) — несущее; реализация механическая. Гонять под miri + soak +
(в #32) TSan. Фичфlag default-off: дефолт не рискует.

## 7. Вне области
- Агрессивные политики (немедленный decommit vs отложенный/по таймеру) — сначала
  консервативная (decommit при live==0 non-current), тюнинг — позже по RSS-замеру.
- RSS-метрика в макробенче — отдельно (нужен платформенный probe); после этого
  обновить MALLOC_BENCH RSS-секцию (сейчас N/A).
