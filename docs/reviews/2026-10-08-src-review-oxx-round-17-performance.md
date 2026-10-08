# Ревью `src/` — раунд 17 (oxx) — производительность и оптимизация

## 1. Охват, инвентарь, метод, ограничения, вердикт

**База:** `main` @ `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`). Тематический отчёт 17-го раунда обзоров `src/`. Тема — производительность и оптимизация: анализ кода, без замеров. Предыдущий раунд — `docs/reviews/2026-10-08-src-review-oxx-round-16.md`.

**Режим: только чтение.** По приказу владельца ревью проведено **без компиляции и без исполнения кода**. Не запускались:
- cargo в любой форме: build, check, test, clippy, bench, rustc, miri, kani, в том числе `cargo rustc --emit asm`;
- iai/callgrind, criterion, Loom, TSan, профилировщики;
- witness-тесты (не создавались).

Ассемблер не смотрелся. Использовались только чтение файлов, grep, git для чтения (`log`, `show`, `diff`, `blame`, `merge-base --is-ancestor`) и подсчёт строк.

Числа в отчёте бывают двух видов:
- прочитанные из **уже закоммиченных** raw-логов прежних раундов (их исполняли другие задачи, здесь они только прочитаны);
- оценки по чтению кода.

Новых измерений нет.

**Ревьюер:** oxx (Claude Opus 5.5, effort=max). Работа в одном контексте, без суб-агентов, в изолированном worktree. Создан ровно один файл — этот отчёт.

**Инвентарь** (пересчитан командами, приложение A): 168 файлов `.rs`, **43 325** физических строк.
- Состав строк: 1 963 пустых, 24 303 строк-комментариев (`//`, `///`, `//!`), 17 059 прочих.
- По группам (файлов / строк): `src/alloc_core/` 88 / 24 802; `src/registry/` 46 / 10 413; `src/global/` 18 / 3 809; `src/concurrent/` 14 / 3 554; `src/lib.rs` + `src/kani_proofs.rs` 2 / 747.
- С базы R16 (`6a0d47f6`) в `src/` вошли коммиты `486f5ace`, `32cacc97`, `222e913d`, `9df9f6b8`, `c8b9344a`, `b707d196`.

**Метод.**
1. Чтение индексов и контекста:
   - `docs/perf/OPEN_ITEMS.md`: current-state карточки всех тиров и Recently resolved;
   - `docs/perf/OPEN_ITEMS_ARCHIVE.md`: § 80 и § 56;
   - `docs/CORRECTNESS_OPEN_ITEMS.md` и grep по `docs/correctness-open-items/*.md`;
   - отчёт R16;
   - `docs/REMOTE_FREE_BOUNDED_RECLAMATION_DECISION_BRIEF_2026-09-29.md`, `docs/perf/PH6B_COST_AB.md`, `docs/perf/PG3R_SCAN_PATCH_IAI.md`;
   - совет R11 `docs/reviews/2026-10-01-src-r11-production-cost-advice-xxs-sol.md`, раздел P3-3.
2. Построчное чтение путей темы (списки ниже).
3. Сравнение закоммиченных iai-логов разных дат при одном наборе фич. Контроль A/A — плечи mimalloc (приложение B).
4. Модель стоимости по чтению кода, сверенная с логами (приложение C).

**Прочитаны полностью:**
- `alloc_core/segment/remote_bitmap/{bitmap_scan,bitmap_cut,sidecar_bitmap}.rs` и `.../sidecar_bitmap/leaf_classes.rs`;
- `alloc_core/segment/segment_table/{issue_transaction,route_slots,active_kind_ops}.rs`;
- `alloc_core/alloc_core/sidecar_drain.rs`, `alloc_core/small/alloc_core_small_reclaim.rs`;
- `registry/segment_route/{directory,small_sidecar,registration,large_state,shard_lock}.rs`;
- `registry/heap_core_xthread/routing.rs`, `registry/heap_core/state/ownership.rs`, `registry/heap_registry/maintenance.rs`, `global/maintenance_service.rs`.

**Прочитаны по диапазонам:**
- `alloc_core/small/alloc_core_small/find_segment.rs` (1–270, 390–720);
- `.../alloc_core_small_impl.rs` (150–262, 360–460, 526–783, 836–972);
- `alloc_core/small/alloc_core_small_magazine.rs` (120–330);
- `.../alloc_core_small_pool/alloc_core_small_pool_impl.rs` (60–200, 650–680);
- `alloc_core/segment/segment_table/segment_table_impl.rs` (250–540, 700–780);
- `alloc_core/alloc_core/{lifecycle,mem/mem_impl,mem/realloc_fastpath}.rs`;
- `alloc_core/large/{alloc_core_large,alloc_core_large_cache}.rs`;
- `registry/heap_core/{alloc/hot,free/dealloc_own_base,free/realloc,state/tcache}.rs`;
- `registry/heap_registry/{claim,stack,counters}.rs`, `global/tls_heap.rs` (195–260);
- нужные функции в `benches/perf_gate_iai.rs`, `benches/global_alloc.rs`, а также `scripts/bench-table.mjs`.

**Скрининг grep по всем 168 файлам:** `System.alloc*`, `SeqCst`, `Instant::now`, `yield_now`/`park_timeout`/`Mutex<`, `repr(align`, `is_multiple_of(block_size)`, глобальные `fetch_add`, `swap(0`.

**Не прочитаны вглубь:**
- `concurrent/*` — регионы вне пути `production`;
- `alloc_core/platform/{numa,sidecar,os}.rs` вне упомянутых строк;
- `global/exact_object/*`, `global/fallback.rs` вне `try_with_heap`;
- `*_diag*.rs`, `kani_proofs.rs`, `large_cache_extended`.

Утверждения «не найдено» относятся только к прочитанному.

**Классы доказательства.**
- **SOURCE-CONFIRMED** — установлено чтением кода, истории git или закоммиченных артефактов на базе. Для чисел из raw-логов это значит: наблюдено прежним исполнением, прочитано здесь.
- **ГИПОТЕЗА** — всё выведенное: механизм, атрибуция, оценка, экстраполяция.

Новых наблюдений исполнением нет.

**Вердикт.** 6 находок (1×P2, 3×P3, 2×P4) и 13 гипотез.

Главная находка — R17-PRF-01 (P2). Owner-скан терминального sidecar выполняет атомарный `swap(0, AcqRel)` на каждое слово pending-bitmap: от начала payload до high-water, у каждого посещённого сегмента, на каждом промахе refill или класса, включая пустые слова. В закоммиченных детерминированных логах многосегментные бенчи выросли в 132–178 раз между 2026-09-22 и 2026-10-02. Модель по коду объясняет эту дельту с точностью 3–4% и даёт квадратичную стоимость при росте кучи.

Вторая — R17-PRF-02 (P3): own-thread free подорожал на +24 Ir (с 46.3 до 70.7 Ir на free). Это вся регрессия горячего churn: с 72.0 до 96.0 Ir на пару, вне любых гейтов.

Это не release GO, не perf GO и не NO-GO. Ни одна гипотеза не утверждается как выигрыш.

**Что не удалось проверить:**
- никаких измерений, ни wall-clock, ни RSS;
- атрибуции bisect'ом;
- ассемблера;
- Ir в чистом `production` без `bench-internals`;
- Loom/Miri для предлагаемых изменений протокола.

## 2. Находки

**Шкала.**
- **P2** — существенный дефект производительности или ресурса на пути `production` по умолчанию, без нарушения memory safety.
- **P3** — ограниченный дефект стоимости или удержания ресурса.
- **P4** — вводящий в заблуждение контракт или комментарий с небольшой ценой.

