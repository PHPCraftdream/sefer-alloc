# sefer-alloc: ревью src/global и его межмодульных контрактов — раунд 1

Автор: Сол-кодекс (sol-codex).
Метка отчёта: 2026-09-10 07:44:42 +02:00, Europe/Berlin.
Проверяемый commit: `58e7a82c1496794e10dcec8b6ac0b6991e55c73c`, ветка `main`.
На начало и окончание чтения tracked working tree был чистым.

## Итог

**NO-GO для рассмотренного allocator-пути: найден P1 use-after-free в обычном cross-thread dealloc.** Кроме него подтверждены два P2: создание/ожидание registry chunks из диагностического `stats()` и обход deferred-free reclamation в Large batch allocation.

| Приоритет | Количество | Смысл |
| --- | ---: | --- |
| P0 | 0 | Безусловная катастрофа независимо от режима не обнаружена |
| P1 | 1 | Нарушение безопасности памяти при корректном использовании |
| P2 | 2 | Существенные ошибки поведения / удержания ресурсов |
| P3 | 6 | Улучшения, ошибки наблюдаемости и документации |
| P4 | 1 | Сопровождаемость и устаревшие локальные пояснения |

Всего **10 замечаний**. Это не вердикт о всём `src`: остальные подсистемы ещё требуют самостоятельных раундов.

Работа — **статическое чтение**. Тесты, сборки, бенчмарки, Miri, loom, примеры и диагностические прогоны не запускались; подагенты не использовались. Контрпримеры ниже выведены из исходников, а не воспроизведены запуском. Единственная запись в репозиторий — этот отчёт; коммит предназначен только для него.

## Границы и метод

После предложения пользователя вести циклы по подпапкам этот раунд сфокусирован на `src/global`, с обязательной проверкой вызовов за границу папки.

Полностью прочитаны:

- `src/global/sefer_alloc.rs`;
- `src/global/tls_heap.rs`;
- `src/global/fallback.rs`;
- `src/global/alloc_stats.rs`;
- `src/global/mod.rs`;
- `src/lib.rs`;
- `docs/INTEGRATION.md`.

Выборочно, по проверяемым цепочкам: registry claim/bootstrap/статистика, `HeapCore` alloc/realloc/zeroed/batch/dealloc, remote-free publication, owner drain, small-pool release, `SegmentTable::recycle`, OS release, configuration builders. Прочитаны относящиеся к этим контрактам части manifest/lockfile, README и тестов. В частности, рассматривались `tests/sefer_alloc_examples.rs`, `stats_reflects_activity.rs`, `regression_r3a_stats_walk_compiled_out.rs`, `regression_fallback_panic_lock.rs`, части `batch_tcache.rs` и `r31_10_trim_current_thread_api.rs`; они не исполнялись.

Применён `rust-intel`: доказательство времени жизни до **последнего** доступа, проверка момента передачи владения, различение доступности API и `doc(hidden)`, проверки ошибочных/холодных путей и соответствия комментариев фактическим вызовам. Утверждения о Rust-контрактах дополнительно сверены с официальной документацией; ссылки приведены у соответствующих замечаний.

Не выполнены полный аудит `registry`/`alloc_core`/`concurrent`, формальная проверка атомарных протоколов, feature powerset, проверка машинного кода и переизмерение скорости. Уже известный post-write residual `InitStateGuard` (correctness item 23) не объявляется новым найденным багом: между записью heap и READY в просмотренном коде не найден достижимый production panic.

### История рассмотренного кода

Это новый проход по текущему срезу, а не предположение, что все проблемы возникли в последнем коммите:

- последнее изменение `src/global` — `4206c44` от 2026-08-09, уточнение независимости handle-API и allocator;
- `7a9b7c7` / `25d6ac4` / `dbb4016` ограничивали диагностический API; фактические gates проверены в исходниках, сами commit messages не используются как доказательство исправности;
- `c270b0c` добавил rollback guard fallback initialization, `e550006` изменил no-panic документацию;
- в связанном `heap_core_xthread.rs` последнее изменение `49929d0` касается fallible chunk resolution на free-path; оно не закрывает G1;
- последние изменения registry в начале сентября связаны с интеграцией `tagged-index-stack`. Его внутренности заново не ревьюились, а его завершённые ревью не заменяют проверку жизненного цикла сегментов.

