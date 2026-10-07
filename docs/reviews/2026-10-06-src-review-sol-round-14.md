# Ревью `src/` — раунд 14

## 1. Вердикт и проверенная база

**Пять подтверждённых пунктов: четыре P3 и один P4. Новых подтверждённых production P0/P1/P2 не найдено.** Это ограниченный по глубине обще-каталожный обзор, не доказательство отсутствия уязвимостей и не release GO. Известный **P1-box, correctness 164, остаётся принятым, но не исправленным**: интрузивная запись в освобождённый Small-блок пересекает protector живого by-value Box. Нативные результаты ниже не делают аллокатор Miri-clean.

- Дата начала: 2026-10-06. База: `d6417c6c85f9f8c124110eac77080523df06b004`, `fix: close src review round 13 findings`.
- Предыдущий обзор: `docs/reviews/2026-10-06-src-review-sol-round-13.md`. Номер 14 относится к последовательности обзоров src, не к историческому perf-round R34.
- На старте основная ветка была чистой. Работа изолирована в `worktrees/r14-src-review`; исходники при составлении отчёта не менялись. Две временные executable-пробы удалены после исполнения.
- Инвентарь базы: **168 Rust-файлов / 43 261 физическая строка в src**. Прежний обзор имел 43 035 строк на другой базе `e90a3575`; это не одинаковые снимки.
- Проверена дельта `021399a3..d6417c6c`, отдельно текущие пути GlobalAlloc, lease/fallback, terminal publication, route directory, Small/Large, epoch/RCU/sharding и тестовые контракты.
- Rush применил тематические срезы rust-intel: unsafe/FFI, concurrency, drop/RAII, API/lifetimes, testing, semantics, data/performance, deps/security, async applicability, предыдущая дельта и индексы. Были перезапуски неуспешных workers. Количество попыток не выдаётся за количество независимых доказательств.
- **Zero-trust:** черновики содержали ложные выводы, включая «удалён весь no_stale_doc_references», «в src нет ручных Eq/Hash» и «prune вызывается на thread-exit». Они отвергнуты по исходникам. Последний автоматический reviewer вернул FAIL на остаточные ошибки черновика; этот итоговый текст синтезирован интегратором из проверенных фактов, а не принят по агентскому вердикту.
- Все тексты src прошли каталоговый скрининг опасных конструкций и инвентаризацию unsafe. Углублённая ручная трассировка выборочная: **построчное доказательство всех 43 261 строк не заявляется**. Companion crates не получили самостоятельного полного аудита; их отдельные контракты читались на границе вызовов.
- LSP references: rust-analyzer завершился `server exited unexpectedly (code 0)`; ошибка зарегистрирована в report_issue. Использована текстовая трассировка, её полнота ниже symbol-aware references.

Среда: rustc `1.97.0 (2d8144b78 2026-07-07)`, cargo `1.97.0 (c980f4866 2026-06-30)`, LLVM `22.1.6`, host `x86_64-pc-windows-msvc`. Cargo.lock: sefer-alloc 0.3.0; aligned-vmem 0.2.0; numa-shim 0.2.0; once-ptr-cell 0.1.0; size-classes 0.1.0; sefer-region 0.2.0; tagged-index-stack 0.1.0; arc-swap 1.9.1; crossbeam-epoch 0.9.20; core_affinity 0.8.3; loom 0.7.2; proptest 1.11.0; serde 1.0.228; serde_json 1.0.150. tagged-index-stack — verification-only cfg(loom/kani), не production runtime. Версии/feature composition не менялись.

## 2. Находки

| ID | Уровень | Доказательство | Суть |
|---|---|---|---|
| R14-01 | P3 | исходник + исполненный reachability witness | Общий auto-Sync SmallSidecar выпускает owner-мутацию наружу; два prepare могут потерять System-leaf и коды классов |
| R14-02 | P3 | структурное доказательство; UNMEASURED по latency/Ir/RSS | Новый полный prune на каждом cold bind даёт квадратичную сумму проверок живых claims |
| R14-03 | P3 | документы ↔ текущие исходники/история | Удалённый tripwire назван действующим; исторический ring-spec всё ещё объявлен текущим authoritative protocol |
| R14-04 | P3 | воспроизведено native | Поздний TLS-деструктор выигрывает токен без release guard и получает AccessError; следующий writer пропускает shard 0 |
| R14-05 | P4 | текущие карточки/перечни | Индексы имеют неверные census/tier/closure-placement и оборванную evidence-ссылку |

### R14-01 — owner capability обходится через общий SmallSidecar

База, места: `src/registry/segment_route/registration.rs:23–29`, `small_sidecar.rs:44–58`, `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:74–115,224–239`.

RouteRegistration запрещает Sync через `PhantomData<Cell<()>>`. Но `small_sidecar()` отдаёт `&SmallSidecar`; сам SmallSidecar auto-Sync, а его `prepare` и `issue` — public safe `&self`. Два scoped-потока получают owner-mutating API без owner capability.

