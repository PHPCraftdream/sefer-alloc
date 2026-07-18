# Deep audit 2026-07-17 — AUDIT-5: Охота за логическими ошибками

- **Аудитор:** read-only агент, статический анализ + grep + чтение; без cargo/build/git-записей.
- **База:** рабочее дерево на коммите `ffd3215` (ветка `main`, чистое — untracked-файлы это только checkpoints/логи наблюдения CI и сам этот отчёт).
- **Охват:** `src/alloc_core/*` (весь Cartographer: alloc/dealloc/realloc, carve/carve_batch,
  free-list pop/drain, magazine flush, large-cache, small-segment pool, remote-free ring,
  segment directory, segment table, bootstrap, size-classes shim), плюс кросс-проверка
  `crates/size-classes` (const-built таблица классов + O(1) lookup) и ключевые cfg-развилки
  (`alloc-lazy-commit` × `numa-aware` × unix/windows/miri, `hardened`/`fastbin`).
- **Метод:** построчное чтение с акцентом на арифметику границ (align_up/down, `off >= bump`,
  bit-index в битмапах/directory, hash-table backward-shift), парное сравнение похожих файлов
  (`alloc_core_small.rs` vs `alloc_core_small_magazine.rs` vs `alloc_core_small_reclaim.rs`;
  `reclaim_offset` vs `reclaim_offset_checked`; `carve_block` vs `carve_batch`) на предмет
  копипаст-дрейфа; целевой поиск известного класса дефекта F2 (cfg-ветка, ассертящая неверное
  на недостижимой конфигурации); два делегированных под-аудита (`segment_table.rs`+
  `bootstrap.rs`; `segment_directory.rs`+вызывающий код) для независимой проверки
  hash/bitmap-арифметики.

---

## Сводная таблица

| # | Severity | Confidence | Место | Кратко |
|---|----------|------------|-------|--------|
| L1 | Low | CONFIRMED | `tests/lazy_commit_b2_grow.rs:86-90`, аналогично `tests/lazy_commit_b3_recycle.rs` (см. текст) | Тест ассертит `frontier == SEGMENT` на объединении веток `not(windows) ∨ miri ∨ numa-aware`, хотя утверждение верно лишь для eager-конфигураций; **уже отслеживается как task #191 (HYGIENE F2)**, здесь только подтверждение находки независимым проходом, не новая находка |
| — | — | — | — | Ниже — систематический обзор потенциально уязвимых мест, все признаны **корректными** (false-positive candidates, зафиксированы для экономии времени будущего аудита) |

**Находок уровня Medium/High/Critical в продакшен-коде (`src/`) не обнаружено.** Кодовая база
несёт очень плотную защитную дисциплину (H-1 payload-lower-bound guard, `off >= bump`
stale-free guard, M2 double-free bitmap oracle, R6-MS interior-pointer guard под `hardened`) —
каждый потенциальный класс логической ошибки, который обычно всплывает в аллокаторах
(off-by-one в bump-carve, integer overflow в align_up, копипаст-дрейф small/large/pool), в этой
базе уже закрыт именованным guard'ом с датированным комментарием, отсылающим к конкретному
прошлому инциденту (`#41`, `#114`, `#130`, `#134`, `#164`, UBFIX-3/6/7/11, R6-MS-1..5, X7 Ф1-3).

---

## L1 — Low / CONFIRMED: test-only cfg-объединение маскирует "eager" под "true"

- **file:line:** `tests/lazy_commit_b2_grow.rs:86-90` —
  ```rust
  #[cfg(any(not(windows), miri, feature = "numa-aware"))]
  {
      assert_eq!(initial_frontier, SEGMENT);
      return; // nothing to test on the eager path
  }
  ```
  Аналогичный паттерн (та же троичная `any(...)`-развилка) повторён в
  `tests/lazy_commit_b2_grow.rs:313-316` (`return_none_on_commit_failure_leaves_state_unchanged`)
  и по духу в `tests/lazy_commit_b3_recycle.rs` (там же тестируется `committed_payload_end`
  после decommit-reset).