## G1 — P1: чтение заголовка после передачи блока владельцу допускает use-after-free

**Режим:** `alloc-global + alloc-xthread + alloc-segment-directory + alloc-decommit`; включён в `production`. Не требует `internals`, неправильного Layout, double-free или использования указателя после dealloc со стороны приложения.

**Ключевые места:**

- `src/global/sefer_alloc.rs:1007`, `:1055`: обычный и bind-less dealloc сходятся на foreign routing;
- `src/registry/heap_core_xthread.rs:1233`–`:1237`: сначала успешный `ring.push(packed)`, потом `set_dirty_bit_for_segment(base, packed)`;
- `src/registry/heap_core_xthread.rs:1341`–`:1347`: тот же порядок после успешного retry;
- `src/registry/heap_core_xthread.rs:340`–`:342`: helper заново читает `segment_id` и `owner_state` из памяти сегмента;
- `src/alloc_core/remote_free_ring.rs:1223`: Release-публикация записи; `:1352`–`:1368`: Acquire-чтение и обработка владельцем;
- `src/alloc_core/alloc_core_small.rs:913`–`:966`: drain может довести live count до нуля и вызвать release;
- `src/alloc_core/alloc_core_small_pool.rs:373`–`:383` → `src/alloc_core/segment_table.rs:459` → `src/alloc_core/os.rs:452`: реальное освобождение OS reservation.

**Контрпример по исходникам:**

1. У владельца O есть Small-сегмент S, отличный от primordial и текущего bump-сегмента. В нём осталось два живых пользовательских блока; остальные уже возвращены из magazines/free paths. Pool выключен через разрешённый `pool_segments(0)` либо заполнен.
2. Первый блок опубликован в remote ring вместе с dirty hint.
3. Поток P освобождает последний блок: `ring.push` публикует запись и возвращает успех. P приостановлен **до** вызова `set_dirty_bit_for_segment`.
4. O по уже существующему dirty hint либо linear-scan fallback читает обе записи. Last reclaim уменьшает `live_count` до нуля; после окончания drain O освобождает S через `table.recycle`.
5. P продолжает выполнение: `segment_id_at(S)` делает `Node::read_u32` из уже освобождённой памяти (`segment_header_views.rs:203`–`:205`). Затем потенциально читается и `owner_state`.

С default pool проблема не исчезает: сегмент может быть освобождён при заполненном pool; если его сначала сохранили в pool, владелец может выполнить trim до возобновления P.

Комментарий `heap_core_xthread.rs:353`–`:356` доказывает лишь сохранность сегмента **до drain**. Здесь drain законно происходит раньше, чем producer закончил все свои обращения к сегменту. В `remote_free_ring.rs:320`–`:345` уже описана возможность забрать такую запись по чужому dirty hint / полному сканированию, но рассмотрена только discoverability, не lifetime.

