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
    - **Sub-path fixed 2026-10-19 (R1-01 cap branch):** независимое read-only ревью нашло валидный (без двойного free/UAF) триггер этого дефекта вне paused-сценария: при `cnt == cap < TCACHE_CAP` (fastbin magazine, классы > 4 KiB) own free сразу сбрасывал ТЕКУЩИЙ блок в substrate, и `flush_run` писал `next` в его тело внутри живого кадра `Box::drop` (`src/registry/heap_core/free/dealloc_own_base.rs`, R1-01-ветка). Исправлено swap-top: в substrate уходит верхний блок magazine (его кадр завершён), текущий блок занимает слот; `count` и байт-бюджет R1-01 не меняются. Свидетель `tests/miri_r1_01_cap_frame.rs` ( path-activation через `dbg_r1_01_cap_route_frees`) зелёный в SB и TB под `alloc-global fastbin internals bench-internals`; на прежней ветке он красный (`Node::write_next` из R1-01-ветки). CI-шаги «installed Box R1-01 cap-frame flush witness (SB / under TreeBorrows)». Без `fastbin` R1-01-ветки нет, и тот же сценарий красный на уже учтённом здесь `dealloc_small`-пути — это часть этой же открытой карточки, не новое.
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

215. **[T → CLOSED 2026-10-18] Inconsistent semantics of `foreign_or_unroutable_frees`.** (Filed 2026-10-15, round-6 src review; CLOSED 2026-10-18, round-6 task R6-T55, wave-14 executor r6-u30.)

    > **Current state**
    > - **Status:** CLOSED (R6-T55) — the public counter is back to a single well-defined predicate: foreign route drops only (address-only lookup miss or rejected terminal publication), exactly as `AllocStats::foreign_or_unroutable_frees`'s doc reads. Local no-op rejections (interior-address Large frees, `small_free_guard` `RejectNoOp`s) moved to the new `internals`/`bench-internals`-gated `LOCAL_REJECT_FREES` counter (read via `AllocCore::dbg_local_reject_frees`); `reject_large_interior_free` is a pure predicate again. A plain `production` build counts local rejects nowhere (measurement-only gating).
    > - **Counterfactual tests:** `tests/r6_t55_local_reject_frees.rs` (red before the fix: the local interior Large free bumped the public counter) and `tests/r19_large_interior_free.rs` (rewritten to pin public-flat + `LOCAL_REJECT_FREES` +1 per local reject). Both run only on CI — local checks were `cargo check`/`clippy`/`rustfmt` only.
    > - **Evidence:** `docs/agent_reviews_round6/correctness_review.md` R6-CR-10; `docs/agent_reviews_round6/SUMMARY.md` R6-T55.

246. **[T] U-R5-5 residual in its new form: a stale/double free after reservation reissue frees the CURRENT owner's block (Small and Large); the deferred-large mechanism is gone but the indistinguishability remains.** (Filed 2026-10-15, round-6 src review — card created now because round-5's U-R5-5 residual had no card in this index; see note below.)

    > **Current state**
    > - **Status:** OPEN — tracked residual; round-6 task R6-T51 evaluated the hardened issue-generation tag and recorded it as a design decision (below) instead of implementing it (2026-10-16, r6-u28).
    > - **Current:** round-5's U-R5-5 residual (deferred-large post-reuse stale free, `docs/agent_reviews_round5/memory_safety_review.md`) was re-examined by round-6: the deferred-large mechanism no longer exists, but a stale free after the reservation is reissued is still indistinguishable from a valid one and now frees the CURRENT owner's block — for both Small and Large (R6-MS-4). Caller-contract UB, as before, but the residual survives in its new shape.
    > - **Round 6 (2026-10-15):** R6-MS-4 confirms the stale/double free after reissue frees the current owner (Small and Large); the deferred-large mechanism is gone but the residual lives in its new form.
    > - **Clarification (2026-10-19, independent read-only review):** duplicate/reissue до reclaim уже явно оговорён (`src/registry/heap_core/free/dealloc_own_base.rs:180–186`); уточнение последствия — при free→own-free→переиздании→reclaim блок линкуется в бин и снимается credit, так что следующая выдача может ПЕРЕСЕЧЬСЯ с живым экземпляром: живая память возвращается в бин. Классификация не меняется: Invalid-use (двойной free), не дефект валидной программы.
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
    - **Clarification on (b) (2026-10-19, independent read-only review):**
      batch-api additionally lacks the production `divides` check itself
      (`src/registry/heap_core/free/dealloc_batch.rs:311–312`), which scalar
      performs in `dealloc_own_base.rs:417–427` — so in production a
      16-aligned interior pointer with wrong stride class can enter the
      magazine even without hardened, while scalar would reject it by the
      class map. Card item 247(b) explicitly fixes SEC-1; the missing batch
      `divides` is recorded here.
    - **Next trigger:** an owner decision to spend the iai budget on (a); a
      batch-api promotion round for (b).
    - **Evidence:** fix tests `tests/r6_t06_freelist_continuation_bounds.rs`
      and `tests/r6_t33_t05_own_free_kind_and_class.rs` (landed with the
      R6-T05/T06/T33 fixes).

