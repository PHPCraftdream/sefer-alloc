# Ревью `src/` — раунд 17 (oxx) — логическая корректность алгоритмов и инвариантов (однопоточная семантика и учёт)

## 1. Охват, инвентарь, метод, ограничения, вердикт

**База:** `main` @ `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`). С базы R16 (`6a0d47f6`) прошло 20 коммитов. Шесть из них трогают `src/`: 22 файла, +130/−122 строки. Это исправления R15-01, R15-02, R16-01 и R16-03, perf item 84 и правка формулировок по item 154 (`git log --oneline 6a0d47f6..453439c1 -- src/`).

**Ревьюер:** oxx (Claude Opus 5.5, effort=max). Один контекст, без суб-агентов, в изолированном git worktree. Тема — одна из восьми параллельных: логическая корректность алгоритмов и инвариантов в однопоточной семантике, а также учёт. Многопоточные гонки вынесены в §4.

**Режим: ревью проведено без компиляции и исполнения кода.** Так распорядился владелец (приказ передан координатором). Не запускались `cargo` (build, check, test, clippy, rustdoc, miri, kani, bench), `rustc`, witness-тесты и мутанты.

Использовались только инструменты чтения:
- чтение файлов и `rg`/grep;
- `git` только на чтение: `log`, `show`, `diff`, `rev-parse`, `ls-files`, `status`;
- `find`/`wc` для подсчёта строк.

Ожидаемое и фактическое поведение в находках вычислено вручную по коду. Классы доказательства — только SOURCE-CONFIRMED и ГИПОТЕЗА; класс «исполненный witness» в этом режиме недоступен. Временных файлов не создавалось. Единственное изменение в дереве — этот отчёт.

**Инвентарь** (пересчитан командами из приложения A, а не скопирован). 168 Rust-файлов, 43 325 физических строк:
- 17 059 непустых строк, не начинающихся с `//`;
- 24 303 строки `//`, `///`, `//!`;
- 1 963 пустые строки.

По группам:

| Группа | Файлов | Строк |
|---|---:|---:|
| `src/alloc_core/` | 88 | 24 802 |
| `src/registry/` | 46 | 10 413 |
| `src/global/` | 18 | 3 809 |
| `src/concurrent/` | 14 | 3 554 |
| `src/lib.rs` + `src/kani_proofs.rs` | 2 | 747 |

Unsafe-инвентарь по шаблону из CLAUDE.md: в `src/` 21 модульный allow (tier 1) и 83 allow на уровне элементов (tier 2) в 30 файлах. Совпадает с R16.

**Метод.**
1. **Полное построчное чтение** кода и комментариев:
   - `src/alloc_core/`:
     - `platform/{size_classes,os,sidecar,sidecar_stats}.rs`;
     - `alloc_core/{mod,state,alloc_core_impl,bootstrap,lifecycle,sidecar_drain,counters}.rs`, `alloc_core/mem/{mem_impl,realloc_fastpath}.rs`;
     - `large/{alloc_core_large,alloc_core_large_cache,alloc_core_large_cache_eviction,large_cache_extended,reservation_state}.rs`;
     - `small/alloc_core_small/{alloc_core_small_impl,find_segment,reserve,dealloc,directory}.rs`, `small/alloc_core_small_pool/{alloc_core_small_pool_impl,decommit}.rs`, `small/{alloc_core_small_magazine,alloc_core_small_reclaim,reserved_small_segment}.rs`;
     - `segment/segment_table/{segment_table_impl,hash,active_kind_index,active_kind_ops,route_slots}.rs`, `segment/segment_directory/segment_directory_impl.rs`, `segment/segment_header/{segment_header_layout,segment_header_impl,segment_header_meta_fields,block_kind,descriptors}.rs`, `segment/bitmap/{segment_bitmap,alloc_bitmap,magazine_bitmap}.rs`, `segment/segment_layout.rs`, `segment/remote_bitmap/{sidecar_bitmap,bitmap_scan,bitmap_cut}.rs` и `.../sidecar_bitmap/leaf_classes.rs`;
     - `config/{large_cache_config,small_segment_pool_config,profile,large_cache_mode}.rs`.
   - `src/registry/`: `heap_core/alloc/{hot,batch}.rs`, `heap_core/state/{tcache,ownership,tcache_flush}.rs`, `heap_core/free/{dealloc_own_base,dealloc,realloc,dealloc_batch}.rs`, `heap_core/core.rs`, `heap_core_xthread/routing.rs`, `heap_registry/{maintenance,claim,counters}.rs`, `segment_route/small_sidecar.rs`.
   - `src/global/`: `alloc_stats.rs`, `sefer_alloc/{diag,global_alloc}.rs`, `exact_object/{narrow,exact_shard,exact_table}.rs`.
   - Вне `src/`: `crates/size-classes/src/lib.rs` (строки 180–970); `docs/INVARIANTS.md`; `docs/LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md`; `docs/perf/R34_11_CATCHUP_DECAY_GATE.md` §1–§6; отчёт R16.
2. **Выборочное чтение по диапазонам:** `src/registry/segment_route/directory.rs` (1–290, 700–909), `crates/aligned-vmem/src/api/reserve.rs` (1–80), `tests/r8_large_alignment.rs` (1–50), `tests/large_cache_decay.rs` (заголовки тестов).
3. **Скрининг grep-запросами** с чтением каждого попадания:
   - сужающие приведения `as u8`/`as u16`;
   - ветки `align >= SEGMENT`;
   - все места записи `last_decay_tick`, `large_cache_decay_op_count`, `large_cache_used_bytes`, `last_pool_decay_tick`;
   - вызовы `maybe_decay_large_cache()`;
   - инкременты счётчиков статистики относительно веток отката;
   - пары `Segment::reserve*` / `mem::forget` / `release_segment`;
   - `LARGE_REMOTE_RETIREMENTS`;
   - функции `fallback.rs`.
