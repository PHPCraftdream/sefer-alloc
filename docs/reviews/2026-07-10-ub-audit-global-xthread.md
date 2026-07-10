# UB / Memory-Safety Audit — GlobalAlloc boundary & cross-thread free path

**Date:** 2026-07-10
**Scope:** `src/global/` (`SeferAlloc`, `tls_heap`, `fallback`), `src/registry/`
(`heap_registry`, `heap_slot`, `heap_core`, `bootstrap`), cross-thread free
mechanism (`remote_free_ring`, `deferred_large`), alloc→free→reuse lifecycle
across threads. Read-only audit (Read/Glob/Grep), no code executed.

**Headline:** no Critical finding. No new UB, use-after-free, double-issue,
or data race was found on any path reachable by a *contract-conforming*
`GlobalAlloc` caller. The verified-sound areas are listed at the end. The
findings below are resource/availability defects (unbounded or permanent
retention, slot leaks) and one robustness fragility in the registry claim
protocol, plus two documented residuals restated with their exact trigger
conditions.

---

## Finding 1 — Unbounded retention of cross-thread-freed Large segments when the owner stops allocating Large

- **File:** `src/registry/heap_core.rs` lines 594–603 (drain gate in
  `HeapCore::alloc`), lines 1202–1211 (realloc drain gate);
  `src/alloc_core/deferred_large/drain.rs` line 47.
- **Severity:** Medium (resource exhaustion; no UB).
- **Issue:** the per-heap deferred-free Treiber stack of cross-thread-freed
  Large segments (`HeapSlot::thread_free`) is drained ONLY when the owning
  thread performs a Large-classified `alloc`/`realloc`
  (`if class.is_none() { self.drain_large_deferred_free(); }`). There is no
  drain on the small-alloc path, no periodic drain, and no drain at thread
  exit (the `AbandonGuard` releases the slot only; the next claimant drains
  only on ITS first Large alloc).
- **Scenario:** thread A allocates N large buffers (e.g. 1000 × 4 MiB) in a
  startup phase, hands them to worker threads, then switches to small-only
  work for the rest of its life. Workers free all N buffers → all N segments
  are queued on A's deferred stack. A never issues another Large request →
  4+ GiB of mapped, fully dead segments (plus their `SegmentTable` slots)
  are retained indefinitely. Same holds after A exits if no new thread claims
  the slot and allocates Large. This is unbounded relative to live data, not
  the documented "bounded ring leak".
- **Fix direction:** drain the deferred-large stack opportunistically on the
  *small* alloc slow path too (e.g. in `refill_magazine_slow` /
  `find_segment_with_free`, where a cold branch already exists), and/or in
  `AbandonGuard::drop` before `recycle` (the owner is still the sole consumer
  at that point). A cheap `head.load(Relaxed) != null` pre-check keeps the
  hot path unaffected.

## Finding 2 — `finish_bind` can claim a registry slot without arming the `AbandonGuard` → permanent LIVE-slot leak

- **File:** `src/global/tls_heap.rs` lines 436–449 (`finish_bind`),
  specifically `let _ = GUARD.try_with(|g| g.heap.set(heap));` (line 447)
  and `let _ = LOCAL.try_with(|c| c.set(heap));` (line 446).
- **Severity:** Medium (availability / resource leak; no UB).
- **Issue:** both TLS publications swallow `Err`. Two failure legs:
  1. `GUARD.try_with` fails (first access to a destructor-carrying TLS during
     the thread's TLS-destruction phase can return `Err` on some platforms —
     e.g. another thread-local's `Drop` performs the thread's *first*
     allocation during teardown). The slot was already claimed
     (`HeapRegistry::claim` succeeded) but the guard is never armed → the
     slot stays `LIVE` forever, never recycled.
  2. `LOCAL.try_with` fails but the claim succeeded: `LOCAL` stays null, so
     EVERY subsequent allocation on this dying thread re-enters `bind_slow`
     and claims ANOTHER slot, overwriting `GUARD.heap` each time — every slot
     but the last is leaked `LIVE`.
