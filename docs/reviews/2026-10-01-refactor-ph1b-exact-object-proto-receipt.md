# Ph1b: opt-in прототип «точный System-объект + внеобъектный descriptor» (receipt)

Только наблюдённое. Это **не production backend**: feature `exact-object-proto` не входит в `production`/default. База: `94e1a317f0334a2110814ec22bacfbd436210dee`, worktree `refactor-ph1b`, изменения не закоммичены. Entry point под тестом: реальный `#[global_allocator] SeferAlloc` (`GlobalAlloc::{alloc, alloc_zeroed, realloc, dealloc}`), настоящий `Box<[u8; N]>`.

## 1. Дизайн

- Граница: `ExactNarrow::is_narrow(layout)` = `1 <= size <= 16 && align <= 8` (`MAX_SIZE = 16`, `MAX_ALIGN = 8`, `src/global/exact_object/narrow.rs`). Остальные размеры идут старым путём без изменений.
- `alloc`/`alloc_zeroed`: `System.alloc(layout)` / `System.alloc_zeroed(layout)` с исходным `Layout`, затем регистрация `(addr, size, align, generation)` во внеобъектной таблице. Регистрация не удалась (OOM таблицы) -> объект тут же освобождается, возвращается null.
- Таблица (`exact_shard.rs`, `exact_table.rs`): 64 шарда, шард = младшие 6 бит хэша точного адреса; в шарде spinlock (`AtomicBool`) + open-addressed массив слотов (`#[repr(C)] {addr, size, align, generation}` = 32 байта) в **System-памяти** (`alloc_zeroed`/`dealloc`, рост x2 или rehash без tombstone, фаллибельный, под локом). Ни `Vec`/`Box`/`HashMap` через глобальный аллокатор. Пустой слот `0`, tombstone `usize::MAX`. `live_hint` (AtomicUsize) даёт lock-free ответ «шард пуст»: корректно, т.к. освобождаемый адрес зарегистрирован до free, а happens-before даёт передача pointer'а вызывающим кодом.
- `generation`: глобальный счётчик `NEXT_GENERATION`, каждая регистрация получает новое ненулевое значение (incarnation).
- Дубликат живого дескриптора на том же адресе = протокольное нарушение: `fatal` (запись статического сообщения в stderr без аллокации + `abort`), вне лока.
- `dealloc` (`ExactNarrow::try_dealloc`, вызывается ДО маршрутизации по куче, для ВСЕХ pointer'ов): 1) `take(addr)` под локом шарда: найти + снять descriptor (unlink); нет дескриптора -> `false`, старый путь; 2) сверка `size/align` с дескриптором, несовпадение -> `fatal`+abort; 3) только после этого `System.dealloc(ptr, layout)` по исходному pointer'у и исходному Layout; 4) только под `cfg(all(miri, internals, bench-internals))`: `TerminalPublicationGate::pause_after_publication(addr)` ПОСЛЕ unlink и физического free (кадр `Box::drop` -> `GlobalAlloc::dealloc` жив, accounting уже сделан).
- `realloc`: если старый или новый класс узкий -> всегда move (alloc нового через `self.alloc`, copy `min(old,new)`, `self.dealloc(old)`); in-place не обещается. Обычные размеры -> старый путь.
- Инвариант порядка: unlink -> free -> (тестовая пауза). Адрес не может быть переиспользован `System` пока его дескриптор жив, поэтому живой дубликат при корректном порядке невозможен; нарушение порядка (M2) ловится дубликатом.
- Файлы (one file - one export, `mod.rs` только реэкспорты): `src/global/exact_object/{mod,exact_fatal,exact_shard,exact_table,insert_outcome,narrow}.rs`. Подключение: `src/global/mod.rs` (`#[cfg(feature="exact-object-proto")] #[doc(hidden)] pub mod exact_object;`), хуки в `src/global/sefer_alloc/global_alloc.rs`, feature + `[[test]]` в `Cargo.toml`, два bullet'а seam'ов в заголовке `src/lib.rs`.
- Test-хуки: `ExactNarrow::dbg_counts()`, `dbg_generation_of(addr: usize)` (безопасные наблюдатели по числу, без raw pointer, `bench-internals`); `dbg_set_early_free(bool)` (только `cfg(miri, internals, bench-internals)`, переключатель мутанта M2).

