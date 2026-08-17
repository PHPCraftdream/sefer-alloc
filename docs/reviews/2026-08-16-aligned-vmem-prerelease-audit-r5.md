# `aligned-vmem` — новый статический prerelease-аудит (R5)

Дата: 2026-08-16  
Проверенная ревизия: `58f4c0ba2d23e30388db8b0162dc0cb540e19884` (`58f4c0b`)  
Объект: `crates/vmem`, его Cargo/CI-конфигурация, integration tests, benchmark и связанные open-items.

## Режим и границы

- Только чтение исходников, конфигурации, diff и истории.
- `cargo test`, `cargo check`, `cargo clippy`, benchmarks и runtime-проверки не запускались.
- Отчёт — единственный созданный артефакт; production-код аудитором не менялся.
- Во время аудита пользовательский diff исправил feature-gating в `crates/vmem/tests/decommit_capability.rs`; этот diff сохранён и не перезаписывался.
- Это bounded static pass одного агента: поведение реальных Windows/Linux/Darwin/BSD/hugetlb-хостов и фактическая сборка target/feature matrix не подтверждены.

## Итог

Текущий рабочий tree стал заметно лучше после CR3/R4-исправлений, но без ещё одного цикла исправлений и проверки я бы не объявлял релиз готовым.

Главные актуальные риски:

1. capability API сообщает слишком сильную гарантию для huge-page reservations и Miri;
2. новый `from_raw_parts` contract расходится с реально производимыми `ReservationFullParts`, особенно на 16 KiB page hosts;
3. один новый unsafe-тест вызывает `from_raw_parts` с заведомо невалидным fake pointer и маскирует это `mem::forget`.

Найденный в начале pass compile-blocker с безусловными imports в `decommit_capability.rs` уже устранён текущим незакоммиченным diff. Если релиз собирается от одного только `HEAD`, а не от текущего working tree, этот blocker всё ещё присутствует в `HEAD`.

## Findings

### R5-1 — MEDIUM: `decommit_reclaims_and_zeroes()` overclaims capability

Evidence:

- `crates/vmem/src/lib.rs:874-906` объявляет `Reservation::decommit_reclaims_and_zeroes()` как target-wide associated `const fn` и возвращает `cfg!(not(any(Darwin, BSD)))`.
- `crates/vmem/src/lib.rs:2238-2246` одновременно документирует, что `decommit` и `decommit_lazy` не работают на huge-page reservations.
- Miri backend в `crates/vmem/src/lib.rs:3894-3906` реализует decommit/recommit как no-op, но при Linux target `cfg!(not(...))` всё равно возвращает `true`.
- `crates/vmem/tests/decommit_capability.rs:23-55` проверяет только target OS и не проверяет Miri или huge reservation.

Последствия:

- На Linux/Windows `true` может быть прочитано вместе с `reservation.is_huge() == true`, после чего пользователь ожидает reclaim+zero-fill, хотя crate сам документирует silent no-op для huge pages.
- Под Miri capability выглядит как native guarantee, хотя тестовый backend не меняет физическое состояние и не моделирует reclaim.

Рекомендация: либо явно сузить docs до “ordinary native backend only” и вернуть `false` под `miri`, либо добавить instance-level query вроде `can_decommit_reclaims_and_zeroes(&self)`, которая учитывает `!self.is_huge()`. Добавить отдельные cfg-тесты для Miri и проверки huge/non-huge semantics.

### R5-2 — MEDIUM: `from_raw_parts` contract не согласован с реализацией и lossless round-trip

Evidence:

- Rustdoc `crates/vmem/src/lib.rs:1194-1215` требует runtime-page alignment для `base`/`reservation`, runtime-page multiples для `len`/`reservation_len` и утверждает, что это “asserted at construction”.
- Реальная проверка `crates/vmem/src/lib.rs:1294-1316` проверяет только `PAGE`-multiples; `page_size()` в этих invariants не участвует.
- `Reservation::reservation_len()` прямо предупреждает (`crates/vmem/src/lib.rs:780-804`), что на Apple Silicon 16 KiB OS mapping может иметь logical `reservation_len() == 4096`, хотя kernel mapping занимает 16 KiB.
- `into_full_parts()` сохраняет эти значения (`crates/vmem/src/lib.rs:966-989`), а `ReservationFullParts::into_reservation()` без изменений передаёт их в unsafe constructor (`crates/vmem/src/lib.rs:1455-1477`).

Сценарий: допустимый `reserve_aligned(PAGE, PAGE)` на 16 KiB-page host производит logical `len == 4096` и under-reported `reservation_len == 4096`. Такой объект можно losslessly разобрать через `into_full_parts`, но его parts не удовлетворяют новому документированному требованию “multiple of runtime `page_size()`”. Если усилить assert до docs, безопасный producer/consumer round-trip начнёт отвергать значения, которые сам crate производит; если оставить текущий assert, docs и сообщение об assert ложны.

Рекомендация: зафиксировать одну модель данных. Наиболее прямой вариант — явно разделить logical lengths (достаточно `PAGE`-multiple) и address/operation alignment (runtime page), убрать ложное “both are asserted”, а для full reservation хранить либо фактически rounded OS length, либо документировать, что native `munmap` допускает under-reporting. Добавить round-trip contract test для 16 KiB/64 KiB page configurations без fake pointers.

### R5-3 — MEDIUM: unsafe regression test использует недействительное владение памятью

Evidence: `crates/vmem/tests/smoke.rs:1651-1694`.

