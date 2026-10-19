# B4-hot2: собственные горячие пути alloc/free — точный профиль

Задача: точечная доводка own-thread alloc/free после волны B4-hot. Инструменты: детерминированный callgrind через `.rush/wsl/prof_ao_win.sh` (маргинальный N/2N), larson-churn 768 живых, 16..512 B, вызовы за `#[inline(never)]`; судья — iai A/B `perf_gate_iai` на GitHub CI (`scripts/r20_r6_ab_iai_run.mjs`, `--features "production bench-internals internals"`).

## 0. Резюме

- Baseline: **w_alloc 63 Ir/op / w_free 119 Ir/op** (mimalloc: 51 / 23).
- Шаг 1 (const-folded fast path `class_for`) — оставлен.
- Шаг 2 (cold-вынос веток free) — **NO-GO по iai, откачен**: локальная проба «только аллокатор» показала w_free 119 → 112, но iai A/B (run 1, §1.2) — регрессия на пакетных free (+0.9…+1.3 %), потому что переполнение magazine в этих нагрузках не холодное.
- Шаг 3 — обоснованный отказ; шаг 4 (refill-буфер) — измерен и откачен как отрицательный результат.

## 1. Замеры

### 1.1 Локальная проба (callgrind, prof_ao)

| шаг | w_alloc | w_free | larson T=1 | mstress T=1 |
|---|---|---|---|---|
| base | 63 | 119 | — | — |
| s1 class_for fast path | 63 | 115 | — | — |
| s2 cold-вынос free | 63 | 112 | 322* | 492* |
| s12 (s1+s2) | 63 | 112 | 321 | 489 |

(* — замер с неоткатным тогда шагом 4)

### 1.2 iai A/B run 1 (s1+s2) — регрессия, шаг 2 снят

A = `e522300339728a78328d688413dd2bf90cd1c640` (main), B = `dfa6701b8ef827c9d4b8631fcc8b44d22cb09b34` (s1+s2), GitHub Actions run 39199097756, rustc 1.99.1, Linux 6.17.0-1022-azure. Сырые данные: [`_raw_b4_hot2_iai_run1.log`](_raw_b4_hot2_iai_run1.log), [`B4_HOT2_OWN_THREAD_run1_summary.csv`](B4_HOT2_OWN_THREAD_run1_summary.csv). Таблица — `node scripts/r6_gate_table.mjs docs/perf/B4_HOT2_OWN_THREAD_run1_summary.csv <cells>`:

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `cold_alloc_free_256x16b` | 106718 | 107678 | +960 | +960 / 106718 = 0.90 % |
| `cold_alloc_free_256x16b_4n` | 307172 | 311204 | +4032 | +4032 / 307172 = 1.31 % |
| `recycle_alloc_free_256x16b` | 171412 | 173328 | +1916 | +1916 / 171412 = 1.12 % |
| `dealloc_free_only_16b_n16` | 51957 | 51959 | +2 | +2 / 51957 = 0.00 % |
| `dealloc_free_only_16b_n17` | 52615 | 52648 | +33 | +33 / 52615 = 0.06 % |
| `dealloc_free_only_16b_n32` | 54109 | 54173 | +64 | +64 / 54109 = 0.12 % |
| `dealloc_free_only_1088_16b_n1024` | 304428 | 308336 | +3908 | +3908 / 304428 = 1.28 % |
| `large_cache_hit_only_4mib` | 57736 | 57871 | +135 | +135 / 57736 = 0.23 % |
| `large_cache_eviction_hit_only_41x4mib` | 233172 | 234484 | +1312 | +1312 / 233172 = 0.56 % |
| `growth16_n` | 14998235 | 14955183 | -43052 | -43052 / 14998235 = -0.29 % |
| `growth16_4n` | 116483828 | 116311752 | -172076 | -172076 / 116483828 = -0.15 % |
| `r19_prf01_joined_producer_same_word` | 1445925 | 1437737 | -8188 | -8188 / 1445925 = -0.57 % |
| `r19_prf01_publication` | 143065 | 144461 | +1396 | +1396 / 143065 = 0.98 % |

cells: 106; delta > 0: 43; delta < 0: 13; delta = 0: 50.

