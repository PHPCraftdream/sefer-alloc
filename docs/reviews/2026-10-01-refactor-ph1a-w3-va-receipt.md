# Фаза 1a: W+3 (re-issue удержанного cap) и VA/incarnation overlap — приёмочная записка

Дата: 2026-10-01. Только изолированные installed-System свидетели в `tests/`; `src/` не менялся. НЕ Sefer shipping, НЕ release GO.
Источник: база `main` 859683f5 + незакоммиченные файлы ниже (worktree `refactor-ph1a`).

## Окружение и идентичность

- Native: `rustc 1.97.0 (2d8144b78 2026-07-07)`, Windows 10 x86_64 MSVC, `System` = HeapAlloc.
- Miri: `miri 0.1.0 (3659db0d3e 2026-07-05)`, `rustc 1.99.0-nightly (3659db0d3 2026-07-05)`; SB = по умолчанию, TB = `MIRIFLAGS=-Zmiri-tree-borrows`. Protector'ы и валидация не отключались.
- `CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/target/wt`, перед прогонами `touch tests/*.rs`. Сырые логи (вне репо, все < 200 KiB): `D:/dev/rust/sefer-alloc/target/wt/logs/ph1a/` (`native-*`, `miri-*`, `cf1-*`, `cf2-*`).
- sha256 (итоговые, без мутаций; до и после контрфактуалов совпали):
  - `tests/r11_box_cap_gate_w3.rs` 929dfe54f2fca90c6aeddae4928684fe9b245e5feddc80e933fd8d780fa784d2
  - `tests/r11_box_cap_gate_va.rs` fb65d1c18c40d2864f08e805f859f30b5f5130118c53ad02d10e65acd66f5068
  - `tests/support/r11_box_cap_reissue.rs` 0f4c202051d9eba5a5048e0e8cf9e0a23a4fc191936c1cdcc12c4cb19139da08
  - `Cargo.toml` 523cbc833ce9dd430fc4cee433233cc5f58c23caec40e3c58a504ba2bf8718d9 (только +2 записи `[[test]]` по образцу w1/w2, `harness=false`, `test=false`, `required-features=["std"]`)
  - W1/W2 и их support не менялись (f9959efe…, 87f6900e…, d830824e…).
- Добавление сценария (C) `reissue-live-across` (ревью координатора) изменило w3 и support; все Miri/native-прогоны (A)/(B)/VA выполнены до этого добавления и логически не затронуты (код (A)/(B)/VA не менялся), (C) прогнан на итоговых файлах.
- Оговорка: единственная правка после первых Miri-прогонов w3 — `rustfmt` переноса аргументов `println!` в `run_va` (семантически нейтральна). Прогоны Miri `va` и native перепроверены уже на итоговом файле; Miri w3 live/после — на файле до этой правки.

## Что реализовано

`Gate` (System-обёртка): терминальный `dealloc` кладёт входящий pointer в `cap`, делает `live: 1->0`, паузится внутри настоящего `Box::drop` (`PUBLISHED`/`RESUME`). Owner решает судьбу cap сам.

- W+3 `reissue::<N>`: `Box::from_raw(cap)` БЕЗ физического dealloc, запись нового паттерна, чтение/сверка, затем обычный `drop(Box)` (один физический `System.dealloc` с точным Layout). Перед этим маршрут `target` снимается (иначе новый drop попал бы в паузу-ловушку).
  - (A) `reissue-live`: re-issue, пока producer ещё внутри `Box::drop` (protector жив); после — `RESUME`, join.
  - (C) `reissue-live-across` (ревью: настоящий риск W+3): re-issue через `Box::from_raw(cap)` пока producer приостановлен; fresh Box остаётся ЖИВЫМ через `RESUME.wait()` и `producer.join()` (кадр старого `Box::drop` завершается над живым новым объектом); после join запись нового паттерна (`0xA5+round`), проверка, затем `drop(fresh)` — единственный физический free. В отличие от (A), где fresh дропается до выхода кадра producer'а.
  - (B) `reissue-after`: сначала `RESUME` + join (кадр завершён), потом re-issue.
  - Негативный контроль (только `cfg(miri)`) `reissue-live-root`: то же (A), но через исходный root вместо cap.
  - Размеры 1..7 (`Box<[u8; N]>`, align 1), 2 раунда.
- VA `va-overlap` (W+2-подобная схема, owner физически освобождает cap): per round owner делает `System.dealloc(cap)`, затем `alloc_zeroed` того же Layout (фаза "live": producer ещё в кадре), освобождает, `RESUME`+join, затем снова `alloc_zeroed` (фаза "after"). Out-of-band метка `Incarnation{addr, generation}` (generation — глобальный монотонный счётчик на каждую выдачу). Если новый VA == старый VA, `old.accepts(new)` обязан быть false (ABA-различимость); внутри assert, плюс арифметика `rejected == live_reuse + after_reuse` и `rounds == 64`. Размеры 1..8 (align 1) x 8 раундов = 64 раунда на запуск.

