# `aligned-vmem` — аудит третьей волны closing-review фиксов (R3)

Дата: 2026-08-16. Режим: **только чтение**. Ни один файл крейта, CI-workflow
или индекса не изменён; создан только этот отчёт. `cargo test`/`clippy`/
`cargo doc` не запускались — все выводы получены статическим чтением кода,
диффов и `git show` по историческим ревизиям. Единственная исполненная команда —
read-only `node scripts/vmem-doc-drift-guard.mjs` (exit 0), поскольку волна
трогала ровно ту прозу, которую этот guard стережёт.

## Область

Диапазон диффа: `a088a0c..HEAD` по `crates/vmem/`, `.github/workflows/ci.yml`,
`docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/perf/OPEN_ITEMS.md`. Семь коммитов
(порядок применения — снизу вверх по `git log`):

| SHA | Batch | Заявленный охват |
|---|---|---|
| `b26d32b` | C | README MIPS/musl target-matrix (C-3, C-7) |
| `fcb96ba` | B | зачистка «align<=64KiB» large-page threshold (C-2, C-8) |
| `0a2f396` | E | висячая скобка decommit-контракта + устаревший SAFETY (C-11, C-16) |
| `0d875f6` | G | INFO-гигиена (C-15, C-17, C-18, C-19) |
| `dddb7c2` | D | мис-цитаты OPEN_ITEMS, номера finding'ов в ci.yml, item 52/53 (C-5, C-6, C-9, C-10) |
| `c695f7a` | F | тестовая гигиена (C-12, C-13, C-14) |
| `c0c52d1` | A | HIGH CI-блокер + flaky Windows-тест (C-1, C-4) |

Дополнительно прочитано целиком текущее состояние: `crates/vmem/src/lib.rs`,
`crates/vmem/tests/huge_pages.rs`, `crates/vmem/tests/lazy_commit.rs`,
`crates/vmem/README.md`, `crates/vmem/CHANGELOG.md`, `crates/vmem/Cargo.toml`,
`.github/workflows/ci.yml` (job `aligned-vmem` gates), `docs/CORRECTNESS_OPEN_ITEMS.md`
(items 48–55 + хвост «Recently resolved»), `docs/perf/OPEN_ITEMS.md` (item P2-4).
Зоны конфликтного мержа (`huge_pages.rs` — batches A+F; `lib.rs` — A/B/E/F/G;
`ci.yml` — D/G) читались в финальном состоянии, а не по диффу.

## Итог

**Оба обязательных фикса — C-1 и C-4 — сделаны правильно.** Это главный
результат: HIGH-находка действительно закрыта (проверено против фактического
кода `unix_reserve`, включая mock-строку Linux CI, которую предыдущий отчёт не
рассматривал), а Windows-флейк действительно устранён логически неопровержимой
формой ассерта `single_calls + two_call_pairs == 1`. Мерж двух коммитов, которые
оба трогали `huge_pages.rs`, ничего не потерял и не откатил. Ни одного нового
`unsafe` без `// SAFETY:`, C-15 добит полностью (в файле не осталось ни одной
пары подряд идущих `// SAFETY:`), doc-drift-guard зелёный.

Найдено **14 дефектов**: 3 MEDIUM, 5 LOW, 6 INFO. Ни одного HIGH и ни одного
CI-блокера. Тем не менее у волны есть один устойчивый мотив, который стоит
назвать прямо: **три из семи коммитов содержат в commit-message утверждение,
которого их собственный дифф не подтверждает** — batch D заявляет «verified
every one of the nine new citations ... all nine match exactly» (ни одна из
девяти не совпадает даже на своём собственном коммите), batch F заявляет
«adapting the comment for this file» (комментарий скопирован дословно и
описывает чужой cfg), batch B озаглавлен «finish cleaning up» (осталось ещё
минимум пять сайтов, один из которых — ровно та же фраза в той же функции).
Плюс batch E закрыл один из двух экземпляров одного и того же ложного
SAFETY-утверждения.

---

## Findings

### MEDIUM

#### R3-1. Фикс C-1 корректен на 64-bit, но на 32-bit Linux/Android `assert_eq!(attempts, 1)` ложен по построению — и это доказывает doc-комментарий, добавленный в этой же волне

