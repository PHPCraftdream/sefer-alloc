# PG-2: приёмочная записка спайка внетельного учёта свободных блоков (off-body)

Дата: 2026-10-01. Worktree: `worktrees/ph2-pg2`, ветка `ph2-pg2`. ADR: `docs/design/2026-10-01-adr-physical-boundary-and-progress.md`. Одноразовый спайк, не для слияния без приёмки.

## 1. Окружение и идентичность

- Host: Windows x86_64 (`x86_64-pc-windows-msvc`). Native: `rustc 1.97.0 (2d8144b78 2026-07-07)`. Miri: `miri 0.1.0 (3659db0d3e 2026-07-05)` (`cargo +nightly miri --version`).
- `CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/target/wt`; перед каждым прогоном `touch tests/*.rs`.
- MIRIFLAGS SB: `-Zmiri-strict-provenance -Zmiri-disable-isolation -Zmiri-report-progress=10000000`; TB: то же + `-Zmiri-tree-borrows`. Protector'ы не отключались.
- Сырые логи (вне репо): `D:/dev/rust/sefer-alloc/target/wt/logs/pg2/`.
- `git diff | sha256sum` = `078f1e413a33aec7da4f365e8f7426454df732a9408576b8eaa6ffaa73c563e7`.

## 2. Что заменено / не заменено

Замена: интрусивный freelist (`next` в теле блока) → **NextTable** — внетельный per-class LIFO-список: один `u32 next` (смещение или `NULL=0`) на `MIN_BLOCK`-слот сегмента, в метаданных по `Layout::next_table_off()`; head остаётся в `BinTable[class]`. Семантика цепи байт-в-байт как у интрусивной (push на голову, walk по next), только `next` живёт в метаданных. Retire (reclaim_sidecar_record, dealloc_small, flush_run) пишет только метаданные; pop/drain читает только метаданные. Тело блока на всех путях retire/refill не читается и не пишется.

Места (файл:строка после правки):
- `src/alloc_core/small/alloc_core_small_reclaim.rs` — `reclaim_sidecar_record`: `write_next` → `next_table().set_next` (строки ~54–65).
- `src/alloc_core/small/alloc_core_small/dealloc.rs` — `dealloc_small`: `write_next` (быв. :118) → `set_next` (строки ~105–120).
- `src/alloc_core/small/alloc_core_small_magazine.rs` — `flush_run`: цепочка `write_next` (быв. :615) → `set_next` (строки ~600–612).
- `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` — `pop_free` и `try_drain_freelist_batch`: `read_next` (быв. :383/:555/:583) → `next_table().next` (строки ~360–470); scan-вариант из первой итерации спайка удалён.
- `src/alloc_core/small/alloc_core_small_diag.rs` — `dbg_corrupt_freelist_head_next`: интрусивного слова больше нет; хук — no-op, возвращающий наличие свободного блока класса (строки ~145–165).
- `src/alloc_core/platform/node.rs` — `Node::write_next`/`read_next` НЕ удалены (нужны для мутант-клетки), помечены `#[allow(dead_code)]`; вызовов вне node.rs в src нет (кроме kani_proofs.rs, не компилируется вне kani).

Новые файлы: `src/alloc_core/segment/next_table.rs` (`NextTable`, без unsafe — доступ через seam `Node::read/write_u32_unaligned`/`Node::zero`); проводка: `Layout::next_table_off()` (`segment_header_layout.rs`), `SegmentMeta::next_table()` (`descriptors.rs`), miri-only zero-init в `bootstrap.rs` (primordial) и `reserve.rs` (small). `small_meta_end()` вырос на `NextTable::FOOTPRINT` = 1 МиБ на сегмент 4 МиБ (наивная геометрия спайка; ADR §Решение предполагает однородные per-class leaves в реальном дизайне — запись о деградации RSS, не perf).

