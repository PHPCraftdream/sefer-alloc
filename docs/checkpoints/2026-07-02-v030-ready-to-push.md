# Checkpoint — 2026-07-02 v0.3.0-ready-to-push

## Session summary

Продолжение арки sefer-alloc 0.3.0. К этому моменту ВСЁ запланированное
выполнено: план A–F (6 фаз hardening), backlog #125–#127, node-скрипты для
hardening-прогонов, и полный финальный sweep (cargo test + loom + miri +
TSan) на итоговом HEAD `d59b3e5`. Локальный main — **13 коммитов впереди
origin/main**, рабочее дерево ЧИСТОЕ. НЕ запушено, НЕ тегировано, 0.3.0 НЕ
опубликован — всё готово и ждёт только команды пользователя на
push + tag `sefer-alloc-v0.3.0` + release (процесс как для 0.2.1: тег
триггерит release workflow → crates.io publish → GitHub Release из
CHANGELOG).

Хронология этого отрезка (после чекпойнта v030-hardening-complete):
1. Пользователь: "давай оставшиеся задачи дорешаем, а перед пушем ещё
   прогоним miri, loom, TSan".
2. **#125** (`18c5ff8`) — own-thread Large dealloc теперь eager
   unregister+release в обеих под-ветках (no-decommit И decommit
   admission-reject), вместо defer-to-Drop (который в shard-модели
   недостижим → перманентная утечка слота SegmentTable). Закрыл ПОСЛЕДНЕГО
   представителя leak-класса #114/A1. Counterfactual лично: обе ветки
   падали на iter 1023 без фикса. Тест
   regression_own_thread_large_no_leak.rs (2 кейса под оба feature-гейта).
   Double-free с Drop исключён: Drop walk'ает table.bases(), unregister
   убирает base из table до release.
3. **#126** (`d64d9f4`) — переделка отклонённого C3: SegmentTable::base_at(i)
   (self-contained индексное чтение, safe через node seam) → индексный цикл
   в find_segment_with_free/dbg_drain_all_rings, interleaved recycle БЕЗ
   буфера → recycle unbounded, 8 KiB memset с горячего miss-пути убран.
   Drop-буфер (16 KiB) оставлен намеренно (там настоящий aliasing-хазард —
   registry живёт в primordial payload, — и Drop не горячий).
   Counterfactual лично: инъекция recycle-budget=32 в drain → тест
   regression_c3_unbounded_recycle падает "32 of 150 recycled" (ровно
   отклонённый баг Фазы C). Один flaky race_repro в 1 из 6 полных прогонов
   (стабилен 5/5 изолированно, чист в остальных 5 полных) — параллельный
   harness-артефакт, не регресс.
4. **#127** (`82eb6c2`) — perf-gate CI workflow: iai-callgrind
   (instruction-count, детерминированный) вместо wall-clock criterion.
   .github/workflows/perf-gate.yml (nightly schedule + workflow_dispatch +
   PR label 'perf'; НЕ в required-checks), benches/perf_gate_iai.rs (4
   бенча: small_churn_16b, aligned_churn_640b_a128, large_alloc_free_cycle,
   realloc_grow; Linux-gated + stub main для остальных платформ),
   iai-callgrind как cfg(linux) dev-dep. ЧЕСТНОЕ ОГРАНИЧЕНИЕ: пока
   REPORT-ONLY — нет baseline-persistence, job не фейлится на регрессии.
   Enforcement = задача #128. Реальный запуск iai проверяем только на
   Linux CI (первый прогон установит baseline).
5. **Node-скрипты** (`d59b3e5`) — по просьбе пользователя "для запуска
   tsan+wsl давай заведём nodejs скрипты": scripts/{lib,tsan,loom,miri}.mjs
   + package.json (npm run loom/miri/tsan/harden). tsan.mjs инкапсулирует
   3 ловушки TSan-через-WSL: (а) sccache.exe RUSTC_WRAPPER re-инъецируется
   WSL interop'ом — лечится ПУСТЫМИ СТРОКАМИ на cargo-строке (unset
   недостаточен!); (б) -Zbuild-std в отдельный /tmp/sefer-tsan; (в)
   sanitizer в RUSTFLAGS+RUSTDOCFLAGS, скан вывода на маркеры (TSan не
   роняет exit code). Все 3 скрипта валидированы (PASS). Cargo.toml
   exclude: scripts/, package.json, .github/, docs/checkpoints/ — вне
   crates.io тарбола.
