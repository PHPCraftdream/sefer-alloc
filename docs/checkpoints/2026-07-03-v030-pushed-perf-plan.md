# Checkpoint — 2026-07-03 v0.3.0 pushed, CI in flight, perf-plan queued

## Session summary

Продолжение арки sefer-alloc 0.3.0. К этому моменту ВЕСЬ hardening-backlog
закрыт (#128–#143 + review-фиксы, детали в чекпойнте
`2026-07-03-v030-review-complete-ready-to-push.md`): 17 дефектов, 5
memory-safety (2 блокера #129/#130, aliasing-UB #142, leak-класс #143 и
#138-clamp), каждый с личным counterfactual. Финальное `/fxx`-ревью + перф-
замеры сделаны, README/ALLOC_BENCH обновлены свежими 0.3.0-числами.

В ЭТОМ отрезке: пользователь дал команду запушить и наладить CI, и отдельно —
исследовать ускорение small/medium аллокаций (обогнать mimalloc). Запущены
ДВА фоновых агента: (1) **@sxx push+CI** (`ac49ad9…`) — УЖЕ ЗАПУШИЛ (origin/main
= HEAD, коммит `d542f51` "ci: fix clippy --all-features on numa-shim linux
platform module"), продолжает дожимать `ci.yml` до зелёного, читая логи `gh`;
(2) **@fxx research** (`a9f6de6…`, ЗАВЕРШЁН) — отдал глубокий обоснованный
кодом план ускорения. Затем в режиме `/fxx` (созерцание→озарение) я собрал
план в 5 «эврик» и завёл поэтапный план оптимизаций в задачи **#144–#149** +
записал его в `docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`.

**Ключевая перф-диагностика:** мы instruction-bound (плоские ~28 µs на всех
размерах), не page-fault-bound → разрыв на мелочи устраним кодом. Два фронта:
(A) холодная мелочь 16–64B (mimalloc 2–2.6× быстрее — carve round-trip через
BinTable тавтологичен, свежий bump-блок уже «allocated»); (B) 256B churn
(mimalloc 1.25× — `lock xadd` счётчика на каждый hit + нет точного класса 256).

**Текущее состояние:** 0.3.0 ЗАПУШЕН в origin/main. CI-агент ещё чинит jobs в
фоне (ждём его финального доклада — я перепроверю через `gh` сам). Тег
`sefer-alloc-v0.3.0` НЕ создан, crates.io НЕ опубликован (это ОТДЕЛЬНОЕ решение
пользователя; агенту явно запрещено тегировать/публиковать). Перф-фазы
#144–#149 — pending, стартуют ПОСЛЕ устаканивания CI (P0 трогает .github/).
babysit НЕ активен.

## Active goal

none (стоп-хук снят ранее).

## TaskList

### pending (перф-план, цепочка blockedBy; стартует после CI)
- #144 PERF-P0 измерительный фундамент — iai cold/churn бенчи + baseline
- #145 PERF-P1 Э5 счётчик без lock + Э4 единый class_for + Э2 одна ветка сентинелов + класс 256B (blockedBy #144)
- #146 PERF-P2 Э3 own-segment cache (blockedBy #145)
- #147 PERF-P3 Э1 bump-direct carve — главный рычаг фронта A + retire P7 (blockedBy #146)
- #148 PERF-P4 flush word-merge + S3 alloc_zeroed virgin-skip (blockedBy #147)
- #149 PERF-P5 финальное измерение + README/вердикт (blockedBy #148)

### recently completed (предыдущий отрезок)
- #128–#143 весь hardening-backlog + #142/#143 (новые баги) — см. предыдущий чекпойнт

## Decisions

- **Перф — «удаляем тавтологии, не защиты».** Ни одна гарантия (M2/D1/A1/
  xthread/forbid-unsafe) не ослабляется. M2-битмап на НАСТОЯЩЕМ free остаётся
  — цена exact-гарантии, платим полностью. Feature-gate guard'а (fast/hardened)
  — отклонён на сейчас (отдельное продуктовое решение 0.4+).
- **Порядок фаз P0-first.** iai-бенч холодного пути ОБЯЗАН быть до P1/P3 —
  иначе фронт A слеп для instruction-count на шумном хосте.
- **P3 (bump-direct) — источники в прежнем порядке:** freelist/ring-drain
  ПЕРЕД bump — иначе freed-блоки прокисают и ломается xthread-reclaim.
- **CI-агенту запрещён тег/релиз/publish** — только push + чинить ci.yml.
  Тег/публикация 0.3.0 — явное отдельное решение пользователя.
- **@fxx research — read-only** (код не тронут); план — вход для реализации
  под построчный zero-trust, не автопатч.

## Open questions

- **CI зелёный?** — @sxx ещё дожимает; финальный статус jobs перепроверить
  через `gh run list --branch main` / `gh run view`. Главный риск-кандидат:
  miri strict-provenance job vs exposed-provenance (#140/#142) — registry-
  тесты туда намеренно не входят, но если reclaim_offset_unit зацепит exposed-
  путь, фикс — точечно исключить кейс из strict-матрицы (не ослаблять глобально).
- **Тег + релиз 0.3.0** — по явной команде пользователя (после зелёного CI).
  Yank 0.2.1 — опционально.
- **Старт перф-фаз** — по команде «делай», после устаканивания CI. Начинать с
  P0 (#144) + P1 (#145).
- **shamir-db перепрогон на 0.3.0** — давний хвост, после публикации.
- Untracked чекпойнты/план-документ — включать в коммит по решению пользователя.

## Repo state

```
?? docs/checkpoints/2026-07-03-v030-pushed-perf-plan.md   (этот файл)
?? docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md      (план оптимизаций)
(рабочее дерево иначе чистое; CI-агент может добавить свои CI-фикс-коммиты)
```

```
d542f51 ci: fix clippy --all-features on numa-shim linux platform module
a4cb666 docs(checkpoints): 0.3.0 four-agent review + review-complete/ready-to-push
3a2ff11 docs: refresh README + ALLOC_BENCH perf tables with re-measured 0.3.0 numbers
fc84493 fix+docs(review): mitigation clamp symmetry, CHANGELOG fold, tooling drift
f635ee8 docs: fix broken intra-doc link in #140 provenance-model section
```

origin/main = HEAD (0.3.0 ЗАПУШЕН, ~20 коммитов от c08bbb9). crates.io =
0.2.1 live (0.1.0/0.2.0 yanked). Cargo.toml = 0.3.0, CHANGELOG [0.3.0] -
2026-07-03. Тег sefer-alloc-v0.3.0 НЕ создан, publish НЕ выполнен.

## Активные фоновые агенты
- @sxx `ac49ad9034c0d6fc9` — push (сделан) + чинит ci.yml. Ждём финал.
- @fxx `a9f6de67c012d3bf8` — research ЗАВЕРШЁН (план в docs/perf/).
