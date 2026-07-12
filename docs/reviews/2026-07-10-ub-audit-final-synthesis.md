# UB / Memory-Safety Audit — Final Synthesis vs. Current Tree (2026-07-11)

This is the consolidating pass over the five-domain UB audit
(`2026-07-10-ub-audit-{alloc-core,registry,concurrent-os,segment-bitmap,global-xthread}.md`
→ `2026-07-10-ub-memory-audit-summary.md`, 20 findings) **re-verified line by
line against the CURRENT working tree** (2026-07-11), after the session's
refactors landed:

- **RAD-1** (`34260ea`): lazy `next_free` init in `src/registry/bootstrap.rs`.
  Confirmed sound; **no finding depends on the removed eager loop** (the
  registry audit already listed RAD-1 as sound and that holds).
- **RAD-2** (`bcff0f1`): the 4875-line `alloc_core.rs` monolith was split into
  `alloc_core.rs` / `alloc_core_small.rs` / `alloc_core_large.rs` /
  `alloc_core_large_cache.rs` / `alloc_core_small_pool.rs`. **The five domain
  reports were written against the split tree** (see global-xthread report's
  closing line "includes the uncommitted `alloc_core` file split"), so their
  `alloc_core_small.rs` line refs are current. Only the *verification* document
  (`2026-07-10-ub-memory-audit-verification.md`, findings F1–F6) cited the OLD
  monolith line numbers — those are stale and re-anchored below.
- **RAD-3**: landed. The working tree is clean (only `benches/global_alloc.rs`
  modified); `alloc_core_small_pool.rs` / `segment_header.rs` /
  `small_segment_pool_config.rs` are committed. Findings that touch those files
  (M-4's `dec_live_and_maybe_decommit`, L-5's `kind_at`) were re-checked
  against the post-RAD-3 code and still reproduce.

The git-status snapshot in the task prompt (`M alloc_core.rs`, `?? …_small.rs`)
is **stale**; those files are now committed and the tree is clean.

---

## Executive summary

**Every one of the 20 summary findings still reproduces on the current tree.**
Nothing in the five-domain audit was closed by RAD-1/RAD-2/RAD-3 or by an
earlier PASS-1..5. The refactor was purely mechanical (file split) and
line-preserving for the audited bodies; the defensive-guard gaps the audit
found were carried across verbatim.

| Bucket | Count | Findings |
|---|---|---|
| **Actionable now** (reproduces, worth a fix) | 15 | H-1, H-2, M-1, M-2, M-3, M-4, M-5, M-6, M-7, M-8, M-9, L-3, L-4, L-5, L-9 (7 sub-items) |
| **Actionable but doc/hardening-only** (record, low value) | 3 | L-6, L-7, L-8 |
| **Already tracked in #59 / #60** (do NOT duplicate) | — | F1→#59, F2→#59, F6→#60 (from the *verification* doc, disjoint from the 20) |
| **Documented residual — do NOT act** | 2 | L-1 (=F4, X7-pinned), L-2 |
| **Closed by session refactor / outdated** | 0 | — |

Cross-check with the earlier verification doc (F1–F6): **H-1 is NOT F1.**
F1 is the foreign-pointer read in `AllocCore::realloc`'s move-leg
(now `alloc_core.rs:1111`, was cited as 2160); H-1 is the missing payload
*lower-bound* guard in the small-**free** paths (`dealloc_small` / `reclaim_offset*`
/ `flush_run`). Different functions, different defect class, disjoint fixes —
both real, both open. L-1 IS F4 (the X7-pinned re-issue-before-drain residual)
and must stay untouched.

---

## Actionable findings — verified against current code

Confidence = confidence the described defect is a real problem. Risk = risk of
the *fix* in this session's idiom (H-1-adjacency / MPSC ring protocol ⇒ HIGH;
M2/D1 decommit-invariant paths ⇒ MEDIUM+; owner-only single-word / doc ⇒ LOW).

### H-1 — missing payload lower-bound guard (metadata-region free corrupts segment/registry)

