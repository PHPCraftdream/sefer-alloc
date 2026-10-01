# Фаза 0 рефакторинга: приёмочная записка исходных обязательств

Дата: 2026-10-01. План: `docs/design/2026-10-01-113842-src-foundational-refactoring-plan-xs-sol.md` (§3, §4 шаг 0).
Это измерение и фиксация, не фикс: runtime (`src/`) не менялся; installed-Box P1 остаётся красным (см. §2). Не release GO.

Источник статуса в таблицах: **[нбл]** = наблюдала сама в этом прогоне, **[док X]** = со слов документа X, не перепроверялось.

## 1. Окружение и идентичность источника

- Host/OS: Windows 10 x86_64 (`x86_64-pc-windows-msvc`). Miri исполняет тот же host-target; `System` под Miri = аллокатор Miri.
- Native: `rustc 1.97.0 (2d8144b78 2026-07-07)`. Miri: `miri 0.1.0 (3659db0d3e 2026-07-05)`, `rustc 1.99.0-nightly (3659db0d3 2026-07-05)`.
- Worktree `refactor-ph0`, база `ceffecb5c07d45e9d715670dfcaec3de69b2f9d2`, tree `HEAD^{tree}` = `c1fa03eae961c5ae2b442ec963c2e3978c0b3941`. Рабочее дерево на момент старта чистое.
- Единственный изменённый tracked-файл: `tests/support/r8_global_box_witness.rs` (+9 строк, §5). `git diff | sha256sum` = `0004c9834e6fd24bddc84184681038022b71aef0197087b5b5c63507e42dc053`.
- sha256 файлов свидетеля (после правки):
  - `tests/support/r8_global_box_witness.rs` 9a7007c58de7fe7ecccdabdf141c28ab13da124ddc67ca06e0c661ff66f5c1e4 (до правки, = HEAD: 29fff9c63d0659d4c442a23bf60b8bde4807400711b18ab37262c144676c1a58)
  - `tests/miri_global_box_acceptance.rs` 6699df154a3e7b2308e00310be7b5d61448f56851ce716ca8072e26d2882a1a8
  - `tests/r11_box_cap_gate_w1.rs` f9959efe44d88937c147cab836813fadf850fc16870235d83344b45f24e79313
  - `tests/r11_box_cap_gate_w2.rs` 87f6900e808190be4a5c674136c04bbb7db76b86a7c2117c5648d20caed99c47
  - `tests/support/r11_box_cap_gate.rs` d830824e6d37603cd8c752f60dc61250effb30978cb6a274624ca914e9fe1528
  - `Cargo.toml` afeed5db3a9aca0b81d995818d07c18fa018f6b0ddbcd87bec48e201ec95917a
  - src (не менялись), для привязки места падения: `src/alloc_core/platform/node.rs` 65534137be1391f68897f69b5bc1b65a3e6179a90d30b2d1bbbd12cc049a0721; `src/alloc_core/small/alloc_core_small_reclaim.rs` 75c39b0a01501d03326c5ab2377a60a9a8ff11878a33d6a104c5d6705eb4133b; `src/registry/heap_core_xthread/routing.rs` 7255bde223f9fbfab85d18c0260b25e518617bf6e0020e5e2e57eefecdaa1b97; `src/registry/segment_route/terminal_publication_gate.rs` 3cdf3eb049f1b0c870dd908c3a71260170c2d95ec6e68bc57654890091fc65c4.
- Перед каждым прогоном `touch tests/*.rs`; `CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/target/wt` (loom: `.../target/wt-loom`).
- Сырые логи (вне репо): `D:/dev/rust/sefer-alloc/target/wt/logs/ph0/` (90 602 байт всего, 31 файл). Префиксы: без префикса `sb-/tb-/native-` = прогон до правки witness; `v3-` = после правки (итоговая идентичность). Все под 200 KiB.

## 2. Команды