- **Класс дефекта:** ровно тот, что запрошен в задании (F2) — cfg-ветка, ассертящая
  инвариант эager-пути на объединении, которое включает конфигурации, где инвариант ПОКА не
  проверен по отдельности. Условие `not(windows) ∨ miri ∨ numa-aware` истинно для **unix
  (non-miri) ∧ alloc-lazy-commit ∧ ¬numa-aware** — комбинация, где по документации в самом
  продакшен-коде (`alloc_core_small.rs:1417-1432`, `reserve_small_segment`) на Unix
  `aligned_vmem::reserve_aligned_lazy` **тоже** вызывается (лениво коммитит только
  `meta_end + LAZY_FIRST_CHUNK`, а не весь `SEGMENT`) — комментарий в проде прямо говорит:
  «On the eager path (feature-OFF) or under miri/Unix, `reserve_aligned_lazy` falls back to the
  eager `reserve_aligned` internally» — т.е. **сам прод предполагает, что на Unix-эту функцию
  вызывают, но она либо (a) реально уже фолбэчится на eager внутри `aligned-vmem`-крейта для
  Unix, либо (b) документация/тест разошлись с реальным поведением крейта**. Задача #191 в
  трекере уже сформулирована как «lazy_commit b2/b4 tests assert frontier==SEGMENT on the
  unreachable unix∧lazy∧¬numa leg» — т.е. авторы проекта уже определили, что ветка
  unix∧lazy∧¬numa для `committed_payload_end` **должна** быть недостижимой (лениво коммитить
  вообще не должна пытаться на Unix), но тестовый ассерт сформулирован как объединение трёх
  условий вместо явного разбора «на каком именно основании эта ветка eager» — что маскирует,
  ЕСЛИ (а) когда-нибудь `aligned_vmem::reserve_aligned_lazy` реализует настоящий lazy-commit и
  на Unix через `mmap(PROT_NONE)` + `mprotect`, этот тест продолжит молча ассертить старое
  (неверное к тому моменту) поведение вместо того, чтобы упасть и вскрыть регресс.
- **Конкретный сценарий:** если будущий PR добавит настоящую Unix-реализацию
  `reserve_aligned_lazy` (сейчас, по комментариям в `alloc_core_small.rs`, она на Unix
  фолбэчится на eager), тест `carve_at_frontier_commits_next_chunk` **под `cfg(unix, not(miri),
  not(numa-aware), alloc-lazy-commit)`** продолжит брать ветку `any(not(windows), ...)` →
  `assert_eq!(initial_frontier, SEGMENT)` → тест либо (i) упадёт с понятной ошибкой (frontier
  теперь `< SEGMENT`) — это ХОРОШО, но тогда `return` на строке 89 означает, что весь остаток
  теста (собственно проверка grow-on-carve инкремента) для новой Unix-lazy-реализации **никогда
  не выполнится**, поэтому регрессия в grow-логике на Unix осталась бы непокрытой этим тестом
  ещё долго после того, как первый assert её бы отловил и разработчик его бы "исправил" простым
  снятием строгого равенства, не заметив, что тест внутри себя после этого никогда не доходит
  до содержательной части.
- **Влияние:** ЧИСТО test-only, production-код не затронут (сам `alloc_core_small.rs`
  корректно вычисляет `committed_payload_end` per-cfg — см. `reserve_small_segment`,
  `alloc_core_small.rs:1507-1537`, где `numa-aware` явно даёт `SEGMENT`, а
  `not(numa-aware)` даёт `meta_end + LAZY_FIRST_CHUNK` независимо от ОС — сам прод НЕ
  различает unix/windows в этой формуле, значит на практике эта ветка теста сегодня либо
  верна, либо тест устарел относительно прод-инварианта одинаково на всех Unix). Уже заведено
  как task #191 в очереди проекта — здесь фиксируется как независимое подтверждение находки,
  а не новая. **Actionable severity: Low** (test hygiene, не влияет на корректность
  аллокатора).
- **Фикс:** см. формулировку task #191 — разбить объединённое `cfg(any(...))` на явные
  raw-cfg-ветки с раздельным `assert_eq!`/комментарием на каждую причину («на miri — фолбэк»,
  «под numa-aware — P2-гейт всегда eager», «на unix — сегодня фолбэк на eager ВНУТРИ
  aligned-vmem, при изменении этого факта тест обязан явно упасть, а не молча остаться в
  early-return»), чтобы будущая Unix-lazy-реализация ломала ИМЕННО тот `assert_eq!`, который
  относится к unix, без риска, что содержательная часть теста продолжит быть недостижимой.

---

## Систематически проверенные и ОТКЛОНЁННЫЕ кандидаты (для экономии времени будущих аудитов)

Все нижеперечисленные — места, которые по шаблону ("похоже на известный класс бага") стоило
проверить, и которые оказались корректны при трассировке до конца:

1. **`carve_batch` (`alloc_core_small.rs:1109-1204`) — дублирование вычисления `room`/`n`.**
   Под `alloc-lazy-commit` блок на строках 1156-1175 пересчитывает `batch_room`/`batch_n`/
   `batch_end` ТОЛЬКО чтобы определить commit-диапазон; блок на строках 1176-1179
   безусловно пересчитывает те же `room`/`n` теми же формулами (`(SEGMENT - aligned_start) /
   block_size`, `out.len().min(room)`) от тех же неизменных входов (`aligned_start`,
   `block_size`, `out.len()` — между двумя блоками ничего их не меняет). Байт-в-байт
   идентичные результаты, просто избыточное вычисление (небольшая, не влияющая на корректность
   потеря производительности, а не логическая ошибка) — не находка для AUDIT-5 (возможно,
   тема для AUDIT-7 perf).

