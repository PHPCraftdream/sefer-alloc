# `tagged-index-stack`: предрелизное ревью — Sol-codex, прогон 9

- Время начала исследования: 2026-09-03 08:02:50 CEST (`Europe/Berlin`)
- Метка запроса: 08:02
- Проверенный `HEAD`: `4713f1213d0c91f49015806969681ed46ac89e84`
- Последний кодовый коммит крейта: `ec966d058019e1af186c2d540d6f3dd232d57d46`
- База предыдущего Sol-codex-прогона: `dcb742c96a00a016ff05aae78fed930cf9e22a7f`
- Режим: самостоятельное статическое ревью, без агентов и под-агентов
- Не запускались: `cargo`, тесты, doctest, clippy, rustdoc, loom, Miri, benchmarks, examples и scripts
- Единственное изменение репозитория в рамках ревью: этот Markdown-отчёт и его коммит

## Вердикт

**NO-GO к публикации в текущем виде.**

Исправление полного оборота тега по существу корректно: текущий production-алгоритм больше не переиздаёт старое `(index, tag)`-состояние. Успешный push с `tag = TAG_MAX - 1` устанавливает `TAG_MAX`, все последующие push отказывают до каких-либо действий, pop сохраняет тег и продолжает дренировать уже опубликованную цепочку. Линеаризация, release-sequence и domain/liveness-контракт в основном согласованы.

Но новая тестовая поверхность повторно открыла soundness-дыру: обычный, доступный downstream-потребителю Cargo feature `test-internals` экспортирует **safe** write-hook, способный испортить связанную с `ArrayIndexStack` цепочку и заставить последующие safe `pop()` повторно выдать индекс после полностью корректных unsafe push. `#[doc(hidden)]` и имя `_for_test` не являются границей безопасности. Это P1-блокер.

Кроме того, новый A/B build-check зелёный при проигнорированных `Result` всех трёх push, критические non-loom тесты terminal seal не исполняются ни одной CI-командой, а несколько текущих публичных/внутренних описаний всё ещё утверждают прямо противоположное новому алгоритму — что тег оборачивается. До исправления P1 и трёх P2 публикацию не рекомендую.

## Область и полнота проверки

Прочитаны текущие production-файлы `src/lib.rs` и `src/imp.rs`, manifest, README/CHANGELOG, изменения после прогона 8, новый `tag_seal` oracle и дополнения loom-модели, compile-fail/custom-storage/narrow-domain/threaded тестовая поверхность, bench и latency example, A/B driver/template, релевантные CI jobs и production consumer `sefer-alloc::Registry` с loom shim.

Проверены категории `rust-intel`, применимые к этому крейту: unsafe boundary и локальные unsafe-операции, atomic publication/release sequence, числовые границы и shifts, публичный blanket impl/coherence, error/API contracts, тестовые оракулы, feature/package surface и соответствие заявленным гарантиям.

Это bounded single-context review. Generated fixture lockfiles и сохранённые perf-логи проверялись как поверхность зависимостей/доказательств, но не перечитывались построчно. После механического census не углублялся в отсутствующие здесь классы: async/cancellation, FFI, raw-pointer ownership, внешние RAII-ресурсы, serde, crypto, сеть и файловые протоколы. Никакие динамические утверждения независимо не перепроверялись запуском — по прямому требованию пользователя.

## Блокирующая находка

### P1-1. `test-internals` публикует safe link-writer, нарушающий exclusive issuance

Места:

- `crates/tagged-index-stack/src/imp.rs:1782-1797` — `ArrayIndexStack::store_next_for_test(&self, index, next)` безопасен, `pub`, gated не только `cfg(loom)`, но и обычным `feature = "test-internals"`;
- `crates/tagged-index-stack/Cargo.toml:89-106` — feature объявлен в публикуемом manifest и может быть включён любым downstream-потребителем;
- `crates/tagged-index-stack/README.md:221-260` — README подчёркивает, что `#[doc(hidden)]` само по себе не закрывает API, но перечисляет только read-only probes и не упоминает новый write-hook;
- `crates/tagged-index-stack/tests/loom_aba.rs:1189-1200` — единственный найденный реальный consumer write-hook находится под `cfg(loom)` и намеренно обходит алгоритм.

Статический контрпример при включённом `test-internals`:

1. Создать `ArrayIndexStack::<16, 4>`.
2. Корректно выполнить `unsafe push(0)` и `unsafe push(1)`: оба индекса в domain, оба до push не live. Получается `1 -> 0 -> TAIL`.
3. Из safe-кода вызвать `store_next_for_test(0, 1)`. Получается цикл `1 -> 0 -> 1`, причём self-loop detector его не видит.
4. Три safe `pop()` возвращают `1`, затем `0`, затем снова `1`. Один индекс выдан двум владельцам.

