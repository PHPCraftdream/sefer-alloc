# Independent review of `src/` — xs, round 15

## 1. Scope, inventory, limitations, and verdict

**Scope:** the root-crate `src/` tree at base commit `b1a1e4f9ad232e7a3aef6e7b8b488c5b27eddfe9`. This is the fifteenth **source-review** round, not historical performance round R15. Reviewer: **xs / Extra Sol 6.1**, independent read-only inspection. No source remediation is authorized here.

**Derived inventory: 168 Rust files, 43,317 physical lines.** Every file was included in the catalogue and executable/declaration text screen. The screen displayed all **17,034 nonblank lines not beginning with `//`**, including feature attributes and their continuations. The tracked verbatim excerpts additionally included selected contracts/comments: **18,629 distinct source lines across all 168 files**. This excerpt count is a conservative, reproducible accounting of the Eval reader's line sets, not an assertion that every comment or every possible execution was proved correct. Additional ranged reader views are not added to that count. Appendix A lists every file and its physical line count; the group census below and the unsafe inventory are derived from the actual files, not copied from R14's older 43,261-line snapshot.

| Source group | Files | Physical lines | Distinct lines in tracked verbatim views |
|---|---:|---:|---:|
| `src/alloc_core/` | 88 | 24,814 | 10,025 |
| `src/concurrent/` | 14 | 3,536 | 2,168 |
| `src/global/` | 18 | 3,807 | 1,652 |
| `src/registry/` | 46 | 10,413 | 4,568 |
| `src/lib.rs`, `src/kani_proofs.rs` | 2 | 747 | 216 |
| **Total** | **168** | **43,317** | **18,629** |

Counting method: UTF-8 text read through the file reader; physical lines are `len(text.splitlines())`; files were inventoried with `glob` and grouped by their first `src/` component. The executable/declaration-screen denominator is the explicit lexical filter above, **not** a parser-derived LOC metric. The corresponding excerpts were displayed with original file/line labels. No generated review script or source fixture was left in the checkout.

**Limitations:**

- Static reading and call-path/feature-gate tracing only. **No tests, builds, benchmarks, formatters, clippy, rustdoc, Miri, Loom, Kani, executable probes, advisory scans, commits, or pushes were run by this reviewer.** Reading existing tests is not executing them. Proposed witnesses below are unexecuted.
- Delegation is disabled in this reviewer environment. This is a bounded single-context pass, not a fresh independent worker for every rust-intel module. Unsafe/FFI, concurrency/state, Drop/RAII, lifetimes/API, numeric/data, and semantic-conformance guidance was consulted. Async, dependency/macros, security/advisory, and testing themes did not receive standalone full-module audits; async applicability, resolved dependency versions, and the named regression oracles were checked at relevant boundaries.
- Textual references, not rust-analyzer/AST reachability or a compiled feature powerset. All executable text was screened, but the strongest analysis is the concrete paths cited below. **Coverage is incomplete as a soundness proof**, especially for weak memory, aliasing/provenance, platform syscalls, panic hooks, dependency internals, and unusual feature intersections.
- Companion crates were not independently re-audited. `aligned-vmem::Reservation::drop` was read to establish the root wrapper's cleanup boundary; `crossbeam-epoch` ownership semantics are also stated explicitly by the root seam. Neither constitutes a dependency-wide certificate.
- Cargo.lock directly resolves `crossbeam-epoch 0.9.20`, `arc-swap 1.9.1`, `slotmap 1.1.1`, `aligned-vmem 0.2.0`, `numa-shim 0.2.0`, `once-ptr-cell 0.1.0`, `size-classes 0.1.0`, `sefer-region 0.2.0`, and root `sefer-alloc 0.3.0`. Cargo.toml declares edition 2021 / MSRV 1.93. No newly exercised compiler-version or target-runtime claim is made.

**Verdict: three source-confirmed findings — two P3 defects and one P4 documentation finding.** No new confirmed P0/P1/P2 or code regression attributable to R14 was established. This is **not** an absence-of-bugs proof or a release/performance GO. The accepted, uncorrected **P1-box (correctness 164)** remains open; the separate observed experimental epoch/default-collector Miri failure **171** remains open without a false-positive classification. Their inherited evidence is not a run performed for this report.

## 2. Confirmed findings

Severity here: P3 = bounded correctness/resource/diagnostic defect worth fixing, without a demonstrated new production memory-safety exploit; P4 = misleading contract/maintainability defect with no identified runtime fault from that text alone. `SOURCE-CONFIRMED` means a concrete source execution or contradiction is established, **not** that a witness was executed.

| ID | Severity | Evidence class | Reachable scope | Finding |
|---|---|---|---|---|
| R15-01 | P3 | SOURCE-CONFIRMED; parent temporary native witness passed | `experimental`; also the sharded tier through `pinning` | A panicking live value destructor stops `EpochRegion::drop`, leaking remaining live pointees/resources |
| R15-02 | P3 | SOURCE-CONFIRMED; parent temporary OOM-injection witness passed | `alloc-global`, including `production`; diagnostics always available | Primordial route-attachment failure releases its mapping via RAII but never increments the root release counter |
| R15-03 | P4 | SOURCE-CONFIRMED contract/source contradiction | `experimental` binding docs and `alloc-global` module wiring | Binding docs promise handle rerouting/mutex-free async safety, and live production routing is labelled unconnected |

All three are **inherited defects/text**, not attributed to R14's source changes. R14's five closed findings are not recycled as new findings.

### R15-01 — live epoch values after a panicking destructor are never dropped

**Locations / symbols:** `src/concurrent/epoch/epoch_region.rs:682–692`, `Drop for EpochRegion<T>`; `src/concurrent/epoch/hand.rs:526–540`, `AtomicSlot::drop_value`; `hand.rs:584–591`, explicit absence of an owning slot destructor. Existing positive oracle: `tests/epoch.rs:158–187`, `region_drop_runs_live_value_destructors_once`.

**Configuration and call path:** ordinary safe `EpochRegion::with_capacity` → `insert` for a `T: Send + 'static` with a legal panic-capable destructor → ordinary `drop(region)` under `panic = "unwind"`. No `internals`, malformed handle, concurrent access, remote free, or generation setter is required. `ShardedRegion` contains `EpochRegion` shards and inherits the per-shard problem. The experimental tier is deprecated, but remains callable.

**Concrete mechanism:**

1. Region Drop manually walks `self.slots`, calling `slot.drop_value()`.
2. `drop_value` swaps the current pointer to null and drops `shared.into_owned()` immediately.
3. If this `T::drop` unwinds, the manual loop is exited. Later slots still hold their live `Atomic<T>` pointers.
4. Rust's ordinary field/slice cleanup drops the `AtomicSlot` values, but `AtomicSlot` has **no `Drop` implementation** and the atomic pointer handle does not own/drop its pointee. The root seam explicitly documents that fact at `hand.rs:584–591`.
5. The remaining live allocations were never evicted, so no `defer_destroy` owns them either. Neither region cleanup nor the global collector will subsequently run their destructors.

A two-slot counterfactual is sufficient: construct capacity two; insert the nonpanicking value owning a drop counter/resource first (initial index1), then the value whose destructor panics once (index0); catch the **outer** region-drop unwind. The parent's temporary native witness passed with the panicking value's drop count at 1 and the later live value's count at 0. The witness follows **actual slot order**: insertion consumes the initially ascending free-index vector with `pop`, while Drop walks the slot slice from index0 upward. The temporary test source was removed after verification.

**Impact:** permanent loss of remaining live pointees and any resources their destructors own after a recoverable panic. This contradicts the I5/drop-once explanation, not merely a process-exit EBR grace-period caveat. It is **not a demonstrated UAF/data race**. Under `panic = "abort"`, or a second panic during outer unwind, process termination is a different limitation and is not the witness here.

**Why the existing test is insufficient:** its `DropCounter::drop` only performs an atomic increment. It detects removing the normal manual loop, but cannot distinguish unwind-safe ownership from cleanup that only reaches later elements when every earlier destructor returns normally. That is an oracle-coverage explanation, not a claim that the test failed or that no other test exists anywhere in the repository.

**Correction direction:** give each still-owned slot/pointee a cleanup obligation that survives interruption of the region's manual loop, or provide an unwind cleanup guard for the unprocessed tail. Per-slot owning Drop is one straightforward direction; eviction must still null the slot before deferred ownership transfer, and normal/manual cleanup must remain idempotent to avoid double destruction. Do not suppress the user's panic, equate deferred and still-live ownership, or leak the remaining tail deliberately while retaining the drop-once guarantee.

