# Checkpoint — 2026-07-06 08:39 [retro-verdict-r-campaign-planned]

## Session summary

Продолжение после чекпойнта `2026-07-06-x-arc-done-x7-planned` (X-кампания завершена и запушена, X7 распланирован). С тех пор произошло главное: **фоновый @fxx-агент завершил адверсариальную ретроспективу X-кампании** и вынес вердикт «фундамент годен для X7 с одной существенной поправкой». **C1 (CONFIRMED, исполняемый PoC):** X2-заявление «in-magazine нога закрыта» ложно для окна refill — `refill_class_bump_impl` (alloc_core.rs:1887–1957) вычерпывает freelist в `out` ДО ринг-дренажа, предикат (heap_core.rs:1390, `if k == c → false`) слеп к `out[0..filled]` → стейл-записка на блок P даёт двойную выдачу («P double-issued (2 times)» на `--features production` через dbg_flush_all + dbg_push_to_ring). Я лично подтвердил механизм чтением кода — не гипотеза. Таксономия residual теперь ТРИ ноги: in-magazine (закрыта X2), refill-окно (закрывается дёшево, БЕЗ поколений — вариант А: out-membership в обёртке предиката внутри _impl; вариант Б с дренажем-до-freelist отвергнут по Ir-цене), re-issue-before-drain (теорема §8 — territory X7). Также C2 (production-мёртвый дубликат AllocCore::realloc + лгущий док-блок HeapCore::realloc), C3 (стейл #[ignore]-сообщение → X7), PLAUSIBLE (README-таблицы с пре-X числами; MT malloc_macro не перегнан после X2), NOTE (дрейф 561,910/561,912; шапка IAI_BASELINE). Полный вердикт зафиксирован в **docs/reviews/2026-07-06-x-arc-retrospective.md** (untracked — коммитится первым коммитом R1 вместе с X7-планом = Ф0). По команде пользователя (/fxx «составь план исправления и заведи задачи») заведена пре-фикс кампания **R1–R4 = таски #194–197**, цепочкой перед X7-Ф1 (#189 теперь blockedBy #197). Описания тасок самодостаточны для холодного crush-старта (файлы:строки, форма фикса, гейты, судья, фазовые коммит-месседжи, session-slug'и). Второй открытый вопрос закрыт: **CI 28771478504 — success**. Явного «работай/реализуй» на R-кампанию пользователь ещё НЕ давал — следующий шаг по его команде: стартовать R1 через crush (session retro-c1-refill-window), с обязательным fl-аудитом.

## Active goal

нет (стоп-хук кампании X1–X6 исчерпан; новый не ставился).

## TaskList

### pending
- #188 X7: hardened generational ring entries — зонтик #189–193; описание обновлено: постановка сужена до ноги 3, пре-фиксы R1–R4 обязательны до Ф1
- #194 R1 (retro C1): закрыть refill-окно double-issue — out-membership в предикате дренажа + pinned-тест; включает Ф0-коммит (X7-план + retro-файл); fl-аудит обязателен
- #195 R2 (retro C2): свести дубликат AllocCore::realloc, правдивый док-блок HeapCore::realloc  (blockedBy: #194)
- #196 R3 (retro C3+NOTE): доко-свип — ignore-msg → X7, дрейф 561,912, шапка IAI_BASELINE, датировка README-таблиц  (blockedBy: #195)
- #197 R4: перегнать MT malloc_macro post-fixes, датированные MT-строки в ALLOC_BENCH/README  (blockedBy: #196)
- #189 X7-Ф1: gen-таблица 256KiB в метаданных сегмента (cfg hardened)  (blockedBy: #197)
- #190 X7-Ф2: перепаковка ring-записи [gen:8|class:6|off16:18]  (blockedBy: #189)
- #191 X7-Ф3: три касания + loom — СЕРДЦЕ, fl-аудит  (blockedBy: #190)
- #192 X7-Ф4: швы decommit-reset/recycle/adopt  (blockedBy: #191)
- #193 X7-Ф5: цены hardened в ledger, wrap-тест 1/256, доки, прогоны  (blockedBy: #192)

## Decisions

- C1-фикс = вариант А (out-membership в предикате внутри refill_class_bump_impl, реборроу &out[..filled] на время find) — ноль цены на single-thread пути; вариант Б (дренаж до freelist-пула) отвергнут: Ir на каждый refill.
- R-кампания серийная (R1→R2→R3→R4→Ф1): R1/R2 делят alloc_core.rs, R3/R4 делят README/ALLOC_BENCH — параллельные crush-запуски по одному файлу запрещены.
- Вердикт ретроспективы вынесен в durable-файл docs/reviews/ — crush-агенты стартуют холодными, таски ссылаются на него как на источник.
- Ф0-коммит X7 (план + retro) выполняется внутри R1 первым коммитом, не отдельной таской.
- Постановка X7 сужена: закрывает только ногу 3 (re-issue-before-drain); §1 плана правится в R1.

## Open questions

- Старт R-кампании — ждёт явного «работай/реализуй» пользователя.
- Тег/publish 0.3.0 + yank 0.2.1 — по-прежнему отдельное решение пользователя.
- (закрыто в этой сессии: CI 28771478504 = success; вердикт ретроспективы получен.)

## Repo state

```
?? docs/checkpoints/2026-07-06-x-arc-done-x7-planned.md
?? docs/design/X7_GENERATIONAL_RING_PLAN.md
?? docs/reviews/   (2026-07-06-x-arc-retrospective.md — войдёт в Ф0-коммит R1)
```

```
6b20596 docs(bench): post-X-arc comparative re-run — realloc 40x/1500x vs mimalloc
9a73e88 docs(changelog): X-arc section — realloc_grow -63% Ir / -47% cycles, #164 narrowed, 4 honest-rejects
cf16d85 docs(perf): X5 ledger — per-segment free-classes bitmap honest-rejected
490974d docs(perf): X6 ledger — clz class_for honest-rejected (EstCycles 10/11 worse)
b551b51 docs(perf): X4 ledger — both recycle experiments honest-rejected with numbers
```

origin/main == HEAD. Судья-reference: таблица «Post-X1+X2+X3» в docs/perf/IAI_BASELINE.md. WSL-ловушки: RUSTC_WRAPPER сбрасывать, /tmp tmpfs (профили сразу в $HOME), кавычки в `wsl bash -lc` гибнут — скрипт-файлы.
