# Ревью `src/` — раунд 16 (oxx)

## 1. Охват, инвентарь, ограничения, вердикт

**База:** `main` @ `6a0d47f62b14eb724e027ab37054ad16037aa185` (`docs(process): update R15 manifest after cross-target follow-up`). Это шестнадцатый раунд **обзоров `src/`**, не исторический perf-раунд R16 (`docs/checkpoints/r16-complete.md`) и не R34-нумерация. Номер проверен: последний отчёт серии — `docs/reviews/2026-10-07-src-review-xs-round-15.md`, последний манифест — `docs/perf/round-manifests/SRC_REVIEW_R15_MANIFEST.md`; файлов с большим номером нет ни в `docs/reviews/`, ни в `docs/perf/round-manifests/`, ни в ветках (`git branch -a`).

**Ревьюер:** oxx (Claude Opus 5.5, effort=max), один контекст, без суб-агентов, в изолированном git worktree; исходники не правились (временные мутанты восстановлены `git checkout --`, временный witness-тест удалён до коммита).

**Инвентарь (пересчитан скриптом, не скопирован):** `git diff b1a1e4f9..6a0d47f6 -- src/` пуст, поэтому `src/` побайтно совпадает с базой R15. **168 Rust-файлов, 43 317 физических строк; 17 034 непустых строк, не начинающихся с `//`; 24 322 строк-комментариев `//`/`///`/`//!`; 1 961 пустая.** По группам: `src/alloc_core/` 88 файлов / 24 814 строк; `src/registry/` 46 / 10 413; `src/global/` 18 / 3 807; `src/concurrent/` 14 / 3 536; `src/lib.rs` + `src/kani_proofs.rs` 2 / 747. Unsafe-инвентарь командой из CLAUDE.md (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`): в `src/` **21** модульный (tier 1) и **83** item-уровня (tier 2) в 30 файлах; в `crates/` 6 + 20. Метод подсчёта — приложение C.

**Метод.** Сначала обязательное чтение обоих индексов открытых пунктов (§5), отчётов R12–R15 и их манифестов. Затем три слоя по `src/`:

1. **Полное построчное чтение** (код и комментарии): `alloc_core/platform/os.rs`, `alloc_core/alloc_core/{bootstrap,lifecycle,sidecar_drain}.rs`, `alloc_core/segment/segment_table/{route_slots,segment_table_impl}.rs`, `alloc_core/segment/remote_bitmap/{mod,bitmap_cut,bitmap_record,bitmap_scan,sidecar_bitmap}.rs`, `.../sidecar_bitmap/leaf_classes.rs`, `alloc_core/small/alloc_core_small_reclaim.rs`, `alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs`, `alloc_core/platform/size_classes.rs`, вся `registry/segment_route/` (13 файлов), `registry/heap_core_xthread/*`, `tests/r14_sidecar_owner_capability_negative.rs` и его фикстуры.
2. **Кодовое чтение без прозы** (все строки кода, комментарии — первой строкой блока плюс строки с SAFETY/ordering/panic-ключами; инструмент в приложении C): `registry/heap_core/{core,alloc/hot,free/dealloc,free/dealloc_own_base,free/realloc}.rs`, `alloc_core/alloc_core/mem/realloc_fastpath.rs`, `global/tls_heap.rs`, `registry/heap_registry/claim.rs`, `lib.rs` и все `mod.rs`; выборочно по диапазонам — `alloc_core/large/alloc_core_large.rs` (120–480, 740–835), `global/fallback.rs` (160–215, 536–575), `concurrent/epoch/{epoch_region,hand}.rs` (Drop, insert/remove, Send/Sync), `concurrent/lock_free/lock_free_region.rs` (insert/remove), `concurrent/sharded/sharded_region.rs` (claim/bind/prune), `registry/heap_core/state/{ownership,tcache_flush}.rs`, `global/sefer_alloc/{mod,diag}.rs` (no-panic контракт, trim).
3. **Скрининг всех 168 файлов** grep-запросами с последующим чтением каждого попадания: релизные `expect/unwrap/panic!/unreachable!/assert!` вне `debug_assert`; пары `Segment::reserve*`/`mem::forget`/`release_segment`; битовые маски `1u64 << …`; остатки снятого ring/overflow-протокола (`RemoteFreeRing`, `HeapOverflow`, `dbg_push_to_ring`).

**Не прочитаны вглубь (только скрининг):** `alloc_core/platform/{node,sidecar,numa,sidecar_stats}.rs`, `alloc_core/large/{large_cache_extended,alloc_core_large_cache,alloc_core_large_cache_eviction}.rs` вне названных диапазонов, `alloc_core/small/alloc_core_small/{alloc_core_small_impl,find_segment,reserve,directory}.rs` вне названных строк, `alloc_core_small_magazine.rs`, `segment_directory_impl.rs`, `segment_header/*`, `config/*`, все `*_diag*.rs`, `global/exact_object/*`, `global/maintenance_service.rs`, `registry/bootstrap/*`, `registry/heap_slot.rs`, `concurrent/pinning.rs`, `lock_free_page_table.rs`, `kani_proofs.rs`. Утверждения «не найдено» ниже относятся только к прочитанному.

**Исполнено** (детали — §6 и приложения A/B/D): временный witness-тест (4/4 passed: R15-01, R15-02 и структурный факт гипотезы H1); R14-регрессии на неизменённом дереве (9/9 passed) и против трёх контрфактуальных мутантов (каждый мутант пойман); `no_stale_doc_references` и `verify-commit-prefixes` после правок индексов; только чтение статусов GitHub Actions через `gh`. Среда: rustc 1.97.0 (`2d8144b78`), cargo 1.97.0, host `x86_64-pc-windows-msvc`, debug-профиль, `RUSTC_WRAPPER=` пустой, `--locked`, worktree-local `CARGO_TARGET_DIR`.

**Не запускалось:** Miri, Loom, Kani, TSan/ASan, release-профиль, MSRV-тулчейн, полный `npm run check`, clippy/rustfmt/rustdoc, feature-powerset, бенчмарки/iai, NUMA, не-Windows хосты, удалённый CI (только чтение уже завершённых прогонов).

**Вердикт:** **три новые находки, все P4** (R16-01 — неполный no-panic контракт и релизные `expect` на пути `GlobalAlloc`; R16-02 — CI-гейт R15 молча снял API-проверку R14-01 с нативного arm64; R16-03 — латентная u64-маска классов без статической границы), **три гипотезы оптимизации** (без измерений), одно закрытие наследуемого пункта (correctness 22 — механизм снят, как 157 в R15). R15-01 и R15-02 **воспроизведены собственным исполненным witness** и остаются открытыми (172/173). Новых P0/P1/P2/P3 не найдено **в прочитанной области и прочитанным методом**; это не доказательство отсутствия ошибок, не release GO и не perf GO. Принятый P1-box (164) и Miri-остаток 171 не тронуты и не переклассифицированы.

## 2. Находки

Шкала как в R15 §2: P3 — ограниченный дефект корректности/ресурса/диагностики без нового production memory-safety эксплойта; P4 — вводящий в заблуждение контракт, пробел покрытия или поддерживаемость без установленного runtime-сбоя от одного текста. SOURCE-CONFIRMED — противоречие или путь установлены по исходнику, без исполнения; «исполненный witness» — наблюдён запуском.

| ID | Severity | Класс доказательства | Достижимость | Суть |
|---|---|---|---|---|
| R16-01 | P4 | SOURCE-CONFIRMED (трасса вызовов + `git log -S`) | `production` и любой `alloc-global + fastbin`; публичный `SeferAlloc::trim_current_thread`, `GlobalAlloc::dealloc`/`alloc` | Два релизных `.expect("magazine slot belongs to a live segment")` на путях `GlobalAlloc` не входят в «четыре release tripwire» no-panic контракта и противоречат соглашению abort-not-panic |
| R16-02 | P4 | исполнено в CI (логи двух прогонов) + SOURCE-CONFIRMED | тестовая инфраструктура; затрагивает нативный macOS arm64 job | `target_arch = "x86_64"` в `20b7c443` — неточная замена условия «host == target»: R14-01 API-проверки перестали исполняться на нативном arm64, хотя модульный doc утверждает обратное |
| R16-03 | P4 | SOURCE-CONFIRMED | любой `alloc-global + alloc-xthread`; латентно при `SMALL_CLASS_COUNT > 64` | Маска `changed_classes: u64` строится `1u64 << record.class` без статической проверки `SMALL_CLASS_COUNT <= 64` |

### R16-01 — no-panic контракт перечисляет «четыре» release tripwire, а на путях `GlobalAlloc` их шесть

**Места.** `src/registry/heap_core/free/dealloc_own_base.rs:482–491` (цикл снятия magazine-битов перед overflow-flush, `.expect` на :486); `src/registry/heap_core/state/tcache_flush.rs:76–95` (`flush_all_tcache`, `.expect` на :90). Контракт: `src/global/sefer_alloc/mod.rs:95–139` («Four release-surviving invariant tripwires … All four live in the large-cache slot take/set helpers») и :179–198 («no path reachable from a `GlobalAlloc` method … panics», catch-all про `expect`s on internally-derived indices). Пин-тест: `tests/no_panic_doc_accuracy.rs:70` (`four_invariant_tripwires_pinned_by_message`) закрепляет только четыре сообщения в `alloc_core_large_cache.rs`.

**Трасса.**
- `GlobalAlloc::dealloc` → `tls_heap::current_for_dealloc` (`Own`) → `HeapCore::dealloc` (`free/dealloc.rs:219–224`) → `dealloc_routing` (`heap_core_xthread/routing.rs:21–31`) → `dealloc_own_thread_with_base` (`dealloc_own_base.rs:341`) → ветка переполнения magazine при `cnt == TCACHE_CAP` (классы до 4 KiB, у которых `FREE_PARK_CAP == TCACHE_CAP`) → `self.core.canonical_block_of(flushed).expect(…)` для каждого из `FLUSH_N` слотов.
- `flush_all_tcache` вызывается из `trim_cold_retention` (`state/ownership.rs:140–156`), а та — из `trim_for_recycle` (`:108–115`) и `background_maintenance_step` (`:119–138`). `trim_for_recycle` достижим из публичного `SeferAlloc::trim_current_thread` (`global/sefer_alloc/diag.rs:255–265`), из TLS-деструктора `AbandonGuard::drop` (`global/tls_heap.rs:197–225`) и из повторного захвата слота в `HeapRegistry::claim_impl` (`registry/heap_registry/claim.rs:317`), то есть из холодного bind-пути `GlobalAlloc::alloc`.

**Наблюдено.** Обе строки появились коммитом `2da9a941` (2026-09-29, `fix: keep HeapCore own-path pointers allocator-derived`; `git log -S'magazine slot belongs to a live segment' -- src/`) — позже перечисления «четырёх» и пин-теста; перечисление и тест не обновлены. Остальной код на тех же owner-only путях оформляет нарушение инварианта как `std::process::abort()` (например, `route_slots.rs:95–96,111–113,137–139`, `small_sidecar.rs:39–42`, `sidecar_drain.rs:128–136`), и сам контракт объясняет почему: при панике хук и boxed payload снова входят в аллокатор посреди операции (R2-08, `sefer_alloc/mod.rs:157–177`). Здесь это была бы середина overflow-flush: часть magazine-битов уже снята, `flush_class` ещё не вызван, `count` не уменьшен.

**Выведено.** Для валидного использования ветка недостижима: блок в magazine считается живым кредитом (D1), поэтому его сегмент не может быть снят, и `canonical_block_of` находит корень. Срабатывание возможно только после уже случившейся порчи метаданных. Поэтому P4: дефект контракта и hardening-несогласованность, а не runtime-сбой при корректной работе.

**Рекомендация.** Заменить оба `expect` на `unwrap_or_else(|| std::process::abort())` (или на маскирование корня, как в R12-02 `clear_magazine_on_issue`, `alloc/hot.rs:43–58`, — см. гипотезу H2), и в том же изменении поправить перечисление в `sefer_alloc/mod.rs` и пин-тест. **Oracle исправления:** расширить `tests/no_panic_doc_accuracy.rs` так, чтобы он перечислял релизные `expect(`/`panic!`/`unreachable!` во всех файлах, достижимых из `GlobalAlloc`, а не четыре строки одного файла. На текущем дереве такой тест красный (находит два неучтённых `expect`), после исправления — зелёный. Поведенческий oracle (форсировать порчу magazine-слота) не рекомендуется: он сам потребовал бы небезопасного hook.

### R16-02 — x86_64-гейт R15 снял R14-01 проверку с нативного arm64 и оставил устаревшие правила пропуска

**Места.** `tests/r14_sidecar_owner_capability_negative.rs:5–7` (утверждение «Native CI retains these API-visibility checks»), `:29–33` (`#![cfg(all(feature = "alloc-global", feature = "internals", target_arch = "x86_64"))]`, коммиты `20b7c443` + `b24a181f`), `:14–16` (модульный doc: пропускаются «only missing/private `registry` import errors»), `:176–198` (фактический фильтр после `247a52ad`/`c82c88c5`/`749dbfa9` пропускает ещё E0460 и E0463), `:209–216` (единственная проверка — «хотя бы один совместимый кандидат»); `tests/compile_fail/r14_positive_probe/src/main.rs:1–7` (то же устаревшее правило «E0432/E0433/E0603»).

**Наблюдено (исполнено в CI, только чтение).** Job `test macos (production)` идёт на образе `macos-26-arm64`, host `aarch64-apple-darwin`, шаг `cargo test --features "production internals"` без `--target`.
- Run `37644289617` (SHA `714ea5b7`, до гейта), job `112870910476`: `Running tests/r14_sidecar_owner_capability_negative.rs` → `running 3 tests` → все три `ok`.
- Run `37704825252` (SHA `6a0d47f6`, после гейта), job `113076568267`: тот же бинарник → `test result: ok. 0 passed`.

Значит, нативный arm64, где host-`rustc` совпадает с target, исполнял и проходил проверки, а гейт их снял. Реальная причина сбоя — несовпадение host и target (`cross test --target aarch64-unknown-linux-gnu`). Архитектура x86_64 — лишь её приближение: оно исключает лишний нативный случай и не исключает, например, x86_64-цель, отличную от host, под `--target`.

**Дополнительно (SOURCE-CONFIRMED).** Харнесс проверяет только, что **хотя бы один** rlib-кандидат совместим (`:209–216`), но не то, что совместима **текущая** сборка, к которой линкуется сам тестовый бинарник. С фильтром E0463 (корректным для устаревших кандидатов: под `--target` proc-macro `rustversion` от `arc-swap` лежит в host-deps, а не в target-deps) текущая сборка с `experimental` под `--target <host>` была бы пропущена, и негатив проверялся бы только против устаревшего rlib. Ни одна текущая строка CI так не собирает (в `multi-arch` x86_64-строки с `--target` идут без `experimental` вместе с `internals`). Поэтому это остаток, а не исполненный сбой.

**Рекомендация.** Заменить архитектурный гейт проверкой «host == target» во время выполнения (сравнить `host:` из `rustc -vV` с целевой тройкой, переданной сборкой через env, либо `rustc --print cfg` с `cfg!` бинарника; при несовпадении — явный пропуск с сообщением) и вернуть нативный arm64. Обновить модульный doc и doc позитивной пробы до фактических правил пропуска (E0432/E0433/E0460/E0463/E0603). Добавить проверку, что совместимым признан кандидат текущей сборки. **Oracle:** лог macOS job снова показывает `running 3 tests`; мутант R14-01 (`pub fn prepare`) роняет тест на arm64 так же, как на x86_64 (приложение B, M1).

### R16-03 — `changed_classes: u64` без статической границы числа классов

**Места.** `src/alloc_core/alloc_core/sidecar_drain.rs:126,139` (bounded drain) и `:211,220` (полный drain), `src/alloc_core/small/alloc_core_small/find_segment.rs:142,156` (drain внутри поиска сегмента), потребитель `src/alloc_core/small/alloc_core_small/directory.rs:238–261` (`sync_directory_for_segment_classes`, итерация `trailing_zeros`). Единственная статическая граница числа классов — `SMALL_CLASS_COUNT < u8::MAX` (`remote_bitmap/sidecar_bitmap.rs:29`).

**Наблюдено.** `changed_classes |= 1u64 << record.class`. Сейчас максимум `SMALL_CLASS_COUNT` — 58 (`medium-classes-wide`, `platform/size_classes.rs:36–39,159–162`), поэтому дефекта сегодня нет.

**Выведено (латентно).** Рост лестницы классов выше 64 (например, новый medium-список) приведёт к переполнению сдвига: паника в debug, а в release сдвиг маскируется (`1 << (class % 64)`). Тогда sync директории обновит бит чужого класса и пропустит настоящий, а условие `changed_classes != 0` для освобождения кредита сегмента останется истинным. Ошибка тихая и не ловится существующими const-assert'ами.

**Рекомендация.** Один `const _: () = assert!(SMALL_CLASS_COUNT <= u64::BITS as usize)` рядом с идиомой (или маска `[u64; N]`). **Oracle:** const-assert ломает сборку конфигурации с 65+ классами; сегодняшние конфигурации собираются без изменений.

### Кандидаты, сознательно не повышенные до находок

- **`SmallSidecar::scan` — `pub &self` с doc «Owner-only»** (`registry/segment_route/small_sidecar.rs:153–158`), доступен через `RouteRegistration::small_sidecar()` под `internals`, как и закрытые R14-01 `prepare/issue`. Конкурентные сканы безопасны: `BitmapScan::next_cut` атомарно забирает слово целиком (`swap(0, AcqRel)`, `bitmap_scan.rs:66–79`), каждый бит достаётся ровно одному сканеру, внутреннего состояния сайдкара для порчи нет. Худший исход — делёж detached-битов между вызывающими в пределах их собственной регистрации. Production-регистрации лежат в owner-only `RouteSlots`, а `RoutePin` sidecar не отдаёт (`pin.rs:6–8`). Не находка; отмечено в §3.
- **Переполнение в `alloc_large`** (`needed.div_ceil(SEGMENT) * SEGMENT`, `alloc_core_large.rs:193–196`) при `size <= isize::MAX` невозможно. Путь promotion (`free/realloc.rs:589`) зовёт `alloc_large` с `new_size` до валидации `Layout`, но переполнение даёт ровно `0` → резервирование отклоняется → null, и только при нарушении контракта `realloc`.
- **Выравнивание при повторном использовании Large-кэша.** Biased-резервации (`align >= SEGMENT`, корень лишь page-aligned) в кэш не попадают (`mem/mem_impl.rs:363`, `alloc_core_large.rs:767`). Повторно используемые корни SEGMENT-aligned, поэтому `root + align_up(header, align)` выровнен.
- **Порча `writer mutex` паникующим `T::drop`** в `EpochRegion::insert/remove` и `LockFreeRegion::insert/remove` не найдена: значения либо возвращаются вызывающему (`Arc<T>`), либо уходят в `defer_destroy` до захвата мьютекса (`epoch_region.rs:491–511`), так что пользовательский `Drop` под мьютексом не выполняется. Остаток R15-01 (паника в `Drop` самого региона) — отдельно, item 172.
- **Учёт R15-02 на других путях резервирования.** Все пары `Segment::reserve*` → `mem::forget` → `release_segment` согласованы (`alloc_core_large.rs:648/667`, `small/alloc_core_small/reserve.rs:243/253`, `decomp_hooks.rs:70–77`, `numa.rs:155–158`). Внутренние `None`-выходы `reserve_biased` (`os.rs:189–208`) роняют резервирование до инкремента счётчика, поэтому согласованы. R15-02 остаётся единственным случаем — первичный `attach_owner`.

## 3. Ревью недавней работы

| Объект | Что сделано | Вердикт |
|---|---|---|
| R13 `d6417c6c` | Закрыт и проверен R14 (`2026-10-06-src-review-sol-round-14.md` §3). Сам подтвердил только маркер R13-05: `ShardGuard { _unique: PhantomData<&'a mut T> }` (`shard_lock.rs:33–38`). | Остальное не перепроверял; R13-тесты не запускал |
| R14 `62b16ce9` (src: `route_slots.rs`, `small_sidecar.rs`, `registration.rs`, `sharded_region.rs`, doc-правки в `reserved_small_segment.rs`, `alloc_core_large_cache_eviction.rs`) | Прочитан весь src-диф (478 строк). Запущены 9 R14-тестов: 9/9 passed. Три контрфактуальных мутанта, каждый пойман: M1 `pub fn prepare` роняет только prepare-фикстуру (issue запечатан независимо); M2 без раннего выхода в `prune_dead_claims` даёт `78 vs 0` проверок; M3 с `router_intact = true` роняет 2 late-TLS теста (приложение B). | R14-01…04 **подтверждены и не вакуумны**. Остатки: `scan` остаётся `pub &self` (безвредно, §2); глобальный death-hint (R15 гипотеза 4) в силе |
| `247a52ad` (E0433 wording) | Точное совпадение двух формулировок rustc. | OK |
| `c82c88c5` (E0460) | Пропуск кандидатов с несовпавшим SVH зависимостей: так выглядят устаревшие rlib, чей `aligned_vmem` перезаписан той же хэш-меткой файла. | OK для устаревших кандидатов |
| `749dbfa9` (E0463) | Механизм установлен по CI-логу run `37644289617`, job `test (x86_64-unknown-linux-gnu)`: под `--target` proc-macro `rustversion` (через `arc-swap`) лежит в host-deps, а кандидат из шага `--features experimental` его требует. Пропуск корректен. | OK; ослабление ограничено и описано в R16-02 |
| `20b7c443` / `b24a181f` | Гейт по `target_arch`. | **R16-02**: снимает нативный arm64; doc не соответствует |
| Документы R15 (`02197768`, `6b407dd8`, `024c3e8f`, манифесты) | Полная проверка — в разделе «Независимая проверка (oxx, 2026-10-08)», который третий коммит этого раунда дописывает в конец отчёта R15. | Есть фактические неточности: число карточек ACTIVE исправлено этим коммитом, остальное — третьим |

Состояние CI (только чтение): run `37704825252` на `6a0d47f6` — success (50 jobs success, 4 skipped); Kani `37704825276` — success. Runs `37644289617` (`714ea5b7`, 2 failed: aarch64 E0461 и x86_64 E0463) и `37681398463` (`d5fb531b`, 1 failed: aarch64 E0461) — красные. У коммитов R14 (`7be55396`, `62b16ce9`, `b1a1e4f9`) нет собственных прогонов: первый прогон, включавший их, — `37644289617`; последний зелёный до R15 — `37505286215` на `d6417c6c`.

## 4. Гипотезы оптимизации и поддерживаемости

**Все пункты не измерены.** Это не speedup, не RSS/latency-результат, не promotion GO и не повод переоткрывать отвергнутые эксперименты. Для каждой гипотезы указаны слой, oracle и жертва; судья обязан соблюдать правила R26-4/R30-8/entry-point/regime из CLAUDE.md.

- **H1 — Large-realloc сжатие всегда переезжает** (perf item 83). `realloc_inplace_fast_path_known_base` разрешает in-place только рост (`realloc_fastpath.rs:291`: `if new_eff >= old_eff`). Любое сжатие Large идёт по move-ветке `HeapCore::realloc` (`free/realloc.rs:336–407`): новое резервирование, копия `min(old, new)`, освобождение старого. Исполненный структурный факт (приложение A, W3): `realloc` 8 MiB → 6 MiB вернул новый адрес (`moved=true`), то есть скопировал 6 MiB. Идея: оставлять блок на месте при умеренном сжатии и, по политике, decommit'ить хвост. Слой — `HeapCore::realloc` / `GlobalAlloc::realloc`, судить на real `#[global_allocator]`. Oracle активации — `RELOC_INPLACE_LARGE_CALLS` против числа move-переездов. Жертва — `Vec::shrink_to_fit`/`truncate` больших буферов. Обязательна парная RSS-ось: in-place сжатие удерживает память, если хвост не возвращается.
- **H2 — overflow-flush и `flush_all_tcache` разрешают корень хэш-поиском** (perf item 84). На каждый из `FLUSH_N` слотов — `canonical_block_of` (Tier-1/Tier-2 lookup) плюс `.expect` (R16-01). Тогда как выдача из magazine после R12-02 (`7232598b`, `hot.rs:43–58`) использует маску сегмента с `debug_assert` канонического корня. Указатели в magazine происходят от корня аллокатора, так что маска сохраняет provenance — тот же аргумент, что в R12-02. Слой — `HeapCore` dealloc overflow и trim. Oracle — iai churn-бенчи с подсчётом overflow-событий (путь активирован), kill-gate ±10 Ir. Не путать с исчерпанным per-block регионом perf item 1 (`flush_class`, coalescing битов): это предшествующий цикл поиска корня, который R24–R28 не трогали.
- **H3 — `reserve_biased` коммитит первую страницу в начале резервирования**, а не в полезном окне (`os.rs:178`: `try_reserve_aligned_lazy(raw_len, SEGMENT, page_size())`, затем `commit_range(root, 0, committed)` на :203). На Windows это одна закоммиченная, но неиспользуемая страница на каждую Large-аллокацию с `align >= SEGMENT`, если `root_offset >= page`. Проверка `committed > useful` (:197) выполняется уже после резервирования, хотя её можно сделать заранее. Эффект — максимум одна страница; заводится только как пометка в H1/§4, не отдельным perf-пунктом.
- **Гипотезы R15, перепроверенные на базе:** повторный `guard.find(key)` в `RouteDirectory::lookup` (`directory.rs:884`); повторный `canonical_block_of` в `dealloc_own_thread_with_base` (`dealloc_own_base.rs:347`) после `dealloc_routing` (`routing.rs:22`); копия слов директории (`os.rs:706–724`); перекрёстный death-hint шардов. Все четыре стоят как в R15, не измерены. Полный O(L)-обход активных Large-маршрутов на каждый скалярный Large-alloc (`alloc/hot.rs:249–254`) уже принадлежит perf item 78(c) с его correctness-предусловием (нельзя потерять приостановленного публикатора) — не новый пункт.
- **Поддерживаемость:** `dec_live_and_maybe_decommit` больше не уменьшает счётчик (`alloc_core_small_pool_impl.rs:84–119`), так что имя `dec_live_*` вводит в заблуждение. `dbg_push_to_ring` удалён, но intra-doc ссылки на него как на «`unsafe fn` in this file» живы (`header_diag.rs:138`, `table_diag.rs:260`); строгий rustdoc их не ловит, потому что элементы `#[doc(hidden)]`. Doc `SeferAlloc::trim_current_thread` всё ещё противопоставляет себя «legacy foreign `dealloc` ring/overflow/stack» (`global/sefer_alloc/diag.rs:252–254`). Это примеры уже открытого долга 154, а не новый пункт.

## 5. Решения по прежним открытым пунктам (оба индекса)

Прочитаны целиком `docs/perf/OPEN_ITEMS.md` (header, все карточки `[A]`, `[D]`, `[L]`, Recently resolved; длинные исторические нарративы `[D]/[L]` — по их current-state карточкам), `docs/CORRECTNESS_OPEN_ITEMS.md` (вместе с lookup-таблицей), `docs/correctness-open-items/ACTIVE.md` и все девять `TRACKED_*.md`. У `TRACKED_publish_readiness.md` и хвоста `TRACKED_process_record.md` прочитаны заголовки и Status-блоки, их нарративы — нет: они вне root `src/`. `RESOLVED.md` и `ARCHIVE.md` — по требованию.

Если строка не указывает иное, каждый ID ниже получает **LEAVE — текущий status, verdict, next trigger и историческая запись без изменений**. Причина одна для всех: это обзор исходников, а не исполнение аппаратных, publication, Miri/Loom/Kani, CI-coverage или performance gate, которых требуют их триггеры.

| Файл карточек | ID | Решение |
|---|---|---|
| `docs/perf/OPEN_ITEMS.md`, текущие карточки | 1, 13, 81, 40, 41, 2, 3, 4, 5, 6, 14, 15, 25, 26, 28, 51, 29, 30, 31, 33, 38, 42, 48, 7, 8, 9, 10, 11, 12, 50, 52, 53, 54, 49, 44, 16, 17, 18, 19, 20, 21, 22, 23, 24, 34, 35, 27, 36, 37, 45, 39, 43, 47, 55, 56, 57, 58, 59, 60, 61, 62, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 78, 82 | LEAVE (74 карточки; список выведен из заголовков файла и совпал с R15). 81 INCONCLUSIVE, 82 НЕ СЕЙЧАС, 72 и 78 — со своими триггерами. Закрытые карточки, оставленные в тирах (7, 10, 15 и CLOSED-карточка 56), — без изменений |
| `docs/perf/OPEN_ITEMS.md`, Recently resolved | 79, 80 | LEAVE CLOSED (archive pointers) |
| `docs/perf/OPEN_ITEMS.md`, новые | **83** (H1), **84** (H2) | заведены `[L]`, «hypothesis, not measured» |
| `ACTIVE.md` | 1, 2, 62, 11, 13, 162, 163 | LEAVE |
| `ACTIVE.md` | 172, 173 | LEAVE OPEN; R15-01/R15-02 воспроизведены собственным witness (дата-апдейт — в коммите проверки R15) |
| `ACTIVE.md`, новый | **174** (R16-01) | заведён `[A]` |
| `TRACKED_hook_safety.md` | 5, 7, 8, 9 | LEAVE |
| `TRACKED_verification_coverage.md` | 17, 18, 41, 61, 84, 167, 171 | LEAVE (171 не перезапускался) |
| `TRACKED_platform_contracts.md` | 6, 26, 43, 44, 47, 48, 52, 53, 58, 59, 59a, 59b, 60, 152 | LEAVE |
| `TRACKED_ci_gate_coverage.md` | 19, 25, 50, 51, 54, 55, 64, 65, 70, 72, 73, 74, 76, 80, 82, 87, 88, 92, 95, 107, 140, 151, 156 | LEAVE |
| `TRACKED_ci_gate_coverage.md`, новый | **175** (R16-02) | заведён `[T]` |
| `TRACKED_test_flakiness.md` | 12, 14, 63, 69, 96, 143, 145, 146, 147, 150, 153 | LEAVE |
| `TRACKED_correctness_residuals.md` | 16, 23, 66, 155, 164, 165, 166 | LEAVE (23: `InitStateGuard` жив, `fallback.rs:536–572`) |
| `TRACKED_correctness_residuals.md` → `RESOLVED.md` | **22** | **CLOSED/SUPERSEDED**: `RemoteFreeRing`/`DrainHeadPublish` в `src/` нет (только упоминания в комментариях: `lib.rs:120`, `heap_core/alloc/hot.rs:659` и др.). Текущий drain забирает слово целиком до reclaim (`bitmap_scan.rs:66–79`), а `reclaim_sidecar_record` только abort'ит (`alloc_core_small_reclaim.rs:21–68`). Повтора in-flight элемента при unwind больше нет: остаток — at-most-once (потеря кредита), недостижимый без паники, которой на этом пути нет. Логика та же, что у закрытия 157 в R15 |
| `TRACKED_publish_readiness.md` | 24, 27, 28, 29, 46, 85, 90, 91, 93, 97, 94, 98, 99, 100–106, 108–139, 141, 142, 144 | LEAVE (companion-crates, вне `src/`) |
| `TRACKED_process_record.md` | 10, 20, 21, 67, 68, 78, 79, 81, 83, 86, 89 | LEAVE |
| `TRACKED_misc.md` | 45, 49, 158, 159, 160, 161 | LEAVE |
| `TRACKED_misc.md` | 154 | LEAVE OPEN, R16 evidence update: устаревшие doc-ссылки на удалённый `dbg_push_to_ring`; `#[allow(dead_code)]` на живом production-модуле `segment_route` (`registry/mod.rs:50–54`); doc `trim_current_thread` про «legacy foreign `dealloc` ring/overflow/stack» (`sefer_alloc/diag.rs`) |
| `TRACKED_misc.md`, новый | **176** (R16-03) | заведён `[T]` |
| `RESOLVED.md` | 157, 168, 169, 170 и прочие | LEAVE CLOSED |

Перепись после раунда: ACTIVE — **10** нумерованных карточек (1, 2, 62, 11, 13, 162, 163, 172, 173, 174; R15 записал «8», хотя было 9). TRACKED — 141 нумерованная запись (140 − 22 + 175 + 176); строк lookup-таблицы на `TRACKED_*.md` — 141. Perf — 76 заголовков в разделе Open items.

## 6. Исполненная верификация и остаток

1. **Witness** (`tests/zz_r16_review_witness.rs`, временный, удалён; код — приложение A): `cargo test --locked --features "production internals bench-internals experimental" --test zz_r16_review_witness -- --test-threads=1 --nocapture` → **4 passed**. W1 control: `slot1_drops=1 slot0_drops=1`. W1 panic: `caught=true panicking_drops=1 later_live_drops=0`. W2: `refused attach: reserved_delta=1 released_delta=0`, retry `reserved_delta=1 released_delta=0`. W3: `large shrink 8MiB->6MiB: moved=true first_byte_preserved=true`.
2. **R14-регрессии** (тот же feature-набор, `--test r14_sidecar_owner_capability_negative --test r14_shard_late_tls_teardown --test r14_shard_prune_work_bound -- --test-threads=1`): на неизменённом дереве **9 passed**; против трёх мутантов (приложение B) — 5 failed / 4 passed, ровно ожидаемые. Мутанты сняты `git checkout -- src/registry/segment_route/small_sidecar.rs src/concurrent/sharded/sharded_region.rs`; `git status` после этого показывал только неотслеживаемый witness-файл.
3. **Индексные проверки перед коммитом:** `cargo test --locked --features "production internals alloc-stats bench-internals batch-api" --test no_stale_doc_references -- --test-threads=1` и `node scripts/verify-commit-prefixes.mjs` — результаты в манифесте раунда.
4. **Остаток для владельца:** исправления R16-01/02/03 и R15-01/02 (172/173) не входят в этот read-only раунд. Miri/Loom/Kani, release, MSRV, полный `npm run check` и CI для этих документов не запускались; push не выполнялся.

## Приложение A. Код и вывод временного witness

```text
//! TEMPORARY src-review round 16 witnesses (deleted before commit).
#[cfg(feature = "experimental")]
mod epoch_unwind_tail {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    struct Probe { panics: bool, drops: Arc<AtomicUsize> }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
            if self.panics { panic!("R16 witness: deliberate destructor panic"); }
        }
    }
    #[allow(deprecated)]
    fn region_with(first_panics: bool, second_panics: bool)
        -> (sefer_alloc::EpochRegion<Probe>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let region = sefer_alloc::EpochRegion::with_capacity(2);
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        // first insert -> slot 1, second -> slot 0; Drop walks 0, 1
        assert!(region.insert(Probe { panics: first_panics, drops: Arc::clone(&a) }).is_ok());
        assert!(region.insert(Probe { panics: second_panics, drops: Arc::clone(&b) }).is_ok());
        (region, a, b)
    }
    #[test] fn r15_01_control_no_panic_drops_both() { /* drop(region); assert (1, 1) */ }
    #[test] fn r15_01_panicking_slot0_skips_live_slot1() {
        let (region, later, panicking) = region_with(false, true);
        let caught = catch_unwind(AssertUnwindSafe(move || drop(region)));
        assert!(caught.is_err());
        assert_eq!(panicking.load(Ordering::SeqCst), 1);
        assert_eq!(later.load(Ordering::SeqCst), 0);
    }
}
#[cfg(all(feature = "alloc-global", feature = "internals", feature = "bench-internals"))]
mod primordial_rollback_counter {
    use sefer_alloc::alloc_core::AllocCore;
    use sefer_alloc::registry::segment_route::RouteDirectory;
    use sefer_alloc::registry::HeapRegistry;
    #[test] fn r15_02_failed_attach_counts_reserve_but_not_release() {
        let (r0, l0) = (AllocCore::dbg_segments_reserved_total(), AllocCore::dbg_segments_released_total());
        RouteDirectory::fail_next_registration_for_test();
        assert!(HeapRegistry::dbg_claim_lease().is_none());
        // reserved +1, released +0; then a retry control: +1 / +0
    }
    #[test] fn r16_large_realloc_shrink_moves() {
        // lease.core(): alloc 8 MiB (align 16), write 0xA5, realloc to 6 MiB,
        // report q != p and first byte preserved, dealloc with the new layout.
    }
}
```

```text
running 4 tests
test epoch_unwind_tail::r15_01_control_no_panic_drops_both ... R16-W1 control: slot1_drops=1 slot0_drops=1
ok
test epoch_unwind_tail::r15_01_panicking_slot0_skips_live_slot1 ...
thread '…' panicked at tests\zz_r16_review_witness.rs:20:17:
R16 witness: deliberate destructor panic
R16-W1 panic: caught=true panicking_drops=1 later_live_drops=0
ok
test primordial_rollback_counter::r15_02_failed_attach_counts_reserve_but_not_release ... R16-W2 refused attach: reserved_delta=1 released_delta=0
R16-W2 retry control: reserved_delta=1 released_delta=0
ok
test primordial_rollback_counter::r16_large_realloc_shrink_moves ... R16-W3 large shrink 8MiB->6MiB: moved=true first_byte_preserved=true
ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Тела, свёрнутые в комментарии выше, буквально следуют описанию рядом (полный файл существовал только в worktree; повторить можно по этому тексту).

