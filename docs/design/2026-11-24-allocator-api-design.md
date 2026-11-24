# Дизайн-предложение: реализация `std::alloc::Allocator` для SeferAlloc

> **Статус:** проектная рекомендация и исходная proposal, фаза 1; фаза 2 реализована в отдельном opt-in helper crate `crates/core/sefer-alloc-api`. Proposal ниже сохраняет исходные решения и provenance; актуальные детали реализации и её проверок ведутся в §7.4. Все локальные пути ниже относительные к корню репозитория; абсолютные/машинные пути не используются.
>
> **Дата:** 2026-11-24. **Область:** стабилизированный API `Allocator` Rust 1.100.0 и его возможная интеграция с существующим аллокатором SeferAlloc. Это не gate-отчёт и не утверждение о производительности.

## 0. Резюме и рекомендация

Рекомендуется продолжить во второй фазе с отдельным opt-in helper crate, который зависит от SeferAlloc и реализует `Allocator` для собственного тонкого адаптера вокруг `SeferAlloc`. Не следует реализовывать `Allocator` напрямую на `HeapCore`/`AllocCore`: `AllocCore` — локальный эксклюзивный субстрат, `!Send`/`!Sync`, а не разделяемый или клонируемый дескриптор. Не следует считать `sefer-region` подходящим аллокатором для std-коллекций: его slotmap-хранилище независимо, а arena/clear-поведение должно удовлетворять длительности жизни всех выделений и правилам инвалидирования стандартного `Allocator`.

Рекомендация helper crate условна: перед её принятием во второй фазе нужно подтвердить, что wrapper действительно может дать корректный `Allocator`-контракт без нового production-состояния и без небезопасного расширения lifetime. Helper crate с `rust-version = 1.100` позволит не менять MSRV основного crate и companion crates; новые зависимости и обновление их версий не нужны. Если размещение API в root crate окажется необходимым, отдельная осознанная альтернатива — поднять MSRV root до 1.100, понимая, что это меняет базовую совместимость root-крейта и не повышает автоматически MSRV companion crates.

`Allocator` стабилизирован в Rust 1.100.0, выпущенном 2026-11-12. Перечень связанных API в release listing шире среза стабилизации: в частности, он не означает стабилизацию `allocator_api`-расширений из списка unstable ниже. В этой фазе никаких manifest, source, test, CI, индекса или иных документов менять нельзя и не менялось.

## 1. Стабильный API Rust 1.100 и его контракт

### 1.1. Точные обязательные методы

Стабилизированный trait имеет два required methods:

```text
fn allocate(&self, layout: Layout)
    -> Result<NonNull<[u8]>, AllocError>;

unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
```

`allocate` — безопасный trait method: он может сообщить об отказе через `AllocError`. Документация trait рекомендует возвращать ошибку при нехватке ресурсов, но реализация может внутренне abort-ить; это не даёт вызывающему гарантии recoverable OOM. Ни один метод `Allocator` не должен unwind-ить через границу метода. `deallocate` — unsafe, его вызывающий обязан передавать живой блок, выделенный эквивалентным allocator, с подходящим layout, ровно один раз.

У trait есть предоставленные методы `allocate_zeroed`, `grow`, `grow_zeroed` и `shrink`. `grow`/`grow_zeroed`/`shrink` — unsafe операции, поскольку исходный блок, layout и владение указателем задаются вызывающим кодом. Для каждого успешного grow/shrink прежний указатель считается инвалидированным даже тогда, когда реализация физически расширила/уменьшила блок на месте. При `Err` старый блок и его исходные байты остаются живыми и доступны по исходному указателю; новый результат не возникает. Переход успешен на новой паре pointer/layout, старую пару затем нельзя использовать. При grow копируется требуемый старый префикс; `grow_zeroed` дополнительно гарантирует нули для нового участка. `shrink` сохраняет доступные первые байты нового размера.

### 1.2. Размер результата, `fits` и alignment

Успешный `allocate` возвращает `NonNull<[u8]>`, а не только указатель. Его slice length — доступный размер, который может быть больше запрошенного. Совместимость результата с layout определяется правилом `Allocator::fits`: alignment должен совпадать точно, а размер должен быть в диапазоне от исходного запрошенного размера до фактической длины возвращённого slice включительно. Нельзя подменять фактическую длину блока размером запроса или предполагать, что capacity равна запрошенной величине.

Для `grow`/`shrink` новый layout может иметь другой alignment. Это отличается от `GlobalAlloc::realloc`, где alignment сохраняется из старого layout. Реализация adapter должна явно обрабатывать смену alignment; нельзя передавать такие случаи в `realloc` как будто это эквивалентные операции. Если существующий underlying путь не может выполнить операцию, корректный план — allocate нового layout, скопировать ровно требуемый префикс, затем освободить старый блок только после полного успеха. При ошибке allocation исходный блок не трогать.