Инварианты: класс блока (per-class цепи 1:1 как раньше), live_count/credit (inc_live/add_live/dec_live/sub_live не менялись), magazine_bitmap (не трогалась), decommit/pool (reset ставит heads в NULL; устаревшие слоты NextTable недостижимы), Primordial/Small (payload_start-логика не менялась).

Честные ограничения: `alloc_bitmap` остался только двойной-фри-гардом; поиск свободных бит (tzcnt) не реализован — вместо него точный per-class список; диагностический хук `dbg_*` для интрусивного next деградировал до no-op.

## 3. Команды

```text
cargo +nightly miri test --features "<A|B>" -j 1 --test miri_global_box_acceptance -- paused
cargo +nightly miri test --features "production internals bench-internals" -j 1 --test r11_pg2_model_limit -- <seg-release|reissue|large>
cargo test --features "production internals bench-internals" --test <наборы>
cargo clippy --all-targets --features "production internals bench-internals" -- -D warnings
rustfmt --check --edition 2021 <изменённые .rs>
```
A = `production internals bench-internals`, B = `alloc-global internals bench-internals`.

## 4. Таблица клеток

Baseline до спайка (SB A/B — прогон до правок; TB A/B — перегон на чистом HEAD после того, как первые TB-прогоны попали на середину правок и были невалидны):

| Клетка | Режим | До спайка | После спайка |
|---|---|---|---|
| paused A | Miri SB | красный, exit 1: `not granting access to tag <1263> … weakly protected`, место `node.rs:90:18` (write_next ← reclaim_sidecar_record) | **зелёный**, COMPLETE |
| paused B | Miri SB | красный, exit 1, тот же текст, `node.rs:90:18` | **зелёный**, COMPLETE |
| paused A | Miri TB | красный, exit 1 | **зелёный**, COMPLETE |
| paused B | Miri TB | красный, exit 1 | **зелёный**, COMPLETE |

Мутант (временный `Node::write_next` в `reclaim_sidecar_record`, затем откат; sha256 reclaim.rs до мутанта = после отката `e8264fd1…c91d`, у мутанта `57520758…7a1d`): paused A SB **красный**, exit 1, `error: Undefined Behavior: not granting access to tag <1263> because that would remove [Unique for <888821>] which is weakly protected`, место — `src/alloc_core/platform/node.rs:94:18` (write_next). ✔ покраснел именно на node.rs.

MODEL-LIMIT (набор A, новые harness-free свидетели `tests/r11_pg2_model_limit.rs`, installed SeferAlloc):

| Клетка | SB | TB |
|---|---|---|
| (i) trim/release сегмента при паузе (`seg-release`) | **зелёный**, COMPLETE | **зелёный**, COMPLETE |
| (ii) reissue при паузе (`reissue`) | **красный**, exit 1: `not granting access to tag <1263> … weakly protected`, UB в `tests/r11_pg2_model_limit.rs:93:21` — `Box::new(i)` владельца, получающего те же байты (root-тег из `crates/aligned-vmem/src/os/miri.rs:21`) | **красный**, exit 1: `write access through <1216> (root of the allocation) … is forbidden`, то же место :93:21 |
| (iii) Large при паузе (`large`) | **красный**, exit 1: `deallocating while item [Unique for <889019>] is weakly protected`, UB в `std::sys::alloc::windows.rs:199` (`HeapFree`) — физическое освобождение reservation при паузе | **красный**, exit 1: `deallocation through <751971> (root of the allocation) … is forbidden`, то же место |

(i) зелёный — наблюдённая находка: после спайка trim/release сегмента не трогает тело блока; физическое освобождение сегмента под Miri тело не затрагивает. (ii)/(iii) красные ровно в месте reissue/release, не в retire — по пред-регистрации ADR C4. Замечание к (iii): диагностика указывает на сам `HeapFree` (release reservation), функция освобождения — путь release Large у владельца.

