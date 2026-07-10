# UB / memory-safety audit — `src/concurrent/` + OS-level memory management

**Date:** 2026-07-10
**Scope:** `src/concurrent/hand.rs` (the confined unsafe organ), the rest of
`src/concurrent/` (epoch_region, sharded_region, lock_free_region, pinning —
all declared safe-only), `src/alloc_core/remote_free_ring.rs`,
`src/alloc_core/os.rs`, `src/alloc_core/numa.rs`, `src/alloc_core/node.rs`
(the pointer seam the ring/os paths route through), and `crates/vmem/src/lib.rs`
(the actual `mmap`/`VirtualAlloc` layer).
**Mode:** read-only static review (no cargo/miri/loom runs).

**Bottom line:** no exploitable memory-safety UB (use-after-free, double-free,
double-allocation, OOB, uninitialized read, aliasing violation, data race)
was found in the audited files. The atomic protocols (seqlock read in
`AtomicSlot::read_with`, generation-CAS eviction in `try_evict_at`, the
Vyukov-style MPSC `RemoteFreeRing`) check out under the documented invariants.
One genuine logic/invariant bug (generation-saturation retirement never
happens) and several low-severity items are recorded below.

---

## Finding 1 — `try_evict_at` never reports `reusable: false`: the saturation/retirement invariant is dead code, breaking the documented "no live handle ever carries `u32::MAX`" invariant

- **File:** `src/concurrent/hand.rs`, lines 295–375 (esp. 303–307 and 371–374);
  consumers: `src/concurrent/epoch_region.rs` lines 306–327 (`remove`) and
  356–375 (`remote_evict`).
- **Severity:** Medium (logic/invariant bug, permanent leak + wedged slot; NOT
  memory-UB — the failure mode is a no-op `Stale`, which touches nothing).
- **Description:** `EvictOutcome::Evicted { reusable }` is documented (hand.rs
  69–74, epoch_region.rs 287, 362–364) as `reusable == false` when the slot
  saturates, so the caller retires it. But `try_evict_at` handles saturation
  only for `expected_gen == u32::MAX` (returns `Stale` upfront, line 303) and
  then unconditionally returns `Evicted { reusable: true }` (line 374). The
  transition that **creates** a saturated slot — a winning CAS
  `u32::MAX - 1 → u32::MAX` — is reported as `reusable: true`. Consequently:
  1. `EpochRegion::remove`/`remote_evict` re-add the now-at-MAX slot to the
     free list (epoch_region.rs 321–323 / 368–375) instead of retiring it.
  2. A later `insert` pops it and mints a live `EpochHandle` carrying
     generation `u32::MAX` — directly violating the invariant stated at
     hand.rs 297–303 ("no live handle ever carries MAX"), which is the
     load-bearing justification for the upfront `Stale` guard.
  3. That handle's value can then **never be removed**: every
     `try_evict_at(u32::MAX, ..)` returns `Stale`, so `remove`/`remote_evict`
     return `false` forever. The value is reclaimed only at region drop
     (`drop_value`), `len` stays permanently inflated, and the slot is wedged
     occupied.
  Additionally, `drain_remote_free` (epoch_region.rs 211–232) does **not**
  perform the "defensive re-check that the slot's generation is not saturated"
  its own doc comment (lines 205–210) claims — it pushes every drained index
  blindly — so there is no second line of defense.
- **Concrete scenario:** one slot is evicted+reinstalled `2^32 - 1` times
  (long-lived process, hot slot). The `(MAX-1) → MAX` eviction wins the CAS,
  returns `reusable: true`, the slot re-enters the free list, `install` mints
  a handle at gen `MAX`, and the entry becomes unremovable (leak + `len`
  desync). No UAF/double-free results (a second remover at MAX also gets
  `Stale`), but the type's documented I3/I5 story is broken at the boundary.
