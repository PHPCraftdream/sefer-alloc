# concurrent perf/quality review — 2026-09-20

Scope: `src/concurrent/` only (`epoch/`, `lock_free/`, `sharded/`, `pinning`), as of worktree
`review-concurrent` @ `2fe99299` (+ the `72189f0d` regroup). Read-only review; no code changed.

Method: read every file in the tier end-to-end; verified dead-code claims against real callers
(`src/kani_proofs.rs`, `tests/{epoch,lock_free,sharded,sharded_remote,loom_epoch,loom_sharded,concurrent_stress,pinning}.rs`);
checked `docs/CORRECTNESS_OPEN_ITEMS.md` and `docs/perf/OPEN_ITEMS.md` for already-tracked items
(none for this tier; the round-2 findings perf#4/#5/#6 were adjudicated "documented, no fix" by
T12, commit `b90553a`); confirmed the reorg was a pure rename via `git diff -M` (`72189f0d^..72189f0d`);
confirmed `crossbeam-queue` is absent from `Cargo.lock` (the claim in `epoch_region.rs:42` is true);
`cargo check --features experimental` clean on this worktree (3 s). All claims below are from code
reading; nothing was benchmarked, and magnitudes are labeled hypotheses.

## Summary

No soundness bug found. The epoch tier's core protocol held up under adversarial reading: the
three-`Acquire` seqlock in `AtomicSlot::read_with` is ordered correctly per the C++11 model (the
first generation `Acquire` prevents the value load from hoisting above it; the value `Acquire`
prevents the `g2` re-load from hoisting below it), the `try_evict_at` single-linearization-CAS +
no-reinstall proof is consistent with the actual free-list protocol (an index can only reach a
free list *after* a unique CAS win, and `install` is provably the first event at a fresh
generation), and `drop_value`'s `unprotected()` is sound under `&mut self` exclusivity (handles
are non-borrowing, but every read/write path takes `&self`, so no reader can coexist with
`Drop`). The lock-free tier contains zero `unsafe` of its own. The one real correctness defect is
a documented-panic-vs-silent-empty divergence in `LockFreeRegion::with_pages`. On the cost side,
the frozen writer path carries three pieces of removable work (a useless `epoch::pin()` per
insert, an unconditional remote-free lock whose own comment claims a fast path that does not
exist, and `AcqRel` counter RMWs that nothing consumes the ordering of).

## P0 — correctness/soundness

Nothing that produces UB, a data race, ABA, or use-after-free was found. One correctness-of-contract
defect (not memory-unsafety; low practical likelihood, high confidence):

- **`src/concurrent/lock_free/lock_free_region.rs:197` — `u32::try_from(page_count).unwrap_or(0)`
  silently turns an oversized `page_count` into an *empty* region, contradicting the documented
  panic.** The doc (`lock_free_region.rs:185-189`) promises: "Panics on `u32` index overflow …".
  Concrete failure scenario: on a 64-bit host with enough RAM, `with_pages(2u64.wrapping_add(16) as usize)`
  passes `u32::try_from`'s range, `Vec::with_capacity(page_count)` at `:192` succeeds (~32 GiB of
  `Arc` slots), then `total_pages` truncates to `0`, the `:198` loop never runs, and the function
  returns a region with **zero pages and `free_head: None`** — every subsequent `insert` silently
  grows a fresh page from index 0, and the caller's pre-provisioned capacity vanished without
  error. Contrast the epoch tier, which does this correctly with `expect`
  (`epoch/epoch_region.rs:155`). Suggested fix direction: `u32::try_from(page_count).expect(...)`
  to match the documented contract (note `:192`'s `with_capacity` already runs on the untruncated
  value, so the truncate-to-0 path also leaves a huge-but-empty allocation behind). The sibling
  overflow sites in the same file (`:191`, `:201`, `:206`, and `insert_growing` at `:412-416`) all
  `expect` correctly — this one line is the outlier. Not memory-unsafe; an absurd input silently
  misbehaving instead of panicking is a §F-style documented-guarantee divergence.

Checked and clean (so the next reviewer does not re-derive these):

- `read_with` seqlock (`epoch/hand.rs:181-246`): sound; see Summary for the ordering argument.
  Generation monotonicity per slot (`try_evict_at` is the only bumper, never wraps: saturation
  retires — the M-8/UBFIX-9 fix at `hand.rs:394-406`) closes ABA.
