# Read-only review of the new waves after R29

**Date:** 2026-07-30  
**Reviewed range:** `68e2019..14a9ef3`  
**Commits:** 20  
**Previous review endpoint:** `68e2019`  
**Current endpoint:** `14a9ef34145cc62188d734cf6987bcfd4dbcb088`

## Scope and method

This is a static, read-only review performed at the user's request.

I inspected only:

- Git history and committed diffs;
- source files;
- tests, benches, examples and scripts as text;
- existing raw-data summaries and reports as committed files.

I did **not** run builds, tests, Clippy, rustfmt, Miri, Kani, benchmarks,
examples, scripts or generated-table tools. Consequently, conclusions about
runtime results below are a fact-check of the committed implementation and
measurement methodology, not an independent remeasurement.

The untracked `.claude/` directory was not touched.

## Executive verdict

### Did these waves make the default allocator faster?

**No. Round 30 contains zero default-runtime acceleration.**

That is consistent with the round's own `CHANGELOG.md` statement:
`Runtime improvements this round: 0`.

Static evidence:

- `production` is still
  `alloc-global + alloc-xthread + alloc-decommit + fastbin +
  alloc-segment-directory + primordial-lazy-commit + class-aware-dirty`;
- neither the `production` feature composition nor the default config changed;
- the only production-reachable algorithmic edit,
  `reserve_small_segment`, is now an inline wrapper around the old body and
  still publishes `small_cur` for all three production callers;
- `Profile::{Rss,Balanced,Throughput}` and `SeferAlloc::with_profile` are new
  **opt-in configuration APIs**, not a change to `SeferAlloc::new()`;
- the remaining commits are correctness, diagnostics, measurement, CI,
  process or documentation work.

### Was the wave valuable?

**Yes, mainly for correctness and engineering discipline.**

The strongest real code improvement is R30-1: it closes a confirmed
measurement-hook use-after-release path where a released segment could remain
published as `small_cur`. R30-2 then widens the structural tripwire intended
to prevent the same class of diagnostic-hook defect.

R30-5/R30-8/R30-9/R30-11/R30-12/R30-14 also improve evidence quality,
feature-matrix coverage, test naming and ownership of deferred decisions.

### Is the project at the limit of radical acceleration?

**No. The default scalar small-allocation hot path is close to a local
micro-optimization limit, but several workload-specific multiplicative
opportunities remain.**

Most importantly, R30 itself appears to have rejected one of them using the
wrong allocator layer: the R30-3 `virgin-zero-skip` NO-GO does not measure the
production `SeferAlloc -> HeapCore` magazine path. That decision should be
reopened before any further work treats the feature as exhausted.

## Review of the production changes

### R30-1: dangling `small_cur` fix — correct and useful

Commit: `25433c3`

The old measurement hooks called `reserve_small_segment`, which published the
new segment into `self.small_cur`, and then could release that reservation
without restoring the cursor. A later small allocation could dereference a
cursor into released memory.

The refactor in `src/alloc_core/alloc_core_small.rs` separates:

- `reserve_small_segment_impl`: reserve/register/initialize without publishing
  the cursor;
- `reserve_small_segment`: production wrapper which calls the helper and then
  publishes `self.small_cur`.

The two diagnostic reserve/release paths now use the cursor-free helper.
The three production callers still use the publishing wrapper. This is the
right separation: the diagnostic hook no longer mutates allocator liveness
state it cannot safely own.

This is a correctness fix, not a runtime speedup.

One adjacent diagnostic-only problem remains explicitly open:
`docs/CORRECTNESS_OPEN_ITEMS.md` records that the native-Windows R29-3
decomposition example can touch decommitted pages without recommitting them.
That does not affect the default allocator path, but the example should not be
presented as a usable Windows judge until fixed.

### R30-7: named profiles — useful API, but not yet trustworthy as broad policy names

Commit: `b5efe8c`

The implementation is mechanically simple:

| Profile | Small pool | Large-cache headroom |
|---|---:|---:|
| `Rss` | 4 segments / 16 MiB | 16 MiB |
| `Balanced` | 4 segments / 16 MiB | 64 MiB |
| `Throughput` | 8 segments / 32 MiB | 64 MiB |

Every other large-cache knob remains at its normal default, including
`budget_bytes = None` in a normal production build.

