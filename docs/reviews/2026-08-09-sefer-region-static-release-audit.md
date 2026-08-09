# Статический release-аудит `sefer-region`

**Дата:** 2026-08-09  
**Зафиксированный снимок:** `10503790db195c68e8c703d5e05f9a413fddf7b1` (`main`)  
**Последний коммит, менявший файлы крейта в этом снимке:** `6cb3f6b`  
**Область:** `crates/region`, его публичная документация, тестовые оракулы,
benchmark/package surface и интеграционные обещания корневого крейта.  
**Метод:** только чтение файлов и истории Git по правилам `rust-intel`.

Ни тесты, ни сборка, ни Miri, ни Clippy, ни benchmark, ни сторонние программы
в рамках этого аудита не запускались. Во время работы в рабочем дереве появились
чужие незакоммиченные изменения в `crates/region`, `CHANGELOG.md`, `.cargo/` и
других файлах. Они не являются частью снимка `1050379`, не проверялись как
готовое решение и этим отчётом не изменялись.

## Итог

**Вердикт: HOLD / NO-GO для публикации текущего снимка.**

В собственном production-коде `sefer-region` не найдено подтверждённых UB,
use-after-free, double-free, выхода за границы, гонки данных или ложных ручных
`Send`/`Sync`. Это маленькая безопасная оболочка над `slotmap` и `RwLock`, без
собственного `unsafe`, FFI и сырых указателей. Базовые операции `insert`, `get`,
`remove`, итерация и блокировка по инспекции реализованы прямолинейно.

Однако релизная спецификация сейчас сильнее реальной реализации. Несколько
публичных утверждений либо прямо ложны, либо завязаны на приватные детали
`slotmap 1.1.1`, хотя зависимость разрешает любую `slotmap 1.x`. Есть также два
false-green теста и дефект standalone-упаковки benchmark. Главные блокеры:

1. `sefer-region` всё ещё имеет версию `0.1.0`, которая уже опубликована;
2. обещание, что перенос в свежий `Region` инвалидирует старые handles, ложно;
3. проект объявляет реализованную compaction, которой у `SlotMap` нет;
4. корневой rustdoc обещает вечную stale-handle защиту и retirement, которых нет;
5. проект описывает Region и аллокатор как одно хранилище, хотя они независимы;
6. точная partial-clear семантика при панике `Drop` не принадлежит этому крейту;
7. критические тесты `reserve` и partial `clear` не доказывают заявленное;
8. опубликованный benchmark не самодостаточен вне workspace.

После устранения этих пунктов архитектура самого крейта выглядит пригодной для
патч-релиза, если сохраняется нынешняя модель cross-region handles. Если нужна
строгая привязка handle к конкретному Region, это уже отдельный дизайн и,
вероятнее всего, версия `0.2.0`.

## Что исправляет этот отчёт в предыдущем ревью

Коммит `1050379` добавил
`docs/reviews/2026-08-09-sefer-region-release-prep-review.md`. В нём есть полезная
инвентаризация, но его общий вывод `GO-WITH-FIXES` и несколько заключений требуют
коррекции:

- фраза «every one of I1-I5 is accurate as written» неверна для I5: safe
  `mem::forget`, а также передача владения через `remove`, опровергают абсолютное
  «never leaked / dropped on remove»;
- совет пересобрать свежий Region «which invalidates every Handle» ложен;
- отсутствие packaged `bench-iters.txt` названо безвредным, хотя standalone
  benchmark пытается калиброваться и писать вне каталога опубликованного крейта;
- заголовок о расширении panic surface примерно на четыре десятичных порядка не
  следует из приведённого сравнения: `usize::MAX / 16` отличается от
  `usize::MAX` лишь в 16 раз, около 1.2 порядка;
- «no new irreversible risk» слишком сильный вывод при плавающей `slotmap = "1"`,
  точном layout-обещании и неразрешённой идентичности handles;
- итог «готовность» не учитывает, что `0.1.0` уже опубликована, а manifest всё ещё
  содержит `0.1.0`.

Это не обесценивает предыдущую работу: она нашла реальный false-green overflow
test и полезные package/metadata gaps. Но релизный verdict следует заменить на
HOLD до закрытия перечисленных ниже P0/P1.

## P0 — блокеры релиза