---
248. **[T, filed 2026-10-20, b4-sbyte] Cross-heap free of a magazine-resident block — reclaim MAG rejection is checked through safe diagnostic observers; ABA after reissue remains the separate item-246 residual.**

    - **Status:** OPEN, residual — not a proven bug. The cross-heap
      (foreign) free of an owner-magazine-resident block is handled by the
      owner-side reclaim rejection, tested with the existing synthetic-record
      seam and observed through safe `dbg_*` hooks. This does not establish
      general stale-publication safety after block reissue; that remains item
      246's ABA residual.
    - **Current-number-or-verdict:** the synthetic MAG record is rejected
      without changing state byte, magazine residency, bin/live accounting;
      b4-sbyte adds probes in `tests/state_byte_heap_magazine.rs` for scalar,
      batch, and refill-retained paths. Safe `dbg_*` observers are present.
    - **Next trigger:** reopen if the MAG-reject ordering or sidecar reclaim
      semantics change; preserve the safe diagnostic and regression coverage.
    - **Related residual:** item 246(a) remains OPEN: the separate owner state
      byte uses its own bit 7 for FREE, while the remote `ClassLeaves` entries
      remain class+1 and retain bit 7 for the existing ABA/epoch candidate.
      The ABA residual remains: MAG is rejected before a hit, but a stale
      publication can be indistinguishable from LIVE after reissue. This is
      not an unobservability claim; safe `dbg_*` observers exist. No double-
      issue guarantee is inferred beyond these checks.
    - **Evidence:** branch `b4-sbyte`, tests above; mutant runs recorded in
      the b4-sbyte chunk-3 report.