ClassLeaves::prepare проверяет готовность, выделяет/инициализирует свежий System-leaf, затем безусловно делает `mixed[leaf].store(fresh, Release)`. Допустимый интерливинг: A прошёл проверку отсутствия mixed; B выделил и опубликовал свой leaf; A опубликовал другой leaf. Указатель B потерян; Drop освобождает только последний указатель. Если B уже записал новые коды классов, поздняя публикация A также стирает их из текущего leaf. Все байты атомарны: это **логическая ошибка/утечка локальных метаданных, не утверждение о data race или production UB**.

Реальный allocator владеет своими registration в закрытом owner-only RouteSlots; извне их получить нельзя. Публичная достижимость рассматриваемого пути требует `internals` и caller-owned RouteDirectory, то есть нестабильную test/internal поверхность. Она всё равно должна сохранять свой owner-only контракт.

Интегратор исполнил safe-пробу: реальный выделенный буфер, выровненный подspan, локальная registration, общий sidecar, два scoped prepare. Вывод:

```text
R14 owner API probe: safe shared writers admitted; no leak schedule asserted
```

Это доказывает доступность API и Sync, **не детерминированное исполнение утечки**. Потеря указателя установлена указанным source schedule, а не RSS/stress-измерением.

Решение для следующей фазы: закрыть `SmallSidecar::prepare/issue` до crate-private и перенести публичные owner-операции на уже !Sync RouteRegistration; мигрировать всех callers. Не вводить новый lock/CAS ради поддержки ненужных многовладельческих мутаций и не создавать `&mut SmallSidecar` поверх живых producer-atomic ссылок. Оракулы: actual-API negative compilation, запрет shared owner, настоящие uniform→mixed/class-publication/scan/lifetime сценарии. При таком fix утечный schedule запрещается типовой границей; специальный racing allocation hook для него не требуется.

### R14-02 — prune платит за все живые claims на каждом холодном входе

Места: `src/concurrent/sharded/sharded_region.rs:253–255,486,669`.

`Vec::retain` делает `Weak::strong_count` для каждого TLS-claim. Полный проход вызывается на каждом implicit cache miss и каждом **explicit bind**, включая неэксклюзивное modulo sharing. **На thread-exit prune не вызывается:** ErasedGuard::drop лишь освобождает живые won tokens.

Для последовательного первого binding N одновременно живых одношардовых регионов: длины проходов 0,1,…,N−1; сумма N(N−1)/2. При FIFO-cache 8 и round-robin working set >8 каждое повторное обращение снова cold и снова проходит все claims. Это новая структурная стоимость R13-02; исправление прежнего удержания token arrays остаётся корректным.

Слой — deprecated `experimental`, не production allocator. **Wall-clock/Ir/RSS не измерены; конкретный speedup, Pareto-выигрыш и performance GO не заявляются.** Исправить ненужный повторный проход можно без hardware promotion: ограничить sweep случаями, когда действительно могла умереть token backing, сохранить все живые release obligations и прежний bounded dead-claim residual. Детерминированный work-bound oracle должен различать «все регионы живы» и «наступила смерть»; сравнение измеренной latency остаётся отдельным, не проведённым здесь gate.

### R14-03 — текущие safety/protocol-свидетельства ссылаются на снятый механизм

`tests/dbg_hook_safety_tripwire.rs` был удалён коммитом `a4245965` при terminal cutover. Это старый source-text/allowlist-сканер, **его восстановление не рекомендуется**. Однако CLAUDE.md, пункт 4 benchmark-hook rules, продолжает говорить `live test file`, `the test passes`, `current, comprehensive resolution mechanism`. На него также ссылаются текущий критерий TRACKED_hook_safety в `docs/CORRECTNESS_OPEN_ITEMS.md`, пример CARGO_MANIFEST_DIR в ACTIVE.md и комментарии исходников.

Подтверждённое нарушение — достоверность evidence/current-state prose, не найденный новый unsafe-hook и не доказательство отсутствия всех содержательных safety-тестов. Реальные raw-pointer mutators с `pub unsafe fn`/Safety-контрактами, ownership/type boundaries и поведенческие tests не заменяются наличием grep-сканера.

Отдельный пример того же дрейфа: `docs/CROSS_THREAD_STATE_MACHINES.md:1–7` называет себя **authoritative specification**, но его actors/ABANDONED/ring-channel/slot-incarnation объяснения принадлежат старому протоколу. Текущая Large foreign-publication машина в System-дескрипторе и её связь с физическим owner-only reservation word не отделены. Простое добавление девяти переходов «Large descriptor» было бы тоже неверным: production descriptor обычно проходит LIVE→PENDING→CONSUMING→unlink; физический owner word отдельно использует cache/reuse/release/rollback и generation saturation.

Решение: сохранить исторические аргументы как историю, явно обозначить supersession и записать текущую границу descriptor pin ≠ reservation credit, terminal publication, lease authority/ownerless maintenance. Не менять runtime ради соответствия снятой модели; не обещать Miri-clean при принятом P1-box. Общий prose-долг 154 этим точечным обновлением не закрывается.

### R14-04 — поздний TLS-drop оставляет токен без освобождающего guard

Места: `sharded_region.rs:440–493,648–677`; `ERASED_GUARD.with` после CAS и после cache update.