| ID | Severity | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R17-PRF-01 | P2 | SOURCE-CONFIRMED (путь, логи) + ГИПОТЕЗА (атрибуция, экстраполяция) | любой production heap (`HeapCore` всегда routed), каждый промах refill/класса Small | Owner-скан sidecar: `swap` на каждое слово до high-water каждого кандидата, без пропуска пустых слов; O(Σ high-water / 1 KiB) атомарных RMW на промах; квадратично при росте |
| R17-PRF-02 | P3 | SOURCE-CONFIRMED (логи, blame) + ГИПОТЕЗА (атрибуция) | каждый own-thread free | Free (push в magazine) подорожал с 46.3 до 70.7 Ir (+24.4); churn-пара с 72.0 до 96.0 Ir; ~11 Ir объясняет повторное разрешение корня, остаток не атрибутирован |
| R17-PRF-03 | P3 | SOURCE-CONFIRMED (путь, логи) + ГИПОТЕЗА (разложение) | `production`, Large-кэш | Каждый цикл Large-кэша снимает и заново регистрирует глобальный маршрут: 2 System alloc, 2 System free, 2 шард-лока, глобальный CAS; цикл hit/deposit с 378 до 1 303 Ir; протокол reuse маршрута есть, но вызывается только из тестов |
| R17-PRF-04 | P3 | SOURCE-CONFIRMED (политика) + ГИПОТЕЗА (величина) | `production`, волны короткоживущих потоков | Fresh-first claim и одна перезаписываемая подсказка: волна из K потоков материализует до K−1 новых heap при K−1 свободных; рост до `MAX_HEAPS` = 4096; обещанный советом R11 замер ни в один индекс не заведён |
| R17-PRF-05 | P4 | SOURCE-CONFIRMED | `production` (`fastbin`) | Doc `PerClass` обещает одну кэш-линию на hit/push, но установившийся churn идёт на `count` 15↔16: `count` по смещению 0, горячий слот по смещению 128 |
| R17-PRF-06 | P4 | SOURCE-CONFIRMED (+ `git blame`) | `production`, каждый `realloc` своего блока | `realloc_inplace_fast_path_known_base` документирован как «без повторного probe таблицы», но с `a4245965` снова вызывает `canonical_base_of` |

### R17-PRF-01 — owner-скан sidecar обменивает каждое слово до high-water, включая пустые

**Места.**
- Скан:
  - `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs:5` — doc: «One cut is taken per word, including empty words»;
  - `bitmap_scan.rs:15–28` — `next_cut`, на `:22` стоит `pending.swap(0, Ordering::AcqRel)`;
  - `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:121–145` — скан «exactly `ceil(high_water / (64 * MIN_BLOCK))` words, without dirty hints».
- Discovery:
  - `src/alloc_core/small/alloc_core_small/find_segment.rs:136–172` — `drain_segment_sidecar`: high-water = `bump_of()` на `:141`, диапазон слов на `:145–146`, цикл на `:153–159`;
  - `find_segment.rs:398–399` — `trust_negative = !self.table.is_routed()`;
  - `find_segment.rs:463–506` — полный скан активных Small/Primordial; drain кандидата на `:489` идёт до проверки BinTable на `:504–506`;
  - `find_segment.rs:712` — drain в `validate_directory_candidate`.
- Частота вызова:
  - `src/alloc_core/small/alloc_core_small_magazine.rs:173–236` — защёлка `free_exhausted` живёт только в пределах одного refill, `find_segment_with_free` вызывается на `:221`;
  - скалярный путь: `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:183`, раз на 32 блока (`REFILL_BATCH = 31`, `find_segment.rs:56`).
- Те же слова сканируют:
  - строгий trim `src/alloc_core/alloc_core/sidecar_drain.rs:186–267`;
  - он вызывается из `trim_for_recycle` (`src/registry/heap_core/state/ownership.rs:108–115`) на выходе потока (`src/global/tls_heap.rs:221`) и при повторном захвате слота (`src/registry/heap_registry/claim.rs:317`);
  - фоновый ограниченный drain `sidecar_drain.rs:88–184`, где одно слово = одна единица бюджета `BACKGROUND_INGRESS_BUDGET = 64` (`ownership.rs:14`).
- Маршрутизация всегда включена: `src/registry/heap_core/core.rs:300` → `AllocCore::new_with_owner` → `attach_owner` (`src/alloc_core/alloc_core/lifecycle.rs:274–277`).

**Трасса (SOURCE-CONFIRMED).**
1. `GlobalAlloc::alloc(16 B)` → промах magazine → `refill_magazine_slow` (`src/registry/heap_core/alloc/hot.rs:754–779`) → `refill_class_bump(c, slots[0..16])`.
2. Freelist текущего сегмента пуст → `find_segment_with_free(c)`.
3. У routed-ядра отрицательный ответ директории не принимается (item 82). Цикл обходит все активные Small/Primordial слоты.
4. Индекс ведётся по виду сегмента, а не по наличию свободных блоков: `src/alloc_core/segment/segment_table/active_kind_ops.rs:14`, установка на `segment_table_impl.rs:424,444`, снятие на `:518,679`.
5. Для каждого кандидата `drain_segment_sidecar` делает один `swap` на слово в диапазоне `[payload_start / 1 KiB, ceil(bump / 1 KiB))`, независимо от того, было ли туда что-то опубликовано.
6. Цикл останавливается на первом сегменте со свободным блоком класса. При чистом росте свободных блоков нет, и обходятся все сегменты.

Hit magazine не сканирует ничего (`hot.rs:246–257`), поэтому платят только промахи.

**Наблюдено (SOURCE-CONFIRMED, закоммиченные логи; приложение B).** Условия сравнения: набор фич везде `production bench-internals internals`; тела этих бенчей между датами не менялись (`git diff 42d42061 fd954856 -- benches/perf_gate_iai.rs` их не затрагивает); плечи mimalloc совпадают побитово.

| Бенч | 2026-09-22 | 2026-10-02 (`7d80d7e1`) | 2026-10-05 (`fd954856`) | Рост | Ir на пару alloc+free |
|---|---|---|---|---|---|
| `multiseg_cold_256k` (68 пар) | 30 394 | 4 200 265 | 4 028 010 | ×132.5–138.2 | ≈447 → ≈59 235 |
| `seg_cycle_decommit_256k` (204 пары) | 72 256 | 12 858 979 | 12 374 664 | ×171.3–178.0 | ≈354 → ≈60 660 |

Число пар — `scripts/iai.mjs:190–193`.

**`pool_cap_sweep/cap=0/16B` (criterion, wall-clock).**
- Было: 1.2130 s (`docs/perf/_raw_r13_10_bench_table_full.log:564`, июль).
- Стало: 206.05 s (`docs/perf/_raw_ph6b_benchtable_base_build.log:538`).
- Прогон Ph6b прерван с пометкой «diagnostic group pool_cap_sweep inflates a full sweep to hours» (`:602`).
- Оговорка: это не один и тот же харнесс. В июле работало owner-local ядро и фичи `production`. Сейчас — routed `dbg_new_routed_with_config_for_test` (`benches/global_alloc.rs:1153`) и фичи `production internals bench-internals`. Значит, разница — это цена routed-конфигурации, которую имеет каждый production heap.

**Почему регрессия не была замечена.**
- `npm run bench:table` вызывает `cargo bench --bench global_alloc` без фильтра (`scripts/bench-table.mjs:397–398`). Каноническое обновление таблицы, которое требует CLAUDE.md, теперь включает многочасовую группу.
- Item 70 прямо фиксирует: «No speedup or regression number exists for this cutover».
- База B0 в Ph6b (`7d80d7e1`) уже после cutover: `a4245965` — её предок, проверено `git merge-base --is-ancestor`.
- Гейты Ph6b — HOT/REFILL/ZEROED/REALLOC (`docs/perf/PH6B_COST_AB.md:66–95`). В PG-3r `multiseg_cold_256k` только report-only (`scripts/pg3r_iai_table.mjs:133–134`).