## 2. Команды, toolchain

- Toolchain: `rustc 1.97.0 (2d8144b78 2026-07-07)` (native, stable, win-msvc); `rustc 1.99.0-nightly (3659db0d3 2026-07-05)`, `miri 0.1.0 (3659db0d3e 2026-07-05)`.
- Native: `cargo test --features "exact-object-proto internals bench-internals" --test r11_exact_object_proto -- <selector>`.
- Miri SB: `MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation" cargo +nightly miri test --features "exact-object-proto internals bench-internals" -j 1 --test r11_exact_object_proto -- <selector>`; TB: то же + `-Zmiri-tree-borrows`. Для `reissue` и `m2` добавлено `-Zmiri-address-reuse-rate=1.0 -Zmiri-address-reuse-cross-thread-rate=1.0` (иначе Miri переиспользует VA вероятностно; для остальных селекторов флаги не используются). Скрипт: `D:/dev/rust/sefer-alloc/target/wt/logs/ph1b/run.sh`.
- Сырые логи (вне репо): `D:/dev/rust/sefer-alloc/target/wt/logs/ph1b/` (`sb-*.log`, `tb-*.log`, `native-*.log`, `baseline-*-paused.log`, `mut-*.log`; прежние прогоны в `first-run/`, `run2/`, `run3/`).
- Один сценарий на процесс, harness=false (как `miri_global_box_acceptance`).

## 3. Селекторы

- `narrow`: `Box<[u8;N]>`, N = 1..7 и 16, 2 раунда; alloc на main, drop на другом потоке; проверяется что адрес маршрутизирован exact (descriptor есть) и снят сразу после drop; `Box<[u8;17]>` не exact (drop на другом потоке).
- `paused`: producer приостановлен внутри `Box::drop`/`dealloc` после unlink+free; owner проверяет, что descriptor снят и счётчик dealloc вырос на 1, делает `trim_current_thread`, выдаёт новый объект (регистрируется), RESUME, join; финальный marker после join.
- `m1` (W-1): при паузе owner пишет в root pointer освобождённого объекта. `m2`: переключатель «физический free ДО unlink» + owner при паузе выделяет 64 узких объекта (повторное использование VA). `m3`: `GlobalAlloc::dealloc` с неверным Layout (size 2 вместо 1).
- `zero` (`alloc_zeroed` размеров 1/3/8/16 после грязного free + `vec![0u8;5]`), `release` (32 Box по 5 байт, часть drop на другом потоке: дельта `allocs == deallocs`), `reissue` (освободить пачку, выдать пачку: каждый повторно использованный VA получает новый generation и отвергает старый; требуется >= 1 повтор).
- `legacy17` (базовая линия, не positive): `Box<[u8;17]>` (не exact) dropped на потоке-владельце.

## 4. Матрица (финальные прогоны на итоговых файлах, sha ниже)

N/A native: gate паузы и мутанты `m1`/`m2` существуют только под `cfg(miri)`.

| селектор | native | Miri SB | Miri TB |
|---|---|---|---|
| narrow | OK `COMPLETE narrow` | OK `COMPLETE narrow` | OK `COMPLETE narrow` |
| paused | N/A | OK `COMPLETE paused` | OK `COMPLETE paused` |
| zero | OK `COMPLETE zero` | OK `COMPLETE zero` | OK `COMPLETE zero` |
| release | OK `allocs=32 deallocs=32` | OK `allocs=32 deallocs=32` | OK `allocs=32 deallocs=32` |
| reissue | OK `same_va_reissues=275 of 512` (10 повторов: 274..393 из 512) | OK `same_va_reissues=62 of 128` | OK `same_va_reissues=62 of 128` |
| m1 (ожидается красный) | N/A | КРАСНЫЙ: `Undefined Behavior: memory access failed: alloc137882 has been freed, so this pointer is dangling` (`root.write(0x99)`; alloc `narrow.rs:44`, free `narrow.rs:84`) | КРАСНЫЙ: та же строка |
| m2 (ожидается красный) | N/A | КРАСНЫЙ: `exact-object: duplicate live descriptor for address` + `error: abnormal termination: the program aborted execution` | КРАСНЫЙ: та же пара строк |
| m3 (ожидается красный) | КРАСНЫЙ: `exact-object: dealloc layout differs from descriptor`, abort (exit code `0xc0000409`) | КРАСНЫЙ: та же строка + `abnormal termination: the program aborted execution` | КРАСНЫЙ: та же пара строк |
| legacy17 (базовая линия, ожидается красный под Miri) | OK `COMPLETE legacy17` | КРАСНЫЙ: `Undefined Behavior: not granting access to tag <2735> because that would remove [Unique for <402747>] which is weakly protected` (`Node::write_next`, `node.rs:90`) | КРАСНЫЙ: `write access through <2659> (root of the allocation) at alloc1421[0x3ff2b0] is forbidden`; защищённый тег Reserved -> Disabled |

