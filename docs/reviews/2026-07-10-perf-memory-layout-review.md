# Performance review — memory layout and cache locality (2026-07-10)

**Scope:** struct layout, field ordering, padding, alignment, and cross-
thread false-sharing risk in the hot structures touched on every
alloc/dealloc. **Method:** fxx (Fable-5, effort=max) research agent.
Layout data empirically confirmed via nightly `-Zprint-type-sizes` on
`--features production`, run in a scratch target directory — no repo
files were touched, modified, or built with non-standard flags. No
findings below have been implemented.

## Measured ground truth

rustc 1.9x nightly, `--release --features production`:

- `HeapSlot` = 7024 B / align 8
- `HeapCore` = 6976 B (field order chosen by rustc:
  `core`@0, `thread_free`@568, `tcache`@576, `tcache_hits`@6952,
  `last_stamped_segment`@6960, `id`@6968)
- `Tcache` = 6376 B (`slots` 6272 + `count` 98)
- `AllocCore` = 568 B (rustc placed the cold 384-B `large_cache` first, hot
  `table` at 448)
- `SegmentHeader` = 104 B, `#[repr(C)]`, exact offsets below
- `SegmentTable` = 72 B (`own_cache` first — good)
- Registry = 4096 × 7024 B ≈ 27.4 MiB of `'static` zeroed slots

## Ranked findings

### 1. `SegmentHeader` hot fields straddle two cache lines — HIGH-MEDIUM confidence, LOW risk

- **File:** `src/alloc_core/segment_header.rs:210-345` (`#[repr(C)]`).
- Measured offsets: `magic`@0, `kind`@4, `segment_id`@8, `bump`@16,
  `large_size`@24, `large_align`@32, `span_usable`@40, `reservation`@48,
  `reservation_len`@56 — **line 0**; `owner_thread_free`@64,
  `owner_state`@72, `next_abandoned`@80, `live_count`@88, `decommitted`@92,
  `node_id`@96 — **line 1** (the header sits at a 4MiB-aligned segment
  base, so the 64-B split at offset 64 is exact, always).
- Hot-path groupings: refill/carve touches `bump` (line 0) + `live_count`
  (line 1, ×16 per refill) + `owner_state` (line 1) → 2 lines.
  Cross-thread dealloc routing touches `magic` (line 0) +
  `owner_thread_free` (line 1) + `large_size` (line 0) → 2 lines. Decommit
  bookkeeping touches `live_count`/`decommitted` (line 1) + `bump` (line 0)
  → 2 lines. Line 0 is majority-cold (`large_*`, `reservation*`,
  `segment_id` are Large-only/teardown-only/unregister-only).
- **Fix direction:** reorder so the small-segment per-op set (`magic`,
  `kind`, `bump`, `owner_state`, `owner_thread_free`, `live_count`,
  `decommitted`, ≈40 B) occupies bytes 0..64, pushing `large_*`,
  `span_usable`, `reservation*`, `next_abandoned`, `segment_id`, `node_id`
  past 64. Every access already goes through `offset_of!`-based accessors
  (`bump_of`, `magic_at`, `owner_thread_free_at`, …), so no offset is
  hardcoded anywhere; `size_of` stays 104 so `Layout::page_map_off()` and
  all downstream metadata offsets are byte-identical — the "layout is
  feature-invariant" doc contract is preserved.
- **Risk: low** (`repr(C)` = deterministic outcome; `offset_of!`-only
  access; re-run the const layout asserts at the bottom of
  `segment_header.rs` and any test that snapshots header bytes).

### 2. `HeapSlot`: remote-CASed `thread_free` word shares a line with owner-hot fields — confirmed textbook false sharing — HIGH confidence on existence, MEDIUM on real-world impact, LOW risk

- **File:** `src/registry/heap_slot.rs:94-248` (`#[repr(C)]`); interacts
  with `src/registry/heap_core.rs:282` (`last_stamped_segment`).
