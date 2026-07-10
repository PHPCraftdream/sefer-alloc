# UB / Memory-Safety Audit — Consolidated Summary (2026-07-10)

Consolidation of five read-only audit reports:

1. `2026-07-10-ub-audit-alloc-core.md` (alloc-core)
2. `2026-07-10-ub-audit-registry.md` (registry)
3. `2026-07-10-ub-audit-concurrent-os.md` (concurrent-os)
4. `2026-07-10-ub-audit-segment-bitmap.md` (segment-bitmap)
5. `2026-07-10-ub-audit-global-xthread.md` (global-xthread)

Duplicated findings across reports are merged; both sources are noted.

---

## Executive summary

No **Critical** finding was reported by any of the five audits: no exploitable
UB was found on paths reachable by a contract-conforming caller. All memory-UB
findings sit in the *defensive* ("bad free is a no-op, never corruption" — M2)
contract, in caller-UB edges, or are latent (one refactor away).

Unique findings after dedup: **20** (the Low bucket groups seven minor
robustness items under one entry, L-9).

| Severity | Count |
|----------|-------|
| Critical | 0 |
| High     | 2 |
| Medium   | 9 |
| Low      | 9 (8 individual + 1 grouped entry of 7 minor items) |

Cross-report agreement is strong: the two highest-impact issues
(metadata-region free, freelist-tag ABA reset) plus three Medium issues were
independently found or corroborated by two reports each.

---

## High

### H-1. Missing payload lower-bound guard: invalid free of a metadata-region pointer corrupts segment metadata (incl. the primordial registry)

- **Sources:** alloc-core (Finding 1) + segment-bitmap (Finding 1) — same issue, merged.
- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1941–2013` (`dealloc_small`)
  - `src/alloc_core/alloc_core_small.rs:184–287` (`reclaim_offset`), `70–176` (`reclaim_offset_checked`)
  - `src/alloc_core/alloc_core_small.rs:810–1056` (`flush_run` guard pass)
- **Description:** All small-free paths guard `off >= bump` (decommit builds
  only), `off % block_size`, and the alloc-bitmap `is_free` bit, but never
  check the lower bound `off >= small_meta_end()` (`primordial_meta_end()` for
  the primordial segment). Metadata bitmap bits are never touched (read as
  "allocated"), so a class-aligned offset inside the metadata region passes
  every guard.
- **Failure scenario:** caller (or garbled/malicious cross-thread ring entry)
  frees `base + 0` or `base + 4096` of a registered segment with a small
  `Layout`. `Node::write_next` writes 8 bytes into the segment header / page
  map / bin table (e.g. directly over `SegmentHeader::bump`), the offset is
  published as a freelist head, and the next `alloc` hands out segment
  metadata as a live allocation. On the primordial segment this corrupts the
  self-hosted `SegmentTable` registry → every later `contains_base`/
  `unregister` desynchronizes → arbitrary UAF/double-release downstream.
  Directly violates `dealloc`'s documented M2 contract.
- **Fix:** unconditional `if off < payload_start { return; }` in all four
  paths, before `is_free`; one integer compare against a compile-time constant.

### H-2. `free_slots` ABA tag resets to 0 on empty-stack transition → stale CAS can succeed → registry slot double-claim (two threads share one `HeapCore`)

- **Source:** registry (Finding 1).
- **Files/lines:** `src/registry/heap_registry.rs:594–597` (pop), `636–638`
  (push); `src/registry/tagged_ptr.rs:139–143` (`empty()` packs tag = 0).
- **Description:** the Treiber-stack 48-bit ABA tag is derived from the
  current head word, but `pop_free_slot` writes `TaggedPtr::empty()` (tag 0)
  when it pops the last element — the tag counter restarts at 1 after every
  drain-to-empty. The "∼89 years to wrap" monotonicity assumption is false.
- **Failure scenario:** thread X parks mid-pop having seen head `(B, 5)` with
  `B.next = A`; stack drains to empty (tag→0); slot A is claimed LIVE by
  another thread; five pushes recur `(B, 5)`; X's CAS succeeds and puts LIVE
  slot A back on the free stack → after the owner's recycle, two threads can
  be handed the same `*mut HeapCore`, breaking the single-writer invariant all
  `UnsafeCell`/`Sync` proofs depend on → unsynchronized concurrent mutation of
  one heap → memory corruption / double-allocation. Milder variant: silent
  stack truncation leaking every slot on it.
- **Fix:** preserve the running tag in the empty sentinel
  (`pack(INDEX_MASK, observed_tag)`); index half alone denotes emptiness. Add
  a loom model crossing the empty state with a parked popper.

---

## Medium

### M-1. `off >= bump` guard is `alloc-decommit`-gated: non-decommit builds admit a free of a never-carved offset → double allocation

- **Sources:** alloc-core (Finding 2) + segment-bitmap (Finding 2) — merged.
- **Files/lines:** `src/alloc_core/alloc_core_small.rs:1976–1979`
  (`dealloc_small`), `243–245` (`reclaim_offset`), `113–115`
  (`reclaim_offset_checked`), `871–878` (`flush_run`) — all
  `#[cfg(feature = "alloc-decommit")]`.
