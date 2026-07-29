# Read-only review: Round 27–28

Date: 2026-07-29

Reviewed range: `8940b17..b7ff9fe`

Review mode: read-only. I inspected Git history, committed diffs, source files,
tests, benchmark sources, raw committed measurements and reports. I did **not**
run builds, tests, benchmarks, formatters, linters, Node scripts or project
executables. The only filesystem change made by this review is this report.

## Executive verdict

The short answer is:

- **No, Round 27–28 did not make the default shipping allocator faster.**
  The default production algorithms and the `(4 segments, 16 MiB)` small-pool
  policy remain unchanged.
- **Yes, the project learned how to make one important workload materially
  faster by configuration:** the explicitly configured `(8, 32 MiB)` small
  pool is about **22% faster** than `(4, 16 MiB)` in the measured 1024-byte
  churn-with-teardown workload and eliminates its observed decommit cliff
  (`9 -> 0` decommits per process run).
- That win is **not free and not universal**. The same configuration retains
  about **8 MiB more per materialised heap** after the pressure workload,
  scales to roughly **255 MiB more at 32 heaps**, and does not disappear during
  pure idle in the measured implementation.
- Round 27 is therefore best described as a successful **measurement,
  correction and product-policy wave**, not a shipping speed wave.
- Round 28 adds a useful cost attribution (`flush_class(8)` = 449 callgrind
  instructions) and a stronger correctness test, but again changes no normal
  production path.

The project is now near a local optimum for ordinary scalar small-allocation
churn on the currently measured machine and workload family. There are still
ways to obtain large wins, but they are mostly:

1. workload-specific configuration or API adoption;
2. removal of OS lifecycle work rather than scalar instruction shaving;
3. Linux-only elimination of large promotion copies;
4. architectural changes with explicit consumers and correctness gates.

I would not start a sixth micro-optimization attempt in the current
magazine-overflow implementation.

## What changed in shipping code

The production-source diff in the reviewed range is unusually small relative
to the 10,000+ lines of reports, probes and evidence:

- `src/registry/heap_core_dealloc_batch.rs`: 250 lines removed. This deletes
  the rejected, `bench-internals`-gated R26 lazy-stage experiment.
- `src/registry/heap_core_diag.rs`: diagnostic hooks were gated or removed,
  and one new `bench-internals`-only `dbg_flush_class_only` measurement hook
  was added.
- No normal `alloc`, `dealloc`, `realloc`, refill, remote-reclaim, pool,
  segment-directory, large-cache or VM implementation body was optimized.
- `DEFAULT_POOL_SEGMENTS` remains 4 and `DEFAULT_POOL_BYTE_CAP` remains
  16 MiB.

Therefore an application rebuilding the same revision with the same production
feature set and default `SeferAlloc::new()` should not expect a runtime
speedup from this range.

There are small non-runtime wins:

- less experimental code and fewer unsafe diagnostic surfaces;
- smaller `batch-api + bench-internals` maintenance/compile surface;
- a more reliable JavaScript command runner (`shell: false`, executable
  rejection of `shell: true`, and a wired argv regression test);
- better correctness coverage around promotion/free accounting.

Those are valuable, but they are not allocator throughput improvements.

## Review findings

### P0 — the advertised 22% win is not yet available from the user-facing documentation

R27-5 recommends keeping the conservative default while prominently documenting
an `(8, 32 MiB)` throughput recipe. That is the right policy conclusion, but
the recommended product work has not landed.

The README configuration section documents only the large-cache knobs. It does
not show:

```rust
LargeCacheConfig::new().pool(
    SmallSegmentPoolConfig::new()
        .pool_segments(8)
        .pool_byte_cap(32 * 1024 * 1024),
)
```

It also says that `SeferAlloc::new()` uses defaults “tuned for
throughput-first workloads”, while the newly documented small-pool decision
explicitly keeps the RSS-conservative `(4, 16 MiB)` default instead of the
measured faster `(8, 32 MiB)` profile. Those statements can coexist only if
the README distinguishes the large-cache policy from the small-pool policy;
today it does not.

This is the cheapest way to turn Round 27 into a real user-visible speedup:

1. add a “small-segment pool” row to the configuration table;
2. add the measured `(8, 32 MiB)` throughput recipe;
3. state the measured scope honestly: 1024-byte churn with teardown, Windows,
   one host, about 22% lower elapsed time, about +8 MiB retained per active
   heap under the pressure victim;
