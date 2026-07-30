# Read-only review: Round 29 and its post-review follow-up

Date: 2026-07-30

Reviewed range: `b7ff9fe..68e20195c1b2507a3ea8b7bc40a717c82d06beaf`
(22 commits).

Baseline choice: `b7ff9fe` is the `HEAD` covered by the preceding
`2026-07-29-r28-readonly-review.md`; the reviewed endpoint is the current
committed `HEAD`, including Round 29, the first independent R29 review, and
follow-up commit `68e2019`.

Review mode: strictly read-only except for creating this report. I inspected
Git history, diffs, source, tests, benches, raw logs, and documentation. I did
**not** build, test, lint, benchmark, run examples, execute project scripts, or
change allocator code. Therefore all statements below are static source and
evidence reviews, not fresh runtime verification. The pre-existing untracked
`.claude/` directory was not touched.

---

## Executive verdict

### Did this wave actually make the allocator faster?

**No, not for ordinary users of the shipped `production` configuration.**

This is not an adverse interpretation: it is the round's own accurate result.
`CHANGELOG.md` says, correctly, **“Runtime improvements this round: 0.”**
Static inspection agrees:

- `production`'s feature composition did not change;
- no normal production algorithm or default was replaced;
- the `src/` additions are overwhelmingly feature-gated diagnostic counters,
  measurement hooks, safety guards, or measurement-only accounting types;
- the `(8, 32 MiB)` small-segment-pool setting was documented, not made the
  default;
- `virgin-zero-skip` remains opt-in;
- the decommit/reservation, promotion-frequency, alloc-hit, and large-cache
  tasks measured or classified existing behavior rather than accelerating it.

Round 29 is therefore a **correctness, measurement, and project-hygiene wave**,
not a speed wave. It still has real value:

- two unsafe diagnostic entry points were made honestly `unsafe` and moved
  behind `bench-internals`;
- a foreign-pointer header dereference received the missing containment check;
- NUMA feature coverage moved into per-PR CI;
- a genuine flaky-test-oracle defect was reproduced and corrected;
- stale/open performance decisions were indexed;
- large-cache retention was finally measured and exposed as a serious product
  default trade-off;
- post-review commit `68e2019` fixed two build/lint defects and corrected the
  invalid R29-16 wall-clock interpretation.

The strongest remaining likely speed opportunity is **already-implemented
`virgin-zero-skip`**, but its only native wall-clock judge is invalid and has
not been replaced. The project must repair the judge before claiming or
shipping a win. Beyond that, the evidence increasingly says that generic hot
small-allocation churn is near a local optimum: large future gains will be
workload/profile/API specific, not another scalar-path micro-tweak.

### Highest-priority findings

1. **P1, diagnostic-only memory safety:** the safe
   `dbg_decomp_full_cycle` hook can release the segment it just installed as
   `small_cur`, leaving a dangling allocation cursor and a possible
   use-after-unmap on the next small allocation. The paired
   `dbg_decomp_reserve_and_keep` / `dbg_decomp_release` interface has the same
   state hazard. It is gated by `bench-internals`, so this is not reachable in
   plain `production`, but it is still an unsound safe API in measurement
   builds.
2. **P1, strongest speed gate still open:** R29-16's wall-clock “virgin” bench
   recycles its blocks after the first Criterion iteration. Follow-up
   `68e2019` correctly withdrew the conclusion but did not redesign or rerun
   the judge. The real wall-clock value of `virgin-zero-skip` remains unknown.
3. **P1, product/RSS:** default large-cache headroom converges to roughly
   238–241 MiB retained **per materialized heap**, while pure idle time
   reclaims nothing because decay is event-driven. At 32 heaps, the measured
   cache state corresponds to roughly 7.4–7.5 GiB even after forced
   convergence, and 9 GiB at the report's 288 MiB/heap pre-decay point. This
   is more important to users than most remaining single-digit hot-path ideas.
