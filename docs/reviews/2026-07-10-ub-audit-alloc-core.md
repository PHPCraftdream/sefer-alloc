# UB / Memory-Safety Audit — `src/alloc_core/` (2026-07-10)

Scope: deep read of `alloc_core.rs`, `alloc_core_small.rs`, `alloc_core_small_pool.rs`,
`alloc_core_large.rs`, `alloc_core_large_cache.rs`, plus the unsafe seams they rest on
(`node.rs`, `alloc_bitmap.rs`, `remote_free_ring.rs` pack/unpack, `os.rs::segment_base_of_ptr`,
`segment_header.rs::align_up`). Read-only static audit; no code was executed or modified.

Classes searched: UB, use-after-free, double-free, double-allocation (same pointer issued
twice), dangling pointers, uninitialized reads, out-of-bounds, aliasing/data races,
incorrect `unsafe` usage.

Overall assessment: the hot paths (carve/pop/flush, large cache deposit/hit, pool
admit/evict, Drop) are internally consistent — live-count/decommit/recycle ordering, the
`off >= bump` stale-ring guard, the #134 `span_usable` carry-forward, and the R1
out-membership guard in `refill_class_bump_impl` all check out. The findings below are
gaps in the *defensive* (M2 "invalid free is a no-op, never corruption") contract and in
cross-thread edges, not in the happy path.

---

## Finding 1 — Missing payload lower-bound guard: invalid free of a metadata-region pointer corrupts segment metadata (incl. the primordial registry)

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1941-2013` (`dealloc_small`)
  - `src/alloc_core/alloc_core_small.rs:184-287` (`reclaim_offset`; also `reclaim_offset_checked`, lines 70-176)
  - `src/alloc_core/alloc_core_small.rs:810-1056` (`flush_run` guard pass)
- **Severity:** High (within the code's own stated M2 defensive contract; requires an invalid-free input)
- **Issue:** All small-free paths guard `off >= bump` (upper bound, decommit builds only)
  and the alloc-bitmap `is_free` bit, but **never check `off >= payload_start`**
  (`SegLayout::small_meta_end()`, or `primordial_meta_end()` for the primordial segment).
  The alloc bitmap covers the whole segment and is initialised to all-zeros
  ("allocated"), so a metadata offset *passes* the `is_free` guard. The `bump` guard
  passes too (`bump >= small_meta_end > off`). The hardened `off % block_size` check does
  not help for offsets that happen to be class-aligned (e.g. `off = 4096` for the 4096
  class, or `off = 64` for the 64 class).
- **Scenario:** a caller (or, on the ring path, a cross-thread freer with a bad pointer)
  frees `base + 4096` with a small `Layout` where `base` is one of this heap's registered
  segments. `dealloc` → `contains_base` passes → `kind` is Small/Primordial →
  `dealloc_small(base, base+4096, class)`. All guards pass; `Node::write_next` stores an
  8-byte pointer **into the segment header / page map / bin table region**, then
  `bt.set_head(class, 4096)` publishes that offset. The next `alloc` of that class pops
  it and returns a pointer into segment metadata — from then on user writes overwrite the
  BinTable/PageMap/AllocBitmap (and, on the **primordial** segment, the self-hosted
  `SegmentTable` registry itself lives in `[small_meta_end, primordial_meta_end)` —
  corrupting it desynchronizes every later `contains_base`/`unregister` decision →
  arbitrary UAF/double-release downstream).
- **Why it matters despite being caller error:** `dealloc`'s doc explicitly promises
  "foreign pointer or double-free is a no-op (M2 — never UB, never corrupts the
  allocator)". This input is exactly the "defence-in-depth" class the guards exist for,
  and it corrupts the allocator.
- **Fix direction:** add an unconditional lower-bound rejection alongside the bump guard
  in `dealloc_small`, `reclaim_offset`, `reclaim_offset_checked`, and `flush_run`'s
  per-block guard pass: `if (off as usize) < SegLayout::small_meta_end() { return; }`
  (for the primordial kind, compare against `primordial_meta_end()`). One integer compare
  against a compile-time constant — negligible cost, closes the whole class.

---

## Finding 2 — `off >= bump` guard is `alloc-decommit`-gated: without the feature, an invalid free of an *uncarved* offset leads to double allocation

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1976-1979` (`dealloc_small`, `#[cfg(feature = "alloc-decommit")]`)
  - `src/alloc_core/alloc_core_small.rs:243-245` (`reclaim_offset`, same gate)
  - `src/alloc_core/alloc_core_small.rs:113-115` (`reclaim_offset_checked`, same gate)
  - `src/alloc_core/alloc_core_small.rs:871-874` (`flush_run`, same gate)
