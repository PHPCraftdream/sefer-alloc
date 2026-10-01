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

**Current-path update:** 2026-09-30 dirty terminal-sidecar snapshot.
Historical phase/performance measurements below are not acceptance evidence
for this cutover. No release GO or new performance verdict is implied.

---

## Table of contents

1. [The big picture: two independent APIs, one package](#1-the-big-picture-two-independent-apis-one-package)
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

## 1. The big picture: two independent APIs, one package

`sefer-alloc` ships two APIs that share no backing memory. `Region<T>` /
`Handle<T>` is a typed handle store over a third-party `slotmap::SlotMap` —
it never touches the segment substrate below. `SeferAlloc` is a separate,
OS-backed segment allocator with its own Cartographer/Hand tiers:

```
  +--------------------------+   +-----------------------------------+
  |  Region<T> / Handle<T>   |   |  SeferAlloc (alloc face)           |
  |  typed, generational     |   |  unsafe impl GlobalAlloc            |
  |  wraps slotmap::SlotMap  |   |  MT, acceptance pending (feature:   |
  |  #![forbid(unsafe_code)] |   |  alloc-global + alloc-xthread)      |
  |  own storage, no         |   |                                     |
  |  segment substrate       |   |  CARTOGRAPHER -- 100% safe          |
  |  (concurrent tier: an    |   |   integer arithmetic (size classes, |
  |  optional confined       |   |   bin tables, page maps, segment    |
  |  `hand` module of its    |   |   registries, placement, decommit,  |
  |  own, unrelated to the   |   |   O(1) owner lookup)                |
  |  alloc face's Hand)      |   |                                     |
  |                          |   |  SEGMENT SUBSTRATE -- self-hosted   |
  |                          |   |   OS-backed memory (Phase 8+): 4 MiB|
  |                          |   |   SEGMENT-aligned spans; metadata   |
  |                          |   |   carved from segments; System-     |
  |                          |   |   backed route sidecars (M5)       |
  |                          |   |                                     |
  |                          |   |  HAND -- confined unsafe seams      |
  |                          |   |   (see SS2)                          |
  +--------------------------+   +-----------------------------------+
```

- `Region<T>` / `Handle<T>` -- a typed, generational handle store. A stale
  handle returns `None`, never undefined behaviour. The single-threaded core
  wraps the `slotmap` crate and is `#![forbid(unsafe_code)]` (no
  version-scoped audit record for `slotmap` is tracked by this project --
  see `crates/sefer-region/README.md` "## Safety"); it has
  no OS segment, no page map, no page registry of its own. The
  concurrent tier (feature `experimental`) adds lock-free reads via `arc-swap`
  (RCU, zero own `unsafe`) or `crossbeam-epoch` (one confined `unsafe` module) --
  this is a separate, `Region`-scoped confined-`unsafe` seam, not the same
  `Hand` organ the alloc face uses.
- `SeferAlloc` -- `unsafe impl GlobalAlloc` over the per-thread segment heap.
  `alloc-global` implies `alloc-xthread`; foreign frees now publish to a
  pinned route sidecar, not a segment/heap ring. `production` bundles
  `alloc-global`, `alloc-xthread`, `alloc-decommit`, `fastbin`,
  `alloc-segment-directory` and `primordial-lazy-commit`. Its terminal
  cutover still needs full acceptance; autonomous ownerless progress is
  opt-in through fallible `SeferAlloc::start_maintenance()`.

Full safety invariants for both APIs: [INVARIANTS.md](INVARIANTS.md)
(I1-I7 for `Region`/`Handle`, M1-M8 for `SeferAlloc` -- the M-series
invariants describe `SeferAlloc`'s own segment substrate only and do not
apply to `Region<T>`, which has none).

**Additional opt-in features** (default OFF, NOT part of `production` — see the
feature table in [README.md](../README.md#feature-flags)):

- `hardened` (additive over `fastbin`) — own-thread interior-pointer
  detection. The historical X7 ring-note generation guard was removed with
  the ring; it is not a current foreign-free guarantee.
- `alloc-stats` — per-hit diagnostic counters: bumps `stats().tcache_hits`
  (magazine) and `stats().large_cache_hits` (large cache) on each hit. The
  per-hit increment is compiled out when off (those two fields then read 0); the
  counter storage is always present so toggling never changes layout/ABI.

---

## 2. Three organs: Cartographer / Membrane / Hand

The founding principle (from [DESIGN.md](DESIGN.md)):

> **All the intelligence lives in the safe Cartographer** (pure arithmetic over
> `u32` indices and offsets), so the Hand stays mechanical and tiny. You prove
> a total membrane and an integer algorithm, not a tangle of pointer math.

| Organ | Responsibility | Safety |
|---|---|---|
| **Cartographer** | All placement / free-list / decommit logic -- pure integer arithmetic over indices. Never touches memory directly. | `safe` |
| **Membrane** | Typed API: `Handle<T>`, generation checks, lifetimes; `AllocCore::alloc` / `SeferAlloc::alloc` -- total, cannot express UB. | `safe` |
| **Hand** | Confined `unsafe` seams that touch raw memory or issue OS syscalls. | `confined unsafe` |

### Workspace companions

Before discussing the internal seams, the workspace structure matters for the
audit story. The four original extractions were:

```
sefer-alloc
 ├── sefer-region    (crates/sefer-region)       — Handle<T>/Region<T>/SyncRegion<T>
 ├── aligned-vmem    (crates/aligned-vmem)         — OS virtual-memory aperture  (feature: alloc-core)
 ├── numa-shim       (crates/numa-shim)         — NUMA detection + binding    (feature: numa-aware)
 └── malloc-bench-rs (crates/malloc-bench-rs) — portable GlobalAlloc bench harness (standalone)
```

The workspace now has ten companion crates; the complete current list is in
[README §Workspace](../README.md#workspace-ten-independently-publishable-companion-crates).
Each original extraction is a real crates.io crate. The
extraction **improved the audit story**: the two OS-unsafe sub-problems are now
small, single-responsibility crates that can be audited in complete isolation.

### Confined unsafe seams (current inventory — verifiable with `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`)

**External publishable crates** (independently auditable):

| Crate | Path | Unsafe story |
|---|---|---|
| `aligned-vmem` | `crates/aligned-vmem/` | `#![allow(unsafe_code)]` — entire crate IS the OS aperture; sole responsibility = SEGMENT-aligned mmap/VirtualAlloc + decommit. Small, audit in isolation. |
| `numa-shim` | `crates/numa-shim/` | `#![allow(unsafe_code)]` — entire crate IS the NUMA syscall shim; sole responsibility = mbind(2)/VirtualAllocExNuma. Small, audit in isolation. |
| `malloc-bench-rs` | `crates/malloc-bench-rs/` | `#![allow(unsafe_code)]` — confined to alloc_block/free_block/drain_mailbox helpers; every unsafe block carries `// SAFETY:`. Bench harness, not runtime. |
| `sefer-region` | `crates/sefer-region/` | `#![forbid(unsafe_code)]` — zero own unsafe (shown for contrast; does **not** match the grep above); slotmap's core owns the generational layout (no version-scoped audit record for `slotmap` is tracked by this project — see `crates/sefer-region/README.md` "## Safety"). |

**Internal sefer-alloc seams — tier 1 (module-level)** (compiler-enforced):

The current tree has **26** tier-1 `#![allow(unsafe_code)]` files (20 in
`src/`, 6 in `crates/`) and **103** item-scoped allows across **34** files.
The ordinary `production` build activates 16 internal tier-1 seams;
`--cfg loom` adds its bootstrap shim, while `batch-api`,
`large-cache-extended` and `experimental` add optional seams. See
[README §Where unsafe lives](../README.md#where-unsafe-lives-the-complete-list)
for the path-by-path current inventory. In particular,
`registry::segment_route::directory` owns System-backed route entries and
pins, and `registry::heap_registry::maintenance` owns the exclusive
maintenance handoff. Neither the removed overflow sidecar nor the old
remote inbox is an active seam. `numa-aware` adds no internal unsafe seam;
its wrapper delegates to `numa-shim`.

Outside these tier-1 modules AND the tier-2 item-scoped allows (individual
`unsafe fn` boundaries in otherwise-safe files — see README §"Where unsafe
lives" for the tier-2 table, task #101 / R4-9), an `unsafe` token is a hard
compile error (`#![deny(unsafe_code)]` when any of those features are active;
`#![forbid(unsafe_code)]` with only the default `std` feature). The
source-of-truth catalogue is in [`src/lib.rs`](../src/lib.rs) at the
top-level comment.

---

## 3. The segment substrate (Phase 8)

### Segment layout

Ordinary Small/Primordial segments are `SEGMENT`-aligned (4 MiB) OS spans.
Large requests with alignment `>= SEGMENT` use biased geometry: the OS
release token, page-aligned usable metadata root, and aligned payload are
distinct addresses. Numeric payload lookup selects the stored root; it
never turns the caller pointer into a metadata capability. The ordinary
segment header and metadata are laid out as follows:

```
  [0x000000] SegmentHeader  (magic, kind, segment_id, bump, owner_thread_free,
                             owner_state, live_count, decommitted, node_id, ...)
  [0x001000] PageMap        (per-page Kind: Free / SmallClass(idx) / Large)
  [after]    BinTable       (per-size-class free-list head, intrusive pointers
                             into payload blocks)
  [after]    AllocBitmap    (1 bit per MIN_BLOCK slot -- O(1) double-free guard)
  [payload]  block data     (carved by bump cursor, returned to callers)
```

The route directory and its typed sidecars are separately allocated via
`System`, not carved from payload. Outstanding issue credits keep a
reservation live until its owner consumes terminal publications. Sidecar
pins can outlive reservation release without dereferencing that reservation.

### SegmentTable

A per-heap `SegmentTable` starts in the primordial segment; route handles
register each issued segment in the process-stable directory before exposure.
The fixed table has 4096 slots, with slot 0 reserved for Primordial.
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
segments. Route descriptors and sidecars are allocated through `System`,
bypassing the installed `GlobalAlloc`; no `Vec`/`Box` recursion is introduced
on allocator callbacks. This is M5 (reentrancy-freedom). See
[ALLOC_PLAN.md](ALLOC_PLAN.md) §1 for the earlier self-hosting rationale.

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
    1. if same-thread segment: push to BinTable head via node seam
    2. if foreign: numeric route lookup + terminal sidecar bitmap publish (§5)
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
the historical R2 ring residual note (task #164), which does not describe
the current terminal-sidecar ingress.

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

### Terminal sidecar ingress

The foreign `GlobalAlloc::dealloc` route treats the caller's pointer as a
numeric lookup key. A shard lock protects pin acquisition on a published
descriptor; the pin retains a System-backed sidecar and stored owner root.
Small/Primordial issue records the class in the sidecar. A foreign Small
publisher touches only that sidecar and terminates at a bitmap `fetch_or`;
Large terminates at its instance state transition. No caller-derived header
root, block-slack predecessor, ring, heap overflow or spill is involved.

An exclusive owner sweep fixes its table high-water on entry, then visits
each live Small/Primordial issued-word once with `swap(0, AcqRel)`. It
consumes detached records through the stored root, updates the free list and
retires exactly one issue credit. A post-cut free waits for a later pass;
strict trim finitely covers all correctly published pre-entry words without
waiting for new publishers to stop. Large consumes only a claimed `PENDING`
descriptor. Directory pins are retired independently of OS reservations.

Public `trim_current_thread()` and TLS teardown use this sweep. The
ownerless executor is **explicitly** started with fallible
`SeferAlloc::start_maintenance()`: success confirms a running process-life
worker; failure leaves no autonomous guarantee. Its round-robin passes try
exclusive maintenance leases for FREE heaps and the fallback lock, without
stealing a live owner. Fair scheduling is required for eventual logical
retirement; physical OS return still depends on live credits and retention
policy. See [the sidecar revision](REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md)
for proof obligations and acceptance gates. The earlier ring designs in
[RACE_DRAIN_RECLAIM.md](RACE_DRAIN_RECLAIM.md) are historical.

---

## 6. Phase 35 -- M6 decommit (alloc-decommit feature)

Full design: [PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md).

### What it does

When a small segment's `live_count` drops to zero AND the segment is not the
current carve target, the owner routes it through
`release_or_pool_empty_segment` (Mechanism 2, task #51; pool-admission
semantics finalized by R8-10, task #223):

1. **Pool admission** (pool enabled and not full): the segment is pushed onto
   the hysteresis pool's LIFO front and left EXACTLY as it was the instant it
   emptied — still registered, pages still committed, free lists intact,
   nothing reset or decommitted (identically on the eager and
   `alloc-lazy-commit` legs; R8-10 removed the former B3
   decommit-on-admission, which cost 50–75× more commit/decommit syscalls per
   empty→pool→reuse cycle than the eager path's zero). Reuse goes through
   `find_segment_with_free`'s free-list path with no OS work.
2. **Release** (pool disabled/full, decay-tick eviction, or pool drain under
   OS memory pressure): the release-follows fast reset runs (`bump =
   small_meta_end` + the `decommitted` flag — the only load-bearing pieces,
   since the whole reservation immediately goes back to the OS via
   `table.recycle` → `os::release_segment`), and the SegmentTable slot is
   NULLed, making it recyclable.

Pooled retention is temporary, not merely bounded: the decay tick
(`maybe_decay_small_pool`) evicts the coldest entry once the workload goes
quiet, releasing its whole reservation.

Feature `alloc-decommit` is default-off. Without it the behavior is unchanged;
no layout changes occur (all new fields are present in every build for layout
stability).

**Platform note (macOS/XNU):** on Darwin `madvise(MADV_DONTNEED)` is advisory
and lazy — RSS reclamation is best-effort, not the prompt Linux behavior, and
it carries no zero-fill-on-next-access guarantee. Correctness is unaffected:
`alloc_zeroed` zeroes explicitly on every REUSED span (the Large
`large_cache`-hit path, and every Small allocation); the only zeroing skip
(#221) applies to a genuinely fresh OS reservation, whose zero-fill comes
from the initial `mmap`/`VirtualAlloc` itself, never from a decommit
round-trip — so `MADV_DONTNEED` laziness cannot feed a non-zeroed byte into
an `alloc_zeroed` result.

### Key insight: no epoch reclamation (M11) needed

The original Phase 12 design assumed M11 (crossbeam-epoch) was required before
decommit because the old intrusive cross-thread free wrote `next` *inside* the
block. The current foreign publisher touches only a separately pinned
sidecar, never reservation bytes. Owner issue credits keep the reservation
live until all valid published/unpublished frees are retired, and only the
exclusive owner decommits or releases payload. A post-publication producer
pin retains the descriptor, not the reservation. This removes the old
payload-write race without epoch reclamation; the full new proof obligations
are in [the sidecar revision](REMOTE_FREE_SIDECAR_REVISION_2026-09-29.md).
The earlier argument is preserved in
[PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md) as historical design.

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
  inline decay (the separately started maintenance worker is a later feature),
  every knob resolved at compile time from the `const fn` builder — no
  environment reads, no runtime parse errors (env vars
  `SEFER_LARGE_CACHE_BUDGET` / `SEFER_LARGE_CACHE_MODE` were removed in
  0.2.0).

- *Phase 3 — mode selector (background-thread stub).* `LargeCacheMode
  { Lazy }` (`#[non_exhaustive]`) is wired through the
  `LargeCacheConfig::mode(m)` builder method. Default `Lazy` preserves
  Phase 2 behaviour bit-for-bit. Earlier revisions carried unimplemented
  `Background`/`Both` variants that silently fell back to `Lazy`, then
  briefly panicked at heap-materialisation time — both removed in round3
  (`docs/reviews/2026-07-12-round3-remediation-plan.md`, решение №2) in
  favour of leaving the enum `#[non_exhaustive]`: adding a real mode later
  (Mutex refactor + registry iteration + safe spawn timing + TSan
  validation) is a non-breaking addition, not a re-add of a removed variant.

Full configuration table is in the README "Tuning the large-segment cache"
section.

---

## 7. NUMA-aware path (Phase 58, numa-aware feature)

Full design: [PHASE_NUMA_DESIGN.md](PHASE_NUMA_DESIGN.md).

### OS seam: `src/alloc_core/platform/numa.rs`

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

Best-effort NUMA benefit depends on threads staying on their node: when a
thread is pinned to a core, its NUMA node membership is stable, so new segments
consistently land on that node. Without pinning the OS may migrate threads; new
segments go to the new node but existing segments remain on the old one (MVP
strategy: ignore migration). Note that the `pinning` feature's `PinnedRunner`
only pins the *`ShardedRegion` worker threads* of the legacy concurrent tier —
it does NOT pin `SeferAlloc`'s own allocating threads. To get stable NUMA
membership for allocator threads you pin them yourself (e.g. via
`core_affinity` directly, or your runtime's affinity API); `numa-aware` then
steers each pinned thread's fresh segments to its node.

---

## 8. Verification stack

File counts below cover root `tests/*.rs` targets; nested support and
compile-fail fixtures are not counted as integration targets.
Older manual/CI verification rows retain historical evidence only; they
do not certify the terminal-sidecar snapshot.

| Tool | What it verifies | Location |
|---|---|---|
| Unit / integration tests | Construction, edge cases, invariants; snapshot acceptance pending | `tests/*.rs` (320 files) |
| proptest differential | Op-stream agreement between `AllocCore` and a reference model | [`tests/alloc_core_differential.rs`](../tests/alloc_core_differential.rs), [`tests/differential.rs`](../tests/differential.rs) |
| miri | Selected provenance/aliasing checks; snapshot execution pending | `scripts/miri.mjs`, including `r8_global_box_provenance` and tagged-index-stack `narrow_domain_unchecked_storage`; not a whole-project proof |
| loom | Bounded interleavings; snapshot execution pending | **Root (11 files):** `tests/loom_active_kind_index.rs`, `tests/loom_epoch.rs`, `tests/loom_r8_maintenance_lease.rs`, `tests/loom_r11_epoch_false_full.rs`, `tests/loom_r11_registry_claim.rs`, `tests/loom_r11_small_sidecar.rs`, `tests/loom_registry_free_slots.rs`, `tests/loom_sharded.rs`, `tests/loom_sidecar_bitmap.rs`, `tests/loom_terminal_large.rs`, `tests/loom_terminal_owner_drain.rs`; **member suites:** `crates/once-ptr-cell/tests/loom_once_ptr_cell.rs`, `crates/tagged-index-stack/tests/loom_aba.rs` |
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

All figures in this section predate the terminal-sidecar snapshot and are
historical workload measurements, not its speed/RSS verdict.

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
| [INVARIANTS.md](INVARIANTS.md) | I1-I7 (Region/Handle face) and M1-M8 (alloc face); why handles, not pointers |
| [PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md) | M6 decommit policy; the proof that epoch reclamation (M11) is not needed under Variant-2 cross-thread free |
| [PHASE_NUMA_DESIGN.md](PHASE_NUMA_DESIGN.md) | NUMA OS seam; integration points; migration strategy; testing without real multi-socket hardware |
| [CROSS_THREAD_STATE_MACHINES.md](CROSS_THREAD_STATE_MACHINES.md) | Formal state-machine spec for SM-BLOCK and SM-SEGMENT; actor rules; loom verification target |
| [RACE_DRAIN_RECLAIM.md](RACE_DRAIN_RECLAIM.md) | Full diagnostic trail of the drain-reclaim UAF (§1-§14); the true root cause (class derivation §13); the shipped fix |
| [ALLOC_BENCH.md](ALLOC_BENCH.md) | Single-thread and MT benchmark results; OPT-E large cache; heap-core pinning honest verdict; all numbers in context |
| [PROFILE_FLAMEGRAPHS.md](PROFILE_FLAMEGRAPHS.md) | Flamegraph analysis across 4 workloads; 6 OPT candidates (A-H) with estimated impact |
| [DURABILITY.md](DURABILITY.md) | Ultra-long-run counter inventory: every monotonic/wrapping/saturating cursor with its width, wrap arithmetic, verdict, and boundary test — and the rule for adding a new one |
| [GLOSSARY.md](GLOSSARY.md) | Identifier glossary: decodes the ID families in source comments (I1-I7, M1-M11, Phase/P/Ф codes, Э-series, OPT-A…H, X7, W/A/MUST/SEC items, `task #NNN`) |