**Выведено (ГИПОТЕЗА; приложение C).**
- *Атрибуция.* В окне 2026-09-22 → 2026-10-02 лежат 129 src-коммитов. Per-word drain вошёл в discovery коммитом `a4245965` (`git log -S drain_segment_sidecar`). Тот же коммит ввёл `trust_negative` и удалил O(1)-«pre-drain empty-guard» кольца: см. `git show a4245965^:src/alloc_core/small/alloc_core_small/find_segment.rs`, doc PERF-PASS-4 G9/C2 около строк 247–259. Bisect не выполнялся.
- *Модель.* Цена одного слова ≈12 Ir — собственная цифра проекта: `docs/perf/PG3R_SCAN_PATCH_IAI.md:279`, а также карточка item 80 («до 4096 атомарных swap (≈49k Ir) на полностью нарезанном сегменте», `docs/perf/OPEN_ITEMS_ARCHIVE.md:2092`).
  - По геометрии бенчей сканируется ≈3.2·10^5 слов (multiseg) и ≈1.02·10^6 слов (seg_cycle).
  - Отсюда дельты, подразумеваемые моделью: 12.4 и 12.0 Ir на слово при наблюдённых +3 997 616 и +12 302 408 Ir.
  - Один параметр объясняет два бенча, а сам он измерен независимо. Это сильный аргумент за механизм, но не доказательство.
- *Квадратичность.* При чистом росте 16-байтных объектов через `GlobalAlloc` refill берёт 16 блоков (256 B), а средний сканируемый high-water равен половине нарезанного. Число swap ≈ H² / 2^19, где H — нарезанные байты.
  - Цену слова ≈7.7 ns выводит та же модель из прогона `pool_cap_sweep`: ≈2.7·10^10 слов за 206 s.

  | H | Объектов по 16 B | swap | Ir | Время в одном потоке |
  |---|---|---|---|---|
  | 4 MiB | 0.26 M | 3.4·10^7 | ≈4·10^8 | ≈0.26 s |
  | 16 MiB | 1.05 M | 5.4·10^8 | ≈6.4·10^9 | ≈4 s |
  | 64 MiB | 4.2 M | 8.6·10^9 | ≈1·10^11 | ≈66 s |

  Это экстраполяция, а не замер.
- *Даже при хорошем кандидате.* С материализованной директорией (≥32 сегментов) и свободным блоком у кандидата каждый промах всё равно платит хотя бы один полный скан кандидата, около 4 000 swap: drain выполняется до проверки BinTable (`find_segment.rs:487–506`, `:712`).
- *Побочные цены.*
  - Swap пишет каждое слово: 32 KiB pending-bitmap на сегмент проходят через L1/L2 с записью и вытесняют рабочий набор пользователя. Каждый swap отнимает линию у продюсеров, которые её держат.
  - Строгий trim на выходе потока и на re-claim платит те же O(Σ слов).
  - Фон тратит бюджет на пустые слова: FREE heap с одним нарезанным сегментом проходится примерно за 63 визита.

**Почему это не повтор.**
- Item 80 закрыт патчем S (скан со слова payload, `18d76472`). Остаток «пропуск ПУСТЫХ payload-слов» не открыт; карточка оговаривает: «новая карточка — только по такому измерению» (`OPEN_ITEMS_ARCHIVE.md:2096`).
- PG-3r мерил бенчи, у которых high-water составляет единицы слов.
- Новые аргументы:
  1. детерминированные многосегментные бенчи (те же тела, те же фичи, A/A = 0) выросли в 132–178 раз;
  2. цена слова из самого PG-3r количественно объясняет этот рост;
  3. вызов происходит на каждый refill, а routed full scan (item 82) умножает его на число сегментов.
- Items 9, 20, 22, 23 и 34 отклонены при модели «скан при n = 3 — это три горячие итерации». С тех пор стоимость кандидата выросла с O(1) до тысяч RMW, то есть посылка их вердиктов изменилась. Сами вердикты здесь не пересматриваются.

**Рекомендация.**
1. Завести `[A]`-пункт: остаток item 80 или новую карточку.
2. Первый кандидат — R17-OPT-01: загрузка перед обменом на стороне владельца, продюсер не меняется. Структурный вариант — R17-OPT-02.
3. До исправления:
   - сделать `npm run bench:table` исполнимым — исключить `pool_cap_sweep` или сузить его `SIZES`;
   - сделать `multiseg_cold_256k` и `seg_cycle_decommit_256k` гейтируемыми по Ir/op для изменений drain/discovery.
4. Владельцу стоит решить, блокирует ли это perf GO текущего `production`.

**Oracle исправления.**
- iai: Ir/op `multiseg_cold_256k` и `seg_cycle_decommit_256k`.
- Path-activation: счётчики под `bench-internals` — сколько слов просмотрено, сколько обменяно, сколько непустых cut. Активность должна быть > 0 на multiseg и = 0 на churn.
- Kill-gate: churn-бенчи ±10 Ir (ожидается 0), ΔIr mimalloc = 0.
- Слой — `SeferAlloc` (`GlobalAlloc`), как в этих бенчах.
- Wall-clock: новый судья «рост N объектов по 16 B в одном потоке» для 1–64 MiB по схеме N/2N/4N (виден наклон), отдельно от `pool_cap_sweep`.
- Корректность: существующие тесты терминальной публикации с приостановленным продюсером (Loom/Miri) плюс новый litmus «публикация между загрузкой и пропуском».

### R17-PRF-02 — own-thread free подорожал на +24 Ir, вне гейтов

**Места.**
- Первое разрешение корня — `src/registry/heap_core_xthread/routing.rs:10–20`, вызов `canonical_block_of` на `:11` (`git blame`: `a4245965`).
- Второе разрешение того же корня — `src/registry/heap_core/free/dealloc_own_base.rs:347–352` (`git blame`: `2da9a941`, 2026-09-29).
- Оба вызова идут через `canonical_base_of_mut` (`src/alloc_core/segment/segment_table/segment_table_impl.rs:743–758`). В сборках с `bench-internals` там же стоят счётчики Tier-1 `fetch_add` (`:746–748`, `:751–753`).
- Бенч `benches/perf_gate_iai.rs:950–969`: 64 предаллокации, затем 16 push в пустой magazine через `HeapCore::dealloc`. Setup менялся (лизовый `claim_leaked_heap`), но тело цикла free — нет; setup сокращается в разности.

**Наблюдено (SOURCE-CONFIRMED, закоммиченные логи; приложение B).** Стоимость free = (`dealloc_free_only_16b_n16` − `dealloc_prealloc_only_16b`) / 16:

| Метрика | 2026-09-22 | 2026-10-02 | 2026-10-05 | 2026-10-08 (`265a571a`) |
|---|---|---|---|---|
| Ir на free | 46.3 | 65.5 | 70.3 | 70.7 |
| churn, Ir на пару (`small_churn_16b`, (2N − N)/64) | 72.0 | 104.0 | 105.8 | 96.0 |

Сторона alloc в итоге не изменилась: clear-блок hit стоил 12.19 Ir, к 2026-10-02/05 вырос до 23.3–24.1 и после R12-02 вернулся к 11.44. Итоговые +24 Ir на пару (+33%) целиком приходятся на free. Это «won front» в смысле правила X4-B: kill-gate ±10 Ir.

**Выведено (ГИПОТЕЗА).**
- Второе разрешение корня — это лишний probe Tier-1 (≈8.2 Ir по цифрам item 1 и R32-10). В судье с `bench-internals` к нему добавляется ещё один счётчик (+3 Ir/вызов, item 56). Вместе ≈11 Ir из +24.4.
- Остаток ≈13 Ir не атрибутирован: ≈+19 Ir приходятся на окно `1f4ac4f3`..`7d80d7e1` (129 коммитов) и ≈+5 Ir на `7d80d7e1`..`fd954856`.
- В чистом `production` (без счётчиков) дельта меньше на неизвестную величину, не больше ~6 Ir.
- Wall-clock не измерен.

**Почему это не повтор.** Повторный `canonical_block_of` уже записан гипотезой R15 (перепроверена в R16 §4). Новое здесь: измеренная регрессия горячего пути и её масштаб, вне гейтов (item 70 говорит, что чисел нет; item 81 сравнивает только B и C, обе точки после cutover).