Механизм: одиночный free в пределах magazine не изменился (`n1…n16` ±3 Ir), каждое переполнение (`n17`, `n32`: +33 / +64 Ir — по ~+31 Ir на событие) подорожало: вынесенный `#[inline(never)]` `tcache_park_overflow_slow` теряет константы и регистры вызывающего кода. В пакетных нагрузках (cold/recycle `256×`, `n1024`) переполнение происходит каждые `FLUSH_N` free, то есть ветка горячая. Вынос Large-хвоста в `route_to_large_free` дал `large_cache_*` +0.2…+0.56 %. Шаг 2 откачен целиком (`dealloc_own_base.rs` = main), включая `cold_reject_noop_local` (действует только под `internals`/`bench-internals`, на production не влияет). Проба prof_ao не ловила этот эффект: её larson-churn почти не переполняет magazine.

### 1.3 iai A/B run 2 (только s1)

Ожидается после отката шага 2; результат — §1.3 обновляется по CI.

## 2. Реализовано

### Шаг 1: class_for fast path (const-fold)

- `src/alloc_core/platform/size_classes.rs` — `class_for_small_align`: const-свёрнутый fast path (MIN_BLOCK_SHIFT/S2C_LEN/SIZE2CLASS — константы; `seed_idx = max(size,1)-1 >> shift`; guard `seed_idx >= S2C_LEN-1 → None`), развилка `align <= MIN_BLOCK` внутри `SizeClasses::class_for` — все hot-сайты покрыты автоматически; `debug_assert` power-of-two сохранён. `crates/size-classes` не тронут.
- Эквивалентность: для `align ≤ 16` slow-путь недостижим (все block_size кратны 16); `need = max(size, align)` при `need ≤ 16` даёт `seed_idx = 0`, при `size ≥ 17` — `need = size`; `size = 0` обработан явно (наивный wrapping дал бы `None` вместо `Some(0)`).
- Тест `tests/class_for_small_align_equivalence.rs`: исчерпывающий `size 0..=262144` × align-степени 2^0..2^16 + границы классов + huge; эталон — независимая generic-реимплементация над публичными таблицами.
- Counterfactual: наивный `(size-1)>>shift` без `size==0` — тест краснеет (size=0, live=`None` vs model=`Some(0)`); восстановлено.

### Шаг 2: cold-вынос на own-thread free — NO-GO, откачен

См. §1.2. Код `src/registry/heap_core/free/dealloc_own_base.rs` совпадает с main; счётчик tier-2 в README остаётся 98.

### Шаг 3 (overflow-flush, план партии): обоснованный отказ

`flush_class_internal` (`src/alloc_core/small/alloc_core_small_magazine.rs:538-577`) после R3 уже строит runs за один проход и hoist'ит run-инварианты; батчинг `AllocBitmap::mark_free` невозможен без пессимизации из-за per-block M2-гвардов, способных отклонить блок внутри run; остаточная избыточность (`segment_base_of_ptr` на слот в cold-ветке) ≤ 0.3 Ir/op и не окупает план (пере-порядок слотов менял бы LIFO freelist). Код не менялся.

### Шаг 4 (refill: сохранить результат probe-прохода в буфер): реализован и откачен

`try_drain_freelist_batch` с буфером `[(*mut u8, u32);16]` дал **w_alloc 64** (+1), **larson 322** (хуже 321), **mstress 492** (хуже 489). A/B-замер без шага: larson 321 / mstress 489. ВЫВОД: повторное чтение цепочки дешевле буфера (LLVM уже держит данные в регистрах); шаг НЕ лендим, откат подтверждён (git diff по файлу пуст). Записан как отрицательный результат.

## 3. Тесты

- Шаг 1: `tests/class_for_small_align_equivalence.rs` (исчерпывающая эквивалентность, counterfactual — §2).
- Остальная сетка — на GitHub CI (PR #78).

## 4. Гарантии

Не ослаблены: M2-оракулы и их порядок, payload_start/bump/is_free guard'ы, divides (не-hardened) и hardened small_class_matches, R1-01 budget, RAD-5 mark/clear, C6 (prepare до мутаций), exactly-once credit, hardened cycle detection. unsafe не добавлялся; free-путь совпадает с main.

## 5. Урок

Локальная проба «только аллокатор» на larson-churn не представительна для веток, частота которых зависит от формы нагрузки (переполнение magazine). Вынос в `#[cold] #[inline(never)]` оценивать только iai A/B по всему `perf_gate_iai`, а не по одной пробе.

## 6. Риски

- `class_for`: поведение для contract-violating align=0 (не степень двойки, release) формально отличается от прежнего (было через debug_assert внутри crate — тоже UB-подобный контракт; в debug ассерт теперь в форвардере). Все in-contract входы эквивалентны (исчерпывающий тест).
- README-счётчик тест-файлов 433→434 (новый тест), проверяется структурными тестами.

## 7. Коммит

Префикс: `perf(runtime)` — только при GO по iai run 2 (§1.3).
