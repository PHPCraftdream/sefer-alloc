# Read-only review Round 18

**Дата:** 2026-07-26  
**Диапазон:** `a99314b..3b8fdc0`  
**HEAD:** `3b8fdc0570324a3132cc5c0f1d87bbc948a96ecf`  
**Режим:** только чтение истории Git и файлов. Сборка, тесты, Miri, Kani и
бенчмарки в рамках этого ревью не запускались. Единственная запись — этот
отчёт, явно запрошенный пользователем.

## Короткий вердикт

Round 18 — не новая волна радикального ускорения production allocator.
Из 14 коммитов только `912740f` меняет исполняемый код аллокатора; остальные
изменяют тестовую диагностику, измерительные артефакты, документацию и процесс.

Для plain `production` новая волна не ускоряет runtime-код: изменённая
medium-promotion ветка полностью компилируется прочь. Для opt-in
`production,medium-classes` изменение полезно: освобождения меньше 256 KiB
теперь обходятся без чтения `SegmentHeader::kind_at(base)`. Это статически
правильное удаление лишней работы, но его величина не измерена A/B против
предыдущего medium-кода. Записанные 8,275 Ir — новый baseline, а не delta.

Главный результат Round 18 — не новый speedup, а более честная переоценка уже
существующего `medium-classes`:

- alloc 256 KiB–1 MiB примерно в 25 раз быстрее baseline Large-пути;
- free примерно в 184–200 раз быстрее;
- последовательный realloc-grow остаётся примерно в 380–1,180 раз медленнее
  почти бесплатного in-place Large-grow;
- с `large-cache-extended` сумма средних времён alloc/free/realloc на данном
  специально realloc-heavy сценарии получилась около break-even;
- следовательно, medium-механизм уже является сильным ускорением для
  alloc/free-heavy профилей, но пока не годится как универсальный default.

Это не предел возможного ускорения. Самый большой оставшийся алгоритмический
барьер — первый перенос 256 KiB при medium→Large promotion. Устранение этого
копирования или предоставление growth-headroom способно превратить уже
доказанные 25×/200× alloc/free выигрыши в практически применимый профиль без
realloc-клифа.

## Что вошло в волну

| Коммит | Характер изменения | Runtime-эффект |
|---|---|---|
| `dc95d1a` | watchdog race-теста: `abort()` → `exit(124)`, progress и configurable deadline | Нет, только тесты |
| `912740f` | сужение `kind_at`-проверки до реально достижимой promotion-области | Малый положительный эффект только под opt-in `medium-classes`; ноль под plain production |
| `8833baa` | повторный medium A/B/B/A gate и raw/CSV результаты | Нет, измерение существующего кода |
| `290374b`, `4ba35dc` | инвентаризация cold-gap к mimalloc и исправление факта о CI | Нет |
| `1d2c9cd` | исправления противоречивых комментариев/отчётов | Нет |
| `ed75b06` | design-only защита от stale derived literals | Нет |
| `60633e3` | design-only adaptive Large policy | Нет |
| `3b8fdc0` | процессный индекс `OPEN_ITEMS.md` | Нет |
| остальные | reviews/checkpoints/CHANGELOG/CLAUDE | Нет |

Итого по diff: около 4,934 добавленных и 93 удалённых строк, но production
source затронут только в `src/registry/heap_core_free.rs` (+115/−43). Объём
изменений нельзя интерпретировать как объём ускорения: почти всё — документы.

## Что действительно стало лучше

### 1. Лишний header load под `medium-classes` удалён корректно

До `912740f` ветка читала `kind_at(base)` на каждом small-classified free под
`medium-classes`, включая 16/32/64-byte объекты, которые не могли возникнуть
через promotion. Теперь non-hardened путь сначала проверяет:

```rust
layout.size() >= MEDIUM_REALLOC_PROMOTION_THRESHOLD
```

и только затем читает kind. Cfg ветки также приведён к фактическому promotion
predicate. Для plain production весь блок отсутствует; для medium tiny-free
остаётся дешёвое сравнение вместо зависимого чтения заголовка.