`crates/vmem/tests/huge_pages.rs:225-228` (batch A) утверждает безусловно:

```rust
assert_eq!(
    attempts, 1,
    "2 MiB shape must increment UNIX_EXACT_RESERVE_ATTEMPTS; ..."
);
```

а собственный комментарий теста двумя строками выше (`huge_pages.rs:220`)
формулирует это как факт: *«so attempts is **always** 1, but hits may be 0»*.

Механика на 32-bit хосте с `huge-pages` и `nr_hugepages == 0`:

1. `unix_reserve` (`crates/vmem/src/lib.rs:2707`) входит в huge-блок
   (`huge && align == LINUX_HUGE_PAGE_SIZE`) и инкрементирует
   `UNIX_EXACT_RESERVE_ATTEMPTS` (`:2710`) → **1**;
2. `libc_mmap(size, true)` возвращает null, блок проваливается вниз (`:2736-2738`);
3. следующий же блок — `#[cfg(target_pointer_width = "32")] if let Ok(..) =
   try_reserve_aligned_exact(size, align, huge)` (`:2743-2748`) — вызывает
   функцию, которая инкрементирует **тот же** счётчик первой же строкой тела
   (`:2858-2859`) → **2**;
4. эта попытка тоже проваливается (тот же `MAP_HUGETLB`), управление уходит в
   общий over-reserve путь, который успешно возвращает обычные страницы;
5. тест входит в ветку `Ok(r)` и падает на `assert_eq!(attempts, 1)` с
   `attempts == 2`.

Это не гипотеза: **ровно этот сценарий описан batch G** в doc-комментарии,
добавленном к самому счётчику в этой же волне (`crates/vmem/src/lib.rs:239-247`):

> **Double-increment caveat (32-bit Linux/Android only):** … a single logical
> `reserve` call can increment this counter twice: once when the huge-page
> exact-size fast path is tried (in `unix_reserve`) and falls through, and again
> when execution reaches the 32-bit `try_reserve_aligned_exact` fast path
> immediately below.

То есть волна одновременно (a) документирует, что счётчик на 32-bit считает
дважды, и (b) добавляет тест, который жёстко утверждает, что он считает один
раз, для той же самой формы (`align == LINUX_HUGE_PAGE_SIZE`, `huge = true`) —
единственной, для которой каveat вообще применим.

**Почему не HIGH и не CI-блокер:** тест гейтится `#[cfg(target_os = "linux")]`,
а единственные 32-битные строки CI — `cargo check --target
i686-unknown-linux-{gnu,musl} --all-targets` (`.github/workflows/ci.yml:199-204`),
которые компилируют, но не исполняют тесты. Значит, сегодня это латентный
дефект: он выстрелит у любого, кто запустит `cargo test` на i686/armv7 Linux —
т.е. ровно у той аудитории, ради которой II-14 добавляло 32-битное покрытие.

**Что нужно:** либо `assert!(attempts >= 1)`, либо разделить по
`target_pointer_width` (`== 1` на 64-bit, `<= 2` на 32-bit) со ссылкой на
double-increment caveat, чтобы два места волны перестали противоречить друг
другу.

#### R3-2. C-16 закрыт наполовину: то же самое ложное SAFETY-утверждение осталось на 2 сайтах, один из которых — в той же функции, тремя строками выше исправленного

Batch E (`0a2f396`) в commit-message пишет:

> C-16: `winapi_virtual_decommit`'s SAFETY comment claimed `VirtualFree` with
> `MEM_DECOMMIT` is safe "for any address/len within a committed region" …
> Corrected the comment to state the real contract.

Исправлен один экземпляр (`crates/vmem/src/lib.rs:2579-2580`, теперь корректно
говорит «within a `MEM_RESERVE`d region; decommitting an already-uncommitted
sub-range is a defined, safe no-op»). Но:

* `crates/vmem/src/lib.rs:2564` — **первая строка тела той же функции**:
  ```rust
  unsafe fn winapi_virtual_decommit(addr: *mut u8, len: usize) {
      // SAFETY: caller guarantees `[addr, addr+len)` is within a committed region.
  ```
  Это функционально контракт функции (единственный `// SAFETY:` в позиции
  «предусловие вызывающего»), и он утверждает ровно то, что C-16 объявил
  неверным. Проверено против `a088a0c` — строка не менялась вообще.
