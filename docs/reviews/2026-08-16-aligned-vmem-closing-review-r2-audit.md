# `aligned-vmem` — аудит второй волны closing-review фиксов (R2)

Дата: 2026-08-16. Режим: **только чтение**. Ни один файл крейта, CI-workflow
или индекса не изменён; создан только этот отчёт. Тесты, clippy, benchmarks и
`cargo doc` не запускались — все выводы получены статическим чтением кода и
диффов.

## Область

Диапазон диффа: `5403c95..HEAD` по `crates/vmem/`, `.github/workflows/ci.yml`,
`docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/perf/OPEN_ITEMS.md`. Пять коммитов:

| SHA | Batch | Заявленный охват |
|---|---|---|
| `ae0d89c` | K | CHANGELOG factual errors + doc bundle (II-13, misc) |
| `bcc0c51` | I | Darwin/BSD decommit contract + `is_huge()` doc-drift (P1-1, P1-2) |
| `3a25a48` | M | vacuous Windows test, backwards README claims, stale OPEN_ITEMS (II-17, P3-9) |
| `3fb7d31` | L | 32-bit CI coverage, edition-2024 unsafe debt, statics visibility (II-14, P3-8, P3-10) |
| `a088a0c` | J | II-3 empirically false + II-4 path-activation oracle (II-3, II-4) |

Дополнительно прочитано целиком текущее состояние: `crates/vmem/src/lib.rs`,
`crates/vmem/src/mock.rs`, `crates/vmem/Cargo.toml`, `crates/vmem/README.md`,
`crates/vmem/CHANGELOG.md`, `crates/vmem/tests/huge_pages.rs`,
`crates/vmem/tests/lazy_commit.rs`, `.github/workflows/ci.yml`,
`docs/CORRECTNESS_OPEN_ITEMS.md` (items 48–54).

## Итог

Волна в целом сделала то, что заявляла: II-3 действительно опровергнут
эмпирически (и это опровержение — самая ценная вещь во всей волне), вакуумные
тесты заменены на path-activation-оракулы, SERIAL-паттерн из `lazy_commit.rs`
корректно перенесён в `huge_pages.rs`, `pub` → `pub(crate)` для двух статиков
доведён до конца (ни одного `pub static` в `lib.rs` не осталось), а
edition-2024 `unsafe {}`-обвязка действительно закрывает все восемь названных
сайтов плюс два новых в бенче.

Но найдены **19 дефектов**, включая один HIGH, который с высокой вероятностью
делает `main` красным на Linux-строке CI, и семь MEDIUM. Три из них — прямые
рецидивы того самого класса, ради которого волна и существовала: новое
фактически ложное утверждение в README вместо старого ложного, шесть выдуманных
номеров строк в «Evidence» индекса, и один невычищенный комментарий, который
до сих пор утверждает ровно тот тезис, который batch J объявил опровергнутым.

---

## Findings

### HIGH

#### C-1. Новый path-activation-оракул на Linux требует реально выданных huge-страниц — тест падает на любом хосте без hugetlb-пула, включая `ubuntu-latest`

`crates/vmem/tests/huge_pages.rs:216-223` (batch J).

Тест `reserve_aligned_huge_exact_size_for_2mib_align` теперь жёстко утверждает

```rust
assert_eq!(attempts, 1, ...);
assert_eq!(hits,     1, ...);
```

внутри ветки `Ok(r)`.

Механика в `crates/vmem/src/lib.rs:2661-2692`: счётчик `ATTEMPTS`
инкрементируется **до** `libc_mmap(size, true)`, а `HITS` — только внутри
`if !p.is_null() { … if region_addr.is_multiple_of(align) { … } }`. При
`nr_hugepages == 0` (дефолт на GitHub-hosted `ubuntu-latest`)
`mmap(..., MAP_HUGETLB | MAP_HUGE_2MB, ...)` возвращает `MAP_FAILED` (ENOMEM),
`libc_mmap` отдаёт null, блок проваливается в общий over-reserve путь, который
ретраит `libc_mmap(over, false)` и **успешно** возвращает обычные страницы.
Итог: `try_reserve_aligned_huge` отдаёт `Ok`, тест входит в ветку `Ok(r)`, и
получает `attempts == 1, hits == 0` → `assert_eq!(hits, 1)` падает.