The API is opt-in and therefore does not regress existing users. The concern
is semantic: the names imply wider guarantees than the measurements support.

#### `Profile::Rss` is not an RSS cap

`headroom_bytes` is an eventual decay floor, not an admission limit:

- the cache budget remains unbounded in ordinary `production`;
- decay is event-driven;
- idle time alone does not run decay;
- a burst can therefore leave substantially more than 16 MiB per heap
  resident indefinitely until later traffic or thread teardown.

`Rss` can reasonably mean "lower eventual headroom", but today it is easy to
read it as "memory-bounded". Either:

1. rename it to something like `LowHeadroom`;
2. state directly in its rustdoc that it is **not** an RSS bound and point to
   `.budget_bytes(...)`; or
3. give it a finite, measured budget and gate that policy separately.

#### `Profile::Throughput` contains an unproven throughput reduction

The profile increases the small pool from 4/16 MiB to 8/32 MiB, but also
reduces large-cache headroom from the default 256 MiB to 64 MiB.

R30-6 proves parity only in a workload whose rounded cached working set fits
inside 64 MiB. It does not prove that 64 MiB preserves hit rate for working
sets between 64 and 256 MiB. A profile called `Throughput` should not reduce
that cache window without same-regime evidence.

Until that gate exists, safer choices are:

- keep 256 MiB for `Throughput`;
- split the axes into named presets such as `SmallPoolThroughput` and
  `LowLargeCacheHeadroom`;
- or make the current limitation prominent in the profile documentation.

## Findings ordered by importance

## P0 — R30-3's production NO-GO measures the wrong allocator layer

Files:

- `benches/r30_3_virgin_zero_skip_native_gate.rs`;
- `src/global/sefer_alloc.rs`;
- `src/registry/heap_core_alloc.rs`;
- `src/alloc_core/alloc_core_small_magazine.rs`;
- `src/registry/tcache.rs`;
- `tests/r13_3_magazine_virgin_hit_skips_zero.rs`;
- `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md`;
- `docs/perf/OPEN_ITEMS.md`, item 19.

This is the most important new finding.

### What the R30-3 judge actually measures

The judge deliberately constructs `AllocCore::new()` and times
`AllocCore::alloc_zeroed()` directly. It does not use:

- `HeapCore::alloc_zeroed`;
- `SeferAlloc`;
- a real `#[global_allocator]`.

On that substrate-only path, `carve_block_with_refill` returns one freshly
carved block and places 31 proactive blocks into the intrusive free list.
Later direct `AllocCore` calls pop those blocks as ordinary free-list entries,
where the interface no longer carries a virgin flag. This is the source of
the judge's `VIRGIN_BATCH = 1` and its claimed structural 1-in-32 ceiling.

That observation is correct **for the bare `AllocCore` path**.

### What production actually does

The production call chain is different:

```text
GlobalAlloc::alloc_zeroed
  -> SeferAlloc::current_heap
  -> HeapCore::alloc_zeroed
  -> alloc_small_zeroed_via_magazine
  -> refill_magazine_slow_virgin
  -> AllocCore::refill_class_bump_virgin_checked
```

For a bump-carved refill:

1. `refill_class_bump_virgin_checked` emits a per-slot `u16` virgin mask;
2. `HeapCore` stores retained bits in `PerClass::virgin_mask`;
3. subsequent magazine hits read and clear the corresponding bit;
4. those later blocks can also skip the explicit zero pass.

`TCACHE_CAP` is 16 and the refill byte budget is 64 KiB, so production can
preserve virginity for an entire freshly carved refill, not only its first
block. For example, the refill count is bounded approximately as follows:

| Class size | Production refill count |
|---:|---:|
| 4 KiB | 16 |
| 16 KiB | 4 |
| 64 KiB | 1 |
| 128 KiB | 1 |

The existing
`tests/r13_3_magazine_virgin_hit_skips_zero.rs` already asserts the critical
mechanism: after a pure carve refill, every retained magazine slot is marked
virgin, and the second `HeapCore::alloc_zeroed` magazine hit skips explicit
zeroing too.

Therefore the R30-3 statement that same-class production bursts are
structurally capped at about `1 / 32` is false for the normal
`production + virgin-zero-skip` configuration, because `production` includes
`fastbin` and uses the magazine path.

