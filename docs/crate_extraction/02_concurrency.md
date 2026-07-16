# Crate extraction — lane 2: lock-free / cross-thread concurrency primitives

**Research question:** which loom-verified concurrency primitives in
`sefer-alloc` could/should be extracted into standalone community crates?

**Method:** read of `src/alloc_core/remote_free_ring.rs`,
`src/registry/{heap_overflow,heap_slot,heap_registry,bootstrap,tagged_ptr,
registry_chunk,heap_core_xthread}.rs`, `src/alloc_core/deferred_large/*`,
`src/alloc_core/segment_directory.rs`, `src/concurrent/*`, all 18
`tests/loom_*.rs` models, `docs/CROSS_THREAD_STATE_MACHINES.md`.

**Extraction precedent already exists in-repo:** `crates/` already hosts
single-file seam crates (`aligned-vmem`, `sefer-region`, `numa`,
`malloc-bench`) — the workspace pattern for a new crate is established.

**One overarching observation (applies to every candidate).** Every loom
harness in `tests/` is a *shadow model*: it re-transcribes the protocol with
`loom::sync::atomic`, it does **not** compile the production type under
`cfg(loom)`. That is a deliberate in-tree compromise (the real types sit on
raw segment memory / OS reservations that loom cannot host), but it carries a
standing drift risk — the model can silently diverge from the code it vouches
for. Extraction inverts this: a standalone crate can alias its atomics
(`#[cfg(loom)] use loom::sync::atomic; #[cfg(not(loom))] use
core::sync::atomic;`) so **the shipped loom tests exercise the real
implementation**, not a transcription. That is the single biggest testability
gain of extraction, and it is also the community pitch: lock-free crates that
ship *executable* loom proofs (with `#[should_panic]` counterfactuals proving
the harnesses non-vacuous) are genuinely rare on crates.io.

---

## Candidate 1 — bounded MPSC index ring (Vyukov-style, CAS-reserve / single-consumer drain)

**What it is / files.** A bounded MPSC ring of small `Copy` payloads with a
sentinel "empty" value: monotonic wrapping `tail` (producers CAS-reserve) and
`head` (consumer advances), publish-after-reserve, drain that **stops at the
first reserved-but-unpublished slot** ("later drain picks it up"), overflow →
`Err` (caller-defined bounded-loss policy), and a Relaxed-load *drain-guard*
(`tail_relaxed()` vs. an owner-cached head, sound by cursor monotonicity).
Two production instances of the same protocol:

- `src/alloc_core/remote_free_ring.rs` — `RemoteFreeRing`, single-`u32`-entry
  ring carved over raw segment metadata (256 slots, cache-line-separated
  cursor blocks, power-of-two-CAP wrap pin, packed `(offset, class)`
  entries).
- `src/registry/heap_overflow.rs` — `HeapOverflow`, the **two-field-entry**
  variant (`base: AtomicUsize` + `packed: AtomicU32` per slot; publish
  `packed` Relaxed then `base` Release; drain gates on `base` Acquire — the
  Release-sequence pair-publish idiom), plus the R2-4 "drain must return its
  ACTUAL stop position, not the entry-time tail snapshot" contract.

**Loom models (5):** `tests/loom_remote_ring.rs` (exactly-once, two
counterfactuals: skip-unpublished-slot and no-clear-on-drain);
`tests/loom_remote_ring_drain_guard.rs` (Relaxed empty-guard + cached-head
liveness, slot-reuse boundary); `tests/loom_heap_overflow.rs` (two-field torn-
read counterfactual); `tests/loom_heap_overflow_drain_guard.rs` (the R2-4
return-value bug as a counterfactual); `tests/loom_overflow_first_retry.rs`
(two-ring composed retry policy, double-saturation, no-loss/no-dup). Plus
non-loom isolation tests (`tests/remote_ring_unit.rs`,
`tests/miri_heap_overflow_unit.rs`) and a u32-cursor-wrap test.

**Coupling.** The *protocol* is fully general; the *instances* are coupled:
`RemoteFreeRing` views raw bytes via the `Node` seam and packs
allocator-specific `(offset, class)` words; `HeapOverflow` embeds the lazy
sidecar tier (candidate 2) and `INLINE_CAP`/miri-cfg sizing. But
`HeapOverflow` itself demonstrates the protocol works over plain safe-Rust
atomic arrays — that is the extraction shape.