### 1.3. Обязательства allocator, эквивалентность и lifetime

Контракт trait требует, чтобы каждый успешный блок оставался выделенным и пригодным до корректного освобождения; любые исключения/инвалидирование должны соответствовать правилам API. Память, переданная allocator-у при освобождении, не должна одновременно считаться другим живым выделением. Выделения должны быть непересекающимися, адресная геометрия/размер не должны переполнять допустимое адресное пространство, и блок должен удовлетворять требуемому layout с нужным alignment. Нельзя читать/писать память живого блока через allocator-метаданные или иным образом изменять payload, пока владение остаётся у клиента.

Allocator-ы эквивалентны, если выделения одного можно освобождать через другой без нарушения правил; реализация trait может быть параметризована/реализована для ссылок на allocator. Стабильный trait сам по себе не требует `Clone`. Если конкретный API обещает семантику `Clone`, её контракт обязан гарантировать эквивалентность клона исходному allocator, а не просто скопировать handle, потеряв backing heap или route. `AllocatorClone` остаётся unstable.

Ссылка на allocator эквивалентна самому `A` только пока соответствующий borrow жив. Официальный trait docs прямо перечисляет истечение любого borrow-checker lifetime в типе allocator-а как условие инвалидирования его текущих allocations. Поэтому реализация и владелец обязаны обеспечить, чтобы lifetime ссылки/контейнера не истёк, пока выделенные блоки остаются live; ссылка сама по себе не сохраняет allocations после окончания borrow и не даёт бессрочного владения. Для `&A` и `&mut A` эквивалентность underlying allocator действует в рамках живого lifetime, а не сверх него. Любой borrowing wrapper обязан доказывать, что все allocations, созданные через него, остаются пригодными ровно столько, сколько требует API вызывающих коллекций; освобождение/перемещение владельца, уничтожение heap или сброс arena до освобождения всех его выделений недопустимы.

Для нулевого размера `Allocator` допускает корректный zero-size allocation. Требуется корректный ненулевой dangling pointer с alignment, удовлетворяющим layout; фактическое поведение должно соответствовать документированному slice/fits контракту и освобождение должно быть no-op/корректно принято. Не переносить сюда контракт `GlobalAlloc`: передача layout размера 0 в его методы — UB, тогда как `Allocator` поддерживает zero-size layout. Для ZST в `Box`/`Vec` нужны тесты и отдельное обоснование pointer provenance/alignment.

### 1.4. Точная стабилизированная поверхность Rust 1.100