### F1. Нельзя повторно опубликовать текущую версию `0.1.0`

**Где:** `crates/region/Cargo.toml:2-3`; история публикации зафиксирована в
`docs/reviews/2026-08-06-sefer-region-publish-readiness-review.md`.

Реальный `sefer-region 0.1.0` уже существует в crates.io, тогда как committed
manifest снимка всё ещё равен `0.1.0`. Тега `sefer-region-v0.1.0` в локальной
истории нет: первая публикация прошла вне нынешнего tag workflow.

**Действие:** после выбора API-пути поднять версию до `0.1.1`, обновить lockfile,
release notes и выпускать тег `sefer-region-v0.1.1`. Корневая зависимость
`version = "0.1"` уже принимает `0.1.1`. Если принимается breaking-дизайн
domain-aware handles, целевой номер должен быть `0.2.0`, а не патч.

### F2. Свежий Region не гарантирует инвалидность старых handles

**Где:** `crates/region/src/region.rs:109-115`,
`crates/region/README.md:175-181`, контрпример уже существует в
`crates/region/tests/smoke.rs:98-130`.

`Handle<T>` содержит только `DefaultKey`; идентификатора экземпляра Region в нём
нет. Два свежих `Region<T>` обычно выдают одинаковый первый ключ. Поэтому старый
handle может успешно разрешить или удалить уже другой объект в заново построенном
Region. Это особенно опасно именно рядом с советом «пересоберите Region для
компактации»: пользователь может следовать документации и получить тихую
логическую подмену.

Равенство и `Hash` также делегируются только raw key. Handles от двух разных
`Region<T>` могут считаться равными и сливаться в одном `HashMap`/`HashSet`.

**Патч-путь 0.1.1:** удалить обещание инвалидности; прямо сказать, что старые
handles запрещено использовать с новым Region, но библиотека не может это
обнаружить. Добавить предупреждение непосредственно в rustdoc `Handle<T>` и
`Region<T>`, включая область определения `Eq`/`Hash`.

**Строгий путь 0.2.0:** добавить domain/region identity в handle либо применить
generative branding. Это увеличит handle и изменит API/ABI, зато wrong-region
resolution станет проверяемым. До решения этого вопроса не стоит добавлять API,
которые усиливают текущую identity-модель (`Ord`, сериализацию, массовое смешение
handles, «компактацию» с переносом).

### F3. I6 объявляет compaction, которой нет