- Measured slot offsets: `state`@0, `generation`@8, `heap`@16..6992,
  `next_free`@6992, `initialised`@6996, `tcache_hits`@7000,
  `large_cache_hits`@7008, `thread_free`@7016. Inside `heap`, rustc placed
  `last_stamped_segment` at slot offset **6976** and `id` at **6984**. So
  the single 64-B cache line 6976..7040 simultaneously contains:
  `last_stamped_segment` + `id` (read/compared by the owner on every stamp
  fast-path check), `thread_free`@7016 (CASed by remote threads on every
  cross-thread Large free, Acquire-loaded on the drain check),
  `tcache_hits`/`large_cache_hits`@7000/7008 (read cross-thread by the
  `stats()` aggregator), and the **next slot's `state`/`generation`**
  (slot stride 7024 is not a multiple of 64, so slot boundaries drift
  through cache-line phase across the array).
- The H1 hoist (this session's confirmed UB fix) solved the Stacked-
  Borrows aliasing problem but physically re-created the adjacency: a
  remote CAS on `thread_free` now invalidates the very line holding the
  owner's stamp cache. Under any workload with cross-thread Large frees,
  the owner's refill/stamp path eats coherence misses it never earns.
  Confirmed via grep: **zero** `#[repr(align)]`/CachePadded anywhere in
  `src/`.
- **Fix direction:** (a) group the remote/foreign-access fields
  (`thread_free`, `tcache_hits`, `large_cache_hits`) into a
  `#[repr(align(64))]` sub-struct (or explicit padding to a 64-B boundary
  after `heap`), and (b) add `#[repr(align(64))]` on `HeapSlot` itself so
  the stride becomes a 64-multiple (7024 → 7040; +64 KiB across the whole
  4096-slot registry — negligible, pages are lazily committed anyway).
  Slot layout is bootstrap-carved from zeroed pages via `addr_of_mut!`
  field writes, which follow the type's layout automatically — no offset
  arithmetic to update.
- **Risk: low** (pure padding/alignment; `#[repr(C)]` keeps it
  deterministic; verify the registry-footprint const in bootstrap still
  fits the primordial segment). **Impact caveat:** only materializes
  under cross-thread Large-free traffic — irrelevant to the single-
  threaded 16B benchmark that motivated this whole investigation.

### 3. No `[profile.release]`/`[profile.bench]` tuning at all — HIGH confidence config is suboptimal, MEDIUM confidence on magnitude, effectively ZERO risk

- **File:** `Cargo.toml:442-463` (the existing §0 comment block explicitly
  declines a `[profile.*]` section, but only for debug-info reasons).
- No release/bench profile is defined workspace-wide, and
  `scripts/bench-table.mjs` passes no `RUSTFLAGS`/`target-cpu`. Every
  wall-clock comparison against mimalloc (whose C core is compiled by
  `cc` at its own full optimization inside the crate's build script) runs
  the Rust side at default 16 codegen units with no LTO and unwind
  landing pads on every call. The hot path is heavily `#[inline(always)]`
  so cross-CGU inlining mostly survives, but `codegen-units = 1` +
  `lto = "thin"`/`"fat"` reliably improves register allocation/code
  layout on exactly this kind of branchy ~30-instruction fast path, and
  `panic = "abort"` in `[profile.bench]` removes landing-pad code bloat
  from the icache. Does not conflict with the existing comment's rationale
  (that concerned `debug = true` only).
- **Fix direction:** add `[profile.release] lto = "thin"` (or `"fat"`),
  `codegen-units = 1`; optionally `[profile.bench] panic = "abort"`. A
  library cannot set a *consumer's* profile, but the crate's own
  benchmarks — where the 2x number was measured — it fully controls.
- **Risk: effectively zero** — measure `npm run bench:table` + `npm run
  iai` before/after; note iai instruction counts may shift baselines and
  CI perf-gate thresholds may need re-pinning. **This is the one lever
  that applies to every instruction of the measured benchmark and costs
  nothing to try first.**

### 4. `RemoteFreeRing`: `head`, `tail`, `overflow`, and the first 12 slots share one cache line — HIGH confidence on existence, MEDIUM impact only under sustained cross-thread traffic, LOW risk

- **File:** `src/alloc_core/remote_free_ring.rs:394-412` (`CURSOR_BLOCK` =
  16; `HEAD_OFF`=0, `TAIL_OFF`=4, `OVERFLOW_OFF`=8, `SLOTS_OFF`=16).
- The ring's in-segment base is 64-B aligned, so bytes 0..64 = `head` +
  `tail` + `overflow` + slots[0..12]. Producers CAS `tail` and Acquire-
  load `head` on every push; the consumer Release-stores `head`, Acquire-
  loads `tail`, and reads/clears slots — all on the **same line**. Some
  head/tail sharing is protocol-inherent, but the current packing
  guarantees maximal ping-pong: a consumer's `head` publish invalidates
  the producers' `tail` CAS line *and* the first 12 data slots.
- **Fix direction:** widen `CURSOR_BLOCK` to 128 (`HEAD_OFF`=0,
  `TAIL_OFF`=64, `OVERFLOW_OFF`=68, `SLOTS_OFF`=128). Costs 112 bytes per
  4MiB segment. `FOOTPRINT` derives from `CURSOR_BLOCK`, and all
  downstream offsets derive from `FOOTPRINT`, so the layout re-composes
  automatically; existing layout const-asserts re-verify at compile time.
- **Risk: low**; re-pin any test hardcoding `FOOTPRINT`/meta offsets.
  Invisible to single-threaded benches — the owner only drains on
  free-list miss.

### 5. `Tcache::count` is `[u16; 49]` (98B, 2 lines, straddling) — LOW-MEDIUM confidence, LOW risk

- **File:** `src/registry/tcache.rs:105-112`.
- `count` occupies `HeapCore` bytes 6848..6946 — straddles a line no
  matter what. `TCACHE_CAP = 16 ≤ 255`, so `u8` suffices: 49B fits one
  line (if placed at a 64-B boundary). Saves at most one L1-hot line per
  op.
- **Risk: low** (arithmetic already goes through `as usize`; check
  `FLUSH_N` paths for `u16` assumptions).

### 6. `AllocCore`: 448 cold bytes placed ahead of the dealloc-hot `table.own_cache` — LOW confidence, LOW risk

- **File:** `src/alloc_core/alloc_core.rs:253-365`.
- rustc placed `large_cache` (384B, Large-path-only) @0, budget/decay/tick
  @384..448, then `table` @448 (`own_cache` at `table`+0, i.e. `HeapCore`
  bytes 448..480). Owner-private data, no sharing hazard — the only cost
  is the per-free hot set spread over ~4 non-adjacent lines instead of
  1-2. Completing move if findings 1-2 are done: fold into a small
  `#[repr(C)]` hot-header sub-struct at the front of `HeapCore`.

### 7. Confirmed clean (no findings)

- `src/alloc_core/node.rs` (`Node`) — exactly one pointer overlapped with
  the block's own first word, `NODE_SIZE = 8`, zero out-of-band bytes.
- `src/alloc_core/alloc_bitmap.rs` — flat sequential byte array, one bit
  per 16-B granule, single byte RMW per op, no scanning. One 64-B bitmap
  line covers 8KiB of payload. The extra-line cost vs mimalloc is an
  algorithmic (M2 design) cost, not a layout defect.
- `SegmentTable` — `own_cache` already the first field, 32B, fine.

## Summary recommendation

Highest-confidence, lowest-risk change: the pair of `#[repr(C)]` fixes
fully controlled by manual reordering — split `SegmentHeader` (finding 1)
so the per-operation field set lives entirely in bytes 0..64 (every
access already goes through `offset_of!` accessors, so this is a pure
layout edit with byte-identical downstream metadata offsets), and in the
same pass cache-line-partition `HeapSlot` (finding 2) with
`#[repr(align(64))]` plus padding so the remote-CASed `thread_free` word
stops sharing a line with the owner's stamp-cache fields (the physical
false-sharing residue of the H1 hoist). For the single-threaded 16B gap
specifically, temper expectations: the magazine-hit path already touches
only ~4 hot lines, so layout alone won't close 2x — but finding 3 (adding
`lto`/`codegen-units=1` to a currently-absent `[profile.release]`) is the
one lever that applies to every instruction of the measured benchmark and
costs nothing to try first.