**Рекомендация.**
1. Атрибуция: bisect на детерминированной разности n16 − prealloc по окну `1f4ac4f3`..`7d80d7e1` — около 7 шагов — и затем по `7d80d7e1`..`fd954856`.
2. Убрать повторное разрешение: вызывающий уже передаёт корень и производный от него блок. Kill-gate — churn.

**Oracle.**
- Free ≤ ~50 Ir.
- Churn-пара ≤ ~75 Ir.
- Тесты провенанса own-path: блок производен от корня (`2da9a941`) — должны остаться зелёными.
- ΔIr mimalloc = 0.

### R17-PRF-03 — Large-кэш на каждом цикле пересоздаёт глобальный маршрут

**Места.**
- Депозит (free в кэш):
  - `src/alloc_core/alloc_core/mem/mem_impl.rs:386–453`, вызов `self.table.unregister(base)` на `:401`;
  - → `segment_table_impl.rs:489–537`, `routes.remove(slot_id)` на `:515–516`;
  - → `src/alloc_core/segment/segment_table/route_slots.rs:203–212`, drop `RouteRegistration`;
  - → `RouteDirectory::remove` под шард-локом (`src/registry/segment_route/directory.rs:891–897`);
  - → `EntryHandle::drop` с `System.dealloc` для `LargeState` и `Entry` (`directory.rs:210–224`).
- Hit (выдача из кэша):
  - `src/alloc_core/large/alloc_core_large.rs:451–467`, `register_payload`;
  - → `segment_table_impl.rs:403–408`;
  - → `route_slots.rs:74–92`;
  - → `RouteDirectory::register` (`directory.rs:789–865`): глобальный `next_incarnation.fetch_update` на `:825–829`, `System.alloc(LargeState)` на `:72`, `System.alloc(Entry)` на `:85`, шард-лок и вставка на `:832–848`.
- Протокол, который сохранял бы маршрут: `src/registry/segment_route/registration.rs:55–75` — `cache_large_consumed`, `begin_large_reuse`, `finish_large_reuse_after_reset`, `release_cached_large`. Его вызывают только `tests/r6_route_directory.rs:151–165` и `tests/r6_route_directory_model.rs:44–53`. Production-кэш ведёт фазу в слове заголовка (`alloc_core_large.rs:251`).

**Наблюдено (SOURCE-CONFIRMED).** Разность `large_cache_hit_only_4mib` − `large_cache_prefill_only_4mib` — ровно один hit-alloc и один deposit-free на слое `HeapCore` (`benches/perf_gate_iai.rs:2268–2339`):

| | 2026-09-22 | 2026-10-02 (`7d80d7e1`) | 2026-10-05 (`fd954856`) |
|---|---|---|---|
| Ir на цикл | 378 | 1 340 | 1 303 |

Рост — в 3.4 раза.

**Выведено (ГИПОТЕЗА).**
- На каждый цикл приходятся:
  - 2 вызова System-аллокатора и 2 освобождения;
  - 2 захвата шард-лока, который делят и foreign-free продюсеры этого шарда;
  - бинарные поиски с разыменованием `Entry`;
  - 1 CAS по глобальному `next_incarnation` — одна линия на процесс, точка конкуренции для многопоточного Large-churn.
- Оценка по коду: ~500–800 Ir из +925. Остальное — CAS терминального состояния в заголовке.
- HITM и wall-clock не измерены.

**Рекомендация.** R17-OPT-06.

**Oracle.**
- Детерминированная iai-пара hit/prefill.
- Счётчик регистраций маршрута за цикл: 0 на hit.
- Kill-gate: `large_alloc_free_cycle` ±10 Ir, churn 0.
- Корректность: `tests/r6_route_directory*.rs` и новый тест — устаревший foreign free прежнего экземпляра после reuse отбрасывается по поколению.

### R17-PRF-04 — fresh-first claim размножает heap при волнах потоков

**Места.**
- `src/registry/heap_registry/stack.rs:108–140` (`pick_with_saturation`): `hint.swap` на `:116`, затем `bump()` на `:124`, скан только у предела на `:130`.
- Подсказка `reuse_hint` одна, и её перезаписывает каждый переход LIVE→FREE (`src/registry/heap_registry/claim.rs:539`) и каждый визит обслуживания (`claim.rs:435`).
- `MAX_HEAPS = 4096` — `src/registry/bootstrap/registry.rs:45`.
- Primordial-сегмент не decommit'ится: `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:94–105`; `release_empty_current_small_for_trim` пропускает сегменты, не являющиеся Small (`:656–660`).
- Обслуживание проходит 32 индекса раз в 10 ms по всему `count` (`src/global/maintenance_service.rs:29–30`, `src/registry/heap_registry/maintenance.rs:17–40`) и запускается только явно.

**Трасса (SOURCE-CONFIRMED).**
1. K потоков завершились → K heap в состоянии FREE, подсказка указывает на последний.
2. Следующая волна из K потоков: первый поток берёт подсказку, остальные K−1 получают `bump`.
3. Каждый `bump` — новый heap: primordial-резервация, `HeapCore::new`, маршрут с `SmallSidecar` на 41 KiB, `RouteSlots`.
4. Прежние K−1 FREE heap ждут насыщения до 4096.
5. `heaps_claimed_high_water` (`src/registry/heap_registry/counters.rs:67`, в `SeferAlloc::stats()`) растёт примерно на K−1 за волну.

**Наблюдено (SOURCE-CONFIRMED, косвенно).** Цена материализации heap вместе с первым Large-циклом — `large_alloc_free_cycle`:

| 2026-09-22 | 2026-10-02 (`7d80d7e1`) | 2026-10-05 (`fd954856`) |
|---|---|---|
| 4 410 Ir | 49 922 Ir | 49 951 Ir |

**Выведено (ГИПОТЕЗА).**
- Каждый лишний heap удерживает:
  - тронутые страницы primordial — и метаданные, и payload;
  - System-память маршрута: 41 KiB sidecar плюс mixed-листы;
  - 4 MiB VA.
- Каждая материализация стоит до ~45 K Ir.
- При `count` = 4096 обслуживание посещает слот раз в ~1.3 s. Вместе с R17-PRF-01 это растягивает фоновую утилизацию.
- Совет R11 P3-3 требовал: «On real runs measure claim latency *and* peak materialized chunks/commit under the same churn regime before accepting the fresh-first tradeoff» (`docs/reviews/2026-10-01-src-r11-production-cost-advice-xxs-sol.md:45`). Коммит `fe65746d` — без тела и без гейта.
- В индексах такого пункта нет: grep по `heaps_claimed_high_water` и `fresh-first` ничего не находит. Это пробел процесса.

**Рекомендация.**
1. Завести perf-пункт с замером:
   - сетка: волны K = 4/8/32 × W;
   - метрики: `heaps_claimed_high_water`, RSS/commit, latency первого alloc.
2. Сравнить с вариантами из того же совета — R17-OPT-13.

**Oracle.**
- После W волн `heaps_claimed_high_water` ≈ K, а не 1 + W(K−1); RSS выходит на плато.
- Kill-gate: latency первого alloc в волне, где все потоки живы, не хуже нынешней. Это и есть смысл P3-3.

### R17-PRF-05 — doc `PerClass` обещает одну линию, установившийся churn использует две

**Места.**
- Doc:
  - `src/registry/heap_core/state/tcache.rs:188–202`: «for the common case where a hit/push touches only the top few slots — the SAME 64-byte cache line»;
  - `tcache.rs:212–229`, F4: «shallow-magazine top-of-stack accesses (`count` 1-3, the documented churn-workload common case)».
- Структура: `tcache.rs:230–288`, 136 B; пины смещений на `:307–324`.
- Глубина magazine:
  - refill ставит `count = n − 1` (`src/registry/heap_core/alloc/hot.rs:122–124`);
  - для классов до 4 KiB `n = 16` (`tcache.rs:109,127–139`);
  - hit читает `slots[cnt − 1]` (`hot.rs:291–299,340`);
  - free делает push, пока `cnt < FREE_PARK_CAP` = 16.

