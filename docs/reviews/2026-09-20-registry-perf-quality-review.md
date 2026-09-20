# registry perf/quality review — 2026-09-20

Scope: `src/registry/` only (`HeapCore` split, `HeapRegistry` slot table +
bootstrap chunk materialisation, `HeapSlot`, `HeapOverflow`,
`heap_core_xthread`). Review-only; no code changed.

## Summary

No new soundness bug found in the claim/recycle lifecycle or the cross-thread
free routing — the ABA defences (tagged `free_slots` head + slot-state CAS,
stable `'static` slot words across recycle, G1 resolve-before-publish,
wedge-safe sidecar materialisation) all check out against their own documented
arguments. The findings below are two genuine hot-path defects worth fixing
(one a redundant classification on the `calloc` path under the default feature
set; one a permanently-missing ownership-stamp cache on the fallback heap),
one doc/field contradiction around `HeapSlot::generation` that will mislead
the next auditor, and a block of mechanical-split duplication the split was
supposed to prevent. All citations are OBSERVED (file read this session); perf
effects are labelled as untested hypotheses unless a `docs/perf/` gate already
measures them.

## P0 — correctness/soundness

**None found.** What was explicitly checked and ruled out, so the next reviewer
does not re-derive it:

- **claim/recycle ABA** — `pick_slot`'s pop is authority-transferring and the
  `FREE → LIVE` CAS is the ownership gate (`src/registry/heap_registry/claim.rs:260-263`,
  `:70-75`); the head tag seals instead of wrapping
  (`src/registry/heap_registry/stack.rs:203-305`, ABA argument at
  `src/registry/heap_registry/mod.rs:20-44`). `Err(TagExhausted)` degrades to a
  documented one-slot leak, never a double-listing (`stack.rs:203-232`).
- **Stale-TLS / use-after-recycle** — slot words (`thread_free`, `overflow`,
  counters) are `'static` and never re-pointed across `recycle → claim`
  (`src/registry/heap_slot.rs:189-207`, `bind_slot_counters` at
  `src/registry/heap_registry/claim.rs:354-399`); ownership routing keys on the
  slot *index*, so a stamp from a previous occupant routes to the slot's
  current claimant, whose drains re-validate via `reclaim_offset(_checked)`
  (`src/registry/heap_core_xthread/ring.rs:23-38`).
- **`HeapOverflow` wedge/ABA** — sidecar ensured before the irreversible tail
  CAS (`src/registry/heap_overflow.rs:613-642`); a reserved-but-unpublished
  slot can never be re-reserved while the drain sits on it (`tail - head <
  CAP` invariant, `heap_overflow.rs:607`); `drain` returns the final `head`,
  not the entry `tail` (R2-4 fix, `heap_overflow.rs:734-747`, `:777-783`).
- **G1 resolve-before-publish** — `resolve_dirty_bit_target` runs only while
  the block still holds `live_count >= 1` (segment cannot be released under
  the freer); `apply_resolved_dirty_bit` touches no segment memory
  (`src/registry/heap_core_xthread/overflow.rs:203-213`, `:292-306`, `:658-674`).
- **Mid-claim UB window** — the `initialised` Release/Acquire publish pairs
  correctly with both aggregator walks (`claim.rs:95-104`;
  `src/registry/heap_registry/counters.rs:90-111`, `:163-179`), which after W3
  read only slot-resident atomics and never dereference `heap`.
- **`MaybeUninit` slot init** — OS-zeroed pages are a valid all-zero
  `RegistryChunk`/`HeapOverflowSidecar` (RAD-1 lazy links; `next_free = 0`
  never observed before a push) (`src/registry/bootstrap/ensure.rs:101-114`,
  `src/registry/bootstrap/mod.rs:93-103`).

## P1 — complexity / hot-path cost

### P1-1 — `alloc_zeroed` classifies twice on the default (non-`virgin-zero-skip`) path