### Why this invalidates the promotion verdict

The feature was being judged for inclusion in `production`, but the benchmark
excluded the production layer that preserves its activation.

The direct-`AllocCore` judge can still answer a narrower substrate question,
but it cannot support:

- the production hit-rate ceiling;
- the production wall-clock NO-GO;
- closure of `OPEN_ITEMS.md` item 19;
- a claim that the mechanism is exhausted.

There is a second oracle weakness: the OFF binary's zero-pass counter does not
provide a symmetric production-path activation proof. The critical evidence
must be per arm and must establish that each binary went through the expected
production branch, not merely that its config compiled.

### Required correction

Reopen the R30-3 production decision and build a real gate:

1. separate OFF/ON binaries using an actual `#[global_allocator]`, or call
   `HeapCore::alloc_zeroed` on genuinely fresh isolated heaps;
2. test same-class bursts, not only one call per heap;
3. include 4, 16, 64 and 128 KiB, but interpret the latter two knowing their
   refill count is one under the current 64 KiB budget;
4. record magazine miss/hit, refill count, virgin-mask activation and explicit
   zero-pass counts per arm;
5. keep setup and teardown outside the timer;
6. measure no-touch, sparse-touch and full-touch consumers separately;
7. decide promotion only from that production result.

This is not a request to promote the feature now. It is a finding that the
current NO-GO has not measured the target implementation.

## P1 — R30-6 combines hit-rate and RSS results from incompatible regimes

Files:

- `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md`;
- `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv`;
- `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md`;
- `src/alloc_core/profile.rs`;
- `docs/perf/OPEN_ITEMS.md`, item 27.

The R30-6 hit-rate workload is described as `8 x 6 MiB = 48 MiB`, but the
allocator rounds each requested span to an 8 MiB usable span. The measured
cache working set is therefore 64 MiB.

That explains the exact result:

- 64 MiB headroom: 8/8 hits;
- 256 MiB headroom: 8/8 hits.

In this experiment, the 256 MiB arm's additional 192 MiB is never needed.
The 64-vs-256 parity is consequently real but narrow: it is parity while the
working set is at or below 64 MiB.

The headline then combines this with R29-13's roughly sevenfold retention
difference, measured under a much larger fill and forced-drain regime. The two
facts do not jointly prove:

> 64 MiB gives 256 MiB's hit rate while using seven times less RSS

for the same workload. The hit-rate test does not enter the range where 64
and 256 can differ; the retention test does.

This extrapolation is now copied into public profile rustdoc, so it is no
longer only a report-wording issue.

### Required experiment

Use one matrix that crosses both thresholds:

- rounded cached working sets: 32, 64, 96, 128, 192, 256 and 384+ MiB;
- headrooms: 0, 16, 64, 128 and 256 MiB;
- real decay opportunities between bursts;
- in the same arm, record retained bytes, evictions, next-burst hit rate and
  next-burst latency;
- run through the real global allocator for the decision-facing axis.

Until then:

- 64 MiB is a valid low-headroom opt-in candidate;
- it is not proven throughput-equivalent to 256 MiB generally;
- the `Balanced`/`Throughput` rustdoc should say "parity at a 64 MiB rounded
  working set", not broad "full-hit-rate parity".

## P1 — R30-7's application-shaped gate never differentiated the profiles

File:
`docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`.

The review-response correctly added the missing per-arm activation table:

- default: `decommit_calls_total = 40`;
- throughput: `decommit_calls_total = 40`;
- large-cache hits are also identical.

Thus the treatment did not change the mechanism it was intended to test in
that workload. The wall-clock null result cannot validate or reject the
small-pool part of `Profile::Throughput`.

The corrected report also states a minimum detectable effect around 18.8%.
So the result is both:

- mechanism-inert for the intended pool effect;
- too weak to exclude a material effect below roughly the stated detection
  floor.

### Strong next experiment

Sweep the paired pool settings through the real allocator:

```text
(4 segments, 16 MiB)
(8 segments, 32 MiB)
(16 segments, 64 MiB)
(32 segments, 128 MiB)
```

For every arm require:

- requested and resolved config;
- separate decommit/reserve counts;
- a **between-arm** mechanism delta;
- wall-clock;
- committed/RSS retention per heap and process-wide.

