# sefer-alloc

[![CI](https://github.com/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml/badge.svg)](https://github.com/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/sefer-alloc.svg)](https://crates.io/crates/sefer-alloc)
[![Documentation](https://docs.rs/sefer-alloc/badge.svg)](https://docs.rs/sefer-alloc)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV: 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://www.rust-lang.org/)

> A safe-by-construction, **100 % Rust** memory toolkit: a single-threaded
> handle store and a drop-in `#[global_allocator]` over **one verified segment
> substrate**, compiler-enforced unsafe-confinement, **no C / C++ libraries
> pulled in** (no `libnuma`, no `mimalloc`, no `jemalloc`, no `snmalloc` /
> `tcmalloc`) — and **up to ~18× faster than `mimalloc`** on cached large
> alloc/free after the OPT-E large-cache.

`sefer-alloc` ships two faces over one substrate:

- **`Region<T>` / `Handle<T>`** — a safe-by-construction handle store. Generational
  handles instead of pointers; a stale handle returns `None`, never UB. Default
  feature `std`; also builds `no_std` + `alloc`. The default build is
  `#![forbid(unsafe_code)]` at the top; the only `unsafe` comes from
  `slotmap`'s audited core, wrapped by a thin typed membrane.
- **`SeferMalloc`** — a drop-in `#[global_allocator]` over the same segment
  substrate (opt-in `production` feature). Under the recommended `production`
  feature the crate becomes `#![deny(unsafe_code)]` and every `unsafe` lives
  in **seven named confined seams** (`alloc_core::{os, node}` +
  `global::{sefer_malloc, tls_heap, fallback}` +
  `registry::{heap_slot, heap_registry}`) — never in the alloc-path body
  outside them. Every `unsafe` block carries a `// SAFETY:` proof. The
  compiler enforces the confinement; a stray `unsafe` outside a named seam
  is a hard error. The complete inventory by feature is in
  [Where unsafe lives](#where-unsafe-lives-the-complete-list) below.

The substrate is the same for both faces: SEGMENT-aligned (4 MiB) OS-backed
spans, self-hosted metadata (no `Vec`/`HashSet`/`std::alloc` on any alloc path),
per-thread heaps, non-intrusive cross-thread free through a per-segment MPSC
ring. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the 30-minute
end-to-end tour.

---

## Quick demo

### Single-threaded handle store

```rust
use sefer_alloc::Region;

let mut region = Region::new();
let a = region.insert("alpha");
let b = region.insert("beta");

assert_eq!(region.get(a), Some(&"alpha"));

region.remove(a);
assert_eq!(region.get(a), None);          // stale handle → None, never UB
assert_eq!(region.get(b), Some(&"beta")); // others stay valid
```

### Drop-in global allocator

```rust
use sefer_alloc::SeferMalloc;

#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

fn main() {
    let v: Vec<u8> = (0..1024).map(|i| i as u8).collect();
    let s = format!("vector of {} bytes", v.len());
    println!("{s}");
}
```

```toml
[dependencies]
sefer-alloc = { version = "0.1", features = ["production"] }
```

The `production` feature is shorthand for `alloc-global + alloc-xthread +
alloc-decommit` — the recommended set for any long-running multi-thread or
async workload (without `alloc-decommit` the `SegmentTable`'s slot-recycle is
off and the 1024-segment ceiling becomes a hard cap). See
[Features matrix](#features-matrix) below.

---

## Why bother

Two things, both rare in the same crate.

**Pure Rust, no C / C++ libraries pulled in.** Every comparable allocator in
the ecosystem wraps a C or C++ codebase: `mimalloc` (C++), `jemalloc`
(C, via `tikv-jemallocator`), `snmalloc` (C++), `tcmalloc` (C++). The most
common NUMA crates wrap `libnuma` (C). `sefer-alloc` is 100 % Rust — it
calls into the OS directly (`mmap` / `VirtualAlloc` / `mbind` etc. — the
same syscalls every allocator uses), but it does not link a single C or
C++ library. The only C dependency anywhere in this repository is the
optional `mimalloc` *dev*-dependency used as a baseline in benchmarks; it
is never on a consumer's runtime path. If a Rust-only build matrix matters
to you (cross-compilation, audit perimeter, supply-chain surface),
`sefer-alloc` is one of the few production-track choices.

**Safety claim is structural, not prose.** Most Rust allocators have
`unsafe` smeared across their hot paths and ask auditors to trust the
narrative. `sefer-alloc` makes the claim compiler-enforced: the default
build is `#![forbid(unsafe_code)]` at the top;
the moment any allocator feature (`experimental`, `byte`, `alloc-core` and
above) is on, the crate switches to `#![deny(unsafe_code)]` and the
confined seams lift it with `#![allow(unsafe_code)]` only inside named
files. The compiler enforces it — a stray `unsafe` outside a named seam is
a hard error in every configuration. The intelligence (placement, free
lists, page maps, segment registries, bin tables, alloc bitmaps, owner
stamping, recycle policy) lives in pure safe integer arithmetic; the hand
(OS aperture, intrusive free-list r/w, NUMA syscalls, the
`unsafe impl GlobalAlloc` trait obligation, the TLS-binding raw-pointer
handoff, the heap-slot table) is split across small audited files. The
complete inventory by feature is in
[Where unsafe lives](#where-unsafe-lives-the-complete-list) below.

The performance is honest (numbers from a single Windows dev host with
criterion `sample_size(10)` — see [Performance](#performance) for the
disclaimer):

- On **large alloc/free** (`alloc_large` / `dealloc_large`) sefer-alloc is
  **~16× faster than `mimalloc` on 4 MiB and ~18× faster on 16 MiB** after
  the OPT-E large-segment cache (4 MiB cycle: ~45 ns vs ~718 ns).
- On **MT cross-thread** (`malloc_macro` larson/mstress at T=4) it is
  competitive with `mimalloc`.
- On **realloc-grow under neighbour pressure** it improved **−28.6 %** with
  OPT-F in-place realloc.
- On **single-thread small-class churn** it is roughly 1.2–2× behind
  `mimalloc` — the remaining gap, called out honestly in
  [`docs/MALLOC_BENCH.md`](docs/MALLOC_BENCH.md).

The verification stack is also honest: 51 integration tests, 6 loom models,
proptest differential against a reference model, miri with strict-provenance,
ThreadSanitizer (×3 clean runs), Valgrind memcheck (clean), aarch64 (qemu),
libFuzzer, soak / RSS / tokio-burn-in harnesses. The
[Verification evidence](#verification-evidence) section spells out what each
one actually proves.

---

## Architecture & principles

### Two faces, one substrate

```
         ┌───────────────────┐         ┌────────────────────────┐
         │  Region<T>        │         │  SeferMalloc           │
         │  Handle<T>        │         │  #[global_allocator]   │
         │  (safe membrane)  │         │  (unsafe trait impl)   │
         └─────────┬─────────┘         └──────────┬─────────────┘
                   │                              │
                   ▼                              ▼
         ┌─────────────────────────────────────────────────────┐
         │  Heap (per-thread, opt-in alloc-xthread)            │
         │  HeapCore (registry + stamp + xthread routing)      │
         │  AllocCore (single-thread alloc/dealloc/realloc)    │
         │  SegmentTable + page_map + bin_table + alloc_bitmap │
         │  RemoteFreeRing (per-segment MPSC, non-intrusive)   │
         │                                                     │
         │  Hand (confined-unsafe seams):                      │
         │    os::      mmap/VirtualAlloc, decommit/recommit   │
         │    node::    intrusive free-list pointer r/w        │
         │    numa::    mbind / VirtualAllocExNuma (opt-in)    │
         └─────────────────────────────────────────────────────┘
```

The same OS-backed segments serve both faces. The handle store reaches in via
the safe Cartographer (slot tables + generation checks); the global allocator
reaches in via the same Cartographer plus the documented `unsafe impl
GlobalAlloc` aperture. The Hand is always the same three modules — there is
no second copy of `mmap` somewhere else in the crate.

### Three organs

| Organ | Responsibility | Safety |
|---|---|---|
| **Cartographer** | All placement / free-list / page-map / segment-registry / bin-table / alloc-bitmap / decommit-policy / NUMA-preference logic. Pure integer arithmetic over indices and offsets. Never touches raw memory. | safe |
| **Membrane** | The typed APIs (`Handle<T>`, `Region<T>`, `AllocCore::alloc`, `SeferMalloc::alloc`). Total — cannot express UB at the type level. | safe |
| **Hand** | The confined-`unsafe` seams that touch raw memory. Each is a single audited file; every `unsafe { ... }` block carries a `// SAFETY:` proof. | confined |

The deliberate inversion: all the intelligence lives in the safe Cartographer,
so the Hand stays mechanical and small. Verification is over a total Membrane
and an integer algorithm, not a tangle of pointer math.

### Where `unsafe` lives (the complete list)

Twelve confined-`unsafe` module files, each gated by a specific feature
and each carrying a `#![allow(unsafe_code)]` opt-out from the crate-level
`#![deny(unsafe_code)]`. Source-of-truth listing lives in
[`src/lib.rs`](src/lib.rs) and is verifiable with one command:
`grep -l 'allow(unsafe_code)' src/`.

| Module | What it owns | Loaded under |
|---|---|---|
| [`src/alloc_core/os.rs`](src/alloc_core/os.rs) | `mmap`/`munmap`/`madvise` on Unix, `VirtualAlloc`/`VirtualFree` on Windows, the over-reserve+trim for SEGMENT alignment, decommit/recommit | `alloc-core` |
| [`src/alloc_core/node.rs`](src/alloc_core/node.rs) | Intrusive free-list node r/w through raw pointers (the generalised "hand" discipline); also `release_segment` thin wrapper | `alloc-core` |
| [`src/alloc_core/numa.rs`](src/alloc_core/numa.rs) | NUMA syscalls: Linux `mbind(2)` via `syscall(2)`, Windows `VirtualAllocExNuma`, sched_getcpu, sysfs cpumap reader (macOS / miri no-op) | `numa-aware` |
| [`src/global/sefer_malloc.rs`](src/global/sefer_malloc.rs) | The `unsafe impl GlobalAlloc` malloc-face seam — the trait obligation + pointer handoff to the `Heap` | `alloc-global` |
| [`src/global/tls_heap.rs`](src/global/tls_heap.rs) | Raw-pointer TLS binding + `AbandonGuard` seam — the `*mut HeapCore` handoff under the single-writer invariant; `unsafe fn recycle` / `abandon_segments` from the guard's drop | `alloc-global` |
| [`src/global/fallback.rs`](src/global/fallback.rs) | The primordial fallback heap — `static mut MaybeUninit<HeapCore>` + atomic-init state-machine + spinlock-guarded `&mut` handout (so the global allocator survives reentrant / early-init / teardown access) | `alloc-global` |
| [`src/registry/heap_slot.rs`](src/registry/heap_slot.rs) | `Sync`/`Send` impls on `HeapSlot` under the atomic single-writer protocol; the slot's `UnsafeCell` hand-off | `alloc-global` |
| [`src/registry/heap_registry.rs`](src/registry/heap_registry.rs) | The global heap slot-table — the `*mut HeapCore` pointer handoff out of a slot, used by every cross-thread routing decision | `alloc-global` |
| [`src/concurrent/hand.rs`](src/concurrent/hand.rs) | The legacy epoch-tier `AtomicSlot<T>` (older experimental concurrent tier; superseded by `alloc-xthread` for the global allocator path) | `experimental` |
| [`src/byte/byte_region.rs`](src/byte/byte_region.rs) | Research-tier size-classed byte arena | `byte` |
| [`src/byte/byte_allocator.rs`](src/byte/byte_allocator.rs) | The Phase-4 experimental `unsafe impl GlobalAlloc` over `byte_region.rs` (superseded by `alloc-global` for production) | `byte` |
| [`src/byte/sharded_byte_arena.rs`](src/byte/sharded_byte_arena.rs) | N-way sharded byte arena for parallel raw allocation (research; superseded by `alloc-xthread`) | `byte-sharded` |

Under the recommended `production` feature
(`alloc-global + alloc-xthread + alloc-decommit`) the active seams are the
first eight rows — `alloc_core::{os, node}` plus `global::{sefer_malloc,
tls_heap, fallback}` plus `registry::{heap_slot, heap_registry}`.
`alloc-xthread` and `alloc-decommit` themselves do **not** open new
`unsafe` seams — they extend existing safe code paths.

`numa-aware` adds one more seam (`alloc_core::numa`). `experimental` and
`byte` / `byte-sharded` open the older research-tier seams; the production
build does not pull them in.

That's the full list. Everywhere else in the crate is forbidden / denied
`unsafe`; a stray `unsafe` outside these files is a hard compile error in
every configuration.

### The segment substrate (Phase 8)

Each segment is `SEGMENT = 4 MiB` of OS-backed, SEGMENT-aligned virtual
memory. The first metadata page hosts: a `SegmentHeader` (kind, magic, bump
cursor, owner state, NUMA node id, live-count); a `page_map` (one byte per
page, per-page descriptor); a `BinTable` (per-size-class free-list heads);
an `AllocBitmap` (1 bit per `MIN_BLOCK` slot, the O(1) double-free guard); a
`RemoteFreeRing` (the per-segment MPSC ring for cross-thread frees).

A self-hosted `SegmentTable` carved from the **primordial** segment indexes
every live segment by base pointer. It is **append-only with NULL-slot
recycle** under `alloc-decommit` (see [`docs/ARCHITECTURE.md §3`](docs/ARCHITECTURE.md))
and from `0.1.0` ships an **open-addressing hash side-index** for O(1)
`contains_base` at DBMS scale. There is no `Vec` / `HashSet` / `std::alloc`
on any alloc path — M5 reentrancy-freedom is upheld structurally.

### Per-thread heaps and the lock-free fast path

A thread allocates from its own `Heap`'s per-class `BinTable` via a single
pointer read; deallocates with a single pointer write through the `node`
seam. No lock, no atomic on the common case. Slow path: refill `REFILL_BATCH
= 31` blocks from the current segment (the constant is **measured** — see
commit `81fec54`, bigger refills hurt locality).

Cross-thread free (opt-in `alloc-xthread`) does **not** dereference the
block: the freer pushes `(offset | class)` into the segment's
`RemoteFreeRing` (whose memory lives in metadata pages that are never
decommitted), and the owner reclaims lazily on its alloc-slow-path. The
freer stamps the class because the `page_map` is unreliable for mixed-class
pages produced by a shared bump cursor — the §13 race investigation
([`docs/RACE_DRAIN_RECLAIM.md`](docs/RACE_DRAIN_RECLAIM.md)) traced this
through four iterations of "peeling" before identifying the true root.

### Decommit (Phase 35) and large-cache (OPT-E)

When a small segment's live-count drops to zero AND it is not the current
carve target, payload pages are returned to the OS (`madvise MADV_DONTNEED`
/ `VirtualFree MEM_DECOMMIT`); the segment is reset to a clean blank,
re-committed on first reuse. **No epoch reclamation (M11) is needed** —
the four-point safety argument is recorded in
[`docs/PHASE35_DECOMMIT_DESIGN.md §1`](docs/PHASE35_DECOMMIT_DESIGN.md):
Variant-2 cross-thread free dissolves the only reason epoch was ever
considered.

`OPT-E` adds a small fixed-slot cache (2 slots × ≤ 64 MiB) inside each
`AllocCore` that holds freed large-segment OS reservations and reuses them
on the next `alloc_large` of comparable size — **without** decommitting and
re-committing pages, so the hit path is a `register` + header rewrite
(~42 ns at 4 MiB instead of 254 µs).

### NUMA-aware path (opt-in `numa-aware`)

The same hot path stamps `SegmentHeader::node_id` to the current thread's
NUMA node when `numa-aware` is on, and `find_segment_with_free` prefers
local-node segments with foreign-node fallback. The OS syscalls live in
[`src/alloc_core/numa.rs`](src/alloc_core/numa.rs) (Linux `mbind` via
`syscall(2)`, no `libnuma` dependency; Windows `VirtualAllocExNuma`;
macOS / miri no-op). Honest caveat: a QEMU `-numa` topology verifies
correctness, not latency-asymmetry — that needs real 2-socket hardware
(AWS `*.metal`, Graviton, dual-socket dev box). See
[`docs/PHASE_NUMA_DESIGN.md`](docs/PHASE_NUMA_DESIGN.md).

---

## Performance

Numbers from the criterion benches on a single Windows dev host,
sefer-alloc 0.1.0 vs `mimalloc 0.1` vs `System`. Per
[CLAUDE.md](CLAUDE.md) the project's bench profile is the quick one —
`sample_size(10)`, short warm-up — so these are honest comparative
measurements, **not** a rigorous statistical benchmark suite. Treat the
multipliers as "order of magnitude correct" rather than exact. The
source-of-truth tables (and the longer commentary on what each bench
exercises) live in [`docs/MALLOC_BENCH.md`](docs/MALLOC_BENCH.md).
**Higher is better** for throughput rows, **lower is better** for latency
rows.

### Large alloc / free (`benches/large_realloc.rs`, headline)

| Workload | SeferMalloc | mimalloc | System | vs mimalloc |
|---|---|---|---|---|
| `alloc(4 MiB) + free` | **~45 ns** | ~718 ns | ~16.7 µs | **~16× faster** |
| `alloc(16 MiB) + free` | **~48 ns** | ~869 ns | ~17.6 µs | **~18× faster** |
| `alloc(64 MiB) + free` | ~2.0 ms | ~2.4 µs | ~19.9 µs | not cached |

The 64 MiB case is uncached by design (`MAX_CACHED_LARGE_BYTES = 64 MiB`)
so the cache cannot pin more than `2 × 64 MiB = 128 MiB` of unused RSS.
4 MiB and 16 MiB stay in the cache and pay only the `register`-plus-
header-rewrite cost (the cache-hit path is dominated by ~40 ns of header
fixup).

### Realloc grow under adversarial neighbour pressure

| Bench | SeferMalloc | mimalloc | Notes |
|---|---|---|---|
| `realloc_grow_geometric` | 173 µs | 368 µs | sefer-alloc 2.1× faster |
| `realloc_in_place_unfavorable` | **125 µs** | 1.31 ms | sefer-alloc 10.5× faster (OPT-F in-place realloc skip-copy) |

### MT cross-thread (`examples/malloc_macro.rs`, larson + mstress)

Aggregate ops/sec at T=4 worker threads:

| Workload | SeferMalloc | mimalloc | System |
|---|---|---|---|
| larson | 40 M | 32 M | ~6 M |
| mstress | ~65 M | ~65 M | ~6 M |

Competitive with `mimalloc` on the multi-thread cross-thread path; single-
thread small-class hot path is still ~1.2–2× behind `mimalloc` (see
[`docs/MALLOC_BENCH.md`](docs/MALLOC_BENCH.md) for the full table).

Reproduce with:

```bash
cargo bench --bench large_realloc --features "alloc-global alloc-decommit"
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"
```

### Honest verdict

- **Where sefer-alloc wins:** large alloc / free (OPT-E cache), realloc
  in-place small→small (OPT-F), DBMS-scale long-running multi-thread
  workloads (no SegmentTable ceiling under `alloc-decommit`).
- **Where it ties:** MT cross-thread macro-bench at typical T=4.
- **Where it loses:** single-thread small-class fixed-size churn — ~1.2–2×
  behind `mimalloc`. The flamegraph at
  [`docs/PROFILE_FLAMEGRAPHS.md §1`](docs/PROFILE_FLAMEGRAPHS.md) shows the
  remaining hot path; OPT-C lazy-stamp shaves ~1 % of it, the structural
  gap remains.

---

## Verification evidence

This is a verification-first build. Every claim above is backed by a tool,
a test file, and a reproducible command. **51 integration test files** ship
in `tests/` (45 conventional + 6 loom models — counted separately below);
**5 example binaries** in `examples/`; **10 benches** in `benches/`
(`byte_alloc`, `byte_sharded`, `global_alloc`, `heap_alloc`,
`heap_async_pattern`, `heap_xthread`, `large_realloc`, `locality`,
`pinned_write`, `sharded_write`); **2 libFuzzer targets** in `fuzz/`
(`region_ops`, `global_alloc_ops`).

| Tool | What it proves | Where in repo |
|---|---|---|
| Unit / integration tests | Construction, edge cases, end-to-end behaviour | `tests/*.rs` (51 files) |
| `proptest` differential | Op-stream agreement with a reference model (M1–M4) | `tests/alloc_core_differential.rs`, `tests/differential.rs` |
| `loom` | Cross-thread protocol agreement (Phase 12, Phase 10) | `tests/loom_xthread_protocol.rs`, `loom_remote_ring.rs`, `loom_thread_free.rs`, `loom_registry.rs`, `loom_sharded.rs`, `loom_epoch.rs` (6 models) |
| `miri` (strict-provenance) | UAF, races at byte level, double-free, exposed-provenance casts | CI gate: `region_invariants`, `decommit_miri_cycle`, `reclaim_offset_unit`, `byte` |
| ThreadSanitizer | Real cross-thread data races on a live binary | CI job + manual ×3 verified clean on `race_repro`, `race_norecycle`, `global_alloc_mt`, `heap_cross_thread`, `decommit_stale_ring`, `decommit_soak` |
| Valgrind `memcheck` | UAF, leaks, invalid reads at the process level | Manual: clean on all three cross-thread test binaries. Note: `helgrind` / `DRD` are inapplicable to lock-free atomic code (Valgrind doesn't model Rust atomics) — TSan is the right concurrency detector here. |
| aarch64 via `qemu-user` | Code-gen + relaxed-memory smoke on ARM | CI job + manual 13/13 tests pass. Honest caveat: TCG translation does not fully model ARM's weak-memory; real ARM hardware verification is a follow-up. |
| libFuzzer | Op-stream invariants under random input | `fuzz/fuzz_targets/region_ops.rs`, `global_alloc_ops.rs` |
| Soak harness | N-thread × hours stability | `examples/soak_xthread.rs` (32 / 64 / 128 workers) |
| tokio burn-in | Live `#[global_allocator]` under tokio multi-thread runtime | `examples/tokio_burn_in.rs` |
| RSS probe | Memory recovery under asymmetric cross-thread pressure | `examples/rss_probe.rs` |
| Macro-bench | MT throughput vs `mimalloc` and System | `examples/malloc_macro.rs` (larson + mstress) |
| Flamegraph profiling | Hot path identification per workload | `docs/PROFILE_FLAMEGRAPHS.md` (4 scenarios) |

Every CI job is wired (`.github/workflows/ci.yml`) and runs on every push:
test matrix on x86_64 + aarch64, six feature combinations, miri with
strict-provenance, ThreadSanitizer, libFuzzer build, clippy, rustfmt.

The full safety stack and the relationship between layers is documented in
[`docs/ARCHITECTURE.md §8`](docs/ARCHITECTURE.md) and
[`docs/INVARIANTS.md`](docs/INVARIANTS.md).

---

## Features matrix

| Feature | Pulls in | What it enables | Default | When to use |
|---|---|---|---|---|
| `std` | — | `SyncRegion`, all `std`-gated tiers | **on** | almost always |
| `alloc-core` | `std` | The segment substrate (`AllocCore`) | off | building on `AllocCore` directly |
| `alloc` | `alloc-core` | Per-thread `Heap` + intrusive free lists | off | single-thread allocator |
| `alloc-xthread` | `alloc` | Lock-free cross-thread free via `RemoteFreeRing` | off | multi-thread allocator |
| `alloc-global` | `alloc` | The `SeferMalloc` `#[global_allocator]` face | off | process-wide allocator |
| `alloc-decommit` | `alloc-core` | Return empty-segment payload pages to OS + `SegmentTable` slot-recycle | off | long-running / DBMS workloads |
| `numa-aware` | `alloc-core` | NUMA-node stamping + local-node preference (Linux `mbind`, Windows `VirtualAllocExNuma`) | off | multi-socket NUMA hardware |
| **`production`** | `alloc-global + alloc-xthread + alloc-decommit` | **The recommended combo for long-running multi-thread workloads.** | off | **DBMS, async runtimes, anything that allocates over hours.** |
| `experimental` | `std` + deps | Lock-free `LockFreeRegion` / `EpochRegion` / `ShardedRegion` (older tier) | off | RCU / epoch experiments |
| `pinning` | `experimental` + `core_affinity` | Thread-per-core pinning with `core_affinity` | off | `shard == core` workloads |
| `byte` | `std` | Research-tier byte arena (older, superseded by `alloc-core+`) | off | rarely; legacy |
| `byte-sharded` | `byte` | Sharded byte arena (research) | off | rarely; legacy |

`production` is the right starting point for almost any multi-thread or
async use of `SeferMalloc`. Without `alloc-decommit` the `SegmentTable`
slot-recycle is off and the 1024-segment ceiling is a hard cap — a tokio
server with hundreds of tasks will eventually OOM. For embedded / `no_std`
use, stay with the default `std` feature.

---

## Quick start

### Add to `Cargo.toml`

Application-level handle store (single-threaded core):

```toml
[dependencies]
sefer-alloc = "0.1"
```

`no_std` + `alloc`:

```toml
[dependencies]
sefer-alloc = { version = "0.1", default-features = false }
```

Drop-in global allocator for a long-running multi-thread workload:

```toml
[dependencies]
sefer-alloc = { version = "0.1", features = ["production"] }
```

### Run the examples

```bash
# Single-threaded handle store
cargo run --example global_allocator --features alloc-global

# Multi-thread macro-benchmark (larson + mstress, T=1/2/4)
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"

# Tokio async burn-in (256 tasks × 10 s)
cargo run --release --example tokio_burn_in --features "alloc-global alloc-xthread"

# Stability soak (default: avail_par threads × 5 s)
cargo run --release --example soak_xthread --features "alloc-global alloc-xthread"

# Production-style RSS probe
cargo run --release --example rss_probe --features "alloc-global alloc-xthread alloc-decommit"
```

---

## Documentation map

| Doc | What it covers |
|---|---|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 30-minute end-to-end technical tour |
| [`docs/INVARIANTS.md`](docs/INVARIANTS.md) | The I1–I6 (Region) and M1–M8 (Malloc) invariants |
| [`docs/DESIGN.md`](docs/DESIGN.md) | Cartographer / Membrane / Hand model for `Region<T>` |
| [`docs/MALLOC_PLAN.md`](docs/MALLOC_PLAN.md) | Detailed Phase 8+ allocator plan |
| [`docs/PHASE35_DECOMMIT_DESIGN.md`](docs/PHASE35_DECOMMIT_DESIGN.md) | M6 decommit + why no epoch reclamation is needed |
| [`docs/PHASE_NUMA_DESIGN.md`](docs/PHASE_NUMA_DESIGN.md) | NUMA-aware path design |
| [`docs/CROSS_THREAD_STATE_MACHINES.md`](docs/CROSS_THREAD_STATE_MACHINES.md) | The cross-thread-free state-machine spec |
| [`docs/RACE_DRAIN_RECLAIM.md`](docs/RACE_DRAIN_RECLAIM.md) | The §13 / §14 race investigation (the four "peelings") |
| [`docs/MALLOC_BENCH.md`](docs/MALLOC_BENCH.md) | Full benchmark results, OPT-E numbers, honest verdicts |
| [`docs/PROFILE_FLAMEGRAPHS.md`](docs/PROFILE_FLAMEGRAPHS.md) | Flamegraph profiling report (4 scenarios, 6 optimisation candidates) |
| [`docs/HEAP_BENCH.md`](docs/HEAP_BENCH.md), [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md), [`docs/BYTE_BENCH.md`](docs/BYTE_BENCH.md), [`docs/BYTE_SHARDED_BENCH.md`](docs/BYTE_SHARDED_BENCH.md) | Per-tier bench writeups |
| [`docs/PLAN.md`](docs/PLAN.md), [`docs/MALLOC_PLAN_PHASE12-13.md`](docs/MALLOC_PLAN_PHASE12-13.md) | Phase plans, dependency DAGs, risk registers |

---

## Honest limitations

- **Single-thread small-class hot path is ~1.2–2× behind `mimalloc`.** The
  flamegraph at [`docs/PROFILE_FLAMEGRAPHS.md §1`](docs/PROFILE_FLAMEGRAPHS.md)
  shows where; OPT-C lazy stamp recovers ~1 %, the structural gap remains.
- **NUMA latency-speedup is not benchmarked on real hardware.** QEMU
  `-numa` verifies correctness, not asymmetry. Real measurement needs a
  2-socket dev box / cloud `.metal` instance — flagged in
  [`docs/PHASE_NUMA_DESIGN.md`](docs/PHASE_NUMA_DESIGN.md).
- **ARM weak-memory is partial coverage.** aarch64 13/13 under `qemu-user`
  proves code-gen + most race-conditions; TCG does not fully model ARM's
  weak memory. Verification on real ARM hardware (Graviton / Apple Silicon
  / Raspberry Pi) is a follow-up.
- **Valgrind helgrind / DRD are inapplicable.** Both report thousands of
  false positives on legitimate lock-free atomic load/store pairs (Valgrind
  does not model Rust atomics). `ThreadSanitizer` is the right concurrency
  detector for this codebase. Valgrind `memcheck` is run and clean.
- **`large_alloc_free/64 MiB` is uncached by design.** Cap at
  `MAX_CACHED_LARGE_BYTES = 64 MiB` bounds the cache RSS to
  `LARGE_CACHE_SLOTS × MAX = 128 MiB`. Workloads with sustained > 64 MiB
  large allocations will not see the OPT-E speedup.
- **`alloc-decommit` is opt-in.** Without it, slot-recycle is off and the
  1024-segment cap is a hard ceiling for cumulative segment registrations.
  Use the `production` feature alias to avoid this.

---

## MSRV

**1.88.** The single-threaded core is plain safe Rust and will build on
much older toolchains; we pin a known-good floor from day one. MSRV bumps
are minor releases.

---

## Contributing

PRs welcome — please read [`CONTRIBUTING.md`](CONTRIBUTING.md) first. The
short version: this is a verification-first project, so a PR is expected
to come with tests + run the right verification layer for what it changes
(`cargo test --features production` minimum; miri / loom / TSan for
cross-thread; `// SAFETY:` for any new `unsafe`).

The codebase conventions are documented in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and `CLAUDE.md` (one
export per file; `mod.rs` only re-exports; tests live in `tests/` not
inline; `unsafe` only in named seams). The compiler enforces the unsafe
discipline; the rest is convention.

---

## Security

Memory-safety bugs, soundness holes, and `unsafe`-contract violations
qualify as security issues. **Please do not open public issues for these.**
Use GitHub Security Advisories (private) or email the maintainer per
[`SECURITY.md`](SECURITY.md). Acknowledgement within 72 hours; coordinated
disclosure standard.

---

## Code of Conduct

This project adopts the [Contributor Covenant 2.1](CODE_OF_CONDUCT.md).

---

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE),
at your option. Contributions are accepted under the same terms (per
`CONTRIBUTING.md`).

[`Handle`]: https://docs.rs/sefer-alloc
[`slotmap`]: https://crates.io/crates/slotmap
