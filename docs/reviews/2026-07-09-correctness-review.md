# Ревью корректности: держит ли код заявленные инварианты (I1–I6, M1–M8)

**Дата:** 2026-07-09. **Исполнитель:** независимый ревьюер №2 из 7 (угол:
ПРАВИЛЬНОСТЬ — следует ли выполнение инвариантов из кода на самом деле, и не
вакуумны ли тесты, которые это «доказывают»). **Режим:** только чтение и
анализ; код/тесты/конфиги не менялись, тесты не перезапускались (7 агентов
работают в одном рабочем дереве — cargo-прогоны конфликтовали бы; все
контрфакты ниже — трассировка кода, не перепрогон; уровень уверенности указан
на каждой находке).

## Scope

- Спецификация: `docs/INVARIANTS.md` (I1–I6 handle-store; M1–M8 аллокатор,
  включая задокументированный RESIDUAL-лимит M2).
- Код: `src/alloc_core/` (size_classes, alloc_core, segment_header,
  alloc_bitmap, remote_free_ring, segment_table, bootstrap, node, os,
  deferred_large, run_stack), `src/registry/` (heap_core, tcache),
  `src/global/` (sefer_alloc, tls_heap, fallback), `crates/region/src/*`,
  `crates/vmem/src/lib.rs`, `src/lib.rs`.
- Тесты (контрфактный разбор): `differential.rs`, `region_invariants.rs`,
  `compaction.rs`, `alloc_core_invariants.rs`, `heap_invariants.rs`,
  `alloc_core_differential.rs`, `double_free_guard.rs`, `heap_core_tcache_m2.rs`,
  `realloc_in_place.rs`, `regression_inplace_large_realloc.rs`,
  `regression_realloc_cross_class_shrink.rs`,
  `regression_refill_window_double_issue.rs`, `decommit_soak.rs`,
  `size_classes_lookup.rs`, `regression_fastbin_aligned_roundtrip.rs`,
  `alloc_core_reentrancy.rs`, `regression_xthread_double_free_residual.rs`.

## Методология

1. Для каждого инварианта — личная трассировка кода, доказывающего его
   (арифметика size-class пересчитана независимым скриптом: 49 классов,
   `SMALL_MAX = 258 752` (~252,7 KiB), класс, покрывающий 131 072 — **132 464**;
   это число ниже стало ключом к находке F3).
2. Разбор граничной арифметики: carving/bump (off-by-one на границе сегмента),
   округление классов, overflow в `align_up`/`needed` большого пути, OOM-пути.
3. Контрфактный разбор тестов: «упал бы тест, если бы инвариант сломали?» —
   мысленный revert охраняемого механизма и трассировка исхода assert'ов.
4. Сверка спецификации `INVARIANTS.md` с фактическим состоянием кода/тестов.

---

## Находки (по убыванию severity)

### F1 — Medium. Два именованных «M2»-юнита вакуумны как детектор double-free; в одном — ложный стейл-комментарий

- `tests/alloc_core_invariants.rs:80` (`m2_double_free_is_noop`) и
  `tests/heap_invariants.rs:77` (`m2_double_free_does_not_crash`).
- **Контрфакт (трассировка):** уберите битмап-гард из `dealloc_small`
  (`src/alloc_core/alloc_core.rs:3305`) — второй `dealloc` сделает
  `write_next(block, old_head == сам block)` → self-loop головы фрилиста.
  Оба теста после этого делают ОДИН `alloc` и проверяют только `!is_null()` —
  self-loop отдаёт тот же блок, non-null, **тест зелёный при сломанном M2**.
  Двойную выдачу поймал бы только ВТОРОЙ alloc с проверкой различности —
  которой в этих тестах нет.
- Покрытие M2 в целом спасают другие тесты: `tests/double_free_guard.rs:21`
  (различность 16 указателей после double-free — настоящий контрфакт),
  `tests/heap_core_tcache_m2.rs:65,144,221` (магазинный слой, `assert_ne!` на
  повторной выдаче) и `alloc_core_differential.rs` (M3-оракул перекрытий
  ловит повторную выдачу после double-free в каждом прогоне). Т.е. **инвариант
  покрыт, но не теми тестами, которые носят его имя**.
- Стейл-комментарий: `tests/heap_invariants.rs:82-84` — «Second dealloc:
  pushes again onto the free list. Not ideal but not UB» — описывает поведение
  ДО Phase 13.4a; с битмапом второй dealloc не пушит вообще. Комментарий
  прямо противоречит текущему механизму M2.