**Где:** `docs/INVARIANTS.md:28-31`, `tests/compaction.rs:1-14,85-142`,
`docs/PLAN.md`, `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, связанные места в
корневом README.

Реальный `Region<T>` использует `slotmap::SlotMap`: после удалений остаются
tombstone holes, итерация идёт по high-water slot array, а shrink/compact API
нет. Тест с названием `compaction` проверяет лишь:

- что surviving handles продолжают разрешаться;
- что `len()` совпадает с числом элементов итератора;
- что freed slots/capacity повторно используются.

Ни одно из этих свойств не доказывает физическую плотность или compaction.

**Действие:** удалить или переопределить I6 как «повторное использование
освобождённых слотов и ограничение роста историческим high-water mark»;
переименовать тест и его комментарии; вычистить dense/compact утверждения во
всей актуальной документации. Исторические планы следует явно пометить как
устаревшие, если их нельзя удалить.

### F4. Корневой rustdoc обещает вечную защиту от stale handle

**Где:** `src/lib.rs:6-7,16-17`, `tests/region_invariants.rs:5-7`; фактическая
семантика правильно описана в `crates/region/src/region.rs:44-66`.

Корневой API говорит, что stale handle «never resolves» и что slotmap применяет
version-saturation retirement. Фактически 32-битное поколение оборачивается
примерно после `2^31` циклов reuse одного hot slot, а retirement в крейте нет.
После wrap очень старый handle может разрешить или удалить другой объект. Это
логическая ABA-проблема, не memory unsafety.

**Действие:** перенести конечную гарантию и wrap-caveat в корневой rustdoc;
убрать retirement и абсолютное `never`; исправить комментарии Miri/invariant
tests. Термин I3 лучше назвать bounded stale-handle detection, а не безусловным
«no ABA».

### F5. Region и аллокатор ошибочно описаны как одно хранилище

**Где:** корневой `Cargo.toml:9`, `README.md:332-336`,
`docs/ARCHITECTURE.md:45-70`, `docs/ALLOC_PLAN.md:76-115,368-378`,
`src/global/sefer_alloc.rs:189-191`.

Документация использует модель «два лица над одним verified segment substrate».
Реальная архитектура другая:

- `Region<T>` — независимый контейнер над сторонним `slotmap`;
- `SeferAlloc` — отдельный OS-backed segment allocator;
- корневой crate только реэкспортирует API Region.

Ошибка может повлиять на решения пользователей о владении памятью, RSS,
локальности, настройке allocator и области действия его гарантий.

**Действие:** заменить «same substrate / same governed memory / two faces» на
«два независимых API в одном package» и явно назвать backing каждого. Не
переносить allocator invariants M1… на Region и наоборот.

## P1 — исправить до патч-релиза

### F6. Точный partial-clear контракт принадлежит приватной реализации slotmap

**Где:** `crates/region/src/region.rs:216-221`,
`crates/region/src/sync_region.rs:175-182`,
`crates/region/tests/clear_partial_under_panic.rs`; зависимость в
`crates/region/Cargo.toml:20`.

Документация обещает, что при панике `T::Drop` уже посещённые элементы удалены,
а более поздние остаются live. История коммита `0931d35` прямо выводит это из
порядка действий `slotmap 1.1.1`. Но `slotmap = "1"` допускает будущую 1.x,
которая может сначала удалить все записи либо иметь другой unwind cleanup,
сохранив при этом собственный публичный контракт.

Кроме того, слово «dropped» неточно: если деструктор запаниковал, доказано, что
он был вызван, но не что его cleanup завершился.

**Предпочтительный патч:** обещать только валидность контейнера и допустимую
частичность clear без точного survivor set. Тест считать наблюдением текущей
зависимости, а не вечным API oracle. Если точный survivor contract нужен, цикл
remove/drop должен контролироваться самим `sefer-region` через стабильный
примитив. Exact pin зависимости — лишь временная страховка.

### F7. `reserve` overflow test не тестирует wrapper guard

**Где:** guard в `crates/region/src/region.rs:134-139`, тест в
`crates/region/tests/coverage_gaps.rs:480-494`.

На пустом Region вызов `reserve(usize::MAX / 2)` не переполняет
`len() + additional`: `len == 0`. Наблюдаемая паника может прийти позднее от
RawVec/allocator capacity, поэтому удаление wrapper guard не делает тест красным.

**Действие:** создать хотя бы один live element и вызвать
`reserve(usize::MAX)`, проверяя именно собственное сообщение/ветвь. Без guard в
release arithmetic может wrap-нуться в ноль и стать no-op; с guard должна
возникнуть детерминированная wrapper panic до allocation.

### F8. Partial-clear panic test выбрасывает главный результат

**Где:** `crates/region/tests/clear_partial_under_panic.rs:180-193`.

Внутренний `catch_unwind` вычисляется, но его результат не возвращается и не
проверяется. Родитель проверяет только успешный `join`. Если `clear` внезапно
перестанет паниковать и полностью очистит Region, последующие счётчики всё ещё
могут пройти.

**Действие:** вернуть результат unwind из потока или проверить `is_err()` внутри
него; сохранить независимые oracles целостности и exactly-once ownership.

### F9. Exact 8-byte layout обещан сильнее, чем гарантирует crate

**Где:** `crates/region/src/handle.rs:13-19`,
`crates/region/tests/handle_static_asserts.rs:9-15,65-86`,
`crates/region/Cargo.toml:20`.

`#[repr(transparent)]` гарантирует совпадение layout `Handle<T>` с
`DefaultKey`; он не замораживает сам `DefaultKey` на 8 bytes и его niche.
`DefaultKey` оборачивает `KeyData`, чьи поля приватны и имеют Rust layout.
Downstream разрешает любую совместимую slotmap 1.x, а не workspace lock 1.1.1.

**Действие до первой публикации этого нового обещания:** документировать только
layout-equivalence с `DefaultKey`; 8 bytes назвать наблюдаемым свойством текущей
resolved версии/тестовым tripwire. Relative size/alignment assertions честнее
абсолютного ABI-контракта. Если действительно нужен стабильный 64-bit ABI,
нужна собственная репрезентация и явный контракт сериализации/FFI, а не только
dependency pin.

