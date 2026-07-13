# Round4 — план реализации (2026-07-13)

Источники: `docs/agent_reviews_round4/{memory_safety_review,performance_review,code_quality_review}.md`.
Таски: #93–#102 (цепочка `blockedBy` в порядке ниже).
Ключевые утверждения ревью выборочно сверены с кодом оркестратором
(`src/lib.rs:243-260` — doc-hidden `pub mod alloc_core/global/registry`;
`src/registry/heap_slot.rs:233-270` — `state`/`generation` `pub` «только для
integration tests»; `src/alloc_core/alloc_core_small.rs:607-622` —
`refill_class` возвращает `want` при `out.len() < want` в release). Остальные
строки-ссылки — из ревью, каждый исполнитель обязан перепроверить их сам
(zero-trust: ревью — это claim, не receipt).

---

## Созерцание: общая картина

Round4 распадается на **три темы**, а не десять несвязанных пунктов:

**Тема A — «doc-hidden ≠ приватно» (таски #93, #101, частично #94).**
Установленный в проекте "test-only export pattern" (doc-hidden `pub mod`,
чтобы integration-тесты дотягивались до внутренностей) прополз дальше, чем
задумывался: теперь через него наружу торчат не только read-only test hooks,
но и **мутабельный control plane** реестра (`state.store`, `free_slots`,
`dbg_pack`) и raw-memory writers (`init_test_buffer`, `RunStack`, gen-table).
Это одна архитектурная проблема с одним системным решением, а не набор
точечных заплаток. Round2/T3 добавил release-guards на index/alignment — это
было правильно, но не закрывает validity/lifetime: guard не может проверить,
что переданный указатель указывает на живую памать нужного размера.

**Решение темы A (единое для #93 и #101):** двухслойная граница.
1. Всё, что мутирует состояние или пишет по caller-supplied указателю, —
   `unsafe fn` с формальным `# Safety` (это честная граница: контракт
   невыразим в типах ⇒ он обязан быть в сигнатуре).
2. Visibility сужается до `pub(crate)` везде, где integration-тесты могут
   быть переведены на существующие безопасные обёртки; где не могут —
   test hook остаётся, но становится `unsafe fn` (тест в `tests/` спокойно
   пишет `unsafe { ... }` — тестам unsafe разрешён, downstream-крейту
   сигнатура честно сообщает контракт).
   Вариант «отдельный непубликуемый test-support crate» рассматривался и
   отложен: он требует перекроить 149 файлов тестов и дублировать
   `pub(crate)`-доступ; `unsafe fn` + сужение полей даёт ту же soundness
   за долю цены. Если при выполнении выяснится, что какой-то тест
   принципиально требует `pub` поля — эскалировать, не расширять поверхность
   молча.

**Тема B — жизненный цикл памяти и конфигурации (таски #95, #97, #96).**
Whole-slot reuse (замена старого abandonment-протокола) оставил три хвоста:
(1) teardown не отдаёт кеши — RSS прилипает к пику числа потоков;
(2) config прилипает к recycled slot — first-bind-wins стал ещё и
first-*materialization*-wins; (3) документация местами описывает старый
abandonment-мир. Плюс сам мёртвый abandon/adopt-substrate внутренне
противоречив. Это одна история: «доопределить и дореализовать модель
whole-slot reuse до конца», и порядок внутри неё важен — сначала поведение
(#95), потом решение о судьбе мёртвого кода (#97), документация (#96) может
идти до или между ними, она дешёвая.

**Тема C — перф-ядро (таски #99, #100).** Три подтверждённых двумя раундами
bottleneck'а (O(S)-скан, retry storm, per-hit RMW) + p99-spike нашего же
tombstone-фикса. Это RAD-кампания с iai-гейтами, по образцу RAD-4/RAD-5:
каждый эксперимент — честный GO/NO-GO, NO-GO с полным revert — тоже результат.

Вне тем: #94 (два точечных бага — делать рано, они маленькие и дают
немедленную ценность), #98 (механическая уборка), #102 (рефакторинг
монолитов — последний, опциональный).

**Что сознательно НЕ делаем:** ре-литигацию M2 (safe `dealloc`/`realloc`).
Round3 закрепил design decision, round4 не принёс нового эксплойта — только
новые формулировки старого аргумента. Берём из R4-MS-1/2 два нере-литигационных
зерна (`magic_at(0)`-барьер → #94b; вопрос «дешёвые hardened-guards в
production?» → измерить в рамках #99 через iai, решить по цифрам).

---

## Порядок исполнения (цепочка тасков)

```
#93 → #101 → #94 → #95 → #96 → #97 → #98 → #100 → #99 → #102
 └─ Тема A ──┘      └── Тема B ──────┘          └─ Тема C ─┘
```

Логика: сначала CRITICAL soundness (тема A целиком — #101 сразу после #93,
потому что решение одно и то же, и второй таск переиспользует
паттерн/инфраструктуру первого); затем точечные баги; затем поведение
lifecycle; перф — после стабилизации поверхности (иначе iai-baseline будет
плавать под ногами); монолиты — в самом конце.

---

## Схемы по задачам

### #93 (R4-1) — закрыть control plane registry — CRITICAL

**Суть дыры:** safe-код downstream может исполнить руками транзакцию
`LIVE → FREE → повторный claim`, потому что все её ингредиенты публичны:
`bootstrap::ensure()` → `®istry.slots[i].state` (`heap_slot.rs:242`,
`AtomicU8`, pub) + `generation` (`:270`, pub) + `free_slots` +
`tagged_ptr::dbg_pack`. TLS hot path (`tls_heap.rs:367-388`) не
перепроверяет state — два потока получают `&mut HeapCore` на один слот.

**Схема фикса:**
1. `HeapSlot::state`, `HeapSlot::generation` → `pub(crate)`.
2. Тестам (`tests/registry_basic.rs`, `tests/regression_counter_wrap.rs` —
   список уточнить grep'ом `\.state\.` / `\.generation\.` по `tests/`) дать
   узкие doc-hidden **читающие** аксессоры (`dbg_slot_state(idx) -> u8`,
   `dbg_slot_generation(idx) -> u64`) и, для counter-wrap counterfactual,
   один **`unsafe fn dbg_preset_generation(idx, val)`** с `# Safety`
   («слот не должен быть LIVE / не конкурировать с claim»).
3. `Registry.slots`/`free_slots`/`count`/`abandoned_segs` → `pub(crate)`;
   `bootstrap::ensure()` может остаться (вернуть `&'static Registry` с
   приватными полями безвредно) — проверить, что тесты не лазят в поля мимо
   аксессоров.
4. `tagged_ptr::dbg_pack` и константы: если используются только в
   `tests/tagged_ptr*`-юнитах — оставить (чистая функция упаковки битов
   опасна ТОЛЬКО в сочетании с публичным `free_slots`; после шага 3 она
   перестаёт быть ингредиентом атаки). Отметить это в доке.
5. **Не делать** ревалидацию state/generation на TLS hot path — это
   перф-цена за защиту от уже-невозможного (после шагов 1–3 safe-код не
   может увести слот). В доке `unsafe impl Sync` (`heap_slot.rs:398-411`)
   добавить строку: инвариант опирается на `pub(crate)`-границу.

**RED→GREEN:** компилируемость атаки. RED — минимальный сниппет
(`reg.slots[0].state.store(...)`) компилируется сегодня (доказать до фикса,
например тестом в `tests/`, который потом удаляется); GREEN — после фикса
такой код не компилируется из `tests/` (а значит и из downstream). Плюс
позитивный тест: все существующие registry-тесты зелёные через новые
аксессоры.

**Риск:** масштаб правок в тестах. Митигируется grep-инвентаризацией ДО
начала: `grep -rn "\.state\.\|\.generation\.\|free_slots\|dbg_pack" tests/`.

### #101 (R4-9) — raw test-surface → unsafe fn / pub(crate)

Продолжение паттерна #93 на остальные hooks. Классификация:

| Hook | Класс | Действие |
|---|---|---|
| `RemoteFreeRing::{init,over}_test_buffer` | пишет/читает по caller-ptr `FOOTPRINT` байт | `unsafe fn` + `# Safety` |
| `RunStack::{push,pop,peek,is_empty}` по `base` | raw R/W по caller-base | `unsafe fn` |
| gen-table `gen_at`/`bump_gen`/`init_gen_table_in_place` | raw + `&'static` fabrication | `unsafe fn` |
| `dbg_recycle`, `dbg_corrupt_freelist_head_next`, `dbg_drain_freelist_batch` | мутируют по вычисленному base без membership | `unsafe fn`; в `dbg_*`-дампах два оставшихся `debug_assert!` длины → `assert!` |
| `numa::bind_segment` | полагается на внутреннее владение reservation | `pub(crate)` если снаружи не нужен; иначе `unsafe fn` |

Механика: пометить `unsafe fn`, перенести прозу контракта в `# Safety`,
обновить все вызовы в `tests/` (`unsafe { ... }` + короткий комментарий,
почему контракт соблюдён). Поведенческих изменений нет ⇒ regression-тест
здесь — компиляционная граница (RED: safe-вызов компилируется; GREEN: не
компилируется без `unsafe`).

### #94 (R4-2) — два точечных бага

**(a) `refill_class` (`alloc_core_small.rs:607-622`).** Схема — источник
истины `out.len()`:
```text
let take = want.min(out.len());
for (i, slot) in out.iter_mut().take(take).enumerate() { ... }
take   // вместо want
```
плюс `assert!(out.len() >= want)` можно сохранить как контрактный сигнал —
но возврат обязан быть правдой независимо от assert'а. Тест: release-профиль,
`out` короче `want`, до фикса возвращается `want` > заполненного (RED),
после — фактический count (GREEN). Проверить единственного внутреннего
вызывающего (grep `refill_class(`) — не полагается ли он уже на возврат.

**(b) `magic_at(0)`-барьер.** В foreign-leg `HeapCore::{dealloc,realloc}`
(под `alloc-xthread`) перед `SegmentHeader::magic_at(base)` вставить
process-global membership-проверку **без разыменования**: у нас уже есть
структуры, знающие все живые сегменты (segment_table membership /
registry-слоты). Требование: проверка по значению адреса (диапазон/хеш),
не по чтению памяти по адресу. NB: `base == 0` (null) — отдельный дешёвый
early-out, покрывающий конкретный контрпример `1 as *mut u8`; но фикс
должен закрывать класс (любой мусорный адрес, чей вычисленный base не
зарегистрирован), не только null. Тест: `dealloc(1 as *mut u8, layout)` и
`realloc(...)` под `alloc-xthread` — no-op без чтения (RED ловится
miri-таргетом на этот тест: до фикса miri сообщает invalid read).
**Явно НЕ трогаем** сам safe-контракт M2.

### #95 (R4-3) — memory envelope (N1+N2)

Самая содержательная поведенческая задача раунда. Разбить на два коммита.

**N1 — teardown trim.** Схема:
1. В `HeapRegistry::recycle`/slot-teardown path добавить `trim_for_recycle()`
   на `HeapCore`: flush tcache → drain small pool (возврат сегментов до
   decommit/release) → evict large cache. Всё это выполняет **сам умирающий
   поток до** перевода `LIVE → FREE` (никакой чужой квиесценции не нужно —
   owner ещё единственный писатель).
2. Warm reserve: оставить 0 по умолчанию при recycle (слот FREE — никто не
   обещает, что его скоро возьмут). Прежний кеш-выигрыш живых потоков не
   трогаем.
3. Глобальный budget retained bytes — **отложить** (отдельное решение с
   contended-счётчиком; teardown-trim закрывает главный сценарий «волна
   коротких потоков» без него). Записать как follow-up в доку задачи,
   не тащить в этот коммит.

Тест N1: волна из N коротких потоков, каждый делает alloc/free разных
классов; после join — через существующие счётчики (`decommit_calls`/
`segments_released_total`, см. `working_set_cycle`-бенч) доказать, что
сегменты отпущены (RED: до фикса дельта 0; GREEN: после — ≥ ожидаемого).
Плюс `npm run bench:table` до/после — teardown-trim не должен трогать
hot-path числа (он строго на пути смерти потока).

**N2 — честный config.** Минимальное честное решение: при
`claim_with_config` на слот с уже материализованным core — **сравнить**
запрошенный config с действующим; при расхождении дебаг-путь: `debug_assert!`
+ release-путь: один раз (per-process, atomic flag) логировать/учитывать в
`AllocStats` счётчик `config_conflicts`. Полный reconfigure-with-trim —
сознательно НЕ делаем (цена/риск не оправданы: легитимный кейс — один
глобальный аллокатор). Документацию `with_config` дополнить: «per-slot
first-materialization-wins». Тест: два конфига на recycled slot →
детектируемый сигнал вместо silent ignore.

### #96 (R4-4) — док-фиксы

Механика по списку из таска (Cargo.toml `alloc-xthread`-блок → RemoteFreeRing
/ whole-slot reuse; `registry/mod.rs` + `tls_heap.rs` заголовки → убрать
«guard abandons segments»; `crates/region/README.md` → синхронизировать со
своими же source docs, cross-**type** branding, per-method complexity).
Правило: **новую прозу сверять с кодом, а не с ревью** — ревьюер тоже мог
ошибиться. Doc-only: `cargo test --features production` (нет случайного
code-touch), `--doc == 0`.

### #97 (R4-5) — abandon/adopt + feature-матрица

**Рекомендация оркестратора: вариант (А) — удалить substrate.** Аргументы:
(1) production-путь — whole-slot reuse, abandonment недостижим; (2) код
внутренне противоречив (`try_adopt` игнорирует fail регистрации,
`reset_stamp_cache` не вызывается, `next_abandoned` шарится с deferred-large)
— «проверенная основа» является иллюзией, а исправлять мёртвый код дороже,
чем восстановить его из git при реальной необходимости; (3) прецедент
уже есть — `LargeCacheMode::{Background,Both}` удалили по тому же принципу
«make invalid states unrepresentable», и git-история хранит всё. Удаление:
`push/pop_abandoned_segment`, `try_adopt`, `abandon_segments`, поле
`abandoned_segs`, связанные тесты; CHANGELOG BREAKING-запись по формату
`121a445`/`eb0dbd3`. **Но** это архитектурная развилка — перед исполнением
показать пользователю этот параграф и получить подтверждение.

Feature-матрица (второй коммит): `alloc-runfreelist` — удалить из manifest
(NO-GO зафиксирован в `PERF3_RUN_FREELIST_EXPERIMENT.md`, код эксперимента
восстановим из git); `experimental`-tier — объявить срок удаления в
CHANGELOG/README (не удалять сейчас — там публичные типы, это отдельный
breaking с собственной миграцией); `pinning` — развязать от полного bundle,
если это возможно без нового публичного API. Прогон: все три CI-конфигурации
(`""`, `--features experimental`, `--all-features`) + `npm run check`.

### #98 (R4-6) — bitmap-дедуп + Reservation-тест

**#7:** приватный `struct SegmentBitmap { bits: ... }` c `init/locate/test/
set/clear`; `AllocBitmap`/`MagazineBitmap` — newtype-обёртки с доменными
именами методов (`mark_alloc`/`clear_magazine` и т.п.), делегирующими внутрь.
`#[repr(transparent)]`, `#[inline]` на всё — iai до/после обязателен
(нулевая Ir-дельта — критерий приёмки: это чистый рефакторинг).
**#8:** `Reservation::is_empty` — рекомендация: **deprecate + удалить тест**
(non-empty RAII handle; `len == 0` вне валидного state space, значит метод
не имеет осмысленного вызова). Zero-length-как-валидное-состояние — не
вводить (расширение unsafe-контракта ради тестируемости бессмысленного
метода). Если `is_empty` кем-то используется — grep покажет; тогда
эскалировать.

### #100 (R4-8) — tombstone rebuild spike + aligned lookup

**N3, схема — incremental rebuild с бюджетом:** вместо «на 513-м tombstone
синхронно всё перестроить» — при превышении порога переключаться в режим
миграции: новый hash-массив (второй буфер уже есть смысл держать статически,
таблица фиксированного размера), и каждый последующий unregister/register
переносит ≤ K слотов (K ≈ 64). Lookup в режиме миграции проверяет оба
массива (два probe вместо одного — только в окне миграции). Альтернатива
дешевле — **backshift deletion** (вообще без tombstones): для
open-addressing с linear probing это классика; оценить первым — если probe
distances короткие (а таблица 2048/1024 слотов с низким load factor),
backshift может быть и проще, и лучше. Порядок: (1) прототип backshift →
iai + targeted p99-тест; (2) если backshift ломает какой-то инвариант —
incremental migration. Тест: последовательности 511/512/513 tombstones,
латентность каждого unregister (существующий паттерн из ревью), инвариант
lookup-корректности property-тестом (вставки/удаления/поиски против
эталонной HashMap).

**R8** — только если N3 закрыт с запасом времени: константная таблица
`next_compatible[class][align_log2]` (генерится build-скриптом или
`const fn`), exhaustive equivalence-тест по всем `(size, align)` уже
существует (`tests/size_classes_slow_path_equivalence.rs`) — расширить.

### #99 (R4-7) — RAD-кампания перф-ядра

Три независимых эксперимента, каждый по протоколу RAD (baseline iai →
изменение → iai → GO/NO-GO, NO-GO = полный revert + запись в
`IAI_BASELINE.md`). Порядок — от наименее рискованного:

**R2 (retry storm) — первый.** Локальное изменение с очевидной семантикой:
после первого fail — ≤ 16 попыток с экспоненциальным backoff (spin_loop),
затем overflow; overflow-статистика инкрементится один раз на логическую
free. Dead-owner gate сохранить. Ловушка: НЕ раздуть happy path — retry-код
должен остаться в cold-ветке (`#[cold]`/`#[inline(never)]`). Тест: paused
owner (поток, держащий lock/спящий) + producer — измерить attempts-счётчик
до/после; loom-модель push/drain не должна деградировать.

**R3 (MagazineBitmap RMW) — второй.** Самый ценный и самый опасный
(double-free oracle). Схема-кандидат: не убирать бит, а **батчить** —
clear_magazine выполнять при refill/flush пачкой (магазин и так уже
проходит по блокам), а на per-hit пути не трогать bitmap вовсе; точность
оракула сохраняется, если own-free-проверка учитывает «блок в магазине»
через in-magazine scan (уже существует) + бит, который теперь означает
«был в магазине с последнего flush» — таблица переходов состояний
обязана быть выписана и покрыта property/state-machine тестом ДО замера.
Если инвариант точности не сводится — честный NO-GO. Отдельно измерить
через iai вопрос из R4-MS: сколько стоят дешёвые hardened-guards
(Large-kind check в tcache-пути) — если ≤ ~2 Ir/op, предложить включить
в production (решение — пользователю).

**R1 (O(S)-скан) — третий.** Самый большой по объёму: owner-private
per-class availability bitset (u64-слова по сегмент-индексам, `MAX ~
count()`), обновление в местах: refill-исчерпание (clear), free/ring-drain
(set), recycle/decommit (clear), NUMA-кандидаты отдельным bitset'ом.
Модельный тест переходов ДО оптимизации: обёртка, которая на каждом шаге
сверяет bitset с brute-force сканом (в debug). Риск lost-wakeup (сегмент
есть, бита нет) = утечка ёмкости — сверка обязана быть property-тестом.

**Гейты кампании:** churn ±10 Ir; cold/recycle — цель −15…−25 Ir/op
минимум для GO по R1/R3; р99-бенч для R2. После кампании — `npm run
bench:table` + обновить `IAI_BASELINE.md` "CURRENT reference".

### #102 (R4-10) — монолиты (опционально, последний)

Только после стабилизации всего остального (иначе каждый split порождает
конфликты с содержательными правками). Механика: по одному файлу за коммит,
`impl`-блоки по подсистемам в подфайлы (`alloc_core_small_refill.rs`,
`..._free.rs`, `..._diag.rs` и т.п. — паттерн уже начат:
`alloc_core_large.rs`/`alloc_core_small_pool.rs` существуют); историю
task-ID и rejected alternatives — в `docs/` (ADR), в коде оставить контракт
и safety-proof. Критерий приёмки: `git diff` показывает только перемещения
(проверять `git diff --color-moved`), публичная поверхность идентична,
iai-дельта нулевая, все тесты зелёные. Если к моменту старта задача
покажется неоправданной по цене — честно закрыть как «отложено» с этой
формулировкой.

---

## Сквозные требования (все таски)

- Каждый фикс — с RED→GREEN regression-тестом (проверенным counterfactual:
  откат фикса — тест падает); тесты в `tests/`, не inline; никаких doctests.
- Полный `cargo test --features production`, `cargo fmt --all -- --check`,
  `clippy --features production --all-targets -- -D warnings` — лично
  оркестратором, не со слов агента.
- Перф-чувствительные таски (#95, #98, #99, #100, #102): `npm run iai`
  до/после; вердикты по Ir, не по wall-clock.
- Breaking-изменения (#97, возможно #93/#101, если что-то из doc-hidden
  поверхности кто-то считал API): запись в `CHANGELOG.md [Unreleased]` по
  установленному формату; версию НЕ поднимать.
- Коммит после каждого таска (санкционированные phase-boundary коммиты);
  push — только по явной просьбе.
- Закрывающее независимое ревью (@oh) — после всех этапов, по образцу
  раундов 2/3.

## Точки эскалации (требуют слова пользователя до исполнения)

1. **#97:** подтвердить удаление abandon/adopt substrate (рекомендация — да).
2. **#99/R3:** если дешёвые hardened-guards окажутся ≤ ~2 Ir/op — включать
   ли их в production.
3. **#93/#101:** если какой-то integration-тест принципиально не переводится
   на суженную поверхность.
