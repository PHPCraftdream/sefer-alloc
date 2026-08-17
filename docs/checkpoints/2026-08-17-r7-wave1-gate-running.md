# Checkpoint — 2026-08-17 09:33 [r7-wave1-gate-running]

## Session summary

Продолжение многораундовой кампании ревью-и-фикс крейта `aligned-vmem`
(`crates/vmem`) перед первой публикацией 0.2.0 на crates.io (там сейчас 0.1.0).

**Что закрыто за сессию.** Две полные очереди — `[R6]` (моё @oh-ревью волны,
находки F1–F13) и `[PA6]` (независимый аудит, R6-1…R6-8), десять содержательных
задач, девять коммитов от `8d68715` до `116dcc3`. Все коммиты прошли
`npm run check` из ОСНОВНОГО чекаута со статусом ALL GREEN. Не пушено.

**Сейчас в полёте.** Раунд `[R7]` по отчёту
`docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r7.md` (написан на
ревизии `2ad2607`, то есть на состоянии до последнего коммита). Первая волна
из четырёх батчей отработала, их правки СЛИТЫ В РАБОЧЕЕ ДЕРЕВО основного
чекаута, но НЕ ЗАКОММИЧЕНЫ — идёт `npm run check` (фоновый id `bopjm6jfc`,
запущен ~09:21). Его вывод буферизуется и на момент чекпоинта равен 0 байт;
вердикт брать ТОЛЬКО из тела вывода, не из кода возврата — фоновая обёртка за
эту кампанию дважды рапортовала «exit code 0» на красном `check-all`.

**Главная находка волны 1.** Батч #1047 добавил в `scripts/check-all.mjs` шаг
`doc (aligned-vmem --all-features, warnings-as-errors)` с полем
`env: { RUSTDOCFLAGS: '-D warnings' }`, но цикл в этом файле вызывал
`run(step.cmd, step.args, { cwd: REPO_ROOT })` и `step.env` не читал НИКОГДА.
Шаг существовал, назывался «warnings-as-errors» и не мог упасть. Починено
одной строкой (мерж `step.env` поверх `process.env`, иначе `spawn` затрёт
`PATH`). Доказано контрфактуалом: одна и та же битая intra-doc ссылка даёт
`exit 0` без env и `exit 101` с ним.

**Дефект оркестрации, важный для будущих волн.** Отчёты ревью в
`docs/reviews/` — UNTRACKED файлы, поэтому в git-worktree их нет. Батч #1048
честно написал «R7 не существует как документ» и работал по цитатам из моего
промпта. Так шла ВСЯ кампания — этим объясняется, почему промпты приходилось
делать такими подробными. Перед волной 2 отчёт R7 скопирован во все четыре
worktree (`sa-w1-c1`…`c4`) вручную.

**Статистика переделегирований** за прошлый раунд: 9 из 10 батчей. За волну 1
раунда R7: 0 переделегирований, но 3 из 4 батчей потребовали правок поверх.

## Active goal

Verbatim из Stop-hook:

> доделать все задачи по крейту

Ранее в сессии действовал `/babygoal` с условием про `/crush` + ревью через
`@oh`; пользователь снял его через `/goal clear`, затем перевзвёл `/babygoal`
с тем же текстом, затем задал текущую, более короткую цель.

## TaskList

### in_progress (волна 1 R7 — работа сделана, ждёт гейта и коммита)
- #1043 [R7] A: R7-1 — матрица zero-fill в rustdoc `Reservation`/`as_ptr`
- #1046 [R7] D: R7-7 — `VmemError::Display` печатал «size/align» для всех классов
- #1047 [R7] E: R7-8 — паритет локального гейта с CI (тут и был дефект `step.env`)
- #1048 [R7] F: R7-3+R7-4 — счётчик «huge отказал, plain сработал»; R7-4 = NULL

### in_progress (вне кампании, не трогать без слова пользователя)
- #657 numa-shim — republish (blockedBy: #658)
- #658 aligned-vmem — publish 0.2.0 (blockedBy: #842, #848, #849)
- #659 racy-ptr-cell, #660 size-classes, #661 tagged-index-stack — первые публикации

### pending (волна 2 R7 — промпты написаны, НЕ запущены)
- #1045 [R7] C: R7-9 — семь stale-сайтов (README ×2, Cargo.toml, lib.rs ×2, индекс ×3).
  Промпт: `.crush/stdin/r7-c.prompt`, worktree `sa-w1-c1`
- #1049 [R7] G: R7-5+R7-6+R7-10+R7-11 — три карточки + правка `libc_mmap`.
  Промпт: `.crush/stdin/r7-g.prompt`, worktree `sa-w1-c4`
- #1050 [R7] H: закрытие раунда (blockedBy: #1043, #1044, #1045, #1046, #1047, #1048, #1049)

### pending (вне кампании)
- #662, #763 — bench-scale-tool

### recently completed
- #1044 [R7] B: R7-2 — карточка item 66 дополнена (сделал лично, кода не трогал)
- #1042, #1036 — закрытие раундов PA6 и R6
- #1041, #1040, #1039, #1038, #1037 — очередь PA6
- #1035, #1034, #1033, #1032, #1031 — очередь R6