4. Для каждой находки цитируемые строки перечитаны на базе. Дубликаты проверены grep-ом по `docs/perf/OPEN_ITEMS.md`, `docs/CORRECTNESS_OPEN_ITEMS.md` и `docs/correctness-open-items/*.md`. Оба индекса прочитаны в части карточек, которые пересекаются с темой: perf 42; correctness 154, 158, 164, 166 и соседние.

**Не прочитаны вглубь (только скрининг):**
- `src/concurrent/*`;
- `src/registry/bootstrap/*`, `src/registry/heap_slot.rs`;
- `src/registry/segment_route/*`, кроме перечисленного выше;
- `src/global/{fallback,tls_heap,maintenance_service}.rs`, `src/global/sefer_alloc/{mod,core,batch,maintenance}.rs`;
- `src/alloc_core/platform/{node,numa}.rs`;
- `segment_header/{segment_header_gen_table,segment_header_views,terminal_words,layout_asserts}.rs`;
- все `*_diag*.rs`, `segment_state_*.rs`, `decomp_hooks.rs`, `sidecar_test_hooks.rs`;
- `src/kani_proofs.rs`, `src/lib.rs`.

Утверждения «не найдено» ниже относятся только к прочитанному.

**Не запускалось ничего, ни на одном хосте:** сборка, тесты, witness, мутанты, Miri, Loom, Kani, TSan/ASan, бенчмарки и iai, feature-powerset, clippy/rustfmt/rustdoc. NUMA и платформы кроме Windows рассматривались только по коду. CI-логи не читались. Числовой эффект R17-LOG-01 (hit-rate, RSS, латентность) не измерен.

**Вердикт.** Три новые находки.
- **Два P3.**
  - R17-LOG-01 — долг таймера decay large-кэша. Catch-up R34-11 превращает простой ниже headroom и повторное использование слота в повторяющиеся 8-шаговые вытеснения. Это противоречит публичному контракту «минимальный интервал между тиками» и «never more aggressively».
  - R17-LOG-02 — `large_cache_hits` засчитывает попадание, которое потом откатилось в медленный путь.
- **Один P4.** R17-LOG-03 — `docs/INVARIANTS.md` M2/M4/M6 (и формула M7) противоречат коду, а M2 ссылается на удалённые тесты.

Ни одна находка не исполнена; все установлены по исходнику с ручным расчётом. P0, P1 и P2 в прочитанной области прочитанным методом не найдены. Это не доказательство отсутствия ошибок, не release GO и не perf GO.

## 2. Находки

Шкала из задания:
- **P0** — эксплуатируемая memory-safety/UB в production-конфигурации из безопасного кода.
- **P1** — memory-safety/UB при нарушении контракта, достижимая из безопасного публичного API или принятая как known-box.
- **P2** — реальный дефект корректности, утечка или взаимная блокировка в штатной работе.
- **P3** — ограниченный дефект корректности, ресурса или диагностики без нового production memory-safety эксплойта.
- **P4** — вводящий в заблуждение контракт, пробел покрытия или поддерживаемость без установленного runtime-сбоя.

Классы доказательства:
- **SOURCE-CONFIRMED** — путь или противоречие установлены по тексту базы, ожидаемое и фактическое посчитаны вручную, без исполнения.
- **ГИПОТЕЗА** — указано условие, при котором она ложна.

| ID | Severity | Класс | Достижимость | Суть |
|---|---|---|---|---|
| R17-LOG-01 | P3 | SOURCE-CONFIRMED (трасса + ручной расчёт) | `production` (`alloc-decommit`). Публичные `GlobalAlloc::alloc`/`dealloc` для Large-блоков с `align < SEGMENT`. Проявляется, когда кэш потока выше `headroom_bytes`: штатно для `LargeCachePolicy::LowHeadroom` (16 MiB) и `Trimmed64MiB` (64 MiB) через `SeferAlloc::with_config`; при 256 MiB по умолчанию — когда кэш больше 256 MiB | Таймер decay не двигается, пока кэш ≤ headroom, и не сбрасывается при триме и повторном захвате слота. Catch-up R34-11 продвигает таймер не больше чем на 8 интервалов за чтение часов, а остаток переносит как долг. Пока долг не погашен, каждое чтение часов (раз в 64 Large-вызова выше headroom) даёт до 8 шагов вытеснения. Публичные доки обещают минимальный интервал между тиками и «never earlier, never more aggressively» |
| R17-LOG-02 | P3 | SOURCE-CONFIRMED | `alloc-decommit` + `alloc-stats` (публичный `SeferAlloc::stats().large_cache_hits`) или `internals` (`AllocCore::dbg_large_cache_hits`). Срабатывает, когда `register_payload` отказывает на попадании в кэш: таблица полна (4096 сегментов) или регистрация маршрута падает по OOM | Счётчик попаданий увеличивается до `register_payload`. Если регистрация отказала, span из кэша освобождается, запрос обслуживает `alloc_large_slow`, а попадание остаётся засчитанным |
| R17-LOG-03 | P4 | SOURCE-CONFIRMED (текст против кода; проверка существования файлов) | Спецификация, общая для всех сборок. M4 касается фасадов `AllocCore` и `GlobalAlloc`, M6 — `production` | `docs/INVARIANTS.md`: M4 обещает `null` для `align >= SEGMENT`, а код и тесты выдают такие блоки. M6 описывает decommit с последующим recommit, которых в production нет. M2 опирается на удалённый `RemoteFreeRing` и два удалённых теста. Формула M7 неверна для biased Large |

### R17-LOG-01 — catch-up decay large-кэша копит долг устаревшего таймера и вытесняет агрессивнее контракта

**Места.**

`src/alloc_core/large/alloc_core_large_cache.rs`:
- `:523–525` — быстрый выход при `large_cache_used_bytes <= headroom_bytes`, до любого касания таймера и счётчика страйда.
- `:539–555` — страйд: часы читаются раз в `DECAY_CLOCK_CHECK_STRIDE = 64` (`:24`) вызовов выше headroom.
- `:559–583` — сброс счётчика, `Instant::now()`, прайминг при `None` и выход при `elapsed < decay_interval`. Обоснование прайминга (`:564–567`): иначе первая операция увидит «an arbitrarily large "elapsed" … potentially flushing the cache unnecessarily».
- `:598–615` — catch-up R34-11:
  - `ratio = min(elapsed / interval, DECAY_CATCHUP_MAX_STEPS)` (`:605–606`), где константа равна 8 (`:43`);
  - таймер становится `t + interval * due` (`:608–610`);
  - затем выполняется `due` шагов (`:613–615`).