Проценты: paused 4/4 зелёных (знаменатель 4); мутант 1/1 красный; MODEL-LIMIT: 1/3 клеток зелёные, 2/3 красные с подтверждённой причиной (знаменатель 3, по 2 режима на клетку — итог 2 зелёных / 4 красных из 6 прогонов).

## 5. Нативные тесты

Набор `production internals bench-internals`, все зелёные (exit 0):
- обязательный список: `r8_global_box_provenance`, `r8_terminal_global`, `r6_terminal_owner_drain`, `r8_owner_sidecar_miss`, `r6_class_issue` — 23 passed / 0 failed;
- расширенные: `freelist_reuse`, `regression_batch_freelist_drain`, `regression_flush_class_unsafe_boundary`, `regression_magazine_oracles`, `regression_magazine_scan_bounds`, `regression_r1_01_magazine_free_budget`, `heap_core_tcache`, `batch_tcache`, `r10_active_kind_small_recycle`, `regression_paused_owner_multisegment` — все ok.

Красные тесты спайка (не маскировались):
- `regression_freelist_next_validation` (features `hardened internals`): 2 теста FAILED — `pop_free_rejects_out_of_segment_next` (assert :110) и `drain_freelist_batch_rejects_out_of_segment_next` (assert :179). Причина: тесты проверяют hardened-гард обрезки интрусивной цепи; цепи больше нет, head стал флагом/точным head новой цепи, `dbg_corrupt_freelist_head_next` — no-op. Тесты требуют переработки под off-body модель (вне рамок спайка).

## 6. Качество

- `rustfmt --check --edition 2021` на всех изменённых .rs — чисто.
- `cargo clippy --all-targets --features "production internals bench-internals" -- -D warnings` — чисто (exit 0).
- `cargo clippy --all-targets -- -D warnings` — чисто (exit 0).
- `cargo check` на наборах A, B, default, `--all-features` — без ошибок и предупреждений спайка (3 pre-existing deprecated-предупреждения `fetch_update` не от спайка).

## 7. sha256 изменённых/новых файлов (финальные)

- `src/alloc_core/segment/next_table.rs` (новый) `165f3a22be13ec3504253d71c43fedb874a0e4e29097bd07a9c7ec3d0d2cca26`
- `src/alloc_core/small/alloc_core_small_reclaim.rs` `e8264fd14282799574b9e6e6b29837543dbea9fb545e6bbdb4dd53c74136c91d`
- `src/alloc_core/platform/node.rs` `d22ae79379970b6236da8ef862b307f4920627d5511727be0afcb990040f30b5`
- `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` `cf368596c6aa2ad63b9a84d1c732b1c042967e839a4b6274d298d68a9401ddc7`
- `src/alloc_core/small/alloc_core_small/dealloc.rs` `fd430bdd697741290ecd1e12bf31e30cb56482345752030e5589cb9ab74b44e6`
- `src/alloc_core/small/alloc_core_small_magazine.rs` `bf4c7d4022b2aa947ca837d16a950c7e493c71f8054d36c2be5f612525e163a1`
- `src/alloc_core/small/alloc_core_small_diag.rs` `d06b8f5fd041f7e970333631e9308571a266ebf2329d89ec53cf08d8bcca4770`
- `src/alloc_core/segment/mod.rs` `871214d23e1aeacf01907d2198112c83539415e6f84dd0c176aa3a218e61252b`
- `src/alloc_core/segment/segment_header/segment_header_layout.rs` `07481a2af7db9bccab3cfb401f95d705ebd43fb1c077f40a0be34d50bed6be94`
- `src/alloc_core/segment/segment_header/descriptors.rs` `128f7b6ce2869bbe9f49100e48d6fea0b6a413d55b8013ae5bf3c12f39a059a1`
- `src/alloc_core/alloc_core/bootstrap.rs` `76d41a610216261a32562707fe5b573ef8ed349f43b1cb1b3ceca74613571aad`
- `src/alloc_core/small/alloc_core_small/reserve.rs` `bb7bdfa3efe1bbdcfc4edcd1aae86a372bf3c6b8da214d53201ad528167fff93`
- `Cargo.toml` `f29315b5c4067f4aac07f8885e264cec3301007e3ef77c11ddc9b3851089e7e2`
- `tests/r11_pg2_model_limit.rs` (новый) — sha256 после финальных прогонов не фиксировался отдельно (после прогонов менялся только оракул reissue; финальная версия (с `kept`-вектором; ею подтверждены reissue SB/TB и large SB/TB) `109d750294cd2cef5dc5b3bb77d540024c4aedf1624058087b236d7b1a965665`; см. п.9).

