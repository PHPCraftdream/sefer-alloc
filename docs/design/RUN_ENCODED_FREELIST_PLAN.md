# X-arc PERF-3 — run-encoded freelist for the recycle path: план реализации (Ф0–Ф6)

**Статус:** план предложён (2026-07-07), реализация не начата. Задача TaskList
#207 «PERF-3: run-encoded freelist for the recycle path».
**Предшественники:**
- `docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md` (honest-reject; установил,
  что выигрыш лежит в «дешевле за refill», а не «реже за refill» — этот план
  атакует именно первую форму).
- Э8 (task #162) `flush_class` / `flush_run` (same-segment-run batching) — уже
  детектит runs, но всё ещё пишет per-block `next` pointers; этот план убирает
  эти записи для contiguous runs.
- X7 `X7_GENERATIONAL_RING_PLAN.md` — структурный шаблон (key-insight → §2
  fixed decisions → phases → risks → readiness), которому следует этот док.

**Это DESIGN-DOC-ONLY задача.** Никакого кода в `src/`, никаких тестовых файлов,
никаких коммитов. Файл оставлен untracked для ревью человеком; §6 — ПРЕДЛОЖЕНИЕ
фаз для превращения в TaskList-записи (не создавать их здесь).

---

## 1. Что закрываем — и что НЕ закрываем (главный инсайт)

### Механизм, который атакуем (CONFIRMED-by-reading код)

На recycle-пути (magazine flush → segment freelist → later refill drain) сегодня
существует **serial dependent-load pointer chase**. Конкретно:

- **Сторона flush** (`alloc_core.rs:2144` `flush_class` → `flush_run`,
  Э8): для каждого принятого блока выполняется `Node::write_next(block_nn,
  next_ptr)` (`node.rs:74`) — запись первого слова тела блока, строящая LIFO-
  цепочку внутри run. Э8 уже ** hoisted ** `set_head`/`bin_table`/`alloc_bitmap`
  views и single `set_head` per run, но **per-block body write остался**.
- **Сторона drain** (`alloc_core.rs:2690` `drain_freelist_batch`): каждый шаг
  цикла делает `let next = Node::read_next(block_nn)` (`node.rs:94`) — load,
  который **зависит от предыдущего load** (адрес `next` живёт в теле блока,
  только что прочитанном), и при типичном stride блоков это **один cold cache-
  line miss на блок**. Комментарий в коде (`alloc_core.rs:2655`) сам называет
  это «the dependent load that walks the intrusive chain ... mimalloc pays this
  too; there is no way to hoist it» — при **linked-list** представлении это
  правда. Этот план вводит **другое представление**, где hoist возможен.

### Что закрываем

Если `flush_run` обнаруживает, что принятые блоки образуют **contiguous run**
(один класс, смежные offsets `off, off+block_size, off+2*block_size, …`),
вместо per-block `write_next` + LIFO-сшивания, мы кладём **compact run
descriptor** `(start_offset, count)` на маленький per-class **run-stack** в
segment metadata. Позже `drain_freelist_batch` реконструирует адреса **stride
arithmetic**-ом (`start_offset + i*block_size`) — **ноль body writes на flush,
ноль dependent loads на drain**. Это в точности «cheaper per-block work на hot
recycle path» — форма, которую PERF-2 идентифицировал как выигрышную.

PERF-2-вывод («mimalloc's advantage is a structurally cheaper refill, not a
deeper magazine») назвал это семейство атак напрямую; этот план — конкретная
реализация этой формы для recycle-path.

### Что НЕ закрываем (честный scope, в стиле X7 §1)

1. **Isolated/scattered single-block frees не выигрывают.** Run descriptor
   помогает только когда блоки **genuinely contiguous**. Шаблон освобождения
   должен реально порождать runs. Холодный storm (1024 distinct allocs → 1024
   frees в обратном порядке, как `global_alloc` bench) — **порождает** long
   runs (Э8 уже доказал, что flush batch ~100% same-segment; contiguous — это
   более сильное условие, но LIFO-free паттерн даёт именно его). Разреженный
   freeing (random-order, multi-segment) — **деградирует в runs длины 1**, что
   fallback-нет на classic linked freelist (см. §2.6) — ноль regression, ноль
   win. **Критерий go/no-go (см. §5) явно требует, чтобы бенчи, где runs не
   порождаются, НЕ регрессировали** — это условие отсутствия regression, а не
   условие win.
2. **Virgin carve path уже оптимален.** Э1/W4 (`carve_batch`) уже пишет блоки
   напрямую в `out` без BinTable round-trip; этот план **не трогает** carve
   path. Recycle-path (flush→freelist→drain) — единственная мишень.
3. **Cross-thread remote free path не ускоряется.** Remote free приходит по
   одному блоку за раз (`reclaim_offset`/`_checked`, `alloc_core.rs:768/883`) —
   никогда не образует contiguous run; он **всегда** идёт classic linked-list
   путём (см. §2.4). Это структурно, не артефакт дизайна.
4. **Ни один из 11 iai-бенчей не обязан улучшиться.** Часть бенчей (churn,
   large_alloc_free_cycle) не проходит recycle-path достаточно, чтобы
   почувствовать run-encoding. Условие win — на **cold/recycle** бенчах;
   условие no-regression — на всех 11.

---

## 2. Зафиксированные дизайн-решения (Ф0)

### 2.1 Run-descriptor encoding: 8 байт, `(start_off: u32, count: u16, _spare: u16)`

- `start_off: u32` — segment-relative offset первого блока run. `off < SEGMENT`
  (= 4 MiB), и `u32::MAX` зарезервирован как sentinel (как `FREE_LIST_NULL`),
  так что реальный диапазон — `u32` без верхнего значения; хватает с огромным
  запасом.
- `count: u16` — число блоков в run. `count >= 1`. Cap: realistic flush batch =
  `FLUSH_N = TCACHE_CAP/2 = 8` (текущий default); даже если TCACHE_CAP поднимут
  (PERF-2 отверг, но防御но), `u16` покрывает до 65535 блоков на run.
- `_spare: u16` — padding до 8 байт (8-byte aligned для metadata region; та же
  дисциплина, что `BinTable` heads u32 / ring slots u32 / gen-table bytes). Spare
  зарезервирован под будущее (напр. generation-stamp для run-as-a-unit под
  hardened — см. §2.5, не используется в v1).

**Layout как массив дескрипторов в metadata region** (новая per-segment
`RunStack`), см. §2.2.

### 2.2 RunStack storage: новый metadata region, 8-byte aligned, AFTER gen-table

`Layout::run_stack_off()` = `align_up(small_meta_end_pre_runstack, 8)`, где
`small_meta_end_pre_runstack` — текущее `small_meta_end()` (включая gen-table
под hardened). Это **сдвигает `small_meta_end()` вверх**, уменьшая ёмкость
segment под payload — точно так же, как X7-Ф1 сдвинул мету на ~256 KiB под gen-
table.

**Размер RunStack (sizing — ключевое решение):**

- `RUNSTACK_CAPACITY = 8` дескрипторов на class на segment. При 49 classes →
  `49 * 8 * 8 B = 3136 B ≈ 3 KiB` на segment. Это <1% от 4 MiB segment — на
  порядок дешевле gen-table X7 (~256 KiB).
- **Почему 8 (а не 4 или 16):** realistic flush batch = `FLUSH_N = 8` блоков
  (`TCACHE_CAP=16 / 2`). Типичный cold-storm даёт один long contiguous run на
  flush (covering весь batch) → **1 дескриптор**. Адверсариальный random-
  freeing может фрагментировать batch в череду runs-of-1; при `RUNSTACK_CAPACITY
  = 8` и 49 classes мы покрываем до 8 одновременных runs на class — это
  консервативно с огромным запасом (одновременно живущих runs на один class в
  одном segment в реальности 1–2). **16 не оправдано** (двойная мета ради
  нереалистичного сценария); **4 рискованно** (легко переполнить при
  interleaved multi-batch flush).
- **Overflow policy (§2.6 fallback):** если RunStack для class полон (8 runs
  уже записаны), **новый contiguous run fallback-нет на classic linked-list
  path** — `flush_run` пишет `write_next` per-block как сегодня. Это
  документированный, безопасный degradation; не panic, не abort.

**Важная инварианта:** RunStack — это **HINT**, не source of truth. Bitmap
(§2.3) — sole ground truth для free/allocated. RunStack говорит «адрес block N
восстановим как `start_off + i*block_size`», но **выдача блока всё равно
проходит через bitmap check** (см. §2.3 для double-free; см. §2.7 для bitmap-
RMW-merge возможности).

### 2.3 M2 (double-free bitmap oracle) — SOLE ground truth; run = reconstruction HINT

**Это единственное safety-critical решение в доке; относится с X7-generational-
table-level rigor.**

**Инварианта (load-bearing):** `AllocBitmap` (`alloc_bitmap.rs`) остаётся
**единственным** источником правды для free/allocated состояния блока.
Run-descriptor — это **fast-path hint для реконструкции адресов**, НИКОГДА не
замена bitmap check.

**Как ловится double-free блока, входящего в active run:**

Два под-случая, оба разрешены явно:

**(a) Draining run ПЕРЕ-валидирует каждый reconstructed address против bitmap.**
`drain_freelist_batch`, когда вытягивает блоки из run descriptor, для каждого
реконструированного `off = start_off + i*block_size` делает:

```
if !bm.is_free(off) { skip / continue; }   // блок уже allocated — не выдаём
out[k] = reconstructed_ptr;
bm.mark_alloc(off);                          // transitions FREE → ALLOC
```

Это **identical** существующему per-block `mark_alloc` в `drain_freelist_batch`
(`alloc_core.rs:2726`) — тот же RMW, тот же invariant. Run-descriptor **не
обходит** bitmap; он только **заменяет pointer-chase способом получения адреса**
(stride arithmetic вместо `read_next`). Двойной free блока в active run: второй
free пишет ring-entry → reclaim → `is_free(off) == true` (блок ещё FREE в
bitmap, потому что run-drain ещё не дошёл до него) → reclaim ставит его на
**classic linked freelist** через `write_next` (см. §2.4 — cross-thread всегда
linked-list). Позже drain: linked-list drained first OR run drained first:

  - **run drained first:** `mark_alloc(off)` ставит ALLOC; linked-list head всё
    ещё указывает на этот off; позже linked-list drain делает `is_free(off) ==
    false` → **skip** (существующий guard в M2-пути). Блок выдан один раз.
  - **linked-list drained first:** `mark_alloc(off)` → ALLOC; run drain делает
    `is_free(off) == false` → **skip**. Блок выдан один раз.

В обоих случаях **блок выдан ровно один раз**, потому что **каждый путь выдачи
проходит `is_free`/`mark_alloc` RMW**, и первая выдача атомарно переводит бит.
Run-descriptor **не создаёт новой гонки** — он лишь меняет способ получения
адреса.

**(b) Double-free ЧЕРЕЗ тот же magazine flush (не cross-thread).** Собственный
(`dealloc_small`, `alloc_core.rs:2964`) double-free блока, уже в magazine и
сейчас флашущегося: существующий guard в `flush_run` (`bm.is_free(off)` at
`alloc_core.rs:2202`) **остаётся per-block** (Э8 явно оставил его per-block,
см. док `flush_run` "the TWO guards STAY per-block"). Run-descriptor **не
обходит этот guard**: flush_run сначала проходит все блоки run с guard-ом
(`if bm.is_free(off) continue`), и только **accepted** блоки (те, что прошли
guard) формируют contiguous accepted-sub-run, который кодируется как
descriptor. Т.е. **descriptor кодирует только ACCEPTED блоки** — rejected
(double-free, decommit-stale) блоки в него не попадают.

**Почему НЕ нужно per-block re-check ПОСЛЕ reconstruction в случае (b):**
потому что guard уже выполнен **до** кодирования (на flush-стороне), и между
кодированием и drain нет пути, который бы перевёл ACCEPTED блок обратно в
FREE без прохождения через `drain` (который и есть этот самый drain). НО мы всё
равно оставляем `is_free` check в drain (random (a)-корректность для cross-
thread interleave), что является defense-in-depth — бесплатно, потому что
`mark_alloc` всё равно нужен.

### 2.4 Cross-thread remote free path — ВСЕГДА classic linked-list (structural)

`reclaim_offset` (`alloc_core.rs:883`) и `reclaim_offset_checked`
(`alloc_core.rs:768`) линкуют reclaimed блок на **pointer-based freelist** через
`Node::write_next` (`alloc_core.rs:866`, `:971`). Это **single-block операция**
(один ring-entry → один блок), и cross-thread free **никогда не образует
contiguous run** (каждый remote free — отдельное событие от remote thread).

**Дизайн-решение: remote free path НЕ МЕНЯЕТСЯ.** Он всегда идёт classic linked-
list путём, независимо от того, есть ли active runs для этого class. Это
**simplest correct option (a)** из предложенных в задаче.

**Доказательство того, что блок НЕ может быть ОДНОВРЕМЕННО remote-freed И в
active run descriptor (X7-style «prove it's unreachable»):**

Инварианта: блок, входящий в active run descriptor, имеет `bm.is_free(off) ==
true` (он был принят `flush_run` только если `!is_free` на момент flush, и
затем `mark_free` перевёл его в FREE — см. `alloc_core.rs:2219`). Блок в
active run — **FREE в bitmap**.

Remote reclaim в `reclaim_offset_checked` делает guard `if bm.is_free(off) {
return false; }` (`alloc_core.rs:817`) **ДО** `write_next` — то есть reclaim
**отказывается** линковать блок, который уже FREE. Значит, если блок В active
run descriptor (FREE в bitmap), remote reclaim **drop-нет** ring-entry (return
false), НЕ линкуя его на linked-list. **Блок не может одновременно быть в
active run и быть linked-list-reclaimed** — reclaim сам от него отказывается.

Это **structural unreachability**, доказанная из существующих guards — никаких
новых инвариант не нужно. Run-descriptor и linked-list **не пересекаются по
блокам**: блок либо в run (FREE, ждёт drain), либо в linked-list (FREE, ждёт
drain), но не в обоих — потому что оба пути drain делают `mark_alloc` (FREE →
ALLOC), и второй drain увидит ALLOC и skip-net. Cross-thread reclaim не может
перевести FREE-блок в linked-list, потому что его собственный guard
`is_free==true → return false` отказывается.

**Resolved:** option (a) — simplest, correct, structural. Никаких run-
invalidation/split на remote free не нужно.

### 2.5 Decommit-reset interaction — decommit MUST clear RunStack (как X7 §2.2)

`decommit_empty_segment` (`alloc_core.rs:1202`) сегодня: decommits payload,
resets `bump = small_meta_end`, NULLs every class head, re-marks page-map,
zeros bitmap, sets decommitted flag.

**Проблема:** если active run descriptors ссылаются на offsets в payload, а
payload decommitt-нут и затем segment re-carve-ится (bump снова с начала),
**stale run descriptor указал бы на re-carved регион** — блок, который теперь
принадлежит новому владельцу, был бы невалидно восстановлен как free.

**Дизайн-решение (параллельно X7 §2.2 про gen-table continuity):** RunStack —
это **metadata** (живёт в metadata region, НЕ payload). Decommit возвращает ОС
только payload pages `[small_meta_end, SEGMENT)` (`alloc_core.rs:1207`); метаданные
(header/page-map/bin-table/bitmap/ring/gen-table/**runstack**) **остаются
committed**. Поэтому **RunStack физически переживает decommit** (как gen-table).

Но **логически** RunStack ДОЛЖЕН быть очищен при decommit, потому что:
- `bump` сбрасывается на `small_meta_end` → все offsets в payload теперь
  `>= bump` → stale-free guard (`off >= bump`) в `flush_run`/`reclaim` всё
  равно reject-net любые блоки из старого run. **Но** `drain_freelist_batch` на
  этом segment после decommit найдёт **stale descriptors** и попытается
  реконструировать `start_off + i*block_size` — а `start_off` теперь `>= bump`,
  и reconstructed ptr укажет на decommitted/unmapped страницу.

**Fix:** `decommit_empty_segment` **дополнительно** зануляет RunStack для
каждого class (49 classes × `RUNSTACK_CAPACITY` descriptors × 8 B = 3136 B
занулить). Это дёшево (один memset), и **структурно идентично** тому, как
decommit уже NULLs каждый class head (`alloc_core.rs:1218-1220`). После
decommit RunStack пуст → drain на этом segment видит пустой run-stack →
fallback на (тоже пустой) linked-list → return 0. Re-carve после recommit
стартует с чистого segment, как сегодня.

**Доказательство эквивалентности:** сегодняшний decommit NULLs heads → linked-
list drain видит `head == FREE_LIST_NULL` → return 0. После fix: decommit NULLs
heads AND clears RunStack → run-drain видит пустой stack → return 0; linked-
list drain видит NULL → return 0. **End-state byte-identical** (оба пути
возвращают 0; ничего не выдано). Recycle после re-carve работает как сегодня.

### 2.6 Fallback to classic linked freelist (overflow / non-contiguous)

`flush_run` сегодня (`alloc_core.rs:2193-2226`) обрабатывает run как LIFO-chain
build. Новый код:

1. **Detect contiguous accepted sub-run:** внутри существующего loop-а over
   `run`, после guards (`is_free`, `off >= bump`), **collect accepted offsets**.
   Затем scan collected offsets на contiguous runs (adjacent `off + block_size`).
   Это O(accepted) — дёшево.
2. **Для каждого contiguous accepted sub-run длины ≥ 2:** push `(start_off,
   count)` на RunStack (если capacity позволяет).
3. **Overflow OR sub-run длины 1:** classic linked-list path — `write_next` +
   link, как сегодня.

**Mixed flush:** один `flush_run` может породить **смесь** — часть блоков в
run-descriptors, часть в linked-list (singletons или overflow). Это корректно:
оба представления сосуществуют, drain обрабатывает оба (см. §3 Ф3). Linked-list
head и RunStack — независимые структуры; `drain_freelist_batch` сначала drain-
ит RunStack (stride arithmetic, zero dependent loads), затем linked-list
(pointer chase). Порядок внутри не определён спецификацией (оба множества
disjoint по bitmap — см. §2.3).

### 2.7 Bitmap RMW merge — OPTIONAL v1.5, NOT v1

PERF-2 и задача упоминают, что contiguous runs дают **mergeable bitmap ranges**
(несколько бит в одном bitmap-byte). **В v1 мы НЕ делаем merge bitmap RMW.**
Причины:
- `mark_free(off)` per-block в flush — уже hoisted в existing flush_run; это
  trivial RMW.
- Merged `mark_free_range(start, count)` — новая операция на AllocBitmap, со
  своими invariants (alignment, byte-boundary crossing). Это отдельная
  optimization, рискованная для v1.
- Win от run-encoding — в **elimination of pointer-chase**, не в bitmap RMW.
  Bitmap RMW merge — второй-order win; оставить на v1.5/follow-up.

Документируем как **future work** в §5.

### 2.8 Gating: OWN cfg feature `alloc-runfreelist` (experimental в v1)

В стиле X7 (который gating-нул gen-table под `hardened`), этот feature gating-
ится под **собственный cfg** `alloc-runfreelist`, initially **off по default**.
Причины:
- Это **production-path performance change** (не safety, как X7). Risk: layout
  shift (RunStack занимает payload space), new metadata region, new codepaths.
- X7 был hardened-only (safety feature, opt-in). Этот — perf feature; gating под
  собственный cfg даёт **byte-identical default build** (как X7 под non-
  hardened).
- Production (`production = alloc-global + alloc-xthread + alloc-decommit +
  fastbin`) **не включает** `alloc-runfreelist` в v1. Только после Ф6 go-decision
  (§5) — если win подтверждён — feature добавляется в `production` (отдельный
  коммит, отдельная judge-проверка).

**`Cargo.toml` add:** `alloc-runfreelist = ["alloc-core"]` (под feature,
включающий metadata region). Все RunStack-touching codepaths — под `#[cfg(feature
= "alloc-runfreelist")]`; non-feature build — **byte-identical** pre-PERF-3.

### 2.9 X7 orthogonality — cleanly independent, NO coordination needed

X7 (gen-table, hardened-only) и этот feature (RunStack, alloc-runfreelist) —
**ортогональны**:

- **Разные metadata regions:** gen-table — после remote-ring (`Layout::
  gen_table_off()`, `segment_header.rs:791`); RunStack — после `small_meta_end()`
  (т.е. после gen-table, если hardened). **Не пересекаются по адресам.**
- **Разные cfg:** gen-table под `hardened`; RunStack под `alloc-runfreelist`.
  Возможны все 4 комбинации; layout assertions (как X7-Ф1 `const _: () = assert!
  (Layout::small_meta_end() + PAGE <= SEGMENT)`) — **под каждой комбинацией**.
- **Разные paths:** gen-table — remote-free staleness guard (hardened cross-
  thread); RunStack — own-thread flush/drain recycle path. Не делят hot-path.
- **Spare в run-descriptor** (`_spare: u16`) зарезервирован под будущий
  «run-as-unit generation stamp» (один gen-byte на run вместо per-block) — это
  v2 idea, **не используется в v1**, но spare гарантирует forward-compat.

**Resolved:** X7 и PERF-3 — cleanly orthogonal. Координации не нужно. Layout-
assert под всеми 4 комбинациями cfg — единственное требование (Ф1).

---

## 3. Фазы (коммит после каждой, zero-trust + судья)

Каждая фаза: independently committable + testable. Между фазами — `cargo test`
(+ miri/loom где применимо), green, commit (CLAUDE.md санкционирует phase-
boundary commits). После каждой фазы — ZERO-TRUST review (прочитать diff,
запустить тесты лично, counterfactual audit).

### Ф0 — дизайн-док
Этот файл. Без кода.

### Ф1 — RunStack storage + Layout (mirror X7-Ф1)
- **blockedBy:** Ф0 (этот док утверждён).
- **Work:**
  - Новый module `src/alloc_core/run_stack.rs` (one export: `RunStack`).
    `RunStack { entries: *mut RunDesc }` где `RunDesc { start_off: u32, count:
    u16, _spare: u16 }` (8 B). `RUNSTACK_CAPACITY = 8` per class.
    `FOOTPRINT = SMALL_CLASS_COUNT * RUNSTACK_CAPACITY * 8`.
  - `Layout::run_stack_off()` = `align_up(current_small_meta_end, 8)` — под
    `#[cfg(feature = "alloc-runfreelist")]`.
  - `Layout::small_meta_end()` — обновить: под `alloc-runfreelist` прибавить
    `RunStack::FOOTPRINT` и align-up до PAGE. Под non-feature — byte-identical.
  - `const _: () = assert!(Layout::small_meta_end() + PAGE <= SEGMENT)` под
    всеми 4 cfg-комбинациями (hardened × runfreelist).
  - `BinTable::init_in_place`-style `RunStack::init_in_place` — зануляет (все
    descriptors → `count = 0` sentinel).
  - Accessors: `RunStack::new(base)`, `push(class, start_off, count) -> bool`
    (false = overflow), `is_empty(class)`, `peek(class) -> Option<RunDesc>`,
    `pop(class) -> Option<RunDesc>`, `clear_all()` (для decommit).
- **Tests:** layout-offset assertions (как X7-Ф1); `push`/`pop`/`clear` unit
  tests; overflow → false; capacity boundary.
- **Gates:** miri (индексация/aliasing metadata); **production-судья 11/11
    байт-в-байт** — `alloc-runfreelist` off → zero diff to production (главный
    neutrality gate).
- **Commit gate:** production-судья byte-identical; layout asserts green под
  всеми cfg.

### Ф2 — detect-contiguous-run logic on flush side (pack)
- **blockedBy:** Ф1.
- **Work:**
  - В `flush_run` (`alloc_core.rs:2173`): после existing loop-а, который собирает
    accepted offsets (с guards `is_free`, `off >= bump`), добавить **contiguous-
    run detector** над accepted offsets. Для каждого contiguous sub-run длины ≥
    2 — `RunStack::push(class, start_off, count)`. Overflow или sub-run длины 1
    — classic linked-list path (существующий `write_next` loop, refactor-нутый
    чтобы обрабатывать только non-run блоки).
  - **Все под `#[cfg(feature = "alloc-runfreelist")]`;** non-feature `flush_run`
    — byte-identical pre-PERF-3.
- **Tests:**
  - Contiguous accepted offsets → 1 descriptor pushed.
  - Non-contiguous (gap) → multiple descriptors or fallback.
  - Overflow (RUNSTACK_CAPACITY exceeded) → fallback to linked-list, no panic.
  - Mixed (some contiguous, some singletons) → both representations populated.
  - **Counterfactual:** с `alloc-runfreelist` off → byte-identical end-state
    (bitmap, head, live_count) к feature-on (linked-list-only drain должен дать
    то же множество выданных блоков — см. Ф3).
- **Gates:** miri (flush path); production-судья byte-identical (off-case).
- **ВАЖНО (найдено @o46m ревью):** между Ф2 и Ф3 run-encoded блоки — честно
  «утекают» внутри segment-а: они FREE в bitmap и в RunStack, но НЕ на
  linked-list и ещё НЕ drainable (Ф3 ещё не landed). Тест «все freed блоки
  переиспользуемы» с `alloc-runfreelist` ON **закономерно упадёт** между Ф2 и
  Ф3 — это НЕ баг Ф2, это ожидаемое промежуточное состояние. **Feature-on
  vs feature-off behavioral equivalence test** (drain both, compare output
  sets) поэтому переносится на **Ф3** (только там RunStack-drain существует и
  множества блоков реально можно сравнить) — Ф2 сам по себе equivalence не
  проверяет, только "no panic, no incorrect double-encode" (bitmap/head
  state после flush идентичен non-feature build для non-run blocks).
- **Commit gate (Ф2):** flush-side unit tests (contiguous/non-contiguous/
  overflow/mixed) green; production-судья byte-identical (off-case). Полная
  equivalence — Ф3 gate.

### Ф3 — reconstruct-on-drain logic (the core win)
- **blockedBy:** Ф2.
- **Work:**
  - В `drain_freelist_batch` (`alloc_core.rs:2690`): перед существующим linked-
    list drain loop, **сначала** drain RunStack. Для каждого `RunDesc`: цикл `i
    in 0..count`, reconstruct `off = start_off + i*block_size`, `ptr =
    Node::deref(segment, off)`, **guard `is_free(off)`** (defense-in-depth, §2.3),
    `mark_alloc(off)`, `out[k] = ptr`. Когда RunStack пуст для class — fallback
    на linked-list drain (существующий код).
  - `set_head` для linked-list — только если linked-list drain что-то сделал.
  - `inc_live` — на общее `k` (run-drain + linked-list drain).
  - **Все под `#[cfg(feature = "alloc-runfreelist")]`.**
- **Tests:**
  - Run-encoded drain: zero `read_next` calls (counterfactual: mock/spy на
    `Node::read_next` — или просто behavioral: blocks выданы в stride order).
  - **M2 double-free через run:** блок в active run, second free через ring →
    reclaim отказывается (§2.4); блок выдан ровно один раз.
  - **Mixed drain:** RunStack + linked-list → все блоки выданы, no dup.
  - **Decommit-clears-runstack test:** decommit → RunStack пуст → drain return
    0.
  - counterfactual (feature-off) panic-test: если бы `is_free` guard убрали,
    double-free corrupt-нул бы (тест-демонстрация invariant-ы).
- **Gates:** miri (drain path + interleave с reclaim); loom (если модель
  покрывает flush/drain); production-судья byte-identical (off-case).
- **Эта фаза получает adversarial fl-audit перед коммитом** (X7-стиль).

### Ф4 — lifecycle seams (decommit, recycle, adopt/abandon)
- **blockedBy:** Ф3.
- **Work:**
  - **decommit-reset:** `decommit_empty_segment` (`alloc_core.rs:1202`) —
    дополнительно `RunStack::clear_all()` для segment (после зануления heads,
    перед set_decommitted). Mirror существующего `bt.set_head(c,
    FREE_LIST_NULL)` loop.
  - **recycle/release:** RunStack живёт в metadata region — умирает с segment
    release (как gen-table). Существующие guards (`contains_base`/`magic_at`)
    drop-ят записи до доступа к RunStack. Тест-подтверждение.
  - **adopt/abandon (Phase 12.4):** если segment мигрирует между heaps, RunStack
    едет с segment (как все metadata). Тест, если путь достижим.
- **Tests:**
  - Decommit → re-carve → drain на re-carved segment: stale descriptor НЕ
    указывает на новый блок (RunStack был очищен).
  - Recycle после decommit: drain на умершем base → guards reject.
- **Gates:** miri; production-судья byte-identical.

### Ф5 — honest cost/benefit ledger + go/no-go gate (mirror X7-Ф5)
- **blockedBy:** Ф4.
- **Work:**
  - **Judge 1 — iai (11 bench):** `npm run iai` под `production` (feature off —
    baseline) vs `production + alloc-runfreelist` (feature on). Записать Δ Ir
    для всех 11.
  - **Judge 2 — wall-clock criterion:** `cargo bench --features production
    --bench global_alloc` (feature off vs on) — cold-storm shape (главный
    target).
  - **Ledger entry:** `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md` — таблицы,
    win/regression per bench, mechanism analysis.
- **Go/no-go criteria (объявлены заранее):**
  - **GO (feature добавляется в `production`):** cold/recycle benches
    (`cold_alloc_free_256x16b`, `cold_alloc_free_256x64b`,
    `recycle_alloc_free_256x16b`, `recycle_alloc_free_256x64b`) показывают **≥
    5% improvement** в Ir (iai) И ≥5% в wall-clock на `global_alloc/16B` +
    `global_alloc/64B`; **zero regression** (≤ +1% Ir) на остальных 7 бенчах.
  - **NO-GO (honest-reject, как PERF-2):** если ANY из: (a) cold/recycle
    improvement < 5%, OR (b) regression > 1% на любом из остальных 7, OR (c)
    wall-clock не подтверждает iai-тренд. → doc записывает reject, feature
    остаётся off, `src/` revert-ится до pre-Ф1 (или feature оставлен как
    opt-in-only, без включения в `production`).
  - **5% threshold:** обоснован — PERF-2 показал, что mimalloc gap на 16B = 2.5×;
    5% Ir improvement — заметный, не-шумовой сигнал (PERF-2 regression была
    17–153%, так что 5% — консервативный bar для win).
- **Commit gate:** ledger published; go/no-go decision записан.

### Ф6 — (conditional on Ф5=GO) enable in `production`, publish
- **blockedBy:** Ф5 (только если GO).
- **Work:**
  - `Cargo.toml`: `production = [...existing..., "alloc-runfreelist"]`.
  - Re-run production-судья: теперь `production` includes feature; judge должен
    показать win относительно pre-PERF-3 `production`.
  - CHANGELOG; docs sync (FASTBIN_DESIGN / heap_core notes — run-encoded
    freelist как recycle-path optimization).
- **Commit gate:** production-судья показывает win; docs synced.

---

## 4. Риски и открытые вопросы (X7-style enumeration — BEFORE any code)

**X7-урок:** "a missed call site caused a real bug." Применяем ту же дисциплину:
перечислить ВСЕ touch points ЗАРАНЕЕ.

### 4.1 Все sites, читающие/пишущие pointer-linked freelist

Каждое из этих мест — candidate на «нужно также handle run-descriptor» или
«явно НЕ нужно (с обоснованием)»:

| Site | File:line | Что делает | Run-impact |
|------|-----------|------------|------------|
| `Node::read_next` | `node.rs:94` | load `next` из тела free-блока | Вызывается ТОЛЬКО из linked-list drain (`drain_freelist_batch:2723`) и `pop_free`. Run-drain **не вызывает** `read_next` (stride arithmetic). **Не меняем.** |
| `Node::write_next` | `node.rs:74` | store `next` в тело free-блока | Вызывается из `flush_run:2218`, `dealloc_small:2964`, `reclaim_offset:866/971`. Run-encoded flush **не вызывает** для contiguous-accepted блоков. `dealloc_small` (single) и `reclaim` (cross-thread) — **всегда linked-list** (§2.4). **Не меняем seam.** |
| `drain_freelist_batch` | `alloc_core.rs:2690` | batched pop из freelist | **ГЛАВНОЕ изменение Ф3** — добавить RunStack drain перед linked-list drain. |
| `flush_class` / `flush_run` | `alloc_core.rs:2144/2173` | batched push в freelist | **ГЛАВНОЕ изменение Ф2** — detect contiguous accepted, push RunStack. |
| `pop_free` | `alloc_core.rs:~2595` | single pop (non-fastbin) | Under `production` (fastbin), не достигается (magazine path). Если достигается (non-fastbin config) — linked-list pop; RunStack drain может быть добавлен как fast-path, но **НЕ в v1** (pop_free — single block; run-drain только в batched `drain_freelist_batch`). **Оставить linked-list; documented.** |
| `reclaim_offset` | `alloc_core.rs:883` | cross-thread reclaim, single block | **ВСЕГДА linked-list** (§2.4, structural). **Не меняем.** |
| `reclaim_offset_checked` | `alloc_core.rs:768` | cross-thread reclaim + magazine check | То же — **всегда linked-list.** **Не меняем.** |
| `dealloc_small` | `alloc_core.rs:~2940` | own-thread single free | Single block → не образует run → **всегда linked-list.** **Не меняем.** Но: если dealloc_small вызывается в цикле извне (не через flush), блоки МОГЛИ бы стать run — но такого call site нет (verify в Ф2). |
| `dbg_drain_freelist_batch` | `alloc_core.rs:1539` | TEST-only driver | **Должен пройти через тот же `drain_freelist_batch`** — автоматически получит run-drain. Test-only; verify в Ф3. |
| `dbg_freelist_head_for` | `alloc_core.rs:1518` | TEST-only: читает `BinTable::head` | Читает linked-list head. RunStack — отдельная структура; test может нуждаться в `dbg_runstack_peek_for` для полноты. **Добавить в Ф1** (test accessor). |
| `decommit_empty_segment` | `alloc_core.rs:1202` | reset segment на empty | **Ф4: добавить `RunStack::clear_all`.** |
| `BinTable::init_in_place` | `segment_header.rs:709` | init heads на NULL | При segment init — **также** `RunStack::init_in_place` (zero all descriptors). **Ф1.** |

### 4.2 Layout shift risk (как X7-Ф1)

`small_meta_end()` растёт на `RunStack::FOOTPRINT` (~3 KiB) под feature. Это:
- Уменьшает payload capacity segment-а (~0.07%).
- Геометрия multiseg-бенча и любые тесты, считающие blocks-per-segment, могут
  сдвинуться. **Инвентаризовать в Ф1** (grep на `small_meta_end`, `SEGMENT -
  small_meta_end`, block-count assertions).
- `primordial_registry_off` / `primordial_hash_off` — следуют за `small_meta_end`
  под feature; primordial segment layout меняется. Verify в Ф1.

### 4.3 `_spare: u16` в RunDesc — forward-compat, не v1

Spare НЕ используется в v1. НЕ инициализировать его осмысленным значением (zero
при init). Future: generation-stamp per-run (v2). Документировать как reserved.

### 4.4 Mixed-representation drain ordering

`drain_freelist_batch` drain-ит RunStack first, linked-list second. **Порядок
выдачи блоков** меняется (run-blocks first, then linked). Это **не нарушает**
correctness (оба множества disjoint по bitmap, §2.3), но может **изменить
allocation addresses** в тестах, хардкодящих конкретные ptr-ы. **Инвентаризовать
в Ф3** (grep на тесты, сравнивающие выделенные адреса).

### 4.5 RUNSTACK_CAPACITY = 8 — sizing risk

Если real-world workload фрагментирует flush batches сильнее, чем ожидается,
RunStack переполняется → fallback to linked-list → no win but no regression.
**Risk: silent no-op** (не crash). Ф5 ledger явно замеряет, как часто overflow
происходит (counter). Если overflow >10% flushes — рассмотреть capacity=16 в
v1.5.

### 4.6 X7 interplay (per §2.9 — orthogonal, но verify)

Все 4 cfg-комбинации (hardened × runfreelist) должны пройти layout asserts (Ф1) и
хотя бы smoke-test. Особенно `hardened + alloc-runfreelist`: gen-table + RunStack
оба в metadata, оба после ring — verify no overlap, verify `small_meta_end()`
account for both.

---

## 5. Критерии готовности арки (mirror X7 §5)

1. **Ф1–Ф4 committable green:** `cargo test --features production` green на
   каждой фазе; miri green на region/run invariants; loom (если применимо).
2. **Production-судья byte-identical** (feature OFF) на каждой фазе — главная
   neutrality guarantee (как X7 under non-hardened).
3. **Behavioral equivalence test** (Ф2/Ф3): feature-on vs feature-off drain
   выдают **то же множество блоков** (order may differ; set identical).
4. **M2 double-free safety** (§2.3) — explicitly tested: блок в active run,
   double-free через ring → reclaim drop (§2.4); блок выдан ровно один раз.
5. **Decommit-clears-runstack** (§2.5) — explicitly tested: stale descriptor
   после decommit+recarve НЕ восстанавливает невалидный адрес.
6. **Ф5 ledger published** (`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`) —
   iai 11-bench + wall-clock, GO or explicit honest-REJECT (как PERF-2).
7. **Если GO:** Ф6 — feature в `production`, production-судья показывает win,
   docs synced. **Если REJECT:** doc фиксирует, feature остаётся opt-in/off,
   `src/` pristine (как PERF-2 final tree).
8. **All 4 cfg-combinations** (hardened × runfreelist) — layout asserts green,
   smoke-test green.

**Honest-reject — валидный исход.** PERF-2 (TCACHE_CAP sweep) был honest-reject;
этот arc может им же оказаться, если pointer-chase — НЕ bottleneck на real
workloads (например, если cache-line prefetcher уже покрывает dependent loads
на типичном stride). Критерий §Ф5 (5% win, zero regression) — factual gate; если
не выполнен — reject, не «давайте ещё попробуем».

**Future work (v1.5+, если v1 GO):**
- Bitmap RMW merge (§2.7) — `mark_free_range` для contiguous run.
- Run-as-unit generation stamp (Spare field, §2.1) — один gen-byte на run под
  hardened вместо per-block bump.
- `pop_free` run-fast-path (single-pop из run head) для non-fastbin configs.

---

## 6. Предложение фаз для TaskList (НЕ создавать здесь — для ревью человеком)

Это **предложение** структуры; человек превращает в TaskList-записи после
ревью этого док-а.

- **Ф0** — дизайн-док (этот файл). [DONE — этот коммит/файл]
- **Ф1** — RunStack storage + Layout (`run_stack.rs`, `Layout::run_stack_off`,
  `small_meta_end` update, layout asserts, init/clear). **blockedBy:** Ф0
  approved.
- **Ф2** — detect-contiguous-run on flush side (refactor `flush_run` to detect
  contiguous accepted offsets, push RunStack, fallback to linked-list on
  overflow/singletons). **blockedBy:** Ф1.
- **Ф3** — reconstruct-on-drain (add RunStack drain before linked-list drain in
  `drain_freelist_batch`, with `is_free` guard + `mark_alloc` per reconstructed
  block). **blockedBy:** Ф2. [adversarial audit gate]
- **Ф4** — lifecycle seams (`decommit_empty_segment` clears RunStack; recycle/
  adopt verification). **blockedBy:** Ф3.
- **Ф5** — cost/benefit ledger + go/no-go gate (iai 11-bench + wall-clock;
  `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`; GO ≥5% cold/recycle + zero
  regression, OR honest-reject). **blockedBy:** Ф4.
- **Ф6** — (conditional on Ф5=GO) enable in `production`, re-judge, publish.
  **blockedBy:** Ф5 (GO only).

---

*Структура, тон и rigor следуют `X7_GENERATIONAL_RING_PLAN.md` (§1 key insight →
§2 fixed decisions → §3 phases → §4 risks → §5 readiness). Фазовая дисциплина
(коммит после каждой, zero-trust review, judge) — из CLAUDE.md «Phased delivery».
Honest-reject — установленный project precedent (PERF-2 / X4-A / X5 / X6).*