- **Failure scenario:** build without `alloc-decommit`; an invalid free (own
  thread or garbled ring entry) of a class-aligned offset in the uncarved
  region `[bump, SEGMENT)` passes the bitmap guard (bit never set) and lands
  on the freelist. `pop_free` hands `base + off` to caller 1; later the bump
  carver advances past `off` and `carve_block` hands the same bytes to
  caller 2 (carve never consults the bitmap) — two live allocations aliasing
  the same memory, silent corruption.
- **Fix:** make the `off >= bump` rejection unconditional in all four sites
  (`bump_of()` is a single owner-side word load present in every build).

### M-2. Non-atomic full-struct `SegmentHeader` writes on the Large deposit/hit/reclaim paths race remote field reads (data race / UB; re-opens the §11 class)

- **Sources:** alloc-core (Finding 3) + segment-bitmap (Finding 4) — merged.
- **Files/lines:**
  - `src/alloc_core/alloc_core.rs:743, 826–828` (`dealloc` Large branch, `write_struct(hdr_zero)`)
  - `src/alloc_core/alloc_core_large.rs:165–174` (cache-hit fresh-header full write), `346–348` (`reclaim_large_segment`)
  - Racing remote readers: `src/alloc_core/segment_header.rs:569–599`
    (`kind_at`, `magic_at`), `638–641` (`large_size_at`), `677–680`
    (`span_usable_at`) in `dealloc_routing`.
- **Failure scenario:** thread A legitimately frees/reuses a Large segment
  while thread B (via a stale/duplicate cross-thread free — app misuse, but
  exactly what the defensive reads exist for) is mid-`magic_at`/`kind_at`.
  Full-struct plain write vs. plain read of the same bytes = data race, formal
  UB. Distinct from the documented released-segment dangling-read residual.
- **Fix:** zero/write the remotely-read fields via atomic views
  (`Node::atomic_u32_at` for `magic`, as done for `owner_state`) or field-wise
  single-word writes instead of `write_struct`.

### M-3. Freelist intrusive `next` pointer trusted without in-segment validation: one user UAF write → OOB pointer arithmetic (UB) + wild allocation handed out

- **Sources:** segment-bitmap (Finding 3, Medium) + alloc-core (Finding 5,
  rated Low there) — merged; kept at the higher rating.
- **Files/lines:** `src/alloc_core/alloc_core_small.rs:1455–1463`
  (`pop_free`), `1693–1702` / `1738–1750` (`drain_freelist_batch`);
  `src/alloc_core/node.rs:100–108`, `320–328` (`Node::offset` contract).