Детерминированная native-проба: TLS-объект LATE с Arc<ShardedRegion> инициализируется **раньше router TLS**. Writer вставляет и удаляет значение в shard 0. На exit guard сначала освобождает токен; затем после уничтожения router TLS LATE::drop вызывает insert под catch_unwind. Новый CAS опять занимает shard 0, но `.with` уже уничтоженного ERASED_GUARD паникует до регистрации claim. Токен остаётся true. После join новый writer получает shard 1.

```text
thread '<unnamed>' ... panicked ... std/src/thread/local.rs:428:25:
cannot access a Thread Local Storage value during or after destruction: AccessError
R14 late TLS probe: caught_panic=true; next_shard=1
```

Паника была намеренно поймана пробой; exit executable успешен. Это **не подтверждение double-unwind abort**: такой abort — отдельный риск, если panic выпустят из Drop во время unwind. Occupied — advisory: allocation capacity не пропадает, fallback sharing остаётся mutex-сериализованным. Теряются эксклюзивный scheduling token и ожидаемое отсутствие неожиданной teardown panic.

Решение: удостовериться в доступности release bookkeeping **до** приобретения exclusive token; при недоступном TLS обычный insert может modulo-share без orphan claim. Explicit bind не должен подтверждать привязку, которую teardown TLS сохранить не может. Оракул после fix: late insert успешен/handle работает, panic нет, после join следующий exclusive claim получает shard 0. Нужны также normal FIFO/live-claims/exit-release сценарии и explicit-bind teardown.

### R14-05 — индексы не полностью отражают текущую структуру

- Thin correctness index перечисляет пять ACTIVE-карточек, пропуская 162/163; актуальные numbered headings ACTIVE.md — 1,2,62,11,13,162,163. При добавлении/закрытии нового пункта census тоже должен обновляться.
- TRACKED_correctness_residuals.md имеет устаревшее числовое описание; в TRACKED_misc.md неоднозначно, считает ли header только open или также closed pointers.
- Perf 79/80 — CLOSED с полноценными карточками внутри [A] и уже существующими Recently resolved pointers. Полную историю следует держать в archive, не в active tier.
- Perf 40 обозначен [A], но расположен внутри [D]; 42 использует не входящий в tier key [P]. Status/trigger не следует менять при исправлении размещения/метки.
- Evidence для perf 26 оборвана на `lines 231–238, the`; исправление — по реальному cited source.

Это P4 документальная консистентность, не новые runtime-дефекты/measurement verdicts. Исправлять только названную структуру, не проводить заодно publish/version/hardware решения и не переоткрывать закрытые эксперименты.

## 3. Ревью выполненной работы раунда 13

| Исправление | Результат на текущей базе |
|---|---|
| R13-01 | Increment перед zero-word skip; feature-off, absent bitmap, empty scan и early hit различаются. Регрессия прошла повторно |
| R13-02 | Weak на TokenBlock с out-of-line Box: слабые claims не удерживают token array после final strong Drop; временные exit upgrades оговорены; live claims сохраняются. Новый cost R14-02 отдельно |
| R13-03 | Указанные terminal/credit/pin SAFETY-описания и ссылки исправлены; оба настоящих strict rustdoc gate прошли. Это не тотальная миграция всей исторической prose |
| R13-04 | Удалены только read_ptr/write_ptr; mutable pair и read_struct_with_atomic_word сохранены |
| R13-05 | Guard Send требует T:Send; Guard Sync требует **T:Send+Sync** (lock field + marker); lock Send+Sync при T:Send. Marker делает T invariant. Actual-source negative compile и позитивные owner/moved/shared сценарии прошли |

`no_stale_doc_references.rs` не удалён: удалён один incidental link-text test. Весь файл по-прежнему существует; восстановление source-wording oracle не требуется. Закрытые correctness 168/169 остаются закрытыми по исходным контрактам; новый teardown edge и cost не скрываются их старой closure.

## 4. Покрытие, риск и отвергнутые кандидаты

| Категория | Рассмотрено | Предел |
|---|---|---|
| Unsafe/FFI | node/OS/sidecar, entry/route storage, manual Send/Sync, allocation/reallocation forwarding, typed ownership | Не новый Miri/TSan/FFI-platform сертификат |
| Concurrency | lock bounds, publication ordering, counted pins/unlink, heap lease, fallback init, sharded claims, epoch/RCU | Модель weak-memory каждого пути не исполнена |
| Lifetimes/RAII | guard drop, TLS teardown, descriptor vs physical storage, error-path ownership release, cache/reuse | Длинный tail cfg-путей просматривался выборочно |
| API/semver | stable configs/errors, internals reachability, owner mutation leakage, raw caller contracts | Companion crates не получили независимый semver audit |
| Testing | три r13 regression targets, actual compiler negative oracle, before-exit retention handshake; удалённый wording test | Нет полного нового cargo test/npm check в audit-only фазе |
| Semantic conformance | GlobalAlloc forwarding, copy span, stale handles, phase/generation, current vs historical spec | Native tests не исключают UB |
| Cost | full claim prune, repeated locked lookup, existing refill/directory pressure | Новые latency/Ir/RSS A/B отсутствуют |
| Deps/security | lockfile versions, cfg gating, executable panic/unsafe/process patterns | Advisory scanners/remote databases не проверены в этом раунде |
| Async | в исполняемом src не найден async/await/Tokio spawn; maintenance — синхронный process-lifetime worker | Dev-only tokio/example не runtime async surface |

