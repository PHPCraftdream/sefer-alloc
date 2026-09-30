# Terminal remote-free cutover — independent XS/Sol6 acceptance review

## Decision and scope

**Decision: NO-GO for an unconditional terminal/ownerless OS-release acceptance claim.** This bounded, source-read-only review found **no demonstrated P0/P1 source counterexample** to valid-use terminal publication, exclusive reclamation, or Large biased geometry. It found one **P2 acceptance-oracle gap** and one **P3 documentation mismatch** below. The P2 is a defect in the evidence for physical release, **not** a claim that the current source fails to release. Source acceptance may proceed conditionally after that oracle is strengthened and the parent's targeted verification succeeds; this report itself does not certify a production release.

The reviewed artifact is `2ebb6d80be665a0800d4abf50ae70e9c9ff3f354` **plus the uncommitted tracked changes and new source/tests in this worktree**. The commit alone does not represent the candidate snapshot. This is the unfinished terminal remote-free integration, not a fresh 59-category audit. I read the rust-intel unsafe/FFI, concurrency/state, Drop/RAII, and semantic-conformance modules and traced the relevant production paths and test oracles. No tests, build, runtime script, Miri, Loom, or source edit was run or made. Other rust-intel categories were not independently covered.

## Findings

### P2 — Ownerless OS release is not an observed acceptance postcondition

**Locations:** `tests/r8_autonomous_maintenance.rs:28-49,91-96`; `src/alloc_core/large/alloc_core_large.rs:695-700,744-790`; `src/registry/heap_core/state/ownership.rs:108-125`; `src/alloc_core/large/alloc_core_large_cache_eviction.rs:58-72`.

**Counterfactual witness:** In the `alloc-decommit` child scenario, a remote Large free can be claimed and its route unlinked at `reclaim_large_segment`, then deposited into the Large cache. If the cold-trim `evict_all()` step at `ownership.rs:125` were omitted, the 5 MiB test allocation would remain cached/mapped under the ordinary cache budget. `wait_for_retirement` would still see the route disappear and a later completed worker pass, and the child would still print `RETIRED_WITHOUT_ALLOCATOR_CALL`. Thus the test detects logical retirement and worker progress, but cannot distinguish that regression from actual OS release. It does not compare the reservation/release counters or observe the original release token. The same absence of a direct release oracle applies to the Small check; the Small route assertion is compiled only under `alloc-decommit`.

**Impact/limit:** This is an acceptance false-positive for the requested *physical* ownerless-release claim, not evidence of a live leak in the inspected source: current `trim_for_recycle` does call `evict_all`, and its eviction path calls `os::release_segment`. The contract also permits policy retention before cold trim and excludes stalled `OWNED`/`MAINTENANCE`, unfair scheduling, and process exit. A negative control that suppresses the final OS-release call while preserving route unlink, plus an isolated exact release-token/counter oracle after the last free and without a new claim/alloc, would close this specific evidence gap. Merely asserting `maintenance_running` or route absence would not.

### P3 — Public architecture text still describes the removed authoritative ingress

**Locations:** `README.md:363-369,846-853`; production replacement at `src/registry/heap_core_xthread/routing.rs:26-70` and `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:62-71`.

**Witness:** A correct foreign `Box<u8>` free now resolves a System-backed route pin and publishes a Small sidecar bit. It never queues into the removed per-segment/per-heap rings or writes an intrusive spill note into the freed block, yet the README states both as the current `SeferAlloc` behavior. The README also describes lazy alloc-slow-path reclaim as the only consumer, while explicit ownerless maintenance now exists.

**Impact/limit:** Documentation drift, not a demonstrated allocation/free failure. It can misstate the provenance and liveness contract to downstream users/reviewers. The design documents still explicitly distinguish target behavior from a release guarantee, so this README finding alone is not a source-safety blocker.

## Valid-use path review and limits

