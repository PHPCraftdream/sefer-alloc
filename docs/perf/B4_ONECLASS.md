# B4 ONE-CLASS (D1′: карта стартов вместо ClassLeaves) — отчёт об измерении

Дата 2026-10-20. Фазы 1a + 1b D1′ (одна карта класса — байт состояния; карта стартов
«старт выдавался» в sidecar; ClassLeaves, `IssueTransaction`/`IssueRun`/`prepare_*`
и spill-OOM удалены; production drain — однопроходный; OPT-пакет: per-run
`SmallIssueRun` вместо per-block guard-цепочки).

Метки: **ИЗМЕРЕНО** — число из сырого лога `docs/perf/_raw_b4_oneclass_*.log`
(закреплены `tree-diff-sha256` в начале каждого лога); **ОЦЕНКА** — вывод, всегда с
диапазоном. Все таблицы сгенерированы скриптом
`scripts/b4_oneclass_table.mjs` со строгими assert'ами арифметики
(Δ = B − A точно; проценты с округлением до 0.1; committed Δ ≠ 0 → FAIL).

## Identity

Manifest digest: `d9779ff4b816c1cb8af6c7fbad2dd3d6e3b2391b6bea9aac027b4c7b0fd2afed` (source manifest includes tracked and untracked source/tests/scripts, including the table generator; excludes this report, generated CSV, and `docs/perf/_raw_*.log`).



## 1. Резюме

Плечи:

- **A** = `oc-base*` — база `bbc3c7b4` (b4-sbyte-строка: байт состояния + ClassLeaves
  + issue-транзакция);
- **B′** = `oc-b*` — интермедиат-плечо: фазы 1a+1b **до** OPT-правки (per-block
  `issue_small` guard-цепочка);
- **B** = `oc-b2*` — голова с OPT (per-run `SmallIssueRun`).

Судьи J1 (callgrind T=1, маргинально 50k→100k, WSL) — ИЗМЕРЕНО:

| Плечо | Метрика | A | B′ | B | Δ=B−A | Δ% | Порог | Вердикт |
|---|---|---|---|---|---|---|---|---|
| mstress | Ir/step | 380 | 386 | 347 | −33 | −8.7 % | GO ≤ −15 | **GO** |
| larson | Ir/step | 282 | 283 | 278 | −4 | −1.4 % | GO ≤ −1.5 | **GO** |
| foreign | Ir/free | 136 | 137 | 134 | −2 | −1.5 % | цель ≤ −4 (§8.4c) | **FAIL порога (цель не достигнута)** |
| ao | alloc Ir/op | 50 | 51 | 46 | −4 | −8 % | Δ ≤ +1 | **GO** |
| ao | free Ir/op | 92 | 92 | 92 | 0 | 0 % | Δ ≤ +1 | **GO** |
| aoc/rt | alloc/free | 50/92 | 51/92 | 46/92 | −4/0 | −8/0 % | Δ ≤ +1 | **GO** |
| aoc/c16 | alloc/free | 22/54 | 22/54 | 22/54 | 0/0 | 0 % | Δ ≤ +1 | **GO** |
| aoc/c64 | alloc/free | 22/54 | 22/54 | 22/54 | 0/0 | 0 % | Δ ≤ +1 | **GO** |
| batch | alloc/free | 114/140 | 132/140 | 96/140 | −18/0 | −15.8/0 % | alloc ≤ −5; free ≤ +1 | **GO/GO** |
| batchc | alloc/free | 94/110 | 112/110 | 76/110 | −18/0 | −19.1/0 % | alloc ≤ −5; free ≤ +1 | **GO/GO** |
| batchl | alloc/free | 120/155 | 138/155 | 102/155 | −18/0 | −15/0 % | alloc ≤ −5; free ≤ +1 | **GO/GO** |
| thrash64 | alloc Ir/op | 22 | 22 | 22 | 0 | 0 % | Δ ≤ +1 | **GO** |
| thrash64 | free Ir/op | 91 | 94 | 94 | **+3** | +3.3 % | Δ ≤ +1 и EstCycles ≤ +1 % | **NO-GO → владельцу** |

EstCycles (ИЗМЕРЕНО, информативно): ao alloc 61.4 → 50.8, ao free 99.4 → 99.1;
thrash64 free 100.2 → 103.7 (**+3.5 % > +1 %** — вторая составляющая NO-GO).

Итог: 10 из 11 решающих судей GO; thrash64 free +3 Ir/op (+3.5 % EstCycles) —
превышение порога, помечено владельцу (см. §8). foreign −2 < цели −4 — FAIL
порога (не «ZONE»), решение владельцу.

### Таблица судей (сгенерировано `scripts/b4_oneclass_table.mjs`; CSV:
`docs/perf/B4_ONECLASS_summary.csv`; колонки включают identity каждой строки:
| `shoulder_a_source_identity` / `shoulder_b_source_identity`; historical missing source = UNKNOWN; `snapshot_patch_sha256` is the external manifest digest. Perf rows do not inherit memory configuration.)

`node scripts/b4_oneclass_table.mjs` regenerates the summary CSV from repo raw logs and writes it only after all assertions pass; any parse/assert failure exits nonzero without overwriting CSV.

## 2. Методология

- **Профилировщик:** callgrind, T=1, маргинально 50k→100k итераций; WSL; генераторы
  те же, что у `scripts/b4_state_byte_table.mjs` (`prof_win.sh` larson|mstress,
  `prof_ao_win.sh`, `prof_ao_c_win.sh`, `prof_batch_win.sh`, `prof_batch_c_win.sh`,
  `prof_batchl_win.sh`, `prof_thrash64_dir.sh`).
- **Идентичность прогона:** в начале каждого сырого лога закреплена строка
  `tree-diff-sha256 <hex>` — хэш diff дерева измерявшегося плеча. Логи санитизированы
  (`~/`, `<repo>`, `<wsl>`), лежат в `docs/perf/_raw_b4_oneclass_*.log` (макс. размер
  19.3 КиБ — gzip не потребовался, порог 200 КиБ).
- **Memory judge inputs:** текущие repo raw logs `_raw_b4_oneclass_mem4-{a,b}-{full,sparse,cold,deadpins}.log`; результаты в таблице выводятся скриптом.

## 3. Интермедиат-плечо B′ и OPT-правка (история регрессии)

В плече B′ (фазы 1a+1b без OPT) batch-судьи регрессировали: alloc 114→132 (**+18**
Ir/блок на всех трёх batch-плечах: batch 132, batchc 112, batchl 138), mstress
380→386 (+6). Причина (ВЫВОД ИЗ КОДА + построчная атрибуция логов `oc-b-*`):
после удаления `IssueTransaction`/`prepare_*` каждая выдача блока в drain и
batch-carve шла через guard-цепочку per-block `issue_small` (fatal-сторожа слота на
каждый блок: index < cap, слот занят, `root == base`, есть Small-sidecar).