* `crates/vmem/src/lib.rs:2407-2408` — единственный вызывающий,
  `decommit_pages_impl`: *«SAFETY: caller guarantees `[base+start, +len)` is
  within a **committed** reservation»*. Именно этот вызов и делает
  `safe_decommit_over_never_committed_tail_succeeds` (`tests/lazy_commit.rs:583`)
  над never-committed хвостом — тест, который batch E цитирует в собственном
  commit-message как доказательство ложности утверждения.

Итог: в цепочке из трёх SAFETY-комментариев, описывающих одну операцию,
исправлен средний; крайние продолжают утверждать предусловие, которое крейт
намеренно нарушает в CI-покрытом тесте. Для `unsafe fn` это не косметика —
внешний читатель `# Safety`/`// SAFETY` реконструирует контракт именно отсюда.

#### R3-3. Batch D утверждает, что сверил девять новых цитат с файлом — ни одна из девяти не совпадает даже на его собственном коммите

Commit-message `dddb7c2`:

> Replaced all nine with symbol names (function names, primarily) plus a current
> line number as a convenience … **Verified every one of the nine new citations
> against the actual file before committing — all nine match exactly.**

Фактически номера соответствуют **`a088a0c`** — базе ДО волны, а не `dddb7c2`,
поверх которого уже лежали batches C/B/E/G (последний добавил ~19 строк
doc-комментариев в начале `lib.rs`). Сверка (`git show <sha>:crates/vmem/src/lib.rs | grep -n`):

| Цитата в item 49/52/53 | Реально на `dddb7c2` | Реально на HEAD |
|---|---|---|
| `fn release_reservation` (Windows) «at line 2344» | 2363 | **2385** |
| `fn winapi_virtual_reserve` «at line 2514» | 2532 | **2554** |
| `fn winapi_virtual_decommit` «at line 2523» | 2541 | **2563** |
| `fn winapi_virtual_release` «at line 2546» | 2565 | **2593** |
| `fn release_reservation` (Unix) «at line 2870» | 2889 | **2917** |
| `fn libc_madvise` «at line 3429» | 3448 | **3476** |
| `fn madv_free_advice` «(line 2974)» | 2993 | **3021** |
| `pub unsafe fn from_raw_parts` «(line 1033…)» | 1052 | **1073** |
| конструктор `granted_huge` «(line 1118…)» | — | **1158** |

Смещение на HEAD — 40–47 строк; на собственном коммите — 18–19. Все девять
чисел точно равны значениям на `a088a0c`, что однозначно указывает: номера были
сняты со старого дерева и перенесены без пересчёта, а заявленная сверка не
проводилась.

Практический ущерб ограничен: batch D добавил рядом **имена символов**, и именно
имена делают цитату восстановимой (это и было рекомендацией C-5 — «заменить
номера строк именами символов, а не поправлять номера»). Но фикс C-5
воспроизвёл дефектный класс C-5 внутри самого себя, а commit-message
зафиксировал верификацию, которой не было. Для кампании, где «An agent's
statement is a claim, not a receipt» — прямое правило CLAUDE.md, это стоит
MEDIUM.

**Что нужно:** либо удалить числовые номера и оставить только имена символов
(они самодостаточны — `git grep 'fn winapi_virtual_decommit'` находит сайт за
одну команду), либо пересчитать и добавить в фикс-пасс шаг «пересчитать после
мержа всех батчей», поскольку любой номер, снятый до мержа, устаревает
детерминированно.

---

### LOW

#### R3-4. «Finish cleaning up the stale align<=64KiB threshold» не закончено: осталось ≥5 сайтов, один из них — та же фраза в той же функции

Batch B (`fcb96ba`) исправил три сайта (README:67, Cargo.toml:98, lib.rs:1996)
и озаглавлен «finish cleaning up». В `crates/vmem/src/lib.rs` осталось:

* **`:2375-2377`** — самый заметный:
  ```
  // task #921/V-7: the two-call path never requests MEM_LARGE_PAGES ...
  // Only the single-call fast path (align <= WIN_ALLOCATION_GRANULARITY)
  // can grant huge pages.
  ```
  Это буквально то предложение, которое batch B переписал на `:1996-1999`
  («For large-page requests, the fast-path condition is `align <=
  GetLargePageMinimum()`…»), и оно находится **внутри `win_reserve_commit`** —
  функции, чей собственный порог вычисляется на `:2176-2183` как
  `if extra_commit_flags != 0 { GetLargePageMinimum() } else {
  WIN_ALLOCATION_GRANULARITY }`. Читатель кода получает опровергнутый ответ на
  расстоянии 200 строк от самого кода, который его опровергает.
* **`:286`** — doc счётчика `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`: *«which the
  fast path uses when `align <= 64 KiB` and `commit_len == size`»*. Это тот
  самый счётчик, который `reserve_aligned_huge_2mib_still_two_call_path_unprivileged`
  (`huge_pages.rs:381`) читает для формы `align = 2 MiB`.
* **`:301`** — doc `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`: *«This path is used
  when `align > 64 KiB`»*.
* **`:2101`** — заголовочный rustdoc самой `win_reserve_commit`:
  *«**Single-call fast path** (`align <= WIN_ALLOCATION_GRANULARITY &&
  commit_len == size`)»* — при том, что `:2139-2145` двадцатью строками ниже
  описывает II-3-расширение.
* **`:195-199`** (module-level design comment, три упоминания) и **`:2266`**
  («Two-call path (align > 64 KiB, …)»).

Для `:195-199`, `:2266`, `:2101` формулировка описывает общий случай, где 64 KiB
всё ещё верны для ordinary-запросов — это спорная половина. Для `:2375-2377`,
`:286`, `:301` спора нет: они говорят про large-page-грант и про счётчики,
которые считают large-page fast path.

#### R3-5. Комментарий к новому `#![allow(clippy::let_unit_value)]` скопирован дословно из `lazy_commit.rs` и описывает cfg, которого в `huge_pages.rs` нет

`crates/vmem/tests/huge_pages.rs:35-40` (batch F):

> `serial_guard()` returns a real `MutexGuard` on **Windows+bench-internals+
> non-mock** builds and `()` everywhere else …

Это байт-в-байт `crates/vmem/tests/lazy_commit.rs:8-13`, где условие
действительно `#[cfg(all(windows, feature = "bench-internals",
not(aligned_vmem_mock)))]` (`lazy_commit.rs:19`, `:59`). В `huge_pages.rs`
`serial_guard()` гейтится **только** на `#[cfg(feature = "bench-internals")]`
(`:74-79`), а `()`-вариант — на `#[cfg(not(feature = "bench-internals"))]`
(`:78-79`). То есть на Linux с `bench-internals` (единственная конфигурация, где
этот файл вообще читает счётчики — см. `SERIAL` на `:67-68`) `serial_guard()`
возвращает настоящий `MutexGuard`, а «Windows» и «non-mock» к условию
отношения не имеют вовсе.

Commit-message batch F заявляет «Added it, **adapting the comment for this
file**» — адаптации не произошло. Сам `allow` нужен и корректен; неверно
описание условия, ради которого он нужен.

#### R3-6. Новая публичная функция добавлена без записи в CHANGELOG и без упоминания в трёх местах, которые перечисляют счётчики

Batch F добавил `pub fn windows_virtualfree_decommit_attempts()`
(`crates/vmem/src/lib.rs:442-449`) — публичный (за `bench-internals`) API.
Перечисления счётчиков не обновлены:

* **`crates/vmem/CHANGELOG.md:18-23`** — раздел `### Added`, подсписок
  «`bench-internals` feature with diagnostic counters for path activation»:
  перечислены `unix_exact_reserve_*`, `windows_reserve_commit_*`,
  `unix_madvise_*`, `reset_bench_internals_counters()`, `validate_page_size()`.
  Нет ни новой `windows_virtualfree_decommit_attempts()`, ни
  `windows_virtualfree_decommit_failures()`/`unix_munmap_failures()` (эти два
  отсутствуют с более ранней волны P2-6). Крейт публикуется как 0.2.0, а у
  кампании есть собственная находка II-13 ровно про фактические ошибки в
  CHANGELOG.
* **`crates/vmem/Cargo.toml:90-115`** — doc-комментарий фичи `bench-internals`
  перечисляет accessor'ы поимённо; Windows-decommit-пара не упомянута вовсе.
  Batch B правил этот самый комментарий (`:98`) в этой же волне.