- **Severity:** Medium
- **Issue:** the `off >= bump` check exists only under `alloc-decommit` (it was added for
  the post-decommit stale-free case). In a build **without** `alloc-decommit` there is no
  guard at all against an offset in the never-carved region `[bump, SEGMENT)`: the bitmap
  bit there is 0 ("allocated"), so an invalid free of such an offset is accepted,
  `write_next` writes into the (committed but uncarved) payload, and the offset is pushed
  onto the class free list.
- **Scenario (double allocation):** with `alloc-decommit` off, a garbled ring entry or an
  invalid own-thread free targets `off = bump + k*block_size` (class-aligned, below
  `SEGMENT`). It is linked onto the freelist. A later `alloc` pops it and hands out
  `base + off`. Subsequently, the bump cursor advances past `off` and `carve_block`
  **hands out the same bytes again** to a second caller — two live allocations aliasing
  the same memory (silent data corruption for the user).
- **Fix direction:** make the `off >= bump` rejection unconditional (drop the `cfg`
  gates). `bump_of()` is a single owner-side word load present in every build; the guard
  is equally valid without decommit and closes the double-allocation window.

---

## Finding 3 — Data race on the `SegmentHeader` between the owner's Large-free full-struct write and remote header field reads (`alloc-xthread`)

- **Files/lines:**
  - `src/alloc_core/alloc_core.rs:743, 826-828` (`dealloc` Large branch: `SegmentHeader::read_at(base)` then `Node::write_struct(base as *mut SegmentHeader, hdr_zero)`)
  - `src/alloc_core/alloc_core_large.rs:346-348` (`reclaim_large_segment`: same `write_struct` of the zero-magic header)
