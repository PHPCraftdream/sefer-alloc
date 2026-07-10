# UB / memory-safety audit — segment & bitmap layer (2026-07-10)

Scope: `src/alloc_core/` — `segment_header.rs`, `node.rs`, `alloc_bitmap.rs`,
`alloc_core_small.rs` (carve/free/reclaim/flush), `alloc_core.rs` (dealloc/realloc
routing, Drop), `alloc_core_large.rs`, `alloc_core_small_pool.rs` (decommit),
`remote_free_ring.rs`, `run_stack.rs`, `segment_table.rs`, `bootstrap.rs`, `os.rs`.
Read-only audit; no code changed.

Hunted classes: UB, use-after-free, double-free, double-allocation, dangling
pointers, uninitialized reads, out-of-bounds, aliasing/data races, incorrect
`unsafe` usage.

Overall: the core disciplines (single-writer bump/bitmap/BinTable, offset-only
remote ring, field-specific header accessors, `off >= bump` decommit guard,
compile-time layout asserts) are coherent and carefully argued. The findings
below are gaps at the edges of those disciplines — mostly in the *defensive*
paths whose documented contract ("a bad free is a no-op, never corrupts") is
not fully upheld.

---

## Finding 1 — Missing payload-start lower bound: an aligned free into the METADATA region corrupts the segment header / page map / bin table

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1941–1997` (`dealloc_small`; guards at 1962, 1977, 1982; `write_next` at 1995)
  - `src/alloc_core/alloc_core_small.rs:184–287` (`reclaim_offset`; alignment guard 224–226, `write_next` 272)
  - `src/alloc_core/alloc_core_small.rs:70–176` (`reclaim_offset_checked`)
  - `src/alloc_core/alloc_core_small.rs:810–911` (`flush_run`; guards 872–878, `write_next` 903)
- **Severity:** High
- **Description:** Every small-free path validates `off` (segment-relative
  offset) as: class in range, `off % block_size == 0` (hardened / reclaim only),
  `off < bump` (only under `alloc-decommit`), and `!bitmap.is_free(off)`. None
  of them checks the LOWER bound `off >= Layout::small_meta_end()` (or
  `primordial_meta_end()` for the primordial segment). Real blocks always start
  at `off >= small_meta_end`, so any `off` inside the metadata region is by
  definition invalid — yet it passes all four guards: `off = 0` is a multiple
  of every `block_size`; `0 < bump` always; the bitmap bits covering metadata
  are never set (the module doc of `alloc_bitmap.rs` explicitly says "the
  metadata bits are simply never touched"), so `is_free` reads `false`.
- **Concrete scenario:** a caller erroneously frees the segment base itself
  (or any 16-aligned pointer into the header / page map / bin table / bitmap
  pages) with a small `Layout`, e.g. `dealloc(base, Layout(16,16))`:
  `AllocCore::dealloc` → `contains_base(base)` = true → `kind_at` = Small →
  `dealloc_small(base, base, class 0)` → `off = 0` passes every guard →
  `Node::write_next(base, old_head)` writes 8 bytes at offset 0 — **directly
  over `SegmentHeader::bump`** — then `set_head(0, 0)` + `mark_free(0)`. The
  next `alloc_small` `pop_free`s head 0 and **hands the segment header out as
  a live allocation**; the caller's writes destroy the header, page map and
  bin table → arbitrary memory corruption / crash. The same entry exists
  cross-thread: a garbled or malicious `RemoteFreeRing` entry with `off <
  small_meta_end` (the exact "garbled ring value" `reclaim_offset`'s own doc
  claims defence-in-depth against) reaches `write_next` into metadata; and via
  the magazine flush (`flush_run`) for a bad pointer that slipped into the
  magazine. This directly contradicts `dealloc`'s documented contract ("a
  foreign pointer or double-free is a no-op — never UB, never corrupts").
- **Fix direction:** add an unconditional `if off < payload_start { return; }`
  guard (payload_start = `small_meta_end()`; for `SegmentKind::Primordial`
  use `primordial_meta_end()`) in `dealloc_small`, `reclaim_offset`,
  `reclaim_offset_checked`, and `flush_run`, placed before `is_free`. One
  integer compare — negligible cost; pairs naturally with the existing
  `off >= bump` upper-bound guard.

---

## Finding 2 — `off >= bump` guard is `alloc-decommit`-gated: non-decommit builds allow double-allocation via a free of a never-carved offset

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1976–1978` (`dealloc_small`, `#[cfg(feature = "alloc-decommit")]`)
  - `src/alloc_core/alloc_core_small.rs:243–245` (`reclaim_offset`), `113–115` (`reclaim_offset_checked`), `872–874` (`flush_run`)