Ни один caller-side safety contract двух исходных push не нарушен. Инвариант ломает последующий safe публичный метод самого крейта. Для контейнера индексов это логическая порча; для документированного allocator consumer — путь к двум владельцам одного слота и UB за пределами крейта. Поэтому проблема относится к soundness feature-конфигурации, а не к «неподдерживаемому тестовому удобству».

Лучшее исправление: сузить `store_next_for_test` до `#[cfg(loom)]`, поскольку единственный текущий consumer именно loom-counterfactual. Если normal-build hook действительно понадобится, он обязан быть `unsafe fn` с точным `# Safety`, запрещающим использование на live binding без внешней эксклюзивности. Одновременно синхронизировать Cargo/README inventory. Просто добавить предупреждение в prose недостаточно.

## Существенные исправления до публикации

### P2-1. A/B harness игнорирует `TagExhausted`, а новый build-check принимает предупреждение

Места:

- `crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs:98-107` — prefill игнорирует `Result`;
- `.../harness_bin.rs:143-155` — warm-up pop/re-push игнорирует `Result`;
- `.../harness_bin.rs:166-180` — counted loop игнорирует `Result`, после чего безусловно увеличивает `ops`;
- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:836-876` — `build-check` вызывает plain `cargo build` и проверяет только exit status;
- `crates/tagged-index-stack/scripts/tis_p3_ab/scratch_Cargo.toml.tmpl:1-31` — `unused_must_use`/warnings не подняты до deny;
- `.github/workflows/ci.yml:798-814` — regular-CI gate обещает ловить drift template/API, но warnings его не роняют.

После миграции `push -> Result<(), TagExhausted>` выражение в каждом unsafe-блоке даёт `unused_must_use`, но warning не меняет код возврата `cargo build`. Поэтому свежедобавленный gate уже сейчас принимает нарушенный контракт.

При достижении terminal seal harness извлекает индекс, не возвращает его, а counted loop всё равно записывает успешную операцию. Дальше free-list дренируется, и сравнение вариантов измеряет всё меньше реальных churn-операций. Для текущего короткого процесса 48-битный бюджет практически не исчерпывается, но это не оправдывает ложный measurement oracle и CI, зелёный на предупреждении о потере результата.

Исправление: на всех трёх call sites явно обработать результат, предпочтительно `.expect("bounded measurement run never reaches TAG_MAX")`, чтобы невозможность для заданного окна была исполняемым утверждением. В scratch manifest дополнительно поставить `unused_must_use = "deny"` либо сделать build-check warning-hard; иначе следующий `must_use` drift снова пройдёт.

### P2-2. Основные non-loom seal-тесты добавлены, но ни разу не исполняются в CI

Места:

- `crates/tagged-index-stack/tests/tag_seal.rs:54-161` — `Ok, Ok, Err`, неизменность head при отказе, post-seal drain и permanence находятся под `feature = "test-internals"`;
- `.github/workflows/ci.yml:1832,1843` — общие debug/release test rows идут с default features, поэтому эти тесты compiled out;
- `.github/workflows/ci.yml:1866,1878` — feature включён только для `threaded_conservation` и `stack_unit`; `tag_seal` не выбран;
- `.github/workflows/ci.yml:2164` — clippy с feature только компилирует target и не исполняет assertions;
- `.github/workflows/ci.yml:2717` — loom запускает отдельный concurrent seal model, но не заменяет профиль обычных atomics и API-test из `tag_seal.rs`.

В результате новая single-threaded regression suite центрального soundness-исправления присутствует в дереве, но её substantive tests не запускаются. Default build выполняет лишь ungated арифметический `tag_max_is_the_exact_pack_ceiling`.

Предпочтительное исправление против будущего list drift: вместо точечного списка feature-тестов запускать `cargo test -p tagged-index-stack --release --features test-internals --no-fail-fast`. Если стоимость мешает — добавить хотя бы отдельный `--test tag_seal`, но общий feature-row надёжнее и автоматически подхватит будущие gated tests.

### P2-3. Текущая документация одновременно обещает non-wrapping seal и описывает wrapping tag

Прямые противоречия текущему коду:

- `crates/tagged-index-stack/src/lib.rs:61-68` — публичный crate rustdoc называет tag wrapping;
- `crates/tagged-index-stack/src/imp.rs:110-115` — публичный `TaggedIndex` rustdoc делает то же;
- `crates/tagged-index-stack/src/imp.rs:222-258` — комментарий `pack`/`pack_truncating` утверждает, что hot push намеренно передаёт `2^TAG_BITS`, а truncation является wrap-механизмом; актуальный `push_index_impl:1357-1367` возвращает `Err` до этого;
- `crates/tagged-index-stack/src/imp.rs:1605-1611` — публичный `ArrayIndexStack` снова описан как wrapping/mitigating и с residual wrap bound;
- `crates/tagged-index-stack/README.md:41-48` — раздел packed word говорит wrapping, хотя первые строки README обещают strictly monotonic;
- `crates/tagged-index-stack/CHANGELOG.md:32-40` — net-state первого, ещё не выпущенного релиза утверждает wrap и «behaviorally unchanged», тогда как `:234-252` позже утверждает обратное;
- `crates/tagged-index-stack/Cargo.toml:10` — crates.io description всё ещё позиционирует tag только как ABA-mitigating.

Это не косметика: stale комментарий у `pack_truncating` описывает именно запрещённое действие, возврат которого вновь откроет P1 из прогона 8. Публичные описания одновременно дают потребителю две несовместимые модели отказа и ABA.

Исправление: представить в README/rustdoc/manifest один net contract — non-wrapping monotonically increasing tag, terminal `TagExhausted`, pops-after-seal. В private helper явно записать, что production callers доказывают `tag <= TAG_MAX`, а push вызывает helper только после проверки `tag < TAG_MAX`; truncation больше не является механизмом wrap. Для первого unreleased changelog лучше описать итоговый 0.1.0, а историю промежуточной wrapping-реализации оставить в ADR.

## Неблокирующая инженерная находка

### P3-1. Документация остаётся слишком большой и самодублирующейся для размера алгоритма

`src/imp.rs` содержит 2073 строки, `src/lib.rs` — 490, README — 286, CHANGELOG — 312. Существенная часть — повторяющиеся proof narratives, точные ручные unsafe-counts, имена внутренних тестов/ревью и исторические состояния. `StackStorage` rustdoc после полезного сокращения всё ещё занимает примерно `imp.rs:573-919` (около 347 строк).

Нормативная подробность unsafe contract оправдана и должна остаться рядом с API. Но текущая кратность копий уже породила P2-3 и неполный inventory P1-1 сразу после исправляющего коммита. Это практический maintenance defect, а не вкусовое замечание.

Рекомендация: в публичном rustdoc оставить компактные нормативные `# Safety`, ordering contract, error/panic behavior и таблицу hazard/detection boundary. Полноту census, историю ревью, доказательные walk-through и привязку к конкретным тестовым именам хранить в одном ADR. CHANGELOG первого релиза должен описывать итог, не журнал каждого внутреннего передизайна.