`src/registry/heap_core/alloc/hot.rs:546-551` computes
`size`/`align`/`class_for(size, align)`; the small arm at
`src/registry/heap_core/alloc/hot.rs:607-617` then calls `self.alloc(layout)`,
which re-derives the identical triple at `src/registry/heap_core/alloc/hot.rs:63-79`.
Under plain `production` (which does not include `virgin-zero-skip`), every
`calloc`-shaped small allocation pays `class_for` twice plus the duplicated
`size().max(MIN_BLOCK)` / `align()`. This is exactly the redundancy Э9
(P7.1, task #160) removed inside `alloc` itself ("classify ONCE",
`hot.rs:53-62`) — the call-chain half of the same fix was never applied to
`alloc_zeroed`'s non-virgin arm. Failure mode is pure cost, not correctness.
Fix direction: an internal `alloc_classified(class, layout)`-style split (or
hoisting the small arm into `alloc_zeroed` the way the `virgin-zero-skip` arm
already does at `hot.rs:568-578`), keeping `pub fn alloc` as the sole
`GlobalAlloc`-shaped entry.

### P1-2 — fallback heap's OPT-C stamp fast path can never hit; `pack_owner` does not mask `owner_id`

`stamp_segment_owner`'s fast path validates the cache with
`unpack_owner_id(cur) == self.id` (`src/registry/heap_core/state/ownership.rs:146-158`).
The process-global fallback heap is constructed with `id = u32::MAX`
(`src/global/fallback.rs:217`; sentinel documented at
`src/registry/heap_core/core.rs:325-327`). But `unpack_owner_id` masks the id
field to 31 bits (`src/alloc_core/segment/segment_header/mod.rs:104`,
`:130-132`), so a word stamped as
`pack_owner(OWNER_STATE_LIVE, u32::MAX, 0)` (`ownership.rs:176`) unpacks to
`0x7FFF_FFFF` (`OWNER_ID_NONE`, `segment_header/mod.rs:114`) — never equal to
`u32::MAX`. Consequences, both CONFIRMED by code reading (perf effect
unmeasured; path is reached only when the 4096-slot registry is exhausted):

1. On the fallback heap the OPT-C cache *permanently* misses: every alloc from
   the same segment takes the slow path and re-executes the Release store
   (`ownership.rs:163-180`, `:234`) — the exact cost the cache exists to skip,
   on every fallback allocation forever.
2. `pack_owner` does not mask `owner_id` before shifting
   (`segment_header/mod.rs:121-125`), so `u32::MAX << 1` leaks a 1 into the
   generation bit (bit 32) of the stored `owner_state`. Nothing reads the
   generation bits today (`grep unpack_owner_gen` → no hits), so this is
   benign now, but it is a latent trap for any future reader of the
   generation field — and the same masking asymmetry is what breaks (1).

Fix direction: mask `owner_id` inside `pack_owner` (or stamp
`OWNER_ID_NONE`-clamped ids), and/or special-case the fallback heap in
`stamp_segment_owner`'s cache compare (compare against the *unpacked* id, or
give the fallback heap a self-consistent id). Also worth a one-line comment on
`HeapCore::id`'s sentinel warning that `u32::MAX` is not round-trip-stable
through `pack_owner`/`unpack_owner_id`.

