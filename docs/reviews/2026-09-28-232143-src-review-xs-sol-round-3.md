# Independent `src/` review — round 3

Time: 2026-09-28 23:21:43 Europe/Berlin (CEST, UTC+02:00). Reviewed tree: `a3a3a003d2e3211c49ad5fd8cd9ba163c5b747e6`, branch `review/src-xs-round3-20260928`, worktree `worktrees/src-review-xs-20260928-r3`. This is a fresh review of the checked-out source, not a digest of earlier reviews.

## Verdict

**GO for the ordinary `production` allocator surface on the evidence inspected**, not a formal safety certification. No P0/P1/P2 defect was confirmed. The two P3 findings are confined to the deprecated `experimental` sharded region and an `internals`-exposed test ring; neither is a demonstrated failure of contract-respecting `production` allocation/free. Consumers that require an exact concurrent empty check from `ShardedRegion` should treat that part as **NO-GO until its semantics are clarified or changed**.

## Confirmed findings

### P0 / P1 / P2

None confirmed. Absence of a finding is not proof of absence, particularly for weak-memory and allocator lifetime interleavings.

### P3-1 — `ShardedRegion::is_empty` can report empty when the region was never empty

`src/concurrent/sharded/sharded_region.rs:264-271` sums separate shard counters for `len()` and tests them sequentially for `is_empty()`. The method docs at lines 258-271 call the result a total live-entry count / momentary observation and claim I4 accounting at lines 92-93. No cross-shard snapshot or common counter makes these reads a single moment.

Reachable schedule with two shards: initially shard 0 is empty and shard 1 has one live entry; reader observes shard 0 as empty; writer inserts into shard 0 and only then removes the entry from shard 1; reader observes shard 1 as empty. `is_empty()` returns `true` (and `len()` can return `0` under the analogous schedule), although at every instant at least one entry was live. A caller using the boolean to stop draining or shut down can skip live work. This requires concurrent mutation, `experimental`, and the already-deprecated `ShardedRegion`; single-threaded accounting is unaffected. Either document the methods as non-linearizable/approximate under mutation, or implement a genuinely coherent observation if the claimed momentary/exact semantics are required.

### P3-2 — a safe `HeapOverflow::push(null, ...)` poisons the ring in release builds

`src/registry/heap_overflow/push.rs:52-54,154-158,204-219` exposes a safe `push`; its only null-base check is `debug_assert_ne!`. A safe caller can obtain a standalone ring through `HeapOverflow::new_boxed_for_test` (`src/registry/heap_overflow/heap_overflow_impl.rs:275-290`) when `internals` is enabled (`src/lib.rs:466-473`). In a release build, `push(null_mut(), packed)` reserves a tail position and returns `true`, then publishes the same null value used as `ENTRY_EMPTY_BASE`. `try_drain` stops at that position (`src/registry/heap_overflow/drain.rs:264-275`); subsequent valid entries cannot pass it. This is a persistent logical wedge, not a proven production-path memory-safety issue: real allocator callers supply a live non-null segment base, and the standalone constructor is documented as test-only. The safe API should reject null in every build before reserving a cursor (or express the precondition in an unsafe boundary if there is a reason not to check it).

### P4-1 — the `HeapCore::dealloc` summary contradicts its pointer safety contract

The public method's summary says foreign pointers are a “safe no-op” at `src/registry/heap_core/free/dealloc.rs:183-190`, while its `# Safety` section explicitly excludes foreign/unmapped pointers at lines 197-213. Under `alloc-xthread`, an unrecognised non-null address that masks to a non-null segment base reaches `SegmentHeader::magic_at(base)` at `src/registry/heap_core_xthread/routing.rs:180-183`; the magic check itself reads that address and can fault if it is unmapped. This is a documentation defect, not a claim that valid callers trigger UB. A reader following the summary instead of the formal contract could make an unsafe invalid call expecting a harmless no-op. Remove or qualify the summary's blanket claim.

## Unconfirmed hypotheses / boundaries

- The `RemoteFreeRing` and `HeapOverflow` cursor-publication, wrap and unwind protocols are substantial concurrency proofs (`src/alloc_core/segment/remote_free_ring/ops.rs`, `src/registry/heap_overflow/{push,drain}.rs`). Static reading found no additional concrete counterexample; this review did not execute Loom, Miri or a weak-memory model, so it cannot certify those protocols.
- The allocator's magic-header defensive checks are not a general validation of arbitrary raw pointers. `GlobalAlloc` and the internal unsafe free/realloc contracts already exclude stale, unmapped and foreign allocations; those cases are not counted here as implementation defects merely because a defensive read can fault.
- Per-heap cache retention and event-driven decay described in `README.md` are intentional configuration trade-offs. I did not turn them into unmeasured leak or RSS claims.

## Optimization ideas — no speedup claimed

- `LockFreeRegion` clones the snapshot page table and a full slot page on successful writes (`src/concurrent/lock_free/lock_free_region.rs:356-370,431-464,501-519`). A write-heavy benchmark could compare a more granular persistent structure; it would trade implementation complexity and read costs for fewer copies. No A/B measurement was run.
- `ShardedRegion::len` and `is_empty` inspect all shards (`src/concurrent/sharded/sharded_region.rs:264-271`). A shared count might make the query O(1), but introduces an RMW/contention point and alone does not solve snapshot semantics. Measure query-heavy and write-heavy workloads before changing it.
- The fallback heap uses a global lock and yields after a bounded spin (`src/global/fallback.rs:447-461`). Under registry exhaustion it may serialize allocation; this is a workload hypothesis, not an observed bottleneck. No synthetic load was generated.

## Method and limitations

I inventoried and mechanically scanned **all 145 Rust files under `src/`** (46,078 physical lines; approximately 2.52 MB), including unsafe/raw-pointer entry points, panics/assertions, wrap/saturating arithmetic, atomics, and unfinished-work markers. I then read and cross-checked the relevant implementations across `alloc_core`, `concurrent`, `global`, and `registry`, with deeper attention to allocation/free/realloc, TLS/fallback lifecycle, segment hashing, large-cache allocation, remote-free and overflow rings, batch free, and the experimental region implementations. I checked the applicable public-contract context against `README.md`, `docs/ARCHITECTURE.md`, `docs/INVARIANTS.md`, `CLAUDE.md`, and feature gates in `Cargo.toml`; the architecture overview is dated and was not treated as stronger than current code. This is **not** a claim that all 46,078 lines were manually proved, nor an audit of companion crates in `crates/`.

Read-only review only: no source edits, tests, benchmarks, build, model checking, runtime reproduction, or platform-specific OS validation. P3-1 is a source-level interleaving; P3-2 is a direct release-configuration control-flow consequence. No latency or throughput benefit is asserted without measurements.
