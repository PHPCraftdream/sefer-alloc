# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-06-30

Post-0.2.1 hardening pass — six phases (A–F), each independently reviewed,
counterfactual-verified, and committed.

### Fixed

- **A1 — permanent leak: cross-thread free of a Large/huge segment.** A
  remote free of a Large segment no-op'd instead of reclaiming it — the
  segment (≥4 MiB) and its `SegmentTable` slot leaked forever under any
  allocate-here/free-there workload (the canonical case: an async runtime
  migrating a task holding a large buffer to a different worker thread). Now
  reclaimed via a per-heap deferred-free stack, drained lazily on the
  owner's next large allocation.
- **A2 — `fastbin` buildable without `alloc-xthread` (unsound).** A
  cross-thread free with `fastbin` alone had no ownership-checked routing
  path — a data race into another thread's private magazine. `fastbin` now
  requires `alloc-xthread` (Cargo feature unification + a `compile_error!`
  guard).
- **B1 — page-aligned allocations (512 B – 16 KiB, `align` a multiple of
  512/1024/2048/4096) still burned a dedicated Large segment**, the last gap
  in #114's fix. Eight page-aligned size classes added to the table.
- **Latent `realloc` cross-class-shrink bug**, exposed by B1: `AllocCore::realloc`'s
  in-place fast path aliased a shrink across size classes, corrupting the
  smaller class's free list on a later layout-derived free. Restricted to
  same-class in-place; a cross-class shrink now relocates.
- **F1 — fallback-heap init livelock.** If the CAS winner initialising the
  process-global fallback heap hit primordial OOM, every other thread
  spun forever waiting for a `READY` that would never come. Losers now
  observe the rollback and re-race the CAS.

### Changed — performance

- **C1 — the per-thread magazine (`fastbin`) now serves `align > 16`
  requests** (tokio task cells, page-aligned buffers), not just the
  historical `align <= 16` case — the main remaining hot-path gap for the
  workload #114/B1 targeted.
- **C2 — `realloc`'s in-place fast path is now reachable through the
  `#[global_allocator]` face**, not just the lower-level `AllocCore` API; a
  same-class resize through `SeferAlloc` no longer pays a redundant
  alloc+copy+dealloc.
- **D1 — `LARGE_CACHE_SLOTS` raised 2 → 8**, with a correctness fix: eviction
  now uses a true insertion-order FIFO (a monotonic sequence number) instead
  of an index-order assumption that only held at 2 slots. A workload cycling
  more than two distinct large sizes now gets real cache reuse instead of
  thrashing to an OS round-trip on every allocation.
- **D3 — magazine refill is now a per-class byte budget** (≈64 KiB) instead
  of a fixed 16-block count for every class; a large size class no longer
  parks several MiB in one idle thread's cache after a single refill.

### Added

- **`SeferAlloc::stats() -> AllocStats`** — a cheap, lock-free, process-wide
  diagnostic snapshot (cache hits, decommit calls, cross-thread reclaims,
  ring overflows, segments reserved/released, heaps claimed). Previously
  every one of these counters was crate-internal and invisible in
  production; `segments_reserved_total - segments_released_total` is the
  single most useful field for spotting a segment leak before it escalates
  to an OOM abort. `#[non_exhaustive]`, stable field set across every
  feature combination.
- **D2 — process-wide `RemoteFreeRing` overflow counter**, feeding
  `AllocStats::ring_overflows`.
- Rustdoc: a "Multi-thread safety" section on `SeferAlloc` spelling out the
  `alloc-global`-without-`alloc-xthread` footgun (cross-thread frees leak
  monotonically), and a "std-only" note.

### Internal

- CI: `-D warnings` restored on the clippy gate after a warnings-cleanup
  pass; miri matrix extended to the task-#114 align-regression tests; a
  process-global-state test flake in `heap_core_bulk_bypass` fixed at its
  real root cause (whole-heap slot reuse carrying stale P7 state across
  tests in one binary).

## [0.2.1] - 2026-06-30

### Fixed — `align > 16` allocations no longer burn a dedicated segment each

`SizeClasses::class_for(size, align)` unconditionally returned `None` for
any `align > SMALL_ALIGN_MAX` (= `MIN_BLOCK` = 16). Every allocation with
a larger alignment — including the `tokio::runtime::task::core::Cell<T,S>`
shape (≈640 B, `#[repr(align(128))]` against false sharing) — was routed
to the dedicated-segment Large path, consuming a full ~4 MiB segment and
one `SegmentTable` slot per request.

Under concurrent task-spawning workloads (canonical reproducer: the
shamir-db `duplex_throughput/duplex_cap32/32` bench — 32 in-flight
tokio tasks × 55 iterations), cumulative live segments exceeded
`MAX_SEGMENTS = 1024`, then `alloc_large_slow → SegmentTable::register`
returned `None`, then the `GlobalAlloc` face returned null, then
`std::alloc::handle_alloc_error` aborted the process with
`memory allocation of 640 bytes failed`.

`class_for` now searches for the smallest small class whose
`block_size >= max(size, align)` AND `block_size % align == 0`. M4
(alignment fidelity) is preserved: the segment base is `SEGMENT`-aligned,
the offset within is a multiple of `block_size`, and `block_size` is a
multiple of `align`, so the returned pointer is naturally `align`-aligned
without any per-block padding. The fast path for `align ≤ MIN_BLOCK = 16`
(the typical case) is byte-identical to the previous behaviour — one
`SIZE2CLASS` load. The slow path is a forward walk over at most
`SMALL_CLASS_COUNT = 40` entries; in practice it settles in 0–3 steps
for power-of-two alignments common in async runtimes (32 / 64 / 128 / 256).

For `(640, align=128)` the resolver picks the existing class with
`block_size = 768` (768 % 128 == 0). Per-allocation memory cost drops
from ~4 MiB to ~768 B, and the per-process `SegmentTable` is no longer
touched on the hot path.

Regression test: `tests/regression_large_align_no_segment_exhaustion.rs`
(2048 sequential `(640, 128)` allocations + 1500 sequential allocations
each for 4 representative `(size, align)` shapes). Counterfactual
verified — reverting the fix makes the test fail on iteration 1023
(= `MAX_SEGMENTS − 1`, primordial segment holds the first slot).

