# External read-only review — Round 17

**Date:** 2026-07-25  
**Reviewed range:** `daf36de..a99314b`  
**Mode:** read-only audit of Git history, diffs, source files, tests, and
performance reports. No build, test, benchmark, or executable was run during
this review.

## Executive verdict

Round 17 is a useful wave. It fixed two real unsafe API defects, removed a
redundant heap-bootstrap cost, and root-caused a serious 4 MiB-per-cycle leak
under the opt-in `medium-classes` configuration.

The default `production` profile is genuinely faster when a new
`AllocCore`/heap is created: R17-3 removes a flat approximately 81,966
instruction initialization cost. The steady-state production hot path is
otherwise essentially unchanged.

The most important performance conclusion is still pending. R17-4 proved
that the old medium-class realloc judge measured code containing a real
Large-segment leak. Consequently, its headline “1,700–2,300× slower” verdict
is no longer valid evidence for the corrected code. Re-running that gate is
now the strongest candidate for discovering another substantial production
speedup.

## Findings

### P1 — the likely source of `STATUS_STACK_BUFFER_OVERRUN` is the test watchdog

`tests/race_repro.rs` documents two load-sensitive
`STATUS_STACK_BUFFER_OVERRUN` crashes as a possible Windows scheduler or
stack-guard artifact. The same file, however, contains a 20-second watchdog
which intentionally calls `std::process::abort()`:

- `DEADLINE_SECS = 20` at `tests/race_repro.rs:87`;
- `std::process::abort()` at `tests/race_repro.rs:112`.

On Windows an intentional abort/fast-fail can surface as
`STATUS_STACK_BUFFER_OVERRUN`. Both observed occurrences happened under heavy
system load, exactly when a normally-correct stress test can exceed a fixed
20-second deadline.

The most likely explanation is therefore that the watchdog itself produced
the reported status, rather than undiagnosed allocator memory corruption.

Recommended actions:

1. Replace `abort()` with a clearly distinguishable timeout exit code such as
   `process::exit(124)`.
2. Print elapsed time and current progress before terminating.
3. Make the timeout configurable and increase it on loaded CI workers.
4. Do not silently ignore the watchdog thread's join result.
5. Re-evaluate the flake classification after the timeout signal is
   unambiguous.

### P1 — the current `medium-classes` NO-GO was measured before the leak fix

R17-4 found that deallocation of a promoted and subsequently in-place-grown
Large block could be routed into the small magazine path because dispatch was
based on `class_for(layout.size())` rather than the segment kind. The Large
segment then never reached the Large deallocation/cache path and leaked.

The old R10/R14 realloc judge shows the same leak-shaped signature:

- roughly 324 segment reservations;
- roughly 1.3 GiB committed;
- repeated fresh 4 MiB reservations.

After R17-4, the pad-target probe changes from zero cache hits and roughly
1 GiB commit to approximately 68 MiB commit and 35–40 microseconds per growth
sequence. This confirms that the old result was materially contaminated by
the bug.

Nevertheless, `R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` still repeats the old
“1,700–2,300× slower” kill-gate result. The complete R10-2 judge was not
re-run after R17-4.

Required follow-up:

1. Re-run the R10-2 alloc/free/realloc gate on the corrected HEAD.
2. Measure at least:
   - `production`;
   - `production,medium-classes`;
   - `production,medium-classes,large-cache-extended`;
   - optionally the reserved-capacity combination.
3. Record wall time, commit, segment reservations, and cache hit rate.

This is the highest-priority performance experiment. Previous medium-class
alloc/free results were approximately 31×/211× faster, while the principal
realloc argument against production promotion is now stale.

### P2 — R17-4 adds a segment-kind load to every small free under `medium-classes`

The R17-4 correctness routing is necessary for promoted Large blocks.
However, the current implementation performs
`SegmentHeader::kind_at(base)` for every layout that classifies Small while
`medium-classes` is enabled, including tiny 16/32/64-byte frees.

The check is also compiled when `medium-classes` is combined with a feature
configuration in which promotion itself is compiled out, such as the
zero-headroom `exact-span-large` combination without reserved capacity.

The routing can be narrowed to the actual reachable domain:

```rust
if promotion_is_compiled
    && size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD
    && SegmentHeader::kind_at(base) == SegmentKind::Large
{
    // route by Large kind
}
```

A promoted block that shrinks below the promotion threshold uses the existing
Large-to-Small move leg, so it should no longer remain in a Large segment.
Tiny frees therefore do not need the header read.

Before any medium-class production decision, add a deterministic instruction
gate for small alloc/free under `production,medium-classes`. The existing
plain-production iai check proves only that the block compiles out when the
feature is disabled.

### P2 — `class-aware-dirty` still has no confirmed full-work throughput win

R17-7 added warm-up and repeated the process-level fixed-work comparison:

- run 1: approximately 53.39 ms off versus 54.62 ms on, not significant;
- run 2: approximately 53.46 ms off versus 54.71 ms on, not significant.

Across all four process-level measurements:

- one was statistically significant in the slower direction;
- three were not significant;
- none confirmed a full-round speedup.

The feature does produce a large improvement inside the narrow owner
allocation/drain window, so it may remain valuable as a tail-latency policy.
It has not been demonstrated as a full-work throughput improvement.

The “recoverability grounds” justification should also be stated carefully:
the sidecar OOM latch repairs the correctness of the per-class optimization
when that optimization is enabled. The baseline coarse dirty bitmap does not
need the per-class sidecar or its OOM transition.