Отклонено:

- Отсутствие non_exhaustive у пяти doc-hidden internals enums: нестабильная sanctioned test surface, не новый stable API break.
- Приватный hash_insert_identity может обойти всю таблицу только при нарушенном occupancy-инварианте; MAX_SEGMENTS 4096 против HASH_CAPACITY 8192 исключает full valid table. Добавлять wrap fallback для маскировки коррупции не нужно. Ноль прямых grep-ссылок в tests не означает ноль поведения.
- PageClass — opt-in page-map-diag, не wire/persisted/untrusted codec. Непроверенный invalid-byte hardening — возможность отдельного edge audit, не подтверждённая новая уязвимость.
- ExactShard array_layout overflow: не показана достижимость до OOM в корректном процессе; source-only гипотеза не превращена в bug fix.
- Инвариант-нарушающий dbg hook и debug_assert в finish_bind: не native production repro.
- Weak::strong_count pruning: zero не воскресает, live release claim не должен удаляться; новой потери release в самом prune не найдено. R14-04 происходит **до регистрации claim**, не при prune.
- Eq/Hash: в handle-ах есть ручные согласованные реализации; утверждение «везде только derive» было ложным.

Оптимизационная возможность без решения: второй `find` в RouteDirectory::lookup (`directory.rs:871–887`) под тем же exclusive guard, после pin refcount, при immutable key/incarnation. Повторный поиск структурно избыточен; сохраняемый identity self-check можно сделать дешевле. **Никакой скорости не измерено**. Это рекомендация, не новый correctness finding; root hot-path A/B нужен до speedup/promotion заявления.

Mutable workflow tags — смежный уже известный correctness 140, не новая src-находка и не элемент текущего исправления. CPU/NUMA hardware gates, wall-clock 81/82 и deployment/publish решения не закрыты этим обзором.

## 5. Исполнено интегратором в этом раунде

В изолированном audit worktree, низкий Windows priority, cargo -j2. Сбой sccache transport 10054 устранён для этих дочерних команд через `RUSTC_WRAPPER=""`, без остановки чужого cache server или изменения repo config. Feature names передавались comma-separated из-за quoting wrapper. Первый запуск без experimental дал ноль retention tests — этот ноль не засчитан за покрытие.

```text
cargo test --locked -j 2 --features production,experimental,internals,alloc-stats,bench-internals \
  --test r13_directory_words_examined --test r13_shard_guard_auto_traits \
  --test r13_sharded_dead_region_retention -- --test-threads=1
```

Результат: 1 counter + 4 guard + 3 retention = **8 passed, 0 failed**.

```text
cargo run --locked -j 2 --example global_allocator --features production
```

Результат наблюдённого executable, с установленным SeferAlloc и start_maintenance:

```text
sefer-alloc global allocator OK — summed 100000 ints (=4999950000) and stored 10000 map entries, all through SeferAlloc.
```

Strict documentation:

```text
cargo rustdoc --locked -j 2 --lib --features production -- -D warnings
cargo rustdoc --locked -j 2 --lib --all-features -- -D warnings --document-private-items
```

Оба — PASS. Также исполнены две описанные выше native-пробы R14-01 и R14-04. Их исходники не оставлены как scaffolding. Full npm check и remote CI/Kani на d6417c6c были зелёными до этого запроса; это исторический receipt, **не новый запуск**. В этом audit нет новых Miri/Loom/Kani/TSan/release/performance/advisory gate. Agent statements о тестах не засчитывались вместо собственных command outputs.

## 6. Устойчивый учёт прежних пунктов и очередь

Оба top-level индекса и все ACTIVE/TRACKED theme files прочитаны. Ни один прежний пункт не закрывается лишь от этого source review. **Для каждого ID в приложении B решение — LEAVE текущего status/trigger/closure-record**, кроме документальной коррекции свидетельств/размещения R14-03/R14-05, которая не означает закрытие его runtime/model/hardware остатка. Это включает длинный publish-readiness tail 108…144, пропущенный worker-черновиком; ссылка на удаляемый ignored scratch не является долговечным учётом.

P1-box 164 остаётся owner-accepted not-fixed. 166/167 сохраняют собственные статусы и триггеры (не приписываются автоматически accepted как 164). 154 остаётся общим prose-долгом; perf81 INCONCLUSIVE, 82 НЕ СЕЙЧАС, 72/78 — прежние owned hypotheses. Закрытые 168/169 не переоткрываются без нарушения их собственных критериев.

После отдельного коммита этого отчёта пользователь уже авторизовал wrush-исправления:

1. R14-01 — owner capability и полная миграция callers; negative compile + настоящие leaf/publication сценарии.
2. R14-02 — не повторять all-live sweep без причины; сохранить before-exit retention/release bounds и проверить work shape без speedup-claims.
3. R14-03 — актуальные документы/evidence; scanner не восстанавливать, runtime под старую модель не менять.
4. R14-04 — не приобретать orphan token на недоступном TLS; deterministic before/after lifecycle regression.
5. R14-05 — исправить census/tier/closure/evidence placement без изменения прежних verdicts.

Принятие исправлений требует личного diff-review, собственного исполнения тестов и smoke, переноса в основную ветку, коммита и удаления созданных worktree. **Этот исходный раздел фиксирует findings на базе d6417c6c; будущая closure записывается отдельно, не стирая найденное.**

