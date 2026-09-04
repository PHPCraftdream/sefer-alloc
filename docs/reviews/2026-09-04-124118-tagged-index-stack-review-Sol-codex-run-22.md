# `tagged-index-stack`: prerelease review, Sol-codex, run 22

**Время отчёта:** 2026-09-04 12:41:18 +02:00

**Ревизия:** `954f044e6fb04dc6082c6f7a448c9ab58d60c6f5`

**Режим:** независимый статический обзор одним ревьювером, без под-агентов; production-код не изменялся; тесты, сборка, Clippy, rustdoc, Loom, Miri, benchmark/example и Node-скрипты не запускались. Единственная разрешённая запись — этот отчёт и его отдельный commit.

**Дельта нового раунда:** изменения после ревизии предыдущего независимого отчёта `1e75569`, прежде всего `1e55c43..954f044`.

**Коррекция после аудита правок:** 2026-09-04 14:24:55 +02:00. P3-2 отозван после прямой проверки ordering pair на Rust 1.79 и 1.97; подробность и исправленная рекомендация находятся в исходном разделе P3-2 ниже. Остальные findings и GO-вердикт не изменились.

## Вердикт

**GO по production-коду и публикуемому crate artifact.** Новых P0, P1 или P2 не найдено. Повторный разбор packed-state, tag seal, H-2, CAS loops, release sequence, unsafe-границ, panic/error paths, package surface и новых коммитов не выявил нарушения soundness, ABA-защиты, lock-free progress или заявленного `no_std`/allocation-free контракта.

До финальной публикационной точки желательно закрыть три P3. Один из них — противоречие в current-state correctness tracker, один — зависимость «нулевой стоимости» test-инструментации от оптимизатора, один — нечестно обозначенная публичная semver-поверхность. Они не меняют нынешнюю корректность алгоритма, но их лучше не переносить через первую публикацию.

Сводка после коррекции P3-2: **P0: 0, P1: 0, P2: 0, P3: 3, P4: 4.**

## Что просмотрено

- `src/lib.rs` и `src/imp.rs` целиком: публичные типы/traits, bit packing, terminal seal, head/link ordering, оба CAS retry loop, backoff, unsafe-регионы и test-only probes.
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, package include/exclude и публичные feature/API boundaries.
- Все test/compile-fail/loom направления по дереву; подробно — boundary/property tests, tag seal, custom/narrow storage, threaded conservation и load-bearing Loom counterfactuals.
- Benchmark, latency example, A/B runner/templates, scratch-containment test и относящиеся к crate CI jobs.
- Последние коммиты `1e55c43`, `52c42ec`, `039a55a`, `d942ce2`, `fc79604`, `0aa3c8a`, `7e7956d`, `954f044` и их связь с замечаниями раунда 21.
- Актуальные карточки crate в `docs/perf/OPEN_ITEMS.md` и `docs/correctness-open-items/`.

Из-за запрета на под-агентов применён одноконтекстный bounded review. Async/network/crypto/FFI-модули не применялись: census исходников не обнаружил соответствующей поверхности; atomics, unsafe, API/semver, testing, semantics, lifecycle и performance разобраны явно.

## P3 — исправить или принять осознанным решением

### P3-1. Current-state correctness tracker продолжает объявлять уже исправленный crate `NO-GO`

`docs/correctness-open-items/TRACKED_publish_readiness.md:940-944` всё ещё содержит карточку 141 с заголовком `NO-GO`, помечает закрытым только P1-1, оставляет P1-2 `OPEN` и перечисляет открытыми все P2/P3/P4 старого раунда. Это уже не соответствует текущему дереву:

- конечный tag больше не оборачивается, а seal и честная ABA-формулировка закреплены в коде/документации;
- публичный `pack` проверяет обе половины;
- `test-internals`, release, unsupported-target, package и другие названные там пробелы получили CI/test coverage;
- panic-hook test восстанавливает прежний hook;
- item ссылается на устаревшие имена/пути и до-переделочный API.

Это не исторический архив: файл позиционируется как индекс текущего состояния. В результате официальный release record одновременно говорит `NO-GO`, тогда как текущие отчёты и код говорят `GO`; следующий исполнитель может повторно открыть закрытые задачи или неверно остановить выпуск.

**Рекомендация:** обновить карточку 141 по всем findings либо перенести её в resolved trail по принятому в репозитории правилу «headline истории не переписывается, current status исправляется». Для каждого старого P1/P2/P3 достаточно дать closure commit/current evidence; не оставлять старое `all OPEN`.

### P3-2 — WITHDRAWN. Perf item 63 предлагает допустимую CAS-ordering variant