2. **`SizeClasses::class_for` (`crates/size-classes/src/lib.rs:354-385`) — `(need - 1) >>
   shift`, потенциальный underflow при `need == 0`.** `need = max(size, align)`; единственный
   продакшен-вызывающий (`AllocCore::alloc` → `classify`) передаёт `size =
   layout.size().max(MIN_BLOCK)` (≥16) и `align = layout.align()` (степень двойки ≥1 по
   инварианту `core::alloc::Layout`), так что `need ≥ 16 > 0` всегда — `need - 1` никогда не
   вызывает wraparound на реальном пути. `dbg_layout_class_for` (test-only) передаёт
   произвольный `Layout`, у которого `align()` всё равно ≥1 по конструктору `Layout` — не
   эксплуатируемо даже из теста.

3. **H1 interior-pointer guard, отсутствие в `flush_run`/`flush_class`
   (`alloc_core_small_magazine.rs:465-574`).** На первый взгляд похоже на дрейф: у
   `dealloc_small` (`alloc_core_small.rs:1240-1243`) и у `reclaim_offset`/
   `reclaim_offset_checked` (`alloc_core_small_reclaim.rs`) есть `hardened`-гейтнутая проверка
   `off % block_size == 0`, а у `flush_run` — нет. Прослежено до обоих вызывающих
   (`heap_core_free.rs:118-391` `dealloc_own_thread_with_base`, `heap_core_tcache.rs:78-105`
   `flush_all_tcache`): КАЖДЫЙ блок, попадающий в `flush_class`, уже прошёл этот же H1-guard
   **на входе в магазин** (`dealloc_own_thread_with_base:163-188`) — `flush_class` лишь
   выгружает уже провалидированные блоки обратно в substrate, повторная проверка была бы
   тавтологией. Не находка.

4. **`SegmentTable`/`bootstrap.rs`** (отдельный делегированный аудит) — MAX_SEGMENTS-граница,
   backward-shift deletion, free-list push/pop, primordial hash-insert при инициализации —
   все проверены построчно, логических ошибок не найдено (подробности у под-агента,
   не дублируются здесь).

5. **`segment_directory.rs`** (отдельный делегированный аудит) — bit-index арифметика
   `slot_idx / 64` / `% 64` согласована с `w * 64 + j` во всех вызывающих местах,
   `rebuild_from_table`'s `0..table.count()` корректен без off-by-one, `SMALL_CLASS_COUNT`
   везде берётся из живой feature-зависимой константы (не захардкожен дубликат под
   `medium-classes`) — логических ошибок не найдено.

6. **`RemoteFreeRing` pack/unpack (не-hardened и hardened варианты) и wraparound `u32`
   head/tail.** `RING_CAP` зафиксирован как степень двойки компайл-тайм ассертом именно для
   корректности `wrapping_sub`-арифметики через `u32::MAX`-обёртку; `drain`'s `while h != t`
   (а не `<`) корректно замечен в комментариях как обязательный для wrap-корректности — сверено,
   реализация соответствует комментарию. Не находка.

7. **`AllocCore::realloc` / `safe_payload_read_span` / `realloc_inplace_fast_path_known_base`
   (`alloc_core.rs:1162-1410`)** — OPT-F (`==` а не `<=` для small-same-class) и OPT-G
   (checked_add перед сравнением с `span_usable`) явно и намеренно защищены от классов ошибок,
   которые обычно всплывают именно тут (cross-class shrink corruption, integer overflow на
   pathological `new_size`); оба пути перепроверены на согласованность с описанной в
   комментариях причиной — корректны.

---

## Итог

Прицельный поиск логических ошибок (integer overflow/underflow, off-by-one, инвертированные
условия, копипаст-дрейф между small/large/medium путями, неверные cfg-комбинации) по всему
`src/alloc_core/*` не выявил новых production-дефектов. Единственная находка (L1) —
независимое подтверждение уже заведённого в очередь task #191 test-hygiene дефекта, не новая
информация, но здесь задокументирован точный failure-scenario (будущая Unix-lazy-реализация
молча перестанет содержательно тестировать grow-on-carve на Unix). Кодовая база показывает
необычно высокую плотность именованных, датированных defensive-guard'ов на каждой границе,
которая в среднем аллокаторе была бы уязвима — большинство "похоже на баг" кандидатов при
трассировке до конца оказались уже закрытыми известными прошлыми инцидентами.