4. mention that changing only `pool_segments` to 8 while leaving 16 MiB is a
   no-op because the effective cap is `min(segments, bytes / 4 MiB)`;
5. optionally add a named preset only after deciding that the extra public API
   is worth maintaining.

### P0 — the R28-2 anomalous failure is not explained

`docs/CORRECTNESS_OPEN_ITEMS.md` reports one anomalous failure out of roughly
155 repeated executions of the strengthened promotion/free test and attributes
it to concurrent multi-agent build contention.

That attribution is a hypothesis, not a root cause. Waiting for a shared Cargo
build lock can explain delay; by itself it does not normally explain a
completed allocator correctness assertion failing. The report does not retain
the exact failing assertion, seed/state, stdout/stderr or a minimal
reproduction.

For an allocator, one unexplained failure in a promotion/free/live-count test
must remain open until classified as one of:

- harness/test isolation bug;
- shared external resource collision;
- timing-sensitive implementation bug;
- stale/mismatched binary;
- genuinely non-reproducible infrastructure failure with direct evidence.

The new assertion is conceptually stronger and the two mutation
counterfactuals are good work. The wave should not, however, describe this
part as fully closed while the anomalous failure is only informally explained.

Recommended next action: preserve the exact failure output, run the already
built test binary from a private target directory or serialized environment,
and make the test print the before/after registration and live-count state on
every failure.

### P1 — R27-4 proves a workload-specific configuration win, not a general 22% allocator speedup

The R27-4 A/B is materially better than the earlier R26 measurement:

- it uses the real paired byte caps `(4,16 MiB)` and `(8,32 MiB)`;
- it enters through the real global allocator;
- warm-up is outside the timer;
- it uses A/B/B/A process alternation;
- it has a same-vs-same control;
- its `19/20` direction and `t=8.114` signal are strong;
- cap 4 consistently reserves 16 segments and decommits 9 times, while cap 8
  reserves 8 and decommits zero times.

The conclusion is valid for that victim. It should not be generalized beyond
the measured shape:

- allocation size: 1024 bytes;
- pressure batch: 120;
- fixed prefill/churn/teardown pattern;
- native Windows;
- one CPU/host and power plan;
- elapsed process workload, not a broad application suite.

The result demonstrates that avoiding segment decommit/reserve churn can be a
large win. It does not demonstrate that cap 8 accelerates ordinary steady-state
churn, small allocations of every class, mixed-size workloads, Linux, or
memory-constrained production services.

Before considering a default change, add at least:

- sizes around the relevant segment-demand boundary;
- a mixed-size workload;
- long-lived heaps without teardown;
- Linux native;
- a high-thread-count latency/RSS joint gate;
- one application-shaped trace or macro benchmark.

Until then, the correct product is an opt-in recipe, not a new universal
default.

### P1 — about half of the post-drain retention delta is real but not accounted to a mechanism

R27-3 is a strong correction of R26:

- cap 4 is proven to saturate by non-zero decommit counts;
- cap 8 is proven to use capacity beyond four by `pooled_hw_max = 6`;
- subprocess isolation avoids first-claim slot reuse;
- retention scales consistently per heap;
- explicit drain proves the pooled portion is reclaimable;
- pure idle is shown not to reclaim it.

The report also correctly admits that approximately 4 MiB per heap remains
after draining the pool and is “committed-non-pooled”, but is not reconciled to
a single tracked counter.

The **total RSS delta** is credible. The split “about 4 MiB pooled + about
4 MiB committed-non-pooled” should remain a measured phenomenological split,
not be treated as a fully proven ownership/accounting model. Before designing
an adaptive budget or scavenger around this residual, instrument the states
that can own it:

- current active small segment;
- registered empty but non-pooled segment;
- pooled segment;
- decommitted reservation;
- primordial segment;
- large-cache contribution;
- released/unregistered segment.

The accounting should satisfy a per-heap identity at each snapshot, not infer
the second half from RSS subtraction.

### P1 — R28-1 measures `flush_class`; it does not prove that the whole region is at a mathematical minimum

The strongest R28-1 fact is credible and useful:

```text
flush_class(8 blocks) = 4,338 - 3,889 = 449 Ir
                      = 56.1 Ir/block
```

