# sefer-alloc

[![CI](https://github.com/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml/badge.svg)](https://github.com/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/sefer-alloc.svg)](https://crates.io/crates/sefer-alloc)
[![Documentation](https://docs.rs/sefer-alloc/badge.svg)](https://docs.rs/sefer-alloc)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV: 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://www.rust-lang.org/)
[![100% Rust](https://img.shields.io/badge/100%25%20Rust-no%20C%2FC%2B%2B%20deps-orange.svg)](#why-bother)
[![unsafe: confined](https://img.shields.io/badge/unsafe-confined%20to%20named%20seams-yellow.svg)](#where-unsafe-lives-the-complete-list)

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
the moment any allocator feature (`experimental`, `alloc-core` and
above) is on, the crate switches to `#![deny(unsafe_code)]` and the
confined seams lift it with `#![allow(unsafe_code)]` only inside named
files. The compiler enforces it — a stray `unsafe` outside a named seam is
a hard error in every configuration. The intelligence (placement, free
lists, page maps, segment registries, bin tables, alloc bitmaps, owner
stamping, recycle policy) lives in pure safe integer arithmetic; the hand
(OS aperture, intrusive free-list r/w, NUMA syscalls, the
`unsafe impl GlobalAlloc` trait obligation, the TLS-binding raw-pointer
handoff, the heap-slot table) is split across small audited files.

The workspace extraction **improved the audit story** further: the two
OS-unsafe sub-problems (virtual-memory aperture and NUMA syscalls) are now
independently-publishable crates (`aligned-vmem` and `numa-shim`), each with
a single responsibility, a small line count, and their own `cargo test`. An
auditor who wants to verify the OS-memory unsafe can read those two crates
in isolation — they do not have to navigate the full allocator codebase.

The complete inventory by feature is in
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

### Workspace: four independently-publishable companion crates

The workspace extracted four building blocks. Each is a real crates.io crate
someone can `cargo add` on its own — they are not internal implementation
details but independently useful libraries:

```
sefer-alloc
 ├── sefer-region    (crates/region)       — typed handle store (Handle<T>/Region<T>)
 ├── aligned-vmem    (crates/vmem)         — OS virtual-memory aperture  (feature: alloc-core)
 ├── numa-shim       (crates/numa)         — NUMA detection + binding    (feature: numa-aware)
 └── malloc-bench-rs (crates/malloc-bench) — portable GlobalAlloc bench harness (standalone)
```

`malloc-bench-rs` is not in sefer-alloc's runtime dependency tree — it exists
for anyone who wants to benchmark their own `GlobalAlloc` implementation.

Per-crate status:

| crate | crates.io | docs.rs |
|---|---|---|
| `sefer-region` | [![Crates.io](https://img.shields.io/crates/v/sefer-region.svg)](https://crates.io/crates/sefer-region) | [![Documentation](https://docs.rs/sefer-region/badge.svg)](https://docs.rs/sefer-region) |
| `aligned-vmem` | [![Crates.io](https://img.shields.io/crates/v/aligned-vmem.svg)](https://crates.io/crates/aligned-vmem) | [![Documentation](https://docs.rs/aligned-vmem/badge.svg)](https://docs.rs/aligned-vmem) |
| `numa-shim` | [![Crates.io](https://img.shields.io/crates/v/numa-shim.svg)](https://crates.io/crates/numa-shim) | [![Documentation](https://docs.rs/numa-shim/badge.svg)](https://docs.rs/numa-shim) |
| `malloc-bench-rs` | [![Crates.io](https://img.shields.io/crates/v/malloc-bench-rs.svg)](https://crates.io/crates/malloc-bench-rs) | [![Documentation](https://docs.rs/malloc-bench-rs/badge.svg)](https://docs.rs/malloc-bench-rs) |

### Where `unsafe` lives (the complete list)

The extraction **improved the audit story**, not just reorganised code.
An auditor who wants to verify the OS-memory unsafe no longer has to read
through a large general-purpose allocator crate — they can audit `aligned-vmem`
(~400 lines, sole purpose: OS aperture) and `numa-shim` (~300 lines, sole
purpose: NUMA syscalls) in complete isolation. Each has one responsibility,
one reason to have `unsafe`, and its own `cargo test`.

Source of truth: `grep -rln 'allow(unsafe_code)' src/ crates/`

**External publishable crates (each independently auditable):**

| Crate | Path | Unsafe story |
|---|---|---|
| `aligned-vmem` | `crates/vmem/` | `#![allow(unsafe_code)]` — entire crate IS the OS aperture (`mmap`/`VirtualAlloc`/decommit); single responsibility, small, audit in isolation |
| `numa-shim` | `crates/numa/` | `#![allow(unsafe_code)]` — entire crate IS the NUMA syscall shim (`mbind`/`VirtualAllocExNuma`); single responsibility, small, audit in isolation |
| `malloc-bench-rs` | `crates/malloc-bench/` | `#![allow(unsafe_code)]` — confined to `alloc_block`/`free_block`/`drain_mailbox` helpers; every block carries `// SAFETY:` |
| `sefer-region` | `crates/region/` | `#![forbid(unsafe_code)]` — zero own `unsafe`; `slotmap`'s audited core owns the generational layout |

**Internal sefer-alloc seams** (compiler-enforced — a stray `unsafe` outside
these named files is a hard compile error in every configuration):

| Module | What it owns | Loaded under |
|---|---|---|
| [`src/alloc_core/os.rs`](src/alloc_core/os.rs) | Thin interop wrapper around `aligned-vmem`; delegates SEGMENT-aligned reservation and decommit/recommit | `alloc-core` |
| [`src/alloc_core/node.rs`](src/alloc_core/node.rs) | Intrusive free-list node r/w through raw pointers (the generalised "hand" discipline); also `release_segment` thin wrapper | `alloc-core` |
| [`src/alloc_core/numa.rs`](src/alloc_core/numa.rs) | Thin interop wrapper around `numa-shim`; delegates NUMA-node query and segment binding | `numa-aware` |
| [`src/global/sefer_malloc.rs`](src/global/sefer_malloc.rs) | The `unsafe impl GlobalAlloc` malloc-face seam — the trait obligation + pointer handoff to the `Heap` | `alloc-global` |
| [`src/global/tls_heap.rs`](src/global/tls_heap.rs) | Raw-pointer TLS binding + `AbandonGuard` seam — the `*mut HeapCore` handoff under the single-writer invariant; `unsafe fn recycle` / `abandon_segments` from the guard's drop | `alloc-global` |
| [`src/global/fallback.rs`](src/global/fallback.rs) | The primordial fallback heap — `static mut MaybeUninit<HeapCore>` + atomic-init state-machine + spinlock-guarded `&mut` handout (so the global allocator survives reentrant / early-init / teardown access) | `alloc-global` |
| [`src/registry/heap_slot.rs`](src/registry/heap_slot.rs) | `Sync`/`Send` impls on `HeapSlot` under the atomic single-writer protocol; the slot's `UnsafeCell` hand-off | `alloc-global` |
| [`src/registry/heap_registry.rs`](src/registry/heap_registry.rs) | The global heap slot-table — the `*mut HeapCore` pointer handoff out of a slot, used by every cross-thread routing decision | `alloc-global` |
| [`src/concurrent/hand.rs`](src/concurrent/hand.rs) | The legacy epoch-tier `AtomicSlot<T>` (older experimental concurrent tier; superseded by `alloc-xthread` for the global allocator path; **deprecated**) | `experimental` |

Under the recommended `production` feature
(`alloc-global + alloc-xthread + alloc-decommit`) the active internal seams
are eight — `alloc_core::{os, node}` plus `global::{sefer_malloc, tls_heap,
fallback}` plus `registry::{heap_slot, heap_registry}`. `alloc-xthread` and
`alloc-decommit` themselves do **not** open new `unsafe` seams — they extend
existing safe code paths.

`numa-aware` adds one more internal seam (`alloc_core::numa`), which in turn
delegates to the independently-auditable `numa-shim` crate. `experimental`
opens the older research-tier concurrent seam (now deprecated); the production
build does not pull it in.

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

`alloc(N) + free` round-trip with the OPT-E large-cache (`alloc-decommit`):
the freed segment is parked in a 2-slot cache with pages kept committed;
the next alloc of a compatible size returns it without any OS round-trip.

| Workload | SeferMalloc | mimalloc | System | vs mimalloc |
|---|---|---|---|---|
| `alloc(4 MiB) + free` | **~46 ns** | ~743 ns | ~17.5 µs | **~16× faster** |
| `alloc(16 MiB) + free` | **~46 ns** | ~861 ns | ~14.6 µs | **~19× faster** |
| `alloc(64 MiB) + free` | **~63 ns** | ~2.43 µs | ~16.9 µs | **~39× faster** |

vs `System`: roughly **270–380× faster** at all three sizes. The cache
is byte-budget'd (per-shard, default unbounded — clients set
`SEFER_LARGE_CACHE_BUDGET=…` to cap it), with lazy 10 %/sec exponential
decay back to `live + headroom`. There is no per-span size cap — a 30 GB
segment on a 64 GB box is cacheable now (the old `MAX_CACHED_LARGE_BYTES =
64 MiB` was removed in #90 — see `docs/MALLOC_BENCH.md` "Large-cache
(OPT-E)").

### Realloc grow under adversarial neighbour pressure

| Bench | SeferMalloc | mimalloc | Notes |
|---|---|---|---|
| `realloc_grow_geometric` | 173 µs | 368 µs | sefer-alloc 2.1× faster |
| `realloc_in_place_unfavorable` | **125 µs** | 1.31 ms | sefer-alloc 10.5× faster (OPT-F in-place realloc skip-copy) |

### Small-class steady-state churn (`benches/global_alloc.rs::global_alloc_churn`)

Steady-state churn over a working set of 256 live blocks: each iteration
frees a pseudo-random slot and allocates a replacement (xorshift seed,
deterministic). This is the pattern the `fastbin` per-thread magazine
(P0–P6 of [`docs/FASTBIN_DESIGN.md`](docs/FASTBIN_DESIGN.md)) targets and
the common shape of real allocation workloads.

| Size | SeferMalloc | mimalloc | vs mimalloc |
|---|---|---|---|
|   16 B | ~21.8 µs | ~36.9 µs | **1.7× faster** |
|   64 B | ~22.3 µs | ~37.2 µs | **1.7× faster** |
|  256 B | ~21.9 µs | ~22.1 µs | parity |
| 1024 B | ~21.9 µs | ~159 µs | **7.3× faster** |

### MT cross-thread (`examples/malloc_macro.rs`, larson + mstress)

Aggregate million-ops/sec (op = one alloc + one free), T = 1 / 2 / 4
worker threads, unpinned.

**larson** (server-churn, working-set + occasional cross-thread free):

| T | SeferMalloc | mimalloc | System | vs mimalloc |
|--:|-----------:|---------:|-------:|------------:|
| 1 | ~20.5 M | ~27.9 M | ~6.9 M | **1.36× slower** |
| 2 | ~23.2 M | ~18.2 M | ~6.8 M | **1.28× faster** |
| 4 | ~39.4 M | ~32.5 M | ~13.4 M | **1.21× faster** |

**mstress** (rounds of fill → free-half → refill, with cross-thread):

| T | SeferMalloc | mimalloc | System | vs mimalloc |
|--:|-----------:|---------:|-------:|------------:|
| 1 | ~26.6 M | ~34.0 M | ~4.1 M | **1.28× slower** |
| 2 | ~44.7 M | ~37.6 M | ~6.2 M | **1.19× faster** |
| 4 | ~84.1 M | ~64.0 M | ~13.5 M | **1.31× faster** |

`SeferMalloc` overtakes `mimalloc` at T ≥ 2 on both workloads (the
per-thread heap takes no shared lock; cross-thread frees route through
the lock-free Phase-10/12.6 remote path). Single-thread (T = 1) `mimalloc`
leads — see the verdict below.

### Synthetic bulk worst case (`benches/global_alloc.rs::global_alloc`)

`alloc 1024 → free 1024` — the documented worst case for any per-thread
magazine (every free overflows; every alloc empties and refills). Kept as
a regression guard, **not** a representative workload.

| Size | SeferMalloc | mimalloc | vs mimalloc |
|---|---|---|---|
|   16 B | ~29.4 µs | ~10.3 µs | 2.87× slower |
|   64 B | ~30.3 µs | ~13.7 µs | 2.21× slower |
|  256 B | ~30.8 µs | ~16.8 µs | 1.83× slower |
| 1024 B | ~32.0 µs | ~33.3 µs | parity |

A bulk-mode bypass (detect alloc-without-free streak, skip the magazine)
would close this; for now it is the documented design trade-off of
`fastbin` (default-on in `production`). Disable `fastbin` if your primary
workload is arena-style bulk alloc-then-bulk-free.

Reproduce with:

```bash
cargo bench --bench large_realloc --features "alloc-global alloc-decommit" -- large_alloc_free
cargo bench --bench global_alloc  --features production -- global_alloc_churn
cargo bench --bench global_alloc  --features production -- "^global_alloc/"
cargo run   --release --example malloc_macro --features "alloc-global alloc-xthread"
```

### Honest verdict

- **Where sefer-alloc wins big:**
  - **Large alloc/free OPT-E:** 16–39× faster than `mimalloc`, 270–380× faster
    than `System`. The headline.
  - **Real-world churn (the common shape):** 1.7× on 16/64 B, parity on 256 B,
    **7.3× on 1024 B**.
  - **Realloc** (`realloc_in_place_unfavorable`): 10.5× via OPT-F skip-copy.
  - **MT macro at T ≥ 2:** larson 1.21–1.28×, mstress 1.19–1.31× faster.
- **Where it ties:** churn 256 B; bulk 1024 B; MT mstress T = 2 within noise.
- **Where it loses:**
  - **Single-thread larson/mstress T = 1:** 1.28–1.36× behind `mimalloc`.
    Structural cost of our safety machinery (M2 double-free guard on the
    bitmap, `contains_base` foreign-pointer hash probe, cross-thread routing
    reads on every `dealloc`) — the inline-seam (#101/#102) is fully exhausted;
    closing further requires changing M2 mechanics. See
    [`docs/FASTBIN_DESIGN.md`](docs/FASTBIN_DESIGN.md) §0 and the "what's left"
    section.
  - **Synthetic bulk (16–256 B alloc-1024-then-free-1024):** 1.8–2.9× slower —
    the magazine's design worst case (every free overflows, every alloc
    empties and refills). Documented trade-off; not a real-world pattern.

Every loss above is the price of a safety guarantee `mimalloc` does not
provide (double-free = no-op, never UB; foreign pointer = safe no-op;
forbid(unsafe) at the top level with one audited `unsafe` aperture). On
real workloads — churn, MT, large-alloc — we are net faster while keeping
those guarantees.

---

## Verification evidence

This is a verification-first build. Every claim above is backed by a tool,
a test file, and a reproducible command. **51 integration test files** ship
in `tests/` (45 conventional + 6 loom models — counted separately below);
**5 example binaries** in `examples/`; **8 benches** in `benches/`
(`global_alloc`, `heap_alloc`, `heap_async_pattern`, `heap_xthread`,
`large_realloc`, `locality`, `pinned_write`, `sharded_write`);
**2 libFuzzer targets** in `fuzz/`
(`region_ops`, `global_alloc_ops`).

| Tool | What it proves | Where in repo |
|---|---|---|
| Unit / integration tests | Construction, edge cases, end-to-end behaviour | `tests/*.rs` (51 files) |
| `proptest` differential | Op-stream agreement with a reference model (M1–M4) | `tests/alloc_core_differential.rs`, `tests/differential.rs` |
| `loom` | Cross-thread protocol agreement (Phase 12, Phase 10) | `tests/loom_xthread_protocol.rs`, `loom_remote_ring.rs`, `loom_thread_free.rs`, `loom_registry.rs`, `loom_sharded.rs`, `loom_epoch.rs` (6 models) |
| `miri` (strict-provenance) | UAF, races at byte level, double-free, exposed-provenance casts | CI gate: `region_invariants`, `decommit_miri_cycle`, `reclaim_offset_unit` |
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
| `experimental` | `std` + deps | Lock-free `LockFreeRegion` / `EpochRegion` / `ShardedRegion` (legacy/deprecated; kept for backward compat and research baseline) | off | RCU / epoch experiments only |
| `pinning` | `experimental` + `core_affinity` | Thread-per-core pinning with `core_affinity` (`PinnedRunner` is NOT deprecated) | off | `shard == core` workloads |

`production` is the right starting point for almost any multi-thread or
async use of `SeferMalloc`. Without `alloc-decommit` the `SegmentTable`
slot-recycle is off and the 1024-segment ceiling is a hard cap — a tokio
server with hundreds of tasks will eventually OOM. For embedded / `no_std`
use, stay with the default `std` feature.

### Tuning the large-segment cache (`alloc-decommit`)

The `alloc-decommit` feature carries a per-thread large-segment free-cache.
The default policy is **"client controls the RSS, we ship sane defaults"**:
five environment variables (all read once at first `AllocCore::new()`,
allocation-free, safe even with `SeferMalloc` as the `#[global_allocator]`)
let the operator pick the trade-off without a recompile.

| Variable | Default | Meaning |
|---|---|---|
| `SEFER_LARGE_CACHE_BUDGET` | (unset — **unbounded**) | Per-shard ceiling on total cached bytes. Accepts `64M`, `2G`, raw bytes. **Unset = no admission limit** (any size span can enter the cache); FIFO eviction kicks in only when this is set and the budget would be exceeded. OS-OOM is propagated as `null` per the `GlobalAlloc` contract regardless. |
| `SEFER_LARGE_CACHE_DECAY_RATE` | `10` (10 %/tick) | Integer percent of `excess = cached − headroom` to release back to the OS each decay tick. Float-free parser; `7.5%` is **not** accepted — use a finer interval instead. |
| `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS` | `1000` (1 s) | Wall-clock ms between decay ticks. Lazy: a tick only fires inline on the next large alloc/free after the interval elapsed. Idle process pays nothing. |
| `SEFER_LARGE_CACHE_HEADROOM_BYTES` | `256M` | Floor below which the decay does **not** push (anti-thrashing pad). Accepts K/M/G suffix. |
| `SEFER_LARGE_CACHE_MODE` | `lazy` | `lazy` (default) / `background` / `both`. **Phase 3 stub:** `background` currently prints a one-time warning and falls back to lazy; the mode-selector plumbing is in place for a future background scavenger thread. |

The model is "**allocate fast, release slowly**": on a large `free`, the
span is admitted to the cache (subject to budget); on each subsequent large
op, the excess over `headroom` exponentially decays to the OS at the chosen
rate. Self-damping: aggressive far from target, gentle near target, no
oscillation. The default `BUDGET=unbounded` makes the cache willing to
hold any single span; if you want a hard RSS ceiling (containers, mobile),
set `SEFER_LARGE_CACHE_BUDGET=512M` (or whatever fits).

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
| [`docs/FASTBIN_DESIGN.md`](docs/FASTBIN_DESIGN.md) | Per-thread tcache magazine design (P0–P6), full sweep, win/loss ledger, production decision |
| [`docs/PROFILE_FLAMEGRAPHS.md`](docs/PROFILE_FLAMEGRAPHS.md) | Flamegraph profiling report (4 scenarios, 6 optimisation candidates) |
| [`docs/HEAP_BENCH.md`](docs/HEAP_BENCH.md), [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) | Per-tier bench writeups |
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