* **`crates/vmem/src/lib.rs:186-212`** — module-level секция «bench-internals»,
  которая по-строчно описывает каждую пару счётчиков (Unix exact-reserve,
  Windows reserve-commit, macOS madvise-оракул). Windows-decommit-счётчиков нет.
  При этом doc `reset_bench_internals_counters` (`:469-475`) корректно обновлён
  до «all nine counters» и перечисляет новый — т.е. одно из четырёх мест
  синхронизировано, три нет.

#### R3-7. Два соседних комментария в `ci.yml` теперь противоречат друг другу и по номеру finding'а, и по тому, что степ доказывает

`.github/workflows/ci.yml:196-198` (batch D):

```
# Finding II-14 (2026-08-16 closing review): extend coverage to tests and
# optional features, and add musl target to verify the UB-class fix for
# wrong FFI `off_t` type.
```

`.github/workflows/ci.yml:200-201` (batch G, тремя строками ниже):

```
# Finding II-1 (2026-08-16 closing review): add musl 32-bit target to compile-verify
# that the correct `off_t` arm is selected (gated `not(target_env = "musl")`).
```

Batch D исправил номер с II-3 на II-14, но оставил в том же предложении хвост
«and add musl target to **verify the UB-class fix**» — т.е. musl-степ по-прежнему
приписан II-14 и по-прежнему описан переоценивающей формулировкой «verify»,
которую batch G отдельно менял на «compile-verify» и переатрибутировал на II-1.
Оба фикса (C-6 и C-19) сделаны наполовину, и результат хуже исходного состояния:
раньше оба комментария врали одинаково, теперь они врут по-разному и
противоречат друг другу.

#### R3-8. Удаление musl-буллета из README потеряло `armv7-musl` и единственное обоснование корректности `off_t`

Batch C (C-7) удалил из «Reasoned-from-spec targets» весь буллет:

> **32-bit musl Linux** (e.g., `i686-unknown-linux-musl`,
> `armv7-unknown-linux-musleabihf`) — `off_t` is correctly declared as 64-bit
> (musl uses 64-bit `off_t` on all architectures); no CI runner currently tests
> these targets.

и добавил musl-таргет в CI-verified-буллет `crates/vmem/README.md:199`. Первая
половина правки верна (i686-musl действительно теперь compile-checked). Но
удалён был весь буллет, а не только его i686-часть:

* `armv7-unknown-linux-musleabihf` (и любой другой не-i686 musl) больше не
  упомянут нигде в README — при том, что он по-прежнему reasoned-from-spec, а
  не CI-verified, и `cargo check --target i686-unknown-linux-musl` его не
  покрывает (у ARM другой `target_arch`, хотя `OffT`-арм тот же);
* исчезло единственное в README объяснение, **почему** musl безопасен
  («musl uses 64-bit `off_t` on all architectures») — а это ровно тот факт,
  который II-1/M-2 добывали и вокруг которого построен двухарменный
  `type OffT` (`crates/vmem/src/lib.rs:3380-3388`). Новый CI-verified-буллет
  его не восстанавливает: он называет только команды.

---

### INFO

#### R3-9. Новый счётчик вклинился между `// SAFETY:` и его `unsafe`-блоком — воспроизведён класс дефекта, который уже чинили в этом файле (task #894/T7)

`crates/vmem/src/lib.rs:2579-2583` (batch F):

```rust
// SAFETY: `VirtualFree` with `MEM_DECOMMIT` is safe for any address/len within a `MEM_RESERVE`d region;
// decommitting an already-uncommitted sub-range is a defined safe no-op per the Windows API contract.
#[cfg(feature = "bench-internals")]
WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
let ret = unsafe { VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT) };
```

`// SAFETY:` теперь лексически приклеен к **безопасному** атомарному
инкременту, а сам `unsafe {}` не имеет прилегающего SAFETY-комментария. Это
ровно та мисатрибуция, которую task #894 (finding T7) уже чинил в этом файле
(«rustfmt gluing the `mmap` call's `// SAFETY:` comment onto the trailing
comment of the preceding unrelated `let _ = huge;` line») и которую до сих пор
цитирует заголовок item 49 в `docs/CORRECTNESS_OPEN_ITEMS.md:2146`. На
компиляцию и на soundness не влияет; лечится перестановкой инкремента на строку
выше комментария.

