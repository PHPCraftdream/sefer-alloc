# B4: байт состояния гранулы (вариант «в») — отчёт об измерении

- **База A:** снапшот `897463e6` (каталог `.rush/wsl/sb_rss_base`).
- **Дерево B:** текущее worktree `b4-sbyte` — байт состояния `(c+1)|s<<6`
  (`LIVE=0` / `MAG=1` / `FREE=2`), полностью заменивший оба битмапа
  (`AllocBitmap` + `MagazineBitmap`) как авторитетную запись состояния
  small-сегмента. Оба прежних bitmap-региона удалены из metadata layout
(F6: −64 КиБ), а не сохранены ради совместимости.

После ревью дерево прошло цикл исправлений **F1–F15** (раздел 10): битмапы
и их физические регионы **удалены из раскладки** (F6, метаданные −64 КиБ),
оракул переписан не-тавтологично (F11), ёмкостная таблица по классам
уточнена (F13). Замеры этого отчёта (`sb2-*`) сняты на дереве **после**
фиксов; identity `git diff d965255a | sha256sum =
e8ad0445f6ecfa737a78b5a9810be5478355d90104faaba5f54315daec1ae8e6`
(хвост `_raw_b4_state_byte_tree2.log`).
- **Инструмент:** callgrind (marginal Ir), кэш-модель (EstCycles), RSS-пробник
  `r29_4_segment_state_reconciliation_gate`, оракул байта состояния
  (`tests/state_byte_oracle.rs`, наблюдения через `dbg_state_byte_state_for`,
  вне замеряемой сборки; F11).
- **Сырые логи:** suite, thrash64, RSS и oracle сохранены в `docs/perf/_raw_b4_state_byte_*.log`; `scripts/b4_state_byte_table.mjs` разбирает suite A (`_base.log`), suite B (`_tree2.log`), отдельные thrash64/RSS логи и oracle RESULT.
- **Сводная таблица:** `docs/perf/B4_STATE_BYTE_summary.csv`;
  генератор/верификатор: `scripts/b4_state_byte_table.mjs`.

---

## 1. Резюме

Вариант (в) реализован **полностью**: один байт владельческого состояния на
`MIN_BLOCK`-гранулу (16 Б), размещённый в метаданных сегмента по
`Layout::state_byte_off()`, кодировка `byte = (class + 1) | (state << 6)`,
состояния `LIVE=0` (выдан вызывающему, включая remote-pending), `MAG=1`
(резидент tcache-журнала), `FREE=2` (на bin/free-list). Байт **заменил оба
битмапа**, а их физические регионы удалены из layout F6 — метаданные уменьшены
на 64 КиБ.

**Вердикт: NO-GO по предрегистрированному порогу** — из-за единственного
RSS-гейта (+12.5 % > +5 %); владелец принял известное численное отклонение
2026-10-20 без изменения порога. RSS-гейт остаётся **NO-GO**, не PASS. **Все 8
скоростных Ir-гейтов пройдены с запасом.** Относительно первого прогона
free подорожал на +2 Ir (−14 → −12): цена безусловной проверки выравнивания
F1 — осознанный компромисс ревью. Подробности: раздел 10.

### Таблица судей

Таблица ниже — полный результат `node scripts/b4_state_byte_table.mjs` (27 строк; все числовые строки сверены с suite A/B, отдельными thrash64/RSS логами, oracle RESULT и capacity-границами):

## Таблица судей (сгенерировано scripts/b4_state_byte_table.mjs)

Источник: docs/perf/B4_STATE_BYTE_summary.csv, сверено с сырыми логами
docs/perf/_raw_b4_state_byte_*.log (база A = снапшот 897463e6, дерево B = байт состояния).
Примечание: base rt-alloc отсутствует в sb-base-aoc, использовано значение ao (55).