**Parent verification (2026-10-07):** a temporary two-slot integration witness with one panicking and one later nonpanicking `DropProbe` passed: outer `catch_unwind` caught the panic, the panicking destructor count was 1, and the later destructor count remained 0. The test source was removed; this is a verification receipt, not a permanent regression test or fix.

### R15-02 — OOM rollback permanently inflates the advertised live-segment gauge

**Locations / symbols:**

- `src/alloc_core/platform/os.rs:165`, `Segment(vmem::Reservation)`; successful reservation increments at `:217–228` and lazy counterpart `:390–402`; root release counter increment only in `release_segment`, `:557–565`.
- `src/alloc_core/alloc_core/lifecycle.rs:255–282`, `AllocCore::new_inner`: `prim.table.attach_owner(owner, primordial_base)?` occurs while `prim.segment` remains an owning RAII wrapper, before the successful ownership-transfer `mem::forget`.
- `src/alloc_core/segment/segment_table/segment_table_impl.rs:268–276`, `attach_owner` → `RouteSlots::new`; `src/alloc_core/segment/segment_table/route_slots.rs:20–34`, `:37–50`, `:74–91`, fallible System storage / route registration.
- Downstream guarantee: `src/global/alloc_stats.rs:108–124` and `src/lib.rs:40–44` call reserved-minus-released the live segment count; `os.rs:44–51` even asserts that the two totals cannot desynchronize.
- Actual nested cleanup: `crates/aligned-vmem/src/reservation.rs:1546–1567`, `Reservation::drop` calls its own `release_reservation`, not the root `os::release_segment` accounting seam.

**Configuration and call path:** first registry or fallback heap materialization under `alloc-global` → `AllocCore::new_with_owner` / `new_with_config_for_owner` → successful primordial OS reservation → failure allocating the owner route slots, Small sidecar, descriptor, or directory storage → `new_inner` returns `None`. This is a normal fallible/OOM path in `production`, not invalid allocator use. `internals` adds the existing deterministic registration-refusal seam; it is not required for a real System allocation failure.

**Concrete mechanism:** the successful primordial reservation increments `SEGMENTS_RESERVED_TOTAL`. The `?` at route attachment then drops `Primordial`, including its `Segment`. `Segment` has no root-level accounting Drop; the contained `vmem::Reservation` releases the mapping directly. Consequently `SEGMENTS_RELEASED_TOTAL` never advances for that cleanup. A successful rollback therefore leaves a permanent **+1** error in reserved-minus-released per failed materialization. Retrying cannot repair that lost accounting event. This is not the brief relaxed-load skew acknowledged by `AllocStats`.

**Evidence and limits:** the error path and cleanup forwarding were traced directly. `tests/r6_route_lifecycle_oom.rs:13–18` already drives failed primordial registration plus retry; `tests/r6_route_lifecycle_fallback.rs:11–24` drives the analogous fallback case. They assert failure/retry/routing, not a matching reservation/release delta; they were read, not run. The parent's temporary first-registration-refusal probe passed assertions for `reserved_delta == 1` and `released_delta == 0`, confirming the counter mismatch. No OS-release failure was induced, no physical leak was measured, and no assumption is made that a void OS wrapper independently attests kernel success.

**Impact:** false segment-leak alerts and permanently inaccurate public diagnostics precisely when memory/metadata pressure triggers bootstrap failure. A monitoring consumer or gate using this pair as an ownership ledger can infer retained mappings that no longer exist. **The mapping itself is cleaned up by RAII; this finding does not claim a physical reservation leak.** The sibling sidecar accounting pair is not the same counter family.

**Correction direction:** account destruction of an owning `Segment` on all rollback/unwind exits, while preserving exactly one count when ownership is transferred into `AllocCore` and later released explicitly. An accounted RAII wrapper is the boring direction. Do not remove RAII, add a second raw release beside the nested Reservation Drop, or merely weaken the public live-count documentation to hide a missed event. Amend the incorrect “two totals can never desync” comment with the real pairing obligation.

**Parent verification (2026-10-07):** the same temporary probe exercised the existing `RouteDirectory::fail_next_registration_for_test` seam and asserted reservation delta +1 / release delta 0; it passed and its source was removed. Fallback, Small/Large rollback, and success/retry counter controls remain unrun and are part of the permanent-fix acceptance.

### R15-03 — current source comments still misstate routing and locking contracts

**Locations / symbols:** `src/concurrent/sharded/sharded_region.rs:644–651`, rustdoc for `bind_current_thread_to_shard`; implemented paths at `:548–556` (`insert`), `:575–580` (`get_with`), `:623–638` (`remove`). Supporting lock sites: `src/concurrent/epoch/epoch_region.rs:428`, `insert`, and `:511`, owner `remove`. Separate wiring assertion: `src/registry/mod.rs:50–54`, `segment_route` declaration.

**Configuration and reachable call paths:** the binding text is rendered/callable with `experimental` (and with `pinning`, which implies it). Production `alloc-global` routes foreign frees through `heap_core_xthread::routing::publish_foreign` → `RouteDirectory::global().lookup` → terminal sidecar publication; root module visibility becomes external only under `internals`, but routing itself is not internals-only.

**Concrete contradictions:**

- The binding rustdoc says subsequent `insert/get_with/remove` route to the bound shard id. Only **insertion placement** follows the binding. `get_with` selects `handle.shard`; `remove` also selects `handle.shard`, and uses the calling thread's cached binding only to choose owner versus remote removal. Reading/removing a handle minted on shard 0 after binding to shard 1 must still visit shard 0. The code is correct; the blanket routing statement is false.
- The same paragraph says the hot path “holds no lock” and is “naturally async-safe.” `EpochRegion::insert` takes the writer mutex on every successful or full insertion attempt. Owner removal takes it for bookkeeping; the remote path can block on the queue mutex. Two threads may explicitly bind the same shard, as the neighboring correct `:666–671` paragraph itself acknowledges. No evidence justifies a general nonblocking/executor-safety guarantee for that API.
- `registry/mod.rs` still labels `segment_route` an “Unconnected Stage 3 substrate.” It is directly on the current production foreign-free call path and is also attached during heap bootstrap. It is only its **external visibility** that is test/internal-gated.

**Impact:** misleading API/scheduling and audit-boundary expectations; a user may reason incorrectly about handle destination or blocking on an async worker, and an auditor may incorrectly exclude production routing. No incorrect routing or deadlock was executed or inferred as inevitable from these comments alone. Severity is P4, not a new memory-safety issue.

**Correction direction:** state insertion binding separately from handle-addressed reads/removals and owner/remote dispatch; distinguish lock-free reads/eviction CAS from writer and enqueue mutexes. Describe `segment_route` as connected production infrastructure with internals-only external exposure. This is targeted prose correction, not a request to change the correct routing algorithm or add an async implementation.

**Baseline relation:** R14-03 corrected the removed Rust hook scanner and historical state-machine claims, not every source comment. These examples fit the already-open general prose debt **154**, which remains LEAVE at its current trigger. They do not reopen closed R14 item170 or allege a regression introduced by R14-04's TLS fix.

## 3. Review of R14 work and current boundaries

The R14 report and its appended closure receipt were read (`docs/reviews/2026-10-06-src-review-sol-round-14.md:343–422`). Historical PASS receipts below are not new executions. The full read-only source delta was also inspected with `git diff --unified=0 d6417c6c85f9f8c124110eac77080523df06b004 b1a1e4f9ad232e7a3aef6e7b8b488c5b27eddfe9 -- src/`: six source files changed. Epoch Drop, the primordial ownership/accounting paths, and `registry/mod.rs` are outside that delta; the misleading binding paragraph also remains unchanged within the modified sharded file. This establishes the inherited-versus-R14 attribution independently of the closure verdict.