**Наблюдено (SOURCE-CONFIRMED).**
- Чередование alloc/free после refill идёт между `count` 15 и 16. Горячий слот — `slots[15]`, смещение 8 + 15·8 = 128. `count` лежит на смещении 0.
- Расстояние 128 B: эти байты не могут оказаться в одной 64-байтной линии ни при каком выравнивании.
- После overflow-flush (`FLUSH_N` = 8, `tcache.rs:144`) горячие слоты — `slots[7..8]` со смещениями 64–79. Тоже всегда другая линия.
- Шаг массива 136 B ≡ 8 (mod 64), поэтому выравнивание `PerClass` дрейфует от класса к классу.

**Выведено.**
- Локальность, ради которой закрывали R32-5/F4 (`5df56d3`, «documented one-cache-line magazine layout»), в типичном установившемся режиме не выполняется.
- Цена — лишняя линия на hit/push, когда линии magazine холодные. В Ir эффект нулевой.

**Рекомендация.** Исправить doc и при желании проверить R17-OPT-09. Для правки doc oracle не нужен.

### R17-PRF-06 — «known base» realloc повторно опрашивает таблицу вопреки doc

**Места.**
- Doc:
  - `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:262–266` — «split so `HeapCore::realloc` can reuse its own `contains_base(base)` proof instead of probing the segment table again»;
  - `realloc_fastpath.rs:544–547` — «to avoid a duplicate `contains_base` probe».
- Тело: `realloc_fastpath.rs:275–280`, `canonical_base_of(segment_base_of_ptr(ptr))`.
- Оба вызывающих уже передают канонический корень из `canonical_block_of`: `src/registry/heap_core/free/realloc.rs:143–148,201–203` и `src/alloc_core/alloc_core/mem/mem_impl.rs:644–656`.
- `git blame`: тело — `a4245965` (2026-09-30), doc — `d7fa9c16` (2026-09-20).

**Наблюдено.** Doc и тело расходятся: лишний probe Tier-1/Tier-2 на каждый realloc своего блока.

**Выведено.** Порядка 8–12 Ir на вызов по цифрам проекта (Tier-1 ≈8.2, Tier-2 ≈12.0 Ir). Тот же класс, что R17-PRF-02 и гипотеза R15, но в другом месте.

**Рекомендация.** Либо убрать probe — ветка `base == key` нужна только вызывающему с ключом вместо корня, а такого сейчас нет, — либо исправить doc.

**Oracle.**
- iai `realloc_grow`: Δ < 0, churn 0.
- Тесты провенанса in-place пути остаются зелёными.

## 3. Гипотезы оптимизации

Все пункты — **ГИПОТЕЗА**, без измерений. Шкала: выигрыш / риск / цена проверки, каждое — В (высокий), С (средний) или Н (низкий).

Каждый судья обязан соблюдать правила из CLAUDE.md:
- R26-4 — конфигурация подтверждена;
- R30-8 — есть oracle активации пути;
- entry point назван явно: `AllocCore`, `HeapCore` или `#[global_allocator]`;
- cost и benefit меряются в одном режиме;
- kill-gate на churn ±10 Ir.

| ID | Место | Суть | В / Р / Ц | Связь |
|---|---|---|---|---|
| R17-OPT-01 | `bitmap_scan.rs:15–28` | Загрузка перед обменом: пустое слово без RMW | В / Н / Н | PRF-01, item 80 |
| R17-OPT-02 | `small_sidecar.rs:13–17`, `sidecar_bitmap.rs:110–119` | Summary-слово в sidecar, безусловный RMW продюсера | В / С / В | PRF-01, бриф 2026-09-29 |
| R17-OPT-03 | `find_segment.rs:487–506`, `:712` | Пропуск drain при `live_count == 0`; сначала BinTable | С / С / С | PRF-01, item 82 |
| R17-OPT-04 | `directory.rs:56–65` | Убрать повторную инициализацию обнулённого `SmallSidecar` | Н / Н / Н | PRF-04, item 78(b) |
| R17-OPT-05 | `alloc_core_small_impl.rs:550–607`, `:867–968` | Проверка маршрута один раз на партию | С / Н / С | Ph3a, cold |
| R17-OPT-06 | `mem_impl.rs:401`, `alloc_core_large.rs:451–467` | Маршрут Large остаётся в кэше | С / С / С | PRF-03 |
| R17-OPT-07 | `directory.rs:868–889`, `shard_lock.rs:48–66` | Путь foreign free: меньше RMW, TTAS, padding | С / С / В | item 70 |
| R17-OPT-08 | `alloc_core_small_reclaim.rs:39` | Делимость без `div` | Н / Н / Н | — |
| R17-OPT-09 | `tcache.rs:230–288` | Стек вниз от `count` | Н / С / С | PRF-05, items 18 и 44 |
| R17-OPT-10 | `sidecar_drain.rs:88–184` | Фоновый бюджет считает непустые cut | С / Н / Н | item 78(a), PRF-01 |
| R17-OPT-11 | `realloc.rs:181–191` + `hot.rs:249–254` | Без второго Large-sweep в одном realloc | Н / Н / Н | item 78(c) |
| R17-OPT-12 | `leaf_classes.rs:74–115` | Нарезка по классам, однородные листы | С / В / В | items 3, 5, 78(b) |
| R17-OPT-13 | `stack.rs:108–140` | Больше одной подсказки реестра | С / С / С | PRF-04, совет R11 |

**R17-OPT-01 — загрузка перед обменом на стороне владельца** (В / Н / Н).
- *Механизм.*
  - Владелец делает `let v = pending.load(Relaxed)`. Если `v == 0`, слово пропускается; иначе — `pending.swap(0, AcqRel)`, как сейчас.
  - Продюсер не меняется: `fetch_or(AcqRel)`, `sidecar_bitmap.rs:117`.
  - Начать с discovery и фонового drain. Строгий trim (`drain_sidecar_ingress`) оставить на безусловном `swap`, пока нет отдельного доказательства.
- *Почему корректно (ГИПОТЕЗА).*
  - Загрузка, вернувшая 0, равносильна `swap`, вернувшему 0, по контракту «post-cut publishers wait for a later pass and keep their outstanding credits» (`sidecar_drain.rs:186–190`): опоздавший бит удерживает кредит сегмента, и сегмент не освобождается.
  - Ненулевое слово по-прежнему обменивается RMW, так что синхронизация с публикацией сохраняется.
  - Публикация, которая happens-before загрузки, загрузкой видна (write-read coherence).
  - Summary здесь нет, поэтому потерять бит нельзя: следующий скан читает каждое слово. В отличие от `swap`, загрузка не обязана читать последнее значение в modification order — это влияет только на задержку утилизации.
- *Оценка и кто платит.*
  - Пустое слово: вместо `lock xchg` (порядка 20 тактов на x86, общеизвестно, здесь не проверялось) — обычная загрузка без записи и без RFO. По Ir ≈12 → ≈5–6 на слово.
  - Wall-clock скана — оценочно в 5–15 раз быстрее; 32 KiB на сегмент больше не грязнятся.
  - Непустое слово платит +1–2 Ir.
  - O(W) на кандидата остаётся.
- *Судья.* iai `multiseg_cold_256k` и `seg_cycle_decommit_256k`; судья роста из R17-PRF-01; MT fan-in (item 70) — по HITM.
- *Контроли.* Churn 0 Ir; ΔIr mimalloc = 0; счётчики «пропущено/обменяно»; тесты приостановленной публикации зелёные; слой — `SeferAlloc`.

**R17-OPT-02 — summary-слово в sidecar** (В / С / В).
- *Механизм.*
  - 64 слова `u64`, по биту на слово pending.
  - Продюсер после `pending.fetch_or` делает **безусловный** `summary.fetch_or(AcqRel)`.
  - Владелец делает `summary.swap(0)` и сканирует только помеченные слова.