Итого: positive `narrow/paused/zero/release/reissue` 5/5 зелёные в SB и 5/5 в TB (10/10 ячеек Miri), native `narrow/zero/release/reissue` 4/4 (4 из 4 применимых). Мутанты `m1`/`m2`/`m3` красные 3/3 в SB и 3/3 в TB по ожидаемым причинам.

Контрольный прогон без feature: прежний `tests/miri_global_box_acceptance.rs paused` (`--features "alloc-global internals bench-internals"`) красный в обеих моделях: SB `not granting access to tag <1479> because that would remove [Unique for <493735>] which is weakly protected`, TB `write access through <1414> (root of the allocation) at alloc815[0x403a20] is forbidden` (`baseline-{sb,tb}-paused.log`).

## 5. Контрфактуалы (мутация -> красный), по сырым логам `mut-*.log`

| witness | мутация исходника (временная, откат проверен sha) | исход |
|---|---|---|
| narrow / reissue / release | A: unlink не снимает слот (tombstone не пишется) | native: `exact-object: duplicate live descriptor for address` (abort) во всех трёх |
| paused (Miri SB) | A (тот же) | `panicked ... descriptor not unlinked` (`mut-F-sb-paused.log`) |
| reissue | C: `NEXT_GENERATION.fetch_add(0)` | native: `assertion left != right failed: same-VA reissue kept the old generation` (`mut-C2-native-reissue.log`) |
| release | D: пропуск `DEALLOCS.fetch_add` | native: `assertion left == right failed: exact alloc/dealloc not balanced` |
| zero (Miri SB) | E: `alloc_zeroed` игнорирует `zeroed` | `Undefined Behavior: reading memory at alloc138224[0x0..0x1], but memory is uninitialized` |
| narrow/paused (положительный путь вообще) | прежний backend (feature выключена) | `miri_global_box_acceptance paused` красный SB и TB (см. выше); `legacy17` красный |

Файлы до и после мутаций идентичны (`sha256sum` сравнён `diff`, `RESTORED`).

## 6. sha256 итоговых файлов (без мутаций, `sha_final.txt`)

```
f2522d97e1ccfdf36bd31ca9b3f3c701150e3301359e8bbb2c60bcba54fb99dd  Cargo.toml
55d5c7465ef49ff9f4d0a6b4058096c0bf497188f7937f070fa7a97f725b416b  src/lib.rs
8dcbc165dba34a324233ed7affee7685bd26f2b2a1f45eb1f8e50633944be8a6  src/global/mod.rs
720e41f081010cbff4b3e50a7dad37b7b5ec431dff24a27560c1a57af60ecf44  src/global/sefer_alloc/global_alloc.rs
c7513973e4f80aaad6579a7adcc012cc437bed4e19920dd2e1a24500cc1c148c  src/global/exact_object/exact_fatal.rs
41b6e87be905454097256f70fffe50e574d02e5565e32ba7c5cb66d9e4a128e4  src/global/exact_object/exact_shard.rs
2c0f2a983e745c1271d16e6f153a521a4315f0e96e638944a31017c98d297f9b  src/global/exact_object/exact_table.rs
a853bc2188a9531131b9709622340c6558d576877bafd2a3ba413295a6a708a5  src/global/exact_object/insert_outcome.rs
12180b0c49fd1354e77b9836de1ac46bfd652fe8c3c166dd04eb3ea64c7df34d  src/global/exact_object/mod.rs
9381dfe40bdf237f13d8db3a136885bbb7d2fa35cfcfea2fe47cd59dce5f24c0  src/global/exact_object/narrow.rs
55e82eafb86586ec40f32f3b91d7e18d76a13ec83b3554c806569ffbbfcf4ee6  tests/r11_exact_object_proto.rs
```

