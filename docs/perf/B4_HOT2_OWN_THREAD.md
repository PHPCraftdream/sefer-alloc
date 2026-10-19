# B4-hot2: собственные горячие пути alloc/free — точный профиль

Задача: точечная доводка own-thread alloc/free после волны B4-hot. Инструмент: детерминированный callgrind через `.rush/wsl/prof_ao_win.sh` (маргинальный N/2N), larson-churn 768 живых, 16..512 B, вызовы за `#[inline(never)]`.

## 0. Резюме

- Baseline: **w_alloc 63 Ir/op / w_free 119 Ir/op** (mimalloc: 51 / 23).
- Реализованы шаг 1 (const-folded fast path `class_for`) и шаг 2 (cold-вынос free); шаг 3 — обоснованный отказ; шаг 4 (refill-буфер) — измерен и откачен как отрицательный результат.
- Итог: **w_alloc 63 (без изменений), w_free 119 → 112** (−7 Ir/op, −5.9%); larson T=1 **321**, mstress T=1 **489**.

## 1. Замеры

| шаг | w_alloc | w_free | larson T=1 | mstress T=1 |
|---|---|---|---|---|
| base | 63 | 119 | — | — |
| s1 class_for fast path | 63 | 115 | — | — |
| s2 cold-вынос free | 63 | 112 | 322* | 492* |
| s12 (s1+s2, финал) | 63 | 112 | 321 | 489 |

(* — замер с неоткатным тогда шагом 4)

## 2. Реализовано

### Шаг 1: class_for fast path (const-fold)

- `src/alloc_core/platform/size_classes.rs` — `class_for_small_align`: const-свёрнутый fast path (MIN_BLOCK_SHIFT/S2C_LEN/SIZE2CLASS — константы; `seed_idx = max(size,1)-1 >> shift`; guard `seed_idx >= S2C_LEN-1 → None`), развилка `align <= MIN_BLOCK` внутри `SizeClasses::class_for` — все hot-сайты покрыты автоматически; `debug_assert` power-of-two сохранён. `crates/size-classes` не тронут.
- Эквивалентность: для `align ≤ 16` slow-путь недостижим (все block_size кратны 16); `need = max(size, align)` при `need ≤ 16` даёт `seed_idx = 0`, при `size ≥ 17` — `need = size`; `size = 0` обработан явно (наивный wrapping дал бы `None` вместо `Some(0)`).
- Тест `tests/class_for_small_align_equivalence.rs`: исчерпывающий `size 0..=262144` × align-степени 2^0..2^16 + границы классов + huge; эталон — независимая generic-реимплементация над публичными таблицами.
- Counterfactual: наивный `(size-1)>>shift` без `size==0` — тест краснеет (size=0, live=`None` vs model=`Some(0)`); восстановлено.

### Шаг 2: cold-вынос на own-thread free

- `src/registry/heap_core/free/dealloc_own_base.rs`: `#[cold] #[inline(never)]` — `cold_reject_noop_local` (LOCAL_REJECT_FREES, cfg как раньше), `HeapCore::tcache_park_overflow_slow` (обе пост-кап ветки: one-element flush при `cap<TCACHE_CAP`; overflow half-flush+compact+push — перенесены дословно), non-small/Large-хвост через уже существующую cold `route_to_large_free` (тело байт-идентично).
- Порядок M2-оракулов (in-magazine → bump → flushed-bitmap), hot push, divides/hardened — не тронуты.
- Поведение не менялось; сетка из ~25 тест-файлов зелёная release+debug.
- Побочный эффект: item-scoped `#[allow(unsafe_code)]` у бывшего inline dealloc_at_base-хвоста исчез (тело теперь только внутри `route_to_large_free` со своим allow) — README-счётчик tier-2 98→97 (проверяется тестом).

### Шаг 3 (overflow-flush, план партии): обоснованный отказ

`flush_class_internal` (`src/alloc_core/small/alloc_core_small_magazine.rs:538-577`) после R3 уже строит runs за один проход и hoist'ит run-инварианты; батчинг `AllocBitmap::mark_free` невозможен без пессимизации из-за per-block M2-гвардов, способных отклонить блок внутри run; остаточная избыточность (`segment_base_of_ptr` на слот в cold-ветке) ≤ 0.3 Ir/op и не окупает план (пере-порядок слотов менял бы LIFO freelist). Код не менялся.

