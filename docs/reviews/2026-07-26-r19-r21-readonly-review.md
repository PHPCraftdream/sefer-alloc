# Read-only review: Rounds 19–21

**Дата:** 2026-07-26  
**Предыдущий reviewed snapshot:** `3b8fdc0`  
**Текущий HEAD:** `b6af12dc5df511832989e308c8f81a46ed378109`  
**Рассмотренный diff:** `3b8fdc0..b6af12d`, 16 коммитов  
**Режим:** только чтение Git history/diff и файлов. Никакие сборки, тесты,
Miri/Kani, скрипты и бенчмарки в рамках этого ревью не запускались.
Единственная запись — этот отчёт.

Примечание по истории: предыдущий `3b8fdc0` и нынешний `cf82135` имеют общего
родителя `60633e3` и представляют две версии R18-8; нынешняя версия дополнена
review/plan/checkpoint файлами. Поэтому диапазон включает `cf82135` как
заменивший старый snapshot commit, а не как новое runtime-изменение.

## Короткий вердикт

**Нет, новые Rounds 19–21 практически не ускорили исполняемый allocator.**

- Plain `production` hot path по существу не изменился.
- R19 исправил реальный hardened/UAF-shaped defensive gap, улучшил тестовые
  oracles и документацию. Это correctness/hardening, не speedup.
- R20 измерил уже существующие feature combinations. C4 не ускорил realloc;
  он лишь подтвердил снижение commit charge примерно с 50.5 до 23.9 MiB.
- R21 добавил measurement harness и observation-only counters под
  `alloc-stats`, получил 0 hits и правильно не реализовал OPT-H.
- Ни один новый механизм ускорения не включён в `production`, и feature
  bundle `production` не изменён.

Это была полезная исследовательская волна: она закрыла несколько ложных
гипотез и не позволила внедрить узкий механизм без victim. Но её нельзя
описывать как волну ускорения кода.

Главный новый вывод этого ревью: неудача OPT-H глубже, чем формулировка
“текущий harness не реализует victim”. Текущая class/alignment модель
**математически не позволяет одному неизменному pointer пройти всю medium
ladder in-place**. Для радикального результата нужно менять representation
medium allocations/deallocation routing, а не строить третий похожий probe.

## Что вошло в новые волны

### Round 19

| Задача | Итог | Runtime speed |
|---|---|---|
| R19-1 `46ea2db` | hardened Large mismatched-layout free больше не освобождает segment при несовпадающем size | Нет; дополнительная защита только под hardened |
| R19-2…R19-6 | исправления stale/противоречивых отчётов, CHANGELOG Round 18 | Нет |
| R19-7 `a302d16` | panic watchdog thread теперь проваливает test | Нет |
| R19-8 `e7dbe16` | четыре копии promotion cfg сведены к macro | Поведение не изменено |
| R19-9 `4ca952d` | doc literal + tripwire test | Нет |

### Round 20

| Задача | Итог | Runtime speed |
|---|---|---|
| R20-1 | stale docs исправлены | Нет |
| R20-2 | C4: reserved capacity не уменьшает promotion memcpy; commit charge ниже | Измерение старого кода, realloc speedup не найден |
| R20-3 | design OPT-H | Design-only |
| R20-4 | mimalloc Callgrind arm признан feasible | Исследование, arm ещё не реализован |

### Round 21

| Задача | Итог | Runtime speed |
|---|---|---|
| R21-1 | single-hot-buffer harness | Bench-only |
| R21-2 | OPT-H diagnostic counters, 0/320 и 0/20 hits, NO-GO | Plain production block компилируется прочь; behavior unchanged |

Общий diff — около 6,714 добавленных строк и 111 удалённых. Основная масса —
отчёты, raw logs, designs, tests и комментарии. Большой diff здесь не
означает большое ускорение.

## Мы действительно ускорили код?

### Plain `production`

Нет подтверждённого speedup:

- R19-1 выполняется только в hardened/promotion-reachable defensive path.
- R19-8 — cfg refactor без изменения тел.
- R21-2 помещает весь новый precondition block под `alloc-stats`, которого
  нет в `production`.
- R20 не меняет `src`.
- Cargo diff добавляет только два example targets.

R21-2 добавляет две process-wide atomic statics и два публичных hidden
accessor, скомпилированных всегда, поэтому буквальное утверждение “вообще
никакого footprint” сильнее кода. Но per-operation production hot-path cost
действительно отсутствует по cfg-структуре.