4. **P2, measurement correctness:** R29-3 labels arithmetic means as medians,
   mixes a full-payload-touch workload with a more general verdict, and calls
   the proposed tier a “net loss” even though its direct A′−B comparison
   measured a small saving in both recorded runs.
5. **P2, process:** the round's “zero-trust” per-task workflow still landed
   two build/lint breaks that the subsequent review found. The CI matrix and
   feature reachability rules should be executable/generated rather than
   reconstructed manually for every task.

---

## 1. Review of the code changes

### 1.1 Soundness fixes: good and correctly scoped

The following changes are sound improvements based on source inspection:

- `tls_heap::dbg_restore_local_for_test` is now an `unsafe fn`, is gated by
  `bench-internals`, and documents the thread-local pointer contract.
- `AllocCore::dbg_force_decommit_retain_for` is now an `unsafe fn`, is gated
  by `alloc-decommit + bench-internals`, and states the missing
  `live_count == 0` precondition.
- `HeapCore::dbg_directory_bit_for_ptr` now validates that the derived segment
  base belongs to the heap before reading `SegmentHeader` metadata. Returning
  `None` for a foreign pointer matches the function's safe API contract.
- `dbg_mark_local_torn_for_test` remains safe but is no longer reachable from
  plain `production`; its body mutates a TLS marker without dereferencing a
  caller-controlled pointer, so keeping it safe is reasonable.

These changes do not accelerate the allocator. They reduce the chance that
measurement/test support turns an invalid input into memory corruption.

### 1.2 P1: `dbg_decomp_*` still leaves a dangling `small_cur`

The open concern recorded by the preceding review is confirmed by the current
source, and the uncertainty text in `docs/CORRECTNESS_OPEN_ITEMS.md` is based
on a mistaken call trace.

The actual sequence is:

1. `AllocCore::dbg_decomp_full_cycle` calls `self.reserve_small_segment()`.
2. `reserve_small_segment()` itself ends with:

   ```rust
   self.small_cur = base;
   Some(base)
   ```

3. The hook immediately calls `release_or_pool_empty_segment(base)`.
4. If the pool is full, that function calls
   `release_empty_segment_now(...)` and then `self.table.recycle(base)`.
5. Neither function restores or clears `small_cur`.
6. The next small allocation starts by using `self.small_cur`, which can now
   refer to a released/recycled segment.

The R29-3 harness deliberately pre-fills the pool and repeatedly drives the
release branch, so the hazardous state is not hypothetical. Its current
caller happens not to perform another ordinary small allocation after the
measurement sequence; that avoids an in-tree crash but does not make the safe
hook sound.

`dbg_decomp_reserve_and_keep()` also calls `reserve_small_segment()`, and the
later `dbg_decomp_release(base)` can release that same current segment.
Marking only the raw-pointer release half `unsafe` does not fully express the
state invariant: even a correct pointer can leave the allocator unusable.

Recommended fix, in descending order:

1. Create a measurement-only reservation primitive that performs the desired
   OS/table/metadata work without changing the allocator's active
   `small_cur`.
2. Or save the previous valid cursor and restore it after releasing the probe
   segment, with explicit assertions that the restored segment is still
   registered.
3. If neither is practical, make the whole state-invalidating operation
   `unsafe` and document “no further allocation through this heap,” though
   that is a weaker design than preserving allocator validity.

Also add a counterfactual test: fill the pool, call the full-cycle hook, then
perform and free a normal small allocation on the same heap. The test must
fail with the old hook and pass after the fix.

This issue is confined to `bench-internals`; it is **not evidence of a
shipping allocator UAF**.

### 1.3 The new tripwire is useful, but its guarantee is narrower than stated

`tests/dbg_hook_safety_tripwire.rs` is a worthwhile regression guard. It
enumerates today's safe pointer-shaped `pub fn dbg_*` surface and forces new
members through review.

It does not “close the bug class for good”:

- `has_bench_internals_cfg()` accepts any cfg attribute whose text merely
  contains `"bench-internals"`. It would incorrectly accept
  `not(feature = "bench-internals")` and a permissive
  `any(feature = "bench-internals", ...)`.
