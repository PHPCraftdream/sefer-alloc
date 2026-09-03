# tagged-index-stack — предрелизное статическое ревью

**Ревьюер:** Sol-codex

**Раунд:** Codex run 20

**Метка пользователя:** 2026-09-04 00:24

**Время фиксации ревью:** 2026-09-04 00:41:30 +02:00 (Europe/Berlin)

**Проверенная ревизия:** `d510da3961b4f64df081f7035434ca8f453db465`

**Новые изменения после раунда 19:** `4ef9b8170cc010c5fc420df1bfc942936f5aa0b3..d510da3961b4f64df081f7035434ca8f453db465`

## Вердикт

**GO по результатам статического ревью.** Блокирующих P0, P1 и P2 не найдено. Исправление
единственного P2 из раунда 19 корректно: symlink fixture теперь использует ещё не существующий
child path, положительно проверяет тип созданной ссылки/junction и действительно может дойти до
запуска runner. Production-алгоритм, unsafe-контракты, sealing тега и memory-ordering по текущему
коду выглядят согласованными и sound при соблюдении опубликованных предусловий.

Остались четыре неблокирующих P3 и два P4. Они не указывают на обнаруженную ошибку lock-free
стека, но ослабляют защиту от будущих регрессий и усложняют проверку доказательств. Наиболее
полезные улучшения перед публикацией: исполнять Windows-ветку scratch-guard в CI, запускать
`--mode summary` в arm64 evidence job и добавить тест жизненного цикла временного дерева.

Это **статический** GO: по прямому требованию тесты и другие исполняемые проверки в этом раунде не
запускались. Фактическое состояние удалённого CI данным отчётом не подтверждается.

## Приоритеты

| Уровень | Количество | Смысл |
|---|---:|---|
| P0 | 0 | критических дефектов/soundness holes не найдено |
| P1 | 0 | вероятных серьёзных runtime-дефектов не найдено |
| P2 | 0 | релизных блокеров не найдено |
| P3 | 4 | неблокирующее усиление CI, оракулов и invariant guards |
| P4 | 2 | точность сообщений и сокращение доказательного шума |

## Режим и охват

Ревью выполнено лично, без под-агентов. Просмотрены с нуля:

- весь production source `src/lib.rs` и `src/imp.rs`;
- публичные типы, методы, unsafe trait и blanket implementation;
- packed arithmetic, sentinel/index/tag bounds и необратимое seal-состояние;
- push/pop CAS loops, release sequence, ordering каждой атомарной операции и backoff;
- unit, property, threaded, compile-fail и Loom test oracles;
- benchmark, example, A/B harness, Node.js runner и scratch-guard;
- `Cargo.toml`, README, CHANGELOG, package/release surface и относящиеся к крейту CI jobs;
- все три новых коммита и полный diff после отчёта раунда 19.

Код не исполнялся. Не запускались `cargo`, `rustc`, fmt, clippy, rustdoc, тесты, Loom, Miri,
benchmarks, examples, Node.js runner, package/publish и сгенерированные binaries. Использовались
только чтение файлов, поиск, просмотр Git history/diff и статический `git diff --check`.

В production surface нет async, сетевого/файлового I/O, FFI, raw-pointer dereference, ручных
`Send`/`Sync`, crypto, serialization и resource-owning `Drop`; отдельного риска этих классов у
крейта нет.

## P3 — неблокирующие замечания

### P3-1. Windows-ветка нового junction regression test не исполняется в CI

**Места:**

- `.github/workflows/ci.yml:736-809` — package gates работают на `ubuntu-latest`;
- `.github/workflows/ci.yml:1791-1825,1854-1899` — собственные test rows крейта также Linux-only;
- `.github/workflows/ci.yml:1623-1636` — Windows job запускает только root package;
- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:372-419,484-518`.

Исправленный тест имеет отдельную Windows-реализацию: сначала `symlink_dir`, затем fallback на
`cmd /c mklink /J`. Однако ни одна Windows job не запускает тесты package
`tagged-index-stack`. Команда `cargo test` из корня репозитория тестирует root package, что прямо
зафиксировано комментарием самого workflow; dependency при этом компилируется, но её integration
tests не исполняются.

Статически ветка выглядит корректно. Дополнительно проверено, что Rust 1.79 считает Windows
name-surrogate reparse points, включая mount-point junction, symlink для `FileType::is_symlink`,
поэтому новая положительная проверка fixture не должна ложнопадать только из-за fallback:
[реализация std 1.79](https://github.com/rust-lang/rust/blob/1.79.0/library/std/src/sys/pal/windows/fs.rs#L967-L994).
Но это рассуждение не заменяет реальный запуск на целевой ОС, особенно после изменения именно
Windows-specific защитного сценария.

**Рекомендация:** добавить в `windows-latest` как минимум
`cargo test -p tagged-index-stack --test tis_p3_ab_runner_scratch_guard`, с доступным Node.js;
предпочтительнее — полный default test suite крейта, если стоимость приемлема.

### P3-2. Единственный независимый ratio-oracle не входит в arm64 CI job

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:1200-1235`;
- `.github/workflows/ci.yml:3184-3194,3214-3233`;
- `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` — manual reproduction включает summary mode.