- **Рекомендация:** усилить оба теста проверкой различности (как в
  `double_free_guard.rs`) либо удалить как дубли; переписать комментарий.

### F2 — Medium. Тест «M7» — чистая тавтология, не может упасть

- `tests/alloc_core_invariants.rs:192` (`segment_of_finds_our_segment_base`).
- Тест проверяет: (a) `base % SEGMENT == 0`, где `base = ptr & !(SEGMENT-1)` —
  верно для ЛЮБОГО указателя по определению маски; (b) `ptr ∈ [base,
  base+SEGMENT)` — тоже тождество маски. Ни одно из assert'ов не зависит от
  поведения аллокатора; тест **не может упасть ни при какой поломке M7**.
  Заявленное в заголовке свойство («every live pointer's segment base is one
  of our segment bases») не проверяется — нет обращения к
  `dbg_contains_base`.
- Реальное производственное доказательство M7 существует в другом месте:
  `contains_base`-маршрутизация (`src/registry/heap_core.rs:1289`,
  `src/alloc_core/alloc_core.rs:540`) + `tests/segment_table_o1.rs` /
  `heap_cross_segment.rs`. Вердикт по M7 не меняется, но именованный тест —
  вакуумный.
- **Рекомендация:** добавить `assert!(a.dbg_contains_base(ptr))` — одна строка
  делает тест содержательным.

### F3 — Medium. Ложные допущения о SMALL_MAX в «Large»-тестах: заявленные пути не совпадают с фактически исполняемыми

Фактический `SMALL_MAX = 258 752` (~252,7 KiB; пересчитано независимо от кода
по алгоритму `build_table`). Отсюда:

- `tests/realloc_in_place.rs:177` (`realloc_large_to_large_preserves_data`):
  `old_size = 128 KiB = 131 072` — это **малый класс** (блок 132 464), а не
  «definitely large» (комментарий на :180 «Use a size larger than SMALL_MAX so
  both paths go through the large path» — ложен). Тест фактически проверяет
  small(132464)→Large-переезд, а не Large→Large.
- `tests/regression_inplace_large_realloc.rs:248` (`shrink_large_does_not_pin`),
  комментарий :263 «Shrink to 128 KiB — still Large» — ложен: 128 KiB — малый
  класс; тест покрывает Large→small-переезд.
- `tests/alloc_core_invariants.rs:303` (`many_large_allocs_then_free`): размеры
  50 000·(i+1); для i=0..4 (50k–250k) это малые классы — «many_large» на
  четверть малый.
- **Следствие для покрытия:** настоящий Large→Large-переезд при РОСТЕ покрыт
  (`grow_beyond_span_relocates_and_preserves`,
  `tests/regression_inplace_large_realloc.rs:122`: 3.5 MiB → 5 MiB — оба
  честно Large), а вот **Large→Large-СЖАТИЕ с переездом (напр. 8 MiB → 5 MiB)
  не покрыто ни одним фокусным тестом** — все «shrink»-тесты уходят в малый
  класс на новой стороне. Код этого пути (`realloc` slow-leg: alloc+copy+
  dealloc, `src/alloc_core/alloc_core.rs:1618-1631`) прям и, по трассировке,
  корректен, но заявленное тестами покрытие не совпадает с фактическим.
- **Рекомендация:** поднять размеры в трёх местах выше 258 752 (например,
  512 KiB / 1 MiB) и добавить кейс Large→smaller-Large; в идеале — вывести в
  тестах порог из `SegmentLayout::SMALL_MAX`, а не из литералов, чтобы
  будущая перестройка таблицы классов не разъезжалась с тестами молча.

### F4 — Low. Спецификация `docs/INVARIANTS.md` устарела в четырёх местах (spec-vs-reality drift)

1. `docs/INVARIANTS.md:3-4`: «encoded as tests (unit tests in `src/lib.rs`
   plus ... tests/differential.rs)» — в `src/lib.rs` тестов нет (по политике
   CLAUDE.md они в `tests/region_invariants.rs`, `tests/compaction.rs`,
   `tests/differential.rs`). Указатель на доказательства ведёт в пустоту.
2. `docs/INVARIANTS.md:19-21`: **I6 помечен «Phase 2, not yet implemented»**,
   при этом `tests/compaction.rs:87-143` уже реализует и проверяет I6 в его
   фактической форме («compaction-by-construction» через slotmap: резолюция
   выживших хэндлов после churn, плотный учёт, повторное использование
   слотов без роста capacity). Инвариант держится, но спека утверждает
   обратное его статусу.
