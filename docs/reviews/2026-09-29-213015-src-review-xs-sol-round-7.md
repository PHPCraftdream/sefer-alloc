# sefer-alloc `src/` review — round 7

Date: 2026-09-29 21:30:15 Europe/Berlin. Snapshot: `b8df7836fee6f4ec7f6cd1e670fa8c14f8c31582` (`review/src-xs-round7-20260929`).

## Scope and verdict

Independent static reading of the current `src/` tree (167 Rust files), its module/feature wiring in `Cargo.toml`, and the reachable allocator, remote-free, sidecar, lifecycle, NUMA, and legacy concurrent paths. Findings below are derived from this HEAD's code paths, not from earlier review documents or test outcomes. No tests, benchmarks, builds, dynamic probes, or artificial load were run; this review does not prove absence of other defects.

Confirmed: **P0 0 / P1 1 / P2 2 / P3 4 / P4 1**. Verdict: **release hold** on P1; both P2 findings retain memory after valid frees. No confirmed current-HEAD memory-safety violation was established by static reading. Unresolved provenance question is separated below without severity.

Priority convention: P1 = process-terminating availability failure; P2 = material unbounded retention; P3 = functional/operational or scaling defect; P4 = maintainability debt. No performance speedup is asserted without measurement.

## Confirmed findings

### P1-1 — Registry-chunk reservation failure aborts instead of using a serviceable fallback

**Location:** `src/registry/bootstrap/registry.rs:253-285`; call chain `src/registry/heap_registry/claim.rs:163-170`, `src/global/tls_heap.rs:491-494`, `src/global/sefer_alloc/global_alloc.rs:32-40`.

**Valid scenario:** A new thread's first allocation claims an index in a not-yet-materialized 64-slot registry chunk. Its OS reservation fails (e.g. address-space fragmentation or commit pressure), while the already-created fallback heap still has a free small block. `claim_impl` calls infallible `reg.slot(idx)`; `ensure_chunk` executes `process::abort()` before `claim` can return null and `finish_bind` can select fallback. The chunk-reservation failure is not evidence that the requested user allocation cannot be served from existing memory.

**Consequence:** Process-wide termination on an otherwise recoverable allocator metadata failure. This is also inconsistent with the `GlobalAlloc` face's documented fallback-on-claim-failure policy.

**Direction:** Make chunk lookup fallible on the claim path (`slot_or_none`/equivalent), roll back or skip the uninitialized index safely, and let the allocation route to the existing fallback. Preserve the infallible accessor only where materialization was already proven. Validate the failure path in a later, separately authorized test run.

### P2-1 — Remote frees arriving after slot recycle have no guaranteed consumer

**Location:** `src/global/tls_heap.rs:239-303`; `src/registry/heap_registry/claim.rs:302-337`; `src/registry/heap_core_xthread/routing.rs:221-250,261-300`; `src/alloc_core/large/deferred_large/drain.rs:25-55`.

**Valid scenario:** Thread A allocates large objects and transfers their ownership to thread B. A exits; its destructor drains only notes present *before* the `LIVE -> FREE` recycle and retains the entire `HeapCore` in that free slot. B frees those objects *after* A's recycle. Each valid free is pushed to A's stable deferred-large head; small frees similarly enter A's segment ring/overflow. If no later thread claims that exact slot, no owner executes the drain. B may continue running and using its own heap without servicing A's slot. `HeapRegistry::try_maintenance` exists but has no production caller in `src/`.

**Consequence:** Freed large reservations remain mapped for the rest of the process; small segments remain live because their remote-free credits are never retired. Retention can grow with post-exit frees, not merely with the fixed large-cache budget. It is not a double-free or a lost queue entry, but a missing reclamation trigger.

**Direction:** Add a bounded, ownership-safe scavenging path for `STATE_FREE` slots (for example a maintenance lease invoked from a suitable cold path), including both large-deferred and small ring/overflow drains. Recheck the `LIVE/FREE/MAINTENANCE` hand-off so a producer publishing around recycle cannot strand a note.

### P2-2 — Exit trim leaves an empty current small segment committed

**Location:** `src/alloc_core/small/alloc_core_small/reserve.rs:38-56`; `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs:85-105`; `src/registry/heap_core/state/ownership.rs:256-269`; `src/global/tls_heap.rs:264-303`.