- *Корректность (ГИПОТЕЗА).*
  - TTAS на стороне продюсера (`if summary.load() & bit == 0`) некорректен. Это store-buffer litmus: продюсер пишет pending и читает старый summary с единицей; владелец обменивает summary и читает старый pending. Без SeqCst такой исход допустим, и бит застревает. Поэтому нужен только RMW.
  - Запоздалая установка summary после того, как бит уже потреблён, даёт лишний визит пустого слова. Это безвредно.
  - Доступ продюсера к sidecar после терминального RMW нарушает текущее правило «No sidecar access is permitted after this RMW» (`sidecar_bitmap.rs:106`). При этом память sidecar держит пин маршрута до его drop (`directory.rs:210–224`), так что нужна новая формулировка контракта и Loom/Miri-модель.
  - Опасение брифа 2026-09-29 касалось записи продюсера в метаданные освобождаемого сегмента. Summary во внешнем закреплённом sidecar этот сценарий не воспроизводит.
- *Выигрыш.* O(64 + помеченные слова) на кандидата.
- *Кто платит.* +1 RMW на каждый foreign free на линии, общей для продюсеров 64 KiB сегмента.
- *Судья.* Сторона владельца — как OPT-01. Сторона продюсера — MT fan-in latency/Mops и `perf c2c` в том же режиме.
- *Контроли.* Заранее зафиксированный kill-gate по p99 продюсера; A/A.

**R17-OPT-03 — не дренировать кандидата без возможных публикаций / сначала BinTable** (С / С / С).
- *(а) Пропуск при `live_count == 0`.* Опубликованный бит держит кредит до reclaim (`sidecar_bitmap.rs:107–109`), значит, валидных битов у такого сегмента быть не может.
  - Риск: устаревший бит от невалидного двойного free переживёт повторную выдачу блока. Позже его примут за освобождение живого блока.
  - Сегодня скан опустевшего сегмента такой бит снимает, а reclaim отбрасывает дубликат (`alloc_core_small_reclaim.rs:47–51`). Это изменение hardening — нужно решение владельца.
- *(б) Сначала BinTable, drain — только если пусто.* Цена — задержка утилизации foreign frees (RSS) и сдвиг моментов decommit/pool.
- *Выигрыш.* Не сканируются сегменты пула — у них всё свободно, а bump остаётся в конце (`alloc_core_small_pool_impl.rs:182–194`), — и кандидаты, у которых свободный блок уже есть.
- *Судья.* Как у OPT-01, плюс RSS-ось в том же режиме (правило R31-1).
- *Контроли.* Item 82 не затрагивается.

**R17-OPT-04 — двойная инициализация `SmallSidecar`** (Н / Н / Н).
- *Где.* `directory.rs:56–65` вызывает `System.alloc_zeroed`, а затем `initialize_at`. Тот записывает 4 096 слов (`small_sidecar.rs:25–37`) и 1 024 + 1 024 значения (`leaf_classes.rs:46–60`) — 6 144 записи в 41 KiB, которые уже нулевые.
- *Почему лишнее.* Нулевой образ уже валиден для `AtomicU64::new(0)`, `AtomicU8::new(0)` и `AtomicPtr::new(null)`.
- *Выигрыш.* Около 6 тыс. записей (оценка 6–12 K Ir) на каждую регистрацию Small/Primordial маршрута — на каждый новый сегмент и каждый новый heap. Если System отдаёт свежие страницы, sidecar не трогается до первой публикации; с OPT-01 — и до первого скана.
- *Кто платит.* Контракт `initialize_at` надо сузить до «обнулённой аллокации», либо перейти на `System.alloc` с одним проходом.
- *Судья.* Интерцепт iai `large_alloc_free_cycle`, `seg_cycle_decommit_256k`; Miri.

**R17-OPT-05 — проверка маршрута раз на партию** (С / Н / С).
- *Где.* На каждый блок refill работает цепочка: `segment_table_impl.rs:316–333` → `route_slots.rs:126–148` → `small_sidecar.rs:50–73` → `leaf_classes.rs:62–135`. Commit повторяет все проверки: `issue_transaction.rs:108–122` → `segment_table_impl.rs:298–310` → `route_slots.rs:106–123`.
- *Механизм.*
  - Сейчас на блок минимум дважды проверяются segment_id, `base_at`, слот маршрута и `root`, плюс три и больше вычисления `prepared`.
  - Партия принадлежит одному сегменту и одному классу внутри owner-only функции, так что эти инварианты постоянны.
  - У блока из freelist код класса уже записан, и commit — чистая проверка.
- *Выигрыш.* Оценочно 20–50 Ir на блок refill. Потолок — разница cold-пути: 99.2 → 288.0 Ir/alloc.
- *Кто платит.* Abort при порче метаданных срабатывает раз на партию, а не раз на блок.
- *Судья.* iai `cold_alloc_only_256x16b` по N/2N/4N, `recycle_alloc_*`.
- *Контроли.* Churn 0; мутанты Ph3a (a)/(b)/(c) остаются красными.

**R17-OPT-06 — маршрут Large остаётся в кэше** (С / С / С).
- *Механизм.* Регистрация переезжает в `CachedLarge`. Фазы ведёт протокол CACHED → INITIALIZING → LIVE (`registration.rs:55–75`).
- *Выигрыш.* До ~925 Ir на цикл, без 4 вызовов System-аллокатора, 2 шард-локов и глобального CAS.
- *Кто платит.*
  - Сложность: маршрут хранится вне `RouteSlots`.
  - Маршрут в фазе CACHED виден в каталоге. Устаревший foreign free отклоняется по фазе и поколению — сегодня он так же получает «нет маршрута».
- *Судья.* iai hit/prefill; wall-clock многопоточного Large-churn.
- *Контроли.* `large_alloc_free_cycle` ±10; churn 0; счётчик регистраций.

**R17-OPT-07 — путь foreign free** (С / С / В).
- *Сейчас.* На каждый foreign free приходится минимум 4 RMW:
  - CAS лока шарда;
  - CAS `refs` в `pin_from_array` (`directory.rs:141–157`);
  - `pending.fetch_or`;
  - `refs.fetch_sub` в drop (`:216`);

  плюс два бинарных поиска (`:872`, `:884`).
- *Ожидание лока.* `ShardLock` крутит `compare_exchange_weak`, а не TTAS (`shard_lock.rs:48–66`).
- *Раскладка.* `[ShardLock<Shard>; 64]` — около 40 B на шард, то есть ~1.6 шарда на линию. `next_incarnation` стоит сразу за массивом (`directory.rs:698–701`).
- *Варианты.*
  1. Для не-fallback владельцев публиковать под локом без пина: −2 RMW на линии `Entry.refs`, но лок держится дольше. Ветка fallback пин сохраняет: лок отпускается до `try_with_heap` (`routing.rs:32–44`).
  2. TTAS-ожидание.
  3. Выравнивание шардов на 64/128 B и вынос `next_incarnation` на отдельную линию.
  4. Второй `find` — уже известная гипотеза R15.
- *Судья.* MT fan-in producer → consumer, `perf c2c` HITM, Mops, хвостовая latency.
- *Контроли.* Own-path churn 0 Ir; тесты fallback; A/A.

**R17-OPT-08 — делимость без `div`** (Н / Н / Н).
- *Где.* `alloc_core_small_reclaim.rs:39` безусловно выполняет `!off.is_multiple_of(block_size)` для каждой remote-записи. Делитель переменный.
- *Механизм.* Таблица на класс: маска для степеней двойки, иначе тест Лемира через 64-битное умножение.
- *Выигрыш.* Оценочно с 20–40 тактов до 3–5 на запись.
- *Судья.* Пропускная способность MT remote-free; iai drain после N публикаций.
- *Контроли.* Abort на интерьерный указатель сохраняется — тест по всем классам; `SIDECAR_INGRESS_RECORDS_CONSUMED` > 0.