Это не гипотетика, а прямое противоречие двум местам в том же файле:

* собственный doc-комментарий теста (`huge_pages.rs:184-187`): *«Whether huge
  pages are actually granted depends on host configuration
  (`/proc/sys/vm/nr_hugepages`); the crate's best-effort fallback means this
  must succeed either way, so the test only asserts on the contract-violation
  guard not firing (**not on the actual huge-page grant**)»* — новый assert
  утверждает ровно actual huge-page grant;
* заголовок модуля (`huge_pages.rs:28-32`): *«no **hugetlb-configured** host
  runs these tests, so the `MAP_HUGETLB`-actually-succeeds branch … stays
  untested end to end»* — новый assert требует, чтобы эта ветка отработала.

Тест компилируется под `#[cfg(all(target_os = "linux", feature =
"bench-internals"))]`, а `.github/workflows/ci.yml:180` выполняет
`cargo test -p aligned-vmem --all-features` на `ubuntu-latest` — т.е. строка,
которая его запускает, в CI есть.

**Что нужно:** оракул должен утверждать `attempts == 1` безусловно (это
действительно path-activation: путь был *взят*), а `hits` — только условно
(`assert!(hits <= 1)`, либо `if hits == 1 { … }` с отдельной проверкой
`r.is_huge()`), либо тест должен уметь пропускать себя при
`nr_hugepages == 0`. Перед любым пушем стоит проверить фактический прогон CI на
landing-SHA — если Linux-строка уже красная, это она.

---

### MEDIUM

#### C-2. Комментарий в `reserve_aligned_huge_raw` до сих пор утверждает ровно тот тезис, который batch J объявил эмпирически ложным — с тем же примером «4 MiB», который опровергает его собственный новый тест

`crates/vmem/src/lib.rs:2453-2457`:

```
// This widening makes the Linux and Windows parameter spaces overlap
// (e.g., `reserve_aligned_huge(4 MiB, 4 MiB)` can now be huge on both
// platforms), resolving the disjointness issue where Linux required
// `align >= 2 MiB` but Windows only granted large pages for
// `align <= 64 KiB`.
```

Batch J (`a088a0c`) в собственном commit-сообщении пишет, что этот класс
утверждений «empirically false», исправляет **пять** rustdoc-сайтов и добавляет
тест `reserve_aligned_huge_4mib_still_two_call_path`
(`crates/vmem/tests/huge_pages.rs:382-421`), который утверждает противоположное:
`4 MiB > GetLargePageMinimum() = 2 MiB` ⇒ fast path даже не пытается запуститься
⇒ `single_calls == 0`. То есть `reserve_aligned_huge(4 MiB, 4 MiB)` **не может**
быть huge на Windows ни при каких условиях.

Это самая явная формулировка опровергнутого тезиса во всём крейте, и она
осталась нетронутой ровно в той функции, которая вызывает
`win_reserve_commit(size, align, size, MEM_LARGE_PAGES)`. Читатель кода,
дошедший сюда, получает противоположный ответ по сравнению с читателем rustdoc.

#### C-3. Новое MIPS-утверждение в README фактически ложно: `compile_error!` на MIPS **не** срабатывает

`crates/vmem/README.md:210` (batch M):

> A build on `mips*-unknown-linux-*` would fail closed at compile time via a
> `compile_error!` diagnostic explicitly naming this gap

Единственный релевантный `compile_error!` — `crates/vmem/src/lib.rs:3082-3088`,
и он гейтится (`:3071-3081`) через
`not(any(target_os = "linux", target_os = "android"))` **плюс** `not(any(darwin/BSD))`.
`mips*-unknown-linux-gnu` — это `target_os = "linux"`, поэтому он матчит арм
`#[cfg(all(unix, not(miri), any(target_os = "linux", target_os = "android")))] const MAP_ANON: i32 = 0x20;`
(`:3035-3036`) и **компилируется без единой диагностики** — с неправильной
константой.

Собственный комментарий крейта (`crates/vmem/src/lib.rs:3023-3031`) описывает
реальное поведение прямо противоположным образом: *«`libc_mmap` would issue
`mmap(..., MAP_PRIVATE, -1, 0)` with no anonymous flag set, `fd = -1` causes
`EBADF`, `mmap` returns `MAP_FAILED`, and every `reserve_aligned` call fails
closed **with no diagnostic pointing at the wrong constant**»* — т.е. отказ
происходит в рантайме и без диагностики, а не на компиляции и с ней.