The interesting threshold is the first cap that changes `40 -> 0` or at
least sharply reduces 40. If no reasonable cap does that, this server shape
is not a victim for pool sizing.

## P2 — the widened diagnostic-hook tripwire still accepts `cfg_attr`

File: `tests/dbg_hook_safety_tripwire.rs`, `has_bench_internals_cfg`.

The scanner searches for the five-byte prefix `#[cfg`. That prefix also
matches `#[cfg_attr(...)]`.

A shape such as:

```rust
#[cfg_attr(feature = "bench-internals", inline)]
```

can therefore be parsed as though `feature = "bench-internals"` were the
function's real availability gate, even though it is only the condition for
applying another attribute.

No current live hook was found using this exact bypass shape, so this is
latent rather than an existing exposure.

Fix:

- require the exact token shape `#[cfg(`, allowing whitespace deliberately if
  desired;
- add a negative `cfg_attr(feature = "bench-internals", ...)` test;
- ensure the parser consumes the whole attribute expression.

## P2 — `HeapCore::dbg_large_cache_hits` unnecessarily widens the shipping surface

File: `src/registry/heap_core_diag.rs`.

The new method is documented as measurement-only but is gated only by
`alloc-decommit`, not by `bench-internals`. It is safe and read-only, so this
is not a soundness defect. It is nevertheless inconsistent with the new rule
that measurement-only hooks should not ship in ordinary production builds.

Gate the `HeapCore` delegation with `bench-internals`; the R30-6 probe already
uses that feature where the method is needed.

## P2 — generated CI matrix is not yet the only source of truth

Commit: `3c3ad7d`

The manifest-driven check/test matrix is a good improvement and adds two rows
which would have caught recent feature-combination breaks.

However, the Clippy rows in the workflow remain separately hand-transcribed.
The local generated command covers `check` and `test`, not the entire CI
contract. The claim "single source of truth" is therefore stronger than the
implementation.

Either:

- include Clippy in the manifest and generator; or
- add a structural workflow-vs-manifest consistency test.

## P2 — R30-6 data hygiene issues remain open

The review response filed but did not repair several report-quality problems:

- the "single-digit KiB" idle-delta wording does not match every summary row;
- at least one raw RSS row contains an impossible collapse and is not marked
  excluded;
- the summary CSV still carries a placeholder-like provenance field;
- the latency null lacks a decision-facing MDE in the original headline;
- the latency workload keeps every arm at 100% hits, so it cannot expose the
  cost of hit loss caused by a smaller headroom.

The medians make the main narrow hit-rate observation robust, but future
profile decisions should use regenerated tables with explicit exclusion
rules.

## Per-wave assessment

| Work | Static review verdict | Default speed impact |
|---|---|---:|
| R30-1 cursor safety | Correct fix for diagnostic UAF hazard | 0 |
| R30-2 hook tripwire | Better, one `cfg_attr` blind spot remains | 0 |
| R30-3 virgin judge | Wrong layer for production verdict; reopen | 0 |
| R30-4 report corrections | Useful evidence repair | 0 |
| R30-5 CI matrix | Good partial centralization | 0 |
| R30-6 headroom A/B | Narrow result over-generalized into profiles | 0 |
| R30-7 profiles | Useful opt-in API; policy names/evidence need refinement | 0 by default |
| R30-8 activation rule | Strong process improvement | 0 |
| R30-9 derived-table rule | Strong process improvement | 0 |
| R30-10 hook isolation design | Typed handle is the useful part | 0 |
| R30-11 leak-oracle split | Better test honesty | 0 |
| R30-12 commit tags | Better history semantics | 0 |
| R30-13 retention docs | Important deployment warning | 0 |
| R30-14 owner tripwire | Better deferred-work ownership | 0 |
| `14a9ef3` review response | Corrects all four filed P1 claims, leaves P2s open | 0 |

## What can still be accelerated strongly

The following list is ordered by expected value and evidence quality, not by
implementation convenience.

### 1. Re-measure production `virgin-zero-skip` — highest-priority reopening

Potential target:

- `alloc_zeroed` / calloc-like workloads;
- 4-16 KiB objects benefit from multi-slot virgin magazine refills;
- 64-128 KiB currently refill one at a time, but can still skip a full
  software memset on genuine virgin allocations.

Why it can be strong:

- the production machinery already carries the required per-slot state;
- the expensive work being removed is proportional to allocation size;
- sparse-touch consumers may avoid both software bandwidth and unnecessary
  page touching;
- R30-3's low activation conclusion does not apply to the production path.

The first step is measurement, not new allocator code. A valid gate may show
GO, NO-GO or a size/touch-specific promotion boundary.

### 2. Find the real small-pool threshold for server-shaped workloads

R27-4 already measured about 22% on its victim workload when cap 8 changed
decommit calls from 9 to 0. R30-7's workload still produced 40 decommits in
both cap-4 and cap-8 arms.

This suggests a threshold problem, not proof that pooling has no value. A
cap sweep with an explicit mechanism-delta gate can find:

- a strong win at cap 16/32 with a measurable RSS cost; or
- a clean reject for this workload.

Do not tune scalar instructions in this region before answering that
algorithmic/configuration question.

### 3. Revisit `large-cache-extended` for diverse Large-size turnover

The existing R14-5 gate is one of the strongest workload-specific results in
the repository:

- base 8 slots: 33.3% hits;
- extended 40 slots: 100% hits;
- reported process-level elapsed time:
  `338,987,200 ns -> 939,900 ns` on the narrow 24-distinct-size turnover
  workload.

The mechanism avoids real `VirtualAlloc`/`VirtualFree` round trips, so a large
ratio in exactly that workload shape is plausible. It is not a universal
default win:

- broader scans have a cost;
- retention can multiply;
- the feature now has a finite fallback budget, but it remains per heap;
- the measurement should be refreshed on current code and current budget.

This is an excellent opt-in/profile candidate for services with many recurring
Large sizes. It should be promoted only with:

- current-code A/B;
- finite budget;
- narrow N=1/2/4 regression gate after sidecar materialization;
- multi-heap RSS accounting.

### 4. Promote the batch mechanism only with a real consumer

Earlier rounds measured the implemented batch path around 1.1-1.6x faster
than production scalar calls in relevant shapes. The project correctly
declined public API expansion without a consumer.

The next useful action is not more micro-tuning. It is an integration with a
real downstream owner:

- object pool;
- arena;
- ECS/storage slab;
- runtime task allocation;
- a crate already allocating/freeing homogeneous groups.

Then gate end-to-end latency and retained memory. Without adoption the
allocator's default `Box`/`Vec` path gains nothing.

### 5. Page-run / medium arena only when a real victim appears

The page-run design promises roughly 3-6x higher density for the 1.25-2 MiB
uniform-object regime and avoids one-segment-per-object reservation pressure.
That is algorithmically large, but the repository's own workload search did
not find a present victim, and medium-class promotion caused a severe realloc
regression.

Keep it conditional on a workload with thousands of simultaneously live,
uniform 1.25-2 MiB objects or a demonstrated segment-table/reservation-syscall
bottleneck. It is not the next general-purpose optimization.

## What should be improved in the code

### 1. Make reserve/release diagnostic ownership typed

R30-10's full hook-module relocation is not worth its footprint, but its
consume-on-release handle proposal directly addresses the real defect class.

Replace raw diagnostic reserve/release pairing with an opaque handle that:

- cannot be copied;
- records whether it owns a reserved segment;
- is consumed by release;
- cannot leave `small_cur` published;
- has a clear drop policy for unfinished probes.

This narrows future UAF/double-release opportunities much more effectively
than file relocation.

### 2. Separate profile dimensions

`Rss/Balanced/Throughput` currently combine two largely independent controls:

- small empty-segment pooling;
- Large-cache headroom.

This makes evidence from one workload silently choose policy for another
allocator tier. Prefer composable named policies or a profile builder:

```text
small_pool = Default | Throughput
large_cache = LowHeadroom | Default | DiverseTurnover
memory_bound = None | PerHeap(bytes)
```

The existing low-level config remains the escape hatch.

### 3. Add an explicit trim/scavenge API

The source and R29/R30 reports agree that idle time does not reclaim cached
Large memory. This is a product limitation, not merely documentation.

A safe explicit trim API would let applications reclaim at known lifecycle
boundaries:

- end of request burst;
- worker parking;
- level/map unload;
- memory-pressure callback;
- container memory warning.

The design must define thread/heap ownership and cross-thread behavior, but it
would give `Rss`-sensitive deployments a deterministic control unavailable
today.

