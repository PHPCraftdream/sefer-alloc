# R11 Box cap gates W+1 / W+2: приёмочная записка (Фаза 0 / 1a)

Дата: 2026-10-01. Проверка выполнена заново, логи прошлого исполнителя не использовались.

## Что проверялось

Изолированные установленные `System`-свидетели. Это НЕ Sefer acceptance и не shipping-аллокатор.

- W+1: producer сам делает `System.dealloc` точного объекта до terminal RMW (`Gate<false>`).
- W+2: producer кладёт входящий в `dealloc` pointer (с provenance) в `AtomicPtr`, делает terminal RMW, паузится внутри настоящего `Box::drop`; owner освобождает `System.dealloc(cap, layout)` по переданному pointer, затем делает новый `System.alloc_zeroed` (`Gate<true>`).
- Негативные контроли (только `cfg(miri)`, только W+2): `root-one` (1 байт) и `root-eight` (8 байт, `write_unaligned`) пишут через исходный root-pointer при паузе; обязаны упасть из-за активного protector `Box`.

## Окружение и идентичность

- База: `a9734eb51685d0022c15d202d0948de84d3406de` + незакоммиченные файлы ниже.
- Native: `rustc 1.97.0 (2d8144b78 2026-07-07)`, Windows x86_64 (MSVC), `System` = HeapAlloc.
- Miri: `miri 0.1.0 (3659db0d3e 2026-07-05)`, `rustc 1.99.0-nightly (3659db0d3 2026-07-05)`.
- `CARGO_TARGET_DIR=D:/dev/rust/sefer-alloc/target/wt`, перед запусками `touch tests/*.rs`.
- sha256:
  - `tests/r11_box_cap_gate_w1.rs` f9959efe44d88937c147cab836813fadf850fc16870235d83344b45f24e79313
  - `tests/r11_box_cap_gate_w2.rs` 87f6900e808190be4a5c674136c04bbb7db76b86a7c2117c5648d20caed99c47
  - `tests/support/r11_box_cap_gate.rs` d830824e6d37603cd8c752f60dc61250effb30978cb6a274624ca914e9fe1528
  - `Cargo.toml` afeed5db3a9aca0b81d995818d07c18fa018f6b0ddbcd87bec48e201ec95917a

## Команды

```text
# native (harness=false, test=false: из обычного `cargo test` исключены, выбираются по --test)
cargo test --test r11_box_cap_gate_w1 --features std -- positive
cargo test --test r11_box_cap_gate_w2 --features std -- positive
# Miri Stacked Borrows (по умолчанию)
cargo +nightly miri test --test r11_box_cap_gate_w1 --features std -- positive
cargo +nightly miri test --test r11_box_cap_gate_w2 --features std -- positive|root-one|root-eight
# Miri Tree Borrows
MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri test ...  (те же selector'ы)
```

## Таблица исходов (все 12 клеток наблюдались; 12 из 12 соответствуют ожиданию)

| Режим | W1 positive | W2 positive | W2 root-one | W2 root-eight |
| --- | --- | --- | --- | --- |
| native | exit 0; `COMPLETE owner_free=false sizes=1..7 rounds=2` | exit 0; `COMPLETE owner_free=true sizes=1..7 rounds=2` | не применимо (`cfg(miri)`) | не применимо (`cfg(miri)`) |
| Miri SB | exit 0; 14/14 ROUND + COMPLETE | exit 0; 14/14 ROUND + COMPLETE | exit 1, UB: `not granting access to tag <9352> because that would remove [Unique for <14995>] which is weakly protected` (`<14995> is this argument` = `drop(boxed)`, строка 125) | exit 1, UB: `not granting access to tag <10067> because that would remove [Unique for <15676>] which is weakly protected` (аргумент `drop(boxed)`) |
| Miri TB | exit 0; 14/14 ROUND + COMPLETE | exit 0; 14/14 ROUND + COMPLETE | exit 1, UB: `write access through <9079> (root of the allocation) at alloc3850[0x0] is forbidden ... foreign to the protected tag <14550> ... protected tags must never be Disabled` | exit 1, UB: `write access through <9777> (root of the allocation) ... forbidden ... protected tag <15216> (currently Reserved) to become Disabled` |

14/14 = 7 размеров (1..7) x 2 раунда, строки `ROUND ... joined`. Негативы завершились кодом 1 (UB), а не 3 (`process::exit(3)` после записи, т.е. запись не была пропущена). Ошибка указана на строке записи через root (136 / 144), protector создан в аргументе `drop(boxed)`; посторонних сбоев нет.

## Контрфактуалы (временные, откат подтверждён sha256 = значения выше)

1. Pause после terminal RMW: в `dealloc` fetch_sub(live) перенесён ПОСЛЕ `RESUME.wait()`. Native W+2 positive: `panicked at tests\support\r11_box_cap_gate.rs:128:5 ... left: 1 right: 0`, rc=101. Т.е. штатный harness действительно наблюдает `live==0` при остановленном producer, пока тот внутри `Box::drop`/`dealloc`.
2. Owner-free без передачи cap: `System.dealloc(cap, ..)` заменён на `System.dealloc(root, ..)` (root = pointer из `alloc`). Miri W+2 positive:
   - SB: exit 1, `deallocating while item [Unique for <14970>] is weakly protected`;
   - TB: exit 1, `deallocation through <9058> (root of the allocation) at alloc3851[0x0] is forbidden ... protected tag <14529> ... would become Disabled`.
   Значит зелёный W+2 positive зависит именно от deallocation через переданный capability, а не от самого факта owner-free.
3. Запись через root при паузе — постоянные контроли root-one / root-eight (таблица).

## Качество

- `rustfmt --check --edition 2021` на трёх файлах: rc=0.
- `cargo clippy --test r11_box_cap_gate_w1 --test r11_box_cap_gate_w2 --features std -- -D warnings`: rc=0.
- `cargo clippy --all-targets -- -D warnings` без фич: rc=0 (цели требуют только `std`, не `alloc-core`).
- Harness не менялся: исправлений по итогам перепроверки не потребовалось.

## Граница охвата

- НЕ Sefer shipping: нет Small/Primordial цепи, magazine, registry, last-pin/owner claim, OS release; descriptor статический и живёт весь процесс.
- W+3 не проверялся. Старый `tests/regression_w3_stats_aliasing_miri.rs` не засчитывается.
- Только `u8`-массивы размером 1..7 (`Box<[u8; N]>`, align 1), 2 раунда; один producer + один owner; `System.alloc_zeroed` проверен только для новой выдачи точного размера.
- Совпадение численного VA (`va_reused`) наблюдалось в части раундов (W+2 Miri: `va_reused=true` в 3/14 раундов и в SB, и в TB), но ABA/новая инкарнация по VA не доказывается этим harness.
- Под Miri `System` = аллокатор Miri; native-результаты относятся к HeapAlloc на Windows, другие ОС не проверялись.
- Отказ/отсутствие owner-service не тестировался.
- Модели Miri (SB/TB) экспериментальные, не нормативная спецификация Rust.
