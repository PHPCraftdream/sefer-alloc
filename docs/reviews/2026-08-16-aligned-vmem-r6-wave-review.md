# Независимое read-only ревью волны R6 (`58f4c0b..8d68715`)

Дата: 2026-08-16
Проверенный диапазон: `58f4c0b..8d68715` (8 коммитов)
Объект: `crates/vmem` (src/tests/CHANGELOG/Cargo.toml), `tests/remote_ring_shadow_head.rs`,
`scripts/check-all.mjs`, `.github/workflows/ci.yml`, оба open-items индекса.

## Режим и границы

- Только чтение: исходники, тесты, diff, история, CI-конфиг. Код не менялся, коммитов нет.
- `cargo test` / `cargo clippy` / benchmarks не запускались; все выводы — статические,
  но каждый привязан к конкретной строке и конкретной платформе/входу.
- Первоисточник findings волны прочитан целиком:
  `docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r5.md` (R5-1…R5-5).
- Findings, закрытые самими коммитами волны, не переоткрываются (см. §"Проверено, дефектов
  не найдено" — там перечислено, что именно проверено и почему признано закрытым).

## Итог

Продакшн-код волны корректен: ни одного HIGH. Основная масса найденного — это
**повторение доминирующего класса кампании (документация/комментарий описывает проверку
или поведение, которых в коде нет), на этот раз внутри самих исправлений**, плюс
несколько дыр в покрытии, о которых коммиты либо умалчивают, либо заявляют обратное.

Сводка: **HIGH 0, MEDIUM 4, LOW 7, INFO 2**.

Самое существенное:

1. R5-4 закрыл counter-doc drift и в том же коммите завёл новый: доки нового счётчика
   `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` описывают режим, который он НЕ выделяет
   (F1, MEDIUM) — на обычном Windows-хосте без `SeLockMemoryPrivilege` (в т.ч. на
   `windows-latest` этого репозитория) счётчик будет расти штатно, а не «должен быть ноль».
2. Новый тест `bench_internals_counters.rs` вакуумен на Linux и macOS (F2, MEDIUM);
   дискриминирующая проверка есть только под `#[cfg(windows)]`, а заголовочный док
   утверждает обратное безусловно.
3. Ключевая новая ветка R5-1 (`&& !self.is_huge()`) не покрыта ни одним исполняемым
   assert'ом ни на одном CI-ряду (F3, MEDIUM).
4. Два follow-up'а, явно записанных в тело коммита `66b8508`, не попали ни в один индекс
   (F7, MEDIUM) — ровно сценарий R22-3 из CLAUDE.md.

---

## Findings

### F1 — MEDIUM: доки нового счётчика `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` описывают режим, который он не выделяет

**Где:**
- `crates/vmem/src/lib.rs:465-471` (rustdoc самого статика; ключевая фраза — `:470`)
- `crates/vmem/src/lib.rs:216-224` (module-секция; ключевая фраза — `:222-224`)
- Инкремент: `crates/vmem/src/lib.rs:2622-2627`

**Что заявлено.** `lib.rs:470`: «On a healthy Windows kernel this should be zero, but it
exists as defensive instrumentation for kernel/constant violations».
`lib.rs:222-224`: «Separating these lets a test distinguish "OS refused large pages entirely"
from "large pages granted but alignment contract violated"».

**Что в коде.** Guard инкремента — `if extra_commit_flags != 0` и ничего больше
(`:2623`). R5-4 сознательно убрал терм `!huge_granted`, который был ЕДИНСТВЕННЫМ
дискриминатором между «large pages выданы, но база не выровнена» и «large pages
отказаны, ordinary-retry вернул невыровненную базу».

**Сценарий отказа (конкретный вход/платформа).** Обычный Windows-хост без
`SeLockMemoryPrivilege` — это и `windows-latest` в `.github/workflows/ci.yml:792`, и любой
непривилегированный процесс. Вызов `reserve_aligned_huge(2 MiB, 2 MiB)`:

1. `reserve_aligned_huge_raw` → `win_reserve_commit(size, align = 2 MiB, commit_len = size, MEM_LARGE_PAGES)`
   (`lib.rs:2880`).
2. `fast_path_align_threshold = GetLargePageMinimum()` = 2 MiB (`lib.rs:2546-2551`), значит
   `align <= threshold && commit_len == size` истинно → входим в single-call fast path (`:2553`).
3. `VirtualAlloc(..., MEM_LARGE_PAGES, ...)` падает с `ERROR_PRIVILEGE_NOT_HELD` → NULL.
4. Retry без флага (`:2580-2585`) успешен, база выровнена только на
   `WIN_ALLOCATION_GRANULARITY` = 64 KiB; `huge_granted = false` (`:2588`).
5. Безусловная проверка `!base.addr().is_multiple_of(align)` (`:2618`) не проходит
   для 31 из 32 возможных granularity-выровненных баз →
   `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES += 1` (`:2626`).

Итог: счётчик растёт, хотя large pages были **отказаны**, а не «granted but misaligned».
Этот путь реально прогоняется собственными тестами репозитория на Windows-ряду
(`ci.yml:821`): `crates/vmem/tests/decommit_capability.rs:93` и
`crates/vmem/tests/smoke.rs:1716` оба зовут `reserve_aligned_huge(2 MiB, 2 MiB)`.

**Неверный результат.** Оператор, следующий Next trigger'у из
`docs/perf/OPEN_ITEMS.md:1219` (решение о сужении спекулятивного окна принимается по
сумме двух счётчиков), прочитает большое ненулевое значение как нарушение контракта
ядра/константы, тогда как это штатная стоимость расширения fast-path'а по II-3
(`align <= GetLargePageMinimum()` вместо `<= 64 KiB`). Продакшн-поведение не затронуто —
счётчик диагностический, поэтому MEDIUM, а не HIGH.

**Рекомендация.** Либо вернуть дискриминатор (два инкремента: `huge_granted` → «granted but
misaligned», `!huge_granted` → «refused, retry misaligned»), либо переписать обе доки:
убрать фразу «should be zero on a healthy Windows kernel» и прямо сказать, что доминирующая
популяция счётчика — «large-page запрос отклонён, ordinary-retry вернул базу, не
удовлетворяющую `align`». Заодно поправить `lib.rs:222-224`, которая обещает ровно то
различение, которое код больше не делает.

---

### F2 — MEDIUM: `bench_internals_counters.rs` вакуумен на Linux и macOS, а его собственный заголовок утверждает обратное

**Где:** `crates/vmem/tests/bench_internals_counters.rs:7-10` против `:31-38` и `:74-79`.

**Что заявлено.** `:7-10`, секция «# What this test DOES verify»: «`reset_bench_internals_counters()`
brings all counters to exactly zero **after a successful reservation that demonstrably
incremented at least one counter**. This proves reset actually clears the counters, not that
they were already zero.» Сформулировано безусловно.

**Что в коде.** Единственная проверка «до reset'а счётчик ненулевой» стоит под
`#[cfg(windows)]` (`:74-79`). На 64-битном Unix, как признаёт третий буллет
«does NOT verify» этого же дока (`:31-38`), `reserve_aligned(PAGE, PAGE)` (`:66`) не
гарантирует ни одного инкремента:

- `UNIX_EXACT_RESERVE_ATTEMPTS`/`_HITS` — только `target_pointer_width = "32"`
  (`lib.rs:3134-3160`);
- `UNIX_MADVISE_ATTEMPTS`/`_SUCCESSES` — только при `decommit`/`decommit_lazy`, которых
  тест не делает (он делает `drop(reservation)` на `:69`);
- `UNIX_MUNMAP_FAILURES` — только при отказе `munmap`;
- все `WINDOWS_*` — структурно 0.

**Сценарий отказа.** Закомментировать любую строку `store(0, ...)` в
`reset_bench_internals_counters` (`lib.rs:578-593`), кроме `WINDOWS_RESERVE_COMMIT_*`,
и прогнать `cargo test -p aligned-vmem --all-features` на ubuntu (`ci.yml:180`, `ci.yml:942`)
или macOS (`ci.yml:861`). Все 14 `assert_eq!(..., 0)` (`:87-100`) читают `0 == 0` и проходят.
Регрессия класса R3-6 ловится ровно на одном из трёх OS-джобов. Контрфактическая проверка,
описанная в теле коммита `88592d7` («commenting out one store(0) call and observing the test
fail»), была выполнена на Windows и на Linux/macOS не воспроизводится.

**Рекомендация.** Сделать проверку дискриминирующей и на Unix: зарезервировать
`4 * page_size()` (не `PAGE` — на 16 KiB-хосте Apple Silicon `decommit(0, PAGE)` отсекается
guard'ом `!end.is_multiple_of(ps)` в `lib.rs:1945` и madvise не выдаётся вовсе), вызвать
`reservation.decommit(0, page_size())` ДО `drop`, и добавить
`#[cfg(all(unix, not(aligned_vmem_mock)))] assert!(unix_madvise_attempts() >= 1)`. Плюс
привести первый буллет `:7-10` в соответствие с третьим буллетом `:31-38` — сейчас они
противоречат друг другу в одном doc-комментарии.

---

### F3 — MEDIUM: ключевая новая ветка R5-1 (`&& !self.is_huge()`) не покрыта ни одним исполняемым assert'ом

**Где:**
- Реализация: `crates/vmem/src/lib.rs:990-992`
- Тест: `crates/vmem/tests/decommit_capability.rs:85-115` (huge-ветка — `:96-102`,
  else-ветка — `:103-111`, отсутствующий `else` для `None` — `:95`/`:113-114`)

**Что заявлено.** Док теста `:79-84`: «Counterfactual for huge case: if the implementation
returns `Self::decommit_reclaims_and_zeroes()` (removing `&& !self.is_huge()`), this test
fails on any host where `is_huge() == true`».

**Сценарий отказа.** Удалить `&& !self.is_huge()` из `lib.rs:991` и прогнать весь
vmem-suite на любом ряду CI. Ни один тест не упадёт:

- ubuntu (`ci.yml:150` `aligned-vmem package gates`, `ci.yml:893` `test workspace members`) —
  hugetlb-пул не сконфигурирован, `unix_reserve` проваливает `MAP_HUGETLB`-путь
  (`lib.rs:3100-3131`) и падает в общий over-reserve fallback с `granted_huge = false`;
- windows-latest (`ci.yml:792`) — нет `SeLockMemoryPrivilege`, см. цепочку в F1, шаг 3-5,
  → `is_huge() == false`;
- macos-latest (`ci.yml:841`) — `huge-pages` на Darwin в `unix_reserve` не даёт MAP_HUGETLB
  вовсе.

Во всех трёх случаях исполняется else-ветка `:103-111`, которая утверждает ровно то же,
что уже утверждает соседний тест
`can_decommit_reclaim_and_zero_matches_platform_for_ordinary_reservations` (`:124-164`).
То есть тест на CI структурно дублирующий, а не регрессионный для того, ради чего написан.

Отдельно: `if let Some(ref reservation) = huge_r` на `:95` не имеет `else` — при
`huge_r == None` тест исполняет **ноль** assert'ов и всё равно зелёный (`:113-114`
объявляет это допустимым). Это та же форма, которую сам аудит R5 предъявляет коду.

**Почему MEDIUM, а не HIGH.** Продакшн-код (`lib.rs:991`) корректен; это дыра покрытия,
уже проиндексированная как `docs/CORRECTNESS_OPEN_ITEMS.md` item 59 (отсутствие
hugetlb-runner'а). Но док теста `:79-84` подаёт свой контрфакт как действующий, не
оговаривая, что он недостижим ни на одной машине этого проекта.

**Рекомендация.** (а) Дописать в док теста, что huge-ветка на текущем парке CI не
исполняется, со ссылкой на item 59. (б) Заменить `if let Some(..)` на
`let huge_r = reserve_aligned_huge(size, size).expect(...)` — так же, как это уже сделано в
`crates/vmem/tests/smoke.rs:1716-1717`, чтобы два теста в одном крейте не трактовали
одну и ту же ситуацию противоположно. (в) Если нужен реальный оракул для
`&& !self.is_huge()` без hugetlb-runner'а — единственный честный путь — mock-бэкенд
(`--cfg aligned_vmem_mock`), а не подделка `granted_huge` через `from_raw_parts`
(это ровно тот контракт, который R5-3 только что удалил).

---

### F4 — LOW: `reconstructed_reservation_is_actually_usable` всё ещё молча пропускается, а «hard asserts» тавтологичны

**Где:** `crates/vmem/tests/round_trip_contract.rs:159-186`.

**Что заявлено.** Тело коммита `fb7dac8`: «Review of the first attempt found that block dead
on every host (len == PAGE made the entry condition false at both 4 KiB and 16 KiB); **the
entry conditions are now hard asserts, so it cannot silently skip again**.»

**Что в коде.** Блок по-прежнему целиком под `if Reservation::decommit_reclaims_and_zeroes()`
(`:160`), которое `false` на всей Darwin-семье и под miri (`lib.rs:906-919`). На ряду
`ci.yml:861` (macos-latest, `cargo test -p aligned-vmem --features "... bench-internals"`)
всё тело decommit/recommit молча не исполняется — ровно тот failure mode, который коммит
объявляет закрытым.

Два assert'а, которыми заменили прежнее entry-условие, не могут упасть ни на одном хосте:

- `:164-167`: `decommit_end > decommit_start`, где `decommit_start = runtime_page_size`,
  `decommit_end = 2 * runtime_page_size` (`:161-162`) — это `2p > p` для `p > 0`;
- `:168-171`: `decommit_end <= reconstructed.len()`, где `len == 4 * runtime_page_size`
  (`:139`) — это `2p <= 4p`.

Это форма `assert!(6 > 3)` на двух локальных литералах, которую коммит `4a6c77e` в этой же
волне удалял как дефект.

**Рекомендация.** Убрать guard `:160` целиком: тело блока не проверяет ни reclaim, ни
zero-fill — только что `decommit` не паникует и `recommit` возвращает `true`, а это
корректно определённое поведение и на Darwin (`decommit` — advisory `madvise`,
`recommit_pages_impl` на Unix — no-op `Ok(())`). Тогда macOS-ряд получит реальное покрытие.
Тавтологические assert'ы заменить на содержательный
`assert_eq!(reconstructed.len(), 4 * runtime_page_size)`.

---

### F5 — LOW: ужесточение до точного равенства опирается на неспурьозность `compare_exchange_weak` — верно только на x86

**Где:**
- `tests/remote_ring_shadow_head.rs:249-252` (`assert_eq!(total, ROUNDS)`, favorable)
- `tests/remote_ring_shadow_head.rs:330-334` (`assert_eq!(total, ROUNDS)`, adversarial)
- `tests/remote_ring_shadow_head.rs:183-190` (`assert_eq!(fast_after, fast_before + 1)`)
- Механизм: `src/alloc_core/remote_free_ring.rs:1196-1227`

**Механизм.** `RemoteFreeRing::push` — это `loop { let t = tail.load(); full_check(t)?; ...
tail.compare_exchange_weak(...) }` (`:1196-1227`). `full_check` инкрементирует ровно один из
двух shadow-счётчиков на каждый свой вызов (`:1166`, `:1173`). При **spurious** отказе
`compare_exchange_weak` (`:1215-1220`) цикл повторяется и `full_check` вызывается второй раз
на тот же логический `push`. На LL/SC-архитектурах (aarch64) weak-CAS может провалиться без
всякой конкуренции — потеря exclusive monitor при прерывании, вытеснении или eviction
кэш-линии. То есть `total` там равно `ROUNDS` лишь *обычно*.

Прежние assert'ы были нижними границами (`total >= ROUNDS`, `fast_after > fast_before`) и
этот эффект переживали; после `8d68715` — точное равенство.

**Достижимость сегодня — нет (проверено).** Оба счётчико-читающих теста требуют
`feature = "bench-internals"` (`:149`, `:209`), которого нет ни в `production`
(`Cargo.toml:413`), ни в `experimental` (`Cargo.toml:123`). Все aarch64-ряды
(`ci.yml:1740-1756`) и macOS-ряд (`ci.yml:846`) используют только эти два бандла, значит
тесты там не компилируются. Ряды, где `bench-internals` включён (root-джоб `test`,
`--all-features`, и локальный `npm run check`), — x86_64, где LLVM опускает weak-CAS в
`lock cmpxchg`, который спурьозно не отказывает. Под miri тест тоже не гоняется
(`scripts/miri.mjs` его не содержит; miri как раз симулирует спурьозные отказы weak-CAS).

**Сценарий отказа (латентный).** Добавить в `ci.yml` aarch64-ряд с `bench-internals`
(или просто прогнать `cargo test --all-features` на Apple Silicon) → тест начнёт падать с
`total == 2001` при `ROUNDS == 2000`. Карточка `docs/CORRECTNESS_OPEN_ITEMS.md:2286-2289`
утверждает «the exact count is now trustworthy» без этой оговорки.

**Рекомендация.** Достаточно либо (а) записать в комментарий теста и в карточку item 63,
что точное равенство валидно только на strong-CAS-таргетах, либо (б) вернуть узкий
допуск: `assert!(total >= ROUNDS && total <= ROUNDS + CAS_RETRY_SLACK)` с `CAS_RETRY_SLACK`
порядка 8. Это сохраняет всё, ради чего делалось ужесточение: исходный отказ был
искажением *отношения* посторонним тестом на 256 отсчётов, а не ±несколькими ретраями CAS.

---

### F6 — LOW: некогерентный комментарий в favorable-блоке

**Где:** `tests/remote_ring_shadow_head.rs:245-248`.

Текст: «(the guard does not protect against the adversarial regime block below, which is in
the same test and runs after this block)». Adversarial-блок (`:271-348`) — это
последовательный код в том же потоке, выполняющийся ПОСЛЕ того, как `fast_delta`/`slow_delta`
favorable-блока уже вычислены (`:242-244`). Он физически не может внести вклад в эти дельты,
и «защищать» от него нечего. Комментарий читается как признание оставшейся гонки, которой нет.

**Рекомендация.** Удалить скобочную оговорку.

---

### F7 — MEDIUM: два follow-up'а коммита `66b8508` не попали ни в один индекс

**Где:** `docs/CORRECTNESS_OPEN_ITEMS.md` (отсутствуют), тело коммита `66b8508`.

Коммит `66b8508` явно пишет: «Two follow-ups this commit does NOT do, **recorded so they are
not lost**: (1) the new group sits AFTER the four root test rows, so a root-test failure masks
it …; (2) there is no `cargo test -p aligned-vmem` row with DEFAULT features …». Записаны они
только в сообщении коммита. Аналогично `c08abcf` пишет «Filed as an [audit-vmem] follow-up
rather than fixed here» про отсутствие `-p aligned-vmem` в `npm run check` — карточки нет.

Проверено: `grep -n "check-all\|check-matrix" docs/CORRECTNESS_OPEN_ITEMS.md docs/perf/OPEN_ITEMS.md`
даёт только строки 129/143/2490/2618-2624 (R33-эпоха, про clippy-матрицу) и 692-733
в perf-индексе — ни одна не про этот gap.

**Почему это дефект, а не придирка.** CLAUDE.md, раздел «Phased delivery», описывает ровно
этот сценарий как уже случившийся (R22-3/task #354: «R19-1's commit message flagged a flaky
test and a clippy dead-code combo as follow-ups that then existed in NEITHER index») и
добавляет: «the in-session TaskList does not survive a session boundary, so a fresh session
inherits no memory of prior rounds' flagged-open items — these indexes do». Правило
«When a gate report / commit / review newly flags an open item, add it to the appropriate
index **in the same commit**» здесь не выполнено.

**Рекомендация.** Одна карточка в `docs/CORRECTNESS_OPEN_ITEMS.md` с current-state блоком,
покрывающая остаточный разрыв `npm run check` ⊄ CI: порядок шагов, отсутствующий
default-feature test-ряд, и незаявленный ряд из F8.

---

### F8 — LOW: список «Deliberately NOT covered locally» в `66b8508` неполон; шапка `check-all.mjs` не обновлена

**Где:** `scripts/check-all.mjs:162-196` (новая группа), `scripts/check-all.mjs:16-69` (шапка).

**(а) Незаявленный непокрытый ряд.** Коммит перечисляет как сознательно непокрытые: mock-ряд
clippy, mock-ряд test, miri-cfg check, i686-кросс-ряды; и как follow-up — default-feature
`cargo test -p aligned-vmem`. Не назван и не покрыт ещё один:
`.github/workflows/ci.yml:972` — `cargo test -p aligned-vmem --features "fault-injection lazy-commit"`.
Это тот самый ряд, чей собственный комментарий (`ci.yml:952-971`) фиксирует, что
`fault_injection.rs` «had therefore never executed in ANY CI configuration until task #699».
Комбинация `fault-injection lazy-commit` без `huge-pages`/`bench-internals` не линтуется и не
тестируется ни локально, ни в clippy-джобе CI — только этим одним test-рядом.

Итого из четырёх `-p aligned-vmem` test-рядов CI (`ci.yml:899`, `:942`, `:950`, `:972`)
локальная группа воспроизводит один (`--all-features`).

**(б) Оценка порядка (пункт 4 задания).** Группа стоит после четырёх корневых
`cargo test`-рядов (`:133-161`), а цикл `:290-301` — fail-fast с `break`. Практический эффект
ограничен: маскировка временная (разработчик чинит корневой тест и перезапускает, после чего
vmem-ряды выполняются), но она противоречит принципу упорядочивания, который этот же файл уже
реализует — дешёвые проверки (fmt, clippy) до дорогих тестов. С учётом того, что в
`docs/CORRECTNESS_OPEN_ITEMS.md` уже пять карточек про флейки корневых тестов (items 37, 38,
39, «Recently resolved» 1 и 4, плюс новый item 63), вероятность именно случайной маскировки
не пренебрежима. Дешёвое исправление — перенести шесть vmem-шагов перед блоком `cargo test`.

**(в) Шапка не обновлена.** Блок `scripts/check-all.mjs:16-69` («What it runs, in order»,
явно помеченный «Step numbers below are kept in sync with the actual runtime step list by
hand») шести новых шагов не содержит, из-за чего нумерация 12–19 в шапке теперь смещена на
шесть относительно фактического массива. Это тот же дефект, который сама шапка документирует
как finding F5 из `docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`
(«a numbering drift here (a doubled "7" …)»). Рантайм-баннер (`:287`) обновлён корректно.

**Рекомендация.** Добавить ряд `cargo test -p aligned-vmem --features "fault-injection lazy-commit"`
(или явно внести его в список непокрытого), перенести группу выше test-рядов, обновить шапку
`:16-69`.

---

### F9 — LOW: `from_raw_parts` после R5-2 всё ещё содержит два взаимно противоречащих утверждения о `reservation_len`

**Где:** `crates/vmem/src/lib.rs:1303-1322`.

- `:1303-1308`: «`reservation_len` on Unix and under miri **MUST be the full length of the
  underlying OS mapping/allocation** — … miri's `release` passes it as the exact `Layout` size
  to `dealloc`, so an undersized value leaks memory (Unix) **or is undefined behavior (miri)**».
- `:1312-1322`: «`reservation_len` **may under-report** the actual OS mapping size … This is
  **harmless for correctness** (`munmap` rounds its length argument up the same way;
  `VirtualFree(MEM_RELEASE)` ignores the length on Windows)».

Второе утверждение обосновывает «harmless» только для `munmap` и `VirtualFree` и никак не
оговаривает miri-путь, который первое утверждение называет UB. Заявленная цель коммита
`fb7dac8` — «fixing the documentation to **one coherent model**» — на этой оси не достигнута.

**Сценарий.** Реально недостижимо для значений, произведённых самим крейтом (под miri
`reserve_aligned` выделяет и освобождает по одному и тому же `Layout`), поэтому LOW. Но
внешний вызывающий (тот самый cross-crate handoff, ради которого `from_raw_parts` и
существует — `lib.rs:1274-1281`), читающий `:1312-1322` на 16 KiB-хосте, получает
разрешение передать логическую длину, которое `:1303-1308` называет UB.

**Рекомендация.** Ограничить область «harmless» нативными Unix/Windows release-путями и
явно написать, что под miri `reservation_len` обязан совпадать с размером `Layout`,
которым выделялась память.

---

### F10 — LOW: item 63 нарушает структурное правило индекса (CLOSED с полным нарративом в активном списке)

**Где:** `docs/CORRECTNESS_OPEN_ITEMS.md:2259-2314`.

Карточка заведена с `Status: CLOSED` (`:2280`) и полным закрывающим нарративом **внутри
нумерованного активного списка**, тогда как установленный в этом же файле паттерн —
однострочный указатель плюс нарратив в «## Recently resolved»: см. item 52 (`:2168`) и
item 53 (`:2170`), оба сведённые к «**CLOSED** — see "Recently resolved" below for the full
closure narrative». CLAUDE.md (правило R34-24) требует именно этого: «when an item is closed,
its full closure narrative moves to … its "Recently resolved" section; the main index keeps
only a one-line pointer».

Дополнительно, в Evidence (`:2296-2298`) сказано «the **unfavorable** test's ROUNDS were
polluted by the adversarial test's concurrent execution» — теста с таким именем нет; в файле
два *режима* внутри одного теста, favorable и adversarial (`tests/remote_ring_shadow_head.rs:218`,
`:267`).

**Рекомендация.** Свернуть `:2259-2314` до однострочного указателя, нарратив перенести в
«Recently resolved» (после `:2317`); заменить «unfavorable test» на «favorable regime block».

---

### F11 — LOW: дублирующийся номер item 30 в `docs/perf/OPEN_ITEMS.md`

**Где:** `docs/perf/OPEN_ITEMS.md:405` и `docs/perf/OPEN_ITEMS.md:459`.

Два РАЗНЫХ активных item'а в тире `[D] Deferred designs` (секция `:135-1135`) носят один
номер **30**:

- `:405` — «R828 — sefer-region structural levers (P-perf-1/2/4/5)»
- `:459` — «R14-5 — `large-cache-extended` CONDITIONAL-GO, not promoted…»

(Проверено также: пары `1/2/3` — это преамбула `:16-26` против item'ов `:91/:137/:146`,
конфликта нет; `34` на `:1386` против `:2214` — активный против «Recently resolved», это
санкционированное переиспользование номеров в закрывающем следе.)

Дефект **преждесуществующий**, этой волной не внесён, но задание прямо просит искать
дублирующиеся номера, и волна (`a980618`) как раз работала с нумерацией этого файла.
Практический вред: любая внешняя ссылка «perf item 30» неоднозначна.

**Рекомендация.** Перенумеровать более позднюю карточку в первый свободный номер и
поправить входящие ссылки.

---

### F12 — INFO: сообщение коммита `4a6c77e` описывает `else`-ветку, которой в изменённом тесте нет

**Где:** `crates/vmem/tests/smoke.rs:1708-1741` против тела коммита `4a6c77e`.

Коммит пишет: «A huge-page arm runs when the OS actually grants huge pages, **with an else
arm asserting instance == platform** so the test is not vacuous on a runner without a hugetlb
pool». В `smoke.rs` huge-ветка (`:1712-1741`) не содержит ни `if actually_granted { } else { }`,
ни какой-либо сверки instance-запроса с platform-запросом — она безусловно проверяет
`full_parts_huge.granted_huge == r_huge.is_huge()` и сохранение значения после
`into_reservation()`. Описанная структура — это тест из ДРУГОГО файла и другого коммита
(`crates/vmem/tests/decommit_capability.rs:103-111`, коммит `5e389e3`).

Фактический результат при этом корректен: тест невакуумен и без hugetlb-пула (assert'ы
исполняются в любом случае). Запись только для протокола — сообщение коммита является
частью доказательной базы кампании и здесь описывает не тот код.

---

### F13 — INFO: конвенция «счётчик перечислен в четырёх местах» держится вручную и уже частично нарушена для старых счётчиков

**Проверка нового счётчика — пройдена.** `windows_large_page_alignment_failures` присутствует
во всех четырёх местах: `crates/vmem/CHANGELOG.md:32`, `crates/vmem/Cargo.toml:109-115`,
module-секция `crates/vmem/src/lib.rs:216-224`, док reset-функции `crates/vmem/src/lib.rs:565-575`.
Число словом («thirteen», `:565`) совпадает с фактическими 13 вызовами `store(0, ...)`
(`:579-592`) — проверено подсчётом.

**Остаточные (преждесуществующие) пропуски:**
- `crates/vmem/Cargo.toml:89-128` нигде не называет по имени `unix_exact_reserve_attempts()`,
  `unix_exact_reserve_hits()` и `windows_reserve_commit_calls()` (первая пара описана только
  прозой «a Unix hit/total pair around `try_reserve_aligned_exact`»).
- module-секция `crates/vmem/src/lib.rs:176-228` не упоминает `HUGE_DECOMMIT_ATTEMPTS`.

**Рекомендация.** Тело коммита `88592d7` само отмечает, что это «the R3-6 gap, already fixed
twice» — то есть трижды за кампанию. Дешёвый механический guard: расширить
`scripts/vmem-doc-drift-guard.mjs` (он уже в `npm run check`, шаг `check-all.mjs:271-283`)
проверкой: собрать имена `pub fn <name>() -> u64` под `#[cfg(feature = "bench-internals")]`
из `lib.rs` и потребовать, чтобы каждое встречалось в `CHANGELOG.md`, `Cargo.toml` и в доке
`reset_bench_internals_counters`, а число `store(0` совпадало с числительным в этом доке.

---

## Проверено, дефектов не найдено

Перечислено явно, чтобы отличить «не смотрел» от «смотрел и чисто».

1. **Doctest'ы в `crates/vmem/src/lib.rs` (пункт 1 задания) — чисто.** Все шесть fence'ов —
   `:59`/`:75` (module-doc) и `:970`/`:974`, `:977`/`:985` (пример в
   `can_decommit_reclaim_and_zero`) — открываются как ` ```text `. Runnable-fence'ов
   (` ```rust `, ` ```no_run `, ` ```compile_fail `, голый ` ``` `) в файле нет. Заявление
   коммита `5e389e3` о конвертации `no_run` → `text` подтверждается.

2. **Полнота SERIAL-guard'а в `tests/remote_ring_shadow_head.rs` (пункт 5) — чисто.**
   Все три `#[test]` в этом бинарнике берут guard первой строкой тела (`:84`, `:152`, `:212`),
   в том числе не читающий счётчики `shadow_stale_low_never_causes_spurious_admit`.
   Внешнего пути изменения счётчиков нет: `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`
   инкрементируются только из `RemoteFreeRing::full_check`
   (`src/alloc_core/remote_free_ring.rs:1166`, `:1173`), а `#[global_allocator]` в
   `src/` и `tests/` отсутствует (проверено `grep -rn '^#\[global_allocator\]'` — совпадения
   только в `examples/` и `crates/vmem/tests/mock_reentrancy.rs`, другие бинарники), так что
   аллокатор в этом тест-бинарнике не установлен и посторонних `push` нет. Poison-tolerant
   форма `lock().unwrap_or_else(|e| e.into_inner())` применена во всех трёх местах.

3. **Арифметика ужесточённых порогов — детерминирована, ложных падений не даст (на x86).**
   `RING_CAP = 256` (`src/alloc_core/remote_free_ring.rs:428`).
   Favorable: `cached_head` начинается с 0 и обновляется slow-путём при
   `t - ch >= 256`, т.е. на `t = 256, 512, …, 1792` → ровно 7 slow из 2000 → 99.65 %,
   при пороге 99.0 (`:261`). Adversarial: после заполнения `RING_CAP` каждый раунд
   `dbg_advance_head_only` двигает head на 1, не трогая shadow, поэтому
   `t - ch == RING_CAP` на каждом push → slow = 2000 при пороге `ROUNDS - 10 = 1990`
   (`:342-347`). Оба `assert_eq!(total, ROUNDS)` держатся, потому что `drain` не зовёт
   `full_check`, а CAS в однопоточном режиме на x86 не даёт ретраев (единственный
   остаточный риск — F5, на LL/SC-таргетах, сегодня недостижим).

4. **Контрфакт `round_trip_contract.rs::into_full_parts_round_trip_satisfies_contract`
   реален.** Если вернуть assert в `from_raw_parts` к требованию кратности
   `page_size()` (то, что пыталась сделать task #1019), тест упадёт на macos-arm64
   (`ci.yml:841`, image с 16 KiB страницами): `reserve_aligned(PAGE, PAGE)` даёт
   `len == reservation_len == 4096`, а `page_size() == 16384`. То есть Tier-1/Tier-2
   разделение (`:56-96`) — не декоративное.

5. **R5-4 не сломал сохранившийся сайт `WINDOWS_LARGE_PAGE_RETRY_FAILURES`.** Инкремент
   остался ровно в ветке «оба `VirtualAlloc` вернули NULL» (`lib.rs:2589-2593`), что в
   точности соответствует новой формулировке дока (`lib.rs:304-309`). Сам счётчик и его
   accessor не тронуты.

6. **Feature-gating из `c08abcf` корректен.** `MIB` (`decommit_capability.rs:20`) разгейчен
   в `5e389e3` обоснованно: он используется ungated-тестом `:127`. `SERIAL` (`:17-18`) остался
   под `bench-internals` и используется обоими gated-тестами (`:181`, `:247`). Комбинация
   `bench-internals` без `huge-pages` компилируется (гейт `:175` — `all(...)`), и именно она
   добавлена в локальный гейт (`check-all.mjs:176-180`), хотя в CI её нет.

7. **Мок-бэкенд не ломает Windows-арм `bench_internals_counters.rs`.** Под
   `--cfg aligned_vmem_mock` `try_reserve_aligned` (`lib.rs:1732-1758`) по-прежнему зовёт
   реальный `reserve_aligned_raw`, поэтому `windows_reserve_commit_calls() >= 1` (`:75-79`)
   держится и на mock-ряду `ci.yml:828-830`.

8. **Дубликатов номеров в активных тирах `docs/CORRECTNESS_OPEN_ITEMS.md` нет.** Пары
   49/52/53 — это санкционированный паттерн «активная строка-указатель + нарратив в
   Recently resolved». Пропуска номера 62 нет (карточка на `:111`, в другой секции).

9. **`docs/perf/OPEN_ITEMS.md` item 49 (коммит `a980618`) актуален против кода волны** —
   называет оба счётчика, оба сайта инкремента, оба accessor'а и новый тест-файл. Ошибка в
   нём одна и она унаследована из `lib.rs` — см. F1 (карточка не повторяет фразу «should be
   zero», но и не предупреждает о доминирующей популяции).

---

## Итоговая сводка

| # | Severity | Кратко |
|---|----------|--------|
| F1 | MEDIUM | Доки `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` описывают режим, который счётчик не выделяет; на непривилегированном Windows растёт штатно |
| F2 | MEDIUM | `bench_internals_counters.rs` вакуумен на Linux/macOS; заголовочный док утверждает обратное |
| F3 | MEDIUM | Новая ветка `&& !self.is_huge()` (R5-1) не покрыта исполняемым assert'ом ни на одном CI-ряду; `if let Some` без `else` |
| F7 | MEDIUM | Два follow-up'а `66b8508` и один `c08abcf` не попали ни в один индекс (сценарий R22-3 из CLAUDE.md) |
| F4 | LOW | `reconstructed_reservation_is_actually_usable` молча пропускается на macOS; два «hard assert»-а тавтологичны |
| F5 | LOW | Точное равенство счётчиков опирается на неспурьозность `compare_exchange_weak` (валидно только на x86; латентно) |
| F6 | LOW | Некогерентный комментарий про guard в favorable-блоке |
| F8 | LOW | Список непокрытого в `66b8508` неполон (`fault-injection lazy-commit`); шапка `check-all.mjs` не обновлена, нумерация смещена |
| F9 | LOW | `from_raw_parts`: «MUST be the full length … UB (miri)» против «may under-report … harmless» |
| F10 | LOW | Item 63 закрыт полным нарративом в активном тире вместо «Recently resolved»; «unfavorable test» |
| F11 | LOW | Дублирующийся номер item 30 в `docs/perf/OPEN_ITEMS.md` (:405 и :459) |
| F12 | INFO | Сообщение `4a6c77e` описывает `else`-ветку, которой в изменённом тесте нет |
| F13 | INFO | Конвенция «четыре места» держится вручную; предложен механический guard |

**HIGH 0 · MEDIUM 4 · LOW 7 · INFO 2**