- **Fix direction:** return `EvictOutcome::Evicted { reusable: next != u32::MAX }`
  (i.e. `false` when the winning CAS lands on MAX), and/or make
  `drain_remote_free` actually skip indices whose slot generation is
  `u32::MAX`, matching its doc comment. Update the `EvictOutcome::Evicted`
  doc (hand.rs 69–74), which currently asserts "reusable is always true for a
  CAS win" — an argument that only covers the `expected_gen == MAX` case, not
  the transition that creates saturation.

## Finding 2 — `aligned-vmem`: unchecked address arithmetic in `align_up_addr` and the Windows fit check

- **File:** `crates/vmem/src/lib.rs`, lines 770–777 (`align_up_addr`), 367
  (`debug_assert!(base_addr + size <= region_addr + over)`).
- **Severity:** Low (theoretical).
- **Description:** `align_up_addr` computes `(addr + mask) & !mask` with
  unchecked `+`; the Windows path's in-bounds fit check is a `debug_assert`
  with unchecked additions. If the OS ever returned a mapping whose base sits
  within `align` bytes of `usize::MAX`, the round-up would wrap to a small
  address and the subsequent `MEM_COMMIT`/pointer construction would target
  memory outside the reservation.
- **Concrete scenario:** not reachable on real Windows/Linux user-space
  layouts (the kernel never maps at the top of the address space), which is
  why this is Low — but the arithmetic itself carries no guard, and the fit
  check disappears in release builds.
- **Fix direction:** use `addr.checked_add(mask)?` (return `None` → treated as
  OOM) and promote the fit check to a release-mode guard returning `None`.

## Finding 3 — `numa.rs::reserve_aligned_on_node`: reservation leak on the (contractually dead) null branch