**R17-OPT-09 — стек `PerClass` вниз от `count`** (Н / С / С).
- *Механизм.* Логический элемент k хранится в `slots[CAP−1−k]`. При глубине 8–16 вершина оказывается рядом с `count`.
- *Кто платит.* Переписать overflow, compaction и маппинг `virgin_mask`.
- *Судья.* iai Estimated Cycles и L1-промахи на многоклассовом interleaved-бенче; wall-clock mstress.
- *Контроли.* Ir churn ±10. X4 (item 18) и `FLUSH_N` (item 44) не переоткрываются.

**R17-OPT-10 — фоновый бюджет по непустым cut** (С / Н / Н).
- *Механизм.* Вместе с OPT-01 единицей бюджета становится непустой cut. Пустые слова получают отдельный лимит на визит, например 4 096 загрузок.
- *Выигрыш.* FREE heap проходится за один визит вместо ~63 на сегмент.
- *Судья.* Время утилизации N foreign frees в FREE heap при активном обслуживании; число визитов.
- *Связь.* Item 78(a) — конкретный рычаг для его гейта.

**R17-OPT-11 — второй Large-sweep в одном realloc** (Н / Н / Н).
- *Где.* `free/realloc.rs:181–191` делает полный sweep. Затем move-leg вызывает `HeapCore::alloc`, а тот делает ещё один (`hot.rs:249–254`).
- *Механизм.* Флаг «в этом вызове sweep уже был».
- *Почему не нарушает item 78(c).* Подсказка ничего не заменяет: убирается только повтор в пределах одного вызова. Публикации между двумя sweep дождутся следующего, как и сейчас.
- *Судья.* iai `realloc_grow` при L активных Large; churn 0.

**R17-OPT-12 — нарезка по классам** (С / В / В).
- *Где.* `leaf_classes.rs:74–115`: на путях prepare и carve делается `System.alloc` mixed-листа на 256 B, который живёт до drop маршрута. Bump общий для всех классов (`alloc_core_small_impl.rs:844–972`).
- *Механизм.* Run класса начинается с границы 4 KiB-листа, листы однородны, spill не нужен.
- *Выигрыш.* Без вызова System-аллокатора и 256 записей на каждый смешанный лист — оценочно ~300–500 Ir на 4 KiB нарезки — и без 256 KiB System-памяти на сегмент.
- *Кто платит.* Хвосты листов и сложность нарезки.
- *Oracle.* `mixed_leaves_for_test()` и `system_totals_for_test()` (`small_sidecar.rs:87–89,138–140`).
- *Связь.* Это аргумент для items 3, 5 и 78(b), а не новый пункт.

**R17-OPT-13 — больше одной подсказки реестра** (С / С / С).
- *Варианты* из совета R11:
  - короткий вращающийся probe перед `bump`;
  - точное множество свободных индексов: неаллоцирующая очередь или битмап с подавлением дублей и CAS-валидацией.
- *Выигрыш.* `heaps_claimed_high_water` ≈ пику живых потоков, а не сумме волн.
- *Кто платит.* Доказательство гонок — задержанная публикация FREE, обслуживание, ABA.
- *Судья и контроли.* Как в R17-PRF-04.

### 3.1 Связь с индексами — что нового

| Пункт | Что нового в этом отчёте |
|---|---|
| perf 80, закрыт | Его неоткрытый остаток (пропуск пустых слов) теперь подкреплён измерениями из закоммиченных логов и моделью. Предлагается OPT-01 — без summary-bitmap, который упоминало закрытие |
| perf 70 | Даны первые числа для cutover (×132–178, +24 Ir/free, ×3.4 на Large-цикл), хоть «no number» в нём и записано; механизмы для гейта — OPT-07 |
| perf 82 | Вердикт «НЕ СЕЙЧАС» не трогается. Новое: цену routed-скана определяет drain кандидата, а не число кандидатов; OPT-01 снижает её без доверия к директории |
| perf 78(a), (b), (c) | (a): бюджет фона тратится на пустые слова (OPT-10). (b): двойная инициализация (OPT-04) и spill (OPT-12). (c): двойной sweep в realloc (OPT-11) |
| perf 9, 20, 22, 23, 34 | Посылка «скан при n = 3 дёшев» больше не верна; вердикты не пересматриваются |
| perf 3, 5 | Аргумент от spill mixed-листов (OPT-12) |
| perf 56, закрыт | Его рекомендация (ii) — `bench-internals` в судьях — теперь бьёт по own free дважды (R17-PRF-02) |
| R15 H (повторный `canonical_block_of`, двойной `find`) | Измерен масштаб регрессии free (R17-PRF-02); новое место — R17-PRF-06 |
| correctness 78, подпункт 18 (`TRACKED_process_record.md:313–333`) | `fe65746d` — тот самый коммит политики fresh-first без гейта (R17-PRF-04) |

### 3.2 Не предлагается повторно

- Перепроверены и не переоткрываются: items 17, 18, 19, 21, 44, 45 и 1 (регион `flush_class` исчерпан).
- Item 72 открыт, не дублирую. Item 83 — NO-GO `[L]`, только по решению владельца.
- Items 84 и 56 закрыты.
- Вычисление класса: `class_for` один раз на alloc (`hot.rs:176–217`). В OPT-F realloc вызовов два (`realloc_fastpath.rs:335–338`), но это LUT; повод X6 (item 19) не повторяю.
- calloc на hit Large-кэша: `Node::zero(ptr, size)` (`hot.rs:729–740`). Замену на `MADV_DONTNEED` не предлагаю: R29-3 (item 16) намерил ~196–217 тыс. ns на decommit 4 MiB, что сопоставимо с memset.

## 4. Вне рамок темы

- **Безопасность (hardening).** Взаимодействие OPT-03(а) с невалидным двойным free — устаревший бит переживает повторную выдачу.
- **Тесты и CI.**
  - `npm run bench:table` исполняет `pool_cap_sweep` без фильтра и идёт часами.
  - Группа decommit в гейтах Ph6b и PG-3r только информационная.
  - Оба судьи (`scripts/bench-table.mjs:41`; iai по R56 §9-ii) собираются с `bench-internals`, а на x86 это `lock xadd` Tier-1 на каждом own free — теперь их два.
- **Конкурентность.** Строгий trim по контракту R6-03 при OPT-01 — отдельный Loom-вопрос. Новое правило доступа продюсера для OPT-02.
- **Процесс.** У `fe65746d` нет обязательного замера — R17-PRF-04.

## 5. Проверено без находок

- Hit magazine в `production` без атомарных RMW: счётчик под `alloc-stats` сделан как load+store (`hot.rs:333–339`); clear по маске (`hot.rs:43–58`).
- `class_for` вычисляется один раз на alloc (`hot.rs:176–217`).
- `SeqCst` в `src/` встречается только в тестовых инъекциях: `global/fallback.rs:650,704,712,739`, `registry/bootstrap/ensure.rs:246`.
- Глобальные счётчики не на горячем пути:
  - `LARGE_REMOTE_RETIREMENTS` — только при реальной утилизации (`sidecar_drain.rs:169,251,292,341`);
  - `FOREIGN_OR_UNROUTABLE_FREES` — только на непроходимом free (`routing.rs:28,63`);
  - `GROW_COMMIT_COUNT` — только при росте lazy-commit (`alloc_core_small_impl.rs:934`);
  - остальные — под `bench-internals` или `alloc-stats`.
- Decay Large-кэша: `Instant::now()` за шагом `DECAY_CLOCK_CHECK_STRIDE` (`alloc_core_large_cache.rs:545–560`).
- Large-probe на промахе magazine ограничен 4 инспекциями (`sidecar_drain.rs:65–66,299–353`).
- `HeapSlot` выровнен `align(64)` (`registry/heap_slot.rs:103,150`).
- `BitmapCut::pop` — O(1) на запись (`bitmap_cut.rs:20–35`).
- Шардирование каталога маршрутов — хеш по номеру сегмента (`directory.rs:899–902`). Блоки по 64 указателя (item 75 закрыт `17ba38ae`); квадратичной вставки нет.
- Claim без скана до предела — O(1). Обратная сторона — R17-PRF-04.
- NUMA (`numa-aware`) в `production` не входит и не оценивалась.

## Приложение A. Инструменты (только чтение)

