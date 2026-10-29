# D1′ — sidecar с одной картой классов и чанковый вариант: вердикты и закрытие (измерение, прод не меняется)

Отчёт фиксирует в main итоги двух закрытых без слияния PR семейства D1′
(B4-23). Полные данные, патчи и сырьё лежат в архивных тегах; здесь —
решения, ключевые числа и условия возврата. Прод не менялся: ни один вариант
не влит.

| PR | Ветка → архивный тег | Что пробовали | Вердикт |
|---|---|---|---|
| #94 | `b4-oneclass` → `archive/b4-oneclass` (`42cd3db0`) | одна карта классов вместо `ClassLeaves` (дублировавших state byte), варианты F1 и C2 стоимости регистрации | **NO-GO**; C2 паркуется как D1″ (perf OPEN_ITEMS item 131) |
| #96 | `b4-chunks` → `archive/b4-chunks` (`ddb19650`) | D1′ sidecar с owner-only ленивой конструкцией 512 чанков | **NO-GO / NOT ACCEPTED** |

## 1. D1′ one-class map (PR #94)

Отчёт в ветке: `docs/perf/B4_ONECLASS.md`, ADR
`docs/design/2026-10-20-adr-oneclass-starts-map.md`, пробы
`docs/perf/b4_oneclass_probes/` (все — в `archive/b4-oneclass`).

- Ir (callgrind, A = main `8da6402d`, B = ветка): mstress 380 → 347 Ir/step,
  batch / batchc / batchl alloc −18 Ir/блок, ao alloc −4 Ir/op; 10 из 11
  судей GO, thrash64 free +3 Ir/op (+3.5 % EstCycles) — NO-GO по своему порогу.
  Память sidecar −70…75 %.
- Регистрация сегмента: mmap-sidecar дороже — +16.9 % / +11.3 % по времени
  регистрации (Linux / Windows, n = 18), +45 % minor faults (fix8).
- Раунды fix12–fix16:
  - fix12–13: регресс +505k Ir на регистрацию — spill/reload вокруг вызова в
    цикле `drain_segment_sidecar`; F1 (`consume_nonzero_cut`
    `#[inline(never)]`) его снимает, остаток — обнуление второй 32-КиБ банки;
  - fix14: C4 (summary + пул) — NO-GO, первая регистрация не лечится;
  - fix15: C2 (ленивое обнуление по водяному знаку) — пробы регистрации
    лучше A, но Linux churn/RSS хуже (p = 0.02 / 1e-4); драйвер iai-ячеек fix15
    был невалиден;
  - fix16: разница churn — раскладка кода: Ir F1 ≡ B0, без выравнивания
    +3.7 % (p = 1.7e-4), с `-align-all-functions=6` −0.6 % (p = 0.66), A/A
    +0.46 %. Отсюда правило: судейские сборки — с выравниванием функций и
    блоков.
- Консилиум (4 ревьюера rush и @oh, 2026-10-22), решение утверждено
  владельцем 2026-10-29:
  - F1 — NO-GO окончательно: провалил предрегистрированный гейт (slope +19k,
    fixed +43k, Windows wall +5.4 %);
  - C2 в этом PR — NO-GO: гонка через безопасный API при `internals`
    (`issue_for_test` → `materialize_through` без CAS при `unsafe impl Sync`),
    выпали проверка выравнивания (`checked_granule`) и fail-fast
    `high_water > SEGMENT`, W1 не owner-протокол, нет Miri/Kani/CI iai,
    Linux churn/RSS хуже;
  - C2 паркуется как D1″ с условиями входа и kill-критериями (item 131).
- Наблюдение @oh: в callgrind обнуление (`rep stosb`) стоит ≈ 1 Ir/Б, то есть
  Ir завышает его относительно тактов; бутстрап ≈ 46k Ir против 12k у mimalloc
  почти целиком — обнуление sidecar.

## 2. D1′ chunked sidecar (PR #96)

Отчёт в ветке: `docs/perf/D1_PRIME_CHUNKED_SIDECAR_GATE.md` и
`docs/perf/d1_prime_chunked_sidecar/` (в `archive/b4-chunks`). Кандидат
`734395b7`, база `f80c681b`.

- Безопасность: CI `39869595774` зелёный (Miri SB/TB, Loom, Kani) — это не
  приёмка производительности.
- Производительность — NO-GO:
  - installed-global larson: candidate / baseline 1.281 при T = 4 и 1.191 при
    T = 8, парные CI не включают ноль;
  - dense16: 54 095 745 / 21 817 490 = 2.479 candidate / baseline;
    регистрация тоже хуже;
  - `bench:table`: `global_alloc_churn` 16 B 12.65 / 8.82 = 1.434,
    `segment_decommit_cycle` 253 KiB 12 136.71 / 3 287.20 = 3.692.
- Не закрыты (и не будут — кандидат отклонён): native Linux, Linux
  first-alloc / multisegment PSS.

## 3. Что остаётся

Только D1″ (item 131) — C2 с обязательными условиями до любого нового
замера. Остальные ветви D1′ закрыты.