#### R3-10. `assert!(hits <= 1)` неопровержим по построению — информацию несут только два соседних ассерта

`crates/vmem/tests/huge_pages.rs:229-232`. Счётчики сбрасываются на `:201-202`
под удержанным `SERIAL` (`:198`), после чего делается ровно один
`try_reserve_aligned_huge` (`:203`). Единственный сайт инкремента `HITS`
(`crates/vmem/src/lib.rs:2724`) выполняется максимум один раз — сразу за ним
идёт `return Ok(...)` (`:2728`), а на 32-bit второй сайт (`:2905`) в этом
сценарии недостижим (huge-mmap уже провалился). Значит `hits <= 1` не может
упасть ни на одной платформе и ни при какой конфигурации хоста.

Как tripwire на будущий рефактор это дёшево и не вредно, и рекомендация C-1
допускала именно такую форму. Отмечено только потому, что окружающий
комментарий «PATH-ACTIVATION ORACLE» подаёт весь блок как доказательство, тогда
как доказывают только `assert_eq!(attempts, 1)` и условный `r.is_huge()`.

#### R3-11. Item 49: «eight call sites cited above» при шести процитированных

`docs/CORRECTNESS_OPEN_ITEMS.md:3468` (и идентично в исходном тексте до
переноса) — Evidence: *«direct fixes in `crates/vmem/src/lib.rs` at the **eight**
call sites cited above and `crates/vmem/benches/vmem_bench.rs`'s `fault_pages`»*.
В нарративе выше перечислено **шесть** сайтов в `lib.rs`; ещё два (`base.add`,
`ptr::write_volatile`) — в `vmem_bench.rs` и названы в том же предложении
отдельно. Дефект унаследован дословно при переносе нарратива (batch D), а не
внесён им; отмечено потому, что коммит, который переписывал в этом абзаце все
девять цитат, был естественным местом это поправить.

#### R3-12. `docs/perf/OPEN_ITEMS.md`: строкой выше и строкой ниже отредактированного «Next trigger» остались устаревшие номера строк

Batch G корректно переписал «Next trigger» item'а P2-4 (счётчики на 64-bit
измеряют huge-путь, а не общий over-reserve). В том же item'е остались:

* «Current number/verdict»: *«The 32-bit exact-size fast path
  (`try_reserve_aligned_exact`, `crates/vmem/src/lib.rs:2550-2622`)»*;
* «Evidence»: `:2428-2437`, `:2550-2622`, `:2438-2527`.

Фактически `fn try_reserve_aligned_exact` на HEAD — `crates/vmem/src/lib.rs:2853`
(тело до `:2914`); даже на pre-wave базе `a088a0c` он был на `:2806`, т.е.
диапазон `2550-2622` устарел задолго до этой волны. Не внесено волной —
отмечено как незакрытая половина той же уборки, которую волна вела в соседнем
файле.

#### R3-13. Закрытые items 52/53 сохранили тег тира `[T, INFO]` внутри активной секции `### [T]`

`docs/CORRECTNESS_OPEN_ITEMS.md:2163`, `:2165`. Карточка обновлена до
`**CLOSED** — see "Recently resolved" below`, нарратив перенесён (C-10 выполнен),
но заголовок по-прежнему начинается с `**[T, INFO]**`, и обе заглушки физически
лежат в `### [T] Tracked, not yet actioned` (`:220`). CLAUDE.md: закрытый item
«must NOT look active due to a stale header, **tier placement**, or missing
Status-card update».

Смягчающее обстоятельство: у файла есть собственный прецедент ровно такой формы
— item на `:88` внутри секции `### [A]` тоже оставлен как `CLOSED, see "Recently
resolved" below`. Так что это вопрос конвенции, а не регресс; записано, чтобы
следующий раунд принял решение осознанно (снимать теги тира при закрытии или нет).

#### R3-14. CHANGELOG: выбранное направление фикса C-17 оставило файл несоответствующим объявленному формату

