# Adversarial bug-hunt review (широкая охота на баги)

**Дата:** 2026-07-09
**Ревьюер:** независимый @fxx-агент (угол №7 — самая широкая adversarial-охота:
ловим то, что 6 прицельных ревью пропускают между своими рамками).
**Профиль сборки в фокусе:** `production` (= alloc-global + alloc-xthread +
alloc-decommit + fastbin), плюс `production hardened`, `alloc-runfreelist`,
`numa-aware`, Windows/Unix.

## Scope

Прочитаны целиком: `src/alloc_core/{alloc_core,segment_header,remote_free_ring,
segment_table,alloc_bitmap,os,node,bootstrap,run_stack,numa}.rs`,
`src/alloc_core/deferred_large/*`, `src/registry/{heap_core,heap_registry,
heap_slot,bootstrap,tcache}.rs`, `src/global/{sefer_alloc,tls_heap,fallback}.rs`,
`crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`, `docs/INVARIANTS.md`,
`.github/workflows/{ci,perf-gate,kani}.yml`, образец
`docs/reviews/2026-07-06-x-arc-retrospective.md`, и выборка из ~30
regression-тестов.

## Методология

По 8 разделам ТЗ: (1) off-by-one в offset/size/bound, (2) достаточность
atomic-ordering для каждого нетривиального места, (3) ABA без generation/tag,
(4) double-free/UAF СТРУКТУРНО ПОХОЖИЕ на документированный M2-residual, но в
ДРУГИХ местах, (5) паника/abort на alloc-пути, (6) асимметрия Windows vs Unix
веток, (7) фич-комбинации из Cargo.toml, не покрытые CI, (8) regression-тесты
как подсказки на структурно-похожие незакрытые дыры.

**Итог коротко:** одна реально достижимая Critical/High (крэш в глобальном
аллокаторе на Windows под RSS-давлением — раздел 6), одна значимая Medium
(целый пласт фич не гоняется в CI, только в локальном pre-push gate — раздел 7),
несколько Low/Note. Ядро корректности (M2-оракулы, ring↔freelist в
non-fastbin-конфиге, batched-carve/drain end-states, ABA-теги) выдержало
adversarial-разбор — по разделам 1–4 НОВЫХ дыр сверх документированных не
найдено; это отдельно зафиксировано ниже как положительный результат.

---

## CONFIRMED

### C1 — Windows `recommit` игнорирует отказ `VirtualAlloc(MEM_COMMIT)` → крэш в глобальном аллокаторе (разделы 5+6)

**Severity: High** (реально достижимый abort/крэш процесса в
`#[global_allocator]` под нагрузкой, ради которой `alloc-decommit` и
существует; это НЕ Rust-UB, а OS access-violation, но эффект — падение
процесса из аллокатора, ровно класс «паника-в-аллокаторе = abort» из ТЗ).

- **Место:** `crates/vmem/src/lib.rs:386-399` (`recommit_pages_impl` под
  `cfg(all(windows, not(miri)))`).
- **Дефект:** функция вызывает `VirtualAlloc(addr, len, MEM_COMMIT,
  PAGE_READWRITE)` и **игнорирует возвращаемое значение** (в отличие от
  reserve-пути `winapi_virtual_alloc`/`reserve_aligned_raw:335-337`, который
  проверяет `NonNull::new(p)?`). На Unix `recommit_pages_impl` — no-op
  (`:518-521`, MADV_DONTNEED-страницы возвращаются лениво по page-fault), так
  что **асимметрия платформ реальна**: Windows-ветка может жёстко провалить
  recommit, а код это не проверяет.
- **Кто зовёт:** ТОЛЬКО под `alloc-decommit`, из `AllocCore::carve_block`
  (`src/alloc_core/alloc_core.rs:3132-3136`) и `carve_batch`
  (`:3215-3219`) — когда `meta.is_decommitted()`. Т.е. сегмент был
  decommit-нут (`MEM_DECOMMIT` вернул payload-страницы ОС при
  `live_count==0`), и при повторном использовании мы делаем recommit.