## Общий обзор актуального кода

### Что сделано хорошо

- `eba76e4`: terminal seal закрывает полный оборот тега без вероятностной оговорки. Проверка стоит до первого link-write на первой попытке; loser после CAS-race может оставить только непубличный stale link собственного индекса.
- Поп после seal корректно дренирует цепочку с неизменным `TAG_MAX`; tag не может вернуться к старому значению, поэтому parked stale CAS не воскресает.
- `pushes_remaining = TAG_MAX - unpacked_tag` не переполняется: unpack всегда ограничен шириной tag. `TAG_MAX + 1` в test при допустимых ширинах также помещается в `u64` (`TAG_BITS <= 63`).
- H-2 сохранён: переход к empty сохраняет running tag. Все записи head остаются RMW, поэтому release sequence не разрывается.
- Push: initial/failure `Relaxed`, success `Release`; pop: initial/failure/success `Acquire`. Для текущего proof это согласовано: push не следует по link, pop следует; failed-pop CAS обязан дать Acquire перед следующим `load_next`.
- `f3a2ff6`: `#![deny(unsafe_op_in_unsafe_fn)]` действительно локализовал три прежде ambient unsafe calls. Текущий production census согласован: 1 `unsafe trait`, 10 `unsafe fn`, 6 явных unsafe blocks, 8 item-scoped allow regions, без raw pointers, FFI и manual `Send`/`Sync`.
- Link-domain numeric guard и `pop`-guard активны в release; narrowing в `unpack` обоснован `INDEX_BITS <= 16`; shift counts compile-time bounded.
- Публичный `StackOps` blanket impl намеренно закрывает переопределение CAS-loop downstream-ом. Это широкое coherence/semver-решение, но соответствует заявленной архитектуре и не обнаружено как дефект.
- Normal build остаётся `no_std`, allocation-free и без normal third-party dependencies. Loom/instrumentation gated; unsupported 64-bit-atomic target получает именованный compile error.
- Production consumer `Registry` обработал новый `Result`: при недостижимом в практическом окне seal он осознанно оставляет refused slot владельцу/теряет его из recycler вместо паники в allocator path; поведение подробно задокументировано. Это availability policy, не нарушение soundness крейта.
- `ec966d` корректно устранил default-feature unused imports в `tag_seal.rs`.