Флаги Miri как в CI job `miri-core` (`.github/workflows/ci.yml:2801-2845`): `MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation -Zmiri-report-progress=10000000"`; Tree Borrows = те же плюс `-Zmiri-tree-borrows`. Ни валидация, ни borrow-проверки не отключались.

```text
# installed #[global_allocator] SeferAlloc, harness-free, один сценарий на процесс
cargo +nightly miri test --features "alloc-global internals bench-internals" -j 1 --test miri_global_box_acceptance -- paused
cargo +nightly miri test --features "alloc-global internals bench-internals" -j 1 --test miri_global_box_acceptance -- narrow
# isolated installed-System свидетели (W+1 / W+2 / W-1)
cargo +nightly miri test --features std --test r11_box_cap_gate_w1 -- positive
cargo +nightly miri test --features std --test r11_box_cap_gate_w2 -- positive | root-one | root-eight
cargo test --features std --test r11_box_cap_gate_w{1,2} -- positive            # native
# native Sefer
cargo test --features "alloc-global internals bench-internals" --test r8_global_box_provenance
cargo test --features "production batch-api internals bench-internals" --test r8_terminal_global --test r6_terminal_owner_drain --test r8_owner_sidecar_miss
cargo test --features "alloc-global alloc-xthread alloc-decommit alloc-segment-directory batch-api internals bench-internals" (те же три --test)
# Loom (RUSTFLAGS="--cfg loom", --release; features как в scripts/loom.mjs)
cargo test --release --features "alloc-core alloc-xthread" --test loom_sidecar_bitmap --test loom_terminal_owner_drain --test loom_terminal_large
cargo test --release --features "alloc-core alloc-xthread" --test loom_r11_small_sidecar
# низкоуровневый Miri (CI job miri-plain; MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5", без strict-provenance)
cargo +nightly miri test --features "alloc-global alloc-xthread internals bench-internals" --test r6_terminal_owner_drain -- --exact requested_sizes_one_through_seven_retire_once_and_reissue
```

## 3. Исход четырёх условий шага 0

Все наблюдались [нбл]; ожидание шага 0 выполнено по всем четырём, оракул не сдвинулся и не исчез.

**(а) Пауза после terminal RMW внутри `Box::drop`.**
- Код: `src/registry/heap_core_xthread/routing.rs` вызывает `TerminalPublicationGate::pause_after_publication` только если `pin.publish_small/publish_large` вернул `published == true` (terminal CAS/exchange уже выполнен); продюсер при этом внутри `drop(byte)` (`tests/support/r8_global_box_witness.rs:113`) -> `GlobalAlloc::dealloc`. Контроллер входит в `trim_current_thread` только после `Gate::wait_for_publication()`, который снимается именно в `pause_after_publication`.
- Miri: protector-тег, который нарушает запись, создан «as argument» в `drop(byte)` (`r8_global_box_witness.rs:113:9`, в логах `help: <493111> is this argument` / `the protected tag <...> was created here`). Weakly/Reserved-protected тег жив только пока кадр `Box::drop` активен, значит владелец трогает память, пока продюсер внутри кадра.
- Прямой оракул добавлен в harness (§5): до `Gate::arm` `pin.pending_for_test(ptr) == false` («живой Box не имеет pending publication»), после `wait_for_publication` — `== true` («пауза идёт после terminal publication»). Оба assert прошли в обеих моделях (падение осталось на `write_next`, а не на этих assert; иначе было бы `panicked at ... pause must follow` в логе: `grep -c panicked v3-*-paused.log` = 0). `pending_for_test` читает только независимый sidecar, тело Box не трогает.

**(б) Обе модели падают на записи в root body, exit 1, не timeout.**