- **Инвентарь.**
  - `find src -name "*.rs" | wc -l`;
  - `find src -name "*.rs" -print0 | xargs -0 cat | wc -l`, то же по группам;
  - пустые строки: `grep -c "^[[:space:]]*$"`;
  - комментарии: `grep -cE "^[[:space:]]*//"`.
- **git.**
  - `git log -S drain_segment_sidecar` и `-S trust_negative` по `find_segment.rs`;
  - `git log -S class-aware-dirty -- Cargo.toml`;
  - `git log --diff-filter=A` по `bitmap_scan.rs`;
  - `git show a4245965^:src/alloc_core/small/alloc_core_small/find_segment.rs`;
  - `git blame -L` на `dealloc_own_base.rs:345–352`, `routing.rs:10–17`, `realloc_fastpath.rs:268–283`;
  - `git merge-base --is-ancestor` для пар (`42d42061`, `a4245965`) и (`a4245965`, `7d80d7e1`);
  - `git diff` между `1f4ac4f3`, `fd954856` и `453439c1` по drain/discovery и бенчам;
  - `git log --oneline 1f4ac4f3..7d80d7e1 -- src/ | wc -l` → 129.
- **Логи.** `grep -A1 "^perf_gate_iai::perf_gate::<bench>$" <log>`, первое число строки `Instructions:`.

## Приложение B. Ir из закоммиченных логов

**Источники:**
- (1) `docs/perf/_raw_task1999_alloc_zeroed_stub_gap_after_1982.log` — закоммичен в `1f4ac4f3` (2026-09-22); идентичность измеренного дерева в логе не записана, поэтому граница окна приблизительна;
- (2) `docs/perf/_raw_ph6b_iai_base_run1.log` — B = `7d80d7e1` (`docs/perf/PH6B_COST_AB.md:24`);
- (3) `docs/perf/_raw_ph6b_iai_cand_run1.log` — C = `fd954856`;
- (4) `docs/perf/_raw_r16_perf84_A1.log` — A = `265a571a`, усечённый, только часть бенчей.

Фичи везде: `production bench-internals internals` (для Ph6b — `PH6B_COST_AB_identity.txt:94`). Состав `production` различается только удалённым в `a4245965` `class-aware-dirty`.

| Бенч / производная | (1) 09-22 | (2) 10-02 | (3) 10-05 | (4) 10-08 |
|---|---|---|---|---|
| `small_churn_16b` | 9 469 | 60 629 | 58 346 | 57 518 |
| `small_churn_16b_2n` | 14 077 | 67 285 | 65 118 | 63 662 |
| → Ir на пару, (2N − N)/64 | 72.0 | 104.0 | 105.8 | 96.0 |
| `dealloc_prealloc_only_16b` | 8 527 | 74 148 | 64 265 | 63 472 |
| `dealloc_free_only_16b_n16` | 9 267 | 75 196 | 65 389 | 64 603 |
| → Ir на free, (n16 − prealloc)/16 | 46.3 | 65.5 | 70.3 | 70.7 |
| clear на hit, (`alloc_clear_magazine_only_16b` − `_prefix`)/16 | 12.19 | 23.31 | 24.06 | 11.44 |
| `cold_alloc_only_256x16b` / `_2n` | 25 510 / 50 915 | 156 554 / 269 892 | 116 991 / 190 729 | — |
| → Ir на alloc, (2N − N)/256 | 99.2 | 442.7 | 288.0 | — |
| `multiseg_cold_256k` | 30 394 | 4 200 265 | 4 028 010 | — |
| `seg_cycle_decommit_256k` | 72 256 | 12 858 979 | 12 374 664 | — |
| `large_alloc_free_cycle` | 4 410 | 49 922 | 49 951 | — |
| Large-цикл, hit − prefill | 378 | 1 340 | 1 303 | — |
| `mimalloc_small_churn_16b` (A/A) | 16 629 | 16 629 | 16 629 | — |
| `mimalloc_cold_alloc_only_256x16b` (A/A) | 24 584 | 24 584 | 24 584 | — |

Для сравнения: августовская точка `docs/perf/_raw_item56_endpoint_42d42061.log` (2026-08-05) — churn 72.0 Ir на пару, `multiseg_cold_256k` 27 247, `seg_cycle_decommit_256k` 64 701, Large-цикл 379. Между августом и 2026-09-22 картина стабильна.

## Приложение C. Модель стоимости скана (ГИПОТЕЗА)

**Параметры.**
- Слово pending покрывает 64 гранулы × 16 B = 1 KiB payload; цена слова ≈12 Ir (PG-3r и item 80).
- Блок класса `SMALL_MAX` = 258 752 B ≈ 252.7 слова; 15 блоков на сегмент (doc бенча, `benches/perf_gate_iai.rs:2840–2862`).
- `refill_n` = 1 для этого класса (65 536 / 258 752 = 0 → 1; `tcache.rs:127–139`); `FREE_PARK_CAP` = 1.
- Порядок скана — по индексу слота: P = 0, seg2 = 1, seg3 = 2; остановка на первом сегменте со свободным блоком; drain выполняется до проверки BinTable.
- `end_word` полного сегмента = 4 043; начало payload s неизвестно в пределах [0, 252]; принято s = 150, то есть полный сегмент ≈ 3 893 слова (разброс ±3%).

**multiseg, раунд 1.** 34 промаха, каждый сканирует всё нарезанное:
- P растёт: Σ = 31 863;
- P полон, растёт seg2: 58 395 + 31 863;
- оба полны, растёт seg3: 23 358 + 1 824;
- итого ≈ 1.473·10^5 слов.

В коде 2026-09-22 те же вызовы стоили O(1) на кандидата: работал pre-drain empty-guard кольца.

**multiseg, раунд 2.**
- 1 hit magazine и 3 блока из freelist текущего seg3 — без скана;
- 15 промахов × P (3 893 слова) → hit;
- 15 промахов × (P + seg2 из пула, 7 786) → hit;
- итого ≈ 1.752·10^5 слов.

**Итог multiseg.** ≈ 3.22·10^5 слов; при 12 Ir на слово ≈ 3.87 M Ir. Наблюдённая дельта 4 028 010 − 30 394 = 3 997 616, то есть 12.4 Ir на слово.

**seg_cycle.**
- После free каждого раунда состояние повторяет начало раунда 2: P — 15 свободных, seg2 — 15 свободных и в пуле (по умолчанию `pool_segments` = 4, item 13), seg3 — 3 свободных плюс 1 в magazine.
- Итого 1.473·10^5 + 5 × 1.752·10^5 ≈ 1.02·10^6 слов ≈ 12.3 M Ir. Наблюдено 12 302 408, то есть 12.0 Ir на слово.
- Если seg2 не попадает в пул, а освобождается, получается ≈ 0.88·10^6 слов (≈14 Ir на слово). Вывод о том, что доминирует скан, от этого не меняется. Doc бенча («6 decommits per run») старше пула и здесь не проверялся.

**Рост (16 B, `GlobalAlloc`).**
- Refill берёт 16 блоков, скан на момент t покрывает t/64 слов.
- Σ ≈ N² / 2048 = H² / 2^19.

**`pool_cap_sweep` 16 B (`AllocCore`).**
- N ≈ 1.05·10^7, discovery раз на 32 блока: Σ ≈ N² / 4096 ≈ 2.7·10^10 слов.
- При 206.05 s выходит ≈ 7.7 ns на слово. Это модель по одному прогону, а не замер цены слова.

## Приложение D. Witness

Не запускалось: режим только чтение. Ни одно утверждение отчёта не основано на исполнении этим ревью. Первыми кандидатами на исполнение по решению владельца:
1. iai-пара `multiseg_cold_256k` с OPT-01 и без него, со счётчиками слов;
2. bisect по `dealloc_free_only_16b_n16` − prealloc в окне `1f4ac4f3`..`fd954856`;
3. бенч роста 1–64 MiB по 16 B;
4. волновой бенч реестра с `heaps_claimed_high_water`.