- **Current locations (all verified):**
  - `src/alloc_core/alloc_core_small.rs:1941` `dealloc_small` — guards at
    1961 (`hardened` `% block_size`), 1976–1979 (decommit `off >= bump`),
    1982 (`is_free`); `write_next` at 1995. **No `off >= small_meta_end()`.**
  - `src/alloc_core/alloc_core_small.rs:184` `reclaim_offset` — guards 208–259;
    `write_next` 272. No lower bound.
  - `src/alloc_core/alloc_core_small.rs:70` `reclaim_offset_checked` — guards
    99–156; `write_next` 167. No lower bound.
  - `src/alloc_core/alloc_core_small.rs:810` `flush_run` — per-block guards
    871–880; `write_next` 903. No lower bound.
- **Fix API is already present:** `Layout::small_meta_end()` (const,
  `segment_header.rs:939`) and `Layout::primordial_meta_end()`
  (`segment_header.rs:1043`) are compile-time constants. The fix is one
  `if (off as usize) < payload_start { return; }` per site (primordial kind →
  `primordial_meta_end()`).
- **Verdict:** reproduces. **Confidence HIGH.** **Risk MEDIUM** — four sites,
  all in the M2/decommit-invariant free paths (`reclaim_offset*` is MPSC-drain
  adjacent), needs a counterfactual test (free of `base+0`, `base+PAGE`,
  `base+4096` → no-op, header/bitmap untouched) and a re-run of the M2
  differential + decommit invariants.

### M-1 — `off >= bump` guard is `alloc-decommit`-cfg-gated (non-decommit double-alloc)

- **Current locations:** the `#[cfg(feature = "alloc-decommit")]` gate wraps
  `off >= bump` at exactly the four H-1 sites:
  `alloc_core_small.rs:1976`, `:114`, `:244`, `:872`.
- **Verdict:** reproduces. **Confidence HIGH.** **Risk MEDIUM** — same four
  sites as H-1; dropping the cfg gate makes the guard unconditional. Ships
  naturally in the H-1 patch (identical zone, identical test surface).

### H-2 — `free_slots` ABA tag resets to 0 on empty transition (slot double-claim)

- **Current locations (verified):** `src/registry/heap_registry.rs:595`
  (`pop_free_slot` writes `TaggedPtr::empty()` on last-pop), `:636–638`
  (`push_free_slot` derives tag from current head); `src/registry/tagged_ptr.rs:142`
  (`empty()` = `pack(INDEX_MASK, 0)`, tag 0).
- **Verdict:** reproduces. **Confidence HIGH.** **Risk HIGH** — this is the only
  finding that breaks the registry's single-writer foundation under pure thread
  churn *with no caller error*. Lock-free Treiber-stack tag protocol; fix
  (preserve running tag in the empty sentinel: `pack(INDEX_MASK, observed_tag)`,
  `is_empty` already masks the tag) is localized but MUST land with a loom model
  that crosses the empty state with a parked popper.

### M-6 — same tag-reset in `abandoned_segs` (+ only 22 tag bits)

- **Current locations:** `src/registry/heap_registry.rs:376`
  (`pop_abandoned_segment` → `ABANDONED_HEAD_EMPTY`), push ~719–721;
  `src/registry/bootstrap.rs:265` (`ABANDONED_HEAD_EMPTY: u64 = 0`);
  `ABANDON_TAG_BITS = 22` (`bootstrap.rs:202`).
- **Reachability:** `abandon_segments`/`try_adopt` are **test-only** (Phase 12.5
  shard model retired thread-exit abandonment; the primitive is retained as
  substrate for a future decommit-when-empty policy).
- **Verdict:** reproduces (latent). **Confidence HIGH** (identical structure to
  H-2). **Risk HIGH** (same lock-free protocol). Fix shape is identical to H-2;
  ship in the same change. Not production-reachable today, so it is a
  "fix-before-reactivation" item, not a live corruption.

### M-2 — non-atomic full-struct `SegmentHeader` writes race remote field reads (Large paths)

- **Current locations (verified):**
  - `src/alloc_core/alloc_core.rs:826–828` (`dealloc` Large branch,
    `hdr_zero.magic = 0; Node::write_struct(base, hdr_zero)`).
  - `src/alloc_core/alloc_core_large.rs:174` (cache-hit fresh-header full write).
  - `src/alloc_core/alloc_core_large.rs:346–348` (`reclaim_large_segment`,
    `hdr_zero` full write).
  - Racing remote readers: `segment_header.rs` `magic_at`/`kind_at`/
    `large_size_at`/`span_usable_at`.
