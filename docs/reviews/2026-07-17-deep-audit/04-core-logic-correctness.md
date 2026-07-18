# 04 — Allocator core logic correctness (read-only audit)

**Scope:** `src/alloc_core/` (alloc_core.rs, alloc_core_large*.rs,
alloc_core_small*.rs, bootstrap.rs, segment_header*.rs, segment_directory.rs,
segment_table.rs, size_classes.rs + `crates/size-classes`), plus the
invariant catalogue in `docs/INVARIANTS.md`.

**Method:** static reading only — no cargo/build/test, no git mutation.
Traced every carve/refill/magazine path, the R7-A segment-directory
empty↔non-empty transition sites, the R7-B incremental-commit frontier
arithmetic, bootstrap/primordial layout composition, realloc fast/slow
paths, and `SegmentTable` register/unregister/recycle. Cross-checked the
already-landed `R7-A7` fix (`42f8343`, "clear directory bits on the
lazy-commit pool-admission path") against every other segment-emptying call
site to see whether a sibling desync survived it.

**Note on invariant naming.** The task brief says "I1–I6"; those IDs are
defined in `docs/INVARIANTS.md` for the **Handle/slotmap** face (`insert`/
`get`/`remove`), not the allocator substrate. The allocator substrate's own
invariants are **M1–M8** (same file, "Allocator invariants (Phase 8+,
`alloc-core`)" section). This report audits M1–M8 plus the four
correctness domains named in the brief (size classification, carve/refill,
realloc, segment lifecycle/bootstrap) — I1–I6 are out of scope for
`alloc_core` (they belong to `src/handle`/slotmap, not audited here).

## Summary

No new correctness defect was found in the core allocator logic. The
codebase in this area has already been through many audit/fix rounds
(R6-MS-1..5, R7-A0..A7, R7-B0..B6, UBFIX-1..12); every carve, refill,
directory-transition and commit-frontier path this review traced is
internally consistent, and the one **known** lurking desync class (segment
emptying without clearing the R7-A1 directory bitmap) has a fix already
landed and verified against every call site that empties a segment,
including the one A7 fixed and its siblings. The single open item is a
pre-existing, already-tracked, test-only hygiene gap (task #191) — not a
production-logic bug.

## I1–I6 / M1–M8: status of each

| ID | Statement (abbreviated) | Status | Note |
|----|---|---|---|
| M1 | validity: non-null, sized, aligned | Holds | `class_for` guarantees `block_size >= max(size,align)` and `block_size % align == 0`; large path aligns via `hdr_aligned = align_up(size_of::<SegmentHeader>(), align.max(PAGE))`. |
| M2 | no double-free/UAF (documented `unsafe fn` boundary) | Holds, by design | O(1) alloc-bitmap `is_free` guard on every free path (own-thread, batched flush, ring reclaim); the residual (ring↔magazine in-flight window) is a **documented, tested, accepted** limit, not a new finding. |
| M3 | no overlap | Holds | Bump-carve is strictly monotone (`aligned_bump + block_size <= SEGMENT`, checked before advancing `bump`); free-list reuse only recycles a block whose bitmap bit was set free by a prior legitimate free. |
| M4 | alignment & size fidelity | Holds | `size-classes` crate's `class_for` fast path (`align <= min_block`) and slow "divisibility-jump" path both verified; `align >= SEGMENT` large requests are rejected with null (task #130), not silently misaligned. |
| M5 | reentrancy-freedom | Holds | Traced: no `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` anywhere in the carve/refill/directory/bootstrap paths read for this report; all metadata self-hosts in segment memory via the `node`/`os` seams. |
| M6 | OS return (decommit) | Holds | `dec_live_and_maybe_decommit` / batched variant / `release_or_pool_empty_segment` correctly gate on `live_count==0 && base != small_cur && !is_decommitted && kind==Small`, excluding the primordial segment explicitly. |
| M7 | owner routing O(1) | Holds | `segment_base_of_ptr` mask + `SegmentTable::contains_base` (hash + direct cache). |
| M8 | generational coherence (handle face) | N/A here | Belongs to the slotmap/Handle face, not `alloc_core`. |

## Findings

No CONFIRMED or PLAUSIBLE defects survived verification in this domain.
Everything below is cross-reference / already-tracked, included for
completeness of the audit trail rather than as new action items.

### Already-fixed, re-verified: R7-A7 directory desync on pool-admission decommit-reset

- **file:line:** `src/alloc_core/alloc_core_small_pool.rs:253-275`
- **status:** FIXED (commit `42f8343`), re-verified consistent in this pass.
- **What was wrong (historical):** `release_or_pool_empty_segment`'s
  pool-admission branch called `decommit_empty_segment_impl(.., false)`
  (full metadata reset, zeroing every `BinTable` head) under
  `alloc-lazy-commit`, but did not clear the R7-A1 segment-directory bitmap
  — so bits set by earlier `publish_nonempty` calls survived the reset as
  stale positives, desyncing the incrementally-maintained directory from a
  fresh `rebuild_from_table` at the top medium class under
  `--all-features`.
  **Verification performed in this pass:** re-audited every other
  segment-emptying call site for the same class of gap —
  `release_or_pool_empty_segment`'s release branch
  (`alloc_core_small_pool.rs:284-292`), `maybe_decay_small_pool`
  (`:488-493`), and `drain_small_pool` (`:578-583`) — all three already
  call `clear_segment_directory(slot_idx)` before recycling. No sibling gap
  found.

### Non-issue verified: `publish_nonempty`-then-immediate-empty sequencing in `dealloc_small`

- **file:line:** `src/alloc_core/alloc_core_small.rs:1299-1321`
- `dealloc_small` calls `publish_nonempty(class_idx, slot_idx)` (line
  1302-1306) *before* the `dec_live_and_maybe_decommit` /
  `release_or_pool_empty_segment` sequence a few lines later (1318-1321)
  that can immediately empty and reset/release the very same segment.
  Verified this is sound: `release_or_pool_empty_segment`'s
  `clear_segment_directory`/decommit-reset path clears **all** classes for
  the slot (`SegmentDirectory::clear_slot` loops every class), so a bit set
  moments earlier for `class_idx` is unconditionally subsumed by the
  following clear-all. Set-then-clear-all is safe under single-writer,
  single-thread ordering (the owner thread executes both steps with no
  interleaving possible). Not a defect.

### Non-issue verified: `sync_directory_for_segment` runs before the post-drain decommit/pool routing

- **file:line:** `src/alloc_core/alloc_core_small.rs:610-646` (production
  `find_segment_with_free_impl`) and `alloc_core_small_reclaim.rs:482-496`
  (test-only `dbg_drain_all_rings_impl`, identical ordering).
- Both call `sync_directory_for_segment(base, slot_idx)` — a full
  per-class resync from the live `BinTable` state — immediately after a
  ring drain, and only afterwards check `decommit_happened` to route into
  `release_or_pool_empty_segment`. Verified this ordering cannot leave a
  stale bit: if the drain's last reclaimed block emptied the segment (the
  only way `decommit_happened` is true), the resync at that point reads a
  BinTable that is about to be reset by the pool/release path anyway, and
  the subsequent `release_or_pool_empty_segment` unconditionally clears (or
  the release-follows-fast-path recycles the slot, which also clears via
  `clear_segment_directory` before `table.recycle`). Same
  set/resync-then-clear-all safety argument as above. Not a defect.

### Cross-reference (not re-litigated): task #191, HYGIENE F2

- Already tracked as a pending task (`lazy_commit b2/b4 tests assert
  frontier==SEGMENT on the unreachable unix∧lazy∧¬numa leg`). This is a
  **test assertion** correctness gap (the test's own oracle is wrong on an
  unreachable feature leg), not a defect in `alloc_core`'s production
  carve/commit logic — the commit-frontier arithmetic itself
  (`committed_payload_end`, `carve_block`/`carve_batch`'s
  commit-before-publish ordering in `alloc_core_small.rs:1011-1044` and
  `:1148-1175`) was independently re-verified in this pass and is correct:
  on every grow-on-carve call, `os::commit_pages` is attempted and only on
  success does `set_committed_payload_end` run, and only after that does
  `set_bump` advance — no path publishes an uncommitted offset. Not
  re-reported as a new finding here; left to whichever audit track owns
  test-quality (per the task split, that is AUDIT-8).

## Areas traced with no defect found

- **Size classification** (`crates/size-classes/src/lib.rs`,
  `src/alloc_core/size_classes.rs`): `build_table`'s two-pointer geometric/
  extras merge, `build_size2class`'s monotone-pointer O(1) derivation, and
  `class_for`'s fast (`align <= min_block`) / slow (divisibility-jump)
  paths were traced for off-by-one and underflow risk. `need = max(size,
  align)` is always `>= 1` (caller clamps `size` to `MIN_BLOCK`, `align` is
  never 0 per the `Layout` contract), so `(need - 1) >> shift` never
  underflows. The slow-path jump's termination argument (`next_mult >
  block` implies the re-seeded index is strictly greater, so `i` is
  monotone) holds given `table` is strictly increasing, which `build_table`
  guarantees by construction (each geometric step enforces `next > cur`, a
  minimum step of `min_block`, and `extras` is a caller-verified sorted,
  disjoint list merged by comparison).
- **Carve/refill/magazine** (`alloc_core_small.rs`,
  `alloc_core_small_magazine.rs`): `carve_block`/`carve_batch`'s
  commit-before-publish ordering, the batched refill's "free-drain before
  bump-carve" source-order invariant (non-negotiable per its own doc, and
  actually preserved in `refill_class_bump_impl`), and `flush_class`'s
  same-segment run-splicing (with the `L-4`/UBFIX-11 double-recycle-within-
  one-call guard, `RECYCLED_CAP`-bounded) were all read against their
  documented byte-identical-to-per-block-path proofs; the proofs hold up
  under adversarial re-reading (no missed guard, no reordered write).
- **Realloc** (`alloc_core.rs:1098-1428`): OPT-F (small→small same-class,
  `==` not `<=`, correctly justified against the free-list-corruption
  scenario the doc describes for page-aligned classes) and OPT-G
  (large→large in-place grow, `checked_add` guarded, `MIN_BLOCK`-clamped
  identically to the alloc path so a later cross-thread
  `large_layout_consistent` check cannot desync) both re-derived
  independently and found sound. The slow-path move leg's
  `safe_payload_read_span` upper-bound (computed from header metadata, not
  the caller's `Layout`) correctly prevents an OOB read from a bogus
  `old_layout.size()` on the safe (non-`unsafe fn`) call surface —
  verified the bound is taken from `span_usable_at`/`SEGMENT`, never from
  caller input.
- **Segment lifecycle** (`segment_directory.rs`, `segment_table.rs`):
  `rebuild_from_table`'s one-time full walk (skip null/large, set only
  non-empty heads on an OS-zeroed sidecar) is correct; `register`'s O(1)
  free-list-pop-or-append and `unregister`/`recycle`'s O(1)
  `segment_id`-indexed slot lookup with defensive `current != base`
  no-ops (rather than corrupting the table on a stamped-id mismatch) were
  traced and are consistent with the hash-table/own-cache invalidation
  ordering documented at each site (evict-before-release, never
  release-before-evict).
- **Bootstrap/primordial** (`bootstrap.rs`, `segment_header_layout.rs`):
  the `Layout::primordial_meta_end()`/`small_meta_end()` offset chain
  (header → page map → bin table → alloc bitmap → magazine bitmap →
  remote ring → [gen table under `hardened`] → [registry/hash/free-list
  for primordial only]) is composed with `align_up_const` at every step
  and pinned by four ungated/gated `const` assertions
  (`segment_header.rs:1060-1084`) that would fail the build (not corrupt
  at runtime) if a future metadata region grew past `SEGMENT`. The R7-B6
  lazy-commit primordial path's `initial_commit = primordial_meta_end() +
  LAZY_FIRST_CHUNK` is asserted `<= SEGMENT` at both the call site
  (debug_assert) and compile time (const assert at
  `segment_header.rs:1070-1072`), and every write in `bootstrap::primordial`
  lands strictly inside that committed prefix by construction (the whole
  metadata region is committed up front, not incrementally alongside the
  writes).

## Top findings

None survived verification as a new CONFIRMED/PLAUSIBLE defect. Ranked
observations from this pass, most notable first:

1. **(informational)** R7-A7's fix is complete and correctly generalizes:
   all three segment-emptying call sites in
   `alloc_core_small_pool.rs` (pool-admission, decay-eviction, forced
   drain) now clear the directory before recycling; no sibling desync
   remains.
2. **(informational)** The `publish_nonempty`-then-immediate-clear-all
   sequencing in `dealloc_small` and the `sync_directory_for_segment`-
   then-decommit-routing sequencing in both drain sites are safe by the
   same "clear-all always wins" argument — worth keeping in mind for any
   future change that makes the directory clear finer-grained (e.g.
   per-class instead of `clear_slot`'s all-classes sweep), since that
   would reopen exactly this ordering as a real hazard.
3. **(cross-reference only)** Task #191 (test-oracle bug on an
   unreachable feature leg) is pending under a different audit track;
   the production commit-frontier logic it references was independently
   re-verified correct in this pass.
