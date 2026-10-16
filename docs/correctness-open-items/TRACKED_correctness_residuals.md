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

**Card count:** 7 numbered records in this file (re-counted 2026-10-08, src-review round 16). Composition: 4 open [T] cards (items 23, 155, 164, 166) + 3 closed/resolved records retained for lookup (items 16, 66, and 165); closure evidence is routed through `RESOLVED.md`/`ARCHIVE.md`, with item 165's dated history retained below. Item 22 (the retired `RemoteFreeRing::DrainHeadPublish` residual) was CLOSED/SUPERSEDED in R16 and moved to `RESOLVED.md`.

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
    - **Re-verified 2026-10-13 (fix-known-reds worktree; static wiring and historical runtime receipt):** `scripts/miri-matrix.mjs` carries all four `paused` rows as `expect: EXPECTED-RED, card: 164` with exact SB/TB red-message and `Node::write_next` site pins; the node-only wiring verifier reports OK. CI run `38347654258` has head SHA `c197c70c0971228aad5cce3166eb14c6b2708652` (`c197c70`), NOT `d8e73f5`; the run and `miri (core)` job `115132665836` both concluded success. Personally retrieved job logs contain four actual UB outputs (not merely echoed grep commands): 2026-10-13 **06:06:54 UTC** (SB, weakly protected), **06:07:07 UTC** (TB, write forbidden), **06:07:16 UTC** (SB, weakly protected), and **06:07:24 UTC** (TB, write forbidden), each with frame 0 `Node::write_next`. This confirms the accepted red witness under the wrappers on that `main` SHA; it is NOT a runtime receipt for current `d8e73f55` or this modified worktree. No local Miri/runtime reproduction or moved/muted-UB counterfactual was run here. The witness remains REQUIRED (accepted defect, owner decision), and card 164 remains OPEN.
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

    - **Round 6 (2026-10-15):** R6-CR-2/R6-SEC-2 refine the mechanism: the spill copies `old` into all 256 granules of a leaf; a uniform leaf stamps the whole leaf with the first block's class; the class map is never reset on segment reset; and a cross-thread free of an interior pointer passes all owner checks and silently corrupts the free list (`write_next` into a live block, `dec_live` undercount). Under `hardened` the own-thread interior case is rejected but the cross-thread one is not. Tracked as task R6-T28; Evidence: `docs/agent_reviews_round6/correctness_review.md` R6-CR-2, `docs/agent_reviews_round6/api_security_review.md` R6-SEC-2.

201. **[T] Production does not validate Small free-list links (`next`); only `hardened` does.**

    - **Status:** OPEN — deliberately not executed; recorded so the choice stays visible.
    - **Current-number-or-verdict:** under `hardened` a bad head or continuation, a self-link, a cycle back to the head or a Floyd-detected cycle aborts the process with a one-line stderr diagnostic (`aa94ca1e`). In `production` a corrupted `next` is trusted: the next pop or drain follows it. Estimated cost of validating every free-list block in production: +3…6 Ir per block taken from a free list (estimate from code reading, never measured). Detected corruption is classified as fatal by the failure policy (`src/global/sefer_alloc/mod.rs`, "Failure policy"); not detecting it is the open part.
    - **Next trigger:** an owner request for production-level corruption detection, or an exploit-class report that needs it. Then: pre-registered A/B on `HeapCore` alloc paths with a path-activation oracle (free-list pops versus virgin carves), per the perf-gate rules in `CLAUDE.md`.
    - **Evidence:** `docs/reviews/2026-10-11-owner-questions-decisions-ox.md` question 10; `docs/reviews/2026-10-11-solution-research-hardened-policy-sec03.md`; `tests/regression_freelist_next_validation.rs`.