`docs/perf/OPEN_ITEMS.md:2909-2949` предлагает ослабить только success ordering в `pop_index` с `Acquire` до `Relaxed` и добавить четвёртый A/B variant. Текущий CAS — `compare_exchange(head, new_head, Acquire, Acquire)` (`src/imp.rs:1628`). Failure `Acquire` здесь намеренно load-bearing: возвращённый `actual` сразу становится следующим observed head, по которому retry может читать link.

Первоначальный вывод выше был ошибочен: Rust разрешает failure ordering сильнее success ordering, пока failure не равен `Release` или `AcqRel`. Проверка при аудите правок на MSRV Rust 1.79 и текущем Rust 1.97 приняла `(success = Relaxed, failure = Acquire)` и в обоих случаях выдала LLVM `cmpxchg ... monotonic acquire`. Таким образом failure `Acquire` можно сохранить для retry proof, ослабив только success ordering. Устаревшая ссылка карточки на прежние строки CHANGELOG действительно требовала удаления, но не закрытия кандидата.

**Исправленная рекомендация:** оставить item 63 открытым и добавить четвёртую A/B variant перед запланированным arm64-прогоном. Это подтверждение допустимости ordering pair, а не доказательство ускорения; production hot path менять только по результатам gate. P3-2 больше не считается finding этого обзора.

### P3-3. Нулевая стоимость backoff-инструментации теперь гарантируется оптимизатором, а не cfg-формой исходника

После `fc79604` retry arms безусловно вызывают пустые в обычной сборке helpers `note_push_retry` / `note_pop_retry` и условно вызывают cap helpers (`src/imp.rs:104-139`, `1558-1564`, `1631-1642`). Кроме того, caller исполняет `if backoff.spin()` и в default build. Комментарий `src/imp.rs:107-109` называет это «no production code change»; commit сообщает о byte-identical release asm, что является хорошим evidence для проверенного оптимизированного профиля.

Но `#[inline]` — подсказка, не языковая гарантия. В debug или custom `opt-level = 0` библиотека компилируется профилем конечного binary, и вызовы пустых helpers плюс внешний cap branch могут сохраниться на каждом проигранном CAS. До рефакторинга instrumentation blocks отсутствовали из default MIR по `#[cfg]`. Это небольшой, contention-only риск, но он находится ровно в hot retry path и делает комментарий шире доказательства.

**Рекомендация:** вернуть `#[cfg(any(feature = "test-internals", loom))]` только вокруг двух instrumentation fragments в каждом retry arm, оставив единый `Backoff(u32)` и единый `spin`; либо доказать и зафиксировать codegen не только для release, но и для поддерживаемых low-opt профилей. Комментарий в любом случае сузить до фактически проверенного release codegen.

### P3-4. `TaggedIndex::empty()` — реальная публичная зависимость, скрытая как «не API»

`TaggedIndex::empty()` — `pub const fn` в default build под `#[doc(hidden)]` (`src/imp.rs:330-342`). Это не случайно доступный dead surface: `src/registry/bootstrap.rs:544,561` использует его из другого crate для const-capable loom shim. README одновременно признаёт, что item «not freely removable», и советует на него не полагаться (`README.md:287-290`).

Для crates.io `pub` остаётся semver-поверхностью независимо от `doc(hidden)`. Положение «нужен внешнему crate, но не является API и не имеет стабильности» особенно неудачно именно перед первой публикацией.

**Рекомендация:** принять одно из двух чистых решений до фиксации API: либо сделать функцию честным документированным public API с устойчивым именем/контрактом (например, как canonical bootstrap empty word), либо убрать межкрейтовую зависимость и снизить visibility. Текущую полупубличную форму не оставлять как молчаливый semver debt.

## P4 — качество и сопровождаемость

### P4-1. Excluded scratch test описывает невозможный packaged-test режим

`Cargo.toml:17-21` теперь исключает и `scripts/`, и `tests/tis_p3_ab_runner_scratch_guard.rs` из `.crate`. Однако комментарий самого исключённого теста (`tests/tis_p3_ab_runner_scratch_guard.rs:147-159`) всё ещё подробно объясняет, как этот test якобы запускается из packaged crate в `tagged-index-stack package gates`. После `7e7956d` этот execution path невозможен.

**Рекомендация:** переписать helper comment как checkout-only rationale или удалить ставшую ненужной packaged branch/history.

### P4-2. Финальная диагностика threaded conservation теряет число реально выполненных раундов

Под `test-internals` contention может повторяться до трёх раз (`tests/threaded_conservation.rs:217-238`), но итоговый conservation failure всё ещё сообщает только один шаблон `{NUM_THREADS} x {ITERS_PER_THREAD}` (`:275-280`). При провале после нескольких раундов сообщение занижает выполненную нагрузку и осложняет triage.