Это хорошая микрооптимизация и хорошее исправление feature-combination
логики. Однако честная формулировка результата: **направление выигрыша
доказуемо по коду, величина не доказана**. Commit message приводит 8,275 Ir
только после изменения и прямо признаёт, что число включает прочие расходы
`medium-classes`; pre-change medium baseline отсутствует.

### 2. Watchdog больше не маскируется под memory corruption

`dc95d1a` устраняет диагностическую двусмысленность Windows/MSVC:
watchdog-timeout теперь завершает процесс кодом 124, а не через `abort()`,
который мог выглядеть как `STATUS_STACK_BUFFER_OVERRUN`. Добавлены elapsed
time, progress snapshot и override дедлайна.

Это не ускорение, но важное улучшение качества расследований: следующая
перегруженная CI-машина не должна породить ложную волну поиска corruption.

### 3. R18-2 исправляет стратегическое понимание medium-классов

Свежие результаты существенно полезнее старого бинарного “NO-GO”:

| Конфигурация treatment | alloc | free | realloc | сумма фаз |
|---|---:|---:|---:|---:|
| `medium-classes` | ~24.8× быстрее | ~200× быстрее | ~1,180× медленнее | ~3.4× медленнее |
| `medium-classes,large-cache-extended` | ~25× быстрее | ~184× быстрее | ~380× медленнее | ~1.04× медленнее / около parity |

Процент realloc-регрессии выглядит чудовищно потому, что baseline выполняет
этот grow примерно за 50 ns in-place. Любой обязательный memcpy проиграет
такому denominator на порядки. Поэтому 20% per-phase kill-gate здесь не
нейтральный критерий качества allocator; он кодирует продуктовое решение
“dense medium classes никогда не могут быть default, пока первый grow
копирует данные”.

Для workload без частого realloc в диапазоне 256 KiB–1 MiB вывод обратный:
механизм даёт одно из крупнейших уже реализованных ускорений проекта.

### 4. R18-7 правильно останавливает бесконечный микротюнинг cold-path

Статическая инвентаризация подтверждает, что заявленные Э1–Э11 действительно
приземлены, а две последующие идеи были честно отвергнуты. Остаточный cold
16-byte gap к mimalloc плавает примерно от 1.5× до 2.5×, а текущий
cross-allocator instruction-count baseline отсутствует.

Правильный следующий шаг здесь — сначала измерить сравнимый Ir/branch/load
профиль mimalloc, а не придумывать двенадцатую микрооптимизацию по noisy
wall-clock.

## Findings

### P1 — hardened no-op contract не доказан и фактически нарушается в promotion-ON комбинации

`tests/regression_hardened_large_kind_own_free.rs:63-65` требует, чтобы
освобождение Large pointer с fabricated small layout было no-op и Large
allocation оставалась live. Но при одновременно включённых:

- `hardened`;
- `medium-classes`;
- фактической realloc promotion;

ветка A в `src/registry/heap_core_free.rs:294-303` из-за
`cfg!(feature = "hardened")` безусловно читает kind и при `Large` вызывает
`self.core.dealloc(ptr, layout)`. Substrate dealloc маршрутизирует по kind и
освобождает/кэширует весь Large segment; layout там не сверяется с сохранённым
logical size/alignment.

То есть код делает не заявленный defensive no-op, а освобождение реального
Large allocation по некорректному layout.

Почему существующий тест может быть зелёным:

1. он проверяет, что адрес не попал в **small magazine**;
2. substrate free действительно не кладёт его в small magazine;
3. large cache может оставить страницы mapped и старый байт `0xCC` читаемым;
4. тест после этого читает `large` и затем “освобождает” его повторно —
   если первая операция реально освободила allocation, это уже потенциальный
   UAF/double-free внутри самого контрфактуального теста.

Это не новая ошибка, созданная `912740f`: promotion-ветка уже маршрутизировала
Large в substrate. Но Round 18 повторно объявляет тест доказательством no-op,
не заметив, что oracle различает только “не испортить small magazine”, а не
“не освободить Large”.

