# Round2-аудит: план устранения (группировка похожих задач)

Источник: `docs/reviews/2026-07-12-round2-synthesis.md` (сверка 29 находок из трёх
round2-отчётов с актуальным кодом, HEAD `c26ee57`).

Ниже — те же находки, перегруппированные не по исходному отчёту, а по тому, что
логично чинить **вместе** (общие файлы, общий паттерн исправления, общая природа
проблемы). Каждая группа — один task на TaskList; при исполнении каждая задача
описывает свою стратегию сама (см. `description`), так что резюмировать можно с нуля
без контекста этой сессии.

## Требование к тестированию (обязательно для КАЖДОЙ задачи ниже)

По `CLAUDE.md` — «код без тестов не считается завершённой фазой». Для каждой задачи
(и Группы 1-9, и perf-групп) обязательны:

- **Регрессионный тест на саму находку**, который был бы RED без фикса (контрфакт
  лично проверяется: временно откатить фикс → тест должен упасть; накатить обратно →
  тест зелёный). Тест кладётся в `tests/` отдельным файлом (не inline
  `#[cfg(test)] mod tests`) — по правилу «тесты в отдельной папке с самого начала».
- Для soundness-находок (Группа 1, 2) — там, где применимо, дополнить miri-тестом
  (обнаруживает UB, которое обычный `cargo test` может не воспроизвести
  детерминированно) и/или loom-тестом, если путь конкурентный.
- Для perf-групп (Группы 8-10) — «тест» означает **и** корректность (regression test
  на инвариант, который патч не должен нарушить), **и** perf-судью: до/после через
  `npm run iai`, GO/NO-GO по честному бюджету (см. `docs/perf/IAI_BASELINE.md`), как в
  RAD-5/RAD-4b этой сессии.
- Никакая задача не считается выполненной без зелёного `cargo test` (плюс miri/loom
  там, где применимо) и без личной zero-trust проверки диффа перед коммитом — это
  стандартная дисциплина `CLAUDE.md`, не новое требование, но здесь оно явно
  распространяется на все 12 задач плана.

---

## Группа 1 — Soundness: safe API обходят unsafe-инварианты (HIGH)

Общий паттерн: публичный `pub fn` без `unsafe` в сигнатуре, но с невыполненным
предусловием памяти (UAF / OOB / foreign-pointer). Три отдельные задачи, т.к. разные
файлы и разные фиксы, но естественно делать подряд — один и тот же класс дефекта,
один и тот же ревью-чеклист (soundness patch + regression test, RED→GREEN).

1. **T1 — UAF в safe abandoned-stack API (R2-2).**
   `src/registry/heap_registry.rs:372,401-402` (`push_abandoned_segment`/
   `pop_abandoned_segment`) + `src/alloc_core/alloc_core.rs:1665-1715`
   (`AllocCore::drop` не чистит global stack). Приоритет HIGH (потенциально
   CRITICAL) — самая серьёзная находка ревью.