- **Сценарий отказа (конкретный):** поток A аллоцирует много мелких блоков в
  сегменте S, потом освобождает все → `dec_live_and_maybe_decommit` →
  `decommit_empty_segment` возвращает payload S ОС и ставит `decommitted=1`.
  Позже A аллоцирует снова из S → `carve_block` видит `is_decommitted()==true`
  → `os::recommit_pages(S, small_meta_end, SEGMENT)` → `VirtualAlloc(MEM_COMMIT)`.
  Если в этот момент система на пределе commit-charge (RAM + pagefile
  исчерпаны — именно тот RSS-профиль, который `alloc-decommit`/`rss_probe`
  нагружают), `VirtualAlloc` возвращает NULL, но код игнорирует это,
  выполняет `meta.set_decommitted(false)`, и `carve_block` тут же пишет
  `Node::deref(S, aligned_bump)` → запись в **зарезервированную, но НЕ
  закоммиченную** страницу → STATUS_ACCESS_VIOLATION → падение процесса
  изнутри аллокатора.
- **Вторичное отравление:** после провалившегося recommit флаг
  `decommitted` уже сброшен в `false`. Значит будущие `carve_block` по тому
  же S **пропустят** recommit (`is_decommitted()==false`) и снова упрутся в
  ту же не-закоммиченную страницу — сегмент S навсегда «отравлен» с точки
  зрения аллокатора (если бы AV не убил процесс сразу).
- **Почему прицельные ревью могут пропустить:** FFI/supply-chain-ревьюер (№6)
  увидит это как «не проверен код возврата syscall» и, возможно, оценит как
  Low-нит; unsafe-ревьюер (№1) — как «не Rust-UB, значит вне зоны». Между
  ними теряется, что это **достижимый крэш глобального аллокатора именно на
  той рабочей нагрузке, ради которой фича включается**, на primary dev-OS
  (Windows, см. `ci.yml:114`).
- **Форма фикса:** `recommit_pages_impl` (Windows) должна проверять результат
  `VirtualAlloc(MEM_COMMIT) != NULL`; `os::recommit_pages` / `carve_block` /
  `carve_batch` — вернуть OOM-сигнал (не сбрасывать `decommitted`, вернуть
  `None`/null из carve вместо записи в страницу), как это делает reserve-путь.
  Т.е. recommit должен стать fallible и его отказ — честный OOM, а не «пишем
  в память и надеемся».

---

### C2 — Целый пласт фич-комбинаций НЕ гоняется в CI `cargo test` (раздел 7)

**Severity: Medium** (verification-gap, не баг в коде; но именно этот класс
пропустил red CI на 17 коммитов — см. CLAUDE.md «Before every push»).

Сопоставление feature-матрицы в `.github/workflows/ci.yml` с полным списком
фич в `Cargo.toml`:

- `.github/workflows/ci.yml` во ВСЕХ job-ах ссылается только на `hardened`
  (6×) и `numa-aware` (8×, в выделенных numa-job-ах). НИ ОДНОГО упоминания
  `alloc-runfreelist`, `alloc-stats`, `pinning` в командах `cargo test`.
- `--all-features` встречается ТОЛЬКО в compile-only контекстах: clippy
  (`ci.yml:59`), `cargo check --all-features` MSRV (`:175`), `cargo doc`
  (`:660`). Ни одного `cargo test --all-features`. Т.е. эти фичи
  КОМПИЛИРУЮТСЯ, но их поведение НИКОГДА не ИСПОЛНЯЕТСЯ в CI.

Конкретные необкатанные пути:

1. **`alloc-runfreelist` (PERF-3, RunStack).** Тесты
   `tests/regression_run_stack_{flush,drain,decommit}.rs` файлово гейтятся на
   `alloc-core` (компилируются в CI-строке `--features alloc-core`), НО их
   тело — сами тест-функции, дёргающие `RunStack` — под
   `#[cfg(feature = "alloc-runfreelist")]` (напр.
   `regression_run_stack_flush.rs:59,74,91,114,157,209`). Поскольку ни одна
   CI-строка `cargo test` не включает `alloc-runfreelist`, эти функции
   **вырезаются препроцессором в каждом CI-прогоне** → RunStack
   flush/divert/drain/decommit-clear в CI не исполняется НИКОГДА.
   Единственное реальное исполнение — локальный `npm run check`
   (`scripts/check-all.mjs:63-65`: `cargo test --features "production
   alloc-runfreelist"`). Это критично, потому что в ЭТОМ коде уже нашли
   реальный баг-утечку: Ф4 remainder-pushback (`alloc_core.rs:2962-3014`,
   комментарий «found by @o46m's Ф3 review» — un-drained члены дескриптора
   терялись при `block_size > 8192`). Сложный код (sort-then-divert,
   pop-then-iterate с остаточным push-back) под защитой только pre-push-гейта,
   не CI.
2. **`alloc-stats`.** Инкремент hit-счётчиков (Relaxed load+store) на ХОТ-пути
   (`heap_core.rs:701-707` magazine-hit; `alloc_core.rs:3422-3430`
   large-cache-hit) и ассерты в `tests/stats_reflects_activity.rs:130-146`
   (`assert!(after.tcache_hits > before.tcache_hits)` и т.п.) — под
   `#[cfg(feature = "alloc-stats")]`. CI не включает `alloc-stats` в
   `cargo test` → корректность того, что hit'ы реально считаются, в CI не
   проверяется.
3. **`pinning`.** `tests/pinning.rs:17` — `#![cfg(feature = "pinning")]`. Ни
   одна CI-строка не включает `pinning` → routing-binding
   (`bind_current_thread_to_shard`) в CI `cargo test` не исполняется, только
   компилируется clippy'ем.
- **Форма фикса:** добавить в матрицу `test` (ci.yml:77) строки
  `--features "production alloc-runfreelist"`,
  `--features "production alloc-stats"`, `--features pinning` (и, желательно,
  `--all-features` как замыкающую), чтобы поведение, а не только компиляция,
  гонялось в CI, а не только в локальном `npm run check`.

---

## POSITIVE (adversarial-разбор выдержан — важно для честной картины)

Эти проверки специально искали дыры и НЕ нашли — фиксирую, чтобы отчёт не
создавал ложного впечатления, что всё вокруг сломано.

### P1 — ring↔freelist double-free в non-fastbin xthread-конфиге БЕЗОПАСЕН (раздел 4)

Документированный M2-residual (`docs/INVARIANTS.md` M2) — это конкретно
ring↔**magazine** (fastbin). Проверил структурно-похожий кандидат:
ring↔**freelist** в конфиге `alloc-global alloc-xthread` (без fastbin — это
РЕАЛЬНАЯ CI/deploy-строка). Разбор: кросс-поточный free P кладёт `(off,class)`
в ring, битмап НЕ трогает. Own-thread free P: `dealloc_small`
(`alloc_core.rs:3305`) видит `is_free==false` → линкует P на freelist,
`mark_free(P)` (битмап→FREE). Позже drain: `reclaim_offset`
(`alloc_core.rs:957`) проверяет `bm.is_free(off)` → теперь TRUE → `return
false` (no-op). Итог: P на freelist РОВНО один раз, ring-запись безопасно
отброшена. **Битмап ловит эту ногу.** Дыра из M2 существует именно потому,
что magazine — ОТДЕЛЬНОЕ место, невидимое битмапу; freelist — нет. Дыры в
non-fastbin-конфиге нет.

### P2 — atomic ordering: `drain`'s Relaxed `head`-load обоснован (раздел 2)

`RemoteFreeRing::drain` (`remote_free_ring.rs:621`) грузит `head` Relaxed. Это
не баг: `head` пишет ТОЛЬКО единственный consumer; смена владельца ring'а идёт
через recycle→claim, где `HeapRegistry::recycle`
(`heap_registry.rs:218`, Release) ↔ `claim` CAS
(`heap_registry.rs:78`, AcqRel/Acquire) образуют HB-ребро, так что новый
владелец видит финальный `head` прежнего. Producer'ы `head` не трогают
(пишут только `tail`+slots). Fallback-heap сериализован спинлоком
(`fallback.rs:206`, Acquire/Release). Vyukov-протокол push/drain
(AcqRel/Release/Acquire) корректен. Под-review: OPT-C stamp Relaxed-load
(`heap_core.rs:1544`) — single-writer own-thread, ABA закрыт re-check
`owner_id`; gen-table Relaxed (`segment_header.rs:1266,1297`) — staleness-key,
не sync-примитив, 1/256 wrap задокументирован. НОВЫХ мест недостаточного
ordering не найдено.