**Valid scenario:** Under `alloc-decommit`, a thread fills its primordial segment, so `reserve_small_segment` selects a new 4 MiB `Small` segment as `small_cur`. It then frees every block in that segment and exits. The last free cannot pool/release that segment because `dec_live_and_maybe_decommit` explicitly rejects `base == small_cur`. The exit trim flushes tcache, drains the pool, and evicts large cache, but never finalizes or replaces the current cursor. The `HeapCore` stays in its recycled slot, so its empty current segment stays reserved/committed; no later claimant is required to take the slot.

**Consequence:** At least one unnecessary current small reservation can survive per exited heap slot after all its user blocks are freed (up to 4 MiB committed on an eager small-segment path, subject to OS page behavior). This retention is separate from P2-1's late remote frees and from the directory-sidecar footprint in P3-4.

**Direction:** On cold exit trim, when the current segment is `Small` and has zero live credits after tcache flush and remote-drain coordination, release it and make the primordial segment the valid cursor for future slot claim. Preserve the rule that no unpublished remote-free note can target a released segment.

### P3-1 — Custom large-cache budget is bypassed by the fallback heap

**Location:** `src/global/sefer_alloc/core.rs:186-205,295-305`; `src/global/tls_heap.rs:431-441`; `src/global/fallback.rs:239-243`; `src/alloc_core/config/large_cache_config.rs:255-284`.

**Valid scenario:** Install `SeferAlloc::with_config(LargeCacheConfig::new().budget_bytes(0))`; a TLS destructor allocates and frees a large block after its thread's heap guard has torn down TLS, or a thread uses fallback after registry exhaustion. `current_for_alloc_with_config` returns `Fallback`, but fallback creation uses `HeapCore::new(OWNER_ID_FALLBACK)`, not the supplied config. The fallback large cache can admit that freed span under its default budget even though `budget_bytes(0)` explicitly means “cache nothing.”

**Consequence:** The advertised hard cache ceiling does not cover allocations serviced by the same global allocator in fallback mode; memory retention becomes workload/teardown dependent. This does not claim a bound on total live allocations, only on cached freed spans.

**Direction:** Define fallback configuration semantics explicitly. If the installed global allocator's policy must apply, initialize fallback with that policy once and detect incompatible instance attempts; otherwise document and expose the fallback exemption so a strict-budget user can account for it.

### P3-2 — Valid 4 MiB-aligned layouts are rejected independent of available memory

**Location:** `src/alloc_core/large/alloc_core_large.rs:136-145`; `src/alloc_core/platform/os.rs:125-145`; `src/global/sefer_alloc/global_alloc.rs:32-47`.

**Valid scenario:** A caller passes the valid `Layout::from_size_align(1, 4 * 1024 * 1024)` to `GlobalAlloc::alloc` on a machine with sufficient memory. Small classification cannot serve that alignment; `alloc_large` returns null immediately at `align >= SEGMENT` without trying an OS reservation. A standard allocation wrapper may turn that null into a process abort.

**Consequence:** An alignment-supported-by-the-platform allocation is unavailable through `SeferAlloc` even without OOM. Returning null is not itself a `GlobalAlloc` safety-contract violation, but the source's broad “only true OOM”/“never-null for serviceable requests” wording is too strong for this case.

**Direction:** Either support over-segment alignment by reserving extra address space and choosing an aligned payload while retaining the original reservation for release, or make the alignment limit a documented API constraint and narrow the allocation-success claims.

### P3-3 — Claim falls back to a full 4096-slot scan under saturation

**Location:** `src/registry/heap_registry/claim.rs:271-282`; `src/registry/heap_registry/stack.rs:10-29,55-68`; `src/registry/bootstrap/registry.rs:44,72-78`.

**Valid scenario:** All 4096 registry slots have been claimed, and another thread makes its first allocation. `reuse_hint` has no free index; `scan_claimable_slot` visits every index before `bump_count` reports the cap and fallback is chosen. Repeated newcomers repeat this O(high-water) path. Under near-saturation, a single overwritable hint can similarly be consumed by a racer, leaving a long scan for another claimant.

**Consequence:** First-allocation latency and registry atomic/cache traffic scale with historical slot count exactly when thread contention is high. This is a source-level complexity observation, not a measured slowdown.