`git diff | sha256sum` по tracked-изменениям: `bb0cfab58d06191d97ab6a9bdc3c044d509389d0dff7985704fb34f2d2fd94fc` (новые файлы untracked, покрыты списком выше).

## 7. Качество

- `rustfmt --check --edition 2021` по всем затронутым файлам: чисто.
- `cargo clippy --lib --test r11_exact_object_proto --features "exact-object-proto internals bench-internals" -- -D warnings`, `cargo clippy --all-targets -- -D warnings` (default), `cargo clippy --lib --features "production exact-object-proto" -- -D warnings`: чисто. `cargo clippy --all-targets --features "exact-object-proto internals bench-internals"` падает на ранее существующем `tests/regression_r3a_stats_walk_compiled_out.rs:110` (unused variable `s`, файл не тронут; это известный «born-red» all-targets под такой комбинацией, не от Ph1b).
- Точечные тесты `production internals`: `tests/r8_*` (10 бинарников) и `regression_r2_3_dbg_accessors_membership_guard` зелёные; `r8_*` + `sefer_alloc_examples` зелёные и с `production exact-object-proto internals`. `tests/unsafe_allow_placement.rs` зелёный. `tests/dbg_hook_safety_tripwire.rs` в этом worktree отсутствует; новые `dbg_*` — `bench-internals`-gated.
- `tests/no_stale_doc_references.rs` (production internals): 30 passed, 3 failed — все три счётчика документов, которые я по инструкции не правил: `architecture_test_file_count_matches_reality` (321 файл тестов, нужен токен `(321 files`), `verification_inventory_matches_docs` (README: `**321 integration test files**`), `readme_unsafe_inventory_counts_match_reality` (README: tier-1 теперь 28, токен `**28** tier-1 module-level seams`). Остальное (включая `lib_rs_seam_inventory_matches_canonical_grep` и `cargo_toml_alloc_global_panic_contract_is_accurate`) зелёное после добавления bullet'ов в `src/lib.rs` и размещения feature после `alloc-global`.

## 8. Новые unsafe seam'ы (для README-инвентаря; README не правился)