(No O(n)-or-worse operation was found on any alloc/dealloc-path scan:
`contains_base` is O(1) hash (`routing.rs:35-53`), the dirty bitmap is O(dirty)
(`heap_slot.rs:209-244`), and the only linear scans in the subsystem are
test-only (`queries.rs:30-39`, `:318-335`) or bounded/defensive
(`drain.rs:257-277` dedup, ≤ 64 entries).

## P2 — types / representation

### P2-1 — `HeapSlot::generation` is production-dead while three doc sites claim it is the stamped coherence key

`HeapSlot::generation` is bumped by every claim (`claim.rs:75`, `:166`) and
read by **nothing** outside debug/test accessors (`bootstrap/registry.rs:316-340`;
the only other touch is the test hook `counters.rs:315`). The docs, however,
still assert the old design: `heap_slot.rs:8-10` ("The `generation` field is
the M8/M9 coherence key used elsewhere in the registry (segment-header owner
stamping)"), `heap_slot.rs:377-380` ("Combined with the slot index it forms
the unique `(index, generation)` owner key stamped into segment headers"), and
`src/registry/heap_core/core.rs:7-9` ("its **id** (its slot index + the slot's
`generation`), used by the 12.3 ownership stamping ... (`owner = heap id +
generation`)"). The actual stamp is `pack_owner(OWNER_STATE_LIVE, self.id, 0)`
— generation always 0 (`ownership.rs:176`), and the header side documents
exactly that ("packs a generation (always 0 ...)", `segment_header/mod.rs:105-108`).
The ownership-routing correctness argument nowhere depends on the generation
(the single-writer + slot-index-keying + drain-side revalidation chain above),
so this is not a soundness gap — but an auditor following the M8/M9 trail will
waste time looking for the coherence check that does not exist, and the field
(8 B/slot, always materialised within a touched chunk) is dead weight. Fix
direction: correct the three doc sites to state "bumped for future M8/M9 use;
never stamped today (always 0, see `pack_owner` call in `stamp_segment_owner`)"
— or, if the field is genuinely orphaned, propose removal as a tracked item
(it is `pub(crate)` and test-pinned via `dbg_slot_preset_generation`, so this
is an owner decision, not a drive-by).

### P2-2 — `HeapOverflow` parallel arrays: every ring op touches two cache lines (hypothesis, expected NO-GO — record for completeness)

The inline tier stores `bases: [AtomicPtr<u8>; 64]` and
`packed: [AtomicU32; 64]` as two parallel arrays
(`src/registry/heap_overflow.rs:363-374`); a push writes
`packed` then `base` (`heap_overflow.rs:656-657`) and the drain reads `base`
then `packed` (`heap_overflow.rs:762-773`), so each entry op touches a line in
each array (~2 lines vs 1 for interleaved 16-byte
`struct Entry { base, packed }` entries). Untested hypothesis, flagged only
because the crate's gate methodology would decide it cheaply — but the prior
is NO-GO: this ring is the *last-resort* tier (reached only after a full
`RemoteFreeRing` + retry exhaustion, `overflow.rs:684-686`), and interleaving
costs +33% inline-tier memory (768 B → 1 KiB per slot). Recommend leaving as-is
unless an overflow-heavy profile ever shows up in a flamegraph.

## P3 — code quality / duplication / dead code

### P3-1 — the magazine-issue block (`clear_magazine` + `hardened` `bump_gen`) exists in six near-identical copies

- `alloc` hit arm: `src/registry/heap_core/alloc/hot.rs:232-236` (clear) +
  `:243-253` (hardened bump)
- `alloc_small_zeroed_via_magazine`: `hot.rs:357-361` + `:362-372`
- `refill_magazine_slow_virgin` (issued block): `hot.rs:498-508`
- `refill_magazine_slow` (issued block): `hot.rs:773-791` (+ clear at
  `:766-772` for retained blocks)
- `alloc_batch` step 1: `src/registry/heap_core/alloc/batch.rs:164-176`
- `dbg_clear_magazine_on_hit` self-describes as a "byte-for-byte copy" of the
  production block: `src/registry/heap_core/diag/diag_probes.rs:384-394`

Under `hardened`, the hit-arm copies each compute
`os::segment_base_of_ptr(issued)` twice (once for `clear_magazine`, once for
`bump_gen` — `hot.rs:233` vs `:245`; `:358` vs `:364`); LLVM will CSE the pure
mask, but the source-level duplication is exactly the drift surface
`push_impl`'s own factoring rationale warns about ("written exactly once
rather than duplicated", `heap_overflow.rs:593-600`). The deliberate
alloc-vs-zeroed *call-site* split (F7, `hot.rs:324-330`) is fine — the shared
part is the 3-line issue *tail*, which could be one `#[inline(always)]
fn issue_magazine_block(...)` without changing any inlining outcome.

### P3-2 — `refill_magazine_slow` vs `refill_magazine_slow_virgin`: near-total body duplication

`hot.rs:687-793` vs `hot.rs:433-510` share the drain prelude, refill closure,
stamp-dedupe loop (`:746-756` vs `:465-475`), `mark_magazine` loop
(`:766-772` vs `:478-484`), and pop/issue tail; the virgin variant differs only
in the mask threading. A shared core parameterised by the virgin callback
would leave two thin wrappers. Both are `#[cold]`, so this is maintainability,
not perf.

### P3-3 — `drain_heap_overflow`'s two `#[cfg]` arms duplicate the R11-2/R12-6 emptied-bases machinery verbatim

`src/registry/heap_core_xthread/drain.rs:208-280` (fastbin) vs `:281-320`
(non-fastbin) differ only in `reclaim_offset_checked` vs `reclaim_offset`;
the entire `#[cfg(feature = "alloc-decommit")]` emptied-bases collection block
is copy-pasted (`drain.rs:257-277` vs `:297-317`), including the R12-6
overflow flag. This is the "helper copy-pasted into two sibling arms instead
of shared" shape the reorg was meant to prevent — a `note_emptied(...)`-style
local closure/fn would collapse ~40 duplicated lines.

### P3-4 — `tcache_hits_total` / `large_cache_hits_total` duplicate the aggregation walk

`src/registry/heap_registry/counters.rs:144-187` vs `:227-259`: identical
`count`-bounded walk, `initialised` gate, and saturating add, differing only in
the counter field and cfg gate. A shared `fn walk_slot_counters(f:
impl Fn(&HeapSlot) -> u64) -> u64` would keep the two gates at the call sites.
Cold path; drift-risk finding only.

### P3-5 — `dealloc_batch_small` re-implements `dealloc_own_thread_with_base`'s five guards

`src/registry/heap_core/free/dealloc_batch.rs:296-336` mirrors the F7/H1/M2
oracle chain of `free/dealloc_own_base.rs:196-341`, `:361-368` "in the same
order" by inspection — the docs say so explicitly ("can be diffed by eye",
`dealloc_batch.rs:76-97`, `:303-311`). The R17-4 history in this very tree is
the argument for why keyed-by-layout routing drift is expensive: a shared
`#[inline(always)] fn small_free_guards_ok(c, ptr, base) -> bool` would make
the mirror structural. Low priority (batch-api is experimental, no production
caller); note the existing `medium_promotion_reachable!` macro
(`free/dealloc.rs:69-102`) is the in-tree precedent for exactly this
"single source of truth for a cfg/guard predicate" pattern.

### P3-6 — minor lint-level nits

- `push_with_overflow_retry` is `#[inline]` on a ~270-line cold function with
  loops and a `sleep` (`src/registry/heap_core_xthread/overflow.rs:651-652`);
  the hint cannot be honoured and slightly misstates intent
  (`#[inline(never)]` or plain would match its callers' `#[cold]` discipline).
- `heaps_claimed_high_water`/`config_conflicts_total` read paths are fine; no
  other dead code found — the retired bulk-mode / streak machinery is cleanly
  gone (`state/tcache.rs:27-33`).

## Follow-up measurement ideas

- **classify-once `alloc_zeroed`** (P1-1): iai A/B under plain `production`,
  calloc-shaped churn harness (the r29_16/r30_3 style native gate would also
  need a path-activation oracle if a `virgin-zero-skip` interaction is
  claimed). Expect a small but real Ir delta on `alloc_zeroed` hot loops.
- **fallback-heap stamp fix** (P1-2): not worth a standalone gate — the path
  only runs under registry exhaustion; treat as `fix(perf)`-adjacent cleanup
  with correctness-of-comment value.
- **`HeapOverflow` interleaved entries** (P2-2): only if a future gate
  specifically saturates the overflow tier (e.g. a paused-owner fan-in
  wall-clock harness like `benches/heap_fanin_persistent.rs --reduced`);
  expected NO-GO per P2-2's reasoning.
- The duplication items (P3-1..P3-5) are refactor-only; if touched, pair with
  the existing iai suite to confirm zero Ir delta (the R28-1/R29-10 hooks
  already isolate the issue-block cost, so the A/B is one bench run).
