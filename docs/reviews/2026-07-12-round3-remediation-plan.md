# Round3: план доработки + два принятых решения

Источник: `docs/reviews/2026-07-12-round3-synthesis.md`. Объём осознанно
маленький — round3 подтвердил зрелость кодовой базы; дорабатываем точечно,
не пересматривая решённое.

## Требование к тестированию (как в round2-плане, обязательно)

По `CLAUDE.md`: каждый код-фикс — с regression-тестом в `tests/` (отдельный
файл, не inline), RED→GREEN лично проверяется оркестратором (временный откат
фикса → тест падает → восстановление → зелёный). Полный
`cargo test --features production`, `cargo fmt --check`,
`clippy -D warnings` — перед каждым коммитом. Никаких doctests. Коммит после
каждого этапа (санкционировано правилом phased delivery).

---

## Решение №1 — M2 остаётся в силе: `dealloc`/`realloc` остаются safe

R3-MS-1/2/3 и cq#1/#2 закрываются как «дизайн-решение, не дефект»:

- Round3 не предъявил нового эксплойта, пережившего сверку (self-copy
  контрпример R3-MS-1 опровергнут: разные size-классы → `new_ptr != p`;
  same-class → in-place path).
- READ-сторона ограничена T2 (`safe_payload_read_span` + `contains_base` +
  foreign-null). Остаток — M2 defensive-free контракт, документированный
  information-theoretic limit (как у `free()` в mimalloc/glibc): `unsafe`-маркер
  не устраняет неотличимость повторного free после reuse адреса.
- Переделка в `unsafe fn` — breaking-каскад по всем call-sites ради смены
  позы, не безопасности.

**Уступка round3 (справедливая):** формулировка `alloc_core.rs:17-21`
(«`dealloc`/`realloc` are `unsafe` per the `GlobalAlloc` contract») третий
раунд подряд провоцирует аудиторов на одно и то же прочтение. Чиним
формулировку (речь о семантике контракта, не о Rust-`unsafe`) и добавляем
короткую ссылку на это решение, чтобы round4 не поднимал вопрос в четвёртый
раз. → задача R3-D.

## Решение №2 — `Background`/`Both` удаляются из enum

«Make invalid states unrepresentable»: нет варианта — нет ни panic из
`GlobalAlloc::alloc`, ни silent no-op, ни обещания, которое нечем держать.

- Крейт v0.3.0 (pre-1.0), breaking допустим с CHANGELOG-записью — T5 сам
  отметил удаление как приемлемое.
- `LargeCacheMode` остаётся enum с единственным `Lazy` и помечается
  `#[non_exhaustive]` — возвращение вариантов вместе с реальным
  background-scavenger-ом в будущем будет non-breaking.
- Панический `match` T5 в `new_with_config` и его два `should_panic`-теста
  уходят вместе с вариантами (заменяются тестом Lazy-round-trip; сам
  компилятор — регрессионный гард недоступности удалённых вариантов).
- Снимает cq#3 (panic из global-allocator пути) полностью. → задача R3-B.

---

## Задачи

| ID | Суть | Приоритет |
|---|---|---|
| R3-A | `stats()` честный контракт стоимости: compile-time 0 без walk при выключенном `alloc-stats`; при включённом — walk остаётся (per-slot дизайн W3 сознательный), док уточняется на «O(1) без alloc-stats / O(slots) с ним». Regression-тест на контракт. (N1) | MEDIUM |
| R3-B | Удалить `LargeCacheMode::{Background,Both}`; `#[non_exhaustive]` на enum; убрать панический match T5; обновить тесты/доки/CHANGELOG. (N2, решение №2) | MEDIUM |
| R3-C | Удалить `Default for AllocCore` (нет внутренних пользователей; прячет OS-reservation + panic за «безобидным» Default). CHANGELOG. (N3) | LOW |
| R3-D | M2: уточнить `alloc_core.rs:17-21` + записать решение №1 рядом с M2-доком; закрыть R3-MS-1/2/3, cq#1/#2 как design-affirmed. (решение №1) | LOW |
| R3-E | Самопроверяемость unsafe-инвентаря: якорный паттерн `^#!\[allow\(unsafe_code\)\]` в `lib.rs`/README/CLAUDE.md, убрать захардкоженное «14 files». (N4) | LOW |
| R3-F | Финальное ревью батча агентом `oh`. | — |

Порядок: R3-D → R3-B → R3-A → R3-C → R3-E → R3-F (сначала фиксация решений,
затем breaking-изменение, затем код-фикс, затем механика, в конце ревью).

## Не заводится

- **N5 (PerClass layout)** — гипотеза без измерений, противоречит измеренному
  G7/FP2 bundling; вернуться только с бенч-подтверждением.
- **P0-1 / P0-2 residuals** — T10/T11 NO-GO в силе; новый заход требует нового
  бенча (≥64 сегментов, многопоточный fan-in) — отдельный арк.
- **Все re-litigation пункты** из таблицы синтеза — соответствующие
  round2-решения остаются в силе.