The paired arm calls the real production `flush_class`, and the input blocks
are live, same-class allocations with a bitmap state equivalent to
post-magazine-clear. This is a valid standalone cost measurement.

Three qualifications matter:

1. The arm does not reproduce the surrounding real overflow state. The eight
   blocks were never magazine-resident, and the hook bypasses the tcache
   transition. That is intentional and appropriate for isolating
   `flush_class`, but it is not an end-to-end replacement experiment.
2. The derived `~48 Ir` “compaction + final push” residual subtracts the
   historical 84-Ir bitmap-clear number from a newer 581-Ir overflow total.
   An unchanged source loop does not guarantee unchanged generated cost.
   Treat 48 and the associated percentages as estimates, not current-revision
   direct measurements.
3. “Mostly necessary work” is a good reason not to launch another speculative
   rewrite, but instruction count alone does not prove no structurally
   different representation could remove the work.

The practical verdict still agrees with the report: five attempts have failed
or exhausted this immediate region, so another local rewrite is a poor use of
time. The wording should be “no justified next micro-optimization” rather than
“proven minimum”.

### P2 — `flush_class` still has a bounded quadratic-in-run-count defensive scan, but it is not the current radical target

`AllocCore::flush_class` groups consecutive same-segment blocks and checks each
run against:

```rust
recycled_bases[..recycled_n].contains(&base)
```

This is O(number of runs × recycled runs), bounded by a fixed capacity of 16.
The R28 benchmark uses one same-segment run, so it does not exercise this cost.

This is worth a dedicated mixed-segment diagnostic only if real batch or
magazine inputs are shown to alternate among many segment bases. It is not a
reason to optimize now:

- the bound is small;
- the common measured overflow has one run;
- replacing it with hashing/bitmap state could cost more;
- the scan exists to contain use-after-recycle/double-free hazards.

Do not remove or weaken this guard for speed.

### P2 — the measurement artifact volume is growing faster than the product

This range adds more than 10,000 lines while changing normal runtime behavior
by essentially zero. Much of that is valuable raw evidence, but the current
layout has maintenance costs:

- `docs/perf/OPEN_ITEMS.md` exceeds 1,000 lines and mixes current decisions
  with long append-only historical narratives;
- full raw logs and paired-run JSON duplicate information already summarized
  in CSV/report tables;
- measurement examples are mostly near-copies of prior examples;
- every diagnostic hook adds unsafe inventory/documentation/test burden even
  when feature-gated.

Recommended project cleanup:

- keep a short current-state index and move closure narratives to an archive;
- make a small machine-readable benchmark manifest the source of arm/config
  metadata;
- share workload code between cap4/cap8 binaries, leaving only static config
  in each executable;
- retain raw logs when needed for audit, but compress or attach them as CI
  artifacts rather than growing the normal source review surface indefinitely;
- require every new diagnostic hook to have an owner task and planned removal
  condition;
- continue deleting rejected experimental implementations, as R27-6 did.

### P2 — benchmark provenance can be made stronger

R27-3 and R27-4 correctly say they measured a base commit plus an uncommitted
working tree, and preserve raw evidence. For long-term reproducibility, “base
commit + dirty tree” is weaker than an immutable source identity.

Future reports should record one of:

- a temporary measurement commit SHA;
- a Git tree object SHA;
- a hash of the exact patch applied over the base;
- the built executable hash plus complete feature/config metadata.

This prevents a later reader from having to infer which uncommitted source
produced a committed result.

## Per-wave assessment

### Round 27

R27 is a good review-response wave.

What it did well:

- corrected the ineffective one-knob cap proposal;
- retracted the false “RSS-neutral” interpretation;
- built a victim-activation-proven retention gate;
- re-ran latency at the real paired caps and real entry point;
- made the throughput/RSS trade explicit;
- rejected an adaptive subsystem whose benefit is not demonstrated;
- removed the rejected lazy-stage implementation;
- gated and removed unsafe diagnostic hooks;
- corrected the R26 timer description;
- fixed the JavaScript argv/shell regression at the shared runner boundary.

What it did not do:

- change default allocator performance;
- expose the measured throughput configuration prominently to users;
- fully account for the non-pooled residual retention;
- demonstrate the cap8 benefit across representative workload classes.

Verdict: **high-quality correction and decision-support work; no default
runtime acceleration.**

### Round 28