- **Severity:** Medium
- **Description:** The `off >= bump` rejection exists only under
  `alloc-decommit` (rationale: only decommit resets `bump`). But the guard
  also rejects frees of offsets in the **uncarved** region (`bump ≤ off <
  SEGMENT`). Without the feature, a bogus free of a `block_size`-aligned,
  never-carved in-segment offset passes the bitmap guard (bit never set),
  gets `write_next`-linked onto the class free list, and `mark_free`'d.
- **Concrete scenario (double-allocation):** build without `alloc-decommit`.
  Segment has `bump = B`. Caller (or a garbled ring entry) frees `base + X`
  where `X ≥ B`, `X % block_size == 0`, `X < SEGMENT`. The block lands on the
  free list. (a) `pop_free` hands `base + X` to caller 1. (b) Later the bump
  carver advances past `X` and `carve_block` hands out a block overlapping
  `[X, X + bs)` to caller 2 — **two live allocations over the same bytes**
  (the carve path never consults the bitmap by design). Silent data
  corruption between two innocent allocations, triggered by one bad free.
- **Fix direction:** make the `off >= bump` check unconditional in all four
  sites (it is a single owner-only `usize` load, already race-free; the
  `bump_of` accessor compiles in every build).

---

## Finding 3 — Intrusive free-list `next` pointer is trusted without bounds validation: a single UAF write escalates to out-of-segment pointer arithmetic (UB) and a wild allocation

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:1455–1463` (`pop_free`: `next` → `(next as usize - segment as usize) as u32`)
  - `src/alloc_core/alloc_core_small.rs:1693–1702` and `1738–1750` (`drain_freelist_batch`, both cfg branches — same computation)
  - `src/alloc_core/node.rs:100–108` (`read_next`), `320–328` (`Node::offset` contract: `off <= SEGMENT`)
- **Severity:** Medium
- **Description:** `pop_free`/`drain_freelist_batch` load the freed block's
  first word (`read_next`) and convert it to a segment offset with an
  unchecked wrapping subtraction truncated to `u32`. If `next` does not point
  into `segment` (corrupted by a user use-after-free write into a freed
  block, or by the Finding-1/2 corruption), the computed `off` is arbitrary.
  It is then fed to `Node::deref(segment, off)` — whose documented seam
  contract requires `off < segment_len` — producing `segment.add(off)` with
  `off` up to `u32::MAX`, i.e. pointer arithmetic **outside the allocation:
  immediate UB** per the seam's own SAFETY comment, plus a wild pointer
  handed to the caller as a fresh allocation on the next pop.
- **Concrete scenario:** app frees block P (P joins the free list, P's word0
  = next), then writes 8 bytes through a dangling reference into P (classic
  UAF). Next `alloc` of that class pops P (fine), records `head = (garbage -
  segment) as u32`; the following `alloc` calls `Node::deref(segment,
  garbage_off)` → OOB `add` (UB) → returns an out-of-segment pointer as an
  allocation → arbitrary memory corruption far from the heap.
- **Fix direction:** validate `next` before trusting it: at minimum
  `segment_base_of_ptr(next) == segment` (one mask + compare) and, under
  `hardened`, `off % block_size == 0 && off >= small_meta_end && off < bump`;
  on failure truncate the walk (`set_head(FREE_LIST_NULL)`), never deref.
  This is mimalloc-secure's freelist-corruption detection analogue.

---

## Finding 4 — Non-atomic full-struct header WRITES on the Large deposit/reuse paths race remote field READS (data race / UB under the crate's own §11 discipline)

- **Files/lines:**
  - `src/alloc_core/alloc_core.rs:826–828` (`dealloc` Large branch: `Node::write_struct(base, hdr_zero)` — zeroing `magic` rewrites the WHOLE 104-byte header non-atomically)
  - `src/alloc_core/alloc_core_large.rs:165–174` (cache-hit path: full `Node::write_struct` of a fresh header over a segment being handed to a new caller)
  - `src/alloc_core/alloc_core_large.rs:346–348` (`reclaim_large_segment`: same `hdr_zero` full write)
  - Racing readers: `src/alloc_core/segment_header.rs:596–599` (`magic_at`), `569–586` (`kind_at`), `638–641` (`large_size_at`), `677–680` (`span_usable_at`) — all plain (non-atomic) loads executed by REMOTE threads in `dealloc_routing`.
- **Severity:** Medium (only reachable when the application itself races a
  stale/duplicate cross-thread free against the owner's free/reuse of the
  same Large segment — i.e. app misuse — but these reads exist precisely as
  the *defensive* validation for that case)
- **Description:** The whole task-#33/§11 discipline ("never full-struct
  read/write a header that a remote may field-read concurrently") is upheld
  on the small path but re-opened on the Large path: the owner's cache
  deposit / cache-hit / remote-reclaim all perform a full-struct plain write
  that covers the same bytes (`magic`, `kind`, `large_size`, `span_usable`)
  a concurrent remote `magic_at`/`kind_at`/`large_size_at` may be plainly
  loading. Concurrent non-atomic write + read of the same location is a data
  race → UB (the `large_size_at` doc argues only value-level staleness and
  claims the full-struct rewrite "does not race the owner's disjoint bump
  writes" — but the hazard is the remote's read of the very fields being
  rewritten, not `bump`). Note this is distinct from (and additional to) the
  already-documented "released-segment dangling read → fault" residual.
- **Fix direction:** on the deposit/hit paths, write the remotely-read fields
  through atomic views (e.g. an `AtomicU32` view for `magic` via the existing
  `Node::atomic_u32_at`, matching how `owner_state` is handled) or perform
  field-wise single-word writes for the fields remotes may read; or extend
  the documented residual to explicitly cover this race and gate the
  defensive reads accordingly.

---

## Finding 5 — `SegmentTable::recycle` defensive tail releases the OS reservation but can leave the base resolvable via hash/own_cache → routed writes into unmapped memory

- **File/lines:** `src/alloc_core/segment_table.rs:432–437` (defensive tail of `recycle`)
- **Severity:** Low (requires a corrupted/stale `segment_id` in the header —
  but that is precisely the case the defensive branch exists for)
- **Description:** When the stamped `segment_id` does not match (`slots[id]
  != base`), `recycle` releases the OS reservation anyway "to avoid a leak"
  but performs **no** `hash_remove(base)` / `own_cache_clear(base)` / slot
  NULLing. If `base` is in fact present in the hash (registered under a
  different slot, header id corrupted), `contains_base(base)` keeps
  returning `true` for a now-unmapped segment; the next `dealloc` of any
  pointer in it is routed own-thread → `kind_at` reads unmapped memory →
  fault (or, worse, `dealloc_small` writes into a re-mapped foreign region).
  The main path carefully orders `hash_remove` + `own_cache_clear` *before*
  `release_segment` (lines 399–417) exactly to prevent this; the defensive
  tail skips all of it.
- **Fix direction:** in the mismatch tail, do NOT release (a leak is the safe
  failure mode for a corrupt header), or at minimum `hash_remove(base)` +
  `own_cache_clear(base)` before releasing.

---

## Finding 6 — `flush_class`: a second same-segment run after a decommit-recycle in an earlier run reads unmapped metadata (UAF)

- **Files/lines:**
  - `src/alloc_core/alloc_core_small.rs:781–804` (`flush_class` run splitting)
  - `src/alloc_core/alloc_core_small.rs:1047–1055` (`flush_run` end: `release_or_pool_empty_segment(base)` → possibly `recycle(base)` = full OS release)
- **Severity:** Low (requires duplicate pointers of one segment inside a
  single magazine flush batch, i.e. an upstream double-free that reached the
  magazine — the guards that are supposed to absorb it run too late)
- **Description:** `flush_class` splits the batch into consecutive
  same-`base` runs. If run k for segment A drives `live_count` to 0 and the
  hysteresis pool is full/disabled, `flush_run` recycles A — releasing the
  **entire** reservation, metadata pages included. If the batch later
  contains another run for the same A (pattern `[A…, B…, A-dup]`, possible
  only when the A-dup entries are double-frees, since genuine
  magazine-resident A blocks would have kept `live_count > 0`), the second
  `flush_run(A)` calls `SegmentMeta::new(A).bin_table().head(..)` /
  `bump_of()` / `is_free()` on unmapped memory → UAF read / fault. The
  bitmap/bump guards designed to make double-frees no-ops cannot help,
  because the memory holding them is already gone.
- **Fix direction:** in `flush_class`, record bases for which decommit fired
  during this call and skip subsequent runs with the same base (or defer the
  `release_or_pool_empty_segment` calls to the end of `flush_class`, as the
  ring-drain path defers recycle to after the drain for exactly this hazard
  class).

---

## Finding 7 — `kind_at` maps a corrupt discriminant byte to `Small`, routing Large-segment frees into non-existent BinTable metadata (writes into live user payload)

- **File/lines:** `src/alloc_core/segment_header.rs:569–586` (`kind_at`, `_ => SegmentKind::Small`)
- **Severity:** Low (requires a corrupted header byte — e.g. via Finding 1 —
  but the mapping choice amplifies rather than contains the corruption)
- **Description:** A corrupt/unknown `kind` byte is "defensively" decoded as
  `Small`. For a segment that is actually `Large`, `dealloc` then runs
  `dealloc_small`: `bin_table()` addresses `bin_table_off ≈ PAGE + 1 KiB` —
  which in a Large segment lies **inside the single live user allocation**
  (payload starts at `hdr_aligned ≥ PAGE`). `set_head` writes 4 bytes of
  offset into the user's live data; `write_next` writes 8 more at `ptr`.
  Corruption of live user memory from a single flipped metadata byte. The
  safer defensive default for an unknown discriminant is `Large` (whose
  dealloc path at least re-validates via the full header / registry) or an
  explicit reject sentinel that makes the caller no-op.
- **Fix direction:** decode strictly (`0→Primordial, 1→Small, 2→Large,
  _→reject`), adding a `None`/sentinel arm that every caller treats as a
  no-op — consistent with the "never corrupt on garbage" contract.

---

## Checked and found sound (no finding)

- **`RemoteFreeRing` MPSC protocol** (`remote_free_ring.rs`): orderings
  (AcqRel reserve CAS, Release publish, Acquire slot load, Release head
  publish), wrap-correct `h != t` with `wrapping_sub`, power-of-two
  `RING_CAP` pinned by const-assert, sentinel non-collision (both packings)
  pinned by const-asserts. The Relaxed `head` self-load and the
  `tail_relaxed` pre-drain guard have valid single-consumer/monotonicity
  arguments. Overflow = documented bounded leak, sound.
- **`AllocBitmap`**: single-writer (owner-only) discipline is consistently
  upheld across free/reclaim/flush/pop; `locate` is bounds-safe for any
  `off < SEGMENT` (`FOOTPRINT*8 == SEGMENT/MIN_BLOCK`). Virgin-init elision
  (task #50) is sound: applies only to genuinely fresh OS reservations
  (zero-filled), miri keeps explicit init, decommit-reset re-inits explicitly.
- **`Node` seam**: `write_next`/`read_next` unaligned one-word ops with
  correct exclusivity arguments; `atomic_*_at` lifetime caveats are honest
  (the `'static` is scoped to registered-segment liveness with per-path
  arguments); `atomic_ptr_ref` uses exposed provenance to avoid the
  Stacked/Tree-Borrows remote-write tag-disable (task #142) — correct.
- **Layout composition** (`segment_header.rs::Layout`): all offsets derive
  compositionally; the X7-Ф3 gen-table/registry overlap bug is fixed
  (`primordial_registry_off = small_meta_end()`); const-asserts pin
  `small_meta_end + PAGE <= SEGMENT` under every feature combo and
  `page_map_off == PAGE`.
- **`carve_block`/`carve_batch`**: bump arithmetic cannot overflow
  (`align_up` via `div_ceil`; `aligned + bs > SEGMENT` rejects before any
  deref; `room` computed after the reject). Recommit-failure path correctly
  refuses to advance bump or clear `decommitted` (no write into reserved-only
  pages).
- **`alloc_large` size arithmetic**: `align >= SEGMENT` rejected;
  `checked_add` on `needed`; under the `Layout` invariant
  (`size ≤ isize::MAX − (align−1)`) `align_up(size, align)` cannot wrap;
  `span_usable` carried verbatim on cache-hit (bug #134 fix verified present
  at both write sites).
- **Decommit/pool lifecycle** (`alloc_core_small_pool.rs`): primordial
  exclusion present in both dec-live variants; release-follows fast reset
  keeps the load-bearing `set_bump` for intra-drain stale-entry rejection;
  ring-drain defers `recycle` until after the drain (UAF-on-mid-drain-recycle
  correctly prevented); pooled-segment stale-ring argument (is_free guard,
  metadata never decommitted while pooled) holds.
- **`RunStack`** (Ф1–Ф4): bitmap remains sole ground truth (per-member
  `is_free` re-check before issue), mid-descriptor pushback closes the
  partial-drain leak, decommit `clear_all` closes the stale-descriptor-into-
  unmapped-payload hazard.
- **`SegmentTable` hash/free-list/own_cache**: cache invalidation is
  structurally co-located with `hash_remove` (unregister/recycle/rebuild);
  tombstone-rebuild ordering (after slot NULL) correct; free-list push/pop
  duplicate-guards hold. (Sole gap: Finding 5's defensive tail.)
- **`AllocCore::drop`**: collects reservations into a stack array before
  freeing (no read of the registry after the primordial is freed); cached
  large entries released separately (they are unregistered) — no double-free.

## Summary

| # | Severity | One line |
|---|----------|----------|
| 1 | High | No `off >= small_meta_end` lower bound — aligned free into metadata region corrupts header/page-map/bin-table, then header is handed out as an allocation |
| 2 | Medium | `off >= bump` guard is decommit-gated — non-decommit builds admit never-carved offsets → double-allocation overlap |
| 3 | Medium | Freelist `next` unvalidated — one UAF write → OOB `Node::offset` (UB) + wild pointer handed out |
| 4 | Medium | Full-struct header writes on Large deposit/reuse race remote field reads (data race, re-opens §11 class) |
| 5 | Low | `recycle` mismatch tail releases OS memory without hash/cache eviction → `contains_base` true for unmapped base |
| 6 | Low | `flush_class` second same-base run after mid-batch recycle reads unmapped metadata |
| 7 | Low | `kind_at` decodes corrupt byte as `Small` → BinTable writes into live Large payload |
