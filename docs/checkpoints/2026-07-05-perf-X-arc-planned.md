# Checkpoint — 2026-07-05 · perf/correctness X-arc planned, all pushed & CI green

## Session summary

Продолжение sefer-alloc 0.3.x (100% Rust `#[global_allocator]`, D:\dev\rust\sefer-alloc; branch main; **всё запушено, `origin/main` = `6f6206b`, дерево чистое, CI зелёный**). Эта длинная сессия закрыла ТРИ последовательные ревью-арки и оставила ЧЕТВЁРТУЮ (перф/корректность) РАСПЛАНИРОВАННОЙ, но НЕ начатой.

**Что реализовано и запушено в этой сессии (все с личным zero-trust + counterfactual, коммит между фазами):**
- **Review-followup (#169–172):** MUST-1 C2-realloc-регрессия (стамп+A1-drain в HeapCore::realloc own-ветке — leak-to-abort фикс), reclaim_offset class-bounds guard (panic→skip), CI-арка (Windows/macOS/workspace/MSRV/production-hardened/aarch64-production/release-gate), MUST-2 доки.
- **W1–W6 (#173–178):** W1 WSL iai Ir-судья (`npm run iai`, `scripts/iai.mjs`, valgrind 3.22, byte-deterministic; baseline `docs/perf/IAI_BASELINE.md`); W2 SegmentTable tombstone-rebuild (перф-обрыв долгоживущего сервера); W3 stats→HeapSlot (закрыл SB-щель, miri-доказано) + фича `alloc-stats` (production Ir НИЖЕ baseline); W4 carve_batch (**cold 16–64B −6.3k Ir**, byte-identical, E2/E4 honest-rejected с числами); W5 MSRV dead_code + macOS CI + fuzz×3; W6 plain-miri + TSan Large-xthread.
- **W7 durability (#179–181):** W7a generation→AtomicU64 + TaggedPtr index:16|tag:48 (wrap недостижим, 0 hot-path цены; ЧЕСТНО — defence-in-depth, не живые баги: staleness держит TORN); W7b ring-курсоры u32 (wrap ДОСТИЖИМ, но safe by design — const-assert power-of-two + boundary-тесты через u32::MAX под нагрузкой, counterfactual лично); W7c `docs/DURABILITY.md` (инвентарь всех wrappable-счётчиков + правило).
- **Доки:** CHANGELOG + README синхронизированы (111 тест-файлов, 3 fuzz-таргета, DURABILITY.md в индексе).
- **«Наладил CI»:** CI был КРАСНЫМ на прошлых пушах. Воспроизвёл обе поломки локально в точной среде: (1) fuzz-build — все 3 таргета на устаревшем `#[export_name]` НИКОГДА не собирались (bit-rot, вскрыт W5-джобой); перевёл на `#![no_main]`+`fuzz_target!`, форсил gnu-таргет; проверил в WSL (nightly+cargo-fuzz). (2) clippy --all-features -D warnings — `assertions_on_constants` в W7-тесте → compile-time `const _: () = assert!`. Итог прогон `28738733347` = **success**, 0 упавших джоб.

**СЕЙЧАС В ПОЛЁТЕ:** ничего не исполняется. Пользователь спросил «не осталось ли мест кардинально ускорить»; я проверил код и нашёл ГЛАВНОЕ: **Large-realloc копирует ВСЕГДА** (OPT-F in-place только small same-class; `realloc_grow` бенч = 1 520 714 Ir = 19× любого другого — memcpy-этажи + сегментные церемонии, хотя рост 512K→1M→2M→4M влезает в тот же округлённый-до-4MiB span_usable). Затем по запросу составил приоритетный план и завёл 6 тасок (#182–187). Ждём команду «реализуй».

**Рабочие гипотезы (живые):** X1 (in-place Large realloc) даст падение realloc_grow в разы при нулевом риске гарантий (M4 тот же ptr, #138 сверяет large_size-хедер → обновление консистентно). X2 (#164 F-фикс по готовому дизайну) закроет ПОСЛЕДНИЙ M2-резидуал. Хвост (X4–X6) требует лучших судей (X3: cache-sim/EstimatedCycles — Ir считает udiv за 1 инструкцию; multiseg-бенч; fault-probe).

**Судьи/инфра:** `npm run iai` — детерминированный Ir через WSL; `scripts/{tsan,miri,loom,iai}.mjs`; baseline в `docs/perf/IAI_BASELINE.md` (пост-W7: cold_256x16b=123512, small_churn=80793, recycle_256x16b=175892, realloc_grow≈1.52M). WSL: nightly+cargo-fuzz+valgrind готовы; при вызове cargo из WSL СБРАСЫВАТЬ RUSTC_WRAPPER (Windows sccache наследуется).

## Active goal

none (стоп-хук не активен; babysit НЕ активен — предыдущий self-deleted при опустошении TaskList).

## TaskList

### pending (перф/корректность X-арка — НЕ начата)
- #182 X1: in-place Large realloc growth (рост в span_usable без копии) — главный перф-рычаг, судья готов (realloc_grow + 9 byte-identical)
- #183 X2: F-фикс #164 hybrid conflict-list drain (закрыть последний M2-резидуал) — дизайн готов (docs/design/RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md, кандидат d)
- #184 X3: апгрейд судей — cache-sim/EstimatedCycles + multiseg cold-бенч + fault-probe
- #185 X4: recycle-эксперименты — per-class cap 32 + 64-бит bloom-сигнатура (keep-or-reject по Ir)
- #186 X6: clz-класс вместо SIZE2CLASS-таблицы (blockedBy #184)
- #187 X5: per-class очереди сегментов O(1) refill-miss (blockedBy #184)

### recently completed (эта сессия)
- #169 MUST-1 C2-realloc, #170 reliability, #171 CI-арка, #172 MUST-2 доки
- #173 W1 iai-судья, #174 W2 tombstone, #175 W3 stats/SB+alloc-stats, #176 W4 carve_batch, #177 W5 MSRV/macOS/fuzz, #178 W6 plain-miri/TSan
- #179 W7a generation/TaggedPtr, #180 W7b ring-wrap, #181 W7c DURABILITY.md

## Decisions

- **Порядок X-арки: X1 (перф) → X2 (корректность) первыми** — оба «кардинальные» с готовым судьёй/дизайном и низким/contained риском; хвост (X4–X6) под судьями X3, honest-reject валиден.
- **X1 in-place Large realloc — выбран как единственный оставшийся «кардинальный» перф** (realloc_grow 19× любого бенча = чистые memcpy-этажи). mremap (рост сверх спана) ЯВНО отложен отдельной будущей задачей.
- **Отклонено без тасок:** pre-fault (B) — Ir его не видит, нет fault-судьи; huge pages — привилегии/ниша; E2-LUT и др. уже honest-rejected с числами.
- **W7a подан честно как defence-in-depth, не bug-fix** — generation-wrap не был живым ABA (TORN держит staleness); tag-wrap вероятностный. Расширение за 0 цены (generation byte-identical, tag −4 Ir холодного bootstrap).
- **CI-фиксы — воспроизводить локально в точной среде (WSL) перед пушем**, не push-and-pray; fuzz bit-rot вскрыт именно новой W5-джобой (ценность build-only гейта).

## Open questions

- Ждём команду «реализуй» для запуска X-арки (#182 → #187, @oh, zero-trust + Ir-гейт + counterfactual, коммит между фазами). Порядок по blockedBy: 182,183,184 свободны; 186,187 ждут 184.
- X3 fault-probe: неясно, дадут ли WSL perf-counters page-faults; если нет — судья для pre-fault отсутствует, пункт закрывается отказом.
- Тег/publish 0.3.0 + yank 0.2.1 — по-прежнему отдельное явное решение пользователя (готовность есть; docs/CHANGELOG актуальны).

## Repo state

```
(clean — nothing to commit; origin/main == HEAD, 0 ahead)
```

```
6f6206b fix(test): compile-time const-assert in regression_counter_wrap (clippy gate)
90d4d11 fix(fuzz): port targets to libfuzzer-sys fuzz_target! macro (build was broken)
1668b04 ci: fix fuzz-build — force gnu target so the sanitizer links a real std
98d533e docs: sync CHANGELOG + README with the W7 long-run durability pass
3000cc8 style: cargo fmt (W7b ring-wrap test multi-line asserts)
```

CI: последний прогон `28738733347` — completed/success (0 failures; fuzz-build + clippy-all-features починены). crates.io = 0.2.1 live; Cargo.toml = 0.3.0, тег НЕ создан.