- **Severity:** Medium (requires a concurrently in-flight remote free of the same segment — a caller contract violation — but it converts the promised "safe no-op" into formal UB)
- **Issue:** the codebase's own §11 / task #33 discipline eliminated full-struct header
  writes on the small path precisely because a full-struct `write_struct` races remote
  field-specific reads (`magic_at`, `kind_at`, `large_size_at`, `owner_thread_free_at` in
  `HeapCore::dealloc_routing`). The **Large** free path still performs a full-struct
  write of the header (zeroing `magic` rewrites every byte, including the fields remotes
  read non-atomically). `table.unregister(base)` runs first, but the owner's table is not
  synchronized with a remote's routing check, so a remote that passed routing can still
  be mid-read of `magic`/`kind` when the owner's `write_struct` lands — an unsynchronized
  non-atomic write/read pair on the same bytes: a data race (UB), independent of the
  later `release_segment` unmap (which is the separate, documented "(a)/(b)
  indistinguishable" dangling-free hazard).
- **Scenario:** thread A owns a Large segment and legitimately frees it; thread B
  simultaneously double-frees (or frees a stale copy of) the same pointer. B's
  `dealloc_routing` reads `magic_at(base)` concurrently with A's `write_struct(hdr_zero)`.
- **Fix direction:** on the cache-deposit / reclaim paths, zero only the `magic` field via
  a field-specific (ideally atomic, e.g. `atomic_u32_at(base, offset_of!(.., magic))`
  Relaxed store) write instead of rewriting the whole struct; the remaining header fields
  are unchanged by `hdr_zero` anyway (it is `stale` with only `magic = 0`). That reduces
  the racing footprint to a single word that could then be made formally race-free.

---

## Finding 4 — fastbin residual: unchecked ring drain via `alloc_small`/`find_segment_with_free` can double-issue a magazine-resident block and, with `alloc-decommit`, release a segment whose blocks are still in the magazine (UAF)

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1201-1207` (`find_segment_with_free` — default predicate `&|_, _| false`)
  - `src/alloc_core/alloc_core_small.rs:1089-1092` (`alloc_small` step 2 uses the unchecked variant)
  - `src/alloc_core/alloc_core_small_pool.rs:76-111` (`dec_live_and_maybe_decommit` fires from the reclaim of the duplicate)
- **Severity:** Medium (verdict: PLAUSIBLE — depends on whether `HeapCore` can reach `alloc_small`/unchecked `find_segment_with_free` in a fastbin build; the checked variants exist precisely because the residual is real per task #164)
- **Issue:** task #164 closed the ring↔magazine cross-thread double-free leg only on the
  *checked* call chain (`refill_class_bump_checked` → `find_segment_with_free_checked`).
  `alloc_small` itself (and any other caller of plain `find_segment_with_free`) drains
  rings with a constant-`false` magazine predicate. A cross-thread double-free note for a
  block currently resident in the owner's magazine then passes every guard
  (bitmap reads "allocated"), is `write_next`-linked onto the freelist, and `dec_live`d:
  1. the block is now simultaneously in the magazine and on the freelist → the next
     substrate pop issues it while the magazine copy is still outstanding →
     **double allocation**;
  2. worse, the spurious `dec_live` can drive `live_count` to 0 while magazine-resident
     pointers into the segment still exist → `release_or_pool_empty_segment` may
     **release the reservation** (`table.recycle` → `munmap`/`MEM_RELEASE`); a later
     magazine flush then does `SegmentMeta::new(base)` metadata reads/writes on unmapped
     memory → **use-after-unmap**.
- **Fix direction:** either route *every* ring drain reachable in a fastbin build through
  the checked predicate (thread the magazine predicate into `alloc_small`'s
  `find_segment_with_free` call), or verify and document (with a debug assert) that
  `alloc_small` is unreachable for magazine-managed classes under `fastbin`. The
  generation guard (`hardened`) already mitigates this probabilistically; the structural
  fix is predicate coverage.

---

## Finding 5 — Intrusive `next` pointers are trusted without an in-segment bounds check (user write-after-free → out-of-segment pointer handed out)

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1455-1463` (`pop_free`: `(next as usize - segment as usize) as u32`)
  - `src/alloc_core/alloc_core_small.rs:1738-1750` (`drain_freelist_batch`, same pattern)
- **Severity:** Low (inherent to intrusive freelists; mimalloc/jemalloc share the exposure, and the trigger is a user heap bug)
- **Issue:** a user write-after-free clobbers the first word of a freed block. `pop_free`
  reads it as `next`, computes `next - segment`, truncates to `u32` (silently wrapping
  for an out-of-segment address), stores it as the new head; the *following* pop does
  `Node::deref(segment, head_off)` with an offset up to `u32::MAX` — up to 4 GiB past the
  segment base — and hands that pointer to a caller (out-of-bounds allocation → arbitrary
  corruption).
- **Fix direction:** under `hardened`, validate `next` before accepting:
  `next == null || (next as usize).wrapping_sub(segment as usize) < SEGMENT` (and
  optionally block-size alignment); on failure, drop the rest of the chain (set head to
  NULL) — fail-safe, no panic. Mirrors mimalloc's `MI_SECURE` freelist checks.

---

## Finding 6 — `AllocCore::drop` races in-flight remote ring pushes (releases segments whose rings a remote may still be CASing)

- **File/lines:** `src/alloc_core/alloc_core.rs:1415-1466` (`Drop`)
- **Severity:** Low / informational
- **Issue:** `Drop` releases every reservation with no quiescence handshake against
  remote threads that may be mid-`RemoteFreeRing::push` (the ring lives in the segment's
  metadata pages being unmapped). In production this is moot — registry `HeapCore`s are
  never dropped (documented in `node.rs`'s LIFETIME notes) and a standalone `AllocCore`
  is `!Sync` — but any future "drop a heap mid-process" feature would turn this into a
  use-after-unmap. Worth a doc-level invariant ("an `AllocCore` must not be dropped while
  any other thread can hold pointers into its segments") pinned where `Drop` lives.

---

## Finding 7 — Test-only `dbg_*` accessors dereference segment metadata without an ownership check

- **Files/lines:** `src/alloc_core/alloc_core_small.rs:411-414` (`dbg_freelist_head_for`),
  `:423-427` (`dbg_is_free_for`), `:434-442` (`dbg_drain_freelist_batch`);
  `src/alloc_core/alloc_core.rs:1021-1032` (`dbg_segment_id_of` / `dbg_stamp_segment_id`),
  `:1039-1043` (`dbg_large_size_of`)
- **Severity:** Low (test-only, `#[doc(hidden)]`, but `pub`)
- **Issue:** unlike `dbg_node_id_for`/`dbg_live_count_for` (which check
  `contains_base_ro` first), these derive `base = segment_base_of_ptr(ptr)` and read (or,
  for `dbg_stamp_segment_id`, **write**) at fixed offsets from it unconditionally. A test
  passing a non-heap pointer reads/writes unrelated (possibly unmapped) memory.
- **Fix direction:** add the same `contains_base_ro` early-return the sibling accessors
  use.

---

## Checked and found sound (non-exhaustive highlights)

- **Large-cache accounting/lifetime** (`alloc_core.rs` dealloc Large branch,
  `alloc_core_large.rs` hit path, `alloc_core_large_cache.rs` evict/decay, `Drop`):
  deposits unregister before caching, evictions release exactly once, `Drop` frees the
  cache first and then only table-registered bases — no double-release path found.
  `span_usable` carry-forward (bug #134) is correctly applied on both deposit sites.
- **`flush_run` release safety:** a segment cannot be released mid-`flush_class` while a
  later run in the same batch still holds its blocks — those blocks count as live, so
  `live_count` cannot reach 0 before the last of them is flushed (single-thread path).
- **Pool (Mechanism 2):** pooled segments stay registered+committed; `unpool_if_present`
  on reuse prevents double-pooling; drain/decay release via `recycle` after the bump
  reset, so intra-drain stale ring entries are rejected by the `off >= bump` guard.
- **Ring entry offsets:** `unpack_entry` masks to 22 bits (`off < SEGMENT`) and
  `unpack_entry_hardened` to 18+4 bits — a garbled entry cannot produce an
  out-of-segment `Node::deref`. Class index is bounds-checked before `block_size`.
- **`align_up` overflow:** `q * a` could wrap only for sizes exceeding the
  `Layout` isize-invariant; all call sites receive `Layout`-validated sizes (the realloc
  slow path re-validates via `Layout::from_size_align`, the OPT-G path uses
  `checked_add`). Not reachable.
- **`carve_batch`/`drain_freelist_batch`/`dec_live_batch` equivalence proofs:** verified
  against the per-block paths; bounds (`aligned_start + n*block_size <= SEGMENT`,
  `FLUSH_RUN_DETECT_CAP` array indexing, RunStack remainder pushback) hold.
- **`node.rs` seam:** all `unsafe` blocks carry accurate SAFETY proofs; the
  `expose_provenance` handling in `atomic_ptr_ref` (task #142) and the honest non-static
  `'static` caveats on `atomic_*_at` match the call sites reviewed.

## Coverage note

`segment_table.rs`, `segment_header.rs` (beyond `align_up`/gen-table entry points),
`remote_free_ring.rs` push/drain internals, `run_stack.rs`, `bootstrap.rs`, and `os.rs`
internals were reviewed only at the seam-contract level, not line-by-line; a follow-up
pass over those files (especially the ring's producer CAS protocol and the table's
open-addressing hash) would complete the audit.