| R14 item | Source/document facts at this base | Disposition |
|---|---|---|
| R14-01 owner capability | `SmallSidecar::{prepare,issue}` are crate-private (`small_sidecar.rs:50,62`); public owner forwarding lives on `RouteRegistration` (`registration.rs:28,34`), which retains `PhantomData<Cell<()>>` (`:12`). Closed shared-mutator schedule is not rediscovered. | KEEP CLOSED, item170 |
| R14-02 pruning work | `TokenBlock::drop` bumps a wrapping death hint (`sharded_region.rs:211`); prune compares its saved hint before `retain` (`:277–287`) and saves the pre-sweep snapshot. Live claims are retained; an observed racing death can retrigger a later cold sweep. The all-live cost oracle exists in `tests/r14_shard_prune_work_bound.rs`; not run here. | KEEP CLOSED; no new timing/Ir/RSS verdict |
| R14-03 current evidence | `CROSS_THREAD_STATE_MACHINES.md` explicitly supersedes §§0–8 and its §9 separates descriptor storage/pins from physical reservation credit, cache reuse, and slot lease authority. The former Rust hook tripwire is historical; the closure names the script successor. Scanner success in the receipt is not a proof of every hook invariant. | KEEP CLOSED; R15-03 is different targeted prose debt |
| R14-04 late TLS | `claim_or_get_shard` probes router and release-guard TLS before CAS (`:483–511`); late cold inserts share without an unrecordable claim. Explicit bind checks both TLS cells before recording/CAS (`:683–707`). A remaining intact cache is advisory routing, not an assertion of exclusive ownership. | KEEP CLOSED; no new teardown regression established |
| R14-05 indexes | Current perf79/80 are Recently-resolved archive pointers; 40/41 are in the active tier; current correctness headings include171 and the corrected counts. R14's closure is separately retained. | KEEP CLOSED, preserve all old status/trigger decisions |

### Checked guarantee/call-path matrix

| Area / promised property | Trace considered | Conclusion and remaining limit |
|---|---|---|
| Allocation validity / alignment / zeroing | `GlobalAlloc` scalar/batch dispatch → `HeapCore` class/magazine/refill → core carve/pop or Large geometry | Checked size/alignment classification, zeroing of nonvirgin/cached spans, and fallback selection. Not a new differential/Miri/OS certificate. |
| Realloc success / OOM ownership | own canonical root, same-class/Large capacity fast path, move copy bound, foreign descriptor payload bound, promoted-medium free routing | Copy spans and old-allocation preservation on a null new pointer traced. Rejected arbitrary stale/double-free inputs are not valid-use proofs. |
| Small credit / detached cut | carve/pop increments; magazine retains credit; flush/sidecar reclaim decrements; fixed per-word cut before finalization | Producer writes independent sidecars, not reservation bytes. Full-sweep/budgeted paths do not wait for a producer-held descriptor pin. P1-box still applies when owner writes the intrusive free-list link. |
| Large credit versus descriptor pin | descriptor `Live→Pending→Consuming→unlink`; separately physical `Live→Consuming→Cached/Released`, cache `Initializing→Live`, rollback/exhaustion | Fresh descriptor generation is not physical cache generation. Lookup's self-check is not stale-address validation. No conflation used to claim soundness. |
| Heap single writer / memory ordering | first claim `EMPTY→INITIALIZING→LIVE`; reclaimed claim `FREE→LIVE`; maintenance `FREE→MAINTENANCE`; releasing CAS and initialized publication | CAS winner is authority; chunk/slot lifetime is process-static; no timeout steals an OWNED heap. Weak-memory models were not run. |
| TLS / fallback / worker | LOCAL poison before lease drop; fallible bind; fallback init Release/Acquire and lock; explicit worker startup and failure guards | Detached worker is intentional and process-lifetime; no restart/shutdown/unload promise. Worker failure abort is explicit policy, not a silent lost handle. |
| Error cleanup / metrics | primordial guard through owner attachment, Small/Large registration rollback, cached failure release, sidecar RAII | Physical ownership cleanup is mostly explicit; R15-02 is the missed accounting event, not removal of that cleanup. |
| Handles / equality / reclamation | region id checks; generational eviction; matched Eq/Hash fields; epoch Send/'static ownership and Sync read bounds; RCU copy publication | Epoch live-only unwind cleanup is R15-01. Region-ID and FIFO `seq` wrap horizons are not confused with Large generation saturation. |
| Feature/target boundaries | crate-root wiring; docs.rs `production`; no_std reexports; hardened, medium/exact/reserved, batch, lazy, NUMA and internals branches | Allocator is expressly rejected on non-64-bit pointer widths (`lib.rs:391–401`, README Target support). `--all-features` would not replace the distinct no-NUMA reserved-capacity branch. No feature powerset compiled. |
| Verification-only code | `kani_proofs`, root Loom shim and its safe mirrored stack traits; Miri-only publication gate | Read as models/test machinery, not current production implementation; item167 remains open. Item171's observed collector failure is not suppressed. |

No application crypto, network parser, external wire codec, async/Tokio task body, `extern "C"` export, `transmute`, `Box::from_raw`, `from_raw_parts`, or `Pin::new_unchecked` runtime body was identified in this source screen. OS FFI, collector internals, the facade's companion crates, and dev/examples outside `src/` are outside that negative observation. Unsafe boundaries are enumerated in Appendix C rather than equated to defects.

## 4. Optimization hypotheses and maintainability opportunities

**Every optimization below is UNMEASURED.** Structural extra work is not a speedup, latency result, RSS improvement, promotion GO, or permission to reopen an old rejected experiment. Current performance-index triggers remain unchanged.

1. **Route lookup self-check — inherited R14 opportunity.** `registry/segment_route/directory.rs:871–887` does a second `guard.find(key)` after incrementing the pin refcount under the same exclusive shard lock. Key/incarnation cannot change under that guard, and pinning changes only its refcount. Reuse the established entry identity rather than repeat the search if the self-check is retained. Required before any speed claim: real GlobalAlloc foreign-free workload, with entry-count/contention axes and pin/unlink controls; not a local fake directory alone.
2. **Reduce scalar free-run root lookups without weakening lifetime.** `registry/heap_core/free/dealloc_own_base.rs:347–352` resolves `canonical_block_of` even though its routed caller already resolved the own block in `heap_core_xthread/routing.rs:11–15`. The overflow flush loop at `:482–490` similarly resolves each magazine pointer before `AllocCore::flush_class` groups and resolves runs. A crate-private known-root handoff or per-run coalescing might avoid redundant lookup work. Retain the guards on externally supplied unsafe diagnostic entrypoints and preserve biased Large roots versus ordinary Small mask bases. Measure at HeapCore/GlobalAlloc with mechanism activation, not a lower-layer membership microbenchmark presented as a production win. This must not quietly redo perf item1's exhausted per-block experiments.
3. **Directory snapshot granularity — inherited numeric/copy opportunity.** `alloc_core/platform/os.rs:706–724` copies a whole class-word array so discovery can mutate the directory without a live shared array borrow. Word-at-a-time raw field access or a summary could reduce copying/empty scanning, but must retain the no-overlapping-reference discipline and authoritative terminal-ingress discovery. `find_segment.rs:399–430` deliberately cannot trust a negative routed directory result. No proposal to skip pending frees is endorsed; attribution and the existing routed-miss counters must first establish a victim.
4. **Global death-hint cross-talk — bounded residual, not reopening R14-02.** The new gate eliminates all-live repeated sweeps when no backing dies. A death on any other thread/region can still make the next cold binding scan all local live claims (`sharded_region.rs:178–181,277–287`). This is explicitly documented and correctness-preserving. Consider finer accounting only if a relevant mixed-lifetime workload measures it as material; more abstraction is not justified by the source observation alone.
5. **Focused file responsibilities / history prose.** `alloc_core/config/profile.rs` contains three stable public types (`SmallPoolPolicy`, `LargeCachePolicy`, `Profile`, at `:111,:160,:350`), unlike the one-export file convention's expressly sanctioned protocol-constant clusters. Splitting the policy types while retaining canonical reexports is a maintainability option, not a behavioral correction or semver change. More broadly, repeated task-history prose and obsolete helper names remain the already-indexed debt154; do not turn this review into a wholesale prose rewrite or restore retired ring code/tests.

### Candidates deliberately not promoted to confirmed defects