Коммит `24944ee` правильно удалил тавтологические проверки ratio. Полезная проверка теперь живёт
в `modeSummary`: она заново получает median из sample rows, вычисляет ratio и сверяет его с
`ratio_vs_base`, записанным producer leg. Но manual arm64 job запускает только `codegen` и
`wallclock`, после чего загружает artifacts. `--mode summary` там отсутствует. Значит job может
стать зелёной, не исполнив единственную независимую проверку сериализованного ratio.

Это не делает измерение неверным сегодня: wallclock producer всё ещё пишет samples и summary, а
документированный ручной процесс отдельно предусматривает агрегацию. Проблема — в границе
автоматического gate: зелёный job доказывает меньше, чем доступный runner уже умеет доказать.

**Рекомендация:** после wallclock step и до upload выполнить
`node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode summary`. Если summary намеренно
остаётся только release-операцией, это следует назвать явно и не описывать весь manual job как
полную проверку ratio.

### P3-3. Новый cleanup-контракт runner не имеет regression oracle

**Места:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:83-100,233-239,1242-1272`;
- `crates/tagged-index-stack/tests/tis_p3_ab_runner_scratch_guard.rs:239-250,392-518`.

Текущая реализация хорошая: `fail()` бросает исключение вместо `process.exit`, top-level
`try/catch/finally` очищает собственный `mkdtemp` root при успехе, ожидаемой ошибке и неожиданном
exception; `--keep-scratch` — явный opt-out. `freshDir` теперь fail-if-exists и больше не удаляет
рекурсивно дочерние пути.

Но scratch-guard tests доказывают containment и сохранность canary за symlink/junction, а не новый
lifecycle contract. Нет теста, который снимает snapshot `target/tis_p3_ab-*`, выполняет успешный
и детерминированно падающий запуск и доказывает отсутствие нового root; также не закреплена
обратная семантика `--keep-scratch`. Возврат `process.exit`, перенос cleanup обратно только в
success path или ошибка в catch/finally пройдут существующую suite, хотя именно такой leak был
исправлен в этом раунде.

**Рекомендация:** в disposable skeleton добавить три оракула:

1. успешный `build-check` не оставляет нового scratch root;
2. управляемая ошибка после `mkdtemp` также не оставляет root;
3. та же ошибка с `--keep-scratch` оставляет ровно один принадлежащий тесту root, который test
   guard затем безопасно удаляет.

### P3-4. `pack_truncating` проверяет только одну из двух заявленных границ

**Место:** `crates/tagged-index-stack/src/imp.rs:278-337`.

Документация приватного fast path говорит, что обе половины должны быть в диапазоне, отдельно
подробно доказывает `tag <= TAG_MAX` и утверждает, что range proof дополнительно ловится
`debug_assert!`. В теле проверяется только индекс:

```rust
debug_assert!(index as u64 <= Self::INDEX_MASK, ...);
(tag << INDEX_BITS) | (index as u64)
```

Для `tag` соответствующего `debug_assert!(tag <= Self::TAG_MAX)` нет. Все текущие call sites
действительно устанавливают нужную границу: `empty()` использует ноль, push проверяет seal до
инкремента, pop переупаковывает допустимый tag. Поэтому текущего runtime bug здесь не найдено.
Однако helper специально назван опасным: будущий caller с tag выше поля получит валидно выглядящее
слово с молча отброшенными high bits и может вернуть ABA wrap. Именно такой maintenance drift
дешёвый debug guard должен превращать в раннее падение.

**Рекомендация:** добавить второй `debug_assert!(tag <= Self::TAG_MAX, ...)`. Release hot path не
изменится, а реализация начнёт соответствовать собственному контракту.

## P4 — малые неточности и smell

### P4-1. Success message противоречит `--keep-scratch`

**Места:** `tis_p3_ab_runner.mjs:832,1038,1117,1267-1269`.

Каждый mode до входа в `finally` печатает `scratch tree removed on exit`, даже когда пользователь
передал `--keep-scratch`. Затем `finally` честно печатает, что дерево оставлено. Итоговый лог
содержит два несовместимых утверждения. На результат gate это не влияет, но ухудшает диагностику и
может обмануть обработчик логов.

**Рекомендация:** формировать mode message условно (`will be removed` / `kept by request`) либо
вообще оставить сообщение о lifecycle только в `finally` после фактического действия.

### P4-2. Доказательная история всё ещё смешана с нормативным кодом

Крупнейшие единицы — примерно 2.3k строк в `src/imp.rs`, 1.5k в одном Loom test и 1.2k в runner;
статический поиск находит более ста строк с review/task/run/group identifiers в source, tests,
scripts, README, CHANGELOG и manifest. Краткие safety clauses и ordering proofs здесь оправданы,
но значительная часть текста пересказывает историю прежних решений, номера внутренних задач и
давно закрытые контрфактуалы.

Это уже не только эстетика: важная норма теряется в историческом повествовании, comments требуют
синхронного обновления в нескольких местах, а ревьюеру труднее отличить действующий contract от
археологии. Свежие ошибки прошлых раундов возникали именно в доказательной инфраструктуре, а не в
нехватке ещё одного длинного комментария.

**Рекомендация:** оставить рядом с unsafe/atomic кодом только локальный нормативный contract и
короткое доказательство; историю раундов, номера задач, отвергнутые варианты и measurement receipts
перенести в ADR/perf/review документы. Большие test-файлы делить по одному типу свойства или
контрфактуала без дублирования harness helpers.

## Проверка новых коммитов после раунда 19

### `9b76d18` — ремонт symlink fixture

Исправление P2 корректно. Link path теперь является несуществующим child внутри эксклюзивно
созданного parent; перед созданием это assert-ится, после создания положительно проверяется
`symlink_metadata().file_type().is_symlink()`. Canary assertions и запуск real runner достижимы.
Unix symlink и Windows junction fallback не смешивают владение link path с владением temp root.

### `24944ee` — runner cleanup, fail-if-exists и ratio assertions

Содержательная часть корректна:

- `RunnerFatalError` сохраняет диагностируемый exit code, но больше не обходит RAII-подобный
  `finally`;
- cleanup централизован и касается только собственного случайного `mkdtemp` root;
- `freshDir` перестал быть destructive reset и громко отвергает повторное имя;
- тавтологические ratio assertions удалены, независимая сверка samples с summary сохранена;
- документация обновлена с фиксированного path на per-invocation path.

Оставшиеся пробелы — не дефекты самой правки, а отсутствие CI-вызова summary (P3-2), regression
oracle для lifecycle (P3-3) и неточное сообщение при `--keep-scratch` (P4-1).

### `d510da3` — checked `Instant` в standalone benchmark

Обе прежние операции `Instant + Duration` заменены единым `checked_deadline_add`; failure имеет
понятную диагностику и не зависит от debug/release overflow behavior. Helper находится вне
измеряемого inner loop, поэтому заметного benchmark overhead не добавляет.

## Общий аудит production-кода

### Packed state и границы

- `INDEX_BITS` ограничен диапазоном `1..=16`; compile-time guards централизованы.
- `INDEX_MASK` одновременно задаёт ширину индекса и зарезервированный empty sentinel.
- Публичный `pack` проверяет обе половины и разрешает sentinel только как корректную H-2 форму.
- `unpack` маскирует индекс до lossless `u32` и сдвигает tag без sign/width неоднозначности.
- Tag строго монотонен и никогда не оборачивается: при `TAG_MAX` push возвращает
  `TagExhausted`, seal остаётся постоянным и drain всё ещё возможен.
- Advisory APIs `is_empty` и `pushes_remaining` честно документируют race semantics; их нельзя
  принять за reservation или proof будущего успеха.

Единственный найденный guard gap — P3-4; текущие call sites его предусловие соблюдают.

### Lock-free алгоритм и memory ordering

Статический разбор не нашёл разрыва happens-before:

- push сначала публикует link-cell Release-store, затем меняет head Release-CAS;
- pop импортирует head Acquire-load, читает link Acquire и завершает Acquire-CAS;
- при проигрыше CAS следующий observed head поступает из failure Acquire ordering;
- все успешные head mutations образуют непрерывную RMW release sequence;
- старый popper может прочитать более поздний link, но его CAS со старым tag обязан проиграть;
- первоначальный Relaxed load push используется только как CAS operand и link value, не как
  импорт защищённых данных;
- strong CAS выбран осознанно; имеющиеся codegen artifacts показывают отсутствие выигрыша от weak
  для исследованных lowering, поэтому слепая замена не обоснована;
- bounded spin backoff не влияет на correctness и не содержит неограниченного busy loop.

Seal устраняет повтор packed-head value в течение жизни объекта и тем самым закрывает классический
ABA при соблюдении ownership contract на index. Lock-freedom сохраняется: неуспех CAS означает
прогресс конкурента; достижение terminal tag является документированным исчерпанием ресурса, а не
зависанием.

### Unsafe/API surface

Все реальные unsafe-регионы просмотрены. Основная граница — публичный `unsafe trait StackStorage`
с тремя hooks (`head`, `load_next`, `store_next`) и unsafe push operation. Contracts описывают:

- стабильность head и mapping index → link-cell;
- допустимый домен индексов и reserved sentinel;
- отсутствие параллельного/повторного push одного ownership epoch;
- ordering publication относительно head;
- обязанность caller не переиспользовать выданный index до возврата владения.

Unsafe blocks локальны и сопровождаются `SAFETY`-обоснованиями. Raw pointers, ручные auto-trait
impls и lifetime fabrication отсутствуют. Safe `ArrayIndexStack` не выставляет наружу опасный
storage implementation; его `push` остаётся unsafe, потому что никакая локальная проверка не может
доказать уникальность внешнего ownership epoch.

Публичный blanket impl `StackOps` для всех `StackStorage` — намеренно жёсткое API-решение: оно
фиксирует единое тело алгоритма и не позволяет downstream implementor подменить операции. Для
первой публикации это приемлемо и полезно против semantic drift, но после публикации станет
долгосрочным coherence commitment; текущая документация это решение объясняет.

### Ошибки, panic paths и ресурсы

- `TagExhausted` возвращается до side effects на terminal tag.
- Out-of-domain index отвергается до доступа к storage.
- Self-loop/returned-link guards присутствуют в release-active форме там, где нарушение может
  привести к double issue или вечному циклу.
- Production crate не владеет heap/OS resources и не имеет cleanup-sensitive `Drop`.
- В runner больше нет `process.exit`, обходящего cleanup; hard kill оставляет только уникальное
  gitignored дерево, которое будущий запуск не переиспользует и не удаляет.

Новых silent fallback, swallowed errors или partially-published state не найдено.

## Производительность

Production hot path остаётся allocation-free и состоит из atomic link access, одного head CAS
loop и bounded backoff. Новые три коммита production hot path не меняют.

Потенциальные направления ускорения существуют, но ни одно не следует принимать без измерения:

- `load_next` Acquire / `store_next` Release теоретически можно исследовать как Relaxed при
  сохранении publication через head, но только с формальным обновлением контракта, Loom
  counterfactual и native weak-memory wallclock evidence;
- successful pop CAS потенциально может не требовать Acquire, если импорт полностью обеспечен
  предыдущими loads, но экономия target-specific и риск доказательной ошибки выше ожидаемого
  выигрыша;
- cache-line padding head/links может помочь при конкретном размещении рядом с hot metadata, но
  увеличивает footprint и должен оставаться выбором storage owner после профилирования;
- переход strong → weak CAS по имеющимся codegen данным не даёт оснований ожидать ускорение.

Текущие ordering и backoff не выглядят явно переусложнёнными. Рекомендация — не менять hot path по
интуиции; сначала закрыть P3-2, чтобы measurement pipeline действительно выполнял все свои
оракулы.

## Tests, package и release surface

Статически suite охватывает:

- pack/unpack boundaries и properties;
- LIFO, empty/sentinel и terminal seal;
- custom/narrow storage implementations и compile-fail contracts;
- threaded conservation плюс retry/backoff activation under `test-internals`;
- Loom ABA/release-sequence models и намеренно ломающиеся counterfactuals;
- scratch containment, symlink/junction canaries и old fixed-root redirect;
- packaged default/feature tests и packaged bench build в CI;
- MSRV, unsupported atomic target, clippy/rustdoc и native arm64 evidence jobs.

Обычная сборка остаётся `no_std`, allocation-free и без normal dependencies. Loom optional и
активируется только комбинацией cfg+feature; dev dependencies не попадают в runtime graph.
Manifest содержит license, description, repository, documentation, categories и explicit fixture
exclude. Package gate извлекает `.crate` и проверяет не только library build, но и shipped tests и
bench target. Явных проблем first-publication packaging в просмотренном дереве не найдено.

## Итог

`tagged-index-stack` **готов к публикации по этому статическому проходу**. Исправления раунда 19
закрыли найденный release blocker и остальные конкретные замечания без новой production-регрессии.
Четыре P3 желательно закрыть как укрепление доказательной системы; ни одно из них не является
обнаруженным нарушением текущей семантики стека. Самая дешёвая code hardening правка — второй
`debug_assert` в `pack_truncating`; самая ценная инфраструктурная — реальный Windows запуск
scratch-guard и включение summary mode в arm64 job.