R28-1 closes an attribution question rather than optimizing the function.
The 449-Ir result supports stopping local `flush_class` micro-tuning.

R28-2 improves the test from a weak “no double release” inequality toward a
per-segment leak proof and includes meaningful mutation counterfactuals.
However, the unexplained anomalous failure must stay open.

Verdict: **better observability and correctness coverage; no shipping
acceleration.**

## What can still be accelerated strongly

### 1. Immediate, proven, workload-specific: expose the `(8, 32 MiB)` pool profile

Expected upside: about **1.28× throughput / 22% lower elapsed time** in the
measured teardown victim, with `9 -> 0` decommits.

Cost: about **+8 MiB retained per materialised heap** in the measured pressure
shape, potentially hundreds of MiB across many heaps.

This is ready for documentation and user adoption now. It is not ready as a
universal default.

### 2. Best next allocator-internal investigation: reservation-only overflow tier

R27-11 is the most promising unmeasured continuation of the cap result.

The idea is to keep the current four committed hot pooled segments, but for
additional empty segments:

- decommit payload pages;
- retain virtual-address reservation and enough segment identity/metadata;
- recommit on reuse;
- release later under pressure/decay.

This could preserve part of the 22% win without retaining another fully
committed segment. It cannot avoid recommit/page-fault cost, so it is worthwhile
only if reserve/release + table setup + metadata initialization are a material
part of the current cycle.

Do **not** implement it first. Build the Stage-1 decomposition already named in
`OPEN_ITEMS.md`:

```text
full decommit -> reserve/rebuild cycle
  = OS release/reserve
  + SegmentTable unregister/register
  + metadata initialization
  + recommit
  + first-touch/page faults
```

If the avoidable first three components are material, this becomes a genuine
GO candidate. If recommit/first-touch dominates, close it.

This is the best chance of turning Round 27's configuration-only win into a
shipping algorithmic win with a better RSS trade.

### 3. Highest asymptotic upside: Linux sub-region `mremap` for medium-to-Large promotion

The current promotion path allocates a Large destination and performs
`copy_nonoverlapping(old_size)`. Its work is O(bytes copied).

The corrected R22-16 design leaves a Linux-only conditional path open:

- page-aligned medium allocation;
- exclusive carved byte range;
- `mremap` of the subrange;
- register the destination as a Large/extent allocation;
- ensure the vacated source offset can never be returned through the ordinary
  `BinTable` free list;
- fall back to the existing copy path on any error.

For large promoted buffers this can change the dominant work from copying
hundreds of KiB/MiB to VM metadata operations. That is one of the few remaining
directions with truly radical per-operation upside.

Risks are correspondingly high:

- Linux only;
- new VM FFI;
- exact page-alignment and range ownership requirements;
- source-hole bookkeeping;
- failure atomicity;
- allocator table/segment identity transitions;
- extensive Miri/Kani/loom/native testing needs.

Stage 1 should first measure how often real workloads hit the promotion-copy
path and the copied-byte distribution. No victim, no implementation.

### 4. Consumer-driven batch API

The project already measured a real production-shaped batch mechanism at about
1.1–1.6× over scalar loops for relevant batch sizes. This is a real strong win
only when a downstream consumer can submit batches.

It does nothing for ordinary `Box`, `Vec` and standard `GlobalAlloc` calls.
The correct next step is not more internal batch tuning; it is one real
consumer:

- arena/slab integration;
- object-pool refill/drain;
- packet/message batch allocation;
- an internal crate component that naturally owns contiguous batches.

Keep the API experimental until that consumer proves the contract and shape.

### 5. Deployment profile, not default: medium classes

Earlier rounds measured extremely large alloc/free improvements for selected
medium sizes, but also a catastrophic realloc regression because dense
small-style packing turns an in-place Large grow into a move/copy.

This remains useful only as an explicit profile for workloads that:

- allocate/free those sizes frequently;
- rarely grow them with `realloc`;
- value segment density/alloc latency over grow latency.

Do not promote it wholesale. Consider exposing workload presets only after a
real application trace confirms the classification.

### 6. Conditional cross-thread reclaim batching

R17-10 sub-design A reuses the existing batched live-count/decommit primitive
and is small, but its expected gain is modest. Sub-design B can matter only if
one `drain_dirty_segments` sweep commonly empties multiple segments.