- **Scenario:** a process that churns threads whose only allocations happen
  inside TLS destructors (logging/metrics flush in `Drop`) leaks one (or
  more) of the 4096 registry slots per such thread; after exhaustion, all new
  threads route through the spinlocked fallback heap — correct but heavily
  serialised. No memory-safety violation (the leaked `HeapCore` is simply
  never touched again).
- **Fix direction:** in `finish_bind`, arm the guard FIRST and treat
  `GUARD.try_with == Err` as "cannot own a slot on this thread": recycle the
  just-claimed slot and return `CurrentHeap::Fallback`. Similarly, if
  `LOCAL.try_with` fails, recycle and fall back (there is no point owning a
  slot the fast path can never see).

## Finding 3 — `claim`'s materialisation gate is `new_gen == 1`, and the OOM branch leaks the slot to keep that gate sound

- **File:** `src/registry/heap_registry.rs` lines 83–98 (`claim`) and
  143–158 (`claim_with_config`).
- **Severity:** Low today (leak + latent fragility), but the fragility is
  one refactor away from a Critical uninitialised-memory deref.
- **Issue:** two coupled points.
  1. On primordial OOM (`HeapCore::new` → `None`) the slot is CASed back
     `LIVE→FREE` but is NOT pushed onto `free_slots` — the index is lost
     forever (it also cannot be re-minted: `bump_count` is monotonic). One
     registry slot leaks per bootstrap OOM.
  2. That leak is silently *load-bearing*: the slot's `generation` was
     already bumped to 1 while `heap` is still `MaybeUninit::uninit()`. If a
     future "fix" naively re-pushed the slot onto `free_slots`, a later
     `claim` would pop it, see `new_gen == 2 != 1`, SKIP materialisation, and
     hand the caller a `*mut HeapCore` over uninitialised bytes —
     uninitialised-memory reads / wild-pointer UB on the very next alloc.
     Nothing in the code or comments pins this trap.
- **Scenario (for the latent leg):** any refactor that recycles the OOM'd
  slot, or that reorders the `generation.fetch_add` after a failed
  materialisation, converts a rare OOM into instant UB for an unrelated
  thread.
- **Fix direction:** gate materialisation on the already-existing
  `slot.initialised` flag (`if !slot.initialised.load(Acquire)`) instead of
  `new_gen == 1` — it is precisely the "HeapCore is materialised" predicate
  and is never reset. Then the OOM branch can safely
  `push_free_slot(reg, idx)` (optionally rolling the generation back), fixing
  the leak too. Add a comment pinning the invariant either way.

## Finding 4 — Cross-thread frees into fallback-owned segments are effectively never reclaimed

- **File:** `src/global/fallback.rs` (module design; `heap_ptr` /
  `with_heap`), interaction with `src/registry/heap_core.rs`
  `dealloc_foreign_slow` (lines 1376–1459).
- **Severity:** Low (bounded-by-usage retention; no UB).
- **Issue:** blocks allocated from the fallback heap (pre-TLS window,
  registry exhaustion, teardown) and later freed by normal threads route
  correctly into the fallback's per-segment `RemoteFreeRing`s /
  `FALLBACK_TFS` deferred stack — but those are drained only when the
  *fallback heap itself* next allocates (under its spinlock), which for most
  processes is never after startup. Small rings additionally cap at 256
  entries per segment, after which every further cross-thread free of a
  fallback block is discarded (`DBG_RING_OVERFLOW` bounded leak).
- **Scenario:** early-init allocations (before first TLS bind) served by the
  fallback, handed to worker threads and freed there, are retained for the
  process lifetime.
- **Fix direction:** acceptable by design (document as such), or add a rare
  drain hook — e.g. `stats()` or a periodic maintenance entry that takes the
  fallback spinlock and drains its rings/deferred stack.

## Finding 5 — Documented residual: cross-thread double-free "re-issue-before-drain" leg can double-issue a block (non-`hardened` builds)

