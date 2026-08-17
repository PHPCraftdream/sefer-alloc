# `aligned-vmem` — независимое исследование перед релизом (R6)

Дата: 2026-08-16  
Ревизия: `8d68715749e74c8105fa78e76309d466cb2d6779`  
Пакет: `aligned-vmem` 0.2.0  
Объём: `crates/vmem/src/lib.rs`, публичная документация и README, integration tests, benchmark target, CI и локальные package-gates.

## Режим и ограничения

Это статический read-only аудит. Исходники, тесты, CI-конфигурация и benchmarks не изменялись; создан только этот отчёт. `cargo test`, `cargo check`, `cargo clippy`, `cargo doc`, benchmarks и публикация не запускались по условию задачи. Поэтому утверждения о runtime-поведении Windows/Android/BSD/Miri основаны на чтении backend-кода и контрактов, а не на исполнении.

## Итог

Статический вердикт: **условный NO-GO для публикации с включённым `lazy-commit` до принятия/исправления R6-1 и R6-2**.

R6-1 — противоречивый публичный контракт: обычный `Reservation`/`as_ptr()` описан как доступный на всём `len()`, но lazy Windows reservation возвращается с незакоммиченной хвостовой частью. R6-2 — та же модель допускает размеры и `initial_commit`, кратные только `PAGE`, хотя Windows commit и последующие `commit_range` работают с runtime page size; на системе с page size больше 4 KiB хвост может стать навсегда недоступным для commit.

Остальные замечания — низкого риска: stale test oracle для Android/Windows huge pages, два ошибочных rustdoc-фрагмента, отсутствие package/doc/semver gates именно для `aligned-vmem` и две узкие возможности уменьшить лишние syscalls.

### Что подтверждено как исправленное после R5

- `decommit_reclaims_and_zeroes()` больше не выдаёт capability huge-page и Miri backend; добавлен instance-level `can_decommit_reclaim_and_zero()`.
- Full-parts round-trip теперь использует живую reservation, а fake-pointer unsafe test удалён.
- Контракт `from_raw_parts` согласован с тем, что crate реально производит на 16 KiB-page host.
- Windows large-page retry и alignment failures разделены собственными счётчиками и документацией.
- Bench target теперь содержит как cold/untouched, так и faulted/dirty-page decommit/recommit cycles.

## Findings

### R6-1 — MEDIUM: `Reservation` не выражает незакоммиченный tail lazy reservation

**Доказательства:**

- Общий rustdoc `Reservation` говорит, что `as_ptr()` valid for `len()` bytes, исключая только диапазоны, которые caller decommitted и ещё не recommitted: `crates/vmem/src/lib.rs:698-705`.
- То же утверждение повторяется в `Reservation::as_ptr`: `lib.rs:775-779`.
- Но `reserve_aligned_lazy` прямо обещает: на Windows коммитится только `initial_commit`, а остальная часть span остаётся reserved-but-not-committed: `lib.rs:2199-2205`.
- Safe `commit_range` лишь требует от caller заранее обеспечить commit перед записью; у `Reservation` нет committed length/state, отличающего lazy tail от обычного доступного span: `lib.rs:1208-1225`.

**Сценарий:** `reserve_aligned_lazy(64 * 1024, 4 * 1024, 4 * 1024)` возвращает тот же тип `Reservation`, но на Windows запись в `[base + 4 KiB, base + 64 KiB)` до `commit_range` приводит к access violation. Это не Rust-safe write (у caller всё равно raw pointer), однако текущая общая документация позволяет понять `as_ptr()` как valid на всём `len()` и не описывает исключение для lazy allocation.

Отдельно конфликтует safety-документация `from_raw_parts`: она утверждает, что Windows reservation обязательно создана одним `MEM_RESERVE | MEM_COMMIT` (`lib.rs:1343-1344`), тогда как crate сам создаёт lazy/two-call reservation через отдельный `MEM_RESERVE` и частичный `MEM_COMMIT`; release backend явно принимает такую область независимо от commit state (`lib.rs:2765-2770`). Это делает full-parts reconstruction lazy reservation несогласованной с её собственным producer path.

**Рекомендация:** перед релизом явно определить контракт:

1. либо обновить docs `Reservation`/`as_ptr`/README: у lazy Windows handle writable только committed ranges, а `commit_range` обязателен перед каждым доступом к tail;
2. переписать `from_raw_parts` как требование к live `MEM_RESERVE` region, а не к единственному combined allocation call, и отдельно описать committed/uncommitted state;
3. предпочтительно вернуть из lazy API metadata о committed prefix или отдельный handle/type, если библиотека хочет иметь проверяемый stateful контракт вместо документационного исключения.

