# src review, round 10 — Sol-Codex

Время фиксации: 2026-09-30 19:28:03 CEST (Europe/Berlin) / 2026-09-30 17:28:03 UTC. Ветка `review/src-xs-round10-20260930`, исходный immutable base и проверенный HEAD: `40dcce1141634018672c0cbcd07e2cfef7239101`. Ревью выполнено только в `worktrees/src-review-xs-20260930-r10`; исходники не менялись. Предыдущие отчёты, чекпоинты и чужие выводы не открывались и не использовались как доказательства.

## Итог и границы

Подтверждено: **P0 0, P1 0, P2 0, P3 1, P4 3**. P3 здесь означает доказанную по исходникам асимптотику на достижимом production-пути, **не** измеренную задержку или обещанный выигрыш. P4 — несоответствия документированного контракта фактическому коду, без показанной порчи памяти. Нового подтверждённого counterexample для memory safety при соблюдении формальных контрактов `unsafe` API не найдено; это не доказательство отсутствия таких ошибок.

Инвентарь `rg --files src`: **155 файлов** — `alloc_core/` 83, `registry/` 45, `global/` 12, `concurrent/` 13, корневые `lib.rs` и `kani_proofs.rs` 2. Проверены дерево модулей, feature-гейты и публичные входы; глубоко прослежены `GlobalAlloc` → TLS/fallback → `HeapCore` → `AllocCore` для alloc/free/realloc/zeroed/batch; owner lease/recycle/maintenance; `SegmentTable`, route-directory/pin/registration, Small bitmap и Large terminal state; small pool, large cache, trim; opt-in epoch/RCU/sharded/pinning. Для достижимости сопоставлены `Cargo.toml`, README, `docs/INVARIANTS.md`, `docs/ARCHITECTURE.md` и выборочно тесты. Рабочие инструкции `CLAUDE.md` и rust-intel с относящимися к этим ветвям тематическими правилами прочитаны. `async.md` и `security.md` rust-intel не проходились построчно: поиск в `src/` не выявил реальных `async fn`, Tokio, сетевого/SQL/crypto/serde кода; их отсутствие само по себе не является security-аудитом зависимостей. Предписанное rust-intel разделение полного аудита на агентов не применялось: пользователь прямо запретил субагентов. Поэтому это ограниченный однопроходный аудит, а не формальный полный охват всех категорий/каждой строки.

Тесты, сборки, Miri, Loom, бенчмарки и скрипты проекта **не запускались по запрету**. Никаких динамических результатов, релизной сертификации, измеренных ускорений или утверждения о безусловной безопасности ниже нет. В частности, OS-реализации в `crates/` не входили в основной scope; использованы только видимые из `src/` предпосылки и заявленные контракты. Ни host OOM, ни сознательно документированный предел `MAX_SEGMENTS - 1`, ни вызов `unsafe` вне его формального контракта не считались ошибкой.

## Подтверждённые находки

### P3 — После churn обычный alloc повторно обходит исторический high-water сегментов

**Место:** `src/alloc_core/alloc_core/sidecar_drain.rs:173,175`, `src/registry/heap_core/alloc/hot.rs:250,758`, `src/alloc_core/small/alloc_core_small/find_segment.rs:366,369,418,444`, `src/alloc_core/segment/segment_table/segment_table_impl.rs:404,504,667,674,889`.

**Достижимость:** `production` включает `alloc-global`, `alloc-xthread`, `fastbin`, `alloc-segment-directory`. Один долгоживущий thread-heap сначала материализует много Large-сегментов (либо Small-сегментов разных классов), затем освобождает их и продолжает делать большие выделения или Small magazine misses. `SegmentTable::count` увеличивается при новом слоте (`:404`), а `unregister`/`recycle` кладут индекс в free-list, не уменьшая `count`. Поэтому число активных сегментов может стать малым, но `count` остаётся историческим максимумом `H` (до 4096 на heap).

**Цепочка и последствие:** каждый Large `HeapCore::alloc` вызывает `drain_large_sidecar_ingress` до обращения к Large cache (`hot.rs:250`), а тот без dirty/active фильтра выполняет `for index in 0..end`, где `end = table.count()` (`sidecar_drain.rs:173-175`), включая все пустые слоты. Small magazine miss также вызывает этот полный Large-обход (`hot.rs:758`). После отрицательного directory lookup у routed heap `trust_negative = false` (`find_segment.rs:366`), так что Small discovery идёт в ещё один линейный обход `[0, count)` (`:418,444`). Даже при отсутствии ожидающих remote frees и при одном активном сегменте повторяющаяся операция оплачивает **Ω(H)** проверок; серия из `m` операций — Ω(`mH`) проверок. Это точный статический счёт работы, не оценка наносекунд. README прямо помечает текущий sidecar cutover как не прошедший новую performance-acceptance; исторические цифры не использованы как доказательство регрессии.

**Исправление:** сохранить корректность terminal-publication/credit protocol, но добавить owner-visible индекс активных Large routes и/или атомарную сводку dirty sidecar slots/классов, чтобы обычный вызов обходил текущие кандидаты, а полный проход оставался bounded cold recovery/trim. Обязательны проверка гонки «публикация после cut», generation/incarnation при reuse и fallback при переполнении уведомлений; простое удаление скана могло бы потерять frees.

**Невакуозный свидетель:** в `production internals` создать `H=128` одновременно живых Large-блоков, освободить 127, оставить один текущий блок; счётчиком инспекций `base_at`/route-slot доказать, что один следующий Large alloc без pending remote frees делает порядка `H` проверок сейчас, хотя активных кандидатов O(1). После изменения требовать bound от активного/dirty набора, а не от high-water; отдельный отрицательный контроль — приостановленный после pin foreign free, публикующий бит после cut, должен быть найден следующим проходом. Такой контроль отличает настоящее сокращение обхода от опасного отключения reclaim. Тест не выполнялся.