- `:626–640` — `run_decay_step` считает `release = excess × rate_bp / 10 000`.

`src/alloc_core/large/alloc_core_large_cache_eviction.rs:34–56` — `evict_at_least` вытесняет целые FIFO-записи, минимум одну, пока `released < min_bytes`.

Кто пишет `last_decay_tick`:
- конструктор (`src/alloc_core/alloc_core/lifecycle.rs:323`);
- `alloc_core_large_cache.rs:568, 602, 609`;
- два тестовых хука (`alloc_core_large_cache_eviction.rs:109, 141`).

Других мест записи нет (grep по `src/`). Трим при выходе потока (`src/registry/heap_core/state/ownership.rs:140–156`, вызывает `evict_all`) и повторный захват слота (`src/registry/heap_registry/claim.rs:317`) таймер не трогают. `AllocCore` слота реестра живёт весь процесс.

Контракт, который нарушается:
- `src/alloc_core/config/large_cache_config.rs:307–308` — «Set the minimum wall-clock interval between decay ticks».
- `src/alloc_core/config/large_cache_config.rs:340–350` — тик «fires up to 63 events late (never early)».
- `src/alloc_core/config/profile.rs:189–191` — для `LowHeadroom`: «decay ticks firing up to ~63 large ops later than before (never earlier, never more aggressively)».
- Внутренние doc-комментарии: `alloc_core_large_cache.rs:455–463` и `:490–491` («the sub-interval remainder carries over honestly»), `src/alloc_core/alloc_core/alloc_core_impl.rs:155` («Minimum wall-clock interval between consecutive decay ticks»).

Поведение введено коммитом `73dcecab` (2026-08-04, R34-11). Раньше код выполнял ровно `self.last_decay_tick = Some(now); self.run_decay_step();` (`docs/perf/R34_11_CATCHUP_DECAY_GATE.md:39–41`).

**Минимальный вход и трасса** (ручной расчёт). Обозначения: `U` — `large_cache_used_bytes`, `n` — счётчик страйда, `T` — `last_decay_tick`.

Конфигурация `LargeCachePolicy::LowHeadroom`:
- headroom 16 MiB (`profile.rs:427`);
- интервал 1000 мс и темп 10 % — значения по умолчанию (`large_cache_config.rs:51, 54`; `:460–474` переводит 10 % в 1000 bp).

Блоки `Layout(3 MiB, 16)`: `hdr_aligned = 4096`, `needed = 3 MiB + 4 KiB`, `usable = 4 MiB`. Одна запись кэша занимает 4 MiB.

Решение о decay принимается в начале `alloc_large` (`src/alloc_core/large/alloc_core_large.rs:137–139`) и в начале Large-ветки `dealloc`, до депозита (`src/alloc_core/alloc_core/mem/mem_impl.rs:288–291`).

1. **t = 0 с.** Выделить 8 блоков и освободить все.
   - Освобождения 1–5 видят `U` = 0, 4, 8, 12, 16 MiB и выходят по быстрому пути.
   - Шестое видит 20 MiB > 16 MiB: `n = 1`, но `T = None`, поэтому часы читаются сразу (`n := 0`, `T := 0 с`), шага нет.
   - Седьмое и восьмое дают `n` = 1, 2 и выходят по страйду.
   - Итог: `U = 32 MiB`.
2. **t ≈ 0 с.** Выделить 5 блоков (попадания в кэш).
   - Вызовы при `U` = 32, 28, 24, 20 дают `n` = 3…6 и выходят по страйду.
   - Пятый вызов видит `U = 16` и выходит по быстрому пути.
   - Итог: `U = 12 MiB ≤ headroom`.
3. **0 < t < 100 с.** Large-операций нет, либо все идут при `U ≤ 16 MiB`. Каждая выходит в `:523–525`; `T` остаётся равным 0 с.
4. **t = 100 с.** Освободить эти 5 блоков: `U = 32 MiB`, `n = 9`. Дальше цикл alloc+free того же размера: попадание и депозит, `U` колеблется 32/28 MiB. Часы читаются на 55-м вызове цикла (`n = 64`). Это alloc при `U = 32 MiB`, `excess = 16 MiB`.

| | Ожидаемое (публичный контракт; так же вёл себя код до R34-11) | Фактическое (база) |
|---|---|---|
| Шаги на этом чтении | 1 шаг: `release = 1,6 MiB`, вытеснена 1 запись (4 MiB), `U = 28 MiB`; `T := 100 с` | `ratio = min(100, 8) = 8`. Шаги 1–4 вытесняют по записи (excess 16 → 12 → 8 → 4 → 0), шаги 5–8 пустые. `U = 16 MiB` — 4 вызова `release_segment` внутри одного `alloc`. `T := 0 + 8 = 8 с` |
| Следующий допустимый тик | не раньше t = 101 с | долг 92 интервала |

5. **t = 100,5 с.** Программа освобождает ещё 4 живых блока того же размера — выделенных заранее и не участвовавших в шагах 1–4. Кэш снова выше headroom: `U` поднимается с 16 до 32 MiB. Цикл продолжается.
   - **Ожидаемое:** до t = 101 с ни одного шага, потому что с последнего тика прошло 0,5 с < 1 с.
   - **Фактическое:** на следующем чтении часов (через 64 вызова выше headroom, это могут быть микросекунды) `elapsed = 100,5 − 8 = 92,5 с`. Снова 8 шагов, весь excess вытеснен, `T := 16 с`.
   - Так продолжается на каждом чтении, пока `T` не догонит часы. При плотном трафике это около ⌈92/8⌉ = 12 чтений, примерно 768 Large-вызовов выше headroom. При редком — дольше: долг растёт на время между чтениями.

