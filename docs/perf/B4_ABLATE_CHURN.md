# B4-ablate: абляции churn (A2) — измерение, без изменений runtime

- Ветка: `b4-ablate` (измерительная, НЕ сливается в main), база `412fe33e`.
- Контекст (ИЗМЕРЕНО, `docs/perf/B4_INTERLEAVE_JUDGE.md`): churn16/64 sefer медленнее
  mimalloc в 1.89×/1.83× по времени, при этом по callgrind у sefer МЕНЬШЕ инструкций
  (84.65 против 134.21 Ir/op), промахи D1/LL ≈ 0. Время уходит на то, чего Ir не видит:
  цепочки зависимых загрузок, store-forwarding, ветвления, lock-RMW.
- Метод: однопоточный churn, A/B (arm A = `sefer_alloc`, arm B = вариант из патча);
  замеры выполняет оркестратор. Здесь — только патчи и гипотезы.
- Генерация варианта: `node scripts/b4_ablate_make_variant.mjs <variant-name>`;
  все 12 патчей + `null` применяются генератором успешно (коды возврата 0, см.
  ниже; ревью-фикс P2: патчи ab01–ab09 переписаны на РЕАЛЬНЫЕ удаления кода —
  прежние версии содержали только вставку комментария-маркера и ловились
  assert'ом генератора «абляция не удаляет код»).

## Механизм генерации (ревью-фиксы P1+P2; ИЗМЕРЕНО)

- Расположение копии (ревью-фикс P1): ВНЕ дерева репозитория —
  `<repo-root>/../../.rush/variant/<variant>/`. Внутри: `src/…` (байт-в-байт
  копия `src/` + патч) + сгенерированный `Cargo.toml` + identity/header-файлы.
  Причина: копия внутри дерева (`crates/sefer-variant/`) ломала
  `verify-structure-limits` и dbg-hooks scanner; ослаблять verify-скрипты
  запрещено. `crates/sefer-variant` в дереве НЕ существует; пакет НЕ member
  workspace — корневой `Cargo.toml` подключает его относительным path
  dev-dependency `../../.rush/variant/null` (пакет — корень СВОЕГО workspace,
  без `[lints] workspace`). `git apply` выполняется с cwd=корень worktree во
  временный ин-дерева staging-каталог `.b4_ablate_tmp` (git не пишет вне
  worktree), результат переносится наружу, staging удаляется до выхода.
- Identity (ревью-фикс P1): генератор при КАЖДОЙ генерации (любой вариант,
  включая `null`) зеркалирует identity-файл в фиксированное место
  `<repo-root>/../../.rush/variant/current/src/ab_variant_identity.rs`, и ядро
  судьи включает его через вариант-НЕзависимый относительный путь:
  `include!(concat!(env!("CARGO_MANIFEST_DIR"),
  "/../../.rush/variant/current/src/ab_variant_identity.rs"))`
  (examples/_shared/b4_interleave_core.rs, под `feature = "ablate"`).
  Абсолютных машинных путей нет нигде — только относительные.
- Зеркало в dev-dep путь (обязательно): после записи
  `.rush/variant/<имя>/` генератор ЗЕРКАЛИРУЕТ результат (`src/` +
  `Cargo.toml`, rm+copy) в фиксированный `.rush/variant/null/` — root
  манифест подключает вариант dev-dependency `../../.rush/variant/null`,
  и cargo при сборке судьи ВСЕГДА компилирует код именно оттуда; без
  зеркала каждая фаза 2 отмерила бы null-код вместо абляции (для
  `variant == null` это тот же каталог — двойной работы нет).

- Патчи `docs/perf/b4_ablate/patches/<variant>.patch` — обычные `git diff` по
  `src/` (`a/src/…` / `b/src/…`), применяются как
  `git apply --directory=.b4_ablate_tmp -p1`. Текстовая шапка
  (комментарии «ТОЛЬКО ДЛЯ ЗАМЕРА», гипотеза, ожидаемое направление) остаётся
  В САМОМ `.patch` ПЕРЕД строкой `diff --git` — `git apply` пропускает ведущие
  строки до заголовка диффа (проверено `git apply --check` на всех 9 патчах,
  rc=0); отдельные `.md`-файлы не нужны.
- Генератор: копирует `src/` → `.rush/variant/<variant>/src/` БАЙТ-В-БАЙТ (без
  LF-нормализации на диске — `git apply` байтовый; в `src/` CRLF-файлов нет,
  `rg -l $'\r' src --iglob '*.rs'` — пусто), применяет патч через `git apply`
  (rc≠0 → падение скрипта), затем проверяет пост-условия: `changed_files > 0`
  и sha256(после) ≠ sha256(до) — иначе скрипт падает. Хэширование и хэш-идентичность
  (sha256 в `ab_variant_header.txt`) — по LF-нормализованному содержимому,
  как раньше. Для `null` (патч без `diff --git`) — no-op, rc-проверка не нужна.
- Content-assert'ы генератора (ревью-фикс P2): для не-`null` варианта
  генератор считает построчный дифф (added/removed по ВСЕМ изменённым файлам,
  LCS на общем префиксе/суффиксе) и `+`/`-` строки ТЕКСТА патча (без
  `+++`/`---` и шапки) и требует added==patchPlus && removed==patchMinus
  («патч не соответствует применённому изменению»), а также что удалена ≥ 1
  непустая не-`//` строка («абляция не удаляет код» — защита от
  абляций-комментариев). `null` (нет `diff --git`) освобождён. stdout
  печатает `added=<N> removed=<M> patch(+/-)=<P>/<Q>`.

  Содержание переписанных патчей ab01–ab09 (было → стало; ИЗМЕРЕНО, rc
  генератора 0 у всех):

  | вариант | было (удалено) | стало |
  |---|---|---|
  | ab01 | чтение `state_byte().read(off)` + `expected=encode(c,LIVE)` + вся ветка `if byte != expected {…}` (классификация reject'ов) | комментарий-маркер; выравнивание и bump-проверка сохранены |
  | ab02 | store `meta.state_byte().write(off, c, MAG)` на горячем push | комментарий-маркер (store убран) |
  | ab03 | `SegmentMeta::new(base).state_byte().set_state(off, LIVE)` в `clear_magazine_on_issue` | `_off` + комментарий-маркер |
  | ab04 | memo-плечо `if self.last_key == key.addr() {…}` (N9) | комментарий-маркер; lookup падает в own_cache/hash_find |
  | ab05 | ab04 + own_cache-плечо `let (cached_key, cached_root)…; if cached_key == key.addr() {…}` | `let index = …` + комментарий; каждый lookup → `hash_find` |
  | ab06 | ветка `if SegmentHeader::kind_at(base) == Large {…} else {…}` в `canonical_block_of` | всегда маска `ptr & (SEGMENT-1)` + комментарий |
  | ab07 | `if (off as usize) >= meta.bump_of() { reject }` | комментарий-маркер |
  | ab08 | guard `p.addr().wrapping_sub(1) < usize::MAX - 1` (Э2) в `current_for_dealloc` | guard `!p.is_null()` (null отклоняется, TORN — нет) |
  | ab09 | выбор `payload_start` (Primordial/small_meta_end) + reject `off < payload_start` | комментарий-маркер |
- Identity варианта теперь НЕ константа: генератор пишет
  `<out>/src/ab_variant_identity.rs` и зеркалирует его в
  `.rush/variant/current/src/ab_variant_identity.rs`
  (`pub const VARIANT_NAME` / `VARIANT_SHA256`), судья включает его через
  `include!(concat!(env!("CARGO_MANIFEST_DIR"),
  "/../../.rush/variant/current/src/ab_variant_identity.rs"))`
  (examples/_shared/b4_interleave_core.rs, под `feature = "ablate"`). В raw
  добавлены поля: header — `"variant_name"` и `"variant_sha256"`; каждая
  запись блока — `"variant_name"` (sha в записях избыточен, единый источник —
  header). Не-ablate режимы пишут `"-"`.

## Абляции

| id | патч | цель (файл:строка в `src/`) | что убирает | гипотеза | ожидаемое направление |
|----|------|------------------------------|-------------|----------|----------------------|
| ab01 | `patches/ab01_free_no_state_read.patch` | `src/registry/heap_core/free/dealloc_own_base.rs:348-367` (`small_free_guard`) | чтение байта состояния + сравнение `encode(c, LIVE)` + классификацию reject'ов (M2 oracle-2) | зависимая загрузка байта состояния — скрытая latency own free | время ВНИЗ, если оракул дорог; иначе ~0 |
| ab02 | `patches/ab02_free_no_state_write.patch` | `src/registry/heap_core/free/dealloc_own_base.rs:501-505` (push, `cnt < cap`) | store LIVE→MAG в байт состояния на горячем push | store после цепочки загрузок даёт store-forwarding/retirement паузу | время ВНИЗ при дорогом store; иначе ~0 |
| ab03 | `patches/ab03_alloc_hit_no_state_write.patch` | `src/registry/heap_core/alloc/hot.rs:45-47` (`clear_magazine_on_issue`) | store MAG→LIVE при выдаче из magazine (alloc-стор) | та же store-цена на alloc-стор; ab02 vs ab03 локализует, какой store дорог | время ВНИЗ или ~0; контраст с ab02 диагностичен |
| ab04 | `patches/ab04_own_no_memo.patch` | `src/alloc_core/segment/segment_table/segment_table_impl.rs:801-813` (`canonical_base_of_mut`) | N9 single-entry memo (одно сравнение `last_key`) | owner-lookup — цепочка из 3 зависимых загрузок; даже одно сравнение ощутимо | время ВВЕРХ (абляция добавляет работу); рост = вклад memo |
| ab05 | `patches/ab05_own_no_own_cache.patch` | `src/alloc_core/segment/segment_table/segment_table_impl.rs:801-826` | ОБА Tier-1 плеча (memo + own_cache) → каждый lookup идёт в `hash_find` | разница (ab05 − null) = полная цена Tier-1 во времени; (ab05 − ab04) = вклад own_cache | время заметно ВВЕРХ |
| ab06 | `patches/ab06_own_no_kind_at.patch` | `src/alloc_core/alloc_core/mem/mem_impl.rs:56-72` (`canonical_block_of`) | загрузку `SegmentHeader::kind_at` + ветку Large/non-Large (всегда non-Large маска) | kind_at — зависимая загрузка после owner-lookup; в Ir почти не видна (1 загрузка) | время ВНИЗ при дорогой загрузке; иначе ~0 |
| ab07 | `patches/ab07_free_no_bump_check.patch` | `src/registry/heap_core/free/dealloc_own_base.rs:368-370` | M2 oracle-1.5: загрузку `bump_of()` + сравнение `off >= bump` | вторая зависимая загрузка метаданных — чистая latency-надбавка | время ВНИЗ или ~0 |
| ab08 | `patches/ab08_dealloc_tls_no_range_check.patch` | `src/global/tls_heap.rs:385` (`current_for_dealloc`) | Э2-проверку диапазона TLS-указателя (~2 инструкции → `!p.is_null()`) | TLS-диспетчеризация — измеримая константа на каждый free | время слегка ВНИЗ (~1–2 цикла/free) или ~0 |
| ab09 | `patches/ab09_free_no_payload_start.patch` | `src/registry/heap_core/free/dealloc_own_base.rs:315-322` | выбор payload_start + reject `off < payload_start` (остаётся compare убран целиком) | остаточные guard-сравнения входа free стоят измеримых тактов | слегка ВНИЗ или ~0 |
| ab10 | `patches/ab10_free_push_no_bookkeeping.patch` | `src/registry/heap_core/free/dealloc_own_base.rs:508,522-525` (push, `cnt < cap`) | bookkeeping push-а группой: инкремент `count = cnt+1` + очистку бита virgin_mask; запись слота и store MAG в state-byte ОСТАВЛЕНЫ | пара bookkeeping-записей (dependent store после load cnt) стоит измеримых тактов на hot push | время ВНИЗ; иначе ~0 |
| ab11 | `patches/ab11_alloc_pop_no_bookkeeping.patch` | `src/registry/heap_core/alloc/hot.rs:247,301-304` (hit-плечо pop) | bookkeeping pop-а группой: декремент `count = new_cnt` + очистку бита virgin_mask на issue; чтение count/слота и clear_magazine_on_issue ОСТАВЛЕНЫ | dependent store декремента + mask-AND на каждом hit стоят измеримых тактов на churn-alloc | время ВНИЗ; иначе ~0 |
| ab12 | `patches/ab12_free_no_park_budget_g13.patch` | `src/registry/heap_core/free/dealloc_own_base.rs:495` (free-сторона; G13) | бюджет free-park: `FREE_PARK_CAP[c]` → `TCACHE_CAP` (константное сворачивание: `cnt < cap` всегда истинно, swap-top overflow-ветка :546-580 мертва); alloc-сторона и dealloc_batch не тронуты | бюджет free-park (G13) стоит измеримых тактов на churn free | ВНИЗ или ~0 |

Все патчи помечены в шапке «ТОЛЬКО ДЛЯ ЗАМЕРА: корректность может нарушаться — не
для production». ab01/ab02/ab03/ab07/ab09 ломают оракулы двойных free; ab06 ломает
Large-маршрутизацию (на churn-плече Large-frees нет); ab04/ab05 сохраняют
корректность ответа lookup, но намеренно замедляют; ab08 меняет поведение null/TORN.

ОЦЕНКА (проверки компиляции): `cargo check` вариантов НЕ выполнялся (допускается по
постановке; правки локальны, ab04/ab05 оставляют мёртвые поля `last_key`/`last_root`
записываемыми — только warnings, не errors; ab06 оставляет `SegmentKind` импорт без
использования — аналогично warning).

## lock-RMW на пути

ИЗМЕРЕНО (objdump, mingw64, `--no-show-raw-insn`) по release-бинарю
`b4_interleave_judge.exe` (общий `CARGO_TARGET_DIR=.rush/cargo-target-AB`): всего
878 lock-инструкций в образе (все плечи, включая std/mimalloc — полная атрибуция —
задача параллельного asm-разбора, здесь не дублируется). Состав по мнемоникам:

| мнемоника | штук |
|-----------|------|
| `lock incq/incl/add/addq` (fetch_add-класс) | 445 |
| `lock cmpxchg` (CAS-класс) | 276 |
| `lock or/orq/orl` (fetch_or-класс) | 82 |
| `lock xadd` | 59 |
| `lock and/andq` (fetch_and-класс) | 16 |
| `lock decq` | 16 |

Исходные точки lock-RMW в `src/` (rg по `compare_exchange|fetch_*`), релевантные
пути free/alloc:

- `src/registry/segment_route/shard_lock.rs:85` — `compare_exchange_weak(false,true)`
  (захват shard-lock каталога; не на own-хите, на foreign/каталожных путях);
- `src/registry/segment_route/shard_lock.rs:132` — `compare_exchange_weak(false,true)`
  (release-сторона того же лок-протокола);
- `src/registry/segment_route/directory.rs:224` — `refs.fetch_sub` (unpin, cold);
- `src/registry/heap_core_xthread/routing.rs:39,59,63` — счётчики
  `XNULL_NULL_TAIL_GUARD`/`B4_FOREIGN_PUBLISH_OK`/`FOREIGN_OR_UNROUTABLE_FREES`
  (под `bench-internals`/диагностику, cold-ветки foreign);
- `src/registry/heap_core_xthread/route_pin_cache.rs:71,119` — счётчики pin-cache (cold);
- `src/registry/heap_registry/stack.rs:100` и `src/registry/heap_slot.rs:293` — CAS
  claim/generation слота (не на own churn-паре);
- `src/registry/heap_registry/claim.rs:397` — `generation.fetch_add` (claim, cold).

ОЦЕНКА по исходникам: на own-thread churn-паре (TLS-dispatch → `owned_block_of` →
`small_free_guard` → push / magazine pop) постоянных lock-RMW НЕТ — счётчики под
`bench-internals` и cold-ветки; lock-инструкции в бинарном образе сосредоточены в
foreign/каталожных/claim путях и в плечах mimalloc/std. Проверяемое следствие для
замеров: если arm B с абляцией изменит число lock-RMW на churn-пути, это дефект
патча, а не ожидаемый эффект.

## ПРЕДРЕГИСТРАЦИЯ (фаза 2)

### Прогон

- **Допуск (A/A):** ядро допускается к вердикту только если ОБЕ калибровки
  прошли порог — A/A sefer (`aa_sefer`) И A/A варианта (`aa_variant`):
  на каждое ядро/режим `|median ln(t_B/t_A)| ≤ 1 %` И bootstrap 95 % CI
  полуширина `≤ 1 %` (≥ 2000 resamples, seed таблицы `0x5eed`). Ядро, не
  прошедшее A/A — «не допущена», вердикт по нему не выдаётся.
- **Допуск null («копия = оригинал»):** у null-варианта `ab_variant` ОБЯЗАН
  пройти ТОТ ЖЕ допуск (на каждом ядре `|median ln| ≤ 1 %` И CI
  полуширина `≤ 1 %`); проверяется runner-ом через генератор с флагом
  `--expect-admitted` (для `variant_name == "null"` генератор дополнительно
  проверяет и сам A/B тем же порогом). Нарушение — runner ОСТАНАВЛИВАЕТСЯ с
  кодом ≠ 0 ДО измерения реальных вариантов.
- **Вклад абляции (A/B):** `exp(median ln(t_B/t_A))`, где A = sefer base
  (плечо A судьи), B = вариант (плечо B); CI — bootstrap 95 % по блокам
  (метод J1′: медиана по блокам величины `ln((b1+b2)/(a1+a2))`, percentile
  bootstrap, ≥ 2000 resamples, плюс 10 % trimmed mean). Вклад значим, если
  95 % CI ОТНОШЕНИЯ не содержит 1 (эквивалент: CI логарифма не содержит 0);
  направление интерпретируется по колонке «ожидаемое направление»
  таблицы абляций.
- **Абсолютная метрика B−A (тики/вызов):** рядом с ratio-метрикой для
  каждого ядра считается per-блок разность тиков на вызов шима:
  `(b_ticks/calls_b) − (a_ticks/calls_a)` (a_ticks = a1+a2, b_ticks = b1+b2;
  счётчики вызовов — те же `*_calls`, что и для ratio); медиана по блокам +
  bootstrap 95 % CI (≥ 2000 resamples, тот же seed). Единица — ТИКИ НА
  ВЫЗОВ (не отношение, без логарифма). В markdown-таблице — колонка
  `B−A ticks/call` (медиана); в CSV — `absdiff_median`, `absdiff_ci_lo`,
  `absdiff_ci_hi`.
- Число блоков: финальные прогоны — 10000; dry-run — 24
  блока (формат/валидность пайплайна).

### Форма шимов (asm-таблица)

Пер-вариантная asm-верификация абляций ab01–ab12 (Δинструкций и исчезнувшие
инструкции по каждому варианту, все 12 подтверждены) —
`docs/perf/b4_ablate/asm_notes.md`, раздел «Пер-вариантная верификация
абляций (раунд 3)»; здесь не дублируется.

ИЗМЕРЕНО по дизассемблированию пост-LTO объектников; полный метод и выжимки —
`docs/perf/b4_ablate/asm_notes.md`. Таблица v2 (счётчик вызовов — thread-local
`Cell<u64>`, const-init, **lock = 0 в телах всех шимов**):

| сегмент | insn | ld | tls | st | lock | jcc | pro/epi | chain (ОЦЕНКА) |
|---|---:|---:|---:|---:|---:|---:|:---:|---:|
| sefer alloc, полное | 136 | 13 | 1 | 10 | 0 | 18 | 3/4 | 4 |
| sefer alloc, hit 0x00–0x35 | 12 | 5 | 1 | 1 | 0 | 1 | 3/4 | 3 |
| sefer dealloc, полное | 406 | 47 | 1 | 31 | 0 | 65 | 9/10 | 6 |
| sefer dealloc, fast 0x00–0x3C | 18 | 5 | 1 | 1 | 0 | 1 | 9/10 | 3 |
| mi shim `mi_a_alloc` | 5 | 4 | 1 | 1 | 0 | 0 | 0/0* | 2 |
| mi shim `mi_a_dealloc` | 5 | 4 | 1 | 1 | 0 | 0 | 0/0* | 2 |
| `mi_free` (mimalloc C) | 52 | 6 | 1 | 2 | 1 (`lock cmpxchg`) | 6 | 1/2 | 4 |
| `mi_malloc_aligned` | 47 | 8 | 1 | 5 | 0 | 9 | 1/2 | 6 |

\* mi-шимы — tail-call `jmp`: пролога/эпилога нет, только 5-инструкционный
счётчик (`_tls_index` → `gs:` TLS-база → TLS-слот, plain `inc`, tail `jmp`).
Единственный `lock` в таблице — CAS самого mimalloc в `mi_free`, не счётчик.
Правила подсчёта (v2: `lea`/`nop` не загрузки, `gs:` отдельно), границы
hit-участков и артефакты (полный дизасм, выжимки тел 18 шимов) —
`docs/perf/b4_ablate/asm_notes.md`.

### Команды фазы 2 для оркестратора

Вся фаза 2 выполняется ОДНИМ runner-скриптом:

```
node scripts/b4_ablate_run.mjs --out-dir <OUT>
```

(по умолчанию kernels `churn16,churn64,pair16,larson`, blocks `10000`;
переопределение — `--kernels <csv>` / `--blocks <N>`; обязательно
`CARGO_TARGET_DIR=<TARGET-DIR>` в окружении; `--out-dir` —
каталог сырых jsonl ВНЕ репозитория; dry-run — `--blocks 24` или флаг
`--dry-run`; таймауты — `--build-timeout 900`, `--run-timeout 900`,
`--gen-timeout 120`; свод по всем вариантам дописывается генератором в
`docs/perf/B4_ABLATE_CHURN_summary.csv` — путь переопределяется `--summary`.
`--aa-sefer-only-null` (по умолчанию ВЫКЛ) — экономия процессов судьи для
dry-run: `aa_sefer` гоняется только у null, per-variant генерация таблиц
пропускается — три raw тогда с РАЗНЫХ сборок, identity генератора этого
не допускает; таблицы генерируются ТОЛЬКО для вариантов, у которых есть
все три raw одной сборки (в этом режиме — только null). DRY-RUN ONLY.
`--no-null-gate` (по умолчанию ВЫКЛ, DRY-RUN ONLY): таблица null
генерируется БЕЗ `--expect-admitted` и гейт не останавливает runner — на
24 блоках широкий CI почти наверняка не пройдёт допуск. ФАЗА 2 ОБЯЗАНА
ДЕРЖАТЬ ГЕЙТ (флаг не передавать).)

Runner делает сам, строго по порядку:

1. **Калибровка null:** `node scripts/b4_ablate_make_variant.mjs null` →
   build судьи (`cargo build --profile release --example
   b4_interleave_judge --features production,ablate`) → judge
   `--mode aa_sefer` → judge `--mode aa_variant` → judge
   `--mode ab_variant` (три jsonl `<OUT>/null_*.jsonl`, ОДНА сборка) →
   генератор таблиц для null с `--expect-admitted` — это ГЕЙТ: не прошёл
   допуск → runner останавливается с кодом ≠ 0.
2. **Для каждого варианта по порядку `ab01 … ab12`** (список берётся из
   `docs/perf/b4_ablate/patches/` динамически, `null` исключён; НЕ возвращать
   null между вариантами; после КАЖДОЙ генерации пересборка ОБЯЗАТЕЛЬНА —
   копия участвует в сборке): генерация → build → judge `--mode aa_sefer` →
   judge `--mode aa_variant` → judge `--mode ab_variant` (три raw ОДНОЙ
   сборки — требование генератора) → сразу генератор таблиц для ЭТОГО
   варианта (`--csv <OUT>/b4_ablate/summary_<v>.csv`,
   `--summary-append docs/perf/B4_ABLATE_CHURN_summary.csv`,
   `--kernels`, `--expect-admitted`).
3. **В конце:** возврат null (`b4_ablate_make_variant.mjs null`).

Identity-флаги судьи (все обязательны — raw без них невалиден) runner
вычисляет один раз: `head_sha` = `git rev-parse HEAD`, `source-diff-sha` =
sha256 от точных байт `git diff`, `--rustc`, `--os-version`. ПРИМЕЧАНИЕ:
`source-diff-sha` СТАБИЛЕН на всю фазу — генерация варианта меняет копию
ВНЕ дерева (`.rush/variant/`), поэтому `git diff` её не включает и sha не
меняется.

Справочно, полная команда судьи:

```
<TARGET-DIR>/release/examples/b4_interleave_judge \
  --mode ab_variant --blocks <N> --seed 0xb4110001 --order-seed 0x0b4d5eed \
  --kernels churn16,churn64,pair16,larson \
  --out <OUT>/<variant>_ab.jsonl \
  --head-sha $(git rev-parse HEAD) \
  --source-diff-sha $(git diff | sha256sum | cut -d" " -f1) \
  --features production,ablate --profile release \
  --rustc "$(rustc --version)" --os-version unknown
```

(`--source-diff-sha` — 64 hex sha256 от `git diff` на момент прогона.)

## Результаты

Фаза 2 выполнена 2026-10-25 на i7-11800H, Windows 10.0.19045, rustc 1.97.0,
`--features production,ablate`, release, RUSTFLAGS
`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`.
Ядра: churn16, churn64, pair16. larson исключён после первого прогона: его
null-калибровка не прошла (см. «Отклонения от предрегистрации»).

### Отклонения от предрегистрации

1. **Единица репликации — процесс, а не блок (J1′ v2).** Предрегистрация
   задавала один процесс на режим по 10000 блоков и bootstrap по блокам.
   Два прогона null-варианта по этой схеме на ОДНОМ и том же коде дали
   на churn16 A/B = +0.46 % и −1.83 % при полуширине CI внутри процесса
   ≈ 0.1 % (ИЗМЕРЕНО). Отдельные процессы по 4000 блоков показывали +11.7 %
   (churn16) и +17.1 % (churn64) для копии против оригинала. Разброс между
   процессами на порядок больше CI внутри процесса: состояние, от которого
   зависит время (вероятнее всего, размещение адресов), фиксировано на весь
   процесс. Поэтому фаза 2 выполнена так: 20 процессов на режим × 2000
   блоков, различные order seeds (`0x0b4d5eed + i`), медиана
   `ln(t_B/t_A)` по блокам внутри процесса, затем медиана по процессам и
   bootstrap 95 % по ПРОЦЕССАМ (2000 resamples). Порог допуска прежний
   (|median| ≤ 1 %, полуширина ≤ 1 %). Генератор:
   `scripts/b4_ablate_rep_table.mjs`; runner: `scripts/b4_ablate_run.mjs
   --reps 20`.
2. **larson исключён.** Null-калибровка по старой схеме: A/A sefer на larson
   +1.11 % (CI 0.906…1.309 %) — вне порога.
3. **ab11 аварийно остановил судью** (`segment_header_meta_fields: sub_live
   live_count underflow`, статус 0xC0000409, `docs/perf/b4_ablate/v2/ab11_crash_excerpt.txt`):
   без декремента `count` магазин выдаёт блоки повторно, учёт live
   ломается. Runner остановился; ab12 в этом прогоне не измерен.
   Повторный прогон null + ab12 остановлен null-гейтом: A/A sefer на churn16
   −1.02 % (CI −1.40…−0.88 %) — на 0.02 п. п. за порогом
   (`docs/perf/b4_ablate/v2/rerun_null/`). ab12 не измерен.

### Таблица (вывод генератора, дословно)

A = sefer, B = вариант; отрицательная медиана — вариант быстрее, т. е.
удалённый шаг стоил времени. «A ticks/call» — абсолютная цена вызова
базового sefer в тиках TSC (медиана по процессам). Полный вывод:
`docs/perf/b4_ablate/v2/generator_stdout.txt`.

| Вариант | Ядро | A/B медиана % [CI] | A ticks/call | B−A ticks/call [CI] | Допуск | Вердикт |
|---|---|---|---:|---|---|---|
| null | churn16 | -0.90 [-1.30, -0.20] | 15.45 | -0.14 [-0.21, -0.03] | yes | VARIANT_FASTER |
| null | churn64 | -0.21 [-0.35, 0.05] | 17.33 | -0.04 [-0.06, 0.01] | yes | EQUAL_CI_HAS_0 |
| null | pair16 | 0.11 [-0.00, 0.19] | 9.71 | 0.01 [-0.00, 0.02] | yes | EQUAL_CI_HAS_0 |
| ab01_free_no_state_read | churn16 | -1.55 [-2.16, -1.47] | 15.51 | -0.24 [-0.33, -0.22] | NO | not admitted |
| ab01_free_no_state_read | churn64 | -0.64 [-1.12, 0.12] | 17.36 | -0.11 [-0.20, 0.02] | NO | not admitted |
| ab01_free_no_state_read | pair16 | -2.67 [-2.71, -2.58] | 9.64 | -0.25 [-0.26, -0.24] | yes | VARIANT_FASTER |
| ab02_free_no_state_write | churn16 | -1.05 [-1.40, -0.96] | 15.48 | -0.16 [-0.21, -0.15] | NO | not admitted |
| ab02_free_no_state_write | churn64 | -0.22 [-0.75, 0.26] | 17.33 | -0.04 [-0.14, 0.05] | NO | not admitted |
| ab02_free_no_state_write | pair16 | -0.32 [-0.41, -0.20] | 9.71 | -0.03 [-0.04, -0.02] | yes | VARIANT_FASTER |
| ab03_alloc_hit_no_state_write | churn16 | 407.44 [405.85, 408.26] | 15.70 | 944.09 [935.92, 952.74] | yes | BASE_FASTER |
| ab03_alloc_hit_no_state_write | churn64 | 397.11 [395.46, 397.77] | 17.59 | 931.16 [922.21, 943.58] | yes | BASE_FASTER |
| ab03_alloc_hit_no_state_write | pair16 | 457.01 [456.46, 457.36] | 9.45 | 916.26 [908.65, 930.91] | yes | BASE_FASTER |
| ab04_own_no_memo | churn16 | 2.07 [1.61, 2.63] | 15.35 | 0.32 [0.25, 0.41] | yes | BASE_FASTER |
| ab04_own_no_memo | churn64 | 1.89 [1.59, 2.15] | 17.35 | 0.33 [0.28, 0.38] | yes | BASE_FASTER |
| ab04_own_no_memo | pair16 | 0.35 [0.25, 0.50] | 9.76 | 0.03 [0.02, 0.05] | yes | BASE_FASTER |
| ab05_own_no_own_cache | churn16 | 7.38 [6.64, 7.86] | 15.83 | 1.19 [1.06, 1.30] | NO | not admitted |
| ab05_own_no_own_cache | churn64 | 6.90 [6.40, 7.27] | 17.76 | 1.26 [1.15, 1.33] | NO | not admitted |
| ab05_own_no_own_cache | pair16 | 11.39 [11.02, 12.24] | 9.91 | 1.17 [1.14, 1.26] | yes | BASE_FASTER |
| ab06_own_no_kind_at | churn16 | -2.28 [-2.70, -1.84] | 15.70 | -0.35 [-0.43, -0.29] | yes | VARIANT_FASTER |
| ab06_own_no_kind_at | churn64 | -1.97 [-2.11, -1.60] | 17.72 | -0.34 [-0.38, -0.28] | yes | VARIANT_FASTER |
| ab06_own_no_kind_at | pair16 | -1.02 [-1.20, -0.89] | 9.95 | -0.10 [-0.12, -0.09] | yes | VARIANT_FASTER |
| ab07_free_no_bump_check | churn16 | -1.90 [-2.57, -0.32] | 15.72 | -0.29 [-0.39, -0.04] | NO | not admitted |
| ab07_free_no_bump_check | churn64 | -0.96 [-1.28, -0.01] | 17.70 | -0.17 [-0.23, -0.00] | yes | VARIANT_FASTER |
| ab07_free_no_bump_check | pair16 | -0.57 [-0.76, -0.32] | 9.86 | -0.06 [-0.07, -0.03] | yes | VARIANT_FASTER |
| ab08_dealloc_tls_no_range_check | churn16 | -0.94 [-1.33, -0.43] | 15.90 | -0.15 [-0.21, -0.07] | NO | not admitted |
| ab08_dealloc_tls_no_range_check | churn64 | -0.27 [-0.44, 0.08] | 17.78 | -0.05 [-0.08, 0.01] | NO | not admitted |
| ab08_dealloc_tls_no_range_check | pair16 | -0.33 [-0.51, -0.20] | 9.87 | -0.03 [-0.05, -0.02] | yes | VARIANT_FASTER |
| ab09_free_no_payload_start | churn16 | 0.23 [-0.14, 0.88] | 15.69 | 0.04 [-0.03, 0.14] | yes | EQUAL_CI_HAS_0 |
| ab09_free_no_payload_start | churn64 | 0.23 [-0.37, 0.81] | 17.74 | 0.04 [-0.06, 0.14] | yes | EQUAL_CI_HAS_0 |
| ab09_free_no_payload_start | pair16 | -2.21 [-2.29, -2.02] | 9.96 | -0.22 [-0.22, -0.19] | yes | VARIANT_FASTER |
| ab10_free_push_no_bookkeeping | churn16 | 416.01 [415.75, 416.78] | 15.53 | 1027.81 [1016.88, 1040.79] | yes | BASE_FASTER |
| ab10_free_push_no_bookkeeping | churn64 | 409.43 [409.27, 409.82] | 17.30 | 1054.62 [1038.65, 1063.25] | yes | BASE_FASTER |
| ab10_free_push_no_bookkeeping | pair16 | 468.89 [468.38, 469.41] | 9.34 | 1036.66 [1023.97, 1050.24] | yes | BASE_FASTER |

### Что это значит

**Недействительные абляции (меняют механизм, а не убирают шаг):** ab03 и
ab10 замедляют вариант в 53–109 раз (+916…+1055 тиков/вызов): без записи
состояния (ab03) или без учёта `count` в push (ab10) горячий путь уходит
в медленную ветку на каждом вызове. ab11 роняет процесс. Эти три строки
не говорят о цене шага и в выводы ниже не входят.

**Кэши владельца нужны:** удаление memo (ab04) замедляет churn на
+2.07 % / +1.89 % (+0.32 / +0.33 тика/вызов), удаление own_cache (ab05) —
pair16 на +11.39 % (+1.17 тика/вызов); на churn ab05 не допущен
(A/A варианта вне порога), направление то же (+7.38 % / +6.90 %).

**Цена отдельных шагов own free/alloc** (допущенные ядра, вариант быстрее):

| Шаг | churn16 | churn64 | pair16 |
|---|---|---|---|
| ab06 загрузка `kind_at` + ветка Large | −2.28 % (−0.35 тика) | −1.97 % (−0.34) | −1.02 % (−0.10) |
| ab07 загрузка `bump_of` + сравнение | не допущено | −0.96 % (−0.17) | −0.57 % (−0.06) |
| ab01 чтение байта состояния | не допущено | не допущено | −2.67 % (−0.25) |
| ab09 выбор `payload_start` + reject | CI содержит 0 | CI содержит 0 | −2.21 % (−0.22) |
| ab02 запись байта состояния (free) | не допущено | не допущено | −0.32 % (−0.03) |
| ab08 проверка диапазона TLS | не допущено | не допущено | −0.33 % (−0.03) |

Самый дорогой отдельный шаг — ab06, 0.35 тика на вызов из 15.70
(2.2 %). Сумма всех допущенных экономий на churn64 (ab06 + ab07) —
0.51 тика из 17.7. ИЗМЕРЕНО: ни один из проверенных шагов не объясняет
разрыв с mimalloc на churn. ОЦЕНКА по предварительному
процесс-реплицированному прогону sefer против mimalloc (20 процессов ×
2000 блоков, тот же бинарь, ad-hoc сводка, не гейт): t_mimalloc/t_sefer
churn16 0.532, churn64 0.481, pair16 0.809, т. е. разрыв около
7–9 тиков/вызов на churn и около 2 на pair16 — на порядок больше
суммы цен шагов.

**Наблюдение для следующего шага (ОЦЕНКА из тех же чисел):** у sefer
вызов на churn16 стоит 15.5 тика против 9.7 на pair16 (+5.8 тика), у
mimalloc — около 8.2 против 7.9 (+0.3). Ядра отличаются порядком
освобождения: pair16 освобождает только что выданный блок, churn —
случайный из 256 живых. Значит, основная цена sefer на churn зависит от
порядка освобождения, а не от числа проверок на пути. Кандидаты (не
проверены): предсказание ветвлений по данным блока, зависимые загрузки
метаданных «старого» блока, store-forwarding. Нужны счётчики PMU или
абляции, меняющие порядок, а не шаги.

**Методический вывод для всех прошлых замеров по времени:** вердикт по
одному процессу на этой машине ненадёжен: копия кода против самой себя
давала до ±17 % в отдельном процессе. Прежние NO-GO и «равно», полученные
одним процессом на плечо, стоит перепроверить по схеме J1′ v2
(≥ 20 процессов, медиана и bootstrap по процессам).

### Артефакты

- Сырьё: 741 файл jsonl, 2.04 ГБ (сумма из манифеста) — tier 3, не в git; sha256 и размер
  каждого файла — `docs/perf/b4_ablate/v2/MANIFEST.txt`.
- Свод: `docs/perf/B4_ABLATE_CHURN_summary.csv` (одна строка на
  вариант × ядро), по вариантам — `docs/perf/b4_ablate/v2/summary_<variant>.csv`.
- Вывод генератора: `docs/perf/b4_ablate/v2/generator_stdout.txt`,
  повторный null — `docs/perf/b4_ablate/v2/rerun_null/`.
- Identity: `head_sha`, `source_diff_sha256`, `exe_sha256`,
  `variant_sha256` — в заголовке каждого raw и в CSV; коммит измерительной
  ветки — в сообщении коммита этого отчёта.
- Воспроизведение (из корня worktree, с RUSTFLAGS выше):
  `CARGO_TARGET_DIR=<target> node scripts/b4_ablate_run.mjs --out-dir <dir>
  --blocks 2000 --reps 20 --kernels churn16,churn64,pair16`, затем для
  каждого варианта `node scripts/b4_ablate_rep_table.mjs --dir <dir>
  --variant <v> --reps 20 --kernels churn16,churn64,pair16 --csv <csv>`.

## Коды возврата генератора (все ИЗМЕРЕНЫ в этой сессии)

- `ab01_free_no_state_read`: 0; `ab02_free_no_state_write`: 0;
  `ab03_alloc_hit_no_state_write`: 0; `ab04_own_no_memo`: 0;
  `ab05_own_no_own_cache`: 0; `ab06_own_no_kind_at`: 0;
  `ab07_free_no_bump_check`: 0; `ab08_dealloc_tls_no_range_check`: 0;
  `ab09_free_no_payload_start`: 0; финальный `null`: 0
  (sha256 копии `013c51eb165528d0a5a9a3631c691b3455f23a638adbd1c24cec0f2c3d982c72`).
- Дополнение (абляции magazine push/pop bookkeeping и G13):
  `ab10_free_push_no_bookkeeping`: 0 (sha256 `c44c7f73fd3b62d47d9738faad8cb71f99790bd9c281af8a5fd12debadfd3453`);
  `ab11_alloc_pop_no_bookkeeping`: 0 (sha256 `50d24eb91b0bc39aef4a58b6e26738f1cce9ef7b757094407abfee0402d57c99`);
  `ab12_free_no_park_budget_g13`: 0 (sha256 `48cb1d4b6c0bebf12d2daf8b9e55f812da863baaebc62014ae90f9d9fcc7a44c`).
  cargo check (`judge`, production,ablate) для ab10/ab11/ab12: rc=0 (1 pre-existing
  warning в примере, не связан с патчами); ни один не исключён из фазы 2.

## sha256 вариантов и cargo check (после ревью-фиксов P1+P2; ИЗМЕРЕНО в этой сессии; ab01–ab09 — sha РЕАЛЬНЫХ абляций P2)

| вариант | sha256 копии | changed_files | cargo check (judge, production,ablate) |
|---|---|---|---|
| null | `013c51eb165528d0a5a9a3631c691b3455f23a638adbd1c24cec0f2c3d982c72` | 0 | rc=0 |
| ab01_free_no_state_read | `06350d9f118b4a90ffa0b88aefd94bc71b14d82c29ad6a4fac2fada79ee405d7` | 1 | rc=0 |
| ab02_free_no_state_write | `b89b69172cb3c73f0026a394185acbe732b01a3aa223b8eabfac1adef7b1d975` | 1 | rc=0 |
| ab03_alloc_hit_no_state_write | `f01ec273df4cc7d48c49ecbe8954f87ca2e90a9e5ed3b6f68dfa5d2575ed218a` | 1 | rc=0 |
| ab04_own_no_memo | `eb69a47476fd1ab8728f2468696ef664d890ce8b07d2e25cb747de1df3d6a0de` | 1 | rc=0 |
| ab05_own_no_own_cache | `9a88d4574a7c3a017bcd92f97989c3af6d14e81fc685813ce7d56c27416a47e6` | 1 | rc=0 |
| ab06_own_no_kind_at | `b5b09f73cd2a2f8ee2aca6aa9c0efd9575dc3e93eecb72a9fdbec1789674be27` | 1 | rc=0 |
| ab07_free_no_bump_check | `f48211c0f023130260c8810eddabc5a017ff2ddb411cc06e9e8fe61643fa5f8b` | 1 | rc=0 |
| ab08_dealloc_tls_no_range_check | `097f3e268558b09c8f7fc1e7f9527863715d73ccf6747e53e2240e6741146342` | 1 | rc=0 |
| ab09_free_no_payload_start | `69aa9aa2bac7cd794c9f953a6d85e1d966fda4a72de9abdd459351aeb758ad3f` | 1 | rc=0 |
| ab10_free_push_no_bookkeeping | `c44c7f73fd3b62d47d9738faad8cb71f99790bd9c281af8a5fd12debadfd3453` | 1 | rc=0 |
| ab11_alloc_pop_no_bookkeeping | `50d24eb91b0bc39aef4a58b6e26738f1cce9ef7b757094407abfee0402d57c99` | 1 | rc=0 |
| ab12_free_no_park_budget_g13 | `48cb1d4b6c0bebf12d2daf8b9e55f812da863baaebc62014ae90f9d9fcc7a44c` | 1 | rc=0 |

Все 10 вариантов генерируются (rc=0) и собираются; ни один не исключён из
фазы 2. (Единственное замечание: `cargo check` варианта `ab06` даёт 1 warning
в сгенерированной копии `sefer-alloc-variant` — на сборку судьи не влияет.)

## Dry-run пайплайна и dry-run РЕАЛЬНОЙ абляции (финал раунда; ИЗМЕРЕНО)

- Историческая запись (фаза 1, шаг 5): dry-run абляции был НЕВАЛИДЕН —
  самописный applier генератора не применял ни один патч, копия была
  бит-в-бит равна null. Механизм исправлен (патчи применяются через
  `git apply` с assert'ами) и переверифицирован (см. «Механизм генерации»
  и таблицу sha256: у всех ab01…ab12 sha256 ≠ null и `changed_files=1`).
- **Dry-run РЕАЛЬНОЙ абляции (валидный, этот раунд):** генерация
  `ab01_free_no_state_read`: rc=0, sha256 копии
  `7f529010ec5ebc6fd8fda58f5ddfaab450a83972345ff0a693a9fedb825f80fb`
  (≠ null-sha `013c51eb…`, changed_files=1); build судьи (release,
  `--features production,ablate`): rc=0; ТРИ процесса судьи по ≤10 с
  (`--blocks 24`, seed 0xb4110001, order-seed 0x0b4d5eed):
  `--mode ab_variant` → rc=0, `--mode aa_variant` → rc=0,
  `--mode aa_sefer` → rc=0 (raw калибровки sefer; identity-поля variant =
  ab01/его sha — по ablate-контракту). Во всех трёх raw: header несёт
  `"variant_name":"ab01_free_no_state_read"` и sha ab01, oracle_mismatches=0,
  shim_calls_a = shim_calls_b = 3 079 296, ABBA = BAAB = 132 (строгий баланс).
  Генератор таблиц `b4_interleave_table.mjs` (ablate-контракт, identity
  тройки совпал): rc=0, 11 ядер, 11/11 не допущены (dry-run A/A CI шире
  порога 0.01 — ожидаемо при 24 блоках). Возврат `null`: rc=0, sha256
  совпал с `013c51eb…`.
- **Dry-run ФИНАЛЬНОГО ПАЙПЛАЙНА через runner `b4_ablate_run.mjs` (этот
  раунд):** `--dry-run --variants ab01_free_no_state_read
  --aa-sefer-only-null --no-null-gate --run-timeout 10`, out-dir
  `.rush/wsl/ab3_dry/` (вне репозитория). Runner'ом выполнено: makeVariant
  null (sha256 `013c51eb…`, changed_files=0) → build → 3 судьи null
  (aa_sefer, aa_variant, ab_variant; `--blocks 24`; все rc=0,
  oracle_mismatches=0, ABBA=BAAB=48, каждый raw = header + 4×24=96 записей,
  headers: variant_name=null + sha null). Затем runner УПАЛ (rc=1) на
  генераторе null-таблицы: ENOENT — каталог `<out-dir>/b4_ablate/` не
  создавался перед записью `--csv` (баг-fix: mkdirSync dirname csv-файла
  в runner И в генераторе). Дозапуск вручную (по решению оператора, для
  соблюдения бюджета судей): генератор null-таблицы на тех же raw (БЕЗ
  `--expect-admitted`, т.к. `--no-null-gate`) → rc=0, 4 ядра, 4/4 не
  допущены (dry-run A/A CI шире порога — ожидаемо при 24 блоках);
  `summary_null.csv` в out-dir (47 колонок, вкл. variant_name/variant_sha256);
  свод-строки (null×4) сохранены в `.rush/wsl/ab3_dry_summary_copy.csv`,
  dry-run-артефакт `docs/perf/B4_ABLATE_CHURN_summary.csv` из репозитория
  УДАЛЁН (настоящий свод создаст фаза 2). Затем makeVariant ab01
  (sha256 `06350d9f118b4a90ffa0b88aefd94bc71b14d82c29ad6a4fac2fada79ee405d7`,
  changed_files=1) → build → ровно 2 судьи (aa_variant, ab_variant;
  те же identity-флаги): rc=0, headers: variant_name=ab01_free_no_state_read
  + sha ab01, oracle_mismatches=0, ABBA=BAAB=48. Таблица ab01 НЕ
  генерировалась — ожидаемое следствие `--aa-sefer-only-null` (нет ab01
  aa_sefer; в фазе 2 флаг OFF, все тройки полные). Возврат null: rc=0, sha
  `013c51eb…`, `ABLATION`-маркеров в копии нет.
- **Честный счёт процессов судьи (этот dry-run): 5 из 5** — null
  aa_sefer/aa_variant/ab_variant (runner) + ab01 aa_variant/ab_variant
  (вручную). Полных прогонов фазы 2 не было.

## git diff fingerprint

`git diff | sha256sum` (финал этого раунда; `git diff` покрывает только
TRACKED-изменения — ablate-артефакты untracked и в него не входят, поэтому
значение стабильно):
`f81bfc8810109c5487b652805359a42c7d33f56d4d56cd91cc1813ff8490071b`.