**Рекомендация:** сохранить `rounds_done` в общей переменной или сформировать total iterations/round count в message.

### P4-3. CI-комментарии остаются главным очагом review archaeology / «нейрослопа»

Например, `ci.yml:1862-1948` тратит десятки строк на старые номера раундов, удалённые test files и прежние allowlists вокруг трёх коротких test commands; `:3199-3255` смешивает действующий arm64 contract с историей появления job и будущей первой dispatch. Текущие инварианты присутствуют, но их приходится извлекать из хронологии.

**Рекомендация:** оставить рядом с job только current contract, почему существует каждый oracle и ссылку на ADR/report; task/run archaeology перенести в review/archive документы. Это снизит вероятность следующей неточной правки CI.

### P4-4. `test-internals` является публично включаемой feature/API-поверхностью, несмотря на отказ от semver guarantee

`Cargo.toml:57-65` объявляет обычную downstream-enableable feature, а включённые ею `pub #[doc(hidden)]` probes доступны потребителю. README (`:281-285`) говорит, что они не несут semver guarantee, но Cargo/rustc не проводят такой границы: downstream код технически может зависеть от feature и symbols.

**Рекомендация:** либо явно принять это как документированную unstable escape hatch и учитывать при изменениях, либо до первой публикации вынести интеграционные probes в crate-private/unit-test или отдельный непубликуемый harness. Это API-policy debt, не дефект default artifact.

## Проверка новых коммитов

### `fc79604` — core/backoff

Исправления двух неточных комментариев верны: shift overflow действительно наступил бы при `K = 32`, а преобразование empty-head sentinel в `TAIL` функционально обязательно. `Backoff(u32)` и pre-increment `at_cap` сохраняют saturating sequence `1,2,4,...,64,64...`; retry semantics не изменены. Единственный остаток — P3-3 о low-opt shape и слишком широком утверждении «no production code change».

### `0aa3c8a` и `d942ce2` — test dedup/oracles

Дедупликация pack boundaries сохранила отдельные positive/negative axes и property coverage. Объединение out-dir cases сохраняет postconditions для каждого значения. Bounded repeat у backoff-cap oracle улучшает вероятность активации, не превращая test в бесконечный retry. Найдено только диагностическое P4-2.

### `7e7956d` — package и CI

Исключение repository-only runner и зависимого от него scratch test согласовано: packaged suite больше не содержит target, которому не хватает `scripts/`. Arm64 summary теперь получает `--target aarch64-unknown-linux-gnu` и сверяет CSV того же запуска; artifact list включает summary. Остался устаревший комментарий P4-1, но functional package shape выглядит целостно.

### `1e55c43`, `52c42ec`, `039a55a`, `954f044` — docs/slop cleanup

Изменения преимущественно удаляют дублирование и хронологическую прозу. README стал API-first, CHANGELOG — кратким first-release inventory, unsafe-count details сведены к одному нормативному месту. Корректностные контракты не потеряны. Основной оставшийся шум теперь сосредоточен в CI и repository-only scratch test, а не в публикуемом API doc.

## Повторная проверка production-кода

### Packed word, seal и ABA

- `INDEX_BITS` жёстко ограничен `1..=16`; index sentinel не пересекается с допустимым индексом.
- Публичный `pack` проверяет index и tag, внутренний truncating helper закрыт и имеет debug assertions на оба предусловия.
- Каждый успешный push увеличивает tag; при `TAG_MAX` push возвращает `TagExhausted` до link-store/CAS и больше никогда не возобновляет запись.
- Последний pop сохраняет текущий tag в empty head (H-2), поэтому drain/refill не возвращает старое packed value.
- Empty sentinel и `TAIL` преобразуются явно в обе стороны; после правки документация больше не выдаёт это за косметику.

Статически повторяемого head word в lifetime объекта не найдено; классический ABA закрыт при соблюдении caller/implementor contracts.

### Atomics и progress

- Push записывает link до публикации head и делает Release CAS; начальный/failure head read может быть Relaxed, потому что push не разыменовывает защищённый payload.
- Pop начинает с Acquire head load. После lost CAS `Acquire` failure ordering импортирует новый observed head перед чтением его link на следующей итерации.
- Все изменения head — RMW, поэтому release sequence не разрывается на empty transitions.
- Link Acquire/Release сильнее минимально необходимого proof и служит defense-in-depth. Убирать это сейчас нельзя по интуиции: x86 codegen delta нулевой, AArch64 static delta реальный, native arm64 wall-clock ещё не зафиксирован.
- Strong-to-weak CAS уже имеет codegen-null evidence на исследованных lowering; оснований менять его без нового измерения нет.
- Backoff ограничен, локален одному вызову и не меняет lock-free property; starvation-free гарантия корректно не заявлена.