## 8. Отклонения метода и находки процесса (честно)

- Первые две итерации спайка: (1) scan по `alloc_bitmap` с class-фильтром — забракован самим спайком: класс блока не восстанавливается из bitmap, 16-байтовый блок по кратному 32 смещению выдавался 32-байтовому классу → двойная выдача (наблюдено на наборе B: UB при `thread::spawn`, `alloc815[0x403a60]`); (2) первая версия NextTable — корректна, но попала под чужой `#[cfg]` в `segment/mod.rs`, из-за чего часть прогонов шла на сборке без/с лишним модулем. Итоговые клетки — на финальной версии с безусловным `pub(crate) mod next_table`.
- Один фоновый прогон paused-клеток был частично испорчен параллельными правками исходников и был перегнан полностью; его результаты в записку не включались. Отдельный забытый 20-минутный прогон (v2) также исключён из доказательной базы.
- Baseline TB-клетки: первый прогон попал на середину правок (compile error в логах), перегнан на чистом HEAD — красные, как и ожидалось.

## 9. Границы охвата и что НЕ подтверждено

- Только Windows host; Linux/macOS не прогонялись. Miri nightly 2026-07-05.
- Производительность не измерялась (PG-3): факт деградации — 1 МиБ метаданных на сегмент (RSS) и O(1)-цене pop/drain при цене +24 байта на сегмент-заголовок-геометрию; iai-плеч нет.
- (i) seg-release зелёный: подтверждает отсутствие доступа к телу на пути trim/release под Miri, но не подтверждает, что сценарий реально освобождает сегмент (пула/релиза можно не дождаться на 1-сегментном процессе) — проверялась отсутствие UB, не сам release-факт.
- (iii) Large красный в `HeapFree` — место release; точный вызов владельца (release vs кэш eviction) не дизассемблировался, но красный именно на dealloc reservation, не в retire.
- `regression_freelist_next_validation` (hardened) требует переработки; остальные cfg-gated тесты на интрусивную цепь (kani) не компилировались.
- Mutant-клетка выполнена только на SB (TB-мутант не прогонялся).
- Оракул reissue требует `kept`-вектора (tcache-рециркуляция без него не доходит до retire-блока) — оракул одноразовый, для приёмки требует доработки.

## 10. Статус интеграции (проверка владельцем worktree)

- Спайк-код НЕ влит в `main`: он живёт в ветке `ph2-pg2` (коммит `8a028616`) как справочный материал для Ph3; в `main` перенесена только эта записка.
- Независимая перепроверка: paused-witness `miri_global_box_acceptance -- paused` на наборе A под Miri SB зелёный (`COMPLETE paused_terminal_owner_retirement`, exit 0), результат совпал с таблицей §4.
- Отклонения, которые обязан закрыть Ph3 до приёмки: клетка (i) `seg-release` зелёная, а не красная, как предполагал ADR (требует проверки, что сценарий реально освобождает сегмент); `regression_freelist_next_validation` (hardened) переписать под off-body модель; геометрия NextTable (1 МиБ метаданных на 4 МиБ сегмента) проверяется в PG-3 против RSS-предела ADR ≤ 1.05.
