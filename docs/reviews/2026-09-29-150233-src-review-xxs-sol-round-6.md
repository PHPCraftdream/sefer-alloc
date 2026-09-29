# Independent src review: xxs / Sol 6 Max / round 6

Review time: 2026-09-29 15:02:33 Europe/Berlin (CEST, UTC+02:00).
Base and reviewed HEAD: `35eec665bb4626176f2ade7f137266e3fee303e5`.
Branch: `review/src-xxs-round6-20260929`.
Scope: all 146 Rust files under `src/` (45,960 physical lines). This is a fresh source-only review. Previous review reports were not used as evidence. I inspected the module/API surface and panic/unsafe markers across the tree, then traced the allocator ownership, TLS, free/realloc, ring/overflow, segment, cache, NUMA-directory, and experimental-region paths in detail. Companion crates are outside this scope.
Method limit: no tests, builds, benchmarks, project scripts, fuzzing, Miri, loom, stress simulation, or runtime validation were run. Findings below are static code-path deductions, not measured outcomes.

## Decision

NO-GO for an unconditional memory-bounded production claim. A valid cross-thread free that arrives after its owner has recycled a heap slot has no guaranteed reclamation event (R6-01). No P0 or demonstrated memory-safety violation was found, but a static-only review cannot establish memory safety or performance across all feature/OS combinations. A deployment that explicitly accepts process-lifetime retention of ownerless pending frees has a narrower, conditional GO only after that policy is documented and resource limits are assessed.

Confirmed findings: P0=0, P1=1, P2=2, P3=1, P4=1.

## Confirmed findings

### R6-01 [P1] Late remote frees can pin ownerless segments for the process lifetime

Evidence: `src/global/tls_heap.rs:239`, `src/global/tls_heap.rs:303`, `src/registry/heap_registry/claim.rs:291`, `src/registry/heap_core_xthread/routing.rs:245`, `src/registry/heap_core_xthread/routing.rs:297`, `src/registry/heap_core/alloc/hot.rs:254`, `src/alloc_core/small/alloc_core_small/find_segment.rs:742`.

Reachable scenario: thread A allocates blocks, transfers them to B, and exits. Its guard drains the large-deferred stack once, then recycles the whole slot. B subsequently frees A's still-live blocks. A large free is enqueued on the slot's deferred stack; a small free is enqueued in its per-segment ring/overflow. With no new claimant of that particular slot, there is no writer to run the owner-only drain. Later allocations by existing threads on other slots do not drain A's slot. A subsequent claimant might reclaim the memory, but neither its arrival nor its choice of this slot is guaranteed.

Consequence: valid frees leave mapped segments and segment-table capacity retained despite no live user allocation. The retained footprint scales with past owner slots and their reservations; on a long-running service it can last until process exit and cause resource exhaustion. Whole-slot reuse makes this recoverable in some schedules, not bounded in all schedules.

Fix direction: provide an ownership-safe reclamation route for pending frees on FREE slots (for example an exclusive scavenger claim or a coordinated claim/drain protocol), including in-flight producer publication and concurrent new claimers. Specify a bound or an explicit retention policy. Do not let a foreign thread mutate a slot's non-atomic `HeapCore` directly.

### R6-02 [P2] Fallback-owned frees from unbound threads take a lazy remote path, including same-thread frees

Evidence: `src/global/tls_heap.rs:585`, `src/global/tls_heap.rs:427`, `src/global/sefer_alloc/global_alloc.rs:61`, `src/global/sefer_alloc/global_alloc.rs:114`, `src/registry/heap_core_xthread/ring.rs:94`, `src/registry/heap_core_xthread/routing.rs:245`.

Reachable scenario: all registry slots are occupied (or binding fails), so a thread's `alloc` uses the fallback heap while `LOCAL` remains null. The same thread later calls `dealloc` on that fallback allocation. `current_for_dealloc` returns `ForeignNoBind`, which skips the fallback lock and sends the block to the fallback ring or large-deferred stack. The same path is reachable for fallback allocations made during TLS teardown. If no later fallback operation drains the relevant queue, the memory stays retained; unlike a recycled registry slot, the fallback heap has no thread-exit drain. The source comment's assumption that null `LOCAL` means the thread never allocated through this allocator does not cover the failed-bind fallback case.

Consequence: fallback traffic can retain otherwise freed small segments or large reservations for the rest of the process, and frees on the very thread that allocated them do not reclaim directly. The impact is greatest during registry exhaustion or repeated teardown allocations.

Fix direction: distinguish fallback-owned blocks before the bind-less remote routing and reclaim under the fallback's mutual-exclusion protocol, or add a safe fallback-specific drain/scavenge trigger. Preserve the no-recursive-lock invariant on the fallback path.

### R6-03 [P2] `trim_current_thread` omits already-pending remote frees

Evidence: `src/global/sefer_alloc/diag.rs:192`, `src/registry/heap_core/state/ownership.rs:256`, `src/registry/heap_core/state/ownership.rs:264`, `src/registry/heap_core_xthread/drain.rs:108`, `src/registry/heap_core/alloc/hot.rs:254`, `src/alloc_core/small/alloc_core_small/find_segment.rs:269`.