| Судья | Метрика | A (base) | B (байт) | Δ | Δ% | Порог | Вердикт | Сверка с логом |
|---|---|---|---|---|---|---|---|---|
| ao | alloc Ir/op | 55 | 50 | -5 | -9.09% | <= -3 | GO | OK |
| ao | free Ir/op | 105 | 93 | -12 | -11.43% | <= -5 | GO | OK |
| ao | EstCycles alloc | 62.0 | 61.4 | -0.6 | -0.97% | — | INFO | OK |
| ao | EstCycles free | 110.2 | 100.4 | -9.8 | -8.89% | — | INFO | OK |
| const | alloc rt Ir/op (A=ao fallback) | 55 | 50 | -5 | -9.09% | не хуже +1 | GO | OK |
| const | free rt Ir/op | 105 | 93 | -12 | -11.43% | не хуже +1 | GO | OK |
| const | alloc c16 Ir/op | 26 | 22 | -4 | -15.38% | не хуже +1 | GO | OK |
| const | free c16 Ir/op | 66 | 54 | -12 | -18.18% | не хуже +1 | GO | OK |
| const | alloc c64 Ir/op | 26 | 22 | -4 | -15.38% | не хуже +1 | GO | OK |
| const | free c64 Ir/op | 66 | 54 | -12 | -18.18% | не хуже +1 | GO | OK |
| batch | alloc Ir/call (256/round) | 126 | 114 | -12 | -9.52% | <= +3 | GO | OK |
| batch | free Ir/call (256/round) | 159 | 141 | -18 | -11.32% | <= +3 | GO | OK |
| batch_const | alloc16 Ir/call (256/round) | 106 | 94 | -12 | -11.32% | <= +3 | GO | OK |
| batch_const | free16 Ir/call (256/round) | 128 | 110 | -18 | -14.06% | <= +3 | GO | OK |
| batchl | alloc Ir/call (258/round) | 131 | 120 | -11 | -8.40% | <= +3 | GO | OK |
| batchl | free Ir/call (258/round) | 172 | 155 | -17 | -9.88% | <= +3 | GO | OK |
| larson | Ir/op (T=1) | 302 | 283 | -19 | -6.30% | <= -1% | GO | OK |
| mstress | Ir/op (T=1) | 422 | 384 | -38 | -9.00% | <= 0 | GO | OK |
| thrash64 | alloc Ir/op | 26 | 22 | -4 | -15.38% | <= +3 | GO | OK |
| thrash64 | free Ir/op | 103 | 91 | -12 | -11.65% | <= +3 | GO | OK |
| thrash64 | EstCycles alloc | 26.0 | 22.0 | -4.0 | -15.38% | <= +3% | GO | OK |
| thrash64 | EstCycles free | 112.6 | 100.2 | -12.4 | -11.01% | <= +3% | GO | OK |
| rss | post_total_committed KiB | 32768 | 36864 | 4096 | +12.50% | <= +5% | NO-GO (accepted by owner 2026-10-20) | OK |
| rss | small_pooled segments | 6 | 7 | 1 | +16.67% | — | INFO | OK |
| rss | small_pooled committed KiB | 24576 | 28672 | 4096 | +16.67% | — | INFO | OK |
| oracle | state_byte mismatches /N (denominator = loop N) | — | 0/100000 | 0 mismatches | 100.0000% matching | >= 95% matching | GO | OK |
| capacity | 16B blocks/segment base→pre-F6→post-F6 | 257536 | 257536→241152→245248 | exact aligned strides | — | — | INFO | OK (exact capacity assertions) |

Согласованность CSV ↔ сырые логи: OK (27 строк, 0 расхождений).

### Точная потеря ёмкости (F13)

`node scripts/b4_state_byte_capacity.mjs` enumerates actual class tables for default,
`medium-classes`, and `medium-classes-wide`. Counts include only complete blocks:
`first = align_up(meta_end, size)` and `floor((SEGMENT-first)/size)` when
`first+size <= SEGMENT`; overrunning extents are excluded. Metadata ends are
73,728 (base), 335,872 (pre-F6), 270,336 (post-F6). Exact 16-byte counts:
257,536 → 241,152 → 245,248. Script assertions pin 320 KiB to 11/10/11,
256 KiB to 15/14/14, and 512 KiB to 7 at base. Complete tables are in
`B4_STATE_BYTE_capacity_tables.md`.


Замечание к rt-строкам `const`: в логе базы `sb-base-aoc` строка
`w_alloc (mode=rt)` **не напечаталась** (особенность прогона); rt-значение
базы взято из ao-прогона (`w_alloc` = 55) — тот же бенчмарк-код, отмечено в
таблице суффиксом `(A=ao fallback)`.

---

`docs/perf/B4_STATE_BYTE_capacity_tables.md` is the full generated feature matrix;
`src/alloc_core/segment/bitmap/state_byte.rs` supplies the contract assertions.

Кодировка: `byte = (class + 1) | (state << 6)`, `SMALL_CLASS_COUNT ≤ 63`
(const-assert в `src/alloc_core/segment/bitmap/state_byte.rs`), состояние
занимает биты 6–7, `state == 3` не производится.

- **И1 (ноль = никогда не выдавался / метаданные-интерьер):** любой
  не-стартовый гранул и любой никогда не выданный гранул читается `0`.
  Эквивалентно: байт ненулевой ⇔ гранул — 16-Б-выровненный старт блока,
  выданного хотя бы раз с момента init. Любая запись состояния пишет
  ненулевые биты класса ⇒ `byte == 0 ⇔ (byte & 0x3F) == 0`.
- **И2 (точность LIVE):** `state == LIVE` ровно на гранулах, выданных
  вызывающему прямо сейчас (включая блоки в remote-ingress-конвейере), и
  нигде больше. Транзиентные окна — строго **внутри операции, до её
  возврата**: refill-блоки, уже переданные в журнал, удерживаются LIVE до
  `finish_magazine_refill`; блоки `alloc_batch` остаются MAG, пока лежат в
  буфере `out`, и становятся LIVE при выдаче из него.
- **И3 (точность MAG):** `state == MAG` ровно на гранулах, резидентных в
  tcache-журнале, и нигде больше (см. транзиентные окна И2).
- **И4 (точность FREE):** `state == FREE` ровно на гранулах на bin/free-list
  сегмента, и нигде больше; staged-блоки `dealloc_batch` остаются LIVE до
  фактического flush в bin.
- **И5 (точность переходов):** каждый переход фиксируется владельцем до
  возврата из операции, его вызвавшей: выдача — MAG→LIVE, push в журнал —
  LIVE→MAG, flush/свободный-push — LIVE|MAG→FREE, reclaim/pop/carve —
  FREE→LIVE (через выдачу) либо LIVE при первой нарезке.

|---|---|

**Бит 7 / item 246:** StateByte использует бит 7 для FREE; remote `ClassLeaves` сохраняет class+1 и бит 7 для epoch candidate. Item 246(a) остаётся открытым по ABA residual.

