# Cross-thread free as a system of state machines (the spec)

**Status:** HISTORICAL design record (task #37). Written *before*
implementation, deliberately.

> **SUPERSEDED as the current protocol (2026-10-06, review R14-03).**
> Sections 0–8 below describe the OLD cross-thread design world — the
> per-segment offset ring channel, the `ABANDONED` segment state with
> adopters, and slot-incarnation collection rules. That world is **not**
> the shipped implementation. Everything here is retained only as a
> historical record of the design reasoning and the two witnessed races
> (§8 intrusive-word race, Phase-12.6 ring-ABA). It must NOT be read as
> the authoritative specification of the current cross-thread or
> terminal-publication protocol.
>
> **Current-source summary and dated design anchors:**
> - [`REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md`](REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md)
>   — design of the now-implemented independent sidecar/registration ingress
>   (supersedes the payload-intrusive ingress proposed in
>   [`REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`](REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md)).
> - [`LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md`](LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md)
>   — Large geometry, canonical identity and the four-value split
>   (reservation token / usable root / payload / address key).
> - [`INVARIANTS.md`](INVARIANTS.md), [`ARCHITECTURE.md`](ARCHITECTURE.md),
>   [`GLOSSARY.md`](GLOSSARY.md) — system-wide invariants and vocabulary.
>
> Section 9 (at the end, after historical §8) is the current state-machine
> summary, traced to source.

This document was the authoritative specification of a cross-thread-free
protocol *as of task #37*. Implementation has since moved on; the loom
model checked the historical machine; both the §8
intrusive-word race and the Phase-12.6 ring-ABA are shown below to be the **same
invariant violation**, so fixing the invariant fixes both — that unified
lesson still stands, even though the mechanism it was applied to has been
replaced.

Why this exists: Phases 8–12 drifted from the project's founding discipline
("dangerous memory → proven tools, don't improvise") into hand-rolled lock-free
machinery, because a `#[global_allocator]` cannot put a heap-allocating crate
(crossbeam-epoch, a concurrent queue) on its own path (reentrancy). When you
*must* hand-roll, the proven substitute is not "a cleverer data structure" —
it is **a written state machine with invariants you can model-check**. That is
what this is.

---

## 0. The actors and what they may touch *(HISTORICAL — pre-sidecar ring/ABANDONED model; see the SUPERSEDED notice above and §9 for the current protocol)*

| Actor | Identity | May write |
|---|---|---|
| **Owner** | the one thread bound to a heap slot, for one slot-lifetime | that heap's segments' `BinTable`s; collects their channels |
| **Remote** | any other thread freeing a block it holds | only an atomic *publish* into the block's segment channel |
| **Adopter** | a thread claiming an `ABANDONED` segment | the segment's `owner_state` via CAS, then becomes Owner |

The single discipline: **the Owner is the sole mutator of free-list state; the
only cross-thread write is a Remote's atomic publish into a channel.** Everything
below makes that precise and says what happens at the boundaries where the Owner
identity changes.

---

## 1. SM-BLOCK — the allocation atom *(HISTORICAL — §9 for the current path)*

Scope: one `(segment, offset)` *within a single segment incarnation* (see
SM-SEGMENT for "incarnation"). This is the machine whose invariant the bugs broke.

States:
- `UNCARVED` — the bump cursor has not reached this offset; not yet a block.
- `LIVE` — handed to the application.
- `LOCAL_FREE` — on the Owner's `BinTable` free list (free; Owner-only reachable).
- `REMOTE_FREED` — freed by a Remote; published into the segment channel; **not
  yet collected; NOT allocatable.**

Transitions (and the sole actor permitted to drive each):
```
UNCARVED   --carve (Owner)----------------> LIVE
LIVE       --dealloc by owner (Owner)-----> LOCAL_FREE
LIVE       --dealloc by non-owner (Remote)-> REMOTE_FREED   // the only x-thread edge
REMOTE_FREED --collect (Owner)-----------> LOCAL_FREE
LOCAL_FREE --alloc (Owner)----------------> LIVE
```

Invariants:
- **I-BLOCK-1 (mutual exclusion).** A block is in *exactly one* state at any
  instant. The crash signature (a free-list node holding live app data, `next`
  outside the segment) is a witnessed `LIVE ∧ LOCAL_FREE`.
- **I-BLOCK-2 (no resurrection).** For each block-life, its `REMOTE_FREED →
  LOCAL_FREE` (collect) must *happen-before* the next `LOCAL_FREE → LIVE`
  (alloc) of the same address. Equivalently: **a remote free queued for life N
  must be collected within the owner-context of life N — never applied to a
  later incarnation that reused the address.**
- **I-BLOCK-3 (single free per life).** Each life takes exactly one `LIVE →
  {LOCAL_FREE | REMOTE_FREED}` edge (no double free).

---

## 2. SM-SEGMENT — ownership / incarnation *(HISTORICAL — the `ABANDONED`/adoption edges are not the shipped path; §9)*

> **Hot-path reality (verified, task #37).** In the shipped Phase-12.5 shard
> model, `abandon_segments` is **not called on the hot path** — thread exit does
> `recycle` only and leaves the `HeapCore` whole (whole-heap reuse;
> `heap_registry.rs:234`). So on the hot path a segment is **continuously
> `OWNED`** by its slot; only the bound *thread* changes on recycle. The
> `OWNED→ABANDONED→OWNED(g+1)` edges below are the **adoption substrate**
> (loom-proven, retained for a future decommit-when-empty policy), NOT the
> stress-path. **The observed corruption is therefore a within-continuous-
> ownership violation** (Owner drain/reclaim/reuse racing a Remote publish),
> which is what the loom model in §6 targets first; the incarnation edges are
> modelled second, for the decommit policy.

Scope: one segment base. `owner_state` packs `(state, owner_id, generation)`.

States:
- `UNINIT`
- `OWNED(H, g)` — heap `H` is the sole `BinTable` writer and sole channel
  collector. `g` is the **incarnation number** (bumped on each adoption).
- `ABANDONED(g)` — the prior Owner released the slot/exited; **no live owner**,
  yet blocks may still be `LIVE` in app hands and Remotes may still publish.

Transitions:
```
UNINIT      --first alloc (Owner)----------> OWNED(H, g)
OWNED(H,g)  --abandon on exit (Owner)------> ABANDONED(g)
ABANDONED(g)--adopt, CAS (Adopter)---------> OWNED(H', g+1)
```

Invariants:
- **I-SEG-1 (single owner).** At most one `OWNED` owner; owner = sole free-list
  writer + sole collector.
- **I-SEG-2 (collector continuity / the boundary rule).** A segment's channel
  may be collected only by its *current* `OWNED` incarnation. Across an
  `OWNED → ABANDONED → OWNED(g+1)` boundary, a publish made for incarnation `g`
  must NOT be collected into incarnation `g+1`'s free list. This is I-BLOCK-2
  lifted to the segment.

---

## 3. SM-SLOT — registry HeapSlot *(HISTORICAL — the I-SLOT-1 incarnation rule below is a design claim of the ring era, NOT a rule of the current protocol: nothing in the current source forbids a valid pending foreign publication from a previous owner thread surviving whole-heap slot reuse; see §9)*

Scope: one registry slot index. `HeapCore` is materialised on first claim and
**inherited as-is** on later claims (see `HeapRegistry::claim`).

States: `FREE --claim(gen+1)--> LIVE(gen) --recycle--> FREE`.

Invariant:
- **I-SLOT-1.** The inherited `HeapCore`'s segment table must not let a new
  slot-lifetime collect channel entries that belong to a prior lifetime's
  blocks. (This is the concrete path by which I-BLOCK-2 was violated: producer
  threads exit, slots recycle, the inheritor drains channels of segments whose
  blocks were handed out in the previous lifetime.)

---

## 4. SM-CHANNEL — the cross-thread handoff (per segment) *(HISTORICAL — the ring channel itself was replaced by the sidecar/registration route, see §9)*

This is where representation is usually argued (intrusive word vs offset ring).
The state machine shows the argument is **secondary** — what matters is that a
channel entry is *bound to a block-life*, so it can never be applied to a
different life.

Abstract entry lifecycle: `EMPTY → PUBLISHED(block, life-epoch) → COLLECTED`.

- **Intrusive (mimalloc) representation:** the block *is* the node; "in channel"
  ⟺ `REMOTE_FREED`. A block in the channel is not allocatable; collect swaps the
  whole list to the Owner's `local_free`. I-BLOCK-2 holds *by construction* —
  you cannot alloc what is in the channel, and collect is the only path to
  `LOCAL_FREE`. **Provided** the boundary rule (I-SEG-2) seals/quiesces the
  channel at abandon so a post-boundary owner never reads a pre-boundary node.
- **Non-intrusive (our ring) representation:** the entry is an offset, *decoupled
  from the block's life*. An offset published in life N can be drained in life
  N+1 → I-BLOCK-2 violated. To restore the binding the entry must carry the
  **life-epoch** (the segment generation captured at the block's *alloc*),
  checked at collect against the segment's current generation.

### The unification (why this whole exercise pays off)

Both historical failures are **one** violation:

- **§8 intrusive-word race:** at slot reuse, the block's first word is contended
  between "Remote writing the channel-next (REMOTE_FREED for life N)" and "Owner
  reusing it (LIVE/LOCAL_FREE for life N+1)" → `LIVE ∧ REMOTE_FREED` =
  **I-BLOCK-1/2 broken at the boundary.**
- **Phase-12.6 ring-ABA:** an offset published in life N is collected in life
  N+1 after the block was re-carved → `LIVE ∧ LOCAL_FREE` =
  **I-BLOCK-1/2 broken at the boundary.**

Same invariant, same boundary. So the fix is **not** "intrusive vs ring"; it is
**enforcing I-BLOCK-2 / I-SEG-2 at the OWNED↔ABANDONED↔OWNED boundary.**

---

## 5. The boundary discipline (the actual fix surface) *(HISTORICAL)*

Exactly one of these must hold; both are valid, pick by cost:

- **(Q) Quiesce at abandon.** `OWNED → ABANDONED` may occur only after the Owner
  has drained its channel AND no block of this segment is still `LIVE` in app
  hands. If live blocks remain, the segment is not abandoned (it is retained/
  leaked-bounded until quiescent), so no Remote free can target a re-incarnated
  address. Simple; cost: retains segments with stragglers.
- **(E) Life-epoch on the entry.** Every channel publish carries the segment
  generation captured at the block's *alloc*. Collect applies an entry only if
  its epoch == the segment's current generation; otherwise the entry is a stale
  cross-incarnation free and is dropped (sound; bounded leak only at the
  boundary). Cost: wider entry (offset+gen) / a per-block alloc-epoch stamp.

Either makes I-BLOCK-2 hold. (E) keeps full reclaim within a lifetime; (Q)
trades some retention for a simpler channel. **Recommendation: model both in
loom, ship the one whose loom model is smaller and whose leak bound is
acceptable.** The decision is now a measured one, not a guess.

---

## 6. Verification plan (verification-first) *(HISTORICAL)*

1. Encode SM-BLOCK + SM-SEGMENT + SM-CHANNEL as a loom model over loom atomics
   (NOT the real allocator): a small number of blocks, 1 Owner that
   alloc/free/collect/abandons, 1 Adopter, ≥1 Remote that frees across the
   boundary. Assert I-BLOCK-1/2/3 and I-SEG-1/2 on every interleaving
   (`preemption_bound = 3`).
2. **Counterfactuals (non-vacuity):** the model WITHOUT the boundary discipline
   (no quiesce / no epoch check) must make loom find the `LIVE ∧ LOCAL_FREE`
   interleaving (`#[should_panic]`). This reproduces the §8/ABA bug *in the
   model*, proving the model has teeth and the discipline is what removes it.
3. Only then implement to match, and re-run `tests/race_repro.rs` (×5 under
   reclaim) + `tests/remote_ring_unit.rs` + the full gate.

---

## 7. What this replaces *(HISTORICAL)*

- The ad-hoc Variant-2 ring stays *only* as the channel representation **if** it
  carries the life-epoch (option E); otherwise it is replaced by the intrusive
  channel under the boundary rule (option Q). Either way the `generation-tag`
  idea is no longer a "crutch bolted on" — it is `I-SEG-2` made executable, with
  a loom proof.
- The transient subtract-overflow guards / diagnostic probes are deleted (they
  masked I-BLOCK-1 instead of preventing it).

---

## 8. Open questions for the implementation phase *(HISTORICAL — answered by the shipped sidecar/registration design; see §9)*

- (E) needs the block's alloc-epoch at *collect* time. Where is it stored — in
  the channel entry (widen to u64 `offset|gen`), or read from the segment
  generation and compared to a per-block stamp? The loom model decides.
- (Q) needs "no block of this segment still LIVE" — a per-segment live-count, or
  the existing bump/free accounting. Cheap to add; confirm it is M5-clean.
- Interaction with M11 epoch-guard (#35) for M6 decommit: the same segment
  generation should serve both (decommit-safety and collect-safety are the same
  "don't touch a re-incarnated address" property). Unify, don't duplicate.

---

## 9. CURRENT authoritative anchor — the shipped cross-thread protocol (2026-10-06)

This section describes current mechanisms traced against live source. The
dated contract/design anchors are
[`REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`](REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md),
[`REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md`](REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md)
and
[`LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md`](LARGE_ALIGNMENT_ARCHITECTURE_2026-09-30.md).
They contain historical proposals and are not substitutes for live
call-site evidence or acceptance receipts.
All `file:line` references below are to the source tree at the time of
writing and were verified by reading the files.

### 9.1 Two INDEPENDENT atomic words, one shared transition vocabulary

The same small wrapper type is used over two different physical words;
do not conflate them:

- **The PHYSICAL owner-only Large reservation word.** `LargePhase`
  (`src/alloc_core/segment/segment_header/terminal_words.rs:37-45`) packs
  `Unused=0, Initializing=1, Live=2, Pending=3, Consuming=4, Cached=5,
  Released=6` with a generation (`pack_large_state`,
  `terminal_words.rs:48-51`). The owner drives it under its exclusive
  authority — a registry heap lease or a standalone core's `&mut`
  (`LargeReservationState` doc,
  `src/alloc_core/large/reservation_state.rs:9-18`):
  `Live → Consuming` via `claim_live`
  (`reservation_state.rs:86-101`; call sites
  `src/alloc_core/large/alloc_core_large.rs:730-735`,
  `src/alloc_core/alloc_core/lifecycle.rs:525-526`), then either
  `Consuming → Cached` (`cache_consumed`,
  `reservation_state.rs:104-106`, used at `alloc_core_large.rs:805`) or
  `Consuming → Released` (`release_consumed`,
  `reservation_state.rs:110-112`, `alloc_core_large.rs:830`,
  `lifecycle.rs:529`). Cached reuse advances the **generation without
  wrap**: `Cached(g) → Initializing(g+1)` via `begin_reuse`, which
  returns `None` at `MAX_LARGE_GENERATION = u64::MAX >> PHASE_BITS`
  (`reservation_state.rs:128-143`; `terminal_words.rs:32,74-84`), then
  `Initializing → Live` after the owner completes layout/table reset
  (`finish_reuse`, `reservation_state.rs:147-149`). Eviction/rollback
  edges: `Cached → Released` (`release_cached`,
  `reservation_state.rs:116-118`; call sites
  `alloc_core_large.rs:258-263`,
  `alloc_core_large_cache_eviction.rs:50,241`,
  `lifecycle.rs:60-66`) and `Initializing → Released` on cache-hit
  rollback before user issuance (`release_initializing`,
  `reservation_state.rs:120-124`; `alloc_core_large.rs:459-464`).
- **The INDEPENDENT `LargeState`-backed route descriptor word** — a
  `System`-allocated descriptor (`LargeState`,
  `src/registry/segment_route/large_state.rs:8-64`; allocated by `System`
  at registration, `src/registry/segment_route/directory.rs:69-75`;
  `LargeState::new()` starts at `Live` with generation 1,
  `large_state.rs:14-17`). This is the production
  **foreign-publication** path: a remote producer performs the terminal
  strong CAS `LIVE(g) → PENDING(g)` with success `AcqRel` / failure
  `Acquire` (`publish_pending`, `reservation_state.rs:42-51`; reached
  only through `registry::segment_route`, per
  `reservation_state.rs:39-40`, via `RoutePin::publish_large`,
  `src/registry/segment_route/pin.rs:57-62`, and
  `LargeState::publish_pending`, `large_state.rs:24-28`). The owner's
  table-scan claim is `RouteSlots::claim_large_pending`
  (`src/alloc_core/segment/segment_table/route_slots.rs:189-200`),
  driving `PENDING(g) → CONSUMING(g)` via `LargeState::claim_pending`
  (`large_state.rs:30-32`; `claim_pending`,
  `reservation_state.rs:58-73`; reached through
  `RouteRegistration::claim_large_pending`,
  `src/registry/segment_route/registration.rs:51-53`). `AllocCore` then
  separately claims the **PHYSICAL** reservation word
  `LIVE(g) → CONSUMING(g)` (`claim_live`,
  `alloc_core_large.rs:730-735`), unregisters the **TABLE** entry
  (`alloc_core_large.rs:740`; segment removal in
  `src/alloc_core/segment/segment_table/segment_table_impl.rs:514-516`)
  and drops the `RouteRegistration` (`RouteRegistration::drop →
  directory.remove`, `registration.rs:78-81`) **before** caching the
  physical reservation (`alloc_core_large.rs:805-821`). The unlinked old
  descriptor survives only while old pins hold refcounts, then is freed
  (`EntryHandle::drop`, `directory.rs:210-225`). Reuse advances the
  **PHYSICAL** word `Cached(g) → Initializing(g+1) → Live(g+1)`
  (`alloc_core_large.rs:249-264`; `finish_large_reuse` at
  `alloc_core_large.rs:483`, via `terminal_words.rs:145-160`) and
  registers a FRESH descriptor — `LargeState::new()` again starts at
  `Live` with generation 1 (`directory.rs:69-75`) — plus a new
  registration incarnation.

**What NOT to claim (explicit non-claims, R14-03):**

- NOT all nine primitive transitions of `LargePhase` run on the
  descriptor word in production. On the **descriptor**, the production
  surface is exactly `LIVE → PENDING` (producer) then `PENDING →
  CONSUMING` followed by owner unlink of the table entry. The
  cache/reuse/rollback/generation-saturation transitions
  (`Consuming → Cached`, `Cached → Initializing → Live`, `Cached →
  Released`, `Initializing → Released`) belong to the **PHYSICAL**
  reservation word, not the descriptor. The extra `LargeState` methods
  (`cache_consumed`/`begin_reuse`/`finish_reuse`/`release_cached`,
  `large_state.rs:34-50`) have **no production callers** — they are a
  local/testing façade and must not be presented as production. There is
  no type named `SystemLargeState` anywhere in source; the type is
  `LargeState`, a `System`-allocated descriptor (`directory.rs:72`).
- **A descriptor pin is NOT a reservation credit.** `RoutePin` is "a
  counted descriptor and sidecar capability, not a reservation credit;
  no allocator-origin root is accessible through a producer pin"
  (`pin.rs:4-5`; same distinction in
  `reservation_state.rs:17-18` and
  `src/registry/segment_route/small_sidecar.rs:105-109`). What keeps the
  physical reservation alive is the producer's unique *allocation*
  credit, transferred to the owner by the successful `Pending` CAS
  (`reservation_state.rs:29-38`).
- **The pin `Drop` is storage-only.** After terminal publication the
  pin "touches only the independently allocated descriptor/sidecar"
  (`pin.rs:37-38`); it never reads or writes the reservation — a
  metadata-only refcount update on the independent descriptor is not a
  reservation touch. The old §8 intrusive-word race (owner vs remote
  contending over the block's first word) has no direct analogue on this
  path — the terminal word is never written through a producer-side
  intrusive free-list link — but this contrast does **not** discharge
  the accepted P1-box correctness defect (item 164, §9.7): an
  established paused-witness `Box` remains red under Miri SB/TB, and no
  soundness all-clear follows from the mechanism difference.
- **The old SM-SLOT rule (I-SLOT-1) does NOT forbid a valid pending
  foreign publication from a previous owner thread after whole-heap
  slot reuse.** I-SLOT-1 was a design invariant of the ring/ABANDONED
  era (§2–§3); the current protocol protects cross-lifetime frees by
  registration incarnation and generation, not by that rule (§9.2,
  §9.3).

### 9.2 Registration incarnation — a no-address capability, not stale-pointer validation

Route directory entries carry a monotonically increasing `incarnation`
(`next_incarnation`, `src/registry/segment_route/directory.rs:700,825-830`),
stamped at registration. `lookup`
(`directory.rs:868-888`) takes **only** the address — no expected old
incarnation is supplied. Under one shard lock it pins the **current**
entry, then aborts on an internal pointer/incarnation **self-check**
mismatch (`directory.rs:882-886`). That is snapshot
identity/refcount consistency, **not** validation of an arbitrary
caller-supplied stale pointer against an expected value: an arbitrary
stale pointer at the same VA is not distinguishable by lookup alone —
an address is not proof of allocation ownership (the pin safety contract
says so explicitly, `pin.rs:44`). A pre-existing old pin retains its old
independent descriptor after unlink; that is not automatically a current
capability. What ordinarily prevents premature reuse is the valid unsafe
caller's **unique allocation credit**, held until terminal publication
(`reservation_state.rs:29-38`). Generation/incarnation numbers alone do
**not** prevent double free. A `RoutePin` exposes the incarnation as a
number only (`pin.rs:17-19`) — it is a capability over the *descriptor*,
not over reservation memory, and grants no address/root
(`pin.rs:4-5,20-23`: capacity validation reads only the independent
immutable descriptor).

### 9.3 Terminal publication at the last user allocation touch

The successful terminal CAS is the producer's **last descriptor-state
access**, not an access to reservation bytes (`reservation_state.rs:35-38`).
It transfers the unique allocation credit; the caller must not access or
free the allocation or reservation afterwards, nor access the terminal
publication state again (`pin.rs:40-44,53-56`). The consumed `RoutePin`'s
`Drop` is a separate storage-lifetime operation: it reads/updates
refcounts on the INDEPENDENT descriptor and may free the System-allocated
sidecar ("touches only the independently allocated descriptor/sidecar",
`pin.rs:37-38`). That metadata-only `Drop` is not a reservation touch or
another terminal-state access.
Small/Primordial use the sidecar bitmap `fetch_or` terminal publication
(`src/registry/segment_route/small_sidecar.rs:148-150` delegates to
`src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:99-118`, with the
RMW at `:117`); Large uses the `Live → Pending` CAS
above. Nothing in production waits on, or is scheduled by, the producer
after publication; the Miri-only pause gates
(`src/registry/heap_core_xthread/routing.rs`,
`TerminalPublicationGate` under `cfg(miri)`,
`src/registry/segment_route/terminal_publication_gate.rs`) are test
machinery, not production.

### 9.4 Typed heap lease — OWNED/FREE maintenance excludes paused OWNED

Slot states are plain constants `STATE_EMPTY=0, STATE_OWNED=1,
STATE_INITIALIZING=2, STATE_FREE=3, STATE_MAINTENANCE=4`
(`src/registry/heap_slot.rs:89-99`). A successful state CAS grants exclusive
mutation authority: recycled slots use `FREE → STATE_LIVE` (= OWNED)
(`src/registry/heap_registry/claim.rs:239-244`); first claims use
`EMPTY → INITIALIZING` (`claim.rs:221-234`) and publish
`INITIALIZING → LIVE` after materialization (`claim.rs:295-306`). Both
return a typed `HeapLease` (`claim.rs:339-344,459`). `STATE_LIVE → STATE_FREE`
is published with `Release` only after all mutable accesses (`claim.rs:531`). The
maintenance worker acquires a `MaintenanceLease` via `FREE →
MAINTENANCE` only (`try_maintenance_at`, `claim.rs:83-95`; back-edge
`MAINTENANCE → FREE` at `claim.rs:425-426`). Consequently the
ownerless-maintenance guarantee is conditioned on the heap being `FREE`:
a paused producer holding its heap in `OWNED` is **never** stolen by
timeout (contract §1, `REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`);
maintenance scans only materialized `FREE` slots.

### 9.5 Reservation credits vs descriptor pins

Restating §9.1 as the boundary rule: a *reservation credit* is the
outstanding-allocation obligation that keeps physical reservation bytes
mapped and un-reusable; it is transferred to the owner by the terminal
CAS and discharged exactly once by owner reclaim. A *descriptor pin*
(`RoutePin`, `EntryHandle`) only keeps the independently allocated
route descriptor/sidecar alive; the directory reclaims a retired entry
on the last pin (`directory.rs:214`, `mod.rs:11`: "Pins do not keep
reservation memory alive"). Strict trim does not wait for producers or
pins: reservation finalization depends on discharged credits, not on
independent descriptor refcounts.

### 9.6 Cache / release / rollback paths and generation saturation

After descriptor `Pending → Consuming`, the owner separately claims the
physical word `Live → Consuming` and unlinks the route. Only that physical
word then takes `Consuming → Cached` or `Consuming → Released`, while mapped
and before OS release (`alloc_core_large.rs:730–740,805–830`).
Cache hit reissues via `Cached → Initializing(g+1) → Live(g+1)`
(generation advance without wrap; `reservation_state.rs:128-149`,
physical call sites `alloc_core_large.rs:251` and `terminal_words.rs:145-160`).
Cache-hit failure before issuance rolls back to `Initializing → Released`
(`reservation_state.rs:120-124`); eviction of a cached entry is
`Cached → Released` (`reservation_state.rs:114-118`).
Generation saturation is handled by returning `None` from
`next_large_generation` at `MAX_LARGE_GENERATION` (`terminal_words.rs:74-84`)
— the reservation is retired rather than wrapping
(`reservation_state.rs:126-133`); the packing function asserts the
bound (`terminal_words.rs:49`).

### 9.7 Correctness residual — item 164 (P1-box) is owner-accepted, NOT fixed

Correctness item 164 in
[`docs/correctness-open-items/TRACKED_correctness_residuals.md`](correctness-open-items/TRACKED_correctness_residuals.md)
(the accepted P1-box (correctness item 164) per the Ph3c path-(b)
decision of 2026-10-05, and re-confirmed owner-accepted not-fixed by the
round-14 review `docs/reviews/2026-10-06-src-review-sol-round-14.md`)
remains open: an established `Box` paused-witness is red under Miri
SB/TB because the intrusive free-list link is written into the body of
a freed Small block while the freeing by-value `Box` frame is still
live. **No Miri-clean or UB-free claim may be read into this document**
or into the current protocol it describes; native/loom evidence does
not discharge this residual.
