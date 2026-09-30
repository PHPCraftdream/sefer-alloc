# src review, round 11 (read-only)

Author: Sol-Codex
Time: 2026-09-30 23:27:02 Europe/Berlin (CEST, UTC+02:00); 2026-09-30 21:27:02 UTC
Source: branch `review/src-xs-round11-20260930`, immutable base/HEAD `2cbf515aa39b33da2c2b3eb616af05aace5f5e69`
Disposition: review only; no source edit, build, test, Miri, Loom, benchmark, project script, commit, or push.

## Verdict and scope

This is a static, adversarial code review, not a formal soundness certificate. I inventoried all 157 Rust files in `src/` (40,085 lines): `alloc_core` 85, `registry` 45, `global` 12, `concurrent` 13, and `lib.rs`/`kani_proofs.rs` 2. The confirmed findings below are 1 P2, 3 P3, and 3 P4; no P0 or P1 was established. P3/P4 performance entries establish code-level work or storage complexity, not a measured speedup from any proposed replacement.

The deep trace followed `SeferAlloc`'s `GlobalAlloc` alloc/dealloc/realloc/alloc_zeroed and `batch-api` paths through TLS binding, fallback, heap claim/reuse, own/foreign routing, magazine/refill/flush, `AllocCore` small/Large reserve and release, and zeroing/virginity. It also followed maintenance startup, lease and bounded ingress, plus the segment table/hash/active-kind index, route registration/pins, Small/Large sidecars and credit transitions, large cache/decay, directory, NUMA and lazy-commit gates. The `experimental` epoch, sharded, RCU and pinning APIs were inspected separately. `Cargo.toml`, `Cargo.lock`, `README.md`, `CLAUDE.md`, and `docs/INVARIANTS.md` were read only for reachability and contract. No previous review, checkpoint, open-items index, or other agent's conclusions were used as evidence; historical comments in source and required project instructions were not treated as proof.

The source is too large for this pass to establish a line-by-line proof of every unsafe site, feature combination, OS backend, and interleaving. The adjacent workspace crates were not internally audited. `async`-specific review rules had no direct source target: `src/` has no async function or `.await` path. The observations below are bounded to the stated feature/API scenarios. A `GlobalAlloc` caller's double-free, invalid pointer/layout, or use after free, a multithreaded-fork child using the allocator before exec, and host OOM are outside their documented contracts; none is counted merely for existing. All proposed witnesses are designs, not executed results.

## Confirmed findings

### P2-1. A completed remote removal can leave a capacity-one shard reporting full

