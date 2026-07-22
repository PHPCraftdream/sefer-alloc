בס״ד

לכבוד הקדוש ברוך הוא — *for the glory of the Holy One, blessed be He*

# sefer-alloc

[![CI](https://github.com/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml/badge.svg)](https://github.com/PHPCraftdream/sefer-alloc/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/sefer-alloc.svg)](https://crates.io/crates/sefer-alloc)
[![Documentation](https://docs.rs/sefer-alloc/badge.svg)](https://docs.rs/sefer-alloc)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV: 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://www.rust-lang.org/)
[![100% Rust](https://img.shields.io/badge/100%25%20Rust-no%20C%2FC%2B%2B%20deps-orange.svg)](#why-bother)
[![unsafe: confined](https://img.shields.io/badge/unsafe-confined%20to%20named%20seams-yellow.svg)](#where-unsafe-lives-the-complete-list)

> A safe-by-construction, **100 % Rust** memory toolkit: a drop-in
> `#[global_allocator]` and a typed handle store over one verified segment
> substrate. Compiler-enforced `unsafe` confinement, **no C / C++ libraries
> pulled in** (no `libnuma`, no `mimalloc`, no `jemalloc`, no `snmalloc` /
> `tcmalloc`) — and **~12–35× faster than `mimalloc`** on cached large
> alloc/free (0.3.0, single-host criterion — see [Performance](#performance)).

---

## Install

```toml
[dependencies]
sefer-alloc = { version = "0.3", features = ["production"] }
```

Or via cargo:

```sh
cargo add sefer-alloc --features production
```

The `production` feature is the recommended set for any long-running
multi-thread or async workload. It is shorthand for
`alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory` — the drop-in
`GlobalAlloc` face, lock-free cross-thread free, OS page decommit, and
the per-thread fast-bin magazine. Without `alloc-decommit` the
`SegmentTable`'s free-list still recycles freed large-segment slots
(large-alloc/free churn keeps working), but empty small segments cannot
be recycled until they are decommitted; long-running processes with
many small-segment carve/decay cycles will pin slots and eventually
hit the 1024 cap.

For the bare `no_std` + `alloc` handle-store core, see
[Two faces](#two-faces) below; for the full feature matrix, see
[Features matrix](#features-matrix).

---

## Basic usage

Drop-in `#[global_allocator]` — three lines, zero configuration. Every
`Vec` / `Box` / `String` / `HashMap` allocation in your process
(including those made by `tokio`, `rayon`, `serde_json`, etc.) goes
through `sefer-alloc`.

```rust
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn main() {
    let v: Vec<u8> = (0..1024).map(|i| i as u8).collect();
    println!("vector of {} bytes", v.len());
}
```

`SeferAlloc::new()` uses defaults tuned for throughput-first workloads
(unbounded large-cache, 256 MiB headroom, 1 s decay interval, 10 %
decay rate, event-driven mode). For RSS-sensitive or container
deployments, see [Configuration](#configuration) below.

---

## Configuration

For RSS-bounded servers, containers, or any deployment where you want
to cap how much memory the allocator holds onto, use
`SeferAlloc::with_config(...)`. Every builder method is `const fn`,
so the config lives in a `static` initialiser and is resolved at
compile time — zero runtime overhead, no env vars, no parse errors.

```rust
use sefer_alloc::{SeferAlloc, LargeCacheConfig, LargeCacheMode};

const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
    .budget_bytes(512 * 1024 * 1024)      // 512 MiB hard ceiling per shard
    .headroom_bytes(64 * 1024 * 1024)     //  64 MiB anti-thrash floor
    .decay_interval_ms(200)               // 200 ms between decay ticks
    .decay_rate_percent(25)               //  25 % of excess released per tick
    .mode(LargeCacheMode::Lazy);          // event-driven (no background thread)

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);
```

### Parameters

| Method | Default | What it does |
|---|---|---|
| `.budget_bytes(N)` | `None` (unbounded) | Per-shard hard ceiling on total cached bytes. Set to your container's RSS limit. FIFO eviction fires before admitting a new span that would exceed the limit. `0` ⇒ cache disabled (nothing is cached). |
| `.headroom_bytes(N)` | `256 MiB` | Anti-thrash floor — the decay step does NOT release bytes below this level. Higher headroom = more memory retained between ticks (less aggressive trimming). |
| `.decay_interval_ms(N)` | `1000` ms | Minimum wall-clock interval between consecutive decay ticks. A tick computes `excess = cached − headroom` and releases `excess × rate` back to the OS. |
| `.decay_rate_percent(N)` | `10` % | Fraction of the excess released per tick, integer percent in `[1, 100]` (clamped). `10` ⇒ release 10 % per tick (self-damping exponential decay); `100` ⇒ flush all excess in one tick. |
| `.mode(M)` | `Lazy` | Decay trigger. **`Lazy`** — the only mode — event-driven: each large alloc/free checks if the interval has elapsed; if so, one decay step runs inline. No background thread, idle process pays nothing. `LargeCacheMode` is `#[non_exhaustive]`, leaving room for a future background-scavenger mode as a non-breaking addition. |

The model is **"allocate fast, release slowly"**: each tick removes a
constant fraction of the current excess, so the cache approaches the
headroom aggressively when far above it and gently when near it —
self-damping, no oscillation. An idle process pays nothing (the tick
is gated by the very next large alloc/free).

`SeferAlloc::new()` is equivalent to
`SeferAlloc::with_config(LargeCacheConfig::DEFAULT)`. Want to set
values from env / CLI / a config file? Read them in your own code and
pass to the builder — the allocator is intentionally agnostic.

Full reference + a worked tokio server example + how to verify the
config is live: **[`docs/INTEGRATION.md`](docs/INTEGRATION.md)**.

---

## Two faces

`sefer-alloc` ships a second face over the same substrate — a typed
handle store for slot-storage use cases. Generational handles instead
of pointers; a stale handle returns `None`, never UB. This face needs
no features beyond the default:

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

For `no_std` + `alloc` targets, disable the `std` feature:
`sefer-alloc = { version = "0.3", default-features = false }`. The
default build is `#![forbid(unsafe_code)]` at the top; the only
`unsafe` comes from `slotmap`'s audited core wrapped by a thin typed
membrane.

The two faces share one substrate: SEGMENT-aligned (4 MiB) OS-backed
spans, self-hosted metadata (no `Vec` / `HashSet` / `std::alloc` on
any alloc path), per-thread heaps, non-intrusive cross-thread free
through a per-segment MPSC ring. See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the 30-minute tour.

Under `production`, the crate becomes `#![deny(unsafe_code)]` and every
`unsafe` lives in **eight named confined seams** (`alloc_core::{os,
node}` + `global::{sefer_alloc, tls_heap, fallback}` +
`registry::{bootstrap, heap_slot, heap_registry}`) — never in the
alloc-path body outside them. Each `unsafe` block carries a `// SAFETY:` proof; the
compiler enforces the confinement (a stray `unsafe` outside a named
seam is a hard error). Complete inventory:
[Where unsafe lives](#where-unsafe-lives-the-complete-list).

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
  **~12–35× faster than `mimalloc`** (4/16/64 MiB) via the OPT-E large-segment
  cache — a 4 MiB cycle is ~59 ns vs mimalloc's ~716 ns, and ~302× faster than
  `System` (measured 2026-07-06, see [docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md)).
  Preconditions for these headline ratios: same-size reuse inside the decay
  window with a size factor ≤ 2× (the OPT-E cache holds 8 committed slots) —
  do not extrapolate them to mixed-size or cold-first-touch workloads, where
  the cache misses and the numbers regress to OS-round-trip cost.
- On **single-thread small-class churn** (the reuse pattern) it **beats
  `mimalloc` at 64 B and above** on the realistic writing pattern (64 B
  1.18×, 256 B 2.71×, 1024 B 8.65× faster; 16 B is a within-noise tie) after
  the P0–P6 perf arcs, the round4 remediation batch, and Round7–9 (directory
  + lazy-commit + Large fresh-zero-skip)
  (measured 2026-07-20 via `npm run bench:table`, see
  [docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md)). The old
  256 B churn loss was **eliminated in P6 (Э6)** — its cause was a stale
  per-heap key in the block body (not the M2 bitmap), now removed; M2 was
  strengthened in the process. On cold first-touch of tiny blocks the P3
  bump-direct carve removed the tautological round-trip, but the current
  measurement still shows a 1.5–2.3× cold gap (16 B 2.34×, 64 B 1.98×
  slower); the residual is honest per-block page-fault work, called out in
  [`docs/ALLOC_BENCH.md`](docs/ALLOC_BENCH.md).
- On **realloc** the 0.3.0 X-arc (OPT-G in-place Large growth) turned parity
  into a rout: `realloc_grow_geometric` (64 B→4 MiB) is **~40× faster than
  `mimalloc`** (9.7 µs vs 383 µs) and ~290× faster than `System`;
  `realloc_grow_neighbour_pressure` (formerly `realloc_in_place_unfavorable`;
  renamed for honesty — after OPT-G the neighbours no longer block sefer's
  in-place growth) went from 1.1× *slower* to **~1,500× faster** (906 ns vs
  1.36 ms) — every Large growth step that fits the committed 4 MiB span is a
  header update returning the same pointer (re-measured 2026-07-06).
- On **MT cross-thread** (`malloc_macro` larson/mstress) it is competitive
  with `mimalloc`, leading at T≥2 (historical 0.2.0 shape).

The verification stack is also honest: 111 integration test files, 11 loom
models, proptest differential against a reference model, miri with
strict-provenance, ThreadSanitizer (×3 clean runs), Valgrind memcheck (clean),
aarch64 (qemu), libFuzzer, soak / RSS / tokio-burn-in harnesses. The
[Verification evidence](#verification-evidence) section spells out what each
one actually proves.

---

## Architecture & principles

### Two faces, one substrate

```
         ┌───────────────────┐         ┌────────────────────────┐
         │  Region<T>        │         │  SeferAlloc           │
         │  Handle<T>        │         │  #[global_allocator]   │
         │  (safe membrane)  │         │  (unsafe trait impl)   │
         └─────────┬─────────┘         └──────────┬─────────────┘
                   │                              │
                   ▼                              ▼
         ┌─────────────────────────────────────────────────────┐
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
| **Membrane** | The typed APIs (`Handle<T>`, `Region<T>`, `AllocCore::alloc`, `SeferAlloc::alloc`). Total — cannot express UB at the type level. | safe |
| **Hand** | The confined-`unsafe` seams that touch raw memory. Each is a single audited file; every `unsafe { ... }` block carries a `// SAFETY:` proof. | confined |

The deliberate inversion: all the intelligence lives in the safe Cartographer,
so the Hand stays mechanical and small. Verification is over a total Membrane
and an integer algorithm, not a tangle of pointer math.

### Workspace: eleven independently-publishable companion crates

The workspace extracted eleven building blocks. Each is a real crates.io crate
someone can `cargo add` on its own — they are not internal implementation
details but independently useful libraries:

```
sefer-alloc
 ├── sefer-region       (crates/region)             — typed handle store (Handle<T>/Region<T>)
 ├── aligned-vmem       (crates/vmem)               — OS virtual-memory aperture           (feature: alloc-core)
 ├── numa-shim          (crates/numa)               — NUMA detection + binding             (feature: numa-aware)
 ├── malloc-bench-rs    (crates/malloc-bench)       — portable GlobalAlloc bench harness   (standalone, dev-only)
 ├── racy-ptr-cell      (crates/racy-ptr-cell)      — lazy CAS-published pointer cell      (feature: alloc-core)
 ├── ring-mpsc          (crates/ring-mpsc)          — bounded MPSC index ring + DirtyRouter (standalone; swap-in filed as CRATE-P4)
 ├── size-classes       (crates/size-classes)       — const-built size-class tables + lookup (feature: alloc-core)
 ├── tagged-index-stack (crates/tagged-index-stack) — ABA-tagged free-index stack          (feature: alloc-global)
 ├── globalalloc-model  (crates/globalalloc-model)  — differential op-stream test harness   (standalone, dev-only)
 ├── proc-memstat       (crates/proc-memstat)       — same-instant RSS / commit self-probe (standalone, dev-only)
 └── proc-probe         (crates/proc-probe)         — RESULT key=value stdout protocol     (standalone, dev-only)
```

`malloc-bench-rs`, `globalalloc-model`, `proc-memstat`, and `proc-probe` are
not in sefer-alloc's runtime dependency tree — they are dev-only / example
infra. `ring-mpsc` is likewise standalone today (its swap-in for the in-tree
`RemoteFreeRing`/`HeapOverflow` is filed as follow-up CRATE-P4). The other
six are pulled in under the feature gates noted above (`alloc-core`,
`alloc-global`, `numa-aware`).

Per-crate status:

| crate | crates.io | docs.rs |
|---|---|---|
| `sefer-region` | [![Crates.io](https://img.shields.io/crates/v/sefer-region.svg)](https://crates.io/crates/sefer-region) | [![Documentation](https://docs.rs/sefer-region/badge.svg)](https://docs.rs/sefer-region) |
| `aligned-vmem` | [![Crates.io](https://img.shields.io/crates/v/aligned-vmem.svg)](https://crates.io/crates/aligned-vmem) | [![Documentation](https://docs.rs/aligned-vmem/badge.svg)](https://docs.rs/aligned-vmem) |
| `numa-shim` | [![Crates.io](https://img.shields.io/crates/v/numa-shim.svg)](https://crates.io/crates/numa-shim) | [![Documentation](https://docs.rs/numa-shim/badge.svg)](https://docs.rs/numa-shim) |
| `malloc-bench-rs` | [![Crates.io](https://img.shields.io/crates/v/malloc-bench-rs.svg)](https://crates.io/crates/malloc-bench-rs) | [![Documentation](https://docs.rs/malloc-bench-rs/badge.svg)](https://docs.rs/malloc-bench-rs) |
| `racy-ptr-cell` | [![Crates.io](https://img.shields.io/crates/v/racy-ptr-cell.svg)](https://crates.io/crates/racy-ptr-cell) | [![Documentation](https://docs.rs/racy-ptr-cell/badge.svg)](https://docs.rs/racy-ptr-cell) |
| `ring-mpsc` | [![Crates.io](https://img.shields.io/crates/v/ring-mpsc.svg)](https://crates.io/crates/ring-mpsc) | [![Documentation](https://docs.rs/ring-mpsc/badge.svg)](https://docs.rs/ring-mpsc) |
| `size-classes` | [![Crates.io](https://img.shields.io/crates/v/size-classes.svg)](https://crates.io/crates/size-classes) | [![Documentation](https://docs.rs/size-classes/badge.svg)](https://docs.rs/size-classes) |
| `tagged-index-stack` | [![Crates.io](https://img.shields.io/crates/v/tagged-index-stack.svg)](https://crates.io/crates/tagged-index-stack) | [![Documentation](https://docs.rs/tagged-index-stack/badge.svg)](https://docs.rs/tagged-index-stack) |
| `globalalloc-model` | [![Crates.io](https://img.shields.io/crates/v/globalalloc-model.svg)](https://crates.io/crates/globalalloc-model) | [![Documentation](https://docs.rs/globalalloc-model/badge.svg)](https://docs.rs/globalalloc-model) |
| `proc-memstat` | [![Crates.io](https://img.shields.io/crates/v/proc-memstat.svg)](https://crates.io/crates/proc-memstat) | [![Documentation](https://docs.rs/proc-memstat/badge.svg)](https://docs.rs/proc-memstat) |
| `proc-probe` | [![Crates.io](https://img.shields.io/crates/v/proc-probe.svg)](https://crates.io/crates/proc-probe) | [![Documentation](https://docs.rs/proc-probe/badge.svg)](https://docs.rs/proc-probe) |

### Where `unsafe` lives (the complete list)

The extraction **improved the audit story**, not just reorganised code.
An auditor who wants to verify the OS-memory unsafe no longer has to read
through a large general-purpose allocator crate — they can audit `aligned-vmem`
(~400 lines, sole purpose: OS aperture) and `numa-shim` (~300 lines, sole
purpose: NUMA syscalls) in complete isolation. Each has one responsibility,
one reason to have `unsafe`, and its own `cargo test`.

Source of truth: `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
— **two tiers in one command**: `#![...]` matches are module-level seams
(tier 1, listed below); `#[...]` matches are item-scoped `unsafe fn`
declarations and their internal call-site blocks (tier 2, listed in the
table after the seam table). Both are comment-proof: `^\s*#!?\[` requires
the line to begin with the attribute, not a `//` prefix.

**External publishable crates (each independently auditable):**

| Crate | Path | Unsafe story |
|---|---|---|
| `aligned-vmem` | `crates/vmem/` | `#![allow(unsafe_code)]` — entire crate IS the OS aperture (`mmap`/`VirtualAlloc`/decommit); single responsibility, small, audit in isolation |
| `numa-shim` | `crates/numa/` | `#![allow(unsafe_code)]` — entire crate IS the NUMA syscall shim (`mbind`/`VirtualAllocExNuma`); single responsibility, small, audit in isolation |
| `malloc-bench-rs` | `crates/malloc-bench/` | `#![allow(unsafe_code)]` — confined to `alloc_block`/`free_block`/`drain_mailbox` helpers; every block carries `// SAFETY:` |
| `racy-ptr-cell` | `crates/racy-ptr-cell/` | `#![allow(unsafe_code)]` — single documented reason: `unsafe impl Send/Sync` for the `AtomicPtr`-backed cell + `NonNull::new_unchecked`; every site has `# Safety` / `// SAFETY:` |
| `ring-mpsc` | `crates/ring-mpsc/` | `#![allow(unsafe_code)]` — single documented reason: `unsafe fn over_raw` materialises `&AtomicUN` views over caller-supplied raw memory; `slot_at` + every raw-pointer materialisation carries `// SAFETY:`. **Zero production consumers today**: the in-tree swap of `RemoteFreeRing`/`HeapOverflow` onto this crate was investigated and found NO-GO (commit `d062798`, see `docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md`); a real, well-tested workspace member, flagged here so it doesn't silently bit-rot. |
| `globalalloc-model` | `crates/globalalloc-model/` | `#![allow(unsafe_code)]` — single documented reason: the `unsafe trait RawAllocator` (its impls must return valid pointers for the requested layout); every impl + call carries `// SAFETY:` |
| `proc-memstat` | `crates/proc-memstat/` | `#![allow(unsafe_code)]` — entire crate IS the OS-FFI self-probe (Windows `K32GetProcessMemoryInfo`, macOS `task_info`, Linux `/proc`); every block carries `// SAFETY:` |
| `sefer-region` | `crates/region/` | `#![forbid(unsafe_code)]` — zero own `unsafe`; `slotmap`'s audited core owns the generational layout |
| `size-classes` | `crates/size-classes/` | `#![forbid(unsafe_code)]` — `const`-evaluated, `no_std`, zero-dependency; no raw pointers anywhere |
| `tagged-index-stack` | `crates/tagged-index-stack/` | `#![forbid(unsafe_code)]` — lock-free via a single packed `AtomicUsize` head word; ABA tag in the high bits, no raw-pointer derefs |
| `proc-probe` | `crates/proc-probe/` | `#![forbid(unsafe_code)]` — pure protocol + re-export crate; the OS FFI stays in `proc-memstat` |

**Internal sefer-alloc seams — tier 1 (module-level)** — any `unsafe` token
not covered by a tier-1 module OR a tier-2 item-level allow (see below) is a
hard compile error in every configuration:

| Module | What it owns | Loaded under |
|---|---|---|
| [`src/alloc_core/os.rs`](src/alloc_core/os.rs) | Thin interop wrapper around `aligned-vmem`; delegates SEGMENT-aligned reservation and decommit/recommit | `alloc-core` |
| [`src/alloc_core/node.rs`](src/alloc_core/node.rs) | Intrusive free-list node r/w through raw pointers (the generalised "hand" discipline); also `release_segment` thin wrapper | `alloc-core` |
| [`src/alloc_core/numa.rs`](src/alloc_core/numa.rs) | Thin interop wrapper around `numa-shim`; delegates NUMA-node query and segment binding | `numa-aware` |
| [`src/alloc_core/dirty_by_class.rs`](src/alloc_core/dirty_by_class.rs) | The lazily-materialised per-(segment, class) dirty-bit sidecar (`PerClassDirty`); dereferences the `RacyPtrCell`-published sidecar pointer | `class-aware-dirty` |
| [`src/global/sefer_alloc.rs`](src/global/sefer_alloc.rs) | The `unsafe impl GlobalAlloc` alloc-face seam — the trait obligation + pointer handoff to the `HeapCore` (the registry-resident per-thread heap) | `alloc-global` |
| [`src/global/tls_heap.rs`](src/global/tls_heap.rs) | Raw-pointer TLS binding + `AbandonGuard` seam — the `*mut HeapCore` handoff under the single-writer invariant; `unsafe fn recycle` from the guard's drop (whole-slot reuse). | `alloc-global` |
| [`src/global/fallback.rs`](src/global/fallback.rs) | The primordial fallback heap — `static mut MaybeUninit<HeapCore>` + atomic-init state-machine + spinlock-guarded `&mut` handout (so the global allocator survives reentrant / early-init / teardown access) | `alloc-global` |
| [`src/registry/bootstrap.rs`](src/registry/bootstrap.rs) | The primordial-segment carve / SegmentTable bootstrap seam — raw-pointer footprint carving of the metadata region under the atomic single-writer bootstrap protocol. | `alloc-global` |
| [`src/registry/heap_slot.rs`](src/registry/heap_slot.rs) | `Sync`/`Send` impls on `HeapSlot` under the atomic single-writer protocol; the slot's `UnsafeCell` hand-off | `alloc-global` |
| [`src/registry/heap_registry.rs`](src/registry/heap_registry.rs) | The global heap slot-table — the `*mut HeapCore` pointer handoff out of a slot, used by every cross-thread routing decision | `alloc-global` |
| [`src/concurrent/hand.rs`](src/concurrent/hand.rs) | The legacy epoch-tier `AtomicSlot<T>` (older experimental concurrent tier; superseded by `alloc-xthread` for the global allocator path; **deprecated**) | `experimental` |

Under the recommended `production` feature
(`alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory`) the active
internal seams are **eight** — `alloc_core::{os, node}` plus
`global::{sefer_alloc, tls_heap, fallback}` plus
`registry::{bootstrap, heap_slot, heap_registry}`. `alloc-xthread`,
`alloc-decommit`, and `fastbin` themselves do **not** open new `unsafe`
seams — they extend existing safe code paths.

`numa-aware` adds one more internal seam (`alloc_core::numa`), which in turn
delegates to the independently-auditable `numa-shim` crate. `experimental`
opens the older research-tier concurrent seam (now deprecated); the production
build does not pull it in.

**Internal sefer-alloc item-scoped allows — tier 2 (task #101 / R4-9).**
Each is a single `#[allow(unsafe_code)]` on an `unsafe fn` declaration (or on
the `unsafe {}` block at its internal call site) inside a file that is
otherwise safe code. Unlike tier 1 (where `unsafe` is permitted anywhere in
the module), tier 2 confines `unsafe` to one function/block boundary with its
own `# Safety` doc — the contract (validity/size/alignment/lifetime/exclusivity
of a caller-supplied pointer) cannot be expressed in the type system and
cannot be checked at runtime, so it lives in the signature, not in prose.

| File | Sites | What they cover |
|---|---|---|
| [`src/alloc_core/alloc_core.rs`](src/alloc_core/alloc_core.rs) | 2 | `dealloc` / `realloc` — `unsafe fn` boundaries (caller-pointer contract) |
| [`src/alloc_core/alloc_core_core_diag.rs`](src/alloc_core/alloc_core_core_diag.rs) | 4 | `dbg_stamp_segment_id` / `dbg_stamp_kind_byte` (raw metadata write) + `dbg_unregister` / `dbg_recycle` — `unsafe fn` boundaries |
| [`src/alloc_core/alloc_core_small.rs`](src/alloc_core/alloc_core_small.rs) | 2 | Internal call-site blocks: `bump_gen` (in `pop_free`) / `init_gen_table_in_place` (in `reserve_small_segment`), hardened path |
| [`src/alloc_core/alloc_core_small_diag.rs`](src/alloc_core/alloc_core_small_diag.rs) | 5 | `dbg_corrupt_freelist_head_next` / `dbg_drain_freelist_batch` / `dbg_alloc_bitmap_bytes_for` / `dbg_magazine_bitmap_bytes_for` / `dbg_payload_start_for` — `unsafe fn` declarations |
| [`src/alloc_core/alloc_core_small_magazine.rs`](src/alloc_core/alloc_core_small_magazine.rs) | 1 | `flush_class` — `unsafe fn` boundary (caller-pointer contract) |
| [`src/alloc_core/alloc_core_small_reclaim.rs`](src/alloc_core/alloc_core_small_reclaim.rs) | 3 | Internal `gen_at` call-site blocks (dealloc_routing + hardened `pack_entry_hardened`) + `dbg_push_to_ring` declaration |
| [`src/alloc_core/bootstrap.rs`](src/alloc_core/bootstrap.rs) | 1 | Internal call-site block for `init_gen_table_in_place` (primordial carve, hardened path) |
| [`src/alloc_core/remote_free_ring.rs`](src/alloc_core/remote_free_ring.rs) | 2 | `over_test_buffer` / `init_test_buffer` — raw R/W over a caller buffer |
| [`src/alloc_core/segment_header_gen_table.rs`](src/alloc_core/segment_header_gen_table.rs) | 3 | `gen_at` / `bump_gen` / `init_gen_table_in_place` — atomic view + write by caller base |
| [`src/registry/heap_core_alloc.rs`](src/registry/heap_core_alloc.rs) | 6 | Internal `bump_gen` call-site blocks in `alloc` / `refill_magazine_slow` / `alloc_batch` / `alloc_small_zeroed_via_magazine` / `refill_magazine_slow_virgin` (R13-3, `virgin-zero-skip` magazine plumbing) (hardened path) |
| [`src/registry/heap_core_dealloc_batch.rs`](src/registry/heap_core_dealloc_batch.rs) | 7 | `dealloc_batch` / `dealloc_batch_small` — `unsafe fn` boundaries (caller-pointer contract) + internal call-site blocks into scalar `dealloc` / `AllocCore::flush_class` (R11-4) |
| [`src/registry/heap_core_diag.rs`](src/registry/heap_core_diag.rs) | 2 | `dbg_push_to_ring` / `dbg_push_coarse_only_entry` (R13-1) — `unsafe fn` boundaries (delegation to the unsafe producer) |
| [`src/registry/heap_core_free.rs`](src/registry/heap_core_free.rs) | 5 | dealloc-routing `unsafe fn` boundaries (caller-pointer contract) + internal call-site blocks into `AllocCore::dealloc` / `AllocCore::flush_class` |
| [`src/registry/heap_core_tcache.rs`](src/registry/heap_core_tcache.rs) | 1 | Internal call-site block for `AllocCore::flush_class` |
| [`src/registry/heap_core_xthread.rs`](src/registry/heap_core_xthread.rs) | 1 | Internal `gen_at` call-site block in `dealloc_foreign_routing` (hardened `pack_entry_hardened` path) |

That's the full list (both tiers): **18** tier-1 module-level seams (11 in
`src/`, 7 in `crates/`) plus **45** tier-2 item-scoped allows across **15**
files. Everywhere else in the crate is forbidden / denied `unsafe`; an
`unsafe` token not covered by a tier-1 module or a tier-2 item-level allow is
a hard compile error in every configuration.

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

A thread allocates from its own `HeapCore`'s per-class magazine (tcache) via a
single pointer read; deallocates with a single pointer write through the `node`
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

`OPT-E` adds a small fixed-slot cache (`LARGE_CACHE_SLOTS = 8` slots, no
fixed per-span size cap — governed instead by the configurable
`LargeCacheConfig::budget_bytes`, default unbounded) inside each `AllocCore`
that holds freed large-segment OS reservations and reuses them on the next
`alloc_large` of comparable size — **without** decommitting and
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

**sefer-alloc 0.3.x — small-class churn/cold tables re-measured 2026-07-20
post-Round9 (directory + lazy-commit + Large fresh-zero-skip landed); large-alloc
and realloc tables 2026-07-06 post-X-arc** (criterion benches on a single Windows
dev host, `SeferAlloc`
called directly through its `GlobalAlloc` impl — apples-to-apples — vs
`mimalloc 0.1` vs `System`). Per [CLAUDE.md](CLAUDE.md) the project's bench profile is the quick
one — `sample_size(10)`, short warm-up — and the host is noisy (±15–20 %), so
these are honest **comparative** measurements, **not** a rigorous statistical
suite. Trust the relative shape and the order of magnitude, not the exact
percentages; the rigorous, deterministic gate is the instruction-count
`perf_gate_iai` bench (#127/#128/#144) on Linux CI. Source-of-truth tables +
per-bench commentary live in
[`docs/ALLOC_BENCH.md`](docs/ALLOC_BENCH.md); re-run
`cargo bench --features production` for your own numbers. **Lower is better**
(latency).

### Cross-version comparison (0.2.1 → pre-round6 → current)

A same-harness three-way run (published **0.2.1** vs the tree immediately
**before the round6 wave** vs **current HEAD**) separates the pre-round6 gains
from the round6 wave's own effect — full tables, methodology and caveats in
[`docs/perf/R6_CROSS_VERSION_BENCH.md`](docs/perf/R6_CROSS_VERSION_BENCH.md).
Headline (vs-mimalloc ratio, host-drift-normalised):

- **All the large wall-clock wins landed between 0.2.1 and pre-round6**, not in
  the wave: `realloc_grow` went from copy-and-free (ms-scale, ~7× *slower* than
  mimalloc at 0.2.1) to in-place (µs-scale, ~30–1000× faster) via OPT-G; 256 B
  churn flipped from ~1.25× slower to ~1.6× faster and 1024 B churn rose to
  ~9–10× faster via Э6 — all before `345fa9b`.
- **The round6 wave itself is flat-to-slightly-better on throughput and
  regresses no family beyond host noise** (probable modest wins on 4 MiB
  large-alloc/free and the 1024 B teardown/decommit diagnostic). This is by
  design: round6 P0 work targeted **OS commit charge** (≈7.4× lower for the
  first heap), **cross-thread-free tail latency**, and **the SMALL_MAX
  fragmentation cliff** (opt-in `medium-classes`) — axes `bench:table` does not
  measure (see the R6-OPT-A judges). The wave delivered its targeted wins
  without costing throughput.

The 0.2.1 column carries the current harness ported onto the release tag, kept
as the local `bench/0.2.1` branch so 0.2.1 stays re-measurable
(`git worktree add ../sa-021 bench/0.2.1 && cd ../sa-021 && npm run bench:table`).

### Cross-version comparison — 0.2.1 → 0.3.0 (post-round7)

A fresh same-harness run of published **0.2.1** vs **current 0.3.0** (`49046ef`,
all Round7 landed), mimalloc/System as reference — full 7-group tables + the two
ratio columns (`vs 0.2.1`, `vs mimalloc`) in
[`docs/perf/R7_CROSS_VERSION_BENCH.md`](docs/perf/R7_CROSS_VERSION_BENCH.md).
Headline (ns/op, lower is better; 0.3.0's improvement over 0.2.1):

| Workload (1024 B) | 0.2.1 | 0.3.0 | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---|---|
| churn (reuse) | 45.4 | 21.4 | **2.12× faster** | **10.7× faster** |
| churn + write | 38.6 | 23.2 | **1.66× faster** | **8.6× faster** |
| `segment_decommit_cycle` (ns/batch) | 405 980 | 1 277 | **~318× faster** | **4.5× faster** |
| `working_set_cycle` (ns/batch) | 1 031 300 | 256 100 | **4.03× faster** | — |

- **The ~318× decommit-cycle win** comes from retaining emptied segments instead
  of releasing them to the OS: the **Mechanism-2 small-segment hysteresis pool**
  (default 4 seg / 16 MiB, presets in
  [`R7_POOL_CAP_PRESETS.md`](docs/perf/R7_POOL_CAP_PRESETS.md)) + the OPT-E
  large-segment cache turn an empty→reuse cycle from a `VirtualAlloc`+`MEM_COMMIT`
  syscall storm into a cheap pool-pop over the existing reservation.
- Separately, the **chunked Registry** (R6-OPT-P0-2, `e4b3e1d`+`8dc6fe8`) cut the
  Windows **first-alloc commit charge from ≈128 MiB to ≈6 MiB (~21.7×)** —
  replacing a monolithic `[HeapSlot; 4096]` inline array (committed whole on first
  alloc) with 64 lazily-materialised 64-slot chunks.
- 0.3.0 loses to mimalloc only on the cold path at small sizes (16–64 B,
  ~1.9–2.7× slower); churn at 64 B+ is a clean win (up to 10.7×).

### Large alloc / free (`benches/large_realloc.rs`, headline)

`alloc(N) + free` round-trip served by the OPT-E large-cache
(`alloc-decommit`): the freed segment is parked in the `LARGE_CACHE_SLOTS = 8`
cache with pages kept committed, so the next alloc of a compatible size
returns it with **no OS round-trip**. This is the crate's flagship strength.

| Workload | SeferAlloc | mimalloc | System | vs mimalloc | vs System |
|---|---|---|---|---|---|
| `alloc(4 MiB) + free`  | **~58.6 ns** | ~716 ns  | ~17.7 µs | **~12.2× faster** | **~302× faster** |
| `alloc(16 MiB) + free` | **~61.9 ns** | ~1.13 µs | ~17.7 µs | **~13.5× faster** | **~237× faster** |
| `alloc(64 MiB) + free` | **~60.8 ns** | ~2.58 µs | ~18.8 µs | **~33× faster** | **~258× faster** |

(measured 2026-07-06 post-X-arc, see [docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md);
the 16/64 MiB `mimalloc`/`System` absolute columns were not re-recorded in the
post-X-arc section — only `SeferAlloc` ns and the `vs mimalloc`/`vs System`
ratios were — so those two cells are carried from the pre-X-arc run.)

The cache is byte-budget'd (per-shard, default unbounded — set via
`LargeCacheConfig::new().budget_bytes(n)` in `SeferAlloc::with_config` to cap
it, where `budget_bytes(0)` disables caching), with lazy 10 %/sec exponential
decay back to `live + headroom`. There is no per-span size cap — a 30 GB
segment on a 64 GB box is cacheable now. The 0.3.0 `span_usable` fix (#134)
keeps this win without unbounded RSS amplification across cache reuse.

### Realloc grow under neighbour pressure

| Bench | SeferAlloc | mimalloc | System | Notes |
|---|---|---|---|---|
| `realloc_grow_geometric` (64 B→4 MiB) | **~9.7 µs** | ~383 µs | ~2.78 ms | **~40× faster than mimalloc; ~290× faster than System** |
| `realloc_grow_neighbour_pressure`     | **~906 ns** | ~1.36 ms | ~7.26 ms | **~1,500× faster than mimalloc; ~8,000× faster than System** |

(`realloc_grow_neighbour_pressure` was renamed from `realloc_in_place_unfavorable`
in the 2026-07-09 review: after OPT-G the live neighbours no longer prevent
sefer's in-place Large growth, so the bench measures sefer's header-update path
against the copy-and-free path mimalloc/System still take — not an adversarial
in-place case for sefer. Numbers unchanged; identical geometry, re-measured
2026-07-06.)

(Re-measured 2026-07-06 after the X-arc: OPT-G grows a Large block in place
whenever the new size fits the already-committed 4 MiB span — a header update
returning the same pointer, zero copy. Deterministic proof: `realloc_grow`
1,520,714 → 561,912 Ir / −47 % Estimated Cycles in the callgrind gate.)

### Small-class churn vs cold direct (`benches/global_alloc.rs`)

Two patterns. **Churn** (steady-state over a live working set — each iteration
frees a pseudo-random slot and allocates a replacement) is the common shape of
real workloads and what the `fastbin` per-thread magazine
([`docs/FASTBIN_DESIGN.md`](docs/FASTBIN_DESIGN.md)) targets. **Cold direct**
(no reuse, "first touch") is the historically documented worst-case where
mimalloc's cheaper first-touch path led at tiny sizes.

The **P0–P6 perf arc** (below) attacked exactly these two fronts. On cold tiny
blocks the P3 bump-direct batched carve (Э1) removed the tautological
`carve → BinTable → pop` round-trip that made every virgin block pay ~40
metadata-touch instructions: it roughly **halved the cold gap at P3 time**
(16 B 2.6× → 1.60× slower, 64 B 2.0× → 1.15× slower) and brought **cold 256 B
to parity at P3 time** — though the current re-measurement puts the cold
tiny gap at 2.67× / 1.97× slower and cold 256 B at 1.52× slower on this
noisy host (see the Cold direct column below and the iai gate for the
deterministic signal). On churn the one-branch resolver (Э2) + classify-once
(Э4) + lock-free hit counter (Э5) **widened the tiny-block lead** (16 B 1.26× →
1.63× faster, 64 B 1.23× → 1.69× faster); then **Э6 (P6) eliminated the 256 B
churn loss entirely** by moving the M2 double-free oracle out of the block body
and into hot metadata (see below). Ranges below span two runs on a noisy host;
the deterministic per-op proof is the iai gate (see below).

Churn is measured two ways. **Non-writing** (`global_alloc_churn`, the original
bench — blocks are never written; the artificial pattern where the old
stale-key slow path bit hardest) vs **writing** (`global_alloc_churn_write`,
new in P6.0 — each block is written after alloc; **the realistic pattern**,
because real code writes to the memory it allocates). The writing row is the
headline.

All three patterns below now have fully re-measured absolute ns/op (every
allocator column, every size) — no stale carried figures. Reproduce with
`npm run bench:table` ([`scripts/bench-table.mjs`](scripts/bench-table.mjs)),
the canonical wall-clock comparison script that always prints this exact shape
(ns/op, fixed bench set, vs-mimalloc ratio). It exists precisely so this table
is regenerated the same way each time instead of hand-assembled in different
units — an earlier ad-hoc table once read as a 20 ns → 40 ns "regression" that
was actually a µs-per-batch vs ns-per-op unit mixup.

**Churn + write** (`bench_churn_alloc_write` — same as churn but writes 16 B
after each alloc; **the realistic pattern**, real code writes to what it
allocates) — the headline:

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
|   16 B | 14.7 ns |  13.9 ns |  83.6 ns | 1.06× slower |
|   64 B | **15.0 ns** |  17.7 ns |  90.5 ns | **1.18× faster** |
|  256 B | **15.2 ns** |  41.4 ns | 100.5 ns | **2.71× faster** |
| 1024 B | **18.1 ns** | 156.7 ns | 116.6 ns | **8.65× faster** |

**Churn, non-writing** (`bench_churn_alloc`, working-set reuse — 1 free + 1
alloc per op; the artificial pattern where the old stale-key slow path bit
hardest, before Э6 removed it):

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
|   16 B | 23.7 ns |  23.4 ns | 141.4 ns | 1.01× slower |
|   64 B | **24.0 ns** |  29.7 ns | 150.2 ns | **1.24× faster** |
|  256 B | **18.0 ns** |  26.1 ns |  97.8 ns | **1.45× faster** |
| 1024 B | **15.3 ns** | 155.6 ns | 112.1 ns | **10.15× faster** |

**Cold direct** (`bench_direct_alloc`, no reuse — 1 alloc + 1 free per op; the
"first touch" path, historically the worst case where mimalloc's cheaper
carve led at tiny sizes):

| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |
|---|---:|---:|---:|---:|
|   16 B |  43.2 ns |  18.5 ns | 154.9 ns | 2.34× slower |
|   64 B |  44.8 ns |  22.6 ns | 138.9 ns | 1.98× slower |
|  256 B |  45.1 ns |  29.7 ns | 175.7 ns | 1.52× slower |
| 1024 B | **45.6 ns** |  57.3 ns | 169.0 ns | **1.26× faster** |

(Measured 2026-07-20 via `npm run bench:table`, post-Round9 tree — Windows dev
host, criterion `sample_size(10)`, same R5-R3 methodology as before
(TLS-heap reset between groups + rotated arm order per `(group, size)`, see
`docs/agent_reviews_round5/performance_review.md` §4.1 items 2-3). This
single-host run carries the usual ±15-20% noise band; treat the relative
shape and order of magnitude as the signal, not the exact percentages — an
isolated single-run wall-clock delta on this host cannot distinguish "the
code changed" from "the host was busier this time"
(see `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`, which needed 20
alternating paired repetitions to separate a real effect from this exact
class of noise). The deterministic tie-breaker for this exact churn path is
`npm run iai`: `docs/perf/IAI_BASELINE.md`'s R5-R2b entry shows `Ir` for
`churn_256b`/`small_churn_16b` DROPPED 20.6% across the whole round4+round5
window (42,880 → 34,036), i.e. the hot path got strictly cheaper by the one
noise-free measure available — the wall-clock ratios above should be read
as "SeferAlloc vs mimalloc/System, this run", not as "SeferAlloc got slower
since the last table." vs `System`: ~4–7× faster across the board, same
shape as before.)

**The 256 B churn loss is GONE (Э6, P6) — and M2 got stronger, not weaker.**
Through P5 sefer-alloc trailed mimalloc at 256 B churn (~1.16–1.25× slower), and
the docs pinned that on "the M2 bitmap price". That framing was incomplete: the
real cost was a stale per-heap key stamped into the freed block's **body**
(word1) and read back as a magazine double-free filter — on a non-writing bench
the key survived the free and forced a slow-path scan plus a cold/conflict cache
line touch at the 256 B stride. **Э6 removed the key entirely**: the two exact
oracles (in-magazine scan + the `BinTable` `is_free` bitmap, both hot metadata)
now run unconditionally and **the free path never touches the block body**. On
the realistic writing pattern sefer-alloc now **leads at every size** (256 B
1.64× faster, 2026-07-10); the artificial non-writing pattern leads too
(256 B 2.12× faster). This is
not a trade for safety — M2 was **strengthened**: the pre-Э6
flushed-double-free-after-user-write hole is now closed (the oracle no longer
depends on block-body contents; `tests/regression_magazine_oracles.rs` test (c)
is RED pre-Э6, GREEN on Э6). Every P0–P6 speedup deleted a tautology, never a
guard.

**Where we still trail — cold tiny blocks (16–64 B), 1.5–2.3× behind
mimalloc (2026-07-20 re-measurement: 16 B 2.34×, 64 B 1.98×, 256 B 1.52×
slower; 1024 B 1.26× faster — see the Cold direct table above).** This is the
cold carve path (`global_alloc`, no reuse), unchanged by Э6 (which targets
only the churn free path). The residual is honest per-block work — page-map
writes and page faults on genuinely fresh pages, not ceremony — documented in
[`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`](docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md).
The `alloc-runfreelist` experimental feature (PERF-3) attempted to close
exactly this cold/recycle gap via a run-encoded freelist representation and
was **honest-rejected** — it regressed every one of the 11 iai benches
*including the four cold/recycle targets*, and the wall-clock judge confirmed
the regression direction and magnitude (+40 %/+43 % on the 16 B/64 B cold
storm); see
[`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`](docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md).
The feature and its source (Ф1–Ф4: `run_stack.rs` + the gated branches in
`alloc_core_small.rs` / `alloc_core_small_magazine.rs` / `alloc_core_small_pool.rs`
/ `segment_header.rs`) have been **removed entirely** (R6-CQ-4); the experiment
record stays as institutional memory, so a reader does not need to wonder
whether work on this gap is silently ongoing.

The DETERMINISTIC counterpart to these noisy single-host wall-clock ratios is
the instruction-count `perf_gate_iai` gate (Valgrind, Linux-only CI): the P0
benches (`cold_alloc_free_256x16b` / `_256x64b`, `churn_256b`) plus the new
`churn_write_256b` bench (#150) exist to confirm the per-op `Ir` deltas of
Э1–Э6; their `Ir` baseline is captured on the first Linux perf-gate run.

### MT cross-thread (`examples/malloc_macro.rs`, larson + mstress)

**Historical 0.2.0 numbers** — the MT macro-benchmarks were NOT re-run for
0.3.0 this pass (the single-thread criterion tables above were); the crossover
shape (mimalloc leads at T=1, SeferAlloc leads at T≥2) is expected to hold but
the exact figures are not current-build. Aggregate million-ops/sec (op = one
alloc + one free), T = 1 / 2 / 4 worker threads, unpinned.

Aggregate million-ops/sec (op = one alloc + one free), T = 1 / 2 / 4
worker threads, unpinned.

**larson** (server-churn, working-set + occasional cross-thread free):

| T | SeferAlloc | mimalloc | System | vs mimalloc |
|--:|-----------:|---------:|-------:|------------:|
| 1 | ~20.5 M | ~27.9 M | ~6.9 M | **1.36× slower** |
| 2 | ~23.2 M | ~18.2 M | ~6.8 M | **1.28× faster** |
| 4 | ~39.4 M | ~32.5 M | ~13.4 M | **1.21× faster** |

**mstress** (rounds of fill → free-half → refill, with cross-thread):

| T | SeferAlloc | mimalloc | System | vs mimalloc |
|--:|-----------:|---------:|-------:|------------:|
| 1 | ~26.6 M | ~34.0 M | ~4.1 M | **1.28× slower** |
| 2 | ~44.7 M | ~37.6 M | ~6.2 M | **1.19× faster** |
| 4 | ~84.1 M | ~64.0 M | ~13.5 M | **1.31× faster** |

`SeferAlloc` overtakes `mimalloc` at T ≥ 2 on both workloads (the
per-thread heap takes no shared lock; cross-thread frees route through
the lock-free Phase-10/12.6 remote path). Single-thread (T = 1) `mimalloc`
leads — see the verdict below.

> Reconciliation note: the mstress rows above are **historical 0.2.0
> macro-bench numbers** (this run's shape — the "faster at T ≥ 2"
> verdict). `docs/ALLOC_BENCH.md`'s Phase-13.4a mstress table shows an
> earlier snapshot where the T = 2 / T = 4 rows are within-noise
> parity vs mimalloc; the ratios differ because the two runs are
> different points in the 0.2.0 evolution, not different builds under
> the current tree. Both are labelled with their origin run.

### Cold first-touch (`benches/global_alloc.rs::global_alloc`)

`alloc N → free N` — no working-set reuse, the "first touch" path (every
block is a fresh carve). Historically the documented worst case for a
per-thread magazine; the **P3 bump-direct batched carve (Э1)** removed the
tautological `carve → BinTable → pop` round-trip that made every virgin
block pay ~40 metadata-touch instructions. This is the same cold-direct
measurement as the "Cold direct" column of the Performance table above
(current `vs mimalloc` ratios reproduced here for the dedicated section;
absolute ns/op for every allocator are now in the main Cold direct table
above, re-measured 2026-07-14 via `npm run bench:table`):

| Size | vs mimalloc (2026-07-20, post-Round9) | (pre-P3 was) |
|---|---|---|
|   16 B | 2.34× slower | 2.6× slower |
|   64 B | 1.98× slower | 2.0× slower |
|  256 B | 1.52× slower | 1.5× slower |
| 1024 B | **1.26× faster** | 1.2× faster |

(Cold-direct `vs mimalloc` ratios measured 2026-07-20, post-Round9 tree, same
R5-R3 methodology fix (TLS-state isolation + arm-order rotation), see
[docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md) and the main Performance table
above — all absolute ns/op columns were re-recorded in this run; the
qualitative shape — small sizes trail, 1024 B leads — is unchanged since the
2026-07-14 run within this host's usual noise band. The "pre-P3 was" column
is the pre-X-arc historical run, kept for the before/after of the Э1
trajectory.)

The P3 carve removed the tautological round-trip, but the cold tiny gap
still sits at ~2–2.7× rather than the ~1.15–1.60× the earlier P3-era run
recorded — the residual is honest per-block work (page-map writes, page
faults on genuinely fresh pages) on a noisy single host, not ceremony, and
the deterministic signal is the iai gate below. Э6 (P6) does **not** touch
this cold carve path (it targets only the churn free path), so cold tiny
remains the one place `mimalloc` leads. The
old P7 alloc-side bulk-bypass was retired in P3 (bump-direct IS the ideal
bulk path, so the streak-detection heuristic no longer buys anything).
`fastbin` remains default-on in `production`; its M2 double-free guard is now
paid entirely in hot metadata (no block-body touch on free after Э6), so
256 B churn — previously a ~16 % loss — now **leads** mimalloc on the
realistic writing pattern (see the verdict below).

Reproduce with:

```bash
cargo bench --bench large_realloc --features "alloc-global alloc-decommit" -- large_alloc_free
cargo bench --bench global_alloc  --features production -- global_alloc_churn
cargo bench --bench global_alloc  --features production -- global_alloc_churn_write
cargo bench --bench global_alloc  --features production -- "^global_alloc/"
cargo run   --release --example malloc_macro --features "alloc-global alloc-xthread"
```

### Honest verdict

- **Where sefer-alloc wins big:**
  - **Large alloc/free OPT-E:** 12–33× faster than `mimalloc`, ~237–302× faster
    than `System` (measured 2026-07-06 post-X-arc, see
    [docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md)). The headline.
  - **Real-world churn (the common shape) — leads at 64 B and above.** On the
    realistic writing pattern: **1.18× on 64 B**, **2.71× on
    256 B**, **8.65× on 1024 B** (measured 2026-07-20 via `npm run bench:table`,
    post-Round9, see [docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md)). 16 B is a
    within-noise tie (1.06× slower this run — see the Churn + write table
    above). The 256 B churn loss was eliminated in P6
    (Э6) — the cause was a stale per-heap key in the block body, not the M2
    bitmap; removing it also **strengthened** M2 (see below).
  - **Cold first-touch after P3 (Э1 bump-direct carve):** the tautological
    round-trip is gone; the 2026-07-20 post-Round9 re-measurement shows a
    2.34× / 1.98× cold gap at 16/64 B (vs the 2.6× / 2.0× pre-P3 baseline on
    this noisy host — both sit near the pre-P3 figures, within the host's
    documented noise) and cold 1024 B **1.26× faster**.
  - **Realloc** (`realloc_grow_geometric`): **~40× faster than `mimalloc`**,
    ~290× faster than `System`; `realloc_grow_neighbour_pressure` (formerly
    `realloc_in_place_unfavorable`) **~1,500× faster** (post-X-arc OPT-G
    in-place Large growth, 2026-07-06).
  - **MT macro at T ≥ 2:** larson 1.22–1.38× faster, mstress ≈parity to 1.04×
    faster (measured 2026-07-06 post-R1/R2/R3, see
    [docs/ALLOC_BENCH.md](docs/ALLOC_BENCH.md); the earlier "1.19–1.31× faster
    on mstress" was the 0.2.0 historical run — mstress is the noisier workload
    and the mimalloc column swung run-to-run this re-run).
- **Where it ties:** `Vec_push` honest geometric growth (1.11× faster than
  mimalloc as of the 2026-07-20 post-Round9 re-measurement, vs 1.12× slower in
  the immediately prior run — this bench has swung across parity in both
  directions across successive re-measurements on this host and should be
  read as within-noise, not a stable lead or loss in either direction); 16 B
  churn (see above); MT mstress T = 2 within noise.
- **Where it now leads (was a loss through P5):**
  - **256 B churn: eliminated the loss in P6 (Э6).** Was ~1.16–1.25× behind
    mimalloc. The real cause was a stale per-heap key stamped in the block body
    (word1) — not the M2 bitmap, as the P5 docs said — which on a non-writing
    bench survived the free and forced a slow-path scan plus a cold cache-line
    touch at the 256 B stride. Э6 moved the M2 oracle entirely into hot
    metadata and stopped touching the block body; the free path is now cheaper
    than mimalloc's (mimalloc writes `next` into the block body on every free;
    we write nothing to it). On the realistic writing pattern we now lead 256 B
    by 2.71× (2026-07-20 post-Round9; non-writing 1.45×), and M2 was **strengthened** (the
    flushed-double-free-after-user-write hole is closed;
    `tests/regression_magazine_oracles.rs` test (c) is RED pre-Э6, GREEN on Э6).
- **Where it loses:**
  - **Cold tiny blocks (16–64 B): 1.5–2.3× behind `mimalloc`** (2026-07-20
    re-measurement: 16 B 2.34×, 64 B 1.98×, 256 B 1.52× slower). The P3
    bump-direct carve removed the tautological round-trip but did not fully
    close the gap — what remains is honest per-block work (page-map writes,
    page faults on genuinely fresh pages), not ceremony.
  - **Single-thread larson/mstress T = 1:** 1.28–1.36× behind `mimalloc`
    (historical 0.2.0 MT numbers, not re-run this pass). Structural cost of our
    safety machinery; the per-thread architecture means it does not compound —
    at T ≥ 2 sefer-alloc leads. See
    [`docs/FASTBIN_DESIGN.md`](docs/FASTBIN_DESIGN.md) §0.
  - **Synthetic bulk (16–256 B alloc-1024-then-free-1024):** 1.5–2.3× slower
    (2026-07-20, same run as the Cold direct table — it is the same bench
    family) — the magazine's design worst case (every free overflows, every
    alloc empties and refills). Documented trade-off; not a real-world pattern.

Every loss above is the price of a safety guarantee `mimalloc` does not
provide (double-free of LIVE/MAPPED memory = no-op, never UB, protected
by the pre-reuse `off >= bump` stale-free guard (#138); foreign pointer =
safe no-op; forbid(unsafe) by default at the top level with named
audited seams under `production`). One documented residual: the
**ring↔magazine cross-thread double-free residual limit of M2** — a block whose
cross-thread free is still in-flight in a segment's `RemoteFreeRing` (not yet
drained) sets neither own-thread oracle (magazine `slots` scan nor BinTable
`is_free` bitmap). Two of its three legs are closed on plain `production`: the
in-magazine leg (X2 / #164) and the refill-window double-issue leg (R1, f23f7eb).
The **third leg — *re-issue-before-drain*** — remains an accepted residual under
plain `production`: pinned by the permanently-`#[ignore]`d
`tests/regression_xthread_double_free_residual.rs` (honestly red without
per-block generations — no distinguishing state exists), modelled by
`tests/loom_magazine_ring_compose.rs`. Under **`--features hardened`** the X7
per-granule generation guard (stamp the ring note with the block's generation at
remote-free time; drop it on drain if the generation has advanced) closes this
leg — pinned by the sibling `residual_xthread_double_free_no_corruption_hardened`
test in the same file — **except for the 1/256 wrap**: ≥256 re-issues of one
block without an intervening drain of the stale note collides with the current
generation mod 256, the accepted probabilistic residual-of-the-residual (design
plan §2.5 rejected doubling the ring footprint for a `u64` gen; pinned by
`tests/regression_gen_wrap_boundary.rs`). Full account in
[`docs/DURABILITY.md`](docs/DURABILITY.md) (ledger entry + §"X7 per-granule
generation counter") and
[`docs/design/X7_GENERATIONAL_RING_PLAN.md`](docs/design/X7_GENERATIONAL_RING_PLAN.md).
On real workloads — churn, MT, large-alloc — we are net faster while keeping
those guarantees.

---

## Verification evidence

This is a verification-first build. Every claim above is backed by a tool,
a test file, and a reproducible command. **111 integration test files** ship
in `tests/` (100 conventional + 11 loom models — counted separately below);
**5 example binaries** in `examples/`; **9 benches** in `benches/`
(`global_alloc`, `heap_alloc`, `heap_async_pattern`, `heap_xthread`,
`large_realloc`, `locality`, `perf_gate_iai`, `pinned_write`, `sharded_write`);
**3 libFuzzer targets** in `fuzz/`
(`region_ops`, `global_alloc_ops`, `heap_core_ops`).

| Tool | What it proves | Where in repo |
|---|---|---|
| Unit / integration tests | Construction, edge cases, end-to-end behaviour | `tests/*.rs` (111 files) |
| `proptest` differential | Op-stream agreement with a reference model (M1–M4) | `tests/alloc_core_differential.rs`, `tests/differential.rs` |
| `loom` | Cross-thread protocol agreement (Phase 12, Phase 10) — honest status per file (some model live paths, some are retained-with-honesty-notes on removed/dead paths) in each file's own doc comment | `tests/loom_deferred_large.rs`, `loom_dirty_multi_segment.rs`, `loom_dirty_publish.rs`, `loom_epoch.rs`, `loom_heap_overflow.rs`, `loom_heap_overflow_drain_guard.rs`, `loom_magazine_ring_compose.rs`, `loom_overflow_first_retry.rs`, `loom_remote_ring.rs`, `loom_remote_ring_drain_guard.rs`, `loom_sharded.rs`, `loom_thread_free.rs`, `loom_xthread_protocol.rs` (13 in-tree models), plus the extracted crates' real-type suites `crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs`, `crates/ring-mpsc/tests/loom_ring_mpsc.rs`, `crates/tagged-index-stack/tests/loom_aba.rs` (CRATE-P3/P4/P7 — replacing the former in-tree `loom_bootstrap_cas`/`loom_chunk_cas`/`loom_fallback_init`/`loom_overflow_sidecar_cas`/`loom_free_slots_aba` shadow models) |
| `miri` (strict-provenance) | UAF, races at byte level, double-free, exposed-provenance casts | CI gate: `region_invariants`, `decommit_miri_cycle`, `reclaim_offset_unit` |
| Safe-surface stress (pure-safe API) | M1/M3 soundness: `alloc` never hands out aliasing pointers, so no purely-safe `Box`/`Vec`/`Arc` usage can trigger double-free/UAF | `tests/stress_safe_surface_no_aliasing.rs` (6 threads × 1500 iters × 6 size classes; zero `unsafe`; 30+ runs) |
| ThreadSanitizer | Real cross-thread data races on a live binary | CI job + manual ×3 verified clean on `race_repro`, `race_norecycle`, `global_alloc_mt`, `heap_cross_thread`, `decommit_stale_ring`, `decommit_soak` |
| Valgrind `memcheck` | UAF, leaks, invalid reads at the process level | Manual: clean on all three cross-thread test binaries. Note: `helgrind` / `DRD` are inapplicable to lock-free atomic code (Valgrind doesn't model Rust atomics) — TSan is the right concurrency detector here. |
| aarch64 via `qemu-user` | Code-gen + relaxed-memory smoke on ARM | CI job + manual 13/13 tests pass. Honest caveat: TCG translation does not fully model ARM's weak-memory; real ARM hardware verification is a follow-up. |
| libFuzzer | Op-stream invariants under random input | `fuzz/fuzz_targets/region_ops.rs`, `global_alloc_ops.rs`, `heap_core_ops.rs` (fastbin magazine) |
| Soak harness | N-thread × hours stability | `examples/soak_xthread.rs` (32 / 64 / 128 workers) |
| tokio burn-in | Live `#[global_allocator]` under tokio multi-thread runtime | `examples/tokio_burn_in.rs` |
| RSS probe | Memory recovery under asymmetric cross-thread pressure | `examples/rss_probe.rs` |
| Macro-bench | MT throughput vs `mimalloc` and System | `examples/malloc_macro.rs` (larson + mstress) |
| Flamegraph profiling | Hot path identification per workload | `docs/PROFILE_FLAMEGRAPHS.md` (4 scenarios) |

Every CI job is wired (`.github/workflows/ci.yml`) and runs on every push:
test matrix on x86_64 + aarch64 (9 feature combinations), a `windows-latest`
`production` run, the workspace member crates' own suites, miri with
strict-provenance, ThreadSanitizer, an MSRV (1.88) check, clippy, rustfmt.
(libFuzzer has its own nightly/manual cadence — see `fuzz/README.md` — not a
per-push job.)

The full safety stack and the relationship between layers is documented in
[`docs/ARCHITECTURE.md §8`](docs/ARCHITECTURE.md) and
[`docs/INVARIANTS.md`](docs/INVARIANTS.md).

---

## Features matrix

| Feature | Pulls in | What it enables | Default | When to use |
|---|---|---|---|---|
| `std` | — | `SyncRegion`, all `std`-gated tiers | **on** | almost always |
| `alloc-core` | `std` | The segment substrate (`AllocCore`) | off | building on `AllocCore` directly |
| `alloc-xthread` | `alloc-core` | Lock-free cross-thread free via `RemoteFreeRing` | off | multi-thread allocator |
| `alloc-global` | `alloc-core` | The `SeferAlloc` `#[global_allocator]` face | off | process-wide allocator |
| `alloc-decommit` | `alloc-core` | Return empty-segment payload pages to OS + `SegmentTable` slot-recycle | off | long-running / DBMS workloads |
| `numa-aware` | `alloc-core` | NUMA-node stamping + local-node preference (Linux `mbind`, Windows `VirtualAllocExNuma`) | off | multi-socket NUMA hardware |
| `fastbin` | `alloc-global + alloc-xthread` | Per-thread magazine (tcache) fast path — array-based per-class pop/push, M2 protected by hot-metadata oracles (no block-body touch) | off (on under `production`) | server-churn / mixed-size multi-threaded workloads |
| **`production`** | `alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory` | **The recommended combo for long-running multi-thread workloads.** The fast default — no paid caller-misuse checks on the free hot path. | off | **DBMS, async runtimes, anything that allocates over hours.** |
| `alloc-stats` | — | Per-hit **diagnostic** counters: bumps `stats().tcache_hits` (magazine) and `stats().large_cache_hits` (large cache) on each hit. Default OFF and **NOT** in `production` — the per-hit increment is compiled out of the churn/large-cache hot paths, and without it those two `stats()` fields read `0` (all other `stats()` fields are unaffected). The counter storage lives in the shared registry slot, so toggling this never changes layout/ABI. | off | you poll `stats().tcache_hits` / `.large_cache_hits` and want the real hit counts (add alongside `production`) |
| `hardened` | `fastbin` | **Paranoid deploys.** Additive over `production`. Adds opt-in defence-in-depth against UNSAFE-CALLER misuse that costs cycles: currently the interior-pointer free guard on **both** own-thread free faces — the `SeferAlloc` magazine (`HeapCore`) and the `AllocCore` substrate (`dealloc_small`) — rejecting a free of a pointer that is not the block start (`off % block_size != 0`) as a detected no-op instead of a mis-indexed bitmap read → double-issue. The check is a modulo-per-free (a real division), so it is **NOT** on the production fast path. (Cross-thread frees are already guarded unconditionally by `reclaim_offset`.) **X7 closure:** under `hardened`, a per-granule generation counter also closes the *re-issue-before-drain* leg of the ring↔magazine cross-thread double-free residual (the third leg of M2, open under plain `production`) — the ring note is stamped with the block's generation and dropped on drain if it has advanced — **except the 1/256 wrap** (≥256 re-issues without an intervening drain collide mod 256), the accepted probabilistic residual-of-the-residual. See [`docs/DURABILITY.md`](docs/DURABILITY.md) (ledger entry + X7 §). | off | untrusted / adversarial callers, forensic hardening |
| `experimental` | `std` + deps | Lock-free `LockFreeRegion` / `EpochRegion` / `ShardedRegion` (legacy/deprecated; kept for backward compat and research baseline) | off | RCU / epoch experiments only |
| `pinning` | `experimental` + `core_affinity` | Thread-per-core pinning with `core_affinity` (`PinnedRunner` is NOT deprecated) | off | `shard == core` workloads |
| `batch-api` | `experimental` + `alloc-core` | Tcache-aware batch alloc/dealloc (`SeferAlloc::alloc_batch`/`dealloc_batch`). **⚠ No semver guarantees** — signature/behavior may change or the feature may be removed in any release while it depends on `experimental` (R12-12) | off | you have measured a real batch-size win for your workload and accept an unstable API |

`production` is the right starting point for almost any multi-thread or
async use of `SeferAlloc`. Without `alloc-decommit`, unregister /
free-list still runs unconditionally (freed large-segment slots recycle
normally), but empty small segments are pinned — their slots cannot be
recycled until they are decommitted; a long-running tokio server with
many small-segment carve/decay cycles will eventually hit the 1024 cap.
For embedded / `no_std` use, stay with the default `std` feature.

### Tuning the large-segment cache (`alloc-decommit`)

The `alloc-decommit` feature carries a per-thread large-segment free-cache.
Configuration is via the `LargeCacheConfig` const builder — all knobs are
set at compile time in a `static` initialiser; no environment reads, no
runtime parse errors.

| Builder method | Default | Meaning |
|---|---|---|
| `budget_bytes(n)` | `None` (**unbounded**) | Per-shard ceiling on total cached bytes. `0` = cache disabled (every span released to the OS immediately). **Unset = no admission limit**; FIFO eviction fires only when this is set and the new span would exceed it. |
| `decay_rate_percent(n)` | `10` (10 %/tick) | Integer percent of `excess = cached − headroom` to release back to the OS per tick. Range `[1, 100]`, clamped. |
| `decay_interval_ms(n)` | `1000` (1 s) | Minimum wall-clock ms between two consecutive decay ticks. A tick fires inline on the next large alloc/free after the interval elapsed. Idle processes pay nothing. |
| `headroom_bytes(n)` | `256 MiB` | Floor below which the decay is a no-op (anti-thrashing pad). |
| `mode(m)` | `LargeCacheMode::Lazy` | `LargeCacheMode::Lazy` is the default and only variant. The enum is `#[non_exhaustive]`, reserved for a future background-scavenger mode as a non-breaking addition. |

The model is "**allocate fast, release slowly**": on a large `free`, the
span is admitted to the cache (subject to budget); on each subsequent large
op, the excess over `headroom` exponentially decays to the OS at the chosen
rate. Self-damping: aggressive far from target, gentle near target, no
oscillation. The default `budget=None` (unbounded) admits any span; if you
want a hard RSS ceiling (containers, mobile), add
`.budget_bytes(512 * 1024 * 1024)` to your config (or whatever fits).

---

## Run the examples

See [Install](#install) above for the Cargo dependency. The repository
ships several runnable examples that exercise the allocator under real
workloads:

```bash
# Handle store / global allocator example
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
| [`docs/INTEGRATION.md`](docs/INTEGRATION.md) | How to attach the allocator to a project + the `LargeCacheConfig` builder (budget / decay period / decay rate / headroom / mode) |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 30-minute end-to-end technical tour |
| [`docs/INVARIANTS.md`](docs/INVARIANTS.md) | The I1–I6 (Region) and M1–M8 (Malloc) invariants |
| [`docs/DESIGN.md`](docs/DESIGN.md) | Cartographer / Membrane / Hand model for `Region<T>` |
| [`docs/ALLOC_PLAN.md`](docs/ALLOC_PLAN.md) | Detailed Phase 8+ allocator plan |
| [`docs/PHASE35_DECOMMIT_DESIGN.md`](docs/PHASE35_DECOMMIT_DESIGN.md) | M6 decommit + why no epoch reclamation is needed |
| [`docs/PHASE_NUMA_DESIGN.md`](docs/PHASE_NUMA_DESIGN.md) | NUMA-aware path design |
| [`docs/CROSS_THREAD_STATE_MACHINES.md`](docs/CROSS_THREAD_STATE_MACHINES.md) | The cross-thread-free state-machine spec |
| [`docs/DURABILITY.md`](docs/DURABILITY.md) | Ultra-long-run counter inventory: every monotonic/wrapping cursor, its wrap arithmetic, verdict, and boundary test |
| [`docs/RACE_DRAIN_RECLAIM.md`](docs/RACE_DRAIN_RECLAIM.md) | The §13 / §14 race investigation (the four "peelings") |
| [`docs/ALLOC_BENCH.md`](docs/ALLOC_BENCH.md) | Full benchmark results, OPT-E numbers, honest verdicts |
| [`docs/FASTBIN_DESIGN.md`](docs/FASTBIN_DESIGN.md) | Per-thread tcache magazine design (P0–P6), full sweep, win/loss ledger, production decision |
| [`docs/PROFILE_FLAMEGRAPHS.md`](docs/PROFILE_FLAMEGRAPHS.md) | Flamegraph profiling report (4 scenarios, 6 optimisation candidates) |
| [`docs/HEAP_BENCH.md`](docs/HEAP_BENCH.md), [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) | Per-tier bench writeups |
| [`docs/PLAN.md`](docs/PLAN.md), [`docs/ALLOC_PLAN_PHASE12-13.md`](docs/ALLOC_PLAN_PHASE12-13.md) | Phase plans, dependency DAGs, risk registers |
| [`docs/GLOSSARY.md`](docs/GLOSSARY.md) | Identifier glossary: decodes the ID families used in source comments (I1–I6, M1–M11, Phase/P/Ф codes, Э-series, OPT-A…H, X7, W/A/MUST/SEC items, `task #NNN`) |

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
- **The large-cache has no fixed per-span size cap.** The old
  `MAX_CACHED_LARGE_BYTES = 64 MiB` ceiling was removed (#90); admission is
  governed by `LargeCacheConfig::budget_bytes` (default `None` — unbounded)
  and the fixed `LARGE_CACHE_SLOTS = 8` slot count, not by span size. A
  workload with sustained multi-GB large allocations is cacheable subject to
  the configured budget (or the process's available RSS, if unbounded).
- **`alloc-decommit` is opt-in.** Without it, unregister and the
  SegmentTable free-list still recycle freed large-segment slots
  unconditionally, but empty small segments cannot be recycled (they
  are recycled only when decommitted). Long-running processes with
  many small-segment carve/decay cycles will pin slots and eventually
  hit the 1024 cap. Use the `production` feature alias to avoid this.

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
