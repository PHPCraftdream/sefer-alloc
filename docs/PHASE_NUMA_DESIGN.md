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