### 4. Keep measurement hooks out of normal public builds

Use `bench-internals` consistently for measurement-only observers and typed
actions. A safe observer is lower risk than a raw-pointer mutator, but every
shipping hidden public hook is still surface that must remain compatible and
audited.

## What should be improved in the project

### 1. Add an “allocator layer under test” field to every gate

Every decision-facing report should state one of:

- raw `AllocCore`;
- `HeapCore`;
- `SeferAlloc`;
- real `#[global_allocator]`;
- real downstream application.

For promotion into `production`, the decision must include the production
layer unless the report proves that the omitted layers are behaviorally
irrelevant. R30-3 is precisely the failure this rule would prevent.

### 2. Require a between-arm mechanism delta

“The mechanism fired in both arms” is insufficient. The report must prove the
treatment changed the intended mechanism:

```text
delta_mechanism = treatment_count - control_count
```

If that delta is zero, the performance comparison is not decision-facing for
that mechanism even if both arms are busy.

R30-7's `40 -> 40` decommit result is the canonical counterexample.

### 3. Keep cost and benefit in the same workload regime

Do not combine:

- hit-rate parity where the smaller capacity is not exceeded;
- RSS savings measured where it is exceeded;

into one Pareto claim. The same arm should cross the policy boundary and
measure cost, benefit and latency together.

### 4. Derive every table, including corrections

R30-9's rule is good. Complete it by making:

- raw log;
- exclusion list;
- summary CSV;
- Markdown table;
- headline ratios;
- MDE / confidence interval

outputs of one checked transformation. A review-response correction should
regenerate the authoritative derived artifact rather than append another
hand-maintained interpretation layer.

### 5. Finish CI manifest centralization

Put check, test, Clippy and any release-only feature rows in the same
manifest, or verify workflow equivalence structurally. The matrix should
actually be singular, not singular for only two command kinds.

### 6. Distinguish current verdict from historical text

Append-only corrections preserve audit history, but long reports now make it
easy to quote a superseded early headline. Put a generated current-verdict
box at the top and move the complete immutable narrative below it.

`OPEN_ITEMS.md` should reopen item 19 after the wrong-layer finding above and
weaken item 27's broad 64-vs-256 parity claim to the measured 64 MiB working
set.

## Recommended next wave

### P0 — evidence correction

1. Reopen `virgin-zero-skip` production NO-GO.
2. Build a real `HeapCore`/global-allocator multi-call virgin gate.
3. Correct `R30_3`, `OPEN_ITEMS.md` item 19 and the Round 30 CHANGELOG
   promotion wording according to that result.

### P1 — strong speed candidates

4. Run the small-pool cap threshold sweep with per-arm decommit delta and RSS.
5. Refresh `large-cache-extended` on current code for the diverse-size
   turnover victim, with finite budget and narrow-workload regression gates.
6. If a real downstream batch consumer exists, integrate the batch API and
   judge end-to-end rather than reopening allocator-only micro-tuning.

### P1 — policy correctness

7. Rework or clarify named profiles:
   - `Rss` is not an RSS bound;
   - `Throughput` should not lower large-cache capacity without a crossing
     workload;
   - split small-pool and large-cache axes if possible.
8. Implement the explicit trim API only after the ownership design is
   approved.

### P2 — hardening

9. Fix the `cfg_attr` tripwire bypass.
10. Gate `HeapCore::dbg_large_cache_hits` with `bench-internals`.
11. Include Clippy in the generated CI matrix.
12. Repair R30-6's derived data/provenance and run one same-regime
    capacity-crossing matrix.

## Final conclusion

The new waves did **not** make the default allocator faster. They did make the
project safer, more explicit and better instrumented, especially through the
R30-1 diagnostic UAF fix and the evidence/CI rules.

The most important performance conclusion is not “the allocator is exhausted”.
It is:

> The default scalar hot path has little obvious micro-tuning headroom, but
> the project still has several multiplicative workload-specific levers, and
> R30 accidentally closed the strongest newly examined one using a benchmark
> below the production layer that already preserves its activation state.

The next round should therefore begin with a correct production-layer
`virgin-zero-skip` gate, then pursue pool-threshold and diverse-Large-cache
victims. More scalar instruction shaving is lower value than resolving those
algorithmic/configuration questions.