### Что последние правки закрыли лишь частично

- `fdc1650` восстановил unsafe call boundary A/B harness и подключил regular build path, но не обработал новый `Result` и не сделал gate warning-hard — P2-1.
- `eba76e4` исправил production full-wrap hole и добавил хорошие real-type/counterfactual oracles, но одновременно вынес safe mutating hook в normal Cargo feature и не подключил substantive non-loom oracle к CI — P1-1/P2-2.
- `f3a2ff6` полностью закрывает прежние implicit unsafe operations.
- `ccd160e` правильно заменяет «compiler-checked contract» на «compiler-enforced boundary» и сокращает hazard appendix, но current-state wrapping-текст и объём дублирования остались — P2-3/P3-1.
- `4713f12` — только checkpoint, production-код крейта не меняет.

## Производительность и возможности ускорения

Нового бесспорного hot-path ускорения, которое можно рекомендовать без измерения, статическое ревью не выявило.

Наиболее перспективные кандидаты уже видны в коде:

1. `StackStorage::{load_next,store_next}` / `ArrayLinks` используют Acquire/Release, хотя публикация link уже обеспечивается relaxed link-write, sequenced-before Release head CAS, и Acquire observation head перед чтением link. Ослабление link cells до Relaxed убирает `ldar/stlr` на AArch64, но менять public ordering contract следует только после исправления P2-1 и native weak-memory wall-clock A/B.
2. Success ordering pop-CAS теоретически может быть Relaxed: соответствующий head уже Acquire-наблюдался initial load либо failure CAS, а RMW с Relaxed store-half продолжает release sequence. На x86 это, вероятно, codegen-null; на AArch64 нужен отдельный proof oracle и измерение.
3. Strong→weak CAS по сохранённым codegen evidence сейчас не даёт выигрыша; менять ради стиля не нужно.
4. Backoff cap уже является измеренным throughput/fairness/tail trade-off. Универсально «ускорить» его без смены требований нельзя.
5. Dense `ArrayLinks` даёт потенциальное false sharing, но blanket padding увеличит footprint в 16 раз. Правильная оптимизация остаётся workload-specific `StackStorage` с slot-resident/padded/sharded links.
6. Проверка `tag == TAG_MAX` добавляет одну почти всегда предсказанную ветвь на push, но является soundness-механизмом и не кандидат на удаление. `wrapping_add` после guard можно стилистически заменить на `+ 1`, однако ожидаемый codegen тот же и выигрыша нет.

## Unsafe/system census

- Production `src/`: 1 unsafe trait; 10 unsafe-fn declarations; 6 explicit unsafe blocks; 8 local allow regions.
- Каждый production unsafe block имеет локальное `SAFETY`-обоснование; `unsafe_op_in_unsafe_fn` запрещён.
- Нет raw pointer dereference/arithmetic, FFI, manual `Send`/`Sync`, transmute, union и ownership-reclamation внутри крейта.
- Главная soundness-граница семантическая: стабильность head↔links binding, atomic dedicated cells, domain и liveness.
- Единственная найденная новая брешь границы — safe feature-gated write напрямую в связанный `ArrayIndexStack` (P1-1).
- Public blanket impl (§C1) рассмотрен и признан намеренным coherence seal; расширять trait новыми overridable semantics без нового API-анализа не следует.

## Короткий путь к GO

1. Убрать `store_next_for_test` из `test-internals` normal surface: оставить только под `cfg(loom)` либо сделать unsafe с полным контрактом.
2. Обработать все три `Result` в A/B template и сделать scratch build warning-hard как regression oracle.
3. Запускать весь `test-internals` test set в CI либо минимум новый `tag_seal` target.
4. Удалить все current-state утверждения о wrapping и свести unreleased changelog к итоговой модели terminal seal.
5. После изменений заново проверить статически feature/API surface, atomic proof, loom/non-loom oracles, package contents и root Registry. Динамическую верификацию должен выполнить обычный release process; в этом прогоне она намеренно не запускалась.

После закрытия этих пунктов архитектура выглядит близкой к GO: прежний фундаментальный full-wrap defect исправлен, а оставшийся P1 локален и имеет простое, не влияющее на production hot path решение.