### Шаг 4 (refill: сохранить результат probe-прохода в буфер): реализован и откачен

`try_drain_freelist_batch` с буфером `[(*mut u8, u32);16]` дал **w_alloc 64** (+1), **larson 322** (хуже 321), **mstress 492** (хуже 489). A/B-замер без шага: larson 321 / mstress 489. ВЫВОД: повторное чтение цепочки дешевле буфера (LLVM уже держит данные в регистрах); шаг НЕ лендим, откат подтверждён (git diff по файлу пуст). Записан как отрицательный результат.

## 3. Тесты и прогоны (все exit 0, CARGO_TARGET_DIR=.rush/cargo-target-C)

- release+debug «production internals bench-internals», 21 файл: class_for_small_align_equivalence, alloc_core_batch, double_free_guard, heap_core_tcache, heap_core_tcache_m2, regression_magazine_oracles, regression_magazine_bump_guard, regression_magazine_free_bounds, regression_hardened_interior_ptr, regression_r1_01_magazine_free_budget, r11_ph3a_magazine_refill_partial, regression_tcache_byte_budget, batch_tcache, r6_t55_local_reject_frees, r6_t33_t05_own_free_kind_and_class, r19_large_interior_free, r17_4_inplace_grown_large_dealloc_routes_by_kind, heapcore_own_free_single_canonical_lookup, regression_flush_class_unsafe_boundary, regression_batch_freelist_drain, r14_4_promotion_free_correctness.
- release+debug «production hardened internals bench-internals»: regression_hardened_large_kind_own_free, r11_4_dealloc_batch_hardened_guards, regression_dealloc_metadata_region_guard, regression_freelist_next_validation.
- release+debug «alloc-global alloc-xthread internals bench-internals»: r6_u21_foreign_free_interior_granule, dealloc_only_no_bind, dealloc_sublinear.
- структурные (default): no_stale_doc_references, unsafe_allow_placement, src_file_size_cap — release+debug.
- counterfactual: шага 1 — см. §2; шаг 2 — чисто структурный (поведение байт-в-байт), покрыт существующей сеткой.

Clippy (как в ci.yml, `-D warnings`): все 8 конфигураций exit 0.
Формат: `rustfmt --check` напрямую на трёх файлах — ок (`cargo fmt` в среде падает os error 206, не код).

## 4. Гарантии

Не ослаблены: M2-оракулы и их порядок, payload_start/bump/is_free guard'ы, divides (не-hardened) и hardened small_class_matches, R1-01 budget, RAD-5 mark/clear, C6 (prepare до мутаций), exactly-once credit, hardened cycle detection (шаг 4 откачен, изменений refill нет). unsafe не добавлялся; allow-счётчик уменьшился (мёртвый allow исчез вместе с дублированным телом).

## 5. CI (обязательный прогон)

Miri (path-b-матрица), Loom-модели, iai (`perf_gate_iai`), mt-bench larson/mstress T=1..8. Локальные callgrind-цифры детерминированы (инструмент `.rush/wsl/prof_ao_win.sh` / `prof_win.sh`).

## 6. Эффект и приёмка

w_free **119→112** (−7 Ir/op, −5.9%), w_alloc **63** без изменений; larson T=1 **321**, mstress T=1 **489** (без шага 4). Метрика приёмки: **w_free ≤ 112 в prof_ao на этой ветке**; регресс по w_alloc/w_free относительно этих цифр — блокер. Разрыв с mimalloc (51/23) не закрыт: остаток — гарантирующие пути (см. B4_HOT_PATH_OWN_THREAD.md §5, R4/R5).

## 7. Риски

- `class_for`: поведение для contract-violating align=0 (не степень двойки, release) формально отличается от прежнего (было через debug_assert внутри crate — тоже UB-подобный контракт; в debug ассерт теперь в форвардере). Все in-contract входы эквивалентны (исчерпывающий тест).
- `tcache_park_overflow_slow`: +call/ret на холодных ветках (редкие).
- README-счётчики обновлены (433→434 тест-файла; tier-2 98→97) и проверяются структурными тестами.

## 8. Коммит

Префикс: `perf(runtime)`. Сообщение: «perf(runtime): const-folded class_for fast path + cold extraction on own-thread free (B4-hot2, free 119→112 Ir/op)».