MAG отклоняется до попадания, LIVE после reissue не различает stale publication.
Это не утверждение о необозримости: safe `dbg_*` observers есть.

---

## 3. Таблица переходов (по текущему дереву)

Строки указаны по текущему дереву `b4-sbyte`.

| Место (файл:строка, функция) | Переход / роль |
|---|---|
| `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:381`, `valid_free_node` (hardened) | чтение: гранул обязан быть `FREE` своего класса (валидация free-list) |
| `…alloc_core_small_impl.rs:454–455`, `pop_free` | запись: `FREE → LIVE` при выдаче блока из free-list |
| `…alloc_core_small_impl.rs:621–622`, `try_drain_freelist_batch` | запись: `FREE → LIVE` per-block при batch-выдаче (drain) |
| `…alloc_core_small_impl.rs:801–802`, `carve_block` | запись: `0 → LIVE` при первой нарезке блока из bump |
| `…alloc_core_small_impl.rs:990–991`, `try_carve_batch` | запись: `0 → LIVE` per-block при batch-нарезке |
| `src/alloc_core/small/alloc_core_small/dealloc.rs:103–109`, `dealloc_small` | чтение-гард: уже-`FREE` ⇒ no-op (точный O(1) double-free guard) |
| `…dealloc.rs:127–131`, `dealloc_small` | запись: `LIVE|MAG → FREE` при push на bin (был `mark_free`) |
| `src/alloc_core/small/alloc_core_small_magazine.rs:657–662`, `flush_run` (из `flush_class_internal`) | чтение-гард: класс-поля + не-`FREE`; запись: принятый блок `LIVE|MAG → FREE` |
| `src/alloc_core/small/alloc_core_small_reclaim.rs:51–56`, `reclaim_sidecar_record` | чтение-гары: `FREE` ⇒ skip; `MAG` ⇒ reject (fastbin-gated) |
| `…alloc_core_small_reclaim.rs:73–76`, `reclaim_sidecar_record` | запись: `→ FREE` при reclaim-записи sidecar-блока |
| `src/registry/heap_core/alloc/hot.rs:50–52`, `clear_magazine_on_issue` | запись: `MAG → LIVE` (set_state, класс сохранён) при выдаче из журнала |
| `src/registry/heap_core/alloc/hot.rs:91–96`, `finish_magazine_refill` | запись: `n−1` оставшихся в журнале блоков `→ MAG` |
| `src/registry/heap_core/free/dealloc_own_base.rs:327–331`, `small_free_guard` | чтение-гары: байт обязан быть `encode(c, LIVE)` (были `is_in_magazine`/`is_free` гард-предварители) |
| `…dealloc_own_base.rs:462–467`, `dealloc_own_thread_with_base` (push-ветка) | запись: `LIVE → MAG` при push в журнал (был `mark_magazine`) |
| `…dealloc_own_base.rs:561–563`, `dealloc_own_thread_with_base` (overflow-ветка после flush) | запись: `→ MAG` у заново запаркованного блока |
| `src/registry/heap_core/free/dealloc_batch.rs:347–349`, `dealloc_batch_small` | запись: `→ MAG` при magazine-first батч-парковке |
| `src/alloc_core/small/alloc_core_small/find_segment/class_cursor.rs:213–218`, `pop_from_class_cursor` | debug_assert: canonical-discovery-хит не должен быть `MAG` |
| `src/alloc_core/small/alloc_core_small_diag.rs:128–129`, `dbg_is_free_for` | чтение: `byte != 0 && (byte>>6)==FREE` для диагностики |
| `…alloc_core_small_diag.rs:314`, `dbg_state_bytes_for` | дамп-хук байтов состояния |
| `src/alloc_core/alloc_core/bootstrap.rs:184`, bootstrap init | `StateByte::init_in_place()` (обнуление массива) |
| `src/alloc_core/small/alloc_core_small/reserve.rs:442`, reserve | `init_in_place()` при резервировании сегмента |
| `src/alloc_core/small/alloc_core_small_pool/decommit.rs:~254`, `decommit_empty_segment_impl` | `Node::zero` только used interval state bytes при decommit-reset |

---

## 4. Удалённые чтения/записи битмапов и что осталось

**Удалено (в базе — `sb_rss_base`, в дереве — больше не вызывается):**

