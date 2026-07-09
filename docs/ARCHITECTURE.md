# sefer-alloc -- Architecture Overview

**Target audience:** technical reviewer or contributor who wants to understand
how the crate is structured in 30 minutes, without reading the full set of
phase-by-phase design documents. This file synthesizes key ideas and points to
the authoritative sources.

**One-line positioning.** `sefer-alloc` is a safe-by-construction, **100 %
Rust** memory toolkit (no C / C++ libraries pulled in — no `libnuma`, no
`mimalloc`, no `jemalloc`, no `snmalloc` / `tcmalloc`; it calls the OS
directly via `mmap` / `VirtualAlloc` / `mbind` etc., same as any allocator
does). The only C dependency in the repository is the optional `mimalloc`
dev-dependency used as a baseline in benchmarks — never on a consumer's
runtime path.

**Date:** 2026-07-10 (as of commit 4a4ff5e). Numbers and inventories below are
snapshots as of this commit; they may drift with later commits — treat the
date/commit stamp as the freshness marker and re-verify against the source files
cited in each section if in doubt.

---

## Table of contents

1. [The big picture: two faces, one substrate](#1-the-big-picture-two-faces-one-substrate)
2. [Three organs: Cartographer / Membrane / Hand](#2-three-organs-cartographer--membrane--hand)
3. [The segment substrate (Phase 8)](#3-the-segment-substrate-phase-8)
4. [Per-thread heaps and the lock-free fast path (Phases 9-10)](#4-per-thread-heaps-and-the-lock-free-fast-path-phases-9-10)
5. [Cross-thread free (Phases 10-12)](#5-cross-thread-free-phases-10-12)
6. [Phase 35 -- M6 decommit (alloc-decommit feature)](#6-phase-35----m6-decommit-alloc-decommit-feature)
7. [NUMA-aware path (Phase 58, numa-aware feature)](#7-numa-aware-path-phase-58-numa-aware-feature)
8. [Verification stack](#8-verification-stack)
9. [Performance summary](#9-performance-summary)
10. [Where to read next](#10-where-to-read-next)

---

## 1. The big picture: two faces, one substrate

```
  +-----------------------------------------------------------------+
  |  MEMBRANE -- two faces (safe API surface)                        |
  |                                                                  |
  |   Handle face          |   alloc face                           |
  |   Region<T>/Handle<T>  |   SeferAlloc                          |
  |   typed, generational  |   unsafe impl GlobalAlloc              |
  |   single-threaded core |   MT, production-ready (feature:       |
  |   or concurrent tier   |   alloc-global + alloc-xthread)        |
  +------------------------+-----------------------------------------+
  |  CARTOGRAPHER -- 100% safe integer arithmetic                    |
  |   size classes, bin tables, page maps, segment registries,       |
  |   placement, decommit policy, O(1) owner lookup                  |
  +-----------------------------------------------------------------+
  |  SEGMENT SUBSTRATE -- self-hosted OS-backed memory (Phase 8+)   |
  |   4 MiB SEGMENT-aligned spans; metadata carved from segments;   |
  |   no Vec / Box / std::alloc on any path (M5)                    |
  +-----------------------------------------------------------------+
  |  HAND -- confined unsafe seams (see §2)                          |
  +-----------------------------------------------------------------+
```

Both API faces live on the **same segment substrate**. The difference is the
API surface:

- `Region<T>` / `Handle<T>` -- a typed, generational handle store. A stale
  handle returns `None`, never undefined behaviour. The single-threaded core
  wraps the audited `slotmap` crate and is `#![forbid(unsafe_code)]`. The
  concurrent tier (feature `experimental`) adds lock-free reads via `arc-swap`
  (RCU, zero own `unsafe`) or `crossbeam-epoch` (one confined `unsafe` module).
- `SeferAlloc` -- `unsafe impl GlobalAlloc` over the per-thread segment heap.
  Enabled by feature `alloc-global`; cross-thread `dealloc` requires
  `alloc-xthread`. The `production` alias bundles
  `alloc-global + alloc-xthread + alloc-decommit + fastbin` as the recommended
  long-running deployment set.

Full safety invariants for both faces: [INVARIANTS.md](INVARIANTS.md)
(I1-I6 for the Handle face, M1-M8 for the alloc face).

**Additional opt-in features** (default OFF, NOT part of `production` — see the
feature table in [README.md](../README.md#feature-flags)):

- `hardened` (additive over `fastbin`) — paranoid deploy hardening: an
  interior-pointer free guard (`off % block_size != 0` rejected as a no-op) on
  both own-thread free faces, plus the **X7 per-granule generational ring**: a
  per-granule generation counter stamped on each `RemoteFreeRing` note and
  dropped on drain if it has advanced, closing the *re-issue-before-drain* leg
  of the ring↔magazine cross-thread double-free residual of M2 (except the
  1/256 wrap — the accepted probabilistic residual). Costs a modulo-per-free, so
  it is off the production fast path.
- `alloc-stats` — per-hit diagnostic counters: bumps `stats().tcache_hits`
  (magazine) and `stats().large_cache_hits` (large cache) on each hit. The
  per-hit increment is compiled out when off (those two fields then read 0); the
  counter storage is always present so toggling never changes layout/ABI.
- `alloc-runfreelist` — **experimental** run-encoded freelist storage
  (`RunStack`): a per-segment metadata region that (in later phases) will encode
  contiguous freed-block runs as compact `(start_off, count)` descriptors
  instead of per-block intrusive `next` writes. Its go/no-go verdict is deferred;
  with it off the byte layout is identical to the pre-feature build.

---

## 2. Three organs: Cartographer / Membrane / Hand

The founding principle (from [DESIGN.md](DESIGN.md)):

> **All the intelligence lives in the safe Cartographer** (pure arithmetic over
> `u32` indices and offsets), so the Hand stays mechanical and tiny. You prove
> a total membrane and an integer algorithm, not a tangle of pointer math.

| Organ | Responsibility | Safety |
|---|---|---|
| **Cartographer** | All placement / free-list / compaction / decommit logic -- pure integer arithmetic over indices. Never touches memory directly. | `safe` |
| **Membrane** | Typed API: `Handle<T>`, generation checks, lifetimes; `AllocCore::alloc` / `SeferAlloc::alloc` -- total, cannot express UB. | `safe` |
| **Hand** | Confined `unsafe` seams that touch raw memory or issue OS syscalls. | `confined unsafe` |

### Workspace: four independently-publishable companion crates

Before discussing the internal seams, the workspace structure matters for the
audit story. Four building blocks were extracted as standalone crates:

```
sefer-alloc
 ├── sefer-region    (crates/region)       — Handle<T>/Region<T>/SyncRegion<T>
 ├── aligned-vmem    (crates/vmem)         — OS virtual-memory aperture  (feature: alloc-core)
 ├── numa-shim       (crates/numa)         — NUMA detection + binding    (feature: numa-aware)
 └── malloc-bench-rs (crates/malloc-bench) — portable GlobalAlloc bench harness (standalone)
```

Each is a real crates.io crate (`cargo add aligned-vmem`, etc.). The
extraction **improved the audit story**: the two OS-unsafe sub-problems are now
small, single-responsibility crates that can be audited in complete isolation.

### Confined unsafe seams (current inventory — verifiable with `grep -rln 'allow(unsafe_code)' src/ crates/`)

**External publishable crates** (independently auditable):

| Crate | Path | Unsafe story |
|---|---|---|
| `aligned-vmem` | `crates/vmem/` | `#![allow(unsafe_code)]` — entire crate IS the OS aperture; sole responsibility = SEGMENT-aligned mmap/VirtualAlloc + decommit. Small, audit in isolation. |
| `numa-shim` | `crates/numa/` | `#![allow(unsafe_code)]` — entire crate IS the NUMA syscall shim; sole responsibility = mbind(2)/VirtualAllocExNuma. Small, audit in isolation. |
| `malloc-bench-rs` | `crates/malloc-bench/` | `#![allow(unsafe_code)]` — confined to alloc_block/free_block/drain_mailbox helpers; every unsafe block carries `// SAFETY:`. Bench harness, not runtime. |
| `sefer-region` | `crates/region/` | `#![forbid(unsafe_code)]` — zero own unsafe; slotmap's audited core owns the generational layout. |

**Internal sefer-alloc seams** (10 src modules total; compiler-enforced):

Under the recommended `production` feature
(`alloc-global + alloc-xthread + alloc-decommit + fastbin`) the active
seams are **eight** — the first two `alloc_core::*` rows plus the three
`global::*` rows plus the three `registry::*` rows
(`bootstrap`/`heap_slot`/`heap_registry`). `numa-aware` adds one more
(`alloc_core::numa`), which in turn delegates to the independently-
auditable `numa-shim` crate. The `experimental` tier opens the older
research-tier concurrent seam (now deprecated); the production build does
not pull it in.
`alloc-xthread`, `alloc-decommit`, and `fastbin` do **not** add new
`unsafe` seams — they extend existing safe paths.

| Module | Role | Feature gate |
|---|---|---|
| [`src/alloc_core/os.rs`](../src/alloc_core/os.rs) | Thin interop wrapper around `aligned-vmem`; delegates SEGMENT-aligned reservation and decommit/recommit. | `alloc-core` |
| [`src/alloc_core/node.rs`](../src/alloc_core/node.rs) | Intrusive free-list node read/write: the single place that reads/writes the `next` pointer inside a free block; also `release_segment` thin wrapper. | `alloc-core` |
| [`src/global/sefer_alloc.rs`](../src/global/sefer_alloc.rs) | The `unsafe impl GlobalAlloc` alloc-face seam — the trait obligation + pointer handoff to the `HeapCore`. | `alloc-global` |
| [`src/global/tls_heap.rs`](../src/global/tls_heap.rs) | Raw-pointer TLS binding + `AbandonGuard` seam — the `*mut HeapCore` handoff under the single-writer invariant; `unsafe fn recycle` / `abandon_segments` from the guard's drop. | `alloc-global` |
| [`src/global/fallback.rs`](../src/global/fallback.rs) | Primordial fallback heap — `static mut MaybeUninit<HeapCore>` + atomic-init state-machine + spinlock-guarded `&mut` handout (survives reentrant / early-init / teardown access). | `alloc-global` |
| [`src/registry/bootstrap.rs`](../src/registry/bootstrap.rs) | The primordial-segment carve / SegmentTable bootstrap seam — raw-pointer footprint carving of the metadata region under the atomic single-writer bootstrap protocol. | `alloc-global` |
| [`src/registry/heap_slot.rs`](../src/registry/heap_slot.rs) | `Sync`/`Send` impls on `HeapSlot` under the atomic single-writer protocol; the slot's `UnsafeCell` hand-off. | `alloc-global` |
| [`src/registry/heap_registry.rs`](../src/registry/heap_registry.rs) | Global heap slot-table — the `*mut HeapCore` pointer handoff out of a slot, consulted by every cross-thread routing decision. | `alloc-global` |
| [`src/alloc_core/numa.rs`](../src/alloc_core/numa.rs) | Thin interop wrapper around `numa-shim`; delegates NUMA-node query and segment binding. | `numa-aware` |
| [`src/concurrent/hand.rs`](../src/concurrent/hand.rs) | Epoch-based per-slot atomics for the lock-free concurrent Handle tier (3b-II; superseded by `alloc-xthread` for the global allocator; **deprecated, legacy/research-tier**). | `experimental` |

Outside these modules, `unsafe` is a hard compile error
(`#![deny(unsafe_code)]` when any of those features are active;
`#![forbid(unsafe_code)]` with only the default `std` feature). The
source-of-truth catalogue is in [`src/lib.rs`](../src/lib.rs) at the
top-level comment.

---

## 3. The segment substrate (Phase 8)

### Segment layout

Each segment is a `SEGMENT`-aligned (4 MiB default) OS-reserved span. Its
first page holds the header; subsequent metadata pages follow immediately:

```
  [0x000000] SegmentHeader  (magic, kind, segment_id, bump, owner_thread_free,
                             owner_state, live_count, decommitted, node_id, ...)
  [0x001000] PageMap        (per-page Kind: Free / SmallClass(idx) / Large)
  [after]    BinTable       (per-size-class free-list head, intrusive pointers
                             into payload blocks)
  [after]    AllocBitmap    (1 bit per MIN_BLOCK slot -- O(1) double-free guard)
  [after]    RemoteFreeRing (MPSC ring for cross-thread free; see §5)
  [payload]  block data     (carved by bump cursor, returned to callers)
```

Metadata lives in committed pages and is **never decommitted**, even under
`alloc-decommit`. This is load-bearing for the safety of cross-thread free
(§5, §6).

### SegmentTable

A process-global `SegmentTable` is itself carved from the first (primordial)
segment -- zero-allocation bootstrap. It is append-only with 1024 slots.
Two lookup structures:

- Sequential scan over live slots: used by `find_segment_with_free`.
- (From commit #67) A parallel open-addressing hash for O(1)
  `contains_base(ptr)` lookups on the `dealloc` path. The `segment_table_hash`
  tests cover this; see [`tests/segment_table_hash.rs`](../tests/segment_table_hash.rs).

Slot encoding: a `NULL` base pointer means the slot is recyclable (set during
decommit, see §6). This lifts the hard 1024-segment ceiling for long-running
deployments (`alloc-decommit` feature).

### Self-hosting (the Membrane Inversion)

Before Phase 8, the safe `Region<T>` was a *consumer* of the global allocator
(`Vec<T>` backing). The Membrane Inversion makes the safe slot-table discipline
a *governor* of OS memory: the allocator's own metadata is carved from
segments, so no `Vec` / `Box` / `std::alloc` appears on any alloc/dealloc
path. This is M5 (reentrancy-freedom). See [ALLOC_PLAN.md](ALLOC_PLAN.md) §1.

---

## 4. Per-thread heaps and the lock-free fast path (Phases 9-10)

### Structure

Each thread owns a `HeapCore` (or uses `AllocCore` directly for single-thread
mode), bound via raw-pointer TLS with a reentrancy fallback (introduced in
Phase 11.5 hardening). On `SeferAlloc`, `current_for_alloc()` performs a
registry hop to find or create the per-thread `HeapCore`.

### Hot path

```
  alloc_small(layout):
    1. pop_free(class_idx) from BinTable  -- pure pointer read, no lock, no atomic
    2. if empty: carve_block_with_refill  -- bump REFILL_BATCH=31 blocks from
                                             current segment, push 31 to BinTable
                                             (magazine / free-list), return one to
                                             the caller
    3. if segment exhausted: find_segment_with_free -> reserve_small_segment

  dealloc_small(ptr):
    1. if same-thread segment: push to BinTable head  -- pure pointer write via node seam
    2. if cross-thread segment: push (offset|class) to RemoteFreeRing (§5)
```

The common case (steps 1 / dealloc step 1) has no lock and no atomic
operation. The REFILL_BATCH of 31 was measured in commit 81fec54: larger
batches hurt locality with no throughput gain.

### 4.5 Per-thread magazine (tcache) fast path — `fastbin` / `production`

Under `fastbin` (default on in `production`), a per-thread
array-based magazine per size class is layered in `HeapCore` on top of
the segment substrate. Alloc pops from `slots[c][count[c]-1]` (a
pointer load + count decrement — no metadata touch); free pushes to the
same array. Miss/overflow paths go through the per-class refill/flush
batch APIs against the underlying `AllocCore`. The M2 double-free
guarantee for the two own-thread resting places (this class's magazine
and the BinTable free list) is enforced by two hot-metadata oracles run
unconditionally on every free — an in-magazine `slots` scan and the
BinTable `is_free` bitmap — with the free path **never touching the
block body**. Stats (`tcache_hits`, refill/flush counts) are collected
on the miss path. See `docs/FASTBIN_DESIGN.md` for the full design and
the R2 residual note (ring↔magazine cross-thread double-free residual
limit of M2 — task #164).

### TLS heap binding

`SeferAlloc::alloc` calls `current_for_alloc()`, which loads a thread-local
raw pointer to the current `HeapCore`. On first call per thread the pointer is
null, triggering a one-time `HeapRegistry::claim()` (a `Mutex`-protected slot
claim). After init the TLS pointer is stable for the thread's lifetime;
subsequent calls pay only a TLS load and a null check.

---

## 5. Cross-thread free (Phases 10-12)

Full protocol specification: [CROSS_THREAD_STATE_MACHINES.md](CROSS_THREAD_STATE_MACHINES.md).
Investigation of the drain-reclaim race: [RACE_DRAIN_RECLAIM.md](RACE_DRAIN_RECLAIM.md).

### RemoteFreeRing (Phase 12.6 / Variant-2)

When a thread frees a block it did not allocate, it pushes a ring entry
`(offset | class)` into the owning segment's `RemoteFreeRing` (a lock-free
MPSC ring stored in the segment's metadata, never decommitted). The freer:

- Does **NOT** dereference the block (no write to `block.next` or any
  in-block field).
- Stamps the size class from its own `Layout` argument.

The owner reclaims lazily on the next alloc-slow-path drain. It reads the ring
entry, uses the stamped class (not `page_map`), and returns the block to the
`BinTable`.

### Why the freer stamps the class (§13 fix)

Reading the class from `page_map` on the cross-thread path is unreliable:
the `page_map` class for a page under active bump allocation can be stale or
reflect the bump cursor's current target rather than the block's original class
(mixed-class pages). The freer has the caller's `Layout` at the dealloc call
site -- that is the authoritative class. This fix (§13 in
[RACE_DRAIN_RECLAIM.md](RACE_DRAIN_RECLAIM.md)) resolved the drain-reclaim UAF
that manifested as `STATUS_ACCESS_VIOLATION` in the MT test.

### Field-specific atomic reads (§11 fix)

On the cross-thread path, reading individual fields from the segment header
(`magic_at`, `kind_at`, `owner_thread_free_at`) uses field-specific `offset_of!`
reads rather than reading the full `SegmentHeader` struct. A full `read_at`
would race the owner's `bump` writes to adjacent fields. This is the §11 fix
from commit #43.

### State machines

SM-BLOCK has four states (`UNCARVED`, `LIVE`, `LOCAL_FREE`, `REMOTE_FREED`)
with strict single-actor rules: only the Owner mutates `BinTable`; only the
Remote performs the atomic ring-push. The loom model in `tests/loom_*.rs`
checks these transitions under bounded interleavings.

---

## 6. Phase 35 -- M6 decommit (alloc-decommit feature)

Full design: [PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md).

### What it does

When a small segment's `live_count` drops to zero AND the segment is not the
current carve target, the owner:

1. Decommits payload pages (`madvise MADV_DONTNEED` / `VirtualFree MEM_DECOMMIT`).
   Metadata pages (header, page_map, BinTable, AllocBitmap, RemoteFreeRing)
   remain committed.
2. Resets the segment to blank state: `bump = small_meta_end`, all BinTable
   heads = `FREE_LIST_NULL`, page_map payload pages = `Free`, AllocBitmap = 0.
3. Sets the `decommitted` flag in the header. The SegmentTable slot's base is
   NULLed (from #60), making it recyclable.

On next reuse as `small_cur`, the segment is recommitted
(`os::recommit_pages`), the flag is cleared, and normal carving resumes.

Feature `alloc-decommit` is default-off. Without it the behavior is unchanged;
no layout changes occur (all new fields are present in every build for layout
stability).

**Platform note (macOS/XNU):** on Darwin `madvise(MADV_DONTNEED)` is advisory
and lazy — RSS reclamation is best-effort, not the prompt Linux behavior, and
it carries no zero-fill-on-next-access guarantee. Correctness is unaffected
(every `alloc_zeroed` zeroes explicitly), only the RSS-reclaim timing differs.

### Key insight: no epoch reclamation (M11) needed

The original Phase 12 design assumed M11 (crossbeam-epoch) was required before
decommit because the old intrusive cross-thread free wrote `next` *inside* the
block -- a freer could write into a decommitted page. Variant-2 (Phase 12.6)
dissolves this: the freer pushes to the `RemoteFreeRing` in metadata (never
decommitted) and never dereferences the block. Safety argument:

1. Decommit only when `live_count == 0` -- no live block in the payload range.
2. A late valid cross-thread free is impossible when `live_count == 0`:
   all blocks are already free; freeing a free block is double-free, caught
   by AllocBitmap.
3. Owner-side `reclaim_offset` computes the block address via metadata offset
   arithmetic, reads `magic`/`kind`/bitmap (`is_free`) -- all in metadata --
   and no-ops for free blocks before touching payload. The decommitted page is
   never accessed.
4. Both `reclaim` and `decommit` run on the owner thread -- serialized, no
   reclaim-vs-decommit race.

Full argument: [PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md) §1.

### OPT-E: large-segment free-cache (#65) + Phase 1-3 adaptive policy (#90-#92)

`alloc_large` returns dedicated segments (one per large allocation). Without a
cache, every large alloc+free round-trips through the OS. OPT-E adds a
per-`AllocCore` free-cache (`LARGE_CACHE_SLOTS = 8`): a freed large
segment is held committed rather than munmapped. On a cache hit, the next
large alloc reuses it with no OS call. Flagship measurement on 4 MiB
alloc+free: **~58.6 ns vs ~716 ns for mimalloc (~12.2× faster)** (source:
[ALLOC_BENCH.md](ALLOC_BENCH.md) large alloc+free table, as of commit 4a4ff5e);
64 MiB **~60.8 ns (~33× faster than mimalloc)** — same source/run, absolute
mimalloc figure omitted here to avoid pairing it with a different bench
run's number; see the full table for the paired values. See the
[ALLOC_BENCH.md](ALLOC_BENCH.md) OPT-E section for the full table.

**Adaptive policy** (tasks #90-#92, the "client controls / we ship sane
defaults" model):

- *Phase 1 — byte-budget admission.* The old per-span cap
  (`MAX_CACHED_LARGE_BYTES = 64 MiB`) was an artificial disability — a span
  larger than the cap could never be cached, so a process churning 100 MiB+
  buffers paid the full OS round-trip every cycle. Replaced with a
  per-shard byte budget set via `LargeCacheConfig::budget_bytes(N)`
  (default unbounded when unset); any size span can enter the cache, FIFO
  eviction releases the oldest if the budget would be exceeded. OS-OOM is
  propagated as `null` per the `GlobalAlloc` contract (audited end-to-end).

- *Phase 2 — lazy exponential decay.* "Allocate fast, release slowly":
  on every large op a single `Instant::now()` comparison checks whether
  the configured `LargeCacheConfig::decay_interval_ms(N)` window
  (default 1000 ms) has elapsed; if so, `excess = cached −
  LargeCacheConfig::headroom_bytes(N)` (default 256 MiB) is multiplied by
  `LargeCacheConfig::decay_rate_percent(N)` (default 10 %) and that
  many bytes are FIFO-evicted to the OS. Self-damping (no oscillation),
  no background thread (idle process pays nothing — mobile-friendly),
  every knob resolved at compile time from the `const fn` builder — no
  environment reads, no runtime parse errors (env vars
  `SEFER_LARGE_CACHE_BUDGET` / `SEFER_LARGE_CACHE_MODE` were removed in
  0.2.0).

- *Phase 3 — mode selector (background-thread stub).* `LargeCacheMode
  { Lazy, Background, Both }` enum is wired through the
  `LargeCacheConfig::mode(m)` builder method. Default `Lazy` preserves
  Phase 2 behaviour bit-for-bit; `Background` / `Both` currently fall
  back to lazy while the full background scavenger thread (Mutex
  refactor + registry iteration + safe spawn timing + TSan validation)
  is deferred to a follow-up. The mode-selector plumbing means flipping
  the switch later is a non-breaking change.

Full configuration table is in the README "Tuning the large-segment cache"
section.

---

## 7. NUMA-aware path (Phase 58, numa-aware feature)

Full design: [PHASE_NUMA_DESIGN.md](PHASE_NUMA_DESIGN.md).

### OS seam: `src/alloc_core/numa.rs`

A confined `unsafe` module (modeled after `os.rs`) with three entry points:

- `current_node() -> u32` -- query the NUMA node of the calling thread.
  Linux: `sched_getcpu` + `/sys/devices/system/node/` topology.
  Windows: `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`.
  macOS / miri: returns `NO_NODE` (no-op platform).
- `bind_segment(base, len, node)` -- Linux `mbind(2)` with `MPOL_PREFERRED`
  after `mmap`, before first page fault. No-op on Windows (binding happens at
  reservation time) and macOS.
- `reserve_aligned_on_node(usable, node)` -- Windows path:
  `VirtualAllocExNuma` instead of `VirtualAlloc`.

### Integration points

- `SegmentHeader::node_id: u32` -- layout-stable field present in every build
  (`NO_NODE = u32::MAX` when feature is off). Accessed via `offset_of!`
  field-specific reads.
- `reserve_small_segment` and `alloc_large` stamp `node_id` on the new segment
  immediately after reservation, before any page access.
- `find_segment_with_free` prefers local-node segments, with non-local as
  fallback.

### Honest limitations

QEMU / `numa=fake` verify correctness (correct `mbind` call, correct
`node_id` stored). They do **not** verify latency asymmetry: on one physical
socket all fake nodes have identical access latency. Real measurement requires
2-socket hardware. This is documented in [PHASE_NUMA_DESIGN.md](PHASE_NUMA_DESIGN.md) §5.

Best-effort NUMA benefit pairs naturally with the `pinning` feature
(`core_affinity`): when threads are pinned, NUMA node membership is stable.
Without pinning the OS may migrate threads; new segments go to the new node
but existing segments remain on the old one (MVP strategy: ignore migration).

---

## 8. Verification stack

| Tool | What it verifies | Location |
|---|---|---|
| Unit tests | Construction, edge cases, invariants | `tests/*.rs` (119 files, as of commit 4a4ff5e) |
| proptest differential | Op-stream agreement between `AllocCore` and a reference model | [`tests/alloc_core_differential.rs`](../tests/alloc_core_differential.rs), [`tests/differential.rs`](../tests/differential.rs) |
| miri (strict-provenance) | UAF, races at byte level, double-free, out-of-bounds | `tests/region_invariants.rs`, `tests/decommit_miri_cycle.rs`, `tests/reclaim_offset_unit.rs` |
| loom | Cross-thread protocol correctness under bounded interleavings (11 models) | `tests/loom_bootstrap_cas.rs`, `tests/loom_deferred_large.rs`, `tests/loom_epoch.rs`, `tests/loom_fallback_init.rs`, `tests/loom_free_slots_aba.rs`, `tests/loom_magazine_ring_compose.rs`, `tests/loom_registry.rs`, `tests/loom_remote_ring.rs`, `tests/loom_sharded.rs`, `tests/loom_thread_free.rs`, `tests/loom_xthread_protocol.rs` |
| ThreadSanitizer | Real cross-thread data races (not model-checked) | CI job + manual (verified x3: cross-thread path + decommit path) |
| Valgrind memcheck | UAF, leaks at process level | CI job + manual (verified clean) |
| aarch64 (qemu-user) | Code-gen correctness + relaxed-memory smoke | CI job + manual (verified 13/13 test suites) |
| libFuzzer | Op-stream invariants under random input | `fuzz/fuzz_targets/region_ops.rs`, `fuzz/fuzz_targets/global_alloc_ops.rs`, `fuzz/fuzz_targets/heap_core_ops.rs` (fastbin magazine) |
| Soak harness | N-thread x hours stability | [`examples/soak_xthread.rs`](../examples/soak_xthread.rs) |
| tokio burn-in | Live `#[global_allocator]` under async runtime | [`examples/tokio_burn_in.rs`](../examples/tokio_burn_in.rs) |
| Macro-bench (larson / mstress) | MT throughput vs mimalloc / System | [`examples/malloc_macro.rs`](../examples/malloc_macro.rs) |
| RSS probe | Memory recovery under alloc-decommit | [`examples/rss_probe.rs`](../examples/rss_probe.rs) |
| Flamegraph profiling | Hot path identification, OPT candidates | [`docs/PROFILE_FLAMEGRAPHS.md`](PROFILE_FLAMEGRAPHS.md) |

### Proptest scope and speed

proptest runs a modest default case count (~64) as a smoke check -- not
exhaustive fuzzing. miri runs on specific bounded tests (not the full suite).
Heavy / exhaustive multi-arch runs are CI jobs (the Phase 32 hardening gate,
commit 4e034e5), not the everyday cycle. See CLAUDE.md "Speed" section.

---

## 9. Performance summary

Full measurements and OPT candidates: [ALLOC_BENCH.md](ALLOC_BENCH.md) and
[PROFILE_FLAMEGRAPHS.md](PROFILE_FLAMEGRAPHS.md).

### Single-thread small-class churn

`SeferAlloc` is approximately 1.2-2x behind mimalloc on small classes
(16-256 B). The gap is a constant per-call overhead from the TLS registry hop
(`current_for_alloc`) and the `stamp_segment_owner` Acquire load on every
`alloc`. At 1024 B `SeferAlloc` equals or leads mimalloc.

Flamegraph hot paths (small-class, single-thread):
1. `SegmentTable::contains_base` -- O(segments) scan per `dealloc` (now O(1)
   after #67 hash addition).
2. `HeapCore::stamp_segment_owner` -- Acquire load + conditional Release store
   on every alloc (OPT-A/OPT-C: skip when segment base has not changed).

### Multi-thread (larson / mstress macro-bench)

| Workload | T=1 | T=2 | T=4 |
|---|---|---|---|
| larson SeferAlloc | ~21 M ops/s | ~25 M | **~40 M** |
| larson mimalloc    | ~27 M ops/s | ~19 M | ~32 M |
| mstress SeferAlloc | ~25 M ops/s | ~43 M | ~65 M |
| mstress mimalloc    | ~33 M ops/s | ~43 M | ~65 M |

At T=4 (larson) `SeferAlloc` passes mimalloc. The per-thread heap means the
fast path takes no shared lock; cross-thread frees route through the per-segment
ring without contending the producer. mimalloc shows a dip at T=2 (larson) on
this host that `SeferAlloc` does not exhibit.

### Large alloc / free (after OPT-E large-segment cache)

`SeferAlloc` 4 MiB alloc+free: **~58.6 ns** vs mimalloc ~716 ns -- **~12.2×
faster** (source: [ALLOC_BENCH.md](ALLOC_BENCH.md) large alloc+free table, as of
commit 4a4ff5e; the same number is cited in §6 above).
The cache eliminates the OS round-trip on repeated large alloc+free cycles.
Without the cache (before #65) every large `dealloc` called `munmap`/
`VirtualFree`, making large allocs significantly slower than mimalloc.

### realloc in-place (OPT-F)

When `new_size <= block_size(old_class_idx)`, `realloc` returns the same
pointer without alloc+copy+dealloc. Measured improvement on an unfavorable
pattern: -28.6% time (alloc avoided entirely).

### Honest gap

The single-thread small-class hot path remains the performance gap relative to
mimalloc. OPT-C (stamp cache) and OPT-B (hash lookup) are 1-2% polish items.
The multi-thread story is competitive-to-ahead. These numbers are from a
Windows 10 dev machine; see [ALLOC_BENCH.md](ALLOC_BENCH.md) for the full
context and caveats.

---

## 10. Where to read next

| Document | What it covers |
|---|---|
| [DESIGN.md](DESIGN.md) | Cartographer / Membrane / Hand model; the Region<T> dense generational layout; where `unsafe` lives and why |
| [ALLOC_PLAN.md](ALLOC_PLAN.md) | Phase 8-13 spec: the four showstoppers and how they are dissolved; architecture descent diagram; per-phase contracts |
| [INVARIANTS.md](INVARIANTS.md) | I1-I6 (Region/Handle face) and M1-M8 (alloc face); why handles, not pointers |
| [PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md) | M6 decommit policy; the proof that epoch reclamation (M11) is not needed under Variant-2 cross-thread free |
| [PHASE_NUMA_DESIGN.md](PHASE_NUMA_DESIGN.md) | NUMA OS seam; integration points; migration strategy; testing without real multi-socket hardware |
| [CROSS_THREAD_STATE_MACHINES.md](CROSS_THREAD_STATE_MACHINES.md) | Formal state-machine spec for SM-BLOCK and SM-SEGMENT; actor rules; loom verification target |
| [RACE_DRAIN_RECLAIM.md](RACE_DRAIN_RECLAIM.md) | Full diagnostic trail of the drain-reclaim UAF (§1-§14); the true root cause (class derivation §13); the shipped fix |
| [ALLOC_BENCH.md](ALLOC_BENCH.md) | Single-thread and MT benchmark results; OPT-E large cache; heap-core pinning honest verdict; all numbers in context |
| [PROFILE_FLAMEGRAPHS.md](PROFILE_FLAMEGRAPHS.md) | Flamegraph analysis across 4 workloads; 6 OPT candidates (A-H) with estimated impact |
| [DURABILITY.md](DURABILITY.md) | Ultra-long-run counter inventory: every monotonic/wrapping/saturating cursor with its width, wrap arithmetic, verdict, and boundary test — and the rule for adding a new one |
