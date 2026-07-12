# Round3-аудит: сверка трёх отчётов с состоянием после round2-batch

**Дата:** 2026-07-12. HEAD на момент сверки: `8e46288` (после round2-remediation
T1–T12 + follow-up). Сверка выполнена агентом `oh` (Opus, high), read-only.

Источники (агентские отчёты, написанные ПОСЛЕ round2-batch):
- `docs/agent_reviews_round3/memory_safety_review.md`
- `docs/agent_reviews_round3/performance_review.md`
- `docs/agent_reviews_round3/code_quality_review.md`

Контекст: `docs/reviews/2026-07-12-round2-synthesis.md`,
`docs/reviews/2026-07-12-round2-remediation-plan.md`, коммиты `ce887e5..8e46288`.

---

## Главный вывод

Round3 в основном **не находит новых багов — он несогласен с уже принятыми
round2-решениями**. Ключевое методологическое расхождение: memory_safety и
code_quality настаивают, что `AllocCore::dealloc/realloc`,
`HeapCore::dealloc/realloc` и test-hooks обязаны стать `unsafe fn`; round2
(T2/T3) сознательно выбрал safe-сигнатуру + defensive-free контракт M2 +
ограничение READ-стороны (`safe_payload_read_span`, `contains_base`,
foreign-null) и задокументировал это в самом коде.

**Критических/high среди по-настоящему новых находок — нет.**

## Проверка ключевых заявленных «CRITICAL»

- **R3-MS-1** (safe realloc, self-copy контрпример): контрпример **опровергнут**
  сверкой — `realloc(p_класс16, old=16, new=32)` идёт в разные size-классы,
  `self.alloc(Layout(32))` не может вернуть `p`, self-copy `(p,p,·)` недостижим;
  same-class уходит в in-place path до move-leg. READ-сторона ограничена T2.
- **R3-MS-2** (safe dealloc, interior ptr): это документированный M2
  defensive-free контракт (`alloc_core.rs:692-740`) — осознанное решение,
  не дефект.
- **R3-MS-3 / cq#1 / cq#2** (test-hooks): T3 сознательно выбрал
  release-surviving `assert!` вместо `unsafe fn` (файлы не являются
  unsafe-seams, `forbid(unsafe_code)`).

## Находки, противоречащие принятым решениям (re-litigation, закрываются)

| Round3 | Против чего | Статус |
|---|---|---|
| R3-MS-1/2/3, cq#1/#2 | T2/T3 (M2 safe-API) | Решение подтверждено (см. plan, решение №1) |
| perf P0/P1-3 (MagazineBitmap) | RAD-5 измеренный GO | Отклонено round2-планом явно |
| cq#4 (config-модель) | T5 (документирование first-bind-wins) | Решение T5 в силе |
| cq#5 (удалить features) | T6 (документированный опт-ин) | Решение T6 в силе |
| cq#6 (bitmap dedup) | T6 (вердикт «leave separate», G1 honest-reject) | Решение T6 в силе |
| cq#10 (split модулей) | T9 («no split», правило «one file — one export») | Решение T9 в силе |
| perf P1-6/P2-9 (research-tier, SyncRegion) | T12 («off production path» / «leave as-is») | Решение T12 в силе |
| cq#8 (is_empty тест) | T8 («док был неверен, не код») | Решение T8 в силе; деталь про from_raw_parts(len=0) в тесте — принято к сведению, runtime-риска нет |
| perf P0-1 (O(S) scan), P0-2 (retry storm residual) | T10 NO-GO / T11 NO-GO | Известные architectural residuals; новый заход только с новым бенчем (≥64 сегментов / многопоточный fan-in) — отдельный арк |

## Новые актуальные находки (5, все low/medium)

1. **N1 (MEDIUM) — `stats()` линейно обходит registry в production, док обещает
   обратное.** `heap_registry.rs:1012-1042` и `:1067-1093`
   (`tcache_hits_total`/`large_cache_hits_total` гейтятся на
   `all(alloc-global,fastbin)` и крутят `0..count`), тогда как инкремент —
   под `#[cfg(feature="alloc-stats")]` (`heap_core.rs:978`), которого нет в
   `production`. Счётчики всегда 0, но walk выполняется; док
   `sefer_alloc.rs:285-287` обещает «no segment or heap walk».
   Тройной независимый дубль (perf P1-4 ≡ perf R1 ≡ cq#7).

2. **N2 (MEDIUM) — `LargeCacheMode::{Background,Both}` паникует лениво из
   global-allocator пути.** `alloc_core.rs:552-556` (T5's eager panic)
   достижим через `GlobalAlloc::alloc` (lazy bind) — конфликт с заявленным
   «never panics». (cq#3.)

3. **N3 (LOW) — `Default for AllocCore` прячет fallible OS-reservation за
   `expect`-panic.** `alloc_core.rs:1678-1696`; impl без внутренних
   пользователей. (cq#9.)

4. **N4 (LOW) — рассинхрон «source of truth» unsafe-инвентаря.**
   `src/lib.rs:190` пишет «14 files»; anchored
   `grep -rlE '^#!\[allow\(unsafe_code\)\]'` даёт 13; loose grep из CLAUDE.md
   даёт 15 (2 ложных совпадения в комментариях). (cq#12.)

5. **N5 (LOW, спорная) — `PerClass` layout (136-байт stride, `count` далеко от
   `slots[14]`).** Гипотеза-микрооптимизация без измерений, противоречит
   измеренному bundling-обоснованию G7/FP2 в `tcache.rs:100-122`. Не заводится
   без бенч-подтверждения. (perf P2-8.)

## Сводка

- Уникальных пунктов: ~26 (с дублями ~34 упоминания).
- Закрыто/обработано round2: подавляющее большинство.
- Новых actionable: 4 (N1–N4) + 1 спорная (N5, отложена).
- План устранения и два принятых решения: см.
  `docs/reviews/2026-07-12-round3-remediation-plan.md`.
