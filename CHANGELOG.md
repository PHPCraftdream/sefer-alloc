# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
