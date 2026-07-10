# Верификация внешнего аудита `docs/UB_MEMORY_AUDIT.md`

Дата: 2026-07-10.
Метод: независимая построчная сверка каждой находки аудита с текущим рабочим
деревом (`src/`, `crates/`, `tests/`, `Cargo.toml`) + сверка с уже известными
и осознанными ограничениями (README §"Where unsafe lives" и §M2-residual,
`docs/perf/IAI_BASELINE.md` §G1 honest-reject,
`docs/reviews/2026-07-09-unsafe-soundness-review.md` M7, X7-план,
`docs/DURABILITY.md`). Код не изменялся; динамические прогоны аудита
(repro F2, ignored-тест F4) НЕ перезапускались — ограничение этой задачи;
их исход подтверждён статическим анализом кода и, для F4, самой
документацией проекта (тест обязан быть красным).

Замечание о ревизии: аудит заявляет ревизию `c68f64e`; на момент сессии HEAD
был `a664707` (git-команды в этой задаче запрещены, сверить хэш нельзя).
Существенно другое: **все проверенные file:line аудита совпали с текущим
деревом точно** (realloc 2160, dealloc doc 658–666, dealloc_small 3910–3966,
flush_class 2726–2751, ring hooks 487–511/555–567, gen_at 1401–1444,
heap_core 963–1003 / 1159–1269 / 1420–1459, reclaim 943–1049,
malloc-bench 245–249 / 311 / 435–498). Аудит сделан против актуального
кода, пост-PERF-PASS-1..5.

---

## Краткое резюме

| Вердикт | Находки |
|---|---|
| Подтверждено, реально ново (действие рекомендовано) | **F1** (в узкой части — см. ниже), **F6** |
| Подтверждено фактически, но переоткрытие известного/осознанного | **F2**, **F3**, **F4** |
| Подтверждено технически, известный prose-контракт, низкая практическая ценность | **F5** |
| Отклонено как false positive | нет |

Итого: 6 находок; фактическая точность аудита высокая (ни одного неверного
описания кода не найдено), но **классификация severity завышена**: из двух
"CRITICAL/HIGH soundness"-находок одна (F1) — частично новый акцент на уже
задокументированном мембранном трейд-оффе, F2 — фундаментальное ограничение
интерфейса `free(ptr)`, не дефект реализации, F3/F4 — прямые переоткрытия
явно задокументированных и осознанно принятых решений.

---

## Поэтапный разбор находок

### F1 — заявлено CRITICAL: safe `AllocCore::realloc` читает через непроверенный указатель

**Проверка кода — ПОДТВЕРЖДЕНО дословно.**
`src/alloc_core/alloc_core.rs:2160-2194`: `pub fn realloc(&mut self, ptr: *mut u8, ...)`
— safe; `contains_base` проверяется ТОЛЬКО для in-place fast path; при
`contains_base == false` (foreign/bogus ptr) управление падает в move-leg:
`self.alloc(new_layout)` → `Node::copy_nonoverlapping(ptr, new_ptr, copy)`
(`node.rs:148-155` — настоящий `ptr::copy_nonoverlapping`) → чтение
`min(old_layout.size(), new_size)` байт из произвольного адреса.
`HeapCore::realloc` (`heap_core.rs:1252-1268`) — тот же foreign-leg с
безусловной копией. `AllocCore` реэкспортирован публично и БЕЗ `#[doc(hidden)]`
(`lib.rs:280`). Всё, что аудит написал про механизм, верно, включая вариант
"свой блок + завышенный `old_layout`" (OOB-чтение соседних блоков сегмента;
под `alloc-decommit` возможен fault на чтении за `bump`).