Исправление (OPT, плечо B): **per-run `SmallIssueRun`** — сторожа поднимаются один
раз на run (`small_issue_run(segment)`), внутри run — только
`clear_pending_start_bit(off)` + `issue(off)`:
- `src/alloc_core/segment/segment_table/small_issue_run.rs:60` —
  `clear_pending_start_bit`;
- `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:736,861` —
  один run на carve/carve-batch, `:438,:471,:558` — pop/drain;
- `src/alloc_core/segment/segment_table/active_kind_ops.rs:75` — комментарий о
  per-run семантике.

Результат (ИЗМЕРЕНО): batch alloc −18 Ir/блок против базы (GO ≤ −5 с запасом),
mstress 347 (−33), B′-колонка в таблице сохранена для честности истории.

## 4. Проба удалённого free (foreign)

`MARGINAL Ir/free (foreign locality)` (ИЗМЕРЕНО): A 136 → B′ 137 → B/b2 **134** (−2,
−1.5 %); повторный замер на ФИНАЛЬНОМ дереве фикс-раунда (b3/oc3, лог
`_raw_b4_oneclass_b3-foreign.log`) — те же **134** (Δ = −2): producer-путь не изменился
(reclaim/observer менялись — публикация нет). Цель D1′-гейта §8.4c — ≤ −4
(снятие чтения листа и extra-store на free); порог НЕ достигнут — вердикт
**FAIL-THRESHOLD** (категория ZONE удалена из гейта). Оракул `publish_ok`:
`B4_FOREIGN_PUBLISH_OK` (`src/registry/heap_core_xthread/routing.rs:46`) в foreign-пробе
соответствует числу удалённых free в ячейках r19 (см. §7).

### 4.1. Разбивка foreign-free по SELF-Ir: cached vs uncached, RMW vs gate (ИЗМЕРЕНО, фикс-раунд D1′-2)