**Наблюдено (текст базы).**
- Быстрый выход стоит перед страйдом и таймером (`:523`).
- При истинном `elapsed / interval > 8` таймер продвигается ровно на `8 × interval` (`:605–610`). Перенесённый остаток — это `(ratio − 8)` целых интервалов плюс доля, а не «sub-interval remainder», как утверждает doc (`:490–491`).
- Ни опустошение кэша, ни трим, ни повторный захват слота таймер не сбрасывают (см. «Места»).
- Публичные доки `large_cache_config.rs` и `profile.rs` после R34-11 не обновлялись: catch-up в них не упомянут (`rg -n "catch" src/alloc_core/config` — пусто).

**Выведено (арифметика по коду, не исполнено).**
- **Любой долгий `elapsed` превращается в серию 8-шаговых пакетов**, по пакету на каждое чтение часов, пока долг не погашен. Источники долгого `elapsed`: простой ниже headroom, простой слота между потоками, первое использование давно взведённого таймера. Это тот самый риск «arbitrarily large elapsed», ради которого код праймит таймер (`:564–567`). Только здесь он срабатывает на каждом чтении, а не однажды.
- **Пример с повторно захваченным слотом.** Поток A взвёл таймер и вышел; трим вытеснил кэш. Час спустя слот захватывает поток B. Его следующие ≈ 3600/8 = 450 чтений часов выше headroom дают по 8 шагов. Фактически headroom работает как жёсткий потолок, проверяемый раз в 64 операции.
- **Разреженный режим самого гейта R34-11.** Пауза 150 мс на событие, интервал decay 100 мс, два вызова `maybe_decay` на событие (`docs/perf/R34_11_CATCHUP_DECAY_GATE.md` §2.1, §3.1). Чтения часов идут раз в 64 / 2 × 1,5 ≈ 48 интервалов decay, поэтому долг растёт на 48 − 8 = 40 интервалов за чтение, без ограничения. После перехода к плотному трафику каждое чтение даёт 8 шагов, пока долг не погашен.
- **Почему гейт R34-11 этого не увидел.** В 40-интервальном прогоне у throttled-руки ровно одно чтение часов, на интервале 30 (Table 3, `docs/perf/R34_11_CATCHUP_DECAY_GATE.md:154–173`); второго чтения нет. В throughput-руке `elapsed < interval` на каждом чтении (§4 гейта).
- **Цена.** Лишние `release_segment`, затем новые OS-резервирования и холодные страницы, ниже hit-rate выше headroom. Память не портится и не утекает (освобождается даже больше), поэтому это P3, а не P2. Масштаб в hit-rate, RSS и латентности не измерен.

**Связь с известным.** Это новое свидетельство к perf item 42 (`docs/perf/OPEN_ITEMS.md:1121–1156`, `[D]`). Item 42 описывает обратный край того же механизма: decay недостаточно агрессивен, пиковый разрыв ограничен страйдом. Новое здесь:
1. неограниченный перенос долга при `ratio > 8`;
2. таймер не обновляется ниже headroom и при повторном использовании слота;
3. противоречие публичным докам (`large_cache_config.rs:307–308, 346–347`, `profile.rs:189–191`).

Оформить это отдельным пунктом или дополнением к 42 — решение владельца индексов.

**Рекомендация.**
1. **Обязательная часть, без новых чтений часов.** При `elapsed / interval > DECAY_CATCHUP_MAX_STEPS` ставить `T := now` (или `now − elapsed % interval`). Сдвиг `t + interval·due` оставить только для `ratio ≤ 8`. Разреженный режим гейта R34-11 от этого не меняется: там каждое чтение и сейчас даёт 8 шагов.
2. **Сброс в `evict_all`.** Ставить `last_decay_tick := None` в `evict_all`, то есть при триме потока, повторном захвате слота и явном `trim_current_thread`. Пустой кэш — свежий старт, как у нового heap; таймер взведётся при первом чтении.
3. **Первое чтение после простоя ниже headroom** даже с п. 1 даёт до 8 шагов. Есть два варианта:
   - задокументировать пакет до 8 шагов в `large_cache_config.rs` и `profile.rs`;
   - перевзводить таймер на переходе «≤ headroom → > headroom». Это стоит одного чтения часов на каждый переход, то есть возвращает обрыв R32-8 при колебаниях вокруг headroom; нужен гейт (§3, H1).
4. **Публичные доки.** `decay_interval_ms` после R34-11 задаёт темп, а не минимальный интервал между тиками; это и пакет catch-up нужно описать явно.

**Oracle исправления** (красный до, зелёный после). Тест в `tests/` с features `alloc-core alloc-decommit internals`, `AllocCore::new()`, `dbg_set_decay_config(1000, 50, 16 MiB)`, блоки `Layout(3 MiB, 16)`:
- (а) 8 alloc + 8 free → `U = 32 MiB`, таймер взведён. Дополнительно держать 4 живых блока X1…X4.
- (б) 5 alloc → `U = 12 MiB`.
- (в) `sleep(1 s)` — 20 интервалов.
- (г) 5 free → `U = 32 MiB`. Циклы alloc+free, пока не упадёт `dbg_large_cache_used()`. Это первое чтение часов; и до, и после фикса здесь 4 вытеснения, так что это ещё не oracle.
- (д) Сразу освободить X1…X4 и прогнать 64 цикла alloc+free — один страйд, миллисекунды, много меньше 50 мс.

Утверждение: `dbg_large_cache_used() > 16 MiB`.
- **До фикса:** `elapsed ≈ 1 с − 8 × 50 мс ≈ 0,6 с`, 8 шагов, `U ≤ 16 MiB` — тест красный.
- **После п. 1** (в (г) поставлено `T := now`): `elapsed` много меньше 50 мс, 0 шагов, `U` равно 28 или 32 MiB — тест зелёный.

Тест устойчив к задержкам: чтобы фикс тоже довёл `U` до 16 MiB, внутри одного страйда нужна пауза не меньше 4 интервалов (200 мс).