| Obligation | Static trace and conclusion |
|---|---|
| Route before issue | `SegmentTable::register_payload` prepares the route before installing the owner slot/hash (`src/alloc_core/segment/segment_table/segment_table_impl.rs:343-390`); fresh Large writes its header and returns only after registration (`src/alloc_core/large/alloc_core_large.rs:619-658`). Primordial owner attachment precedes heap handoff (`src/alloc_core/alloc_core/lifecycle.rs:272`). Small free-list pop, batch drain, scalar carve and batch carve each issue the class record before returning blocks (`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:357-367,483-513,637-660,791-824`). Magazine blocks retain their credit and class until issue/flush; issue clears the residency bit before return (`src/registry/heap_core/alloc/hot.rs:43-56,344-392`; `src/registry/heap_core/alloc/batch.rs:190-207`). No valid issue-without-route path was established in these branches. |
| Unique foreign terminal publication | Installed `GlobalAlloc::dealloc` and batch foreign-no-bind entries delegate to `publish_foreign` (`src/global/sefer_alloc/global_alloc.rs:50-68`; `src/global/sefer_alloc/batch.rs:115-137`). The own-heap miss routes there too (`src/registry/heap_core_xthread/routing.rs:10-20`). A ready idle fallback can reclaim synchronously under its try-lock; a busy fallback uses the pinned sidecar (`routing.rs:26-70`; `src/global/fallback.rs:366-383`). Small publishes one AcqRel bit RMW; Large does `LIVE(g) -> PENDING(g)` in an independent atomic state (`src/registry/segment_route/pin.rs:35-59`; `src/alloc_core/large/reservation_state.rs:24-54`). Foreign realloc copies only after route/capacity checks and publishes the old instance only after successful destination allocation (`src/registry/heap_core/free/realloc.rs:370-394`). Failure leaves the source live. These conclusions assume the stated unique current allocation and matching `Layout`; invalid double free is not used as a witness. |
| Pin and reservation lifetime | Lookup pins under the shard lock and unlink uses that same lock; last pin frees only System-backed descriptor/sidecar (`src/registry/segment_route/directory.rs:124-139,192-206,469-507`; `src/registry/segment_route/registration.rs:71-74`). A paused pre-publication valid producer still owns one outstanding credit. Small credit increments are checked/bounded and decrements checked against underflow (`src/alloc_core/segment/segment_header/segment_header_meta_fields.rs:39-77`); the producer never decrements. The owner consumes each detached bit and class before physical finalization (`src/alloc_core/alloc_core/sidecar_drain.rs:16-58`; `src/alloc_core/small/alloc_core_small_reclaim.rs:19-63`). This supports, but is not a formal proof of, zero-credit protection across all feature combinations. |
| Strict cut and ownerless progress | `BitmapScan` fixes a high-water word count and takes one `swap(0, AcqRel)` per word (`src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs:74-91`; `bitmap_scan.rs:13-25`); owner trim finishes its cuts before possible decommit/release (`src/alloc_core/alloc_core/sidecar_drain.rs:16-58`). A producer publishing after its word cut retains its credit for a later pass, so trim need not wait for it. `FREE -> MAINTENANCE` is a winning-CAS-only mutation lease; claim ignores `MAINTENANCE`, and recycle/lease Drop publish `FREE` only after owner work (`src/registry/heap_registry/claim.rs:46-71,191-237,344-421`). The service visits a bounded round-robin index set and retries fallback without waiting on its lock (`src/registry/heap_registry/maintenance.rs:18-44`; `src/global/maintenance_service.rs:125-161`). No lost-publication interleaving was established in the inspected path. |
| Biased Large geometry and rollback | For `align >= SEGMENT`, the physical token, page-aligned metadata root and aligned payload are distinct (`src/alloc_core/platform/os.rs:163-201`; NUMA counterpart `src/alloc_core/platform/numa.rs:148-164`). Registration keys on the numeric payload segment while owner hash returns the stored canonical root (`src/alloc_core/segment/segment_table/segment_table_impl.rs:343-390`; `src/alloc_core/segment/segment_table/hash.rs:241-266`). Local free/realloc use the stored root; terminal Large reclaim unregisters before returning the original OS token (`src/alloc_core/alloc_core/mem/mem_impl.rs:22-42,263-294`; `src/alloc_core/large/alloc_core_large.rs:687-790`). Cache reuse is excluded for over-segment alignment, and failed destination registration releases its reservation without consuming the original realloc source (`alloc_core_large.rs:204-260,619-658`; `src/registry/heap_core/free/realloc.rs:299-308`). No concrete biased-root misrelease or rollback loss was established. |
| Service startup/failure | `start_maintenance` is explicit and fallible; `STARTING` returns `InProgress`, spawn failure resets to `IDLE`, and startup only returns `Ok` after the worker publishes `RUNNING` (`src/global/sefer_alloc/maintenance.rs:16-44`; `src/global/maintenance_service.rs:52-112,125-131`). An unexpected worker return or unwind enters a process-aborting guard (`maintenance_service.rs:61-67,125-162`); the detached worker and static state are documented as process-lifetime, with no fork/unload/shutdown guarantee. No silent worker-death path was found in these branches. |

