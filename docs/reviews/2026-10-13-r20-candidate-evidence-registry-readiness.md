# R20 — план готовности реестра доказательств двух production-кандидатов

Дата: 2026-10-13. Анализ без изменений реестра. Ветки: `r20-prf01-cdp-main`
(кандидат `edf90675`, PR #4 draft) и `r20-prf04-v4prime` (`6951a9d0`, PR #5 draft).

## 1. Точный список нарушений (из логов джобы `evidence-registry`)

Логи: PR #4 — прогон 38327256109, job 115068697754; PR #5 — прогон 38311456554,
job 115017924405. VERDICT у обоих: `RED (strict)`.

### PR #4 (PRF-01 C″, `edf90675`) — `evidence-src-stale(7)`, `evidence-stale(3)`

| id | тип | что изменилось относительно базы `3f44fa0f` |
|----|-----|---------------------------------------------|
| R0-21 | src-stale | `src/alloc_core/segment/remote_bitmap/{bitmap_scan.rs, prf01_sidecar_oracle.rs, sidecar_bitmap.rs}` |
| R4c-05 | src-stale | то же + `src/registry/segment_route/{mod.rs, pin.rs, route_scan.rs, small_sidecar.rs}` |
| R4c-06 | src-stale | то же (7 файлов) |
| R4c-07 | src-stale | то же (7 файлов) |
| R5b-12 | src-stale **+ stale** | 3 файла remote_bitmap; `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs` отличается от версии на `3f44fa0f` |
| R5b-13 | src-stale **+ stale** | то же |
| R5b-14 | src-stale **+ stale** | то же |

### PR #5 (PRF-04 V4′, `6951a9d0`) — `evidence-src-stale(1)`, `evidence-stale(4)`

| id | тип | что изменилось |
|----|-----|----------------|
| R4b-11 | src-stale | `src/registry/heap_registry/claim.rs` (база `3f44fa0f` из `source_scope.csv`) |
| R4c-10 | stale | `tests/registry_basic.rs` отличается от версии на `source_sha` `5083e4bc` |
| R4c-11 | stale | `tests/registry_basic.rs` |
| R4c-13 | stale | `tests/r11_p3_registry_claim_uninit_oom.rs` |
| R4c-18 | stale | `tests/r11_p3_registry_claim_uninit_oom.rs` |

Историческая справка: более ранний прогон PR #5 на `6951a9d0` давал
`evidence-src-stale(7)` — после каскада вопросов нумерация сменилась на текущую;
актуальные числа — из таблиц выше (подтверждено строкой
`reason: violations: evidence-src-stale(7), evidence-stale(3)` / `...(1), ...(4)`
в логах).

## 2. Правила судьи (что именно проверяется)

- `evidence-src-stale` — `scripts/verify-evidence-registry.mjs:442`: для strong-строки
  (PASS / KNOWN-DEFECT / KNOWN-RED) со записью в `source_scope.csv` —
  `git diff --name-only <scope_base ‖ source_sha> HEAD -- <paths>` непуст → красный.
- `evidence-stale` — `scripts/verify-evidence-registry.mjs:918`: test-файлы, названные
  в строке (`rowTests`, :490-510), должны быть байт-в-байт равны (`lf`-нормализация)
  между `source_sha` и HEAD.
- `src-scope-required` (VER-2) — :1041: «new/refreshed strong row needs a
  docs/evidence/source_scope.csv entry». Признак «refreshed» — `evidenceKey`
  отличается от baseline (:1000, `--baseline $PR_BASE_SHA` в ci.yml:1396). Значит
  ЛЮБАЯ перепроверенная строка обязана получить запись в `source_scope.csv`
  (или уже иметь её).
- `scope_base` — :27-33, :410-413, :364: пусто = `source_sha`; иначе обязан быть
  предком HEAD. Семантика по проектному исследованию
  (`docs/reviews/2026-10-11-solution-research-verification-ver.md:266`):
  scope_base — для строк, измеренных на «base + незакоммиченный diff», который
  позже влит. **Поставить scope_base = кандидат, чтобы погасить src-stale без
  перепрогонa, НЕЛЬЗЯ**: строки R0-21/R4c-*/R5b-*/R4b-11 измерены ДО изменений
  кандидата в тех самых файлах scope; такой base утверждал бы «evidence покрывает
  новый src», что ложь. Единственный честный путь — перепрогон и новый `source_sha`.

## 3. Что закрепляет каждая строка и какая перепроверка нужна

Все 12 строк — Linux-совместимы: ни одна не требует Windows-нативного хоста
(loom и miri кроссплатформенны; нативные тесты — обычный `cargo test`; CI
`evidence-registry` сам крутится на ubuntu-24.04). историческая колонка `os`
(«Windows x86_64») при перепрогоне на ubuntu надо честно обновить в строке.

### PR #4

| id | закрепляет (entry / features / profile) | перепроверка | где |
|----|------------------------------------------|--------------|-----|
| R0-21 | Loom shadow-модели sidecar-битмапа и terminal-переходов (`loom_sidecar_bitmap`+2), features `alloc-core alloc-xthread` | `node scripts/loom.mjs loom_sidecar_bitmap loom_terminal_large loom_terminal_owner_drain` + 2 should-panic мутанта (как в `94c6d4e9`) | ubuntu CI/локально |
| R4c-05 | Miri SB strict, `regression_r2_06_header_race_miri` — KNOWN-RED (int-to-ptr cast) | тот же прогон, ожидается тот же известный красный класс + PASS в plain-модели (2 лога, как в `94c6d4e9`) | ubuntu (nightly miri) |
| R4c-06 | Miri SB strict, `regression_xthread_small_ring_miri` — KNOWN-RED | аналогично R4c-05 | ubuntu |
| R4c-07 | Miri plain, `regression_xthread_thread_free_alias_miri` — PASS | `MIRIFLAGS='-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5' cargo +nightly miri test … --test regression_xthread_thread_free_alias_miri` | ubuntu; прерemption 0.5 → возможны повторы |
| R5b-12 | source-pin C5 orderings, native debug, `production` | прогон `tests/r11_ph5b_c5_sidecar_ordering_pinned.rs` на чистом коде | ubuntu |
| R5b-13 | мутант publish fetch_or AcqRel→Relaxed | применить мутант → прогон пина → откат; `mutant_rejected=true` | ubuntu |
| R5b-14 | мутант cut swap AcqRel→Relaxed | аналогично | ubuntu |

Примечание: PRF-01 сам меняет `sidecar_bitmap.rs`/`bitmap_scan.rs` — R5b-13/14
проверяют мутанты в изменённых файлах, т.е. перепрогон на дереве кандидата
одновременно валидирует, что C5-порядки пережили изменение (содержательно
полезно, не только формально).

### PR #5

| id | закрепляет | перепроверка | где |
|----|-----------|--------------|-----|
| R4b-11 | teardown-пин `tests/r11_ph4b_teardown_ordering_pinned.rs` + мутант «torn after lease drop» | source-pin прогон + мутант → откат (2 лога, как в `94c6d4e9`); scope `src/registry/heap_registry/claim.rs` уже объявлен | ubuntu |
| R4c-10 | `registry_basic::latest_hint_then_fresh_defers_older_free` + мутант №2 | прогон registry_basic с применённым мутантом → откат | ubuntu |
| R4c-11 | `registry_basic::recycle_then_claim_reuses_slot…` + мутант №3 | аналогично | ubuntu |
| R4c-13 | `r11_p3_registry_claim_uninit_oom` + мутант №5 | прогон ловца с мутантом → откат | ubuntu |
| R4c-18 | тот же тест + мутант №10 | аналогично | ubuntu |

Важно: R4c-10/11/13/18 сегодня НЕ имеют записей в `source_scope.csv`; после
перепроверки они станут «refreshed» против baseline → судья потребует запись
(src-scope-required, :1041). paths — по затронутым PRF-04 src-файлам
(`src/registry/bootstrap/…`, `src/registry/heap_registry/…` соответственно).

## 4. Пошаговый план

Общий шаблон (по прецеденту `94c6d4e9`, повторён для каждой строки):

1. На ветке кандидата выполнить прогоны п.3; сырые логи
   `docs/evidence/_raw_r20_<id>_*.log` — каждый < 200 KiB, приватные пути
   заменить на `<path>` (правило raw-логов CLAUDE.md, tiers из `94c6d4e9`).
2. В `docs/evidence/registry.csv` для каждой строки: новый `source_sha`
   (= коммит дерева, на котором реально измерялось — кандидат или коммит
   перепроверки на его ветке), обновлённые `receipt`/`activation`/`toolchain`,
   `os` по фактическому хосту прогона; `mutant_rejected=true` для мутантных строк.
3. `docs/evidence/source_scope.csv`: записи для всех перепроверенных строк
   (R4c-10/11/13/18 — новые, у остальных paths уже есть; `scope_base` оставить
   пустым = новый source_sha).
4. `node scripts/pin-evidence-receipts.mjs` — пересчитать `receipt_sha256`
   (LF-нормализация, in-place; `--check` для верификации).
5. `node scripts/generate-evidence-registry-md.mjs` — регенерация `REGISTRY.md`
   (CI проверяет `--check`, ci.yml:1401).
6. Локальный контроль: `node scripts/verify-evidence-registry.mjs --strict
   --baseline origin/main` — запускается в CI на PR автоматически; до пуша
   достаточно `--check`-вариантов (cargo-прогоны запрещены текущей сессией,
   их выполняет владелец/CI).
7. Закоммитить на ветку PR; CI-джоба `evidence-registry` должна стать зелёной
   до слияния. Отдельные `gh workflow run` НЕ нужны: джоба и так перезапустится
   на новом коммите PR; `r20-gates.yml` (dispatch-only) — опционально для
   тяжёлых прогонов руками владельца.

Трудоёмкость: PR #5 — 5 строк, все прогоны быстрые нативные тесты + 5 мутантов:
~1–2 ч работы + 1 коммит. PR #4 — 7 строк, Loom (минуты), 3 Miri-прогона
(десятки минут каждый, R4c-07 недетерминирован из-за preemption 0.5), 2 мутанта:
~полдня, основной риск — время Miri и флейк R4c-07.

## 5. Нельзя сделать без человека / без владельца

- Сами прогоны: текущая сессия работает по протоколу без cargo/rustc — перепрогон
  выполняет владелец (локально, historically Windows; команды кроссплатформенны,
  так что ubuntu-хост/CI тоже годится) либо отдельная dispatch-джоба.
- Мутанты R5b-13/14, R4b-11, R4c-10/11/13/18 применяются ручным edit-run-revert —
  отдельного раннера в репо нет.
- Решение о слиянии и сами merge — владелец.

## 6. Порядок слияния, чтобы реестр обновлялся один раз

Наборы нарушений кандидатов НЕ пересекаются: PRF-01 трогает
`remote_bitmap`/`segment_route` (строки R0-21, R4c-05..07, R5b-12..14), PRF-04 —
`bootstrap`/`heap_registry` (R4b-11, R4c-10/11/13/18). Поэтому:

- Любой порядок даёт «одно обновление на кандидата»; слияние первого НЕ
  рестейлит строки второго (их scope-пути не пересекаются с чужим diff'ом).
- Вторая ветка ребейзится на новый main; её перепроверенные строки остаются
  зелёными, повторной перепроверки не требуется.
- Рекомендованный порядок: **PR #5 (PRF-04) первым** — дешевле (1–2 ч, только
  нативные тесты), закрывает реестр быстро; затем PR #4 (PRF-01) с его Miri/Loom
  полуднём. Это же снимает главный блокер: единственная нетривиальная
  зависимость — Miri-прогоны PRF-01, и их флейк/долгое время не задержит более
  лёгкого кандидата.
- Вариант «слить оба, потом один общий коммит реестра» невозможен без
  admin-override красной джобы на PR — не рекомендуется: красный `evidence-registry`
  обязан быть зелёным на каждом PR до слияния (условие задачи, прецедент `94c6d4e9`).

## 7. Главный блокер

Не технический: все 12 строк закрываются перепроверкой в Linux-окружении, без
Windows-хоста и без правок судьи. Блокер — необходимость ручных (в т.ч. мутантных)
прогонов владельцем и время Miri на PRF-01; пока они не сделаны, оба PR остаются
draft с красной джобой `evidence-registry`.