## Приложение A. Инвентарь unsafe и auto-trait границ базы

Механически извлечены 21 module-level и 83 item-level allow-границ (атрибуты, не совпадения в комментариях). Это не счётчик unsafe-блоков и не доказательство каждого места.

```text
src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs:383 #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref_mut`
src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs:166 #[allow(unsafe_code)] // R6-CQ-2: `unsafe fn` boundary (raw metadata write).
src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs:281 #[allow(unsafe_code)] // R6-CQ-2: `unsafe fn` boundary (raw metadata write).
src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs:308 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs:336 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/alloc_core/bootstrap.rs:207 #[allow(unsafe_code)]
src/alloc_core/alloc_core/lifecycle.rs:432 #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension_mut`
src/alloc_core/alloc_core/mem/mem_impl.rs:223 #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
src/alloc_core/alloc_core/mem/mem_impl.rs:254 #[allow(unsafe_code)] // R6-MS-1/2 sibling: shares `dealloc`'s caller-pointer contract.
src/alloc_core/alloc_core/mem/mem_impl.rs:555 #[allow(unsafe_code)] // R6-MS-1/2 sibling: `unsafe fn` boundary (caller-pointer contract).
src/alloc_core/alloc_core/mem/mem_impl.rs:631 #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
src/alloc_core/alloc_core/sidecar_test_hooks.rs:31 #[allow(unsafe_code)] // Test producer has the same unique-transfer contract as RoutePin.
src/alloc_core/large/alloc_core_large_cache.rs:86 #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension`
src/alloc_core/large/alloc_core_large_cache.rs:134 #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension_mut`
src/alloc_core/large/alloc_core_large_cache.rs:278 #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension`
src/alloc_core/large/alloc_core_large_cache.rs:332 #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension_mut`
src/alloc_core/large/alloc_core_large_cache.rs:376 #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension`
src/alloc_core/large/large_cache_extended.rs:117 #![allow(unsafe_code)]
src/alloc_core/platform/node.rs:37 #![allow(unsafe_code)]
src/alloc_core/platform/os.rs:28 #![allow(unsafe_code)]
src/alloc_core/platform/sidecar.rs:152 #![allow(unsafe_code)]
src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:4 #![allow(unsafe_code)]
src/alloc_core/segment/segment_directory/segment_directory_impl.rs:286 #[allow(unsafe_code)]
src/alloc_core/segment/segment_directory/segment_directory_impl.rs:314 #[allow(unsafe_code)]
src/alloc_core/segment/segment_header/segment_header_gen_table.rs:54 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/segment/segment_header/segment_header_gen_table.rs:97 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/segment/segment_header/segment_header_gen_table.rs:150 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/segment/segment_table/route_slots.rs:2 #![allow(unsafe_code)]
src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:459 #[allow(unsafe_code)]
src/alloc_core/small/alloc_core_small/directory.rs:52 #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref_mut`
src/alloc_core/small/alloc_core_small/directory.rs:112 #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref`
src/alloc_core/small/alloc_core_small/directory.rs:134 #[allow(unsafe_code)] // R14-9 (task #294): calls the `unsafe fn sidecar::deref_mut`
src/alloc_core/small/alloc_core_small/find_segment.rs:175 #[allow(unsafe_code)] // R17-2 (task #319): calls the `unsafe fn`s
src/alloc_core/small/alloc_core_small/reserve.rs:450 #[allow(unsafe_code)]
src/alloc_core/small/alloc_core_small_diag.rs:147 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/small/alloc_core_small_diag.rs:180 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/small/alloc_core_small_diag.rs:214 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/small/alloc_core_small_diag.rs:238 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/small/alloc_core_small_diag.rs:377 #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
src/alloc_core/small/alloc_core_small_magazine.rs:451 #[allow(unsafe_code)] // R6-MS-3: `unsafe fn` boundary (caller-pointer contract).
src/alloc_core/small/alloc_core_small_pool/decommit.rs:96 #[allow(unsafe_code)] // R29-8: `unsafe fn` boundary (live_count==0 precondition).
src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:171 #[allow(unsafe_code)] // task #504: unsafe fn boundary, mirrors dbg_decomp_recommit_payload.
src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:192 #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:273 #[allow(unsafe_code)] // R31-15: unsafe fn boundary, mirrors dbg_decomp_decommit_payload.
src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:350 #[allow(unsafe_code)] // R29-3: unsafe fn boundary (raw-pointer precondition).
src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs:385 #[allow(unsafe_code)] // R31-6: unsafe fn boundary, mirrors dbg_decomp_decommit_payload.
src/concurrent/epoch/hand.rs:51 #![allow(unsafe_code)]
src/global/exact_object/exact_shard.rs:4 #![allow(unsafe_code)]
src/global/exact_object/narrow.rs:4 #![allow(unsafe_code)]
src/global/fallback.rs:51 #![allow(unsafe_code)]
src/global/sefer_alloc/batch.rs:5 #![allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:16 #[allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:261 #[allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:324 #[allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:370 #[allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:412 #[allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:452 #[allow(unsafe_code)]
src/global/sefer_alloc/diag.rs:495 #[allow(unsafe_code)]
src/global/sefer_alloc/global_alloc.rs:6 #![allow(unsafe_code)]
src/global/tls_heap.rs:101 #![allow(unsafe_code)]
src/registry/bootstrap/ensure.rs:1 #![allow(unsafe_code)]
src/registry/bootstrap/loom_shim.rs:14 #![allow(unsafe_code)]
src/registry/bootstrap/registry.rs:1 #![allow(unsafe_code)]
src/registry/heap_core/alloc/batch.rs:139 #[allow(unsafe_code)]
src/registry/heap_core/alloc/batch.rs:182 #[allow(unsafe_code)]
src/registry/heap_core/alloc/hot.rs:76 #[allow(unsafe_code)]
src/registry/heap_core/alloc/hot.rs:152 #[allow(unsafe_code)]
src/registry/heap_core/alloc/hot.rs:380 #[allow(unsafe_code)]
src/registry/heap_core/alloc/hot.rs:507 #[allow(unsafe_code)]
src/registry/heap_core/diag/diag_probes.rs:248 #[allow(unsafe_code)] // R23-3: `unsafe fn` boundary, mirrors `dbg_push_to_ring`/`HeapCore::dealloc`.
src/registry/heap_core/diag/diag_probes.rs:320 #[allow(unsafe_code)] // R28-1: `unsafe fn` boundary, mirrors `dbg_dealloc_own_thread_with_base` above.
src/registry/heap_core/diag/diag_probes.rs:376 #[allow(unsafe_code)] // R29-10: `unsafe fn` boundary, mirrors `dbg_flush_class_only` above.
src/registry/heap_core/diag/diag_probes.rs:472 #[allow(unsafe_code)] // R31-15: unsafe fn boundary, forwarded contract.
src/registry/heap_core/diag/diag_probes.rs:493 #[allow(unsafe_code)] // R29-3: unsafe fn boundary, forwarded contract.
src/registry/heap_core/diag/diag_probes.rs:514 #[allow(unsafe_code)] // R31-6: unsafe fn boundary, forwarded contract.
src/registry/heap_core/diag/diag_probes.rs:636 #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
src/registry/heap_core/diag/diag_probes.rs:657 #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
src/registry/heap_core/free/dealloc.rs:218 #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
src/registry/heap_core/free/dealloc.rs:254 #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `AllocCore::dealloc_with_base`.
src/registry/heap_core/free/dealloc_batch.rs:186 #[allow(unsafe_code)] // R6-MS-3: `unsafe fn` boundary (caller-pointer contract).
src/registry/heap_core/free/dealloc_batch.rs:195 #[allow(unsafe_code)] // R6-MS-3: unsafe call into the batched fast path.
src/registry/heap_core/free/dealloc_batch.rs:210 #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into scalar `dealloc`.
src/registry/heap_core/free/dealloc_batch.rs:238 #[allow(unsafe_code)] // R6-MS-3: `unsafe fn` boundary (caller-pointer contract).
src/registry/heap_core/free/dealloc_batch.rs:277 #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into scalar `dealloc`.
src/registry/heap_core/free/dealloc_batch.rs:326 #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `AllocCore::dealloc`.
src/registry/heap_core/free/dealloc_batch.rs:371 #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
src/registry/heap_core/free/dealloc_batch.rs:386 #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
src/registry/heap_core/free/dealloc_own_base.rs:396 #[allow(unsafe_code)]
src/registry/heap_core/free/dealloc_own_base.rs:458 #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
src/registry/heap_core/free/dealloc_own_base.rs:497 #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
src/registry/heap_core/free/dealloc_own_base.rs:549 #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `AllocCore::dealloc`.
src/registry/heap_core/free/realloc.rs:136 #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
src/registry/heap_core/state/tcache_flush.rs:99 #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
src/registry/heap_core_xthread/routing.rs:9 #[allow(unsafe_code)]
src/registry/heap_core_xthread/routing.rs:24 #[allow(unsafe_code)]
src/registry/heap_core_xthread/sidecar_drain.rs:55 #[allow(unsafe_code)] // Forward the same unique-transfer producer obligation.
src/registry/heap_core_xthread/sidecar_drain.rs:91 #[allow(unsafe_code)] // Synthetic record probes retain the real owner/geometry contract.
src/registry/heap_registry/claim.rs:13 #![allow(unsafe_code)]
src/registry/heap_slot.rs:79 #![allow(unsafe_code)]
src/registry/segment_route/directory.rs:3 #![allow(unsafe_code)]
src/registry/segment_route/pin.rs:45 #[allow(unsafe_code)]
src/registry/segment_route/pin.rs:57 #[allow(unsafe_code)]
src/registry/segment_route/shard_lock.rs:6 #![allow(unsafe_code)]
src/registry/segment_route/small_sidecar.rs:2 #![allow(unsafe_code)]
```

Manual implementations:

```text
src/concurrent/epoch/hand.rs:572 unsafe impl<T: Send + Sync> Send for AtomicSlot<T> {}
src/concurrent/epoch/hand.rs:582 unsafe impl<T: Send + Sync> Sync for AtomicSlot<T> {}
src/global/exact_object/exact_shard.rs:163 unsafe impl Sync for ExactShard {}
src/global/sefer_alloc/global_alloc.rs:33 unsafe impl GlobalAlloc for SeferAlloc {
src/registry/bootstrap/loom_shim.rs:56 unsafe impl<T> Send for OncePtrCell<T> {}
src/registry/bootstrap/loom_shim.rs:58 unsafe impl<T> Sync for OncePtrCell<T> {}
src/registry/heap_slot.rs:283 unsafe impl Sync for HeapSlot {}
src/registry/segment_route/shard_lock.rs:22 unsafe impl<T: Send> Sync for ShardLock<T> {}
src/registry/segment_route/shard_lock.rs:24 unsafe impl<T: Send> Send for ShardLock<T> {}
```

- AtomicSlot: T:Send+Sync, указатель защищён epoch guard; generation CAS даёт единственного reclaim winner.
- ShardLock: T:Send для lock transfer/shared locking; guard marker добавляет T:Sync для shared guard и invariance T.
- HeapSlot: только Sync, не Send; lease CAS и закрытые control words дают единственного физического heap mutator.
- ExactShard: System storage под spinlock, никакого пользовательского T/destructor callback; prototype-only.
- OncePtrCell shim: cfg(loom), передаёт только raw pointer, не dereference произвольного T. Это не доказательство соответствия runtime, residual167 остаётся.
- GlobalAlloc implementation: unsafe caller ptr/layout contract сохраняется на forwarding/copy/free; accepted P1-box отдельно.

Raw ownership/view решений: EntryHandle owns один counted System descriptor и освобождает на последнем Drop; RoutePin не экспортирует reservation root; RouteRegistration lifetime привязан к directory и !Sync owner marker (обход через SmallSidecar — R14-01). RouteSlots/sidecar owners освобождают точный System/VM layout; AllocCore с raw owner tables остаётся !Send/!Sync. CurrentHeap raw borrowed handle используется в закрытой TLS single-writer зоне. Физические Node atomic views имеют caller-held mapping lifetime, не whole-process lifetime. Ни одно из этих текстовых доказательств не заменяет Miri/weak-memory/platform-run.

Скрининг executable строк src не нашёл transmute/from_raw_parts/Box::from_raw/extern C/Pin::new_unchecked, async/await/Tokio spawn/unbounded channel; FFI и unsafe реализации зависимостей этим отрицательным результатом не сертифицируются.

## Приложение B. Полный список disposition-ID на старте

Список получен из numbered card headings текущих файлов; 59a/59b включены отдельно. Процессные пункты 1–3 top-level index не приняты за карточки. LEAVE значит сохранить фактический текущий статус: не объявить уже CLOSED pointer снова open. R14-03/R14-05 меняют evidence/структуру, не runtime-verdict старой карточки.

| Файл | Все ID его карточек | Решение для каждого ID |
|---|---|---|
| `docs/perf/OPEN_ITEMS.md` | 1, 13, 79, 80, 81, 2, 3, 4, 5, 6, 14, 15, 25, 26, 28, 51, 29, 30, 31, 33, 38, 40, 41, 42, 48, 7, 8, 9, 10, 11, 12, 50, 52, 53, 54, 49, 44, 16, 17, 18, 19, 20, 21, 22, 23, 24, 34, 35, 27, 36, 37, 45, 39, 43, 47, 55, 56, 57, 58, 59, 60, 61, 62, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 78, 82 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/ACTIVE.md` | 1, 2, 62, 11, 13, 162, 163 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_hook_safety.md` | 5, 7, 8, 9 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_verification_coverage.md` | 17, 18, 41, 61, 84, 167 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_platform_contracts.md` | 6, 26, 43, 44, 47, 48, 52, 53, 58, 59, 59a, 59b, 60, 152 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_ci_gate_coverage.md` | 19, 25, 50, 51, 54, 55, 64, 65, 70, 72, 73, 74, 76, 80, 82, 87, 88, 92, 95, 107, 140, 151, 156 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_test_flakiness.md` | 12, 14, 63, 69, 96, 143, 145, 146, 147, 150, 153 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_correctness_residuals.md` | 16, 22, 23, 66, 155, 164, 165, 166 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_publish_readiness.md` | 24, 27, 28, 29, 46, 85, 90, 91, 93, 97, 94, 98, 99, 100, 101, 102, 103, 104, 105, 106, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 141, 142, 144 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_process_record.md` | 10, 20, 21, 67, 68, 78, 79, 81, 83, 86, 89 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |
| `docs/correctness-open-items/TRACKED_misc.md` | 45, 49, 154, 157, 158, 159, 160, 161 | LEAVE: для каждого ID сохранить текущие status/trigger и закрытые исторические указатели; обзор src не является hardware/publish/model/perf acceptance. |

## Приложение C. Проверка учёта перед коммитом отчёта

После заведения item170 интегратор исполнил `cargo test --locked -j 2
--features production,internals --test no_stale_doc_references --
--test-threads=1`: **32 passed, 0 failed**. Новый report и карточка не
заменяют тестовые/типовые safety-границы; source-wording oracle не восстанавливался.

## Принятые исправления после ревью

R14-01…05 закрыты в remediation-pass; item170 перенесён из `[A]` в
`docs/correctness-open-items/RESOLVED.md`, полная closure-запись — в
`docs/correctness-open-items/ARCHIVE.md` §170.

- **R14-01:** `SmallSidecar::{prepare, issue}` стали `pub(crate)`;
  owner-операции доступны через `RouteRegistration::{prepare_small,
  issue_small}`. Маркер `PhantomData<Cell<()>>` сохраняет `!Sync`.
  Production callsite сохранил abort на отсутствующем sidecar; все
  тестовые callsites мигрированы. Три actual-crate negative fixtures
  проверяют E0624 для shared mutators и E0277 для `RouteRegistration: Sync`,
  positive probe отбирает feature-compatible rlib перед проверкой ошибок.
- **R14-02:** dead-claim sweep теперь запускается на cold point только после
  изменения wrapping `TOKEN_BACKING_DEATHS`; pre-sweep snapshot сохраняет
  cleanup при racing death. Live release obligations не вытесняются.
  `PRUNE_CLAIM_CHECKS` измеряет фактические per-claim strong-count checks.
  Детерминированные тесты доказывают ноль таких проверок для живых
  FIFO-evicted/revisited claims и ограниченную очистку после deaths.
  **Это work-bound доказательство, не latency/Ir/RSS-замер и не speedup.**
  Сохраняется documented край: полный `2^64`-wrap может отложить pruning
  до следующего отличимого death hint.
- **R14-03:** старый Rust hook tripwire отмечен историческим; действующий
  successor — `scripts/verify-dbg-hook-safety.mjs`, wired into
  `scripts/check-all.mjs` and CI. Текущий прогон: PASS, 141 reviewed safe,
  30 reviewed unsafe, 78 bench-gated safe hooks. Секции 0–8
  `docs/CROSS_THREAD_STATE_MACHINES.md` явно historical; §9 трассирует
  текущий путь и разделяет physical Large reservation word и независимый
  `LargeState` route descriptor. README/ARCHITECTURE unsafe inventory
  сверены: 27 tier-1 (21 `src/`, 6 `crates/`), 103 tier-2 в 34 файлах;
  production активирует 15 internal seams.
- **R14-04:** routing и exit-guard TLS проверяются через `try_with` до
  exclusive-token CAS. Late insert без доступного router TLS modulo-shares;
  explicit late bind возвращает `false`. Claim и release-запись выполняются
  под живым guard borrow. Четыре детерминированных TLS-order tests покрывают
  оба teardown-order варианта, отказ explicit bind и обычный live bind.
- **R14-05:** correctness cards пересчитаны; item170 закрыт, item171 отдельно
  индексирует Miri residual. Perf items 79/80 оставлены в текущем
  Recently-resolved trail с полными архивными closure; 40/41 перемещены в
  `[A]`, 42 размечен `[D]`, item26 evidence восстановлена. Предыдущие
  perf-verdicts/triggers сохранены.

### Финальная верификация remediation

На свежем изолированном target прошёл полный native набор:
`cargo test --locked -j 2 --all-features --tests -- --test-threads=1`.
Feature-точный regression set прошёл **62 tests**:
`no_stale_doc_references` 32; R13 directory/shard controls 8;
R14 late-TLS/work-bound/owner-capability 9; R6/R11 route tests 13.
Дополнительно прошли:

- `node scripts/fmt-check.mjs` — 578 файлов, 2 chunks;
- `cargo clippy --locked -j 2 --all-features --all-targets -- -D warnings`;
- warning-strict rustdoc `production` и `all-features` + private items;
- `node scripts/verify-dbg-hook-safety.mjs`;
- `cargo run --locked -j 2 --example global_allocator --features production`:
  `sefer-alloc global allocator OK — summed 100000 ints (=4999950000) and
  stored 10000 map entries, all through SeferAlloc`.

Один промежуточный прогон `correctness_index_recently_resolved_pointers_carry_verdicts`
поймал неверный формат/порядок нового item170 closure pointer. Pointer приведён
к архивному заголовку и каноническому суффиксу; targeted guard и весь
`no_stale_doc_references` (32 tests) после исправления прошли.

**Miri residual — item171, не закрытый этим remediation:** `cargo +nightly-2026-10-06
miri test --locked -j 2 --features experimental --test epoch
single_threaded_sequence_matches_reference_model -- --exact` падает в
pre-existing `tests/epoch.rs` при регистрации default collector:
`crossbeam_epoch::internal::Local::element_of` (`crossbeam-epoch 0.9.20`,
`internal.rs:562`), вызванный `Global::try_advance` при `epoch::pin()`;
Miri выдаёт Stacked Borrows “tag does not exist”. Тот же класс ошибки
возник и при Miri-прогоне нового late-TLS test. Текущий toolchain:
`nightly-2026-10-06`, rustc `1.101.0-nightly (ea137335b 2026-10-05)`;
Crossbeam также выдаёт integer-to-pointer provenance warning в
`atomic.rs:204`. Попытка Tree Borrows на июльском Miri тоже не прошла:
сам Crossbeam предупреждает, что его integer-to-pointer conversion там
не поддерживается. `EpochRegion`/Cargo.lock не менялись; это не названо
ни false-positive, ни Miri-clean. Item171 оставлен `[T]` до root-cause/
supported-configuration решения; dependency change требует отдельной
явной авторизации.