**Oracle calibration:** `tests/r8_global_box_provenance.rs:13-74` really installs `SeferAlloc` as `#[global_allocator]`, transfers narrow `Box<u8>` and `Box<Tiny>` values, drops them on a foreign thread, and checks pending-bit consumption plus free/route state. Its actual-Box post-terminal paused-producer case exists at `:77-116`, but is `#[cfg(miri)]`; this review did **not** run Miri and does not claim a passing result. `tests/r8_terminal_global.rs:89-123` pauses an explicit `GlobalAlloc` producer *before* publication and checks the retained route/second cut. `tests/r8_autonomous_maintenance.rs:52-96` uses a real installed allocator and a completed-worker-pass acknowledgement after owner exit, but its no-future-allocation premise is a test comment rather than an allocation/claim counter, and the P2 physical-release gap remains. Loom files are model oracles, not proof that the production code was run under Loom here.

**Boundary of decision:** This review does not certify all feature profiles, OS backends, Miri provenance, Loom interleavings, or performance/RSS. It finds no valid-use P0/P1 source counterexample in the specified delta; absence of such a finding is not a proof of soundness. The inherited dirty integration and test outcomes remain for the parent to accept separately.

## Parent follow-up — Windows native acceptance

The original findings above remain the historical review of its snapshot.
The P3 current-ingress documentation has been corrected. The P2 oracle was
strengthened by `tests/r8_os_release_oracle.rs`: capture exact original tokens
while the allocating owner is live, exit that owner, perform the last remote
frees, then use passive worker acknowledgements and native mapping queries.
The negative control unlinks a synthetic route while its reservation remains
mapped, proving route disappearance alone cannot satisfy the OS oracle.

The production and minimal `alloc-global,alloc-decommit` profiles, both with
`internals,bench-internals`, passed on Windows. Small, ordinary Large and biased
Large tokens became unmapped without a subsequent allocation or claimant.
Linux is wired into CI but was not executed in this acceptance run. This
closes the specific Windows evidence gap; it is not an all-platform release GO.
The old aligned-vmem Miri aperture recursively used the installed allocator.
Commit `e5a76ee6` pairs System allocation and release in that aperture; five
focused member Miri tests and 49 native member tests passed. Root tests with
the actual installed allocator still require acceptance: a later interpreter
run exited with allocation failure before the harness started. Its cause is
not established by that exit, and no root Miri pass is claimed.

## Parent follow-up — integrated runtime validation

The full root library/integration runtime suite passed on Windows with
`production,internals,bench-internals,batch-api`, `--locked`, `-j 1`,
`--no-fail-fast` and two test threads. Task
`398c9450-5593-44c6-b8a2-642a62387437` completed with exit code 0 in 244 seconds.
Native cutover code and targets are committed as `a4245965`; CI/model wiring
is committed separately as `c46d5052`.

Commit `911c0266` bounds the differential random fixture and the owner-recycle
fixture without permitting NULL allocations or dropping release accounting.
All six historical native seeds remain in a tracked fixture and are replayed
with their original generator. Actual Large-kind witnesses remain, including
with `medium-classes`; the recycle fixture still detects the former cap of 32.
The earlier NULL failures did not reproduce in these replays; their historical
cause is not proven, so these changes are not described as a production leak fix.

CI guards passed for 101 sentinels, seven wired root Loom targets, and the
reviewed debug-hook safety/gating inventory. This is not a remote CI run or a
claim that Linux, all feature profiles, or the pending root Miri checks passed.

Final `cargo clippy --all-targets` for that same profile with `-D warnings`
passed after a helper-only module rename (`20b7e060`); the Windows OS-release
oracle passed again in the same verification task
`bb6a0bb7-900e-4d9e-affe-608479bb4631`. No lint was suppressed.

Ten completed task worktrees were removed only after preserving their exact
worker snapshots as commits on their existing branches. Their older partial
states were not merged over the integrated main tree. Loose patches and copied
checkpoints were preserved under `worktrees/accepted-recovery-20260930` rather
than deleted or committed as production changes. The active Miri worktree and
unrelated baseline/review worktrees were excluded from cleanup.