### P3 — ABA-теги на месте (раздел 3)

`free_slots` (16-bit index + 48-bit tag, `tagged_ptr` + W7a) и `abandoned_segs`
(full-base + 22-bit tag в low-битах, `bootstrap.rs:204-239`) — оба тегированы,
wrap покрыт `regression_counter_wrap.rs`. `thread_free` deferred-large —
single-consumer (только owner pop) + double-push claim-CAS
(`deferred_large/push.rs`, #143-фикс, loom-проверен). OPT-C stamp-cache ABA —
закрыт сравнением `owner_id` после cache-hit (`heap_core.rs:1545`), а сам кэш
никогда не дереференсится напрямую (сравнивается живой `base` с кэшем,
`heap_core.rs:1539`). Незащищённого прямого сравнения указателя/индекса в
конкурентном контексте не найдено.

### P4 — off-by-one в batched-путях: end-states совпадают с per-block (раздел 1)

`carve_batch` (`alloc_core.rs:3201`): `room=(SEGMENT-aligned_start)/block_size`,
`n=min(out.len(),room)` → `aligned_start+n*block_size <= SEGMENT` (граница
per-block, батчем). `alloc_large`: `needed`/`usable` защищены
`Layout`-инвариантом + `checked_add` в `reserve_aligned:330`. `AllocBitmap::
locate` — `bit=off>>MIN_BLOCK_SHIFT`, debug-assert на границу. Ring wrap —
`wrapping_sub`/power-of-two CAP (const-assert `remote_free_ring.rs:167`).
Batched `flush_run`/`drain_freelist_batch`/`carve_batch` доказательно
byte-identical per-block (проверены doc-инварианты D1/M2 + counterfactual-тесты).
Единственная реальная off-by-one/утечка этого класса (Ф4 remainder-pushback)
уже ИСПРАВЛЕНА и вне production (см. C2 п.1).

### P5 — паника через `unwrap`/`expect` на alloc-пути не достижима (раздел 5)

`large_cache[idx].take().unwrap()` (`alloc_core.rs:3401,3724,3829`) —
`idx`/`victim_idx` из `position()`/`oldest_occupied_slot()`, вернувших `Some`.
`split_first_mut().expect("c < SMALL_CLASS_COUNT")` (`heap_core.rs:1441`) — `c`
из `class_for`, всегда `< SMALL_CLASS_COUNT`. `Default::default().expect(...)`
(`alloc_core.rs:4045`) — только construction-time, задокументировано.
Реальный «крэш на alloc-пути» — это C1 (OS AV, не Rust-panic).

### P6 — macOS lazy `MADV_DONTNEED` НЕ создаёт корректностной баг (раздел 6)

`crates/vmem/src/lib.rs:500-515` честно документирует, что на XNU/*BSD
`MADV_DONTNEED` advisory/lazy (нет Linux zero-fill-on-access). Проверил, что
ни один ВНУТРЕННИЙ путь не полагается на OS-зануление recommit-нутых payload-
страниц: `alloc` отдаёт uninitialized (контракт GlobalAlloc), `alloc_zeroed`
зануляет явно (`Node::zero`, `alloc_core.rs:487` / `heap_core.rs:783`),
метадата инициализируется явными записями. Claim в комментарии верен — не баг.

---

## Разбор по regression-тестам (раздел 8) — структурно-похожие дыры

Прочитаны `regression_ring_cursor_wrap`, `regression_counter_wrap`,
`regression_magazine_scan_bounds`, `regression_refill_window_double_issue`,
`regression_xthread_double_free_residual`, `regression_run_stack_flush`,
`regression_hardened_interior_ptr` (по заголовкам/counterfactual'ам).

- `regression_ring_cursor_wrap` пинит u32-wrap ring'а. Структурный аналог —
  теги Treiber-стеков (`free_slots`/`abandoned_segs`) — ПОКРЫТ
  `regression_counter_wrap`. Не-покрытого аналога wrap нет.
- `regression_magazine_scan_bounds` пинит «не читать slot >= cnt» в magazine.
  Аналогичный паттерн (чтение по индексу за границей живого окна) искал в
  refill/flush-циклах: `refill_class_bump_impl`, `flush_run`,
  `drain_freelist_batch` используют точные границы (`out[..filled]`,
  `run`, `accepted_n < CAP`). Не-покрытого аналога stale-read нет.
- `regression_refill_window_double_issue` (R1/retro-C1) закрыл
  out-buffer-ногу. Проверил остаточную симметрию: guard
  (`alloc_core.rs:2044-2047`) переоборачивает `out[..filled]` каждую
  итерацию, включая блоки из ДРУГИХ сегментов (все класса `class_idx`) —
  покрыто. Новой ноги в этом окне не видно.
- `regression_xthread_double_free_residual` — re-issue-before-drain остаётся
  RED+`#[ignore]` в non-hardened (информационно-неразличимо), DECIDABLE под
  `hardened` (gen-guard). Соответствует докам. Новой дыры нет.

---

## NOTE / Low

- **N1 (Low, раздел 7):** `alloc-runfreelist` × `hardened` (обе фичи двигают
  `small_meta_end` и добавляют метадата-регионы) собираются ВМЕСТЕ только
  `--all-features` clippy (compile-only). Const-assert'ы
  (`segment_header.rs:1358,1371`) поймают переполнение layout при компиляции,
  но рантайм-взаимодействие двух регионов в одном сегменте не исполняется
  нигде. Косметика (const-assert закрывает опасную часть), но стоит упомянуть.
- **N2 (Note):** `AllocCore::realloc` (`alloc_core.rs:1598`) production-мёртв
  (retro-C2 отметил дубликат OPT-F/OPT-G логики; теперь он выделен в общий
  `realloc_inplace_fast_path_known_base`, так что дубликат СВЕДЁН). Проверил —
  расхождения между `realloc` и `try_realloc_inplace_known_base` больше нет
  (общий источник истины). Ранее-C2-hazard закрыт. Фиксирую как проверено-чисто.
- **N3 (Note):** `bootstrap.rs:176` `#[allow(dead_code)]` на
  `ABANDON_SEG_SIZE` — обоснован MSRV-1.88 (dead-code-анализ не считает
  ссылку в `const _: () = assert!` использованием). Не баг.

---

## Итоговый вердикт

- **C1 (High) — исправить до релиза с `alloc-decommit` на Windows:** recommit
  должен быть fallible; провал `VirtualAlloc(MEM_COMMIT)` — честный OOM
  (вернуть null из carve, НЕ сбрасывать `decommitted`, НЕ писать в страницу),
  а не запись в не-закоммиченную память → AV. Это единственная находка с
  реально достижимым падением процесса из глобального аллокатора.
- **C2 (Medium) — расширить CI-матрицу:** `alloc-runfreelist`, `alloc-stats`,
  `pinning` компилируются, но их ПОВЕДЕНИЕ в CI не исполняется (только в
  локальном `npm run check`). Учитывая, что реальный баг (Ф4 remainder-leak)
  жил именно в RunStack-коде, это разрыв верификации того самого класса,
  который CLAUDE.md называет причиной red-CI-инцидента.
- **Ядро корректности выдержало adversarial-разбор:** M2-оракулы,
  ring↔freelist (non-fastbin), batched-carve/drain end-states, atomic-ordering
  (включая спорный Relaxed `head`-load), ABA-теги, no-panic-на-alloc — по
  разделам 1–5 НОВЫХ дыр сверх уже документированных residual'ов не найдено
  (P1–P6). Это положительный сигнал о зрелости фундамента, а не отсутствие
  проверки — каждый пункт разобран с конкретным контр-сценарием.