### Hardened + medium

Код стал безопаснее, но немного дороже на suspicious Large-routing path:
после kind read выполняется дополнительное чтение logical `large_size`.
Это правильный обмен для hardened, не performance regression, которую стоит
оптимизировать.

### Opt-in Large/medium profiles

R20-2 подтвердил существующий memory win:

- C1 commit ≈50.5 MiB;
- C4 (`exact-span-large + large-reserved-capacity`) commit ≈23.9 MiB;
- RSS практически одинаков: около 9.6 MiB;
- realloc difference C1 vs C4 статистически не разрешён: примерно 2%,
  `t=1.209`, sign 10/10.

То есть C4 уменьшает **commit charge**, а не ускоряет promotion и не
уменьшает resident RSS в этом измерении.

## Что сделано хорошо

### 1. R19 действительно закрыл основную ошибку предыдущего review

Тест `regression_hardened_large_kind_own_free` теперь использует
`dbg_owner_id_for` как liveness oracle и отдельно покрывает:

- легитимный promoted Large free;
- fabricated small-layout free в promotion-ON branch;
- сохранение регистрации segment после defensive no-op.

Это намного сильнее прежнего payload-read oracle, который мог читать уже
освобождённую память.

### 2. Watchdog failure semantics теперь корректнее

R19-7 re-panics на основном test thread, если watcher panicked, и не создаёт
double panic, когда основной поток уже unwinding. Это устраняет “зелёный test
с умершим watchdog”.

### 3. Методологические поправки R19-6 правильные

В старом R14-4 отчёте теперь явно сказано:

- исторические 1,700–2,300× нельзя выдавать за current result;
- “full round” был арифметической суммой независимо измеренных фаз;
- cache hit rate выводился proxy-методом, а не direct counter.

### 4. R20-2 не принял cross-session шум за speedup

Наивное сравнение выглядело как улучшение около 24%. Авторы заново измерили
C1 в той же сессии и сделали прямое C1/C4 pairing; эффект исчез. Это хороший
пример отказа от привлекательного, но ложного результата.

### 5. R21 остановился после нулевого Stage-1

Реальный action OPT-H не реализован после 0% hits. Это правильное решение:
не добавлять unsafe/correctness-sensitive bump mutation без доказанного
consumer.

## Findings

### P1 — R19-1 проверяет только size, но не полный `Layout`

`large_layout_consistent` в
`src/alloc_core/deferred_large/layout_consistent.rs:68-70` сравнивает только:

```rust
layout.size().max(MIN_BLOCK) == SegmentHeader::large_size_at(base)
```

R19-1 повторно использует этот helper в hardened own-thread branch
(`heap_core_free.rs:410-412`). Cross-thread path использует тот же helper.

Но `GlobalAlloc::dealloc` требует исходный `Layout`, то есть совпадение не
только size, но и alignment. Header уже хранит `large_align`
(`segment_header.rs:429`), а fresh/cache-hit Large construction записывает
актуальный align.

Следовательно, fabricated layout с правильным size и неправильным align:

- проходит `large_layout_consistent`;
- под hardened считается legitimate;
- реально освобождает текущий Large segment.

Это та же defensive-no-op дыра, только более узкая. R19 закрыл пример
`64 B != 2 MiB`, но не весь контракт.

Рекомендация:

1. заменить helper на проверку `(clamped_size, align)`;
2. добавить field-specific `large_align_at`;
3. обновить оба call site — own-thread hardened и cross-thread mitigation;
4. добавить branch-A/branch-B тесты с одинаковым size и разным alignment;
5. отдельно проверить cache-hit reuse, где header переписывается под нового
   occupant.

Это не safe-API soundness bug: вход уже нарушает unsafe allocator contract.
Но это незакрытая часть обещанной hardened-защиты и post-reuse mitigation.

### P1 — OPT-H не просто “не нашёл victim”: его medium-ladder цель несовместима с текущим alignment invariant

Для in-place grow pointer не меняется. После каждого grow dealloc
классифицирует блок по **новому** layout и кладёт его в free list нового
class. Поэтому offset должен быть кратен размеру каждого class, через который
pointer прошёл.

Medium sizes в единицах 64 KiB:

```text
256 / 320 / 384 / 512 / 768 / 1024 KiB
  4 /   5 /   6 /   8 /  12 /   16
```

Их НОК:

```text
LCM(4, 5, 6, 8, 12, 16) = 240 units = 15 MiB
```

Один неизменный ненулевой offset должен быть кратен 15 MiB, чтобы пройти всю
ladder и оставаться валидным block origin для каждого class. Но segment равен
4 MiB. Такой offset невозможен.

Даже две первые ступени требуют offset, кратный LCM(256, 320) = 1.25 MiB.
Нынешний естественный first carve находится на 256 KiB, поэтому R21 получает
0 hits. Placement bias мог бы сделать **один** переход возможным, но следующий
384 KiB class снова потребует другую совместимость. Positive unit test
768→1024 KiB на offset 3 MiB доказывает лишь существование одной удачной
ступени, не viable grow-through-ladder path.

Следствия:

- R21-1 — именно single-hot-buffer victim по workload shape; проблема не в
  отсутствии victim, а в representation/alignment;
- третий harness на sub-256-KiB classes проверит другой диапазон и не решит
  доказанный medium promotion cliff;
- OPT-H в текущей форме нельзя считать всё ещё главным active lever;
- менять только carve placement недостаточно для последовательного роста.

Чтобы получить радикальный выигрыш, нужно ослабить сам invariant
“dealloc нового layout обязан трактовать pointer как обычный block нового
class”. Это требует нового metadata/origin/run representation.

### P2 — `OPT_H_HITS` документирован как шесть preconditions, но шестой не проверяется

В `alloc_core.rs:1974`:

```rust
let lazy_commit_frontier_ok = true;
```

При этом statics, accessors, tests и отчёт многократно говорят “ALL SIX
preconditions hold”. Сам код честно объясняет, что frontier может не покрывать
grown tail и Stage-1 его переоценивает.

Нулевой результат не искажён: overcount не способен превратить реальные hits
в ноль, а alignment уже блокирует все измеренные случаи. Но название и
контракт counters неверны. Если их оставлять для будущего эксперимента, нужно:

- называть результат “geometry candidates”, а не готовыми OPT-H hits;
- либо реально проверить/смоделировать commit frontier;
- либо удалить шестое условие из заявленного Stage-1 decision contract.

### P2 — после NO-GO в production source оставлено 149 строк диагностического механизма

R21-2 не меняет plain-production hot path, но оставляет:

- две глобальные atomics;
- два hidden public accessors;
- большой cfg-block внутри важного realloc function;
- 298-line test;
- 426-line permanent harness;
- сотни строк отчётной поддержки.

При 0/320 и 0/20 hits и структурной alignment-проблеме это превращается в
постоянный maintenance surface отвергнутой гипотезы.

Если sub-256 experiment не запланирован немедленно, лучше:

- сохранить результаты и design в docs;
- удалить `OPT_H_*` counters/accessors и permanent source branch;
- оставить минимальный standalone diagnostic probe только в bench/example
  при следующем реальном исследовании.

Rejected/NO-GO идеи не должны бесконечно накапливаться в shipping source как
“на всякий случай” diagnostics.

### P2 — R20-2 снова использует неоднозначные memory и full-round формулировки

1. “resident commit roughly half” смешивает два разных показателя. По таблице
   commit уменьшается, RSS остаётся практически прежним. Следует писать
   “process commit charge”, не “resident commit”.
2. Full-round 2.70× снова является суммой independently-paired phase means.
   Новый отчёт ссылается на dual-axis convention, но не повторяет явную
   caveat, которую R19-6 только что добавил в старый отчёт. Для нового reader
   это снова выглядит как собственная paired statistic.
3. Cache hit остаётся `segments_reserved_total` proxy, хотя R19-6 уже
   сформулировал direct-counter replacement.

Эти оговорки не меняют C4 NULL verdict: прямой C1/C4 realloc comparison
сделан корректно. Они влияют на вторичные claims.

### P2 — важная feature combination всё ещё не является полноценной CI matrix entry

Commit R19-1 сообщает:

- `hardened + medium-classes` был ключевой комбинацией дефекта;
- отдельные tests для неё запускались;
- clippy этой комбинации показывал 11 pre-existing dead-code errors;
- комбинация не входит в стандартную CI feature matrix.

Поздние коммиты запускают её tests, но не показывают отдельный clean
`clippy --features "hardened medium-classes" -D warnings`.

После реального UAF-shaped hardened gap эта комбинация перестала быть
экзотической. Её нужно либо:

- добавить в feature-powerset/CI allowlisted matrix;
- либо официально объявить unsupported и не обещать hardened behavior для
  неё.

Промежуточное состояние “важный behavior поддерживаем, но warning-clean build
не является gate” создаёт следующую cfg-регрессию.

### P2 — найденный flaky promotion test не попал в устойчивый issue/open index

R19-1 commit message сообщает, что
`canary_survives_promotion_and_free_leaves_no_leak` падал примерно в одном из
трёх повторов и был flaky ещё до fix. В текущем `OPEN_ITEMS.md` этого нет,
поиск по committed docs почти не находит follow-up.

Сам тест использует counters и allocator state, которые могут зависеть от
параллельного test execution/registry lifecycle. Даже если падение не связано
с R19, известный flaky correctness canary нельзя оставлять только в commit
message.

Нужен project-wide defects/flakes index, отдельный от perf-only
`OPEN_ITEMS.md`, с owner/status/reproduction evidence.

### P3 — stale-literal tripwire дублирует именно те literals, от которых проект хотел уйти

R19-9 сначала правильно формулирует размер через реальные constants, но затем:

- сохраняет три resolved number в source comment;
- копирует те же три числа в `EXPECTED_BYTES`;
- требует при легитимном изменении constants вручную обновить и test, и prose.

Это ловит drift, но создаёт намеренно красный тест на любое корректное
изменение конфигурации и удваивает maintenance. Более устойчивые варианты:

- вообще убрать snapshot numbers и оставить formula;
- генерировать doc table из machine-readable source;
- проверять только semantic budget/upper bound, если размер является
  реальным contract.

То же касается ручного `tests/*.rs (221 files)` в ARCHITECTURE: точное число
не помогает понять coverage, но создаёт постоянный churn.

### P3 — promotion cfg canonicalized только частично

Macro объединяет четыре положительных site, что полезно. Но import union и
hardened negation остаются hand-written. Комментарии честно признают это.

Существующий `dbg_promotion_compiled` canary снижает риск, однако долгосрочное
решение — build-time cfg alias или feature-matrix compile test, а не ещё
больше объясняющего prose рядом с hot code.

## Что ещё можно сильно ускорить

### 1. P0 — MediumExtent/PageRun с независимым origin invariant

Это теперь наиболее обоснованный радикальный путь.

Требования:

- pointer origin не обязан быть кратен каждому последующему size class;
- logical size/capacity хранится в run/extent metadata;
- dealloc/realloc маршрутизируются по extent metadata, а не только по
  `class_for(layout)` и offset divisibility;
- соседний VA резервируется с growth headroom;
- pages commitятся lazy по мере роста;
- свободный extent возвращается в size-aware cache/pool;
- fallback остаётся существующий medium copy/promotion.

Возможные формы:

1. новый `SegmentKind::MediumRun`;
2. несколько page extents внутри более крупной arena;
3. dedicated reserved VA extent для growth-capable medium object;
4. небольшой origin/run sidecar, позволяющий free по stored origin class.

Это сложнее OPT-H, зато устраняет саму причину 256 KiB copy, а не ловит редкую
геометрическую случайность. Gate должен измерять:

- bytes copied и move legs;
- pointer stability;
- alloc/free/realloc wall-clock;
- commit/RSS/reserved VA;
- segment/table pressure;
- xthread free и hardened stale-pointer behavior.

### 2. P0 measurement — реализовать mimalloc Ir arm сейчас

R20-4 уже доказал feasibility и описал маленький scope:

- mimalloc benches в том же `perf_gate_iai.rs`;
- отдельный mimalloc bootstrap proxy;
- arm-aware bootstrap subtraction в `scripts/iai.mjs`;
- одинаковые operation counts/workloads;
- derived Sefer/mimalloc Ir ratio.

Это не ускорение само по себе, но самый дешёвый способ решить, существует ли
ещё 1.5–2.5× instruction-level reserve в cold 16 B path. Если mimalloc Ir
существенно ниже — следующий source optimization получает точную цель. Если
Ir близок — прекращается многолетний микротюнинг неправильного уровня.

### 3. P1 product — allocation-heavy medium profile

Уже измеренный `medium-classes + large-cache-extended` даёт огромные
alloc/free выигрыши и плох только при частом medium realloc.

Вместо универсального default:

- именованный compile-time workload profile;
- явный RSS/commit budget;
- break-even curve по realloc frequency;
- реальный consumer benchmark.

