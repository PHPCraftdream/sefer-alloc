# `sefer-region`: статический аудит перед релизом

Дата: 2026-08-11  
Снимок: `bce871e24c8beb121470c0defc835f5342e0a2ab`  
Режим: только чтение исходников, тестов, документации и Git-истории; тесты, сборка, Miri, Kani и бенчмарки не запускались.

## Краткий вывод

**Вердикт: NO-GO до устранения correctness-blocker'а и согласования публичного контракта.**

Новая волна действительно улучшила crate:

- `Handle<T>` теперь содержит идентичность экземпляра `Region`, поэтому обычный handle из другого region больше не читает и не удаляет чужое значение;
- добавлены capacity guards, более честное описание generation wrap, именованные итераторы, `ExactSizeIterator`/`FusedIterator`, `Debug`, `From<Region<T>>`, `into_inner` и несколько полезных регрессионных тестов;
- документация больше не изображает `Region` как плотное allocator-backed хранилище в основных пользовательских разделах;
- собственных `unsafe`, FFI, raw pointers и ручных `Send`/`Sync` в crate нет.

Но ключевой новый механизм domain identity ломается на exhaustion: `AtomicUsize::fetch_add` оборачивает глобальный счётчик, после одной паники возвращает его к `1` и снова выдаёт старый `region_id`. На 32-bit это не академическая граница.

После исправления блокеров crate выглядит небольшим и в основном аккуратным. Подтверждённых UB, use-after-free, double-free, data race или выхода за границы в собственном коде не найдено. Главные оставшиеся риски — логическая изоляция, ложные/расходящиеся контракты, хрупкие тестовые оракулы и standalone release artifact.

## Объём и ограничения аудита

Проверялись:

- `crates/region/src`, публичный API и auto-trait/lifetime поведение;
- integration tests, examples и benchmark harness;
- member README, корневые спецификации и release/CI wiring;
- текущая реализация и новая волна кодовых изменений;
- memory safety, RAII, concurrency, panic paths, dependency/supply-chain, async integration и performance shape.

Не проверялись исполнением:

- фактическая зелёность CI;
- поведение на 32-bit/no-atomic targets;
- standalone `cargo package`/`cargo bench`;
- численные benchmark claims.

Незакоммиченные файлы, существовавшие до аудита, не читались и не изменялись. Этот документ — единственное изменение рабочего дерева от данного аудита.

## P0 — блокеры релиза

### F1. `region_id` переиспользуется после exhaustion

**Severity: Critical/High; correctness, isolation и DoS.**

Гарантия в `crates/region/src/region.rs:57-71` говорит, что ID никогда не переиспользуется и после исчерпания namespace создание постоянно паникует. Реализация в обоих конструкторах (`region.rs:118-123`, `142-169`) использует:

```rust
NEXT_REGION_ID.fetch_add(1, Ordering::Relaxed)
```

с последующим `NonZeroUsize::new(...).expect(...)`.

Фактическая машина состояний у границы:

1. при `counter == usize::MAX` конструктор получает `MAX`, а атомик оборачивается в `0`;
2. следующий конструктор получает `0`, переводит атомик в `1` и паникует в `NonZeroUsize::new`;
3. если panic поймана или осталась внутри `join`, следующий конструктор успешно получает повторный ID `1`;
4. stale `Handle<T>` старого region с ID `1` и совпавшим `DefaultKey` снова принимается новым region и может прочитать либо удалить неправильный объект.

Это не Rust UB, но это нарушение изоляции и возможное cross-tenant object confusion, если разные `Region` используются как домены. Публичный README приводит около 13.9 млн `Region::new()` в секунду; даже если этот contention-бенч требует исправления, 32-bit exhaustion явно нельзя называть недостижимым.

**Что сделать:**

- вынести выдачу ID в единый helper, используемый `new` и `with_capacity`;
- применить CAS/`fetch_update`, где переход `MAX -> 0` выдаёт последний допустимый ID, а `0` является постоянным exhausted sentinel и никогда не меняется обратно;
- лучше предоставить `try_new`/`try_with_capacity` с typed exhaustion error, оставив нынешние конструкторы panic-wrapper'ами;
- добавить boundary-модель на локальном атомике: `MAX - 1`, `MAX`, `0`, несколько повторных вызовов после exhaustion и конкурентная гонка у границы;
- не мутировать настоящий глобальный static из тестов.