**Сверка с известным.** Системный класс проблемы уже был найден и принят:
M7 в `docs/reviews/2026-07-09-unsafe-soundness-review.md` ("safe-мембраны
выносят UB-рычаги в весь крейт", вердикт — осознанный трейд-офф) и явный
ответный абзац в `src/lib.rs:169-193` ("The soundness boundary is WIDER than
the unsafe-syntax boundary"). НО: и M7, и lib.rs-инвентарь описывают
**`pub(crate)`-мембраны** (Node::*, os::release_segment, HeapSlot). Ни в одном
из просмотренных ревью НЕ зафиксировано, что мембрана распространяется на
**полностью публичный, не-doc-hidden safe API** `AllocCore::realloc` — и что
он асимметричен с `dealloc` (тот защищён `contains_base` no-op'ом,
`alloc_core.rs:706-717`, а `realloc` — нет). Это и есть новая часть находки.

**Смягчающие факты, снижающие severity против заявленного CRITICAL:**
- `AllocCore::realloc` production-мёртв: `SeferAlloc`-фасад ходит через
  `HeapCore::realloc` (зафиксировано ранее: bug-hunt review N2, x-arc retro C2).
  Через `GlobalAlloc`-грань этот код недостижим без нарушения и так
  unsafe-контракта `GlobalAlloc`.
- Для `HeapCore::realloc` foreign-leg — это НЕ дефект, а нагруженный дизайном
  путь: под `alloc-xthread` указатель из ЧУЖОЙ кучи легитимен (валидная
  память, копия корректна), и `dealloc` дальше маршрутизирует cross-thread.
  Схлопывать этот путь нельзя.

**Вердикт: подтверждено.** Confidence **HIGH** (по строгой конвенции Rust —
safe pub fn, разыменовывающая произвольный caller-указатель, есть unsoundness
публичной границы). Практическая severity — **MEDIUM**, не CRITICAL (нет
production-достижимости; нужен прямой substrate-caller вне контракта).
Риск исправления **LOW** — см. рекомендации.

### F2 — заявлено HIGH: safe `dealloc` не выполняет "no-op" контракт для stale/interior указателей

**Проверка кода — механизм ПОДТВЕРЖДЁН статически.**
- Doc-комментарий `alloc_core.rs:658-666` действительно обещает: double-free —
  "no-op (M2 — never UB, never corrupts)" без квалификатора.
- `dealloc_small` (`alloc_core.rs:3910-3982`): единственный lifetime-оракул —
  `AllocBitmap::is_free(off)` (3949-3953). Он различает free/allocated, но не
  "жизни" блока. Сценарий аудита корректен: `free(P)` → `alloc() == P`
  (LIFO-реюз через head BinTable; pop делает `mark_alloc`) → повторный
  `dealloc(P)` видит bit=allocated → `write_next` в живые байты occupant'а +
  `mark_free` → следующий `alloc` выдаёт P второй раз. Decommit-ветка не
  мешает repro (сегмент == `small_cur` исключён из decommit, 3978-3981).
- Interior-pointer guard (`off % block_size`) действительно только под
  `hardened` (3930-3933), `hardened` не входит в `production`
  (`Cargo.toml:163,203`) — подтверждено.

**Сверка с известным — это НЕ новый баг, а предел интерфейса.**
1. Stale-free-after-reuse **неустраним ни в каком аллокаторе с интерфейсом
   `free(ptr)`**: второй вызов `dealloc(P, layout)` побайтово идентичен
   легитимному первому free текущего владельца P. Информации для различения
   не существует (нет generation-токена в сигнатуре); даже `hardened` X7
   защищает только ring-путь — что аудит сам честно признаёт. mimalloc/glibc
   ведут себя так же. Проект уже формулировал этот принцип для смежного leg:
   "information-theoretically identical" (`heap_core.rs:995-1000`).
2. Interior-pointer leg — осознанный, задокументированный трейд
   ("a paid check, so `hardened`-gated", `alloc_core.rs:3914-3929`) с
   контрфактическим тестом `tests/regression_hardened_interior_ptr.rs`,
   который аудит сам и цитирует как доказательство — т.е. проект это знал
   и запинил.
3. Замечание к "safe-boundary UB": в сценарии F2 UB наступает только когда
   ПОЛЬЗОВАТЕЛЬ разыменует один из двух алиасящих указателей — а это уже
   `unsafe` на его стороне (любое использование `*mut u8` из `alloc`
   требует unsafe). Сам `write_next` аллокатора ложится в память его же
   сегмента. Это corruption заявленного контракта, но не UB, совершаемое
   самой safe-функцией (в отличие от F1, где чтение из bogus-адреса делает
   сама функция).

**Что в F2 остаётся реальным:** формулировка doc-комментария
`alloc_core.rs:661-666` шире фактической гарантии. Точная гарантия:
"double-free **до реюза** — no-op" (именно её проверяет differential-тест
M2, `tests/alloc_core_differential.rs:146-147`, и формулировка README ближе
к этому). Однострочная doc-правка закрывает расхождение.

**Вердикт: отклонено как новый баг; подтверждено как неточность
документации.** Confidence **HIGH** (поведение — как описано), severity
фактическая **LOW (doc-precision)**. Риск doc-правки **LOW**. Динамический
repro аудита не перезапускался, но статически неизбежен.

### F3 — заявлено HIGH: публичные `#[doc(hidden)]` safe test hooks позволяют UB

**Проверка кода — факты ПОДТВЕРЖДЕНЫ.**
- `remote_free_ring.rs:500-511`: `over_test_buffer`/`init_test_buffer` —
  safe `#[doc(hidden)] pub`, `init_test_buffer` пишет cursors + все слоты по
  переданному адресу (555-567). Используются `tests/remote_ring_unit.rs`,
  `regression_ring_cursor_wrap.rs`, `regression_ring_overflow_counter.rs`.
- `alloc_core.rs:2726-2751`: `flush_class` — safe `#[doc(hidden)] pub`, БЕЗ
  `contains_base`; `flush_run` (2757+) сразу материализует `SegmentMeta`,
  bin_table/bitmap по вычисленному base и пишет `write_next`. Причём
  `flush_class` — не только тест-хук: это production-API магазина
  (`HeapCore` overflow-flush, `docs/FASTBIN_DESIGN.md`), плюс 5 интеграционных
  тестов зовут его напрямую — сузить до `pub(crate)` без перестройки тестов
  нельзя.
- `segment_header.rs:1401-1444`: `gen_at`/`bump_gen` — safe doc-hidden pub
  (только `hardened`), материализуют `&AtomicU8` по caller-provided base/off.
- `lib.rs:234-242`: `alloc_core` сделан `pub` ровно для этого.

**Сверка с известным — санкционированный паттерн.** "Test-only export
pattern" — явно узаконенное исключение №1 в `CLAUDE.md` и в шапках
`lib.rs`/`mod.rs`; lib.rs честно оговаривает "Nothing in alloc_core is stable
public API". То, что `#[doc(hidden)]` не является ни visibility-, ни
safety-границей, проекту известно (это и есть суть паттерна). По строгой
RustSec-конвенции doc-hidden pub safe fn с UB-потенциалом — всё равно
unsoundness крейта; для опубликованного crate это честный hardening-пункт,
но внутри принятой проектом модели — переоткрытие осознанного решения,
не новый дефект.

**Вердикт: подтверждено фактически; классифицировано как известный,
осознанно принятый трейд-офф.** Confidence **HIGH** (факты), severity в
рамках принятой модели **LOW**; если крейт публикуется на crates.io и
заявляет строгую soundness — **MEDIUM**. Риск возможной правки
(feature-gate `test-internals` off-by-default либо `unsafe fn`) —
**LOW-MEDIUM** (трогает 8+ тестовых файлов и производственный вызов
`flush_class` из `HeapCore`; H1/MPSC-смежность у ring-хуков — правка только
сигнатур, не протокола).

### F4 — заявлено MEDIUM: `production` сохраняет ring↔magazine stale-note corruption при cross-thread double-free

**Проверка кода — ПОДТВЕРЖДЕНО, и это ТОЧНОЕ переоткрытие задокументированного
residual.** Все шесть ссылок аудита сходятся с кодом: gen-стемпинг только под
`hardened` (`heap_core.rs:1446-1459`), gen-проверка на drain только под
`hardened` (`alloc_core.rs:1023-1029` в `reclaim_offset_checked`), u8-gen с
принятым 1/256-wrap (`remote_free_ring.rs:242-246` + `docs`), ignored-тест
существует с исчерпывающим обоснованием
(`tests/regression_xthread_double_free_residual.rs:106` — "known residual:
re-issue-before-drain (leg 3) — information-theoretically indistinguishable…
full fix tracked as X7").

Это в точности "третий leg" M2-residual, целиком описанный в:
- README §"Where unsafe lives"/§производительность, строки 779-800 (включая
  hardened-закрытие X7 и принятый 1/256 residual-of-the-residual);
- `heap_core.rs:963-1003` (блок "RESIDUAL M2 LIMIT … #164 NARROWED", legs
  1-2 закрыты, leg 3 — принятый residual, fix = X7 hardened);
- `docs/DURABILITY.md`, `docs/design/X7_GENERATIONAL_RING_PLAN.md`.

"FAILED" ignored-теста, который аудит получил, — это задокументированное
ОЖИДАЕМОЕ поведение ("honestly red without per-block generations"); тест
намеренно оставлен красным как pin отсутствия различающего состояния.
Сценарий требует double-free, т.е. нарушения unsafe-контракта `GlobalAlloc`
— что аудит сам корректно оговаривает.

**Вердикт: отклонено как новая находка — известное, осознанное,
многократно задокументированное ограничение с готовым opt-in-решением
(`hardened`).** Confidence **HIGH** (что поведение такое — да), новизна —
нулевая. Никаких действий; любые правки non-hardened drain-пути имеют
**HIGH** риск (H1-смежный MPSC-протокол + D1/M2 decommit-инварианты) при
нулевой выгоде.

### F5 — заявлено MEDIUM: safe generic `malloc-bench-rs::run` допускает cross-instance dealloc

**Проверка кода — ПОДТВЕРЖДЕНО.** `crates/malloc-bench/src/lib.rs:435-498`:
`pub fn run<A: GlobalAlloc + Send + 'static>` — safe; `make_alloc()` создаёт
отдельный `A` на поток (467); mailbox-хендофф освобождает блок через
локальный `a` получателя (190-195). Prose-контракт "stateless facade over
shared global state" честно задокументирован (394-406), но типами не
enforced: `A`, владеющий per-instance ареной, удовлетворяет bound'ам и даст
foreign free.

**Оценка.** Технически — та же мембранная unsoundness, что F1/F3, но:
(a) это внутренний benchmark-harness, не runtime-аллокатор; (b) все
in-tree потребители (`System`, mimalloc, `SeferAlloc`) — stateless-фасады;
(c) контракт задокументирован сознательно. Чистое исправление существует и
дешёво: `A: GlobalAlloc + Send + Sync + 'static`, ОДИН инстанс в `Arc`,
разделяемый потоками (методы `GlobalAlloc` берут `&self`) — тогда
per-instance-stateful `A` тоже корректен и prose-контракт вообще исчезает;
либо просто `unsafe fn run`.

**Вердикт: подтверждено; известный prose-контракт, низкая практическая
значимость.** Confidence **HIGH** (факт) / practical severity **LOW**.
Риск правки **LOW** (bench-crate, ядро не затронуто).

### F6 — заявлено LOW: утечка блока при ошибке `send` в malloc-bench

**Проверка кода — ПОДТВЕРЖДЁН НОВЫЙ (тривиальный) баг, даже чуть шире
заявленного.**
- `lib.rs:245-249`: комментарий обещает "Free locally to stay UAF/leak-free",
  но тело `if senders[target].send(block).is_err() { … }` ПУСТОЕ — только
  комментарий. `SendError<Block>` дропается, `Block` без `Drop` (94-103) →
  raw-аллокация теряется. Расхождение код↔комментарий — фактическое.
- `lib.rs:311`: `let _ = senders[target].send(block);` — тот же дроп.
- Дополнение к аудиту (аудит считает путь недостижимым в нормальном прогоне):
  он ДОСТИЖИМ при обычном thread-finish skew — worker B, закончивший свои
  steps, дропает `rx` при выходе из замыкания (469-498), после чего send от
  ещё работающего A возвращает Err. Второй leg, который аудит не назвал:
  блоки, успешно отправленные ПОСЛЕ финального `drain_mailbox` получателя
  (492-496), но до дропа `rx`, умирают в очереди канала — тоже утечка без
  dealloc. Обе — только утечки (не UAF/double-free), только в bench-утилите.

**Вердикт: подтверждено, новое.** Confidence **HIGH**, severity **LOW**
(bounded leak в измерительной утилите), риск правки **LOW**.

---

## Что НЕ перепроверялось независимо

- Негативные секции аудита ("Проверенные области без подтверждённой
  проблемы") — приняты как заявление аудита, не как факт; их независимая
  переверификация не входила в задачу.
- Динамические прогоны аудита (repro F2, ignored-тест F4, miri/loom/clippy
  прогоны из "Выполненные проверки") — не воспроизводились по ограничению
  задачи; исходы F2/F4 подтверждены статическим анализом и существующей
  документацией.
- Заявленная ревизия `c68f64e` — git запрещён в этой задаче; проверка
  выполнена против текущего рабочего дерева, с которым все line-refs
  аудита совпали.

## Сводная таблица

| # | Заявлено | Вердикт | Confidence (реальная проблема) | Risk правки | Статус относительно известного |
|---|---|---|---|---|---|
| F1 | CRITICAL | Подтверждено; практич. MEDIUM | HIGH | LOW (guard в `AllocCore::realloc`) | Частично ново: публичная (не pub(crate)) мембрана + асимметрия с `dealloc`; системный класс известен (M7, lib.rs:169-193) |
| F2 | HIGH | Механизм подтверждён; как «баг» — отклонено (предел интерфейса `free(ptr)`) | HIGH (поведение) / LOW (что это дефект) | LOW (doc-правка) | Interior-leg известен и запинен; stale-after-reuse неустраним by design |
| F3 | HIGH | Подтверждено фактически; известный санкционированный паттерн | HIGH (факт) | LOW-MEDIUM | Test-only export pattern — CLAUDE.md исключение №1, lib.rs:234-242 |
| F4 | MEDIUM | Отклонено как новое — документированный M2 leg-3 residual | HIGH (факт) | HIGH (не трогать) | README:779-800, heap_core:963-1003, X7, ignored-тест |
| F5 | MEDIUM | Подтверждено; внутренний bench, prose-контракт задокументирован | HIGH (факт) / LOW (практика) | LOW | Контракт в doc `run` (394-406) |
| F6 | LOW | Подтверждено, НОВОЕ (+второй teardown-leg сверх аудита) | HIGH | LOW | Ново |

## Рекомендации (для подтверждённых actionable-пунктов)

1. **F1 (единственная правка ядра, рекомендую):** в `AllocCore::realloc`
   (`src/alloc_core/alloc_core.rs:2160`) при `!self.table.contains_base(base)`
   возвращать `null_mut()` вместо падения в move-leg — симметрично
   защитному контракту `dealloc`. Для substrate-уровня foreign-указатель
   никогда не легитимен (в отличие от `HeapCore::realloc`, чей foreign-leg —
   нагруженный cross-heap путь под `alloc-xthread`; его НЕ трогать).
   `AllocCore::realloc` production-мёртв (bug-hunt N2), потребитель —
   differential-тесты; правка их не ломает (они реаллоцируют только живые
   собственные блоки). Обязателен контрфактический тест: safe `realloc`
   незарегистрированного указателя → null, старое состояние не тронуто.
   Альтернатива, если решат сохранить мембрану: пометить fn `unsafe` —
   но это ломает публичный API, guard дешевле.
2. **F2:** однострочное уточнение doc-комментария
   `alloc_core.rs:661-666`: "double-free **до реюза адреса** — no-op";
   после реюза второй free неотличим от легитимного free текущего
   владельца (предел интерфейса, общий для всех аллокаторов).
3. **F6:** в `crates/malloc-bench/src/lib.rs:245-249` и `:311` —
   `if let Err(e) = send(block) { unsafe { free_block(a, e.0) } }`;
   опционально отметить в doc, что блоки, оставшиеся в канале на teardown,
   утекают (или доливать финальный drain после join всех воркеров).
4. **F3/F5:** действий не требуется в текущей модели; если крейты будут
   публиковаться с заявкой строгой soundness — рассмотреть feature-gate
   (`test-internals`, off-by-default) для doc-hidden хуков и
   `Arc<A>`-редизайн (`A: Sync`, один инстанс) для `malloc_bench_rs::run`.
5. **F4:** ничего не делать; residual закрыт opt-in `hardened` (X7),
   документация и pin-тесты уже точны. Любая правка non-hardened
   drain-пути — HIGH-risk (H1-смежный MPSC + decommit-инварианты) без выгоды.