### P4 — Проход NUMA-biased root нарушает записанную предпосылку `Node::offset`

**Место:** `src/alloc_core/platform/node.rs:433,439-443` и `src/alloc_core/platform/numa.rs:156,165,170`; вызов из `src/alloc_core/large/alloc_core_large.rs:516`.

**Достижимость и цепочка:** opt-in `numa-aware`, Large layout с `align > SEGMENT` (например, 16 MiB). `reserve_biased_on_node` резервирует `raw_len = useful + align`, вычисляет и проверяет `root_offset + useful <= raw_len`, затем передаёт `offset` в `Node::offset(origin, offset)` (`numa.rs:156-170`). При `origin` на 4 MiB, но не 16 MiB границе и `metadata` в одну страницу offset до следующего 16 MiB payload равен примерно 12 MiB — больше `SEGMENT` (4 MiB). Между тем документированный контракт и `// SAFETY:` у `Node::offset` утверждают `off <= SEGMENT` и выводят допустимость `base.add(off)` именно из этого (`node.rs:433-442`).

**Последствие:** это **ошибка локального safety-обоснования**, не установленный UB: реальный `numa`-вызывающий путь отдельно проверяет границу своего более длинного OS-reservation, что может делать сам `add` корректным. Но комментарий, которому должен доверять следующий аудитор/рефакторинг, не покрывает достижимый вызов. Исправить контракт `Node::offset` до «`off <= isize::MAX`, результат в одной живой allocation» с per-call extent-доказательством; либо использовать отдельный helper для biased reservation с явно переданной длиной. Отрицательный свидетель — арифметический случай выше (`origin mod 16 MiB = 4 MiB`, `align=16 MiB`, `metadata=page`) даёт `offset > SEGMENT` при сохранении `offset + useful <= raw_len`; закрепить его unit property для расчёта геометрии. Исполнение не проводилось.

### P4 — Rustdoc `SeferAlloc` перечисляет несуществующую production-feature

**Место:** `src/global/sefer_alloc/core.rs:72-73`; фактический список `Cargo.toml:412`.

**Достижимость и цепочка:** читатель публичного `SeferAlloc` rustdoc для `--features production` получает список с `class-aware-dirty`; в Cargo feature-таблице такого feature нет, а `production` включает ровно `alloc-global`, `alloc-xthread`, `alloc-decommit`, `fastbin`, `alloc-segment-directory`, `primordial-lazy-commit`. Следствие — ложная документация о составе поставляемой конфигурации и неверная основа для feature-isolation/профилирования; runtime-дефект этим не доказан. Исправить rustdoc по manifest и держать одно место истины или добавить статическую проверку списка. Невакуозный отрицательный свидетель: сравнение множества имён из rustdoc-перечня с `Cargo.toml [features].production` должно сейчас вернуть лишний `class-aware-dirty`, а после правки — пустую разность. Эта проверка не запускалась.

### P4 — Batch API ошибочно называет пустой вывод признаком OOM

**Место:** `src/registry/heap_core/alloc/batch.rs:34,70,74-75`; внешний wrapper корректнее описывает пустой slice в `src/global/sefer_alloc/batch.rs:46-55`.

**Достижимость и цепочка:** opt-in `batch-api + alloc-global + fastbin`; безопасный вызов `HeapCore::alloc_batch(valid_layout, &mut [])` немедленно возвращает `0` по `want == 0`, без попытки выделения и без OOM. Rustdoc метода обещает «0 only on true OOM». Следствие — потребитель или тест internal/batch поверхности может ошибочно трактовать нормальную пустую партию как отказ памяти; это неточность контракта, не неправильный результат функции. Исправить текст на «0 при пустом выводе или отсутствии успешных выделений», синхронизировав с `SeferAlloc::alloc_batch`. Невакуозный свидетель: отдельный контрактный тест с пустым `out` и валидным `Layout` должен утверждать `0` и отсутствие прироста счётчика OS-reservation failure; такой вызов опровергает именно фразу «only OOM», не тривиально сравнивает метод с самим собой. Не запускался.

## Неподтверждённые возможности (не входят в P-счёт)

- `src/registry/segment_route/directory.rs:298-316,428-466,496-509`: sorted `PointerArray` сдвигает O(routes-in-shard) указателей при register/remove под shard mutex. Это видно из циклов и уже обозначено в `segment_route/mod.rs`; практический выигрыш от иной структуры неизвестен. Измерять на реальной адресной раскладке/частоте route churn, сохраняя pin/unlink lifetime.
- `src/registry/segment_route/small_sidecar.rs:12-14` и `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:11-19`: один Small sidecar хранит 262144 class-byte atomics плюс 4096 `AtomicU64` pending words — статически 288 KiB при обычном 64-bit layout на каждый 4 MiB сегмент. Это точный структурный размер, но RSS/latency эффект и безопасная компрессия class metadata не измерялись.
- `src/concurrent/sharded/sharded_region.rs:31-42,291-307`: одна process-global TLS shard binding используется несколькими `ShardedRegion`, что уже оговорено как ограничение intended topology. Возможная потеря locality/эксклюзивного shard при двух экземплярах — повод для API-дизайна и замера, не установленная ошибка корректности.

## Вердикт

Отчёт годится как статическая очередь действий, прежде всего для устранения доказанной O(high-water) работы и исправления трёх неточных контрактов. Он **не** является release GO, формальным safety-сертификатом или доказательством ускорения. Любая реализация исправлений потребует отдельного in-scope теста с негативным контролем, feature-специфического запуска и, для performance-решения, измерений; в этом раунде это сознательно не выполнялось.
