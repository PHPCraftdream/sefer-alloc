# Phase 35 — M6 decommit (return empty segments to the OS), M11-free

Design spec (written before implementation). Closes the only honest gap in
ALLOC_BENCH: RSS is unbounded (empty segments are not returned to the OS).
Under feature flag `alloc-decommit` (default off — default behavior unchanged).

## 0. What already exists
- OS-seam READY: `os::decommit_pages(base, start, end)` (Win `VirtualFree
  MEM_DECOMMIT` / unix `madvise(MADV_DONTNEED)` / miri no-op) and
  `os::recommit_pages` (Win `VirtualAlloc MEM_COMMIT` / unix implicit / miri
  no-op). Contract: call on a live segment WITHOUT live blocks in the range.
- MISSING: tracking of live blocks (`live_count`) and policy/wiring.

## 1. M11 does NOT require epoch (key insight — justify in code)
Plan §2.5 designed M11 via crossbeam-epoch because the OLD model of intrusive
cross-thread free wrote `next` INSIDE the block — the freer could write into a
decommitted page. **Variant-2 (Phase 12.6) dissolved this:** cross-thread
freer does NOT dereference the block — it pushes `(offset|class)` into
`RemoteFreeRing`, located in the segment's METADATA (metadata pages are NEVER
decommitted).
Proof that decommit is safe WITHOUT epoch:
1. We decommit segment payload ONLY when `live_count == 0` → there are no live
   blocks in the decommitted range.
2. A late valid cross-thread free is impossible when live_count==0: all blocks
   are already free; a repeated free of a free block = double-free.
3. `reclaim_offset` (owner-side) on a stale ring entry: computes the address
   `Node::deref(base,off)` (WITHOUT memory access), reads magic/kind/**bitmap
   is_free** — ALL in metadata — and for a free block (and at live==0 all are
   free) performs a no-op BEFORE any write to the block. Does not touch the
   decommitted page.
4. `reclaim` and `decommit` are both owner-side → serialized on the owner
   thread; no reclaim-vs-decommit race on the same segment.
⇒ UAF/writes to decommitted memory do not occur. epoch/crossbeam NOT needed.
(Record this reasoning in code/docs as justification for "why no M11-barrier".)

## 2. live_count (owner-only, no atomics)
All live_count mutations happen on the owner: own-thread alloc/free + owner-side
reclaim. Cross-thread freer does NOT touch live_count (it pushes into the ring;
the owner decrements during reclaim). ⇒ plain `u32` field, not atomic.
- Field in `SegmentHeader` (new, owner-only). Access field-specific (offset_of!),
  like bump (owner-only).
- Semantics: count of ISSUED (carved-and-not-free) blocks.
  - `pop_free` (issuance): `live += 1`.
  - `carve_block` — block returned TO THE CALLER: `live += 1`. Refill-blocks
    (go straight into the free list): live is NOT changed.
  - `dealloc_small` (own-thread free): `live -= 1`.
  - `reclaim_offset` (cross-thread reclaim): `live -= 1`.
  - Consistency with bitmap: live == (total carved) − (free). Can be verified
    with debug_assert by counting, but in release — the counter.

## 3. Decommit policy (conservative for start)
In `dealloc_small`/`reclaim_offset` AFTER the decrement: if `live_count == 0`
AND the segment is NOT the current carve target (`base != small_cur`; in
HeapCore — not current for any class) →
1. Decommit payload `[small_meta_end, SEGMENT)` (metadata — header/page_map/
   bin_table/**alloc_bitmap**/ring — stays committed: read by cross-thread).
2. RESET segment to clean-empty: `bump = small_meta_end`, all BinTable heads =
   FREE_LIST_NULL, page_map payload-pages = Free, **bitmap = 0**. (Safe:
   live==0, no live blocks.) This turns the emptied segment into a blank for
   reuse.
3. Set a `decommitted` flag in the header (new field/bit).
Under feature flag `alloc-decommit`; without it — current behavior (segment
stays committed, reused via free list — but free list is empty after reset...
NB: without decommit we do NOT reset — leave as-is).

## 4. Recommit on reuse
When a decommitted segment is selected again as `small_cur` / being carved:
- `carve_block`: if the `decommitted` flag is set and we are about to write to
  payload → `os::recommit_pages(base, small_meta_end, SEGMENT)` (Win explicit
  commit; unix implicit), clear the flag. Simplest approach: recommit all
  payload on first reuse (not per-page lazy — simpler and correct).
- Alternative: reuse decommitted segments less eagerly — let alloc reserve
  a fresh segment, and leave decommitted ones as RSS-return until explicit
  revisit. Choose the simpler-correct option; recommit-on-reselect is sufficient.

## 5. Tests (mandatory; safety-sensitive)
- `tests/decommit_soak.rs` (feature flag alloc-decommit): sustained churn,
  emptying segments; assert that decommit IS CALLED when live→0 (call counter
  via test-seam) and that after reuse data is correct (write/readback on
  recommitted pages). Under miri (decommit no-op) — assert BOOKKEEPING
  (live_count→0→decommit-hook called, reset correct), not RSS.
- **miri** on decommit/recommit cycle (bounded): no UAF, no access to "foreign"
  memory.
- Regression: full suite + race ×5 green WITH feature flag and without.
  Especially: cross-thread reclaim of a stale entry into a decommitted segment →
  bitmap no-op (add test: decommit segment, push a stale-offset into its ring,
  drain → no-op, no panic/access).
- (Heavy gate #32) TSan under WSL on decommit path.

## 6. Scope/risk
Safety-sensitive (UAF on decommitted pages — worst failure mode). The proof in
§1 (epoch not needed) is load-bearing; the implementation is mechanical. Run
under miri + soak + (in #32) TSan. Feature flag default-off: default does not
risk.

## 7. Out of scope
- Aggressive policies (immediate decommit vs deferred/timer-based) — start with
  conservative (decommit at live==0 non-current), tuning — later by RSS
  measurement.
- RSS metric in macro-bench — separate (requires a platform probe); after that,
  update the ALLOC_BENCH RSS section (currently N/A).