`into_full_parts_preserves_granted_huge()` создаёт `fake_base = 0x1000 as *mut u8`, передаёт его в `unsafe { full_parts.into_reservation() }`, выставляет `granted_huge = true`, а затем делает `mem::forget`. Это не удовлетворяет safety contract `from_raw_parts`: pointer не является live OS reservation, ownership/exclusivity не доказаны, а ОС-grant huge pages не подтверждён. `mem::forget` предотвращает `Drop`, но не делает сам unsafe call корректным.

Это не production path, но тест сам содержит UB по заявленному API contract и может начать падать/ломать процесс при любом изменении constructor/drop. Его oracle также проверяет только передачу boolean через unsafe boundary, а не корректность huge metadata из реального reservation.

В том же блоке `into_reservation_parts_loses_granted_huge()` (`smoke.rs:1696-1728`) не имеет assertion, подтверждающего название теста: он создаёт обычную reservation, извлекает parts и освобождает их; вывод о “loss” остаётся только в комментариях.

Рекомендация: заменить fake-pointer reconstruction на pure metadata assertions (`ReservationFullParts` имеет public fields), а поведение `into_reservation()` проверять только на реально живых parts с `granted_huge == false`. Если нужен true-branch, нужен отдельный backend fixture/mock, который честно предоставляет live reservation и доказанный metadata contract; иначе такой unsafe тест лучше удалить. Для legacy `ReservationParts` добавить явный assertion/структурный oracle, либо сделать это documentation-only test без misleading `#[test]`.

### R5-4 — LOW: Windows large-page retry counter не соответствует собственному docs

Evidence:

- Contract counter в `crates/vmem/src/lib.rs:295-302` говорит, что считается failure, если **initial large-page attempt или ordinary retry** вернулся с неверно выровненным base.
- В `win_reserve_commit` `crates/vmem/src/lib.rs:2444-2448` initial large-page success устанавливает `huge_granted = true`.
- При alignment miss increment (`crates/vmem/src/lib.rs:2514-2520`) выполняется только при `extra_commit_flags != 0 && !huge_granted`.

Значит, defensive-сценарий “large-page VirtualAlloc succeeded, но base оказался misaligned” освобождается и уходит в two-call path, однако `WINDOWS_LARGE_PAGE_RETRY_FAILURES` не увеличивается — несмотря на прямое обещание docs. Сценарий редкий на корректном Windows kernel, но именно для таких нарушений counter предназначен.

Рекомендация: отделить `large_page_attempted` от `huge_granted` и считать alignment-failure независимо от того, был ли initial call успешен; либо сузить docs до фактического semantics и завести отдельный counter для “large request succeeded but alignment fallback”.

### R5-5 — INFO: `ReservationFullParts` docs оставляют опасный ownership/lifetime impression

Evidence: `crates/vmem/src/lib.rs:966-989` и `1403-1477`.

`into_full_parts()` описывает сценарий “custom allocator that persists metadata across restarts”, хотя raw pointers и live OS mappings не переживают process restart. Кроме того, `ReservationFullParts` — обычный struct без `Drop`/`release_full_parts`; его забывание тихо leaks reservation, а `into_reservation()` требует unsafe reconstruction.

Рекомендация: заменить “across restarts” на “between components within the same process”, явно написать “dropping these parts does not release the reservation”, и добавить unsafe/manual-release path либо пример, показывающий ownership transition.

## Проверенные исправления

Следующие предыдущие R4-сигналы в текущем коде выглядят закрытыми статически:

- MIPS теперь fail-fast через `compile_error!` (`lib.rs:3459-3464`), вместо buildable-but-broken target.
- 32-bit huge exact path не повторяет generic exact attempt (`lib.rs:3036-3055`).
- `mock::drain()` использует `mem::take`, не удерживая `RefCell` borrow на lifetime возвращённого `Vec` (`src/mock.rs:242-259`).
- re-arm race fault injection закрыта `Mutex<FaultState>` (`src/fault_injection.rs:52-159`).
- Windows `VirtualFree(MEM_RELEASE)` failure теперь наблюдаем через counter (`lib.rs:2868-2888`).
- Текущий uncommitted patch правильно сделал imports/тесты `decommit_capability.rs` feature-gated (`:8-21`, `:65-68`, `:131-134`).

## Остаточные release-evidence gaps

Это не новые доказанные runtime bugs, но их нельзя считать закрытыми одной статической проверкой:

- Darwin/BSD eager-decommit остаётся advisory-only; новый capability query честно сообщает `false`, но semantics не исправлены (`lib.rs:1814-1833`).
- CI проверяет Miri только через `cargo check` с `--cfg miri`, а не исполняет crate test suite под Miri (`.github/workflows/ci.yml:187-192`).
- i686 GNU/MUSL в CI имеют compile-only coverage, но не runtime coverage (`ci.yml:193-202`).
- Linux `MAP_HUGE_2MB` и huge success/release ветки по-прежнему reasoned-from-spec, без hugetlb-enabled runner (`lib.rs:3483-3501`).
- 64-bit Unix deliberate `size + align` over-reserve и Windows speculative retry остаются performance opportunities; для них нужны реальные workload measurements, а не новые speculative fast paths (`docs/perf/OPEN_ITEMS.md:1129-1134`, `1214-1222`).

## Release decision

**NO-GO без исправления R5-1, R5-2 и R5-3.**

Минимальный порядок перед релизом:

1. определить и закрепить семантику capability API для native/Miri и huge reservations;
2. согласовать `from_raw_parts`/`ReservationFullParts` contract с 16 KiB/64 KiB page behavior;
3. убрать invalid unsafe fake-pointer test и заменить его валидным oracle;
4. перенести текущий feature-gating fix из working tree в релизную историю;
5. после этого уже выполнить разрешённые проектом feature/target checks и тесты в отдельном verification pass.