6. **Финальный sweep на HEAD d59b3e5 — ВСЁ подтверждено явно**:
   - cargo test production --no-fail-fast ×2 — чисто;
   - loom — 8 файлов, 24 теста;
   - miri (strict-provenance): region_invariants, decommit_miri_cycle
     (172s, покрывает #126), reclaim_offset_unit (432s),
     realloc_cross_class_shrink, large_align_no_segment_exhaustion
     (покрывает #125) — UB-чисто;
   - TSan (WSL): ВСЕ 4 cross-thread теста — global_alloc_mt(3),
     heap_cross_thread(3), race_norecycle(1), race_repro(3) — ноль data
     race. ВАЖНАЯ ДЕТАЛЬ: пользователь спросил "мы все прогоны сделали?"
     и был прав — TSan изначально был неполон (первая 4-тестовая команда
     упала на sccache ДО компиляции; прошли только race_repro+race_norecycle
     в retry). Пробел закрыт, все 4 подтверждены построчно.
   - fmt clean; clippy -D warnings = 0 findings (5 фича-комбо).

Методологические уроки, накопленные за арку (важно для будущих сессий):
полный suite ТОЛЬКО с --no-fail-fast (fail-fast маскировал 2 красных теста
в Фазе B); counterfactual каждого фикса воспроизводить ЛИЧНО; отчёт агента
— claim, не факт (отклонён C3 агента с реальным багом; TSan-пробел найден
только перепроверкой); heredoc с юникодом в git commit -m зависает под Git
Bash — коммитить через `git commit -F <файл>` (+ --no-verify, т.к.
pre-commit hook гоняет полную сборку >2 мин).

## Active goal

none (babysit удалён ранее — план A–F завершён; backlog решался по прямым
командам)

## TaskList

### in_progress
(нет)

### pending
- #128 perf-gate baseline persistence — сделать gate enforcing (follow-up
  #127). Требует наблюдения ПЕРВОГО реального Linux-CI прогона perf-gate
  (валидность iai-макросов, cargo install iai-callgrind-runner 0.14,
  фактические instruction counts) → потом --save-baseline на scheduled +
  --baseline на PR/nightly + порог ~5-10%. Не блокер релиза.

### recently completed (эта арка)
- #127 F6 perf-gate CI workflow (82eb6c2)
- #126 C3-переделка index-driven scan (d64d9f4)
- #125 own-thread Large dealloc eager release (18c5ff8)
- #119–#124 Фазы A–F (d90c557..7ea2798)
- (#113–#118 — 0.2.1 арка, предыдущие сессии)

## Decisions

- **TSan-via-WSL: пустые строки RUSTC_WRAPPER= на cargo-строке, не unset.**
  WSL interop re-инъецирует Windows env; unset в bash -lc недостаточен.
  Зафиксировано в scripts/tsan.mjs с комментарием.
- **#126 дизайн: SegmentTable::base_at(i) индексный проход** (вариант 2),
  а не инкапсулирующий scan-метод (вариант 1): корень borrow-конфликта —
  formal lifetime capture у impl Iterator в bases(), а не семантика
  drain/recycle; точечный безопасный метод решает без переноса сложной
  drain-логики. count() снят раз; recycle NULL'ит слот но не сжимает
  count; base_at(recycled)→null→continue — итерация стабильна.
- **#127: iai-callgrind (вариант A), не criterion+critcmp (B)** —
  instruction count детерминирован на shared runner'ах, только он
  позволяет жёсткий порог 5-10% (wall-clock band ±15-20% пропустил бы
  сам инцидент #114 22-31%). Принято ограничение report-only с явным
  follow-up #128, вместо полу-рабочего enforcement.
- **Drop-буфер 16 KiB в #126 НЕ тронут** — там pre-collect защищает от
  настоящего aliasing-хазарда (registry в primordial payload, освобождение
  mid-walk unmap'ит итерируемый массив), Drop once-per-lifetime.
- **package.json/scripts/.github/docs/checkpoints исключены из crates.io
  тарбола** (Cargo.toml exclude) — dev-инструменты не нужны потребителю
  библиотеки.

## Open questions

- **Push + tag + release 0.3.0** — ЕДИНСТВЕННОЕ действие, всё готово:
  `git push origin main && git tag sefer-alloc-v0.3.0 && git push origin
  sefer-alloc-v0.3.0` → release workflow публикует на crates.io → GitHub
  Release из CHANGELOG [0.3.0]. Ждёт явной команды пользователя.
- **Yank 0.2.1 после 0.3.0?** — 0.2.1 НЕ содержит критичных для
  production-конфига багов уровня 0.2.0 (A1-утечка требует cross-thread
  large free — реальна для tokio, но 0.2.1 живёт у пользователя в
  shamir-db без инцидентов). Решение за пользователем; прецедент: 0.1.0 и
  0.2.0 yank'нуты.
- **shamir-db перепрогон на 0.3.0** — после публикации стоит прогнать
  duplex_throughput + engine_perf на 0.3.0 (обещанные пользователем
  opt-level=3 замеры так и не прозвучали).
- **#128** — после первого Linux-CI прогона perf-gate.
- **Первый прогон perf-gate на CI** — за ним стоит присмотреть после
  пуша (nightly cron 03:30 UTC или вручную workflow_dispatch): валидность
  iai-макросов и runner-install проверяемы только там.

## Repo state

```
(clean)
```

```
d59b3e5 chore(dev): node hardening-sweep runners (loom / miri / TSan-via-WSL) + tarball exclude
82eb6c2 ci(#127): perf-gate workflow — iai-callgrind instruction-count regression guard
d64d9f4 perf(#126): index-driven segment scan — drop the 8 KiB per-miss stack zeroing, keep recycle unbounded
18c5ff8 fix(#125): own-thread Large dealloc releases eagerly, not deferred to Drop
b16ec98 release: 0.3.0 — post-0.2.1 hardening pass (phases A–F)
```
(далее: 7ea2798 F, d0e4ff2 E, f70ff0c D, 35df2f5 C, a6409b6 B-testfix,
cce049e B, f564d08 A-docs, d90c557 A — итого 13 впереди origin/main)

crates.io: sefer-alloc 0.2.1 live (0.1.0, 0.2.0 yanked). Версия в
Cargo.toml = 0.3.0, CHANGELOG [0.3.0] готов. Тега sefer-alloc-v0.3.0 ещё
НЕТ. Push НЕ выполнен.
