# Correctness / CI-debt open items -- [T] Tracked tier -- documented-but-unproven panic-/unwind-safety residuals in shipping code

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_publish_readiness.md`, `TRACKED_process_record.md`, `TRACKED_misc.md`) for the rest of
the tier.

**Criterion for this file:** A card belongs here if it documents a known, honestly-recorded gap in a panic-safety or unwind-safety guarantee of shipping (non-hook, non-platform-specific) code -- a residual the code's own doc comments already name, not yet a proven live bug.

**Card count:** 6.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
16. **[T, filed 2026-08-04, R34-2/task #521] Cross-thread routing's documented
    residual (caller-contract-violation surface) needs to reach the release
    notes (`docs/reviews/2026-08-04-release-stabilization-audit.md` F-3 [low]).**
    `dealloc_foreign_routing` (`src/registry/heap_core_xthread.rs:858-1007`)
    reads and writes foreign segment memory under a "magic != 0" guard only;
    the code documents honestly (`:864-885`) that a live-foreign vs
    already-released segment is O(1)-indistinguishable, so a double free of a
    released segment is "fundamentally UB … not fixed by this change" — the
    standard caller-contract residual every allocator has. The action item is
    NOT a code fix (none is needed — for a single legitimate cross-thread free,
    `live_count ≥ 1` until the owner's drain reclaims, so the segment cannot be
    released underneath the freer); the action is to **state this residual in
    the release notes** so a downstream reader knows the documented limitation.
    Filed because no Round-34 task owns release-notes writing.

    **Status: RESOLVED (2026-08-05, task #597/K2, commit `f43600d`).** The
    exact action this item requested — a release-notes statement of the
    residual — landed in `CHANGELOG.md`'s new "Known limitations (as of
    this release)" subsection. Left in place rather than moved to "Recently
    resolved" / renumbered (that structural cleanup, spanning several
    pre-existing item-numbering gaps in this file, is task M2/#623's
    broader scope, not duplicated here item-by-item).

22. **[T, filed 2026-08-05, task #575/H5, `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H5] `RemoteFreeRing::DrainHeadPublish`'s panic-safety guard is unwind-safe for already-fully-processed elements but NOT exactly-once for the element in flight when a panic occurs — a documented residual (Sol-F5, task #567) never cross-filed into this index.**

    - **Status:** OPEN, residual — not a proven bug, no known reachable
      trigger, filed for tracking per this index's own convention (a
      doc-comment naming a follow-up must also be cross-filed here so a
      future round inherits it without re-deriving from the source).
    - **Current-number-or-verdict:** by inspection, the current production
      `reclaim` closures (`AllocCore::reclaim_offset` /
      `AllocCore::reclaim_offset_checked`,
      `src/alloc_core/small/alloc_core_small_reclaim.rs`) do not panic after
      mutating state on their current code paths — no `unwrap`/`expect`/
      `panic!`/unchecked indexing on the mutation-bearing paths. This is an
      observation about the code AS WRITTEN, not a structural guarantee: the
      type system does not prevent a future `reclaim` closure from
      panicking after a mutation. `RemoteFreeRing::drain`'s loop body calls
      `reclaim(off)` BEFORE clearing the slot and BEFORE
      advancing/publishing `h` — so a reclaim that mutates state and then
      panics leaves the slot non-empty and `h` one short; a
      `catch_unwind`-resuming caller would re-pass that same `off` to
      `reclaim`, i.e. `reclaim` could run twice for the in-flight element.
    - **Why not currently exploitable:** any unwind that escapes through the
      `GlobalAlloc` entry points still aborts the process
      (`src/global/sefer_alloc.rs`'s panic-tripwire docs), so this replay
      window is reachable only through a direct/internal `catch_unwind`
      around `drain` — not through ordinary allocator usage.
    - **What would close it structurally:** a two-phase/idempotent reclaim
      protocol (clear-then-reclaim, or a reclaim that can be safely retried
      against an already-cleared slot), or an explicit poison/skip policy
      for the in-flight element on unwind — out of scope for the
      `DrainHeadPublish` guard itself, which only ever publishes `h` values
      fully advanced past a cleared slot.
    - **Next trigger:** reopen and design the two-phase protocol if a future
      `reclaim` closure gains fallible/panicking code on a mutation-bearing
      path, or if a direct/internal `catch_unwind` caller around `drain` is
      ever added to production code (currently none exists).
    - **Evidence:** `src/alloc_core/segment/remote_free_ring/mod.rs`'s
      `DrainHeadPublish` doc comment (the "Exact contract (Sol-F5, task
      #567 ...)" section, ~lines 861-900);
      `docs/reviews/2026-08-05-sol-release-readonly-review.md` finding F5;
      `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding
      H5.

23. **[T, filed 2026-08-05, task #575/H5, `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H5] `InitStateGuard`'s unwind rollback does not distinguish a pre-write unwind (nothing to clean up) from a post-write unwind (a live `HeapCore` already sits in `FALLBACK`) — a documented residual (Sol-F6, task #568) never cross-filed into this index.**

    - **Status:** OPEN, residual — not a proven bug, no currently-reachable
      trigger, filed for tracking per this index's own convention.
    - **Current-number-or-verdict:** the guard's `Drop` unconditionally
      rolls `INIT_STATE` back to `UNINIT` on an armed unwind, regardless of
      whether the unwind happened before or after the in-place `write(hc)`.
      A post-write unwind lets the next CAS winner `write` a fresh
      `HeapCore` on top of the old one WITHOUT running the old value's
      `Drop` (`AllocCore::Drop`, `src/alloc_core/alloc_core/mod.rs`, releases
      the heap's segment reservations) — so skipping it leaks them. The
      guard therefore guarantees "no permanent `INITIALIZING` livelock", NOT
      "`Drop` always runs for an already-written `HeapCore`".
    - **Why not currently exploitable:** as of this writing, the only unwind
      source in the guarded region between `write(hc)` and the `READY`
      publish is the `internals`-gated test-injection panic, deliberately
      placed BEFORE `HeapCore::new`; `bind_thread_free`
      (`src/registry/heap_core_ownership.rs`) is a plain field assignment
      and cannot panic. So the post-write window is not currently reachable
      by any known panic source in the initialization path — but it is NOT
      structurally closed: a future change adding fallible code between
      `write(hc)` and the `READY` store would silently reopen it.
    - **What would close it structurally:** making the guard aware of
      whether `HeapCore` was written, so an armed unwind after that point
      drops the stale value or poisons the slot instead of just rolling
      back to `UNINIT`.
    - **Next trigger:** reopen and implement the write-aware guard if a
      future change adds fallible/panicking code between `write(hc)` and
      the `READY` store in the guarded region (currently none exists).
    - **Evidence:** `src/global/fallback.rs`'s `InitStateGuard` doc comment
      (the "What this guard does NOT guarantee (Sol-F6, task #568)" section,
      ~lines 375-399); `docs/reviews/2026-08-05-sol-release-readonly-review.md`
      finding F6; `docs/reviews/2026-08-05-sol-remediation-readonly-review.md`
      finding H5.

66. **`Reservation` carried no committed-length state, so a lazy handle's committed prefix was a DOCUMENTED contract rather than a CHECKABLE one (R6-1 variant 3 / R7-2).** — **CLOSED** by the new `LazyReservation` type (task #1051; its `as_reservation()` accessor re-opened the hole from safe code and was deleted by task #1104/H1), see "Recently resolved" §#66 below — including why all five options this card previously listed were set aside for a sixth.

155. **[T] `numa-shim`'s Linux topology initializer needs far more stack than its doc states: the first `current_node()` on a 64 KiB thread aborts with a stack overflow in an unoptimized build.** (Filed 2026-09-28, found while fixing CI for `tests/r1_04_alloc_core_drop_stack_pressure.rs`.)

    - **Status:** OPEN — documented-but-wrong residual. The test works around it by warming the topology on the full-size thread first; no code change in `numa-shim`.
    - **Current-number-or-verdict:** `crates/numa-shim/src/lib.rs`'s `topology()` doc (task #1340) budgets the `OnceLock::get_or_init` initializer at "~12 KiB best case, ~20 KiB worst" (an 8 KiB `ReverseIndex` plus a 4 KiB cpumap buffer). Observed on Linux (x86_64 WSL Ubuntu 24.04 and GitHub `ubuntu-latest`, debug build, `--all-features`, so `numa-aware` is on): `AllocCore::new()` on a thread built with `stack_size(64 * 1024)` dies with `thread '<unknown>' has overflowed its stack` (SIGABRT). The gdb backtrace ends in `OnceLock<numa_shim::cpumap::ReverseIndex>::initialize` → `numa_shim::platform::topology` → `current_node_impl` → `sefer_alloc::alloc_core::platform::numa::current_node` → `AllocCore::new_inner`. The unoptimized `OnceLock`/`Once::call_once_force` closure layers each move the 8 KiB value, so the real peak is well above the documented ~20 KiB. Not observed on Windows, whose topology path differs.
    - **Next trigger:** a user running `numa-aware` allocation on a small-stack thread, or the next `numa-shim` edit. Either measure the real peak (debug and release) and correct the doc numbers, or build the index in place in static storage so the initializer's stack use is small and fixed. Regression check: drop the warm-up in `tests/r1_04_alloc_core_drop_stack_pressure.rs` and run it under `--all-features` on Linux.
    - **Evidence:** CI run `36409924918`, job `test (gated bodies + all-features)`; local WSL reproduction plus gdb backtrace on 2026-09-28.

164. **[T] P1-box — установленный `Box` paused-witness красный под Miri SB/TB: интрузивная связь free-list пишется в тело свободного Small-блока, пока кадр освободившего by-value `Box` ещё жив.** (Filed 2026-10-05; бывший P1 из item 162.)

    - **Status:** OPEN — ACCEPTED KNOWN DEFECT (решение владельца, `docs/design/2026-10-05-adr-addendum-ph3c-path-b.md`; не MODEL-LIMIT). Off-body учёт, закрывавший P1 на спайке B3, провалил пред-регистрированные perf-пределы (`docs/perf/PH3C_B3P_STEP1PRIME_IAI.md`), поэтому production остаётся интрузивным slab'ом.
    - **Current-number-or-verdict:** `miri_global_box_acceptance -- paused` красный на `production` и `alloc-global` в SB (strict-provenance) и TB; первый кадр `Node::write_next` (`src/alloc_core/platform/node.rs`), вызывающий `reclaim_sidecar_record`/`flush_run`/`dealloc_small`. Нативных крашей и miscompilation не известно; aliasing-модели экспериментальны. CI-шаги «EXPECTED RED - P1» (4 шт.) пинят текст UB и место (`fd79861c`); зелёный witness или красный в другом месте роняет шаг. Формулировка «Miri-clean» без оговорки об этом дефекте запрещена.
    - **Next trigger:** (1) нормативное решение Rust о protector'ах `Box`; (2) нативное воспроизведение (крах/miscompilation); (3) новое решение владельца с новой ценой (напр. иная цена refill/flush). Прежде чем переоткрывать: спайк B3 сохранён (`archive/ph3c-b3p-fd3cdb9e`, `docs/perf/PH3C_B3P_SRC.patch`).
    - **Evidence:** `docs/perf/PH3C_B3P_STEP1PRIME_IAI.md`, `docs/perf/PH3C_B3S_SPIKE_IAI.md`, `docs/perf/_raw_ph3c_b3s_miri_*.log`, `docs/design/2026-10-02-adr-addendum-ph3c-escalation.md` §5, `docs/design/2026-10-05-adr-addendum-ph3c-path-b.md`; README «Honest limitations» и rustdoc `SeferAlloc`.

165. **[T → CLOSED 2026-10-05] Легаси registry-поверхность: `pub unsafe fn recycle` и `MaintenanceLease::with_core` оставались публичным unsafe-API за `internals` — долг из ADR addendum §3 с триггером Ph4c.** (Filed and CLOSED 2026-10-05, task #2107 / Ph4c.)

    - **Status:** CLOSED (Ph4c, task #2107) — сужение выполнено в той же фазе, что и его триггер; карточка заведена и закрыта одним проходом для фиксации closure trail. Остаточный долг задокументирован здесь же (ниже), не как отдельный открытый пункт.
    - **Что было:** `HeapRegistry::{claim, claim_with_config, recycle}` и `MaintenanceLease::with_core` были `pub` (recycle/with_core — `pub unsafe fn`), скрытые от документации feature-гейтом `internals`; ADR addendum §3 (`docs/design/2026-10-05-adr-addendum-ph3c-path-b.md`) фиксирует их как долг с триггером Ph4c.
    - **Что сделано (Ph4c, task #2107):** легаси `claim`/`claim_with_config`/`recycle`/`try_maintenance` и `with_core` сужены до `pub(crate)`; `with_core` стал безопасным (`pub(crate) fn`, был `pub unsafe fn`); `recycle` — единственный оставшийся `pub(crate) unsafe fn` в `src/registry/heap_registry` (с `#[allow(dead_code)]`: вызывается только unit-тестами legacy-семантики в `claim.rs`). Безопасная тестовая поверхность под `internals`: `HeapLease::core` (doc-hidden pub), `dbg_claim_lease[_with_config]`, `dbg_try_maintenance`, `MaintenanceLease::dbg_with_core`; ~106 файлов tests/benches/examples мигрированы. Evidence: `docs/reviews/2026-10-05-ph4c-narrow-legacy-registry-receipt.md`.
    - **Остаточный (НЕ закрытый этим) долг:** `recycle` остаётся `unsafe fn` до полного удаления легаси-поверхности — это остаточное состояние, задокументированное в README («Where unsafe lives», строка `src/registry/heap_registry/claim.rs`), а не открытый пункт этого индекса. Никаких известных misuse-путей из безопасного кода нет: снаружи крейта функция недоступна.
    - **EXECUTED by task #2119 (2026-10-05):** остаточный долг выше закрыт — legacy-поверхность удалена целиком (`HeapRegistry::claim`, `claim_with_config`, `unsafe fn recycle`, `HeapLease::into_raw`, `lease_into_raw`, inline `mod legacy_semantics`); `unsafe fn` в `src/registry/heap_registry` больше нет. Внешних вызовов не было (grep-подтверждение); unit-тесты legacy-семантики (null/double `recycle` = no-op) удалены вместе с семантикой: `HeapLease` освобождает слот ровно один раз (Drop по значению), null/двойной recycle невыразимы. README «Where unsafe lives» (строка `claim.rs`) обновлён соответственно.
    - **Evidence:** `docs/reviews/2026-10-05-ph4c-narrow-legacy-registry-receipt.md` (Ph4c receipt), task #2107; README «Where unsafe lives (the complete list)», строка `src/registry/heap_registry/claim.rs`.

166. **[T] R12-01 остаток — внешнее освобождение внутреннего указателя в ВЫДАННЫЙ блок однородного листа классов всё ещё доводит владельца до `abort()`.** (Filed 2026-10-06, fxx round 12, R12-01; частичный фикс `0228d150`.)

    - **Status:** OPEN — известный остаток hardening-класса, не маскируется. Входной указатель нарушает контракт `GlobalAlloc::dealloc` (не получен от `alloc`), поэтому это не дефект корректности для валидного использования; цена — `abort()` владельца вместо отброшенного освобождения.
    - **Current-number-or-verdict:** `SidecarBitmap::publish` (`src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs`) теперь отбрасывает гранулу, чей класс в листе нулевой (метаданные сегмента, некарвленный хвост) ДО RMW; `tests/r12_01_foreign_free_of_unissued_granule.rs` (подпроцессный, 2 случая) красный без этой проверки. Но лист «uniform» возвращает ненулевой класс для КАЖДОЙ гранулы листа, поэтому внутренний указатель `live + 32` внутри выданного блока публикуется и отвергается только владельцем (геометрическая проверка в `BitmapCut::pop`/reclaim) через `abort()`. Предложенное ревью «закрывает всё» исправление неполно — это найдено контрфактуальным тестом.
    - **Next trigger:** (1) производитель получает способ проверить начало блока БЕЗ чтения заголовка сегмента (сегмент малформной записи может быть освобождён параллельно — читать геометрию из заголовка производителю нельзя); или (2) решение владельца заменить `abort()` владельца на счёт+отбрасывание для записей, отвергнутых геометрией (потеряем сигнал порчи). Тогда добавить третий случай `live + 32` в подпроцессный тест.
    - **Evidence:** `docs/reviews/2026-10-06-063308-src-review-fxx-round-12.md` R12-01; модуль-doc `tests/r12_01_foreign_free_of_unissued_granule.rs` («Residual»); doc `SidecarBitmap::publish`; commits `0228d150`, `7d3b3e84`.