**Extraction effort: LOW–MEDIUM.** Generalize to
`MpscRing<T: RingEntry, const CAP: usize>` where `RingEntry` supplies the
sentinel and the (one- or two-word) publish/read protocol; provide (a) an
owned-array constructor (safe, what `HeapOverflow` does) and (b) an `unsafe
fn over_raw(base: *mut u8)` in-place view (what `RemoteFreeRing` does — the
`over_test_buffer` test surface already prototypes exactly this API and its
`# Safety` contract). Keep `drain() -> cursor` (the guard-cache contract) and
`try_push_uncounted`. Drop allocator-specific packing (offer `u32` payload +
a `(usize, u32)` pair payload).

**Testability gain.** All 5 loom models plus both counterfactual families
ship with the crate and run against the *real* type. The R2-4 and
torn-pair-publish regressions become permanent, executable documentation.

**Community value: HIGH.** There is no well-known crate offering a bounded
MPSC ring that is (1) allocation-free, (2) usable over caller-supplied raw
memory (in-metadata rings for allocators, shared-memory IPC, embedded), (3)
explicit about the reserved-but-unpublished drain semantics and the
overflow-as-bounded-loss policy, and (4) shipped with loom proofs.
`heapless::mpmc` and `crossbeam` queues cover neither (2) nor (4).

**Suggested crate:** `ring-mpsc` (or `vyukov-index-ring`).
```text
let ring: MpscRing<u32, 256> = MpscRing::new();          // owned storage
ring.push(off)?;                                          // Err(Full) => caller policy
let stop = ring.drain(|off| reclaim(off));                // returns new head
if ring.tail_relaxed() != cached { /* real drain */ }     // guard idiom
let view = unsafe { MpscRing::over_raw(base) };           // in-place tier
```

---

## Candidate 2 — lazy CAS-published cell ("racy once-pointer" with OOM rollback)

**What it is / files.** The `UNINIT → INITIALIZING(sentinel=1) → READY`
`AtomicPtr` state machine: CAS null→sentinel; winner allocates via a direct
OS call, in-place-inits, publishes with Release, leaks for process lifetime;
losers spin-Acquire until non-null/non-sentinel; on winner OOM the sentinel
is **rolled back to null** and losers *re-race the CAS* instead of spinning
on a READY that never comes. Four production instances of one protocol:

- `src/registry/bootstrap.rs` — whole-registry (historic), per-chunk
  `ensure_chunk`/`ensure_chunk_slow` (`Registry::chunks:
  [AtomicPtr<RegistryChunk>; 64]`), and per-ring
  `ensure_overflow_sidecar`/`deref_overflow_sidecar` (the `HeapOverflow`
  sidecar; incl. the wedge-hazard "ensure sidecar BEFORE winning the tail
  CAS" ordering).
- `src/global/fallback.rs::heap_ptr` — the state-word variant with the
  rollback/re-race liveness fix (Phase F1: loser spins only `==
  INITIALIZING`, not `!= READY`).

**Loom models (4):** `tests/loom_bootstrap_cas.rs`, `tests/loom_chunk_cas.rs`,
`tests/loom_overflow_sidecar_cas.rs` (exactly-once allocation, same pointer
for all, no sentinel/null leak, Release/Acquire happens-before — each with a
Relaxed-publish counterfactual), `tests/loom_fallback_init.rs` (rollback
liveness: no thread spins forever after winner OOM). The repo itself notes
these three files are "the same protocol transcribed three times" — a crate
is the correct deduplication.

**Coupling: LOW.** The protocol is generic over "a fallible allocation
returning a pointer". The only dependency is *what* allocates
(`aligned_vmem::reserve_aligned` here) — take it as a closure.

**Extraction effort: LOW.** ~100 lines.
The key differentiators vs. `once_cell`/`std::sync::OnceLock`:
allocation-free and `no_std`, **safe inside a `#[global_allocator]`**
(no std sync primitives, no reentrancy), *fallible* init with **rollback +
re-race** (OnceLock poisons/blocks; this retries), and pointer-sentinel
encoding (3 states in one word). Must document the spin-wait (no parking).

**Testability gain.** The 4 loom models collapse into one parameterized suite
run against the real type; the Relaxed-publish and spin-on-READY-livelock
counterfactuals ship as `#[should_panic]` proofs.

**Community value: HIGH.** Every hand-rolled allocator, runtime, and
bare-metal bootstrap re-invents exactly this cell (and usually gets the OOM
rollback or the loser-spin condition wrong — this repo found both bugs the
hard way). Small, auditable, proof-carrying.

**Suggested crate:** `racy-ptr-cell` (or `cas-once`).
```text
static CELL: RacyPtrCell<Chunk> = RacyPtrCell::new();
let chunk: Option<&'static Chunk> =
    CELL.get_or_try_init(|| reserve_and_init() /* -> Option<NonNull<Chunk>> */);
```

---

## Candidate 3 — tagged index Treiber stack (ABA-guarded free-index list)

**What it is / files.** A lock-free LIFO free-list of *indices* (not
pointers): head is one `AtomicU64` packing `(index:16 | tag:48)`
(`src/registry/tagged_ptr.rs`), per-slot intrusive `next_free: AtomicU32`
links live in caller storage (`src/registry/heap_registry.rs`
`pop_free_slot`/`push_free_slot`); the tag bumps on every **push** (W7a) and
the empty transition preserves the running tag (the H-2 fix — resetting the
tag on empty reopens ABA). Includes the lazy `next_free` init discipline
(RAD-1: never eagerly written, so OS-zeroed backing pages are never
first-touched).

**Loom model:** `tests/loom_free_slots_aba.rs` (680 lines) — the classic
pop/repush ABA interleaving with the tag forcing the stale CAS to fail, plus
an *untagged counterfactual* where loom finds real free-list corruption. A
wrap counterfactual (`tests/regression_counter_wrap.rs`) pins the 16/48 split
across tag wrap.

**Coupling: LOW–MEDIUM.** `TaggedPtr` is pure bit arithmetic (zero coupling;
strict-provenance-clean by construction since it packs indices, never
addresses). The pop/push functions touch `HeapSlot.next_free` — generalize
via a links trait or by owning an `[AtomicU32; N]` link array.

**Extraction effort: LOW–MEDIUM.** Generic over index width
(`const INDEX_BITS`) with the compile-time sentinel/capacity pins this repo
already has. The H-2 empty-tag subtlety and the 89-year wrap-bound analysis
move into the crate docs.

**Testability gain.** The ABA model + untagged counterfactual + tag-wrap test
ship with the crate, against the real code.

**Community value: MEDIUM–HIGH.** Index-based free-lists are the backbone of
slab allocators, object pools, ECS storages, and connection tables — and ABA
tagging is the part people get wrong. An allocation-free, `no_std`,
loom-proven "free-index stack" fills a real gap (crates like `sharded-slab`
embed one privately; none ship it as a primitive with proofs).

**Suggested crate:** `tagged-index-stack`.
```text
let stack = IndexStack::<16 /* index bits */>::new();     // links owned or via trait
stack.push(idx);                                          // tag bump on push
let idx: Option<u16> = stack.pop();
```

---

## Candidate 4 — lost-wakeup-safe dirty-bitmap router (R7-A4)

**What it is / files.** A publish-then-mark wakeup bitmap: producers publish
a payload into a per-key channel (a candidate-1 ring), **then**
`fetch_or(bit, Release)` into a shared `[AtomicU64; 16]` dirty word array
(`HeapSlot::dirty_segments`, `src/registry/heap_slot.rs:205-240`; producer
side `set_dirty_bit_for_segment`, `src/registry/heap_core_xthread.rs:285-328`);
the consumer `swap(0, Acquire)`s each word and drains exactly the set bits'
channels (`drain_dirty_segments`, reached from
`src/alloc_core/alloc_core_small.rs`). The documented contract: an entry
whose producer stalls between ring-publish and `fetch_or` is *boundedly
deferred*, never lost — found by a later bit-set, another producer's drain of
the same channel, or the unconditional linear-scan fallback (the three-path
argument in `remote_free_ring.rs` lines 114–139).

**Loom models (2):** `tests/loom_dirty_publish.rs` (2 producers publish→mark,
1 consumer swap→drain; no entry permanently invisible),
`tests/loom_dirty_multi_segment.rs` (multiple keys packed in ONE word; a
swap(0) must observe both bits).

**Coupling: MEDIUM.** The bitmap + swap/fetch_or protocol is general. BUT the
*completeness* story leans on an allocator-specific backstop: the guarded
linear scan that unconditionally drains every ring on directory miss. An
extracted crate must be honest that it provides "at-least-once wakeup
routing with bounded deferral", and that a user needs either (a) tolerance
for deferral until the next mark, or (b) their own periodic full sweep. That
is still a coherent, useful contract (it is exactly how sparse epoll-style
ready-lists and mimalloc-style deferred queues behave), but it is a contract,
not magic.

**Extraction effort: MEDIUM.** The mechanism is ~50 lines; the work is
API-shaping (`mark(key)`, `for_each_dirty(|key| ...)` via per-word swap) and
writing the deferral contract precisely. Natural fit as a *module of the
candidate-1 ring crate* (router + ring compose into "many cheap channels,
O(dirty) drain") rather than a standalone crate.

**Testability gain.** Both loom models ship; the lost-wakeup interleaving
(mark lands after the swap) is the exact hazard loom enumerates.

**Community value: MEDIUM.** Useful and rarely packaged (a lock-free
"ready-set" over N channels), but the bounded-deferral caveat narrows the
audience vs. candidates 1–2.

**Suggested crate:** ship inside `ring-mpsc` as `DirtyRouter<const WORDS>`;
standalone name if split: `dirty-bits`.
```text
router.mark(key);                              // producer: AFTER channel publish
router.for_each_dirty(|key| drain_channel(key)); // consumer: swap(0, Acquire) per word
```

---

## Candidate 5 — intrusive MPSC Treiber stack with idempotent (double-push-guarded) push

**What it is / files.** `src/alloc_core/deferred_large/{push,drain,tail}.rs`:
a Treiber stack whose link word lives *inside* the node
(`SegmentHeader::deferred_next: AtomicU64`), with two sentinels
(`ABANDONED_TAIL` = "not on any stack", `DEFERRED_LARGE_TAIL` =
"bottom of THIS stack") and the A1 hardening: push first **claims the link
word** via `compare_exchange(ABANDONED_TAIL → next)` before contesting
`head`, so a racing second push of the same node is a detected no-op — a
lock-free *idempotent* push that degrades a double-free into a no-op instead
of a UAF/double-unmap.

**Loom model:** `tests/loom_deferred_large.rs` (exact protocol shape incl.
both sentinels, retry-on-lost-head-CAS via plain store retarget; properties:
no lost nodes, double-push extracted once, no deadlock). Related generic
shape: `tests/loom_thread_free.rs` (retained model of the superseded
intrusive block stack; its naive-push counterfactual is the generic Treiber
demonstration).

**Coupling: MEDIUM–HIGH.** The production code stores raw *addresses* in
`AtomicU64` (exposed-provenance class, documented in `bootstrap.rs`) and the
link word doubles as an allocator lifecycle field. Extraction requires
switching to `AtomicPtr` + a node trait (`fn link(&self) -> &AtomicPtr<..>`),
losing the address-reuse trickery.

**Extraction effort: MEDIUM.** Protocol is small; the provenance rework and
the "who may own a node's link word when" contract are the work.

**Community value: MEDIUM.** Intrusive MPSC stacks exist (e.g. in `heapless`,
`cordyceps`); the *idempotent-push / double-insert-guard* property with a
loom proof is the genuinely novel part and the honest pitch.

**Suggested crate:** `intrusive-once-stack`.
```text
match stack.push(node) { Pushed => .., AlreadyLinked => /* double-free no-op */ }
stack.drain(|node| ..);   // single consumer
```

---

## Candidate 6 — generation-checked slot (seqlock publication + generation-CAS eviction)

**What it is / files.** `src/concurrent/hand.rs` `AtomicSlot<T>`: a
`(generation, crossbeam_epoch::Atomic<T>)` pair; readers use a seqlock
(gen → value → gen re-check against a handle-baked expected generation);
remote eviction is `compare_exchange(expected_gen → next)` — the CAS is the
linearization point preventing a stale handle from destroying a
newer-generation value. `EpochRegion`/`ShardedRegion`
(`src/concurrent/{epoch_region,sharded_region}.rs`) are safe compositions on
top.

**Loom models (2):** `tests/loom_epoch.rs` (seqlock never resolves a value of
another generation), `tests/loom_sharded.rs` (stale remover never destroys a
newer value; naive load-then-swap counterfactual).

**Coupling / honesty.** Depends on `crossbeam-epoch` for reclamation (miri,
not loom, covers lifetimes — and crossbeam-epoch 0.9.18 itself is not
miri-clean upstream). Crucially, this whole tier is **`experimental`,
`#[deprecated]`, "legacy/research-tier, superseded"** in-repo
(`sharded_region.rs` header). Extraction would be resurrecting code the
project itself retired.

**Verdict:** extract only if there is external demand for a
"generation-handle slot map" primitive; otherwise leave. If extracted:
`gen-slot`, `slot.read_with(gen, guard)` / `slot.try_evict_at(gen, guard)`.
Effort LOW (already self-contained), value LOW–MEDIUM.

---

## Honestly NOT extractable (allocator-specific protocols)

- **`tests/loom_xthread_protocol.rs` / `docs/CROSS_THREAD_STATE_MACHINES.md`**
  — a *specification model* (SM-BLOCK/SM-CHANNEL, invariant I-BLOCK-1:
  never LIVE ∧ free-listed). It verifies the allocator's ownership
  discipline, not a reusable data structure. Its value transfers as
  *methodology* (write the state machine, model-check it), not as a crate.
- **`tests/loom_magazine_ring_compose.rs`** — the magazine/BinTable/ring
  three-resting-places composition and the Э6 two-oracle double-free guard.
  Entirely about sefer-alloc's tcache semantics.
- **`tests/loom_overflow_first_retry.rs`'s policy layer** — the
  ring→overflow→spin-retry→bounded-leak *ordering policy*
  (`HeapCore::push_with_overflow_retry`, incl. `head_relaxed()` progress
  detection) is allocator policy; only its two underlying rings (candidate 1)
  generalize. The model could still ship as a ring-crate *example* of
  composing two rings.
- **`SegmentDirectory`** (`src/alloc_core/segment_directory.rs`) — plain
  non-atomic owner-only bitmap; not a concurrency primitive at all (the
  atomic sibling is candidate 4).
- **Owner-stamping / slot recycle→claim handshake**
  (`heap_core_ownership.rs`, `heap_registry.rs` claim path) — the
  "consumer identity moves with slot ownership" fencing argument
  (`RemoteFreeRing::drain`'s Relaxed-head justification) is deeply tied to
  the registry lifecycle; it survives extraction only as documentation of
  what the ring's single-consumer requirement means.

---

## Ranked shortlist

| # | Crate | Sources | Loom proofs shipped | Effort | Value |
|---|-------|---------|--------------------:|--------|-------|
| 1 | `ring-mpsc` — bounded MPSC index ring (+ drain-guard, + optional `DirtyRouter` module from candidate 4) | `remote_free_ring.rs`, `heap_overflow.rs` | 5 (+2 with router) models, 4 counterfactuals | LOW–MED | **HIGH** — allocation-free, raw-memory-capable, proof-carrying; no crates.io equivalent |
| 2 | `racy-ptr-cell` — lazy CAS-published pointer cell with OOM rollback + re-race | `registry/bootstrap.rs` (3 instances), `global/fallback.rs` | 4 models incl. livelock counterfactual | LOW | **HIGH** — the global-allocator-safe `OnceLock` niche; dedups 3 in-repo copies |
| 3 | `tagged-index-stack` — ABA-tagged lock-free free-index list | `tagged_ptr.rs`, `heap_registry.rs` | 1 large model + untagged counterfactual + wrap test | LOW–MED | MED–HIGH — slab/pool building block |
| 4 | `dirty-bits` — lost-wakeup-safe dirty-bitmap router | `heap_slot.rs`, `heap_core_xthread.rs` | 2 models | MED | MED — best shipped inside #1 |
| 5 | `intrusive-once-stack` — idempotent-push intrusive MPSC stack | `deferred_large/*` | 1 model (+ generic Treiber demo) | MED | MED — the double-free-guard is the novelty |
| 6 | `gen-slot` — generation-CAS slot (deprecated tier) | `concurrent/hand.rs` | 2 models | LOW | LOW–MED — repo itself retired it |

**Recommended first move:** #1 and #2 together. They are the two protocols
the repo itself already reuses ≥3 times each (the strongest internal signal
of generality), their loom suites are the richest, and both close the
model-vs-production drift gap the moment the shipped loom tests run against
the real types.