Measure the distribution first. This is a reasonable cleanup/perf task, not
the leading radical candidate.

## What should improve in code

1. **Keep correctness guards in `flush_run`.** The payload bound, bump bound,
   bitmap free test, recycled-base containment and live-count transition are
   not decorative overhead.
2. **Avoid copied experimental implementations.** R27-6 correctly deleted the
   250-line lazy clone. Future A/Bs should prefer a shared implementation with
   a narrowly injected policy/representation where that does not perturb the
   measurement.
3. **Split diagnostic modules by concern.** `heap_core_diag.rs` is becoming an
   aggregation point for unrelated unsafe hooks. Separate pool, routing,
   bitmap and benchmark-only diagnostics behind their exact feature gates.
4. **Make test-only visibility stricter than `#[doc(hidden)]`.** Continue the
   `bench-internals` policy and audit older public hidden hooks for a real
   production caller.
5. **Add explicit state accounting for segment retention.** A debug snapshot
   should reconcile every registered segment into one state and total its
   committed/reserved bytes. This will make future RSS policy work much less
   inferential.
6. **Keep experimental batch APIs out of `production`.** Their current feature
   boundary is appropriate until a consumer exists.
7. **Investigate the R28-2 failure before adding more mutation-heavy tests.**
   Test infrastructure confidence is part of memory-safety confidence.

## What should improve in the project

1. Publish the small-pool throughput/RSS recipe in README and integration docs.
2. Add a benchmark decision matrix:

   | claim | required evidence |
   |---|---|
   | local instruction win | paired iai arms |
   | shipping speedup | real entry point + wall clock |
   | default change | multi-size/mixed workload + RSS + at least two platforms |
   | memory-policy change | victim activation + post-idle + explicit state accounting |
   | correctness closure | non-vacuous counterfactual + zero unexplained failures |

3. Separate “current open decisions” from append-only history.
4. Record immutable source identity for every measurement.
5. Add at least one application-shaped trace/replay benchmark. The project has
   become very good at proving narrow synthetic facts; the next risk is
   optimizing only its own judges.
6. Treat configuration and public documentation as part of performance
   delivery. A measured opt-in that users cannot discover is not yet a product
   speedup.
7. Establish a failure-artifact policy for stress/flake checks: exact binary
   hash, feature set, seed, output, host load and retry result.

## Recommended Round 29 plan

### P0 — correctness and product closure

1. Resolve or explicitly reopen the R28-2 anomalous failure.
2. Document the `(8, 32 MiB)` small-pool throughput recipe with its RSS trade.
3. Correct the README’s blanket “throughput-first defaults” wording so it
   distinguishes large-cache and small-pool policy.

### P1 — one high-value measurement before any new design

4. Decompose the cap4 decommit-to-new-segment lifecycle and decide whether the
   reservation-only overflow tier has a material avoidable component.
5. Add exact per-heap segment-state/commit accounting to explain R27-3’s
   post-drain residual.

### P1 — one asymptotic opportunity gate

6. Measure medium-promotion frequency and copied-byte distribution on a
   Vec-growth/application-shaped workload.
7. Only if material, build a Linux `mremap` correctness prototype with the
   existing memcpy path as mandatory fallback.

### P2 — broaden confidence

8. Re-run the pool profile decision on Linux, mixed sizes and long-lived heaps.
9. Add one real batch-API consumer or stop investing in batch internals.

### P3 — maintenance

10. Archive resolved `OPEN_ITEMS` narratives, deduplicate probe code and
    establish immutable measurement provenance.

## Final answer

Round 27–28 made the project more honest, safer to maintain and better measured.
It did **not** accelerate the default shipping allocator.

The main proven speed lever is now a deliberate user choice:
`(pool_segments, pool_byte_cap) = (8, 32 MiB)` for teardown-heavy 1024-byte
pressure workloads, trading roughly 22% lower latency for roughly 8 MiB more
retention per materialised heap in the measured victim.

The strongest remaining internal opportunity is not another `flush_class`
micro-tweak. It is to determine whether a reservation-only overflow tier can
avoid a material part of segment lifecycle churn without retaining committed
payload. The largest asymptotic opportunity is Linux-only remap-instead-of-copy
for genuinely frequent medium-to-Large promotions.

Before either optimization, close the unexplained R28-2 failure and deliver the
already-proven pool profile through clear documentation.