- **32-bit saturation arithmetic in decay:** `alloc_core_large_cache.rs:635` can under-release if multiply-before-divide saturates on a hypothetical 32-bit allocator. The actual root `alloc-core` gate rejects that configuration; ordinary supported 64-bit cache sizes do not establish the enormous overflow victim. The earlier xa round-2 review already noted this numeric-domain concern. It is not a new reachable 32-bit production finding here.
- **FIFO `seq` at wrap / `u64::MAX` sentinel:** `oldest_occupied_slot` starts `best_seq = u64::MAX` and selects strictly smaller sequences (`alloc_core_large_cache_eviction.rs:198–216`), while deposits wrap the counter. Numeric boundary behavior deserves a reduced-domain oracle if that policy is changed, but no practical valid-process witness reaching that horizon or new acceptance verdict is supplied.
- **Public internals are not automatically stable API.** Missing `non_exhaustive` on doc-hidden route enums is not a new stable semver issue. Nevertheless a genuinely unsafe hook needs its unsafe contract even there; visibility/doc-hidden alone is not a soundness argument.
- **Relaxed hints are not publications by themselves.** The epoch hint has a forced queue-check on an exhausted insert, mutex-protected queue state, and no demonstrated lost-index schedule here. Source inspection does not close item160 or171. Likewise pruning a zero-strong-count Weak cannot resurrect its destroyed token backing.
- **Process-lifetime storage is not by itself an unbounded leak.** Registry chunks and the started worker intentionally survive to process exit. Standalone `AllocCore` owns/drops its sidecars and segment reservations; R15-02 concerns accounting on failed transfer, not a revival of the previously fixed standalone sidecar leak.

## 5. Complete inherited-item dispositions

The parent supplied the round-start decision: **LEAVE each pre-existing item at its actual current state and trigger unless this source review establishes a specific new fact.** The lists below are mechanically extracted from the round-start numbered card headings in the performance index and all correctness ACTIVE/TRACKED files. This report does not claim to have repeated every card's historical experiment. Closed pointers stay closed; “LEAVE” never means relabeling them as open.

Each ID in each row individually receives **LEAVE — current status, verdict, next trigger, and historical closure record unchanged**, except item157 (CLOSED/SUPERSEDED and recorded in `RESOLVED.md`) and item158 (still OPEN, with its next trigger narrowed to the sync-only clear path described below). Items154 and13 receive evidence updates only; their status and trigger remain unchanged. No hardware, publication, Miri/Loom/Kani, CI-coverage, or performance-gate acceptance is performed by this review.

| Current card file | Actual IDs | Decision for every listed ID |
|---|---|---|
| `docs/perf/OPEN_ITEMS.md` current/retained cards | 1, 13, 81, 40, 41, 2, 3, 4, 5, 6, 14, 15, 25, 26, 28, 51, 29, 30, 31, 33, 38, 42, 48, 7, 8, 9, 10, 11, 12, 50, 52, 53, 54, 49, 44, 16, 17, 18, 19, 20, 21, 22, 23, 24, 34, 35, 27, 36, 37, 45, 39, 43, 47, 55, 56, 57, 58, 59, 60, 61, 62, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 78, 82 | LEAVE current status/trigger; includes retained closed cards7/10/15 |
| `docs/perf/OPEN_ITEMS.md` Recently-resolved moved cards | 79, 80 | LEAVE CLOSED / NO-GO archive pointers, not active cards |
| `docs/correctness-open-items/ACTIVE.md` | 1, 2, 62, 11, 13, 162, 163 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_hook_safety.md` | 5, 7, 8, 9 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_verification_coverage.md` | 17, 18, 41, 61, 84, 167, 171 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_platform_contracts.md` | 6, 26, 43, 44, 47, 48, 52, 53, 58, 59, 59a, 59b, 60, 152 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_ci_gate_coverage.md` | 19, 25, 50, 51, 54, 55, 64, 65, 70, 72, 73, 74, 76, 80, 82, 87, 88, 92, 95, 107, 140, 151, 156 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_test_flakiness.md` | 12, 14, 63, 69, 96, 143, 145, 146, 147, 150, 153 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_correctness_residuals.md` | 16, 22, 23, 66, 155, 164, 165, 166 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_publish_readiness.md` | 24, 27, 28, 29, 46, 85, 90, 91, 93, 97, 94, 98, 99, 100, 101, 102, 103, 104, 105, 106, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 141, 142, 144 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_process_record.md` | 10, 20, 21, 67, 68, 78, 79, 81, 83, 86, 89 | LEAVE current status/trigger/closed record |
| `docs/correctness-open-items/TRACKED_misc.md` | 45, 49, 154, 158, 159, 160, 161 | LEAVE prior statuses; item158 next trigger narrowed per R15; includes retained closed pointers45/49 |
| `docs/correctness-open-items/RESOLVED.md` (moved in R15) | 157 | CLOSED/SUPERSEDED: the legacy intrusive-spill mechanism and its described counters/observer are absent from current `src/`; no new telemetry API is proposed |
| R13/R14 recent correctness closures | 168, 169, 170 | LEAVE CLOSED; no reopening from unrelated findings |

Specific inherited boundaries requiring explicit mention:

- **164:** OPEN, owner-accepted known P1-box defect, not fixed or reclassified as a model limit. `Node::write_next` is still used by Small free-list reclamation; reading its source is not Miri acceptance.
- **171:** OPEN, observed Miri rejection of `crossbeam-epoch 0.9.20`'s default collector on the pre-existing experimental path. The R14 receipt locates it at `Local::element_of` during `epoch::pin()`. No rerun, dependency bump, skip, UB suppression, or false-positive conclusion here.
- **162/163:** LEAVE their existing end-to-end acceptance cards. Implemented sidecar/worker/high-alignment paths are not fresh platform/model acceptance.
- **166/167:** LEAVE their distinct invalid-input hardening and verification-mirror triggers; do not conflate either with accepted164.
- **154:** LEAVE the general prose/history cleanup trigger; R15-03 supplies precise additional examples, not closure of that broader debt.
- **157:** CLOSED/SUPERSEDED. Current `src/` has no live `RemoteFreeRing`/`HeapOverflow` type, `ring_overflows`/`cross_thread_frees_lost` field, or `dbg_spill_ledger_for_test`; foreign frees now publish through route pins and terminal sidecars. This closes only the retired intrusive-spill request, not a future metric need for the current sidecar design. Full closure narrative is in `RESOLVED.md`.
- **158:** LEAVE pending NUMA acceptance. `publish_empty` clears the addressed `(class_idx, slot_idx)` across all node buckets and is called by allocation and stale-candidate validation (`directory.rs:184–205`; `alloc_core_small_impl.rs:430–431,625–626`; `find_segment.rs:727–736`). Post-drain `sync_directory_for_segment_classes` still uses current-node-only `clear_bit` in its empty-class branch (`directory.rs:238–259`), so the possible stale bit is narrowed to that sync path. No NUMA runtime witness was run.
- **Perf81:** LEAVE INCONCLUSIVE on its wall-clock axes. **Perf82:** LEAVE НЕ СЕЙЧАС. **Perf72/78:** retain their existing owned measurement triggers. R15's unmeasured ideas do not discharge them.
- All remaining `RESOLVED.md` / `ARCHIVE.md`-only historical closures are intentionally unchanged. Their prose is historical, not evidence newly exercised for R15.

The round-start baseline contained **148 distinct correctness card IDs** in the ACTIVE/TRACKED files, including retained closed lookup records and171, and **74 performance card headings** in its Open-items section (including three struck-through retained records), plus separately moved79/80 pointers. R15 closes157 and adds active items172/173 (net 149 current ACTIVE/TRACKED IDs). The convention's numbered process instructions were excluded from the baseline census. R15-01/02 are filed as172/173; R15-03 updates existing154 rather than opening a duplicate.

## 6. Parent verification and remaining acceptance

1. **R15-01/R15-02 temporary witnesses:** a targeted two-test integration probe ran in the review worktree with a worktree-local target and `--all-features`; both tests passed. Assertions observed the panic-tail drop count (1, then 0) and the primordial rollback counter deltas (+1 reserved, +0 released). The throwaway test source was removed; no permanent regression or source fix was part of this review.
2. **Gate repair and verification:** the shared-target attempt selected a stale pre-R14 rlib. A worktree-local target exposed E0433 wording variants and E0460 dependency-mismatched `aligned_vmem` rlibs; the classifier was extended for those known candidate cases. The focused mixed-feature command `cargo test --locked -j 2 --features=alloc-global,internals,bench-internals --test r14_sidecar_owner_capability_negative -- --test-threads=1` passed **3/3**.
   The final `npm run check` passed all **65 steps** from a worktree-local target after reclaiming scratch targets. Process-local empty `RUSTC_WRAPPER`/`CARGO_BUILD_RUSTC_WRAPPER` bypassed machine-level sccache; no global Cargo configuration changed. Its IAI step produced **85 benches**, but none was used as evidence for §4's hypotheses.
