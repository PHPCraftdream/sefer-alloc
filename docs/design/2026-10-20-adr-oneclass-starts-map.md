# ADR: карта стартов «старт выдавался» (D1′) — удаление ClassLeaves и issue-транзакции (one-class)

Дата 2026-10-20. Статус: **ПРЕДЛОЖЕН (ожидает принятия владельцем)**; бывший остаток M-h ЗАКРЫТ детерминированным оракулом (§7). Ветка `b4-oneclass`
(фазы 1a + 1b + OPT). Образец: `docs/design/2026-10-19-adr-small-path-v2.md`.
Дизайн-ревью-первоисточник: `<scratch>/art/b4/oneclass_design.md` (§2 эквивалентность,
§6 план замеров, §7 GO-условия, §8.4 D1′); ревью D1′: `<scratch>/art/b4/d1p_rev_rush.md`,
`<scratch>/art/b4/oneclass_rev_rush.md` (артефакты вне репозитория).

Метки: **ИЗМЕРЕНО** — число из сырого лога `docs/perf/_raw_b4_oneclass_*.log`;
**ВЫВОД ИЗ КОДА** — следует из исходника по `file:line`; **ОЦЕНКА** — вывод с
диапазоном. Замеры: `docs/perf/B4_ONECLASS.md` + `docs/perf/B4_ONECLASS_summary.csv`.

## 1. Контекст

После PR #87 (байт состояния гранулы) в аллокаторе осталось **две карты класса**:
`StateByte` (успешная) и `ClassLeaves` — плотные листовые битмапы классов в sidecar
(256-Б System-аллокации, растущие с carve), плюс **fallible issue-транзакция**
(`IssueTransaction`/`IssueRun`/`prepare_*`, spill-OOM) и двухпроходный drain.
Инвентарь читателей/писателей — §1 дизайн-ревью (24 позиции). Цена (ИЗМЕРЕНО,
плечо A = `bbc3c7b4`): mstress 380 Ir/step, larson 282, batch alloc 114 Ir/блок,
foreign free 136 Ir/free; цена sidecar из первоначального судьи памяти
(1 212 928 Б / 4 100 pins) **отозвана** — судья содержал дефект, актуальные числа
см. mem4-замер в `docs/perf/B4_ONECLASS.md` §5; из оценки @ox —
−25…−40 Ir/step на mstress при удалении.

Находка rush №1 (`<scratch>/art/b4/oneclass_rev_rush.md`): наивный D1 (удалить листья,
решение в producer-RMW `fetch_or`) **расширяет окно misuse**: исполнение «фильтр
пройден до carve, producer заснул, RMW после гашения» освобождает живой блок.
D1 в чистом виде — NO-GO.

Владелец принял **D1′**: плотная карта стартов «старт выдавался» (1 бит/гранулу,
32 КиБ в sidecar'е рядом с pending-массивом; на маршрут — 65 536 Б ровно:
pending 32 КиБ + starts 32 КиБ), точка решения producer'а —
**Acquire-загрузка старта до RMW** — окно misuse буквально возвращается к нынешнему
(§8.4a дизайна: вредное исполнение ⇔ загрузка W читает 1 ⇔ она позже set в порядке
модификаций слова; C11 coherence).

## 2. Решение

1. **Карта стартов 32 КиБ в sidecar:** `starts: [AtomicU64; 4096]` рядом с
   `pending` в одной приватной аллокации `SmallSidecar`
   (`src/registry/segment_route/small_sidecar.rs:41`; SAFETY-комментарий `:138–144`).
   Бит старта монотонен внутри поколения сегмента: ставится при carve/issue,
   снимается только quiescent retain-reset.
2. **Удалено:** `ClassLeaves`/`leaf_classes.rs`, `IssueTransaction`/`IssueRun`/
   `prepare_*`, spill-OOM (пути issue инфаллибельны; единственный остающийся OOM —
   честный отказ ОС), двухпроходный production-drain, `fatal` класса в
   `BitmapCut::pop`.
3. **Producer-фильтр прежней семантики:** payload/alignment, затем загрузка старта.
4. **Reclaim — класс из байта,** геометрия и fatal во всех сборках, плюс проверка
   перекрытия выданных стартов.

## 3. Протокол (все ВЫВОД ИЗ КОДА)

- **Producer (чужой free):** payload-фильтр по kind (`primordial_meta_end` /
  `small_meta_end`) и alignment 16 → Acquire-загрузка старта
  (`sidecar_bitmap.rs:98–107`: при нулевом бите `return false` **без** RMW) →
  `pending[...].fetch_or(1 << (g % 64), AcqRel)` (`sidecar_bitmap.rs:108`).
  Terminal-публикация — тот же якорь R5b-13, ровно один `fetch_or(AcqRel)` на файл.