## Команды

```text
cargo test --test r11_box_cap_gate_w3 --features std -- reissue-live | reissue-after
cargo test --test r11_box_cap_gate_va --features std -- va-overlap
cargo +nightly miri test --test r11_box_cap_gate_w3 --features std -- reissue-live | reissue-after | reissue-live-root
cargo +nightly miri test --test r11_box_cap_gate_va --features std -- va-overlap
(то же с MIRIFLAGS="-Zmiri-tree-borrows"; VA-вариант rate1: добавлено -Zmiri-address-reuse-rate=1.0 -Zmiri-address-reuse-cross-thread-rate=1.0)
```

## Таблица исходов (все клетки наблюдены [нбл])

| Сценарий | native | Miri SB | Miri TB |
| --- | --- | --- | --- |
| W+3 (A) reissue-live | зелёный, 3/3 запусков, `COMPLETE reissue sizes=1..7 rounds=2` | зелёный, rc=0, 14/14 ROUND + `COMPLETE` | зелёный, rc=0, 14/14 ROUND + `COMPLETE` |
| W+3 (B) reissue-after | зелёный, 3/3, `COMPLETE ...` | зелёный, rc=0, 14/14 + `COMPLETE` | зелёный, rc=0, 14/14 + `COMPLETE` |
| W+3 (C) reissue-live-across | зелёный, 3/3 запусков, `COMPLETE reissue sizes=1..7 rounds=2` | зелёный, rc=0, 14/14 ROUND + `COMPLETE` | зелёный, rc=0, 14/14 ROUND + `COMPLETE` |
| neg: reissue-live-root | не применимо (`cfg(miri)`) | красный (ожидаемо), rc=1: `Undefined Behavior: not granting access to tag <11854> because that would remove [Unique for <17429>] which is weakly protected` (в `Box::from_raw(root)`; `<17429> is this argument` = `drop(boxed)`) | красный (ожидаемо), rc=1: `write access through <17428> at alloc4785[0x0] is forbidden ... foreign to the protected tag <16914> ... protected tags must never be Disabled` (строка записи `*fresh = ...`) |
| VA va-overlap, дефолтные флаги | зелёный, 3/3 запусков | зелёный, rc=0, `VA_SUMMARY rounds=64 live_reuse=11/64 after_reuse=6/64 aba_rejected=17 false_accepts=0` | зелёный, rc=0, те же числа 11/64, 6/64, 17 |
| VA va-overlap, `address-reuse-rate=1.0`, cross-thread 1.0 | не применимо | зелёный, `live_reuse=64/64 after_reuse=64/64 aba_rejected=128 false_accepts=0` | зелёный, те же 64/64, 64/64, 128 |

Native VA по запускам (live_reuse/64, after_reuse/64, rejected): запуск 1 — 7, 5, 12; запуск 2 — 5, 6, 11; запуск 3 — 5, 4, 9; (предварительный запуск до `rustfmt` — 5, 4, 9). Во всех `false_accepts=0`.

Итог по W+3: (A) и (B) ЗЕЛЁНЫЕ во всех трёх режимах (ожидание заранее не было известно). Механизм (A): новый `Box` — потомок cap-тега (cap = pointer, пришедший в `dealloc`, т.е. сам protected тег / его потомок), поэтому запись и единственный физический free через него не конфликтуют с protector'ом, пока producer в кадре; тот же контроль через root-тег (не потомок) красный на обеих моделях — этим же отличается "через cap" от "через root". Ровно одна физическая выдача в System (никакого double free).

Механизм (C) — вывод из отсутствия ошибок плюс текста красных контролей (самого Miri-диагноза на выходе кадра нет, он зелёный): на выходе кадра producer'а Miri снимает protector с аргумента `drop(boxed)` и проверяет, что защищённый тег ещё валиден. Fresh Box получен из cap, т.е. является потомком этого тега, поэтому его запись/чтение — child-доступы и не делают защищённый тег Disabled/не вытесняют его (SB: Unique-элемент родителя остаётся в стеке под потомком; TB: child write). Поэтому ни retag, ни release protector, ни последующий доступ через fresh ошибки не дают, а память не освобождалась, так что UAF тоже нет. Тот же re-issue через root (не потомок) красный на `Box::from_raw` (SB) / записи (TB) — см. контрфактуал 4.

## Контрфактуалы (временные, откат подтверждён sha256 = значения выше)