Single-threaded substrate change; no concurrency-protocol or wire-format
implications. Full test suite under `features = ["production"]` —
including loom (`loom_bootstrap_cas`, `loom_xthread_protocol`,
`loom_thread_free`) — green.

## [0.2.0] - 2026-06-29

### Changed — BREAKING: `SeferMalloc` renamed to `SeferAlloc`

The headline `#[global_allocator]` type is renamed from `SeferMalloc` to
`SeferAlloc`. The "malloc" suffix was a libc convention inherited from
C-wrapper allocators (`mimalloc`, `jemalloc`, `tcmalloc`) and clashed
with sefer-alloc's positioning as a pure-Rust allocator with no C deps.
The new name aligns the type with the crate name and the Rust ecosystem's
modern `*-alloc` convention.

**Migration:** rename every occurrence of `SeferMalloc` in your code to
`SeferAlloc`. The constructors (`new()`, `with_config(...)`) and the
public API surface are otherwise unchanged.

```rust
// Before (0.1.x):
use sefer_alloc::SeferMalloc;
#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// After (0.2.0):
use sefer_alloc::SeferAlloc;
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
```

`LargeCacheConfig`, `LargeCacheMode`, `Region`, `Handle`, `SyncRegion`,
`AllocCore`, and every other public type are unchanged.

Internal: `src/global/sefer_malloc.rs` → `src/global/sefer_alloc.rs`
(module file rename). User-facing docs (`README.md`, `docs/INTEGRATION.md`,
`docs/ARCHITECTURE.md`) updated to use "alloc face" terminology consistently;
historical / planning docs (`ALLOC_PLAN.md`, `FINDINGS_PHASE12.md`, etc.)
keep their original "malloc face" language as historical record.

`0.1.0` is yanked from crates.io to direct fresh installs to `0.2.0`;
existing `Cargo.lock` references continue to work.

### Changed — const-builder config API replaces env vars (alloc-decommit)