- the scanner only selects safe `pub fn dbg_*` signatures containing
  `*mut`/`*const`;
- integer-address/slot/offset APIs that mutate metadata are outside scope;
- zero-argument state-invalidating hooks are outside scope;
- safe hooks returning an integer-encoded address are outside scope.

The live `dbg_decomp_full_cycle` problem demonstrates the zero-argument hole.

Recommended improvement:

- split the policy into two mechanical rules:
  1. all crate-public `dbg_*` hooks must be `bench-internals` gated unless
     explicitly allowlisted as pure observers;
  2. every safe mutating hook must be allowlisted with a short invariant
     justification, independent of parameter syntax;
- parse Rust attributes structurally (`syn` plus a cfg-expression parser or a
  small exact grammar), rather than testing substrings;
- keep a separate allowlist for pure observers, bounds-checked mutators, and
  unsafe-by-contract hooks.

### 1.4 R29-1 fixed the false positive, but weakened the generic leak oracle

Replacing the windowed
`released_delta <= reserved_delta` assertion with the lifetime cumulative
`released_total <= reserved_total` invariant correctly removes the reported
cross-window false positive and still detects impossible double release.

It is not, by itself, a meaningful leak detector: a missing release only makes
the inequality more comfortably true. The strong per-base before/after proof
is compiled only under `alloc-decommit + alloc-xthread`; other feature
combinations retain only the weak cumulative check.

Recommended improvement:

- rename/split the assertions so the cumulative invariant claims
  “no over-release/double-release accounting,” not “no leak”;
- retain a feature-specific test whose title explicitly promises the per-base
  leak proof;
- for configurations without that diagnostic surface, assert a bounded
  registry/live-count delta or state clearly that leak coverage is absent.

### 1.5 Follow-up `68e2019`: correct fixes, but not full closure

Static inspection confirms that the follow-up commit contains the fixes it
claims:

- four no-op IAI stubs now exist when `virgin-zero-skip` is absent;
- `SegmentStateAccount`, `SegmentStateReconciliation`, and their impl are
  gated by `bench-internals`;
- the doc-comment lint was reworded;
- the R29-16 report now contains a dated correction marking its wall-clock
  result unconfirmed;
- the CHANGELOG now correctly describes the containment-guard solution rather
  than claiming that hook was converted to unsafe.

Because this review was forbidden from running builds, these are source-level
verifications only. More importantly, `68e2019` **documents** the invalid
wall-clock judge but does not replace it, and it files rather than fixes the
remaining hook/tripwire/oracle issues.

---

## 2. Review of the performance evidence

### 2.1 R29-2: `(8, 32 MiB)` is a real opt-in result, not a new speedup

README now gives the useful, correctly paired recipe:

```rust
SmallSegmentPoolConfig::new()
    .pool_segments(8)
    .pool_byte_cap(32 * 1024 * 1024)
```

The documented evidence is appropriately narrow: approximately 22% lower
elapsed time and 9→0 decommit calls in the 1024-byte,
batch-120 churn-with-teardown victim, at a cost of approximately +8 MiB
committed RSS per materialized heap.

This is a project/product improvement: users can now opt into an already
measured trade-off. It does not make `SeferAlloc::new()` faster.

### 2.2 R29-3: the reservation-only conclusion is too broad

The gate measures:

- reserve/release without touching payload;
- raw OS reserve/release;
- `MADV_DONTNEED` plus re-fault of every payload page;
- a full reserve→touch-every-page→release cycle.

Two methodology defects are visible directly in
`examples/r29_3_decomposition_gate.rs`:

1. It prints “median of 200,” but computes one total elapsed duration divided
   by `N`; the component timers are also accumulated and divided by `N`.
   These are arithmetic means, not medians.
2. The “irreducible” arm writes one byte to all roughly 1,006 payload pages.
   That is a full-touch-density workload, not a general small-allocation
   overflow workload.

The recorded direct comparison says:

- run 1: current A′ ≈ 2,215,980 ns versus reservation-only floor
  ≈ 2,102,388 ns — about 113,592 ns or 5.1% saved;
- run 2: current A′ ≈ 2,190,767 ns versus floor
  ≈ 2,154,111 ns — about 36,656 ns or 1.7% saved.

Calling the proposal a “net loss” because the standalone decommit syscall
cost exceeds the avoidable reserve/table overhead is inconsistent with the
gate's own end-to-end A′−B result. The honest conclusion is:

- at full payload touch, the measured design has a small positive ceiling,
  not a radical win;
- at sparse touch, the fixed `MADV_DONTNEED` cost may dominate and reverse
  the result;
- the original low-touch churn victim was not reproduced by this gate;
- therefore the general reservation-only tier remains **NO PRIORITY /
  workload-sensitive**, not mathematically proven negative for all shapes.

Do not implement this tier before a touch-density sweep shows a real victim.

### 2.3 R29-4: useful reconciliation, with a naming caveat

The state accounting convincingly explains the cap-8 post-drain residual:

- cap 4 retains one primordial and one active small segment;
- cap 8 retains one primordial and two active small segments;
- the extra 4 MiB is exactly one additional `small_active` segment, not an
  unidentified leak bucket.

The `committed_bytes` field is modeled from state and segment/span size; it is
not an OS query of physically committed pages. That is acceptable for the
eager small-segment path measured here, but the report/type documentation
should call it **modeled committed bytes** so it is not reused as an RSS or
working-set oracle under lazy commit.

### 2.4 R29-5: “rare” uses the wrong population for the headline

The recorded workload has:

- 33 promotions;
- 60,722 total allocation events;
- 4,040 distinct growth objects;
- 40 deliberately large/population objects capable of approaching the
  promotion region;
- all 33 copies exactly 128 KiB; total copied bytes ≈ 4.1 MiB.

`33 / 60,722 = 0.054%` and `33 / 4,040 = 0.82%` are arithmetically correct but
mostly measure how much unrelated background/small work was mixed into the
denominator. Among the 40 designated large-growth objects, 33 promotions is
**82.5%**.

The useful result is not “promotion is intrinsically rare.” It is:

- promotion work is a small fraction of this whole mixed workload;
- promotion is common among the objects intentionally grown into that region;
- aggregate copy volume is only ~4.1 MiB in this chosen mix, so Linux
  sub-region `mremap` has no measured end-to-end victim here.

Keep `mremap` deferred for general production, but do not use the 0.82%
headline to reject it for a promotion-heavy consumer workload.

### 2.5 R29-10: no radical win remains in alloc-hit clearing

The report isolates approximately 12.19 Ir/hit for the clear-on-hit block,
about 54.5% of its measured 22.38 Ir magazine-pop path. This is a useful cost
decomposition, not a demonstrated removable cost.

The directory/segment-base probe and bitmap read-modify-write exist to preserve
real allocator invariants. Adding parallel per-slot base metadata or another
cache purely to remove the mask/probe would enlarge the hot magazine state and
likely trade a few instructions for more loads/cache footprint. Given the
project's repeated negative results in this region, closing this line of
microtuning is reasonable.

### 2.6 R29-13: the round's most important operational finding

The large-cache gate found:

- approximately 288 MiB cached per heap immediately after teardown in its
  pressure shape, independent of headroom because no decay tick fires;
- pure idle time reclaims zero at every tested headroom;
- forced convergence reaches near-zero/one-span granularity at 0 and 16 MiB,
  roughly 34–37 MiB at 64 MiB, and roughly 238–241 MiB at the shipped
  256 MiB headroom;
- the default is per heap/shard and the cache budget is unbounded unless the
  user configures it.

This is a product-default issue, not an allocator speedup. The README explains
the knobs and now calls the defaults throughput-tuned, but the magnitude
deserves a much more prominent warning next to `SeferAlloc::new()`.

Two report comparisons should be corrected:

- “256 MiB is 32× the small pool's 8 MiB **delta**” compares an absolute
  large-cache floor to the incremental cost of changing the small-pool
  profile. It is not an apples-to-apples absolute-retention ratio.