202. **[T] Abort sites older than the failure policy carry no diagnostic line and no counter.**

    - **Status:** OPEN — deferred; the policy applies to new sites only.
    - **Current-number-or-verdict:** the failure policy (bad input before mutation: counted no-op or null; corrupted allocator-owned state: abort; repeatable OOM: null) is documented in `src/global/sefer_alloc/mod.rs`. Only the `hardened` free-list abort writes a diagnostic first (`src/alloc_core/platform/fatal.rs`); the other `std::process::abort()` sites (for example in `sidecar_drain.rs`, `alloc_core_large.rs`, `lifecycle.rs`, `mem_impl.rs`) abort silently, which on Windows is indistinguishable from any other fast-fail exit (`0xC0000409`). Not enumerated exhaustively here; re-derive with `grep -rn "process::abort" src/`.
    - **Next trigger:** the next change that touches one of those sites, or an owner request. Route each through the shared `fatal` helper with a static one-line message; no counter is needed before an abort.
    - **Evidence:** `docs/reviews/2026-10-11-owner-questions-decisions-ox.md` question 10; `src/alloc_core/platform/fatal.rs`.

211. **[T] Unchecked rounding of `usable` after `checked_add`; promotion calls `alloc_large` before `Layout` validation; debug overflow panic possible out of `GlobalAlloc::realloc`.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T30 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** `alloc_core_large.rs:188-230` rounds `usable` with `div_ceil * SEGMENT` and `align_up` without an overflow check right after a `checked_add`, so a debug overflow can panic out of `GlobalAlloc::realloc`; `HeapCore::realloc` performs promotion (`realloc.rs:297-317`) before `Layout::from_size_align` validation at `:343`.
    > - **Next trigger:** R6-T30 in the round-6 queue; close via that task's unit test (`alloc_large(usize::MAX - 4096, 8)` in debug → null, no panic).
    > - **Evidence:** `docs/agent_reviews_round6/correctness_review.md` R6-CR-4; task R6-T30.

212. **[T] Small-segment pool decay fires only inside `reserve_small_segment_impl`: an idle-but-busy heap never drains, contrary to the doc's "drain to zero when truly idle".** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T31 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** `maybe_decay_small_pool` (`alloc_core_small_pool_impl.rs:422-484`) has exactly one call site — `src/alloc_core/small/alloc_core_small/reserve.rs:126` — so a heap that keeps allocating from magazines but never reserves never decays its pooled segments, while the doc (`alloc_core_impl.rs:572-580`) promises drain-to-zero when truly idle.
    > - **Next trigger:** R6-T31 in the round-6 queue; close via that task's "burst → idle → tick → pool empty" test (or a corrected doc), plus the RSS gate if a decay tick is added.
    > - **Evidence:** `docs/agent_reviews_round6/correctness_review.md` R6-CR-5; task R6-T31.

214. **[T] Biased Large without `numa-aware` does not retry the reservation after draining the pool.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T34 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** in the biased Large path, `medium_promotion_reachable`-gated pool draining runs only where NUMA-aware retry follows; in a non-NUMA build a transient reservation failure returns null even though draining the pool would have freed the space.
    > - **Next trigger:** R6-T34 in the round-6 queue; close via that task's injection test (drain-then-retry succeeds where the current path returns null).
    > - **Evidence:** `docs/agent_reviews_round6/correctness_review.md` R6-CR-9; task R6-T34.

215. **[T] Inconsistent semantics of `foreign_or_unroutable_frees`.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T55 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** the counter's meaning differs between sites: some increments count genuinely unroutable frees and others also count routable-but-refused or batch-path ones, so the statistic cannot be read as a single well-defined predicate (per the report's site-by-site comparison).
    > - **Next trigger:** R6-T55 in the round-6 queue; close via that task's definition unification and per-site counterfactual tests.
    > - **Evidence:** `docs/agent_reviews_round6/correctness_review.md` R6-CR-10; task R6-T55.