Минимальная требуемая семантика: после первой ошибки ни один будущий вызов в процессе не может получить ранее выданный ID.

## P1 — исправить до релиза

### F2. Два разных свойства одновременно называются invariant I6

`docs/INVARIANTS.md:32-37`, `docs/GLOSSARY.md:14`, `docs/PLAN.md:122-132` и `tests/freelist_reuse.rs` определяют I6 как reuse свободных slots/ограниченный рост. `crates/region/src/lib.rs:43-49`, `region.rs:47-71`, member README и `tests/region_invariants.rs` называют I6 instance isolation.

Это не косметика: тест утверждает, что проверяет canonical I1–I6, но проверяет другой I6. Root rustdoc также отсылает читателя к несовместимой спецификации.

**Действие:** сохранить исторический I6 для reuse и назвать instance isolation I7 либо атомарно перенумеровать все source docs, root docs и tests. До релиза один номер должен означать ровно одно свойство.

### F3. Canonical PLAN всё ещё описывает старый и местами несуществующий дизайн

`docs/PLAN.md:53-59,156-168,483-486` описывает handle как `DefaultKey + PhantomData`, без domain ID. В `PLAN.md:85-90,479-482` и одном комментарии `tests/region_invariants.rs:44-46` остаются обещания dense/compacting storage и «all operations O(1)».

Реальный обычный `SlotMap` сохраняет tombstone holes; lookup/remove ожидаемо O(1), insert amortized O(1), а iteration/clear зависят от slot-array high-water length. Следование «canonical» PLAN при следующем рефакторинге может удалить исправление isolation.

**Действие:** обновить PLAN под `region_id` и честную complexity model либо явно пометить его historical/non-authoritative. Исправить «capacity bounded by live high-water»: geometric capacity может быть выше максимального live count; корректная гарантия — повторный churn в уже выделенных slots не вызывает дальнейшего роста.

### F4. Публичные invariant/drop формулировки расходятся с Rust ownership

Canonical `docs/INVARIANTS.md` уже правильно говорит, что `remove` передаёт ownership вызывающему коду. Но `crates/region/src/lib.rs:41-42`, `region.rs:44-46`, member README и `tests/region_invariants.rs` всё ещё говорят, что removed value «dropped» и «never leaked».

`remove` не уничтожает `T`; caller может использовать или `mem::forget` возвращённое значение. Rust также позволяет забыть весь `Region`. Верная гарантия: crate не дублирует и внутренне не забывает ownership; normal drop уничтожает оставшиеся значения один раз; successful remove передаёт ownership ровно один раз.

Дополнительно `Region::clear` уже честно говорит, что точный набор survivors после panicking `Drop` не является стабильным контрактом, но type-level docs `SyncRegion` и README всё ещё обещают, что «later values remain live». Их нужно ослабить до container-valid/partial-clear без обещания конкретного survivor set. Для panicking value корректнее «removed and destructor invoked», а не «successfully dropped».

### F5. Public panic/no_std/async contracts неполны или ложны

1. `SyncRegion::with_capacity` документирует только `capacity == usize::MAX`, хотя delegate отвергает любое значение выше `2^32 - 3`, может упасть на allocation и на `region_id` exhaustion. `SyncRegion::new` также не упоминает exhaustion.
2. `handle.rs:20-23` приводит `riscv32imc` как поддерживаемый no_std target, но ISA без `A` extension не гарантирует pointer-width atomic RMW, тогда как `Region` безусловно требует `AtomicUsize::fetch_add`.
3. `SyncRegion` — blocking `std::sync::RwLock`, но rustdoc/README не предупреждают async-пользователя. Guard через `.await` и синхронный one-shot lock acquisition на runtime worker могут вызвать executor-wide starvation/deadlock; timeout не отменяет блокирующий lock wait.