Batch M заменил одно backwards-утверждение (P3-9) на другое неверное. Для
publish-facing README это ложная гарантия fail-closed поведения.

Дополнительно в том же предложении: `#[cfg(target_arch = "mips*")]` — не
валидный cfg-синтаксис (glob в значении не поддерживается); корректно
`any(target_arch = "mips", target_arch = "mips64")`.

#### C-4. `reserve_aligned_huge_2mib_still_two_call_path_unprivileged` недетерминирован (~1/32) на реальном Windows CI

`crates/vmem/tests/huge_pages.rs:361-368` (batch J) утверждает
`single_calls == 0 && two_call_pairs == 1`, опираясь на тезис из собственного
doc-комментария (`:303-305`):

> `VirtualAlloc` returns a base aligned only to 64 KiB
> (`WIN_ALLOCATION_GRANULARITY`), not to the requested 2 MiB

Это смешение «гарантировано» и «фактически». `VirtualAlloc(NULL, …)`
гарантирует выравнивание **не менее** 64 KiB; ничто не запрещает ядру вернуть
базу, кратную 2 MiB. Сам крейт построен на том, что это совпадение регулярно
случается — fast-reserve sub-path из task #921/V-32
(`crates/vmem/src/lib.rs:2227-2250`) существует именно потому, что
*«VirtualAlloc(NULL, size, MEM_RESERVE, ...) **may return a base already
aligned to the requested alignment**»*.

Сценарий отказа: в `win_reserve_commit` для `align = size = 2 MiB` порог равен
`GetLargePageMinimum() = 2 MiB`, условие fast-path выполнено; первый
`VirtualAlloc` с `MEM_LARGE_PAGES` падает (нет привилегии), ретрай без флага
успешен, и если возвращённая база оказалась кратной 2 MiB (1 шанс из 32 при
64 KiB-грануле), post-call check (`:2201`) проходит, инкрементируется
`WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` (`:2211`) и функция возвращается **до**
двухзвонкового пути. Тест падает с `single_calls=1, two_call_pairs=0`.

Строка CI, которая это запускает: `.github/workflows/ci.yml:822`
(`test windows (production)`, `cargo test -p aligned-vmem --features "… bench-internals"`).
Тот же дефект в формулировке — в doc-комментарии
(`huge_pages.rs:308-312`, «the actual paths that SUCCEED … are still limited to
`align <= 64 KiB` because that's the guaranteed alignment») и в rustdoc,
продублированном batch J в трёх местах (`lib.rs:604-606`, `:738-740`,
`:1912-1914`).

Тест для 4 MiB (`reserve_aligned_huge_4mib_still_two_call_path`) этим не
затронут — там условие fast-path ложно по конструкции, и он детерминирован.

#### C-5. Все шесть номеров строк в «Evidence» item 49 (и по два в items 52/53) указывают не туда — причём уже **на момент своего собственного коммита**

`docs/CORRECTNESS_OPEN_ITEMS.md`, item 49 (batch L, `3fb7d31`) утверждает:

| Цитата | Что там реально было в `3fb7d31:crates/vmem/src/lib.rs` |
|---|---|
| `:2275` — `release_reservation`'s `winapi_virtual_release` call | `// is empirically always rejected by Windows, so requesting it would be a guaranteed` |
| `:2442` — `winapi_virtual_reserve`'s `VirtualAlloc` call | `const MEM_DECOMMIT: u32 = 0x0000_4000;` |
| `:2463` — `winapi_virtual_decommit`'s `VirtualFree` call | `// indicate a bug in this crate's own bookkeeping (not a recoverable external condition),` |
| `:2480` — `winapi_virtual_release`'s own `VirtualFree` call | `}` |
| `:2792` — Unix `release_reservation`'s `libc_munmap` call | `// This is correct because `MAP_HUGETLB` fails the WHOLE `mmap` call when` |
| `:3367` — `libc_madvise`'s `madvise` call | `//` |

То же в batch M (`3a25a48`):