Для п. 2 — второй тест, тот же приём через лиз реестра (`alloc-global`, где есть `evict_all`):
1. заполнить кэш до 32 MiB;
2. выполнить трим — кэш пуст, `U = 0`;
3. подождать 20 интервалов;
4. снова заполнить кэш до 32 MiB и прогнать 64 цикла alloc+free.

До фикса таймер остаётся взведённым, срабатывает страйд, и на 64-м вызове выше headroom приходит пакет из 8 шагов: `U ≤ 16 MiB`. После п. 2 первый вызов выше headroom только взводит таймер, а к 64-му `elapsed` много меньше интервала: `U > 16 MiB`.

### R17-LOG-02 — `large_cache_hits` засчитывает попадание, откатившееся в медленный путь

**Места.** `src/alloc_core/large/alloc_core_large.rs`:
- `:248–266` — `large_cache_slot_take` и `begin_large_reuse`;
- `:287–295` — инкремент счётчика под `#[cfg(feature = "alloc-stats")]`;
- `:451–467` — `register_payload` возвращает `None`, затем `release_initializing`, `release_segment` и `return self.alloc_large_slow(...)`;
- `:483–484` — `finish_large_reuse` и возврат в успешной ветке.

Когда `register_payload` возвращает `None` (`src/alloc_core/segment/segment_table/segment_table_impl.rs`):
- нет свободных слотов и `count >= MAX_SEGMENTS`, где `MAX_SEGMENTS = 4096` (`:396–399`, `:78`);
- при `alloc-global` падает `routes.prepare(...)?` (`:403–408`) внутри `src/alloc_core/segment/segment_table/route_slots.rs:74–92`: либо `ensure_capacity`, либо `RouteDirectory::register(...).ok()`. OOM возникает в `src/registry/segment_route/directory.rs:789–807`.

Контракт: `src/global/alloc_stats.rs:57–69` — «Number of `alloc_large` calls served directly from the per-heap large-object cache».

**Трасса.** Попадание в кэш:
1. слот извлечён, `Cached → Initializing`;
2. счётчик +1;
3. точечные записи заголовка;
4. `register_payload` возвращает `None`;
5. `Initializing → Released`, `release_segment` закэшированного span;
6. `alloc_large_slow`: свежее резервирование и регистрация, возврат `(ptr, true)` (или `null`, если откажет и она).

**Минимальный вход** (ручной расчёт):
1. лиз heap реестра;
2. `alloc` + `free` блока `Layout(3 MiB, 16)` — span уходит в кэш;
3. `RouteDirectory::fail_next_registration_for_test()` (`directory.rs:716–720`, `internals`) — ровно одна следующая регистрация отказывает;
4. `alloc` того же `Layout`.

| Счётчик | Ожидаемое | Фактическое |
|---|---|---|
| `large_cache_hits` | +0 (запрос обслужен свежим резервированием, не кэшем) | +1 |
| `segments_reserved_total` | +1 | +1 |
| `segments_released_total` | +1 | +1 |

Каждое повторение добавляет одно ложное попадание и теряет один закэшированный span. В реальной работе триггер — OOM в `RouteDirectory::register` или 4096 зарегистрированных сегментов у одного heap.

**Наблюдено.** Инкремент стоит между успешным `begin_large_reuse` и `register_payload`. Других веток, где счётчик откатывается, нет.

**Выведено.** `stats().large_cache_hits` завышен ровно на число неудавшихся регистраций при попадании. Поведение аллокатора верное: freshness-флаг `true` у медленного пути корректен. Это ограниченный дефект диагностики, того же класса, что R15-02 (P3).

**Рекомендация.** Перенести блок `#[cfg(feature = "alloc-stats")]` за успешный `register_payload`, например прямо перед `finish_large_reuse`. Изменение живёт под `alloc-stats`, production-путь не меняется.

**Oracle исправления.** Сценарий выше как тест с features `alloc-global internals bench-internals alloc-stats`. Утверждения:
- приращение `dbg_large_cache_hits()` равно 0;
- приращения reserved и released равны +1.

Сейчас тест красный (приращение попаданий 1), после переноса — зелёный.

### R17-LOG-03 — `docs/INVARIANTS.md` M2/M4/M6/M7 противоречат коду; у остаточного предела M2 нет живого пина

**Наблюдено.**