- `try_evict_at` (`epoch/hand.rs:318-407`): the CAS-win → swap window is safe because a slot can
  only re-enter a free list via the winner's own enqueue + owner drain, both strictly after the
  swap; `install` is the first event at any freshly-evicted generation, so no `try_evict_at` can
  target the `next` generation mid-swap.
- `EpochRegion::Drop` → `drop_value`'s `crossbeam_epoch::unprotected()` (`epoch/hand.rs:416-431`):
  sound — `Drop` requires `&mut self`; `EpochHandle` carries no borrow, but every resolving path
  (`get_with`/`remove`/`remote_evict`) takes `&self`, so no reader or evictor can coexist with
  drop; the swapped-out pointer was never registered as collector garbage.
- `len` accounting (`epoch_region.rs:289/:337/:381`): a `fetch_sub` is only reachable through a
  handle, and a handle is only minted after the paired `fetch_add`, so no underflow; unique CAS
  ownership prevents double-decrement (the pre-7b hazard the upfront `u32::MAX` guard at
  `hand.rs:326-328` also blocks).
- Sharded TLS router (`sharded/sharded_region.rs:291-355`, `481-514`, `145-160`): the R5-F2
  append-all-claims fix is intact; out-of-range cached ids are re-validated (`:301`); Acquire-CAS /
  Release-drop pairing on `occupied` tokens is correct.
- Orderings: there is **no `SeqCst` anywhere in the tier**; every `Acquire`/`Release`/`AcqRel`/
  `Relaxed` I checked has a correct pairing argument (`hand.rs:190/197/207/338-343/357`;
  `sharded_region.rs:154/312/323/489`). The only ordering *stronger than needed* is P1-3 below.

## P1 — complexity / hot-path cost

- **`src/concurrent/epoch/epoch_region.rs:275` — `insert` pins an epoch guard for nothing.**
  `let guard = epoch::pin();` is taken on every insert solely to feed `AtomicSlot::install`'s
  `_guard: &Guard` parameter (`epoch/hand.rs:270`) — and `install`'s body never touches it (it
  only does `Owned::new` + a `Release` store + an `Acquire` generation load; nothing there needs a
  guard). `crossbeam_epoch::pin()` is not free: it registers/bumps the thread's participant state
  in the global collector and the guard's unpin can trigger epoch-advance work (magnitude:
  untested hypothesis — several atomic RMWs per insert). Fix direction: delete the `pin()` call
  and the `_guard` parameter (the only caller is `insert` itself). This is the one truly free win
  in the tier — it removes work without changing any semantics.
- **`src/concurrent/epoch/epoch_region.rs:222-239` — `drain_remote_free`'s comment claims a "Fast
  path: no remote frees → no lock acquisition", but the code locks unconditionally.** The very
  next statement after the comment is `self.remote_free.lock()`, with the `is_empty()` check
  *inside* the lock (`:227-237`). Every owner `insert` (`:280`) and `remove` (`:341`) therefore
  pays a second mutex acquisition (after the writer mutex) even in the common zero-remote-free
  case, contending the same cache line `remote_evict` writers hammer. Comment/code mismatch is
  itself a defect (the comment asserts an optimization that does not exist). Fix direction: a
  `remote_free_pending: AtomicUsize` — `fetch_add(1, Relaxed)` after each enqueue, and
  `if self.remote_free_pending.load(Relaxed) == 0 { return; }` before locking in the drain. A
  push that races the check only delays that index's reuse to the *next* owner op — the same
  eventual-drain cadence the design already accepts; it can never lose an index.