* item 52: `:2888-2918` заявлено как «the current `madv_free_advice`
  implementation with BSD arms». В `3a25a48` строка 2888 — `/// advice value is
  shared), not current behavior — REASONED-FROM-SPEC only,`, строка 2918 —
  `target_os = "macos",`. Реальное определение `fn madv_free_advice()` сегодня —
  `crates/vmem/src/lib.rs:2974`.
* item 53: `:992-998` заявлено как сигнатура `from_raw_parts` — в `3a25a48`
  это середина doc-комментария («The reservation must be released **exactly
  once**…»); реальная сигнатура сегодня — `:1033`. `:1077` заявлено как
  «the parameter is now used directly in the `Reservation` constructor» — там
  строка панического сообщения / комментарий про выравнивание; реальный
  `granted_huge,` в конструкторе — `:1118`.

То есть номера не «протухли из-за последующих коммитов волны» — они были
неверны сразу. Это ровно тот класс дефектов, ради которого в этой кампании уже
принимали решение (task #901/U3: «replace stale line-number citation with symbol
names»), и он воспроизведён в двух коммитах подряд.

#### C-6. Обе новые ссылки в `ci.yml` называют неверный номер finding'а

`.github/workflows/ci.yml:196` — `# Closing review finding II-3: extend
coverage to tests and optional features, and add musl target …`
`.github/workflows/ci.yml:200` — `# Finding II-3 (2026-08-16 closing review):
add musl 32-bit target …`

II-3 — это Windows-widening `align <= GetLargePageMinimum()` (batch J). Правка
CI относится к **II-14** (32-bit exact-size path без CI-покрытия), а musl-шаг
мотивирован **II-1** (`off_t` FFI mismatch) — так и написано в самом commit
message batch L. Собственный предшествующий комментарий на `:193` цитирует
II-14 корректно, что делает соседство особенно заметным.

#### C-7. README продолжает утверждать, что 32-bit musl не покрыт CI, — через 2 коммита после того, как эта же волна добавила musl-шаг

`crates/vmem/README.md:206` (раздел «Reasoned-from-spec targets», т.е. «не
проверено в CI»):

> **32-bit musl Linux** (e.g., `i686-unknown-linux-musl`, …) — … **no CI runner
> currently tests these targets**.

Против `.github/workflows/ci.yml:202-203` (batch L, `3fb7d31`), добавленных
**после** README-правки batch M (`3a25a48`):

```yaml
- run: rustup target add i686-unknown-linux-musl
- run: cargo check --target i686-unknown-linux-musl --all-targets --features "…" -p aligned-vmem
```

По собственной таксономии README compile-only проверка считается CI-подтверждением
— `:199` перечисляет «**i686 Linux** (compile-only check via `cargo check --target
i686-unknown-linux-gnu`)» в списке *CI-verified*. Значит musl теперь тоже
должен быть там (и строка `:199` должна назвать оба таргета), а не в
reasoned-from-spec.

Порядок коммитов (M раньше L) объясняет, как это получилось, но не исправляет
факт: HEAD содержит внутренне противоречивую пару.

#### C-8. Зачистка устаревшего порога «align <= 64 KiB» неполна ровно на двух publish-facing поверхностях

Batch K заявляет: *«lib.rs, README.md, docs/perf/OPEN_ITEMS.md: three more
stale "align <= 64 KiB" mentions updated»*; batch J — *«Corrected five rustdoc
sites»*. Остались:

* `crates/vmem/README.md:67` — *«large pages (`MEM_LARGE_PAGES`) are only ever
  requested and possibly granted via the single-call fast path (`align <=
  WIN_ALLOCATION_GRANULARITY`, typically 64 KiB)»*. Это ровно то предложение,
  чей аналог в `lib.rs` batch J переписал трижды (`:598-606`, `:732-740`,
  `:1906-1914`), убрав именно этот параметрический хвост как неверный для
  large-page случая. README — единственная поверхность, которую видит
  потребитель с crates.io.
* `crates/vmem/Cargo.toml:98` — *«the fast path issues a single syscall for
  `align <= 64 KiB` on a full-span commit»* (doc-комментарий фичи
  `bench-internals`).
* `crates/vmem/src/lib.rs:1956` — *«task #848: Windows single-call fast path
  (`align <= 64 KiB`) is the only path that can grant large pages on Windows»*.
  Утверждение «единственный путь» верно; порог в скобках — нет.