- 256 MiB is 16×, not 32×, a 16 MiB small-pool cap.

No lower-headroom throughput/hit-rate A/B exists in this wave, so the report
does **not** justify changing the default yet. It strongly justifies running
that A/B and exposing ready-made RSS/balanced/throughput profiles.

### 2.7 R29-16: real skipped work, unmeasured native benefit

The single-shot IAI arms demonstrate that the opt-in feature takes a different
path:

- virgin 64 KiB `alloc_zeroed`: 3,067 Ir;
- recycled/explicit-zero path: 65,624 Ir;
- reported ratio: approximately 21.4×.

This is valid evidence that explicit zeroing work was skipped. It is **not**
evidence that native code becomes 21.4× faster: Callgrind's instruction
accounting of a bulk memset/REP operation is not a hardware-time model.

The wall-clock judge does not repeatedly exercise virgin bump-carving:
`bench_virgin` frees the full batch inside each `b.iter` closure call, so the
next iteration consumes the recycled free list. Follow-up `68e2019` correctly
documents this. The source bench itself remains structurally unchanged.

Current verdict:

- mechanism correctness: supported;
- large instruction-work reduction: supported;
- native wall-clock improvement: **unknown**;
- production promotion: not yet justified.

---

## 3. What can still be accelerated strongly?

The following queue is deliberately ordered by expected value and evidence,
not novelty.

### P0 — repair the `virgin-zero-skip` native judge

This is the cheapest high-upside task because the implementation and
correctness tests already exist.

Required judge design:

1. Compare feature OFF versus ON in separately built, immutable binaries.
2. Use one-shot subprocesses or `iter_batched` with a fresh heap/segment in
   untimed setup.
3. In the timed region, allocate genuinely never-served blocks and do not free
   them until after the measured batch.
4. Add a path-activation oracle/counter proving how many calls were virgin
   bump-carves and how many were recycled pops; reject the sample unless the
   intended path dominates.
5. Sweep at least 4, 16, 64, and 128 KiB.
6. Measure three consumer behaviors:
   - return from `alloc_zeroed` without touching;
   - read one byte per page;
   - fully read/write the allocation.
7. Cross with `small-segment-lazy-commit`, because eager commit and page-touch
   policy can change where the saved work appears.
8. Use paired process-level sampling and report native time/distribution, not
   an IAI ratio as a speed ratio.

Promotion rule: only consider adding the feature to `production` if at least
one realistic calloc-heavy victim wins materially and no recycled/hot-churn
family regresses beyond the project's normal kill gate.

Potential: large for `alloc_zeroed`-heavy, never-before-served medium/small
blocks; approximately zero for ordinary `alloc` and recycled hot churn.

### P1 — measure and package large-cache profiles

This may not make a nanosecond microbench faster, but it can radically improve
real deployment efficiency by preventing multi-GiB retained-cache states.

Run a paired matrix for headroom 0/16/64/256 MiB and finite budgets, measuring:

- large-cache hit rate and alloc/free latency;
- mixed small/large application throughput;
- peak and post-idle RSS;
- syscall rate;
- 1, 8, and 32 materialized heaps;
- burst→idle→burst behavior.

If 64 MiB or 16 MiB preserves most hit-rate/latency benefit, either lower the
default in a release decision or ship named `rss`, `balanced`, and
`throughput` const profiles. Add an explicit trim/scavenge API for applications
that know when a phase has ended; pure idle cannot trigger today's inline
decay.

Potential: enormous RSS reduction; speed effect unknown until the paired gate.

### P1 — make the existing small-pool throughput profile first-class

The `(8, 32 MiB)` profile has a measured ~22% win on a real cliff victim and a
clearly quantified RSS cost. Documentation is good, but discoverability can be
better than a builder recipe:

- provide a named const/profile constructor;
- add a profile-comparison table next to the main allocator example;
- add one application-shaped cross-thread/server gate so the decision does
  not depend on a single teardown micro-workload.

