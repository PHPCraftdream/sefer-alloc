# Ревью исправлений раунда 6 — корректность

Ревизия: `main` @ `ac23d06e`. Диапазон: `132ab85b..HEAD`. Режим: только чтение. Ничего не собиралось
и не запускалось: ни cargo, ни тесты, ни скрипты. Все выводы получены чтением кода, тестов и
`git show`/`git diff`. Номера строк даны по `HEAD`.

Вне ревью, потому что не влиты в main: R6-T02, R6-T16 (PR #64), R6-T41, R6-T43, R6-T44, R6-T45,
R6-T55, R6-T56, R6-T57.

## 1. Резюме

- Из 48 задач в области ревью 27 закрыты по существу. Ещё 21 закрыта частично. Почти везде
  частичность объявлена честно: так решил владелец, или в карточке есть открытый остаток.
  Задач, где механизм только замаскирован, не найдено.
- Одна находка P2 в коде касается R6-T10. Скалярная Large-аллокация потеряла полный sweep, а
  rescue-повтора на null не получила. Batch- и refill-пути такой повтор имеют. Механизм
  подтверждён чтением, масштаб — оценка. Последствия:
  - отказ (null → `handle_alloc_error`) при свободной, но не вернувшейся памяти;
  - удержание до ~25 % активных Large-слотов мёртвыми в режиме «producer → consumer»;
  - ось цены «удержание RSS» из плана в гейт-отчёт не попала.
- Одна находка P2 по процессу: индексы открытых пунктов расходятся с кодом. Исправлено, но открыто:
  166, 201, 202, 236, а также заметки к perf 4/66/83. Закрыто, но в карточке остался старый текст:
  217, 218. Закрыто, но исправлено не полностью: perf 91 (P2-1) и 217 (P3-1).
- Находок P3 восемь:
  - неполная граница коммита в foreign `realloc` для Primordial в `production` (R6-T50);
  - утечка Large после OOM-сжатия под `hardened` без promotion (R6-T29);
  - вакуозный тест R6-T07 и обход его кэша через OOM конструктора;
  - тест R6-T35 не покрывает production-ожидание;
  - TOCTOU в batch-pop (R6-T06, гипотеза);
  - блокировка stderr внутри `fatal` (R6-T15, гипотеза);
  - незавершённый перенос `rehash` в R6-T23;
  - набор документов, устаревших после самих исправлений.
- Нарушений соседних инвариантов в подтверждённом виде не найдено. Проверены: учёт live, free-list
  и bitmap, prepare→commit, cross-thread free, decommit/recommit, TLS и registry. Две находки
  (P3-2 и P2-1) — это регрессии на краевых путях, а не порча.

## 2. Таблица по задачам

Вердикт: **З** — закрыта, **Ч** — частично, **Н** — не закрыта.

| Задача | Вердикт | Механизм / соседние инварианты / тест | Доказательство |
|---|---|---|---|
| T01 | З | Запись stamp и `last_stamped_segment` удалены. Сброс на cache-hit удалён. В production нет читателей `owner_state` (проверено grep). Layout-поле осталось инертным. | `src/registry/heap_core/state/ownership.rs:1`; `src/registry/heap_core/alloc/hot.rs:200`; `src/alloc_core/segment/segment_header/segment_header_impl.rs:276` |
| T03 | З (с оговоркой P3-3) | Слот после OOM конструктора публикуется в FREE-bitmap, а не в hint. Повтор через recovery ограничен одной попыткой. Тест: hint = `MAX_HEAPS`, popcount ≥ 1. Контрфакт сработал бы. Loom-модель добавлена. | `src/registry/heap_registry/claim.rs:404-417`, `:779-788`; `tests/registry_claim_ctor_oom_recovery.rs` |
| T04 | З | Escape срабатывает по общему null-check в обеих ветках `alloc`, `alloc_zeroed` и `realloc`. Покрыт и `Some(null)` из fallback. Helper-ы `exit_escape_*`. README обновлён. | `src/global/sefer_alloc/global_alloc.rs:21-59`, `:94-99`, `:153-155`, `:202-206` |
| T05 | Ч (по решению владельца) | Default: делимость по reciprocal-таблице. `hardened`: точная проверка класса по class map. Aligned wrong-class free в default проходит (≈1/3 смещений сценария SEC-1). Batch и substrate free не покрыты. Всё это задокументировано (item 247, README). Тест не вакуозен. | `src/registry/heap_core/free/dealloc_own_base.rs:404-422`; `src/alloc_core/platform/size_classes.rs:210-233`, `:319-336`; README:1690-1693 |
| T06 | Ч | Продолжение проверяется на диапазон `[meta_end, bump)` и кратность `MIN_BLOCK`, нарушение → `fatal`. Safe-linking нет (item 247). Повторное чтение в commit-проходе не валидируется (P3-5). | `src/alloc_core/config/continuation_bounds.rs`; `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:447`, `:610`, `:639` |
| T07 | Ч | Негативный вердикт кэшируется в TLS по версии `SaturationHint` с пропуском до 64 попыток. Сериализация на fallback-локе осталась и задокументирована в README:1748-1755. Тест проверяет только чистую функцию (P3-3). | `src/global/tls_heap.rs:135-172`, `:474`, `:491`, `:543`; `tests/tls_saturation_gate_decision.rs` |
| T08 | З (doc + гейт) | Амплификация описана. Гейт `exact-span-large` дал NO-GO по преррегистрации. Код не менялся, это допускает план. | README:174-191; `docs/perf/R6_T08_LARGE_COMMIT_AMPLIFICATION_PREREG.md` |
| T09 | З | Узкие layout в batch идут через `ExactNarrow::alloc`/`try_dealloc`. Остальное — heap-route, как в скалярном пути. | `src/global/sefer_alloc/batch.rs:70-84`, `:134-160`; `tests/r6_t09_batch_exact_narrow.rs` |
| T10 | **Ч (P2-1)** | (а) `kind_at` переведён в `debug_assert`. (б) Ограниченная проба есть, но полный sweep перед OS-резервом удалён (`b86ab03a`), а rescue на null нет. Ось RSS не измерена. | `src/registry/heap_core/alloc/hot.rs:181-190`, `:368-374`, `:614-622`; `src/alloc_core/alloc_core/sidecar_drain.rs:525-580`; `src/alloc_core/large/alloc_core_large.rs:674-686` |
| T11 | З | Пары `(key, root)`, sentinel `usize::MAX`. Все ключи маскируются через `segment_base_of_ptr`, поэтому коллизии с sentinel нет. `own_cache_clear` сравнивает root. | `src/alloc_core/segment/segment_table/segment_table_impl.rs:160`, `:738-753`, `:861-866` |
| T12 | З (NO-GO) | Кандидат откатан. Нетто-изменение `src` у PR #58 равно 0 (`git diff 480669b1^1 480669b1 -- src` пуст). | `docs/perf/R6_T12_OWNER_DRAIN_CONTEXT_GATE.md` |
| T13 | Ч | Маска `& (TCACHE_CAP-1)` и const-assert на степень двойки стоят. Asm не снимался, item 94 OPEN, это консистентно. Маска — тождество, пока `cnt < cap ≤ TCACHE_CAP`. | `src/registry/heap_core/state/tcache.rs:78-83`; `hot.rs:275`, `:422`; `dealloc_own_base.rs:447` |
| T14 | З (release) | `expect`/`unreachable!`/`assert!` на путях `GlobalAlloc` заменены на `fatal`. Debug-остаток (`debug_assert`, индексация, переполнения) задокументирован как residual. | `src/alloc_core/large/alloc_core_large_cache.rs:150`, `:166`, `:172`, `:366`; `src/alloc_core/segment/segment_header/terminal_words.rs:49-53`; `src/global/sefer_alloc/mod.rs:199-204` |
| T15 | З | Голых `process::abort` вне `fatal.rs` нет (grep). Lexical-guard работает с self-test. Дубль `exact_fatal` удалён. Карточка 202 не закрыта (P2-2). Риск блокировки stderr — P3-6. | `src/alloc_core/platform/fatal.rs`; `scripts/verify-no-bare-abort.mjs` |
| T17 | Ч | PR #60 меняет только комментарии (проверено фильтром diff). TFS, ring и прочие удалённые токены исчезли. Остались ссылки `.rs:NNN` и неполный guard — карточка 243 PARTIAL, это консистентно. | `src/alloc_core/alloc_core/sidecar_drain.rs:425-426`, `:451` |
| T18 | З | `consume_small_cuts`/`finish_small_drain`/`retire_large_route` эквивалентны трём копиям. Клампинг `start_word`, тексты fatal и счётчики сохранены. Route-borrow заканчивается до release. | `src/alloc_core/alloc_core/sidecar_drain.rs:206-330`, `:441-487`; `find_segment.rs:165-200` |
| T19 | Ч (по дизайну) | В `scan_directory_candidates` единственный `return` — это `Some`, значит ранних `None`-выходов нет и поведение эквивалентно. Часть функций оставлена неразделённой, причина указана в коммите. Перенос `mem_impl.rs` на 476 строк построчно не сверялся. | `src/alloc_core/small/alloc_core_small/find_segment.rs:202-300` |
| T20 | З | Предикат хранится в одном месте. Один `cfg_attr` (только lint) остался рукописным. Все вызовы — одиночные элементы; проверено сканером. Латентный риск: макрос гейтит только первый элемент тела. | `src/alloc_core/config/medium_promotion.rs:116-195` |
| T21 | З процессно, с рассинхроном (P2-2) | Карточки 206–247 и perf 90–101 заведены. Часть статусов расходится с кодом. | `docs/CORRECTNESS_OPEN_ITEMS.md:140`, `:573-613` |
| T22 | З | `off < span` проверяется до `Node::deref` в обоих местах. Item 222 (Miri-квитанция) PARTIAL, это консистентно. | `src/alloc_core/alloc_core/mem/mem_impl.rs:42-56`; `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:238-250` |
| T23 | Ч (P3-7) | Есть yield и отказ при выходе. `try_dealloc`/`find` при отказе ведут себя как «не наш», `register` на выходе отдаёт незарегистрированный блок. Освобождение старого массива вынесено из-под лока, но `System.alloc_zeroed` в `rehash` остался под локом. | `src/global/exact_object/exact_shard.rs:58`, `:206-225`, `:245-265`; `narrow.rs:50-57` |
| T24 | З | Кандидат в MAINTENANCE переопубликуется, повтор ограничен (8192 yield). Тест с контрфактом (count = 1). | `claim.rs:370-385`; `tests/registry_claim_maintenance_candidate.rs` |
| T25 | З | Вытесненные блоки складываются в `Retired` и освобождаются после `drop(guard)`. Retire вызывается не больше двух раз — этого хватает. | `src/registry/segment_route/directory.rs:513-525`, `:677-683`, `:984-987` |
| T26 | З | TTAS и экспоненциальный backoff. Проба выхода прорежена до раза на 64 yield, отказ при мёртвом держателе сохранён. README устарел (P3-8). | `src/registry/segment_route/shard_lock.rs:64-97`; `src/global/fallback.rs:507-545` |
| T27 | З | Для fallback по умолчанию работает ingress-only, полный trim — после 100 тихих визитов. Самоучёт try-lock корректен. Тест с тремя фазами и мутантами. RSS-ось вынесена в perf item 100. | `src/global/maintenance_service.rs:37`, `:182-232`; `ownership.rs:37-52` |
| T28 | З по механизму | Класс хранится только в стартовой грануле. Внутри маршрута сегмент не сбрасывается: retain-reset — только тестовый хук, pool сохраняет free-lists, release даёт новый sidecar. Тест не вакуозен. Карточка 166 не обновлена (P2-2). Память под spill (до 256 KiB на сегмент) не измерена — это признано в гейт-отчёте. | `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:114-143`; `src/alloc_core/small/alloc_core_small_pool/decommit.rs:141-150`; `tests/r6_u21_foreign_free_interior_granule.rs` |
| T29 | З, с краевой регрессией (P3-2) | При OOM в move-leg сжатия Large возвращается исходный `ptr` и записывается `large_size`. Тест покрывает только Large→Large. | `src/registry/heap_core/free/realloc.rs:354-367` |
| T30 | З | Округление проверяется (`checked_mul`), promotion идёт после проверки `Layout`. | `src/alloc_core/large/alloc_core_large.rs:258-286`; `realloc.rs:314-320` |
| T31 | З | Тик пула стоит рядом с каждым `maybe_decay_large_cache`. Освобождение пула внутри Large-циклов безопасно: индексы стабильны, `base_at` = null после recycle. | `alloc_core_large.rs:204-209`, `:784-787`; `mem_impl.rs:309-314` |
| T32 | З | `maybe_materialize_directory()` перед fallback-сканом. | `find_segment.rs:502` |
| T33 | З | `Unknown` → `RejectNoOp` до M2-оракулов. | `dealloc_own_base.rs:277-279` |
| T34 | З | Слив пула и повтор `reserve_biased` вне NUMA. | `alloc_core_large.rs:620-653` |
| T35 | Ч | Ожидание ограничено 30 с → `fatal`. Вопрос владельца (opt-out) открыт, карточка 228 PARTIAL. Тест проверяет только ожидание `START_PAUSE` (P3-4). | `maintenance_service.rs:44`, `:93-147` |
| T36 | З на момент коммита | `SECURITY.md` и doc `hardened` переписаны 15.10, а T05/T28 снова сделали doc `hardened` устаревшим (P3-8). | `Cargo.toml:671-689` |
| T37 | З | Tier-1 allow снят, на хуке стоит item-level allow. | `src/global/tls_heap.rs:703` |
| T38 | З | Без `internals` регистрация идёт через `register_route` (`pub(crate)`), `register` остался forwarder-ом под `internals`. Сканер инвентаризует сигнатуры с сырыми указателями. | `directory.rs:816-842` |
| T39 | Ч | PF-7: таблица `FREE_PARK_CAP`. PF-9: Relaxed pre-read. PF-10: корень из witness. PF-8 открыт (perf 96). | `hot.rs:657-661`; `claim.rs:169-173`; `dealloc_own_base.rs:578-585` |
| T40 | Ч (по гейту) | OPT-04 (`System.alloc` + полная in-place инициализация) корректен: оба поля `#[repr(C)]` пишутся целиком. OPT-05 откатан, нетто-diff PR #63 — только OPT-04. OPT-09/10/11 не делались. | `directory.rs:54-69`; `small_sidecar.rs:52-66` |
| T42 | Ч | Escape-helpers есть, `exact_fatal` удалён. `finalize_hit` и TLS-classify не сведены, карточка 240 PARTIAL. | `global_alloc.rs:21-59` |
| T46 | Ч | Добавлены cfg-контексты и `dbg_forced_oom`, дубль-счётчик удалён. Судьба R18/R19 cfg не решена. | `find_segment.rs:17-37`; `src/global/fallback.rs:159-166`, `:678-695` |
| T47 | Ч (минимум плана) | Строка CI в `docs/CI_TIERS.md` названа. Сама синхронизация под `internals` по-прежнему отличается, карточка 226 PARTIAL. | `docs/CI_TIERS.md:51-74` |
| T48 | З (doc) | Контракт `kind_at` приведён к поведению. Различие abort и `None` задокументировано. | `src/alloc_core/segment/segment_header/segment_header_views.rs:34-38` |
| T49 | Ч | `#![warn(missing_docs)]` включён, модули `internals` помечены `allow`. Изменения API (semver) не делались. Карточка 236 устарела (P2-2). | `src/lib.rs:320`, `:446`, `:478`, `:491` |
| T50 | **Ч (P3-1)** | Large ограничен фронтиром `span_usable`, у Small отвергаются метаданные. Primordial под `primordial-lazy-commit` (в `production`) не ограничен коммитом. | `realloc.rs:478-497`; `realloc_fastpath.rs:107-125` |
| T51 | З (решение) | Не реализовано, оформлено как design-residual в карточке 246. | `TRACKED_correctness_residuals.md:167-175` |
| T52 | З | Compile-time деструктуризация без `..` плюс `assert_send`. | `src/registry/heap_slot.rs:290-330` |
| T53 | Ч | Loom-модели добавлены. Остаток перечислен в карточке 186: реальный `ShardLock`, `fallback::LOCK`, сертификация saturation. | `TRACKED_verification_coverage.md:236` |
| T54 | З (статически) | Разбор гипотез записан в карточку 221 (PARTIAL), код не менялся. | `TRACKED_verification_coverage.md:279-289` |

## 3. Находки

### P1

Нет.

### P2-1. R6-T10: скалярная Large-аллокация без rescue-sweep; удержание опубликованных Large

- **Подтверждено чтением.** До раунда каждый скалярный Large alloc начинался с полного
  `drain_large_sidecar_ingress()`: `git show 132ab85b:src/registry/heap_core/alloc/hot.rs:195-200`,
  `:640-643`. Теперь там `drain_large_sidecar_ingress_hot_bounded()` — 4 слота с ротацией курсора
  (`hot.rs:368-374`, `:614-622`). Полный sweep перед OS-резервом из первой версии PR удалён коммитом
  `b86ab03a`.
- Если `AllocCore::alloc_large` вернул null, повтора со sweep нет:
  - заполнена таблица (`alloc_core_large.rs:674-686`);
  - отказал резерв, а `drain_small_pool` повторяется только для пула.
- Небезопасные пути при этом повтор имеют:
  - `refill_with_large_rescue` для Small-refill (`hot.rs:676-688`);
  - `alloc_batch_large` для batch (`src/registry/heap_core/alloc/batch.rs:219-240`).
- Асимметрия появилась в этом раунде.
- **Сценарий.** Поток A аллоцирует Large-буферы, поток B освобождает их (cross-thread). Опубликованные,
  но не отозванные маршруты держат слот таблицы и закоммиченный span.
  - Отказ. Вблизи `MAX_SEGMENTS` (4095) или при отказе ОС в резерве (на Windows — commit charge)
  `alloc` возвращает null, хотя полный sweep освободил бы слоты и кэш. Дальше
  `handle_alloc_error` → abort.
  - Удержание. Каждая аллокация отзывает в среднем 4·P/K записей, где P — опубликованные слоты, K —
  все активные. В равновесии «одна аллокация на одно чужое освобождение» получается P ≈ K/4, то есть
  P ≈ L/3 от живых. Это оценка по модели, не измерение. До R6-T10 было P = 0 на каждой аллокации.
- **Последствие:** ложный OOM/abort, рост RSS и commit в режиме cross-thread Large. Гейт-отчёт
  `docs/perf/R6_T10_LARGE_SWEEP_PROBE_GATE.md` меряет только Ir. Ось цены из плана — «удержание RSS при
  cross-thread Large free в том же режиме» — отсутствует, то есть нарушено правило одного режима
  (R31-1).
- **Уверенность:** высокая для механизма, средняя для масштаба.
- **Исправление.**
  - В Large-ветках `alloc_with_class` (`class == None`) и `alloc_zeroed` на null от `alloc_large`
    вызвать `drain_large_sidecar_ingress_rescue()` и повторить один раз. Это та же форма, что
    `refill_with_large_rescue`, только холодный путь. Контракт R12-05 на успешном пути не меняется.
  - Тест: опубликовать K Large из чужого потока и инжектировать отказ резерва. Следующий
    `HeapCore::alloc(Large)` должен получить блок из кэша после rescue. Сейчас тест упал бы.
  - Отдельно — RSS-ячейка «producer → consumer Large» с oracle `LARGE_REMOTE_RETIREMENTS` в том же
    режиме.

### P2-2. Индексы открытых пунктов расходятся с кодом

Подтверждено чтением карточек и кода.

- **Исправлено, но открыто.**
  - Item 166 (`TRACKED_correctness_residuals.md:136-143`) всё ещё описывает uniform-лист и abort
    владельца. R6-T28 это устранил: interior-гранула читает 0 и отбрасывается у производителя. План
    требовал «обновить карточку item 166».
  - Item 201 (`:145-150`) утверждает: «Production does not validate Small free-list links». После
    R6-T06 это не так: остаток описан в item 247, а сама 201 не закрыта и не перенаправлена.
  - Item 202 (`:152-157`) говорит о «голых abort без диагностики». R6-T15 перевёл все такие места на
    `fatal`, но карточка открыта.
  - Item 236 (`TRACKED_publish_readiness.md:995-1001`) числится «queued R6-T49», хотя
    `#![warn(missing_docs)]` уже влит.
  - В perf-индексе заметки раунда 6 к item 66 (`docs/perf/OPEN_ITEMS.md:2988`), item 83 (`:3062`) и
    item 4 (`:278`) описывают состояние до R6-T07/T29/T17/T08. Пример: «threshold is not in the README»
    — теперь он там есть, README:1748.
- **Закрыто, но карточка выглядит активной.** Это структурный дефект по правилу R34-24.
  - Items 217 и 218 (`RESOLVED.md:68-91`): статус CLOSED, но строки «Current:» описывают сам баг, а
    «Next trigger: … then move to RESOLVED.md» и «closure still awaits…» не убраны.
- **Закрыто, но исправлено не полностью.** Perf item 91 (P2-1) и item 217 (P3-1).
- **Ошибка ссылки.** Item 230 называет `a800d040` частью PR #42, а коммит пришёл в PR #46
  (`c398bfc7`).
- **Последствие:** следующий раунд получит ложную картину: открытые пункты, которые уже сделаны, и
  закрытые с остатком.
- **Уверенность:** высокая.
- **Исправление:**
  - 166 — закрыть с переносом в RESOLVED, остаток — в 231 и 247;
  - 201 — закрыть со ссылкой «superseded by 247»;
  - 202 — закрыть;
  - 236 — PARTIAL;
  - 217 и 218 — переписать Current и Next trigger;
  - 91 и 217 — вернуть в PARTIAL до исправления P2-1 и P3-1.

### P3-1. R6-T50: Primordial (production) не ограничен фронтиром коммита

- **Подтверждено чтением.** Foreign-ветка `HeapCore::realloc` для `RouteKind::Primordial` и `Small`
  проверяет только начало payload (`realloc.rs:487-495`). Комментарий там утверждает: «Small and
  Primordial segments commit the whole span eagerly» (`realloc.rs:473-476`).
  - Для Primordial это неверно: `primordial-lazy-commit` входит в `production`
    (`Cargo.toml:423`, `:774-804`). Сначала коммитится только `[0, primordial_meta_end +
    LAZY_FIRST_CHUNK)`.
  - Для Small это неверно под `small-segment-lazy-commit`.
  - Own-ветка этот случай учитывает: `safe_payload_read_span` → `small_committed_bound` →
    `committed_payload_end_of()` (`realloc_fastpath.rs:92-125`).
- **Сценарий.** Блок из primordial-сегмента кучи A передаётся в поток B. B вызывает `realloc` с
  ложным `old_layout`, бо́льшим закоммиченного фронтира. `copy_nonoverlapping` читает незакоммиченные
  страницы → SIGSEGV или access violation вместо null. Это ровно класс R6-MS-3.
- **Уверенность:** высокая для механизма. Предусловие — нарушение контракта, как у самой задачи (P4).
- **Исправление.** Ограничить чтение атомарным Acquire-чтением монотонного `committed_payload_end`.
  Primordial не декоммитится, фронтир только растёт. Либо брать гарантированный минимум
  (`primordial_meta_end + LAZY_FIRST_CHUNK`) и исправить комментарий. Тест: ложный layout на
  primordial-блоке из чужого потока → null.

### P3-2. R6-T29 × `hardened` без promotion: утечка Large после OOM-сжатия

- **Подтверждено чтением.** OOM-ветка сжатия возвращает Large-указатель при любом `new_size < old`,
  в том числе ≤ `SMALL_MAX` (`realloc.rs:354-367`). Затем вызывающий законно освобождает его с
  small-layout.
  - В сборке `hardened ∧ ¬promotion` ветка F7 (B) превращает «Large под small-layout» в no-op
    (`dealloc_own_base.rs:259-268`). Её обоснование — «legitimate Large-with-small-layout is
    structurally unreachable», и R6-T29 это предположение нарушает.
  - Ветка (A) под promotion проверяет согласованность с `large_size` и проходит, потому что R6-T29
    записывает `new_eff`.
- **Последствие:** утечка сегмента ≥ 4 MiB на каждый такой случай в режиме OOM. Тест
  `tests/r6_t29_large_shrink_move_oom_keeps_ptr.rs` покрывает только 8 → 4 MiB.
- **Уверенность:** средне-высокая.
- **Исправление.** В ветке (B) принимать Large, если `large_size_at == layout.size().max(MIN_BLOCK)`
  и `large_align_at == layout.align()` — та же проверка, что в (A). Либо в OOM-ветке R6-T29 оставлять
  блок на месте, только если новый layout классифицируется как Large. Тест: shrink 8 MiB → 64 B под
  инжекцией OOM, затем `dealloc(ptr, Layout(64))` под `hardened` → сегмент освобождён.

### P3-3. R6-T07: тест не проверяет подключение; кэш обходится через OOM конструктора

- **Подтверждено чтением.** `tests/tls_saturation_gate_decision.rs` вызывает только чистую
  `saturation_retry_decision`. Если удалить `if !saturation_retry_allowed()` (`tls_heap.rs:474`,
  `:491`) или `record_saturation_verdict()` (`:543`), тест останется зелёным. План требовал
  счётчик-оракул «claim не повторяется на каждой аллокации».
- **Гипотеза (средняя уверенность).** `push_back_after_oom` вызывает
  `saturation.publish_claimable()` (`claim.rs:787`) и тем меняет версию.
  - Пусть OOM конструктора держится, а непривязанных потоков два или больше. Неудача одного потока
    инвалидирует кэш другого, и каждая аллокация снова пытается выполнить `HeapCore::new` — это
    ОС-резерв.
  - Это и есть симптом R6-CC-1. Пункт (c) R6-T03 — backoff в TLS — так фактически не получен.
- **Исправление.**
  - Тест: счётчик попыток claim в `bind_slow_tagged`, сценарий «registry saturated»; можно через
    injection ограниченного числа слотов или `dbg_with_word_for_test`.
  - Для OOM — не публиковать claimability у неинициализированного слота или хранить в TLS отдельный
    backoff по OOM.

### P3-4. R6-T35: тест не покрывает production-ожидание RUNNING

- **Подтверждено чтением.** Тест (`tests/r6_t35_start_confirmation_bound.rs`) управляет
  `START_PAUSE`. Это bench-internals-ожидание до spawn (`maintenance_service.rs:93-117`), и таймаут
  для него добавлен отдельно. Если откатить production-ограничение ожидания RUNNING (`:127-145`),
  тест останется зелёным.
- **Уверенность:** высокая.
- **Исправление:** test-хук задержки публикации RUNNING в `worker()`, тест на fatal
  «running-state acknowledgement timeout».

### P3-5. R6-T06: повторное чтение `next` в commit-проходе batch-pop не валидируется (гипотеза)

- В `try_drain_freelist_batch` probe-проход проверяет каждое продолжение. Commit-проход читает `next`
  заново и доверяет ему без проверки (`alloc_core_small_impl.rs:628-640`, «validated this same chain
  link by link»).
- Между проходами идут `prepare_small_issue` — это System-аллокации spill. Конкурентная UAF-запись в
  свободный блок (модель угроз SEC-3) в этом окне даёт непроверенный head через `as u32`.
- **Уверенность:** низкая-средняя. Окно узкое, нужна гонка.
- **Исправление:** в commit-проходе тот же `continuation_offset_or_fatal` с уже кэшированными
  `bounds` (одно сравнение) или сравнение с офсетами из probe-прохода.

### P3-6. `fatal()` берёт reentrant-лок std stderr (гипотеза; область R6-T15)

- `fatal` вызывает `std::io::stderr().write_all(msg)` (`src/alloc_core/platform/fatal.rs:5-6`). Это
  берёт `ReentrantLock` stderr и `RefCell`.
- R6-T15 перевёл на эту функцию все ~108 мест, включая OOM-путь
  `registry: chunk materialisation OOM`.
- **Сценарий.** Exit на Windows убил поток, державший лок stderr. Следующий `fatal` вешает процесс
  вместо abort — тот же класс «мёртвый держатель», который закрывали R6-T04 и R6-T23.
  Реентерабельный вход из аллокации внутри уже занятого stderr-borrow даст panic «already borrowed»
  в аллокаторе.
- **Уверенность:** механизм — высокая (поведение std); достижимость — низкая.
- **Исправление:** писать сообщение без лока: `libc::write(2, …)` или
  `WriteFile(GetStdHandle(STD_ERROR_HANDLE))`.

### P3-7. R6-T23 частично: `rehash` аллоцирует под shard-локом

- Освобождение старого массива вынесено за лок. `System.alloc_zeroed` нового массива остался под
  локом (`exact_shard.rs:58`). План требовал вынести `rehash` целиком по паттерну `Surplus`.
- `acquire_unless_exiting` зовёт FFI-пробу на каждой итерации после 64 спинов
  (`exact_shard.rs:206-225`), без прореживания из R6-T26.
- **Уверенность:** высокая. Влияние низкое: только `exact-object-proto`.

### P3-8. Документы, устаревшие из-за самих исправлений раунда

Подтверждено чтением. Переписать по текущему коду.

- `src/global/sefer_alloc/mod.rs:189-190`: «production does not validate free-list links» —
  противоречит R6-T06.
- `Cargo.toml:671-689` (doc `hardened`):
  - говорит, что guard интерьера `off % block_size` есть только в `hardened`, хотя после R6-T05 в
    default есть делимость, а в `hardened` проверка по class map;
  - говорит, что «cross-thread interior free … can still corrupt», хотя R6-T28 это закрыл.
- README:1784-1785: «give up after their initial 64 spins» — после R6-T26 отказ наступает после
  64 спинов и до 64 yield.
- README:1696: «OPT-C lazy stamp recovers ~1 %» — stamp удалён в R6-T01.
- `src/registry/segment_route/directory.rs:120`: «pointer came from System.alloc_zeroed» — после
  OPT-04 (R6-T40) это `System.alloc`.

## 4. Ограничения

- Ничего не запускалось. Вердикты «тест не вакуозен» — это контрфактуальная оценка чтением, а не
  прогон мутанта. Зелёные CI-квитанции не проверялись.
- Не сверены построчно:
  - перенос в `mem_impl.rs` и `realloc_fastpath.rs` из R6-T19 (476 + 90 строк). Эквивалентность
    проверена только для `find_segment.rs` и выделенных cold-функций по точкам выхода;
  - тела loom-моделей R6-T53;
  - изменения сканера `verify-dbg-hook-safety.mjs` (R6-T38) и CON-01 (R6-T04).
- Оценка удержания в P2-1 (P ≈ L/3) — модельная, предполагает равномерную ротацию курсора.
  Нужно измерение.
- P3-3 (обход через OOM), P3-5 и P3-6 — гипотезы по чтению, не воспроизведены.
- Ведомость «закрыто / открыто» в P2-2 составлена по файлам индексов на `HEAD` и коду на `HEAD`.
  Параллельные незакоммиченные правки других ревьюеров не учитывались.
