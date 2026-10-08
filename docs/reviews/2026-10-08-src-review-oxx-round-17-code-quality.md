# Ревью `src/` — раунд 17 (oxx) — качество кода, запахи, сопровождаемость, конвенции, точность комментариев

## 1. Охват, инвентарь, метод, база, вердикт

**База:** `main` @ `453439c123f71a98890408dd0170bd909ad9ecdf` (`docs: close correctness item 175 after green CI and record the red CI of the first push`), изолированный git worktree; `git status` перед записью отчёта чистый. Это 17-й раунд обзоров `src/`; предыдущий — `docs/reviews/2026-10-08-src-review-oxx-round-16.md` (манифест `docs/perf/round-manifests/SRC_REVIEW_R16_MANIFEST.md`). Относительно базы R16 (`6a0d47f6`) `src/` изменился в 22 файлах (+130/−122, R16 follow-up: `486f5ace`, `32cacc97`, `222e913d`, `9df9f6b8`, `c8b9344a`, `b707d196`); этот дифф прочитан целиком.

**Ревьюер:** oxx (Claude Opus 5.5, effort=max), один контекст, без суб-агентов. **Тема (один из 8 параллельных ревьюеров):** качество кода, запахи, сопровождаемость, соблюдение конвенций `CLAUDE.md`, точность комментариев, тестопригодность и `dbg_*`-поверхность, согласованность терминов.

**Режим — только чтение. Ревью проведено БЕЗ компиляции и БЕЗ исполнения кода проекта:** не запускались `cargo` (build/check/test/clippy/doc), `rustc`, Miri, Loom, Kani, бенчмарки, тесты, CI. Использовались только чтение файлов, `grep`, read-only `git` (`log`, `show`, `diff`, `rev-parse`, `status`) и node-скрипты, которые только читают файлы (`node -e`, без записи в репозиторий; временных файлов не создавалось). Поэтому каждый вывод ниже классифицирован как **SOURCE-CONFIRMED** (установлено чтением кода или механическим перечнем) или **ГИПОТЕЗА**; исполненных witness в этом раунде нет.