### F10. Публичные детали generation/LIFO также плавают вместе с slotmap 1.x

**Где:** `crates/region/src/region.rs:32-66`.

Odd/even encoding, `version | 1`, `wrapping_add` и LIFO freelist — детали
реализации проверенной версии 1.1.1, не обязательно стабильный API всей 1.x.
Полезно сохранять публично подтверждённые пределы stale-key semantics, но
механизм и измеренные «~12 секунд» нужно явно маркировать snapshot-описанием
slotmap 1.1.1, либо закреплять и повторно аудировать точный диапазон зависимости.

### F11. `SyncRegion` обещает отсутствие payload-эффекта при панике Clone

**Где:** `crates/region/src/sync_region.rs:25-28`.

Безопасный `Clone::clone(&self)` может изменить `Cell`, atomic или внутренний
Mutex и затем запаниковать. Read lock действительно не poison-ится, но payload
уже мог измениться.

**Действие:** гарантировать только отсутствие структурного изменения
Region/slotmap. Явно передать ответственность за interior side effects типу
`T`. Добавить точечный oracle с interior mutation + panic.

### F12. I5 «never leaked / dropped on remove» невозможно гарантировать

**Где:** `crates/region/src/lib.rs:40-41`,
`crates/region/src/region.rs:40-42`, `crates/region/README.md:75-76`.

`remove` не уничтожает значение, а передаёт владение вызывающему коду. Safe
Rust разрешает `mem::forget(region)` и `mem::forget(removed_value)`. Поэтому
абсолютное «never leaked» ложно, хотя double-drop в реализации не найден.

**Действие:** сформулировать ownership contract:

- у stored value ровно один владелец;
- успешный `remove` ровно один раз передаёт владение, не вызывая Drop;
- значения, всё ещё принадлежащие нормально уничтоженному Region, drop-аются;
- crate не дублирует и внутренне не забывает значения;
- `mem::forget` вызывающего кода вне гарантии.

### F13. Capacity validation не отражает реальный домен SlotMap

**Где:** `Region::with_capacity`, `Region::reserve`, `Region::insert`.

Wrapper проверяет `usize` overflow, но `SlotMap` допускает максимум
`2^32 - 2` live entries. На 64-bit огромный, логически невозможный target
проходит проверку и может инициировать бессмысленную гигантскую allocation до
более позднего отказа. `# Panics` описывает лишь часть allocation/capacity
ошибок.

**Действие:** валидировать публичный domain до allocation; уточнить panic docs.
Желательно предоставить `try_reserve`, поскольку upstream такой путь имеет.
Это не hot-path ускорение, но большое улучшение отказоустойчивости.

### F14. Standalone benchmark не самодостаточен

**Где:** `crates/region/benches/region_bench.rs:25`, workspace-root
`bench-iters.txt`, поведение `bench-scale-tool 0.1.0`.

Published member включает bench, но не включает workspace-root
`bench-iters.txt`. В standalone package `bench-scale-tool` отступает к пути
`<crate>/../../bench-iters.txt`; при отсутствии pinned keys он калибрует все
workloads и пытается записать файл за пределами package. На read-only registry
source это может завершиться ошибкой, а на writable — загрязнить общий каталог.
Локальный package verify внутри исходного workspace способен скрыть дефект.

**Действие:** хранить все 16 entries рядом с member crate и передавать явный
путь либо исправить fallback tool. Проверять распакованный package в
изолированном временном дереве без ancestor workspace.

### F15. Публикация не подкреплена постоянными package gates