## Приложение B. Контрфактуальные мутанты R14

```text
M1  src/registry/segment_route/small_sidecar.rs
-    pub(crate) fn prepare(&self, offset: u32, class: u8) -> bool {
+    pub fn prepare(&self, offset: u32, class: u8) -> bool {
M2  src/concurrent/sharded/sharded_region.rs (prune_dead_claims)
-    if death_epoch == guard.last_death_epoch_seen {
+    if false && death_epoch == guard.last_death_epoch_seen {
M3  src/concurrent/sharded/sharded_region.rs (claim_or_get_shard)
-            MY_SHARDS.try_with(|_| ()).is_ok() && ERASED_GUARD.try_with(|_| ()).is_ok();
+            true || (MY_SHARDS.try_with(|_| ()).is_ok() && ERASED_GUARD.try_with(|_| ()).is_ok());
```

```text
r14_shard_late_tls_teardown: guard_alive_after_routing_tls_death_still_falls_back FAILED
  (late insert must modulo-share shard 0 without a new exclusive token; left: 1, right: 0);
  late_tls_destructor_insert_shares_without_orphan_token FAILED (TLS AccessError at local.rs:428);
  bind_on_live_thread_still_binds_and_routes ok; late_tls_destructor_bind_is_refused_without_token ok
r14_shard_prune_work_bound: cold_binds_without_deaths_are_sweep_free FAILED (left: 78, right: 0);
  death_triggers_one_bounded_sweep_and_keeps_live_claims FAILED (left: 3, right: 2)
r14_sidecar_owner_capability_negative: shared_sidecar_prepare_is_not_callable_from_outside_the_crate FAILED
  (negative fixture exit Some(0), expected Some(1)); issue/Sync fixtures ok
```