`crates/vmem/CHANGELOG.md`. Batch G удалил осиротевшее определение
`[0.1.0]: https://…`, оставив файл без единой ссылочной конструкции, тогда как
шапка (`:5`) заявляет *«The format is based on [Keep a Changelog]»*, чей формат —
`## [x.y.z]` с link-definition'ами. Обе половины (заголовки без скобок,
определений нет) внутренне согласованы между собой, так что выбор защитим; но
заявление о соответствии Keep a Changelog теперь буквально неверно, а C-17
называл именно эту двойственность. Побочно: после удаления файл заканчивается
пустой строкой (`:56-57`), т.е. `\n\n` на EOF.

---

## Проверено и подтверждено корректным (контр-findings)

1. **C-1 действительно исправлен на 64-bit — включая mock-строку Linux CI,
   которую предыдущий отчёт не рассматривал.** `try_reserve_aligned_huge` под
   `--cfg aligned_vmem_mock` НЕ подменяет backend: mock только записывает
   `Call::ReserveHuge` (`crates/vmem/src/lib.rs:2016-2022`) и всё равно
   вызывает `reserve_aligned_huge_raw`, чей Unix-арм (`:2996-3000`) — это прямой
   `unix_reserve(size, align, true)`. Значит `UNIX_EXACT_RESERVE_ATTEMPTS`
   честно доходит до 1 на ОБЕИХ Linux-строках (`ci.yml:180` `--all-features` и
   `ci.yml:184` `RUSTFLAGS="--cfg aligned_vmem_mock"`), а `hits` при
   `nr_hugepages == 0` остаётся 0 и больше не ассертится. Ассерт `attempts == 1`
   на 64-bit неопровержимо верен: 32-битный второй инкремент компилируется
   только под `target_pointer_width = "32"` (`:2743`).
2. **C-4 исправлен полностью и логически исчерпывающе.** В `win_reserve_commit`
   ровно два взаимоисключающих инкремента: `SINGLE_CALLS` (`:2252`, в
   `else`-ветке post-call alignment check, сразу перед `return Ok`) и
   `TWO_CALL_PAIRS` (`:2374`, после успешного `MEM_COMMIT`). Провалившийся
   fast-path releases и проваливается вниз без инкремента (`:2244-2249`).
   Поэтому `single_calls + two_call_pairs == 1` держится и в «совпало
   2 MiB-выравнивание», и в обычном случае — флейк устранён не смягчением, а
   переходом к инварианту, который верен всегда. `reserve_aligned_huge_4mib_still_two_call_path`
   (`huge_pages.rs:435-442`) справедливо оставлен со строгими `== 0`/`== 1`.
3. **Пустой `if single_calls == 1 { }`-блок, о котором говорит commit-message
   batch A, действительно удалён** — на его месте обычный комментарий
   (`huge_pages.rs:387-390`), никакого мёртвого условного блока.
4. **Новый `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS` разведён корректно.**
   Инкремент безусловный и стоит ДО syscall'а (`lib.rs:2582`), сброс добавлен в
   `reset_bench_internals_counters` (`:489` — в теле теперь ровно девять
   `store`, что совпадает с обновлённым doc «all nine counters»), accessor
   `:442-449` симметричен существующим. Ассерт `attempts_after == attempts_before + 1`
   (`lazy_commit.rs:615-619`) точен: `Reservation::decommit` → `decommit` →
   `decommit_pages_impl` делает ровно один вызов `winapi_virtual_decommit`
   (`lib.rs:2409-2410`, без циклов по страницам), baseline снимается ПОСЛЕ
   `reserve_aligned_lazy` (`lazy_commit.rs:568-572`), а `SERIAL` (`:548`) не даёт
   соседнему тесту вклиниться в дельту. Гейт `not(aligned_vmem_mock)` корректен:
   под mock `decommit` уходит в record-and-return и syscall'а нет.
5. **C-3 теперь фактически верен.** README-текст про MIPS (`README.md:210`)
   совпадает с внутрикодовым комментарием (`lib.rs:3059-3081`) дословно по
   существу: `0x20` на MIPS — это IRIX-совместимый `MAP_RENAME`, который Linux
   игнорирует; анонимный флаг не выставлен; `fd = -1` даёт `EBADF`;
   `mmap` возвращает `MAP_FAILED`; отказ — рантайм-овый и без диагностики.
   `compile_error!` (`:3100-3135`) гейтится на targets ВНЕ обоих
   `MAP_ANON`-армов, т.е. `mips*-linux` под него не попадает — что README
   теперь и говорит. cfg-пример исправлен на валидный
   `any(target_arch = "mips", target_arch = "mips64")`.