2. **T2 — Остаток небезопасного `realloc` (R2-1 + cleanup#1).**
   `src/registry/heap_core.rs:1556-1572` (`HeapCore::realloc` foreign-leg без
   membership-барьера) + `src/alloc_core/alloc_core.rs:1324` и парный путь в
   `heap_core.rs` (копирование по caller-controlled `old_layout.size()`).

3. **T3 — Test-only raw-pointer швы публичны и safe (R2-3 + cleanup#2).**
   `src/alloc_core/remote_free_ring.rs:498-511` (`over_test_buffer`/
   `init_test_buffer`), `src/alloc_core/alloc_core.rs:1075-1186` (`dbg_*` accessors —
   guard есть, но только `debug_assert!`, в release пропадает). Решить: cfg(test)-гейт
   vs runtime-guard, который переживает release.

---

## Группа 2 — HeapOverflow drain bug (MEDIUM)

4. **T4 — `HeapOverflow::drain` возвращает `tail`, а не `head` (R2-4).**
   `src/registry/heap_overflow.rs:355-381` (:380 возвращает `t` вместо
   опубликованного `h`), `src/registry/heap_core.rs:772-785` (кэш последствия).
   Standalone — один файл-пара, чинится и тестируется отдельно от Группы 1.

---

## Группа 3 — Конфигурационные ловушки: silent no-op (MEDIUM)

Общий паттерн: публичный конфиг принимает значение, которое ничего не меняет —
пользователь думает, что включил поведение, а оно молчит. Дёшево чинится (либо
реализовать, либо явно депрекировать/паниковать), поэтому — одна задача на обе.

5. **T5 — `LargeCacheMode::{Background,Both}` no-op (cleanup#6) + `SeferAlloc`
   first-bind-wins семантика (cleanup#3).**
   `src/alloc_core/large_cache_mode.rs:21-32`, `src/global/sefer_alloc.rs:171-172,
   219-221`.

---

## Группа 4 — Мёртвый/отклонённый код и дублирование (LOW-MEDIUM)

Общий паттерн: код, который сознательно не используется в проде (rejected experiment,
legacy tier, дублирующие реализации, `#[allow(dead_code)]`). Одна ревизионная задача —
для каждого решить: удалить, изолировать под явный feature-флаг с предупреждением, или
задокументировать как «оставлено намеренно».

6. **T6 — Ревизия мёртвого/отклонённого кода (cleanup#4, #5, #7, #10).**
   `alloc-runfreelist` эксперимент (`Cargo.toml:235`, `run_stack.rs`), legacy
   concurrent tier тянущий `experimental`/`pinning` фичи, дублирование
   `AllocBitmap`/`MagazineBitmap`, `set_small_current` под `allow(dead_code)`
   (`alloc_core.rs:1548`) и другие future-only функции.

---

## Группа 5 — Документация и метаданные (LOW)

Общий паттерн: doc-комментарии или `Cargo.toml`-метаданные не соответствуют факту.
Никакого поведенческого риска, чисто точность документации — одна задача-проход.

7. **T7 — Doc/metadata accuracy pass (cleanup#8, #9, #12, #13, #15).**
   `PageMap` docs vs mixed-class модель, устаревший «GO/NO-GO EXPERIMENT» doc в
   `magazine_bitmap.rs` (RAD-5 уже GO), `crates/region/src/region.rs:7-8` («dense…
   always-compact» vs фактический `SlotMap`), `crates/region/Cargo.toml:13`
   (`no-std::no-alloc` категория неверна), корневой `Cargo.toml`-как-changelog.

## Группа 6 — Точечный баг в vmem (LOW)

8. **T8 — `Reservation::is_empty` всегда `false` (cleanup#14).**
   `crates/vmem/src/lib.rs:116-119`. Standalone, крошечный фикс + тест.

## Группа 7 — Рефакторинг крупных модулей (LOW, отдельно)

9. **T9 — Разбить модули 1700-2500+ строк (cleanup#11).**
   Отдельно от остальных: это чистый рефакторинг структуры файлов (соответствие
   правилу «один файл — один export» из `CLAUDE.md`), не баг и не perf. Низкий
   приоритет, делать не вперемешку с фиксами выше — риск конфликтов diff'ов.

---

## Группа 8 — Perf: горячий путь small-alloc (HIGH потенциал)

Общий паттерн: линейные обходы на горячем пути мелких аллокаций. Оба в
`alloc_core_small.rs`/`size_classes.rs`, оба — кандидаты в один perf-эксперимент
(RAD-стиль: baseline → патч → `npm run iai` GO/NO-GO).

10. **T10 — O(S) обход SegmentTable на miss + `class_for` align>16 loop
    (perf#1, perf#9).**
    `src/alloc_core/alloc_core_small.rs:1476-1673` (`for i in 0..n` в
    `find_segment_with_free_impl`), `src/alloc_core/size_classes.rs:161-181`
    (forward-walk от seed для align>16). Perf#1 — крупнейший потенциал, но требует
    строгого membership-учёта (риск корректности) — не «быстрый win», отдельный
    RAD-стиль эксперимент с честным GO/NO-GO gate.

---

## Группа 9 — Perf: cross-thread free / overflow path (MEDIUM)

Общий паттерн: три находки из одного архитектурного узла (MPSC free-путь и его
диагностика/footprint), одна связана с другой причинно.

11. **T11 — CAS-сериализация residual + diagnostic storm + HeapOverflow footprint
    (perf#2, perf#7, perf#8).**
    `src/registry/heap_core.rs` (retry-storm архитектурное ядро — dead-owner случай
    уже закрыт `aa4ccf9`, но единый MPSC CAS-курсор всё ещё сериализует всех
    producers; линейный `stats()` scrape), inline footprint `HeapOverflow`
    (~24 КиБ/slot).

---

## Группа 10 — Perf: research-tier concurrent regions (LOW, batch review)

Общий паттерн: все четыре — в `src/concurrent/` (deprecated/research tier, не
production hot path). Одна инвестигативная задача вместо четырёх отдельных —
сначала выяснить, используется ли этот tier вообще в продакшен-конфигурации, прежде
чем чинить каждую по отдельности.

12. **T12 — Ревизия research-tier concurrent regions (perf#4, #5, #6, #10).**
    `src/concurrent/lock_free_region.rs` (page-table copy + 64 Arc bumps/write),
    `epoch_region.rs` (`Mutex<Vec>` remote-free, теряет capacity при drain),
    `sharded_region.rs` (TLS process-global, не per-instance), `SyncRegion`
    (coarse `RwLock<Region>`).

---

## Не задача — требует решения пользователя

**performance#3 — MagazineBitmap 32-КиБ RMW.** Предпосылка находки противоречит уже
принятому и измеренному GO-решению RAD-5 (`docs/perf/IAI_BASELINE.md`). Не заводится
как задача на исправление — это скорее «второе мнение» perf-аудита против осознанного
дизайн-выбора текущей сессии. Если стоит пересмотреть — отдельное явное решение
пользователя, не автоматическая задача.

---

## Порядок приоритетов (для последовательного выполнения)

HIGH: T1, T2, T3, T10
MEDIUM: T4, T5, T11
LOW: T6, T7, T8, T9, T12

Группы 1-3 (T1-T5) не пересекаются по файлам критично — можно вести параллельно
разными агентами. T10-T12 (perf) — отдельный трек, требует `npm run iai`
до/после каждого патча (правило GO/NO-GO из `CLAUDE.md`). T6/T9 (мёртвый код,
рефакторинг модулей) стоит делать НЕ одновременно с T1-T4 — высокий риск конфликтов
diff'ов в тех же файлах (`alloc_core.rs`, `heap_core.rs`).