Метод (исправление P2#7): два РАЗДЕЛЬНЫХ callgrind-прогона одной пробы
(`oc6_foreign2.rs`, окно `--toggle-collect='*zz_oc6_foreign2::w_free*'`,
80 000 foreign-free 256-Б блоков), атрибуция КАЖДОГО плеча — только из его
собственного лога, пофункционный SELF-Ir (листинг `callgrind_annotate` — это
self-стоимость; inclusive-кадры нигде не суммируются). Плечи подтверждены
оракулом счётчиков (bench-internals прогон без callgrind):
cached — `pin_hits=79 994 / pin_misses=6` (hit_frac 0,9999);
uncached — `pin_hits=26 176 / pin_misses=53 824` (miss_frac 0,673: 8
чередующихся маршрутов при 4 слотах pin-кэша дают промах на каждом free,
возвращающемся к вытесненному слоту). Отдельного «100 % uncached» плеча
не существует по построению кэша; ниже uncached-строки нормированы на
67,3 % промахов.

| Плечо | Ir/free (окно) | Доминанта self-Ir (файл:функция → Ir/free) |
|---|---|---|
| cached (hit_frac 0,9999) | **131,5** | `route_pin_cache.rs:publish_foreign` 19,5; `routing.rs` 16,0; `pin.rs` 13,0; std `atomic.rs` (в т.ч. RMW) 10,0; `sidecar_bitmap.rs` 6,0; std cell/bool/mem ~8,8 |
| uncached-плечо (смесь, 67,3 % промахов) | 402,4 | `directory.rs:Shard::locate` 82,1; `Shard::find` 25,6; `directory.rs:publish_foreign_uncached` 24,9; `route_pin_cache.rs` (fast+sweep) 39,4; `sweep_dead_slots` (сумма трёх кадров) ~43,5; `routing.rs:publish_foreign_uncached` 19,5 |

- Маржинальная цена ОДНОГО uncached publish (выведено из смеси:
  (402,4 − 0,327·131,5)/0,673) ≈ **534 Ir/free** — доминирует полный
  directory lookup (`Shard::locate`+`find` ≈ 108 Ir) и sweep мёртвых слотов
  (~44 Ir), а не публикация.
- RMW vs gate внутри `sidecar_bitmap.rs` (построчная аннотация обоих окон):
  gate — `starts[...].load` (start-load проверка) 320 000 Ir = **4 Ir/publish**,
  выравнивание `granule()` 160 000 Ir = **2 Ir/publish**; строка
  `pending[...].fetch_or(...)` — 0 self-Ir в файле: одиночная lock-инструкция
  RMW атрибутируется `core/src/sync/atomic.rs` (в uncached-окне весь
  `atomic.rs`-кадр `publish_foreign_uncached` = 1 344 596 Ir ≈ 25 Ir/free,
  включая все атомики пути, не только RMW). Отдельный вывод «RMW-доминанта»
  НЕ подтверждается: gate=6 Ir/publish, RMW-инструкция — единицы Ir;
  доминанта пути — lookup/sweep (см. таблицу). Прежние проценты
  «54,5 % filter / 2,3 % RMW» (суммы inclusive-кадров, raw помечен
  «DO NOT sum») отозваны.
- Логи: `docs/perf/_raw_b4_oneclass_oc6-foreign-{cached,uncached}-b.log`
  (проба `<scratch>/oc6_foreign2.rs`, драйвер `oc6_foreign.sh` /
  `oc6_run_foreign_win.sh`; identity-строка в шапке), построчная аннотация —
  `<scratch>/oc6-foreign-sidecar-lines.log`.

## 5. Судья памяти (полная таблица, ИЗМЕРЕНО; mem4-раунд)

**ОТЗЫВ СТАРЫХ ЧИСЕЛ:** таблицы до mem4-раунда недостоверны. mem/oc3-раунды: (1)
`cold` гонял churn через отдельный TLS-heap `SeferAlloc`, а reconciliation/committed
снимался с RAW lease-heap — committed Δ=0 относился к ДРУГОМУ heap; (2) cold-oracle
`alloc_calls >= 1` удовлетворялся инфраструктурной аллокацией и не доказывал
регистрации; A-side System alloc calls выдавались за число регистраций (это листья);
(3) deadpins-дельта = 3 sidecar (4 освобождённых минус 1 новый оставшийся cached pin),
а отчёт делил на 4 (старые per-route числа 49152/30816 Б ОТЗЫВАЮТСЯ); (4)
`route_registrations` брался из live census, а не из числа регистраций; (5) финальный
MEM-снимок снимался ПОСЛЕ drop fixtures; (6) resolved byte-cap заменялся
requested-only. Старые числа не приводятся и ни в какие таблицы не включаются.

Исправленный судья (mem4, `oc_mem_probe.rs` @ `<scratch>/`):
- baseline System-sidecar/pin-счётчиков ДО bootstrap/claim (симметрично);
- `cold` гоняет churn ЧЕРЕЗ тот же RAW lease-heap, с которого снимается
  reconciliation (один heap на workload и измерение); пул сегментов для cold
  ОТКЛЮЧЁН (`pool_segments(0)`), чтобы каждый цикл был реальной парой
  регистрация→release (включённый пул рециклирует сегменты без регистраций);
- cold-oracle = СЧЁТЧИК регистраций: новый диагностический хук
  `RouteDirectory::dbg_registrations_total()` (монотонный counter, +1 только на
  успешный `register_route`); assert: registrations_delta ≥ 61 = 95% от 64
  заявленных циклов; successful unlink / registration ≥95% — на обоих плечах 448/449 (99.78%), census reductions 448 в точности совпадают со счётчиком unlink.
- resolved byte-cap: новый read-only doc-hidden `bench-internals`-хук
  `dbg_pool_byte_cap_effective()` (`pool_cap × SEGMENT`, делегация
  HeapCore→AllocCore). ЧЕСТНО: байтовое ЗНАЧЕНИЕ ceiling'а в рантайме не
  хранится — резолюция сворачивает `min(pool_segments, byte_cap/SEGMENT)` в
  `pool_cap`; хук возвращает именно РЕЗОЛВИВШУЮСЯ operative-границу из
  сохранённого состояния (не requested-число), probe assert-ит
  `hook == dbg_pool_cap() × SEGMENT`;
- deadpins: знаменатель = фактическое число освобождённых пинов из счётчика
  `PIN_CACHE_SWEEPS` (per-pin reclaims; delta = 4, assert); sidecar-снимок
  делается ПОСЛЕ регистрации sweep-fixture, но ДО её insert (sweep идёт внутри
  insert), поэтому дельта mid→after содержит ТОЛЬКО освобождённые dead-sidecar;
- `route_registrations` = счёт вызовов `register()` в probe (7 = 6 fixtures +
  1 sweep-fixture), НЕ live census (census печатается отдельно с меткой);
- итоговый MEM-снимок делается ДО drop fixtures (fixtures остаются живыми),
  checkpoints в raw-логе размечены фазами.
- Cold A uses `bbc3c7b4` plus read-only instrumentation; B cold source manifest was not captured before build, so B cold source identity is UNKNOWN. These values are measured cold-archive outcomes, not final-tree identity.

| Режим | Метрика | A | B | Δ |
|---|---|---:|---:|---:|
| full | committed_bytes | 20 971 520 | 20 971 520 | 0 |
| full | system_sidecar_bytes | 1 254 016 | 327 680 | −926 336 |
| full | rss_kib | 4 528 | 3 492 | −1 036 |
| sparse (6144 × 4-KiB blocks; identical source/config) | committed_bytes | 29 360 128 | 29 360 128 | 0 |
| sparse | registrations / plan | 6/4 | 6/4 | equal; ≥95% |
| sparse | system_sidecar_bytes | 1 860 480 | 458 752 | −1 401 728 |
| sparse | rss_kib | 30 148 | 28 752 | −1 396 |
| cold (64 cycles; pool disabled) | committed_bytes | 8 388 608 | 8 388 608 | 0 |
| cold | route registrations | 449 | 449 | equal |
| cold | actual successful route unlinks / registrations | 448/449 (99.78%) | 448/449 (99.78%) | each ≥95% |
| cold | census removed / route unlink counter | 448/448 | 448/448 | matched |
| cold | system_sidecar_bytes | 462 592 | 131 072 | −331 520 |
| cold | rss_kib | 11 056 | 9 116 | −1 940 |
| deadpins | route_registrations | 7 | 7 | 0 |
| deadpins | retained_pins before/after sweep | 4→1 | 4→1 | equal |
| deadpins | actual freed pins | 4 | 4 | 0 |
| deadpins | dead_route_bytes | 164 352 | 262 144 | +97 792 |
| deadpins | bytes per freed pin | 41 088 | 65 536 | +24 448 |

Sparse activation is 6/4 in both arms; no sparse committed-delta multiplication is asserted. Both measured the same 6144-block probe under requested pool segments=8, byte cap=33554432 and resolved pool segments=8 / 33554432 bytes. Cold requested segments=8, byte cap=33554432, resolved pool=0 / 0 bytes in both arms. All eight current memory logs report config conflicts 0.

Cold release oracle uses actual `RouteDirectory::remove` successful-unlink counter and per-cycle census. Over 64 iterations each arm registered 449 routes and unlinked 448 (448/449 = 99.78%); the one remaining route is the process primordial route. Census reductions total 448, exactly matching unlink counter. This is not derived from System allocator free-call counts.

- **Historical/current identity policy:** historical mem4 full/deadpins B hashes are src+crates diffs only; sparse's manifest hash is historical and incomplete for current measurement source; current cold B identity is UNKNOWN. No write-tree identity is asserted. The external current source manifest includes tracked+untracked source/tests/scripts (including generator), and excludes this report, generated CSV, and raw logs explicitly.

The check script reads only repository raw files and writes CSV only after passing assertions. It checks workload/config equivalence for every paired mode, sparse activation and cold release oracles, and per-row arithmetic. External source manifest SHA-256 is included; excluded files: this report, generated CSV, raw measurement logs. The table run result and literal output are in the external `oc_resume_table_final.log`.

### 5.1. Цена overlap-check (ИЗМЕРЕНО изолированно, проба oc6_overlap_iso, фикс-раунд D1′-2)

ИЗОЛЯЦИЯ (исправление P2#6): предыдущая атрибуция (5,3/98,9 Ir, окна
`*trim_for_recycle*`) приписывала overlap-check'у self-Ir `bitmap_scan.rs` — это
PENDING-банк, а не starts-банк. Теперь производство overlap-вызова идёт через
doc-hidden `#[inline(never)]`-обёртку `AllocCore::overlap_check_probe_call`
(`alloc_core_small_reclaim.rs`, только `bench-internals`; вызывает ту же
`SegmentTable::reclaim_start_overlap` → `start_bits_between` → `bank_bits_between`
/`for_words_in_range`), и callgrind собирает ТОЛЬКО это окно
(`--toggle-collect='*overlap_check_probe_call*'`). Собранные TOTALS — ровно
production overlap-путь, без drain-контекста и без pending-скана.

Сценарий (как прежде): owner выдаёт блоки одного класса, foreign-поток
публикует их (1 запись/блок), owner `trim_current_thread()`. Во всех плечах
ОДИНАКОВЫЙ owner-объём 32 МиБ → `segments_reserved_total` на конце фазы аллокаций:
**9 = 9** (256 Б и 8 КиБ; мед-плечо 10 — на 1 сегмент больше за счёт warm-up
worker'а блоками 256 КиБ; owner-объём идентичен). Число сегментов выведено в логи.

ГРАНУЛЫ (корректный текст; при MIN_BLOCK = 16 Б): 256 Б = **16 гранул**,
8 КиБ = **512 гранул**, 256 КиБ = **16384 гранул** (не 1/32, как было раньше).
Слов на запись: u64 покрывает 64 гранулы = 1024 Б ⇒ слов = ⌈bs/1024⌉ = 1 / 8 / 256
(+1 при невыровненном начале интервала). «Раннего выхода первого слова» в
`bank_bits_between` НЕТ — проход весь диапазон (подтверждено ревью); таблица
ниже это и измеряет.

| Класс | Гранул/запись | Слов/запись | Записей (N) | Изолированный overlap Ir/запись | Ir/слово |
|---|---|---|---|---|---|
| 256 Б | 16 | 1 | 131 072 | **57,0** (7 471 104 / 131 072) | — |
| 8 КиБ | 512 | 8 | 4 096 | **149,0** (610 304 / 4 096) | ≈ 13,1 (марж.) |
| 256 КиБ (medium-classes) | 16 384 | 256 | 128 | **3 373,0** (431 744 / 128) | ≈ 13,2 (марж.) |

- Medium ИЗМЕРЕН (не оценка): тот же сценарий, features `medium-classes`,
  класс 256 КиБ. Маржинальная цена слова сходится: (149−57)/7 ≈ 13,1 и
  3 373/256 ≈ 13,2 Ir/слово — цена линейна по ⌈bs/1024⌉, без раннего выхода.
- Правки против прежнего текста: гранулы 16/512/16384 (не 1/32); «ранний
  выход первого слова» снят (его нет в `bank_bits_between`); атрибуция
  5,3/98,9 (bitmap_scan.rs, pending-банк) отозвана — это не overlap-путь.
- Логи: `docs/perf/_raw_b4_oneclass_oc6-overlap-{c256,c8k,med}.log`
  (проба `<scratch>/oc6_overlap_iso.{rs,sh}`, драйвер `oc6_run_overlap_win.sh`;
  identity-строка в шапке лога — дерево b4-oneclass + обёртка-хук).

## 6. Оракулы активации (§6.2; все проверены)

1. **Однопроходный drain:** production drain — валидация цепочки до мутации, затем
   один проход с `issue_run.issue(head_off)`
   (`alloc_core_small_impl.rs:451–558`; continuation-проверки перенесены, не удалены —
   `r6_t06_freelist_continuation_bounds` зелёный). Однопроходность закреплена кодом +
   r6_t06 continuation-тестом; bench-счётчик повторных чтений `next` не внедрён —
   остаток (perf OPEN_ITEMS, M-h-карточка correctness-индекса).
2. **System-вызовов на маршрут = 1:** `tests/r11_p3_small_sidecar_routes.rs` — один
   System-аллокатор sidecar'а (32 КиБ), без промотированных листьев
   (`:41` — «System allocation, no promoted leaves (D1' phase-1b)»).
3. **CARVE_CLEAR_*:** `CARVE_CLEAR_BITS` = 0 в чистых бенчах и ≥ 1 в
   состязательном тесте: `tests/oneclass_starts_map.rs` t1/t4 + мутант-батарея
   `tests/oneclass_mutant_battery.rs` (`muta_carve_clear_drops_stale_pending_at_recarve`,
   «CARVE_CLEAR_BITS delta ≥ 1»); `CARVE_CLEAR_WORD_LOADS` — счётчик-оракул загрузок
   (`src/alloc_core/alloc_core/counters.rs:597,607`).
4. **Owner-отказы по причине:** `REMOTE_REJECT_UNISSUED/FREE/MAG`
   (`counters.rs:549–583`) — `tests/oneclass_starts_map.rs:380`
   («REMOTE_REJECT_FREE counted the observed bit»), переписанные
   `tests/r12_01_foreign_free_of_unissued_granule.rs` (кейс 0 — метаданные, публичный
   счётчик +1) и `tests/r6_u21_foreign_free_interior_granule.rs` (интерьер живого
   блока — owner-отказ, payload не тронут).
5. **publish_ok в foreign-пробе:** `B4_FOREIGN_PUBLISH_OK`
   (`src/registry/heap_core_xthread/routing.rs:46`, диагностический вывод
   `examples/b4_collapse_probe.rs:391` `b4_foreign_publish_ok`) равен числу
   удалённых free в ячейках r19.
6. **Activation oracles:** sparse identical probe/config on A/B allocates 6144 × 4-KiB blocks; each arm observes 6 registrations / 4-route plan (threshold 4). Cold runs 64 iterations in each arm; each registers 449 routes, successfully unlinks 448 (99.78%), and per-cycle census reductions equal unlink counter (448).

## 7. Мутанты (статусы по факту фикс-раунда; RED — где указано)

Батарея: `tests/oneclass_mutant_battery.rs` (child-abort-схема `ocmut_child`) +
`tests/oneclass_starts_map.rs`; прогоны `<scratch>/oc-mut2-*.log` (локальные, не в
репо). ИЗМЕРЕНО = `test result: FAILED` в логе.

| Мутант | Что ломается | Краснеет тестом | Лог |
|---|---|---|---|
| M-a | carve-clear не гасит stale pending при recarve | `muta_carve_clear_drops_stale_pending_at_recarve` **FAILED** | oc-mut2-a |
| M-b | публикация/старт-биты не атомарны (load+store вместо RMW); оракулы переписаны (соседний бит) | `loom_oneclass_small_sidecar_actual_battery` **FAILED** | oc-mut2-b |
| M-c | retain-reset диапазон неточен (low/middle/high) | `mutc_retain_reset_range_is_exact_low_middle_high` — child abort (STATUS_STACK_BUFFER_OVERRUN = fatal от flawed-мутанта) | oc-mut2-c |
| M-d | reclaim принимает нулевой байт старта | `mutd_reclaim_zero_state_byte_at_start_is_fatal` **FAILED** | oc-mut2-d |
| M-e | reclaim бинит не по классу байта | `mute_reclaim_bins_into_the_state_byte_class` — child abort | oc-mut2-e |
| M-f | publish принимает метаданные (payload-фильтр снят) | `mutf_publish_metadata_offset_is_rejected_before_the_start_gate` **FAILED** | oc-mut2-f |
| M-g | drain без continuation-границ | `drain_misaligned_aborts`, `drain_out_of_segment_aborts` **FAILED** | oc-mut-g |
| M-h | ранняя публикация стартов batch (d1p_rev находка 3) | ОСТАТОК: не закреплён детерминированным оракулом на allocator-path; закрыт контрактом поблочного set + loom `issue_precedes_handoff`; correctness-индекс (карточка 251) | — |
| M-k | reclaim живой записи со старт-битом 0 не fatal (порядок: старт-бит-0 fatal ДО тихих FREE/MAG-return; MAG no-op — во всех сборках) | `mutk_reclaim_live_record_start_bit_0_is_fatal` **FAILED** | oc-mut2-k |
| M-l | reclaim не проверяет перекрытие выданных стартов | `mutl_reclaim_overlap_of_issued_start_is_fatal` **FAILED** | oc-mut2-l |

M-h is **CLOSED by the deterministic production-path test** (`oneclass_carve_batch_early_publication`, 1 passed, RC=0); its mutant battery is counterfactual RED (9 tests passed, RC=0). The old measurement-round M-h row remains historical context, not an open correctness residual. This performance report is not the full correctness matrix.

Loom-модель «старт-бит ↔ publish ↔ owner-drain» (включая паузу producer'а перед RMW
и мусорный free никогда не выданной гранулы): `tests/loom_oneclass_start_bit.rs`,
`tests/loom_oneclass_small_sidecar_actual.rs` — зелёные (`<scratch>/oc-1a-loom*.log`).
Обоснованно-эквивалентные (не мутанты, а эквивалентные проверки):
`t1_garbage_tail_free_is_rejected_and_never_reclaimed`,
`t2_interior_free_of_live_block_is_rejected_payload_intact`,
`t4_retain_reset_clears_stale_bits_recarve_other_class`,
`t5_misaligned_free_is_rejected`,
`t6_each_class_remote_free_reclaims_once_class_from_byte`
(`tests/oneclass_starts_map.rs:93–370`) — все зелёные на голове.

## 8. Известные отклонения

1. **thrash64 free: +3 Ir/op (91→94) и EstCycles +3.5 % (100.2→103.7) — NO-GO по
   порогу «Δ ≤ +1 Ir/op и EstCycles ≤ +1 %». Пометка владельцу.** ПОСТРОЧНАЯ
   АТРИБУЦИЯ (oc7-раунд, воспроизведена заново: A-плечо = чистый
   `git archive bbc3c7b4`, B = финальное дерево; A дал ровно 22/91 Ir и
   22.0/100.2 EstCycles — методика и числа base/b2 подтверждены):
   `dealloc_own_base` = 16.0 Ir/op в ОБОИХ плечах (Δ=0.0);
   `segment_header_views` = 7.9 (Δ=0.0); весь +3.0 Ir/op — в НЕизменённых
   (git-diff против bbc3c7b4 пуст) статических функциях того же окна,
   инlined-кластер `canonical_base_of`→`hash_find`:
   `segment_table_impl.rs` (last_key/own_cache-проверки) 18.8→19.7 (Δ+0.9:
   `if i >= count` +0.3, `own_cache[index] = …` +0.2, `root == key` +0.2);
   `hash.rs` 15.3→17.5 (Δ+2.2: hash-выражение `(key^key>>13^key>>26)&mask`
   +1.0, probe-строки `hash_find` `entry&!mask==base` и
   `root=if id==0…base_at` по +0.4); `core::ptr` +0.2. Формулировка
   «сдвиг инлайнинга/раскладки» — ГИПОТЕЗА, теперь с числами по строкам:
   добавленной инструкции на own-free пути нет, но россыпь ±0.2–1.0 Ir по
   инlined-строкам может быть как выравниванием, так и иным порядком/
   дубликацией инlined-копий.
   ПОПЫТКИ УСТРАНЕНИЯ (все — production-нейтральные атрибуты; логика и
   проверки НЕ тронуты; каждая с полным замером B-плеча thrash64):
   | Попытка | Атрибут | free Ir/op | EstCycles | Вердикт |
   |---|---|---|---|---|
   | 1 | `hash_slot_write` `#[inline]`→`#[inline(always)]` (hash.rs:49) | 94 | 103.7 | без эффекта |
   | 2 | `own_cache_clear` `#[inline]`→`#[inline(always)]` (impl:844) | 94 | 103.7 | без эффекта |
   | 3 | `canonical_block_of` `#[inline]`→`#[inline(always)]` (mem_impl.rs:41) | 94 | 103.7 | без эффекта |
   | 4 | `canonical_base_of{,_mut}` `#[inline(always)]`→`#[inline]` (impl:738,806) | 94 | 103.7 | без эффекта |

   Логи: `_raw_b4_oneclass_oc7-attempt-{1..4}.log`; все 4 правки откачены,
   дерево идентично pre-attempt (tree-diff `7659fcf3…`). Итог: +3 Ir/op не
   устраняется доступными production-нейтральными атрибутами раскладки;
   это НЕ утверждение о невозможности в принципе — не исключены иные приёмы
   (реорганизация инlined-кластера, `#[cold]` на холодных ветках), требующие
   отдельного раунда. Свежий замер B на финальном дереве
   (`_raw_b4_oneclass_oc7-thrash64-b-attr.log`) — те же **94 Ir/op /
   103.7 EstCycles**: стабильно, не шум. **NO-GO строки остаётся; построчная
   атрибуция приложена** (решение владельцу; AO-клетка и batch от
   регрессии свободны).
2. **foreign −2 < цели −4 (§8.4c)** — порог НЕ достигнут (FAIL-THRESHOLD);
   подтверждено повторным замером на финальном дереве (b3: 134, §4/§4.1);
   доминанта цены — filter/lookup (55–59 %), не RMW-публикация.
3. B′-регрессия batch +18 — устранена OPT-правкой (§3); в таблице оставлена как
   история.
4. **Cold release oracle:** both arms registered 449 routes, unlinked 448 (99.78%; ≥95%), and each saw 448 census reductions matching the successful-unlink counter. Sidecar System free calls are not used as route-release evidence.
5. **T4 route-selection fix:** parent repeat-5 roots `0x201d2800000`/`0x201d3000000` at `tests/oneclass_starts_map.rs:302` showed one other-class allocation did not ensure same route; class cursor/free-list may choose another segment. Fixture checks real candidate roots in a bounded 4096 allocation sequence, retains non-target blocks and MUST reach same route (not same offset). Different class, reset-cleared bits, no stale drain and control drain=1 remain. Thirty repeats and final rerun passed.

## 9. Что осталось на CI

- **J2 iai A/B** (`perf_gate_iai`, `r20_r6_ab_iai_run.mjs`): решающие
  `recycle_alloc_only_256x16b` ≤ −5 %, `recycle_alloc_free_256x16b` ≤ −3 %,
  `cold_alloc_only_256x16b` ≤ −2 %, `growth16_n` ≤ −2 %, `r19_prf01_*`,
  `seg_cycle_decommit_256k`, `multiseg_cold_256k` ≤ 0; нерегресс hot ≤ +0.5 % Ir и
  EstCycles ≤ +1 %; own-free клетки |Δ| ≤ 2 Ir; контроль `mimalloc_*` Δ = 0.
  Примечание: J2 iai идёт со сборкой `production bench-internals internals`; в этой
  сборке в плече B компилируется M-h test-seam (2 store + 1 load на batch) — смещение
  консервативное (против B).
- **J3 mt-bench** (T=1/2/4, 2 прогона): медиана ≥ −2 %, INFO ≥ +2 % при T=4.
- **Miri-клетки:** `tests/class_leaves_publication_miri.rs` удалён (шаг CI снят);
  Miri-режимы лиз/битмапов (`segment_directory_a5_miri`, `r11_ph4a/b lease miri`,
  `regression_r6_t22_foreign_window_key_miri`) — перегнать на голове.
- Wall-clock ничего не мерилось локально (правило R29-6).

## 10. Сырые логи и ссылки

- Логи: `docs/perf/_raw_b4_oneclass_{base,b,b2}-{mstress,larson,ao,aoc,batch,batchc,batchl,thrash64,foreign}.log`,
  `docs/perf/_raw_b4_oneclass_b3-{foreign,thrash64}.log` (финальное дерево oc3),
| `docs/perf/_raw_b4_oneclass_mem4-{a,b}-{full,sparse,cold,deadpins}.log` (current memory evidence; identity types are recorded per measurement mode)
- Фикc-раунд D1′-2 (P2#6–8): `docs/perf/_raw_b4_oneclass_oc6-overlap-{c256,c8k,med}.log`
  (изолированный overlap-путь, §5.1),
  `docs/perf/_raw_b4_oneclass_oc6-foreign-{cached,uncached}-b.log`
  (SELF-Ir пофункционно + построчно, §4.1),
  `_raw_b4_oneclass_mem4-{a,b}-sparse.log` (симметричный rerun: оба плеча выделили 6144×4 КиБ и получили 6 регистраций при плане 4; прежний вакуумный A-сценарий не используется).
- CSV: `docs/perf/B4_ONECLASS_summary.csv`.
- Дизайн-ревью: `<scratch>/art/b4/oneclass_design.md` (§6.1 пороги, §6.2 оракулы, §8.4
  D1′); ревью D1′: `<scratch>/art/b4/d1p_rev_rush.md`, `<scratch>/art/b4/oneclass_rev_rush.md`.
- ADR: `docs/design/2026-10-20-adr-oneclass-starts-map.md`.

## Перенос на main 8da6402d (D1′-PORT, перезамер)

Дата 2026-10-21; merge `294ff32a` × `origin/main` (`8da6402d`), содержимое разрешено, merge `d48534ea` закоммичен в ветке `b4-oneclass` (PR #94); perf-карточки перенесены/обновлены в `docs/perf/OPEN_ITEMS.md` (109–111) — вручную владельцу ничего не остаётся.

- Плечо A: чистый main `8da6402d` + read-only патч `oc_mem_hooks_a2.patch` (УСТАРЕЛО: патч a2/a3 добавляет measurement-счётчики, гейтированные `all(internals, bench-internals)` в обоих плечах — см. PORT2)
  (sha256 `5e01b6c82272a0e43720b186c358d3a65486cea011b870479999fb7d965db586`).
  Write-tree `40fd66d75637e8ac163d8f33384b3e719bad2299`;
  source-manifest `c93793c7259682e248bbab98c134311a14c86713a608c2d2cdc189ff9ef6f936`.
- Плечо B: объединённое дерево. Write-tree `5a73e3377ee37c8ca11fbbc57f28821ca37be1c0`;
  source-manifest `d0baf7dd2d7c2d25e341a008fca2a02d41a18af5d4febe06e199777979ebe776`.

### Таблица судей (измерено, callgrind T=1, маргинально 50k→100k; перезамер на объединённом дереве)

> **ИСТОРИЧЕСКОЕ (port-раунд 1):** эта таблица и `docs/perf/B4_ONECLASS_PORT_summary.csv`
> сгенерированы версией `b4_oneclass_port_table.mjs` из коммита `294ff32a`; скрипт HEAD
> port-раунд 1 не воспроизводит (сейчас он генерирует только port2-раунд).
> CSV и таблица ниже оставлены как зафиксированное историческое свидетельство, без изменений.

A = port-base (чистый origin/main 8da6402d + read-only хуки a2 — УСТАРЕЛО: патч a2/a3 добавляет measurement-счётчики, гейтированные `all(internals, bench-internals)` в обоих плечах, см. PORT2), B = port-b (объединённое дерево b4-oneclass × 8da6402d); B′-колонка пуста — интермедиат на PORT-этапе не измерялся. Построчные проверки: 46 арифметик / 46 процентов / 0 mismatches; RC=0.

| Плечо | Метрика | A | B′ | B | Δ=B−A | Δ% | Порог | Вердикт |
|---|---|---|---|---|---|---|---|---|
| mstress | Ir/step | 368 |  | 336 | -32 | -8.7 % | GO ≤ −15; ZONE −15<Δ≤−5 | GO |
| larson | Ir/step | 279 |  | 275 | -4 | -1.4 % | GO ≤ −1.5; NO-GO ≥ 0 | GO |
| foreign | Ir/free | 136 |  | 134 | -2 | -1.5 % | цель ≤ −4 (D1′-гейт §8.4c) | FAIL-THRESHOLD |
| ao | ao alloc Ir/op | 49 |  | 45 | -4 | -8.2 % | Δ ≤ +1 Ir/op | GO |
| ao | ao alloc EstCycles | 60.4 |  | 49.8 | -10.6 | -17.5 % | информативно | MEASURED |
| ao | ao free Ir/op | 87 |  | 87 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| ao | ao free EstCycles | 94.4 |  | 94.1 | -0.3 | -0.3 % | информативно | MEASURED |
| aoc/rt | alloc Ir/op | 49 |  | 45 | -4 | -8.2 % | Δ ≤ +1 Ir/op | GO |
| aoc/rt | free Ir/op | 87 |  | 87 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| aoc/c16 | alloc Ir/op | 21 |  | 22 | +1 | +4.8 % | Δ ≤ +1 Ir/op | GO |
| aoc/c16 | free Ir/op | 49 |  | 49 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| aoc/c64 | alloc Ir/op | 21 |  | 22 | +1 | +4.8 % | Δ ≤ +1 Ir/op | GO |
| aoc/c64 | free Ir/op | 49 |  | 49 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| batch | alloc Ir/блок | 108 |  | 89 | -19 | -17.6 % | alloc GO ≤ −5; NO-GO > +1 | GO |
| batch | free Ir/блок | 135 |  | 135 | 0 | 0 % | free Δ ≤ +1 | GO |
| batchc | alloc Ir/блок | 88 |  | 69 | -19 | -21.6 % | alloc GO ≤ −5; NO-GO > +1 | GO |
| batchc | free Ir/блок | 105 |  | 105 | 0 | 0 % | free Δ ≤ +1 | GO |
| batchl | alloc Ir/блок | 114 |  | 95 | -19 | -16.7 % | alloc GO ≤ −5; NO-GO > +1 | GO |
| batchl | free Ir/блок | 140 |  | 140 | 0 | 0 % | free Δ ≤ +1 | GO |
| thrash64 | alloc Ir/op | 22 |  | 22 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| thrash64 | free Ir/op | 81 |  | 84 | +3 | +3.7 % | Δ ≤ +1 Ir/op И EstCycles ≤ +1 % (Δcyc=3.1 %) | NO-GO |
| thrash64 | free EstCycles | 90.3 |  | 93.1 | +2.8 | +3.1 % | EstCycles ≤ +1 % (точно 3.1 %) | NO-GO |

### Вердикт по предрегистрации

- Регресс deadpins: +24 448 Б на освобождённый dead pin (+59 %: 41 088→65 536), sidecar +48 896 Б, RSS +264 КиБ — принятая владельцем цена вместе с NO-GO thrash64/foreign.

- mstress — GO; larson — GO; ao/aoc — GO; batch/batchc/batchl — GO.
- NO-GO строки, принятые владельцем как компромисс ради mstress/batch/памяти:
  1. thrash64 free Ir/op 81→84, Δ=+3 (+3.7 %), порог ≤ +1 — NO-GO;
  2. thrash64 free EstCycles 90.3→93.1, +3.1 % — NO-GO;
  3. foreign 136→134, Δ=−2 при цели ≤ −4 — FAIL-THRESHOLD.
- NO-GO строки: причина — те же строки/функции, что в fix2-раунде (hash_slot_write/own_cache_clear/canonical_base_of кластер, атрибутные попытки без эффекта); на PORT-этапе новые попытки устранения не предпринимались; владелец принял компромисс.

### Судья памяти (измерено, port raw-логи)

| Режим | Метрика | A | B | Δ |
|---|---|---|---|---|
| full | committed | 20971520 | 20971520 | 0 |
| full | sidecar | 1254016 | 327680 | -926336 |
| full | rss_kib | 4516 | 3552 | -964 |
| sparse | committed | 29360128 | 29360128 | 0 |
| sparse | sidecar | 1860480 | 458752 | -1401728 |
| sparse | rss_kib | 30184 | 28728 | -1456 |
| cold | committed | 8388608 | 8388608 | 0 |
| cold | sidecar | 462592 | 131072 | -331520 |
| cold | rss_kib | 11036 | 9052 | -1984 |
| deadpins | committed | 4194304 | 4194304 | 0 |
| deadpins | sidecar | 82176 | 131072 | +48896 |
| deadpins | rss_kib | 2396 | 2660 | +264 |

- committed Δ=0 во всех четырёх режимах.
- Cold-оракул (оба плеча): registrations=449, unlink_routes=448, census_removed=448 → 448/449 = 99.78 %; census_removed=448=unlink.
- Sparse-оракул: registrations_delta=6 (mid) / 6 (post-free) ≥ план 4, оба плеча (A и B идентичны).
- Deadpins: registrations 7=7; sweep_freed 4=4; dead_route_bytes 164352→262144; per-freed-pin 41088→65536.
- Config: full/sparse/deadpins requested segments=8,byte_cap=33554432 → resolved segments=8,byte_cap=33554432; cold resolved segments=0,byte_cap=0 (ожидаемо); `config_conflicts_total=0` во всех 8 mem-логах.

### Сырые логи и RC

- Raw `-port` файлы (26 шт.): 9 × `_raw_b4_oneclass_port-base-{ao,aoc,batch,batchc,batchl,foreign,larson,mstress,thrash64}.log`, 9 × `_raw_b4_oneclass_port-b-{ao,aoc,batch,batchc,batchl,foreign,larson,mstress,thrash64}.log`, 8 × `_raw_b4_oneclass_port-{a,b}-{full,sparse,cold,deadpins}.log`.
- Логи RC: `<scratch>/oc_remeas_table.log` (RC=0), `<scratch>/oc_remeas_identity.log`.

### Этап проверки (замечание)

- Адресные тесты D1′: M-h 1, battery 9, starts_map 5, reclaim_mag 1×2, state_byte 1+10, sgie 1, refill 1+1, r6/r12/r14/r11_ph5b — все PASS RC=0; main-сторона: b4_micro_s1 2, xnull 2, xspan 2, n9 1, u31 2 — все PASS RC=0.
- Мутанты M-h и refill-accounting снова RED и откачены.
- Loom: 7/7 групп PASS. run-check-matrix: ALL GREEN (один flake-инцидент 0xC0000409 в oneclass_starts_map, 1 из 4, повторно не воспроизвёлся — 10/10 повторов PASS; зафиксирован честно; УСТАРЕЛО: флейк впоследствии воспроизведён — 2 падения из 805 прогонов (5+200+200+100+300) — и исправлен, см. correctness RESOLVED-карточку 252).
- clippy 5/5; fmt-check RC=0; verify-*.mjs 13/15 RC=0, verify-structure-limits RC=0 (после переноса carve_batch в diag), verify-evidence-registry RED (УСТАРЕЛО: «stale-строки незакоммиченного merge — ожидаемо, реестр не правился» — неверно: 17 из 23 нарушений были вызваны D1′ и устранены правкой реестра — R-Ph3a-native/-iai superseded → R-D1P-01; остаток RED — только reverify-строки EV-1).
- A-worktree удалён после замера.

## PORT2 (гейтирование measurement-счётчиков): перезамер после P2-1/P2-2

Причина перезамера: в port-раунде оба плеча содержали негейтированные measurement-счётчики RouteDirectory (P2-1), достигнутые через расширение приватности `directory.rs` (P2-2). После гейтирования счётчиков (`all(internals, bench-internals)`) и в плече A, и в плече B выполнен повторный прогон всех 26 ячеек: 26 новых raw-логов `docs/perf/_raw_b4_oneclass_port2-*.log` (identity и feature set — в шапке каждого лога, RC — в хвосте).

### Identity плеч port2

- A = port2-base: origin/main `8da6402d` + `oc_mem_hooks_a3.patch` (sha256 `1ff7e14000bb078836d7e455732322b857192bcf69439d2e95cbb03c97c9a1ce`); write-tree `02db40ed426458ebdb351babdcf6e263c9b3cd36`; source-manifest-sha256 `086180efab1faf2bfa9889f4194355bb83cda9a69dbec83a5e2b9e9f992c89b3`. Шапка raw: `identity: shoulder=a base=8da6402d patch=oc_mem_hooks_a3.patch(sha256=1ff7e140…) write-tree=02db40ed… source-manifest-sha256=086180ef…`.
- B = port2-b: объединённое дерево b4-oneclass × 8da6402d; write-tree `19aa0fcf11fa955a7a245f0d338141891ac38d92`; source-manifest-sha256 `58f83d6b71a7e8a16341cae2f1f9c6ccb65af351508659e7cfc85af9bf1e5fc0`. Шапка raw: `identity: shoulder=b base=8da6402d (origin/main) merged with 294ff32a (b4-oneclass D1-prime, mid-merge resolved) write-tree=19aa0fcf… source-manifest-sha256=58f83d6b… tree-diff-sha256(src+crates)=58f83d6b…`.
- Полнота идентичности: full write-tree/manifest identity recorded; tree objects not reachable until commit (pin refs at merge).

### Таблица судей port2 (ИЗМЕРЕНО; дословно вывод `scripts/b4_oneclass_port_table.mjs`; CSV `docs/perf/B4_ONECLASS_PORT2_summary.csv`)

B4 ONE-CLASS PORT2 — таблица судей (ИЗМЕРЕНО, callgrind T=1, маргинально 50k→100k; перезамер после гейтирования measurement-счётчиков в обоих плечах)
A = port2-base (чистый origin/main 8da6402d + oc_mem_hooks_a3.patch, sha256=1ff7e140…), B = port2-b (объединённое дерево b4-oneclass × 8da6402d); B′ в port2-раунде отсутствует; feature set в шапках raw-логов («features:»)

| Плечо | Метрика | A | B′ | B | Δ=B−A | Δ% | Порог | Вердикт |
|---|---|---|---|---|---|---|---|---|
| mstress | Ir/step | 368 |  | 336 | -32 | -8.7 % | GO ≤ −15; ZONE −15<Δ≤−5 | GO |
| larson | Ir/step | 279 |  | 275 | -4 | -1.4 % | GO ≤ −1.5; NO-GO ≥ 0 | GO |
| foreign | Ir/free | 136 |  | 134 | -2 | -1.5 % | цель ≤ −4 (D1′-гейт §8.4c) | FAIL-THRESHOLD |
| ao | ao alloc Ir/op | 49 |  | 45 | -4 | -8.2 % | Δ ≤ +1 Ir/op | GO |
| ao | ao alloc EstCycles | 60.4 |  | 49.8 | -10.6 | -17.5 % | информативно | MEASURED |
| ao | ao free Ir/op | 87 |  | 87 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| ao | ao free EstCycles | 94.4 |  | 94.1 | -0.3 | -0.3 % | информативно | MEASURED |
| aoc/rt | alloc Ir/op | 49 |  | 45 | -4 | -8.2 % | Δ ≤ +1 Ir/op | GO |
| aoc/rt | free Ir/op | 87 |  | 87 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| aoc/c16 | alloc Ir/op | 22 |  | 22 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| aoc/c16 | free Ir/op | 49 |  | 49 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| aoc/c64 | alloc Ir/op | 22 |  | 22 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| aoc/c64 | free Ir/op | 49 |  | 49 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| batch | alloc Ir/блок | 108 |  | 89 | -19 | -17.6 % | alloc GO ≤ −5; NO-GO > +1 | GO |
| batch | free Ir/блок | 135 |  | 135 | 0 | 0 % | free Δ ≤ +1 | GO |
| batchc | alloc Ir/блок | 88 |  | 69 | -19 | -21.6 % | alloc GO ≤ −5; NO-GO > +1 | GO |
| batchc | free Ir/блок | 105 |  | 105 | 0 | 0 % | free Δ ≤ +1 | GO |
| batchl | alloc Ir/блок | 114 |  | 95 | -19 | -16.7 % | alloc GO ≤ −5; NO-GO > +1 | GO |
| batchl | free Ir/блок | 140 |  | 140 | 0 | 0 % | free Δ ≤ +1 | GO |
| thrash64 | alloc Ir/op | 22 |  | 22 | 0 | 0 % | Δ ≤ +1 Ir/op | GO |
| thrash64 | free Ir/op | 81 |  | 84 | +3 | +3.7 % | Δ ≤ +1 Ir/op И EstCycles ≤ +1 % (Δcyc=3.1 %) | NO-GO |
| thrash64 | free EstCycles | 90.3 |  | 93.1 | +2.8 | +3.1 % | EstCycles ≤ +1 % (точно 3.1 %) | NO-GO |

### Судья памяти port2 (ИЗМЕРЕНО; дословно вывод генератора)

Судья памяти port2 (ИЗМЕРЕНО, port2 raw-логи; расходы памяти A/B из этих логов)
| Режим | Метрика | A | B | Δ |
|---|---|---|---|---|
| full | committed | 20971520 | 20971520 | 0 |
| full | sidecar | 1254016 | 327680 | -926336 |
| full | pins | 0 | 0 | 0 |
| full | descriptor_entries | 0 | 0 | 0 |
| full | rss_kib | 4524 | 3472 | -1052 |
| sparse | committed | 29360128 | 29360128 | 0 |
| sparse | sidecar | 1860480 | 458752 | -1401728 |
| sparse | pins | 0 | 0 | 0 |
| sparse | descriptor_entries | 0 | 0 | 0 |
| sparse | rss_kib | 30172 | 28672 | -1500 |
| cold | committed | 8388608 | 8388608 | 0 |
| cold | sidecar | 462592 | 131072 | -331520 |
| cold | pins | 0 | 0 | 0 |
| cold | descriptor_entries | 0 | 0 | 0 |
| cold | rss_kib | 11060 | 9092 | -1968 |
| deadpins | committed | 4194304 | 4194304 | 0 |
| deadpins | sidecar | 82176 | 131072 | 48896 |
| deadpins | pins | 1 | 1 | 0 |
| deadpins | descriptor_entries | 1 | 1 | 0 |
| deadpins | rss_kib | 2440 | 2640 | 200 |
| deadpins | route_registrations | 7 | 7 | 0 |
| deadpins | dead_route_bytes | 164352 | 262144 | 97792 |
| deadpins | dead_route_bytes_per_freed_pin | 41088 | 65536 | 24448 |
| deadpins | retained_pins_after_sweep | 1 | 1 | 0 |


### Сравнение port vs port2

Числа port2 практически байт-в-байт совпадают с port. Единственный сдвиг: A aoc c16/c64 alloc 21→22 (в пределах порога Δ ≤ +1, вердикты GO без изменений). Verdict-картина port2 идентична port: mstress/larson/ao/aoc/batch* — GO; thrash64 free Ir/op +3 (+3.7 %) и free EstCycles +3.1 % — NO-GO без изменений; foreign −2 при цели ≤ −4 — FAIL-THRESHOLD без изменений. В memory-судье sidecar +48 896 Б и per-freed-pin +24 448 Б (+59 %) воспроизведены без изменений; RSS deadpins в port2 — +200 КиБ (2440→2640) против +264 КиБ в port-таблице выше (колебание RSS, вывод не меняет).