Reachable scenario: live owner A allocates a large block or a small segment's last blocks; B finishes freeing them remotely; A calls `trim_current_thread` and then becomes idle. The trim flushes tcache, drains the small pool and evicts the large cache where those features are enabled, but it does not drain A's large-deferred stack, per-segment remote rings, or heap overflow. The later large allocation/refill that would drain them never occurs.

Consequence: the explicit memory-release API can return while fully freed remote objects still pin reservations. This is observable without owner exit and can defeat the documented end-of-burst use case. A free racing after trim cannot be covered by a simple snapshot; the claim here is for frees completed before the call.

Fix direction: drain pending remote large/small/overflow work before cache and pool eviction, under the owner-only writer discipline; define precisely whether trim promises a quiescent snapshot or only best effort under concurrent frees.

### R6-04 [P3] NUMA directory can retain stale bits in the unknown-node bucket

Evidence: `src/alloc_core/segment/segment_directory/segment_directory_impl.rs:369`, `src/alloc_core/segment/segment_directory/segment_directory_impl.rs:373`, `src/alloc_core/segment/segment_directory/segment_directory_impl.rs:480`, `src/alloc_core/small/alloc_core_small/directory.rs:183`, `src/alloc_core/small/alloc_core_small/find_segment.rs:943`.

Reachable scenario (`numa-aware` plus `alloc-segment-directory`): eight node IDs occupy all dedicated buckets. A ninth node N publishes a class-C bit in the unknown bucket. One dedicated bucket later becomes empty. A new publication for N, perhaps in class D, assigns N that dedicated bucket while its older class-C bit remains in unknown. When class C empties, `publish_empty` looks up N's new bucket and clears only there. As long as N has any dedicated bits, repeated validation of the stale unknown class-C bit repeats the same ineffective clear.

Consequence: repeated stale directory candidates, wasted scans, and weaker NUMA locality decisions until N loses its dedicated bucket or the segment slot is fully cleared. This is a performance/bookkeeping defect, not evidence of invalid memory access.

Fix direction: clear a candidate's bit from every possible bucket on empty/stale validation (a helper already exists for all-node clearing), or migrate unknown-bucket bits when assigning a dedicated node bucket. Keep `active_bits_by_node` consistent during migration.

### R6-05 [P4] Batch API incorrectly describes zero as OOM-only

Evidence: `src/global/sefer_alloc/batch.rs:52`, `src/registry/heap_core/alloc/batch.rs:107`, `src/registry/heap_core/alloc/batch.rs:304`.

Reachable scenario: under `batch-api`, a caller passes an empty output slice. The fastbin implementation returns zero immediately; the non-fastbin implementation also returns zero after its empty loop. Neither is OOM, contrary to the public `alloc_batch` documentation.

Consequence: a caller that treats every zero return as allocator failure can report a false OOM for a valid empty request.

Fix direction: document `0` as either empty input or allocation failure (and clarify partial-fill behavior); no allocator change is needed.

## Hypotheses requiring verification (not counted as findings)

- `src/registry/heap_core_xthread/overflow.rs:783` and `:911` call `LAST_STALL_CONCESSIONS.with`, whereas `src/global/tls_heap.rs:428` explicitly tolerates inaccessible TLS via `try_with`. Confirm on every supported TLS backend whether a double-saturated free during teardown can reach `.with` after this key is inaccessible; if so it would panic inside `GlobalAlloc::dealloc`. The source alone does not establish backend behavior, so no severity is assigned.
- The all-features `hardened` generation byte wraps after 256 reissues (`src/alloc_core/segment/segment_header/segment_header_gen_table.rs:109`). Its stale-note residual is acknowledged in the source and involves invalid double-free input; I did not classify it as a new valid-use defect.

## Unmeasured optimization candidates (not counted as findings)

- On double ring saturation with a live but inactive owner, `src/registry/heap_core_xthread/overflow.rs:789` performs up to 128 zero-progress rounds (and a 4096-round safety cap), each with up to 8,192 retry polls (`src/registry/heap_core/core.rs:258`) and a 200-us sleep after the first round (`src/registry/heap_core_xthread/overflow.rs:178`). The existing spill path preserves correctness; compare earlier spill concession against tail latency and CPU cost under real workloads before changing policy.
- With `alloc-stats`, every `stats()` call walks materialized registry slots (`src/registry/heap_registry/counters.rs:342`). If metrics scraping is frequent and thread high-water is large, consider maintained aggregate counters, but measure update contention versus scan cost first.
- `SegmentTable::hash_index` uses only masked low address bits (`src/alloc_core/segment/segment_table/hash.rs:27`). Repeated address patterns separated by the table period could cluster probes/removals. Treat this as OS-address-pattern dependent until a real trace demonstrates it.

No project files other than this report were edited. No project execution or dynamic validation was performed.