Recommended decision:

- retain it explicitly as a latency policy if that is the product goal; or
- remeasure on an independently idle runner before calling it a production
  throughput optimization.

Measurements taken at a disclosed 80–100% background CPU load cannot settle
this decision.

### P3 — contradictory pad-target wording

The updated comment in `src/registry/heap_core_free.rs` correctly says that
the chosen policy is `nopad`, with no artificial padding. A later sentence
says “Padding is default”, which contradicts both the implementation and the
surrounding explanation.

This should read “No padding is the default” or equivalent.

## Confirmed improvements

### Heap bootstrap

R17-3 gates the primordial hash-table and free-list zero loops behind
`cfg(miri)`. On real targets the corresponding pages are newly reserved and
already zeroed by the OS. The structural writes that are not redundant—the
primordial hash insertion and `free_top = 0`—remain unconditional.

The report measures a flat reduction of approximately 81,966 instructions on
each iai scenario. This is a real improvement, but it is a one-time cost per
new `AllocCore`/heap, not a reduction in marginal steady-state
instructions-per-operation. Large relative percentages in very short
microbenchmarks must not be presented as long-running application throughput
gains.

### Medium-class Large-segment leak

R17-4 fixes a real resource defect: a promoted Large segment with a
small-classifying final layout could be misrouted into the magazine and never
released or cached.

For the affected probe, the fix changes the behavior from approximately:

- 249 distinct segments;
- zero Large-cache hits;
- roughly 1 GiB committed;

to approximately:

- 68 MiB committed;
- normal Large-cache reuse;
- 35–40 microseconds per growth sequence.

This is a major improvement for the opt-in configuration, even though it does
not affect plain `production`.

### Unsafe boundaries

The R17-1 and R17-2 repairs look sound under static review:

- `sidecar::reserve_zeroed_with` now passes a raw `*mut T` to the repair
  closure and never materializes `&mut T` before `T` is valid;
- `SegmentDirectory::init_node_ids_raw` uses raw field writes;
- the directory raw-read helpers are now `unsafe fn`;
- their current call sites carry local validity and ownership arguments.

## Next large optimization opportunities

### 1. Re-evaluate medium classes after R17-4

This is the strongest immediate candidate. The old negative realloc gate no
longer describes the corrected implementation.

The particularly important combination is medium promotion plus the extended
Large cache. The R10 workload holds 16 objects while the base cache has only
8 slots; the extension provides enough slots, and the current 256 MiB budget
can hold sixteen 4 MiB spans.

If the corrected realloc gate clears, the already-measured medium alloc/free
wins may become practically promotable.

### 2. Build one adaptive Large policy

The following mechanisms should be measured as a coordinated policy rather
than unrelated feature switches:

- medium-to-Large promotion;
- geometric reserved capacity;
- lazy commit;
- cache extension triggered by slot pressure;
- a finite byte budget.

Individually they already show large workload-specific improvements. Their
interactions currently dominate the result and have repeatedly invalidated
isolated conclusions.

### 3. Narrow the R17-4 kind check

Match the exact promotion feature predicate and promotion threshold before
reading the segment kind. This is a small but important prerequisite for a
fair medium-class performance gate.

### 4. Batched deferred reclaim

The R17-10 design correctly discovers that directory synchronization is
already batched per segment. The remaining opportunity is narrower:

- replace N per-block `dec_live_and_maybe_decommit` calls with the existing
  batched sibling;
- optionally defer finalization of several segments emptied in one sweep.

This is likely a constant-factor cleanup rather than a radical improvement.
The proposed counter-level Stage 1 gate is appropriate; sub-design B should
not be implemented unless multi-empty sweeps are common enough to matter.

### 5. Page-run medium layer

If the corrected medium gate remains limited by cache pressure, the stronger
architectural solution is a page-run layer for approximately 256 KiB–2 MiB
objects:

- several medium objects share a segment;
- adjacent free runs allow in-place growth;
- fewer dedicated OS reservations;
- lower external fragmentation;
- less dependence on the number of Large-cache slots.

This has more potential than continuing to add fixed medium size classes.

## Project-level recommendations

1. Mark a performance verdict stale when a later correctness fix changes the
   code path that the verdict measured.
2. Do not make production A/B decisions on a host already measured at
   80–100% CPU load.
3. Report startup-per-heap, narrow-window latency, full fixed-work throughput,
   and steady-state marginal cost as separate axes.
4. Make watchdog failures distinguishable from corruption failures.
5. Add iai coverage for the enabled `medium-classes` profile, not only
   feature-off non-disturbance.
6. Keep compact summaries and machine-readable CSV/JSON in Git; store large
   raw logs as CI artifacts.
7. Do not treat the deterministic trim primitive test as a resolution of the
   separate TLS teardown ordering flake.
8. Keep hot-path comments focused on current invariants and safety arguments;
   move experimental history into `docs/perf`.

## Final assessment

Round 17 materially improves the project:

- default heap initialization is faster;
- two unsafe API gaps are closed;
- a severe opt-in Large-segment leak is fixed;
- the next reclaim optimization is scoped more honestly.

It does not establish a new steady-state production throughput breakthrough.
The most promising next step is not further scalar micro-tuning: it is
re-running the medium-class gate after the leak fix, preferably together with
the extended/adaptive Large-cache policy. That experiment may overturn the
project's current medium-class NO-GO and expose the next genuinely radical
speedup.