**Действия:**

- зеркально документировать все panic conditions либо дать fallible constructors/reserve;
- ограничить targets через `target_has_atomic = "ptr"` с понятной диагностикой или реализовать fallback; убрать неверный target из примера и закрепить cross-build;
- добавить раздел `Async runtimes`: не держать guards через `.await`, one-shot methods тоже блокируют OS thread, batching допустим только внутри синхронного участка, для async ownership использовать downstream async lock; `spawn_blocking` не делает уже начатую операцию cancellation-safe.

### F6. Standalone benchmark в опубликованном tarball фактически не воспроизводим

`crates/region/benches/region_bench.rs:25-48` сам фиксирует ограничение `bench-scale-tool`: standalone fallback ищет/пишет manifest в `<crate>/../../bench-iters.txt`, вне package root. Поставляемый `crates/region/benches/bench-iters.txt`:

- фактически не читается harness'ом;
- использует другой синтаксис;
- содержит ID, не совпадающие с benchmark IDs.

CI package smoke сознательно не запускает extracted benchmark. Пользователь может получить JIT recalibration, запись за пределы распакованного crate или failure на read-only registry source.

**Действие:** обновить/исправить `bench-scale-tool` с явным package-local manifest path либо исключить неработающий benchmark из publication. Удалить ложный local manifest/comment. Добавить isolated gate: package -> extract в scratch без ancestor workspace -> benchmark smoke.

### F7. Несколько тестов зелёные при сломанном поведении

Release-significant cases:

- `tests/f14_api_ergonomics.rs:29-47`: poison recovery проверяет пустой region и только успешный возврат. Ветка, которая при poison возвращает новый пустой `Region`, останется зелёной. Перед panic нужно вставить value, сохранить handle и после `into_inner` проверить value/len.
- `f14_api_ergonomics.rs:86-109`: locked `Debug` использует sleep и проверяет только слово `SyncRegion`, не marker `<locked>`. Нужен канал/barrier после реального захвата guard и точный oracle.
- `tests/remove_guard_release.rs`: regression deadlock не имеет subprocess timeout, поэтому регрессия навсегда повесит test binary/CI вместо детерминированного red.
- тот же тест оставляет strong cycle `SyncRegion -> ReentrantDrop -> Arc<SyncRegion>` и не завершает RAII lifecycle fixture. Нужно удалить второе значение, drop его и проверить `Arc::try_unwrap`, либо хранить `Weak`.
- `tests/smoke.rs:239-254` требует разных hash output для неравных handles. Rust `Hash` разрешает collisions. Следует проверять `equal => same hash` и поведение `HashSet`/`HashMap`, не hash inequality.
- exact 32-bit layout assertions не входят в текущую CI: bare-metal job строит library, но не integration test. Нужен 32-bit compile-only host/test row.

Это не доказывает runtime bug в соответствующих happy paths, но текущая формулировка «всё покрыто» сильнее фактических оракулов.

### F8. `Handle` layout нужно сознательно определить до релиза

`Handle<T>` теперь публичный multi-field `#[repr(C)]` aggregate, а docs/tests пинят exact sizes и `Option<Handle<T>>` niche. При этом один field — upstream `slotmap::DefaultKey`, а manifest допускает весь `slotmap 1.x`.

`repr(C)` фиксирует порядок/aggregate layout, но не превращает `DefaultKey` в стабильный C ABI и не даёт общей гарантии nullable representation для произвольной много-полевой структуры. Тесты — полезный tripwire в репозитории, но downstream при сборке library их не компилирует.

**Рекомендация:** если FFI/stable ABI не является продуктовой целью, убрать `repr(C)` и представить exact size/niche как observed implementation property. Если это сознательный ABI promise — описать поддерживаемый ABI, владеть стабильным encoding и не полагаться на private layout upstream type. Решение лучше принять до фиксации публичного контракта.

### F9. `captrack` слишком тяжёл и side-effectful для опубликованного dev graph

`captrack` с telemetry нужен одному ignored probe, но добавляет proc-macro/constructor/background/autodump surface. Его side effects подавляются workspace-root `.cargo/config.toml`, который не попадает в standalone member artifact.