| Операция | Где было (база) | Чем заменено |
|---|---|---|
| `alloc_bitmap().is_free(off)` (валидация free-node) | `alloc_core_small_impl.rs:380` | `state_byte().read == encode(c, FREE)` |
| `magazine_bitmap().is_in_magazine(off)` (там же) | `alloc_core_small_impl.rs:381` | исключено из конъюнкта (точность И3/И4 не требует) |
| `alloc_bitmap().mark_alloc(head_off)` (pop) | `alloc_core_small_impl.rs:456` | запись `FREE → LIVE` |
| `bm.mark_alloc` per-block (drain) | `alloc_core_small_impl.rs:623` | запись `FREE → LIVE` per-block |
| `bm.is_free` (double-free guard) | `dealloc.rs:103` | O(1)-гард по байту |
| `bm.mark_free` (push на bin) | `dealloc.rs:121` | запись `LIVE|MAG → FREE` |
| `magazine_bitmap().is_in_magazine` (canonical-discovery assert) | `class_cursor.rs:213` | чтение `state>>6 != MAG` |
| `bm.is_free` / `mark_free` (flush_run) | `alloc_core_small_magazine.rs:653/667` | гард по `class`-полям + запись `→ FREE` |
| `bitmap.is_free` / `is_in_magazine` / `mark_free` (reclaim) | `alloc_core_small_reclaim.rs:49/53/69` | гард по `state>>6` + запись `→ FREE` |
| `mark_magazine` (issue/refill: `finish_magazine_refill`) | `hot.rs:86` | запись `MAG` для `n−1` оставшихся |
| `mark_magazine` (batch push) | `dealloc_batch.rs:346` | запись `→ MAG` |
| `mark_magazine` (own push + overflow) | `dealloc_own_base.rs:461/570` | запись `→ MAG` |
| `is_in_magazine` + `is_free` (own free guard-предварители) | `dealloc_own_base.rs:312/318` | один гард `byte == encode(c, LIVE)` |
| clear pre-pass `×FLUSH_N` перед overflow-flush | `dealloc_own_base.rs:526–…` (цикл по `slots[0..FLUSH_N]`) | удалён целиком — flush_run пишет `FREE` сам |
| magazine-bitmap pre-pass tcache teardown | `state/tcache_flush.rs:102` | удалён |
| `divides` + `small_class_matches` class-identity прелюдии | `dealloc_own_base.rs:418` и др. | заменены сравнением `class`-поля байта (`sb & 0x3F == c+1`) |

**Осталось (после F6):** регионы `AllocBitmap`/`MagazineBitmap`/`SegmentBitmap`
**удалены из раскладки метаданных целиком** (−64 КиБ/сегмент мёртвых бит),
вместе с их init-вызовами и dbg-хуками; их место под байт состояния занял
регион бывшего `alloc_bitmap`. Диагностические dump-хуки байта состояния
остались (`alloc_core_small_diag.rs`); методы битмапов удалены как код.

---

## 5. Тесты

**Новые:**

- `tests/state_byte_transitions.rs` — 14 тестов: каждый переход таблицы §3
  через публичную поверхность (carve, pop, drain, batch, dealloc, flush,
  reclaim, decommit-reset, double-free no-op, guard'ы).
- `tests/state_byte_heap_magazine.rs` — 10 тестов: magazine-контур
  (push/flush/overflow/refill, `clear_magazine_on_issue`,
  `finish_magazine_refill`, FREE_PARK_CAP-ветки).
- `tests/state_byte_property.rs` — 3 свойства (LCG-драйвер, 64/64/96
  операций): модель «байт ↔ состояние» — случайная последовательность
  alloc/free/flush сравнивается с референс-моделью состояний.
- `tests/state_byte_oracle.rs` — read-only observer oracle: mismatches
  `0/100000`, 100.0000% matching; duplicate free at fixed intervals
  counterfactually tests that accepting MAG causes a mismatch.

**Перезаписанные** (по санкции владельца):

- `tests/bitmap_word_boundaries.rs`,
  `tests/regression_virgin_bitmap_skip.rs`,
  `tests/regression_dealloc_metadata_region_guard.rs` — тесты байтов
  битмапа потеряли предмет: битмапы больше не поддерживаются, их биты не
  меняются в ходе работы аллокатора. Тесты переписаны на наблюдаемое
  поведение/байт состояния (границы слов → границы гранул байта; virgin-skip
  → поведение выдачи на никогда-не-выданных гранулах; guard региона
  метаданных → реакция байта/гардов на offsets из региона метаданных).

---

## 6. Мутанты

| Мутант | Изменение | Catcher | Исход | После отката |
|---|---|---|---|---|
| M1 | carve-batch → FREE | `ts1_batch_push…` | красный (abort) | зелёный, файл побайтово восстановлен |
| M2 | pop_free → FREE | `ts1_free_list_pop…` | красный (101) | зелёный |
| M3 | drain → FREE | `ts1_drain…` | красный (101) | зелёный |
| M4 | dealloc_small FREE→LIVE | `ts1_dealloc_small…` | красный (abort, hardened) | зелёный |
| M5 | flush_run FREE→LIVE | `ts1_flush_run…` | красный (101) | зелёный |
| M6 | reclaim FREE→LIVE | `ts1_reclaim…` | красный (101) | зелёный |
| M7 | decommit init удалён | `ts1_decommit_reset…` | красный (101) | зелёный |
| M8 | clear_magazine_on_issue LIVE удалён | `ts1_own_free_push…` | красный (abort) | зелёный |
| M12 | encode class+1→class | `ts1_carve…` | красный (101) | зелёный |

**Первоначально не ловились через публичную поверхность — закрыто в F3:**

- **M9** push MAG→LIVE и **M9b** (overflow push double-free), **M10** (batch
  push duplicate), **M11** (refill retained stale double-free) теперь
  ловятся: существующим LIVE-точностным гардом и новыми тестами
  `ts1_reject_overflow_push_double_free`, `ts1_batch_push_duplicate_exactly_once`,
  `ts1_refill_retained_stale_double_free` — каждый мутант RED 0xC0000409,
  откат зелёный (rc 0). Неверный комментарий в старом catcher-тесте
  исправлен; debug_assert из `class_cursor` (после pop — тавтология,
  старый вариант ловил 0 тестов) перенесён в `pop_free` (assert MAG-головы
  до LIVE-store).

Отдельный **потенциальный продуктовый finding** (по наблюдению автора
catcher-теста): cross-heap free magazine-резидентного блока выдаёт блок
**дважды** — 5/5 прогонов, детерминированно. Требуется ревью; поведение,
вероятно, предсуществующее: MAG-reject был под feature `fastbin` и в базе.
Не блокирует этот отчёт (вне скоупа байта), но должно быть отработано
отдельной задачей.

---

## 7. RSS-разбор (причина NO-GO)

- `post_total_committed`: **32768 → 36864 КиБ (+12.5 %)** при пороге ≤ +5 % —
  единственный непройденный гейт. **RSS-гейт: NO-GO по порогу ≤ +5 %, принят
  владельцем 2026-10-20** (порог не менялся задним числом).
- `post_state_small_pooled`: **6 сегментов / 24576 КиБ → 7 сегментов /
  28672 КиБ**. Потеря ёмкости зависит от класса; точные counts и rounded
  deltas приведены скриптом F13 для base, pre-F6 и post-F6, без общей
  процентной оценки.
  `empty_orphan`) в логах идентичны базе.