- **Verdict:** reproduces. **Confidence HIGH** (formal data race under a
  concurrent stale/duplicate remote free — app misuse, but exactly the case the
  defensive reads exist for). **Risk MEDIUM** — replace the full-struct write of
  the remotely-read fields with atomic/field-wise single-word writes
  (`magic` via `Node::atomic_u32_at`, as `owner_state` already does). Restores
  the crate's own §11 discipline. TSan/loom-relevant.

### M-3 — freelist intrusive `next` trusted without in-segment validation (UAF → OOB pointer)

- **Current locations (verified):** `src/alloc_core/alloc_core_small.rs:1462`
  (`pop_free`: `(next as usize - segment as usize) as u32`), and the identical
  computation in `drain_freelist_batch` (~1693–1702 / 1738–1750 per audit; both
  cfg branches).
- **Verdict:** reproduces. **Confidence HIGH** (per the crate's own contract:
  a user UAF write escalates to `Node::deref` with `off` up to `u32::MAX` — OOB
  `add`, UB per the seam's SAFETY comment, plus a wild pointer handed out).
  **Risk MEDIUM** — `hardened`-gated (at minimum) validation
  `next == null || segment_base_of_ptr(next) == segment` before accepting;
  truncate the chain on failure (`set_head(NULL)`), never deref. mimalloc
  `MI_SECURE` analogue. Touches the hot pop path, so bench-verify under `hardened`.

### M-4 — unchecked ring drain via `alloc_small` can double-issue a magazine-resident block (fastbin)

- **Current locations (verified):** `src/alloc_core/alloc_core_small.rs:1089`
  (`alloc_small` step 2 calls the **unchecked** `find_segment_with_free`),
  vs. the fastbin production path at `:660` which correctly uses
  `find_segment_with_free_checked`. `dec_live_and_maybe_decommit` in
  `alloc_core_small_pool.rs` (post-RAD-3).
- **Reachability note:** `alloc_small` is reachable from `AllocCore::alloc`
  (`alloc_core.rs:640`) and `refill_class_bump` (`alloc_core_small.rs:500`).
  Whether `HeapCore` (the `SeferAlloc` face) ever reaches `alloc_small`'s
  unchecked step-2 drain *for a magazine-managed class under `fastbin`* is the
  open reachability question the audit flagged (verdict PLAUSIBLE).
- **Verdict:** reproduces structurally. **Confidence MEDIUM** (double-issue +,
  with `alloc-decommit`, use-after-unmap — *iff* reachable). **Risk MEDIUM–HIGH**
  — thread the magazine predicate into every fastbin-reachable ring drain, OR
  prove+`debug_assert!` `alloc_small` unreachable for magazine-managed classes.
  Needs the reachability determination FIRST; touches decommit/live-count
  invariants.

### M-5 — `claim` OOM path: `gen==1`/uninit slot, gate is `new_gen==1` not `initialised`

- **Current locations (verified):** `src/registry/heap_registry.rs:84`
  (`claim`, gate `if new_gen == 1`), `:144` (`claim_with_config`, same gate).
  `HeapSlot::initialised` exists (`heap_slot.rs:315`, `AtomicBool`) but is
  published *after* materialization, not used as the gate.
- **Verdict:** reproduces (latent: slot leak today, one refactor from
  uninitialized-read UB). **Confidence HIGH.** **Risk LOW** — gate on
  `!slot.initialised.load(Acquire)`; then the OOM branch can push the slot back
  (fixing the leak). Small, but pin the invariant with a comment/test.

### M-7 — `abandon_segments` ↔ A1 deferred-free share `next_abandoned` (reactivation hazard)

- **Current locations (verified):** `src/registry/heap_registry.rs:259–289`
  (⚠️ REACTIVATION HAZARD note present), `src/registry/heap_core.rs:183–185`
  (cross-reference).
- **Reachability:** dead/test-only today (see M-6).
- **Verdict:** reproduces (latent, thoroughly documented in-code). **Confidence
  HIGH** (the hazard is real if the path is reactivated). **Risk LOW** — the
  action is a *guardrail*: add a test that fails if both stacks ever link one
  segment, and/or a dedicated link field, *before* any abandon/adopt
  reactivation. Not a live bug — a pin.

### M-8 — `try_evict_at` never returns `reusable:false`: saturation retirement is dead code

- **Current locations (verified):** `src/concurrent/hand.rs:303–305`
  (upfront `Stale` for `expected_gen == u32::MAX`), `:371–374`
  (unconditional `Evicted { reusable: true }` on CAS win — NOT
  `next != u32::MAX`). `drain_remote_free` (`epoch_region.rs:211–232`) pushes
  every drained index without the documented gen-MAX skip. Consumers `remove`
  (~296–327), `remote_evict` (~349–379).
- **Verdict:** reproduces. **Confidence HIGH** (logic/invariant bug: mints a
  live handle at gen `u32::MAX`, then unremovable → permanent leak + `len`
  desync; NOT memory-UB). **Risk LOW–MEDIUM** — return
  `Evicted { reusable: next != u32::MAX }`, make `drain_remote_free` skip
  gen-MAX indices per its own doc, fix the `EvictOutcome::Evicted` doc. Self-
  contained in `src/concurrent/`.

### M-9 — unbounded retention of cross-thread-freed Large segments when owner stops allocating Large

- **Current locations (verified):** `src/registry/heap_core.rs:599–603`
  (`HeapCore::alloc` drain gate `if class.is_none()`), `:1208–1211`
  (realloc drain gate); `src/alloc_core/deferred_large/drain.rs`. No small-path
  or thread-exit drain.
- **Verdict:** reproduces. **Confidence HIGH** (resource exhaustion, no UB —
  4+ GiB dead mapped segments for a plausible startup-Large-then-small-only
  workload). **Risk LOW–MEDIUM** — opportunistic drain on the small slow path
  behind a cheap `head != null` pre-check, and/or in `AbandonGuard::drop`
  before `recycle`. Hot-path-sensitive; bench the pre-check.

### L-3 — `SegmentTable::recycle` defensive mismatch tail releases OS reservation without hash/cache evict

- **Current location (verified):** `src/alloc_core/segment_table.rs:432–437`
  — the mismatch tail calls `release_segment` (line 436) with NO `hash_remove`/
  `own_cache_clear`, unlike the main path (`:403`/`:412`).
- **Verdict:** reproduces. **Confidence MEDIUM** (requires a corrupt/stale
  `segment_id` — but that is exactly what the defensive branch is for; leaves
  `contains_base` true for an unmapped base → next dealloc routes own-thread →
  read/write unmapped). **Risk LOW–MEDIUM** — leak instead of release, or
  `hash_remove`+`own_cache_clear` before releasing. Touches the recycle path
  (D1/M2-adjacent).

### L-4 — `flush_class`: second same-base run after a mid-batch decommit-recycle reads unmapped metadata

- **Current locations (verified):** `src/alloc_core/alloc_core_small.rs:781–804`
  (`flush_class` run split — no per-call recycled-base tracking), `flush_run`
  end → `release_or_pool_empty_segment` (~1047–1055).
- **Verdict:** reproduces. **Confidence MEDIUM** (needs a duplicate pointer of
  one segment in a single magazine batch, i.e. an upstream double-free that
  reached the magazine). **Risk MEDIUM** — skip bases already recycled in this
  call, or defer `release_or_pool_empty_segment` to end of `flush_class` (as the
  ring-drain path already defers recycle). Decommit-invariant path.

### L-5 — `kind_at` maps a corrupt discriminant to `Small` (Large free writes BinTable into live payload)

- **Current location (verified, post-RAD-3):** `src/alloc_core/segment_header.rs:617`
  (`kind_at`), `:630–632` (`0→Primordial, 2→Large, _→Small`).
- **Verdict:** reproduces. **Confidence MEDIUM** (requires a corrupted header
  byte — e.g. via H-1 — but the mapping *amplifies* rather than contains
  corruption). **Risk LOW–MEDIUM** — strict decode with a reject sentinel every
  caller treats as no-op. Pairs conceptually with H-1 (amplify-vs-contain on
  corrupt input). Touches routing.

### L-9 — assorted low-severity robustness items (all verified present)

- **L-9a** `claim` unbounded recursion — `heap_registry.rs:81`
  (`return Self::claim();`), `:141`. Fix: loop. **Risk LOW.**
- **L-9b** test-only `dbg_*` accessors deref without ownership check —
  `alloc_core_small.rs:411`/`:423`/`:434` (no `contains_base_ro`, unlike
  siblings at `:301`) and `alloc_core.rs:1021`/`:1030`. Fix: add the sibling
  `contains_base_ro` early return. **Risk LOW** (test-only).
- **L-9c** stale `TaggedPtr` doc describing removed 32-bit base packing —
  `tagged_ptr.rs:100–113` (the module header at `:63–68` already flags it
  HISTORICAL). Doc-only. **Risk LOW.**
- **L-9d** `aligned-vmem` unchecked address arithmetic —
  `crates/vmem/src/lib.rs:770–777` (`align_up_addr` unchecked `+`), `:367`
  (debug-only fit check). Fix: `checked_add` → OOM; promote fit check.
  **Risk LOW** (unreachable on real layouts).
- **L-9e** `numa.rs::reserve_aligned_on_node` leak on dead null branch —
  `src/alloc_core/numa.rs:88–99` (`into_parts()` at ~:94 suppresses RAII BEFORE
  the `NonNull::new(..)?` checks at ~:98–99). Fix: check first. **Risk LOW.**
- **L-9f** ring drain parks on reserved-but-unpublished slot —
  `remote_free_ring.rs:680–698`. BY-DESIGN sound (liveness only). **No action;
  on record.**
- **L-9g** untagged `tls_heap::current()` API hazard —
  `src/global/tls_heap.rs:266–293` (`pub`, `#[allow(dead_code)]`, hands out the
  untagged fallback pointer). Fix: return `CurrentHeap` / keep `pub(crate)` /
  `#[doc(hidden)]`. **Risk LOW.**

---

## Actionable but doc/hardening-only (record, low value)

- **L-6** `finish_bind` can claim a slot without arming `AbandonGuard`
  (`src/global/tls_heap.rs:446–447`, both `try_with` swallow `Err`). Reproduces.
  Availability/resource leak, no UB. **Risk LOW** — arm guard first, recycle on
  Err, return `Fallback`. Worth it only if the deployment churns TLS-destructor-
  allocating threads (couple with M-9's slot-hygiene theme).
- **L-7** cross-thread frees into fallback-owned segments effectively never
  reclaimed (`src/global/fallback.rs`). Reproduces; bounded-by-usage retention,
  no UB. **Document as by-design** or add a rare drain hook in `stats()`.
- **L-8** `AllocCore::drop` has no quiescence handshake vs. in-flight remote
  ring pushes (`src/alloc_core/alloc_core.rs:1415`). Reproduces but **moot today**
  (registry heaps never dropped; standalone `AllocCore` is `!Sync`). **Doc-pin
  the invariant** where `Drop` lives + keep the REACTIVATION HAZARD note load-
  bearing; no code change.

---

## Already tracked — do NOT create new tasks

These are from the *verification* doc (F1–F6), disjoint from the 20 summary
findings; re-anchored to current lines so the trackers point at the right code.

- **#59 UBFIX-1 — F1** foreign-ptr read in `AllocCore::realloc` move-leg.
  Now at **`src/alloc_core/alloc_core.rs:1111`** (verification doc cited the old
  monolith `:2160`); the move-leg `Node::copy_nonoverlapping(ptr, new_ptr, copy)`
  at **`:1142`** runs when `contains_base(base) == false`. Still unguarded.
  Fix: return `null_mut()` on foreign base, symmetric to `dealloc`. **NOT H-1.**
- **#59 UBFIX-1 — F2** double-free "no-op" doc imprecision. The `dealloc` doc
  contract ("no-op — never UB, never corrupts") is wider than the true guarantee
  ("no-op *until the address is reused*"). One-line doc fix.
- **#60 UBFIX-2 — F6** leak on `send` error in
  `crates/malloc-bench/src/lib.rs:245–249` (empty `is_err()` body — comment
  claims "Free locally" but there is NO `free_block` call) and `:311`
  (`let _ = send(block)`). Verified still present; bench-utility leak only.

---

## Documented residual — do NOT act

- **L-1 (= F4)** cross-thread double-free "re-issue-before-drain" residual
  (`src/registry/heap_core.rs:963–1003` RESIDUAL M2 LIMIT; `hardened` guard in
  `alloc_core_small.rs:131–156`). Trigger is caller UB (cross-thread double
  free); X7-pinned, RED test `residual_xthread_double_free_no_corruption`
  (`#[ignore]`). **Any change to the non-hardened drain path is HIGH-risk
  (H-1-adjacent MPSC protocol + decommit invariants) for zero benefit.** Keep
  pinned.
- **L-2** `dealloc_foreign_slow` header read on a possibly-unmapped/released
  segment (`heap_core.rs:1346–1392`; `sefer_alloc.rs:385–394`). Caller UB by the
  `GlobalAlloc` contract, inherent to every allocator. Optional hardening only
  (process-global lossy segment-base filter). Keep documented.

---

## Proposed task grouping (for the user to create after review — NOT created here)

> Recommendation: create these as a small set of cohesive passes. Do NOT
> duplicate #59/#60. RAD-3-zone note: L-5 (`segment_header.rs`) and M-4
> (`alloc_core_small_pool.rs`) sit in RAD-3's files — RAD-3 has now landed
> (tree clean), so these are unblocked, but keep them in a *later* pass than the
> registry/large ones so the two efforts don't collide on those files if RAD-3
> follow-ups appear.

**Task A — "Small-free lower-bound + unconditional bump guard" (H-1 + M-1).**
One patch, four sites in `alloc_core_small.rs` (`dealloc_small`, `reclaim_offset`,
`reclaim_offset_checked`, `flush_run`): add `off >= payload_start` and drop the
`alloc-decommit` cfg on `off >= bump`. Risk MEDIUM. Highest impact-to-effort in
the whole audit. Counterfactual test + M2 differential + decommit invariants.
*Consider extending #59's scope only if the owner prefers all `alloc_core_small`
free-path guards in one task — but H-1/M-1 are a distinct defect class from
F1/F2 (free-path guards vs. realloc-move-leg + doc), so a **separate** task is
cleaner. Do not fold into #59.*

**Task B — "Registry ABA tag preservation" (H-2 + M-6).** Identical fix shape in
`heap_registry.rs` / `tagged_ptr.rs` / `bootstrap.rs`: preserve the running tag
in the empty sentinel for both `free_slots` and `abandoned_segs`. Risk HIGH
(lock-free protocol) — ship with a loom model crossing the empty state with a
parked popper. One task, two stacks.

**Task C — "Registry claim hygiene" (M-5 + L-9a).** `heap_registry.rs`:
gate materialization on `initialised`, push the OOM'd slot back, convert
`claim` recursion to a loop. Same file, same function family. Risk LOW.

**Task D — "Large-header field-wise writes" (M-2).** `alloc_core.rs:826` +
`alloc_core_large.rs:174,346`: atomic/field-wise writes of the remotely-read
header fields. Risk MEDIUM; TSan/loom-relevant. Independent (large-cache zone).

**Task E — "Freelist next validation" (M-3).** `hardened` (min) validation in
`pop_free`/`drain_freelist_batch`. Risk MEDIUM (hot path). Independent.

**Task F — "Fastbin ring-drain predicate coverage" (M-4).** First determine
`alloc_small` reachability under `fastbin` for magazine-managed classes; then
thread the predicate or `debug_assert!` unreachable. Risk MEDIUM–HIGH.
Independent; do the reachability analysis before writing code.

**Task G — "Concurrent region saturation retirement" (M-8).** Self-contained in
`src/concurrent/{hand,epoch_region}.rs`. Risk LOW–MEDIUM. Independent.

**Task H — "Deferred-large + slot availability hygiene" (M-9 + L-6).**
Opportunistic large-deferred drain (small slow path / `AbandonGuard::drop`) plus
`finish_bind` guard-first-recycle-on-Err. Both are long-running-process
availability issues; adjacent (heap_core / tls_heap slot lifecycle). Risk
LOW–MEDIUM.

**Task I — "Defensive-path corruption containment" (L-3 + L-4 + L-5).** Small
amplify-vs-contain fixes on corrupt input: recycle-tail leak-not-release
(`segment_table.rs`), `flush_class` skip-recycled-bases, strict `kind_at`
decode (`segment_header.rs`). Same spirit as H-1. Risk LOW–MEDIUM. L-5 is in the
(now-landed) RAD-3 zone.

**Task J — "Reactivation guardrails" (M-7 + L-8).** Pin the shared
`next_abandoned` hazard with a failing test; doc-pin the `Drop`/ring-push
quiescence invariant. Pure guardrails, no live bug. Risk LOW.

**Task K — "L-9 cleanup batch" (L-9b/c/d/e/g).** `dbg_*` ownership guards, stale
`TaggedPtr` doc rewrite, vmem `checked_add`, numa check-order, `current()`
visibility. One janitorial task. Risk LOW. (L-9a → Task C; L-9f → no action.)

**No task:** L-1 (X7-pinned), L-2, L-7 (document-as-designed), L-9f.