3. **Post-push CI follow-up:** run `37644289617` on landing SHA `714ea5b73c0a5558913a4af5d7b8048042fdd2e6` exposed E0463 for an unqualified candidate rlib missing `rustversion`; commit `749dbfa9` added that candidate filter. A second run, `37681398463` on SHA `d5fb531bb13b065968b014007dc8e2fef114b724`, exposed E0461 because the host `rustc` probe consumed an aarch64 rlib. Commit `20b7c443` gates this host-rustc harness to x86_64 test targets; native x86_64 API checks remain enabled. These are test-harness target-selection issues, not allocator runtime failures.
4. **Remaining R15 acceptance:** permanent unwind and accounting regressions must precede source fixes; test the ownership/counter controls listed under each finding. Item171's collector/Miri issue is separate. R15-03 needs targeted prose correction and warning-strict docs, not an algorithm change.
5. Every optimization hypothesis in §4 remains unmeasured by a targeted judge. No speedup, promotion GO, or production feature change is claimed.

**Handoff:** xs produced the read-only source report. The parent independently verified the findings, ran and removed two temporary witnesses, repaired the compile-fail classifier and its cross-target test guard, updated current-state indexes, and passed `npm run check` (65 steps). R15-01/02 remain open; no production source or feature composition changed.

## Appendix A. Complete source-file census

Every listed file participated in the executable/declaration screen. “Tracked excerpt lines” counts distinct original source lines displayed by the Eval reader, including selected prose; it is not a proof score. All source line counts include comments and blanks.