- Причина скачка 6→7 — **квантование по сегменту**: потеря ёмкости
  переступила границу, где 6 сегментов перестали вмещать рабочий набор
  пробника, и аллокатор зарезервировал седьмой (+4096 КиБ целиком).
- **После F6 (−64 КиБ мёртвых битмапов из метаданных) квантование 6→7 НЕ
  изменилось**: потеря ёмкости определяется 256-КиБ картой байта состояния,
  а не мёртвыми регионами. «Спасать» вердикт запрещено; порог не пересматривался.
- Вывод: **NO-GO по предрегистрированному порогу** при GO по всем 8
  Ir-гейтам; NO-GO принят владельцем. Если RSS-гейт критичен, варианты —
  увеличить сегмент, ввести half-byte/упаковку по классам (решение владельца,
  не в этом отчёте).

---

## 8. Ограничения и риски

1. **Один прогон на лог** — каждый замер снят callgrind'ом однократно;
   callgrind детерминирован (тотальный счётчик инструкций), но расхождение
   версий инструмента/окружения между A и B не исключено полностью.
2. **arzh** — анализ выполнен по фиксированным логам; wall-clock замеры
   запрещены и не проводились.
3. **(Снято F6):** мёртвые регионы битмапов удалены из раскладки
   (−64 КиБ/сегмент); резидуальный риск — только карта байта состояния
   (+256 КиБ/сегмент), см. §7.
4. **Пин/commit +256 КиБ/сегмент** — байт должен быть закоммичен вместе с
   метаданными сегмента; на pin-cost это видно в RSS-гейте.
5. **CI-риск:** `iai` A/B-компаратор и Miri-клетки зоны должны подтвердить
   числа локальных логов; Miri-клетки зоны — обязательны (байт
   задействован в путях, идущих под Miri).
6. Пробел покрытия M9/M10/M11 закрыт (F3); потенциальный double-issue при
   cross-heap free (§6) — задокументирован, требует ревью.

---

## 9. Что прогнать на CI

1. `npm run iai` A/B — база `897463e6` против этого дерева (подтверждение
   marginal Ir по всем точкам таблицы судей).
2. Miri-клетки зоны (loom/miri-скрипты репозитория) — байт состояния на
   путях flush/reclaim/teardown.
3. Полный тест-набор (`cargo test`), включая новые
   `state_byte_*` и перезаписанные bitmap/regression-тесты.

---

## 10. Исправления по ревью (F1–F15)

Ссылки file:line — по текущему дереву `b4-sbyte`. Для каждого F: суть,
место фикса, тест, мутант (RED rc при мутанте, зелёный rc после отката).

### F1 — безусловный reject выравнивания в small_free_guard
- **Суть:** misaligned-оффсет отклоняется безусловно: `off & (MIN_BLOCK-1) != 0`
  — общий guard для scalar/batch-путей; на hot path это одна OR-ветка.
- **Фикс:** `src/registry/heap_core/free/dealloc_own_base.rs:~351–376` (`small_free_guard`).
- **Тест:** `tests/state_byte_misaligned_free.rs`.
- **Мутант:** «убрать alignment-проверку» → RED 0xC0000409; откат → 0.
- **Цена:** +2 Ir free относительно первого прогона (−14 → −12) — принято.

### F2 — state-only FREE-отказ (class-agnostic)
- **Суть:** double-free guard по состоянию без зависимости от класса —
  эквивалент базы. `src/alloc_core/small/alloc_core_small/dealloc.rs:~101–118`.
- **Тест:** `tests/double_free_guard.rs::both_wrong_class_free_orders_are_noops` (порядки 32↔16).
- **Мутант:** class-зависимый guard → RED 101; откат → 0.
- **Аудит class-зависимых мест:**

| Место | Результат аудита |
|---|---|
| reclaim | state-only, эквивалент базы |
| `valid_free_node` (hardened) | строго сильнее: валидатор ходит только по своим спискам |
| flush_run | class-field конъюнкт сильнее; входы всегда своего класса |
| R1-01 / курсор | без изменений / эквивалент |

### F3 — закрытие пробела M9/M9b/M10/M11
- **Суть:** M9 ловится существующим LIVE-точностным гардом; исправлен неверный
  комментарий в catcher-тесте; добавлены тесты M9b/M10/M11; debug_assert из
  `class_cursor` (после pop — тавтология, старый вариант ловил 0 тестов)
  перенесён в `pop_free` (assert MAG-головы до LIVE-store).
