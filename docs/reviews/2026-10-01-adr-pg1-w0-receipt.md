# PG-1 / W0: приёмочная записка — проверка гипотезы MODEL-LIMIT (ADR)

Дата: 2026-10-01. Worktree: `worktrees/ph2-pg1`, ветка `ph2-pg1`.

## 1. Цель и гипотеза

Гипотеза ADR (класс MODEL-LIMIT): внутри жизни by-value кадра, освободившего
`Box`, ЛЮБАЯ повторная выдача тех же байт общей reservation через root-derived
указатель (own-thread tcache reuse) красная в Stacked Borrows (SB) и Tree
Borrows (TB), независимо от паузы и размера. До этой задачи прогноз не
проверялся. Задача — эксперимент.

## 2. Метод

Новый harness-free тест `tests/r11_w0_box_reuse.rs` (установленный
`#[global_allocator] SeferAlloc`, настоящий `Box<T>`, один сценарий на процесс,
selector — первый аргумент, `harness = false`, `test = false`,
`required-features = ["alloc-global", "internals", "bench-internals"]`).
Сценарии, для каждого N ∈ {1, 7, 16, 17, 32} (const generic `Box<[u8; N]>`),
2 раунда × 4 внутренних alloc/free итерации:

- `w0` — reuse ВНУТРИ живого by-value кадра: `#[inline(never)] fn
  consume_then_alloc<const N>(b: Box<[u8; N]>)`: `drop(b)` → `Box::new` →
  запись/чтение через `c`.
- `w0-after` — reuse после возврата by-value кадра `consume_only`.
- `w0-same-frame` — контроль без by-value аргумента (drop и новый Box в одном
  кадре).

Команды (каждая клетка — отдельный процесс, `-j 1` под Miri):

```text
cargo test --features "<набор>" --test r11_w0_box_reuse -- <selector>
cargo +nightly miri test --features "<набор>" -j 1 --test r11_w0_box_reuse -- <selector>
cargo +nightly miri test --features "production internals bench-internals" -j 1 --test miri_global_box_acceptance -- paused
```

MIRIFLAGS SB: `-Zmiri-strict-provenance -Zmiri-disable-isolation
-Zmiri-report-progress=10000000`; TB: то же + `-Zmiri-tree-borrows`.
Наборы: A = `production internals bench-internals`, B = `alloc-global
internals bench-internals`. Перед каждым запуском тестов: `touch tests/*.rs`,
`CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/target/wt`.

Toolchain: `rustc 1.97.0 (2d8144b78 2026-07-07)`, `miri 0.1.0 (3659db0d3e
2026-07-05)` (`cargo +nightly miri --version`), host `x86_64-pc-windows-msvc`.

## 3. Результаты (18 клеток = 3 сценария × {native, SB, TB} × {A, B}, N агрегирован)

Везде native: exit 0, маркер `COMPLETE`, reuse-счётчик **40/40** (8/8 на каждый
N: 2 раунда × 4 итерации) для всех трёх сценариев на обоих наборах — адрес
`c` совпал с адресом `b` во ВСЕХ попытках, т.е. reuse реально происходит.

| Сценарий | Режим | A (production) | B (alloc-global) |
|---|---|---|---|
| w0 | native | зелёный, COMPLETE | зелёный, COMPLETE |
| w0 | Miri SB | **красный**, exit 1 | **красный**, exit 1 |
| w0 | Miri TB | **красный**, exit 1 | **красный**, exit 1 |
| w0-after | native | зелёный, COMPLETE | зелёный, COMPLETE |
| w0-after | Miri SB | зелёный, COMPLETE | **красный**, exit 1 (см. §5) |
| w0-after | Miri TB | зелёный, COMPLETE | **красный**, exit 1 |
| w0-same-frame | native | зелёный, COMPLETE | зелёный, COMPLETE |
| w0-same-frame | Miri SB | зелёный, COMPLETE | **красный**, exit 1 |
| w0-same-frame | Miri TB | зелёный, COMPLETE | **красный**, exit 1 |

Ключевые строки логов (сырые логи: `D:/dev/rust/sefer-alloc/target/wt/logs/pg1/`,
вне репозитория):

- SB A w0: `not granting access to tag <1263> because that would remove
  [Unique for <363628>] which is weakly protected` (sb_A_w0.log:66).
- TB A w0: `write access through <1216> (root of the allocation) … is
  forbidden`; `<1216> … is foreign to the protected tag <361581>` (tb_A_w0.log:56).
- SB B w0-after: тот же SB-текст; место — `src\alloc_core\platform\node.rs:90`
  (sb_B_w0-after.log:56).
- TB B: `write access through <1414> (root of the allocation) … is forbidden`.

## 4. Механизм красных клеток на наборе A (гипотеза подтверждена)