**Действие:** вынести probe в непубликуемый workspace tool/xtask или заменить прямым чтением `Region::capacity()`. Не полагаться на ancestor `.cargo/config` как на свойство опубликованного crate. Если dependency временно остаётся — exact-pin и отдельно проверить package вне workspace.

## P2 — улучшения кода и проекта

### F10. Reentrancy и poison policy требуют более локальной документации

- `SyncRegion::clear` выполняет произвольный `T::Drop` под write lock; reentrant access к тому же region может deadlock.
- `get_cloned` выполняет `T::clone` под read lock; write reentry deadlocks, а новый read может зависнуть за queued writer из-за unspecified fairness.
- паника внутри `Clone` не меняет container structure и не poison'ит read lock, но `Clone` с interior mutability способен частично изменить сам payload. Формулировка «no effect on stored value» была бы ложной; текущая документация должна сохранять различие container state и payload effects.
- после writer panic код всегда восстанавливает guard через `into_inner`, но не очищает poison flag. Все последующие операции постоянно идут по более медленной error/recovery ветке. Если policy действительно всегда доверяет container, можно после первого recovery рассмотреть `clear_poison`; если downstream нужны application invariants — полезнее дать наблюдаемый/fallible poison API.

Type-level warning недостаточен для пользователя, пришедшего прямо к `clear` или `get_cloned`: стоит добавить method-local `# Reentrancy` links.

### F11. Fallible capacity API улучшит эксплуатационную устойчивость

Crate сам не парсит сеть/файлы и не имеет прямой remote boundary. Но downstream легко передаст внешний `usize` в `with_capacity`/`reserve`, после чего получит domain panic или OOM abort.

Добавить `try_with_capacity`/`try_reserve` с конкретной ошибкой полезно до фиксации API. Нынешние infallible методы могут остаться ergonomic wrappers. В документации отдельно потребовать application clamp для untrusted counts; allocator OOM может оставаться abort-prone в зависимости от платформы.

### F12. CI не полностью доказывает заявленный MSRV/member package

MSRV job строит root package и transitively member runtime path, но не весь собственный tests/benches/dev graph `sefer-region`. Member-specific gates используют moving stable. Добавить на Rust 1.88 как минимум:

- `cargo check -p sefer-region --all-features`;
- `cargo check -p sefer-region --no-default-features`;
- `cargo test -p sefer-region --no-run --all-features`;
- при обещании MSRV для tooling — сборку examples/bench targets.

Также member наследует не всю lint policy: стоит явно deny `unexpected_cfgs` и зарегистрировать `cfg(docsrs)` либо осознанно наследовать совместимую workspace lint table.

### F13. Небольшие smells и cleanup

- комментарий над `Handle` equality/clone всё ещё говорит, что identity — только slotmap key; теперь это `(region_id, key)`;
- README говорит «8 -> 16 bytes» без 64-bit qualifier; на 32-bit тесты ожидают 12 байт;
- runtime layout test дублирует const assertions и добавляет мало сигнала;
- обычные Debug tests проверяют field names, но почти не проверяют значения;
- rationale «consuming IntoIterator невозможно, потому что нельзя раскрывать raw key» неверен: wrapper может выдавать только `T`. Либо добавить `IntoValues`/`IntoIterator<Item = T>`, либо исправить комментарий;
- публичные std `RwLockReadGuard`/`RwLockWriteGuard` — сознательный lock-in. Если когда-либо нужен другой lock backend, сейчас дешёвое окно для crate-owned guard wrappers; иначе явно принять std lock как стабильную часть API;
- cargo-deny проверяет advisories/licenses/sources текущего lockfile, но не является code audit. Основной README сейчас это честно признаёт; удалить оставшиеся слова «audited» из старого PLAN/GLOSSARY или приложить конкретный audit record с обозначенной областью проверки.

## Memory-safety и security inventory

По статическому просмотру в собственном коде `sefer-region`:

- `unsafe` blocks/functions/traits/manual impls: **0**;
- raw pointers, `NonNull`, pointer arithmetic, `transmute`, `MaybeUninit`: **0**;
- FFI/`extern`, C ABI entry points, callbacks: **0**;
- ручные `Send`/`Sync`: **0**;
- custom production `Drop`, `ManuallyDrop`, `mem::forget`: **0**;
- async tasks/futures/channels: **0**;
- network, parser, crypto, SQL, shell, archive и user-path surfaces: **0**.

`#![forbid(unsafe_code)]` и package lint запрещают собственный unsafe. `Handle<T>` — числовой token, не pointer owner; выбранный `PhantomData<fn() -> T>` не выдаёт ссылок и не владеет `T`, поэтому unconditional auto-`Send + Sync` handle выглядит sound. `SyncRegion<T>` получает корректные bounds через `RwLock<Region<T>>`.

Upstream `slotmap` содержит unsafe, но является отдельной trust boundary. Подтверждённых upstream UB не найдено. Нельзя называть advisory scan код-аудитом; scope фактической проверки должен фиксироваться отдельно.

**Итог на memory safety:** подтверждённых UB, UAF, double free, OOB или data race нет. F1 — серьёзная логическая ошибка identity/isolation, но она не нарушает правила памяти Rust сама по себе.

## Что ещё можно существенно ускорить

Чудесного scalar micro-tuning в текущем wrapper-коде не видно: shipping source короткий, не содержит лишних heap allocations, строковых преобразований, collect/copy loops или случайного O(n²). Generic wrappers должны хорошо мономорфизироваться. `#[inline(always)]` здесь с высокой вероятностью добавит code size, а не радикальный выигрыш.

Реальные крупные рычаги следующие.

### P-perf-1. Dense alternative для holey iteration

Committed benchmark table показывает примерно 1.53 µs для 1000 live values без holes и около 11.29 µs при 90% holes — порядка **7.4x** разницы при одинаковом live count. Это структурное свойство обычного `SlotMap`: iteration идёт по slot-array high-water length.

Если целевой workload много удаляет и часто полностью итерирует, отдельный `DenseRegion`/`DenseSlotMap`-backed тип даст намного больше любого локального micro-tuning. Цена — иная relocation/order/capacity модель и дополнительный API. Не следует молча менять storage существующего `Region` без benchmark/semantic gate.

### P-perf-2. Batch/guard API для `SyncRegion`

README показывает приблизительно 1221 ns для repeated one-shot reads против 38.7 ns при одном удерживаемом read guard — около **31x** в данном synthetic workload. Механизм уже доступен через `read()`; проблема adoption/эргономики.

Полезное направление — closure/batch API, которое удерживает guard один раз и не позволяет случайно пронести его через `.await`. Это одновременно снижает lock acquisitions и делает рекомендуемый usage pattern заметнее. Сначала нужен реальный consumer benchmark; публичный `read()` удалять нельзя.

### P-perf-3. Исправить и затем измерить contention выдачи `region_id`

Каждый `Region::new` делает RMW в одном process-global cache line. Это может плохо масштабироваться при массовом создании region из многих threads, но нынешний judge не даёт чистого доказательства:

- нет barrier-aligned start;
- threads запускаются последовательно и получают разные фактические окна;
- `Instant::elapsed()` вызывается на каждой итерации;
- нет baseline без shared atomic;
- формулировка «8 threads, 1 second each/evenly balanced» сильнее harness.

Сначала исправить F1, затем сделать fixed-work или batched-clock A/B с Barrier и рядами 1/2/4/8 threads. Самое дешёвое продуктовое решение — переиспользовать `Region`, а не создавать миллионы экземпляров. Thread-local range allocation может снизить RMW, но расходует конечный 32-bit namespace и усложняет exhaustion; применять только после измерения и формального ID budget.

### P-perf-4. Drop вне write lock для `SyncRegion::clear`

Сейчас `clear` держит exclusive lock во время произвольных и потенциально медленных destructors. Двухфазный design — структурно удалить/move values под lock, затем drop ownership вне lock — может сильно уменьшить tail latency и устранить reentrant-Drop deadlock. Но поведение generations, panic survivors и extra allocation нужно спроектировать как отдельную семантическую работу; это не безопасная локальная перестановка строк.

