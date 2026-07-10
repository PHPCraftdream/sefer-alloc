# UB / Memory-Safety Audit — `src/registry/` (2026-07-10)

Scope: `heap_core.rs`, `heap_registry.rs`, `heap_slot.rs`, `bootstrap.rs`,
`tagged_ptr.rs`, `tcache.rs`, `mod.rs` (read-only inspection, no code executed).

Note on scope: the task also named `segment_table.rs` — that file does **not**
live in `src/registry/` (it belongs to the `alloc_core` substrate and is covered
by the companion alloc-core audit). This report covers every file actually
present in `src/registry/`.

Categories hunted: UB, use-after-free, double-free, double-allocation (same
pointer to two callers), dangling pointers, uninitialized reads, OOB,
aliasing/Stacked-Borrows violations, incorrect `unsafe`.

---

## Finding 1 — ABA tag resets to 0 whenever the `free_slots` stack empties → stale CAS can succeed → slot double-claim (two threads share one `HeapCore`)

- **File:** `src/registry/heap_registry.rs` (pop: lines 594–597; push: lines 636–638) together with `src/registry/tagged_ptr.rs` (lines 139–143, `empty()` packs tag = 0)
- **Severity:** High
- **Description:** The `free_slots` Treiber stack relies on a 48-bit monotonic
  tag to defeat ABA. However, `pop_free_slot` sets the head to
  `TaggedPtr::empty()` when it pops the last element — and `empty()` is
  `pack(INDEX_MASK, 0)`, i.e. **tag = 0**. `push_free_slot` then derives the
  next tag from the *current head word* (`unpack(head)` → `tag.wrapping_add(1)`),
  so after every drain-to-empty the tag counter restarts at 1. The tag is not
  process-monotonic; its history is erased on each empty transition. The
  module doc's "∼89 years to wrap" analysis assumes monotonicity that the code
  does not provide.
- **Concrete scenario:**
  1. Head is `(B, 5)`. Thread X (in `pop_free_slot`) loads the head and reads
     `B.next_free = A`, then is preempted before its CAS.
  2. Other threads pop `B`, then `A`, … until the stack drains → head becomes
     `empty()` with **tag 0**.
  3. Slot `A` is claimed (`FREE → LIVE`) and is now a live thread's heap.
  4. Thread churn pushes five recycled slots: tags go 1, 2, 3, 4, and the 5th
     push happens to be slot `B` again → head is `(B, 5)` — bit-identical to
     what X observed (small tags recur quickly after every reset, unlike a
     monotonic 48-bit counter).
  5. X wakes and its CAS `(B,5) → (A,5)` **succeeds**. `A` — currently LIVE —
     is now at the top of the free stack. The next `claim` pops `A`, CASes
     `FREE→LIVE`… whose `state` is LIVE, so that particular claim retries; but
     when `A`'s real owner later recycles it, `A` sits on the stack **twice**:
     two subsequent `claim`s can both pop index `A` (via the two stale chain
     entries) and, after the owner's recycle, both win a `FREE→LIVE` CAS at
     different times while a stale chain still references it — ultimately two
     threads can be handed the same `*mut HeapCore`, breaking the single-writer
     invariant every `UnsafeCell`/`Sync` proof in `heap_slot.rs` depends on →
     concurrent unsynchronized mutation of one heap's `AllocCore`/magazine →
     memory corruption / double-allocation of user blocks. A milder variant of
     the same stale CAS (X had read `B.next == TAIL`) silently truncates the
     stack, leaking every slot on it.
- **Fix direction:** make the tag genuinely monotonic across empty transitions:
  keep the last tag in the empty sentinel (`empty_with_tag(tag)` — the index
  half `0xFFFF` alone denotes emptiness; the tag bits are free to carry the
  running counter), and in `pop_free_slot` build the empty head as
  `pack(INDEX_MASK, observed_tag)` instead of `TaggedPtr::empty()`. `is_empty`
  already ignores the tag, so the change is local. Add a loom interleaving
  that drains the stack to empty and re-pushes the same index (the existing
  `loom_free_slots_aba.rs` model apparently never crosses the empty state with
  a parked popper).

