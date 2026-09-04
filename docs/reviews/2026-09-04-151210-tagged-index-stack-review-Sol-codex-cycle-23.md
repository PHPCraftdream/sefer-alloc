# `tagged-index-stack`: prerelease review, Sol-codex, cycle 23

**Время отчёта:** 2026-09-04 15:12:10 +02:00

**Ревизия:** `1a15a79ef5fab220555a89c1589c5937a26bb323`

**Режим:** новое полное статическое исследование актуального дерева. HS-ревьювер был запущен без предыстории, но не смог начать работу из-за внешнего лимита сервиса; поэтому fallback-review выполнен Sol-codex лично, без использования выводов других агентов. Во время исследовательской фазы тесты, Loom, Miri, Clippy, rustdoc, benchmark и сборки крейта не запускались. Выполнены только read-only инспекция исходников/git/metadata и изолированная проверка dependency graph обычного downstream-потребителя.

## Вердикт

**NO-GO до закрытия четырёх P3.** P0, P1 и P2 не найдены: повторная проверка packed-state, tag exhaustion/seal, H-2, CAS loops и release sequence не выявила нарушения soundness, ABA-защиты или lock-free progress. Однако публикуемый README сейчас двусмысленно ослабляет нормативный unsafe-контракт, тест полного backoff-depth зависит от удачи планировщика, четыре `SAFETY`-обоснования называют неверный link domain, а production-форма `Backoff::spin` сохраняет test-only результат.

Сводка: **P0: 0, P1: 0, P2: 0, P3: 4.** Ниже также перечислены принятые ограничения и performance-кандидаты, которые не являются основанием для изменения hot path без измерений.

## Что просмотрено

- `src/lib.rs` и `src/imp.rs` целиком: публичная поверхность, bit packing, sentinel conversion, tag budget/seal, push/pop CAS loops, head/link orderings, unsafe contracts, corruption guards, backoff и test-only probes.
- `Cargo.toml`, `README.md`, `CHANGELOG.md`, package include/exclude, feature/API boundaries и dependency shape обычного downstream-потребителя.
- Все направления тестов: unit/property/boundary, custom и narrow unchecked storage, compile-fail, real-thread conservation, tiny-tag/tag-seal и Loom/counterfactual models.
- Benchmark, example, Node A/B runner и относящиеся к крейту CI/release jobs.
- Коммиты после прошлого отчёта, особенно `4e0372f`, `cdbf8ca`, `f31c789` и `1a15a79`, а также актуальные correctness/performance tracker entries.
- Интеграция root `Registry` и его Loom shim: head/links binding, slot domain, checked materialisation и `AtomicU32` link cells.

## P3 — исправить до публикации

### P3-1. README разрешает запрещённое временное перепривязывание живого head

`README.md:79-85` говорит, что head должен быть достижим через ровно одно live implementor value **«at a time»**. Нормативный контракт `StackStorage` строже: `src/imp.rs:644-649` требует один binding на всю жизнь head и явно запрещает temporal rebinding даже при единственном live value в каждый момент. `src/imp.rs:797-802` отдельно подчёркивает, что «one at a time is not enough», а `tests/custom_storage_impl.rs:493-567` демонстрирует порчу именно этой формой.

README — первый опубликованный unsafe guide для implementor-а. Его текущая формулировка позволяет прочитать последовательное перемещение `StackHead` из старого backing в новый как допустимое, хотя trait safety contract объявляет это soundness violation.

**Исправление:** заменить «at a time» на недвусмысленное «for the head's whole life» и рядом явно запретить rebinding к другому link storage даже без двух одновременно живых implementor values.

### P3-2. README одновременно отрицает и описывает runtime detector double-push

`README.md:153-157` утверждает «no detector exists», а следующим предложением сообщает, что повторный push текущего head обнаруживается release-active self-loop guard-ом при первом pop. Точный detection boundary уже хорошо описан в `README.md:110-118` и trait docs: detector частичный — self-loop и out-of-range link ловятся, более глубокий in-range cycle может пройти молча.

Это не cosmetic wording: читатель unsafe API должен понимать, какие нарушения гарантированно диагностируются, а какие остаются silent corruption. Абсолютное «detector отсутствует» противоречит реальному поведению.

**Исправление:** написать, что полного liveness/double-push detector нет; сохранить точное уточнение про current-head self-loop и deeper cycles.

### P3-3. Backoff-depth test зависит от недетерминированной глубины проигрышей CAS

`tests/threaded_conservation.rs:93-101,217-235` требует, чтобы за максимум три real-OS-thread раунда **оба** cap counters выросли. Один cap event означает, что один вызов проиграл не менее семи CAS подряд. Barrier синхронизирует старт, но не гарантирует ни параллельное исполнение, ни конкретную серию CAS-поражений; сам комментарий признаёт, что ОС может сериализовать workers. На одноядерном, перегруженном или иначе планируемом runner корректная реализация может никогда не достичь cap, и bounded loop завершится ложным failure.

Большой объём работы повышает вероятность, но не превращает scheduler event в детерминированный oracle. Conservation и факт входа в retry arms полезны; обязательная глубина семь должна проверяться независимо от планировщика.

**Исправление:** оставить real-thread conservation и non-zero retry activation, а progression/saturation самого `Backoff` проверить детерминированным test-only probe/test. Не ослаблять проверку алгоритма до «тест просто завершился»: новый oracle должен падать при cap/reset regression и не зависеть от того, сколько потоков реально исполнялось одновременно.

### P3-4. Четыре `SAFETY`-комментария доказывают вымышленный link domain

В `tests/custom_storage_impl.rs` call-site comments неверно смешивают `INDEX_BITS` с фактическим доменом backing:

- `:156`: `VecStorage::new(4)` имеет domain `0..4`, не `0..16`;
- `:481`: две `ArrayLinks<64>` дают domain `0..64`, не `0..16`;
- `:533`: `Pool { links: ArrayLinks<64> }` имеет domain `0..64`, не `0..16`;
- `:621`: `DualWidth { links: ArrayLinks<64> }` имеет domain `0..64`, не `0..16`.

Используемые индексы фактически находятся в backing, поэтому нынешние вызовы не создают UB. Но у `unsafe` call site комментарий обязан доказывать реальное предусловие. `B = 16` задаёт encoding width (`INDEX_MASK`), а не ёмкость storage; именно это различие публичный контракт подробно подчёркивает в `src/imp.rs:1028-1038`. Ложное доказательство в reference/negative fixtures опасно копировать в production implementor.

**Исправление:** заменить четыре domain-границы на фактические (`0..4` или `0..64`) без изменения тестовой семантики.

## Остаток предыдущего P3 о backoff instrumentation

`4e0372f` правильно cfg-gated все `note_*` вызовы и helpers. Но `Backoff::spin` всё ещё имеет production-сигнатуру `fn spin(&mut self) -> bool` (`src/imp.rs:74-101`), а оба default retry arms создают и отбрасывают `_at_cap` (`:1558`, `:1641`). Возвращаемое значение существует только ради gated cap-oracle. В оптимизированной сборке оно, вероятно, исчезает; в low-opt форме это снова зависит от inlining/DCE.

Этот остаток включён в P3-3 как часть одного исправления oracle/backoff: production `spin` должен возвращать `()`, а test/loom build может до вызова прочитать cfg-gated `will_spin_at_cap()` (или использовать эквивалентную форму), после чего вызвать общий `spin()`. Так default MIR/API shape не несёт test-only результата, а test build сохраняет точную pre-increment semantics.

## Повторная проверка production-кода

### Packed state, seal и ABA

- Допустимы только `INDEX_BITS = 1..=16`; compile-time guard закрывает shift/mask overflow.
- Checked public pack не принимает index/tag вне представимого диапазона; private truncating path используется только после доказанных guards.
- Push увеличивает tag и до любых side effects отказывает при `TAG_MAX`; seal постоянен, pops остаются разрешены.
- Последний pop переносит текущий tag в empty state, поэтому drain/refill не восстанавливает старое packed head value.
- Empty-index sentinel и link-tail sentinel преобразуются явно и не смешиваются с допустимым индексом.

Повторяемого head word в lifetime объекта при соблюдении contracts не найдено.

### Atomics и progress

- Push публикует link перед Release CAS. Его Relaxed initial/failure head loads достаточны: push не читает payload/link, защищённый observed head.
- Pop начинает с Acquire head load; failure ordering Acquire импортирует новый observed head до следующего `load_next`.
- Head изменяется только RMW-операциями, поэтому empty transitions не разрывают release sequence.
- Link `Acquire`/`Release` сильнее теоретического минимума и остаётся осознанной defense-in-depth формой.
- Backoff ограничен и локален одному вызову; stack остаётся lock-free, но корректно не обещает starvation freedom.

### Unsafe/API boundary

- `StackStorage` — честный `unsafe trait`; все три hooks — `unsafe fn` с caller-side contracts. `push_index`/owned `push` также unsafe и требуют domain, liveness и единственной publish/recycle authority.
- Crate-owned blanket `impl StackOps<B> for S where S: StackStorage<B> + ?Sized` не позволяет downstream заменить CAS loop. Это одновременно публичное coherence commitment; для задуманного открытого storage-extension point оно принято осознанно.
- `ArrayIndexStack` не выдаёт отдельный head↔links binding и не реализует `StackStorage`, поэтому прежняя safe shared-head форма не выражается.
- Raw-pointer ownership, FFI, ручные `Send`/`Sync`, fabricated lifetimes и resource-owning `Drop` отсутствуют.

### Errors, corruption boundary и package

- `TagExhausted` возвращается до записи link; invalid numeric index отклоняется release-active panic до storage access.
- Pop release-active ловит out-of-range link и self-loop; более глубокие in-range cycles не обещано обнаруживать.
- `is_empty` и `pushes_remaining` честно документированы как racing advisory snapshots.
- Default runtime dependency graph пуст; optional Loom не попадает в lockfile обычного downstream-потребителя. Repository-only fixtures/scripts исключены из package согласованно с packaged gates.
- `no_std`, MSRV, unsupported-target guard, compile-fail, package isolation и release/loom matrices присутствуют в CI. Их выполнение относится к фазе проверки после правок.

## Возможности ускорения — решения не принимать без gate

Три известных hot-path кандидата остаются только measurement backlog:

1. не повторять `store_next` после lost push CAS, если observed head index не изменился;
2. ослабить link-cell Acquire/Release до Relaxed при сохранении head publication proof;
3. ослабить successful pop CAS до Relaxed при failure Acquire.

Для первого нужна contention-distribution evidence, для второго и третьего — native arm64 wall-clock/A-B gate. На x86 ожидается нулевой или почти нулевой codegen delta. Статический обзор не даёт оснований менять эти orderings или retry writes «по интуиции».

## Условие GO следующего цикла

После исправления P3-1..P3-4 требуется лично проверить диффы и невакуозность новых oracle, затем пройти default/release/test-internals, Loom, MSRV, `no_std`, unsupported-target, Clippy, rustdoc, benchmark build, package/dry-run и root integration gates. Только новый обзор уже исправленного дерева без P0-P3 даёт финальный GO.