| Модель | Исход [нбл] | Ключевые строки (`v3-*-sefer-paused.log`) |
| --- | --- | --- |
| Stacked Borrows | EXIT=1 | `error: Undefined Behavior: not granting access to tag <1479> because that would remove [Unique for <493111>] which is weakly protected` at `src\alloc_core\platform\node.rs:90:18` (`ptr.write_unaligned(next)`); `<1479> was created here, as the root tag for alloc813` (`crates\aligned-vmem\src\os\miri.rs:21:24`); `<493111> is this argument` at `tests\support\r8_global_box_witness.rs:113:9` (`drop(byte)`) |
| Tree Borrows | EXIT=1 | `error: Undefined Behavior: write access through <1414> (root of the allocation) at alloc815[0x403a20] is forbidden` at `node.rs:90:18`; `this foreign write access would cause the protected tag <...> (currently Reserved) to become Disabled`; `protected tags must never be Disabled` |

Общий backtrace (оба режима): `Node::write_next` (`node.rs:90`) <- `reclaim_sidecar_record` (`alloc_core_small_reclaim.rs:63`) <- `AllocCore::drain_sidecar_ingress` (`alloc_core/sidecar_drain.rs:141`) <- `HeapCore::drain_sidecar_ingress` (`heap_core_xthread/sidecar_drain.rs:9`) <- `trim_for_recycle` (`heap_core/state/ownership.rs:112`) <- `SeferAlloc::trim_current_thread` (`global/sefer_alloc/diag.rs:208`) <- `witness::paused_terminal_owner_retirement` (`r8_global_box_witness.rs:124`) <- `main`. Это совпадает с находкой `docs/reviews/2026-10-01-0945-r11-terminal-box-acceptance-sol-codex.md` (там `:63`/`:90`, строка witness тогда 109/115). Код завершения именно 1 (UB), не таймаут: логи содержат `error: aborting due to 1 previous error` и `EXIT=1`; первая клетка (с компиляцией) заняла ~3.5 мин по меткам файлов, `timeout 3000` не сработал.
Затронутая запись приходится на offset `0x403a20` аллокации-reservation, т.е. на тот самый байт payload Box (strict trim -> intrusive `next` в payload через root reservation).

**(в) W−1 негатив (root-write 1/8 байт).** 4 из 4 клеток EXIT=1 с UB на строке записи через root (`tests/support/r11_box_cap_gate.rs:136` для 1 байта, `:144` для 8 байт `write_unaligned`), protector из аргумента `drop(boxed)` (`:125`). Не exit 3 (`process::exit(3)` после записи достижим только если запись прошла).
- SB root-one: `not granting access to tag <9352> because that would remove [Unique for <14995>] which is weakly protected`; root-eight: `<10067> ... [Unique for <15676>]`.
- TB root-one: `write access through <9079> (root of the allocation) at alloc3850[0x0] is forbidden`; root-eight: `write access through <9777> (root of the allocation) at alloc4133[0x0] is forbidden`.

**(г) Положительный System baseline доходит до маркера ПОСЛЕ join.** W+1 positive и W+2 positive: native, SB, TB = 6 из 6 клеток EXIT=0; в каждой Miri/native клетке 14 из 14 строк `ROUND ... joined` (7 размеров x 2 раунда; `joined` печатается после `producer.join()`) и затем `COMPLETE owner_free=<false|true> sizes=1..7 rounds=2`. `va_reused=true` в 3 из 14 раундов W+2 как под SB, так и под TB (знаменатель 14); это не доказательство ABA/новой incarnation.

## 4. Таблица `entry × feature × OS × model × postcondition`

Колонка «охват» — что пункт НЕ доказывает. Статусы не сворачиваются; `CFG_EXCLUDED` = тест существует, но cfg его исключает из данной клетки; `NOT_RUN` = не запускал и документ не даёт результата; `BUILD_ONLY` не наблюдалось ни в одной клетке.