**Где:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`.

Есть default debug test, release test и bare-metal no-default build, но нет
постоянных явных gates для:

- `sefer-region` Clippy по всем targets/features;
- member rustdoc с `-D warnings`;
- host tests без default features;
- прямого MSRV member graph/test-no-run;
- изолированного packaged benchmark smoke;
- semver comparison с опубликованным 0.1.0.

Release workflow также намеренно пропускает member CHANGELOG, а отдельного
`crates/region/CHANGELOG.md` нет. Ручные успешные проверки из прежних отчётов —
полезный snapshot, но не защита будущих релизов.

## P2 — качество тестов, документации и сопровождения

### F16. Конкурентный тест может фактически выполниться последовательно

`coverage_gaps.rs:621-675` не имеет стартового Barrier/yield, а 200 коротких
операций одного потока могут закончиться до реальной встречи потоков. Результат
`contains` отбрасывается. Handles создаются и удаляются внутри одного потока,
поэтому same-entry races не проверяются.

Добавить Barrier и детерминированный shared-handle сценарий: два concurrent
`remove(h)` дают ровно одного победителя; `get_cloned(h)` racing remove видит
только старое значение или `None`. Nondeterministic scheduling не должен быть
частью oracle.

### F17. Capacity tests проверяют не то, что обещают комментарии

`coverage_gaps.rs:386-437` фиксирует capacity уже после первого insert либо
проверяет только `len`. Реализация, которая realloc-ирует на каждом insert,
может остаться зелёной. Нужно снимать capacity сразу после
`with_capacity`/`reserve` и проверять её неизменность на обещанном числе inserts;
то же относится к SyncRegion-вариантам.

### F18. Panic-message helper меняет process-global hook

`coverage_gaps.rs:512-557` сериализует только собственных callers, но другие
intentional-panic tests того же процесса могут одновременно заменить hook.
Возможны flakes, подавление чужого вывода и ложный capture. Предпочтительнее
downcast panic payload или изоляция всего panic suite.

### F19. Reentrancy и fairness стоит документировать локально

Типовой rustdoc предупреждает, что `clear` вызывает `Drop` под write lock, а
`get_cloned` вызывает `Clone` под read lock, но страницы самих методов не ведут
пользователя к предупреждению. Добавить локальный `# Reentrancy`.

`std::sync::RwLock` не обещает portable fairness/starvation freedom. Формулировка
«correct under any interleaving» относится к safety/serialization, но не к
bounded writer latency. Уже удерживаемые readers не блокируют друг друга;
новый reader может задержаться за ожидающим writer в зависимости от ОС.

После первой write panic poison flag навсегда остаётся выставленным, и каждый
последующий вызов идёт через recovery branch. Если политика действительно
безусловно принимает container state, можно очищать poison после первой
успешной recovery; это только небольшая post-panic оптимизация.

### F20. `remove` корректно возвращает T после снятия lock, но контракт не защищён

Текущий tail expression в `SyncRegion::remove` приводит к освобождению временного
write guard до того, как caller уничтожит возвращённый `T`. Поэтому reentrant
Drop возвращённого значения не deadlock-ится на этом guard. Специального
регрессионного теста с reentrant destructor нет. Его лучше добавить в
timeout-safe subprocess/модель, чтобы возможная регрессия не повесила весь CI.

### F21. Формулировка «for atomicity» обещает больше, чем lock даёт

README рекомендует держать write guard «for atomicity», но паника внутри
многооперационной секции не откатывает уже сделанные изменения. Корректные
термины: interleaving isolation / один сериализованный critical section;
all-or-nothing и rollback не предоставляются.

### F22. Performance-таблицы требуют provenance

Member README приводит разные значения одинаково описанного median-of-3 insert
benchmark и отдельно ссылается на Criterion-era таблицу с другой стоимостью
churn/wrapper overhead. Это может быть нормальной разницей harness/session, но
без SHA, hardware, config и raw artifact читатель не может отличить её от
регрессии. Каждая таблица должна иметь snapshot provenance.

### F23. «Audited slotmap» не подтверждено воспроизводимой аттестацией

Формулировка повторяется в package description, README и crate rustdoc.
`cargo-deny` проверяет advisories, licenses и sources текущего lockfile, но не
является code audit. В репозитории не найдено cargo-vet/manual attestation с
версией, scope и результатом.

Либо заменить утверждение на проверяемое «mature third-party dependency; unsafe
находится upstream», либо добавить version-scoped audit record и policy
повторного аудита при update. Это особенно важно при широком `slotmap = "1"`.

### F24. `forbid(unsafe_code)` защищает library target, не весь package

Сейчас инспекцией не найдено unsafe также в tests, bench и example. Но
`#![forbid(unsafe_code)]` в `src/lib.rs` не запрещает будущий unsafe в отдельных
targets. Для package-wide обещания нужен `[lints.rust] unsafe_code = "forbid"`
с inheritance или атрибут в каждом target.

