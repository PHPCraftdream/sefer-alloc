# NUMA-aware path in sefer-alloc — design spec

Research document (written before implementation). Closes gap #7
"no awareness of NUMA nodes" in ALLOC_BENCH. Behind the feature flag `numa-aware`
(default off — default behavior does not change).

---

## §0. What already exists — NUMA-relevant points

### OS-seam (`src/alloc_core/os.rs`)

The file contains the single confined-`unsafe` block for memory reservation
via `mmap`/`VirtualAlloc`. Currently NO NUMA-specific flags or calls are used
anywhere:

- **Linux**: `mmap` is called with `MAP_PRIVATE | MAP_ANON`, without `mbind(2)` and
  without `set_mempolicy(2)`. Pages are allocated according to the default process
  policy (usually "local" on a NUMA system, but not guaranteed).
- **Windows**: `VirtualAlloc(..., MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE)` —
  no `VirtualAllocExNuma`, no node specified.
- **macOS**: no public NUMA API (Apple Silicon is a SoC without classical
  NUMA topology; no public syscalls for `mbind`/`set_mempolicy`).
  **Status: `unsupported`.** The feature compiles as a no-op on Darwin.

`decommit_pages`/`recommit_pages` already exist and are properly wrapped.
NUMA calls should be placed alongside — in a separate file
`src/alloc_core/numa.rs`, similarly `cfg`-gated.

### Segment header (`src/alloc_core/segment_header.rs`)

`SegmentHeader` — `#[repr(C)]`, Copy, purely safe code. Currently
contains: `magic`, `kind`, `segment_id`, `bump`, `large_size/align`,
`reservation`, `reservation_len`, `owner_thread_free`, `owner_state`,
`next_abandoned`, `live_count`, `decommitted`.

**Key constraint (from compile-time assert):**
```
const _: () = assert!(size_of::<SegmentHeader>() <= PAGE);
const _: () = assert!(Layout::page_map_off() == PAGE);
```
The header must fit within a single page (4 KiB). Currently it is ~96 bytes;
a `node_id: u32` field adds 4 bytes — much less than PAGE. Safe.

### AllocCore (`src/alloc_core/alloc_core.rs`)

Segment reservation points:
- `reserve_small_segment` (lines 997–1034): calls `Segment::reserve(SEGMENT)`,
  then initializes metadata and selects the segment as `small_cur`.
- `alloc_large` (lines 957–993): calls `Segment::reserve(needed)` for
  each large allocation.

**Both locations are NUMA policy insertion points**. After `Segment::reserve` and before
writing the header, `numa::bind_segment(base, len, node_id)` must be called.

Selecting the "current" segment on the `alloc_small` path:
1. `pop_free` from `small_cur`
2. `find_segment_with_free` — scans all segments
3. `carve_block_with_refill` → `carve_block` (bump from `small_cur`)
4. `reserve_small_segment` (new segment)

NUMA preference — at steps 2 and 4: first look for a segment with `node_id == my_node`,
then reserve a new one on `my_node`.

### Heap (`src/heap/heap.rs`)

`Heap` is per-thread, created via TLS lazy initialization (see `heap_tls`
and `SeferMalloc`). Binding is to the thread via TLS, not to a CPU or NUMA node.

If a thread migrates between nodes — `Heap` is unaware of it. Segments
remain "owned by" the node on which they were created. This is an MVP assumption
(the "ignore" strategy — see §4).

### Features (`Cargo.toml`)

The pattern for a new flag follows `alloc-decommit`:
```toml
alloc-decommit = ["alloc-core"]
```
Similarly:
```toml
# NUMA-aware segment reservation: when allocating a new segment, requests
# pages from the NUMA node on which the calling thread is running
# (Linux: mbind(2), Windows: VirtualAllocExNuma).
# macOS: no-op (no public NUMA API). Default OFF.
# Requires `alloc-core`.
numa-aware = ["alloc-core"]
```

---

## §1. API targets — NUMA OS interface

### Linux

```c
// Bind already-mapped pages to a node:
int mbind(void *addr, unsigned long len,
          int mode,                      // MPOL_BIND = 2
          const unsigned long *nodemask,
          unsigned long maxnode,
          unsigned int flags);           // 0 for hard binding
```

- `mode = MPOL_BIND` — pages must come from a specific node.
- `mode = MPOL_PREFERRED` — prefer a node, allow another if memory is insufficient.
- `nodemask` — bitmask of nodes; for a single node `n`: `1UL << n`.
- Call AFTER `mmap`, BEFORE the first access to the pages (then pages
  "attach" to the desired node on page-fault).