| File | Physical lines | Tracked excerpt lines |
|---|---:|---:|
| `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs` | 469 | 236 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs` | 352 | 181 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs` | 57 | 11 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/perf_diag.rs` | 265 | 136 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs` | 366 | 142 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs` | 48 | 19 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/vmem.rs` | 99 | 39 |
| `src/alloc_core/alloc_core/alloc_core_impl.rs` | 671 | 118 |
| `src/alloc_core/alloc_core/bootstrap.rs` | 433 | 251 |
| `src/alloc_core/alloc_core/counters.rs` | 428 | 84 |
| `src/alloc_core/alloc_core/lifecycle.rs` | 556 | 386 |
| `src/alloc_core/alloc_core/mem/mem_impl.rs` | 696 | 245 |
| `src/alloc_core/alloc_core/mem/mod.rs` | 10 | 2 |
| `src/alloc_core/alloc_core/mem/realloc_fastpath.rs` | 561 | 172 |
| `src/alloc_core/alloc_core/mod.rs` | 104 | 36 |
| `src/alloc_core/alloc_core/sidecar_drain.rs` | 372 | 306 |
| `src/alloc_core/alloc_core/sidecar_test_hooks.rs` | 66 | 48 |
| `src/alloc_core/alloc_core/state.rs` | 233 | 60 |
| `src/alloc_core/config/large_cache_config.rs` | 490 | 127 |
| `src/alloc_core/config/large_cache_mode.rs` | 39 | 6 |
| `src/alloc_core/config/mod.rs` | 26 | 8 |
| `src/alloc_core/config/profile.rs` | 457 | 88 |
| `src/alloc_core/config/small_segment_pool_config.rs` | 199 | 49 |
| `src/alloc_core/large/alloc_core_large.rs` | 835 | 370 |
| `src/alloc_core/large/alloc_core_large_cache.rs` | 641 | 263 |
| `src/alloc_core/large/alloc_core_large_cache_eviction.rs` | 389 | 183 |
| `src/alloc_core/large/large_cache_extended.rs` | 258 | 33 |
| `src/alloc_core/large/mod.rs` | 27 | 8 |
| `src/alloc_core/large/reservation_state.rs` | 162 | 108 |
| `src/alloc_core/mod.rs` | 262 | 109 |
| `src/alloc_core/platform/mod.rs` | 24 | 7 |
| `src/alloc_core/platform/node.rs` | 580 | 152 |
| `src/alloc_core/platform/numa.rs` | 199 | 88 |
| `src/alloc_core/platform/os.rs` | 895 | 309 |
| `src/alloc_core/platform/sidecar.rs` | 511 | 86 |
| `src/alloc_core/platform/sidecar_stats.rs` | 166 | 70 |
| `src/alloc_core/platform/size_classes.rs` | 349 | 91 |
| `src/alloc_core/segment/bitmap/alloc_bitmap.rs` | 124 | 26 |
| `src/alloc_core/segment/bitmap/magazine_bitmap.rs` | 149 | 28 |
| `src/alloc_core/segment/bitmap/mod.rs` | 19 | 3 |
| `src/alloc_core/segment/bitmap/segment_bitmap.rs` | 118 | 48 |
| `src/alloc_core/segment/mod.rs` | 66 | 20 |
| `src/alloc_core/segment/remote_bitmap/bitmap_cut.rs` | 36 | 29 |
| `src/alloc_core/segment/remote_bitmap/bitmap_record.rs` | 6 | 5 |
| `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs` | 29 | 25 |
| `src/alloc_core/segment/remote_bitmap/mod.rs` | 9 | 9 |
| `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs` | 240 | 196 |
| `src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs` | 151 | 102 |
| `src/alloc_core/segment/segment_directory/directory_stats.rs` | 108 | 13 |
| `src/alloc_core/segment/segment_directory/mod.rs` | 106 | 3 |
| `src/alloc_core/segment/segment_directory/segment_directory_impl.rs` | 682 | 237 |
| `src/alloc_core/segment/segment_header/block_kind.rs` | 68 | 21 |
| `src/alloc_core/segment/segment_header/descriptors.rs` | 339 | 148 |
| `src/alloc_core/segment/segment_header/layout_asserts.rs` | 166 | 83 |
| `src/alloc_core/segment/segment_header/mod.rs` | 65 | 18 |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs` | 158 | 42 |
| `src/alloc_core/segment/segment_header/segment_header_impl.rs` | 763 | 201 |
| `src/alloc_core/segment/segment_header/segment_header_layout.rs` | 264 | 115 |
| `src/alloc_core/segment/segment_header/segment_header_meta_fields.rs` | 239 | 117 |
| `src/alloc_core/segment/segment_header/segment_header_views.rs` | 268 | 98 |
| `src/alloc_core/segment/segment_header/terminal_words.rs` | 164 | 129 |
| `src/alloc_core/segment/segment_layout.rs` | 244 | 76 |
| `src/alloc_core/segment/segment_table/active_kind_index.rs` | 145 | 127 |
| `src/alloc_core/segment/segment_table/active_kind_ops.rs` | 63 | 57 |
| `src/alloc_core/segment/segment_table/harness.rs` | 204 | 130 |
| `src/alloc_core/segment/segment_table/hash.rs` | 270 | 127 |
| `src/alloc_core/segment/segment_table/issue_transaction.rs` | 123 | 56 |
| `src/alloc_core/segment/segment_table/mod.rs` | 74 | 17 |
| `src/alloc_core/segment/segment_table/route_slots.rs` | 234 | 199 |
| `src/alloc_core/segment/segment_table/segment_table_impl.rs` | 994 | 399 |
| `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` | 976 | 498 |
| `src/alloc_core/small/alloc_core_small/dealloc.rs` | 146 | 64 |
| `src/alloc_core/small/alloc_core_small/directory.rs` | 263 | 122 |
| `src/alloc_core/small/alloc_core_small/find_segment.rs` | 751 | 399 |
| `src/alloc_core/small/alloc_core_small/mod.rs` | 37 | 15 |
| `src/alloc_core/small/alloc_core_small/reserve.rs` | 464 | 200 |
| `src/alloc_core/small/alloc_core_small/sidecar_drain_outcome.rs` | 9 | 8 |
| `src/alloc_core/small/alloc_core_small_diag.rs` | 386 | 187 |
| `src/alloc_core/small/alloc_core_small_magazine.rs` | 657 | 277 |
| `src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs` | 895 | 342 |
| `src/alloc_core/small/alloc_core_small_pool/decommit.rs` | 279 | 90 |
| `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs` | 432 | 115 |
| `src/alloc_core/small/alloc_core_small_pool/mod.rs` | 41 | 18 |
| `src/alloc_core/small/alloc_core_small_pool/segment_state_account.rs` | 36 | 8 |
| `src/alloc_core/small/alloc_core_small_pool/segment_state_reconciliation.rs` | 104 | 40 |
| `src/alloc_core/small/alloc_core_small_reclaim.rs` | 69 | 63 |
| `src/alloc_core/small/mod.rs` | 41 | 7 |
| `src/alloc_core/small/reserved_small_segment.rs` | 245 | 41 |
| `src/concurrent/epoch/epoch_handle.rs` | 102 | 55 |
| `src/concurrent/epoch/epoch_region.rs` | 693 | 603 |
| `src/concurrent/epoch/hand.rs` | 591 | 497 |
| `src/concurrent/epoch/mod.rs` | 10 | 3 |
| `src/concurrent/lock_free/lock_free_capacity.rs` | 14 | 6 |
| `src/concurrent/lock_free/lock_free_handle.rs` | 98 | 55 |
| `src/concurrent/lock_free/lock_free_page_table.rs` | 171 | 135 |
| `src/concurrent/lock_free/lock_free_region.rs` | 622 | 296 |
| `src/concurrent/lock_free/mod.rs` | 11 | 4 |
| `src/concurrent/mod.rs` | 38 | 19 |
| `src/concurrent/pinning.rs` | 271 | 68 |
| `src/concurrent/sharded/mod.rs` | 8 | 2 |
| `src/concurrent/sharded/sharded_handle.rs` | 95 | 53 |
| `src/concurrent/sharded/sharded_region.rs` | 812 | 372 |
| `src/global/alloc_stats.rs` | 174 | 58 |
| `src/global/exact_object/exact_fatal.rs` | 7 | 5 |
| `src/global/exact_object/exact_shard.rs` | 266 | 215 |
| `src/global/exact_object/exact_table.rs` | 40 | 29 |
| `src/global/exact_object/insert_outcome.rs` | 9 | 6 |
| `src/global/exact_object/mod.rs` | 11 | 6 |
| `src/global/exact_object/narrow.rs` | 122 | 122 |
| `src/global/fallback.rs` | 750 | 316 |
| `src/global/maintenance_service.rs` | 235 | 232 |
| `src/global/maintenance_start_error.rs` | 34 | 24 |
| `src/global/mod.rs` | 70 | 28 |
| `src/global/sefer_alloc/batch.rs` | 138 | 32 |
| `src/global/sefer_alloc/core.rs` | 375 | 71 |
| `src/global/sefer_alloc/diag.rs` | 503 | 167 |
| `src/global/sefer_alloc/global_alloc.rs` | 167 | 167 |
| `src/global/sefer_alloc/maintenance.rs` | 46 | 10 |
| `src/global/sefer_alloc/mod.rs` | 211 | 7 |
| `src/global/tls_heap.rs` | 649 | 157 |
| `src/kani_proofs.rs` | 223 | 132 |
| `src/lib.rs` | 524 | 84 |
| `src/registry/bootstrap/chunk.rs` | 107 | 18 |
| `src/registry/bootstrap/ensure.rs` | 262 | 54 |
| `src/registry/bootstrap/loom_shim.rs` | 465 | 231 |
| `src/registry/bootstrap/mod.rs` | 190 | 16 |
| `src/registry/bootstrap/registry.rs` | 379 | 101 |
| `src/registry/bootstrap/saturation.rs` | 73 | 49 |
| `src/registry/heap_core/alloc/batch.rs` | 305 | 141 |
| `src/registry/heap_core/alloc/hot.rs` | 813 | 340 |
| `src/registry/heap_core/alloc/mod.rs` | 5 | 2 |
| `src/registry/heap_core/core.rs` | 447 | 139 |
| `src/registry/heap_core/diag/diag_probes.rs` | 667 | 244 |
| `src/registry/heap_core/diag/mod.rs` | 9 | 2 |
| `src/registry/heap_core/diag/queries.rs` | 665 | 241 |
| `src/registry/heap_core/free/dealloc.rs` | 259 | 61 |
| `src/registry/heap_core/free/dealloc_batch.rs` | 392 | 97 |
| `src/registry/heap_core/free/dealloc_own_base.rs` | 554 | 221 |
| `src/registry/heap_core/free/mod.rs` | 18 | 5 |
| `src/registry/heap_core/free/realloc.rs` | 659 | 154 |
| `src/registry/heap_core/mod.rs` | 22 | 8 |
| `src/registry/heap_core/state/mod.rs` | 8 | 4 |
| `src/registry/heap_core/state/ownership.rs` | 157 | 67 |
| `src/registry/heap_core/state/tcache.rs` | 360 | 72 |
| `src/registry/heap_core/state/tcache_flush.rs` | 124 | 61 |
| `src/registry/heap_core_xthread/mod.rs` | 11 | 3 |
| `src/registry/heap_core_xthread/routing.rs` | 75 | 57 |
| `src/registry/heap_core_xthread/sidecar_drain.rs` | 102 | 70 |
| `src/registry/heap_registry/claim.rs` | 601 | 404 |
| `src/registry/heap_registry/counters.rs` | 460 | 137 |
| `src/registry/heap_registry/maintenance.rs` | 47 | 35 |
| `src/registry/heap_registry/mod.rs` | 66 | 66 |
| `src/registry/heap_registry/stack.rs` | 140 | 115 |
| `src/registry/heap_slot.rs` | 304 | 40 |
| `src/registry/mod.rs` | 81 | 81 |
| `src/registry/segment_route/directory.rs` | 909 | 739 |
| `src/registry/segment_route/error.rs` | 8 | 7 |
| `src/registry/segment_route/kind.rs` | 7 | 6 |
| `src/registry/segment_route/large_state.rs` | 70 | 52 |
| `src/registry/segment_route/mod.rs` | 39 | 25 |
| `src/registry/segment_route/pin.rs` | 63 | 43 |
| `src/registry/segment_route/registration.rs` | 82 | 62 |
| `src/registry/segment_route/route_cut.rs` | 22 | 17 |
| `src/registry/segment_route/route_record.rs` | 6 | 5 |
| `src/registry/segment_route/route_scan.rs` | 15 | 11 |
| `src/registry/segment_route/shard_lock.rs` | 89 | 59 |
| `src/registry/segment_route/small_sidecar.rs` | 165 | 119 |
| `src/registry/segment_route/terminal_publication_gate.rs` | 111 | 87 |

## Appendix B. Unsafe auto-trait and ownership-boundary review

The following is an occurrence inventory, **not** a list of nine defects. It records the reasoning boundary and remaining limit instead of claiming that an `unsafe impl` plus a comment is proof.

| Location | Manual implementation | Reviewed obligation / limit |
|---|---|---|
| `src/concurrent/epoch/hand.rs:572,582` | `AtomicSlot<T>: Send + Sync` for `T: Send + Sync` | Installed ownership additionally requires Send/'static; shared reads require Sync; epoch pin protects reads. Live-pointee unwind cleanup is R15-01; collector model residual171 remains. |
| `src/global/exact_object/exact_shard.rs:163` | `ExactShard: Sync` | System descriptor storage only under the shard lock, no user-T callback; prototype feature, not production. Arithmetic exhaustion was not converted into an unsupported runtime claim. |
| `src/global/sefer_alloc/global_alloc.rs:33` | `GlobalAlloc for SeferAlloc` | Raw pointer/layout caller contract forwarded; no unwind permission inferred from allocator shims. Accepted P1-box is not discharged. |
| `src/registry/bootstrap/loom_shim.rs:56,58` | `OncePtrCell<T>: Send + Sync` (unbounded) | Raw pointer publication, not safe dereference/ownership of arbitrary T; `cfg(loom)` mirror only, item167 still limits equivalence. |
| `src/registry/heap_slot.rs:283` | `HeapSlot: Sync` | Stable UnsafeCell storage, single CAS-granted owner/maintenance mutator, initialized publication. No by-value Send impl is asserted. |
| `src/registry/segment_route/shard_lock.rs:22,24` | `ShardLock<T>: Sync + Send` for `T: Send` | Release unlock / Acquire acquisition; guard's `PhantomData<&mut T>` makes T invariant and adds Sync's shared-guard bound. R13 closure retained. |

Raw wrapper/access boundaries considered:

- `EntryHandle` owns one descriptor refcount; `RoutePin` owns independent descriptor/sidecar lifetime, not the reservation. `RouteRegistration` borrows its directory and is !Sync for owner mutation. The R14 shared-mutator bypass is closed; a numerical incarnation does not validate arbitrary stale inputs.
- `RouteSlots`, `BlockIndex`, `SpareBlock`, and prototype `ExactShard::Inner` own or refer to private System allocations under documented owner/lock discipline. Moving storage does not create a second owning copy; unregister/pin-drop ordering is distinct from physical release.
- `AllocCore`, `SegmentTable`, `SegmentBitmap`, `SegmentMeta`, `BinTable`, and PageMap are owner-only raw-pointer internals. Private view constructors rely on their caller's live mapping and index domain. No claim is made that an internal `'static` atomic view lengthens the actual reservation lifetime; field-specific accesses and live call scopes carry that obligation.
- `AccountedSidecar` owns the VM span; generic raw views are lifetime-tied to their owner argument and their unsafe caller must prove exclusivity/valid initialization. Current concrete sidecar payloads do not contain arbitrary user destructors. That is not a general-purpose owning Box API.
- `HeapLease` / `MaintenanceLease` are !Send/!Sync capability values, with core borrows tied to `&mut` lease. TLS's raw cache must become TORN before release of the actual owner lease. A slot's static storage lifetime is not permission for two simultaneous core borrows.
- Experimental handles deliberately use `PhantomData<fn() -> T>` because they own numeric identity, not T. AtomicSlot actually owns/defer-transfers T and its typed atomic field constrains the relevant variance/auto-traits. ShardGuard has the mutable invariant marker required by its shared route to mutable storage.