- **File:** `src/registry/heap_core.rs` lines 963–1003 (RESIDUAL M2 LIMIT
  comment); `src/alloc_core/alloc_core_small.rs` lines 131–156 (the
  `hardened`-only generational guard that closes it).
- **Severity:** Medium impact, but triggered only by caller UB (double
  `dealloc` of the same pointer from another thread) — recorded here for
  completeness, already pinned RED by
  `residual_xthread_double_free_no_corruption` (`#[ignore]`d).
- **Issue:** a stale `(off, class)` ring entry for a block that was already
  reclaimed and *re-issued* to a new user is information-theoretically
  indistinguishable from a genuine delayed cross-thread free. The owner's
  drain relinks the block onto the free list while it is live → the same
  address is handed out twice → silent aliasing/corruption. `hardened`
  narrows it to the 1/256 generation-wrap residual; production (non-hardened)
  has no guard.
- **Fix direction:** as planned (X7): consider promoting the generational
  ring entry to the production feature set, or at least documenting in
  `INTEGRATION.md` that `hardened` is the mitigation for hostile/buggy
  double-free workloads.

## Finding 6 — Documented residual: `dealloc` of a pointer into an already-released segment reads a potentially unmapped header

- **File:** `src/registry/heap_core.rs` line 1376
  (`SegmentHeader::magic_at(base)` in `dealloc_foreign_slow`); rationale at
  lines 1347–1367; also `src/global/sefer_alloc.rs` lines 385–394 (SAFETY
  note).
- **Severity:** Low (inherent to every allocator; caller UB by the
  `GlobalAlloc` contract; correctly documented).
- **Issue:** for `contains_base == false`, the code cannot O(1)-distinguish
  "live segment owned by another heap" from "segment already released to the
  OS". `magic_at(base)` on the latter faults (or, if the address range was
  re-mapped by unrelated code, reads garbage — the magic check then usually
  rejects it, and the #138 `large_layout_consistent` check further narrows
  the Large leg). No action required; noted so the residual stays visible.
- **Fix direction (optional hardening):** a process-global segment-address
  filter (e.g. a lossy bitmap of ever-reserved 4 MiB-aligned bases) would
  convert most wild frees into no-ops without a header read.

## Finding 7 — Note: untagged `current()` hands out the shared fallback pointer without enforcing the spinlock obligation

- **File:** `src/global/tls_heap.rs` lines 266–293 (`current`, `pub`,
  currently `#[allow(dead_code)]`).
- **Severity:** Low (API hazard, not a live defect — the alloc face uses the
  tagged `current_for_alloc`).
- **Issue:** `current()` erases the Own/Fallback distinction; a future direct
  API consumer that mutates through the returned pointer on the TORN/Err
  branches would create an unsynchronised second `&mut HeapCore` over the
  fallback (data race). The doc comment states the obligation, but the type
  does not enforce it.
- **Fix direction:** keep it `pub(crate)` or return `CurrentHeap` here too;
  at minimum mark it `#[doc(hidden)]` until a safe wrapper exists.

---

## Areas examined and found sound

- **`SeferAlloc` (GlobalAlloc impl, `src/global/sefer_alloc.rs`):** each
  entry resolves the heap exactly once; Own-path `&mut` deref is justified by
  the single-writer slot invariant; Fallback path is fully spinlock-guarded
  (`with_heap`, panic-safe RAII `LockGuard`); null/OOM handling never panics.
- **TLS teardown (`tls_heap.rs`):** the TORN-sentinel protocol is correct —
  `mark_local_torn` before `recycle`, monotone TLS accessibility argument
  (a)/(b)/(c) holds; the two-sentinel one-branch compare
  (`addr().wrapping_sub(1) < usize::MAX - 1`) correctly separates
  null/TORN/real. No stale `*mut HeapCore` can be dereferenced after the
  slot is recycled.