| # | Entry | Features | OS | Модель | Postcondition | Статус | Источник | Охват / что не доказывает |
|---|---|---|---|---|---|---|---|---|
| 1 | installed `#[global_allocator] SeferAlloc`, настоящий `Box<u8>`, пауза внутри `Box::drop` после terminal RMW, owner `trim_current_thread`: `miri_global_box_acceptance -- paused` | alloc-global internals bench-internals | Win x86_64 (host-Miri) | Miri SB (strict-prov) | owner логически retire без записи в body, producer-frame жив | **FAIL** (exit 1, `node.rs:90` write_next) | [нбл] `v3-sb-sefer-paused.log`; та же картина без правки harness | Одна конфигурация, одна пара Box; модели experimental. Не «native crash» |
| 2 | то же | то же | Win (host-Miri) | Miri TB (strict-prov) | то же | **FAIL** (exit 1, foreign write -> Reserved->Disabled) | [нбл] `v3-tb-sefer-paused.log` | то же |
| 3 | то же, CI-строки `ci.yml:2831-2845` | то же | ubuntu-latest | Miri SB/TB | то же | **NOT_RUN** (мной); CI смонтирован, результата CI нет | [док ci.yml], [док R11 manifest] фиксирует тот же отказ как P1 (`2ad6e542`) | Linux не проверялся |
| 4 | installed SeferAlloc, `narrow` (два раунда `Box<u8>`+`Box<[u8;7]>`, narrow reborrow, producer join, trim, reissue) | alloc-global internals bench-internals | Win (host-Miri) | Miri SB | retire через typed Drop publication после join; `assert_retired` | **PASS** (exit 0, `COMPLETE narrow_two_rounds`) | [нбл] `v3-sb-sefer-narrow.log` | Пауза внутри кадра НЕ проверяется (producer join до trim); только 2 раунда, 2 размера |
| 5 | то же | то же | Win (host-Miri) | Miri TB | то же | **PASS** (exit 0, `COMPLETE narrow_two_rounds`) | [нбл] `v3-tb-sefer-narrow.log` | то же |
| 6 | то же `narrow` в `r8_global_box_provenance` (libtest) | alloc-global internals bench-internals | Win | native | то же | **PASS** (1 из 1) | [нбл] `v3-native-r8_global_box_provenance.log` | Нативный путь: нет borrow-модели; `paused` в этом target `cfg(miri)` -> **CFG_EXCLUDED** (native) |
| 7 | `paused` как libtest `installed_box_drop_retires_before_terminal_producer_resumes` | то же | Win | native | — | **CFG_EXCLUDED** (`#[cfg(miri)]`) | [нбл] по исходнику `tests/r8_global_box_provenance.rs`; в native-логе 1 тест, не 2 | В native пауза не исполняется вообще |
| 8 | isolated installed-System W+1 positive (producer сам `System.dealloc`) | std | Win | native | marker после join | **PASS** | [нбл] `native-w1-positive.log`: 14/14 ROUND joined + COMPLETE | НЕ Sefer; не shipping lifecycle |
| 9 | W+1 positive | std | Win (host-Miri) | SB | то же | **PASS** | [нбл] `sb-w1-positive.log` 14/14 | то же |
| 10 | W+1 positive | std | Win (host-Miri) | TB | то же | **PASS** | [нбл] `tb-w1-positive.log` 14/14 | то же |
| 11 | W+2 positive (producer передаёт cap, owner `System.dealloc(cap)` при паузе) | std | Win | native | то же | **PASS** | [нбл] `native-w2-positive.log` 14/14 | НЕ Sefer; W+3 (reissue по cap без physical free) не проверен |
| 12 | W+2 positive | std | Win (host-Miri) | SB | то же | **PASS** | [нбл] `sb-w2-positive.log` 14/14 | то же |
| 13 | W+2 positive | std | Win (host-Miri) | TB | то же | **PASS** | [нбл] `tb-w2-positive.log` 14/14 | то же |
| 14 | W−1 negative `root-one` (1 байт через root) | std | Win (host-Miri) | SB | падает UB из-за protector `Box` | **PASS-как-отказ** (EXIT=1, UB на `:136`) | [нбл] `sb-w2-root-one.log` | Свидетель-мутант: PASS здесь = «ожидаемо отвергнут» |
| 15 | W−1 `root-eight` (8 байт `write_unaligned`) | std | Win (host-Miri) | SB | то же | **PASS-как-отказ** (EXIT=1, `:144`) | [нбл] `sb-w2-root-eight.log` | то же |
| 16 | `root-one` | std | Win (host-Miri) | TB | то же | **PASS-как-отказ** (EXIT=1) | [нбл] `tb-w2-root-one.log` | то же |
| 17 | `root-eight` | std | Win (host-Miri) | TB | то же | **PASS-как-отказ** (EXIT=1) | [нбл] `tb-w2-root-eight.log` | то же |
| 18 | root-one/root-eight | std | Win | native | — | **CFG_EXCLUDED** (`cfg(miri)`) | [нбл] по исходнику `tests/r11_box_cap_gate_w2.rs` | — |
| 19 | native Sefer terminal: `r6_terminal_owner_drain` + `r8_owner_sidecar_miss` + `r8_terminal_global` (прямой `SeferAlloc`/Heap API, не installed Box) | production batch-api internals bench-internals | Win | native | foreign terminal free виден strict trim, retire once | **PASS** (8+4+6 = 18 из 18) | [нбл] `native-production-terminal.log` | Нет паузы внутри `Box::drop`; не installed allocator (кроме отдельных тестов этого файла) |
| 20 | то же, non-fastbin набор | alloc-global alloc-xthread alloc-decommit alloc-segment-directory batch-api internals bench-internals | Win | native | то же | **PASS** (7+2+6 = 15 из 15; меньше тестов, часть gated `fastbin`) | [нбл] `native-nonfastbin-terminal.log` | то же |
| 21 | то же | то же | ubuntu-latest (CI `global-regressions`) | native | то же | **NOT_RUN** (мной); Linux строки смонтированы в `ci.yml:2020-2040`; результата CI нет | [док ci.yml] | macOS/Linux результат не наблюдался |
| 22 | `r6_terminal_owner_drain::requested_sizes_one_through_seven_retire_once_and_reissue` (низкий уровень, прямой Heap API) | alloc-global alloc-xthread internals bench-internals | Win (host-Miri) | Miri SB, **без** strict-provenance (CI miri-plain flags) | retire once + reissue, размеры 1..7 | **PASS** (1 из 1, 48.13s) | [нбл] `sb-r6-owner-drain-tiny.log` | «Low-level Miri != installed Box»: не проходит через `Box::drop`/`GlobalAlloc`; без strict-provenance |
| 23 | то же | то же | Win (host-Miri) | Miri TB, без strict-provenance | то же | **PASS** (1 из 1, 30.18s) | [нбл] `tb-r6-owner-drain-tiny.log` | то же |
| 24 | Loom `loom_sidecar_bitmap` (3 теста, 1 `should_panic`-мутант), `loom_terminal_large` (2), `loom_terminal_owner_drain` (2, 1 `should_panic`-мутант) | alloc-core alloc-xthread, `--cfg loom --release` | Win | Loom | shadow-протокол publication/cut/credit | **PASS** (7 из 7) | [нбл] `loom-xthread.log` | Shadow-модель протокола; НЕ доказывает pointer provenance и НЕ shipping-путь |
| 25 | Loom `loom_r11_small_sidecar` (3 теста, 2 `should_panic`-мутанта) | alloc-core alloc-xthread, `--cfg loom --release` | Win | Loom | pointer publication / last pin / early leaf release | **PASS** (3 из 3) | [нбл] `loom-r11-small-sidecar.log` | то же; Acquire/Release-модель, не provenance |
| 26 | Loom остальные root (`loom_r8_maintenance_lease`, `loom_r11_registry_claim`, `loom_r11_epoch_false_full`, `loom_active_kind_index`, ...) | см. `scripts/loom.mjs` | — | Loom | — | **NOT_RUN** (мной) | [док R11 manifest]: четыре reduced Loom queue/hint-кейса PASS (`a08334a5`) и renamed root Loom model (registry claim) 1 passed (`d9c49bed`); сами эти модели мной не запускались | — |
| 27 | Miri focused R11: OOM (2 теста) и lifetime/preflight (2 теста) | features в манифесте не указаны | не указана | Miri | storage/preflight без P1 | **PASS по документу** | [док R11 manifest] задачи `232b3b36...`, `e71ae3e8...`; не перепроверено | Манифест сам: «small-harness Miri passes do not close the actual installed-Box producer-frame failure» |
| 28 | Все строки кроме Windows x86_64: Linux/macOS/aarch64/NUMA, производительность/RSS | — | — | — | — | **NOT_RUN** | — | Нет результата, нет cost-измерения в этой фазе |