- **File:** `src/alloc_core/numa.rs`, lines 88–99.
- **Severity:** Low.
- **Description:** the code extracts `base_ptr`/`reservation_ptr`, then calls
  `r.into_parts()` (suppressing the RAII release), and only afterwards runs
  `NonNull::new(base_ptr)?` / `NonNull::new(reservation_ptr)?`. If either were
  null (the shim's contract says never), the early `return None` would leak
  the already-forgotten OS reservation — the defensive check defends the wrong
  side of the `mem::forget`.
- **Fix direction:** perform the `NonNull` checks (or just trust the contract
  and use `expect`) **before** `into_parts()`, so the RAII drop still fires on
  the impossible branch.

## Finding 4 — Documented residual risk (accepted, re-verified, no action): `'static` atomic views into releasable segment metadata

- **File:** `src/alloc_core/node.rs` (`atomic_u32_at` 370–391, `atomic_u64_at`
  453–467, `atomic_u8_at` 341–359); consumer `src/alloc_core/remote_free_ring.rs`
  (`head`/`tail`/`slot`, lines 571–590).
- **Severity:** Low (documented, inherent-to-design residual).
- **Description:** the seam hands out `&'static AtomicU32` references into
  segment memory, while Large segments (and decommit/eviction paths) ARE
  released mid-process. Soundness rests entirely on per-call-site liveness
  arguments in other files (the deferred-large double-push guard; the
  "dangling free into a released segment is fundamentally UB" honesty note in
  `registry::heap_core::dealloc_routing`). A cross-thread `RemoteFreeRing::push`
  whose target segment is released concurrently would be a write into unmapped
  memory. The lifetime notes in `node.rs` are explicit and correct about this;
  the audit confirms the claim structure but flags that any future
  "release-when-empty" policy for SMALL segments would invalidate the ring's
  push path wholesale (the REACTIVATION HAZARD note in `heap_registry` already
  warns of this).
- **Fix direction:** none required now; keep the REACTIVATION HAZARD note
  load-bearing in review checklists for any future decommit/release policy
  change.

## Finding 5 — Liveness (not safety) note: a stalled ring producer blocks the drain indefinitely

- **File:** `src/alloc_core/remote_free_ring.rs`, lines 680–698 (`drain` stops
  at the first `RING_SLOT_EMPTY` reserved-but-unpublished slot).
- **Severity:** Low (by-design liveness bound, sound).
- **Description:** a producer that wins the tail CAS and is descheduled before
  the `Release` publish store parks the consumer at that slot; all later
  published entries (up to `RING_CAP - 1`) stay undrained and further pushes
  overflow (bounded leak). This is documented and correct — FIFO order is what
  prevents skip-induced double-reclaim — recorded only so the leak-under-stall
  behavior is on the audit record. No corruption path exists: the full-check
  (`t.wrapping_sub(h) >= CAP` with `h` loaded `Acquire`, monotonic cursors)
  provably never lets a producer overwrite an undrained slot, and the
  power-of-two `RING_CAP` const-assert pins wrap continuity.

---

## What was checked and found sound

- **`hand.rs` `read_with` seqlock (g1 → value → g2):** the g2 re-check plus
  the happens-before chain (evictor CAS → free-list enqueue → owner drain →
  reinstall `Release` store → reader `Acquire` value load) rules out the
  torn-generation/ABA read; `defer_destroy` under the pinned guard gives the
  dereference lifetime. No path dereferences a swapped-out pointer.
- **`try_evict_at` uniqueness:** exactly one CAS winner per `expected_gen`;
  the swap-to-null and single `defer_destroy` are owned by the winner; the
  no-reinstall-before-swap proof (free-list re-add happens only after the
  eviction call returns, in both `remove` and `remote_evict`) was verified in
  `epoch_region.rs` — no double-free, no lost-live-value window.
- **`drop_value` / region `Drop`:** `&mut` exclusivity + `unprotected()` guard
  + `into_owned` → each live value dropped exactly once (I5); vacant/null is a
  no-op.
- **`Send`/`Sync` bounds on `AtomicSlot<T>` (`T: Send + Sync`):** correct;
  an unbounded impl would be unsound and is not present.
- **`RemoteFreeRing` MPSC orderings:** producer full-check (`Acquire` head) /
  tail CAS (`AcqRel`) / publish (`Release`); consumer tail `Acquire`, slot
  `Acquire`, clear `Relaxed`, head `Release` — each edge audited; the stale-`t`
  Relaxed load in `push` is conservative (head is monotonic, so occupancy is
  only ever over-estimated → spurious overflow, never under-estimated →
  overwrite). The `Relaxed` head load in `drain` and `tail_relaxed` rest on
  the single-consumer + ownership-handshake argument, which matches the
  shard-reuse model described at the call sites.
- **`os.rs` / `crates/vmem`:** reserve/commit/trim arithmetic in-bounds on
  both platforms (modulo Finding 2); Windows `VirtualFree(MEM_RELEASE, 0)`
  releases the whole reservation regardless of commit state (matches the
  reserve-only + sub-commit scheme); the failed-commit path releases the
  reservation before returning `None` (no VA leak); Unix exact-mmap fast path
  unmaps a misaligned mapping exactly once; the miri fallback's
  `Layout(reservation_len, align)` reconstruction matches the allocation
  exactly; `recommit` failure is surfaced as `false` and callers are
  contractually forbidden from writing (fault-not-UB honesty preserved).
  Monotonic reserved/released counters are diagnostic-only, `Relaxed` is fine.
- **Rest of `src/concurrent/`:** `epoch_region.rs`, `sharded_region.rs`,
  `lock_free_region.rs`, `pinning.rs` contain **no `unsafe` blocks** (grep
  verified — all `unsafe` mentions are documentation); they compose the safe
  `AtomicSlot` API / `arc-swap` / `core_affinity`. `len` accounting uses
  atomics correctly against remote removers.
- **`node.rs` primitives:** `write_next`/`read_next` use `write_unaligned`/
  `read_unaligned` (alignment-robust), one word at offset 0 within a
  `>= NODE_SIZE` block; `atomic_ptr_ref` uses exposed provenance to avoid the
  Stacked/Tree-Borrows tag-disabling hazard for cross-thread Treiber CAS
  (task #142 fix verified present); the (a)/(b) `'static` pointee enumeration
  for `atomic_ptr_ref` is exhaustive as documented.
