# Concurrency & Lock-Free Correctness Audit — sefer-alloc

Date: 2026-07-17 (audit run continued 2026-07-18)
Scope: `src/concurrent/`, `src/registry/`, `crates/ring-mpsc`,
`crates/tagged-index-stack`, `crates/racy-ptr-cell`, `src/alloc_core/remote_free_ring.rs`,
`src/alloc_core/segment_directory.rs`, `src/alloc_core/deferred_large/*`, and
every `tests/loom_*.rs` harness. Read-only static audit — no build/test/miri/loom
run, per instructions.

Method: read every atomic-ordering site in the cross-thread-free / MPSC-ring /
Treiber-stack / dirty-routing code paths, cross-checked each against its
documented ordering rationale, then checked whether a `tests/loom_*.rs` (or
crate-local loom test) actually model-checks the SHIPPED code (lesson #174) —
including whether the loom harness's *interleaving* actually exercises the
race the production code is exposed to, not just a sequential re-statement of
the protocol.

---

## Summary table

| # | Severity | Confidence | Area | One-line |
|---|----------|------------|------|----------|
| F1 | Low | High | `tests/loom_dirty_publish.rs`, `tests/loom_dirty_multi_segment.rs` | Dirty-bitmap loom harnesses `join()` every producer before the consumer ever runs `swap_and_drain` — the concurrent producer-vs-consumer interleaving (the one the real `drain_dirty_segments`/`push_with_overflow_retry` pair is exposed to) is never explored by loom; coverage is sequential-composition-only |
| F2 | Low | Medium | `src/alloc_core/segment_directory.rs` (+ `alloc_core_small.rs` R7-A2/A3/A4 call sites) | The per-class `SegmentDirectory` bitmap is plain `u64` (non-atomic) by design ("owner-only"), but its liveness depends on `drain_dirty_segments` running fully before the directory bit is trusted; no loom/property test models the specific empty→non-empty→empty churn the directory bitmap undergoes across a dirty-drain pass |
| F3 | Informational | High | `src/registry/heap_core_xthread.rs` (`push_with_overflow_retry`, `LAST_STALL_CONCESSIONS`) | The stall-concession fast-concede cache is pure caller-side heuristic state with no loom model; a false-positive fast-concede is provably bounded (documented) but the bound rests on prose reasoning only, not a machine-checked proof |
| F4 | Informational | High | `crates/ring-mpsc`, `crates/tagged-index-stack`, `crates/racy-ptr-cell` | All three extracted seam crates carry real-type loom coverage (atomics aliased under `#[cfg(loom)]`) with non-vacuous `#[should_panic]` counterfactuals — no findings; noted as the positive baseline the rest of the review is measured against |
| F5 | Informational | High | `src/alloc_core/deferred_large/{push,drain}.rs` | Treiber push/pop for the Large cross-thread-free stack is loom-covered with a *documented real historical bug find* (task #141→#143: claim-CAS inside vs. outside the retry loop) — exemplary of the lesson-#174 discipline; no findings |

No High/Critical findings. This module set is unusually well-instrumented:
every hot cross-thread protocol (`RemoteFreeRing`, `HeapOverflow`,
`TaggedIndexStack`, `RacyPtrCell`, the Large deferred-free Treiber stack) has
a dedicated loom harness whose orderings match the shipped code line-for-line,
several with genuine `#[should_panic]` counterfactuals proving non-vacuity.
The two real findings (F1, F2) are both about loom-model *fidelity* — a
sequential-composition harness passing does not by itself rule out the
concurrent case — not about incorrect atomic orderings in the shipped code.

---

## Findings

### F1 — Dirty-bitmap loom harnesses never interleave producer and consumer (Low / High confidence)

**File:** `tests/loom_dirty_publish.rs:112-146` (`dirty_publish_swap_never_loses_entry`),
`tests/loom_dirty_publish.rs:154-191` (`lost_wakeup_bit_survives_swap`),
`tests/loom_dirty_multi_segment.rs:89-122` (`multi_segment_same_dirty_word_no_lost_entry`)

**Description.** The shipped production protocol is: a cross-thread freer
calls `RemoteFreeRing::push`/`try_push_uncounted` then
`set_dirty_bit_for_segment` (`src/registry/heap_core_xthread.rs:307-330`,
`fetch_or(bit, Release)`), fully concurrently with the owner's
`drain_dirty_segments` (`src/alloc_core/alloc_core_small.rs:1770-1830+`,
`swap(0, Acquire)` per word) running on its own `alloc()` slow path — there is
no synchronization forcing producers to finish before a drain starts, and
`find_segment_with_free_impl` is reachable from the owner's hot path at
arbitrary times relative to any number of live remote freers.

The three loom tests above model this pairing, but in every one the producer
thread(s) are `.join()`-ed (or otherwise fully sequenced: producer A "push +
mark", `ta.join()`, THEN the consumer's `swap_and_drain` calls happen) before
any `swap_and_drain` call executes. Concretely:

```rust
// tests/loom_dirty_publish.rs:120-136 — dirty_publish_swap_never_loses_entry
let ta = thread::spawn(move || { m_a.push_and_mark(10, 1 << 0); });
let tb = thread::spawn(move || { m_b.push_and_mark(20, 1 << 1); });
ta.join().unwrap();
tb.join().unwrap();
// Consumer: two drain passes.
let (c1, _) = model.swap_and_drain();
let (c2, _) = model.swap_and_drain();
```

loom's `preemption_bound = 3` still explores interleavings of A's internal
steps vs. B's internal steps (both producers running "concurrently" relative
to each other), but the consumer's `swap_and_drain` is program-order-after
BOTH joins — loom can never explore the schedule where `swap(0, Acquire)`
races a producer's in-flight `fetch_or`/ring-publish. That is exactly the
schedule the module doc's own P4 visibility-contract note
(`src/alloc_core/remote_free_ring.rs:114-139`) argues about ("a producer
stalled between push and fetch_or is invisible... until the bit lands"), and
it is exactly the schedule that determines whether the "at-least-once wakeup"
argument in `crates/ring-mpsc/src/lib.rs:758-781` (`DirtyRouter`'s own doc,
which the in-tree `dirty_segments` bitmap mirrors but does not literally
reuse) is correct.

`multi_segment_producer_during_drain` (`tests/loom_dirty_multi_segment.rs:128-161`)
gets closer — it interleaves "drain 1, then producer B pushes, then drain 2" —
but this is still hand-sequenced (`tb.join()` before `swap_and_drain` #2), not
a loom-explored race between B's `fetch_or` and drain 2's `swap`.

**Why this matters even though the design argument is sound.** The
"bounded-deferral, at-least-once" contract is a genuine, correct design (the
in-code comments and the `ring-mpsc` crate's `DirtyRouter` doc both state it
correctly, and the linear-scan fallback closes the gap even if the fast path
misses a window). The *design* is not in question. What is missing is the
loom-level confirmation that the SHIPPED implementation of the racing
interleaving — specifically: does `swap(0, Acquire)` racing a concurrent
`fetch_or(bit, Release)` ever produce a torn/lost bit under the actual
`AtomicU64` semantics loom would check (memory-order subtleties, not just
algebra)? — actually holds. `fetch_or`/`swap` are both RMW ops so algebraically
this is very likely fine (RMWs total-order on one location), but the whole
point of the loom suite elsewhere in this codebase (per lesson #174 and the
`tagged-index-stack` H-2 precedent) is to not rely on "very likely fine"
reasoning for exactly this class of primitive. This is the one dirty-routing
protocol in the R7-A4 feature where that discipline was not fully applied.

**Failure scenario (hypothetical, not reproduced).** None currently known —
this is a coverage gap, not a demonstrated bug. The theoretical shape a
missing test could hide: producer's `fetch_or` completing concurrently with a
consumer's `swap(0, ..)` such that the bit is visible to the swap that reads
it (fine) but the *ring slot* the bit represents has not yet completed its
own `Release` store when the drain body reads it — this is the "stop at
`RING_SLOT_EMPTY`" case already handled by `RemoteFreeRing::drain`/
`HeapOverflow::drain` independently of the dirty bit, so the bitmap race
itself is unlikely to be the true hazard; but this is prose reasoning, the
same kind lesson #174 exists to not rely on alone.

**Impact.** Low — the fallback linear scan (`find_segment_with_free_impl`'s
non-directory path, still reachable whenever the directory sidecar is not
materialised or a directory miss occurs) independently guarantees
correctness; the dirty-bitmap fast path is provably an optimization layer,
not a soundness-load-bearing one, per its own module doc. The gap is in
verification depth, not in a demonstrated defect.

**Fix.** Add a loom harness (or extend `loom_dirty_multi_segment.rs`) that
spawns the consumer's `swap_and_drain` as a genuinely concurrent `thread`
alongside an in-flight producer — i.e. do NOT `join()` the producer before
starting the consumer thread — and assert the same "every published entry
eventually observed across N drain passes" invariant. This is a small,
mechanical addition (the `DirtyModel`/`MultiSegDirtyModel` structs already
exist); it costs nothing beyond loom's own explosion for one more thread.

---

### F2 — No loom/property model of the `SegmentDirectory` bitmap's churn across a dirty-drain pass (Low / Medium confidence)

**File:** `src/alloc_core/segment_directory.rs:96-181`, call sites in
`src/alloc_core/alloc_core_small.rs:1698-1850` (`publish_nonempty`/
`publish_empty`/`sync_directory_for_segment`/`drain_dirty_segments`)

**Description.** `SegmentDirectory` is explicitly designed as an "owner-only,
plain `u64`, no cross-thread reader" structure (module doc,
`segment_directory.rs:11-19`) — this is correct as stated: no OTHER thread
ever touches `class_nonempty` directly. But its correctness is only as good
as the invariant "every `set_bit`/`clear_bit` call happens strictly after the
`BinTable` head transition it reports, and the owner never reads a stale bit
without re-validating against the live `BinTable` head" — which IS
cross-thread-adjacent, because the transitions themselves are triggered by
draining a ring whose PUBLISH side is a remote producer (R7-A4:
`drain_dirty_segments` calls `sync_directory_for_segment`, which is a
consequence of a remote `fetch_or`+ring-push pair completing on another
thread at an unconstrained time relative to the owner's scan).

The directory-driven lookup path (`find_segment_with_free_impl`,
`alloc_core_small.rs:299-450`) treats every directory-bit hit as a
*candidate*, re-validated against the live `BinTable` head before use
(`alloc_core_small.rs:378-410` and around) — this defensive re-validation is
the right mitigation and is present. What is absent is any loom or targeted
proptest that drives the SPECIFIC sequence: (a) directory bit set by a
dirty-drain pass, (b) a concurrent remote free lands on the SAME class
immediately after, changing the `BinTable` head again, (c) the owner's
directory-driven lookup reads the (now stale-in-a-different-way) bit and
re-validates. The re-validation code path exists and looks correct by
inspection, but — matching F1's theme — the multi-step choreography between
"remote push → dirty bit → owner's dirty-drain → `sync_directory_for_segment`
→ owner's directory-driven lookup on a LATER call" spans several files and is
currently only exercised by non-adversarial integration/property tests (not
found: a loom harness naming `segment_directory` or `sync_directory_for_segment`).

**Failure scenario.** None demonstrated. The theoretical concern is a
directory bit that is set (correctly) by a drain pass, then racily cleared or
re-set incorrectly by an interleaved `publish_empty`/`publish_nonempty` pair
from two different call sites operating on the same `(class_idx, slot_idx)`
in a single-threaded-owner discipline — but since the directory is
"owner-only, single-writer" by construction (only the owning thread ever
calls `publish_nonempty`/`publish_empty`/`clear_bit`/`set_bit`), this
particular race is structurally excluded by the single-writer rule, which
*is* a valid static argument (unlike the RMW-vs-RMW question in F1). This
finding is therefore weaker than F1: it flags an absence of machine-checked
verification for a property that already has a reasonably solid single-writer
argument, rather than flagging a genuinely open race window.

**Impact.** Informational-to-Low. The single-writer discipline is real and
load-bearing; a static per-file read supports it. This finding exists mainly
to record that — unlike `RemoteFreeRing`, `HeapOverflow`, `TaggedIndexStack`,
and the Large deferred-free stack, all of which have dedicated loom
harnesses — `SegmentDirectory`'s cross-module choreography (remote push →
dirty bit → drain → directory sync → later directory-driven read) has none,
despite spanning `alloc-xthread` + `alloc-segment-directory` cross-thread
surface area comparable to the ones that do.

**Fix.** Lower priority than F1. If pursued: a loom (or plain multithreaded
stress) test that drives one remote-producer thread doing repeated
push+mark against one class/slot while the owner thread repeatedly calls
`drain_dirty_segments` + a directory-driven lookup, asserting the
directory-driven result is never a false negative (never fails to find a
block that genuinely exists) — false positives are already handled by the
re-validation code and are safe by construction.

---

### F3 — `LAST_STALL_CONCESSIONS` fast-concede cache has no loom coverage (Informational / High confidence)

**File:** `src/registry/heap_core_xthread.rs:156-241` (cache definition),
`heap_core_xthread.rs:1006-1155` (read/write sites inside
`push_with_overflow_retry`)

**Description.** This is a per-thread `thread_local!` `Cell` of `(segment
base, ring head, overflow head)` snapshots, written only by the SAME thread
that reads it (no cross-thread access at all — it is not shared state). It is
correctly out of loom's normal purview (loom models cross-thread races; this
is single-thread-local heuristic bookkeeping). It is included here only as an
**informational** note because its correctness argument ("a snapshot match
implies zero drain progress since the concession, because both cursors are
monotonic and owner-advanced") rests entirely on the *monotonicity* of
`RemoteFreeRing::head_relaxed()`/`HeapOverflow::head_relaxed()` — both of
which ARE cross-thread-read `Relaxed` loads of atomics written by a different
thread (the owner) — so the soundness of the whole fast-concede mechanism is
downstream of the monotonic-cursor argument that IS loom-adjacent (covered by
`loom_remote_ring.rs`/`loom_heap_overflow*.rs` for the ring protocols
themselves, just not for this specific consumer of `head_relaxed()`).

**Impact.** None demonstrated; the monotonicity argument the cache depends on
is independently well-supported by the ring/overflow loom suites. No action
required beyond noting the dependency chain for future auditors.

---

### F4 — Extracted seam crates: real-type loom coverage confirmed, no findings (Informational)

**Files:** `crates/ring-mpsc/src/lib.rs`, `crates/tagged-index-stack/src/lib.rs`,
`crates/racy-ptr-cell/src/lib.rs`

All three crates alias their atomics to `loom::sync::atomic` under
`#[cfg(loom)]` (verified: `ring-mpsc/src/lib.rs:106-109`,
`tagged-index-stack/src/lib.rs:115-118`, `racy-ptr-cell/src/lib.rs:98-101`),
so the shipped loom tests in `tests/` model-check the REAL type, not a
hand-transcribed copy — directly satisfying lesson #174's "never delete
in-tree loom models for code still shipping inline" concern (the concern does
not even arise here, since the crates are the ONE canonical implementation
now, per CRATE-P4/P7/P3 in the task history).

Verified orderings, all matching their documented rationale:
- **`ring-mpsc::MpscRing`** (`lib.rs:606-730`): push full-check `Acquire` head
  load / `AcqRel` tail CAS / `Release` publish; drain `Acquire` tail load /
  `Relaxed` head load (sound — single consumer) / `Acquire` slot read /
  `Release` head store. Matches `RemoteFreeRing`/`HeapOverflow` exactly (by
  design — CRATE-P4-followup swapped both onto this crate).
- **`ring-mpsc::DirtyRouter`** (`lib.rs:782-853`): `fetch_or(bit, Release)` /
  `swap(0, Acquire)` — the canonical form of the in-tree `dirty_segments`
  bitmap (F1's subject), but this crate's OWN doc (§"HONEST contract") is
  explicit about the bounded-deferral semantics and does not claim more than
  the in-tree code claims. No `DirtyRouter`-specific loom test was found in
  this crate either (only the two in-tree harnesses discussed in F1) — same
  gap, noted once here rather than duplicated.
- **`tagged-index-stack::TaggedIndexStack`** (`lib.rs:324-401`): push
  `Release` link store → `Release` CAS; pop `Acquire` link load → `Acquire`
  CAS. H-2 (empty-transition tag preservation) is structurally enforced
  (`pop`'s `TAIL` branch packs the running tag, never resets to 0) and
  proven by a documented, presumably-shipped `#[should_panic]` loom
  counterfactual (`counterfactual_empty_transition_tag_reset_lets_aba_recur`,
  referenced in the module doc; not independently re-verified in this
  read-only pass beyond confirming the fix code itself at `lib.rs:387-392`).
- **`racy-ptr-cell::RacyPtrCell`** (`lib.rs:268-352`): winner CAS `Acquire`
  success / `Relaxed` failure; publish `Release`; loser spins on `==
  INITIALIZING` (not `!= READY`) — correctly avoids the OOM-rollback deadlock
  the doc describes. `Relaxed`-publish and `!= READY`-spin counterfactuals are
  documented as loom-proven load-bearing.

No findings against any of these three crates in this pass.

---

### F5 — Large deferred-free Treiber stack: exemplary loom coverage with a real historical catch (Informational)

**File:** `src/alloc_core/deferred_large/push.rs:75-144`,
`src/alloc_core/deferred_large/drain.rs:47-78`,
`tests/loom_deferred_large.rs`

Verified: `push_large_deferred_free`'s double-push guard CAS
(`ABANDONED_TAIL → next_link`, `Release`/`Relaxed`) runs EXACTLY ONCE before
the `head`-CAS retry loop (`push.rs:110-120`, loop at `121-143`), matching
`loom_deferred_large.rs`'s `Stack::push` model byte-for-byte in ordering
shape. `drain_large_deferred_free`'s pop CAS is `Acquire`/`Relaxed`
(`drain.rs:70`), correctly stronger than the model's need (the model uses the
identical orderings).

This is the strongest positive evidence in the whole audit that the project's
loom discipline works as intended: the harness's own module doc
(`loom_deferred_large.rs:42-65`) records that WRITING this loom model (not
running it after the fact) found a real production bug — the double-push
claim-CAS originally lived INSIDE the retry loop, which silently dropped a
push under >=2 concurrent pushers of distinct bases racing `head` more than
once (task #141 → fixed in #143). The counterfactual test
(`counterfactual_no_guard_double_extracts_or_corrupts`,
`loom_deferred_large.rs:340-407`) is `#[should_panic]` and reproduces the
pre-fix shape, confirming non-vacuity. No findings.

---

## Cross-thread protocol → loom-model coverage map

| Protocol | Shipped location | Loom harness(es) | Real-type or transcription? | Concurrent producer/consumer interleaving modeled? |
|---|---|---|---|---|
| `RemoteFreeRing` push/drain (MPSC ring) | `src/alloc_core/remote_free_ring.rs` (now backed by `ring-mpsc`) | `tests/loom_remote_ring.rs`, `tests/loom_remote_ring_drain_guard.rs` | Transcription (loom atomics, mirrors the real protocol shape) | Yes — 2 producers vs. 1 consumer, genuinely concurrent (not joined first) |
| `HeapOverflow` push/drain (per-heap 2-tier MPSC ring) | `src/registry/heap_overflow.rs` | `tests/loom_heap_overflow.rs`, `tests/loom_heap_overflow_drain_guard.rs` | Transcription | Yes (by file naming/structure; consistent with the ring harness pattern) |
| `HeapOverflow` sidecar CAS (R6-OPT-P0-2 lazy materialisation) | `heap_overflow.rs` `push_impl`'s `ensure_overflow_sidecar` call | `crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs` (CRATE-P3; replaced the former `tests/loom_overflow_sidecar_cas.rs`) | Real-type (racy-ptr-cell aliases loom atomics) | Models the CAS protocol in isolation, per the module doc |
| `push_with_overflow_retry` overflow-first policy + stall-progress detection | `src/registry/heap_core_xthread.rs` | `tests/loom_overflow_first_retry.rs` | Transcription (392 lines — not read in full this pass; file exists and is named for exactly this protocol) | Not independently verified in this pass; recommend follow-up read |
| Dirty-segment bitmap (`fetch_or`/`swap` lost-wakeup) | `src/registry/heap_slot.rs` (`dirty_segments`), set by `heap_core_xthread.rs::set_dirty_bit_for_segment`, drained by `alloc_core_small.rs::drain_dirty_segments` | `tests/loom_dirty_publish.rs`, `tests/loom_dirty_multi_segment.rs` | Transcription | **No — F1: producers always `join()`ed before the consumer's `swap_and_drain` runs; no genuinely concurrent producer-vs-drain interleaving is loom-explored** |
| `SegmentDirectory` bitmap sync across dirty-drain | `src/alloc_core/segment_directory.rs` | none found | — | No — F2 |
| `TaggedIndexStack` push/pop + H-2 empty-transition tag | `crates/tagged-index-stack/src/lib.rs` | `tests/loom_aba.rs` (crate module doc reference; per-crate test not independently opened this pass, only the H-2 fix code and doc) | Real-type | Documented as yes (module doc references a `#[should_panic]` counterfactual) |
| `RacyPtrCell` init CAS + OOM rollback + loser re-race | `crates/racy-ptr-cell/src/lib.rs` | crate-local loom tests (not individually enumerated this pass) | Real-type | Documented as yes |
| `MpscRing`/`DirtyRouter` (extracted generic ring-mpsc crate) | `crates/ring-mpsc/src/lib.rs` | crate-local loom tests (not individually enumerated this pass) | Real-type | Ring: yes (mirrors `loom_remote_ring.rs`'s pattern). `DirtyRouter`: not found — same gap as the in-tree dirty bitmap (F1), unduplicated here |
| Large deferred-free Treiber stack (push/drain, double-push guard) | `src/alloc_core/deferred_large/{push,drain}.rs` | `tests/loom_deferred_large.rs` | Transcription, with a **documented real bug find** (task #141→#143) | Yes — 2 producers racing `head`, single consumer draining |
| Cross-thread single-owner "never resurrect a LIVE block" (SM-BLOCK/SM-CHANNEL) | conceptual protocol underlying `RemoteFreeRing` + `BinTable` reclaim | `tests/loom_xthread_protocol.rs` | Transcription, with `#[should_panic]` counterfactual (`broken_publish_before_release_resurrects`) | Yes |
| `EpochRegion`/`AtomicSlot` (legacy `experimental` tier: lock-free cross-thread remote_evict) | `src/concurrent/hand.rs`, `epoch_region.rs` | `tests/loom_epoch.rs`, `tests/loom_sharded.rs` | Transcription (not independently opened this pass beyond the module docs' self-description) | Documented as covering the generation-CAS eviction race; deprecated/legacy tier, lower priority |
| Cross-thread "thread_free" head aliasing (Large TFS across `HeapSlot`) | `src/registry/heap_slot.rs` (`HeapSlotRemote::thread_free`), `heap_core_xthread.rs::push_large_deferred_free` | `tests/loom_thread_free.rs` | Transcription | Named for exactly this protocol; not independently re-opened this pass (time-boxed) |
| Magazine/ring composition (fastbin tcache draining a segment's ring) | `src/alloc_core/alloc_core_small_magazine.rs` | `tests/loom_magazine_ring_compose.rs` | Transcription | Named for exactly this protocol; not independently re-opened this pass (time-boxed) |

Rows marked "not independently verified/re-opened this pass" are flagged for
completeness of the map, not because a defect is suspected — the audit's time
budget prioritized the protocols the task brief named explicitly
(alloc-xthread, ring-mpsc, tagged-index-stack, racy-ptr-cell, dirty routing,
remote-free ring, magazine) plus the highest-risk newer addition (R7-A4 dirty
routing, where F1/F2 were found). A follow-up pass should open
`loom_overflow_first_retry.rs`, `loom_thread_free.rs`, and
`loom_magazine_ring_compose.rs` in full and cross-check them the same way
`loom_deferred_large.rs`/`loom_remote_ring.rs`/`loom_xthread_protocol.rs` were
checked here (concurrent, not sequentially-joined, interleavings).

---

## Top findings (ranked)

1. **F1** (Low/High) — dirty-bitmap loom harnesses never model a genuinely
   concurrent producer-vs-consumer race; add one that does not `join()` the
   producer before starting the drain thread.
2. **F2** (Low/Medium) — `SegmentDirectory` bitmap's cross-module churn
   (remote push → dirty bit → drain → directory sync → later directory-driven
   read) has no loom/stress coverage at all, though its single-writer
   argument is sound by inspection.
3. **F3** (Informational) — `LAST_STALL_CONCESSIONS` is thread-local and
   correctly out of loom's scope; flagged only for its dependency on the
   (separately loom-verified) cursor-monotonicity argument.
4. **F4/F5** (Informational, positive) — the extracted seam crates and the
   Large deferred-free Treiber stack are exemplary: real-type loom aliasing,
   non-vacuous counterfactuals, and in one case (`loom_deferred_large.rs`) a
   documented genuine production-bug catch. No action needed.

No atomic ordering in the code actually read this session (`RemoteFreeRing`,
`HeapOverflow`, `TaggedIndexStack`, `RacyPtrCell`, `deferred_large`,
`AtomicSlot`, `SegmentDirectory`, `dirty_segments`) was found to be
insufficient or over-strong relative to its documented pairing. All
Release/Acquire pairings checked were internally consistent and matched their
prose justification.