Рекомендуемое исправление:

- после membership/kind проверки в hardened-path сверять caller layout с
  header logical `large_size` и `large_align`;
- только точное допустимое совпадение/легитимный promoted layout направлять в
  substrate;
- mismatch оставлять no-op;
- тест должен независимо подтвердить, что segment всё ещё зарегистрирован и
  live после mismatched free, а не читать потенциально освобождённый payload;
- отдельно проверить promotion-ON и promotion-OFF feature combinations.

Нарушение исходит от unsafe caller contract, поэтому это не доказанная
soundness-ошибка safe API. Но оно противоречит обещанию и назначению
`hardened`, а текущий тест создаёт ложную уверенность.

### P1 — новый `OPEN_ITEMS.md` уже содержит заведомо закрытый пункт

`docs/perf/OPEN_ITEMS.md:124-128` утверждает, что под `numa-aware`
directory lookup отключён и всё ещё нужен node-aware bit selection.

Текущий код говорит обратное:

- `src/alloc_core/alloc_core_small.rs:554-568` явно фиксирует, что R11-6 уже
  включил node-indexed directory lookup под NUMA;
- `src/alloc_core/alloc_core_small.rs:608-660` строит local/unknown/foreign
  bucket order;
- `CHANGELOG.md:3834-3872` заявляет закрытие 140× cliff и O(1) поведение.

Индекс, созданный для предотвращения забытых и stale задач, в первом же
коммите воскресил уже реализованную R10-6 как open item. Это показывает, что
одной процессной конвенции недостаточно.

Нужно переместить пункт в “Recently resolved” со ссылкой на R11-6/R12-2/R13-2
и добавить простой doc-check: open item не должен ссылаться только на старый
design/verdict, если более новый CHANGELOG/checkpoint помечает его implemented.

### P1 — часть выводов R18-2 сформулирована сильнее, чем позволяют метрики

1. `segments_reserved_total` — не прямой cache-hit counter. В treatment
   присутствуют bootstrap/initial reservations; даже extended arm показывает
   20 reservations, хотя текст трактует их как 20 misses из 320. Поэтому
   “46%/94% hit rate” — грубый proxy, а не измеренная доля. Для следующего
   gate следует включить `alloc-stats` и читать `large_cache_hits` напрямую,
   либо вычесть отдельно измеренный fixed floor.
2. Фраза в `R14_4...md:655` “leak added commit, not wall-clock” причинно не
   доказана и механически слишком сильна. Leak исключал нормальный cache
   deposit/reuse и потому мог менять OS-round-trips и wall time. Допустимая
   формулировка: “на этом шумном R10 workload абсолютный wall-clock после
   фикса существенно не разрешился/не изменился, тогда как commit изменился
   радикально”.
3. Верхняя часть того же отчёта (`:28`, `:41`) всё ещё показывает старые
   1,700–2,300× как основной итог, хотя новая §7.1 (`:399-404`) запрещает их
   цитировать. Исторический §7.2 допустим, но executive summary файла должен
   начинаться с current verdict, иначе читатель получает устаревший вывод до
   того, как дойдёт до поправки.

### P2 — “full round” собран из трёх разных статистических сессий

Workload действительно возвращает `elapsed_ns = alloc_ns + free_ns +
realloc_ns`, но `scripts/r10_2_medium_gate.mjs` запускает отдельную A/B/B/A
сессию для каждой phase metric. Таблица full-round складывает средние,
полученные в разных наборах process launches.

Сумма полезна как оценка, но это не paired full-round sample и для неё нет
собственного SD/t/sign-test. Особенно при заявленной загрузке CPU 66–94%
разница около 4% не должна называться статистически подтверждённым parity.

Исправление дешёвое: четвёртый runner pass по уже выдаваемому `elapsed_ns`,
с собственными paired delta, SD/Δ, t-test и sign-test. Ещё лучше — один runner
должен сохранять все четыре metrics из каждого запуска, чтобы фазы и total
оставались коррелированными.