### Unsafe/API boundary

`StackStorage` остаётся unsafe trait с value-level обязанностями, которые невозможно проверить по одному impl: один стабильный head↔links binding на весь lifetime, один mapping, допустимый domain, атомарные dedicated cells, disjoint reachable populations и корректные stored values. Все три hooks — `unsafe fn`; push также `unsafe fn` и требует уникальной publish/recycle authority. Crate-owned blanket `StackOps` не позволяет downstream подменить CAS loop.

Все production unsafe-регионы локализованы и имеют site-specific `SAFETY` reasoning. Raw pointers, transmute, FFI, ручные `Send`/`Sync`, lifetime fabrication и resource-owning `Drop` отсутствуют. `ArrayIndexStack` не выдаёт наружу head↔links binding и не реализует публичный storage trait, поэтому прежний safe shared-head repro не выражается.

Public blanket impl — осознанный coherence commitment: он ограничит будущую эволюцию trait, но для первой публикации соответствует цели иметь ровно один crate-owned алгоритм. Нового overlap/semver дефекта в impl set не найдено.

### Errors, panics и corruption boundary

- `TagExhausted` возвращается до side effects.
- Numeric index guard выполняется до storage access, но не притворяется доказательством более узкого implementor domain; это честно находится в unsafe caller contract.
- Pop release-active проверяет out-of-range link и self-loop. Более глубокие in-range cycles не обещано обнаруживать; запрет лежит в unsafe ownership contract.
- Advisory `is_empty`/`pushes_remaining` описаны как racing snapshots, а не reservations.

## Производительность: что реально стоит исследовать

Без нового измерения production hot path менять не рекомендую. Две валидные возможности уже корректно отделены от текущего кода:

1. `docs/perf/OPEN_ITEMS.md` item 61 — не повторять `store_next(index, same_next)` после lost push CAS, если observed successor не изменился. Нужны отдельная A/B variant, activation counter «store действительно пропущен» и weak-memory proof; нынешний runner эту variant не содержит.
2. Item 62 — `load_next`/`store_next` Acquire/Release → Relaxed. На x86 это codegen-null, на AArch64 удаляет реальные `ldar/stlr`; решение должно ждать native arm64 wall-clock и Loom/counterfactual пакет.

3. Item 63 — `pop_index` CAS success `Acquire` → `Relaxed` при сохранении failure `Acquire`. Rust 1.79/1.97 принимают эту пару; нужен отдельный A/B arm и target-specific codegen/wall-clock evidence.

Padding `ArrayLinks` внутри crate без consumer profile нецелесообразен: он раздует footprint до 16 раз; slot-resident mapping/padding должен оставаться решением владельца storage по фактическому false-sharing профилю.

## Тестовая и публикационная поверхность — статическая оценка

Набор тестов содержит осмысленные независимые oracles для:

- checked pack/unpack и граничных widths/tags;
- LIFO, empty/H-2, lazy links и terminal seal;
- custom storage и узкого unchecked domain под Miri;
- unsafe-call/unsafe-impl/coherence/invalid-width compile-fail форм;
- threaded conservation плюс retry/backoff activation;
- real-type Loom models, release-sequence paths и намеренно ломающиеся counterfactuals;
- package isolation, unsupported 64-bit-atomic target, default/release/test-internals, no_std, MSRV, Clippy/rustdoc и native arm64 measurement workflow.

Обычная сборка не имеет normal runtime dependencies; loom optional и требует согласованной cfg+feature формы. Manifest metadata, dual license, categories, README и package exclusions выглядят согласованными. Repository-only Node runner больше не загрязняет tarball.

Это только проверка структуры и test-oracle logic. **Ни один test/gate в этом раунде не запускался**, поэтому отчёт не заменяет свежий зелёный CI run и не утверждает фактический runtime pass на ревизии `954f044`.

## Финальный вывод

`tagged-index-stack` **готов к публикации по состоянию production-кода**. Алгоритмического, soundness- или package-blocker уровня P0–P2 не найдено; последние исправления не внесли видимой semantic regression.

Перед финальной точкой особенно полезно закрыть P3-1, P3-3 и P3-4. P3-2 отозван этой коррекцией: item 63 остаётся допустимой, но пока не измеренной perf-возможностью. После исправления трёх действующих P3 остаток — P4-сопровождаемость и measurement-only perf backlog, а не риск корректности публикуемого stack.