Мутанты применены одновременно, но затрагивают независимые тестовые бинарники: каждый отказ однозначно приписан своему мутанту.

## Приложение C. Инвентарь и инструменты

- Перепись: `git ls-files src | node census.cjs --per-file` (Node; физические строки — `split(/\r?\n/)` без хвостовой пустой; «код» — непустые строки, `trim()` которых не начинается с `//`). Результат: `{"files":168,"phys":43317,"code_nonblank_not_slashslash":17034,"blank":1961,"slashslash_comment_lines":24322}`. Посрочное сравнение с приложением A отчёта R15 — все 168 строк совпали, сумма 43 317.
- Компактный просмотр: Node-скрипт печатает каждую строку кода с номером, а для каждого блока `//`-комментариев — первую строку и строки с ключами SAFETY/Ordering/panic/abort/invariant.
- Unsafe-инвентарь: 130 строк команды CLAUDE.md, из них `src/` 21 `#![…]` + 83 `#[…]` (30 файлов), `crates/` 6 + 20. Список совпадает с приложением C R15.
- Скрипты-инструменты находились в игнорируемом `target/` worktree и не коммитятся.

## Приложение D. CI-свидетельства (только чтение, `gh run view`)

| Run | SHA | Итог | Провальные jobs и диагноз |
|---|---|---|---|
| `37505286215` | `d6417c6c` | success | — |
| `37644289617` | `714ea5b7` | failure | `test (aarch64-unknown-linux-gnu)`: `couldn't find crate sefer_alloc with expected target triple x86_64-unknown-linux-gnu` (E0461); `test (x86_64-unknown-linux-gnu)`: `can't find crate for rustversion which sefer_alloc depends on` (E0463). macOS job `112870910476` (`macos-26-arm64`): r14-харнесс 3/3 ok |
| `37681398463` | `d5fb531b` | failure | только `test (aarch64-unknown-linux-gnu)`, E0461 |
| `37704825252` | `6a0d47f6` | success | 50 success, 4 skipped; macOS job `113076568267`: r14-харнесс `0 passed` |
| `37704825276` | `6a0d47f6` | success (Kani) | — |
