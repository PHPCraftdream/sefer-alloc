# Дизайн интегрированного спайка B3 (`ph3c-b3s`) — пер-класс битовые под-листы 4 KiB

Дата: 2026-10-02. Спайк шага 1 (#2109), НЕ для merge. База A = `main` c584a3ae + патч S
(`git diff | sha256sum` = `94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee`,
копия — `docs/perf/PH3C_B3S_BASE_S.patch`). Правила и пределы —
`docs/design/2026-10-02-adr-addendum-ph3c-escalation.md` §4 (hot Ir/EstCycles ≤ 1.02;
refill/flush EstCycles ≤ 1.10 И Ir ≤ 1.20; оракул ≥ 95% refill из free-структуры).

## 1. Текущая модель free-set (база A)

- Свободное множество Small — интрузивный LIFO-список на (сегмент, класс):
  `BinTable.head` + `next` в теле блока (`Node::write_next`/`read_next`).
- `AllocBitmap` (32 KiB в метаданных сегмента, бит = слот 16 Б; 1 = FREE) — только
  O(1) double-free guard; отдельная от списка структура.
- Пишут тело: `reclaim_sidecar_record` (retire), `dealloc_small`, `flush_run`
  (магазинный splice). Читают тело: `pop_free`, `try_drain_freelist_batch`.
- Carve — bump по `align_up(bump, block_size)` без ограничения на границы 4 KiB:
  классы перемешаны внутри 4 KiB листа (route `ClassLeaves` переходит в mixed-span).

## 2. Целевая модель B3

- Свободное множество = биты существующего `AllocBitmap` (бит = слот СВОБОДЕН) —
  семантика та же, отдельного параллельного битмапа нет.
- Под-лист = 4 KiB = 256 слотов = ровно 4 слова по 64 бита `AllocBitmap`
  (`alloc_bitmap_off + leaf*32 .. +32`); 1024 под-листов на сегмент.
- Per-класс структура непустых под-листов `B3Free` (новая область в метаданных
  сегмента, owner-only, без атомиков):
  - `mask: [[u64; 16]; SMALL_CLASS_COUNT]` — бит leaf = «в под-листе есть ≥1
    свободный слот этого класса» (49 × 128 Б = 6272 Б);
  - `leaf_class: [u8; 1024]` — привязка под-листа к классу (0 = virgin, иначе
    class+1), пишется при первом карвинге;
  - `cursor: [u8; SMALL_CLASS_COUNT]` — стартовое слово маски (16 слов, цикл).
  Итого ≈ 7.4 KiB на сегмент, `small_meta_end` округляется до страницы — рост
  commit ≤ 1 страницы на сегмент (для спайка не гейтится; RSS — гейт полной
  интеграции).
- pop: по маске класса (tzcnt, курсор) → под-лист → 4 слова `AllocBitmap` (tzcnt)
  → слот; каждая выдача `mark_alloc` + сброс бита маски при опустении под-листа.
- retire/dealloc/flush: `mark_free` + OR бита маски (в flush — накопление слова в
  регистре, один store на слово). Тело блока не читается и не пишется нигде.

## 3. Инвариант «под-лист одно-классный» и carve

Ключевое решение: **под-лист привязывается к классу при первом карвинге и не
меняется до reset сегмента** (`leaf_class[]` — owner-авторитет; route `ClassLeaves`
продолжает работать как remote-авторитет per-гранулы — они согласованы, т.к. в
лист попадают гранулы только одного класса).

Правило carve (единый хелпер для `carve_block`, `prepare_current_carve`,
`try_carve_batch`):

1. `start = align_up(bump, block_size)`;
2. если `leaf_class[leaf(start)]` привязан к другому классу → `start =
   align_up((leaf+1)*4096, block_size)`, повторить;
3. если `block_size ≤ 4096` и блок пересёк бы конец под-листа → `start =
   align_up((leaf+1)*4096, block_size)`, повторить (шаг 2 не даст зайти в чужой
   лист);
4. блоки > 4 KiB — спан из нескольких под-листов естественно: привязывается
   только лист начала (все свободные биты спана лежат в слоте начала).

Следствие: хвост под-листа (до 4 KiB − block_size) не используется (фрагментация,
worst-case 49 × 4 KiB ≈ 4.8% сегмента — учтено в аддендуме). Absolute
block-выравнивание сохраняется (проверки `off % block_size` в dealloc/reclaim не
меняются). Virgin-skip не нарушается: невыполненные страницы читаются нулями =
«все заняты/не-начало», лист без свободных битов не попадает в маску.

## 4. Места замены (как PG-2, но free-list целиком)

| Место | Было | Стало |
|---|---|---|
| `reclaim_sidecar_record` (alloc_core_small_reclaim.rs) | `Node::deref`+`write_next`+`set_head`+`mark_free` | `mark_free` + `B3Free::set_leaf` |
| `dealloc_small` (dealloc.rs) | `write_next`+`set_head`+`mark_free` | `mark_free` + `set_leaf` |
| `flush_run` (alloc_core_small_magazine.rs) | run-splice через `write_next`, `set_head` 1 раз | пословно: `mark_free` + накопление OR слова маски, один store на слово; guard'ы (`payload_start`, `bump`, `is_free`) сохранены per-block |
| `pop_free` / `try_drain_freelist_batch` (alloc_core_small_impl.rs) | walk цепи `read_next` | tzcnt по маске и словам под-листа; probe-фаза OOM-атомарности сохранена (prepare до мутаций) |
| carve (3 точки) | свободный bump | правило §3 |

- `BinTable` остаётся в layout, но головы Small всегда `FREE_LIST_NULL` (пишутся
  только reset-ами). Directory `publish_empty`/`publish_nonempty` сохранены —
  переходы теперь по «маска класса пуста/непуста».
- `SegmentBitmap` дополнен пословными примитивами (`word`/`clear_word_bit`,
  через seam `Node`), byte-пути не тронуты.
- `B3Free` — новый seam-модуль `src/alloc_core/segment/segment_header/b3_free.rs`
  (один export), инициализация нулями в reserve-пути рядом с `AllocBitmap::init_in_place`,
  сброс в decommit-reset.

## 5. Прочие пути

- zeroed/virgin: зероинг после выдачи — не затрагивается.
- realloc: move = alloc + copy + dealloc — dealloc-путь уже B3.
- Large/Primordial-структуры, sidecar pending/scan (патч S), magazine_bitmap,
  tcache — не меняются. Cross-thread free публикует pending-бит, как раньше.
- Классы-спаны > 4 KiB — реальный путь (§3.4), отказа нет.
- Тесты интрузивного слота (`regression_freelist_next_validation` и сходные,
  ходящие по `read_next` цепям): под B3 неприменимы к free-set (цепей нет);
  переписываются на битовый оракул (`is_free`/маска) либо помечаются
  «неприменимо, потому что интрузивной цепи больше не существует» — по месту.

## 6. Границы спайка

- Не для merge; решающий вердикт — интегрированный iai-гейт §4.1 аддендума.
- RSS/фрагментация/remote-merge-плечо/larson — вне спайка (гейты полной
  интеграции); рост метаданных ≈ 7.4 KiB/сегмент задокументирован выше.
- `hardened`-валидаторы цепей: адаптируются или фиксируются как неприменимые.