### P2 — watchdog по-прежнему позволяет watcher-thread panic не провалить тест

Новый `Drop` печатает payload panicked watchdog thread, но затем возвращает
успех. Это лучше молчаливого swallow, однако сломанный watchdog означает, что
stress-test потерял ограничитель зависания.

Если основной поток не unwinding, join error должен проваливать тест. Если
основной поток уже panicking, достаточно диагностического `eprintln!`, чтобы
не вызвать double-panic abort.

### P2 — promotion predicate размножен как сложное cfg-выражение

Один и тот же логический predicate теперь повторяется в import cfg, branch A,
отрицании branch B и promotion implementation. Именно drift между такими
копиями породил R17/R18 работу.

Стоит иметь один канонический внутренний признак/макрос и feature-matrix test,
который перечисляет ожидаемое состояние promotion для всех важных комбинаций.
Текущие 100+ строк комментариев вокруг короткой ветки объясняют историю, но не
устраняют источник будущего расхождения.

## Где ещё возможно сильное ускорение

### 1. P0 performance — growth-aware medium allocation

Это крупнейший доказанный резерв.

Сейчас dense medium classes выигрывают alloc/free на порядки, но первый grow
через 256 KiB требует выделить Large span и скопировать prefix. Ни extended
cache, ни reserved capacity не могут отменить уже начавшийся первый copy:
они помогают только получить destination дешевле или расти в нём дальше.

Нужен отдельный дизайн, а не ещё одна комбинация существующих флагов:

- medium page-run/extent, в котором соседние pages можно зарезервировать как
  growth headroom;
- lazy commit соседнего VA и in-place grow без смены pointer;
- либо growth-aware классы/arena для объектов, которые действительно
  realloc-grow, с контролируемой внутренней фрагментацией;
- fallback на существующий promotion-copy, когда соседнее пространство занято.

Ключевой gate — не только per-realloc ratio против 50 ns baseline, а
workload-weighted total time, bytes copied, move-leg count, commit/RSS и p99.

Потенциал: сохранить 25×/184–200× alloc/free выигрыш и убрать главный
структурный штраф. Это гораздо сильнее дальнейшего scalar tuning.

### 2. P0 product — именованный throughput-medium профиль

Уже существующая комбинация
`medium-classes + large-cache-extended` почти break-even даже на намеренно
realloc-heavy тесте и резко выигрывает alloc/free. На workload, где medium
объекты обычно создаются/освобождаются, но редко растут, это готовое крупное
ускорение.

Не следует молча включать её в generic `production`. Лучше:

- отдельный compile-time bundle/profile, например allocation-heavy medium;
- явно документированный RSS trade-off;
- workload gate с несколькими realloc intensities и найденной break-even
  кривой;
- реальный consumer/adoption test.

Это продуктовая упаковка уже работающего алгоритма, а не новый unsafe
механизм.

### 3. P1 — закрыть cold 16-byte gap только после cross-allocator Ir профиля

Потенциальный потолок 1.5–2.5× всё ещё велик, но причина не доказана.
Следующий эксперимент должен сравнить Sefer и mimalloc по:

- instructions/op;
- branches и branch misses;
- loads/stores;
- page faults/commit calls;
- отдельно virgin carve, recycle и bootstrap floor.

Если mimalloc действительно выполняет существенно меньше Ir, тогда следует
снова разобрать current cold call chain и объединять metadata transitions.
Если Ir близок, wall-clock gap надо искать в VM/page-touch/cache topology, а
не в Rust-ветках. До этого новая “eureka” — угадывание.

### 4. P2 conditional — page-run для 1.25–2 MiB

Design обещает 3–6× лучшую плотность arena и меньше segment reservations, но
реального victim workload пока нет. Это крупная и correctness-чувствительная
архитектура: новый `SegmentKind`, таблица, remote-free encoding, NUMA и
decommit lifecycle.

Реализовывать её стоит только после workload, который действительно упирается
в `MAX_SEGMENTS` или OS reservation syscalls. Для обычного RSS-вопроса
`exact-span-large` уже дешевле и проще.