1. VA: `Incarnation::accepts` изменён на сравнение только `addr` (без generation). Native: rc=101, `panicked at tests\support\r11_box_cap_reissue.rs:253:9: ABA: old label accepted new object` (`cf1-native-va.log`). Miri SB rate1 (детерминированный reuse): rc=1, та же паника (`cf1-sb-va-rate1.log`). Красный именно из-за различимости по generation. Оговорка: на native красный получается только потому, что в запуске был VA-reuse (на дефолтных 3 запусках он был каждый раз; это не гарантировано).
2. W+3: в начале `reissue` добавлен физический `System.dealloc(p, layout)` перед `Box::from_raw` (re-issue поверх освобождённого). Miri SB и TB, (A) и (B): rc=1, `Undefined Behavior: constructing invalid value of type std::boxed::Box<[u8; 1]>: encountered a dangling box (use-after-free)` (4/4 клеток). Native (A) и (B): `STATUS_HEAP_CORRUPTION` (0xc0000374), exit не 0 (2/2). Значит зелёные (A)/(B) зависят именно от того, что физического free не было.
4. (C): в `adopt` вызов с `cap` заменён на `DESCRIPTOR.root.load(..)` (re-issue через root, пока producer приостановлен, fresh живёт через RESUME/join). Miri SB: rc=1, `Undefined Behavior: not granting access to tag <12254> because that would remove [Unique for <17999>] which is weakly protected` в `Box::from_raw` (`cf3-sb-across-root.log`). Miri TB: rc=1, `write access through <17974> at alloc4925[0x0] is forbidden` на записи `*fresh = [pattern; N]` (`cf3-tb-across-root.log`). Native: rc=0 (зелёный) — native не отслеживает provenance, мутация там не ловится и не должна. Откат: sha256 итоговых файлов совпал с таблицей выше.
3. Постоянный негативный контроль `reissue-live-root` (таблица) — red-by-design для различения cap vs root.

Не мутировались: assert'ы содержимого записи/чтения новым Box (тривиальны: паттерн `0x5A+round`, последний байт `^0x0F`), assert `live==0`/`cap.addr()==target.addr()` (унаследованы от W+2 и там уже имеют контрфактуал, см. receipt w1/w2).

## Качество

- `rustfmt --check --edition 2021` на трёх новых файлах: rc=0.
- `cargo clippy --test r11_box_cap_gate_w3 --test r11_box_cap_gate_va --features std -- -D warnings`: без предупреждений.
- `cargo clippy --all-targets -- -D warnings` (default): без предупреждений.

## Граница охвата и ограничения

- НЕ Sefer shipping: нет Small/Primordial цепи, magazine, registry, last-pin, OS release; descriptor статический.
- `u8`-массивы: W+3 — размеры 1..7 x 2 раунда (14 раундов на клетку); VA — размеры 1..8 x 8 раундов (64 раунда на запуск). Один producer + один owner; единственная thread-пара на раунд.
- Native: Windows 10, HeapAlloc; другие ОС/аллокаторы не проверялись. Под Miri `System` = аллокатор Miri; `va` под Miri детерминирован при фиксированном seed (SB и TB дали одинаковые 11/64, 6/64) — повторяемость по seed, а не свойство реального allocator.
- VA reuse: native нестабилен (по 4 запускам: live от 5/64 до 7/64, after от 4/64 до 6/64); Miri дефолт — 11/64 live и 6/64 after; Miri при rate=1.0 — 64/64 и 64/64 (принудительный режим, не естественный). Доля: aba_rejected / (число раундов с reuse) = 17/17 (SB дефолт), 128/128 (rate1), 9/9..12/12 (native); различимость проверена только в этих раундах. Раунды без reuse (53/64 и т.п.) ничего об ABA не доказывают.
- Что НЕ доказывает: VA-тест проверяет логику out-of-band метки (addr+generation), а не корректность метки реального allocator'а/registry; `generation` в тесте — глобальный монотонный счётчик, чего у реальной системы может не быть. Он не доказывает, что reuse возможен на любом размере/Layout, кроме 1..8 align 1.
- (C) закрывает случай «кадр старого Box::drop завершается над ещё живым новым объектом»; не проверено: отложенное освобождение fresh на другом потоке, несколько fresh подряд. Результат (C) — только для дефолтных Miri-флагов SB/TB без strict-provenance и для ровно этого pattern (child от cap).
- W+3: re-issue проверен только для Box-кадра, паузящегося после terminal publication, и только при передаче именно incoming-dealloc pointer (cap); re-issue через иную производную/root красный (контроль). Отказ/отсутствие owner-service, несколько re-issue подряд, re-issue другого Layout не тестировались.
- Модели Miri (SB/TB) экспериментальные, не нормативная спецификация Rust.