### F25. `captrack` слишком тяжёл для одного ignored probe

Dev-dependency нужен одному диагностическому probe, но расширяет build и
supply-chain несколькими crates/proc macro и имеет lifecycle/autodump side
effects. Обычные runtime consumers его не получают. Лучше вынести probe в
непубликуемый tools crate либо использовать узкую registry-only feature.
Repository-local `.cargo` mitigation сама по себе не делает crates.io member
самодостаточным.

### F26. Малые API/documentation улучшения

- добавить `#[must_use]` на `insert`: проигнорированный handle оставляет значение
  недоступным до `clear`/drop;
- не спешить с `FromIterator`/`Extend`, потому что они естественно теряют handles;
- конкретные `std::sync::RwLock*Guard` уже были опубликованы в 0.1.0; менять их
  в 0.1.x нельзя;
- явно документировать conditional auto-traits `SyncRegion<T>`;
- README examples должны компилироваться как tests/doctests;
- принять и зафиксировать policy для `#![deny(missing_docs)]`, member release
  notes и docs.rs features.

## Memory-safety и concurrency inventory

По зафиксированному снимку:

- production `unsafe` blocks/functions/impls: **0**;
- raw pointers, `NonNull`, `from_raw_parts`, `transmute`, pointer-int casts: **0**;
- FFI/`extern "C"`/`no_mangle`/`repr(C)`: **0**;
- ручные `Send`/`Sync`: **0**;
- собственные `Drop`, `ManuallyDrop`, `mem::forget` в production: **0**;
- async runtime, channels, atomics, multi-lock ordering: **нет**;
- один `RwLock`, поэтому ABBA-класса нет;
- `Handle<T>` не владеет `T`, не выдаёт ссылок и не является pointer wrapper;
  его unconditional auto-`Send + Sync` при нынешнем API sound;
- `SyncRegion<T>` получает auto-traits от `RwLock<Region<T>>`, компилятор
  сохраняет необходимые bounds на `T`;
- `contains`, `len`, `is_empty` — snapshot operations; check-then-act требует
  одного удерживаемого guard, что уже в основном документировано;
- `clear` и `get_cloned` исполняют пользовательский `Drop`/`Clone` под lock;
  reentrancy может deadlock-нуть, но это liveness/API hazard, не data race;
- upstream `slotmap` содержит unsafe, которому wrapper доверяет; собственных
  признаков UAF, double-drop, OOB или corruption не обнаружено.

Иными словами: **memory-safety blocker не найден**, но это не оправдывает ложные
семантические гарантии. Для safe API wrong-object resolution после cross-region
ошибки или generation wrap остаётся серьёзной логической проблемой, даже если
Rust memory safety не нарушается.

## Что можно ускорить

### 1. Не микротюнинг wrapper, а отдельный dense storage для holey iteration

Нынешний `Region<T>` — очень тонкая оболочка. Для lookup/churn добавлять
`inline(always)`, кэширование или собственный unsafe fast path нецелесообразно:
основная стоимость и алгоритм находятся в `SlotMap`.

Сильный оставшийся алгоритмический рычаг — workload с большим историческим
high-water mark и малым live set. Собственные опубликованные числа проекта
показывают, что sweep 1000 live values после 90% holes занимает около 11.482 µs
против 1.319 µs без holes — приблизительно 8.7× cliff. Это не доказательство,
что новый тип автоматически даст 8.7×, а верхняя граница возможности.

Практичный дизайн — отдельный `DenseRegion<T>` над `DenseSlotMap`, а не смена
backing существующего Region:

- потенциально O(live) dense iteration вместо O(high-water slots);
- цена по текущим docs: lookup примерно на 16% хуже, churn около 2.8× хуже;
- поэтому `SlotMap` должен остаться default для lookup/churn workload;
- дизайн следует делать только после решения cross-region identity, иначе новая
  «компактация» усилит опасность старых handles.

### 2. Batch reads под одним guard — уже главный ускоритель SyncRegion

В README one-shot read при 8 readers указан около 1221 ns/op, а 64 reads под
одним guard — около 38.7 ns/op, то есть приблизительно 31.6×. Это уже доступно
через `read()` и существенно сильнее любого микротюнинга метода
`get_cloned`/`contains`.