## Appendix C. Complete unsafe-allow boundary inventory

Derived anchored-attribute census: **21 module-level allows and 83 item-scoped allows** in root `src/`. These count allowance attributes, not unsafe blocks, not reachable production seams under every cfg, and not proofs. The occurrence filter is `^\s*#!?\[allow\(unsafe_code\)\]`, excluding comment mentions. Module and item locations are separated below; feature/type/caller contracts were considered in the source screen and the named paths above.

### Module-level seams

| File | Attribute line |
|---|---:|
| `src/alloc_core/large/large_cache_extended.rs` | 117 |
| `src/alloc_core/platform/node.rs` | 37 |
| `src/alloc_core/platform/os.rs` | 28 |
| `src/alloc_core/platform/sidecar.rs` | 152 |
| `src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs` | 4 |
| `src/alloc_core/segment/segment_table/route_slots.rs` | 2 |
| `src/concurrent/epoch/hand.rs` | 51 |
| `src/global/exact_object/exact_shard.rs` | 4 |
| `src/global/exact_object/narrow.rs` | 4 |
| `src/global/fallback.rs` | 51 |
| `src/global/sefer_alloc/batch.rs` | 5 |
| `src/global/sefer_alloc/global_alloc.rs` | 6 |
| `src/global/tls_heap.rs` | 101 |
| `src/registry/bootstrap/ensure.rs` | 1 |
| `src/registry/bootstrap/loom_shim.rs` | 14 |
| `src/registry/bootstrap/registry.rs` | 1 |
| `src/registry/heap_registry/claim.rs` | 13 |
| `src/registry/heap_slot.rs` | 79 |
| `src/registry/segment_route/directory.rs` | 3 |
| `src/registry/segment_route/shard_lock.rs` | 6 |
| `src/registry/segment_route/small_sidecar.rs` | 2 |

### Item-scoped allowances

| File | Attribute lines |
|---|---|
| `src/alloc_core/alloc_core/alloc_core_core_diag/directory_diag.rs` | 383 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs` | 166 |
| `src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs` | 281, 308, 336 |
| `src/alloc_core/alloc_core/bootstrap.rs` | 207 |
| `src/alloc_core/alloc_core/lifecycle.rs` | 432 |
| `src/alloc_core/alloc_core/mem/mem_impl.rs` | 223, 254, 555, 631 |
| `src/alloc_core/alloc_core/sidecar_test_hooks.rs` | 31 |
| `src/alloc_core/large/alloc_core_large_cache.rs` | 86, 134, 278, 332, 376 |
| `src/alloc_core/segment/segment_directory/segment_directory_impl.rs` | 286, 314 |
| `src/alloc_core/segment/segment_header/segment_header_gen_table.rs` | 54, 97, 150 |
| `src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs` | 459 |
| `src/alloc_core/small/alloc_core_small/directory.rs` | 52, 112, 134 |
| `src/alloc_core/small/alloc_core_small/find_segment.rs` | 175 |
| `src/alloc_core/small/alloc_core_small/reserve.rs` | 450 |
| `src/alloc_core/small/alloc_core_small_diag.rs` | 147, 180, 214, 238, 377 |
| `src/alloc_core/small/alloc_core_small_magazine.rs` | 451 |
| `src/alloc_core/small/alloc_core_small_pool/decommit.rs` | 96 |
| `src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs` | 171, 192, 273, 350, 385 |
| `src/global/sefer_alloc/diag.rs` | 16, 261, 324, 370, 412, 452, 495 |
| `src/registry/heap_core/alloc/batch.rs` | 139, 182 |
| `src/registry/heap_core/alloc/hot.rs` | 76, 152, 380, 507 |
| `src/registry/heap_core/diag/diag_probes.rs` | 248, 320, 376, 472, 493, 514, 636, 657 |
| `src/registry/heap_core/free/dealloc.rs` | 218, 254 |
| `src/registry/heap_core/free/dealloc_batch.rs` | 186, 195, 210, 238, 277, 326, 371, 386 |
| `src/registry/heap_core/free/dealloc_own_base.rs` | 396, 458, 497, 549 |
| `src/registry/heap_core/free/realloc.rs` | 136 |
| `src/registry/heap_core/state/tcache_flush.rs` | 99 |
| `src/registry/heap_core_xthread/routing.rs` | 9, 24 |
| `src/registry/heap_core_xthread/sidecar_drain.rs` | 55, 91 |
| `src/registry/segment_route/pin.rs` | 45, 57 |

## Независимая проверка (oxx, 2026-10-08)

Раздел дописан без изменения текста выше (append-only). Проверяющий — oxx (Claude Opus 5.5, effort=max), один контекст, без суб-агентов, в изолированном worktree от `6a0d47f62b14eb724e027ab37054ad16037aa185`.

**Что прочитано:** этот отчёт целиком; `docs/perf/round-manifests/SRC_REVIEW_R15_MANIFEST.md`; все коммиты `b1a1e4f9..6a0d47f6` (`git log`, `git show`, `--stat`); записи R15 в `CHANGELOG.md`, `docs/CORRECTNESS_OPEN_ITEMS.md`, `ACTIVE.md`, `RESOLVED.md`, `TRACKED_misc.md`. `git diff b1a1e4f9..6a0d47f6 -- src/` пуст, поэтому код базы раунда (`b1a1e4f9`) и проверяемого дерева совпадают побайтно. Строки ниже относятся к обоим.

### Вердикты по находкам

| Находка | Вердикт | Обоснование |
|---|---|---|
| R15-01 | **CONFIRMED** (P3, достижимость, места, рекомендация) | `epoch_region.rs:682–692` вызывает `drop_value` в цикле; `hand.rs:526–540` обнуляет указатель и дропает значение; у `AtomicSlot` нет `Drop` (`hand.rs:584–591`). Собственный исполненный witness: паника в слоте 0 → `caught=true panicking_drops=1 later_live_drops=0`; контроль без паники `1/1`. Порядок слотов из отчёта (первая вставка → слот 1) подтверждён. Для `ShardedRegion` вывод сделан по включению `EpochRegion`-шардов; отдельным запуском **НЕ ПРОВЕРЕНО** |
| R15-02 | **CONFIRMED** (P3, места, механизм) | `lifecycle.rs:274–277`: `attach_owner(..)?` роняет `Primordial`; поле `segment` дропается раньше `table`; у `SegmentTable` нет `Drop`, а `routes` после отказа — `None`, так что use-after-unmap нет. RAII-освобождение идёт мимо `SEGMENTS_RELEASED_TOTAL` (`os.rs:557–566`). Собственный witness: reserved +1 / released +0; повтор +1/+0. Проверены все остальные места `Segment::reserve*`: пары `forget` + `release_segment` согласованы, так что дефект единственный. Fallback-вариант и Small/Large-откаты не запускались — **НЕ ПРОВЕРЕНО**, как и отмечено в отчёте. `crates/aligned-vmem/src/reservation.rs:1546–1567` не перечитывал |
| R15-03 | **CONFIRMED** (P4) | Текст `sharded_region.rs:644–651` и `registry/mod.rs:50–54` совпадает с цитатой. Дополнение: тот же `pub mod segment_route` помечен `#[allow(dead_code)]` (`registry/mod.rs:53`), хотя модуль живой в production; это маскирует действительно мёртвые элементы. Добавлено к item 154 |