6. **C-15 добит полностью.** Программная проверка всего `lib.rs` на пары подряд
   идущих строк, обе содержащих `// SAFETY:`, даёт пустой результат — ни одного
   дублирующего сайта не осталось. Три склейки (`:2386-2388`, `:2555-2556`,
   `:2918-2920`) читаются как единые связные комментарии и сохранили
   нередундантную часть каждой из половин.
7. **Ни одного нового `unsafe`-блока волна не добавила**, и ни один
   существующий не остался без SAFETY-комментария (единственная претензия —
   позиционная, R3-9).
8. **C-14 (перегейтовка теста) корректна во всех четырёх релевантных
   конфигурациях.** Внешний гейт `#[cfg(target_os = "linux")]` (`huge_pages.rs:196`),
   `reset_bench_internals_counters()` и оракул — во внутренних
   `#[cfg(feature = "bench-internals")]` (`:201`, `:221`). Под
   `not(bench-internals)` биндинг `r` всё ещё используется (`:205` `r.as_ptr()`),
   так что unused-variable под `-D warnings` не возникает; `let _guard =
   serial_guard();` покрыт новым crate-level `allow`.
9. **C-9 закрыт**: Evidence item'а 52 больше не утверждает, что item 53 открыт —
   перенесённый нарратив (`docs/CORRECTNESS_OPEN_ITEMS.md:3472`) явно говорит
   «item 53 … is also now closed».
10. **C-11 корректен на обоих сайтах, и удаление предложения в свободной
    `decommit` ничего не потеряло.** `Reservation::decommit` (`lib.rs:859-874`)
    и свободная `decommit` (`:1508-1518`) больше не имеют висячей скобки; при
    этом предупреждение про обязательный `recommit` на Windows сохранено в
    гораздо более сильной форме в секции «Platform divergence» (`:1532-1541`,
    «a **write** … before [`recommit`] is a hard `STATUS_ACCESS_VIOLATION`
    crash»), на которую метод явно ссылается (`:872-874`).
11. **`node scripts/vmem-doc-drift-guard.mjs` на HEAD — exit 0**
    («no unconditional over-reserve/trim statements found»), т.е. правки прозы
    в `lib.rs` не пробили этот guard.
12. **Мерж-конфликты разрешены без потерь.** `huge_pages.rs` (batches A+F)
    содержит одновременно и восстановленный внешний гейт C-14, и новый
    crate-level `allow` C-13, и полную оракульную логику C-1 — ни одна правка
    не затёрта. `ci.yml` (batches D+G) содержит обе правки комментариев (см.
    R3-7 — они противоречат друг другу по смыслу, но обе физически на месте).
    `lib.rs` (A/B/E/F/G) содержит все пять наборов правок.

---

## Приоритет действий

1. **R3-1** — единственная находка с реальным сценарием падения теста. Дёшево:
   одна строка `#[cfg]` или замена `==` на `>=`. Стоит сделать до публикации,
   иначе первый же контрибьютор на 32-bit Linux получает красный `cargo test`
   от кода, чей собственный doc-комментарий объясняет, почему.
2. **R3-2** — два SAFETY-комментария на `unsafe fn`, утверждающие предусловие,
   которое крейт нарушает в CI-покрытом тесте. Механическая правка двух строк
   (`lib.rs:2564`, `:2407-2408`).
3. **R3-3, R3-4, R3-7** — мис-цитаты и незаконченные уборки. Для R3-3
   рекомендуется удалить номера строк совсем (имена символов уже есть) вместо
   очередного пересчёта, который устареет на следующем же коммите; для R3-7 —
   свести два соседних комментария в один.
4. **R3-5, R3-6, R3-8** — гигиена перед публикацией 0.2.0: чужой cfg в
   комментарии, недостающая строка CHANGELOG для нового `pub fn`, потерянный
   armv7-musl.
5. **R3-9 … R3-14** — по желанию; R3-9 стоит одной перестановки строк.