Для workloads “allocate medium object once, use, free” это уже существующее
сильное ускорение, ожидающее упаковки/adoption.

### 4. P1 memory profile — exact-span + reserved capacity

C4 не ускоряет первый promotion, но commit charge примерно вдвое меньше.
Это стоит оформить как memory-oriented policy и отдельно измерить:

- address-space reservation;
- commit peak;
- RSS peak;
- later grows, где reserved capacity действительно может помочь;
- cache turnover.

Не называть memory win speedup.

### 5. P2 conditional — batched deferred reclaim

Сначала нужен Stage-1 counter “segments finalized per drain sweep”. Если
multi-segment finalization редок, не реализовывать sub-design B. Sub-design A
может быть маленьким выигрышем, но по текущим данным не выглядит радикальным.

### 6. P2 conditional — page-run 1.25–2 MiB

Возвращаться только при реальном MAX_SEGMENTS/OS-reservation-bound workload.
Новый MediumExtent может частично поглотить эту задачу, поэтому сначала
следует выбрать общую representation, а не развивать два конкурирующих run
слоя.

## Что улучшить в коде

1. Сделать Large layout consistency полной: size + align.
2. После NO-GO убрать OPT-H diagnostics из shipping source либо дать им
   ближайший конкретный experiment deadline.
3. Переименовать OPT-H counters в geometry candidates, если они остаются.
4. Не держать 100+ строк исторического комментария внутри hot function;
   invariant рядом с кодом, chronology — в design doc.
5. Свести medium promotion/hardened cfg к одному проверяемому alias.
6. Перестать использовать pointer alignment нового class как единственную
   возможность dealloc grown medium block; проектировать explicit origin/run
   metadata.
7. Для debug counters дать scoped/resettable snapshot API или per-allocator
   counters, чтобы tests не зависели от process-global history.

## Что улучшить в проекте

1. Добавить `hardened + medium-classes` в поддерживаемую CI matrix.
2. Завести durable correctness/flaky index, не ограниченный `docs/perf`.
3. Добавить Rounds 19–21 в CHANGELOG: сейчас Unreleased подробно описывает
   Round 18, но новые волны присутствуют только через отдельные docs и commit
   history.
4. После каждого round summary разделять:
   - runtime optimization landed;
   - correctness fix;
   - measurement;
   - design-only;
   - rejected/NO-GO;
   - docs/process.
5. Не превращать каждую найденную цифру в permanent source/test literal.
6. Для full-round всегда измерять собственный `elapsed_ns` paired pass, если
   число используется в выводе.
7. Для cache results использовать direct hit/miss counters.
8. Проверять memory terminology: reserve, commit charge и RSS — разные оси.
9. Ограничить рост diagnostic API. После завершения исследования удалять
   probes, которые не являются постоянными regression gates.
10. Отчёты должны фиксировать не только “что делать дальше”, но и “что
    удалить после NO-GO”.

## Рекомендуемый следующий этап

1. **Correctness P1:** дополнить `large_layout_consistent` alignment check и
   оба routing test family.
2. **Project P1:** добавить hardened+medium CI gate и зафиксировать flaky
   promotion test.
3. **Measurement P0:** реализовать mimalloc Ir arm — дизайн уже готов.
4. **Cleanup:** удалить или заморозить OPT-H instrumentation после признания,
   что natural medium ladder несовместима с invariant.
5. **Radical design:** спроектировать MediumExtent/PageRun representation,
   которая хранит origin/capacity и допускает page-granular in-place grow.
6. **Product:** отдельно оценить allocation-heavy medium profile; не ждать
   универсального medium default, чтобы использовать уже доказанные
   alloc/free выигрыши.

## Итог

Rounds 19–21 сделали проект **надёжнее и честнее**, но не быстрее для plain
production. R19 исправил реальные defensive gaps. R20 доказал, что
destination reserved capacity не отменяет first promotion copy. R21
правильно получил NO-GO до реализации.

До предела ускорения далеко, но следующий скачок требует архитектурного
шага. OPT-H показал фундаментальную границу текущей модели: один pointer не
может быть корректно выровнен под всю medium ladder внутри 4 MiB segment.
Нужна representation с явным run/origin/capacity, либо отдельный
growth-oriented allocation profile.

Самый дешёвый следующий шаг — mimalloc Ir arm. Самый сильный потенциальный
шаг — MediumExtent с lazy-commit growth, устраняющий первый 256 KiB copy.