### Вердикты по прочим пунктам проверки

| Пункт | Вердикт | Обоснование и поправки |
|---|---|---|
| §3, исправления R14 (`62b16ce9`) и закрытие R14-01…05 | **CONFIRMED** для R14-01/02/04 (исполнено); **PARTIAL** для R14-03/05 (только чтение) | Прочитан src-диф (6 файлов). 9 R14-тестов на неизменённом дереве — 9 passed. Контрфактуальные мутанты, каждый пойман и откачен `git checkout --`: `pub fn prepare` → падает только prepare-фикстура (`exit Some(0)`), issue/Sync-фикстуры зелёные, то есть запечатаны независимо; без раннего выхода в `prune_dead_claims` → `78 vs 0` проверок и `3 vs 2` sweep; `router_intact = true` → два late-TLS теста красные (`left: 1, right: 0`; TLS AccessError). Строки таблицы §3 сверены: `small_sidecar.rs:50,62`, `registration.rs:12,28,34`. Остаток: `SmallSidecar::scan` остался `pub &self` с doc «Owner-only»; это безвредно, потому что слово забирается целиком атомарным `swap`, и не возвращает дефект R14-01 |
| CI follow-up `247a52ad` (E0433) | **CONFIRMED**, не ослабляет | Добавлено точное совпадение второй формулировки rustc |
| `c82c88c5` (E0460) | **CONFIRMED** | Пропускаются только устаревшие кандидаты, чьи зависимости перезаписаны под той же хэш-меткой файла |
| `749dbfa9` (E0463) | **CONFIRMED**; механизм уточнён | По логу run `37644289617`, job `test (x86_64-unknown-linux-gnu)`: `can't find crate for rustversion which sefer_alloc depends on`. Под `--target` proc-macro `rustversion` (через `arc-swap`, `experimental`) лежит в host-deps, а кандидат из шага `--features experimental` его требует. Пропуск корректен |
| `20b7c443` / `b24a181f` | **PARTIAL**: E0461 устранён, но утверждение «Native CI retains these API-visibility checks» **ОПРОВЕРГНУТО** | macOS job (`macos-26-arm64`, host `aarch64-apple-darwin`) исполнял харнесс 3/3 на `714ea5b7` (job `112870910476`) и 0 на `6a0d47f6` (job `113076568267`). Гейт по архитектуре — неточная замена условия «host == target». Дополнительно: модульный doc и doc позитивной пробы перечисляют устаревшие коды пропуска; нет проверки, что совместима именно текущая сборка. Заведено как correctness item 175 (R16-02). `b24a181f` — только rustfmt |
| Вакуумность после follow-up'ов | **CONFIRMED не вакуумны** на x86_64 | Мутант `pub fn prepare` роняет тест; на arm64 тест не исполняется вовсе (см. выше) |
| Числа §1 и приложений A/C | **CONFIRMED** | Скрипт: 168 файлов, 43 317 физических строк, 17 034 непустых строк, не начинающихся с `//`. Все 168 строк приложения A совпали по физическим строкам; сумма «tracked excerpt lines» 18 629 сходится арифметически. Что именно показывал ридер, проверить нельзя — **НЕ ПРОВЕРЕНО**. Unsafe: 21 + 83 командой CLAUDE.md; выборочно сверены строки `os.rs:28`, `directory.rs:3`, `shard_lock.rs:6`, `routing.rs:9,24`, `pin.rs:45,57` |
| Версии в §1 | **CONFIRMED** | `Cargo.lock`: crossbeam-epoch 0.9.20, arc-swap 1.9.1, slotmap 1.1.1, aligned-vmem 0.2.0, numa-shim 0.2.0, once-ptr-cell 0.1.0, size-classes 0.1.0, sefer-region 0.2.0, sefer-alloc 0.3.0; edition 2021, MSRV 1.93 |
| §5, таблицы disposition | **PARTIAL** | Perf-список из 74 ID совпал с заголовками файла. Списки correctness по файлам и перепись (148 → 149; 140 `[T]` и 140 строк lookup) верны. **Пропущено:** item 22 (`RemoteFreeRing::DrainHeadPublish`) оставлен открытым, хотя его тип отсутствует в `src/` — та же логика, по которой закрыт 157. Закрыт как superseded в R16. **Ошибки индекса:** тонкий индекс писал «8 cards currently» при 9 фактических ACTIVE-карточках, а список «`[A]` tier currently contains 1, 2, 11, 13, 62, 162, and 163» не включал 172/173. Оба исправлены R16 (теперь 10 карточек с item 174) |
| §4, гипотезы | **CONFIRMED** как применимые | Двойной `guard.find` (`directory.rs:884`); повторный `canonical_block_of` (`dealloc_own_base.rs:347` после `routing.rs:22`); копия слов директории (`os.rs:706–724`); death-hint. Пункт 5 (`profile.rs`, три публичных типа) — **НЕ ПРОВЕРЕНО** |
| «Candidates not promoted» (§4) | **НЕ ПРОВЕРЕНО** | Насыщение decay в 32-битной арифметике, wrap FIFO `seq` — не перепроверялись |
| §6 «что запускалось» | **PARTIAL** | Утверждения автора «ничего не запускалось» согласуются с отчётом. Временные witness'ы родителя удалены и по коммиту не воспроизводятся, но их результат подтверждён независимыми witness'ами R16. «`npm run check` passed all 65 steps» и «IAI 85 benches» — **НЕ ПРОВЕРЕНО** (не перезапускалось) |
| §6.3 и манифест §4, CI-нарратив | **ОПРОВЕРГНУТО по порядку событий** | В отчёте и манифесте сказано, что run `37644289617` «exposed E0463», а второй run `37681398463` «exposed E0461». На деле первый run уже имел **два** красных job'а: `test (aarch64-unknown-linux-gnu)` с E0461 (`couldn't find crate sefer_alloc with expected target triple x86_64-unknown-linux-gnu`) и `test (x86_64-unknown-linux-gnu)` с E0463. `749dbfa9` исправил только второй, и E0461 повторился во втором run. Та же ошибка — в датированной записи ACTIVE item 13 |
| Статус CI | **CONFIRMED** | `6a0d47f6`: CI `37704825252` success (50 jobs success, 4 skipped), Kani `37704825276` success. `714ea5b7`: `37644289617` failure (2 jobs). `d5fb531b`: `37681398463` failure (1 job). У коммитов R14 (`7be55396`, `62b16ce9`, `b1a1e4f9`) собственных прогонов нет: первым их проверил красный `37644289617`; последний зелёный до R15 — `37505286215` на `d6417c6c` |

### Внесённые изменения статусов и записей (этот коммит)

- `docs/correctness-open-items/ACTIVE.md`: датированная поправка к записи item 13 (CI-нарратив, append-only); датированные записи независимого подтверждения к items 172 и 173. Статусы без изменений.
- `docs/perf/round-manifests/SRC_REVIEW_R15_MANIFEST.md`: добавлен §5 с поправкой CI-нарратива и ссылкой на этот раздел (append-only).
- `docs/CORRECTNESS_OPEN_ITEMS.md` (абзац R16) и `CHANGELOG.md` (запись R16): ссылка на эту проверку.
- Раньше, коммитом записи R16: закрытие item 22 и исправление переписи ACTIVE; новые items 174–176.

### Исполненные проверки и пределы

- `cargo test --locked --features "production internals bench-internals experimental" --test zz_r16_review_witness -- --test-threads=1 --nocapture` (временный файл, удалён; код — в приложении A отчёта R16) → 4 passed.
- `cargo test --locked --features "production internals bench-internals experimental" --test r14_sidecar_owner_capability_negative --test r14_shard_late_tls_teardown --test r14_shard_prune_work_bound -- --test-threads=1` → 9 passed; с тремя мутантами → 5 failed / 4 passed, ровно ожидаемые; после `git checkout --` дерево чистое.
- Только чтение CI: `gh run list --commit <sha>`, `gh run view <id> --json jobs`, `gh run view <id> --log-failed`, `gh run view --job <id> --log`.
- Пределы: Windows x86_64 host, debug-профиль, rustc 1.97.0. Не запускались Miri/Loom/Kani/TSan, release, MSRV, полный `npm run check`, aarch64/macOS локально. Feature-набор R15 (`alloc-global,internals,bench-internals`) не повторялся: использован более широкий набор с `production` и `experimental`.