3. `docs/INVARIANTS.md:61-63` (M6): «eager decommit lands in Phase 10» —
   decommit давно реализован (Phase 35, `alloc-decommit`;
   `decommit_empty_segment` `src/alloc_core/alloc_core.rs:1201`, тесты
   `decommit_soak.rs`, `decommit_miri_cycle.rs`). Спека всё ещё обещает его в
   будущем времени.
4. `docs/INVARIANTS.md:48-50` (M4): «large/huge allocations honour
   **arbitrary** alignment via a dedicated segment» — фактически `align >=
   SEGMENT` (4 MiB) отклоняется null'ом by design
   (`src/alloc_core/alloc_core.rs:3356`, task #130). Null — легальный отказ
   аллокации, но «arbitrary» — оверклейм; корректная формулировка — «alignment
   up to SEGMENT».
- **Рекомендация:** освежить INVARIANTS.md (это документ-спецификация,
  по методологии проекта он обязан совпадать с кодом).

### F5 — Low. Стейл-доки, которые ложно описывают механизм маршрутизации (риск для будущих правок)

- `src/alloc_core/segment_header.rs:613`: «The Cartographer consults this
  [PageMap] on `dealloc` to route a freed block» — ложь после Phase 13.3:
  ни один production-путь не выводит класс из `page_map` (own-thread —
  из Layout, `alloc_core.rs:699-717`; reclaim — из класса в ring-записи,
  `alloc_core.rs:883-885`). Там же :174 «Pages are dedicated to a single
  class once carved» — противоречит §13 RACE_DRAIN_RECLAIM (общий bump даёт
  mixed-class страницы; page_map фиксирует только первый класс). PageMap
  сегодня не несёт корректность — но док приглашает будущего разработчика
  снова на него опереться (ровно тот класс бага, что уже чинили в §13).
- `tests/alloc_core_invariants.rs:175` (`m4_large_alignment_uses_dedicated_segment`):
  имя и комментарий утверждают «Alignment > SMALL_ALIGN_MAX → dedicated
  segment» — после #114/B1 запрос (32, align=4096) обслуживается МАЛЫМ классом
  4096 (проверено трассировкой `class_for`: 4096 % 4096 == 0). Assert
  выравнивания остаётся истинным, но тест не проверяет то, что называет.
- **Рекомендация:** поправить доки/имена; для PageMap — явная пометка
  «not load-bearing; do not derive classes from it».

### F6 — Low. `differential.rs` заявляет I1–I5, но I3 (no-ABA) не прогоняет

- `tests/differential.rs:5-8`: «encodes invariants I1–I5» — модель удаляет
  хэндл из `live` при remove и больше НИКОГДА его не опрашивает; сценарий
  «слот переиспользован новым insert, старый хэндл обязан давать None»
  (собственно ABA) не возникает ни в одном прогоне. I3 покрыт только юнитом
  `tests/region_invariants.rs:49` (`stale_handle_after_reuse_is_none`) —
  одиночное переиспользование слота; контрфактно он честен (сравнение только
  по индексу без поколения дало бы `Some(&2)` → красный). Планка CLAUDE.md
  («proptest и/или unit») формально выполнена, но заголовок differential
  оверклеймит, а усиление тривиально.
- **Рекомендация:** держать в модели множество «мертвых» хэндлов и на каждом
  шаге проверять, что ни один не резолвится (усиливает и I2-«forever», которое
  сейчас проверяется только в момент remove).

### F7 — Low. Магазинный free-путь не проверяет `kind` сегмента — асимметрия защиты при layout-mismatch free (территория contract-violation)

- `src/registry/heap_core.rs:854` (`dealloc_own_thread_with_base`): при
  `class_for(layout) == Some(c)` путь сразу идёт к оракулам магазина/битмапа,
  не глядя на `kind` сегмента. Для указателя в **Large**-сегменте, освобождённого
  с малым layout'ом (нарушение контракта GlobalAlloc — UB на вызывающем),
  «битмап» читается из байтов ПОЛЕЗНОЙ НАГРУЗКИ Large-аллокации
  (`alloc_bitmap_off` ≈ 5.4 KiB < начала payload'а нет — payload начинается с
  4096, т.е. чтение попадает в живые данные пользователя); при «нуле» в этих
  байтах указатель уедет в магазин и позже будет выдан как малый блок,
  алиасясь с живым Large-блоком. Субстратный путь (`AllocCore::dealloc`,
  `alloc_core.rs:548`) на том же нарушении деградирует до no-op (маршрутизация
  по `kind` первой). Это НЕ нарушение M2 (двойной free с чужим layout —
  вне его scope), но раз проект держит hardened-класс защит (H1 interior-ptr,
  `heap_core.rs:887`), симметричный `kind != Large`-чек под `hardened` закрыл
  бы асимметрию почти бесплатно.
- **Рекомендация:** под `hardened` добавить в магазинный free-путь
  отсечение `SegmentHeader::kind_at(base) == Large → no-op`.

### F8 — Note (позитивные верификации; фиксирую как факт ревью)

- **R1-фикс (retro C1) на месте и запинен честно:**
  `src/alloc_core/alloc_core.rs:2044-2046` — предикат ring-дренажа внутри
  refill обёрнут `issued_so_far.contains(&ptr)`;
  `tests/regression_refill_window_double_issue.rs:133` воспроизводит ровно
  C1-интерливинг (P — единственный блок класса на фрилисте + стейл-записка в
  ринге → один refill) и упал бы при снятии гарда (двойная выдача P на
  последовательных позициях — контрфакт задокументирован и правдоподобен по
  трассировке).
- **残 M2-residual (нога 3) держится честно-красным:**
  `tests/regression_xthread_double_free_residual.rs:106` — `#[ignore]`-мессадж
  обновлён на X7 (ретро-находка C3 закрыта).
- **OPT-F `==`-правило** (`alloc_core.rs:1769`) + образцовый контрфактный тест
  `tests/regression_realloc_cross_class_shrink.rs:70` (явная проверка
  предусловий «классы реально разные», затем `assert_ne!` на релокацию —
  предохранитель от вакуумизации при будущей смене геометрии таблицы).
- **Арифметика классов**: `build_size2class` (size_classes.rs:321) корректно
  резолвит бакет k на верх диапазона `(k·16, (k+1)·16]`; полный свип
  `tests/size_classes_lookup.rs` против независимого линейного скана —
  настоящий оракул; align>16 покрыт свипом :179/:207 и
  `regression_fastbin_aligned_roundtrip.rs:89,128` (включая (640,128) — токио-кейс).
- **Carving без off-by-one**: `carve_block` (alloc_core.rs:3119) отклоняет
  `aligned + block > SEGMENT` (точная подгонка `== SEGMENT` легально
  допускается — последний байт SEGMENT-1); `carve_batch` (:3209) проверяет
  границу ДО вычисления `room = (SEGMENT - aligned_start)/bs` — underflow
  недостижим.
- **Большой путь без overflow**: `align >= SEGMENT` отклоняется до арифметики
  (:3356); `align_up(size, align)` ограничен инвариантом `Layout`
  (size ≤ isize::MAX) + align < 2^22 → без переполнения; OPT-G использует
  `checked_add` (:1751).
- **OOM-пути**: все ветки `register`/`reserve`-отказов освобождают резервацию
  и возвращают null (:3523-3529, :3957-3963); `Layout::from_size_align`-ошибки
  в realloc → null. Единственный panic-сайт — `Default::default()`
  (конструкция, не alloc-путь, задокументирован).
- **Ринг**: wrap-корректность (`h != t`, wrapping_sub; compile-time
  `RING_CAP.is_power_of_two()` — remote_free_ring.rs:167) — трассировка MPSC
  протокола не нашла потери/повторного дренажа слота.
- **Drop**: двухфазный collect-then-free (alloc_core.rs:4078-4098) не
  освобождает примордиал (хост реестра) во время итерации; NULL-слоты
  (recycled) отфильтрованы — двойного release нет.
- **M5**: `tests/alloc_core_reentrancy.rs` — считающий глобальный аллокатор с
  thread-local счётчиком: настоящий рантайм-оракул (нулевая дельта), не смок.
- **M6**: `tests/decommit_soak.rs:47` — контрфактный якорь (`dbg_decommit_count`
  обязан вырасти; при перевёрнутом live-провизо остался бы 0) + readback после
  recommit. Держится.

---

## Итоговый вердикт по инвариантам

| Инвариант | Вердикт | Обоснование / оговорки |
|---|---|---|
| **I1 — resolution** | **держится** | Тонкая мембрана над slotmap (`crates/region/src/region.rs:96`); differential (I1-assert на каждом insert/get) + юниты — не вакуумны. |
| **I2 — tombstone** | **держится** | slotmap-поколения; differential проверяет None+повторный remove в момент удаления; «forever» — только до следующего опроса (см. F6-усиление). |
| **I3 — no ABA** | **держится** | slotmap generation bump; юнит `region_invariants.rs:49` контрфактно честен. Proptest-покрытия нет — заголовок differential оверклеймит (F6). |
| **I4 — accounting** | **держится** | `len()`-делегация; differential сверяет с моделью после каждой операции. |
| **I5 — drop-once** | **держится** | Владение хранением у slotmap; drop-счётчики в differential (`drops == total_inserts`) и `region_invariants.rs:71` — настоящие оракулы (двойной drop/утечка сдвинули бы счётчик). |
| **I6 — compaction** | **держится (by construction), но спека лжёт о статусе** | `tests/compaction.rs` проверяет резолюцию после churn, плотный учёт, capacity ≤ high-water; `INVARIANTS.md:19` всё ещё «not yet implemented» (F4.2). |
| **M1 — validity** | **держится** | Трассировка: границы carve, соответствие класса, fit больших в `span_usable`; differential + инварианты с write/readback по всему size. |
| **M2 — no double-free/UAF** | **держится в заявленных границах** | Битмап (субстрат) + два точных оракула магазина (Э6) + `off >= bump` (post-decommit) + R1-гард окна refill; residual-нога 3 (re-issue-before-drain) и hardened-wrap 1/256 задокументированы и запинены честно-красным. Именованные юниты частично вакуумны (F1), но сильные тесты (`double_free_guard`, `tcache_m2`, differential-M3-оракул) несут покрытие. Layout-mismatch free — вне scope, но см. асимметрию F7. |
| **M3 — no overlap** | **держится** | Дизъюнктность bump-нарезки + битмап против повторной выдачи + Э8/PERF-3 splice-доказательства (`flush_run` rebuild корректен по трассировке: диверсия в RunStack не оставляет блока одновременно в цепочке и в дескрипторе); differential-оракул перекрытий — реальный. |
| **M4 — alignment/size fidelity** | **держится; формулировка спеки оверклеймит** | Цепочка: `block % align == 0` (class_for) + offset кратен block (carve) + база SEGMENT-aligned ⇒ выровнено; свипы + fastbin-roundtrip — сильные. `align >= SEGMENT` → null (легально), но спека говорит «arbitrary» (F4.4); тест-имя F5. |
| **M5 — reentrancy-freedom** | **держится** | Структурно (нет Vec/Box/std::alloc на пути; inline TFS ломает Box-рекурсию bind-пути) + рантайм-оракул `alloc_core_reentrancy.rs`. Fallback — спинлок без std::alloc. |
| **M6 — OS return/decommit** | **держится (qualified)** | `decommit_empty_segment` + recycle слота; soak-контрфакт реальный. Оговорки (обе задокументированы в коде): (a) ring-overflow → bounded leak: застрявшие блоки держат `live_count > 0` и пинят сегмент — при устойчивом adversarial-переполнении RSS-возврат по этому сегменту не наступает; (b) спека всё ещё датирует decommit «Phase 10» (F4.3). |
| **M7 — owner routing** | **держится** | Маска + membership (`contains_base`-хэш с tombstone-rebuild W2, own-cache инвалидация структурно в тех же функциях, что и `hash_remove`); кросс-thread маршрутизация читает только write-once поля. Именованный тест — тавтология (F2), покрытие несут другие. |
| **M8 — generational coherence (Handle face)** | **держится** | Тот же slotmap-механизм, что I3; отдельного субстратного кода нет — и не требуется. |

**Сводный вердикт:** нарушений инвариантов в коде не найдено; заявленные
RESIDUAL-лимиты M2 совпадают с фактическим кодом и честно запинены. Основной
долг — в слое ВЕРИФИКАЦИИ (вакуумные/мисс-лейбл тесты F1–F3) и в дрейфе
спецификации (F4–F6): сегодня инварианты держит код, но три именованных теста
не заметили бы их поломку, а спека в четырёх местах описывает не тот проект,
который лежит в репозитории.

## Ограничения ревью

- Тесты не перезапускались (общее рабочее дерево с 6 параллельными агентами);
  контрфакты F1–F3 — детальная трассировка кода, не эмпирический revert.
  Рекомендую владельцу сессии перепроверить F1/F2 эмпирически (закомментировать
  `bm.is_free`-гард / добавить `dbg_contains_base`) — по моей трассировке
  исходы однозначны.
- loom/miri/TSan-прогоны не перезапускались; конкурентные свойства ринга и
  deferred-large оценены трассировкой протокола и приняты по существующим
  loom-моделям (`loom_remote_ring`, `loom_deferred_large`,
  `loom_magazine_ring_compose`).
- `crates/vmem` смотрен на предмет M1/M6-контрактов (over-reserve+trim,
  decommit/recommit, miri-aperture) — расхождений с использованием в `os.rs`
  не найдено; глубокий аудит unsafe — зона ревьюера №1.