- **Owner (единственный писатель старта):** поблочно, в точке выдачи, не раньше
  нынешнего leaf-store — `load` → `store(old | mask, Release)` без `lock`
  (`sidecar_bitmap.rs:70–71`), вызывается из `SmallIssueRun` per-run:
  `small_issue_run.rs:60`, `alloc_core_small_impl.rs:736,861` (carve/carve-batch),
  `:438,471,558` (pop/drain); до записи LIVE/MAG и до выдачи.
- **Carve-clear (гашение стартовых бит pending):** RMW `fetch_and` по словам
  диапазона (`small_sidecar.rs:225–229`, `for_words_in_range`) — ради гонки с
  retain-reset; счётчики `CARVE_CLEAR_BITS` / `CARVE_CLEAR_WORD_LOADS`
  (`counters.rs:597,607`).
- **Retain-reset (quiescent, bench-internals/internals хук):** атомарное гашение
  pending **и** starts одним диапазоном (`small_sidecar.rs:214–229`; доказательство
  ограничено документированной quiescence хука `decommit.rs` ~78–85 — не разрешение
  конкурентного retain).

### Порядки памяти

- start-bit store — **Release**, producer-load — **Acquire**: Acquire-загрузка,
  прочитавшая старт-бит, НЕ доказывает видимость более поздней LIVE-записи
  (release/acquire упорядочивает только пару «эта запись ↔ эта загрузка»). Для
  валидного producer'а видимость LIVE-байта обеспечивается передачей выделенного
  указателя (Release→Acquire handoff цепочки публикации), а потребление pending —
  owner-only: owner завершает carve до drain (§3; d1p_rev_rush.md §(2)).
  Producer, прочитавший 0, не дойдёт до RMW (точка решения — загрузка).
