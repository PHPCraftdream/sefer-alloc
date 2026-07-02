# Checkpoint — 2026-06-30 v0.3.0-hardening-complete

## Session summary

Продолжение сессии sefer-alloc после релиза 0.2.1 (fix #114). Пользователь
дал `/fxx` исследование репо → отчёт по ~20 находкам → я составил план из 6
фаз (A–F) → `/babygoal` "реализуй план, используй агентов sh". Весь план
**A–F реализован, проверен zero-trust, закоммичен** (8 коммитов, HEAD
`7ea2798`). Стратегия: sequential sub-agents (sh/Sonnet), каждая фаза =
один агент пишет код+тесты (агенты НЕ коммитят), я делаю личный line-by-line
review + перезапуск тестов + counterfactual'ы + коммит между фазами.

Реализовано (все с регрессионными тестами + counterfactual):
- **A** (`d90c557`): A1 утечка cross-thread free Large-сегментов (per-heap
  deferred-free Treiber-стек, drain на alloc_large); A2 fastbin unsound без
  alloc-xthread (feature dep + compile_error). `f564d08` — docs REACTIVATION
  HAZARD про next_abandoned field-sharing.
- **B** (`cce049e`): B1 page-aligned классы 512–16384 (таблица 40→48); +
  вскрыт латентный realloc cross-class-shrink баг (OPT-F `<=`→`==`).
  `a6409b6` — test-fix phase13_3 + realloc_in_place под новую геометрию.
- **C** (`35df2f5`): C1 magazine для align>16; C2 realloc in-place на
  global-face. C3 (8 KiB memset) ОТКЛОНЁН — нашёл баг (RECYCLE_BUF_CAP=32
  теряет recycle → slot pin) → задача #126. C5 отложен (UB-риск) → тест есть.
- **D** (`f70ff0c`): D1 LARGE_CACHE_SLOTS 2→8 + настоящий seq-FIFO (был
  slot-0-oldest баг); D2 ring-overflow счётчик; D3 per-class refill
  byte-budget.
- **E** (`d0e4ff2`): E1 публичный `SeferAlloc::stats() -> AllocStats`
  (#[non_exhaustive], монотонные segments_reserved/released счётчики); E2
  docs multi-thread footgun + std-only.
- **F** (`7ea2798`): F1 fallback livelock fix + loom_fallback_init; F3
  warnings-cleanup + `-D warnings` вернули в CI (0 findings по 5 combos);
  F4 heap_core_bulk_bypass flaky fix (dbg_reset_bulk_state — реальная
  причина: whole-heap slot reuse несёт stale P7 state); F5 miri align-тесты
  в CI. F6 (perf-gate) отложен → задача #127.

**Текущее in-flight:** пользователь дал `/sl пропиши версию 0.3.0` затем
"везде перепиши на 0.3". Я обновил версию 0.2.1→0.3.0 в: Cargo.toml
(+Cargo.lock авто), README.md (2 install-строки: "0.2"→"0.3"),
docs/INTEGRATION.md (2 install-строки), CHANGELOG.md (добавлена полная
запись [0.3.0] с разбивкой Fixed/Changed/Added/Internal). Исторические
упоминания 0.2.0/0.2.1 в CHANGELOG (заголовки прошлых релизов, migration
примеры) и в README бенч-отчёте ("sefer-alloc 0.2.0 vs mimalloc" —
фактическая запись прогона на 0.2.0, не перезамерялось на 0.3.0)
СОЗНАТЕЛЬНО оставлены — это точные исторические записи, не текущая версия.
`cargo build --features production --release` собирается как sefer-alloc
v0.3.0. **НЕ закоммичено, НЕ запушено.**

Дисциплина review, усвоенная за сессию: полный suite ТОЛЬКО через
`cargo test --features production --release --no-fail-fast` (без --no-fail-fast
cargo прерывается на первом упавшем бинарнике и МАСКИРУЕТ остальные — так я
пропустил 2 красных теста при первом review Фазы B: realloc_in_place:82 и
phase13_3, оба кодировали старое поведение, починены в a6409b6). Каждый
counterfactual воспроизводил ЛИЧНО (не доверяя отчётам агентов).

babysit был армирован (cron 5a49ff7f, 15m) на время выполнения плана,
СЕЙЧАС УДАЛЁН (CronDelete) — цель /babygoal (план A–F) достигнута.

## Active goal

none (babysit удалён; /goal не использовался)

## TaskList

### in_progress
(нет)

### pending — BACKLOG (отложенные находки, НЕ часть плана A–F; ждут решения
пользователя по приоритету; НЕ подхватывать автономно — babysit удалён
намеренно)
- #125 Own-thread Large dealloc без alloc-decommit — тот же leak-паттерн что
  A1, но own-thread путь. Место: AllocCore::dealloc ветка SegmentKind::Large
  под #[cfg(not(alloc-decommit))] + fallback когда large_cache admission
  отклонён — откладывает release на Drop, которого в shard-модели нет →
  perm slot pin. production (с alloc-decommit) основной путь не затронут при
  успешном admission. Фикс по аналогии с reclaim_large_segment (eager
  unregister+release). Нужен regression >1024 own-thread large cycle без
  decommit; counterfactual → abort ~iter 1023.
- #126 C3 переделка — убрать 8 KiB stack-zeroing в find_segment_with_free
  (alloc_core.rs ~1379) без потери recycle. Отклонённая agent-версия имела
  RECYCLE_BUF_CAP=32 → при >32 опустевших за scan теряет recycle → slot pin.
  Правильные варианты в описании задачи (SegmentTable::scan_and_recycle
  метод / loop-restart / персистентный is_decommitted проход). Перф-
  оптимизация, pre-collect сейчас корректен но платит 8 KiB.
- #127 F6 perf-gate CI workflow (criterion baseline / iai-callgrind на
  churn-16B, churn-640B-align128, large-cycle, realloc-grow; schedule/label).