Наблюдается (Miri, SB и TB, w0): UB возникает при `Box::new` внутри живого
кадра `consume_then_alloc` (tests/r11_w0_box_reuse.rs:20), ещё ДО любой записи
через `c`. Аллокатор на alloc-пути (tcache reuse) пишет в тело переиспользуемого
блока через **root-тег reservation** (тег создан `System.alloc` в
`crates/aligned-vmem/src/os/miri.rs:21`). SB: эта запись сняла бы
`Unique`-защищённый тег by-value аргумента `b` («weakly protected» — protector
живого кадра). TB: root-запись «foreign» к защищённому `Reserved`-тегу `b` и
запретила бы его (Disabled). Т.е. red-конфликт создаёт alloc-путь, а не
тестовая запись: гипотеза MODEL-LIMIT на наборе A ПОДТВЕРЖДЕНА. Контрольные
`w0-after`/`w0-same-frame` на A зелёные (6/6 клеток на A: 2 сценария × {native, SB, TB}; перепрогнаны оркестратором на финальном файле в SB и TB) — без живого protector'а тот же reuse
легален.

## 5. Отдельная находка: набор B — красные контроли

На B (`alloc-global` без `production`) красны ВСЕ три сценария, включая
`w0-after`/`w0-same-frame`. Механизм по логу sb_B_w0-after.log: UB в
`Node::write_next` (`src/alloc_core/platform/node.rs:90`,
`ptr.write_unaligned(next)`), вызванном из `dealloc_small`
(`src/alloc_core/small/alloc_core_small/dealloc.rs:118`) ← `dealloc_at_base` ←
`dealloc_with_base` — интрусивная freelist-запись происходит **немедленно в
dealloc**, внутри кадра `Box::drop`, и конфликтует с protector'ом
освобождаемого `Box`. Т.е. на B запись не отложена терминальной публикацией
(как на A), поэтому красным становится сам `drop(Box)`. Это НЕ часть гипотезы
MODEL-LIMIT (про live-кадр и reuse), а самостоятельный факт о наборе B.

## 6. Перепрогон paused-свидетеля на наборе A

`miri_global_box_acceptance -- paused`, набор A: SB **красный**, exit 1
(`not granting access to tag <1263> … weakly protected`, место —
`node.rs:90:18`, protector — `drop(byte)` в
tests/support/r8_global_box_witness.rs:113, поток producer); TB **красный**,
exit 1 (`write access through <1216> (root) … is forbidden`). Совпадает с
известным исходом на B: пишущая функция — `Node::write_next` через root-тег
reservation.

## 7. Контрфактуалы

- Reuse-свидетельство: native-счётчик совпадений адреса `c` vs `b` = 40/40 на
  сценарий в каждом наборе (итого 240/240 попыток по всем прогонам) — тест
  действительно наблюдает reuse, а не разные блоки.
- Клетки «красные» — именно на доступе аллокатора/через `c` при живом
  protector'е; «зелёные» контроли проходят тот же объём записей/чтений через
  `c` без UB, т.е. краснота w0 обязана живому by-value кадру.
- Целостность файлов: sha256 `tests/r11_w0_box_reuse.rs` после всех прогонов
  `7bf712291b6604468d84f4a24353b7cca77e1f9399c52675f846b0f2c93a9f4b`;
  `Cargo.toml` — `8f1be0806a3469ed69587e2f0f0ab729163b3c64ce9e4253052ddd3b934fde4e`
  (финальные версии; единственная правка после Miri-прогонов — clippy-фикс
  `&mut *c` → `&mut c` без изменения логики, после чего native-прогоны
  повторены на финальном файле: 3/3 COMPLETE). Хэш самой записки в себя не
  включается.

## 8. Качество

`rustfmt --check --edition 2021 tests/r11_w0_box_reuse.rs` — чисто.
`cargo clippy --test r11_w0_box_reuse --features "production internals
bench-internals" -- -D warnings` — чисто. `cargo clippy --all-targets`
(default features) — чисто. `required-features` держит цель вне default-сборки
(обычные `cargo test` без фич её не строят; `test = false` защищает от
пустого selector'а).

## 9. Границы охвата и что НЕ подтверждено

- Host: Windows (`x86_64-pc-windows-msvc`); Linux/macOS не прогонялись.
- Miri: интерпретатор miri 0.1.0 (nightly 2026-07-05); поведение строгих
  SB/TB может меняться между nightly.
- Один процесс = один сценарий; producer/owner — максимум 2 потока (w0
  сценарии однопоточные; paused — 2 потока).
- N-набор {1, 7, 16, 17, 32}, 2 раунда × 4 итерации; большие классы
  (Large/резервные спаны) не охвачены.
- Набор B: краснота контролей наблюдалась, но её полная причинная модель
  (какие пути B включают немедленную freelist-запись при включённой
  терминальной публикации) требует отдельного разбора — зафиксирован только
  наблюдаемый стек (dealloc_small → write_next).
- Причина зелёности `narrow`-свидетеля на B при красных контролях w0 не
  устанавливалась (narrow на B в этой задаче не перегонялся).