## Finding 2 — Same tag-reset flaw in the `abandoned_segs` stack (plus only 22 tag bits)

- **File:** `src/registry/heap_registry.rs` lines 375–391 (`pop_abandoned_segment`: `new_head = ABANDONED_HEAD_EMPTY` — tag 0) and lines 719–721 (push derives tag from current head); `src/registry/bootstrap.rs` line 265 (`ABANDONED_HEAD_EMPTY: u64 = 0`)
- **Severity:** Medium (the protocol is currently unreachable from production paths — Phase 12.5 shard model; `abandon_segments`/`try_adopt` are exercised only by tests — but the primitive is retained explicitly as a substrate for a future decommit-when-empty policy)
- **Description:** Identical structure to Finding 1: a pop that empties the
  stack sets the head to the all-zero word, resetting the ABA tag to 0; the
  next push resumes from tag 1. Additionally the tag here is only
  `ABANDON_TAG_BITS = 22` bits wide (wraps at ~4.2 M pushes even without the
  reset). A stale popper whose CAS succeeds against a recurred `(base, tag)`
  installs a stale `next_abandoned` chain — a segment already adopted (LIVE,
  registered in an adopter's table) can be re-popped and re-adopted by a second
  heap → two heaps' `SegmentTable`s both contain the segment → both allocate
  from it → double-allocation of the same blocks.
- **Concrete scenario:** as Finding 1, with segment bases instead of slot
  indices: park a popper on `(seg_B, 5)`, drain to empty (tag→0), adopt
  `seg_A`, push 5 segments ending with a re-abandoned `seg_B` at tag 5; the
  stale CAS resurrects `seg_A` (now live in an adopter) onto the abandoned
  stack.
- **Fix direction:** same as Finding 1 — preserve the tag in the empty word
  (`base == null` alone denotes empty; `abandoned_head_is_empty` already masks
  the tag out). Fix before any future reactivation of the abandon/adopt path.

## Finding 3 — `claim` OOM path leaves a slot with `generation == 1` but an uninitialized `HeapCore`; the re-claim gate is `new_gen == 1`, not `initialised` → latent uninitialized-read UB

- **File:** `src/registry/heap_registry.rs` lines 83–98 (`claim`), 143–158 (`claim_with_config`)
- **Severity:** Medium (latent — not reachable today, one code change away from UB)
- **Description:** On a first claim, `generation` is bumped to 1 **before**
  `HeapCore::new()` runs. If `HeapCore::new()` fails (primordial OOM), the code
  CASes the slot back `LIVE→FREE` and returns null — but `generation` stays 1
  and `initialised` stays false, and the slot is **not** pushed onto
  `free_slots`. The materialization gate on a later claim is
  `new_gen == 1` — for this poisoned slot a hypothetical re-claim would compute
  `new_gen == 2`, **skip** materialization, and hand out
  `slot.heap.get().cast::<HeapCore>()` pointing at `MaybeUninit::uninit()`
  bytes — a fully-fledged read-of-uninitialized-memory UB on the very next
  `alloc` through that pointer. Today the slot is unreachable (never pushed to
  the free stack; `bump_count` is monotonic; `recycle` needs a heap pointer
  nobody has), so this is a leak (one slot index + the `count` bump are
  permanently consumed per OOM'd claim), not live UB. But the invariant
  "generation ≥ 1 ⇒ heap materialised" is false, and the code's own safety
  argument ("slot is LIVE and initialised; we are sole writer" — line 117) is
  carried by an accident of reachability rather than by the checked condition.
- **Concrete scenario (one refactor away):** any future change that pushes the
  OOM'd slot back onto `free_slots` (the natural "don't leak the slot" cleanup)
  makes the next claimer pop it, see `new_gen == 2`, skip `HeapCore::new`, and
  return a pointer into uninitialized memory; first magazine access through it
  reads garbage `count`/`slots` → wild pointer handed to the user.
- **Fix direction:** gate materialization on `!slot.initialised.load(Acquire)`
  instead of `new_gen == 1` (the flag is documented as the authoritative
  "heap is materialised" bit and is monotonic true), or roll `generation` back
  (`fetch_sub`) on the OOM path. Optionally also push the slot back to
  `free_slots` once the gate is the flag, eliminating the leak.

## Finding 4 — `dealloc_foreign_slow` header read on a potentially unmapped segment (documented residual)

- **File:** `src/registry/heap_core.rs` lines 1346–1392 (`dealloc_foreign_slow`)
- **Severity:** Low (inherent to the design; triggered only by a caller-side contract violation)
- **Description:** For a pointer whose base fails `contains_base`, the code
  reads `SegmentHeader::magic_at(base)` / `owner_thread_free_at(base)` /
  `kind_at(base)` from memory it does not own. For case (b) in the code's own
  comment — a segment already released to the OS — this read **faults or reads
  a foreign mapping** (if the address range was re-mapped by unrelated code, a
  stale `SEGMENT_MAGIC` coincidence would route a push into a wild
  `owner_thread_free` word). This is acknowledged in-code as the universal
  double-free-after-release limitation; the #138 `large_layout_consistent`
  check narrows but does not close it. Listed for completeness: the *trigger*
  is caller UB (free of a stale pointer), so this is a robustness residual,
  not a soundness hole in correct usage.
- **Concrete scenario:** thread A frees a Large block cross-thread; owner
  drains and OS-releases the segment; a buggy caller double-frees the same
  pointer from thread B → `magic_at(base)` dereferences unmapped memory →
  SIGSEGV (best case) or, if remapped and magic matches by chance, a wild
  Treiber push corrupting an unrelated allocation.
- **Fix direction (hardening only):** a global segment-address registry
  (shared read-only map of live segment bases) consulted before any header
  read, gated behind `hardened`.

## Finding 5 — `abandon_segments` ↔ A1 deferred-free stack share the `next_abandoned` link (documented reactivation hazard)

- **File:** `src/registry/heap_registry.rs` lines 259–290 (hazard note), `heap_core.rs` lines 183–185
- **Severity:** Medium (latent; dead code today, thoroughly documented in-code)
- **Description:** The global abandoned-segments stack and the per-heap A1
  Large deferred-free stack both use the segment header's `next_abandoned`
  field as their intrusive link. If a future decommit policy reactivates
  `abandon_segments` while a Large segment is mid-flight on a heap's local
  deferred stack, the link is clobbered: one stack's chain becomes unreachable
  (leak) and a later pop can follow a link **into the other stack's chain** —
  a wild/foreign pointer read and potential double-reclaim (segment released
  while still queued elsewhere → use-after-free of the whole segment). No test
  exercises the two stacks concurrently on one segment. Confirming the
  in-code warning as a real finding so it is tracked, not just commented.
- **Fix direction:** give each stack its own dedicated link field, or make the
  reactivated walk skip `SegmentKind::Large`, *before* any reactivation; add a
  test that fails if both stacks ever link the same segment.

## Finding 6 — `claim` retries via unbounded recursion

- **File:** `src/registry/heap_registry.rs` line 81 (`return Self::claim();`), line 141 (`claim_with_config`)
- **Severity:** Low
- **Description:** Losing the `FREE→LIVE` slot race retries by recursing.
  Rust does not guarantee tail-call elimination; under pathological contention
  (many threads repeatedly racing the same popped slots) the recursion depth is
  unbounded → theoretical stack overflow inside the allocator (which, as the
  global allocator, aborts the process). Also, each lost race permanently
  discards the popped index from consideration by *this* claimer only — no
  leak, but the retry loop shape hides the invariant. (Note: per the analysis
  of the current protocol a popped slot cannot actually be LIVE, so today the
  branch is defensive-only — which further argues for a loop.)
- **Fix direction:** convert to an explicit `loop`.

## Finding 7 — Stale doc comment in `tagged_ptr.rs` describes a removed (unsound) 32-bit base packing

- **File:** `src/registry/tagged_ptr.rs` lines 100–113
- **Severity:** Low (documentation only)
- **Description:** The `TaggedPtr` type doc still says "for `abandoned_segs`
  it is a segment base address … restricts segment bases to the low 32 bits",
  contradicting the module header (lines 11–16) which correctly states that
  `abandoned_segs` moved off `TaggedPtr` (FINDINGS №1 fix). A future reader
  could re-introduce pointer packing through `TaggedPtr` on the strength of
  this paragraph — recreating the >4 GiB address-truncation corruption.
- **Fix direction:** delete/rewrite the stale paragraph; `TaggedPtr` is
  index-only.

---

## Items examined and found sound (no finding)

- **`bootstrap.rs` init protocol:** `null → SENTINEL → real ptr` state machine
  with Release publish / Acquire read; `#[cfg(miri)]` zeroing closes the
  std-alloc-fallback uninit gap; OOM rollback (`rollback_registry_sentinel`)
  prevents the #131 livelock; sentinel is `without_provenance_mut` and never
  dereferenced. RAD-1 lazy `next_free` is sound: the only reader
  (`pop_free_slot`) reads it strictly after a `push_free_slot` Release-write of
  the same slot, established by the head CAS Acquire/Release pairing.
- **`initialised` publish gate** (`heap_slot.rs` / `tcache_hits_total` /
  `large_cache_hits_total`): correct Release-after-`write(hc)` / Acquire-before-
  read pattern; after W3 the aggregators read only slot-resident atomics and
  never materialize `&HeapCore` — the Stacked-Borrows foreign-read gap is
  closed.
- **H1/W3 hoists:** the remote-CASed `thread_free` word and the diagnostic
  counters live in `HeapSlotRemote` (own 64-byte line, `'static` address),
  outside every `&mut HeapCore` retag range; the exposed-provenance pairing
  (`expose_provenance` at stamp, `with_exposed_provenance_mut` at the remote
  reconstruction) is consistent at every site inspected.
- **Magazine (tcache) paths:** all `slots[c]` indexing is bounded by
  `cnt <= TCACHE_CAP` (the Э10 chunked double-free scan carefully avoids
  reading stale entries `>= cnt`); overflow half-flush arithmetic
  (`remaining + 1 = 9 ≤ 16`) is in-bounds; the refill split-borrow invariant
  (`count[c] == 0` during class `c`'s own refill) holds because refill runs
  only on a miss.
- **`bump_count` rollback race:** `fetch_add` returns unique indices; the
  best-effort `fetch_sub` can transiently over/under-shoot only above
  `MAX_HEAPS`, never re-issuing a valid index; readers clamp with
  `.min(MAX_HEAPS)`.
- **`HeapSlot: Sync` (no `Send`)** proof and the M6/M7 visibility narrowing are
  consistent with the claim-CAS single-writer protocol.
- **`recycle` double-recycle guard** (CAS LIVE→FREE, no push on failure) is
  correct; a slot cannot be double-pushed onto `free_slots` via this path.
- **Known, deliberately-pinned residual:** the cross-thread
  re-issue-before-drain double-free window (ring entry vs. re-issued block) is
  documented, RED-pinned (`residual_xthread_double_free_no_corruption`,
  `#[ignore]`d) and scheduled as task X7 — not re-reported here.

## Summary

| # | Severity | File | Issue |
|---|----------|------|-------|
| 1 | High | heap_registry.rs / tagged_ptr.rs | `free_slots` ABA tag resets to 0 on empty → stale CAS → slot double-claim / stack truncation |
| 2 | Medium (latent) | heap_registry.rs / bootstrap.rs | Same tag-reset in `abandoned_segs` (+ only 22 tag bits) |
| 3 | Medium (latent) | heap_registry.rs | OOM'd first claim leaves `gen==1`/uninit slot; re-claim gate is `new_gen==1`, not `initialised` |
| 4 | Low | heap_core.rs | Foreign-dealloc header read on possibly-unmapped segment (documented residual) |
| 5 | Medium (latent) | heap_registry.rs / heap_core.rs | Shared `next_abandoned` link between two stacks (documented reactivation hazard) |
| 6 | Low | heap_registry.rs | Unbounded recursion in `claim` retry |
| 7 | Low | tagged_ptr.rs | Stale doc describing removed 32-bit pointer packing |