## Decisions

- **#1044 сделал сам, не делегировал.** Это подготовка решения владельца, а не
  код: в карточку item 66 добавлен ЧЕТВЁРТЫЙ вариант из R7 (явно принять
  caller-tracked контракт либо исключить `lazy-commit` из release-профиля),
  которого в моей вчерашней версии не было. Отвергнут вариант «реализовать
  committed_len молча» — это форма публичного API перед первым релизом.
- **R7-4 остаётся NULL, второй раз подряд.** Заведена карточка perf item 52 с
  контрпримером (exact просит `size`, общий путь — `size + align`, это разные
  запросы; «вторая попытка гарантированно провальна» верно только на машине
  без hugetlb-пула). Отвергнута правка fallback-семантики без замера, который
  на Windows-хосте недостижим.
- **Волна 2 разбита на два батча, а не три.** #1045 и #1049 разведены по
  файлам (correctness-индекс против perf-индекса; регионы `lib.rs` далеко
  друг от друга). #1044 вынут из делегирования вовсе.
- **Babysit-крон снят и перевзведён.** Старый `47b3282c` нёс устаревший
  контекст волны #964–989 и трижды заставил отвечать «ничего не делаю»; новый
  `537addc8` (каждые 15 мин, off-minute `7,22,37,52`, session-only) несёт
  актуальный контекст раунда R7 с явным запретом верить «exit code 0».

## Open questions

Оба требуют решения владельца и оба дёшевы ТОЛЬКО до публикации 0.2.0:

- **item 66 / R7-2 — `Reservation` не несёт committed-length.** Это ВТОРОЕ из
  двух условий условного NO-GO в R7; кодом не снимается. Пять открытых
  вариантов: поле `committed_len`; отдельный тип `LazyReservation`; кортеж
  `(Reservation, usize)`; явно принять caller-tracked контракт; исключить
  `lazy-commit` из поддерживаемого профиля. Первое условие (R7-1) закрывается
  кодом задачей #1043.
- **item 68 — асимметрия имён `decommit_reclaims_and_zeroes` /
  `can_decommit_reclaim_and_zero`.** Цена посчитана: 20 и 15 вхождений, все
  внутри `crates/vmem`. Сама карточка внутренне противоречива по написанию —
  это чинит задача #1045.

Не задавать вопрос о публикации 0.2.0: пользователь ранее сказал «не спрашивай
— я сам скажу когда публиковать».

## Repo state

```
 M crates/vmem/CHANGELOG.md
 M crates/vmem/Cargo.toml
 M crates/vmem/src/error.rs
 M crates/vmem/src/lib.rs
 M docs/CORRECTNESS_OPEN_ITEMS.md
 M docs/perf/OPEN_ITEMS.md
 M scripts/check-all.mjs
?? scripts/aligned-vmem-semver-check-optional.mjs   (новый файл волны 1, ещё не в индексе)
?? docs/checkpoints/*.md                            (7 чекпоинтов, untracked)
?? docs/reviews/*.md                                (отчёты ревью, untracked — см. дефект оркестрации выше)
```

```
116dcc3 fix: tasks #1036 + #1042 — two of this round's own regressions, both caught by `npm run check` and neither by any per-crate verification
2ad2607 docs(vmem): task #1041 — mark the decommit capability query as advisory, state miri's provenance requirement as its own Safety clause, cost out the zeroes/zero rename without doing it
84bc9ac fix(perf), docs: tasks #1040 + #1035 — skip the provably-useless decommit syscall on huge reservations, de-contradict from_raw_parts's reservation_len contract, record two citation hazards (R6-7, R6-8, F9, F12, F13)
dd6d027 fix(perf), docs: tasks #1037 + #1034 — validate lazy reservations against the RUNTIME page size, correct the lazy/from_raw_parts contracts, and index four follow-ups that existed only in commit bodies (R6-1, R6-2, F7, F8, F11)
a924dd0 fix(perf), test(vmem): task #1031 — WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES's doc claimed a value that is routinely wrong on ordinary Windows; make the counter-reset test discriminating on Unix (F1, F2)
```

## Среда — то, что легко забыть

- `npm run check` НЕЛЬЗЯ запускать из worktree этой кампании: `cargo fmt --all`
  падает там с `os error 206` (лимит длины командной строки Windows). Только
  из основного чекаута. Поэтому worktree названы коротко (`sa-w1-c*`).
- Target-директория `D:/dev/rust/.cargo-target` общая между worktree. Перед
  любой верификацией `touch crates/vmem/src/lib.rs`, иначе cargo рапортует
  «Finished» из кэша без единой строки «Checking».
- `gh run watch --exit-status` возвращал 0 на КРАСНОМ прогоне минимум дважды.
  Вердикт CI брать только из `gh run list` / `gh run view --json conclusion`.
- Есть гейт `scripts/vmem-doc-drift-guard.mjs`: валит сборку на предложении со
  словами «over-reserv»/«trim» без квалификатора области или со словом
  «unconditional». Уже ловил мою формулировку.