216. **[T] `canonical_block_of`/`Node::deref` do `ptr::add` with an offset derived from a foreign address before any `off < span` check → formal UB before reject under `exact-span-large`/biased Large.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T22 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** `mem_impl.rs:36-44` computes `Node::deref(base, ptr.addr() − base.addr())` and `node.rs:141-153` does `add` without checking `offset < len`; under `exact-span-large` or biased Large the key window is wider than the reservation, so a foreign pointer from the same window is `add`-ed out of the object before the reject at `mem_impl.rs:271`/`:666`.
    > - **Next trigger:** R6-T22 in the round-6 queue; close via that task's Miri test (foreign address in the window → clean reject, no UB diagnostic).
    > - **Evidence:** `docs/agent_reviews_round6/memory_safety_review.md` R6-MS-1; task R6-T22.

217. **[T] The foreign `realloc` branch checks capacity against the reservation, not the committed range.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — FIX IN TREE (round-6 wave 5, worktree r6-u28); closure pending a green CI run of the landing commit.
    > - **Current:** the foreign path of `HeapCore::realloc` validates `new_size` against the reservation length, so a size that fits the reservation but exceeds the committed range passes the check and the copy can read/write past the committed prefix.
    > - **Round 6 fix (2026-10-16, r6-u28, rev. 2):** the defence lives in the foreign leg of `HeapCore::realloc` (`free/realloc.rs`), right after the unchanged `RoutePin::contains_payload` capacity check: Large routes are bounded by the segment's CURRENT committed frontier read from the live header at the descriptor root (`span_usable_at(route.root())` via a minimal new `RoutePin::root()` accessor) — the same quantity `AllocCore::try_grow_large_reserved_capacity` advances, so an owner-grown block still reallocs legitimately; Small/Primordial addresses below `small_meta_end()`/`primordial_meta_end()` are rejected. Registration descriptors and `lookup` geometry are UNCHANGED — an earlier immutable registration-time bound was rejected in review: it false-OOMs foreign reallocs of owner-grown blocks and shifts route lookup for every consumer. The code comment justifies the header read: same caller-contract trust as the copy itself (live block ⇒ live segment ⇒ mapped header); `span_usable`'s only writers are init, the block's own OPT-G grow (happens-before the pointer handoff), and post-free large-cache reuse. Tests `tests/r6_t50_foreign_realloc_committed_span.rs` (5): forged 2 MiB layout on a ~1 MiB reserved-capacity Large → null (pre-fix: Windows access violation; eager-commit backends: garbage non-null); a Small segment's metadata address → rejected; owner grows Large 1→2 MiB IN PLACE, foreign realloc with the true grown layout → non-null with data preserved (red under any registration-time immutable bound — the rejected design's counterfactual); two positive controls.
    > - **CI:** runs in the Windows `large-reserved-capacity` cargo-test step (ci.yml:2332) and the check-matrix `test-r8-large-geometry-policies` row.
    > - **Next trigger:** first green CI run of the landing commit; then move to RESOLVED.md with the test name as evidence.
    > - **Evidence:** `docs/agent_reviews_round6/memory_safety_review.md` R6-MS-3; task R6-T50.

218. **[T] `unsafe impl Sync for HeapSlot` moves a `!Send` `HeapCore` across threads without a semantic-`Send` argument.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — FIX IN TREE (round-6 wave 5, worktree r6-u28); closure pending a green CI run of the landing commit.
    > - **Current:** the `Sync` impl makes `&HeapSlot` reachable from other threads and thereby transfers the `HeapCore` it contains across threads, but no SAFETY argument establishes that `HeapCore`'s state is actually safe to observe/mutate cross-thread (a semantic-`Send` claim), only that the cell is atomic.
    > - **Round 6 fix (2026-10-16, r6-u28, rev. 2):** `heap_slot.rs` gains a compile-time audit that DESTRUCTURES `&HeapCore` with no `..`, mirroring each field's `#[cfg]` gate: non-excluded fields get `assert_send`; `core: AllocCore` and `tcache: Tcache` are bound to `_` with their no-thread-affine-resource justification (NUMA cache is a plain claim-invalidated value, R11-5). Counterfactual: a NEWLY added `Rc<()>`/`Cell`-typed field fails the build with E0027 ("pattern does not mention field") — the author must add an assert or extend the exclusion list; retyping an existing asserted field to a non-Send type fails E0277. (A first revision that asserted a hand-written field LIST missed the addition case — caught in review.) `HeapCore`'s doc checklist describes the E0027 mechanism. The audit lives in `src/`, so no runtime test is required; closure evidence is any green build of the landing commit.
    > - **Next trigger:** first green CI run of the landing commit; then move to RESOLVED.md.
    > - **Evidence:** `docs/agent_reviews_round6/memory_safety_review.md` R6-MS-5; task R6-T52.

219. **[T] A claimant hitting a `MAINTENANCE` slot loses the candidate; bump precedes scan → permanent heap-count growth.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T24 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** `claim.rs:278-326` consumes a candidate and then `continue`s when the slot turns out `MAINTENANCE`; because `stack.rs:157-178` bumps before scanning, each such loss mints a brand-new 4 MiB heap permanently (`count` grows without bound under maintenance contention).
    > - **Next trigger:** R6-T24 in the round-6 queue; close via that task's deterministic `MaintenanceLease` test (claimant takes the freed slot; `count` does not grow).
    > - **Evidence:** `docs/agent_reviews_round6/concurrency_review.md` R6-CC-4; task R6-T24.

220. **[T] `Shard::remove` releases a `Block` via `System.dealloc` under the shard lock, bypassing the `Surplus` pattern.** (Filed 2026-10-15, round-6 src review.)

    > **Current state**
    > - **Status:** OPEN — tracked; queued as round-6 task R6-T25 (docs/agent_reviews_round6/SUMMARY.md).
    > - **Current:** `directory.rs:610-657` (`Shard::remove` → `recycle` → `Block::release`) performs the OS release while the shard lock is held; the file's own `Surplus` pattern (`:499-503`, `:876-878`) exists precisely to move such releases outside the lock.
    > - **Next trigger:** R6-T25 in the round-6 queue; close via the existing route tests after returning the displaced block as `Option<SpareBlock>` freed after `drop(guard)` (behaviour-neutral refactor).
    > - **Evidence:** `docs/agent_reviews_round6/concurrency_review.md` R6-CC-5; task R6-T25.

246. **[T] U-R5-5 residual in its new form: a stale/double free after reservation reissue frees the CURRENT owner's block (Small and Large); the deferred-large mechanism is gone but the indistinguishability remains.** (Filed 2026-10-15, round-6 src review — card created now because round-5's U-R5-5 residual had no card in this index; see note below.)

    > **Current state**
    > - **Status:** OPEN — tracked residual; round-6 task R6-T51 evaluated the hardened issue-generation tag and recorded it as a design decision (below) instead of implementing it (2026-10-16, r6-u28).
    > - **Current:** round-5's U-R5-5 residual (deferred-large post-reuse stale free, `docs/agent_reviews_round5/memory_safety_review.md`) was re-examined by round-6: the deferred-large mechanism no longer exists, but a stale free after the reservation is reissued is still indistinguishable from a valid one and now frees the CURRENT owner's block — for both Small and Large (R6-MS-4). Caller-contract UB, as before, but the residual survives in its new shape.
    > - **Round 6 (2026-10-15):** R6-MS-4 confirms the stale/double free after reissue frees the current owner (Small and Large); the deferred-large mechanism is gone but the residual lives in its new form.
    > - **Round 6 decision (2026-10-16, r6-u28):** NOT implemented this round; recorded as a design residual. Design sketch — a per-issued-block issue-generation tag the producer copies before publication and the owner re-checks at consumption. Small candidates: (a) steal the free high bit of each `ClassLeaves` class byte (`SMALL_CLASS_COUNT < 128`, so bit 7 is unused) — zero layout growth but a 1-bit epoch detects only ONE reissue (ABA on the second); (b) a dedicated per-granule generation byte/word — ~256 KiB per routed Small segment of extra System sidecar memory (~8× today's 32 KiB pending bitmap), or a per-sidecar 64-bit epoch counter whose per-block copy needs a carrier (the header `owner_state` generation sub-field is hardcoded 0; using it adds an alloc-hot-path header write — the write class R6-T01 just removed as dead). Large: `LargeState` already carries a generation word, but `publish_pending()` publishes for the CURRENT generation — the fix needs the issue-time generation carried to the producer's pin, the same carry problem. Cost: a sidecar format change shared by EVERY `alloc-global` build (a hardened-only tag means a cfg'd format split or the cost everywhere), new producer/consumer protocol states plus a loom model for the epoch compare, and hot-path writes the perf rounds just removed. Kept as caller-contract UB residual (documented in `mem_impl.rs`'s `# Safety` and `alloc_core_small_pool_impl.rs`).
    > - **Next trigger:** owner decision — schedule a hardened-format design round (candidate (a) 1-bit epoch is the cheapest start; its one-reissue ABA window must be documented explicitly) or close as accepted caller-contract UB residual.
    > - **Evidence:** `docs/agent_reviews_round6/memory_safety_review.md` R6-MS-4 (and the U-R5-5 row in its R5 closure table); task R6-T51.

247. **[T, filed 2026-10-16, R6-T06/R6-T05 round 6
    (`docs/agent_reviews_round6/api_security_review.md` R6-SEC-3, R6-SEC-1)]
    Free-list poisoning residuals left open after the R6-T06 production-floor
    validation (range + `MIN_BLOCK` alignment + metadata bound, corruption →
    `fatal`): no safe-linking XOR mask, and the class-map free check covers
    only the own-thread dealloc path.**

    - **Status:** OPEN, residual — the R6-T06 minimum landed as the effective
      no-flag default (`Node::continuation_offset_or_fatal` now rejects, with
      `fatal`, a continuation below the segment base, inside the metadata
      region, at/past `bump`, at/after `SEGMENT` (the former silent `as u32`
      truncation), or not `MIN_BLOCK`-aligned; class-stride divisibility stays
      hardened-only in `valid_free_node`, iai cost ~10 Ir per link on PR #57); the two
      sub-items below were judged outside the no-flag tolerance budget and
      are tracked, not actioned.
    - **Sub-item (a) — safe-linking:** free-list `next` words still store raw
      addresses, so (i) a UAF write can forge a continuation that passes
      every range/multiple check (any free-looking in-segment offset), and
      (ii) the first word of a freshly issued, non-zeroed block still leaks
      the old `next` heap address (ASLR aid). Design if actioned
      (glibc-style): store `next ^ per-segment secret` in `Node::write_next`,
      unmask in `Node::continuation_offset_or_fatal` before the existing
      checks; the secret is a per-segment word fixed at route registration.
      Cost: one XOR on the free-push and pop continuations plus secret
      materialization; requires an iai A/B gate on `small_churn_16b{,_2n}`
      and `cold_alloc_only_256x16b` within the standing ±0.5% tolerance
      (`docs/perf/R6_T01_OWNER_STAMP_REMOVAL_GATE.md`) before any promotion.
    - **Sub-item (b) — class-map check coverage:** in `hardened`, the R6-T05 owner class-map
      free check guards the fastbin own-thread dealloc body
      (`HeapCore::dealloc_own_thread_with_base`, which also serves the
      fastbin realloc free leg and own-route xthread routing); NOT covered:
      `dealloc_batch_small` (batch-api) and the non-fastbin substrate free,
      which still trust the caller's `Layout`, and stale class bytes after
      an in-route segment reset remain (R6-SEC-2's own residual, see
      `docs/agent_reviews_round6/SUMMARY.md` R6-T28). By the owner decision
      (round2 PR57), default own-thread validation checks only stride
      divisibility; aligned wrong-class pointers can pass, while hardened keeps
      exact owner-class identity at the cost noted in the owner decision after
      the PR #57 iai A/B (~33 Ir/free).
    - **Next trigger:** an owner decision to spend the iai budget on (a); a
      batch-api promotion round for (b).
    - **Evidence:** fix tests `tests/r6_t06_freelist_continuation_bounds.rs`
      and `tests/r6_t33_t05_own_free_kind_and_class.rs` (landed with the
      R6-T05/T06/T33 fixes).