### P-perf-5. Шардинг — только отдельный concurrent type

Один `RwLock` неизбежно сериализует writes и конфликтующие operations. Если production profile действительно contention-heavy, `ShardedRegion` с несколькими независимыми maps/locks способен дать кратный throughput. Это новый тип с иными ordering/iteration/handle semantics, а не оптимизация текущего `SyncRegion`. Для редких writes текущий дизайн проще и предсказуемее.

## Оценка новой волны

Новая волна **содержательно улучшила код**, особенно устранив immediate cross-instance aliasing прежней модели. Она также улучшила API и честность значительной части документации. Однако считать задачу закрытой пока нельзя:

- commit, заменивший `AtomicU64` на `AtomicUsize`, исправил width/no_std aspect, но **не исправил** ранее известный wrap/recycle state machine;
- новый I6 test проверяет обычное различие instances, но не exhaustion и одновременно конфликтует с canonical numbering;
- новый crate-local `bench-iters.txt` создаёт видимость standalone reproducibility, но harness его не использует;
- часть новых тестов проверяет наличие строки/успех функции, а не именно обещанное состояние.

Иными словами: архитектурное направление правильное, но release evidence пока содержит несколько false-green.

## Рекомендуемый порядок работ

### Этап A — correctness blocker

1. Исправить необратимое exhaustion-state `region_id`.
2. Добавить deterministic sequential и concurrent boundary tests через локальный helper.
3. Решить target policy для отсутствия pointer-width atomic RMW.

### Этап B — единый публичный контракт

1. Развести I6/I7 во всех docs/source/tests.
2. Обновить или архивировать canonical PLAN.
3. Синхронизировать ownership/I5, partial-clear, capacity panic, handle-size и async/reentrancy wording.
4. Принять явное решение по `repr(C)`/exact layout.

### Этап C — release artifact

1. Исправить или исключить standalone benchmark.
2. Убрать `captrack` из publishable member dev graph либо полностью изолировать его.

### Этап D — evidence

1. Усилить poison/debug/deadlock/hash test oracles.
2. Добавить 32-bit integration compile coverage и правильный atomic target gate.
3. Добавить member-specific MSRV/no-default/all-target release jobs.
4. После исправлений выполнить отдельно от этого readonly-аудита: fmt, clippy all-targets/features, test matrix, docs `-D warnings`, Miri по meaningful member targets, package-list/extracted artifact smoke и точный release CI SHA.

### Этап E — performance после correctness freeze

1. Перестроить judge `region_new`.
2. Измерить dense alternative на holey workloads.
3. Проверить ergonomic batched-lock API на реальном consumer profile.
4. Проектировать drop-outside-lock/sharding только при подтверждённом production bottleneck.

## Release checklist

До релиза должны быть закрыты как минимум:

- [ ] ID никогда не переиспользуется после exhaustion;
- [ ] repeated/concurrent exhaustion tests действительно краснеют на старой реализации;
- [ ] выбран и проверен no-atomic target policy;
- [ ] I6/I7 и canonical PLAN согласованы;
- [ ] I5/clear/panic/layout/async docs больше не обещают лишнего;
- [ ] standalone package не содержит заведомо сломанный benchmark path;
- [ ] false-green tests усилены;
- [ ] 32-bit/member MSRV/package gates добавлены и зелёны;
- [ ] release artifact проверен вне workspace;
- [ ] только после этого создан и отправлен release tag.

## Итоговая рекомендация

Не выпускать текущий `bce871e`.

Сам код невелик, memory-safe на уровне собственного Rust и уже заметно лучше прежней реализации. Для release-ready состояния не требуется ещё одна широкая перепись: нужен точечный ремонт ID allocator, одноразовая консолидация контракта и усиление release evidence. Крупные performance-инвестиции следует направлять не в scalar wrapper, а в dense storage для holey iteration, batching lock operations и, только при подтверждённой нагрузке, отдельный sharded concurrent type.