**Инвентарь (пересчитан скриптом, приложение B.1; не скопирован из R16):** 168 Rust-файлов, **43 325** физических строк: **17 059** строк кода (непустые, не начинающиеся с `//`), **24 303** строк-комментариев (из них **17 113** `///`/`//!`), **1 963** пустых. По группам: `src/alloc_core/` 88 файлов / 24 802 строк; `src/registry/` 46 / 10 413; `src/global/` 18 / 3 809; `src/concurrent/` 14 / 3 554; `src/lib.rs` 524; `src/kani_proofs.rs` 223. Дельта к R16 (43 317) — +8 строк, ровно дифф follow-up. Функций с телом — 1 125. Unsafe-инвентарь командой `CLAUDE.md` (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`): `src/` 21 tier-1 + 83 tier-2 (51 файл), `crates/` 6 + 20 — как в R16. Распределения и топ-20 — приложение A.

**Метод.** Сначала прочитаны целиком `docs/CORRECTNESS_OPEN_ITEMS.md` (с lookup-таблицей), `docs/correctness-open-items/ACTIVE.md`, `TRACKED_misc.md`, карточки-заголовки всех девяти `TRACKED_*.md` (тела — по теме: 9, 84, 95, 107, 154, 158, 161, 167), `docs/perf/OPEN_ITEMS.md` (шапка, все карточки `[A]`/`[D]`/`[L]`, Recently resolved; длинные aligned-vmem/numa-shim нарративы 57–62 — по current-state карточкам, они вне `src/`), отчёт и манифест R16, тематические разделы R1 (R1-07/08/11) и R15 (§4). Затем три слоя по `src/`:

1. **Полное построчное чтение:** `lib.rs`; все `mod.rs` под `alloc_core/`, `alloc_core/segment/`, `alloc_core/small/`, `alloc_core/platform/`, `alloc_core/large/`, `registry/`, `registry/segment_route/` (wiring-часть), `registry/heap_registry/` (шапка), `global/` (шапка); `segment_directory/directory_stats.rs`; `small/alloc_core_small_reclaim.rs`; `segment_route/registration.rs`; `alloc_core_core_diag/vmem.rs`; `heap_core/free/dealloc_own_base.rs:1–520`; `small/alloc_core_small/find_segment.rs:85–115, 170–625`; `small/alloc_core_small/dealloc.rs:1–120`; `heap_core/state/tcache_flush.rs:60–125`; весь src-дифф R16 follow-up.
2. **Выборочно по диапазонам** (каждая цитата ниже перечитана на базовом SHA): `platform/os.rs`, `platform/node.rs`, `platform/sidecar.rs`, `platform/sidecar_stats.rs`, `global/sefer_alloc/mod.rs:88–212`, `alloc_core_core_diag/{header,table,perf,directory}_diag.rs`, `alloc_core_small_pool/{alloc_core_small_pool_impl,decommit}.rs`, `alloc_core_small_magazine.rs`, `mem/{mem_impl,realloc_fastpath}.rs`, `heap_core/free/{realloc,dealloc,dealloc_batch}.rs`, `large/{alloc_core_large,alloc_core_large_cache_eviction,large_cache_extended}.rs`, `segment_header/{segment_header_gen_table,segment_header_impl,segment_header_layout,terminal_words}.rs`, `segment_table/{harness,segment_table_impl,route_slots}.rs`, `segment_route/{small_sidecar,pin,large_state}.rs`, `remote_bitmap/sidecar_bitmap.rs`, `heap_core/alloc/{hot,batch}.rs`, `bootstrap/{mod,loom_shim,registry}.rs`, `heap_slot.rs`, `heap_core_xthread/routing.rs`, `concurrent/{epoch/epoch_region,sharded/sharded_region}.rs`.
3. **Механический скрининг всех 168 файлов** (14 скриптов/команд, приложение B): размеры файлов и функций, приблизительная цикломатика и вложенность; перечень `allow`-подавлений; cfg-матрица; дубликаты окон кода; doctest-ограждения и indented code blocks; inline-тесты; TODO; приватные пути; ссылки `docs/…`, `item N`, intra-doc ссылки; идентификаторы и `.rs`-имена в комментариях, которых нет в коде; SAFETY/`# Safety`-покрытие; перепись `dbg_*`/`*_for_test(s)`; «один файл — один экспорт»; содержимое `mod.rs`; булевы параметры с литеральными аргументами; функции без вызовов в `src/`.

**Не прочитаны вглубь (только скрининг):** `registry/segment_route/directory.rs` (просмотрены лишь сигнатуры и тестовые хуки около `:559`, `:718–790`, `:868`), `global/fallback.rs`, `global/tls_heap.rs`, `global/maintenance_service.rs`, `global/exact_object/*`, `concurrent/lock_free/*`, `concurrent/pinning.rs`, `segment_directory_impl.rs` вне `:200–235` и `:270–320`, `alloc_core_large_cache.rs`, `kani_proofs.rs`. Утверждения «не найдено» относятся только к прочитанному и к тому, что ловят скрипты.

**Не запускалось:** всё, что требует компиляции или исполнения (режим только чтение): `cargo test/clippy/doc`, Miri/Loom/Kani, бенчмарки/iai, feature-powerset, CI. Поэтому для всех «скрытых предупреждений» и последствий во время выполнения ниже стоит класс ГИПОТЕЗА.

**Вердикт.** **13 находок: 1×P3, 8×P4, 4×P5.** Самое значимое: под `hardened` таблица поколений X7 только пишется, но нигде не читается (R17-CQ-01); на magazine-пути защита `off >= bump` осталась за `alloc-decommit`, хотя комментарий обещает паритет с `dealloc_small` (R17-CQ-02). Отдельно 94 комментария в настоящем времени ссылаются на 45 несуществующих (в основном удалённых) идентификаторов, включая аргументы об инвариантах (R17-CQ-04). Это доказывает, что исправлять пример за примером, как `c8b9344a`, недостаточно: нужен механический гейт. P0/P1/P2 в теме **не найдены** — для прочитанной области и этим методом. Это не доказательство отсутствия дефектов, не release GO и не perf GO.

## 2. Находки

Шкала из задания: P3 — ограниченный дефект корректности/ресурса/диагностики; P4 — вводящий в заблуждение контракт, пробел покрытия или сопровождаемость без установленного runtime-сбоя; P5 — косметика/долг без риска. Классы: SOURCE-CONFIRMED — по коду или механическому перечню; ГИПОТЕЗА — вывод о поведении при выполнении, не проверенный исполнением.

| ID | Sev | Класс | Достижимость | Суть |
|---|---|---|---|---|
| R17-CQ-01 | **P3** | SOURCE-CONFIRMED (стоимость — арифметика по исходнику) | только `hardened` (opt-in, не `production`) | Таблица поколений X7 (256 KiB на сегмент) инициализируется и бампается на каждой выдаче, но `gen_at` не вызывает никто; комментарии обещают защиту, которой после cutover нет |
| R17-CQ-02 | P4 | SOURCE-CONFIRMED + ГИПОТЕЗА (последствие) | `alloc-global + fastbin` без `alloc-decommit`: `--features fastbin`, `--features hardened`; не `production` | В `small_free_guard` проверка `off >= bump` под `cfg(alloc-decommit)`; `dealloc_small` сделал её безусловной (UBFIX-3), doc утверждает паритет |
| R17-CQ-03 | P4 | SOURCE-CONFIRMED + ГИПОТЕЗА (остаток в новой схеме) | любой `alloc-global + fastbin`; только нарушение контракта (двойной free) | Doc «RESIDUAL M2 LIMIT» ссылается на удалённый `#[ignore]`-тест и трекер X7; остаток не отслеживается ни одним индексом |
| R17-CQ-04 | P4 | SOURCE-CONFIRMED (механический перечень) | все конфигурации (комментарии) | 94 ссылки в настоящем времени на 45 несуществующих идентификаторов (46 файлов), 96 — на 20 несуществующих `.rs`, 24 строки описывают снятый ring; среди них — аргументы об инвариантах |
| R17-CQ-05 | P4 | SOURCE-CONFIRMED | все `alloc-core`-сборки, включая `production` | Живой production-модуль `remote_bitmap` помечен «Experimental … not a production route» и под `#[allow(dead_code)]`; `segment_directory` описан как «experimental, off by default» |
| R17-CQ-06 | P4 | SOURCE-CONFIRMED | документация unsafe-аудита | Ручные перечни unsafe-швов в `mod.rs`/`lib.rs` расходятся с канонической grep-командой в обе стороны |
| R17-CQ-07 | P4 | SOURCE-CONFIRMED | `production`, все `GlobalAlloc`-пути | No-panic контракт говорит об «одном преднамеренном abort», в коде 106 сайтов `process::abort()` в 25 файлах |
| R17-CQ-08 | P4 | SOURCE-CONFIRMED | диагностика (`alloc-stats`, `internals`, `bench-internals`) | Документация счётчиков расходится с кодом: фантомные счётчики, «storage only» у живых, «Reads 0 until … wires», неверные числа и номера пунктов |
| R17-CQ-09 | P4 | SOURCE-CONFIRMED | `internals`/`bench-internals` и production-сборки (неотгейченный тестовый код) | Тестовая поверхность: 4 схемы имён, сканер видит только `dbg_*`; тестовый код без гейта компилируется в production; 7 мёртвых хуков |
| R17-CQ-10 | P5 | SOURCE-CONFIRMED | production hot/cold пути | Копипаст: цикл снятия magazine-битов (уже дважды правился парно в R16), move-leg `realloc` ×3, deposit/release Large-кэша, валидация базы в 25 местах diag-файлов |
| R17-CQ-11 | P5 | SOURCE-CONFIRMED | все | Горячие точки сложности (`find_segment_with_free_impl`: 427 строк, CC≈34) и cfg-лес (предикат promotion скопирован 18 раз) |
| R17-CQ-12 | P5 | SOURCE-CONFIRMED (+ГИПОТЕЗА о скрытых предупреждениях) | все | Мёртвые флаги (`own_segment` всегда `true`), устаревшие `allow(dead_code)` с ложными обоснованиями, три blanket-allow в `lib.rs` |
| R17-CQ-13 | P5 | SOURCE-CONFIRMED | все | Двойные пути модулей, лишние `#[path]`, конфликтующие термины и имена, перепись «один файл — один экспорт», пробелы `GLOSSARY.md` |

### R17-CQ-01 — P3 — `hardened`: таблица поколений X7 только пишется, читателя нет

**Места.** Читатель: `src/alloc_core/segment/segment_header/segment_header_gen_table.rs:55` (`gen_at`). Писатели: `:98` (`bump_gen`), `:151–158` (`init_gen_table_in_place` — цикл по `GEN_TABLE_FOOTPRINT` байтам). Размер: `segment_header_impl.rs:160` (`GEN_TABLE_FOOTPRINT = SEGMENT / MIN_BLOCK` = 4 194 304 / 16 = 262 144 B); раскладка `segment_header_layout.rs:52, 84` (под `hardened` таблица входит в `small_meta_end`). Вызовы: инициализация `alloc_core/alloc_core/bootstrap.rs:209`, `small/alloc_core_small/reserve.rs:452`; бамп при выдаче `small/alloc_core_small/alloc_core_small_impl.rs:445–462`, `registry/heap_core/alloc/hot.rs:77` (`bump_gen_on_issue`) с вызовами `hot.rs:154, 382, 509` и `alloc/batch.rs:141, 184`.

**Наблюдено.** У `gen_at` нет ни одного вызова в `src/`: есть только определение (`:55`) и реэкспорт (`segment_header_impl.rs:763`); вызывают его лишь тесты `regression_gen_table_*`. Писатели при этом активны. Комментарии описывают потребителя, которого нет:
- модульный doc (`segment_header_gen_table.rs:14–15`): «Ф1 wires ONLY the byte-level … primitives — nothing in the alloc/dealloc/refill/drain paths consults the table yet» — бамп давно подключён;
- doc `gen_at` (`:27–29`): «the remote-free drain compares this against the generation stamped in the ring note (Ф3)»;
- doc `init_gen_table_in_place` (`:130–135`): «A stale ring note whose generation matches …»;
- `alloc_core_small_impl.rs:445–454`: «X7 Ф3 … touch (a) … for correctness and defense-in-depth».

Тем временем `README.md:1426` прямо говорит: «the old X7 ring generation path was removed with the ring». Ring и его записи сняты в `a4245965`. Текущий reclaim `reclaim_sidecar_record` (`small/alloc_core_small_reclaim.rs:21–66`) поколений не читает.

**Выведено (арифметика по исходнику, не измерено).** Под `hardened` каждый Small/Primordial-сегмент резервирует под таблицу 262 144 B метаданных — 6,25 % 4-MiB сегмента уходит из payload. При каждом резервировании эти байты записываются побайтным циклом в 262 144 итераций, то есть касаются и коммитятся. Каждая Small-выдача платит одну RMW по `AtomicU8`. На этот механизм приходятся 12 из 83 tier-2 `#[allow(unsafe_code)]` в `src/` (по таблице README: `segment_header_gen_table.rs` 3, `hot.rs` 4, `batch.rs` 2, `alloc_core_small_impl.rs` 1, `reserve.rs` 1, `bootstrap.rs` 1). Пользователь `hardened`, читающий doc-комментарии, будет считать, что защита поколениями от межпоточного double free существует.

**Почему P3.** Это ограниченный ресурсный дефект: RSS, payload и CPU тратятся в opt-in профиле без всякой пользы, а документация защиты вводит в заблуждение. Production не затронут.

**Рекомендация.** Снять механизм целиком: раскладку, init, бамп, аксессоры, `Node::atomic_u8_at` (`node.rs:427–430`; его единственный потребитель — эта таблица) и тесты `regression_gen_table_*`. Обновить README и `docs/GLOSSARY.md:23`. Если sidecar-reclaim когда-нибудь понадобятся поколения — подключить настоящего читателя и тест на нём. Изменение задевает тесты и unsafe-инвентарь, поэтому по правилам проекта нужен отдельный коммит с ревью.

**Oracle.** До: `grep -rnE 'bump_gen|init_gen_table_in_place' src/ | grep -vE '^[^:]+:[0-9]+:\s*//'` даёт 13 кодовых строк; после — 0. Под `hardened` `const _: () = assert!(Layout::small_meta_end() == <значение без hardened>)` сейчас красный, после — зелёный. tier-2 в `src/` уменьшится на 12, что требует синхронно обновить README (`readme_unsafe_inventory_counts_match_reality`). Если механизм решат сохранить: тест «устаревшее поколение отклоняется production-путём» сейчас невозможен (нет читателя), то есть красный.

### R17-CQ-02 — P4 — magazine-путь: защита `off >= bump` осталась под `alloc-decommit`, doc утверждает паритет с `dealloc_small`

**Места.** `src/registry/heap_core/free/dealloc_own_base.rs:170–173` (doc `small_free_guard`: «**M2 oracle 1.5** (`alloc-decommit`): stale-free (`off >= bump`) … parity with `dealloc_small` (`alloc_core.rs`)»); код `:292–295`:

```text
#[cfg(feature = "alloc-decommit")]
if (off as usize) >= meta.bump_of() {
    return SmallFreeGuard::RejectNoOp;
}
```

Для сравнения — `src/alloc_core/small/alloc_core_small/dealloc.rs:82–99` (`dealloc_small`) с комментарием `:93–96`: «M-1 (UBFIX-3): previously `#[cfg(feature = "alloc-decommit")]`-only … Corruption containment must not depend on the decommit feature; unconditional now.» `small_free_guard` используют и `dealloc_own_thread_with_base` (`:341`), и `dealloc_batch_small` (`free/dealloc_batch.rs:239`).

**Наблюдено.** Обоснование UBFIX-3 («сдерживание порчи не должно зависеть от decommit») применили к substrate-пути, а magazine-guard, выделенный позже (задача #2002), сохранил старый гейт. Doc называет поведение паритетным и ссылается на несуществующий `alloc_core.rs`.

**Достижимость.** `alloc-global + fastbin` без `alloc-decommit`: `--features fastbin`; `--features hardened` (`hardened = ["fastbin"]`, `Cargo.toml:687`); clippy-строка `hardened medium-classes internals`. Не `production` — там гейт выполнен. Срабатывает только при нарушении контракта: освобождается указатель в неразмеченный хвост собственного Small-сегмента. `canonical_block_of` такой указатель принимает, а magazine- и bitmap-оракулы видят «allocated».

**Выведено (ГИПОТЕЗА, не исполнено).** Такой указатель попадает в magazine (LIFO) и выдаётся повторно, а позже bump-carve выдаёт пересекающийся диапазон — две живые аллокации перекрываются. Именно профиль `hardened`, предназначенный для строгой детекции misuse, остаётся без этой проверки.

**Рекомендация.** Снять `cfg` (одно сравнение и одно чтение; `production` уже платит эту цену) и исправить doc. **Oracle.** Тест под `--features "hardened internals"`: через lease `HeapCore` освободить адрес не ниже `bump` текущего Small-сегмента (с выравниванием по классу), затем дважды выделить тот же класс — ни один выданный адрес не должен быть `>= bump` или дублироваться. Сейчас ожидается красный (ГИПОТЕЗА), после исправления — зелёный. Дешёвый статический пин: source-scan, что в теле `small_free_guard` перед проверкой bump нет `cfg`.

### R17-CQ-03 — P4 — «RESIDUAL M2 LIMIT»: пин и трекер давно удалены, остаток не учтён

**Место.** `src/registry/heap_core/free/dealloc_own_base.rs:179–191`. Doc утверждает: «all production drain paths now consult the magazine via `reclaim_offset_checked`'s `is_in_magazine` predicate … the remaining re-issue-before-drain leg is pinned RED by `residual_xthread_double_free_no_corruption` (`#[ignore]`d) — full fix tracked as task X7 (hardened, generational ring entry; `RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8.4)».

**Наблюдено.**
- `#[ignore]`-тест удалили ещё на R6-MS-1/2 как «moot»: `git show a4245965^:tests/regression_xthread_double_free_residual.rs`, строки 71–82, «the `#[ignore]`d residual test was removed (moot)» — двойной free стал UB по контракту `unsafe fn`.
- Весь файл, включая hardened-сиблинг `residual_xthread_double_free_no_corruption_hardened` и defence-in-depth тесты, удалён в `a4245965`.
- `reclaim_offset_checked` не существует.
- Механизм X7 потерял потребителя (R17-CQ-01).
- В обоих индексах остаток не упомянут (поиск по `docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/correctness-open-items/*.md`, `docs/perf/OPEN_ITEMS.md` — 0 совпадений).

**Выведено (ГИПОТЕЗА, только рассуждение по исходнику).** В терминальной sidecar-схеме существует аналогичная нога:
1. Чужой free блока B вызывает `SidecarBitmap::publish` и выставляет pending (`remote_bitmap/sidecar_bitmap.rs:110–119`).
2. Ошибочный собственный free того же B проходит `small_free_guard`, потому что bitmap всё ещё «allocated».
3. B уходит в magazine и выдаётся снова; `SidecarBitmap::issue` (`:83–96`) pending не проверяет.
4. Позже `reclaim_sidecar_record` (`small/alloc_core_small_reclaim.rs:46–64`) проверяет только `is_free`/`is_in_magazine` и возвращает живой B во free-list.
5. B выдаётся второй раз.

По контракту это UB вызывающего (двойной free), то есть речь о defence-in-depth, а не о soundness.

**Рекомендация.** Переписать абзац под текущую схему. Затем выбрать: завести остаток в `TRACKED_correctness_residuals.md` со сценарием выше либо запинить его `#[ignore]`-тестом на sidecar-пути. Ссылку на X7 снять (связано с R17-CQ-01). **Oracle:** ни один комментарий `src/` не называет тест, которого нет в `tests/` (покрывается гейтом R17-CQ-04); в индексе есть карточка.

### R17-CQ-04 — P4 — комментарии описывают удалённый код как текущий (полная механическая перепись)

**Перепись (приложение B.3–B.5; полный перечень — приложение A.6).**
- **94** упоминания в настоящем времени (в обратных кавычках) **45** идентификаторов, которых нет нигде в Rust-коде (`src`+`crates`+`tests`+`benches`+`examples`), в **46** файлах; ещё 47 упоминаний оформлены как история и не считаются. Чаще всего: `reclaim_offset` ×13, `PerClassDirty` ×9, `decommit_empty_segment` ×8, `dirty_by_class` ×6, `class_nonempty` ×5, `large_layout_consistent` ×4, `dealloc_foreign_routing` ×3.
- **96** ссылок в настоящем времени на **20** несуществующих `.rs`-имён в 45 файлах (+95 исторических): `alloc_core.rs` ×23, `alloc_core_small.rs` ×19, `alloc_core_small_pool.rs` ×13, `segment_header.rs` ×7, `heap_core.rs` ×7, … — включая удалённые `tests/regression_gen_wrap_boundary.rs` и `examples/r13_9_class_aware_dirty_sidecar_rss.rs`.
- **24** строки комментариев в 14 файлах описывают снятый ring-протокол как текущий (список в A.6).
- Intra-doc ссылки: из 1 329 проверенных целей несуществующая одна — `alloc_core_core_diag/header_diag.rs:331–332` `[dbg_force_decommit_retain](Self::dbg_force_decommit_retain)` (фактически `dbg_force_decommit_retain_for`, `alloc_core_small_pool/decommit.rs:97`). Строгий rustdoc её не ловит: элемент `#[doc(hidden)]`.

**Подмножество с аргументами об инвариантах (проверено вручную).**
1. `small/alloc_core_small_pool/decommit.rs:110–128, 209–215`. Обоснование, почему decommit можно сокращать, опирается на то, что «subsequent stale ring entries … are rejected by the `off >= bump` guard in `reclaim_offset`». Ring-записей больше нет, а замена `reclaim_sidecar_record` не отклоняет такие записи, а делает `abort` (`alloc_core_small_reclaim.rs:38–45`).
2. `small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:33–43, 76–81`. Doc говорит «credit retirement by … `reclaim_offset`» и «the `small_cur` snapshot and `table` raw pointer are threaded in», но сигнатура — `(base, small_cur)` (`:84`). Утверждение «ONLY after any in-progress ring drain … stale ring entries can still read the (still-committed) metadata» внутренне противоречит соседнему абзацу (кто делает recycle).
3. `registry/heap_core/free/dealloc_own_base.rs:123–131`. Hardened-проверка layout названа «(`large_layout_consistent`, the same primitive `heap_core_xthread::dealloc_routing` uses)». Такого примитива нет: проверка написана inline (`:241–243`), а `dealloc_routing` (`heap_core_xthread/routing.rs:10–20`) layout не проверяет вовсе.
4. `registry/heap_core/free/realloc.rs:359–389`. Замкнутый перечень «every path that can unregister a segment» ссылается на «the ring-drain loops in `find_segment_with_free_impl`» и на `alloc_core_small_pool.rs`.
5. `alloc_core/large/large_cache_extended.rs:39–46, 84–87`, `alloc_core/large/mod.rs:20–24`. Расширение кэша описано как «`leak_zeroed_pages`-reserved» и «тот же паттерн, что `dirty_by_class`/`PerClassDirty`». Код же резервирует через `sidecar::reserve` + `AccountedSidecar`, который освобождается вместе с ядром (`:107–169`); сам `sidecar.rs:36` пишет, что `leak_zeroed_pages` использовался «up to and including R14-9». Расходится утверждение о времени жизни памяти.
6. `alloc_core/small/mod.rs:31–32`. Doc модуля `alloc_core_small_reclaim` («Cross-thread (ring-drain) small-path reclaim — `reclaim_offset` and its `hardened` generation-checked variant»), тогда как в файле одна функция `reclaim_sidecar_record`.
7. `global/sefer_alloc/mod.rs:103–104`. No-panic контракт противопоставляет себя «`AllocCore::reclaim_offset` style — "bounds-check FIRST and no-op"», а замена как раз abort-ит.
8. `registry/bootstrap/registry.rs:155–160`. «F-3 context (documented, not fixed): the two production callers — `resolve_dirty_bit_target` and `resolve_heap_overflow`» — оба вызывающих удалены.
9. `small/alloc_core_small_magazine.rs:180–233`. Один и тот же блок комментариев говорит и «drains remote rings» (`:182, 190, 215–217, 233`), и «publishes into the route sidecar» (`:196–200`).

**Связь с item 154.** Item 154 отслеживает долг прозы; R15 и R16 добавляли примеры, `c8b9344a` исправил перечисленные. Но из 17 файлов, которые он затронул, как минимум 8 до сих пор содержат ссылки в настоящем времени: `header_diag.rs`, `platform/sidecar.rs`, `segment_directory/mod.rs`, `find_segment.rs`, `alloc_core_small_magazine.rs`, `alloc_core_small_pool/decommit.rs`, `heap_core/alloc/batch.rs`, `heap_core/diag/queries.rs`. Исправление пример за примером не сходится. **Новое:** исчерпывающая перепись и механический oracle.

**Рекомендация и oracle.** Расширить `tests/no_stale_doc_references.rs` — паттерн уже есть: `no_removed_heap_type_doc_mentions` и `no_stale_abandon_adopt_substrate_references` используют ручные denylist. Перейти на обратное правило: каждый идентификатор в обратных кавычках и каждый `*.rs`-путь в комментариях `src/**` должен существовать в коде или файловой системе либо стоять в явном allowlist `HISTORICAL` (например, `dbg_overflow_bitmap_clear_pass`, который `CLAUDE.md` цитирует намеренно). Сейчас тест красный (45 идентификаторов и 20 имён файлов), после чистки — зелёный. Чистить начиная с подмножества про инварианты, по модулю на коммит (это и есть триггер item 154).

### R17-CQ-05 — P4 — живые production-модули помечены «experimental / not a production route», на одном — blanket `allow(dead_code)`

**Места.** `src/alloc_core/segment/mod.rs:58–60`:

```text
/// Experimental non-intrusive remote-free ingress; not a production route.
#[allow(dead_code)]
pub(crate) mod remote_bitmap;
```

`remote_bitmap` — это production ingress: его потребляют `registry/segment_route/small_sidecar.rs:9`, `route_cut.rs:1`, `route_scan.rs:1`, `directory.rs:59, 116`. Его `sidecar_bitmap/leaf_classes.rs:4` — tier-1 шов, указанный как production в `lib.rs:258–261` и `README.md:682`. Там же `segment/mod.rs:12–17`: `segment_directory` «Feature-gated behind `alloc-segment-directory` (experimental, off by default). … Lookup wiring is A3 scope.» На деле `alloc-segment-directory` входит в `production` (`Cargo.toml:415`), а поиск подключён (`find_segment.rs:269–429`).

**Наблюдено.** Это точный сиблинг случая R15-03/R16 (item 154): `registry/mod.rs` «Unconnected Stage 3 substrate» и `#[allow(dead_code)]` на `segment_route` исправлены в `c8b9344a`, а этот сиблинг пропущен.

**Выведено.** Allow на уровне модуля подавляет `dead_code` во всём production ingress (`bitmap_cut`, `bitmap_record`, `bitmap_scan`, `sidecar_bitmap`, `leaf_classes`). Скрывает ли он сейчас реальные предупреждения — без компиляции неизвестно (ГИПОТЕЗА). Для `segment_route` в `c8b9344a` это проверяли на 6 clippy-строках и 7 узких наборах; здесь нужно так же.

**Рекомендация.** Удалить allow, исправить оба doc. **Oracle:** `grep -nB1 'mod remote_bitmap' src/alloc_core/segment/mod.rs` не показывает allow; расширить `no_stale_doc_references` правилом «doc модуля, компилируемого под `production`, не содержит "not a production route" / "experimental, off by default"» — сейчас красный.

### R17-CQ-06 — P4 — ручные перечни unsafe-швов противоречат канонической команде

**Места.**
- `src/alloc_core/mod.rs:5–7`: tier-1 «in this tree» — `platform::{os,node,sidecar}`, `large::large_cache_extended`, `segment::segment_table::route_slots`. Пропущен `segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:4` (grep даёт 6 tier-1 под `src/alloc_core`). И это в том же абзаце, который говорит «Do not hand-count these».
- `src/registry/heap_registry/mod.rs:17–20`: «[`claim`], [`counters`], and [`stack`] each have their OWN tier-1 `#![allow(unsafe_code)]` seam … every `unsafe` block across the three files». Шов есть только у `claim.rs:13`; в `counters.rs` и `stack.rs` `unsafe` нет совсем (в `stack.rs` слово «unsafe» не встречается ни разу). `counters.rs:141–142` повторяет «beyond what this module's header comment already documents as its seam».
- `src/registry/bootstrap/mod.rs:164–166`: «the `bootstrap::loom_shim` path is preserved for `heap_registry`'s cfg-gated `StackStorage` impl» — такой реализации нет. `loom_shim.rs:315–317`: «SAFETY proofs (see `heap_registry/stack.rs`'s `push_free_slot`)» — `stack.rs` теперь «Slot discovery without an intrusive free list», без `push_free_slot` и без unsafe.
- `src/lib.rs:173–183`: запись про `tagged-index-stack` повреждена — «both `unsafe fn` under a three-clause link-domain + liveness + exclusive-ownership free-slot packing proofs use it under `cfg(kani)`»: потеряна часть предложения. `lib.rs:103–106`: «Six are pulled into sefer-alloc's runtime dep tree under named feature gates». В runtime-графе пять (`sefer-region` вообще без гейта), а `tagged-index-stack` подключается только под `cfg(any(loom, kani))` (`Cargo.toml:933–936`) — сам список на `:112` говорит «not a runtime dependency».

**Наблюдено.** Существующий гейт `tests/no_stale_doc_references.rs::lib_rs_seam_inventory_matches_canonical_grep` (`:945`) проверяет только пункты в `lib.rs`, а `readme_unsafe_inventory_counts_match_reality` (`:606`) — только итоговые суммы. Фрагменты в `mod.rs` не проверяет никто.

**Почему P4.** `CLAUDE.md` требует сверять любой аудит с выводом команды; эти фрагменты вводят аудитора в заблуждение в обе стороны. Принуждение компилятором при этом не страдает.

**Рекомендация.** Убрать перечни по модулям (оставить ссылку на команду) либо распространить тест `lib.rs` на все `src/**/mod.rs`. **Oracle:** тест сейчас красный — `alloc_core/mod.rs` без `leaf_classes`, `heap_registry/mod.rs` называет `counters`/`stack`.

### R17-CQ-07 — P4 — no-panic контракт занижает поверхность убийства процесса

**Место.** `src/global/sefer_alloc/mod.rs:188–191`: «The one deliberate process kill on the alloc path is a direct `std::process::abort()` (registry chunk-materialisation OOM, `registry/bootstrap/registry.rs`)».

**Наблюдено.** В коде **106** сайтов `std::process::abort()` в **25** файлах (B.8): `segment_table/route_slots.rs` 22, `segment_route/directory.rs` 15, `heap_registry/claim.rs` 7, `segment_table_impl.rs` 7, `alloc_core/sidecar_drain.rs` 7, `active_kind_index.rs` 6, `alloc_core_large.rs` 5, `mem_impl.rs` 4, `alloc_core_small_reclaim.rs` 4 и др. Примеры, прослеженные до `GlobalAlloc` под `production`:
- попадание в Large-кэш: `alloc_core_large.rs:251` → `SegmentMeta::begin_large_reuse`, который abort-ит при фазе не `Cached` (`terminal_words.rs:145–152`);
- линейный скан abort-ит на не-Small активном слоте (`find_segment.rs:474–481`);
- owner drain: `reclaim_sidecar_record` abort-ит на записи вне границ или с неверным выравниванием (`alloc_core_small_reclaim.rs:24–45`).

`tests/no_panic_doc_accuracy.rs` закрепляет только релизные panic (`production_release_panic_sites_match_explicit_allowlist`, `:494`). Абзац перечисляет механизмы, способные к panic, но умалчивает об abort-тripwire — основном fail-stop механизме.

**Почему P4.** Вводящий в заблуждение контракт для пользователя: что вызов аллокатора может сделать при порче метаданных или нарушении контракта. Runtime-сбоя этот текст сам не создаёт.

**Рекомендация.** Описать класс abort-тripwire (нарушение инварианта или контракта → `abort`, без раскрутки стека), инвентарь давать командой. Добавить сканер abort-сайтов с allowlist, по аналогии с panic-сканером. **Oracle:** тест «doc не утверждает "The one deliberate process kill"; abort-сайты по файлам совпадают с allowlist» — сейчас красный.

### R17-CQ-08 — P4 — документация диагностических счётчиков расходится с кодом

**Места.**
- `src/alloc_core/segment/segment_directory/directory_stats.rs:12–29`, «Counter inventory»: 14 строк таблицы. Две — фантомы: `dirty_segments_drained`, `wasted_dirty_drains` — статиков нет нигде. Из 12 реальных счётчиков 10 помечены «storage only», хотя у всех есть места инкремента (под `alloc-stats`): `find_segment.rs:372, 407, 417, 426, 471, 597, 607, 634–636, 663, 691, 704, 734, 745`; `alloc_core_small_impl.rs:250, 348`; `alloc_core_small_magazine.rs:314`.
- 7 устаревших «Reads 0 until A3/R8-2 wires the increment»: `alloc_core_core_diag/perf_diag.rs:21–22, 29–30, 37–38, 212–213`; `directory_stats.rs:33–35, 39–40, 43–45`.
- `alloc_core_core_diag/vmem.rs:81–93` (фраза на `:83`): reset «resets SIX counters total … the four this crate exposes its own forwarders for». Фактически `aligned_vmem::reset_bench_internals_counters` сбрасывает 16 (`crates/aligned-vmem/src/bench_internals/reset.rs:15–49`), а форвардеров в этом файле 5, включая `dbg_windows_reserve_commit_calls` (`:53`).
- `alloc_core/counters.rs:108` приводит в пример конвенции удалённый `WASTED_DIRTY_DRAINS`.
- Ссылки «item 78(c)» про доверие отрицательному ответу директории: `perf_diag.rs:225`, `directory_stats.rs:71`, `find_segment.rs:397` («O-4 / perf OPEN_ITEMS item 78(c)»). O-4 — это perf item 82; 78(c) — про pending-Large hint, и его собственная карточка ссылается на 82 как на сиблинга со стороны директории.
- У двух читателей счётчиков нет потребителей вообще: `dbg_directory_hits`, `dbg_directory_stale_hits` (`perf_diag.rs:25, 33`).

**Почему P4.** Правило path-activation oracle (R30-8 в `CLAUDE.md`) делает эти счётчики доказательством GO/NO-GO. Судья, поверивший «storage only», отбросит живой счётчик или будет опираться на фантомный.

**Рекомендация.** Генерировать таблицу из статиков и мест инкремента (или убрать колонку «Live in A0?»), удалить «until … wires», исправить числа и номер пункта. **Oracle:** тест, разбирающий `directory_stats.rs`: строки таблицы совпадают со статиками, а «storage only» стоит тогда и только тогда, когда в `src` нет `fetch_add` — сейчас красный.

### R17-CQ-09 — P4 — тестовая поверхность: перепись и именование не принуждаются

**Наблюдено (B.9).**
- 246 `pub fn dbg_*` (из них 30 `pub unsafe fn`), 455 атрибутов `#[doc(hidden)]` в 67 файлах.
- Четыре схемы имён: `dbg_*` (246); `*_for_test(s)` без `dbg_` (48, из них 1 unsafe: `small_sidecar.rs` 8, `maintenance_service.rs` 8, `segment_route/directory.rs` 7, `sharded_region.rs` 6, `header_diag.rs` 5, `lock_free_region.rs` 4 и др.); `dbg_*_for_test(s)` (13); `_*_for_tests` с ведущим подчёркиванием (12, `concurrent/`).
- `scripts/verify-dbg-hook-safety.mjs:375` перечисляет только `\bpub\s+(unsafe\s+)?fn\s+(dbg_\w+)\b`. Вне автоматической переписи остаются 48+12 тестовых хуков без префикса `dbg_` и тестовые API без «хуковых» имён (`SegmentHashHarness`, Large-обёртки `RouteRegistration`). Прецедент: item 9, P2-12 (R31) — один случай закрыли переименованием `ReservedSmallSegment::base` → `dbg_base`; `reserved_small_segment.rs:175–195` прямо описывает эту слепую зону префикса, но сканер расширен не был.
- Тестовый код без гейта в production-сборках. `segment_table/mod.rs:58–59` (`mod harness;` без cfg), `segment_table_impl.rs:9` (`pub use harness::SegmentHashHarness;`), `alloc_core/mod.rs:259–260` — харнес на `Vec` компилируется в каждую `alloc-core`-сборку. В production он мёртв и спрятан blanket-allow `lib.rs:448` (что он вызвал бы предупреждение — ГИПОТЕЗА).
  - То же для `RouteRegistration::{cache_large_consumed, begin_large_reuse, finish_large_reuse_after_reset, release_cached_large}` (`registration.rs:55–76`) и `LargeState::{cache_consumed, begin_reuse, finish_reuse, release_cached}` (`large_state.rs:34–50`): вызывающих в `src` нет. Production выполняет эти переходы на слове заголовка через `LargeReservationState` (`mem_impl.rs:435`, `alloc_core_large.rs:251–262`, `alloc_core_large_cache_eviction.rs:50, 241`, `lifecycle.rs:66`); вызывают обёртки только `tests/r6_route_directory.rs:149–165` и `r6_route_directory_model.rs:43–53`. То есть тесты проверяют cache-lifecycle на route-слове, которое production так не ведёт.
- Неотгейченные `#[doc(hidden)] pub fn _…_for_tests` на публичных экспериментальных типах (реэкспорт в корне, `lib.rs:490–494`): `EpochRegion::_remote_free_queue_buffer_identity_for_tests` (`epoch_region.rs:655–656`), `ShardedRegion::_reset_my_shard_binding_for_tests` (`sharded_region.rs:718–719`, мутирует TLS), `ShardedRegion::_remote_free_queue_buffer_identity_for_tests` (`:727–728`). Их соседи в тех же файлах загейчены `internals`.
- 7 `dbg_*`-хуков без единого вызова во всём репозитории (поиск по `.rs/.mjs/.toml/.yml`): `alloc_core_core_diag/directory_diag.rs:79` `dbg_directory_get_bit_for_node`; `perf_diag.rs:25` `dbg_directory_hits`; `:33` `dbg_directory_stale_hits`; `table_diag.rs:151` `dbg_recycle_unverified_base_total`; `vmem.rs:65` `dbg_windows_reserve_commit_single_calls`; `vmem.rs:77` `dbg_windows_reserve_commit_two_call_pairs`; `small/alloc_core_small/find_segment.rs:98` `dbg_find_segment_with_free_for_test`.

**Проверено и не дефект.** В четырёх diag-файлах (`alloc_core_small_diag.rs`, `header_diag.rs`, `table_diag.rs`, `heap_core/diag/queries.rs`) 28 строк выводят базу из указателя вызывающего. Из них 19 валидируют базу через `canonical_base_of`/`contains_base_ro`; 2 имеют собственный guard членства (`table_diag.rs:228`, `queries.rs:245`); 2 строки одной функции — чистая арифметика (`dbg_segment_base_of_ptr`, `queries.rs:610–611`); 5 — `pub unsafe fn` с `# Safety` (`alloc_core_small_diag.rs:148, 181, 215, 239, 378`). Небезопасных хуков не найдено; находка — о процессе и покрытии.

**Рекомендация.** Одна схема имён (префикс `dbg_`) либо расширение сканера на все `#[doc(hidden)] pub fn`, `*_for_test(s)` и `_*`. Загейтить `internals` харнес, route-обёртки Large и три экспериментальных хука. Удалить 7 мёртвых хуков. **Oracle:** самопроверка сканера с перечнем неклассифицированных публичных тестовых функций — сейчас красная (≥60), после классификации — зелёная.

### R17-CQ-10 — P5 — копипаст между похожими путями

1. Снятие magazine-битов перед flush: `registry/heap_core/free/dealloc_own_base.rs:482–495` ≡ `registry/heap_core/state/tcache_flush.rs:86–98` (тело идентично, отличаются граница среза и сообщение). Цена уже заплачена: исправление R16-01 правило обе копии дважды (`9df9f6b8`, затем `b707d196`).
2. Move-leg `realloc` (граница R2-1 → `Layout::from_size_align` → alloc → проверка null → copy → free): `alloc_core/mem/mem_impl.rs:677–694`, `heap_core/free/realloc.rs:336–407` и `:419–437`; граница повторена в `try_promote_to_large` (`realloc.rs:558`). Каждая копия несёт 10–20 строк обоснования R2-1.
3. Помещение в Large-кэш: `mem_impl.rs:432–452` ≡ `alloc_core_large.rs:802–821` (комментарий прямо называет это «Same fix as the mirror site»).
4. Освобождение кэшированного Large: `alloc_core_large_cache_eviction.rs:41–54` ≡ `:235–248`, плюс варианты `lifecycle.rs:60–66`, `alloc_core_large.rs:255–262`.
5. Магазинный push с `FREE_PARK_CAP`: `free/dealloc_batch.rs:340–356` против `dealloc_own_base.rs:404–420`.
6. В трёх diag-файлах 25 вызовов `segment_base_of_ptr` (`header_diag.rs` 9, `alloc_core_small_diag.rs` 9, `table_diag.rs` 7), и за каждым — своя копия валидации через таблицу (либо её осознанное отсутствие под `unsafe fn`); дисциплина provenance из R2-05 держится на каждом месте отдельно.

Детектор (B.7): 10 регионов от 8 нормализованных строк, 29 групп от 5. **Рекомендация:** хелперы `clear_magazine_bits(&[*mut u8])`, move-and-free, `deposit_large_cached`, освобождение кэшированного слота, `owned_root_of(ptr)` для diag. Горячие пути — с iai kill-gate (`CLAUDE.md`). **Oracle:** счётчик детектора уменьшается, iai нейтрален.

### R17-CQ-11 — P5 — горячие точки сложности и cfg-лес

- `find_segment_with_free_impl` (`small/alloc_core_small/find_segment.rs:186–612`): охват 427 строк, 222 строки кода, приблизительная CC ≈ 34, вложенность 6; в файле 63 cfg-атрибута. Параметр `#[cfg(feature = "alloc-segment-directory")] rescue: bool` (`:197`) и `finalize_hit(…, #[cfg] Option<bool>, #[cfg] bool, #[cfg] bool)` (`:621–630`) вызываются трижды (`:530–539, 545–554, 584–593`). Все 10 cfg-гейтированных позиционных аргументов вызовов, найденных эвристикой по `src`, — в этом файле. Комментарий `:250–267` пересказывает, что говорил удалённый абзац.
- Другие точки (строк кода / охват): `alloc_large` (`alloc_core_large.rs:136`) 145/354; `primordial` (`bootstrap.rs:46`) 135/380; `reserve_small_segment_impl` (`reserve.rs:100`) 127/364; `refill_class_bump_impl` (`alloc_core_small_magazine.rs:154`) CC 29; `dealloc_at_base` (`mem_impl.rs:255`) 96/287, вложенность 7, с `#[inline(always)]`.
- Терм предиката promotion `all(feature = "large-reserved-capacity", not(feature = "numa-aware"))` скопирован 18 раз в 9 файлах: 3 — внутри макроса `medium_promotion_reachable!` (`free/dealloc.rs:69–102`), 15 — вручную (например, `alloc_core/mod.rs:210–217` «reproduced here verbatim», `dealloc_own_base.rs:28–32` «must stay hand-written and in sync by inspection»). Рассинхронизация уже случалась (`alloc_core/mod.rs:198–205`, поймана clippy `--all-features`).
- 1 302 атрибута `cfg`/`cfg_attr`, 161 различный предикат, 26 features; 29 предикатов содержат ≥3 комбинатора.
- No-op `unsafe fn init_node_ids_raw` (`segment_directory_impl.rs:302–316`) держится ради паритета сигнатур и добавляет tier-2 allow.
- Файлы у предела в 1000 строк (`tests/src_file_size_cap.rs`): `segment_table_impl.rs` 994, `alloc_core_small_impl.rs` 973, `segment_route/directory.rs` 909.

**Рекомендация.** Разрезать `find_segment_with_free_impl` на directory-probe / linear-scan / finalize со структурой контекста, которая держит cfg-поля и убирает cfg-позиционные аргументы. Сделать единый источник предиката: `build.rs` с `cargo:rustc-cfg=…` и `check-cfg`. Компромисс: build-скрипт добавляет время компиляции и поверхность supply chain. Альтернатива — оставить макрос и добавить source-scan тест, что все 15 копий побайтно совпадают. Ввести guard на размер функций по аналогии с лимитом на файл. **Oracle:** копий предиката — 1 (или тест на тождество); скрипт метрик B.2.

### R17-CQ-12 — P5 — мёртвые флаги и устаревшие подавления lint-ов

- `AllocCore::safe_payload_read_span(…, own_segment: bool)` (`alloc_core/mem/realloc_fastpath.rs:128–132`): все 3 вызова передают `true` (`mem_impl.rs:677`; `realloc.rs:336, 558`). Ветка `false` «retained for shape only: it has no caller today and would require its OWN independent mapping/ownership proof» (`:114–117`). Получается безопасная `pub(crate) fn` на сырых указателях с веткой, корректность которой не доказана.
- Частные функции с литеральными bool: `decommit_empty_segment_impl(…, release_follows)` — 2/2 (`alloc_core_small_pool/decommit.rs:106, 132`); `EpochRegion::drain_remote_free(…, force)` — 3/3 (`epoch_region.rs:431, 436, 513`).
- Безусловный `#[allow(dead_code)]` с устаревшим обоснованием на **используемых** элементах:
  - `SegmentHeader::kind_at` (`segment_header_views.rs:9`, «Used by Phase 9+ cross-thread routing; kept for that» — 35 вхождений в коде без комментариев и строк);
  - `LargePhase` (`terminal_words.rs:36`, «Pending/Consuming belong to the next ingress stage» — оба варианта используются в `reservation_state.rs:46, 60, 67, 95`);
  - `SegmentTable::count` (`segment_table_impl.rs:706`, «tests / Phase 9 use it» — используется в `sidecar_drain.rs:73, 96, 201, 276, 306` и `find_segment.rs:435`).
  - На **мёртвом** элементе: `SizeClasses::is_huge` (`platform/size_classes.rs:340–343`, «Phase 10 (M6) consumes this; kept for that») — ни одного вызова в `src/tests/benches/examples`.
  - Двойной allow на `Node::atomic_u8_at` (`node.rs:427–428`); его единственный потребитель — мёртвая таблица из R17-CQ-01.
- Allow на уровне модулей: `segment/mod.rs:59` (R17-CQ-05); `platform/sidecar_stats.rs:48` `#![allow(dead_code)]` на весь файл (обоснован перекрёстным произведением трёх features, `:37–47`).
- Три blanket `#[allow(dead_code, unused_imports)]` на `alloc_core`/`global`/`registry` (`lib.rs:447–449, 469–471, 481–483`). Follow-up-заметка R16 к item 154 называет только `:482`, R1-08 — только `alloc_core`. Без `internals`, то есть в `production`, они делают невидимым всё перечисленное выше.
- Перепись `allow`: `dead_code` 57, `deprecated` 17, `unused_imports` 16, `clippy::missing_docs_in_private_items` 8, `unused_variables` 6, `unused_mut` 4, `clippy::too_many_arguments` 2, `clippy::mut_from_ref` 2, `clippy::unused_self` 1, `clippy::module_inception` 1; `#[expect]` — 0.

**Рекомендация.** Заменить `#[allow(dead_code)]` на `#[expect(dead_code, reason = "…")]` (MSRV 1.93 ≥ 1.81): ненужное подавление станет предупреждением. Удалить `is_huge` и параметр `own_segment`. **Oracle:** под `-D warnings` `#[expect]` упадёт на `kind_at`/`LargePhase`/`count` (ГИПОТЕЗА — нужна сборка); `grep -c` по B.6.

### R17-CQ-13 — P5 — wiring модулей, именование, «один файл — один экспорт», пробелы глоссария

- **Слой алиасов** `alloc_core/mod.rs:43–154`: около 45 строк реэкспортов держат пути до реорганизации (`alloc_core::os` ↔ `alloc_core::platform::os`). В коде используются обе формы: `alloc_core::os::` 66 раз против `alloc_core::platform::os` 2; `node` 46/3; `sidecar` 34/6. В том же файле 10 `#[allow(unused_imports)]` и 9 приватных `use` — формально вне правила «mod.rs: только `mod`/`pub mod`/`pub use`».
- **31 атрибут `#[path]`:** около 26 избыточны (совпадают с путём по умолчанию, например `alloc_core_small/mod.rs:18–35`, `alloc_core/alloc_core/mod.rs:45–102`). 5 межкаталожных (`segment/mod.rs:34, 36, 38, 40, 56`) делают `segment::segment_header_layout`, `segment::directory_stats` и др. сиблингами `segment_header`/`segment_directory`, хотя файлы лежат в их каталогах. Плюс 7 glob-реэкспортов (`pub use *_impl::*` ×5, `sidecar_bitmap::*`, `terminal_words::*`).
- **Термины:**
  - `tcache` (126 вхождений в коде, 15 идентификаторов) и `magazine` (80, 16) обозначают одну структуру;
  - `base`/`root`/`key`: `canonical_root_for` (`mem_impl.rs:26`) делегирует в `canonical_base_of`; суффикс `_mut` в `canonical_base_of_mut` (`segment_table_impl.rs:743`) означает «с заполнением кэша», а та же ось у `contains_base`/`contains_base_ro` выражена иначе;
  - `foreign`/`remote`/`xthread` (24/70/94);
  - состояние слота `LIVE` против `OWNED`: `STATE_LIVE` — «Compatibility name» для `STATE_OWNED` (`heap_slot.rs:98–99`), в комментариях «LIVE → FREE» 37 раз и «OWNED → FREE» 4 раза;
  - одно имя `STATE_INITIALIZING` со значениями 1 (`global/fallback.rs:63`) и 2 (`registry/heap_slot.rs:91`), оба `pub` и реэкспортированы;
  - одно имя `begin_large_reuse` для двух разных слов состояния (`registration.rs:61` — route-слово, `terminal_words.rs:145` — слово заголовка).
- **«Один файл — один экспорт»** (B.10): 14 не-`mod.rs` файлов с ≥2 публичными элементами верхнего уровня. Санкционированы (категории 1/2): `heap_slot.rs`, `bootstrap/registry.rs`, `bootstrap/ensure.rs`, `terminal_words.rs`, `sidecar_stats.rs`, частично `fallback.rs`. Не санкционированы (несколько основных элементов):
  - `registry/heap_registry/claim.rs` — 3 структуры `HeapRegistry`/`MaintenanceLease`/`HeapLease` (`:34, 366, 459`);
  - `alloc_core/config/profile.rs` — 3 типа (уже отмечено в R15 §4 п. 5);
  - `global/tls_heap.rs` — 2 enum и 4 резолвера;
  - `alloc_core/platform/numa.rs`;
  - `segment_header_gen_table.rs` — 3 fn (R17-CQ-01);
  - `segment_header_impl.rs` — константы owner-state, pack/unpack и `GEN_TABLE_FOOTPRINT`;
  - `segment_table_impl.rs` — `pub use harness` и fn.
- **`docs/GLOSSARY.md`** не расшифровывает самое частое семейство в комментариях `src` — `R<n>-<id>` (1 169 токенов), а также `RAD-N` (60), `UBFIX-N` (37), `PhN` (45), `CRATE-PN` (10), идентификаторы с тегами ревьюеров (26). Нет и доменных терминов tcache/magazine, base/root/key, sidecar/route/pin.

**Рекомендация.** Свернуть слой алиасов (механической заменой на канонические пути), убрать избыточные `#[path]`, дополнить глоссарий, выбрать один из терминов tcache/magazine, переименовать `STATE_INITIALIZING` в одном из модулей, разнести `HeapLease`/`MaintenanceLease` по своим файлам. **Oracle:** скрипт подсчёта алиасов, скрипт переписи экспортов B.10.

### 2.14 Новые доказательства к существующим пунктам (статусы не меняю — решение за оркестратором)

- **Correctness item 154.** Числа на базе: 24 303 строк-комментариев (17 113 `///`/`//!`) против 17 059 строк кода (R1-11 мерил ≈20 000 doc-строк против ≈15 600 кода). В 2 022 строках комментариев есть токены истории (`R<n>-<id>` 1 169, `task #N` 734). Комментарных блоков ≥40 строк — 101, ≥20 — 305; в телах функций не-doc блоков ≥15 строк — 63; ссылок `docs/…` — 128 на 62 пути (все существуют). Остаток «parent allow» — это три allow (`lib.rs:448/470/482`), а не один. Механический oracle — R17-CQ-04.
- **Correctness item 107.** По исходнику все 6 указанных мест теперь выведены из комбинации `numa-aware internals`: импорты `alloc_core_small_reclaim.rs:3–14` — под `all(alloc-global, alloc-xthread)`; `MagazineBitmap::mark_magazine/clear_magazine` — под `cfg(feature = "fastbin")` (`magazine_bitmap.rs:133–135, 144–146`). Повторный запуск команды карточки может её закрыть (не запускался).
- **Correctness item 161.** Из 5 мест: импорт `Layout` в `queries.rs` загейчен (`:13–14`), импорт `SegmentMeta` исчез; `is_in_magazine` в `find_segment.rs` больше нет; два лишних `mut` теперь подавлены `#[allow(unused_mut)]` (`alloc_core_large.rs:596`, `reserve.rs:214`) — это подавление, а не исправление. Номера строк в карточке устарели.
- **Correctness item 167.** Src-зеркало `StackHead`/`StackStorage`/`StackOps` (`loom_shim.rs:237–465`) не имеет ни одного потребителя ни в `src`, ни в `tests`: `tests/loom_registry_free_slots.rs:23` импортирует настоящие типы `tagged_index_stack`. `bootstrap/mod.rs:164–166` и `loom_shim.rs:315–317` указывают на несуществующих потребителей. Это усиливает вариант (б) карточки.
- **Perf item 67.** Карточка называет только `dealloc_own_thread_with_base`. Всего 139 сайтов `#[inline(always)]`, из них 7 тел ≥40 строк кода: `dealloc_at_base` (`mem_impl.rs:255`, 96), `dealloc_own_thread_with_base` (`dealloc_own_base.rs:341`, 85), `small_free_guard` (`:195`, 72), `alloc_small` (`alloc_core_small_impl.rs:163`, 60), `alloc_with_class` (`hot.rs:237`, 51), `dealloc_small` (`dealloc.rs:34`, 48), `pop_free` (`alloc_core_small_impl.rs:374`, 44). Не измерено.
- **Correctness item 9 (линия P2-12)** — обобщение в R17-CQ-09.
- **Correctness item 158** — комментарий APPEND-ONLY по-прежнему на `directory_diag.rs:438`, изменений нет.

### 2.15 Гипотезы оптимизации через призму темы (не измерены, не speedup-claim)

1. Снятие R17-CQ-01 под `hardened`: −256 KiB RSS на Small-сегмент, +6,25 % payload, резервирование без цикла в 262 144 итерации, выдача без RMW. Судья — только по R26-4/R30-8: path-activation oracle — число вызовов `bump_gen` и байты таблицы, слой — `HeapCore`/реальный `#[global_allocator]` под `hardened`.
2. Кандидаты perf item 67 (выше): cold-ветки, вынесенные из `#[inline(always)]`-тел. Нужна пара `cargo bloat` + iai `Ir` с oracle активации.
3. Хелперы R17-CQ-10 задуманы perf-нейтральными; это проверяет iai kill-gate.

## 3. План декомпозиции и упрощения (порядок, при котором правки безопаснее; сам не правил)

1. **Сначала гейты, потом правки (только docs и тесты):** oracle R17-CQ-04 (существование идентификаторов и файлов с allowlist `HISTORICAL`), расширение `lib_rs_seam_inventory_matches_canonical_grep` на `mod.rs` (R17-CQ-06), разбор таблицы `directory_stats.rs` (R17-CQ-08), allowlist abort-сайтов (R17-CQ-07). Каждый красный на текущем дереве — это и есть контрфактуальная проверка невакуумности.
2. **Чистка комментариев по модулю на коммит** (триггер item 154): сначала подмножество R17-CQ-04 про инварианты (`decommit.rs`, `alloc_core_small_pool_impl.rs`, `dealloc_own_base.rs`, `realloc.rs`, `large_cache_extended.rs`, `small/mod.rs`, `sefer_alloc/mod.rs`, `bootstrap/registry.rs`, `alloc_core_small_magazine.rs`), затем остальное. Несколько тестов делают `include_str!` и закрепляют фразы — после каждого модуля перезапускать doc-tripwires.
3. **Мелкие кодовые правки с тестом:** R17-CQ-02 (снять `cfg` у проверки bump + регресс под `hardened`); R17-CQ-05 (снять allow с `remote_bitmap` + те же 6 clippy-строк и узкие наборы, что в `c8b9344a`); R17-CQ-03 (карточка остатка или `#[ignore]`-пин).
4. **Удаление мёртвого кода (отдельные коммиты):** R17-CQ-01 (таблица поколений: раскладка, init, бамп, тесты, README tier-2 −12); 7 мёртвых хуков; `is_huge`; флаг `own_segment`; зеркало `StackHead` в `loom_shim` (item 167, вариант б); гейт `internals` для `SegmentHashHarness`, route-обёрток Large и трёх `_…_for_tests`.
5. **Структурные рефакторинги без изменения поведения (по одному, с iai):** хелперы R17-CQ-10 (начать с цикла magazine-битов — он уже дважды требовал парных правок), затем move-and-free и deposit/release Large-кэша.
6. **Крупное:** разрезать `find_segment_with_free_impl` со структурой контекста; единый источник предиката promotion (`build.rs` cfg-alias или тест на тождество копий); свернуть слой алиасов и избыточные `#[path]`; разрезать файлы у лимита (`segment_table_impl.rs` 994, `alloc_core_small_impl.rs` 973) **до** следующей правки в них.
7. **Именование:** tcache/magazine, глоссарий base/root/key, `STATE_LIVE`→`STATE_OWNED`, `STATE_INITIALIZING`, переименование `canonical_base_of_mut` (например, `…_cached`).

## 4. Вне темы (место + одна строка)

- `README.md:666` — `node.rs` «also `release_segment` thin wrapper»; на деле `release_segment` в `os.rs:564`.
- `README.md`, tier-2 таблица — строка `src/global/sefer_alloc/diag.rs` говорит 6, grep даёт 7; сумма строк по `src` 82 при 83 по grep; тест закрепляет только итоги.
- `tests/src_file_size_cap.rs:3` ссылается на удалённый `src/registry/heap_core_xthread/overflow.rs`; `tests/no_panic_doc_accuracy.rs:9` — «five tripwires», а doc говорит о четырёх.
- `Cargo.toml`, комментарий к `hardened` (около `:672–686`), цитирует удалённый `reclaim_offset`.
- `docs/GLOSSARY.md:23` (X7 «generational ring»), `:25` (`src/registry/heap_core.rs` теперь каталог).
- `Node::atomic_{u8,u32,u64}_at` (`node.rs:430–490`) — безопасные `pub(crate) fn`, возвращающие `&'static` на память, которую освобождают посреди процесса (оговорено в SAFETY) — для ревьюера unsafe/membrane.
- `HeapCore::dealloc_routing` (`heap_core_xthread/routing.rs:10–20`) — безопасная `pub(crate) fn`, чей SAFETY («reached only from the allocator's unique current-instance free») — допущение о вызывающем; стоит рассмотреть `unsafe fn` — для того же ревьюера.
- `dealloc_own_base.rs:386–393` — SAFETY «reached only from `HeapCore::dealloc`», хотя есть ещё вызовы из `HeapCore::realloc` (`realloc.rs:404, 653`) и из dbg-хука.
- Route-слово `LargeState` и слово заголовка `large_state_atomic`: production ведёт route-слово только по LIVE→PENDING→CONSUMING, а `LargeState::new()` всегда начинает с поколения 1 — стоит подтвердить семантику перерегистрации при повторном использовании кэша (ревьюер конкурентности).

## 5. Что проверено и не дало находок

- **Нет runnable doctest-ов:** 20 парных ограждений ```` ```text ```` в 12 файлах; голых, `rust`, `no_run`, `compile_fail` нет; markdown indented code blocks — 0 (единственный кандидат `realloc_fastpath.rs:114` в контексте списка).
- **Нет inline-тестов:** `#[cfg(test)]`, `mod tests`, `#[test]` в `src/` — 0 (13 `kani::proof` — санкционированная категория 4).
- **TODO/FIXME/XXX/HACK** — 0. **Приватные пути машины и имена пользователя** — 0.
- **Ссылки `docs/…`:** 128 на 62 различных пути — все существуют (2 перенесены строкой и разрешены).
- **`mod.rs` (33 файла):** нет логики, типов и функций; нет `#![allow(unsafe_code)]` в `mod.rs` (R1-07 держится). Единственное отклонение — 9 приватных `use` в `alloc_core/mod.rs` (R17-CQ-13).
- **Unsafe-инвентарь:** `src` — 21 tier-1 + 83 tier-2, `crates` — 6 + 20 (как в R16); список `lib.rs` (21) и строки tier-1 в README (21) совпадают с grep; production-набор 15 совпадает.
- **SAFETY-покрытие:** 219 блоков `unsafe {}` — у всех есть SAFETY (5 флагов эвристики — SAFETY выше 14 строк); 68 `unsafe fn` — у 67 есть `# Safety` (исключение — частная `dealloc_batch_small`, `dealloc_batch.rs:239`, `batch-api`); 9 `unsafe impl` — у всех есть SAFETY.
- **Diag-хуки на сырых указателях:** из 28 строк в четырёх diag-файлах, где база выводится из указателя вызывающего, 19 валидируют её через таблицу, 2 имеют собственный guard членства, 2 (одна функция) — чистая арифметика, 5 — `pub unsafe fn` с `# Safety` (R17-CQ-09).
- **Intra-doc ссылки:** 1 329 целей, несуществующая одна (R17-CQ-04).
- **Константы:** повторяющиеся имена — это форварды (`SegmentLayout` зеркалит `os`/`size_classes`), значения не расходятся; исключение — `STATE_INITIALIZING` (R17-CQ-13). `SHARDS = 64` дважды — для независимых таблиц.
- **Ссылки `item N` в `src` (24):** цели верны, кроме трёх «78(c)» (R17-CQ-08).
- **Изменения R16 follow-up:** маска `segment_base_of_ptr` с `debug_assert_eq!(canonical_root_for(..))` в обоих местах согласована с R12-02; `Drop for Segment`, изоляция panic в `EpochRegion::drop` и const-assert `SMALL_CLASS_COUNT <= 64` по форме верны. Претензии к ним — только дублирование (R17-CQ-10) и устаревший соседний текст (R17-CQ-04, R17-CQ-07).

## Приложение A. Инвентарь

**A.1 Итог.** Файлов 168; физических строк 43 325; кода 17 059; комментариев 24 303 (doc 17 113); пустых 1 963.

**A.2 Группы (файлов / строк / код / комментарии).** `alloc_core` 88 / 24 802 / 9 693 / 14 039; `registry` 46 / 10 413 / 4 365 / 5 553; `global` 18 / 3 809 / 1 442 / 2 176; `concurrent` 14 / 3 554 / 1 343 / 2 049; `lib.rs` 1 / 524 / 84 / 424; `kani_proofs.rs` 1 / 223 / 132 / 62.

**A.3 Гистограмма размеров файлов (физ. строк).** 0–50: 40; 51–100: 20; 101–200: 32; 201–400: 35; 401–600: 18; 601–800: 15; 801–1000: 8; >1000: 0.

**A.4 Топ-20 файлов (физ. / код / коммент. / коммент÷код).**

| # | Файл (`src/…`) | физ | код | комм | ÷ |
|---|---|---|---|---|---|
| 1 | `alloc_core/segment/segment_table/segment_table_impl.rs` | 994 | 399 | 553 | 1.39 |
| 2 | `alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` | 973 | 498 | 457 | 0.92 |
| 3 | `registry/segment_route/directory.rs` | 909 | 739 | 102 | 0.14 |
| 4 | `alloc_core/platform/os.rs` | 898 | 265 | 597 | 2.25 |
| 5 | `alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs` | 895 | 342 | 527 | 1.54 |
| 6 | `alloc_core/large/alloc_core_large.rs` | 835 | 370 | 447 | 1.21 |
| 7 | `registry/heap_core/alloc/hot.rs` | 813 | 340 | 446 | 1.31 |
| 8 | `concurrent/sharded/sharded_region.rs` | 811 | 314 | 454 | 1.45 |
| 9 | `alloc_core/segment/segment_header/segment_header_impl.rs` | 763 | 201 | 540 | 2.69 |
| 10 | `alloc_core/small/alloc_core_small/find_segment.rs` | 750 | 399 | 324 | 0.81 |
| 11 | `global/fallback.rs` | 750 | 316 | 393 | 1.24 |
| 12 | `concurrent/epoch/epoch_region.rs` | 712 | 216 | 476 | 2.20 |
| 13 | `alloc_core/alloc_core/mem/mem_impl.rs` | 696 | 245 | 437 | 1.78 |
| 14 | `alloc_core/segment/segment_directory/segment_directory_impl.rs` | 682 | 237 | 420 | 1.77 |
| 15 | `alloc_core/alloc_core/alloc_core_impl.rs` | 671 | 118 | 521 | 4.42 |
| 16 | `registry/heap_core/diag/diag_probes.rs` | 666 | 244 | 392 | 1.61 |
| 17 | `registry/heap_core/diag/queries.rs` | 664 | 241 | 392 | 1.63 |
| 18 | `registry/heap_core/free/realloc.rs` | 659 | 154 | 499 | 3.24 |
| 19 | `global/tls_heap.rs` | 649 | 157 | 466 | 2.97 |
| 20 | `alloc_core/small/alloc_core_small_magazine.rs` | 648 | 269 | 364 | 1.35 |

**A.5 Функции.** 1 125 функций с телом. Гистограмма строк кода в теле: 1–10: 838; 11–25: 197; 26–50: 58; 51–100: 26; 101–150: 5; >150: 1. Топ-20 по строкам кода (код / охват / ≈CC / вложенность):

| # | Место (`src/…`) | fn | код | охват | CC | влож. |
|---|---|---|---|---|---|---|
| 1 | `alloc_core/small/alloc_core_small/find_segment.rs:186` | `find_segment_with_free_impl` | 222 | 427 | 34 | 6 |
| 2 | `alloc_core/large/alloc_core_large.rs:136` | `alloc_large` | 145 | 354 | 17 | 5 |
| 3 | `alloc_core/alloc_core/bootstrap.rs:46` | `primordial` | 135 | 380 | 8 | 2 |
| 4 | `alloc_core/large/alloc_core_large.rs:503` | `alloc_large_slow` | 131 | 198 | 20 | 4 |
| 5 | `alloc_core/small/alloc_core_small/reserve.rs:100` | `reserve_small_segment_impl` | 127 | 364 | 11 | 3 |
| 6 | `alloc_core/small/alloc_core_small_magazine.rs:154` | `refill_class_bump_impl` | 105 | 182 | 29 | 6 |
| 7 | `global/fallback.rs:144` | `heap_ptr_impl` | 99 | 184 | 13 | 6 |
| 8 | `alloc_core/alloc_core/mem/mem_impl.rs:255` | `dealloc_at_base` | 96 | 287 | 17 | 7 |
| 9 | `registry/heap_registry/claim.rs:198` | `claim_impl` | 96 | 149 | 16 | 5 |
| 10 | `registry/heap_core/free/realloc.rs:137` | `realloc` | 91 | 300 | 17 | 5 |
| 11 | `alloc_core/alloc_core/sidecar_drain.rs:88` | `drain_sidecar_ingress_bounded` | 90 | 97 | 19 | 6 |
| 12 | `registry/heap_core/free/dealloc_own_base.rs:341` | `dealloc_own_thread_with_base` | 84 | 216 | 10 | 6 |
| 13 | `alloc_core/alloc_core/mem/realloc_fastpath.rs:268` | `realloc_inplace_fast_path_known_base` | 83 | 186 | 18 | 4 |
| 14 | `alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:725` | `dbg_segment_state_reconciliation` | 83 | 170 | 14 | 4 |
| 15 | `alloc_core/alloc_core/lifecycle.rs:255` | `new_inner` | 81 | 117 | 4 | 2 |
| 16 | `alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:844` | `try_carve_batch` | 78 | 129 | 12 | 4 |
| 17 | `alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:526` | `try_drain_freelist_batch` | 75 | 112 | 17 | 3 |
| 18 | `registry/heap_core/alloc/batch.rs:71` | `alloc_batch` | 75 | 140 | 10 | 5 |
| 19 | `alloc_core/alloc_core/sidecar_drain.rs:191` | `drain_sidecar_ingress` | 73 | 77 | 13 | 7 |
| 20 | `registry/heap_core/free/dealloc_own_base.rs:195` | `small_free_guard` | 72 | 107 | 11 | 4 |

Топ по ≈CC за пределами таблицы: `alloc_small_with_virgin` (`alloc_core_small_impl.rs:293`) 24; `alloc_small` (`:163`) 21; `RouteDirectory::register` (`segment_route/directory.rs:789`) 21. ≈CC = 1 + число `if`/`while`/`for`/`loop`/`=>`/`&&`/`||`/`?` в теле без комментариев и строк. Вложенность — максимальная глубина `{}` внутри тела минус 1.

**A.6 Полный перечень R17-CQ-04 (ссылки в настоящем времени на несуществующие идентификаторы; 94 вхождения, 45 идентификаторов, 46 файлов; `src/…`).**

```text
alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs: 283 PerClassDirty; 284 dirty_segments; 285 DIRTY_BITMAP_WORDS
alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs: 332 dbg_force_decommit_retain
alloc_core/alloc_core/alloc_core_impl.rs: 18 dealloc_foreign_routing
alloc_core/alloc_core/counters.rs: 108 WASTED_DIRTY_DRAINS; 379 dealloc_foreign_routing; 407 DBG_LARGE_XTHREAD_RECLAIMED
alloc_core/alloc_core/mem/mem_impl.rs: 339 try_evict_to_fit; 411 magic_at; 589 large_layout_consistent
alloc_core/alloc_core/mem/realloc_fastpath.rs: 200 large_layout_consistent
alloc_core/large/alloc_core_large.rs: 312 magic_at
alloc_core/large/large_cache_extended.rs: 44 dirty_by_class, PerClassDirty; 50 dirty_by_class, PerClassDirty; 128 PerClassDirty
alloc_core/platform/mod.rs: 16 PerClassDirty
alloc_core/platform/sidecar.rs: 24 init_node_ids; 64 dirty_by_class, PerClassDirty; 66 PerClassDirty, dirty_by_class; 74 PerClassDirty; 86 ensure_per_class_dirty, get_per_class_dirty; 89 PerClassDirty
alloc_core/segment/bitmap/alloc_bitmap.rs: 9 free_list_contains; 25 local_free; 53 reclaim_offset
alloc_core/segment/mod.rs: 12 class_nonempty
alloc_core/segment/segment_directory/mod.rs: 1 class_nonempty
alloc_core/segment/segment_directory/segment_directory_impl.rs: 69, 207, 620 class_nonempty
alloc_core/segment/segment_header/segment_header_gen_table.rs: 25 realloc_inplace_fast_path; 130 decommit_empty_segment
alloc_core/segment/segment_header/segment_header_impl.rs: 306 owner_heap_id; 327, 333 reclaim_offset; 540 decommit_empty_segment
alloc_core/segment/segment_header/segment_header_layout.rs: 26 local_free
alloc_core/segment/segment_header/segment_header_views.rs: 87 register_segment
alloc_core/segment/segment_table/issue_transaction.rs: 16 IssueTicket
alloc_core/segment/segment_table/mod.rs: 23 segment_count; 48 decommit_empty_segment
alloc_core/segment/segment_table/segment_table_impl.rs: 540 decommit_empty_segment
alloc_core/small/alloc_core_small/dealloc.rs: 58 reclaim_offset
alloc_core/small/alloc_core_small/find_segment.rs: 275 deref_directory_sidecar_mut; 497 RingDrainOutcome
alloc_core/small/alloc_core_small/mod.rs: 17 dirty_by_class
alloc_core/small/alloc_core_small/reserve.rs: 51 finalize_orphaned_empty_segments
alloc_core/small/alloc_core_small_diag.rs: 363 reclaim_offset, reclaim_offset_checked
alloc_core/small/alloc_core_small_magazine.rs: 365 reclaim_offset; 385 prev_accepted; 403 decommit_empty_segment; 473 FLUSH_RUN_DETECT_CAP
alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs: 40, 224 reclaim_offset; 70, 75, 559 decommit_empty_segment; 345 finalize_orphaned_empty_segments
alloc_core/small/alloc_core_small_pool/decommit.rs: 119, 121, 214 reclaim_offset
alloc_core/small/mod.rs: 28 dirty_by_class; 31 reclaim_offset
global/fallback.rs: 579 acquire_lock
global/sefer_alloc/core.rs: 224 bind_slow
global/sefer_alloc/mod.rs: 103 reclaim_offset
registry/bootstrap/chunk.rs: 12 HeapOverflow; 84 ensure_slow
registry/bootstrap/ensure.rs: 146 rollback_chunk_sentinel
registry/bootstrap/loom_shim.rs: 317 push_free_slot
registry/bootstrap/mod.rs: 43 NEXT_FREE_TAIL; 61 push_free_slot, pop_free_slot
registry/bootstrap/registry.rs: 156 resolve_dirty_bit_target, resolve_heap_overflow
registry/heap_core/alloc/hot.rs: 262, 390 refill_class_stamped; 659 HeapOverflow
registry/heap_core/core.rs: 67 DBG_LARGE_XTHREAD_RECLAIMED; 157 bind_counters
registry/heap_core/diag/diag_probes.rs: 364 dbg_overflow_bitmap_clear_pass   (цитата правила CLAUDE.md — кандидат в HISTORICAL)
registry/heap_core/diag/queries.rs: 234 drain_heap_overflow
registry/heap_core/free/dealloc.rs: 147 large_layout_consistent
registry/heap_core/free/dealloc_batch.rs: 230 dealloc_foreign_routing
registry/heap_core/free/dealloc_own_base.rs: 126 large_layout_consistent; 186 reclaim_offset_checked
registry/heap_core/state/tcache_flush.rs: 63 maybe_decommit
```

Несуществующие `.rs`-имена в настоящем времени (96 вхождений, 20 имён): `alloc_core.rs` ×23, `alloc_core_small.rs` ×19, `alloc_core_small_pool.rs` ×13, `segment_header.rs` ×7, `heap_core.rs` ×7, `alloc_core_core_diag.rs` ×6, `src/global/sefer_alloc.rs` ×3, `heap_core_diag.rs` ×3, `segment_directory.rs` ×2, `dirty_by_class.rs` ×2, `src/alloc_core/alloc_core_large.rs` ×2; по ×1: `examples/r13_9_class_aware_dirty_sidecar_rss.rs`, `src/registry/heap_core_free.rs`, `src/alloc_core/alloc_core.rs`, `src/alloc_core/profile.rs`, `heap_core_free.rs`, `tests/regression_gen_wrap_boundary.rs`, `alloc_core/large.rs`, `src/registry/heap_core_diag.rs`, `xthread.rs`.

Ring-протокол как текущий (24 строки, 14 файлов; `src/…`): `alloc_core/config/small_segment_pool_config.rs:8`; `alloc_core/segment/bitmap/magazine_bitmap.rs:51`; `alloc_core/segment/segment_header/segment_header_meta_fields.rs:13`; `alloc_core/small/alloc_core_small/dealloc.rs:137`; `alloc_core/small/alloc_core_small/directory.rs:223, 229`; `alloc_core/small/alloc_core_small_magazine.rs:182, 190, 215, 233, 364`; `alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:80, 81, 175, 220, 239`; `alloc_core/small/alloc_core_small_pool/decommit.rs:112, 118`; `alloc_core/small/mod.rs:31`; `registry/heap_core/alloc/batch.rs:101`; `registry/heap_core/alloc/hot.rs:655`; `registry/heap_core/free/dealloc_batch.rs:229`; `registry/heap_core/free/dealloc_own_base.rs:190`; `registry/heap_core/free/realloc.rs:372`.

## Приложение B. Команды и скрипты (все только читают файлы; запускались из корня worktree)

Скрипты выполнялись через `node -e`; ниже — в развёрнутом виде. Общая функция `strip(t)` заменяет пробелами комментарии `//` и `/* */`, строковые литералы (включая raw и byte) и char-литералы, сохраняя переводы строк, так что позиции и номера строк не сдвигаются. `walk(d)` — рекурсивный список `*.rs`.

**B.1 Перепись строк.** Вход — `git ls-files src`. Физические строки — `split(/\r?\n/)` без хвостовой пустой; «код» — непустые строки, у которых `trim()` не начинается с `//`; «комментарий» — начинающиеся с `//`; «doc» — с `///` или `//!`.

**B.2 Метрики функций.**

```text
for each match of /\bfn\s+(\w+)/ in strip(file):
  body = from the first '{' at paren-depth 0 to the matching '}'  (';' first => no body)
  code = non-blank lines of strip(body)
  cc   = 1 + count(\bif\b, \bwhile\b, \bfor\b, \bloop\b, =>, &&, ||, ?)
  nest = max brace depth inside body - 1
```

**B.3 Идентификаторы в комментариях, которых нет в коде.**

```text
ids = every [A-Za-z_]\w* in strip() of src, crates, tests, benches, examples (+ file stems)
for each comment line in src/**: for each `x::y::z` / `name()` in backticks:
  for each segment with '_' or CamelCase compound: if !ids.has(seg) -> stale
  context (prev 2 + next line) matches /remov|retir|former|used to|\bwas\b|\bwere\b|no longer|
    historical|\bpre-|legacy|deleted|replac|supersed|\bgone\b|\bold\b|previous|dropped|
    cutover|until task|originally/i -> "historical", else "present-tense"
```

Результат: 94 present-tense / 45 идентификаторов / 46 файлов; 47 historical.

**B.4 Несуществующие `.rs` в комментариях.** То же для `/[A-Za-z0-9_\/-]+\.rs\b/` против полного списка файлов репозитория (полный путь для `src/|tests/|crates/|benches/|examples/`, иначе — имя файла). Исключены `src/imp.rs` (путь относительно крейта) и `//docs.rs` (URL). Результат: 96 present-tense / 20 имён / 45 файлов; 95 historical.

**B.5 Ring-протокол.** `grep -rnE '^\s*//.*\b(ring[- ]drain|remote-free ring|ring entr(y|ies)|the ring\b|rings\b|RemoteFreeRing|ring_drain)' src/` (25 строк в 15 файлах), затем фильтр `grep -viE 'remov|retir|former|legacy|no longer|was |were |old |replac|superseded|cutover|histor'` — остаётся 24 строки в 14 файлах.

**B.6 Подавления.** `grep -rhoE '#!?\[(cfg_attr\([^]]*,\s*)?allow\(([^]]*)\)' src/ | grep -v 'allow(unsafe_code)'`, разбить по запятым, `sort | uniq -c`; места — `grep -rnE '#!?\[(cfg_attr\([^]]*,\s*)?allow\([^]]*(dead_code|unused_imports|unused_variables|unused_mut|clippy::)' src/`.

**B.7 Дубликаты.** Нормализованные строки кода (trim, схлопнутые пробелы, без строк из одних `{}();,`, без комментариев и атрибутов); окна по W строк длиной ≥200 символов (W=8) или ≥170 (W=5); перекрывающиеся окна одного файла схлопываются.

**B.8 Механические перечни (grep).**

```text
unsafe tiers : grep -rnE '^\s*#!\[allow\(unsafe_code\)\]' src/ | wc -l     -> 21
               grep -rnE '^\s*#\[allow\(unsafe_code\)\]'  src/ | wc -l     -> 83
abort sites  : grep -rnE 'process::abort\(\)' src/ | grep -vE '^[^:]+:[0-9]+:\s*//' | wc -l   -> 106 (25 files: `cut -d: -f1 | sort -u`)
gen table    : grep -rnE 'bump_gen|init_gen_table_in_place' src/ | grep -vE '^[^:]+:[0-9]+:\s*//' | wc -l   -> 13
               grep -rnE 'gen_at\b' src/ | grep -vE '^[^:]+:[0-9]+:\s*//'    -> 2 (definition + re-export only)
doc fences   : grep -rnE '^\s*//[/!]\s*```' src/        -> 20 x ```text + 20 closing
inline tests : grep -rnE '#\[cfg\((all\()?test|^\s*mod tests\b|#\[test\]' src/   -> 0
markers      : grep -rnE '\b(TODO|FIXME|XXX|HACK)\b' src/                       -> 0
predicate    : grep -rn 'all(feature = "large-reserved-capacity", not(feature = "numa-aware"))' src/ | wc -l -> 18
dbg census   : grep -rnE '^\s*pub (unsafe )?(const )?fn dbg_' src/ | wc -l     -> 246 (30 unsafe)
for_test     : grep -rhoE '^\s*pub (unsafe )?(const )?fn [a-z_0-9]+_for_tests?\b' src/ | grep -v 'fn dbg_' | wc -l -> 48
doc(hidden)  : grep -rnE '#\[doc\(hidden\)\]' src/ | wc -l                     -> 455 (67 files)
```

**B.9 Функции без вызовов в `src`.** Для каждого `fn NAME` вне блоков `impl … for` и `trait` (заголовок охватывающего `{` определяется по тексту между предыдущими `;`/`{`/`}` и самим `{`) считаются вхождения `NAME` в `strip()` всего `src` минус число определений; при нуле — вхождения в `tests`/`benches`/`examples`. Совпадения, требующие ручной проверки, перепроверены поиском по репозиторию (`.rs/.mjs/.toml/.yml`).

**B.10 «Один файл — один экспорт».** В каждом не-`mod.rs`/`lib.rs` файле — элементы верхнего уровня (глубина `{}` = 0) с `pub` (отдельно `pub(crate|super|in …)`) видов `fn|struct|enum|trait|type|const|static|mod|use|union`. Результат: 134 файла; 14 с ≥2 элементами `pub`; 40 с ≥2 с учётом ограниченной видимости.