---

### LOW

#### C-9. Item 52 в том же коммите объявляет item 53 открытым, а item 53 в этом же коммите закрывается

`docs/CORRECTNESS_OPEN_ITEMS.md`, item 52, Evidence (batch M):

> … items 48 (the decommit/Darwin gap) and **53** (the `from_raw_parts`
> interaction) **remain open** as separate issues.

Тот же дифф `3a25a48` переводит item 53 в `**CLOSED**`. Внутреннее
противоречие внутри одного коммита.

#### C-10. Закрытые items 49/52/53 оставлены в активном тире с полным нарративом — нарушение правила, которое цитирует commit message самого batch M

`docs/CORRECTNESS_OPEN_ITEMS.md:41` (правило самого файла): *«When you close an
item: move its entry to §"Recently resolved" …»*. CLAUDE.md формулирует то же
жёстче: *«the round that closes it MUST update the card to `Status: CLOSED`
**and move the narrative** in the SAME commit»*.

Items 52 и 53 после `3a25a48` имеют `Status: CLOSED`, но физически остаются в
секции `### [T] Tracked, not yet actioned` (`:2166-2178`) с полным нарративом;
секция `## Recently resolved` (`:2231`) их не содержит. Item 49 (`:2144-2147`,
batch L) — то же самое. Commit message batch M явно называет исходное
состояние *«a violation of CLAUDE.md's "OPEN_ITEMS indexes are CURRENT-STATE"
rule»* — и закрывает только половину этого правила.

#### C-11. Правка контракта `Reservation::decommit` оставила висячую скобку, противоречащую предыдущему предложению

`crates/vmem/src/lib.rs:820-825` (batch I):

> On the Darwin family (macOS/iOS/tvOS/watchOS) and the four BSDs
> (FreeBSD/DragonFly/NetBSD/OpenBSD), this is a best-effort hint with no
> zero-fill or reclaim guarantee — the physical pages may remain resident and
> old data may be observed after a decommit+recommit roundtrip **(after
> [`Self::recommit`] on Windows; implicitly on Linux)**.

Скобка — остаток предыдущей редакции, где она относилась к
zero-fill-гарантии. Пришитая к Darwin/BSD-предложению, она читается как «на
Windows/Linux старые данные тоже могут наблюдаться», что прямо противоречит
предшествующему предложению («On Linux and Windows this is **guaranteed** to
return physical backing and zero-fill on next access»).

#### C-12. Новый assert в II-17-тесте односторонний — он не доказывает, что `VirtualFree(MEM_DECOMMIT)` вообще был вызван

`crates/vmem/tests/lazy_commit.rs:607-613` (batch M) проверяет
`failures_after == failures_before`. Счётчик `WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`
инкрементируется только при `ret == 0` (`crates/vmem/src/lib.rs:2536-2540`).
Следовательно assert проходит и в случае «syscall прошёл успешно», и в случае
«syscall не выполнялся вовсе» — а `Reservation::decommit` по документации имеет
несколько silent-skip веток (`lib.rs:827-830`: пустой диапазон, out-of-bounds,
нарушение page-multiple контракта).

Commit message формулирует цель как *«could not distinguish a genuine
successful decommit from a silently-swallowed VirtualFree failure»* — вторая
половина закрыта, первая нет. На Unix у крейта для этого есть корректная пара
`unix_madvise_attempts()`/`unix_madvise_successes()`; для Windows attempts-счётчика
нет, поэтому по правилу path-activation-оракула из CLAUDE.md тест остаётся без
доказательства активации пути. Это не ложный assert, но он слабее, чем
подразумевает окружающий комментарий.

#### C-13. `huge_pages.rs` не получил `#![allow(clippy::let_unit_value)]`, который есть у `lazy_commit.rs` для того же паттерна

`crates/vmem/tests/lazy_commit.rs:8-14` содержит явный crate-level allow с
объяснением: под конфигурациями, где `serial_guard()` возвращает `()`,
`let _guard = serial_guard();` — это unit-value binding, «otherwise a real
clippy smell but is exactly the point here».

