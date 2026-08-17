# Checkpoint — 2026-08-16 14:40 [vmem-cr3-r4-pushed-ci-pending]

## Session summary

Многораундовая кампания ревью-и-фикс крейта `aligned-vmem` (`crates/vmem`) перед публикацией 0.2.0 на crates.io. К началу этой сессии уже были закрыты три раунда (исходный fxx-аудит на 25 находок → @oh-ревью «CR» на 12 → @oh-ревью «CR2» на 19, включая реальный HIGH-баг). Оставались заведёнными, но НЕ решёнными 14 задач двух последних раундов: CR3 (#1010–1015, из моего же @oh-ревью, `docs/reviews/2026-08-16-aligned-vmem-closing-review-r3-audit.md`, находки R3-1…R3-14) и R4 (#1016–1023, из внешнего отчёта, предоставленного пользователем напрямую, `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md`, находки R4-1…R4-13 + секция coverage gaps).

Сессия начиналась в заклиненном состоянии: механический Stop-hook повторял невыполненное babygoal-условие, а пользователь ранее явно сказал «заведи таски и остановись». Заклинивание разрешилось тем, что пользователь САМ перезапустил `/babygoal` с тем же условием — это была новая, не механическая инструкция, и я приступил к работе.

Стратегия — 12 изолированных git worktree (`sefer-alloc-vmem-final-a…l`, ветки `vmem-final-*`), по одной `/crush`-сессии на батч, запущены параллельно. Задачи #1010 и #1016 объединены в батч A, поскольку чинят один и тот же участок кода (иначе получились бы конфликтующие фиксы).

Главный результат сессии — **zero-trust review поймал дефекты в 6 батчах из 12**, которые их собственные crush-сессии отрапортовали как готовые. Самый серьёзный: батч A отчитался «✅ cargo test / clippy — чисто», но обе команды выполнялись на Windows, где `cfg`-предикаты вырезают изменённый код целиком; тот же дифф НЕ компилируется на Linux (E0658 «attributes on expressions are experimental» + E0308 «`else` clause of `let...else` does not diverge») — ровно на той платформе, ради которой фикс писался. Проверено отдельным минимальным репро с `all()`/`any()` вместо реальных cfg. Батч J: контрфактическая проверка (откатил только фикс, оставил новый тест) показала, что тест проходит против самого бага в 2 запусках из 3 и даёт разные вердикты на идентичном коде. Батч H вернул уже закрытый долг item 49 (`unsafe_op_in_unsafe_fn`) и добавил `debug_assert!` на runtime `page_size()`, которые с высокой вероятностью уронили бы macOS ARM64 CI (там 16 KiB против `PAGE`=4096). Батчи F и K создали дублирующиеся номера item'ов в индексах и цитаты по номерам строк — то есть регрессию находок R3-3/R3-6, которые ЭТА ЖЕ волна чинила в соседних файлах. Батч L добавил CI-степ, оканчивающийся на `|| echo`, то есть неспособный упасть в принципе.

Все шесть переделегированы с точными формулировками ошибок (включая фактический вывод команд, доказывающих дефект) и после этого приняты. Две мелкие правки (комментарий в батче I, переоценивавший силу теста; снятие фальшиво-зелёного CI-степа в L) сделаны мной вручную и явно указаны в текстах коммитов.

Итог: 11 коммитов слиты в `main` (несколько мержей потребовали ручного разрешения конфликтов в `lib.rs`, `Cargo.toml`, `CHANGELOG.md` — счётчики bench-internals добавляли сразу три батча, число в доке `reset_bench_internals_counters` пришлось сводить к `twelve`). `npm run check` — **ALL GREEN** (rustfmt, все 6 рядов clippy из ci.yml, 4 конфигурации тестов, структурные гейты, iai на 85 бенчах); он же поймал rustfmt-дрейф в новом тестовом файле батча G, что было исправлено до пуша. Запушено `71cfa98..58f4c0b`, landing SHA прочитан с remote (`git rev-parse origin/main`), не из локального HEAD.

**Что в полёте прямо сейчас:** CI на `58f4c0b`. Workflow `Kani verification` (id 31947203126) уже **success** за 41s. Основной workflow `CI` (id 31947203139) — **in_progress**, идёт ~10 минут; на него запущено блокирующее ожидание `gh run watch 31947203139 --exit-status` в фоне (background task `bgy52bmhz`), уведомление придёт по завершении.

**Что осталось по babygoal-условию:** после зелёного CI — запустить 4-й раунд `@oh`-ревью этих фиксов и завести новые задачи с префиксом `[audit-vmem]`, если ревью что-то найдёт. Это последний незакрытый пункт условия и последняя незакрытая задача #1015.

Активен babysit-cron `47b3282c` (каждые 15 минут, off-minute 7/22/37/52), поставленный ранее в сессии; самоудалится, когда TaskList опустеет.

## Active goal

Verbatim из Stop-hook (аргумент `/babygoal`, перезапущенного пользователем в этой сессии):

> [audit-vmem] - решли все задачи с помощью /crush, между задачами делай коммиты. Когда все эти задачи будут выполнены - сделай ревью этих задач с помощью агента @oh и заведи новые задачи по ревью с тем же префиксом (если ревью что-то найдет)

Статус выполнения: стадия 1 (решить все задачи через /crush с коммитами между ними) — **выполнена**, 13 листовых задач закрыты, 11 коммитов в `main`. Стадия 2 (@oh-ревью) — **не начата**, ждёт зелёного CI. Стадия 3 (завести задачи по находкам ревью) — не начата.

## TaskList

### in_progress
- #1015 [audit-vmem][CR3] F: merge/закрытие — верификация, npm run check, решение о раунде 4 (blockedBy: #1010–#1014, #1016–#1023 — ВСЕ разрешены)
- #657 numa-shim — verify/prepare for crates.io republish (blockedBy: #658) — не относится к кампании, не трогалось
- #658 aligned-vmem — publish 0.2.0 (blockedBy: #842, #848, #849) — не трогалось
- #659 racy-ptr-cell — first publish to crates.io — не трогалось
- #660 size-classes — first publish to crates.io — не трогалось
- #661 tagged-index-stack — first publish to crates.io — не трогалось

### pending
- #662 Root sefer-alloc: design note for applying bench-scale-tool alongside criterion/iai — не относится к кампании
- #763 Root sefer-alloc: implement bench-scale-tool per the approved design (blockedBy: #662) — не относится к кампании

### recently completed (эта волна, все смержены в main)
- #1010 CR3 A: R3-1+R3-2 — 32-bit assert vs doc-caveat + ложный SAFETY на 2 сайтах
- #1016 R4 A: R4-2 — 32-bit huge exact-попытка дважды (решалась вместе с #1010)
- #1011 CR3 B: R3-3+R3-11 — мис-цитаты в CORRECTNESS_OPEN_ITEMS.md
- #1012 CR3 C: R3-4+R3-7 — align<=64KiB зачистка + противоречие в ci.yml
- #1013 CR3 D: R3-5+R3-6+R3-8 — чужой cfg в комментарии, счётчик не в 3 местах, потерян armv7-musl
- #1014 CR3 E: R3-9…R3-14 — INFO-гигиена bundle
- #1017 R4 B: R4-1+R4-13 — MIPS compile_error! + минимальная версия ядра
- #1018 R4 C: R4-3+R4-4 — decommit capability API + huge-decommit счётчик
- #1019 R4 D: R4-10+R4-6+R4-11 — from_raw_parts контракт + ReservationFullParts
- #1020 R4 E: R4-7 — счётчик отказов winapi_virtual_release
- #1021 R4 F: R4-9+R4-8 — mock::drain reentrancy + fault_injection race
- #1022 R4 G: R4-5 — наблюдаемость Windows large-page retry
- #1023 R4 H: coverage gaps → items 58–61 в индексе

## Decisions

- **Приступил к работе, несмотря на прежнее «остановись»** — потому что пользователь сам перезапустил `/babygoal` с тем же условием, а это новая инструкция человека, а не повтор механического хука. Отвергнут вариант продолжать отвечать «(без изменений)».
- **6 батчей переделегированы, а не допилены вручную.** Отвергнут вариант чинить самому: правки были больше однострочных (сломанный cfg-паттерн, вакуумные тесты, отсутствующие тесты на новый публичный API), а переделегирование с точной формулировкой ошибки обучает цикл. Исключение сделано ровно дважды, для правок в 2 строки, и оба раза раскрыто в тексте коммита.
- **Батч L: CI-часть снята целиком, документация оставлена.** Отвергнут вариант оставить `sudo sysctl vm.nr_hugepages=128` + `cargo test ... || echo`: степ, который не может упасть, читается как покрытие, которого нет — хуже, чем его отсутствие. Настройка hugetlb-раннера записана как «Next trigger» item'а 59.
- **Батч H: четыре `debug_assert!` на runtime `page_size()` удалены, усиление `# Safety`-документации оставлено.** Отвергнут вариант оставить ассерты: проверить их с Windows невозможно, а на macOS ARM64 (16 KiB) они с высокой вероятностью роняют существующий тест `from_raw_parts_accepts_a_valid_reservation` — тот же класс, что HIGH-находка C-1 этой кампании. R4-6 просил уточнить контракт, а не добавить рантайм-проверку.
- **Батч G: счётчик переименован `HUGE_DECOMMIT_INCOMPATIBLE_SKIPS` → `HUGE_DECOMMIT_ATTEMPTS`** (вариант А из двух предложенных). Имя обещало «подтверждённые пропуски», а код считал каждый decommit huge-резервации; вариант Б (инкрементировать только там, где исход syscall известен) отвергнут — такого места в платформенном коде нет.

## Open questions

- **R4-1 (MIPS `compile_error!`) и R4-3 (Darwin/BSD capability-API) — решения уровня мейнтейнера.** Применены дефолтные суждения, зафиксированные в описаниях задач #1017/#1018, и явно помечены как таковые в текстах коммитов. Пользователь их не подтверждал. Альтернативы: для MIPS — добавить настоящие cfg-арм с правильными константами вместо отказа компиляции; для Darwin/BSD — сузить заявленную target matrix вместо capability-API.
- **Нужен ли 4-й раунд @oh-ревью** — по букве babygoal-условия да, и я собираюсь его запустить после зелёного CI. Но отмечу: задача #1015 в исходной формулировке содержала пункт «решение о раунде 4», а R4-отчёт пришёл от пользователя напрямую, а не был запущен мной, поэтому нумерация раундов в кампании слегка разъехалась.
- **CI ещё не зелёный целиком.** Kani прошёл, основной workflow идёт. Если он покраснеет — правки поверх `58f4c0b` до запуска @oh-ревью.

## Repo state

```
?? docs/checkpoints/2026-08-13-2100.md
?? docs/checkpoints/2026-08-14-vmem-r2-complete.md
?? docs/checkpoints/2026-08-14-vmem-r2-inflight.md
?? docs/checkpoints/2026-08-16-vmem-r3r4-stopped-goal-stuck.md
?? docs/checkpoints/2026-08-16-vmem-r4r5-ci-fixes.md
?? docs/reviews/2026-08-16-aligned-vmem-closing-review-r2-audit.md
?? docs/reviews/2026-08-16-aligned-vmem-closing-review-r3-audit.md
?? docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md
?? docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md
```

(Отслеживаемых изменённых файлов нет — рабочее дерево чистое, всё запушено. Review-доки и чекпоинты по конвенции кампании остаются неотслеживаемыми.)

```
58f4c0b docs(vmem): CR3/R4 wave -- CHANGELOG entries for the closing-review round
427d63b feat(vmem): CR3/R4 batch G -- make the decommit platform guarantee queryable, count huge-page decommit attempts (R4-3, R4-4)
6f9d83d fix(vmem): CR3/R4 batch J -- close the mock::drain reentrancy hazard and the fault-injection re-arm race (R4-9, R4-8)
d772d99 bench(vmem): CR3/R4 batch K -- make the Windows large-page speculative retry cost observable (R4-5)
8d5a1e0 feat(vmem): CR3/R4 batch H -- correct the from_raw_parts contract docs, add a lossless parts round-trip (R4-10, R4-6, R4-11)
```

Landing SHA на remote: `58f4c0ba2d23e30388db8b0162dc0cb540e19884` (подтверждён через `git fetch && git rev-parse origin/main`).

Ещё существуют 12 worktree `D:/dev/rust/sefer-alloc-vmem-final-{a..l}` с ветками `vmem-final-{a..l}` — не удалены, потому что кампания ещё не закрыта; после зелёного CI и @oh-ревью их надо снять (`git worktree remove --force` + `git branch -D`).