- **Тесты:** `ts1_reject_overflow_push_double_free` (M9b),
  `ts1_batch_push_duplicate_exactly_once` (M10),
  `ts1_refill_retained_stale_double_free` (M11) — `tests/state_byte_heap_magazine.rs`.
- **Мутанты:** current MAG-store mutant each RED (native Windows abort rc 3221226505); green baseline is 12/12 tests. Logs: `sb_final_mutant_{scalar,batch,refill}.log` and `sb_final_magazine.log`.

### F4 — семантика счётчика reject'ов
- **Суть:** `RejectNoOpSilent` для wrong-class aligned ∧ misaligned (оба без
  инкремента, как базы `divides`-return); state-отказы — с инкрементом.
- **Фикс:** `src/registry/heap_core/free/dealloc_own_base.rs` (guard-перечисление).
- **Тест:** `tests/state_byte_misaligned_free.rs::f4_wrong_class_free_counter_semantics`.
- **Мутант:** → RED 0xC0000409; откат → 0.

### F5 — безопасность dbg-хука `dbg_state_bytes_for`
- **Суть:** хук внесён в whitelist-реестр `verify-dbg-hook-safety.mjs`.
- **Статус:** PASS 257/33/128 (после F6-удалений и добавления PURE_OBSERVERS).

### F6 — удаление битмапов и их регионов
- **Суть:** удалены `AllocBitmap`/`MagazineBitmap`/`SegmentBitmap`, их регионы
  из раскладки (метаданные −64 КиБ: state byte занял место alloc_bitmap),
  init-вызовы и dbg-хуки битмапов (+ их UNSAFE_HOOKS-записи).
- **Фикс:** `src/alloc_core/segment/bitmap/{alloc,magazine,segment}_bitmap.rs` (удалены),
  `src/alloc_core/segment/segment_header/segment_header_layout.rs`, `bitmap/mod.rs`.
- **Тесты:** `regression_dealloc_metadata_region_guard` / `regression_virgin_bitmap_skip`
  переведены на state-byte.

### F7 — decommit обнуляет только фактически использованный диапазон
- **Суть:** обнуление `[payload_start>>4, bump_до_сброса>>4)` через `Node::zero`.
- **Фикс:** `src/alloc_core/small/alloc_core_small_pool/decommit.rs` (`decommit_empty_segment_impl`).
- **Доказательства по путям повторного entering:** fresh reserve/bootstrap —
  OS demand-zero; retain-decommit — scoped; release→reserve — через fresh;
  pooled reuse — без reset и не нужен; Large→Small reuse — отсутствует.
- **Мутант:** no-op zero → `ts1_decommit_reset_zeroes_state_bytes` RED 101; откат → 0.

### F9 — README-инвентарь
- **Суть:** tier-2 count 98 → **97** (фактический grep после F6), строка
  таблицы `alloc_core_small_diag` 5→4.
- **Тест:** `readme_unsafe_inventory_counts_match_reality` GREEN.

### F10 — форматирование/импорты
- **Суть:** импорты приведены к edition 2021; пофайловый
  `rustfmt --edition 2021 --check` по 40 файлам — чисто
  (`cargo fmt --all -- --check` падает OS 206 на этом хосте).

### F11 — удаление горячих счётчиков + read-only хук
- **Суть:** 4 горячих счётчика STATE_BYTE_* удалены (iai собирает
  `production bench-internals internals` — мерил бы 4 lock xadd); новый
  read-only safe hook `dbg_state_byte_state_for` (internals, PURE_OBSERVERS).
- **Фикс:** `src/alloc_core/small/alloc_core_small_diag.rs:~408`,
  `src/alloc_core/alloc_core/counters.rs`.
- **Оракул переписан не-тавтологично:** знаменатель = N цикла,
  числитель = наблюдения байта через `dbg_state_byte_state_for`;
  `RESULT state_byte_oracle mismatches=N/M (p% matching)`.
- **Mutant verification:** `small_free_guard` accepting `LIVE | MAG` was
  rejected by the oracle (one test, nonzero exit); raw logs:
  `sb_final_oracle_mutant.log` and green `sb_final_oracle.log`.

### F12 — production-варианты тестов
- **Суть:** production-варианты misaligned/wrong-class тестов (без hardened,
  не-вакуумные: count + никогда-не-выдан + байты), оба порядка F2,
  overflow-MAG/M10/M11.
- **Файл:** `tests/state_byte_misaligned_free.rs` (production-блоки).