Агрегат без «зелёного»: из 28 строк installed-Sefer PASS есть только на `narrow` (4,5,6), единственная строка с настоящей паузой внутри `Box::drop` (1,2) — FAIL в обеих моделях; W+1/W+2 PASS относятся к System, не к Sefer.

## 5. Изменение harness и его причина

Оракул не сдвинулся. Но условие (а) раньше проверялось лишь косвенно (по месту падения), поэтому в `tests/support/r8_global_box_witness.rs::paused_terminal_owner_retirement` добавлены два `assert!` на read-only оракул `RoutePin::pending_for_test`:

1. после создания pin и до `Gate::arm`: `!pin.pending_for_test(ptr)` («a live Box has no pending publication») — доказывает, что оракул различает состояния;
2. после `Gate::wait_for_publication()`, до `trim_current_thread`: `pin.pending_for_test(ptr)` («pause must follow the terminal publication») — прямая проверка (а).

Контрфактуал: если бы пауза срабатывала до terminal RMW, assert 2 дал бы panic раньше UB; assert 1 фиксирует, что до публикации значение `false`. Оба прошли в SB и TB; результат (b) не изменился (то же место, тот же стек). `pending_for_test` читает только независимый sidecar-бит; к телу Box и root-аллокации не обращается. Вне `cfg(miri)` код не компилируется (witness `paused_*` под `#[cfg(miri)]`). `rustfmt --check --edition 2021` на `r8_global_box_witness.rs`, `miri_global_box_acceptance.rs`, `r8_global_box_provenance.rs`: чисто. Не менялись: `src/`, Cargo.toml, CI, scripts.