### R6-2 — MEDIUM, platform-specific: lazy validation разрешает неисполняемые логические хвосты на Windows с page size > `PAGE`

**Доказательства:**

- `validate_size_align` и `validate_initial_commit` проверяют только кратность `PAGE`: `lib.rs:1660-1667` и общий size contract.
- Windows `VirtualAlloc(MEM_COMMIT)` коммитит целые runtime pages, а `try_commit_range` принимает только offsets, кратные `page_size()`: `lib.rs:2159-2162`, Windows two-call path `lib.rs:2741-2745`.
- Сам crate признаёт, что `page_size()` может быть больше `PAGE`; это значение читается через `GetSystemInfo`: `lib.rs:630-689`.

**Сценарий:** если Windows runtime page size равен 64 KiB, вызов с `size = 17 * PAGE` (68 KiB) и `initial_commit = PAGE` формально проходит текущую проверку. Первый `VirtualAlloc(MEM_COMMIT, 4 KiB)` фактически коммитит 64 KiB. Логический tail `[64 KiB, 68 KiB)` остаётся незакоммиченным, но `commit_range(64 KiB, 68 KiB)` отвергается: конец не кратен runtime page size. Попытка коммитить 128 KiB отвергается bounds-check по `Reservation::len()`. В результате часть возвращённого `len()` нельзя сделать writable через публичный API.

Это также показывает более общий недостающий invariant: lazy `size` должен быть совместим с granularity backend, а не только с минимальной compile-time константой.

**Рекомендация:** на Windows валидировать `size` и `initial_commit` относительно `page_size()` (либо явно округлять и возвращать фактический span). Минимально безопасный вариант — отвергать lazy request, если `size` или `initial_commit` не кратны runtime page size; это нужно отразить в rustdoc и ошибке. Для eager path некомплементарный к runtime page size размер менее опасен, потому что весь logical span уже committed, но lazy path требует строгого правила.

### R6-3 — LOW: stale huge-page test oracle ломает Android и неверно описывает Windows

`crates/vmem/tests/huge_pages.rs:98-102` под `#[cfg(not(target_os = "linux"))]` утверждает, что на «non-Linux Unix and Windows» `is_huge()` всегда false. Это больше не соответствует production contract:

- backend explicitly включает Android вместе с Linux для `MAP_HUGETLB`/`HUGE_SUPPORTED`: `lib.rs:3065-3073`, `3437-3465`, `3618-3631`;
- Windows rustdoc допускает `is_huge() == true`, если `MEM_LARGE_PAGES` действительно выдан и включён privilege: `lib.rs:2305-2324`.

Практически обычная CI-машина чаще получит fallback, но при настроенных huge pages Android test может падать на корректном grant; на Windows assertion фиксирует ложный универсальный запрет. Соседние Linux-only tests также не покрывают Android.

**Рекомендация:** ограничить false-assert только реально no-op платформами (`not(windows)` и `not(any(target_os = "linux", target_os = "android"))`), а Linux-kernel tests расширить до Android. Для Windows проверять контракт best-effort без универсального `!is_huge()`; привилегированный positive path можно оставить отдельным opt-in test.

### R6-4 — LOW: runnable-looking huge-page example использует заведомо запрещённую форму

В `Reservation::can_decommit_reclaim_and_zero` example (`lib.rs:976-985`) вызывается `reserve_aligned_huge(1024 * 1024, 1024 * 1024)`. На Linux/Android при feature `huge-pages` оба аргумента обязаны быть кратны 2 MiB (`lib.rs:2296-2303`), поэтому этот example получает `None` до syscall и вообще не демонстрирует huge reservation. Он помечен `text`, поэтому CI это не поймает.

**Рекомендация:** использовать, например, `2 * 1024 * 1024` для `size` и `align`, либо явно пояснить platform-specific rejection.

### R6-5 — LOW: `ReservationFullParts` rustdoc ссылается на несуществующие поля

`lib.rs:1064-1071` советует вручную вызвать `release`, используя `ptr`, `len` и `align` из `ReservationParts`, хотя текущий объект — `ReservationFullParts`, у которого поля называются `reservation`, `reservation_len`, `align`; поля `ptr` нет, а `len` означает usable span, не reservation length (`lib.rs:1524-1536`). Это copy-paste ошибка в ownership/release инструкции и легко приводит к неправильному ручному release.

**Рекомендация:** либо явно написать `release(parts.reservation, parts.reservation_len, parts.align)`, либо показать безопасное преобразование в `ReservationParts::new(...)`.

### R6-6 — LOW: у aligned-vmem нет собственных package/doc/semver release gates

`.github/workflows/ci.yml:136-202` для `aligned-vmem` проверяет clippy, tests, mock cfg, Miri cfg compile-only и i686 compile-only. Но в этой job нет:

- `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --all-features --no-deps`;
- `cargo package`/`cargo publish --dry-run -p aligned-vmem`;
- semver compatibility check;
- проверки, что опубликованный package действительно содержит необходимые файлы и не конфликтует с уже опубликованной версией.

Такие gates уже присутствуют для `sefer-region` в верхней части того же workflow, поэтому разрыв выглядит accidental. `scripts/check-all.mjs` также добавляет для aligned-vmem только clippy/test rows (`scripts/check-all.mjs:162-196`).

**Рекомендация:** перенести минимальный doc/package/semver gate в `aligned-vmem-gates` или сделать общий reusable step. Это не исправляет runtime bug, но снижает риск релиза с невалидным rustdoc/package или неожиданным semver break.

### R6-7 — LOW, performance: safe decommit делает заведомо бесполезный syscall для huge reservation

Docs прямо говорят, что huge-page `decommit`/`decommit_lazy` на Windows и Linux не работает и оставляет старые данные: `lib.rs:2347-2355`. При этом safe methods всё равно вызывают backend после счётчика: `lib.rs:1115-1130` и `1149-1160`. На Windows это заведомо неуспешный `VirtualFree(MEM_DECOMMIT)`, на Linux обычно бесполезный `madvise` с неподходящей granularity.

**Рекомендация:** в safe methods рано возвращаться для `self.is_huge()` после обновления диагностического счётчика; свободные raw functions оставить без такого предположения. Нужно только уточнить semantics счётчика (attempted API call vs skipped incompatible operation). Это небольшая оптимизация и не должна подменять capability contract.

### R6-8 — LOW, performance: Linux huge exact failure повторяется во втором пути

Для Linux/Android при `align == 2 MiB` сначала выполняется exact `mmap(size, MAP_HUGETLB)` (`lib.rs:3101-3132`). Если он возвращает null, общий путь ниже снова делает `mmap(size + align, MAP_HUGETLB)` и только после этого ordinary fallback (`lib.rs:3164-3193`). На обычной машине без hugetlb pool это две гарантированно неуспешные huge-page попытки на один logical reserve.

**Рекомендация:** если exact attempt вернул null, переходить непосредственно к ordinary fallback; сохранять общий retry только для необычного случая, когда exact mapping вернулся с неожиданно неправильным alignment и хочется оставить defensive recovery. Это кандидат на измерение, а не повод менять fallback semantics без benchmark evidence.

## Низкоприоритетные уточнения

- Имена capability API несимметричны: `decommit_reclaims_and_zeroes` и `can_decommit_reclaim_and_zero` различаются `zeroes/zero`; это не bug, но ухудшает discoverability. Если API ещё не стабилизирован, стоит унифицировать spelling.
- `can_decommit_reclaim_and_zero()` вычисляется только из compile-time capability и `is_huge`; это нормально, но его docs должны явно оставаться advisory contract, потому что backend syscall errors всё ещё silently discarded в Drop-oriented API.
- Для `from_raw_parts` под Miri стоит отдельно и недвусмысленно потребовать allocation provenance от `std::alloc` с тем же `Layout`, который затем передаётся в `dealloc` (`lib.rs:1303-1307`, Miri release backend около `lib.rs:3971`). Сейчас это следует из слов «compatible with release path», но не сформулировано как отдельное safety requirement.

## Остаточные принятые риски, не являющиеся новыми R6 findings

- Darwin/BSD eager decommit остаётся advisory и не даёт zero/reclaim guarantee; capability query теперь это честно отражает.
- Miri и i686/musl в CI проверяются compile-only; реального Miri execution, Android/BSD runtime и privileged huge-page runner нет.
- Linux kernels до поддержки `MAP_HUGE_*` и 64-bit Unix over-reserve `size + align` остаются задокументированными portability/performance assumptions.
- Ошибки `munmap`/`VirtualFree`/`madvise` в Drop-oriented release/decommit API не становятся recoverable; diagnostic counters доступны только в `bench-internals`.

## Рекомендуемый порядок перед публикацией

1. Уточнить и исправить R6-1: lazy committed-range contract и Windows `from_raw_parts` safety wording.
2. Исправить R6-2: runtime-page invariant для Windows lazy size/initial commit.
3. Исправить test cfg/oracles и huge example (R6-3/R6-5/R6-4).
4. Добавить aligned-vmem doc/package/semver gates (R6-6).
5. Отдельно измерить R6-7/R6-8; это оптимизации после закрепления contract semantics.

После этих изменений нужен обычный CI/runtime pass по Windows, Linux/Android и Miri. В рамках настоящего read-only исследования он сознательно не выполнялся.