`crates/vmem/tests/huge_pages.rs` вводит идентичный паттерн (`:71-72`, `:76`,
`:139`, …) без такого allow. Сегодня не стреляет только потому, что ни одна
CI-строка не включает `huge-pages` без `bench-internals` (default clippy-строка
вообще не компилирует файл из-за `#![cfg(feature = "huge-pages")]`). Любая
будущая строка `--features "huge-pages"` без `bench-internals` под `-D warnings`
её сломает. Файл при этом заявляет, что «зеркалит» фикс из `lazy_commit.rs`.

#### C-14. II-4-покрытие целиком ушло за `bench-internals`

Batch J изменил гейт `reserve_aligned_huge_exact_size_for_2mib_align` с
`#[cfg(target_os = "linux")]` на
`#[cfg(all(target_os = "linux", feature = "bench-internals"))]`
(`huge_pages.rs:188-190`). Не-оракульная часть теста (выравнивание, writability,
«не должно классифицироваться как invalid_argument») теперь тоже недоступна в
сборках без `bench-internals`. `--all-features` её включает, так что CI
покрытие есть, но разделение «контрактные assert'ы безусловно + оракул под
фичей» было бы дешевле и не потеряло бы покрытие.

---

### INFO

#### C-15. Дублирующиеся `// SAFETY:` пары на трёх сайтах batch L

`crates/vmem/src/lib.rs:2345-2348`, `:2515-2516`, `:2871-2873` — новый
однострочный `// SAFETY:` приклеен непосредственно под уже существующим
`// SAFETY:` того же вызова, давая два подряд идущих SAFETY-комментария к одной
операции. Batch J в этой же волне отдельно чинил похожий дефект («A duplicated
5-line comment block … Trimmed the call-site copy»), так что паттерн уже
опознан как нежелательный.

#### C-16. SAFETY-комментарий `winapi_virtual_decommit` уже́ фактического контракта

`crates/vmem/src/lib.rs:2530-2531`: *«`VirtualFree` with `MEM_DECOMMIT` is safe
for any address/len **within a committed region**»*. Крейт намеренно вызывает
его на **never-committed** хвосте — это и есть предмет II-17-теста
(`lazy_commit.rs:548-613`). Требование на самом деле — попадание в
`MEM_RESERVE`-регион, а не в committed.

#### C-17. В CHANGELOG осталось осиротевшее определение ссылки `[0.1.0]:`

Batch K корректно расшил висячую ссылку `## [0.2.0]` → `## 0.2.0`
(`crates/vmem/CHANGELOG.md:8`), но зеркальная проблема на последней строке
файла осталась: определение `[0.1.0]: https://github.com/...` не имеет ни одной
ссылающейся `[0.1.0]` в документе. Плюс заголовок файла заявляет соответствие
Keep a Changelog, чей формат — как раз `## [x.y.z]` со ссылочными
определениями; фикс выбрал направление «убрать скобки», оставив файл
непоследовательным.

#### C-18. Разделение одной пары счётчиков между двумя путями создаёт двойной инкремент на 32-bit Linux и загрязняет уже запланированное измерение

`UNIX_EXACT_RESERVE_ATTEMPTS` теперь инкрементируется и в huge-блоке
(`lib.rs:2664`), и в `try_reserve_aligned_exact` (32-bit). На 32-bit Linux с
`huge-pages` и `align == 2 MiB` один логический `reserve` может дать
`attempts += 2` (huge-блок промахнулся → провалился в 32-битный fast path,
`lib.rs:2694-2699`), что делает знаменатель hit-rate не равным числу
резервирований.

Отдельно: `docs/perf/OPEN_ITEMS.md` (item «Next trigger», строка ниже
изменённой batch K) по-прежнему предписывает измерять 64-битный hit rate именно
`UNIX_EXACT_RESERVE_HITS`/`_ATTEMPTS`. Batch K обновил «Current
number/verdict», но не «Next trigger» — а после II-4 эти счётчики на 64-bit
измеряют huge-путь, а не тот over-reserve-путь, о котором item.

#### C-19. Формулировка мотивации musl-шага в CI сильнее того, что шаг делает