**M4** (`docs/INVARIANTS.md:95–98`): «Requests with `align >= SEGMENT` are rejected with `null` by design (task #130)». В коде такие запросы обслуживаются:
- `src/alloc_core/large/alloc_core_large.rs:143–147` — заголовок для `align >= SEGMENT`;
- `:517–521` и `:630–639` — `numa::reserve_biased_on_node` и `os::Segment::reserve_biased`;
- `src/alloc_core/platform/os.rs:175–218` — biased-геометрия.

Тесты закрепляют успех: `tests/r8_large_alignment.rs:6–48` (`AllocCore`, `align` = 1, 2 и 16 × `SEGMENT`, ненулевой выровненный указатель) и `tests/r8_terminal_global.rs` (тот же класс через `GlobalAlloc`).

| Вход | По спецификации | По коду |
|---|---|---|
| `alloc(Layout(37, 2 × SEGMENT))` | `null` | ненулевой указатель, кратный 8 MiB |

**M6** (`:108–113`): «an empty small segment's payload pages are decommitted when its live-block count drops to zero and recommitted on first reuse». В production:
- `dec_live_and_maybe_decommit` только сообщает, что сегмент можно отдать (`src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:84–119`);
- `release_or_pool_empty_segment` либо кладёт сегмент в пул без decommit и без сброса метаданных, либо освобождает всю резервацию (doc `:167–215`, тело с `:255`);
- ветка decommit-and-retain не имеет production-вызовов (`decommit.rs:258–277`) и достижима только из `bench-internals`-хука `dbg_force_decommit_retain_for` (`decommit.rs:93–108`).

Общая часть M6 (память возвращается ОС, RSS ограничен) верна; описанный механизм — нет.

**M2** (`:62–77`):
- Остаточный предел описан как «queued in a segment's `RemoteFreeRing`». Такого типа в `src/` нет: correctness 22 закрыт в R16 как superseded.
- «Pinned by `tests/regression_xthread_double_free_residual.rs`; modelled by `tests/loom_magazine_ring_compose.rs`» — оба файла удалены коммитом `a4245965` (`git log --diff-filter=D`).
- Блок «UB-vs-soundness» ссылается на `tests/regression_xthread_double_free_residual.rs:71-89`.
- Код описывает тот же остаток уже через pending-биты sidecar маршрута и ссылается на несуществующий тест `residual_xthread_double_free_no_corruption` (`src/registry/heap_core/free/dealloc_own_base.rs:180–191`; в `tests/` такого имени нет).

**M7** (`:114–116`): «owning segment is found in O(1) via `segment_of(ptr) = ptr & ~(SEGMENT-1)`». Для biased Large маска даёт только ключ поиска. Корень берётся из хэша владельца (`src/alloc_core/alloc_core/mem/mem_impl.rs:34–42`, `src/alloc_core/segment/segment_table/hash.rs:244–258`); он выровнен только по странице и не равен ключу. Оценка O(1) верна, формула — нет.

**Попутно (не находка).** I1–I7 в том же файле относятся к Handle-фасаду `sefer-region`, что сказано в `:8–11`. Инварианты аллокатора — M1–M8.

**Выведено.** На эту спецификацию ссылаются тесты (`tests/alloc_core_differential.rs:6` — «M1–M4») и CLAUDE.md. Читатель или новый тест по M4 будет ждать `null` там, где выдаётся память. По M6 будут искать путь recommit, которого нет. У остаточного предела M2 после `a4245965` нет ни пина, ни модели. Runtime-сбоя от самого текста нет — поэтому P4.

**Рекомендация.**
- **M4:** описать biased-геометрию со ссылками на `docs/LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md` и `tests/r8_large_alignment.rs`.
- **M6:** описать пул и освобождение; явно указать, что ветки retain-decommit в production нет.
- **M2:** описать терминальные sidecar маршрутов и честно пометить, что пина нет (или завести новый пин).
- **M7:** «маска даёт ключ, корень — из таблицы владельца».
- Ссылку на несуществующий тест в `dealloc_own_base.rs:188–191` убрать. Это doc-комментарий в `src/`, относится к item 154.

**Oracle исправления.** Расширить `tests/no_stale_doc_references.rs`:
- каждый путь `tests/*.rs`, упомянутый в `docs/INVARIANTS.md`, существует;
- в файле нет `RemoteFreeRing`;
- нет фразы «rejected with `null`».

Сейчас тест красный: два отсутствующих файла и обе фразы. После правки — зелёный. Поведенческая сторона M4 уже закреплена `tests/r8_large_alignment.rs`.

### Кандидаты, сознательно не повышенные до находок

- **Таймер decay пула малых сегментов.** `maybe_decay_small_pool` (`alloc_core_small_pool_impl.rs:515–556`) тоже не трогает `last_pool_decay_tick`, пока пул пуст (`:517–519`). Но catch-up там нет: не больше одного вытеснения за вызов и `T := now` (`:535`). Doc обещает только «interval has elapsed since the last tick», а не минимальное время пребывания в пуле. Контракт не нарушен, долга нет. Механизм R17-LOG-01 сюда не переносится.
- **`dbg_force_decay_tick` после R34-11.** Перемотка ровно на один интервал (`alloc_core_large_cache_eviction.rs:103–120`) даёт `ratio = ⌊(I + ε)/I⌋ = 1`. Контракт «each call produces exactly one decay step» держится.
- **Переполнения в catch-up нет.** `due ≤ elapsed / interval`, значит `t + interval·due ≤ now`, и паники `Instant + Duration` быть не может. `interval·due ≤ 8 × (2³² − 1)` мс — `Duration` это вмещает.

## 3. Неизмеренные гипотезы оптимизации и поддерживаемости

**Все пункты не измерены.** Это не speedup, не RSS- или latency-результат и не promotion GO. Судья должен соблюдать правила R26-4, R30-8, entry-point и regime из CLAUDE.md.

**H1 — цена правильной семантики перехода через headroom (п. 3 рекомендации к R17-LOG-01).** Перевзвод таймера на переходе «≤ H → > H» стоит одного `Instant::now()` на переход.
- Нагрузка, где `U` колеблется вокруг H на каждой паре alloc/free, снова платит за чтение часов на каждую пару. Это обрыв R32-8: там измерено ≈ 74–138 нс на вызов (`alloc_core_large_cache.rs:440–446`).
- Альтернатива «задокументировать пакет до 8 шагов» ничего не стоит.
- Судья: A/B в духе R32-8 на реальном `#[global_allocator]`. Оракул активации — `MAYBE_DECAY_GUARD_PASSED` плюс новый счётчик переходов. Нагрузки две: колебание вокруг H и простой → всплеск. Жертва — hit-rate и число `release_segment`/OS-резервирований на всплеске.

**H2 — дрейф doc-комментариев в теме.** Это новые примеры к item 154, а не новый пункт:
- `SegmentHeader::span_usable` (`src/alloc_core/segment/segment_header/segment_header_impl.rs:363–370`) — «`n_segments * SEGMENT`», «Set exactly once … NEVER recomputed». Под `exact-span-large` значение кратно странице (`alloc_core_large.rs:206–207`). Под `large-reserved-capacity` OPT-G его увеличивает (`src/alloc_core/alloc_core/mem/realloc_fastpath.rs:530–540`).
- Инвариант `CachedLarge` (`src/alloc_core/alloc_core/alloc_core_impl.rs:165–174`):
  - «`usable_size` … `n_segments * SEGMENT`» — то же расхождение;
  - шаги «1. Re-register 2. Write a fresh `SegmentHeader`» идут в обратном порядке относительно UBFIX-6/F12. На деле сначала точечные записи заголовка, затем `register_payload` (`alloc_core_large.rs:400–467`).
- Doc `alloc_large` (`alloc_core_large.rs:105–114`):
  - попадание якобы «avoids … registration» и стоит «one recommit call». На деле попадание регистрирует сегмент заново (`:451`), а страницы в кэше закоммичены без recommit (`mem_impl.rs:387–396`);
  - «Cost: one `Instant::now()` + one duration compare on the common path». После R32-8 на общем пути часов нет: быстрый выход и страйд.
- Сводка `maybe_decay_large_cache` (`alloc_core_large_cache.rs:424–425`) — «if so, run one decay step». После R34-11 шагов до 8.

Первые два пункта касаются не-production фич, последние два — чистая документация. Предлагается свернуть их в следующий документационный раунд item 154.

## 4. Наблюдения вне темы (по одной строке)

- **Безопасность / hostile input.** `small_free_guard` (`src/registry/heap_core/free/dealloc_own_base.rs:195–301`) на production-фасаде `SeferAlloc` не проверяет H-1 `off < payload_start`. Эта проверка есть в `AllocCore::dealloc_small` (`src/alloc_core/small/alloc_core_small/dealloc.rs:63–81`). Асимметрия — SOURCE-CONFIRMED. Последствие — ГИПОТЕЗА: при освобождении адреса в области метаданных зарегистрированного сегмента битовые оракулы пропустят такой `off`, и блок уйдёт в magazine на повторную выдачу. Ложна, если такие смещения отсекаются раньше по стеку; в прочитанном `dealloc_routing` → `canonical_block_of` (`src/alloc_core/alloc_core/mem/mem_impl.rs:34–42`) такой проверки нет. Возможно только при UB вызывающего.
- **Тесты и верификация.** Остаточный предел M2 (own-thread double free против незавершённого cross-thread free) после `a4245965` без живого пина и модели (R17-LOG-03). Модель — тема конкурентности.
- **Не переоткрыто.** Item 166 (освобождение внутреннего указателя в выданный блок однородного листа ведёт к `abort`) и item 158 (устаревший бит unknown-bucket NUMA-директории). Прочитанный код согласуется с их карточками, новых свидетельств нет.

## 5. Проверено — находок нет

- **Классы размеров.** Таблица вычислена вручную (приложение C):
  - 49 классов: 40 геометрических `round_up(ceil(prev × 5/4), 16)` и 9 extras (`src/alloc_core/platform/size_classes.rs:117–118`);
  - `SMALL_MAX = 258 752`, длина LUT `258 752 / 16 + 1 = 16 173` (совпадает с doc `:188`);
  - граница: 258 752 → последний класс, 258 753 → `None` → Large;
  - `size = 0` поднимается до 16 во всех точках входа;
  - `Layout(4097, 4096)` → медленный путь → класс 8192 (6144 не кратно 4096);
  - `align = 32 768` → ни один класс от 34 704 до 258 752 не кратен → Large, где `hdr_aligned = align_up(144, 32 768)` выравнивает payload;
  - const-assert `SMALL_CLASS_COUNT ≤ 64` на месте (`:213–217`, R16-03).

  Выравнивание блока гарантировано, потому что база сегмента выровнена по `SEGMENT`, carve идёт по абсолютной кратности `block_size`, а `class_for` возвращает только классы с `block_size % align == 0`.
- **Magazine и refill** (`src/registry/heap_core/state/tcache.rs:63, 109, 127–138, 144, 176–182`): `refill_n = clamp(65 536 / block, 1, 16)` — 16 для блоков до 4096, 14 для 4640, 1 от 34 704. `FREE_PARK_CAP` совпадает. `TCACHE_CAP ≤ 16` закреплён const-assert под u16-маску virgin. `FLUSH_N = 8`.
- **Геометрия Large** (`alloc_core_large.rs:143–207`): `checked_add` для `needed`; округление `usable` без переполнения при `size ≤ isize::MAX`; best-fit в диапазоне `[usable, 2·usable]` (`:216–247`, `LARGE_CACHE_SIZE_FACTOR = 2`).
- **Biased-геометрия** (`os.rs:178–218`): `root_offset ∈ [0, align − 1]`; корень выровнен по странице; `root_offset + useful ≤ raw_len` выполняется всегда. На каждом раннем выходе резервирование сбрасывается RAII до инкремента счётчика reserved, так что счётчики согласованы.
- **Откаты Large:**
  - `begin_reuse` вернул `None` → `release_cached` + `release_segment` (`:251–265`);
  - отказ регистрации на попадании → `release_initializing` + `release_segment` (`:459–465`);
  - отказ регистрации в медленном пути → `release_segment` (`:658–669`).

  Переходы `LargeReservationState` (`src/alloc_core/large/reservation_state.rs:103–161`) — CAS с ожидаемой генерацией.
- **Учёт кэша:**
  - `large_cache_used_bytes`: +usable при депозите (`mem_impl.rs:452`), −usable при попадании (`alloc_core_large.rs:254–255, 297–298`) и при вытеснении (`alloc_core_large_cache_eviction.rs:42–44, 236–238`);
  - цикл бюджета завершается: каждая итерация либо вытесняет, либо выходит; заведомо невместимый депозит пропускается (`mem_impl.rs:362–384`);
  - biased-span не кэшируется (`mem_impl.rs:363`);
  - `span_usable` и `reserved_capacity` переносятся из слота (bug #134).
- **realloc** (`realloc_fastpath.rs:286–343, 514–541`):
  - OPT-G срабатывает только на рост и только от канонического начала payload;
  - конец считается через `checked_add`;
  - при `large-reserved-capacity` граница коммита округляется до 64 KiB с клампом по `reserved_capacity`;
  - OPT-F срабатывает только при совпадении класса;
  - move-ветка `HeapCore::realloc` копирует `min(old, new)` (`src/registry/heap_core/free/realloc.rs`).
- **`alloc_zeroed`:** пропуск обнуления только для свежего резервирования `alloc_large_slow` (`cfg!(not(miri))`); попадание в кэш всегда обнуляется явно.
- **Таблица сегментов:**
  - идентичность в хэше — `key | (id + 1)` в младших 22 битах, `MAX_SEGMENTS < 2²²` закреплено assert (`hash.rs:5, 80–87`);
  - условие backward-shift (`:187–197`);
  - устаревшая идентичность в `hash_find` даёт `None` (`:253–257`);
  - `unregister` удаляет запись по ключу payload и чистит own-cache по сохранённому корню (`segment_table_impl.rs:509–536, 866–873`);
  - `recycle` сначала проверяет членство (`:614–701`);
  - `register_payload` сначала берёт свободный слот из free-list и соблюдает предел `MAX_SEGMENTS` (`:385–446`).
- **Малый путь** (без цитирования строк, прочитано целиком):
  - carve: `align_up(bump, block_size)` через `div_ceil` и проверка конца сегмента;
  - порядок «сначала свободное, потом bump» в refill;
  - `dec_live*` с исключением primordial;
  - O(1)-членство пула;
  - доводка курсора;
  - grow-on-carve по 256 KiB с коммитом до сдвига `bump`.
- **Sidecar-биты:**
  - `SmallSidecar::prepare`: `offset < SEGMENT` и кратность гранулам (`src/registry/segment_route/small_sidecar.rs:50–56`);
  - `ClassLeaves::prepare`: границы гранул и класса (`src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:74–77`);
  - scan/cut забирают слово целиком;
  - guard'ы reclaim.
- **Конфигурация:**
  - значения по умолчанию совпадают с доками: 256 MiB, 1000 мс, 10 % (`large_cache_config.rs:46–54`); пул 4 сегмента / 16 MiB (`small_segment_pool_config.rs:126–129`);
  - процент клампится в [1, 100] и переводится в bp (`large_cache_config.rs:456–474`);
  - `decay_interval_ms: u32` не переполняет `Duration`;
  - headroom профилей 16 / 64 / 256 MiB (`profile.rs:425–441`).
- **Счётчики:**
  - `SEGMENTS_RESERVED_TOTAL` и `SEGMENTS_RELEASED_TOTAL` после фикса R15-02: `Drop for Segment` считает освобождение (`os.rs:167–172`), все конструкторы `Segment` считают резервирование, пары `mem::forget` + `release_segment` согласованы (`alloc_core_large.rs:648/667`, `reserve.rs:243/253`, `decomp_hooks.rs:76–77`), двойного счёта нет;
  - `DECOMMIT_CALLS` считает путь release-follows (`decommit.rs:131–133, 142–155`);
  - `RELOC_*` увеличиваются только в своих ветках возврата.
- **Последствия R16 в теме:**
  - R16-01: `.expect` заменены на маску и `debug_assert` (`dealloc_own_base.rs:482–494`). Magazine не держит Large/biased-блоков, поэтому маска совпадает с каноническим корнем;
  - R16-03: const-assert присутствует.

## Приложение A. Инструменты и команды (только чтение)

Состояние дерева и коммиты:
- `git rev-parse HEAD` → `453439c1…`
- `git status --short` — до записи отчёта пусто
- `git log --oneline 6a0d47f6..453439c1` — 20 коммитов
- `git diff --stat 6a0d47f6..453439c1 -- src/` — 22 файла, +130/−122
- `git log -S"DECAY_CATCHUP_MAX_STEPS" -- src/` — `73dcecab`
- `git log --diff-filter=D -- tests/regression_xthread_double_free_residual.rs tests/loom_magazine_ring_compose.rs` — `a4245965`
- `git ls-files src`

Подсчёт строк:
- `find src -name '*.rs' | wc -l` → 168
- `find src -name '*.rs' -print0 | xargs -0 cat | wc -l` → 43 325; то же по каждой группе
- `… | grep -cE '^[[:space:]]*$'` → 1 963
- `… | grep -cE '^[[:space:]]*//'` → 24 303
- код: непустые строки без `//` в начале → 17 059

Unsafe-инвентарь: `rg '^\s*#!\[allow\(unsafe_code\)\]' src` → 21 в 21 файле; `rg '^\s*#\[allow\(unsafe_code\)\]' src` → 83 в 30 файлах.

Поиск и проверка дубликатов:
- писатели `last_decay_tick` и `large_cache_decay_op_count`;
- вызовы `maybe_decay_large_cache()`, `bind_large_cache_hits`, `trim_for_recycle`, `fail_next_registration_for_test`;
- упоминания `INVARIANTS.md` в `tests/` и `docs/`;
- по индексам: `catch-up|R34-11|decay`, `INVARIANTS`, `small_free_guard|payload_start`, `X7|double-free residual`.

## Приложение B. Witness

**Не запускалось: режим только чтение.** Мутанты тоже не запускались по той же причине. Oracle'ы в §2 спроектированы как тесты «красный до, зелёный после», но ни один не собран и не исполнен.

## Приложение C. Ручной расчёт таблицы классов (`production`, без `medium-classes`)

Геометрическая часть, 40 классов, где каждый следующий равен `round_up(ceil(prev × 5 / 4), 16)`:

```text
16 32 48 64 80 112 144 192 240 304 384 480 608 768 960 1200 1504 1888 2368 2960
3712 4640 5808 7264 9088 11360 14208 17760 22208 27760 34704 43392 54240 67808
84768 105968 132464 165584 206992 258752
```

Extras, 9 классов (`size_classes.rs:118`):

```text
256 512 1024 2048 4096 6144 8192 12288 16384
```

Слитая отсортированная таблица, 49 классов, индексы 0…48:

```text
16 32 48 64 80 112 144 192 240 256 304 384 480 512 608 768 960 1024 1200 1504
1888 2048 2368 2960 3712 4096 4640 5808 6144 7264 8192 9088 11360 12288 14208
16384 17760 22208 27760 34704 43392 54240 67808 84768 105968 132464 165584
206992 258752
```

Совпадает с doc модуля: «49 fine classes … `SMALL_MAX` (~253 KiB)» — 258 752 байт ≈ 252,7 KiB.