Два новых tier-1 (`#![allow(unsafe_code)]`) модуля: `global::exact_object::exact_shard` (сырой массив descriptor'ов в System-памяти под spinlock, `unsafe impl Sync`) и `global::exact_object::narrow` (`System.alloc/dealloc` точного объекта). Оба только под `exact-object-proto`. README tier-1 счётчик должен стать 28 (по canonical grep при `exact-object-proto` не гейтится: grep считает файлы независимо от feature).

## 9. Границы охвата и что НЕ покрыто

- НЕ production, не в `production`. Только узкие размеры 1..=16, align <= 8. Всё, что >= 17 байт, остаётся на старом slab-пути, где P1 воспроизводится и без паузы: `legacy17` (drop `Box<[u8;17]>` на своём потоке) красный в SB и TB (`write_next` в тело блока при слабо защищённом аргументе `Box`).
- Не покрыто: Small/Primordial slab для узких размеров (они вообще не доходят до slab), матрица realloc (проверен только код-путь «узкий -> move»; отдельного теста realloc нет), fallback/TLS teardown (exact-путь от кучи не зависит, но специально не тестировался), OOM таблицы descriptor'ов (путь есть, не вызывался), `batch-api` (`alloc_batch`/`dealloc_batch` идут мимо `GlobalAlloc` и не покрыты), многопоточная конкуренция шардов под нагрузкой, цена.
- Цена не измерена. Известно по коду: lookup в таблице делается в `dealloc` для ВСЕХ pointer'ов (lock-free ранний выход только когда шард пуст, иначе spinlock шарда); каждый узкий объект теперь отдельная `System`-аллокация + 32 байта слота.
- M3: «widening» физического `System.dealloc` (больший size при free) под Miri не наблюдаем: на этом target `System.dealloc` = `HeapFree`/`free` без проверки размера, поэтому такой мутант был удалён из исходника (он остался бы незамеченным). Красным ловится только wrong-Layout на входе `GlobalAlloc::dealloc` через проверку descriptor'а (abort, строка выше). Это проверка прототипа, не Miri.
- `reissue`: native `System` на Windows (LFH) возвращает VA в случайном порядке; первая версия теста (один VA, 2000 попыток) флейкнула 1 из ~8 запусков (`FAIL reissue: VA never reused`), заменена на пакетную (`same_va_reissues` x из n; native 10/10 повторов зелёные, 0 флейков в 5x4 повторных запусках narrow/zero/release/reissue). Под Miri нужны флаги address-reuse (см. команды).
- Во время работы был пойман артефакт сборки: после отката мутаций `cargo` один раз использовал устаревший артефакт (`release` красный/`narrow` Miri красный из-за остатка мутанта в бинарнике, `*.STALE-*.log` в `run2/`); все итоговые прогоны выполнены после `touch` исходников и принудительной пересборки, sha проверены `diff` после прогонов (`SHA_FINAL_UNCHANGED`).

## 10. Шаг 2 (W+2-форма: producer передаёт cap owner'у) — НЕ сделан

Причина: нельзя сделать чисто минимальным добавлением к шагу 1. Нужны: (1) идентичность owner'а в descriptor (токен потока/кучи, переживающий TLS teardown и recycle слота); (2) per-owner mailbox в System-памяти, хранящий именно `*mut u8` (под strict provenance целочисленный `addr` в таблице теряет provenance, free по нему невозможен), т.е. второй слой аллоцируемых структур с собственным ростом и OOM-путём; (3) триггеры drain в `alloc`/`trim_current_thread`, (4) политика для mailbox умершего/переданного потока (осиротевшие cap'ы) и (5) пересмотр инварианта unlink->free: при передаче cap'а unlink и free разнесены по разным потокам, и «accounting завершён» в кадре producer'а надо определять заново. Изолированный W+2-свидетель уже зелёный (`tests/r11_box_cap_gate_w2.rs`); для shipping-формы это отдельная дизайн-задача, а не правка прототипа.

## 11. Вывод

Шаг 1 проходит paused-Box witness в installed `SeferAlloc` в обеих моделях Miri (Stacked Borrows strict-provenance, Tree Borrows) и native для узких размеров; мутанты красные по нужным причинам; counterfactuals и baseline (feature выключена: прежний `paused` красный, `legacy17` красный) показывают, что witness не пустой. Модель «точный System-объект + внеобъектный descriptor» пригодна только как доказательство для узких классов: общий P1 для объектов >= 17 байт этим прототипом не закрыт.

## 12. Статус интеграции (оркестратор, 2026-10-01)

Код прототипа НЕ влит в `main`. Он сохранён локально в ветке `proto/exact-object-ph1b`, коммит `cabc785b2b18adbc1bb7278b95da5f4fca412b51` (ветка не опубликована). В `main` лежит только эта записка. Причины:

- Полный `cargo test --all-features --no-fail-fast` на дереве с прототипом: 2 упавших таргета (из них один — только счётчики документов). Содержательный провал: `tests/r8_global_box_provenance.rs` (`installed_box_drop_narrow_transfer_and_reissue`) — паника на `tests/support/r8_global_box_witness.rs:41` (`issued Box route`): существующий свидетель ждёт slab-маршрут для узких `Box`, а при `exact-object-proto` такого маршрута нет. `--all-features` включает прототип, поэтому влитая feature перестроила бы поведение глобального аллокатора для узких размеров во всех тестах этого CI-ряда.
- Прототип добавляет два tier-1 unsafe-seam'а (`global::exact_object::exact_shard`, `global::exact_object::narrow`): нужны записи в README-инвентаре и обновление счётчиков (`readme_unsafe_inventory_counts_match_reality` красный на ветке).
- Какая физическая граница выбирается (exact-object для узких размеров, глобально или иная), решает владелец в Ph2 (ADR). До этого решения прототип в `main` — лишняя поверхность без потребителя.
- Если ADR выберет exact-object: вернуть ветку, перевести `r8_*` свидетели на feature-aware ожидания, добавить seam'ы в README и считать это стартом Ph5a, а не готовым backend'ом.