### F13 — честная таблица потери ёмкости по классам
- **Суть:** «единые −4.7 %» заменены таблицей по классам (§1,
  `scripts/b4_state_byte_capacity.mjs`: точные strides для 49/55/58 классов,
  включая base / pre-F6 / post-F6; процент потери зависит от класса и
  строится по абсолютному выравниванию, не по payload/class.
- **Identity перед замерами:** `git diff d965255a | sha256sum =
  e8ad0445f6ecfa737a78b5a9810be5478355d90104faaba5f54315daec1ae8e6`
  (хвост `_raw_b4_state_byte_tree2.log`).

### F14 — property-финал и чистка
- **Суть:** property-финал переписан (проверяет нулевую карту до прежнего
  high-water после reset; `assert!(any_recycled)`); устаревшие
  комментарии/литералы обновлены (reserve.rs, hot.rs, diag_probes.rs,
  magazine, dealloc.rs, тестовые литералы 270336/389120); мёртвый код удалён
  (4×`small_class_matches`, `SmallSidecar::class_matches`).
- **Тест:** `tests/state_byte_property.rs`; **мутант** no-op → RED 101; откат → 0.

### F15 — точный guard `dealloc_small`
- **Суть:** guard усилен до `off & 15 == 0 ∧ byte == encode(c, LIVE)`
  (все вызывающие передают LIVE — mem_impl dealloc_at_base, scan.rs refill).
- **Фикс:** `src/alloc_core/small/alloc_core_small/dealloc.rs:~100–130`.
- **Тест:** `tests/dealloc_substrate_exact_guard.rs` (3 теста).
- **Мутанты:** RED 101; откат → 0.

## Evidence registry (resolved)

The source-scope-stale rows were reverified through the EV-1 runner on CI
(gate-run, run 39420852160, E0 PASS, head `801d84bc`): R-Ph3a-native, R-Ph3b,
R4c-05, R4c-06 (preserve KNOWN-RED), R4c-07, R4c-08 (new recipe
`docs/evidence/reverify-recipes/R4c-08.json`). The R4b-*/R4c-02/R4c-04 rows
dropped out of the stale set after the rebase onto `main` `2730d02a` (the
`tls_heap.rs` change they tracked is not part of this branch).

---

## 11. Цикл G1–G4

Пост-ревью верификационный цикл. Замеры G-suite сняты на **retained
two-branch дереве**; санитизированные сырые логи:
`docs/perf/_raw_b4_state_byte_sb3_{ao,aoc,batch,batchc,larson,mstress,thrash64}.log`
(приватные пути вычищены; hash-строки repo-relative, сохранены дословно).

### G1 — A/B: two-branch против fused

- **A (retained, two-branch):** ao alloc 50 / free 93 Ir/op.
- **B (fused):** alloc 50 / free 95 Ir/op.
- **Вердикт:** fused **отклонён**, two-branch **сохранён**. Причина отклонения:
  +2 Ir на free у fused варианта.

### Per-measurement diff hash

Замеренные прогоны ao/batch/batchc/larson/batch — identity
`d4d7f72260bb10320590b2923770994191588f026e61d6cff1b429b7c3c15bd3`;
прогоны mstress/thrash64 — identity
`5cd5074d3fba00cbb5dc0a7b868d5ebf9313af6747fc23e95b4c60654c5b3a28`.
Hash различается между группами из-за удаления строк-комментариев между
прогонами; семантика источника не менялась, тесты зелёные в обоих случаях.

### G2 — семантика counter'а reject'ов

Zero-below-bump — **молча** (без инкремента), past-bump — **с инкрементом**.
Тесты: `tests/state_byte_misaligned_free.rs:202-228`. Mutant rc: первый
попыткой мутант провалился как задумано — interior assertion left=1 right=0;
в финальном протокольном прогоне rc ненадёжен (STATUS_STACK_BUFFER_OVERRUN) —
документировано честно, без пересказа в зелёный.

### G3 — точный FREE-класс

`debug_assert_eq` точного FREE-класса в
`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:453-457`.

### G4 — верификация

- `verify-*`: все rc 0, **кроме** `verify-evidence-registry` rc 1 — ожидаемо:
  15 stale-строк отложены в EV-1 на CI.
- `verify-structure-limits` rc 0 (после удаления строк-комментариев).

### Clippy matrix (rc)

| Профиль | rc |
|---|---|
| production,internals | 0 |
| all-features | 0 |
| hardened,medium-classes,internals | 0 |
| production-only | 101 — pre-existing negative-probe example, идентичен базовой точке `d965255a` (не G-регрессия) |

production-equivalent зелёная строка — `production,internals`;
`verify-internals-negative-boundary` — зелёный.

### G-suite cross-check

`node scripts/b4_state_byte_table.mjs` (расширен секцией G-suite cross-check)
сверяет все 20 судейских значений `_sb3_*.log` с колонкой B CSV: 0
расхождений, rc 0. Значения G-suite идентичны колонке B (ao 50/93, cycles
61.4/100.4; rt 50/93, c16 22/54, c64 22/54; batch 114/141; batchc 94/110;
larson 283; mstress 384; thrash64 22/91, cycles 22.0/100.2).

## 12. CI-квитанции (iai A/B и mt-bench)

Таблицы ниже выведены `node scripts/b4_state_byte_ci_table.mjs` из
закоммиченных сырых файлов: `docs/perf/_raw_b4_state_byte_iai_ab.csv` и
`_raw_b4_state_byte_iai_ab_verdict.log` (r20 gate-run 39422462237,
`scripts/r20_r6_ab_iai_run.mjs`), `docs/perf/_raw_b4_state_byte_mt_base.log`
(mt-bench 39403460449 на `2730d02a`) и `_raw_b4_state_byte_mt_pr87.log`
(mt-bench 39422510174 на `30e57b62`, логи усечены до таблиц, маркер в файле).
ИЗМЕРЕНО (CI).

iai A/B (r20 gate-run, A = `2730d02a`, B = `30e57b62`, features `production bench-internals internals`):
- cells with A > 100 Ir: 77; better 50, worse 16, equal 11
- mimalloc control cells: 13, all Δ = 0: yes

| cell (largest gains) | A Ir | B Ir | Δ | Δ % |
|---|---:|---:|---:|---:|
| `recycle_alloc_free_256x16b` | 163720 | 151581 | -12139 | -7.41 % |
| `recycle_alloc_free_256x64b` | 165223 | 153084 | -12139 | -7.35 % |
| `dealloc_free_only_1088_16b_n1024` | 289268 | 269515 | -19753 | -6.83 % |
| `cold_alloc_free_256x16b_4n` | 290972 | 271360 | -19612 | -6.74 % |
| `recycle_alloc_only_256x16b` | 132314 | 124257 | -8057 | -6.09 % |
| `cold_alloc_free_256x16b_2n` | 166208 | 156484 | -9724 | -5.85 % |
| `growth16_n` | 14269275 | 13489142 | -780133 | -5.47 % |
| `cold_alloc_free_256x16b` | 102758 | 97978 | -4780 | -4.65 % |

| cell (all regressions) | A Ir | B Ir | Δ | Δ % |
|---|---:|---:|---:|---:|
| `carve_batch_only_16b` | 2752 | 3453 | 701 | 25.47 % |
| `carve_batch_only_16b_2n` | 8205 | 9610 | 1405 | 17.12 % |
| `r19_prf01_publication` | 144790 | 149610 | 4820 | 3.33 % |
| `seg_cycle_decommit_256k` | 1632161 | 1673547 | 41386 | 2.54 % |
| `multiseg_cold_256k` | 1519611 | 1554997 | 35386 | 2.33 % |
| `r19_prf01_joined_producer_same_word` | 476020 | 480117 | 4097 | 0.86 % |
| `realloc_large_grow_6_to_8mib_prefix` | 6337958 | 6337960 | 2 | 0.00 % |
| `realloc_large_shrink_8_to_6mib_prefix` | 8435110 | 8435112 | 2 | 0.00 % |
| `realloc_large_shrink_8_to_4p5mib_prefix` | 8435110 | 8435112 | 2 | 0.00 % |
| `realloc_large_shrink_8_to_3mib_prefix` | 8435110 | 8435112 | 2 | 0.00 % |
| `realloc_large_equal_8mib_prefix` | 8435110 | 8435112 | 2 | 0.00 % |
| `realloc_large_shrink_8_to_3mib` | 11929544 | 11929546 | 2 | 0.00 % |
| `realloc_large_shrink_8_to_4p5mib` | 13675400 | 13675402 | 2 | 0.00 % |
| `realloc_large_shrink_8_to_6mib` | 15421256 | 15421258 | 2 | 0.00 % |
| `realloc_large_grow_6_to_8mib` | 15421265 | 15421267 | 2 | 0.00 % |
| `realloc_large_equal_8mib` | 16823946 | 16823948 | 2 | 0.00 % |

mt-bench, production, SeferAlloc / mimalloc ops-per-sec ratio (run1 / run2, same job):

| OS | workload | T | base `2730d02a` | PR #87 |
|---|---|---:|---|---|
| ubuntu-24.04 | larson | 1 | 0.83 / 0.77 | 0.91 / 0.93 |
| ubuntu-24.04 | larson | 2 | 1.01 / 0.82 | 1.30 / 1.02 |
| ubuntu-24.04 | larson | 4 | 1.12 / 1.09 | 1.27 / 1.26 |
| ubuntu-24.04 | mstress | 1 | 0.60 / 0.60 | 0.77 / 0.76 |
| ubuntu-24.04 | mstress | 2 | 0.66 / 0.67 | 0.75 / 0.75 |
| ubuntu-24.04 | mstress | 4 | 0.55 / 0.59 | 0.69 / 0.74 |
| windows-2022 | larson | 1 | 0.70 / 0.71 | 0.75 / 0.72 |
| windows-2022 | larson | 2 | 0.89 / 0.76 | 0.95 / 0.73 |
| windows-2022 | larson | 4 | 0.84 / 0.82 | 0.83 / 0.65 |
| windows-2022 | mstress | 1 | 0.52 / 0.52 | 0.54 / 0.54 |
| windows-2022 | mstress | 2 | 0.76 / 0.55 | 0.73 / 0.55 |
| windows-2022 | mstress | 4 | 0.57 / 0.55 | 0.66 / 0.56 |

Регрессии iai — механизм, не шум (контрольные ячейки mimalloc Δ = 0):
`carve_batch_only_16b` (+701 Ir), `multiseg_cold_256k` и
`seg_cycle_decommit_256k` (+2.3…+2.5 %) — свежий carve теперь пишет байт
состояния каждому блоку (в базе нулевой alloc-битмап уже означал
«выделен», carve его не трогал); `r19_prf01_publication` и
`r19_prf01_joined_producer_same_word` (+0.9…+3.3 %) — owner-сторона
reclaim читает и пишет байт; путь producer'а не менялся. Эти ячейки не
входили в предрегистрированные гейты (судьи §§ выше), сквозные ячейки
улучшились на 1…7 %. Сокращение стоимости carve — задачи B4-24 (S1:
финальное состояние сразу) и B4-23 (D1′: без per-issue записи листа).

mt-bench: на Linux отношение к mimalloc выросло во всех ячейках (mstress T=1
0.60 → 0.77). На Windows ячейки в пределах разброса двух прогонов, кроме
larson T=4 run2 (0.82 → 0.65 при run1 0.84 → 0.83) — одиночный выброс VM,
повтор не делался.