Optionally — `set_mempolicy(2)` per-thread (changes the policy for all
subsequent `mmap` calls in the thread), but this is thread-global and dangerous.
Per-mapping `mbind` is preferable.

Topology information source (no external dependencies):
```
/sys/devices/system/node/node<N>/cpumap   — CPU mask belonging to node N
/sys/devices/system/node/online           — list of active nodes
/proc/self/status → Cpus_allowed          — allowed CPUs for the process
/proc/self/task/<tid>/stat → field[38]    — current CPU of the thread
```
For MVP it is sufficient to: read the current CPU (`sched_getcpu(3)` — a wrapper over
`getcpu(2)`), then find the node via `/sys/devices/system/node/node*/cpumap`.

### Windows

```c
LPVOID VirtualAllocExNuma(
    HANDLE hProcess,     // GetCurrentProcess()
    LPVOID lpAddress,    // NULL — OS chooses address
    SIZE_T dwSize,
    DWORD  flAllocationType,  // MEM_RESERVE | MEM_COMMIT
    DWORD  flProtect,         // PAGE_READWRITE
    DWORD  nndPreferred       // node number
);
```

Node discovery:
```c
UCHAR  node_count;
GetNumaHighestNodeNumber(&node_count);  // number of NUMA nodes - 1

PROCESSOR_NUMBER proc;
GetCurrentProcessorNumberEx(&proc);     // current logical processor

USHORT node_number;
GetNumaProcessorNodeEx(&proc, &node_number);  // node of this processor
```

**Note**: on Windows there is no equivalent of `mbind` for already-reserved
memory. Therefore on Windows there is no separation between reservation and binding —
`VirtualAllocExNuma` must be called instead of `VirtualAlloc`. This changes the
insertion point: `node_id` must be passed to `reserve_aligned`.

### macOS

No public NUMA API. Apple Silicon is a UMA (Unified Memory Architecture)
without physical NUMA asymmetry in the public model. The `numa-aware` feature
on Darwin compiles as a no-op: detection returns node 0,
`bind_segment` is a no-op.

---

## §2. Insertion points in our code

### New file `src/alloc_core/numa.rs`

Following the pattern of `src/alloc_core/os.rs`:

```rust
//! NUMA-seam: detect current NUMA node and bind a segment to a node.
//! Confined-`unsafe` module (the only place for NUMA syscalls).
//! Gated under `#[cfg(all(feature = "numa-aware", not(miri)))]`.
#![allow(unsafe_code)]  // analogous to os.rs

/// No node / feature disabled. Sentinel.
pub const NO_NODE: u32 = u32::MAX;

/// Detect the NUMA node of the current thread.
/// Returns `NO_NODE` if the API is unavailable or the feature is disabled.
pub fn current_node() -> u32 { ... }

/// Bind `[base, base+len)` to NUMA node `node`.
/// Call AFTER mmap/VirtualAlloc, before the first access to the pages.
/// On Windows: not applicable (binding happens at reservation time);
/// no-op when `node == NO_NODE` or on macOS.
pub fn bind_segment(base: *mut u8, len: usize, node: u32) { ... }

/// Version of `reserve_aligned` with NUMA preference (for Windows,
/// where `VirtualAllocExNuma` is needed instead of `VirtualAlloc`).
/// On non-Windows: reserves the usual way, then calls `bind_segment`.
pub fn reserve_aligned_on_node(usable: usize, node: u32)
    -> Option<(NonNull<u8>, NonNull<u8>, usize)> { ... }
```

All `unsafe` blocks have a `// SAFETY:` comment. Full analogy with `os.rs`.

### `src/alloc_core/segment_header.rs` — new field