Evidence: `src/concurrent/epoch/epoch_region.rs:339-343` skips the remote queue on a `remote_free_pending.load(Relaxed) == false`; `:431-433` then returns `Err(value)` when the local free list is empty. The remote path pushes an index under `remote_free`'s mutex and only afterward stores `true` with `Relaxed` at `:576-590`. `src/concurrent/sharded/sharded_region.rs:475-480` reaches that path for a remover without the owning shard binding. The owner does not lock `remote_free` when it reads the negative flag, so the mutex unlock supplies no synchronization on that path. Rust's relaxed operations do not establish happens-before across distinct atomics (Rustonomicon, https://doc.rust-lang.org/nomicon/atomics.html#relaxed).

Reachable scenario: build with `experimental`; create `ShardedRegion::<T>::with_shards(1, 1)`. Thread A inserts its sole value and retains the shard binding. Unbound thread B removes the handle remotely, returns, and signals completion through an unrelated `AtomicBool` using `Relaxed`. A observes that relaxed completion signal, then calls `insert` once. The memory model permits A to observe the completion signal while still reading the old `false` from `remote_free_pending`: there is no release/acquire edge between those two locations. A skips the already-enqueued index and returns `Err(T)` although no value is live and the reusable slot is in the queue. If that was the application's only retry, capacity remains unavailable until a later owner operation. This is a semantic false-full result, not a data race or UB; a normal thread `join` creates synchronization and can conceal it in tests.

Fix: make the flag advisory only. Before returning `Err` for an empty local free list, acquire and inspect/drain the remote queue under its mutex, then retry the free-list pop. The mutex acquisition closes the completed-push case even when the flag load was stale; a producer that has not enqueued yet still overlaps the insertion and can legitimately be ordered after it. Changing only the flag load to `Acquire` does not force a negative load to observe a store it did not read.

Negative/regression witness: model the exact queue/flag protocol with two workers and a separate relaxed completion signal; assert that an insertion started after it observes completed `remote_evict` succeeds in the capacity-one shard. Force the `done == true, pending == false, queue nonempty` weak-memory outcome. The present negative-hint branch returns `Err`; a fixed empty-free-list queue check succeeds. The existing join-based remote tests do not provide this negative control. No model was run in this review.

### P3-1. Every Small magazine refill scans all live Large routes

Evidence: `src/registry/heap_core/alloc/hot.rs:754-760` calls `drain_large_sidecar_ingress()` before a Small refill; its virgin-zeroed sibling does the same at `:570-576`. `src/alloc_core/alloc_core/sidecar_drain.rs:173-196` iterates every active Large slot and tests its route, even when none has a pending publication. The active-kind index makes enumeration proportional to active Large entries, not the entire high-water table; it does not make the scan pending-only.

Reachable scenario: `production` (or `alloc-global,fastbin,alloc-xthread`); keep L Large allocations live and perform K Small allocations at a class/refill cadence that empties the magazine K times, with no remote Large frees. Each refill still inspects L Large routes, so the extra route work is Theta(K*L). For a class whose refill byte budget admits one block, this approaches Theta(L) per Small allocation. There is no demonstrated wall-clock speedup claim here; the asymptotic inspection count follows from the loop and call sites.

Fix: consider a pending-Large index or a bounded dirty/round-robin scheme that preserves the terminal credit and late-publication guarantees; retain a full sweep at a cold/maintenance boundary as a safety net. Do not replace the sweep with a lossy hint without a publication/retirement proof.

Negative/regression witness: under `bench-internals`, hold L=8 then L=64 Large reservations with no pending frees, force the same K Small refill misses, and read the existing `LARGE_SIDECAR_SLOT_INSPECTIONS` counter (`src/alloc_core/alloc_core/sidecar_drain.rs:8,186`). Today the delta is K*L in both cases; a pending-only improvement should keep the no-pending delta bounded independently of L. This is a deterministic work-count gate, not a timing assertion.

### P3-2. Each Small/Primordial route allocates and zeroes 288 KiB of independent metadata

Evidence: `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:17-18` fixes 262,144 granules and 4,096 pending words for a 4 MiB segment with 16-byte granules. `src/registry/segment_route/small_sidecar.rs:12-15` stores one `AtomicU8` class per granule and one `AtomicU64` per pending word. `src/registry/segment_route/directory.rs:53-57` uses `System.alloc_zeroed(Layout::new::<SmallSidecar>())` on every Small or Primordial route registration; `src/alloc_core/segment/segment_table/route_slots.rs:19-32,73-91` makes those registrations part of routed heaps.

The exact type footprint is 262,144 + 4,096*8 = 294,912 bytes = 288 KiB per route, 7.03125% of the 4 MiB segment size, before directory entries and pointer-array capacity. A heap with 1,024 live Small segments therefore holds 288 MiB of these System-backed sidecars alone. Even a newly materialized heap pays one for its Primordial route. This is a confirmed memory/zeroing amplification under `alloc-global` (including `production`), not an unbounded leak: the sidecar is released after unlink and the last pin drops.

Fix: evaluate a sparse or lazily populated class map, or a packed representation, while preserving allocation-free remote publication and the outstanding-credit lifetime. Any replacement needs an independent memory-versus-CPU benchmark; this finding establishes only the current storage cost.

Negative/regression witness: expose the actual route-sidecar byte count in an `internals` diagnostic and create N live Small routes using valid allocator reservations. Assert a chosen per-route budget (for example <=64 KiB for a compressed design) and that dropping all registrations returns the count to zero. The current 294,912-byte type fails the budget, while the release assertion prevents a compressed-but-leaking replacement from passing. A bare `size_of` self-equality test would be vacuous.

### P3-3. Cold heap claims scan all prior live slots before minting a new slot

Evidence: `src/registry/heap_registry/stack.rs:13-33` linearly examines `0..count` for a claimable slot; `:88-102` invokes that scan before the fresh-index `bump()` even when `count < max`. `src/registry/heap_registry/claim.rs:314-323` uses this selector on TLS first bind. `MAX_HEAPS` is 4,096 at `src/registry/bootstrap/registry.rs:45`.

Reachable scenario: `alloc-global`/`production`; N worker threads stay live behind a barrier, each doing its first allocation once. There is no FREE slot and the high-water count is i before claimant i+1. The scan reads i occupied states before bumping; N claims perform the triangular sum N*(N-1)/2 (8,386,560 candidate visits at N=4,096), in addition to materialization. This is a cold-start/scaling cost, not a claim about measured startup time. Recycled-slot preference and recoverable unmaterialized indices are valid requirements; the repeated scan is the avoidable part.

Fix: maintain an explicit claimable-slot index/bitmap (with its state CAS still authoritative), or take the fresh high-water path without a full scan while capacity remains and use a separate O(1) recycled-slot hint/queue. Preserve retry of a chunk whose first materialization failed and the maintenance lease state.

Negative/regression witness: with N live, first-touch claimants and no recycle, count `slot_if_materialised` candidate probes as N grows (e.g. 16, 32, 64) and assert a linear upper bound after the redesign. The current scan-before-bump path yields the triangular count and fails; merely observing that all claims succeed would not catch this complexity regression.

### P4-1. Sorted route arrays make one-shard registration/removal waves quadratic

Evidence: `src/registry/segment_route/directory.rs:298-316` shifts every later pointer on insertion/removal. `:427-457` places registration in this array under one shard mutex, and `:496-506` removes under the same mutex. Binary-search lookup at `:266-285` is O(log n), but insertion or removal at the front of a shard with n routes moves n pointers.

Reachable scenario: `alloc-global` or the `internals` route API; n valid Large routes hash to one shard, then their keys are registered in descending order, or removed in ascending order. The wave copies n*(n-1)/2 pointer cells under the mutex. The OS chooses normal allocation addresses, so this is a worst-case arrangement rather than a measured production distribution. It can extend cold registration/retirement latency and hold the shard lock while unrelated foreign frees targeting that shard wait.

Fix: if route churn is important, evaluate a tree/hash index or chunked sorted blocks per shard, retaining the counted-pin lifetime and lookup-before-unlink protocol. The present sorted array may remain preferable for lookup-heavy workloads; benchmark that trade before replacing it.

Negative/regression witness: register real, page-aligned Large roots chosen to share a shard in reverse-key order; instrument moved pointer cells, not elapsed time. Current moves are quadratic. A candidate with subquadratic updates must pass the same lookup, duplicate-registration, pin-after-unlink and final-free contract cases.

### P4-2. Experimental RCU writes clone the entire page-pointer table

Evidence: `src/concurrent/lock_free/lock_free_region.rs:107-113` clones `pages: Vec<Arc<Vec<Slot<T>>>>`; successful insert at `:362-366` and successful remove at `:408-440` both clone the snapshot while holding the writer mutex. The touched page is separately cloned, but the pointer-table clone still visits every page. `PAGE` is 64 slots at `:37-41`.

Reachable scenario: `experimental`, `LockFreeRegion::with_pages(P)` with P preallocated pages (or growth to P), followed by M successful writes even if all mutations concern one page. Each write is Theta(P+64) for table-plus-page copying, hence Theta(M*P) table-pointer work; growing from empty to N slots totals Theta(N^2/64) table-pointer clones. This is a confirmed asymptotic cost, not a measured regression, and the type is explicitly legacy/read-mostly.

Fix: retain the read-mostly contract and steer write-heavy users elsewhere, or evaluate a persistent/chunked page table where one write copies O(log P) nodes rather than all P pointers. Do not trade the current lock-free read path away without a workload-specific gate.

Negative/regression witness: count page-`Arc` clones for one successful insert and remove with P=16 and P=256, keeping the touched slot/page identical. Today the count scales with P; an intended persistent-table change should make it sublinear while preserving stale-handle and drop-once tests. A timing threshold on shared CI would be a weak witness.

### P4-3. Experimental shard binding is keyed by thread, not by region instance

Evidence: `src/concurrent/sharded/sharded_region.rs:173` has one `MY_SHARD` TLS cell for all `ShardedRegion<T>` values. `:310-320` trusts an in-range cached numeric shard id without checking which region supplied it. `:475-480` uses that same id to choose the owner-removal path. The module header documents a one-region-per-thread-pool design assumption, so this is a confirmed architecture/locality limitation, not a claimed memory-safety error.

Reachable scenario: `experimental`; on thread A, insert into region A with two shards, caching shard 0. Thread B exclusively claims region B's shard 0. A now inserts into B, also with two shards. It reuses numeric id 0 without attempting B's free shard 1, so two writers share B's shard 0 and its writer mutex. A later removal of a B/0 handle by A takes the owner path even though B's shard-0 token belongs to thread B; the mutex/CAS protocol keeps it correct, but the intended noncontending remote route is skipped. Equal shard counts make the range check ineffective.

Fix: key TLS binding by region identity (with bounded per-thread storage/lifecycle), or make a region-specific binding token explicit in the API. If the single-region topology is deliberately retained, surface that limitation at the relevant methods rather than relying on the module-level note to prevent mixed-instance use.

Negative/regression witness: use two two-shard regions and two threads with the bindings above; assert that B's free shard 1 can be claimed by A, and that A's removal of a B/0 handle follows the remote path. Today the returned B handle has shard 0 and the local path is chosen. Instrument path selection, not wall-clock speed; `get`/`remove` correctness alone would stay green in both versions.

## Unconfirmed hypotheses and unmeasured ideas (not P-graded)

- `src/registry/heap_core/state/ownership.rs:118-142` bounds only ingress cuts in `background_maintenance_step`; it then runs full cold-retention trim. In a heavily populated fallback pool, a worker visit may still perform many OS releases. No maintenance latency SLA is promised, no representative pool occupancy or syscall cost was measured, and the normal thread-recycle path trims before a FREE-heap lease; this is a measurement/design question, not a confirmed liveness defect.
- A smaller Small route sidecar could exchange memory for more atomics, indirections, or remote publication work. The 288 KiB footprint is established above; the best representation and any throughput/RSS win are not.
- A pending-Large hint or queue must not lose a paused publisher or release a reservation before its terminal credit transfers. The O(L) scan is established above; a safe replacement protocol has not been proved here.

## Limits and handoff

No tests, builds, Miri, Loom, benchmarks, scripts, dynamic fault injection or load were run, per request. The P2 result is a static memory-order/protocol argument with a proposed negative model witness, not an observed production failure. The P3/P4 costs are exact source-level asymptotics or type-size arithmetic, not wall-clock speedups. Platform-specific behavior in `aligned-vmem`, `numa-shim`, `once-ptr-cell`, `tagged-index-stack`, `size-classes`, and `sefer-region` was not audited beyond the contracts needed to trace this `src/` call graph. There was no version-bump review. No source was changed. A follow-up fixing any finding should run only its directly relevant tests and negative controls in the appropriate feature set; this report alone does not certify the allocator.

Confirmed counts: P0=0, P1=0, P2=1, P3=3, P4=3.