- **Failure scenario:** app frees block P, then writes 8 bytes through a
  dangling pointer (classic UAF), clobbering P's `next` word. The next pop
  computes `(garbage - segment) as u32` (silently wrapping), and the following
  pop does `Node::deref(segment, off)` with `off` up to `u32::MAX` — pointer
  arithmetic outside the allocation (UB per the seam's own SAFETY contract)
  and an out-of-segment pointer returned as a fresh allocation.
- **Fix:** validate `next == null || segment_base_of_ptr(next) == segment`
  (plus, under `hardened`, alignment/bounds) before accepting; on failure
  truncate the chain (`set_head(NULL)`), never deref. mimalloc `MI_SECURE`
  analogue.

### M-4. fastbin residual: unchecked ring drain (`alloc_small` / plain `find_segment_with_free`) can double-issue a magazine-resident block; with `alloc-decommit`, release a segment whose blocks are still in the magazine (use-after-unmap)

- **Source:** alloc-core (Finding 4; verdict PLAUSIBLE).
- **Files/lines:** `src/alloc_core/alloc_core_small.rs:1201–1207`
  (`find_segment_with_free`, default `&|_, _| false` predicate), `1089–1092`
  (`alloc_small` step 2); `src/alloc_core/alloc_core_small_pool.rs:76–111`
  (`dec_live_and_maybe_decommit`).
- **Failure scenario:** task #164 closed the ring↔magazine double-free only on
  the *checked* chain. A cross-thread double-free note for a block resident in
  the owner's magazine, drained via the unchecked chain, is relinked and
  `dec_live`d: (1) double allocation (magazine copy + freelist pop); (2)
  spurious `dec_live` drives `live_count` to 0 while magazine pointers still
  exist → segment released → later magazine flush reads/writes unmapped memory.
- **Fix:** thread the magazine predicate into every ring drain reachable under
  `fastbin`, or prove+debug-assert `alloc_small` unreachable for
  magazine-managed classes.

### M-5. `claim` OOM path: slot left with `generation == 1` but uninitialized `HeapCore`; materialization gate is `new_gen == 1`, not `initialised` → slot leak today, latent uninitialized-read UB one refactor away

- **Sources:** registry (Finding 3) + global-xthread (Finding 3) — merged.
- **Files/lines:** `src/registry/heap_registry.rs:83–98` (`claim`), `143–158`
  (`claim_with_config`).
- **Failure scenario:** on primordial OOM the slot is CASed back LIVE→FREE but
  never re-pushed (permanent slot leak per OOM). If any future cleanup pushes
  it back to `free_slots`, the next claimer sees `new_gen == 2`, skips
  `HeapCore::new`, and returns `slot.heap.get()` over `MaybeUninit::uninit()`
  bytes — read-of-uninitialized-memory UB on the very next alloc. Nothing pins
  the trap.
- **Fix:** gate materialization on `!slot.initialised.load(Acquire)` (the
  authoritative, monotonic flag) instead of `new_gen == 1`; then the OOM
  branch can safely push the slot back (fixing the leak too).

### M-6. Same ABA tag-reset flaw in `abandoned_segs` (plus only 22 tag bits) — latent

- **Source:** registry (Finding 2).
- **Files/lines:** `src/registry/heap_registry.rs:375–391` (pop), `719–721`
  (push); `src/registry/bootstrap.rs:265` (`ABANDONED_HEAD_EMPTY = 0`).
- **Failure scenario:** identical structure to H-2 with segment bases: a stale
  CAS against a recurred `(base, tag)` resurrects an already-adopted (LIVE)
  segment onto the abandoned stack → two heaps' `SegmentTable`s both contain
  it → both allocate from it → double-allocation of the same blocks. Currently
  unreachable from production (test-only Phase 12.5 primitive), but retained
  as substrate for a future decommit-when-empty policy; tag also wraps at
  ~4.2 M pushes even without the reset.
- **Fix:** same as H-2 (preserve tag in the empty word) — before any
  reactivation.

### M-7. `abandon_segments` ↔ A1 deferred-free stack share the `next_abandoned` link field — latent reactivation hazard

- **Source:** registry (Finding 5); corroborated by concurrent-os Finding 4's
  REACTIVATION HAZARD note.
- **Files/lines:** `src/registry/heap_registry.rs:259–290` (hazard note),
  `src/registry/heap_core.rs:183–185`.
- **Failure scenario:** if a future decommit policy reactivates
  `abandon_segments` while a Large segment is mid-flight on a heap's local
  deferred stack, the intrusive link is clobbered: one chain becomes
  unreachable (leak) and a later pop can follow a link into the other stack's
  chain — wild pointer read and potential double-reclaim (segment released
  while still queued elsewhere → whole-segment UAF).
- **Fix:** dedicated link field per stack, or make the reactivated walk skip
  `SegmentKind::Large`; add a test failing if both stacks ever link one segment.

### M-8. `try_evict_at` never returns `reusable: false`: saturation retirement is dead code — a live handle can carry `u32::MAX` and its value becomes unremovable

- **Source:** concurrent-os (Finding 1). Logic/invariant bug, not memory-UB.
- **Files/lines:** `src/concurrent/hand.rs:295–375` (esp. 303–307, 371–374);
  consumers `src/concurrent/epoch_region.rs:306–327`, `356–375`;
  `drain_remote_free` at 211–232 also lacks its documented saturation re-check.
- **Failure scenario:** the winning CAS `(MAX-1) → MAX` is reported
  `reusable: true`; the saturated slot re-enters the free list; `insert` mints
  a live `EpochHandle` at generation `u32::MAX` (violating the documented
  invariant), and every later `try_evict_at(MAX, ..)` returns `Stale` — the
  value can never be removed, `len` stays inflated, the slot is wedged until
  region drop. Permanent leak, no UAF.
- **Fix:** return `Evicted { reusable: next != u32::MAX }`; make
  `drain_remote_free` skip gen-MAX indices per its own doc; fix the
  `EvictOutcome::Evicted` doc.

### M-9. Unbounded retention of cross-thread-freed Large segments when the owner stops allocating Large

- **Source:** global-xthread (Finding 1). Resource exhaustion, no UB.
- **Files/lines:** `src/registry/heap_core.rs:594–603`, `1202–1211` (drain
  gates); `src/alloc_core/deferred_large/drain.rs:47`.
- **Failure scenario:** thread A allocates 1000 × 4 MiB in a startup phase,
  workers free them all → all segments queue on A's deferred stack, drained
  only on A's next Large alloc — which never comes → 4+ GiB of dead mapped
  segments retained for the process lifetime (also after A exits, until a
  claimant allocates Large). Unbounded relative to live data.
- **Fix:** opportunistic drain on the small-alloc slow path (cheap
  `head != null` pre-check) and/or in `AbandonGuard::drop` before `recycle`.

---

## Low

### L-1. Cross-thread double-free "re-issue-before-drain" residual (non-`hardened` builds) — documented, RED-pinned

- **Source:** global-xthread (Finding 5); explicitly excluded-as-known by
  registry report. Trigger is caller UB (double free from another thread).
- **Files/lines:** `src/registry/heap_core.rs:963–1003` (RESIDUAL M2 LIMIT);
  `src/alloc_core/alloc_core_small.rs:131–156` (`hardened` generational guard).
- **Failure scenario:** a stale `(off, class)` ring entry for an already
  re-issued block is indistinguishable from a genuine delayed free; the drain
  relinks a live block → same address handed out twice. `hardened` narrows to
  a 1/256 generation-wrap residual; production has no guard. Tracked as X7.

### L-2. `dealloc_foreign_slow` header read on a possibly-unmapped/released segment — documented residual

- **Sources:** registry (Finding 4) + global-xthread (Finding 6) — merged.
- **Files/lines:** `src/registry/heap_core.rs:1346–1392` (rationale
  1347–1367, read at 1376); `src/global/sefer_alloc.rs:385–394`.
- **Failure scenario:** double-free of a pointer into a segment already
  released to the OS → `magic_at(base)` faults (best case) or, if remapped and
  magic matches by chance, routes a wild Treiber push into foreign memory.
  Caller UB by the `GlobalAlloc` contract; inherent to every allocator.
  Optional hardening: process-global lossy segment-base filter.

### L-3. `SegmentTable::recycle` defensive mismatch tail releases the OS reservation without `hash_remove`/`own_cache_clear` → `contains_base` true for an unmapped base

- **Source:** segment-bitmap (Finding 5).
- **File/lines:** `src/alloc_core/segment_table.rs:432–437`.
- **Failure scenario:** corrupted/stale header `segment_id` → defensive tail
  releases anyway but leaves the base resolvable; next dealloc routes
  own-thread → `kind_at` reads unmapped memory (fault) or writes into a
  re-mapped foreign region. Fix: leak instead of release, or evict
  hash/cache first (as the main path does at lines 399–417).

### L-4. `flush_class`: second same-base run after a mid-batch decommit-recycle reads unmapped metadata (UAF; requires an upstream double-free in one magazine batch)

- **Source:** segment-bitmap (Finding 6).
- **Files/lines:** `src/alloc_core/alloc_core_small.rs:781–804` (run split),
  `1047–1055` (`flush_run` end → `release_or_pool_empty_segment`).
- **Failure scenario:** batch `[A…, B…, A-dup]` where the first A run drives
  `live_count` to 0 and pool is full/disabled → A is fully recycled; the
  second `flush_run(A)` reads BinTable/bump/bitmap on unmapped memory. Fix:
  skip bases already recycled in this call, or defer releases to end of
  `flush_class` (as the ring-drain path does).

### L-5. `kind_at` maps a corrupt discriminant byte to `Small` → Large-segment free writes BinTable metadata into live user payload

- **Source:** segment-bitmap (Finding 7).
- **File/lines:** `src/alloc_core/segment_header.rs:569–586`.
- **Failure scenario:** one flipped header byte (e.g. via H-1) makes a Large
  segment decode as Small; `dealloc_small` then `set_head`/`write_next`
  writes 12 bytes into the single live user allocation. Fix: strict decode
  with a reject sentinel treated as no-op.

### L-6. `finish_bind` can claim a registry slot without arming the `AbandonGuard` → permanent LIVE-slot leak (TLS-destructor-phase allocations)

- **Source:** global-xthread (Finding 2). (Rated Medium there for
  availability; consolidated as Low-priority resource issue — no UB. If slot
  exhaustion matters for the deployment, treat with M-9.)
- **File/lines:** `src/global/tls_heap.rs:436–449` (swallowed `try_with` Errs
  at 446–447).
- **Failure scenario:** a thread whose only allocations happen inside TLS
  destructors claims a slot but `GUARD.try_with` fails → slot stays LIVE
  forever; if `LOCAL.try_with` fails, every subsequent alloc claims another
  slot — leaking all but the last. After 4096 leaks all new threads serialize
  through the spinlocked fallback. Fix: arm guard first; on Err recycle the
  slot and return `CurrentHeap::Fallback`.

### L-7. Cross-thread frees into fallback-owned segments effectively never reclaimed

- **Source:** global-xthread (Finding 4).
- **File:** `src/global/fallback.rs`; interaction with
  `src/registry/heap_core.rs:1376–1459`.
- **Failure scenario:** early-init fallback allocations freed by worker
  threads sit in the fallback's rings/deferred stack, drained only when the
  fallback itself next allocates (usually never); rings cap at 256/segment,
  then further frees are discarded (bounded leak). Fix: document as designed,
  or add a rare drain hook (e.g. in `stats()`).

### L-8. `AllocCore::drop` has no quiescence handshake against in-flight remote ring pushes; `'static` atomic views into releasable segment metadata are call-site-argued

- **Sources:** alloc-core (Finding 6) + concurrent-os (Finding 4) — related
  design residuals, merged.
- **Files/lines:** `src/alloc_core/alloc_core.rs:1415–1466` (`Drop`);
  `src/alloc_core/node.rs:341–391, 453–467` (`atomic_*_at`);
  `src/alloc_core/remote_free_ring.rs:571–590`.
- **Failure scenario:** moot today (registry heaps never dropped; standalone
  `AllocCore` is `!Sync`), but any future "drop a heap mid-process" or
  "release-when-empty for Small segments" policy makes a concurrent ring push
  a write into unmapped memory. Fix: pin the invariant in docs at `Drop` and
  keep the REACTIVATION HAZARD note load-bearing in review checklists.

### L-9. Assorted low-severity robustness items

- **`claim` retries via unbounded recursion** — registry (Finding 6);
  `src/registry/heap_registry.rs:81, 141`. Theoretical stack overflow inside
  the global allocator under pathological contention. Fix: convert to a loop.
- **Test-only `dbg_*` accessors deref segment metadata without ownership
  check** — alloc-core (Finding 7);
  `src/alloc_core/alloc_core_small.rs:411–442`,
  `src/alloc_core/alloc_core.rs:1021–1043`. A test passing a non-heap pointer
  reads/writes unrelated memory. Fix: add `contains_base_ro` early return.
- **Stale `TaggedPtr` doc describes removed unsound 32-bit base packing** —
  registry (Finding 7); `src/registry/tagged_ptr.rs:100–113`. Doc-only; could
  seduce a reader into re-introducing >4 GiB address truncation. Fix: rewrite.
- **`aligned-vmem` unchecked address arithmetic** — concurrent-os (Finding 2);
  `crates/vmem/src/lib.rs:770–777, 367`. `align_up_addr` wraps if the OS ever
  mapped within `align` of `usize::MAX` (unreachable on real layouts); fit
  check is debug-only. Fix: `checked_add` → treat as OOM.
- **`numa.rs::reserve_aligned_on_node` leak on contractually-dead null
  branch** — concurrent-os (Finding 3); `src/alloc_core/numa.rs:88–99`.
  `NonNull` checks run after `into_parts()` suppressed RAII. Fix: check first.
- **Ring drain parks on a reserved-but-unpublished slot** — concurrent-os
  (Finding 5); `src/alloc_core/remote_free_ring.rs:680–698`. By-design
  bounded liveness leak, sound; on record only.
- **Untagged `tls_heap::current()` API hazard** — global-xthread (Finding 7);
  `src/global/tls_heap.rs:266–293`. Future direct consumer could form a second
  `&mut` over the fallback. Fix: return `CurrentHeap` / keep `pub(crate)`.

---

## Recommended action plan

**Wave 1 — fix now (small, closes real corruption classes, corroborated by
two independent reports each):**

1. **H-1** — payload lower-bound guard in `dealloc_small`, `reclaim_offset`,
   `reclaim_offset_checked`, `flush_run`. One compile-time-constant compare;
   closes metadata corruption incl. the primordial registry. Highest
   impact-to-effort ratio in the whole audit.
2. **M-1** — drop the `alloc-decommit` cfg gates on `off >= bump` (same four
   sites; naturally shipped in the same patch as H-1). Closes the
   non-decommit double-allocation window.
3. **H-2** — preserve the ABA tag across empty transitions in `free_slots`
   (and, in the same change, **M-6** for `abandoned_segs` — identical fix
   shape). Localized change + a loom model crossing the empty state. This is
   the only finding that breaks the registry's single-writer foundation under
   pure thread churn with no caller error.

**Wave 2 — fix soon (formal UB / latent traps, moderate effort):**

4. **M-2** — replace full-struct header writes on the Large
   deposit/hit/reclaim paths with atomic/field-wise writes of the
   remotely-read fields. Restores the crate's own §11 discipline; removes
   formal data-race UB.
5. **M-5** — switch the claim materialization gate to `initialised` and push
   the OOM'd slot back. Defuses the "one refactor from uninitialized-read UB"
   trap and fixes the slot leak; trivially small.
6. **M-3** — `hardened` (at minimum) freelist `next` validation in
   `pop_free`/`drain_freelist_batch`. Contains user-UAF escalation, mirrors
   mimalloc-secure.
7. **M-4** — thread the magazine predicate through every fastbin-reachable
   ring drain (or prove/assert unreachability). PLAUSIBLE double-issue +
   use-after-unmap; needs a reachability check first.

**Wave 3 — hardening / resource hygiene (no UB in correct usage):**

8. **M-9** and **L-6** — opportunistic deferred-large drain on the small slow
   path + `AbandonGuard`-first `finish_bind` with recycle-on-Err. Both are
   availability issues that bite long-running processes.
9. **M-8** — `reusable: next != u32::MAX` in `try_evict_at` +
   `drain_remote_free` saturation skip. Correctness of the concurrent
   region's documented invariants.
10. **L-3, L-4, L-5** — small defensive-path fixes in the same spirit as
    Wave 1 (leak-not-release recycle tail; skip recycled bases in
    `flush_class`; strict `kind_at` decode). Cheap, all amplify-vs-contain
    choices on corrupt input.

**Wave 4 — documentation and guardrails:**

11. **M-7** — pin the shared `next_abandoned` hazard with a failing test
    before any abandon/adopt reactivation.
12. **L-8** — doc-pin the `Drop`/ring-push quiescence invariant and the
    release-when-empty REACTIVATION HAZARD.
13. Remaining **L-9** items (loop-ify `claim`, `dbg_*` guards, stale
    `TaggedPtr` doc, vmem `checked_add`, numa check order, `current()`
    visibility) — batch as a single cleanup task.
14. **L-1, L-2, L-7** — already-documented residuals: keep pinned
    (X7 for L-1), document fallback retention as by-design.

**Rationale for ordering:** Wave 1 items are the only ones where a single bad
input (or, for H-2, no bad input at all — just thread churn) corrupts
allocator state that everything downstream trusts, and each fix is a few
lines. Wave 2 removes formal UB and refactor traps before they are triggered
by unrelated changes. Waves 3–4 improve robustness and keep documented
residuals from silently regressing.