- **Registry claim/recycle (`heap_registry.rs`, `heap_slot.rs`):** the
  FREE↔LIVE CAS protocol, W7a 48-bit-tagged `free_slots` Treiber stack (ABA
  defended), and the recycle→push→pop→claim Release/Acquire chain correctly
  transfer happens-before, so a re-claiming thread observes ALL of the prior
  owner's non-atomic `HeapCore`/magazine/BinTable state. The `initialised`
  Release/Acquire publish gate correctly protects the diagnostic aggregators
  from the mid-materialisation `MaybeUninit` window. RAD-1 lazy `next_free`
  is sound: the field is read only for a stack head, and every head was
  published by a `push_free_slot` that wrote `next_free` (Release) first.
  `bump_count` cannot double-mint an index (old value ≥ #successful mints).
- **`RemoteFreeRing` (MPSC):** full-check-then-CAS reservation cannot
  overfill (head is monotone, so a stale head only under-estimates free
  space); the publish/drain Release/Acquire pairing plus the EMPTY-slot stop
  rule handles the reserved-but-unpublished window; slot clear (Relaxed) is
  ordered before the next producer's write via the head-store(Release) /
  head-load(Acquire) edge; wrap-correct `h != t` with power-of-two CAP
  pinned by const-assert; consumer-identity transfer across slot recycle is
  fenced by the registry handshake (justifying the Relaxed head load).
- **Deferred-large Treiber stack (`deferred_large/push.rs`, `drain.rs`):**
  the `ABANDONED_TAIL`→link claim CAS (once, outside the head-retry loop)
  correctly prevents double-push self-loops (double-free of the same base is
  a no-op); pop is single-consumer so the classic Treiber pop-ABA
  (dereferencing a freed node's link) cannot occur — a queued head segment
  stays mapped until the owner itself pops it; the re-push-of-a-reclaimed-
  base interleaving leaves a consistent chain. Exposed-provenance
  store/load pairing is complete on both stacks.
- **Header field discipline (`segment_header.rs`):** cross-thread reads
  (`magic_at`, `kind_at`, `owner_thread_free_at`, `large_size_at`) touch only
  write-once-before-first-pointer-escape fields via `offset_of!` — disjoint
  from the owner's `bump` writes; the plain `owner_thread_free` write in
  `stamp_segment_owner` happens-before any remote read of it through the
  user's pointer-handoff synchronisation (the first pointer of a segment
  escapes only after the stamp). H1/W3 hoists (slot-resident `thread_free`,
  `tcache_hits`, `large_cache_hits` + `HeapSlotRemote` align(64)) correctly
  remove every remotely-touched word from the owner's `&mut HeapCore` retag
  range; `expose_provenance` at the stamp site pairs with
  `Node::atomic_ptr_ref`'s reconstruction.
- **M2 own-thread double-free guards:** magazine scan (exact-`cnt` bound —
  no stale-slot reads) + bitmap oracle order is correct; the
  `alloc-decommit` `off >= bump` stale-free guard is present symmetrically in
  `dealloc_own_thread_with_base`, `reclaim_offset`, and
  `reclaim_offset_checked`; `reclaim_offset*` bounds-check `class_idx`
  before the table index (no-panic discipline).
- **Fallback bootstrap (`fallback.rs`) and registry bootstrap
  (`bootstrap.rs`):** both hand-rolled init state machines are
  publish-correct (Release store after full construction, Acquire on every
  read), OOM-rollback prevents livelock (task #131), and `FALLBACK_TFS` is
  bound before the READY publish, so no unstamped-fallback window exists.
- **`AllocCore::dealloc` foreign-pointer guard:** `contains_base` (own-table
  O(1) hash, never touches the foreign bytes) is checked before any header
  read on both the own-thread and routing paths — a cross-thread free
  without `alloc-xthread` degrades to the documented leak, never a foreign
  BinTable write.

*Audited by claude-fable-5 (effort=max), read-only pass over the working
tree as of 2026-07-10 (includes the uncommitted `alloc_core` file split).*