Ниже приведён полный allocator-related срез стабилизации 1.100 по [release listing](https://doc.rust-lang.org/1.100.0/releases.html#version-11000-2026-11-12) и соответствующим rustdoc 1.100. Сигнатуры сокращены только опущением полных путей типов и bounds; `A: Allocator` подразумевается там, где API требует allocator.

- Типы: [`Allocator`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html), [`Global`](https://doc.rust-lang.org/1.100.0/std/alloc/struct.Global.html), [`System`](https://doc.rust-lang.org/1.100.0/std/alloc/struct.System.html).
- Стабилизированные impls `Allocator`: [`Global`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-Global), [`System`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-System), [`&A`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-%26A) и [`&mut A`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-%26mut+A) при `A: Allocator`, [`Box<T, A>`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-Box%3CT,+A>), [`Rc<T, A>`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-Rc%3CT,+A>) и [`Arc<T, A>`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html#impl-Allocator-for-Arc%3CT,+A>); для последних трёх `T: Allocator + ?Sized, A: Allocator`.
- Стабильные allocator-aware `Box` API (каждый помечен rustdoc «Stable since 1.100.0»): [`new_in`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.new_in): `fn new_in(x: T, alloc: A) -> Box<T, A>`; [`from_raw_in`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.from_raw_in): `unsafe fn from_raw_in(raw: *mut T, alloc: A) -> Box<T, A>`; [`from_non_null_in`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.from_non_null_in): `unsafe fn from_non_null_in(ptr: NonNull<T>, alloc: A) -> Box<T, A>`; [`into_raw_with_allocator`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.into_raw_with_allocator): `fn into_raw_with_allocator(b: Box<T, A>) -> (*mut T, A)`; [`into_non_null_with_allocator`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.into_non_null_with_allocator): `fn into_non_null_with_allocator(b: Box<T, A>) -> (NonNull<T>, A)`; [`allocator`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.allocator): `fn allocator(b: &Box<T, A>) -> &A`.
- Стабильные allocator-aware `Vec` API (каждый помечен rustdoc «Stable since 1.100.0»): [`new_in`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.new_in): `fn new_in(alloc: A) -> Vec<T, A>`; [`with_capacity_in`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.with_capacity_in): `fn with_capacity_in(capacity: usize, alloc: A) -> Vec<T, A>`; [`from_raw_parts_in`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.from_raw_parts_in): `unsafe fn from_raw_parts_in(ptr: *mut T, length: usize, capacity: usize, alloc: A) -> Vec<T, A>`; [`from_parts_in`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.from_parts_in): `unsafe fn from_parts_in(ptr: NonNull<T>, length: usize, capacity: usize, alloc: A) -> Vec<T, A>`; [`into_raw_parts_with_allocator`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.into_raw_parts_with_allocator): `fn into_raw_parts_with_allocator(self) -> (*mut T, usize, usize, A)`; [`into_parts_with_allocator`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.into_parts_with_allocator): `fn into_parts_with_allocator(self) -> (NonNull<T>, usize, usize, A)`; [`allocator`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.allocator): `fn allocator(&self) -> &A`.

**Явно не стабилизировано:** `try_*` allocator API (включая `Box::try_new_in` и `Vec::try_with_capacity_in`, помеченные в 1.100 как nightly-only `allocator_ext`), allocator-aware `Box::new_uninit_in` / `new_zeroed_in` (nightly-only `allocator_ext`) и `pin_in` (nightly-only `allocator_ext`, с bound `StaticAllocator`). `AllocatorClone` и `StaticAllocator` также unstable, под `allocator_ext`; `GlobalAllocator` из списка связанных extensions не следует считать stable. Не переносить отметку unstable на stable `Box::new_uninit` / `new_zeroed` без `_in`: это отдельные API. `VecDeque`, `String` и прочие коллекции с allocator-параметром не входят в перечисленную стабилизированную поверхность. Конкретные сигнатуры и gates здесь сверены по rustdoc именно версии 1.100, а не выведены из одного release listing.


## 2. Кандидатные backing-модели

| Кандидат | Звуковость и lifetime | Потоки / клонирование | Сложность и цена | Совместимость и риски |
|---|---|---|---|---|
| **A. `SeferAlloc` / ZST-handle поверх global routing** | `SeferAlloc` сейчас реализует только `GlobalAlloc` (`src/global/sefer_alloc/global_alloc.rs:96`). Под `alloc-decommit` структура имеет config field, без него — ZST. У `SeferAlloc` есть `const`-конструктор. Глобальная маршрутизация хранит ownership/route независимо от экземпляра handle; это потенциально естественная основа для эквивалентных экземпляров, но это нужно доказать для всех alloc/dealloc путей, включая fallback, shutdown и cross-thread free. Нельзя автоматически выводить soundness из того, что тип ZST. | Root allocator рассчитан на registry-backed thread heaps и `alloc-xthread`; `alloc-global` включает `alloc-xthread`. Thread exit перерабатывает слот whole-slot, а не уничтожает backing storage; поздние cross-thread frees маршрутизируются по адресу. Это не тождественно owner-scoped borrowed handle, lifetime которого завершился: последнему нужен явный контракт о том, как allocations остаются освобождаемыми после окончания borrow. Никакого `unsafe impl Send/Sync` не добавлять без доказательства инвариантов `SeferAlloc` и config. | Удобный высокоуровневый путь, хороший кандидат на тонкий adapter без удержания `HeapCore`; возможно, системный dispatch / registry lookup дороже локального handle. Измерений в фазе 1 нет. | Требует `std` и существующих allocator features; не `no_std`. Может сосуществовать с feature bundles только при точном feature closure. Нельзя менять `production` без отдельного обоснования/согласования. `alloc-xthread` и virgin-zero-skip не должны ослаблять контракт: cross-thread dealloc должен оставаться корректным, а zeroing — гарантированным. hardened mode и Windows/Linux должны быть проверены отдельно. |
| **B. HeapCore/registry-bound handle** | Handle, непосредственно держащий конкретный heap, упрощает локальную привязку, но узок по lifetime/эксклюзивности. `AllocCore` — `!Send`/`!Sync` (pin-тест `tests/oxx_r2_07_alloc_core_not_send.rs`) и должен оставаться локальным эксклюзивным субстратом, а не `Clone`/shareable allocator handle. Не превращать `&mut`/owner pointer в безопасный многопоточный allocator. Реализация allocator-а должна обеспечивать возможность корректно освободить каждый block, включая после выхода thread-owner. | Owner-thread mutation и cross-thread free — разные операции: существующая registry/route-модель позволяет маршрутизировать free, thread exit перерабатывает slot целиком. Нельзя заявлять, что allocation исчезает при exit; нельзя также считать, что handle, заимствованный у владельца, автоматически переживает lifetime владельца. Clone/thread-transfer требует самостоятельного доказательства allocator-equivalence и ownership. | Потенциально минимальный dispatch для локального сценария, но высокая сложность: безопасное владение heap, registry leases, эксклюзивность, shutdown и fallible paths. Производительность не измерена. | Затрагивает глубоко внутренний API и его unsafe seam; риск конфликтов с `alloc-xthread`, hardened конфигурацией и feature combinations. Не рекомендуется для первого stable adapter. |
| **C. `sefer-region` / arena** | Это отдельный slotmap store, не backing `AllocCore` и не текущая память SeferAlloc. Arena может быть полезна для lifetime-scoped объектов, но `clear`/drop не могут инвалидировать allocation, пока клиентские `Box`/`Vec` или raw parts ещё живы. Must satisfy `Allocator` deallocation and equivalence semantics, включая перенесённые контейнеры. | Может сохранять локальность/эксклюзивность и не требует глобальной heap-модели; однако перенос/Clone/`Send`/`Sync` зависят от точного API region и lifetime, а не от trait. Не обобщать поведение `Region` на `Allocator`. | Это отдельный дизайн candidate, вероятно потребует отдельной стратегии reclaim/alignment/resize и non-alloc metadata. Не является дешёвым adapter к уже существующему `AllocCore`. | Возможно `no_std`/`alloc` там, где подходит region, но стандартные allocator-коллекции требуют Rust 1.100. Совместимость нельзя обещать до проверки target/feature/soundness; `virgin-zero-skip` не эквивалентен zero-fill. |

### Вывод по кандидатам

Исходный API, соответствующий текущей модели, — adapter над route-capable `SeferAlloc`, а не над owner-local `AllocCore`. При этом нельзя предполагать, что сама реализация глобального trait `Allocator` на статическом ZST автоматически безопасна: надо подтвердить эквивалентность всех его экземпляров и lifetime выделений, включая thread exit, cross-thread drop и exit/fallback routing. Если такой контракт не удаётся сформулировать простым и проверяемым образом, следует отложить реализацию, а не расширять unsafe surface.

## 3. Отображение операций на существующие пути

### 3.1. Allocate / zeroed allocation

`allocate(layout)` должен преобразовать null/failure из существующего пути в `Err(AllocError)` лишь если данный путь действительно сохраняет recoverable failure semantics. Если текущая конфигурация/путь abort-ит либо ошибку нельзя получить без изменения поведения, adapter обязан это документировать; нельзя утверждать, что trait гарантирует recoverable OOM.

`allocate_zeroed(layout)` гарантирует нули для всего доступного клиенту результата согласно контракту. Существующий `GlobalAlloc::alloc_zeroed` и механизм virgin-zero-skip описаны в `src/global/sefer_alloc/global_alloc.rs` и `src/global/sefer_alloc/core.rs`: пропуск обнуления разрешён только для действительно virgin памяти; reused block должен быть полностью очищен. Virgin-zero-skip — оптимизация исполнения гарантии, а не её ослабление. Нельзя возвращать reused memory как zeroed только потому, что включён feature.

### 3.2. Grow и shrink

`GlobalAlloc::realloc(ptr, old_layout, new_size)` принимает старый layout и новый размер, но alignment не меняет. Текущий `SeferAlloc::realloc` имеет in-place fast path и fallible alloc-copy-dealloc fallback; при OOM исходный allocation остаётся жив. Это не полная замена `Allocator::grow`/`shrink`, у которых новый layout может менять alignment.

Proposed mapping:

1. Проверить старый pointer/layout в соответствии с unsafe контрактом вызывающего; сформировать новый layout без переполнения.
2. Для совместимого alignment и операции, где текущий путь сохраняет нужные guarantees, использовать underlying realloc-механику только после точной сверки её размеров и semantics.
3. Для изменившегося alignment (или любого непокрытого случая) выделить новый блок с новым layout, проверить `fits`/returned slice length и alignment, скопировать требуемые bytes (`min(old_size, new_size)` для обычного grow/shrink), затем освободить старый блок только после успеха.
4. При ошибке оставить старый блок и его bytes доступными и вернуть ошибку. Нельзя освобождать исходный блок до успешного получения нового.
5. `grow_zeroed` должен гарантировать нулевую инициализацию расширенной области даже если новый backing block частично переиспользован. Недостаточно считать, что `alloc_zeroed` либо virgin-zero-skip сам по себе zero-fills нужную новую часть: измеряемая/фактическая длина tail и новый запрошенный layout — разные величины.
6. После любого успеха старый pointer инвалидирован, даже при in-place реализации. Документация adapter-а должна не обещать сохранение старого указателя.

В частности, различать (а) запрошенный новый layout, (б) фактическую длину slice, вернувшуюся от allocator-а и (в) размер/extent, который существующий allocator реально зарезервировал. В каждом случае `fits` разрешает использовать только диапазон, разрешённый исходным запросом и returned length при совпадающем alignment.

## 4. Размещение кода, unsafe seams и доказательства

### 4.1. Место реализации

Предварительный вариант фазы 2 — новый отдельный helper crate (см. §5), с локальной ответственностью за adapter и его narrowly scoped unsafe implementation. Альтернатива, если owner решит держать API в root, — новый отдельный cfg-gated module. В обоих случаях нельзя изменять production GlobalAlloc-реализацию в `src/global/sefer_alloc/global_alloc.rs`.

`Allocator` — unsafe trait: реализация должна иметь явное доказательство контрактов allocator equivalence, no overlap, alignment, lifetime/invalidation, ZST, failure preservation и корректного drop. Не вводить новый tier-1 seam автоматически. Tier 1 — `#![allow(unsafe_code)]` на модуле; tier 2 — item-level allow на конкретном unsafe item с собственной причиной, `# Safety` и комментариями `// SAFETY:` у каждого unsafe site. Перед добавлением определить, может ли реализация поместиться в уже допустимую узкую seam; если новый tier-1 seam неизбежен, обновить полный inventory в README §“Where unsafe lives — the complete list” и сохранить в `src/lib.rs` указатель на README и canonical grep, проверяемые через `scripts/verify-doc-inventories.mjs`. Не считать дизайн-документом разрешённым обходом inventory policy.

Не добавлять `dbg_*` hooks «для удобства» adapter-а. Если в будущем появится диагностический hook, raw-pointer-зависимый hook обязан быть `unsafe fn` с `# Safety`, а hook без production caller по умолчанию должен быть закрыт за `bench-internals`; исключения требуют индивидуального reviewed оправдания согласно `scripts/verify-dbg-hook-safety.mjs`. Диагностический API не является доказательством корректности allocator-а.

### 4.2. Production неизменность

Требуемое проектное свойство — существующая реализация `GlobalAlloc` остаётся буквально неизменной в production bundle. Изоляция: adapter живёт в новом отдельном модуле/crate и исключён из `production`; никакого изменения `global_alloc.rs`. Проверка фазы 2 должна включать zero-trust сравнение с базой, подтверждающее идентичность байтов/текста `global_alloc.rs`, и аудит closure features по root `Cargo.toml` и `scripts/verify-production-feature-list.mjs` (и соответствующим gates). Одного совпадения текста недостаточно для заявления о машинной идентичности: если затрагиваются production-сборки или есть ghost-связь, `ghost-zero-cost` требует normalized `.text` diff и callgrind Ir production probe против base. Формулировать результат точно: неизменный файл — source-level факт, не доказательство совпадения machine code.

`production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit"]`. Нельзя добавлять новую feature в этот bundle без отдельного одобрения/evidence. Если adapter opt-in, убедиться, что его feature closure не включён в production и что root production feature list не меняется.

### 4.3. Ghost / ordering

Если реализация добавляет atomics/ordering/protocol sites, это отдельная область проектирования: каждый новый/изменённый site требует catalog scope и при protocol — rule, fixture mutant в `tests/ghost_lint_fixtures/`, а изменяющий код commit — соответствующий `Invariant-Check:` trailer с `Change`, `Why`, `Covered-by`, ghost id и fixture. Ghost не добавляет production fields, не выделяет память, не пишет production data; production machine identity проверяется `ghost-zero-cost`. Если новых atomic/order sites нет, новых ghost rule и Loom модели не требуется; это предложение не предполагает их добавления.

## 5. Выбор MSRV и размещения API

Известные значения на момент proposal: root `Cargo.toml` — edition 2021, `rust-version = "1.93"`; companion alloc-core/registry MSRV 1.93; `sefer-region` MSRV 1.88; workspace содержит отдельно публикуемые crates. Версии crate и dependencies не повышать. Владелец разрешил реализацию и, при необходимости, поднятие MSRV до 1.100.

| Вариант | Польза | Цена / ограничение |
|---|---|---|
| **1. Поднять root MSRV 1.93 → 1.100** | Наиболее прямой public API root-крейта; не нужен дополнительный adapter package. | Все потребители root, включая те, кому нужен только старый API, должны иметь Rust 1.100. Влияет на root/workspace support и MSRV CI row. Не повышать MSRV всех companion crates без потребности. Проверить docs.rs точный `production` feature set. |
| **2. Opt-in feature в root** | Сохранить API физически рядом с SeferAlloc и обозначить opt-in. | `rust-version` в manifest — crate-level, не feature-specific: feature не позволяет честно обещать, что один и тот же root package на 1.93 собирается без feature и на 1.100 с ним. Значит это не сохраняет полноценную MSRV-совместимость root package для старого compiler. Кроме того, нельзя добавлять feature в production без отдельного разрешения; feature closure/CI и документация должны быть точны. |
| **3. Отдельный helper crate с MSRV 1.100 (рекомендация)** | Root остаётся на 1.93; allocator API и новый MSRV локализованы в отдельном package; dependency направлена helper → root, без обратной зависимости и циклической зависимости. Не требуется менять версии root/dependencies. | Дополнительный package и его собственные MSRV/CI/docs.rs/feature-set rows; опубликование и совместимость helper — отдельная поверхность. Нужно проверить, что helper способен реализовать safe lifetime/ownership contract, не требуя приватных деталей или новых unsafe seams root. |

Рекомендация — вариант 3. Во второй фазе координация `.github/**`, `docs/CI_TIERS.md`, manifest-ов helper, package-list/CI ownership и открытых вопросов/индексов выполняется в той же фазе отдельными commits по CLAUDE.md. Если выяснится, что helper невозможен без root API/изменения root, вернуться к варианту 1 отдельным решением, не выдавая вариант 2 за feature-specific MSRV.

В любом выбранном варианте фазы 2: явно сверить затронутые `rust-version` во всех manifest-ах (не менять companion MSRV без прямой нужды), MSRV job в `.github/workflows/ci.yml`, единую таблицу строк CI `scripts/check-matrix.mjs`, а также docs.rs конфигурацию. Для crate с `package.metadata.docs.rs.features` документационный lint обязан запускаться ровно с тем feature list, что публикуется, помимо `--all-features`, если они отличаются. Не менять версии crate/deps и не обещать поддержку коллекций за пределами стабилизированного API.

## 6. План тестирования фазы 2

Все executable tests — только в `tests/`, без runnable doctests. Предлагаемые integration test groups (точные имена выбрать при реализации, не создавать сейчас):

1. **Layout/alignment:** минимальный и повышенный alignment; несколько классов размеров; проверка адреса и фактической slice length; проверка правила `fits`; rejecting/ошибочная геометрия без overflow.
2. **ZST/zero-size:** zero-size `Layout` для корректных alignment, dangling non-null pointer с верным alignment, корректное освобождение/no-op, `Box`/`Vec` с ZST; убедиться, что не вызывается GlobalAlloc contract размера 0 как будто он разрешён.
3. **Allocate/drop:** `Vec::new_in`, `Vec::with_capacity_in`, push/read/drop; `Box::new_in` и drop; циклы allocation/drop с reused memory. Проверить, что allocator handle/эквивалентный handle может освободить все блоки в соответствии с заявленным API.
4. **Resize:** grow и shrink сохраняют требуемый prefix; success инвалидирует старый pointer в тестовой модели; меняющийся alignment; проверка фактической returned length; grow_zeroed гарантирует нули в добавленном диапазоне, включая reused block.
5. **Failure:** принудительный/детерминированный OOM на allocate и на fallback leg grow/shrink; при ошибке старый pointer и bytes остаются живыми и освобождаемыми, а allocator не освобождает исходный блок заранее. Различать ожидаемую `AllocError` от путей, которые могут abort.
6. **Global allocator semantics:** zeroed allocation на virgin и reused блоках; доказывать, что reused block очищен и virgin-zero-skip влияет лишь на действительно virgin memory.
7. **Cross-thread lifetime:** allocation в одном thread, drop/free в другом; owner thread завершается до free; slot whole-slot recycling/reuse не теряет возможность route/free. Тест не должен ошибочно утверждать, что сам allocation погибает при exit; отдельно проверяет borrowed/owner-scoped adapter semantics, если они предлагаются.
8. **Feature/platform:** выбранный helper/root feature set, production closure не содержит adapter; hardened feature row, Windows и Linux CI rows, no_std граница (SeferAlloc adapter остаётся std-only). Проверки выполнять только в CI/после явного разрешения; здесь они не запускались.

**Miri:** целевые bounded integration tests на pointer provenance/ownership для allocate/deallocate и resize, ZST/dangling alignment, смену alignment, error-preserves-old-block; учитывать известное ограничение текущего GlobalAlloc пути и не объявлять Miri-clean без фактического результата.

**Loom:** только если фаза реализации добавит или изменит синхронизацию/orderings/протокол. Без новых atomic/order sites — Loom не требуется, не создавать модель искусственно.

**IAI:** если есть утверждение о стоимости/mechanism, отдельно измерять entry/construct-or-use adapter, allocate/free циклы, realloc/grow/shrink и zeroed сценарии; не объединять в один aggregate, masking paths. Именовать точный entry point и сопоставлять равные work units/features. Любой механизм-ориентированный benchmark обязан иметь path-activation oracle на каждую arm: доказательство, что измерялся нужный fast/in-place/fallback/virgin/reused path, а не только включённый feature или ожидаемая конфигурация. Не добавлять hooks ради теста/bench без необходимости; проверить существующие counters, а при неизбежном новом dbg hook соблюдать `bench-internals`/unsafe правила §4.

## 7. Поэтапная реализация и риски

### 7.1. Phase 2 последовательность

1. **Закрыть design questions до кода:** выбрать root vs helper; зафиксировать allocator-equivalence/clone policy (предпочтительно не предоставлять Clone без нужды), lifetime model, post-owner-exit free contract, ZST и allocation failure behavior. Подтвердить Rust 1.100 stable surface по источникам §8.
2. **Создать adapter отдельно от GlobalAlloc:** helper crate предпочтителен; если root, отдельный narrow cfg-gated module. Не редактировать `src/global/sefer_alloc/global_alloc.rs`; не трогать production bundle. Unsafe contract и per-site comments до реализации tests.
3. **Реализовать allocate/deallocate и контрактные tests** в `tests/`; сначала alignment, ZST, routing, drop/cross-thread. Добавить только нужную tier-2/tier-1 seam и inventory updates, если они действительно требуются. Не добавлять диагностические hooks без обоснования.
4. **Реализовать zeroed/grow/shrink semantics** с fallible allocation preserving old block; тесты на изменение alignment, фактический slice size, zeroed reused tail и failure. Не считать `GlobalAlloc::realloc` полной реализацией Allocator API.
5. **Интегрировать features/MSRV/CI** после согласования scope owner в той же фазе, отдельными commits по CLAUDE.md: координация `.github/**`, `docs/CI_TIERS.md`, manifest-ов, открытых вопросов и индексов. Не расширять фазу #128 сверх согласованного scope и не объединять эти изменения скрыто с текущим документальным deliverable. Обновить exact-feature docs.rs row и `scripts/check-matrix.mjs` source-of-truth row; проверить `.github/workflows/ci.yml` MSRV job.
6. **Zero-trust verification:** source diff подтвердит неизменность GlobalAlloc-файла; проверить feature closure через production gate и исключение adapter из `production`; CI запускает контрактные tests, Miri (выбранные targets), платформенные/MSRV строки. Ghost/Loom/`Invariant-Check:` применяются только если реально менялись ordering/protocol sites. Производительность не объявлять без заранее определённого измерительного протокола и фактических результатов.
7. **Завершение фазы:** review целого diff, проверка inventory/invariants, commit только после CI согласно проектным фазовым правилам. Conventional commit prefix отражает реальный характер изменения: `feat(...)` для API/реализации; `test(...)` для изолированной test работы; `build(...)`/`ci(...)` для build/CI; `docs(...)` для документации. Не использовать `perf(runtime)` для дизайн-документа, тестов или measurement-only работы. Если работа только измерительная — `bench(...)` по таксономии CLAUDE.md. Никаких git-записей в этой фазе.

### 7.2. Evidence / gate taxonomy

В фазе 1 измеренных чисел нет, поэтому не создаются gate report и evidence-registry row; этот документ — design proposal, а не perf-gate. Не делать числовых latency/throughput/Ir/RSS claims. Если фаза 2 захочет сделать performance claim или gate, до получения чисел предварительно зарегистрировать вопрос/критерии в evidence registry по принятому process; после измерения добавить соответствующие registry rows и воспроизводимые evidence artifacts, с точным entry point, feature set, source identity и path-activation oracle. Gate verdict, зависящий от измеренных чисел, требует raw logs и summary CSV по CLAUDE taxonomy; отчёт без измеренных чисел их не требует. Invariant trailers нужны только для действительно затронутых защищённых ordering/protocol rules.

### 7.3. Риски и не-цели

- `Allocator` — не переименование `GlobalAlloc`: failure model, zero-size и изменяемый alignment различаются.
- Факт thread exit не означает уничтожение текущего allocation: registry slot перерабатывается whole-slot; отдельно опасен lifetime-scoped borrowed adapter, чья привязка закончилась, пока allocation ещё существует.
- `AllocCore` нельзя превратить в `Clone`/общий handle, игнорируя `!Send`/`!Sync` и эксклюзивность.
- `sefer-region` не является alias/backing layer для SeferAlloc; arena clear и живые allocations требуют отдельного решения.
- Успех trait-метода не должен обещать recoverable OOM, если underlying путь abort-ит; никакие методы не должны unwind-ить.
- Reused memory должна быть zeroed даже при virgin-zero-skip; расширение не может раскрыть старые bytes.
- Не включать adapter в `production`, не менять существующий GlobalAlloc код, не поднимать версии crate/deps, не расширять API нестабильными `allocator_api` extensions.
- Не заявлять win по скорости/памяти; performance gate, ghost instrumentation и новые dbg hooks не являются целью этой фазы.

### 7.4. Реализация (Task #128, phase 2)

Реализация находится в отдельном `sefer-alloc-api` helper package, с `rust-version = 1.100`; root остаётся на MSRV 1.93. Helper не входит в root `production` feature list. Его `SeferAllocator` — отдельный не-Clone handle поверх route-capable `SeferAlloc`; не добавлялись поля в root allocator и не редактировался `src/global/sefer_alloc/global_alloc.rs`.

Все nonzero backend callbacks выполняются синхронно через приватный `unsafe extern "C"` trampoline с stack-local borrowed call record. Rust non-unwinding ABI aborts if unwind reaches the boundary; when a callback panics, the panic hook runs before unwind reaches `extern "C"` and the boundary aborts. The hook's payload/re-entry risk is grounded in `src/global/sefer_alloc/mod.rs:154-157` (panic hook and payload re-entry). OOM/internals may also be unrecoverable; `AllocError` does not promise recoverable OOM. Contract tests не инжектируют детерминированный отказ: нет публичного deterministic failure control, поэтому поведение allocate-before-free гарантирует, что исходный блок не освобождается до успешной замены, но конкретный отказный путь не проверен.

Zero-size allocate и resize-to-zero возвращают aligned dangling pointer; deallocation zero-size не вызывает backend. Resize выделяет replacement до освобождения старого блока, копирует `min(old.size(), new.size())`, для grow-zeroed очищает нужный добавочный хвост. Contract coverage включает alignment/ZST/collections, изменение alignment при grow/shrink, reused zeroed storage и child-process cross-thread reclamation; отдельный ZST target предназначен для Miri. Linux-only panic subprocess gate requires SIGABRT (Linux ABI signal 6), отсутствия exit code, non-unwinding runtime diagnostic и callback probe text. When the callback panics, the hook runs before unwind reaches the C ABI boundary, which aborts; hook/payload re-entry is covered at `src/global/sefer_alloc/mod.rs:154-157`. При регрессии trampoline на Rust ABI panic callback unwind-ится вместо требуемого SIGABRT/diagnostic; родительский тест падает, так что обычный unwind-результат не может пройти.

Helper остаётся workspace member: аудит фактических команд в `.github/workflows/ci.yml`, `.github/actions/**`, `scripts/check-all.mjs` и `scripts/check-matrix.mjs` не выявил прямых build/test команд с `--workspace`. Именованные Cargo build/check/test команды без `-p` выбирают root default package; jobs для member явно задают `-p`. `cargo fmt --all` охватывает workspace members, но не выбирает compiler MSRV. Следовательно, ни одна проверенная CI-задача не builds/resolves helper ниже 1.100. Это намеренное ограничение workspace-subcommand, а не гарантия all-member MSRV; ни одна CI-задача не использует `--workspace`. Helper остаётся member без отдельного lockfile/workflow. Наблюдавшийся `cargo check --workspace` с default 1.97 завершается ошибкой как MSRV audit, а не CI invocation.


Эта текущая implementation note не заменяет proposal: первоначальная рекомендация и provenance источников выше остаются source of truth для design rationale.

## 8. Официальные источники

Указанные ниже rustdoc/release страницы — версия Rust 1.100.0. В тексте выше stable/unstable distinction относится к этим источникам, а не к неверсированному nightly rustdoc.

1. [Rust 1.100.0 release listing, 2026-11-12](https://doc.rust-lang.org/1.100.0/releases.html#version-11000-2026-11-12) — подтверждает стабилизированный срез custom Allocator API, `std::alloc::Global`/`System` и связанные реализации, allocator-параметризованные `Box`/`Vec` и перечисленные raw/parts API; listing не означает, что весь список API стабилен.
2. [Rust 1.100.0 release blog](https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/) — контекст выпуска и стабилизации.
3. [`std::alloc::Allocator`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.Allocator.html) — required/provided methods, `fits`, safety и allocation semantics.
4. [`core::alloc::Allocator`](https://doc.rust-lang.org/1.100.0/core/alloc/trait.Allocator.html) — core-facing trait reference.
5. [`std::alloc::Global`](https://doc.rust-lang.org/1.100.0/std/alloc/struct.Global.html) — глобальный allocator type.
6. [`std::alloc::GlobalAlloc`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.GlobalAlloc.html) — контрастирующий контракт, включая zero-size ограничения.
7. [`Box::new_in`](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html#method.new_in) — allocator-aware Box constructor.
8. [`Vec::new_in`](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html#method.new_in) — allocator-aware Vec constructor.
9. [`AllocatorClone`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.AllocatorClone.html), [`StaticAllocator`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.StaticAllocator.html) и [`GlobalAllocator`](https://doc.rust-lang.org/1.100.0/std/alloc/trait.GlobalAllocator.html) — нестабильные extension traits; rustdoc указывает gate `allocator_ext`.
10. [`Box` 1.100 rustdoc](https://doc.rust-lang.org/1.100.0/std/boxed/struct.Box.html) и [`Vec` 1.100 rustdoc](https://doc.rust-lang.org/1.100.0/std/vec/struct.Vec.html) — полные сигнатуры и стабильность allocator-aware методов; unstable methods показывают gate `allocator_ext`.