**Direction:** Consider a bounded free-index queue/stack or a saturation hint with a carefully specified ABA/state protocol; retain a scan only as a correctness backstop. Benchmark thread-churn shapes before claiming a speedup.

### P3-4 — Directory sidecar survives empty-slot trim for the process lifetime

**Location:** `src/alloc_core/small/alloc_core_small/directory.rs:58-95`; `src/alloc_core/segment/segment_directory/segment_directory_impl.rs:69,131-142,227`; `src/registry/heap_core/state/ownership.rs:256-269`; `src/global/tls_heap.rs:264-303`.

**Valid scenario:** A short-lived thread simultaneously uses enough small segments to cross the 32-slot directory-materialization threshold, frees its blocks, then exits. The exit trim flushes the tcache, drains the small pool, and evicts the large cache, but leaves `directory_sidecar_vm` in the persistent slot-resident `AllocCore`. The slot is recycled whole. Repeating this across many distinct slots retains one directory per slot even when almost all segment slots have been recycled and no user block remains live. Under `numa-aware`, the bitmap's statically allocated outer dimension is nine node buckets rather than one.

**Consequence:** Retained VM/commit scales with peak directory-materialized heap slots, not active segment count. The bitmap alone is `NODE_BITMAPS * SMALL_CLASS_COUNT * 64 * 8` bytes per materialized heap (before page rounding and other fields); no benchmark is needed for that footprint arithmetic.

**Direction:** On cold trim, release/rebuild the owner-only directory when live segment state is sufficiently small, or make its backing size track active/high-water segments. Preserve the existing dirty-publication invariants when re-materializing.

### P4-1 — Three remote-free substrates are compiled while only one is wired to production

**Location:** `src/alloc_core/segment/mod.rs:58-75`; `src/registry/mod.rs:64-68`; active route `src/registry/heap_core_xthread/routing.rs:261-300`.

**Valid scenario:** Build the allocator with `alloc-global` (and no `internals`). The active small remote-free route packs an entry for `RemoteFreeRing`/heap overflow; `RemoteInbox`, `remote_bitmap`, and `segment_route::RouteDirectory` are also in the module tree but have no production call sites in `src/` (their references are their own modules and test hooks). The inactive path includes additional unsafe code and a separate System-backed directory.

**Consequence:** More code and protocol variants must be reviewed whenever remote-free ownership changes, while dead-code allowances hide the absence of integration. There is no asserted runtime cost from uncalled code.

**Direction:** Put the staged implementation behind an explicit experimental feature with a stated integration boundary, or remove it until needed; keep one authoritative production protocol and its invariants visible.

## Hypotheses / follow-up questions — no severity assigned

1. **Foreign free and narrow provenance.** `src/alloc_core/platform/os.rs:144-145` masks the caller's pointer with `map_addr`; `src/registry/heap_core_xthread/routing.rs:189-193` and `src/alloc_core/segment/segment_header/segment_header_views.rs:91-114` then dereference header fields through that derived base. The own-heap path deliberately substitutes a canonical table-owned root (`src/alloc_core/segment/segment_table/segment_table_impl.rs:598-637`), but the foreign path has no equivalent. Static reading alone does not settle whether every valid `GlobalAlloc` pointer arriving from a narrow `Box`/reference reborrow still grants header access under the relevant Rust aliasing/provenance model. A targeted Miri/Tree-Borrows probe and/or a global canonical-root lookup design should decide this; no UB claim is made here.
2. **Class-aware dirty residual work.** `src/alloc_core/small/alloc_core_small/directory.rs:382-425` scans one class's dirty bitmap while a drain reclaims the full segment ring. Per-class bits for other classes can remain set after that full drain, so later class lookups may re-visit an empty ring. This is a plausible redundant-work cost, not a proven net regression; a safe clearing protocol must account for concurrent producers before any change.

## Architecture notes

The source has substantial historical task/review narration embedded in hot-path modules (for example `src/alloc_core/small/alloc_core_small/find_segment.rs` and `src/registry/heap_core_xthread/overflow.rs`). It often explains real invariants, but buries the current protocol among old experiments and claimed measurement outcomes. Keep the executable invariant and safety preconditions beside the code; move historical rationale and benchmark receipts to design/performance documents. This is maintainability advice, not an additional P finding or a speedup assertion.