Не сделан (нужно правка src): прямой мутант «пауза до terminal RMW» против этого harness; контрфактуал для assert 2 подтверждён только рассуждением, не прогоном.

## 6. Progress- и cost-обещания, действующие сейчас

Выборка грепом по README.md и `docs/correctness-open-items/ACTIVE.md`; не исчерпывающий перечень всех числовых заявлений README.

**Progress / liveness**
- `README.md:62-64`: «This alone does **not** start autonomous maintenance; an application depending on ownerless progress must call and handle `SeferAlloc::start_maintenance()` during initialization.»
- `README.md:118-122`: «Explicit, fallible `SeferAlloc::start_maintenance()` now offers autonomous ownerless sweeps. It is **not** started by allocation, deallocation or `SeferAlloc::new()`; callers needing that guarantee must handle `MaintenanceStartError` and confirm success. The worker is process-lifetime, with no shutdown API.»
- `README.md:839-842`: «Owner sweeps exchange each covered bitmap word once. `trim_current_thread()` and TLS teardown use that finite sweep; after successful `start_maintenance()`, a fair worker also visits ownerless heaps. No ring-capacity retry or intrusive spill remains.»
- `README.md:838-839`: «the foreign publisher never reads the header or writes block slack.»
- `README.md:1413` (`production` row): «Intended long-running bundle; terminal-sidecar acceptance and new performance gates are still pending. Explicit `start_maintenance()` is required for autonomous ownerless progress.»
- `README.md:1605-1611`: «Terminal-sidecar acceptance is pending. ... it has not earned a release GO or fresh latency/RSS verdict. `start_maintenance()` must be called outside allocator callbacks and handled as a `Result`; it starts a process-lifetime worker with no shutdown, no reclamation deadline, and no guarantee of physical OS return while live allocations or retention policy prevent it.»
- `docs/correctness-open-items/ACTIVE.md:164` (card 162, Current-number-or-verdict): «Cold trim/TLS teardown use finite owner sweeps. Successful explicit `SeferAlloc::start_maintenance()` is required for its autonomous worker guarantee.» и «actual owner `Node::write_next` overlaps a weakly protected real `Box::drop` frame in task `2ad6e542-...`» (Status: OPEN, P1 blocker).
- `ACTIVE.md:165` (Next trigger): «parent acceptance of actual `GlobalAlloc` Box/narrow-reborrow paths, paused-producer strict trim, last free after owner exit without further calls, fallback, startup failure/retry and worker-failure policy, Miri/Loom, feature/OS matrix and measured RSS/latency. Do not close from source inspection or owner-only tests.»