Это именно allocator-side UAF, а не последствие нарушения caller contract. Raw read требует живой памяти на момент доступа; совпадение адреса с поздней reservation не восстанавливает прежнее владение. См. [Rust: pointer safety и provenance](https://doc.rust-lang.org/std/ptr/index.html#safety).

**Исправление:** до публикации получить immutable routing snapshot: segment id и ссылку/указатель на process-lived `HeapSlotRemote` либо другие действительно бессмертные notification metadata. После успешной публикации не обращаться к заголовку, payload или ring storage освобождаемого сегмента. Dirty notification делать после publish, но из snapshot. Проверить обе успешные ветви — initial push и retry.

Просто передвинуть dirty-bit write перед ring publish нельзя: это меняет протокол видимости и допускает потребление hint до появления записи. Усиление Ordering само по себе тоже не продлевает время жизни памяти.

**Критерий закрытия, не исполнен:** детерминированно остановить producer непосредственно после публикации, дать владельцу drain и OS release, затем возобновить producer. Нужна проверка отсутствия post-release обращений, а не только равенство числа allocations/frees. Отдельно покрыть retry-success и pool-disabled/full варианты.

## G2 — P2: stats() с alloc-stats может создавать память, ждать initializer и abort на OOM

**Режим:** `alloc-global + alloc-stats` и хотя бы один из `fastbin` / `alloc-decommit`; например, `production + alloc-stats`.

**Места:** `src/global/sefer_alloc.rs:433`–`:447`, `:473`–`:505`; `src/registry/heap_registry.rs:1000`–`:1016`, `:1081`–`:1090`; `src/registry/bootstrap.rs:827`–`:833`, `:888`–`:920`.

Оба hit-counter агрегатора вызывают материализующий `reg.slot(idx)`. Обоснование «все chunks ниже count уже существуют» неверно: `bump_count` публикует увеличенный `count` на `heap_registry.rs:878`, а `claim` вызывает `reg.slot(idx)` только позже, на `:140` (аналогично `claim_with_config`).

На границе нового chunk, например при mint индекса 64 (`CHUNK_SLOTS = 64`), поток claimant может остановиться между этими действиями. Другой поток вызывает `stats()`, видит новый count и сам создаёт chunk. Если initializer уже стартовал, наблюдатель ждёт его через initialization state machine. Если OS reservation не удалась, `ensure_chunk` вызывает `std::process::abort()`.

Таким образом публичный диагностический accessor с обещанием «no locks, no allocation» способен выполнить OS allocation, busy-wait и аварийно завершить процесс. Это не только постоянная стоимость двух обходов.

**Исправление:** отдельный passive peek готового chunk/slot: Acquire-load, отсутствие либо INITIALIZING → пропустить, без materialization и без ожидания. `slot_or_none` не является таким исправлением: он тоже пытается создать chunk. Неполный snapshot допустим для уже заявленного диагностического контракта.

Попутно оба счётчика можно собирать одним обходом готовых chunks/slots: меньше повторных pointer/initialised loads. Это снижение константы, а не O(H) → O(1); ускорение не измерялось.

**Не затронуто:** без `alloc-stats` эти обходы действительно вырезаны. `bootstrap::ensure()` само по себе возвращает адрес const static, не создаёт registry. Замечание не отменяет исправление прежнего обхода compile-time нулей.

**Критерий закрытия, не исполнен:** задержать mint до chunk initialization и внутри initializer; `stats()` должен завершаться без участия в initialization, без увеличения reservation counters и без ожидания другого потока.

## G3 — P2: Large alloc_batch обходит deferred-free reclamation

**Режим:** `production + batch-api`; также `alloc-global + alloc-xthread + batch-api` без `fastbin`. В обычном `production` без opt-in batch API этого пути нет.

**Места:** `src/global/sefer_alloc.rs:905`–`:913`; `src/registry/heap_core_alloc.rs:886`–`:890`, `:1071`–`:1100`.

Large-ветвь сразу вызывает `alloc_batch_large`; эта функция циклом вызывает `self.core.alloc(layout)` и stamp. Non-fastbin batch делает то же самое. Ни одна не вызывает `drain_large_deferred_free`.

Скалярный `HeapCore::alloc`, напротив, выполняет drain перед Large request (`heap_core_alloc.rs:84`–`:89`). Наличие drain на `:985` batch не спасает: он находится в Small magazine-refill ветви, после раннего выхода Large-ветви.

**Контрпример:** живой owner A многократно вызывает `SeferAlloc::alloc_batch` с одним Large layout и выходным массивом длины 1. B освобождает блок и подтверждает завершение до следующего batch. Вызовы скалярного alloc/realloc и thread teardown у A не происходят. Free успешно ставит сегмент в deferred stack (`heap_core_xthread.rs:948`–`:974`), но очередной batch не обрабатывает её.

Число удерживаемых сегментов и занятых table entries растёт с числом циклов, хотя приложение держит не более одного блока. В конце возникает реальный OOM либо отказ `table.register` (`alloc_core_large.rs:606`–`:611`). До лимита это O(K) удержание вместо ограниченного steady state; добавление случайного scalar Large alloc скрывает проблему, потому что именно оно наконец делает drain.

**Исправление:** сохранить обязательную owner-side housekeeping часть scalar allocation и для batch. Для Large достаточно общего корректно расположенного drain prelude на batch; для non-fastbin отдельно сохранить необходимые overflow drains. Stamp уже есть и не заменяет reclamation. Не переносить эту работу на magazine-hit hot path без необходимости.

**Критерий закрытия, не исполнен:** batch-only owner + foreign frees + acknowledgment, без случайного scalar drain. Проверять рост reclaimed count, повторное использование table slots и ограниченное удержание после многих циклов; отдельно fastbin/non-fastbin. Просмотренный `batch_tcache.rs` сосредоточен на Small и same-thread сценариях и эту цепочку не доказывает.

## G4 — P3: explicit trim не обслуживает уже завершённые remote frees

**Места:** `src/global/sefer_alloc.rs:574`–`:580`; `src/registry/heap_core_ownership.rs:252`–`:264`.

`trim_current_thread` вызывает только flush magazines, drain small pool и evict large cache. Deferred Large stack и remote/overflow queues не входят в эту последовательность.

Пример: A выделил Large, B корректно освободил и подтвердил это, A перед idle вызывает trim. Блок всё ещё стоит в deferred stack, в large cache его ещё нет; evict ничего для этого блока не делает. Повторные trim без нового allocation traffic также не помогают. При thread exit соответствующий Large drain имеется отдельным шагом **перед** trim (`tls_heap.rs:238`–`:264`).

Это **функциональный пробел cold maintenance API**, а не доказанное нарушение его узкой перечисленной спецификации: документация прямо перечисляет cache/pool операции и не обещает полного remote drain. Но рекомендация использовать API в конце batch перед idle практически важна именно для такого сценария.

**Улучшение:** либо добавить owner-side обработку уже опубликованных remote frees перед cache eviction, либо явно отделить cache trim от более полного scavenge. Для расширенного варианта задать предел работы / snapshot semantics при продолжающемся потоке producers и уточнить стоимость. Не добавлять обход всех сегментов на каждый alloc.

**Критерий закрытия:** owner, foreign free, явный acknowledgment, trim без последующих allocations; наблюдать reclamation. Нельзя подменять этот сценарий alloc после trim: allocation само способно обработать очередь.

## G5 — P3: fallback не получает пользовательскую cache/pool configuration

**Места:** `src/global/tls_heap.rs:523`–`:532`; `src/global/sefer_alloc.rs:981`–`:983`, `:1098`–`:1099`, `:1115`–`:1117`; `src/global/fallback.rs:217`; `src/registry/heap_core.rs:614`–`:615`.

Для own-thread heap применяется `claim_with_config`, но переход в Fallback теряет этот путь. `fallback::heap_ptr` безусловно строит `HeapCore::new(u32::MAX)`, то есть core с DEFAULT, а не configuration единственного установленного `SeferAlloc`.

Даже если пользователь выбрал `budget_bytes(0)` и `pool_segments(0)`, поздние allocations после TLS teardown и allocations при registry exhaustion обслуживает fallback с обычным кешированием. При последующих drains Large-frees могут попасть в его default cache. Этот heap не имеет thread-exit trim, а публичный `trim_current_thread` для TORN — намеренный no-op.

Это не нарушение Rust memory safety и не требование независимых allocator instances. Пример использует **один** global allocator. Проблема — неочевидное исключение из выбранной политики, особенно неприятное для конфигурации «ничего не кешировать».

**Исправление/решение:** передавать configuration также в first initialization fallback с явным first-init-wins контрактом либо установить и документировать отдельную консервативную fallback policy. Уточнить исключение в `with_config`/integration docs, если различие намеренное. Не обещать, что default cache headroom является process RSS limit.

## G6 — P3: ring_overflows документирован как счётчик потерянных блоков, хотя восстановление ещё впереди

**Места:** `src/global/alloc_stats.rs:94`–`:105`; `src/alloc_core/remote_free_ring.rs:1202`–`:1208`; `src/registry/heap_core_xthread.rs:1248`, `:1341`–`:1348`, `:1391`–`:1395`, `:1467`–`:1471`.

`AllocStats::ring_overflows` описывает discarded block / actual leak на каждый overflow. На самом деле `DBG_RING_OVERFLOW` увеличивается при первом неудачном ring push; затем запись может успешно попасть в heap overflow ring либо в исходный ring на retry и быть освобождена штатно.

Простейший контрпример: segment ring заполнен, heap overflow имеет место. Counter растёт, второй push успешен, блок не потерян. Следовательно, делать вывод об утечке из этого поля нельзя.

**Исправление:** назвать его first-attempt ring pressure и убрать трактовку «один overflow = leaked block». Отдельно экспонировать окончательные concessions, для которых уже существует `DBG_RING_PUSH_RETRY_EXHAUSTED`, с точным охватом. Синхронно исправить старые утверждения о немедленном discard в module doc `remote_free_ring.rs`.

Это не предложение заново отменять ранее принятый bounded-loss overflow policy: замечание о том, **какая стадия** этого policy измеряется публичным полем.

## G7 — P3: no-panic обоснование опирается на более сильную гарантию, чем даёт GlobalAlloc

**Места:** `src/global/sefer_alloc.rs:111`–`:118`, `:961`–`:972`; `src/global/fallback.rs:304`–`:323`.

Утверждение «panic из любого GlobalAlloc method всегда abort, не UB, независимо от consumer panic strategy» смешивает compiler-generated global allocation shims и сам публичный trait. Нормативный [GlobalAlloc Safety contract](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#safety) запрещает unwinding. Прямой вызов trait method на `SeferAlloc` — реальный способ использования, в том числе в тестах проекта — не обязан проходить через `__rust_alloc` shim.

Вторая проблема доказательства: panic hook исполняется **до** panic runtime и при abort, и при unwind. Поэтому guard, который отпускает fallback lock лишь при unwinding, не защищает от allocating/reentrant panic hook, вызванного пока lock ещё удерживается. Это следует из [официального контракта set_hook](https://doc.rust-lang.org/std/panic/fn.set_hook.html), а не из предположения, что catch_unwind подавляет hook.

**Граница замечания:** новый достижимый при корректном production input tripwire в этом раунде не установлен. Поэтому это P3 ошибки обоснования/документации, **не второй подтверждённый P1** и не утверждение, что любой panic обязательно приводит к UB.

**Рекомендация:** раздельно описать обязательный trait contract, фактическую защиту конкретных shims и internal direct-call режим. Если выбран abort-on-corruption, использовать минимальный no-allocation/no-unwind abort path, не полагаться на пользовательский panic hook. Из proof перед `unsafe impl` убрать устаревшие blanket обещания «HeapCore never panics / only true OOM».

## G8 — P3: batch wrappers создают heap даже для пустой операции и foreign-only free

**Места:** `src/global/sefer_alloc.rs:905`–`:908`, `:945`–`:950`; сравнить со scalar dealloc на `:1007`–`:1055`.

Обе batch-функции разрешают `self.current_heap()` до проверки длины. На свежем потоке `alloc_batch(layout, &mut [])` либо `dealloc_batch(layout, &[])` может materialize полноценный heap. Проверка `out.len() == 0` в `heap_core_alloc.rs:878`–`:880` стоит уже слишком поздно для устранения TLS bind.

Непустой foreign-only `dealloc_batch` на unbound thread также создаёт own heap, хотя scalar dealloc давно имеет `ForeignNoBind` без создания heap и fallback lock. Это лишние OS reservations/initialization на рабочем потоке, который только возвращает чужие блоки.

**Улучшение:** пустой batch возвращать до TLS; dealloc batch под `alloc-xthread` разрешать passive dealloc resolver. Для Own сохранять batching; для ForeignNoBind вызывать уже существующий независимый routing по ненулевым элементам. Non-xthread policy не менять неявно.

Сложность free по N элементам остаётся O(N); выигрыш — устранение ненужной холодной инициализации и связанных allocations, а не новый асимптотический алгоритм. Это opt-in `batch-api`, не налог на default hot path.

## G9 — P3: Integration Guide даёт неверную модель memory limits и устаревший рецепт проверки

**Места:** `docs/INTEGRATION.md:61`, `:86`–`:89`, `:125`–`:135`, `:179`–`:228`; сравнить с `src/global/sefer_alloc.rs:250`–`:273`, `:307`–`:340`, `src/lib.rs:337`–`:355`.

Практически значимые расхождения:

- Пример «RSS-bounded server» с 512 MiB RSS ceiling настраивает **512 MiB cached bytes на heap**, не на процесс. При H heaps допустимый совокупный cache budget — H × B. Live allocations, small pools и metadata вообще не покрыты этим пределом. Это не прогноз фактического RSS, а отсутствие обещанной process-wide гарантии.
- Утверждение, что headroom ограничивает накопление **до** следующего события, неверно: это decay floor, не admission ceiling. Idle сам decay не запускает.
- `const` configuration — изменение исходников с последующей сборкой; фраза «a code change, no recompile needed» противоположна реально предоставленному API.
- Recipe читает `sefer_alloc::alloc_core::...` и `dbg_*` после рекомендации включить только `production`. Но путь `alloc_core` извне закрыт без `internals`. Следует явно назвать диагностические gates и использовать root reexports там, где они доступны. Проверка отдельного `AllocCore` не становится проверкой live global binding — собственное предупреждение guide об этом нужно сохранить.

**Исправление:** переписать guide в терминах per-heap cache budget, отдельно live memory / pool / metadata / fallback и event-driven decay; удалить обещание process RSS cap. Обновить feature composition по manifest и обозначить diagnostic-only recipe. Это пользовательская документация установки allocator, не только исторический комментарий.

## G10 — P4: рядом с текущими proof остаются противоречащие им исторические комментарии

Примеры из полностью прочитанной папки:

- `tls_heap.rs:120`, `:132`–`:147`: reverse **declaration** destructor order и давно удалённый abandon-segments этап; module doc выше уже правильно не полагается на этот порядок;
- `tls_heap.rs:659`–`:693`: повторный bind назван «cheap re-claim of the same slot», хотя `pick_slot` не знает caller thread и в принципе не гарантирует тот же slot. При этом сам LOCAL-error сценарий здесь не доказан достижимым; это ошибочное объяснение, не заявленная новая утечка;
- `sefer_alloc.rs:1047`–`:1053`: «dangling/garbage ptr cannot fault» противоречит raw header read и явному caller contract в `heap_core_xthread.rs:876`–`:919`;
- `sefer_alloc.rs:659`–`:662`, `:703`–`:706`: diagnostic hook одновременно «never claims a new slot» и вызывает binding `current_heap()`;
- `fallback.rs:14`–`:18`, `:334`–`:336`: contention объявлен почти невозможным потому, что tearing-down thread выходит. Несколько потоков могут исполнять поздние TLS destructors одновременно; registry exhaustion тоже маршрутизирует их в общий fallback.

**Улучшение:** у unsafe seam оставить короткий актуальный контракт, lifetime/order доказательство и ссылку на историческое решение; историю уже исправленных схем вынести из соседства с исполняемым кодом. Не удалять доказательства владения ради малого числа строк. В этом раунде длинный комментарий «live_count защищает freer» как раз скрывал разрыв во времени жизни G1.

## Что выглядит обоснованно в прочитанном коде

- Три разрешения — alloc, passive dealloc и passive trim — действительно имеют разные роли. Scalar foreign-only dealloc не создаёт heap; production trim на unbound/TORN thread не привязывает новый slot.
- `AbandonGuard` записывает TORN до relinquish ownership; drain/trim расположены до `HeapRegistry::recycle`. Не найдено второй записи в bins старым TLS owner после этого перехода в просмотренной цепочке.
- Fallback readiness публикуется Release и наблюдается Acquire; lock использует Acquire/Release, а failed initialization возвращает state к UNINIT. Одни лишь Relaxed diagnostic counters не являются дефектом.
- `SeferAlloc` имеет один `unsafe impl GlobalAlloc`; ручных `unsafe impl Send/Sync` внутри `src/global` нет. `CurrentHeap`, `CurrentHeapForDealloc` и `AbandonGuard` содержат concrete raw pointers, не generic owner `T`; дополнительные variance/PhantomData markers здесь не нужны. Dereference остаётся unsafe обязанностью вызывающего кода; само наличие pointer enum её не доказывает.
- `with_heap` не отдаёт безопасный `&mut HeapCore` за пределы closure и освобождает lock обычным RAII. Это подтверждение нормального execution path, не blanket гарантия при произвольном reentrant hook.
- В просмотренных realloc move legs новый блок проверяется на null до копирования/освобождения старого; длина копирования — `min(old_size, new_size)`. Это не полная проверка всех внутренних in-place/OS paths.
- `alloc_zeroed` делегирует отдельному HeapCore path; он различает свежие OS-backed Large spans и reused cache entries. Повторную zero-fill поверх этого в global wrapper добавлять не нужно.
- Feature boundary реальна: default — `std`, allocator opt-in; `production` не включает `batch-api`, `alloc-stats` и `internals`. G2/G3/G8 нельзя представлять как одинаково достижимые в любом build.

## Ускорение и снижение затрат: порядок действий

1. **G1 до любого perf tuning:** исправить время жизни без post-publish чтений освобождаемого сегмента.
2. **G3:** восстановить reclamation в batch-only workloads — это устранение O(K) удержания и лишних OS reservations, не косметическая экономия инструкций.
3. **G8:** убрать ненужную инициализацию heap для пустых batches и foreign-only batch dealloc.
4. **G2:** passive обход готовых chunks и совместный сбор двух hit counters. O(H) сохраняется, а initialization/wait из диагностического пути исчезает.
5. **G4/G5:** согласовать explicit maintenance и fallback с выбранной memory policy.
6. Дополнительный TTAS/backoff для fallback spinlock — только кандидат после реального подтверждения contention. CAS loop действительно может гонять cache line при массовом teardown, но замеров здесь нет; обычный короткий редкий путь не объявляется bottleneck по одному синтаксису.

Новых измеренных ускорений в этом отчёте **нет**. Не предлагаются уменьшение memory ordering «на глаз», unsafe unchecked вместо проверок, новые зависимости либо изменения defaults ради неподтверждённого бенчмарка.

## Как продолжать циклы по подпапкам

Разделять по ownership/жизненному циклу, не просто по числу файлов:

1. `global`: закрыть G1–G10 и сделать повторный проход entry points + стыков.
2. `registry`: claim/recycle/bootstrap, publication readiness, slot-resident metadata, exit/reuse.
3. `alloc_core` small: magazines, free lists, remote rings, dirty routing, pools и reclamation.
4. `alloc_core` large: cache, deferred frees, realloc/promotion, reserved capacity.
5. `alloc_core` infrastructure: segment/table/directory/bitmap/OS/NUMA/lazy commit contracts.
6. `concurrent`: независимый проход typed concurrent stores.
7. Сквозной проход: feature combinations, GlobalAlloc contract, системные пределы и public documentation.

Каждый раунд: исходный SHA, точный модуль, проверенные входы/выходы, P-список, counterexample и критерий закрытия. Исправление в соседней папке не освобождает от повторной проверки вызывающего модуля. G1 — конкретный пример того, почему «проверять только файлы выбранной папки» недостаточно.

В текущем задании fixes и execution gates не выполнялись. Требующий исполнения критерий закрытия остаётся описанием следующей проверки, а не отметкой о её прохождении.