`.github/workflows/ci.yml:200-201`: *«add musl 32-bit target to **verify** the
wrong FFI `off_t` type fix»*. Фикс II-1 — это двухарменный `type OffT`
(`lib.rs:3325-3341`); `cargo check` подтверждает только, что нужный арм
выбирается и компилируется. Реального исполнения `mmap` с 64-битным `off_t` на
musl не происходит (это compile-only шаг, без раннера). Формулировка «compile-
verify that the musl arm is selected» была бы точной.

---

## Проверено и подтверждено корректным (контр-findings)

Чтобы отчёт не читался как «всё плохо», ниже — то, что было заподозрено и
оказалось в порядке:

1. **SERIAL-покрытие в `huge_pages.rs` полное.** Три теста без guard'а
   (`reserve_aligned_huge_rejects_non_huge_page_aligned_size`/`_align`,
   `reserve_aligned_huge_error_type_is_vmem_error`) отсекаются валидацией
   (`lib.rs:2628-2634`) **до** любого инкремента счётчика — счётчики они не
   трогают. Guard'ы стоят ровно на четырёх тестах, которые реально доходят до
   `unix_reserve`/`win_reserve_commit`.
2. **Новый counter-read в `lazy_commit.rs` держит `SERIAL`.**
   `safe_decommit_over_never_committed_tail_succeeds` берёт `serial_guard()` на
   `:548`, и оба чтения счётчика (`:569`, `:609`) гейтятся тем же cfg, что и
   guard.
3. **Ни одного `pub static` в `lib.rs` не осталось** — все восемь диагностических
   статиков `pub(crate)` (`:246`, `:260`, `:273`, `:286`, `:305`, `:313`,
   `:325`, `:337`). P3-10 закрыт полностью.
4. **Все новые `unsafe {}` имеют `// SAFETY:`**, включая два новых сайта в
   `benches/vmem_bench.rs:134-137`. Ни одного нового `unsafe` без комментария.
5. **`--features` вместе с `-p` в новых CI-шагах работает** — корневой
   `Cargo.toml` содержит `[package] sefer-alloc`, но уже существующие зелёные
   шаги (`ci.yml:163`, `:180`, `:822`) используют ровно такую же форму.
6. **README-утверждение «AArch64 macOS (Apple Silicon, macos-latest CI)» верно**:
   `ci.yml:842` — `runs-on: macos-latest` (Apple Silicon с 2024), и `:862`
   реально запускает `cargo test -p aligned-vmem --features "… bench-internals"`.
7. **Cross-reference в `mock.rs:55`** (`the `record` function's `# Reentrancy
   safety` section`) резолвится: заголовок существует на `mock.rs:281`.
8. **BSD-правки контракта `decommit_lazy` (batch I) фактически верны** —
   `madv_free_advice()` (`lib.rs:2974`) действительно диспатчит
   FreeBSD/DragonFly → `MADV_FREE_BSD_5`, NetBSD/OpenBSD → `MADV_FREE_BSD_6`,
   а `MADV_DONTNEED` на BSD действительно не даёт reclaim/zero-fill.
9. **Переформулировка `# Safety`-пункта `from_raw_parts` (batch I) корректна**:
   `granted_huge` читается только `is_huge()`/`Debug`, никакого UB неверное
   значение не создаёт — decommit на huge-страницах документирован как silent
   no-op.
10. **Тест `reserve_aligned_huge_4mib_still_two_call_path` детерминирован** —
    в отличие от 2 MiB-варианта (C-4), здесь `align > GetLargePageMinimum()`
    делает условие fast-path ложным по конструкции.

---

## Приоритет действий

1. **C-1** — блокер: почти наверняка красит Linux-строку CI. Чинить до пуша.
2. **C-4** — flaky-тест на Windows CI (~3% за прогон); чинить вместе с C-1,
   поскольку это тот же diff.
3. **C-2, C-3** — фактически ложные утверждения (одно в коде, одно в
   publish-facing README). Дёшево и обязательно до публикации 0.2.0.
4. **C-5, C-6, C-7, C-8** — мис-цитаты и doc-drift; механические правки.
   Для C-5 рекомендуется заменить номера строк именами символов (как уже решено
   в task #901/U3), а не «поправить номера».
5. **C-9 … C-14** — гигиена; C-10 стоит делать вместе с C-5/C-9 одним проходом
   по `docs/CORRECTNESS_OPEN_ITEMS.md`.
6. **C-15 … C-19** — по желанию.