**Cost**
- `ACTIVE.md:164`: «R11 storage requests 41,984 base bytes plus 256 per mixed leaf on 64-bit targets; all-mixed 304,128 exceeds old 294,912, with no RSS/CPU claim.»
- `README.md:114-123`: «Measured RSS win: **128.0 MiB during idle** ... [`R31_10`] ... These older RSS numbers predate the terminal-sidecar cutover and are not fresh performance evidence for it.» (явно помечено устаревшим).
- `README.md:816-817`: «`AllocBitmap` (1 bit per `MIN_BLOCK` slot, the O(1) double-free guard). Foreign-free sidecars are independent System allocations outside the payload.»
- `README.md:1198-1201`: «`SeferAlloc` overtakes `mimalloc` at T >= 2 on both workloads ... Single-thread (T = 1) `mimalloc` leads.» (старое измерение, до cutover; README:122 оговаривает «older RSS numbers»).
- `README.md:1615`: «Single-thread small-class hot path is ~1.2-2x behind `mimalloc`.»
- `README.md:227-228`: «the 256 MiB budget bounds retention to ~248 MiB PER HEAP (vs ~432 MiB/heap unbounded for `Default`)».
- Фактическая граница на момент записки: ни одно из cost-обещаний выше не перепроверялось в этой фазе (измерений CPU/RSS/latency нет; шаг 6 плана).

## 7. Границы охвата и что не подтверждено

- Только Windows x86_64 (MSVC), toolchain выше; Miri на host-target Windows. CI-строки Linux/macOS (`ci.yml:2020-2040`, `:2801-2845`, `:2929-2955`, `:3010-3030`) мной не исполнялись.
- Miri SB/TB и Loom: модели experimental/shadow; Loom не доказывает provenance; Miri проверяет один исполняемый путь.
- Использованы две версии harness для `paused`/`narrow` (до/после §5), результат (красный в обеих моделях, narrow зелёный) совпал. W-логи относятся к harness без изменений.
- Прогоны единичные (одна итерация каждой клетки, не повторялись); флейки не исследовались: определённость детерминированного Miri (одинаковые tag `<1479>/<1414>` между прогонами) косвенно подтверждает воспроизводимость места падения, но не является статистикой.
- Cost/progress-обещания не измерены; autonomous-progress (worker, owner exit без новых вызовов) в этой фазе не проверялся.
- W+3, shipping-shaped prototype, fallback/TLS teardown, OOM-пути под installed Box — вне Фазы 0.
- Реестр обязательств: черновик `docs/design/2026-10-01-proof-obligation-registry-draft.md`.