---
262. **[T, filed 2026-11-06, J13b #47] THP tuple conflicts are detected only at heap materialisation; a re-claimed or fallback heap with a differing THP config is silently ignored.** (Declared limitation of `docs/perf/J13B_BOUNDED_THP_PREREG.md`, "2026-11-06 #47 implementation amendment" item 6.)

    - **Status:** OPEN, declared limitation — not a defect of the shipped default (no THP knob set means nothing is published and nothing is counted).
    - **Current-number-or-verdict:** the process-wide tuple (`huge_page_mode`, `huge_page_block_min_bytes`, `huge_page_budget_bytes`, `huge_page_gradient`) is published first-wins-whole, and `AllocStats::thp_tuple_conflicts` is bumped, only from `new_with_config_and_owner`. A recycled heap slot re-claimed through `claim_lease_with_config` and the fallback heap (`with_heap_config`) do not compare the THP knobs, so a differing config there is ignored without a count. A judge that requires a zero conflict delta (R26-4) therefore cannot detect that case from this counter.
    - **Next trigger:** the J13b judge adapter work, or any change to claim/fallback config comparison (note: `claim.rs` carries ghost catalog rules, so touching it needs Invariant-Check trailers).
    - **Evidence:** `src/alloc_core/alloc_core/lifecycle.rs` (publication site), `tests/j13b_thp_publish_first_wins.rs`, `tests/j13b_thp_publish_same_tuple.rs`. No test was executed locally.

263. **[T, filed 2026-11-06, J13b #47] While a THP tuple is being published (`PUBLISHING`), a concurrent Large allocation sees the default tuple and a concurrent loser over-counts one conflict.** (Declared limitation, prereg amendment item 6.)

    - **Status:** OPEN, declared limitation, conservative direction.
    - **Current-number-or-verdict:** the write-once cell never makes a reader or loser spin. A Large allocation on another thread during the few stores of the publication reads the default tuple (`Off`, so no advice for that one allocation); a loser that meets `PUBLISHING` counts one conflict, so a same-tuple race inflates `thp_tuple_conflicts` by one. A winner that dies inside `PUBLISHING` leaves every thread on the default forever (never a hang).
    - **Next trigger:** the CI execution of the now-written actual-source loom models of the cell and the ledger (`tests/loom_thp_tuple_cell.rs`, `tests/loom_thp_ledger.rs`; wired into `scripts/loom.mjs` and the ci.yml loom row, never run locally), or a judge run showing a nonzero conflict delta from a same-tuple race.
    - **Evidence:** `src/alloc_core/config/policy/large_thp_runtime/thp_tuple_cell.rs`; `tests/j13b_thp_publish_concurrent.rs`, `tests/j13b_thp_publish_same_tuple.rs`. No test was executed locally.

264. **[T, filed 2026-11-06, J13b #47] `SegmentHeader` grew from 128 to 136 bytes to carry `thp_charge`; the instruction-count (iai Ir) effect is unknown.** (Prereg amendment item 8.)

    - **Status:** OPEN — UNKNOWN until CI iai output exists.
    - **Current-number-or-verdict:** no 4-byte hole existed, so even a narrower field would have grown the struct. `read_at`/`write_struct` on cold paths copy one more word, and the fresh Large path adds one Acquire load. The terminal words moved to offsets 136/136/144; `tests/r6_terminal_header_atomic.rs` pins were updated. No exact-size equality assert was added (`tests/no_stale_doc_references.rs` forbids one); only `<= PAGE` and offset relations hold.
    - **Current state (2026-11-08, #48):** the observation token grew `SegmentHeader` 136 → 144 bytes (total header now 144; terminal `LARGE_STATE_OFF` moved 144 → 152; `tests/r6_terminal_header_atomic.rs` pins 144/144/144/152); Ir still UNKNOWN until CI iai.
    - **Next trigger:** CI iai comparison on Large-touching benches; if a nonzero delta matters, store the charge in an otherwise inert header field instead.
    - **Evidence:** `src/alloc_core/segment/segment_header/segment_header_impl.rs`, `layout_asserts.rs`, `tests/r6_terminal_header_atomic.rs`.

266. **[T, filed 2026-11-06, J13b #47] A registration failure on the Large cache-reuse path is not injectable, so the charge-carrying release there has no test.** (Declared limitation.)

    - **Status:** OPEN, declared test gap.
    - **Current-number-or-verdict:** `dbg_set_inject_large_register_fail` (`internals`) forces `register_payload` failure only on the FRESH reservation path, where the charge is a local; that path is covered by `tests/j13b_thp_registration_failure.rs`. The reuse path's failure release reads the charge from the header through the same `release_large_mapping` funnel, but no hook forces it (it needs a full segment table).
    - **Next trigger:** a reuse-path injection flag, or a segment-table-full test.
    - **Evidence:** `src/alloc_core/large/alloc_core_large.rs`, `tests/j13b_thp_registration_failure.rs`, `tests/j13b_thp_header_charge_carry.rs`.

272. **[T, filed 2026-11-08, J13b #48] Remote owner-reclaim controls are consumed as UNKNOWN/invalidated, control windows are group-seeded, a sliding 8-window may hold 2 controls, and eligibility is narrow.** (Declared limitation / conservative direction, prereg 2026-11-08 amendment.)

    - **Status:** OPEN, declared limitation / conservative direction.
    - **Current-number-or-verdict:** (a) remote owner-reclaim controls are consumed as UNKNOWN/invalidated because a foreign moving realloc may copy the payload without owner-header invalidation — a future owner-terminal protocol would be needed to sample validly. (b) Control windows are seeded per request group, not per request, so same-group same-geometry controls reuse window positions (correlation, bounded). (c) A sliding 8-window can legally contain 2 controls across a cohort boundary (accepted amendment semantics). (d) Eligibility: Linux/Android, `alloc-decommit`, non-`numa-aware`, fresh Large path, `align < SEGMENT` only. (e) Realloc invalidations are recorded separately (telemetry field 4, token-derived group, exactly-once; remote-reclaim consumption stays field 3), so downstream #49 can quantify invalidation/lifetime bias. (f) The fresh-premise ordering proof — fresh OS reservation → `MADV_NOHUGEPAGE` BEFORE header write/terminal init/payload — and its never-touched oracle (`tests/j13b_control_premise_fresh.rs`: a never-touched mapping must read `resident == 0`) establish that `MADV_NOHUGEPAGE` policy on a fresh mapping cannot be applied over pre-existing huge backing. (g) Bounded alignment-slack owner geometry is accepted: a reservation root strictly below the SEGMENT-aligned usable base is valid ordinary-backend geometry, per the production-bound pure validator with oracle `tests/j13b_control_owner_geometry.rs` (native slack presence is address-luck). (h) Unknown control tokens fail closed as an observation error attributed to the recorded request size — never a token-derived group, never OOB. (i) Reserved-capacity growth captures the control token BEFORE the helper whose span setter clears it; success accounts exactly once and failure preserves the token so the moving leg accounts exactly once (dedicated row `test-j13b-control-reserved-capacity-invalidation`). (j) Remote consumption is asserted only after the explicit guaranteed owner drain (`dbg_trim_current_thread`) with an exactly-once second-trim assertion; before the drain only foreign-publication guarantees hold (field 3 deliberately unconstrained). (k) A failed-premise token (9..=14) consumed by a moving realloc counts as a premise failure exactly once — never realloc-invalidation, never double-counted (`tests/j13b_control_premise_recovery.rs`). (l) Concurrent competing-ordinal coverage exists (`tests/j13b_control_concurrent_ordinal.rs`: 64 competing 16 MiB allocations on 4 barrier-synchronized threads via the installed GlobalAlloc; the oracle reads the runtime's actually consumed tickets and decisions through the bounded bench-internals ticket log — captured records must equal the consumed window with pairwise-distinct tickets, so a folded or duplicated ticket is caught even when counter deltas and aggregate selections stay unchanged; every captured decision must equal the pure `dbg_thp_control_decision_at` passthrough at that ticket; the per-cohort census over the actual captured decisions holds one control per complete aligned cohort, closed against telemetry selections == 8; aggregate telemetry alone proves neither property; a sliding 8-window may legally hold 2 and is never asserted). (m) VM no-THP activation evidence is layered: smaps `VmFlags: nh` REQUIRED on native Linux/Android; the mincore granularity oracle proves fault granularity only, not activation. (n) Unknown-token fail-closed accounting is source-verified only — a native malformed-token injection oracle would require a raw-header token setter, a forbidden safe raw-pointer metadata mutation API.
    - **Next trigger:** an owner-terminal invalidation protocol, or a judge acceptance decision.
    - **Evidence:** `src/alloc_core/alloc_core/mem/advise_fresh_large.rs`, `src/alloc_core/alloc_core/mem/large_control/observe_large_control.rs`, the reclaim block in `src/alloc_core/large/alloc_core_large.rs`, `tests/j13b_control_owner_geometry.rs`, `tests/j13b_control_premise_recovery.rs`, `tests/j13b_control_concurrent_ordinal.rs`, `tests/j13b_control_oom_shrink_invalidation.rs`, `tests/j13b_control_reserved_capacity_invalidation.rs`, `tests/j13b_control_remote_unknown.rs`, `crates/aligned-vmem/tests/disable_transparent_huge_pages.rs`, `docs/verification/ghost-rules.json` (thp-control/ordinal-probe-inventory), `tests/ghost_lint_fixtures/p26_ordinal_probe/`, `docs/perf/J13B_BOUNDED_THP_PREREG.md` (2026-11-08 amendment). No test was executed locally.