This is a user-selectable strong speedup already available today, not a new
algorithm.

### P2, consumer-triggered — adopt `batch-api` only with a real caller

Earlier evidence reports roughly 1.1–1.6× over the production scalar path for
the implemented mechanism. There is still no in-tree/downstream consumer.

Do not promote an API in the abstract. If a queue, arena, object pool, parser,
or storage engine can naturally request/free batches, integrate it there and
measure the whole consumer. Without adoption, allocator benchmark speedup is
zero product speedup.

### P2, workload-triggered — Linux sub-region `mremap`

R29-5 does not show a general victim, so no unconditional implementation is
justified. Reopen only when a real consumer demonstrates large numbers of
medium→Large promotions or meaningful copied-byte volume. For that workload,
33/40 promotable objects in the current synthetic population suggests the
mechanism can be relevant even though it is diluted in the whole-workload
denominator.

### P2 — post-fix `small-segment-lazy-commit` combined gate

The feature remains opt-in based partly on historical pre-fix complexity and
syscall behavior. Its post-R8-10 steady-state native result was never closed.
Measure it only as a combined startup/RSS/calloc experiment, preferably in the
same matrix as `virgin-zero-skip`; do not run another isolated feature survey
without an application-shaped victim.

### Explicitly do not reopen

Current evidence does not justify more work on:

- magazine clear/flush microtuning;
- full flush or alternative `FLUSH_N` values;
- run-encoded free lists without a batch consumer;
- CLZ/scanning replacement for the class LUT;
- generic reservation-only overflow tiers before a touch-density victim;
- NUMA's old O(S) scan issue — node-indexed directory routing already fixed it
  in R11-6;
- unconditional medium/wide-class promotion, whose realloc regressions remain
  decisive.

---

## 4. What should improve in the codebase and project?

### 4.1 Treat measurement hooks as a separate unsafe subsystem

The recurring `dbg_*` defects show that `#[doc(hidden)]` is not isolation.
Recommended architecture:

- place unsafe/stateful benchmark hooks in one module or a separate internal
  harness crate;
- compile the module only with `bench-internals`;
- expose opaque typed handles instead of caller-supplied raw pointers/bases;
- make destructive operations consume the handle, preventing double release;
- preserve allocator validity after every safe hook;
- forbid diagnostic hooks from mutating `small_cur` unless the mutation is
  explicitly restored.

This reduces both audit surface and the chance that a measurement creates the
very UB it is trying to observe.

### 4.2 Generate the feature/check matrix

Round 29's own review discipline missed:

- missing IAI stubs in the default perf-gate feature set;
- ungated dead-code definitions under plain `production`;
- a doc lint in another CI combination.

The solution is not another prose reminder. Define feature bundles once in
machine-readable data and generate:

- CI rows;
- local check commands;
- benchmark required-features validation;
- “feature absent” compile checks for every conditionally registered IAI arm.

At minimum, add plain `production` clippy and the exact perf-gate default
feature command to required per-PR CI.

### 4.3 Require path-activation oracles in every performance judge

R29-16 joined a recurring class of benches that measured a different path than
their label implied. Every judge should record/assert its mechanism:

- virgin bump-carves versus recycled pops;
- cache hits versus misses;
- decommit/release calls;
- promotions;
- directory hits/fallbacks;
- pool cap actually resolved and victim actually activated.

If the intended path does not account for the required fraction of operations,
the judge must fail rather than emit a performance verdict.

### 4.4 Make reports data-driven and arithmetic-checked

R29 produced correct raw evidence but several misleading headlines:

- mean labeled median;
- absolute quantity compared with an incremental delta;
- wrong 256/16 ratio;
- whole-workload denominator presented as mechanism frequency;
- direct end-to-end saving described as net loss.

Recommended workflow:

1. write raw per-sample structured data first;
2. derive summary CSV/JSON and Markdown tables from one checked script;
3. print sample statistic names from the code that actually computes them;
4. require every percentage to name numerator and denominator;
5. distinguish absolute retention from delta retention;
6. add arithmetic assertions for headline ratios;
7. require a clean immutable commit identity before measurement, not an
   unverifiable hash assembled after the fact.

