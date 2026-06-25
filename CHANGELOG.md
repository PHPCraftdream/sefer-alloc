# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
  `unsafe` stays confined. **Honest verdict (`docs/MALLOC_BENCH.md`):** on the
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
  (`docs/INVARIANTS.md`, spec in `docs/MALLOC_PLAN.md` §4) and encoded as a
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