```rust
#[repr(C)]
pub(crate) struct SegmentHeader {
    // ... existing fields ...
    pub live_count: u32,
    pub decommitted: u32,
    /// NUMA node on which the pages of this segment were allocated.
    /// `NO_NODE` (u32::MAX) means "unknown / not used".
    /// Present in EVERY build (layout is stable); read/used
    /// only under `#[cfg(feature = "numa-aware")]`.
    pub node_id: u32,
}
```

Similarly to `live_count`/`decommitted` — the field is always present so that
the header layout is stable regardless of the feature set. Access is via
`offset_of!` following the same discipline as `bump_of`/`set_bump`.

**Size check**: adding a `u32` will not violate the assert `size_of::<SegmentHeader>() <= PAGE` — the current size is ~96 bytes, it will remain ~100 bytes, much less than 4096.

### `src/alloc_core/alloc_core.rs` — changing `reserve_small_segment`

```rust
fn reserve_small_segment(&mut self) -> Option<*mut u8> {
    // New code under numa-aware:
    #[cfg(feature = "numa-aware")]
    let my_node = numa::current_node();

    // First check: is there already an empty (decommitted) segment
    // on the desired node? (reuse instead of new reservation)

    #[cfg(feature = "numa-aware")]
    let segment = numa::reserve_aligned_on_node(SEGMENT, my_node)?;
    #[cfg(not(feature = "numa-aware"))]
    let segment = Segment::reserve(SEGMENT)?;

    // ... rest of the code unchanged ...

    // Write node_id to the header:
    #[cfg(feature = "numa-aware")]
    {
        let off = core::mem::offset_of!(SegmentHeader, node_id);
        Node::write_u32(Node::offset(base, off) as *mut u32, my_node);
    }
    // ...
}
```

### NUMA preference in `find_segment_with_free`

```rust
pub(crate) fn find_segment_with_free(&self, class_idx: usize) -> Option<*mut u8> {
    #[cfg(feature = "numa-aware")]
    let my_node = numa::current_node();

    let mut fallback = None;
    for base in self.table.bases() {
        if !matches!(SegmentHeader::kind_at(base), ...) { continue; }
        // ...ring drain...

        #[cfg(feature = "numa-aware")]
        {
            let seg_node = segment_node_id(base);
            if seg_node != my_node && seg_node != numa::NO_NODE {
                // Not our node — remember as fallback, continue searching
                if fallback.is_none() { /* store */ }
                continue;
            }
        }
        let bt = SegmentMeta::new(base).bin_table();
        if bt.head(class_idx) != FREE_LIST_NULL {
            return Some(base);  // local node — take immediately
        }
    }
    // fallback: segment from another node, if no local one found
    fallback
}
```

---

## §3. Per-node segment pools

**Are separate BinTable / free-lists per node needed?** — No, for MVP.

Rationale:
- `Heap` is already per-thread. In a typical DBMS workload, the thread lives on
  a single core and a single node (especially with the `pinning` feature, which already exists).
- Segments are already per-heap (not shared between threads in steady-state, only
  via cross-thread free).
- Splitting BinTable by node would mean doubling/tripling metadata
  inside each segment and complicating `find_segment_with_free` — with no benefit
  when `pin` is used.

**Sufficient**: a `node_id` tag in the header + preference for local segments
in `find_segment_with_free` + allocating new segments on `my_node`.

If workload shows that cross-node segments dominate (for example, during
heap-balancing or adoption), — then consider splitting, BUT that is phase N+1.

---

## §4. Thread migration

**Problem**: if the OS migrates a thread to another NUMA node, `current_node()`
returns the new node, but all existing segments of that heap retain the old
`node_id`. Allocations will still be served from "remote" segments.

**Strategies:**

| Variant | Description | Complexity | Choice |
|---------|-------------|------------|--------|
| (a) Ignore | Thread works with old segments; `current_node()` affects only NEW reservations | Minimal | **MVP** |
| (b) Periodic re-subscription | On every `alloc` check `current_node()` vs `segment node_id`; on mismatch — migrate | High, no benefit | No |
| (c) User-side pinning | Pin thread via the `pinning` feature (`core_affinity`) — then migration does not occur | Minimal (already exists) | **Recommendation** |

**MVP decision**: strategy (a) + documentation recommending use of the
`pinning` feature. This is honest: NUMA benefit manifests precisely where the thread
is pinned to a core on the node. Without pinning, NUMA is a best-effort optimization, not a guarantee.

**Synergy with `pinning`**: the `pinning` feature already pulls in `core_affinity`
(a safe wrapper over `sched_setaffinity`/`SetThreadAffinityMask`).
Document: `numa-aware + pinning` is the recommended combination.
`numa-aware` alone without `pinning` gives best-effort (helps under low
migration, does not help under high migration).

### §4.1 `current_node()` cache (R11-5)

**Problem being addressed.** Strategy (a) above calls `numa::current_node()`
on every `find_segment_with_free` invocation (i.e. on every freelist miss),
plus at every new-segment reservation. The shim's `current_node()` is NOT
cheap — on Linux it loops over up to 64 NUMA nodes, opening and reading a
sysfs `cpumap` file *for each one* (`/sys/devices/system/node/nodeN/cpumap`),
and on Windows it issues two Win32 API calls
(`GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`). Per-call cost is
therefore real kernel transitions (Windows) or potentially dozens of
`open`/`read`/`close` syscalls (Linux worst case) — paid on every freelist
miss, not once per process. This is the cheap NUMA optimisation to do
*before* the bigger node-indexed directory work (R11-6, a separate task).

**What is cached.** The `u32` returned by `numa::current_node()` for the
calling thread. Stored as a per-`AllocCore`-instance field
`cached_numa_node: Option<u32>` (`src/alloc_core/alloc_core.rs`), gated on
`#[cfg(feature = "numa-aware")]`. `None` = "not yet queried on this claim";
`Some(n)` = "the last query returned `n`".