### recently completed
- #124 Фаза F — CI/loom/гигиена
- #123 Фаза E — наблюдаемость (stats API)
- #122 Фаза D — пределы/тюнинг
- #121 Фаза C — перф горячего пути
- #120 Фаза B — align≥512 + doc size-class
- #119 Фаза A — критическая корректность
- (#113–#118 — предыдущая сессия, fix #114 / 0.2.1)

deleted: #115 (standalone repro — решили работать на shamir-db как testbed)

## Decisions

- **Версия 0.3.0, исторические 0.2.x оставлены.** Cargo.toml/README/
  INTEGRATION/CHANGELOG обновлены на 0.3.0. НО заголовки прошлых релизов в
  CHANGELOG, migration-примеры (0.1→0.2), и README бенч-отчёт "0.2.0 vs
  mimalloc" оставлены как есть — это фактические исторические записи, не
  install-инструкции. Отвергнуто: слепой sed 0.2→0.3 везде (переписал бы
  историю + невернул бы бенч-числа которые реально мерялись на 0.2.0).
- **C3 отклонён, не смёржен.** Agent-версия вводила RECYCLE_BUF_CAP=32 с
  потерей recycle при overflow (тот же slot-pin класс что #114/A1). Correctness-
  first: откатил к корректному pre-collect (8 KiB), завёл #126 на правильную
  переделку. Отвергнуто: смёржить ради перф-оптимизации с найденной дырой.
- **--no-fail-fast обязателен для верификации.** После пропуска 2 красных
  тестов в Фазе B (fail-fast маскировал) — все последующие полные прогоны
  через --no-fail-fast, 2-3 раза. Отвергнуто: grep первого failed (дырявый).
- **Агенты НЕ коммитят; git только через главную сессию** после личного
  review + counterfactual + перезапуск тестов. Каждая фаза = отдельный
  коммит (методология проекта: test+commit gate между фазами).
- **babysit удалён по завершении плана A–F**, backlog #125-127 НЕ
  подхватывается автономно — это новый scope, требует явной команды
  пользователя (приоритизация). Отвергнуто: оставить babysit → он бы начал
  #125 сам.

## Open questions

- **Пуш + релиз 0.3.0?** Всё в main локально (8 коммитов + version-bump
  uncommitted). НЕ пушено. Релиз-процесс как для 0.2.1: коммит version-bump,
  тег sefer-alloc-v0.3.0, push, release workflow публикует на crates.io,
  GitHub Release из CHANGELOG. Ждёт команды пользователя.
- **version-bump ещё НЕ закоммичен** (CHANGELOG/Cargo.toml/Cargo.lock/README/
  INTEGRATION modified). Нужен коммит перед тегом.
- **Backlog #125-127** — какой приоритет, делать ли сейчас или отложить.
  #125 (own-thread large leak) — тот же класс что критичный A1, стоит
  рассмотреть до широкого раскатывания на non-decommit конфиги.
- **shamir-db верификация 0.3.0** — пользователь тестировал на shamir-db
  (17× vs mimalloc). После публикации 0.3.0 стоит перепрогнать
  duplex_throughput + основные бенчи на 0.3.0 (path override или crates.io).

## Repo state

```
 M CHANGELOG.md
 M Cargo.lock
 M Cargo.toml
 M README.md
 M docs/INTEGRATION.md
?? docs/checkpoints/2026-06-30-post-021-hardening-plan.md
?? docs/checkpoints/2026-06-30-v030-hardening-complete.md
```
(version-bump 0.2.1→0.3.0 uncommitted; run_tsan.sh был закоммичен ранее в
c08bbb9; агент F возможно тронул его — проверить не нужно, не в scope)

```
7ea2798 fix(0.3.0-dev): F1 fallback livelock, F3 warnings-clean + -D warnings restored, F4 bulk-bypass flaky, F5 miri align tests
d0e4ff2 feat(0.3.0-dev): E1 public SeferAlloc::stats() -> AllocStats, E2 multi-thread/std-only docs
f70ff0c perf(0.3.0-dev): D1 larger large-cache + true FIFO, D2 ring-overflow counter, D3 per-class refill byte budget
35df2f5 perf(0.3.0-dev): C1 magazine serves align>16, C2 realloc in-place on global face
a6409b6 test-fix: reseed phase13_3 + update realloc_in_place for B1/realloc-fix behaviour (Phase B follow-up)
```
(до них: cce049e B, f564d08 A-docs, d90c557 A, c08bbb9 fmt, 5d75bf3 0.2.1)

crates.io: sefer-alloc 0.2.1 live (0.2.0/0.1.0 yanked). 0.3.0 ещё НЕ
опубликован. main НЕ запушен (origin на c08bbb9-era? — проверить git push
статус; локально 8 hardening-коммитов впереди).