- **`LargeCacheConfig` const builder** — new type (re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`). All five knobs
  that were previously set via environment variables are now expressed at
  compile time via a `const fn` builder chain:

  ```rust
  use sefer_alloc::{SeferMalloc, LargeCacheConfig, LargeCacheMode};

  const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
      .budget_bytes(512 * 1024 * 1024)
      .headroom_bytes(64 * 1024 * 1024)
      .decay_interval_ms(200)
      .decay_rate_percent(25)
      .mode(LargeCacheMode::Lazy);

  #[global_allocator]
  static GLOBAL: SeferMalloc = SeferMalloc::with_config(CONFIG);
  ```

- **`SeferMalloc::with_config(config: LargeCacheConfig) -> Self`** (`const fn`,
  only under `alloc-decommit`) — constructs the allocator with a custom
  large-cache config. The config is plumbed into each per-thread `AllocCore`
  on first TLS bind.

- **`SeferMalloc::new()`** unchanged — equivalent to
  `SeferMalloc::with_config(LargeCacheConfig::DEFAULT)`.

- **`AllocCore::new_with_config(config: LargeCacheConfig) -> Option<Self>`**
  (`alloc-decommit` only) — new constructor for direct `AllocCore` users.

- **Env vars removed entirely** — `SEFER_LARGE_CACHE_BUDGET`,
  `SEFER_LARGE_CACHE_HEADROOM_BYTES`, `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS`,
  `SEFER_LARGE_CACHE_DECAY_RATE`, `SEFER_LARGE_CACHE_MODE` are no longer read.
  The allocation-free env-var parser in `src/alloc_core/os.rs` is deleted.
  Default values are byte-identical to what the parsers produced when no variable
  was set (headroom=256 MiB, interval=1000 ms, rate=10 %, budget=unbounded,
  mode=Lazy).

- **Tests updated** — `tests/large_cache_budget.rs`, `tests/large_cache_decay.rs`,
  and `tests/large_cache_mode.rs` no longer use `std::env::set_var`. The
  env-var test cases are replaced with equivalent `AllocCore::new_with_config`
  tests that are deterministic and safe to run in parallel.

## [0.1.0] - 2026-06-28

### Changed — workspace extraction (tasks #74–#86)

Four independently-publishable companion crates extracted from sefer-alloc
into `crates/`. Each is a real crates.io package someone can `cargo add`
on its own:

- **`sefer-region`** (`crates/region/`) — typed handle store
  (`Handle<T>` / `Region<T>` / `SyncRegion<T>`). `#![forbid(unsafe_code)]`.
  ([docs.rs/sefer-region](https://docs.rs/sefer-region) — link live after publish.)

- **`aligned-vmem`** (`crates/vmem/`) — OS virtual-memory aperture:
  SEGMENT-aligned `mmap`/`VirtualAlloc` + page decommit/recommit.
  `#![allow(unsafe_code)]` — sole purpose IS the OS unsafe, single
  responsibility, small codebase, independently auditable.
  ([docs.rs/aligned-vmem](https://docs.rs/aligned-vmem) — link live after publish.)

- **`numa-shim`** (`crates/numa/`) — dependency-free NUMA detection and
  binding. Linux `mbind(2)` via `syscall(2)` (no `libnuma`), Windows
  `VirtualAllocExNuma`. `#![allow(unsafe_code)]` — sole purpose IS the NUMA
  syscall unsafe, single responsibility, independently auditable.
  ([docs.rs/numa-shim](https://docs.rs/numa-shim) — link live after publish.)

- **`malloc-bench-rs`** (`crates/malloc-bench/`) — portable `GlobalAlloc`
  benchmark harness (larson + mstress). Callable against any allocator without
  installing it as `#[global_allocator]`. Not in sefer-alloc's runtime dep
  tree.
  ([docs.rs/malloc-bench-rs](https://docs.rs/malloc-bench-rs) — link live after publish.)

**sefer-alloc itself** re-exports `sefer-region`'s surface for backward
compatibility — existing `use sefer_alloc::{Region, Handle, SyncRegion}` code
compiles unchanged. `alloc_core::os` and `alloc_core::numa` are now thin
interop wrappers that delegate to `aligned-vmem` and `numa-shim` respectively.

**Audit story improved:** an auditor no longer has to navigate the full
allocator codebase to verify the OS-memory unsafe. `aligned-vmem` (~few hundred
lines, single purpose) and `numa-shim` (~few hundred lines, single purpose) can
each be audited in complete isolation with `cargo test` confirming green.

### Added — large-cache redesign Phase 3 (alloc-decommit, mode-selector + future stub)

- **`LargeCacheMode { Lazy, Background, Both }`** enum, re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`. The mode is selected
  via the new `SEFER_LARGE_CACHE_MODE` env var (case-insensitive: `lazy` /
  `background` / `both`; unrecognised values fall back to `Lazy`).

- **Default = `Lazy`** — Phase 2 behaviour is preserved bit-for-bit. Setting
  `SEFER_LARGE_CACHE_MODE=background` currently prints a one-time process
  warning ("background mode requested but not yet implemented — falling back
  to lazy") and continues with lazy decay. The full background-thread
  implementation has identified risks documented inline (Mutex refactor +
  HeapRegistry iteration API + safe spawn timing + TSan validation) and is
  intentionally deferred to a follow-up; the mode-selector plumbing lets a
  future commit turn it on without any user-facing API change.

- **`tests/large_cache_mode.rs`** — 3 new tests covering default-Lazy,
  per-shard mode storage, and env-var parsing.

### Changed — large-cache redesign Phase 2 (alloc-decommit)

- **Lazy exponential decay**: large-cache excess over the headroom target
  decays toward the OS at 10 %/tick by default. On every large `alloc` or
  `free`, a single `Instant::now()` comparison checks whether
  `decay_interval` has elapsed; if so, `excess = used - headroom` and
  `release = excess × rate` bytes are FIFO-evicted to the OS. No background
  thread — the decay is fully inline, paying nothing while the process is idle
  (mobile/embedded friendly). Phase 3 will add an optional background thread.

- **Three new env vars** (all read once at `AllocCore::new`, allocation-free):
  - `SEFER_LARGE_CACHE_DECAY_RATE` — integer percent (`"10"`, `"10%"`;
    default 10). Parsed without floats to avoid any floating-point dependency.
  - `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS` — integer ms (default 1000).
  - `SEFER_LARGE_CACHE_HEADROOM_BYTES` — bytes with K/M/G suffix (default
    256 MiB). The cache is allowed to hold up to this many bytes; only the
    excess above it is subject to decay.

- **Generalized `os::read_env_var_raw(name_nul, buf)`**: the allocation-free
  env-var reader is now parameterized on the variable name (NUL-terminated
  `&[u8]`). `read_env_budget_raw` is kept as a thin backward-compatible
  wrapper. This lets all three decay env parsers share the same reentrancy-safe
  pattern without duplicating the Windows/Unix platform dispatch.

- **Test seams** (`dbg_set_decay_config`, `dbg_force_decay_tick`,
  `dbg_decay_config`): deterministic test control without sleep or real
  wall-clock advances. `dbg_force_decay_tick` rewinds `last_decay_tick` by
  `decay_interval` and immediately invokes one decay step.

- **`tests/large_cache_decay.rs`**: 5 new tests covering excess release,
  headroom invariant, no-op when under target, interval guard, and env-var
  parsing.

### Changed — large-cache redesign Phase 1 (alloc-decommit)

- **Removed `MAX_CACHED_LARGE_BYTES`** (was 64 MiB per-span cap). Spans of
  any size can now enter the large-cache, removing the arbitrary ceiling that
  prevented caching of 100 MiB+ allocations.

- **Per-shard byte-budget admission** replaces the old per-span cap. A new
  `AllocCore::large_cache_budget_bytes: Option<usize>` field (under
  `alloc-decommit`) tracks the total bytes of all cached spans. When the
  budget would be exceeded, the oldest cached slot (FIFO: lowest index) is
  evicted to the OS before the new span is admitted. `None` = unbounded
  (default when the env var is not set).

- **`SEFER_LARGE_CACHE_BUDGET` environment variable** is read once at
  `AllocCore::new()` via a raw OS call (no heap allocation — safe even when
  `SeferMalloc` is the `#[global_allocator]`). Accepted formats: `"64M"`,
  `"2G"`, `"1024"` (raw bytes), etc. Parsed case-insensitively.

- **`large_cache_used_bytes` invariant counter**: maintained on every deposit
  and every eviction / cache hit. Verified by new tests via
  `dbg_large_cache_used()` / `dbg_large_cache_slot_sizes()` test seams.

### Removed

- **`byte` / `byte-sharded` features** — research-tier `ByteRegion` /
  `ByteAllocator` / `ShardedByteArena` removed. They were never expected to
  compete with mimalloc (see the BYTE_BENCH / BYTE_SHARDED_BENCH writeups in
  git history) and are fully superseded by the production stack (`alloc-global`
  + `alloc-xthread` + `alloc-decommit`). Old Phase 4 / Phase 7d log entries
  below are intentionally left intact as historical record.

### Deprecated

- **`experimental` concurrent regions** (`EpochRegion`, `LockFreeRegion`,
  `ShardedRegion`) — marked `#[deprecated]`. Superseded by the production
  `alloc-xthread` cross-thread free path. `PinnedRunner` is NOT deprecated.

### Summary

The initial public release.

**Pure Rust, no C / C++ libraries.** Unlike `mimalloc` (C++), `jemalloc`
(C), `snmalloc` (C++), `tcmalloc` (C++), or the typical `libnuma`-wrapping
NUMA crates, `sefer-alloc` is 100 % Rust — it calls into the OS directly
(`mmap` / `VirtualAlloc` / `mbind` etc.), but does not link a single C or
C++ library. The only C dependency in the repository is the optional
`mimalloc` dev-dependency used as a baseline in benchmarks (never on a
consumer's runtime path).

Two faces on one verified substrate:

- **`Region<T>` / `Handle<T>`** — a safe-by-construction handle store
  (default `std`, also `no_std` + `alloc`). `#![forbid(unsafe_code)]`
  at the top — the only `unsafe` is `slotmap`'s audited core wrapped
  by a typed membrane.

- **`SeferMalloc`** — a drop-in `#[global_allocator]` (opt-in
  `production` feature = `alloc-global + alloc-xthread +
  alloc-decommit`). Up to **~18× faster than `mimalloc` on cached
  large alloc/free** after the OPT-E large-cache (4 MiB cycle ≈ 45 ns
  vs ~718 ns ≈ **~16×**; 16 MiB ≈ 48 ns vs ~869 ns ≈ **~18×** — single
  Windows dev host, criterion `sample_size(10)`, see
  `docs/ALLOC_BENCH.md`); competitive with `mimalloc` on multi-thread
  cross-thread paths (`examples/malloc_macro.rs`). Confined-`unsafe`
  inventory under `production` (eight files): `alloc_core::{os, node}`
  + `global::{sefer_malloc, tls_heap, fallback}` +
  `registry::{heap_slot, heap_registry}`. `numa-aware` adds one more
  (`alloc_core::numa`). The crate is `#![deny(unsafe_code)]` (or
  `#![forbid]` in the default `std`-only build) and every `unsafe`
  block carries a `// SAFETY:` proof; compile-enforced.

Verification stack: 51 integration test files, 6 loom models
(`tests/loom_*.rs`), proptest differential vs reference model, miri
with strict-provenance (CI gate), ThreadSanitizer (×3 verified
clean on cross-thread + decommit), Valgrind memcheck clean,
aarch64 13/13 under qemu-user, libFuzzer (`region_ops`,
`global_alloc_ops`), soak / RSS / tokio-burn-in harnesses,
criterion benches with flamegraph profiling. Full details in
`docs/ARCHITECTURE.md` and `docs/ALLOC_BENCH.md`.

### Added

- **OPT-B (#67) — O(1) `SegmentTable::contains_base`**: a self-hosted
  open-addressing hash (2048 slots, 16 KiB in the primordial segment)
  replaces the O(count) linear scan. Tombstone encoding for removed
  entries keeps probe chains intact under recycle/decommit churn.
  Matters at DBMS scale (50–100+ live segments).
- **OPT-C (#66) — lazy `stamp_segment_owner`**: `HeapCore` caches the
  last-stamped segment base; cache-hit fast path is a single Relaxed
  load + ownership compare (no Release-store), skipping the costly
  MFENCE on 99 % of hot-segment allocations.
- **OPT-E (#65) — large-segment free-cache** (the headline win):
  1-2 fixed slots per `AllocCore` hold freed OS reservations; the
  next similarly-sized `alloc_large` reuses without mmap.
  **Measured: 4 MiB from 254 µs to 42 ns (~6,000× speedup, 18× faster
  than mimalloc 788 ns); 16 MiB from 701 µs to 48 ns.** Pages stay
  committed inside the cache (eliminates Windows
  `VirtualAlloc(MEM_COMMIT)` cost on hit). Bounded RSS at
  `LARGE_CACHE_SLOTS × MAX_CACHED_LARGE_BYTES = 2 × 64 MiB =
  128 MiB`. Gated on `alloc-decommit` for `SegmentTable` `unregister`
  consistency.
- **OPT-F (#64) — in-place small→small realloc**:
  `AllocCore::realloc` short-circuits when `new_size` resolves to the
  same or smaller size class as `old_size` — returns the same pointer,
  no copy, no alloc, no dealloc. Bench `realloc_in_place_unfavorable`
  improved 28.6 %.
- **OPT-G (#63) — `production` feature alias** + README guidance:
  `production = ["alloc-global", "alloc-xthread", "alloc-decommit"]`
  is the recommended set for long-running multi-thread workloads
  (DBMS, async runtimes); without `alloc-decommit` the
  `SegmentTable` slot-recycle path is disabled and the 1024-slot
  table is a hard ceiling.
- **NUMA-aware path** (Phases A–E of #58): opt-in `numa-aware`
  feature, default OFF. New confined-`unsafe` module
  `src/alloc_core/numa.rs` (Linux `mbind(2)` via `syscall(2)` —
  avoids `libnuma` dep — `MPOL_PREFERRED`; Windows
  `VirtualAllocExNuma`; macOS / miri no-op). Layout-stable
  `SegmentHeader::node_id` (present in every build).
  `reserve_small_segment` / `alloc_large` stamp the current thread's
  NUMA node; `find_segment_with_free` prefers local-node segments
  with foreign-node fallback. Tests: `numa_seam` (5),
  `numa_segment_id` (2), env-guarded `numa_alloc` (3, run with
  `SEFER_NUMA_TEST=1` under multi-NUMA topology). Honest caveat:
  QEMU verifies correctness, not latency-asymmetry; real measurement
  requires 2-socket hardware. See `docs/PHASE_NUMA_DESIGN.md`.
- **SegmentTable slot-recycle** (#60): under `alloc-decommit`, an
  empty decommitted segment NULLs its table slot for future
  re-registration, lifting the hard `MAX_SEGMENTS = 1024` cumulative
  ceiling. Found by the #52 tokio burn-in hitting OOM at >512
  concurrent tasks. New `recycle` (atomic NULL + `release_segment`)
  and partner `unregister` (NULL without release; used by OPT-E
  cache deposit).
- **strict-provenance miri fix** (#59): converted 11 sites of the
  `os::segment_base_of(ptr as usize) as *mut u8` idiom to the
  provenance-preserving `os::segment_base_of_ptr(ptr) =
  ptr.map_addr(|a| a & !(SEGMENT - 1))`. The CI miri job (which
  runs with `-Zmiri-strict-provenance`) now passes
  `decommit_miri_cycle` and `reclaim_offset_unit`.
- **Highload-hardening harnesses**:
  - `examples/soak_xthread.rs` (#51) — N-thread × hours stability
    test (32 / 64 / 128 workers); end-of-run invariant
    `total_alloc == total_free`.
  - `examples/rss_probe.rs` (#53) — measures peak / final RSS under
    sustained asymmetric cross-thread free; smoke: `alloc-decommit`
    keeps peak 13 % lower (91 → 79 MB).
  - `examples/tokio_burn_in.rs` (#52) — SeferMalloc installed as
    `#[global_allocator]` under tokio multi-thread runtime with a
    DBMS-pipeline-shaped workload.
  - `benches/large_realloc.rs` (#54) — three groups (large
    alloc+free, geometric realloc grow, realloc under neighbour
    pressure) comparing SeferMalloc, mimalloc, System through their
    `GlobalAlloc` traits.
- **Low-noise criterion benches** (#62): `benches/heap_xthread.rs`
  (direct ring push/drain, no channels) and
  `benches/heap_async_pattern.rs` (synthetic async-like pattern
  without tokio) — allocator visibility rises from 1.7 % to 13 % of
  self-time vs the noisier `global_alloc` / `large_realloc` benches.
- **Comprehensive verification runs** (one-off, evidence preserved
  in `docs/`):
  - ThreadSanitizer ×3 clean on `race_repro`, `race_norecycle`,
    `global_alloc_mt`, `heap_cross_thread`; ×3 clean on
    `decommit_stale_ring`, `decommit_soak`.
  - aarch64 (qemu-user 8.2.2) 13/13 tests pass, with honest caveat
    about TCG vs real ARM weak-memory.
  - Valgrind memcheck clean on three cross-thread test binaries;
    helgrind / DRD inapplicable to lock-free atomic code (known
    Valgrind limitation — TSan is the right tool).
  - Full Linux feature-matrix (6 combos × 248 tests) all green.
- **Documentation**:
  - `docs/ARCHITECTURE.md` — compact technical overview (synthesis
    of design memos).
  - `docs/PHASE_NUMA_DESIGN.md` (#55) — full NUMA design.
  - `docs/PROFILE_FLAMEGRAPHS.md` (#61) — flamegraph profiling
    report on 4 scenarios with 6 prioritised optimisation
    candidates (OPT-B/C/E/F/G all realised in this release; OPT-H
    documented but deferred as low impact).
  - `docs/ALLOC_BENCH.md` — extensive update with OPT-E large-cache
    numbers, NUMA section, honest verdicts.
- **OSS infrastructure** (preparing for crates.io publication):
  `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`.
  `Cargo.toml` metadata refreshed for crates.io (description
  mentions both faces, `keywords` rebalanced to `["allocator",
  "arena", "generational", "handle", "lock-free"]`, `categories`
  extended with `concurrency` and `no-std`, `repository` /
  `homepage` / `documentation` URLs added).
- **Build infrastructure**: `cargo-fuzz` metadata fix to enable
  `cargo fuzz build` (#56); `region_ops.rs` idiom corrected to match
  `arbitrary` 1.4.2 (#56); `malloc_macro` registered as
  `[[example]]` with `required-features` (was missing, causing CI
  `cargo test` without `--tests` to fail with E0601).

- **Phase 35 — M6 decommit: return empty segments to the OS** (behind a new
  opt-in `alloc-decommit = ["alloc-core"]` feature; **default OFF — the default
  build is byte-for-byte unchanged**). When a small segment's live-block count
  drops to zero and it is not the current carve target, its payload pages
  `[small_meta_end, SEGMENT)` are returned to the OS (`VirtualFree MEM_DECOMMIT`
  / `madvise MADV_DONTNEED`; no-op under miri) and the segment is reset to a
  clean blank (`bump = small_meta_end`, `BinTable` heads = NULL, payload
  page-map = Free, alloc-bitmap = 0, `decommitted` flag set); the payload is
  recommitted on the first reuse. This bounds steady-state RSS under churn (the
  one honest gap in `ALLOC_BENCH`). Bookkeeping: a new **owner-only** `u32`
  `live_count` field in `SegmentHeader` (present in every build's layout so the
  byte layout is stable; mutated only under the feature) — `+1` on
  `pop_free`/`carve_block` hand-out, `−1` on `dealloc_small`/`reclaim_offset`;
  refill blocks net to zero (carve `+1`, push-to-free-list `−1`). **No
  crossbeam-epoch / M11 barrier is needed** — Variant-2 (Phase 12.6) already
  removed the only reason the original plan reached for epoch: the cross-thread
  freer never dereferences the block (it pushes `offset|class` into the
  in-metadata `RemoteFreeRing`, and metadata pages are never decommitted). The
  full safety argument is recorded in code at the decommit point and in
  `docs/PHASE35_DECOMMIT_DESIGN.md` §1. A **post-decommit stale-free guard**
  (`off >= bump` after the reset) in both `dealloc_small` and `reclaim_offset`
  closes the window where a late free / double-free / stale ring entry targeting
  a reset segment would write a free-list `next` into a decommitted page. NO new
  dependency, NO new `unsafe` site (the OS seam already existed; the bookkeeping
  is plain safe arithmetic through the `node` seam). Tests (`alloc-decommit`):
  `decommit_soak` (decommit fires on `live→0` + recommit readback; counterfactual
  proven — the soak goes red if the hook is disconnected), `decommit_stale_ring`
  (stale ring entry into a decommitted segment is a no-op, no UAF),
  `decommit_miri_cycle` (bounded miri decommit/recommit bookkeeping). Verified:
  full suite green WITH and WITHOUT the feature (incl. `alloc_core_differential`,
  the heap suite, `race_repro`/`race_norecycle`/`global_alloc_mt`), clippy clean,
  miri on the bounded cycle. `heap_cross_segment`'s strict free-list-reuse
  invariant is relaxed under `alloc-decommit` to a bounded-footprint invariant
  (decommitted segments are legitimately re-carved, not free-list-reused).

- **Phase 12 — production multithreaded trust + Phase 12.6 cross-thread-free
  reclaim** (behind `alloc-xthread`). The installed `#[global_allocator]` is now
  a SOUND multithreaded drop-in: heap-as-shard isolation (each heap = a shard
  owned by one thread via a FREE/LIVE slot token), a self-hosted `HeapRegistry`,
  raw-pointer TLS with a never-null fallback heap, and loom-gated segment
  adoption (12.1–12.5). **Phase 12.6** closes the cross-thread-free
  *reclaim*: a non-intrusive per-segment MPSC ring carries each freed block's
  `offset | class` (the freer has the `Layout`; the owner's `page_map` class is
  unreliable for the mixed-class pages a shared bump cursor produces — the true
  root, found via ThreadSanitizer + a Linux free-list audit; NOT a data race).
  The owner reclaims lazily on its alloc-slow-path. This removes the Phase-12.5
  bounded-leak *discard* — cross-thread-freed blocks are now **reused**. Also
  fixed a real `SegmentHeader` data race (field-specific `bump`/`magic`/
  `owner_thread_free` accessors). Verified on Windows + Linux: `race_repro` ×5,
  `race_norecycle` (reliable Linux repro), isolated ring + reclaim unit tests,
  loom protocol/ring models with counterfactuals, full suite, clippy.
  See `docs/RACE_DRAIN_RECLAIM.md` (§13 root, §14 fix) and
  `docs/CROSS_THREAD_STATE_MACHINES.md` (the state-machine spec).
- **Phase 13.1 — O(1) size-class lookup** (`const SIZE2CLASS` table replacing the
  per-alloc linear scan).

- **Phase 11 -- the `malloc` face: `SeferMalloc` (`#[global_allocator]`) +
  no-panic hardening + honest mimalloc verdict** (behind a new opt-in
  `alloc-global = ["alloc"]` feature). `SeferMalloc` is an `unsafe impl
  GlobalAlloc` over the per-thread segment heap (one substrate, two faces: the
  typed `Handle` face and this raw `*mut u8` drop-in face), routing
  `alloc`/`dealloc`/`realloc`/`alloc_zeroed` through the no-panic TLS binding
  `with_heap_try` (returns null / no-ops instead of panicking — a panic in a
  global allocator aborts the process). **No-panic hardening:** the substrate's
  alloc-path panic sites were made graceful — the `alloc_small` `.expect` is
  gone, `SegmentTable::register` and `Segment::reserve` now return `Option`
  (null on failure, never `assert!`-panic). **Reentrancy-freedom (M5)** holds on
  the malloc path (no `Vec`/`Box`/`std::alloc`/`format!`). The `unsafe impl
  GlobalAlloc` is the documented malloc-face seam (every method `// SAFETY:`);
  `unsafe` stays confined. **Honest verdict (`docs/ALLOC_BENCH.md`):** on the
  alloc/dealloc hot path `SeferMalloc` is competitive with `mimalloc` (faster at
  1024 B and on realistic `Vec` push/grow churn; ~1.2-2x behind on small
  fixed-size churn) and consistently **~2.5-5x faster than the Windows system
  allocator** — safe by construction. Proven working as a real
  `#[global_allocator]` for a single-threaded workload
  (`examples/global_allocator.rs`: 100 k-`Vec` + 10 k-`HashMap`), and correct via
  direct-API tests (`tests/global_alloc.rs`: aligned, non-overlapping, reusable,
  realloc-prefix-preserving, 20 k churn). **NOT yet production-trusted:** as a
  *process-wide multithreaded* `#[global_allocator]` (e.g. under libtest's
  reentrancy-heavy harness) the current TLS binding returns null on
  reentrant/early-init/teardown access and aborts — a bootstrap-safe,
  reentrancy-tolerant TLS discipline is the remaining work, alongside the
  deferred heavy gate (`cargo-fuzz` CPU-hours, aarch64 multi-arch CI,
  ThreadSanitizer) and the Phase-10 deferrals (abandoned-heap adoption, M6
  decommit wiring). Honestly documented; for a process-wide allocator today, use
  `mimalloc`.
- **Phase 10 -- cross-thread free (M7), opt-in via `alloc-xthread`** (extends
  the `alloc` feature). Correct, lock-free cross-thread `dealloc` behind a
  new opt-in `alloc-xthread = ["alloc"]` sub-feature. When a thread frees a
  block it does NOT own, it pushes it onto the owning heap's atomic Treiber
  stack via a `compare_exchange` loop (the Phase-7b linearization protocol,
  re-based onto the Phase 8/9 segment substrate). The owner drains the stack
  in bulk on its next operation and returns each block to its per-class
  `FreeList`. O(1) owner lookup via `segment_base_of(ptr)` -> segment header
  -> `owner_thread_free` pointer (a stable `*const AtomicPtr<u8>` stored in
  each segment's header, pointing to the owning heap's `Box`-allocated Treiber
  head). The `ThreadFreeStack` is pure safe composition over
  `core::sync::atomic::AtomicPtr` + the `Node` seam (one new
  `Node::deref_atomic_ptr` in the existing `node` unsafe seam; no new unsafe
  module). **Thread-death soundness via abandonment-leak:** under
  `alloc-xthread`, `Heap::drop` intentionally LEAKS its segments (via
  `ManuallyDrop<AllocCore>`) and the Treiber head (via
  `ManuallyDrop<ThreadFreeStack>`) so that late cross-thread `dealloc` calls
  from other threads never touch unmapped memory or a freed `Box` -- segments
  stay mapped, the `AtomicPtr` stays allocated. This is a BOUNDED leak on
  thread death (one heap per thread), acceptable for the target long-lived
  thread-pool workload. Full abandoned-heap adoption (reclaiming leaked
  segments) is a Phase 11 deliverable. **Default `alloc` (no `alloc-xthread`)
  is unchanged Phase 9:** the single-thread-owner allocator with no
  `ThreadFreeStack`, no owner stamping, and normal segment release on
  `Heap::drop` (sound: single owner, no cross-thread refs). **Large / unstamped
  cross-thread free:** under `alloc-xthread`, a cross-thread free of a large
  block (`SegmentKind::Large`) or a block in an unstamped segment
  (`owner_thread_free == null`) is a documented no-op -- the block is
  conservatively leaked until the owning heap drops (or until Phase 11
  adoption). This avoids mis-accounting and is sound. **Decommit (M6) is NOT
  delivered** -- the `os::decommit_pages` / `os::recommit_pages` seam landed in
  Phase 10 (ready to wire) but is not integrated into the heap path. M6 is a
  Phase 11 deliverable. The soak test (`tests/heap_soak.rs`) asserts bounded
  segment growth via free-list reuse, not via decommit. Verification: **loom**
  model-check (`tests/loom_thread_free.rs`, 2 pushers + 1 drainer,
  `preemption_bound = 3`) with a proven counterfactual -- the naive non-CAS
  push demonstrably loses blocks under loom (the
  `counterfactual_naive_push_loses_blocks` test `#[should_panic]`s).
  Cross-thread differential proptest (`tests/heap_cross_thread.rs`, 64 cases,
  multiple threads, pattern write+readback -- non-vacuous). Soak test
  (`tests/heap_soak.rs`) -- bounded segment usage under sustained churn.
  Miri-clean on the cross-thread atomic seam (`tests/heap_miri_xthread.rs`,
  2-thread alloc/free, with `-Zmiri-ignore-leaks` for the intentional
  abandonment-leak).
- **Phase 9 -- per-thread heap + intrusive free lists (the lock-free fast
  path)** (behind a new opt-in `alloc` feature = `["alloc-core"]`). Each
  thread owns a `Heap` with per-size-class intrusive free lists stored inside
  the freed blocks themselves (via the Phase 8 `node` seam -- zero metadata
  allocation). The hot path (`alloc_small` / `dealloc_small`) is a single
  pointer read/write -- no lock, no atomic, no `Vec`/`Box`/`std::alloc` (M5
  reentrancy-freedom upheld). On free-list drain, a batch refill carves
  blocks from the Phase 8 `AllocCore` substrate. TLS heap binding via
  `std::thread_local!` with lazy, allocation-free init (`with_heap`); heap
  released on thread exit. Large/huge allocations route through the Phase 8
  dedicated-segment path. No new `unsafe` module -- the heap is pure safe
  composition over the Phase 8 `os` + `node` seams. Cross-thread free is
  Phase 10. Differential proptest (M1--M4 through the heap, 64 cases),
  targeted unit tests (alignment, reuse, refill, realloc, churn, multi-thread
  isolation), miri-clean. Single-thread throughput bench vs mimalloc and the
  system allocator (`benches/heap_alloc.rs`, `docs/HEAP_BENCH.md`): the heap
  matches the system allocator but is ~7--12x slower than mimalloc on the hot
  path; the architecture is structurally correct (same design as mimalloc) and
  the constant-factor gap is implementation overhead targeted for Phase 11.
- **Phase 8 — segment substrate + self-hosted metadata (the Membrane
  Inversion)** (behind a new opt-in `alloc-core` feature). The foundation of a
  real general-purpose allocator: the safe slot-table discipline stops
  *consuming* `Vec<T>` and starts *governing* OS-backed, SEGMENT-aligned memory
  (default 4 MiB), with the allocator's own metadata **carved from the segments
  it manages** (no `Vec`/`HashSet`/`std::alloc` on any alloc path). `unsafe`
  stays confined to exactly two documented seams: `os` (the OS aperture —
  `VirtualAlloc`/`VirtualFree` on windows, `mmap`/`munmap` on unix, via an
  over-reserve+trim for SEGMENT alignment; replaces `std::alloc` entirely) and
  `node` (the intrusive free-list node r/w, generalising the `hand` discipline).
  Everything between — `SegmentTable` (self-hosted generational registry),
  `PageMap`/`BinTable` (per-segment page descriptors + per-class free bins), the
  primordial `bootstrap`, the ~40-class size scheme, and `AllocCore`'s
  single-threaded `alloc`/`dealloc`/`realloc`/`alloc_zeroed` — is pure safe
  integer arithmetic (the Cartographer). Invariants **M1–M8** documented
  (`docs/INVARIANTS.md`, spec in `docs/ALLOC_PLAN.md` §4) and encoded as a
  differential proptest (M1–M4 vs a reference model), targeted unit tests, and a
  **runtime reentrancy audit (M5)** — a counting global allocator proves the
  alloc path never recurses into `std::alloc`. The core is **miri-clean**:
  because miri cannot execute the raw OS FFI, the `os` aperture has a
  `#[cfg(miri)]`-only fallback to `std::alloc` (test instrumentation; the
  production aperture is unchanged and the M5 proof runs without miri). Single
  confined unsafe per seam; `forbid`/`deny(unsafe_code)` everywhere else.
  **Supersedes** the Phase-4 `byte_region.rs` `std::alloc` fallback and its
  `Vec`/`HashSet` metadata. Per-thread heaps (Phase 9), cross-thread free +
  decommit (Phase 10), and the `GlobalAlloc` face (Phase 11) build on this.
- Initial scaffold of the `sefer-alloc` crate.
- Single-threaded `Region<T>` — a thin typed membrane over the
  [`slotmap`](https://crates.io/crates/slotmap) crate (`insert` / `get` /
  `get_mut` / `remove` / `contains` / `iter` / `clear`, all `O(1)`), built under
  `#![forbid(unsafe_code)]`; `slotmap`'s audited `unsafe` owns the dense
  generational engine, including version-saturation slot retirement.
- Typed, copyable `Handle<T>` — a newtype over `slotmap::DefaultKey` with
  hand-written `Copy`/`Eq`/`Hash`/`Debug` impls that hold for every `T`.
- `SyncRegion<T>` — the always-shippable concurrent baseline: a
  `RwLock<Region<T>>` with a guard API plus one-shot convenience methods, with
  poison recovery, still `#![forbid(unsafe_code)]`.
- `LockFreeRegion<T>` (behind the opt-in `experimental` feature) — **lock-free
  reads** via `arc-swap` RCU with page-granularity copy-on-write: readers load
  an immutable snapshot and resolve handles without any lock; rare writers
  serialise, copy only the touched page, and publish atomically. Values live
  behind `Arc<T>`; reclamation is plain `Arc` refcounting. **Zero `unsafe` of
  our own** — the crate stays `#![forbid(unsafe_code)]` with the feature on.
- `EpochRegion<T>` (behind `experimental`) — the fixed-capacity epoch tier with
  O(1) per-slot writes: lock-free reads via a seqlock-validated
  `(generation, value)` publication protocol and `crossbeam-epoch` reclamation.
  Introduces the crate's **single confined `unsafe` organ** (`concurrent::hand`,
  `AtomicSlot<T>`); confinement is compiler-enforced (`#![deny(unsafe_code)]`
  crate-wide under the feature, lifted only in that one module). The publication
  protocol is **loom-model-checked**; live values are dropped on region drop
  (I5). miri cannot run the tier only because `crossbeam-epoch`'s global
  collector is not miri-clean upstream — our `unsafe` is not implicated.
- `ShardedRegion<T>` and `ShardedHandle<T>` (behind `experimental`, Phase 7a) —
  **N-way parallel writes** via the single-writer principle: a `Box<[EpochRegion]>`
  of shards plus a thread-local router that lazily binds each writer thread to one
  shard (atomic round-robin), so two writers in different shards never meet on a
  lock. Reads stay the untouched lock-free `EpochRegion` seqlock. **Pure safe
  composition — zero new `unsafe`**; the module compiles under the crate's
  unsafe-confinement. `ShardedHandle` carries the shard id so reads/removes route
  back to the owning shard. Honest 7a edge: a claimed shard is not released
  (fits a bounded pool of long-lived threads; the shard lifecycle + lock-free
  cross-thread remove land in 7b). A multi-shard differential proptest (I1–I4
  across shards) and a routed concurrent stress test guard it; a write-scaling
  bench (`benches/sharded_write.rs`) compares it to the `SyncRegion` / `Arc<Mutex>`
  baselines.
- **Phase 7b — lock-free cross-thread removal + shard lifecycle** (behind
  `experimental`). A non-owner thread can now `remove` a handle WITHOUT taking
  the owning shard's writer mutex: `AtomicSlot::try_evict_at` performs a
  generation **`compare_exchange`** as the single linearization point — exactly
  one thread wins per generation, so exactly one schedules `defer_destroy` and
  decrements the (now `AtomicUsize`) live count (no double-free, no
  lost-live-value). The freed index is enqueued to a per-shard remote-free queue
  the owner drains on its next op (free list stays owner-only). `EpochRegion`
  gains `remote_evict`; `ShardedRegion::remove` routes owner-path vs lock-free
  remote-path by the calling thread's shard. Shards are now **releasable**: a
  thread-local `Drop` guard frees the shard's `occupied` token on thread exit,
  so a dead thread's shard can be adopted by a new thread while its live slots
  stay resolvable (reads are ownership-free). The relaxed "any thread may evict"
  contract is **loom-model-checked** (`tests/loom_sharded.rs`, 1 owner + 1
  remote-remover + 1 reader, `preemption_bound = 3`) — verified to FAIL on the
  naive load-then-swap protocol. `unsafe` stays confined to `concurrent/hand.rs`.
- **Phase 7c — thread-per-core pinning** (behind a new opt-in `pinning` feature
  = `["experimental", "dep:core_affinity"]`). `ShardedRegion::bind_current_thread_to_shard`
  deterministically routes a thread to a specific shard (the auto round-robin
  claim cannot), and `PinnedRunner` spawns one worker per core, pins worker *i*
  to core *i* (via `core_affinity`, a safe wrapper — **zero new `unsafe`**), and
  binds it to shard *i* — so `shard == core` and the hot path has no lock and no
  cross-shard contention (also why it composes with `glommio`/`monoio`/`tokio`
  current-thread-per-core without "lock across `.await`"). `core_affinity` is an
  **optional** dependency: the default and `experimental` builds do not pull it.
  Pinning is best-effort (honoured per OS); the shard binding (the routing
  truth) always holds, so tests assert routing, not affinity. A `pinned_write`
  bench compares pinned vs unpinned with an honest, workload-dependent verdict.
- **Phase 7d — `ShardedByteArena`** (behind a new opt-in `byte-sharded` feature
  = `["byte"]`, research-flagged). N per-thread `ByteRegion` shards
  (`Box<[Mutex<ByteRegion>]>`) for parallel raw allocation: a thread binds to its
  own shard via a TLS round-robin router, so threads in different shards never
  contend on one lock. Cross-thread `dealloc`/`realloc` route to the owning shard
  via a scan over `ByteRegion::contains_ptr` (safe pointer-comparison, no
  dereference) — a pointer is never freed against the wrong shard. `prewarm()`
  carves a chunk per shard and touches its pages up front to remove cold-start
  latency (callable from a background thread; the arena is `Send + Sync`). The
  only added `unsafe` is a one-line `unsafe impl Send for ByteRegion` (the region
  owns all its memory; access is `Mutex`-serialised) — everything else is safe
  composition; `unsafe` stays confined to `src/byte/*`. Correctness (cross-thread
  free, concurrent per-shard churn, bounded chunk growth, realloc byte
  preservation) is covered by `tests/byte_sharded.rs` and is **miri-clean**.
  Honest verdict (`docs/BYTE_SHARDED_BENCH.md`): it parallelises across shards
  but is NOT a `mimalloc` competitor and never returns memory to the OS until
  drop — research, not production.
- `ByteRegion` and `ByteAllocator` (behind the research-flagged `byte` feature)
  — the descent to raw bytes: a size-classed free-list byte arena whose
  placement logic is pure safe integer arithmetic (the Cartographer), with the
  single irreducible `*mut u8` aperture confined and documented, plus an
  experimental `unsafe impl GlobalAlloc` delegating through a `Mutex`. The
  second confined-`unsafe` module; confinement stays compiler-enforced. The
  whole byte tier is **miri-clean**. Honest scope: it does not aim to beat the
  system allocator / `mimalloc` (see `docs/BYTE_BENCH.md`); resocks5's global
  allocator stays `mimalloc` regardless.
- Safety invariants I1–I5 documented (`docs/INVARIANTS.md`) and encoded as
  unit tests plus a proptest differential harness against a reference model
  (`tests/differential.rs`).
- Full detailed implementation plan — per-phase goals, deliverables, steps, and
  gates, plus dependency DAG, risk register, decisions log, and open questions
  (`docs/PLAN.md`) — alongside architecture notes (`docs/DESIGN.md`).
- Dual MIT / Apache-2.0 licensing; MSRV pinned to 1.88.