**Why per-`AllocCore`-instance, not thread-local.** `AllocCore` is already
single-writer per thread-owned registry slot (the owning thread is its sole
mutator), so a plain `Option<u32>` field — no `Cell`, no `AtomicU32`, no
interior mutability machinery — is the natural fit: it inherits the same
single-writer invariant `AllocCore` already relies on for `small_cur`,
`large_cache`, etc. A thread-local cache would be a second source of truth
that has to be kept in sync with slot recycling indirectly (a recycled
slot's stale TLV would survive until that thread next touched it), whereas a
field on `AllocCore` invalidates *with* the slot, automatically, at the
single known recycle boundary. The field costs 8 bytes (`Option<u32>` with
padding) per live heap.

**Population policy.** Lazy on first use. The cached accessor
`AllocCore::current_node_cached(&mut self) -> u32` returns the cached value
if `Some`, otherwise queries `numa::current_node()`, stores the result, and
returns it. Every former call site of `numa::current_node()` on a hot path
now calls the cached accessor instead:

- `find_segment_with_free_impl` (`src/alloc_core/alloc_core_small.rs`) — the
  per-miss path, the call site this cache exists to defray.
- `reserve_small_segment` (same file) — new-segment small reservation.
- `alloc_large`'s cache-hit re-stamp and `alloc_large_slow`
  (`src/alloc_core/alloc_core_large.rs`) — the large path's two sites.

The one-time bootstrap call in `AllocCore::new_inner`
(`src/alloc_core/alloc_core.rs`, ~line 726) is **left as a direct
`numa::current_node()` call** — it runs exactly once per
`AllocCore::new_inner()`, never in a hot loop, so routing it through the
cache would add a field write for zero amortisation benefit. Forcing it
through the cache mechanically would be cargo-cult uniformity, not a
judgement call.

**Invalidation policy — the slot-recycle correctness point.** Registry
slots are recycled across different OS threads
(`HeapRegistry::claim`/`HeapRegistry::recycle`,
`src/registry/heap_registry.rs`). A "cache once, never invalidate" approach
is **wrong**: thread A claims a slot, populates the cache with its NUMA
node; the slot is later recycled; thread B (a different physical thread,
likely on a different core/NUMA node) claims the SAME slot — a stale cache
would silently apply thread A's NUMA node to thread B's allocations for the
entire lifetime of B's claim, defeating the entire purpose of the cache.

The cache is therefore invalidated at `claim()` / `claim_with_config()`
time: immediately before returning the freshly-claimed `*mut HeapCore` to
the caller, the registry calls `HeapCore::invalidate_numa_node_cache()`,
which delegates to `AllocCore::invalidate_numa_node_cache()` and resets
`cached_numa_node = None`. Soundness rests on the same single-writer
discipline the registry already enforces:

- The caller of `claim` is the CAS winner of the `FREE → LIVE` transition
  (AcqRel on success, Acquire on failure). At the point of invalidation the
  slot is `LIVE` and the previous owner has fully quiesced (its `recycle`
  did a Release `LIVE → FREE` CAS; this claim's Acquire-side of the
  subsequent `FREE → LIVE` CAS sees all of the previous owner's writes,
  including the stale cache value).
- The new owner is the sole writer from this point until its own
  `recycle`. The invalidating write to `cached_numa_node` is therefore
  race-free.
- First materialisation (a slot that has never been claimed before) starts
  with `cached_numa_node = None` from `AllocCore::new_inner`, so the
  invalidation is a no-op there; the call is made uniformly so the recycle
  boundary is the single source of truth, not the materialisation branch.

Standalone `AllocCore`s built directly by tests via `AllocCore::new()`
(no registry slot, no `claim`/`recycle`) populate the cache on first use
and stay populated for the `AllocCore`'s lifetime — correct, because such
an `AllocCore` is single-threaded by construction (`AllocCore: !Sync`) and
never crosses a thread boundary, so there is no recycle boundary to
invalidate at.

**Resulting staleness bound.** Combining §4 Strategy (a) (existing) with
this cache (new): a thread's `current_node()` *reads* may lag the OS's real
answer for **the duration of the current slot claim** — from `claim()` to
`recycle()`, whichever comes first. Within a single claim, every cached
access returns the same node the first miss of that claim observed. This is
a strict, small extension of the §4 Strategy (a) "ignore migration"
trade-off: the *previous* trade-off was "existing segments keep their old
`node_id` if the OS migrates the thread mid-life" (a per-segment lag); the
*new* part is "the query itself now also lags real migration by up to one
claim's worth of allocations". Both sources of staleness are bounded by the
next `recycle()` (which re-queries on the next claim); both are
*performance* staleness only (a wrong `node_id` never causes UB, just
suboptimal locality); and the recommended mitigation for both is identical
— use `numa-aware + pinning` so migration does not occur in the first
place. A `numa-aware`-only workload that experiences mid-claim migration
will see the cached node lag by up to one claim's allocations before
self-correcting on the next claim; that is the accepted bound, written
down here rather than left implicit.

**Test coverage.** `tests/numa_cache_invalidation.rs` (gated on the
`numa-aware-mock` feature, which enables `numa-shim/mock` for deterministic
control of `current_node()`'s return value) proves the invalidation fires
at `claim()`: it scripts node A for the first claim, exercises the cached
path, recycles, scripts node B, re-claims, and asserts the newly-claimed
slot's cached value is B (not the stale A) — the exact bug this subsection
exists to prevent.

**Measured (`benches/numa_current_node_cache.rs`, this host: Windows,
single-NUMA — `current_node_impl` cost here is two Win32 API calls,
`GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`; the Linux
sysfs-loop cost, potentially dozens of `open`/`read`/`close` syscalls per
call, is qualitatively larger but not directly measurable on this host —
no number is claimed for it):

| Benchmark | Uncached `numa::current_node()` | Cached accessor | Speedup |
|---|---|---|---|
| Per-call | ~230 ns | ~985 ps | ~233x |
| Batch of 1024 (realistic claim lifetime) | ~227 µs | ~573 ns | ~396x |

### Bounded mid-claim refresh (R12-5)

**Problem being addressed.** §4.1's staleness bound above ("a thread's
`current_node()` reads may lag the OS's real answer for the duration of the
current slot claim — `claim()` to `recycle()`") is *unbounded in wall-clock
time* for a long-lived claim. A `HeapCore` held by a single thread for
minutes or hours (a long-running worker thread that is never re-pinned and
that the OS scheduler eventually migrates to a different NUMA node —
`sched_setaffinity`/scheduler load-balancing on Linux, or the analogous
Windows scheduler behaviour, acting on a thread that never called
`SetThreadAffinityMask`) would, pre-R12-5, keep steering every new segment
reservation toward the STALE pre-migration node for the rest of that claim's
lifetime — silently undoing R11-6's directory-locality win, since the
directory's node-bucket preference is driven by this same cached value. This
is not a memory-safety defect (a wrong `node_id` never causes UB, only
suboptimal locality — same characterisation as the base staleness bound
above), but an unbounded-duration one is a real regression risk for anything
that is not both `numa-aware` AND `pinning`.

**Fix — a bounded per-claim refresh counter.** `AllocCore` gains a second
field, `numa_node_hits_since_refresh: u32`, counting cache hits served by
`current_node_cached()` since the value was last populated by a real
`numa::current_node()` query. `current_node_cached()` now returns the cached
value only while this counter is below
`AllocCore::NUMA_NODE_REFRESH_PERIOD` (`128`); once the budget is exhausted,
the very next call forces a real re-query — exactly as if the cache had been
`None` — and resets the counter to `0`. `invalidate_numa_node_cache()`
(the existing R11-5 claim-boundary invalidation) also resets the counter, so
the two fields never observably disagree (a fresh claim always starts a
fresh 128-call budget).

**Why the trigger is "N calls to the cached accessor", not "N allocations"
or a wall-clock timer.** Every call site of `current_node_cached()` is
already a refill-miss or new-segment-reservation path —
`find_segment_with_free_impl`, `reserve_small_segment`
(`src/alloc_core/alloc_core_small.rs`), and `alloc_large`/`alloc_large_slow`
(`src/alloc_core/alloc_core_large.rs`) — never the bump-pointer
alloc/dealloc fast path (that path never touches NUMA state at all). Each of
these call sites is already paying for a free-list linear scan or an actual
OS segment reservation (a real mmap/VirtualAlloc round-trip, page-table
work), so charging one extra `numa::current_node()` call there every 128
times is negligible relative to the work already being done at that call
site — unlike gating on a wall-clock timer (which would need an
`Instant::now()` read, itself non-trivial, on a path some builds want to
stay branch-cheap) or a raw allocation counter (which WOULD run in the hot
bump-pointer path and reintroduce per-op overhead R11-5 specifically
eliminated).

**Why 128.** Sits in the middle of the 64–256 per-class range this task's
review suggested, and deliberately matches the order of magnitude of
`DIRECTORY_MISS_FULL_SCAN_PERIOD` (`src/alloc_core/segment_directory.rs`,
`= 64`) — the sibling "periodic re-validation" cadence the directory-miss
trust window already established as "rare enough to be free, frequent
enough to bound drift" for a structurally similar problem (bounding how
long a trusted-but-possibly-stale signal can go unchecked). Reusing that
order of magnitude is a judgement call, not a re-derivation from first
principles: this task's `benches/numa_current_node_cache.rs` per-call
cache-hit cost on this host went from ~1.1 ns (pure `Option` load) to ~3.6
ns (load + compare + increment) after adding the counter check — a ~2.5 ns
increase that is still ~70x cheaper than the ~250 ns real syscall/API-call
cost the cache exists to defray, and (per the paragraph above) is paid only
at refill-miss/reservation call sites, never in the bump-pointer fast path.
A future tuning pass could sweep the exact constant against a live
migration-heavy multi-socket workload; 128 is the honest "reasonable
default in the suggested range, justified by the existing sibling
constant's precedent" choice made without such a workload available on this
development host.

**What is NOT changed.** The directory full-rescan / periodic-revalidation
path (`find_segment_with_free_impl`'s `periodic_revalidation_active` branch,
gated on `alloc-segment-directory`) was considered as an ADDITIONAL, free
invalidation point (it already runs an O(S) linear scan every
`DIRECTORY_MISS_FULL_SCAN_PERIOD` misses, so one more `current_node()` call
there would be unmeasurable) but was NOT wired up: that path is
`#[cfg(not(feature = "numa-aware"))]`-adjacent in its directory-driven form
(the R11-6 comment at `alloc_core_small.rs` notes the directory-driven
lookup itself is disabled under `numa-aware`, with the linear-scan two-pass
fallback handling NUMA preference instead), so the periodic-revalidation
counter that would need to trigger the extra refresh is not on the
`numa-aware` hot path in the first place. The per-call counter in
`current_node_cached()` already covers every `numa-aware` call site
directly and uniformly; adding a second trigger through the directory path
would be redundant complexity without a call site that needs it.

**Resulting staleness bound (supersedes the plain §4.1 statement above).**
A thread's `current_node()` reads now lag the OS's real answer for **at
most `min(claim lifetime, NUMA_NODE_REFRESH_PERIOD refill-misses)`** — the
existing claim-boundary invalidation still applies (a `recycle()`/`claim()`
cycle always re-queries immediately), and now additionally, within a single
long-lived claim, staleness cannot exceed 128 refill-misses regardless of
how long the claim itself lives. The per-segment lag from §4 Strategy (a)
("existing segments keep their old `node_id` if the OS migrates the thread
mid-life") is unchanged by this fix — only the *query* staleness is now
bounded, not the existing segments' stamped `node_id`s, which is the same
distinction the plain §4.1 bound already drew.

**Test coverage.** `tests/numa_periodic_refresh.rs` (gated on
`numa-aware-mock`) scripts the mock to node A, populates the cache, then
scripts the mock to node B WITHOUT going through `claim`/`recycle`
(simulating an OS-level migration the registry never observes), drives
`NUMA_NODE_REFRESH_PERIOD + 1` more calls through the cached accessor, and
asserts the cache has caught up to node B. A companion assertion
immediately after the simulated migration (before the refresh budget is
exhausted) checks the cache STILL reports the stale node A — proving the
mechanism is genuinely caching, not vacuously always re-querying, which is
the premise the refresh-bound assertion depends on. Verified as a genuine
red-before/green-after regression: temporarily reverting
`current_node_cached()` to its pre-R12-5 form (dropping the refresh-budget
branch) makes this test fail with `left: Some(1), right: Some(9)` — the
cache stuck at the pre-migration node exactly as the unbounded-staleness
problem predicts.

---

## §5. Testing without real hardware

### QEMU fake-NUMA (Linux)

Run a Linux VM with a fake NUMA topology:

```sh
qemu-system-x86_64 \
  -m 2G \
  -smp 4,sockets=2,cores=2,threads=1 \
  -numa node,nodeid=0,cpus=0-1,mem=1G \
  -numa node,nodeid=1,cpus=2-3,mem=1G \
  -numa dist,src=0,dst=1,val=20 \
  ...
```

Inside the VM:
- `numactl --hardware` shows 2 nodes.
- `numactl --cpunodebind=0 ./sefer_test` — run the test on node 0.
- Verify that our code requests node 0 for thread 0, node 1 for thread 1.
- `/proc/<pid>/maps` + `numastat -m` — verify where pages physically came from.

Alternative without QEMU — kernel boot parameter `numa=fake=4` (4 virtual
NUMA nodes on a single physical socket). Does not require a VM.

### Test `tests/numa_seam.rs`

Unit test for `src/alloc_core/numa.rs`:

```rust
#[test]
#[cfg(feature = "numa-aware")]
fn current_node_returns_valid_value() {
    let node = numa::current_node();
    // Either NO_NODE (unsupported) or < 64 (reasonable bound)
    assert!(node == numa::NO_NODE || node < 64);
}

#[test]
#[cfg(all(feature = "numa-aware", target_os = "linux"))]
fn bind_segment_does_not_panic() {
    // Reserve a segment, bind to node 0, free.
    // Verifies that mbind does not fail (EINVAL etc.)
    ...
}
```

### Test `tests/numa_alloc.rs`

Integration test under `alloc-global + alloc-xthread + numa-aware`:

```rust
#[test]
#[cfg(all(feature = "numa-aware", feature = "alloc-global"))]
fn alloc_from_local_node() {
    // Launch 2 threads pinned to different NUMA nodes.
    // Each allocates N blocks.
    // Verify: segment.node_id == thread_numa_node for the majority of segments.
    ...
}
```

### IMPORTANT — honesty about limitations

QEMU / `numa=fake` verify CORRECTNESS of binding (the right `mbind`/
`VirtualAllocExNuma` call is made, `node_id` is recorded correctly). They DO NOT verify
the latency benefit: on a single physical socket all "nodes" have the same
access latency.

**A performance improvement figure requires real 2-socket hardware:**
- AWS c5n.metal, i3.metal (Xeon, 2 sockets)
- AWS r6g.metal (Graviton 2, multiple NUMA domains)
- Dual-socket dev box

This is an MVP limitation — record it in ALLOC_BENCH and as part of implementation
phase E.

---

## §6. Risk and scope

### Safety

New confined-`unsafe` block `src/alloc_core/numa.rs`:
- `mbind` syscall: does not modify segment data, only the physical page
  allocation policy. Primary risk: passing an incorrect `addr`/`len` or node.
  Protection: call ONLY on a live segment immediately after `mmap`, before any
  use; `len` comes from `Segment::len()` (a multiple of `SEGMENT`);
  `node` comes from `current_node()`, which is bounded by the system `node_count`.
- `VirtualAllocExNuma`: same semantics as `VirtualAlloc`, plus a node parameter.
  If the node is unavailable — returns NULL (OOM path, handled normally).
- `// SAFETY:` on every `unsafe` block — mandatory.

### Regression

- Feature default OFF (`numa-aware` without `= default`).
- Without the flag: byte-for-byte old behavior. `Segment::reserve` is unchanged.
- New field `SegmentHeader::node_id` is initialized to `NO_NODE` in
  constructors `small()` and `large()` — layout is stable.
- Compile-time assert `size_of::<SegmentHeader>() <= PAGE` still
  holds.

### Compatibility with `alloc-decommit`

`decommit_empty_segment` resets `live_count`, `decommitted`, `bump`.
The `node_id` field is NOT reset — it reflects the physical binding of the segment,
which does not change on decommit/recommit. After recommit the segment returns
to the same node.

### Scope

| Artifact | Estimate |
|----------|----------|
| `src/alloc_core/numa.rs` | 250–400 lines |
| `src/alloc_core/segment_header.rs` | +8 lines (field + constructors) |
| `src/alloc_core/alloc_core.rs` | +30–50 lines (`#[cfg(feature)]` blocks) |
| `src/alloc_core/mod.rs` | +1 line (`pub(crate) mod numa;`) |
| `tests/numa_seam.rs` | ~60 lines |
| `tests/numa_alloc.rs` | ~120 lines |
| `Cargo.toml` | +6 lines |

---

## §7. Out of scope

- **Policy tuning** (MPOL_INTERLEAVE vs MPOL_BIND vs MPOL_PREFERRED): the MVP
  ships `MPOL_PREFERRED` (soft preference — the kernel falls back to any node on
  memory pressure; see `crates/numa/src/lib.rs`), NOT the harder `MPOL_BIND`.
  Interleave is for HPC workloads; whether to expose a stricter `MPOL_BIND` mode
  is to be decided based on measurement results.
- **NUMA-aware pinning runner**: synergy with the `pinning` feature (already has
  `core_affinity`); API extension for explicit binding of a thread to a NUMA node +
  shard — separate task.
- **Latency-asymmetry measurement**: impossible without real 2-socket hardware.
  Placeholder in ALLOC_BENCH: "NUMA: opt-in, verified under QEMU, latency
  requires hardware".
- **Per-node free-list sharding** inside a segment: not needed for MVP;
  consider if data shows high cross-node "pollution" during heap
  adoption.
- **Large-block NUMA**: `alloc_large` creates a dedicated segment; binding
  there is useful but less critical (large blocks are rarer). Include in the same
  phase A (a single `reserve_aligned_on_node` call).

---

## §8. Implementation steps

### Phase A — `src/alloc_core/numa.rs` (OS-seam)

New confined-`unsafe` module with topology detection and `bind_segment` /
`reserve_aligned_on_node`. Covered by unit tests in `tests/numa_seam.rs`.

Platforms:
- `#[cfg(all(target_os = "linux", not(miri)))]`: `mbind` + `sched_getcpu`
- `#[cfg(all(windows, not(miri)))]`: `VirtualAllocExNuma` +
  `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`
- `#[cfg(target_os = "macos")]` + `#[cfg(miri)]`: no-op, returns `NO_NODE`

### Phase B — `SegmentHeader.node_id` (layout)

New `u32` field with `NO_NODE` in both constructors. Compile-time assert that
`size_of::<SegmentHeader>() <= PAGE` still holds. Field-specific
accessor `node_id_of` / `set_node_id` via `offset_of!`.

### Phase C — NUMA selection in `reserve_small_segment` + `find_segment_with_free`

Insertion of `current_node()` and `reserve_aligned_on_node()` into
`reserve_small_segment`. NUMA preference in `find_segment_with_free` —
first segments with `node_id == my_node`, then the rest. Similarly
`alloc_large` for large allocations.

### Phase D — QEMU correctness test

`tests/numa_alloc.rs`: run only when `SEFER_NUMA_TEST=1` (guard env
var), since it requires a real NUMA topology or QEMU. Document in README
(`numactl --hardware` prerequisite).

### Phase E — ALLOC_BENCH update

Add a "NUMA" section with an honest description: opt-in under `numa-aware`,
correctness verified under QEMU/fake-NUMA, latency benefit is measurable
only on real multi-socket hardware. RSS metric is not affected
(NUMA binding does not change the number of segments allocated).

---

## Summary table of code points

| File | Action |
|------|--------|
| `src/alloc_core/numa.rs` | New confined-`unsafe` NUMA-seam |
| `src/alloc_core/os.rs` | No changes (read only) |
| `src/alloc_core/segment_header.rs` | + field `node_id: u32`, accessors |
| `src/alloc_core/alloc_core.rs` | + `#[cfg(numa-aware)]` in `reserve_small_segment`, `alloc_large`, `find_segment_with_free` |
| `src/alloc_core/mod.rs` | + `pub(crate) mod numa;` |
| `src/heap/heap.rs` | No changes (NUMA logic is below, in AllocCore) |
| `Cargo.toml` | + feature `numa-aware = ["alloc-core"]` |
| `tests/numa_seam.rs` | New OS-seam test |
| `tests/numa_alloc.rs` | New integration test |
| `docs/ALLOC_BENCH.md` | + "NUMA" section |