### 5. P2 conditional — batched deferred reclaim

Sub-design A может убрать повторные per-block decommit checks. Sub-design B
имеет смысл только если один drain sweep регулярно опустошает несколько
segments. Сначала нужен счётчик распределения “segments finalized per sweep”.
Без такого victim это вероятнее единицы процентов, а не радикальный выигрыш.

## Что улучшить в коде

1. Исправить hardened Large-layout validation и сделать тест no-op
   невакуозным.
2. Канонизировать promotion cfg predicate вместо нескольких ручных копий.
3. Разделить короткую инвариантную документацию рядом с hot code и длинную
   историческую хронику в design/review doc. Сейчас около 100 строк комментария
   обслуживают ветку в несколько строк и затрудняют собственно code review.
4. Вынести явные типизированные helpers:
   `is_legitimate_promoted_large_dealloc` и
   `reject_mismatched_large_layout`, чтобы policy была видна из имён.
5. Для защитных тестов проверять состояние allocator metadata/counters, а не
   делать последующие raw reads из объекта, который тестируемый баг мог уже
   освободить.
6. Watchdog panic должен быть test failure, если основной тест не unwinding.

## Что улучшить в проекте и измерениях

1. Исправить stale NUMA item в `OPEN_ITEMS.md`; затем проверять новые пункты
   против более поздних implementation commits.
2. Хранить current verdict в начале каждого perf-report, исторические данные
   — в явно помеченном appendix. Не оставлять опровергнутую executive table
   перед актуальной поправкой.
3. Мерить direct cache hits, а не выводить их из total segment reservations.
4. Добавить настоящий paired `elapsed_ns` gate вместо суммы средних разных
   phase sessions.
5. Для medium decision перейти от универсального “каждая phase не хуже 20%”
   к двум уровням:
   - safety/regression gates по отдельным операциям;
   - workload-weighted product profiles с break-even кривой.
6. Добавить cross-allocator Ir feasibility probe; только после него решать,
   есть ли смысл продолжать cold-path microtuning.
7. Сократить документационный churn. В этой волне тысячи строк prose
   сопровождают десятки строк runtime logic, причём уже появились stale и
   противоречивые утверждения. Критические числа и статусы лучше генерировать
   из CSV/manifest, а не копировать вручную в несколько отчётов.
8. В summary каждой волны разделять:
   - runtime code landed;
   - measured existing behavior;
   - design-only;
   - docs/process.
   Тогда “выполнено 9 задач” не будет неявно читаться как “код стал быстрее
   девятью способами”.

## Рекомендуемый следующий этап

1. **Сначала correctness/hardening:** закрыть Large mismatched-layout gap и
   усилить oracle теста.
2. **Дешёвая методология:** direct hit counters, paired `elapsed_ns`, убрать
   stale NUMA item и старый executive verdict.
3. **Главный perf design:** growth-aware medium arena/headroom, цель —
   устранить первый 256 KiB copy.
4. **Параллельный product gate:** оформить allocation-heavy medium profile и
   измерить break-even по realloc intensity.
5. **Cold gap:** добавить mimalloc Ir arm; не начинать новую микрооптимизацию
   до результата.

## Итог

Да, проект в целом за последние раунды ускорен сильно, и R18 подтверждает
особенно большой выигрыш medium alloc/free. Но именно новая волна R18 почти
не меняет скорость plain production: она удаляет один лишний header read в
opt-in конфигурации и главным образом исправляет измерения и процесс.

До абсолютного предела далеко. Наиболее перспективный следующий скачок —
не ещё одна проверка, bitmap или cache constant, а устранение structural
medium→Large copy либо создание отдельного allocation-heavy профиля, который
уже сегодня может собирать доказанные 25×/200× выигрыши там, где realloc
редок.

Перед следующей performance-волной стоит исправить hardened oracle и качество
метрик: текущая документация местами уверенно доказывает больше, чем реально
проверяют тест и counters.