- **`src/concurrent/epoch/epoch_region.rs:176, 289, 337, 381` — `len` uses `Acquire`/`AcqRel`
  where `Relaxed` is sound.** Justification for the weakening: `len` is a standalone counter that
  is never a synchronization edge — nothing publishes or consumes other memory through it
  (readers re-validate via the slot generation seqlock in `read_with`; free-list correctness rests
  on the writer mutex + the CAS, not on `len`; `len()`/`is_empty()` are documented momentary
  observations). RMWs on one atomic observe that variable's own modification order regardless of
  the stated ordering, so `Relaxed` on `fetch_add`/`fetch_sub` (and on `len()`'s load) preserves
  I4 exactly. Honest caveat on the benefit: on x86-64 lock-prefixed RMWs are fully ordered by the
  hardware regardless of the annotation, so the expected gain is Arm codegen (`ldaxr/stlxr` →
  `ldxr/stxr`) and optimizer headroom — hypothesis, ~zero on x86; measure before/after with iai if
  done.
- **`src/concurrent/lock_free/lock_free_region.rs:290-306, 326-368` — every `insert`/`remove` is
  O(page-count), even the no-growth reuse path.** `(*cur).clone()` (`:293`, `:329`) refcount-bumps
  every page `Arc` (n/64 entries) and allocates a fresh page-table `Vec`, then the CoW clones the
  full 64-slot page (`:334`, `:386`) — 64 `SlotState::clone` refcount bumps per single-slot
  mutation. For a region with n entries that is O(n/64) per write where O(log n) (chunked page
  table) or O(1) (per-page `ArcSwap` behind a small directory) is conceivable. **Adjudicated
  already:** this is round-2 finding perf#4, closed by T12 (commit `b90553a`) as a documented
  tradeoff of a frozen research tier — restated here only so the review record is complete; the
  recommendation remains *no fix* unless the tier is revived.

## P2 — types / representation

- **No oversized or no_std-discipline-violating representation found.** Specifically checked:
  `AtomicSlot<T>` is 16 B (`AtomicU32` + `Atomic<T>` ptr; `epoch/hand.rs:108-118`) — compact;
  `LockFreeRegion::Slot<T>` is 24 B (`u32` gen + `SlotState` enum whose `Arc<T>` /
  `Option<u32>` arms both occupy 8 B); `ShardedHandle<T>` is 12 B (`u16` + two `u32`s);
  `PinnedRunner` is a `Vec<CoreId>`. The whole tier is std-gated (`experimental = ["std",
  "dep:arc-swap", "dep:crossbeam-epoch"]`, `Cargo.toml:134`), so `std::sync::Mutex` /
  `Box<[T]>` here do not violate the crate's no_std layering — this tier simply does not compile
  without `std`.
- **`EpochRegion` carries two mutexes (`epoch_region.rs:132-137`).** Worth documenting as a
  deliberate cost: merging `remote_free` into `FreeState`'s mutex would be *wrong* — it would
  force every `remote_evict` onto the owner's writer mutex and destroy the Phase-7b property the
  module exists to provide. The two-lock shape is the price of the lock-free-ish remote path; the
  only cheap improvement is the P1-2 pending-counter fast path, not a merge.
- **`Snapshot.pages: Vec<Arc<Vec<Slot<T>>>>` (`lock_free_region.rs:82`)** is two pointer hops and
  two allocations per page — acceptable at PAGE=64, but it is what makes the P1-4 O(pages) clone
  unavoidable without a representation change (chunked/persistent page table, or `ArcSwap` per
  page). Same frozen-tier caveat as P1-4.

## P3 — code quality / duplication / dead code

- **Reorg residue — one stale old-flat-path reference survived `72189f0d`.** The reorg commit
  claims "Updated every doc/comment reference to the old flat paths repo-wide", but
  `src/alloc_core/platform/node.rs:4` still says "the `concurrent::hand` discipline" — that module
  has been `concurrent::epoch::hand` since the regroup (prose, not an intra-doc link, so it
  compiles; `tests/no_stale_doc_references.rs` guards only the removed `Heap` entity, not these
  paths). Outside `src/concurrent/`, so left unmodified per this review's read-only scope — flag
  for whichever reviewer owns `alloc_core`. Inside `src/concurrent/` itself: no stale paths found
  (checked `hand.rs:9`, `epoch_region.rs:65`, all `super::` refs, and the three new `mod.rs`
  files).
- **Reorg otherwise clean.** `git diff -M 72189f0d^ 72189f0d -- src/concurrent` shows 7 of 9 moved
  files byte-identical (R100); the remaining diffs are doc-reference fixes plus new reexport-only
  `mod.rs` files — they comply with the "mod.rs = wiring only" rule. No duplication was introduced
  by the split.
- **Saturation-retirement semantics: doc drift in `lock_free_region.rs:139-145`, and a
  cross-tier divergence.** The struct doc says a slot whose generation "would reach `u32::MAX` on
  removal" is "retired … never reused", but the code (`:349-359`) retires only a slot *already
  at* MAX; a slot at MAX-1 is bumped **onto** MAX and threaded back onto the free list, so it is
  reused exactly once at generation MAX (that final gen-MAX handle is removable exactly once —
  sound, but one reuse more than the doc describes). The epoch tier does the opposite:
  `try_evict_at` retires the slot that *lands* on MAX (`epoch/hand.rs:394-406`). Both protocols
  are individually sound (I verified no ABA in either); the fix is to pick one semantics, then
  align the `lock_free` struct doc and note the difference (or unify) across tiers. Note the
  `remove`-method doc (`:322-325`, "if the slot's generation **is** `u32::MAX`") *does* match the
  code — it is the struct-level doc that is wrong.
- **Handle trait-impl triplication.** `epoch/epoch_handle.rs:48-76`,
  `lock_free/lock_free_handle.rs:47-75`, `sharded/sharded_handle.rs:64-94` hand-write the same
  six impls (Clone/Copy/PartialEq/Eq/Hash/Debug) over `(integers + PhantomData<fn() -> T>)` —
  ~90 duplicated lines that must be kept in sync. A shared macro (or a `pub(crate)` generic
  handle core re-exported under three names) would dedup while preserving the one-export rule.
  Low priority in a frozen tier.
- **`src/concurrent/pinning.rs:197` — the `R: Send` bound on `run` (and `:237` on `run_arc`) is
  unused**: worker returns are discarded at `:223`, and `R` is never captured by the spawned
  closure, so the bound over-constrains callers for no benefit. `run_arc` itself is a pure
  deref-coercion wrapper, but it is *not dead* — verified callers at `tests/pinning.rs:152,206`.
- **Dead-code audit of `hand.rs`'s `AtomicSlot<T>` (per the brief) — nothing is removable.**
  What I checked: `AtomicSlot` is the slot type of `EpochRegion` (`epoch_region.rs:65,128,156,285`)
  and the subject of 8 `#[cfg(kani)]` proof harnesses (`src/kani_proofs.rs:156-166`);
  `EvictOutcome` is consumed at `epoch_region.rs:327,336,376,389`; `generation()` by
  `drain_remote_free` (`epoch_region.rs:248`); `set_generation_for_tests` (`hand.rs:156`) by the
  `#[doc(hidden)]` `_set_slot_generation_for_tests` (`epoch_region.rs:426-428`), whose sole caller
  is the UBFIX-9 counterfactual test (`tests/epoch.rs:229`) — the established doc-hidden
  test-only-export pattern (integration tests cannot see `#[cfg(test)]` items, so it must stay
  ungated); `drop_value` by `EpochRegion::Drop` (`epoch_region.rs:445-449`). The only removable
  artifact in the file is `install`'s unused `_guard` parameter (P1-1).
- **Missing `#[inline]` on trivially small cross-crate `pub fn`s** (published crate surface under
  `experimental`): `epoch_region.rs:175` (`len`), `:181` (`is_empty`), `lock_free_region.rs:240`
  (`len`), `:246` (`is_empty`), `:275` (`contains`), `sharded/sharded_handle.rs:59` (`shard`),
  `sharded/sharded_region.rs:250` (`shard_count`), `pinning.rs:145` (`worker_count`). Cosmetic;
  benefit small but free.

## Follow-up measurement ideas

- **iai Ir A/B for dropping `epoch::pin()` from `EpochRegion::insert` (P1-1)** — a deterministic
  Ir counter on the insert path is the right judge (per CLAUDE.md's gate discipline); expect a
  small per-insert drop. Any report is `perf(opt-in)` at best (research tier, off `production`) —
  per the R30-12 taxonomy it must not be framed as a production speedup.
- **Paired A/B for the `remote_free` pending-counter fast path (P1-2)** — two arms: zero-remote
  churn (where the lock is pure overhead) and remote-evict-heavy churn (where it is contended).
  Needs a mechanism-activation oracle per the R30-8 rule (count fast-path hits vs lock
  acquisitions) so a NULL can't hide a non-activated arm.
- **`len` Relaxed (P1-3)** — cheap to include in the P1-1 iai run; on x86 expect ~0 (state that
  up front to avoid a surprising NULL); the real question is Arm codegen, which this host cannot
  measure.
- **If the lock-free tier is ever revived:** page-table representation A/B (current
  `Vec<Arc<Vec<Slot>>>` vs chunked directory) at 10³–10⁵ pages, wall-clock writer path plus an
  explicit reader-latency check (arc-swap load cost must not regress) — cost and benefit measured
  in the same workload regime per the R31-1 rule.
- **Saturation-unification test hook (P3 divergence):** the epoch tier has
  `_set_slot_generation_for_tests` to reach the MAX boundary without 2³² cycles; the lock-free
  tier has none, so its once-at-MAX reuse behavior is currently untestable under the
  short-scenario policy. If the semantics are unified, add the analogous doc-hidden hook for
  `LockFreeRegion` — that would also make the doc/code alignment mechanically checkable.