### 4.5 Separate “code speed,” “configuration speed,” and “measured knowledge”

The CHANGELOG does this correctly at the top of Round 29, but individual task
titles still use `perf(...)` for measurement-only work. Use explicit tags:

- `perf(runtime)` — shipping algorithm/default changed;
- `perf(opt-in)` — feature/profile code changed;
- `bench` / `measurement` — only a judge/report changed;
- `docs(config)` — an existing tuning option was documented.

This prevents a wave with many performance reports from being mistaken for a
wave that accelerated user code.

### 4.6 Present memory policy at allocator construction

The default large cache is unbounded by budget, has 256 MiB per-heap headroom,
and does not decay during pure idle. Put the measured implication directly by
the `SeferAlloc::new()` quick-start, including that the value is per
materialized heap/shard. Offer an RSS-oriented example/profile before users
need to discover a 400-line gate report.

### 4.7 Keep the active indexes small and current

Splitting `OPEN_ITEMS.md` into an active index and archive was the right move.
Continue with:

- one compact current verdict per active item;
- append-only history in the archive/report, not repeated in the active entry;
- link corrections rather than duplicating full narratives;
- automatically check that every `CONDITIONAL-GO`/`NEVER-DECIDED` feature has
  exactly one active owner/next trigger.

---

## 5. Recommended Round 30 work plan

### Phase A — correctness before new measurements

1. Fix `dbg_decomp_*` so no safe hook can leave a dangling `small_cur`.
2. Add the fill-pool→hook→normal-allocation counterfactual.
3. Strengthen the tripwire to cover all public diagnostic mutators and parse
   cfg predicates correctly.
4. Rename/split R29-1's over-release versus leak assertions.

Exit criterion: measurement builds preserve allocator validity after every safe
hook; diagnostic policy no longer depends on raw-pointer signature text.

### Phase B — one decisive speed experiment

5. Replace R29-16's invalid Criterion design with the one-shot/fresh-heap,
   activation-proven wall-clock matrix described above.
6. Run feature OFF/ON A/B with immutable source identities.
7. Decide `virgin-zero-skip`: promote, keep opt-in with named profile, or close
   as native NO-GO for the measured victims.

Exit criterion: a native wall-clock verdict backed by a judge that proves it
actually exercises virgin allocations.

### Phase C — memory/product decision

8. Run the lower-headroom throughput/RSS matrix.
9. Add named RSS/balanced/throughput configurations and an explicit trim API
   proposal.
10. Decide whether 256 MiB/heap remains an acceptable default.

Exit criterion: default/profile decisions compare both latency/hit-rate and
multi-heap retained memory.

### Phase D — only consumer-led optimization

11. Integrate `batch-api` only when a real caller appears.
12. Reopen Linux `mremap` only with a promotion-heavy victim.
13. Stop scalar microtuning unless a fresh profile identifies a new dominant
    production cost.

---

## Final assessment

Round 29 did **not** accelerate shipped code, and it should not be presented as
having done so. It improved safety, CI coverage, documentation, and knowledge
of the allocator's real trade-offs. The follow-up commit repaired the two
confirmed build/lint problems visible in source and honestly withdrew an
invalid wall-clock conclusion.

The round also reveals the next priorities clearly:

- first fix the diagnostic `small_cur` use-after-release hazard;
- then obtain the missing trustworthy native verdict for
  `virgin-zero-skip`;
- treat large-cache retention as a product-default decision;
- package already-proven configuration/API wins for the workloads that can
  use them;
- stop searching for universal 10× gains in a hot scalar path whose remaining
  candidates have repeatedly measured in single digits or regressed.

The credible route to another strong win is now **specialization with an
activated victim**—calloc/virgin pages, a throughput pool profile, batching, or
promotion-heavy Linux growth—not another broad claim that the default
allocator became faster when its runtime code did not change.