Если adoption важен, полезны удобные closure/batch API, которые направляют
пользователя к одному lock acquisition. Не стоит менять опубликованные concrete
guard return types в 0.1.x.

### 3. Sharding — отдельный concurrent type, не патч существующего

`SyncRegion` глобально сериализует writes и заставляет one-shot reads бороться
за одну cache line. `ShardedSyncRegion` может сильно помочь независимым keys,
но shard/domain должен кодироваться в handle и измеряться на реальных mixed
read/write workloads. Это новый тип и вероятный 0.2 дизайн, а не безопасная
локальная оптимизация.

### 4. Улучшить failure path, а не обещать hot-path выигрыш

Ранняя проверка `2^32 - 2`, `try_reserve` и честные capacity errors устранят
катастрофическую лишнюю allocation/abort работу на ошибочном вводе. Это важное
улучшение robustness, но не steady-state throughput.

## Рекомендуемый порядок работ

### Этап A — восстановить правдивую спецификацию

1. Исправить F2-F5 и F12 во всех публичных документах, а не только в member
   README.
2. Переопределить I6 и переименовать false compaction oracle.
3. Ослабить partial-clear promise либо начать владеть его реализацией.
4. Исправить Clone/interior-mutation, transaction atomicity и Drop wording.
5. Исправить прежний release-prep report или добавить в него явный superseded
   notice со ссылкой на этот аудит.

### Этап B — закрыть false-green evidence

1. Сделать reserve overflow test контрфактуальным.
2. Обязательно проверить, что partial clear действительно паниковал.
3. Усилить capacity и concurrency tests.
4. Убрать process-global panic-hook race.
5. Добавить reentrant remove-Drop и interior-mutation Clone regressions.

### Этап C — принять API-решение

1. Если cross-region aliasing принимается — выпускать 0.1.1 с очень явным
   contract и без обещания инвалидности.
2. Если оно неприемлемо — спроектировать domain-aware handles и выпускать 0.2.0.
3. До решения не обещать стабильный 8-byte ABI и не строить compaction API.

### Этап D — сделать artifact воспроизводимым

1. Исправить packaged benchmark и crate-local pinned iterations.
2. Добавить member Clippy/rustdoc/MSRV/no-default/package/semver gates.
3. Свести performance tables к provenance: SHA, harness, hardware, config,
   raw artifact.
4. Определить policy для slotmap audit/update и captrack dev tooling.
5. Добавить release notes/CHANGELOG для member.

### Этап E — только затем выпуск

1. Поднять версию, обновить lockfile и release notes.
2. На чистом точном commit выполнить полный release matrix и isolated package
   verification.
3. Убедиться, что CI зелёный именно для release SHA.
4. Создать и отправить единственный корректный tag.

## Release checklist

- [ ] manifest больше не равен уже опубликованной версии;
- [ ] old-handle-after-rebuild contract правдив;
- [ ] I6/compaction docs и test oracle исправлены;
- [ ] root stale-handle/retirement rustdoc исправлен;
- [ ] «same segment substrate» удалено;
- [ ] exact partial-clear survivor promise либо ослаблено, либо принадлежит коду;
- [ ] I5 сформулировано как ownership, а не неограниченное отсутствие leaks;
- [ ] exact Handle ABI/layout promise исправлено;
- [ ] reserve и partial-clear tests контрфактуальны;
- [ ] standalone packaged benchmark самодостаточен;
- [ ] member CI/release/semver gates постоянны;
- [ ] dirty concurrent fixes отдельно reviewed и committed;
- [ ] release выполняется с чистого exact SHA.

## Заключение

`sefer-region` не выглядит memory-unsafe и не требует срочной переписи runtime
ядра. Его главный релизный риск сейчас — **semantic overclaim**: документация и
тестовые названия обещают compaction, глобальную идентичность handles, вечную
ABA-защиту, единый allocator substrate и точный unwind survivor set, которых
реальная абстракция не предоставляет.

Это хорошая ситуация для исправления перед релизом: большинство блокеров можно
закрыть точными docs/tests/package changes без усложнения hot path. Единственная
большая развилка — считать ли cross-region handle aliasing допустимым контрактом
0.1.x или исправлять его новой identity-модель в 0.2. После явного решения этой
развилки, правки спецификации и укрепления release gates крейт можно довести до
честного `GO`.