- pending `fetch_or`/`fetch_and` — **AcqRel** (якорь R5b-13; согласован с
  owner-drain'ом и retain-reset).
- Байт состояния — без изменений (existing protocol PR #87).
- Гонка «retain-reset vs producer-RMW» (единственное отличие от нынешнего окна,
  §8.4a): carve-clear закрывает гонку ТОЛЬКО при соблюдении quiescence-контракта
  retain-хука (`decommit.rs` ~78–85: эксклюзивный доступ, без pending frees и
  конкурентных обращений). Произвольно задержанный RMW после reset он не
  закрывает (d1p_rev_rush.md находка 2) — документированное ограничение;
  конкурентный retain вне контракта (`t4_retain_reset_clears_stale_bits_recarve_other_class`).

## 4. Обязательные условия 1–8 (постановка; каждое — как выполнено)

1. **Поблочно, не раньше leaf-store; single-writer load→store(old|mask, Release).**
   Выполнено: `sidecar_bitmap.rs:70–71` (Release-store), точки вызова в
   `alloc_core_small_impl.rs:736,861` (до записи LIVE), `:438,471,558`; все записи
   карты атомарны. Тест: `loom_oneclass_start_bit.rs`, мутант oc-mut2-a RED.
2. **Producer: payload-фильтр + alignment 16 + Acquire-загрузка до fetch_or(AcqRel)
   на всех путях публикации** (включая `publish_small_borrowed`, кэш R1
   `route_pin_cache`). Выполнено: `sidecar_bitmap.rs:98–108`;
   `pin.rs:89–128` (`publish_small`/`publish_small_borrowed` → `sidecar.publish`);
   Terminal `fetch_or(AcqRel)` и owner `swap(AcqRel)` не менялись. Тесты:
   `t5_misaligned_free_is_rejected`, мутант oc-mut2-f RED
   (`mutf_publish_metadata_offset_is_rejected_before_the_start_gate`).
3. **Reclaim: класс из байта; только `encode(c, LIVE)`; FREE/MAG — тихий no-op;
   fatal (старт-бит 0 у опубликованной записи, байт 0, state 3, класс ≥ COUNT,
   невалидная геометрия) — во всех сборках; + проверка перекрытия `(off, off+bs)`**
   в production. Выполнено: `alloc_core_small_reclaim.rs:131–139`
   (`reclaim_start_bit` fatal; `:134` `reclaim_start_overlap` — production-проверка
   перекрытия по открытым интервалам, маски крайних слов, правая граница
   исключена); `fatal` на нулевой байт/класс ≥ COUNT — `child_zero_state_byte_md`.
   Тесты: `t2_interior_free_of_live_block_is_rejected_payload_intact`,
   `t6_each_class_remote_free_reclaims_once_class_from_byte`; мутанты oc-mut2-d,
   oc-mut2-e, oc-mut2-l, oc-mut-g RED.
4. **Однопроходный drain: continuation-проверки перенесены, не удалены; hardened
   Floyd и FREE-узлы сохранены.** Выполнено: `alloc_core_small_impl.rs:451–558`
   (валидация до мутации, один проход); `r6_t06_freelist_continuation_bounds`
   зелёный; мутант oc-mut-g RED (`drain_misaligned_aborts`,
   `drain_out_of_segment_aborts`).
5. **Retain-reset — только quiescent.** Выполнено: `small_sidecar.rs:214–229`
   (атомарное гашение обоих массивов одним диапазоном); хук под
   internals/bench-internals; quiescence `decommit.rs` ~78–85. Тест:
   `t4_retain_reset_clears_stale_bits_recarve_other_class`; мутант oc-mut2-c RED.
6. **Гашение при carve — только стартовых бит pending, RMW `fetch_and`.**
   Выполнено: `small_sidecar.rs:225–229` + `small_issue_run.rs:60`;
   `CARVE_CLEAR_BITS ≥ 1` в состязательном тесте
   (`oneclass_mutant_battery.rs:9` — «delta ≥ 1»), 0 в чистых бенчах; мутант
   oc-mut2-a RED.
7. **Счётчики: публичный `foreign_or_unroutable_frees` — та же семантика;
   внутренние owner-отказы как «наблюдённые отклонённые записи/биты».**
   Выполнено: `REMOTE_REJECT_UNISSUED/FREE/MAG` (`counters.rs:549–583`, Relaxed
   fetch_add на пути отказа, гейт internals) — тестируются как наблюдённые биты
   (`oneclass_starts_map.rs:380`, `r12_01`/`r6_u21`); `CARVE_CLEAR_*` — погашенные
   биты. Публичный счётчик не менял семантики (producer-side drops).
8. **Карточки:** item 246 — носитель бита 7 листа удалён, 246 остаётся OPEN (запись
   в карточке); item 166 CLOSED — по тесту live+32 (без мутаций), trigger 2 не
   потребовался; item 247(b) — поподпунктное закрытие (см. карточку 247 в
   `docs/correctness-open-items/TRACKED_correctness_residuals.md`, D1′ phase-1b
   partial closure 2026-10-20): закрыты (i) покрытие классом batch/non-fastbin
   free (класс теперь только из state byte, owner reclaim принимает ровно
   `encode(c, LIVE)` во всех сборках) и (ii) stale class bytes после in-route
   reset (классовых байтов больше нет). Остаются ОТКРЫТЫМИ: sub-item (a)
   safe-linking, отсутствующий batch `divides`-чек, недопуск
   aligned wrong-class указателей в default-сборке; остаток «единственный источник
   класса» — §7, новая запись в correctness-индексе.

## 5. Замеры

Полностью — `docs/perf/B4_ONECLASS.md` (судьи J1 + судья памяти, скрипт
`scripts/b4_oneclass_table.mjs`, RC=0, CSV `docs/perf/B4_ONECLASS_summary.csv`).
Ключевое (ИЗМЕРЕНО): mstress 380→347 (−33, GO), larson 282→278 (−4, GO), batch alloc
114→96 (−18, GO), ao/aoc нерегресс, batch free/committed Δ=0, sidecar full
−926 336 Б (исправленный oc3-судья памяти; ~64 КиБ/занятый маршрут против
~48 КиБ+листья в A; старые mem-числа отозваны — дефект судьи, см.
B4_ONECLASS.md §5), foreign 136→134 (−2; подтверждено повторным замером на
финальном дереве b3; порог ≤ −4 НЕ достигнут — FAIL-THRESHOLD),
thrash64 free +3 Ir/op — NO-GO (oc7-раунд: NO-GO строки воспроизведён
независимо; построчная атрибуция: собственно own-free путь не менялся
(Δ=0.0), прирост — в неизменённых инlined-функциях окна hash.rs/segment_table_impl
(«сдвиг инлайнинга/раскладки» — гипотеза с числами по строкам); 4 попытки
layout-only атрибутов — без эффекта; см. B4_ONECLASS.md §8.1).

## 6. Тест-доказательства

- Loom: `tests/loom_oneclass_start_bit.rs` (старт-бит ↔ publish ↔ owner-drain,
  пауза producer'а перед RMW, мусорный free никогда не выданной гранулы),
  `tests/loom_oneclass_small_sidecar_actual.rs`; обновлены
  `loom_sidecar_bitmap*.rs`, `loom_b4_scan_skip_empty_publish.rs`,
  `loom_b4_r1_pin_cache.rs` (`<scratch>/oc-1a-loom*.log` — зелёные).
- Native: `tests/oneclass_starts_map.rs` (t1/t2/t4/t5/t6),
  `tests/oneclass_mutant_battery.rs`, переписанные `r12_01`, `r6_u21`,
  `r6_terminal_owner_drain{,_model}`, `state_byte_heap_magazine`,
  `r11_p3_small_sidecar_routes` (System-вызовов на маршрут = 1).
- Мутант-таблица — `docs/perf/B4_ONECLASS.md` §7: M-k RED
  (`mutk_reclaim_live_record_start_bit_0_is_fatal`); порядок reclaim: старт-бит-0
  fatal ДО тихих FREE/MAG-return; M-b оракулы переписаны (соседний бит);
  M-h — зафиксированный остаток (§7); T4 — реально другой класс; T6 — все
  small-классы (`SIZE_CLASS_TABLE.len()`); MAG no-op — во всех сборках;
  safe-observer `class_at_global_address_for_test` заменён на
  `start_state_at_global_address_for_test` (только атомарные банки sidecar).

## 7. OWNER-ACCEPTED RESIDUAL (ADR proposed; design choice accepted by owner)

The owner explicitly accepts **state byte as the sole class source** for D1′. D1′ has no independent class copy after leaf removal. A geometrically compatible wrong-class LIVE-byte mutation (`bs(d)` divides `off`, `off+bs(d) ≤ bump`) is not detected by geometry/overlap alone; smaller or co-sized substitutions may leak the block tail instead of overlapping a neighbor. The design decision is owner-accepted; reintroducing an independent witness is deferred unless the owner requests it. The ADR itself remains proposed pending owner adoption; that does not make the accepted design decision unaccepted.

M-h's separate deterministic production-path early-publication defect is closed by the test in `tests/oneclass_carve_batch_early_publication.rs`; T4's same-route reset fixture is separately repaired and repeat-verified. The flaky T1 repeat-harness defect (wrong absolute `live_count` model; 2 failures in ~738 runs) was fixed by a deterministic segment-delta oracle — 200 Windows + 200 WSL repeats, 0 failures, mutant RED 3/3 — recorded as correctness `RESOLVED.md` §252 (evidence `<scratch>/oc_flake_*`).

Перенос на main: D1′ переносится merge-коммитом `294ff32a` × `8da6402d` (незавершённый merge в ветке `b4-oneclass`). После ревью порта (P2-1/P2-2: measurement-счётчики RouteDirectory гейтированы `all(internals, bench-internals)`) выполнен PORT2-перезамер всех 26 ячеек: числа совпали с port-раундом в пределах порога (кроме A aoc c16/c64 alloc 21→22); NO-GO thrash64 (free +3 Ir/op, EstCycles +3.1 %) и foreign (−2 при цели ≤ −4) — без изменений; канонические RSS-числа — PORT2-генератор (full −1052, sparse −1500, cold −1968, deadpins +200 КиБ; sidecar +48 896 Б). Отчёт: `docs/perf/B4_ONECLASS.md` PORT2-раздел; evidence-реестр: R-Ph3a-native/-iai → superseded, новая строка `R-D1P-01`.

## 8. Отклонённые варианты

- **D1 (удалить листья, решение в producer-RMW) — NO-GO.** Окно rush №1:
  исполнение «фильтр до carve, сон, RMW после гашения» освобождает живой блок;
  множество вредных исполнений строго шире нынешнего (§8.0 дизайна, два ревью).
  Закрыто D1′ переносом точки решения в Acquire-загрузку старта.
- **S2 (ослабление проверок: слить MAG/FREE, убрать геометрию reclaim из
  production) — NO-GO.** Убирает детектор порчи; геометрия reclaim и fatal-классы
  сохранены во всех сборках (условие 3); оркестратор: подъём защитных бюджетов без
  решения владельца не принимается.
- **C‴ (бюджет item 82 за счёт цены producer'а) — NO-GO (§4.6 дизайна).** Бюджеты
  item 82 — Δ «B против A = main»; запрет «no post-hoc budget»; плюс C‴ провалил
  strict trim +99, bounded maintenance +99, construction +42, mimalloc-контроль.
  Item 82 не переоткрывать.

## 9. Miri / CI-заметки

- Miri-шаг `class_leaves_publication_miri` удалён вместе с файлом; сентинелы
  `loom_actual_leaves_*` / `negative_early_leaf_release` заменены сентинелами
  carve-clear Loom-тестов; `verify-dbg-hook-safety.mjs` PASS на новой таблице хуков.
- Осталось на CI: J2 iai A/B (решающие + нерегресс + mimalloc-контроль Δ=0), J3
  mt-bench (медиана ≥ −2 %), Miri-клетки лиз/битмапов, wall-clock только CI.
- Реестр доказательств: stale-строки `src/alloc_core/segment/remote_bitmap`,
  `src/registry/segment_route`, `src/registry/heap_core_xthread` — перепроверка
  владельцем через EV-1 (§6.4 дизайна).
