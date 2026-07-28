# Round 24: readonly performance, correctness and project review

Date: 2026-07-28  
Reviewed range: `6e0dbad..1164718`  
Mode: source and Git history inspection only

## Executive verdict

Round 24 is a useful measurement and correction wave, but it did **not**
radically accelerate the normal production allocator.

- The ordinary production runtime path received no performance algorithm
  change in this range.
- The one merged runtime optimization, `STAGE_CAP: 512 -> 64`, applies only
  when the opt-in `batch-api` feature is enabled.
- That batch-only optimization is real and substantial for the measured
  inputs: approximately `-47.7% Ir` at `N=16` and `-24.2% Ir` at `N=64`.
- Two proposed hot-path optimizations were correctly rejected after they
  regressed instruction counts.
- The round materially improved understanding of the remaining gaps:
  ordinary interleaved churn is already strong, while cold/bulk free and
  segment teardown remain the credible victims.
- A new public safe diagnostic method can write through arbitrary raw
  pointers. This is a P0 soundness issue and should be removed or made
  explicitly unsafe and benchmark-only before further optimization work.

So the honest answer is:

> Yes, the experimental batch deallocation path became much faster for the
> measured small batches. No, Round 24 did not make the default production
> allocator materially faster.

The project is no longer at a universal “radical speedup” opportunity. The
remaining large gains are workload-specific and sit in three places:

1. overflowing a full small-object magazine during bulk/cold free;
2. segment retention versus release under teardown-heavy workloads;
3. opt-in paths such as batch API and NUMA, if they obtain real consumers.

## Scope and limitations

The review inspected:

- commit history and diffs in `6e0dbad..1164718`;
- affected allocator, diagnostic, feature, benchmark and test sources;
- the checked-in Round 24 measurement reports and raw result summaries;
- relevant earlier reports where they constrain the interpretation of the
  new results.

No build, test, benchmark, formatter, linter or project script was run.
Consequently, this review can validate source-level reasoning, internal
consistency and the shape of the checked-in evidence, but cannot independently
reproduce the reported measurements.

The pre-existing local modifications to `scripts/lib.mjs` and the untracked
`.claude/` directory were excluded and left untouched.

## What changed in Round 24

| Commit | Result | Production impact |
|---|---|---|
| `14a86ce` | Corrected the R23-3 `80.8 Ir` interpretation | Documentation/evidence only |
| `3bc9c91` | Split full-magazine free into cheap and overflow states | Measurement support; also introduced the unsafe-contract problem described below |
| `e530a9f` | Tried bitmap-clear prepass in `flush_magazine_class` | Regressed by about `37 Ir/event`; reverted |
| `9dc0e22` | Tried dynamic bulk bitmap clearing | Regressed by about `14 Ir/block`; reverted |
| `9a5b1f3` | Split cold alloc/free gap | Measurement support |
| `6d4eec6` | Added `bench-internals` gating to selected hooks | Project hygiene |
| `7378160` | Documented warm batch range | Documentation |
| `839b4af` | Reduced batch deallocation staging from 512 to 64 pointers | Large improvement, but only under `batch-api` |
| `ce17311` | Made current state more prominent in performance docs | Documentation quality |
| `9594570` | Isolated teardown/pool-cap root cause | Measurement and design evidence |
| `1164718` | Round summary and checkpoints | Documentation |

The change volume is dominated by reports, raw measurements and benchmark
apparatus. The runtime source delta is intentionally small.

## Did we really accelerate the code?

### Default production allocator: no material runtime acceleration

The Round 24 changelog says that the “plain production composition changed
exactly once” in R24-8. That wording is misleading.

The changed `STAGE_CAP` lives in `dealloc_batch_small`, which is compiled only
under the `batch-api` feature. The normal `production` feature bundle does not
enable `batch-api`. Therefore:

- the default scalar allocation/deallocation algorithm is unchanged;
- the ordinary production hot path does not execute the smaller staging
  array;
- Round 24 contains zero retained performance algorithm changes for plain
  production.

The changelog should say that the runtime changed once in the **opt-in batch
configuration**, and zero times in the plain production configuration.

### Opt-in batch API: yes, a genuine acceleration

The previous implementation created a 512-element stack staging array for
small-object batch deallocation. That means 4096 bytes of pointer-array
initialization even when the input batch is small.

R24-8 reduces the array to 64 entries. The checked-in evidence reports an
input-independent reduction of 4065 instructions:

| Batch | Before | After | Improvement |
|---|---:|---:|---:|
| `N=16` | 8514 Ir | 4449 Ir | 47.7% |
| `N=64` | 16757 Ir | 12692 Ir | 24.2% |

The constant delta and the matching zero-initialization diagnosis make this
mechanistically credible. It removes real work rather than moving it between
accounting buckets.

However, the evidence currently covers only batches fitting the new stage.
For `N > 80` the implementation starts performing repeated 64-entry flushes.
The correctness test includes `N=200`, but the performance boundary was not
measured. Before calling 64 the final cap, measure at least:

`N = 16, 64, 80, 81, 128, 200, 512, 1024`.

This is especially important because the optimization exchanges fixed stack
initialization for potentially more control-flow and flush iterations on
large inputs.

### Rejected experiments were still valuable

R24-3 and R24-4 are good examples of disciplined optimization:

- moving bitmap clearing into a standalone prepass looked cheap in isolation,
  but regressed the real flush context by about `37 Ir/event`;
- a generic dynamic `clear_many` primitive regressed allocation by about
  `13.9–14 Ir/block`.

Both were reverted. This does not accelerate shipped code, but it prevents
plausible-looking slowdowns from being merged. The lesson is specific:

> Do not extract or generalize the current bitmap operation unless the
> resulting code is measured inside its final caller and compiler context.

## Correctness and soundness review

### P0: safe public diagnostic method can write through arbitrary pointers

`HeapCore::dbg_overflow_bitmap_clear_pass` is a safe public function accepting
`&[*mut u8]`. For every supplied pointer it derives a segment base and offset,
then clears allocator metadata through `SegmentMeta`.

The method does not establish that a pointer is:

- non-null;
- mapped;
- owned by this heap;
- a live block belonging to a Sefer segment;
- associated with metadata that remains valid for the duration of the call.

The lower bitmap/node helpers ultimately perform raw memory reads and writes
under caller-side validity assumptions. A safe Rust caller can therefore pass
a null, foreign, dangling or unmapped pointer and trigger an invalid metadata
access. Encapsulating the final raw write in an internal `unsafe` block does
not make the public safe contract sound.

The hook is also compiled with `alloc-global + fastbin`, not
`bench-internals`, despite existing only to support a benchmark experiment.
It therefore expands the safe public surface of a normal production feature
combination.

Recommended resolution, in order:

1. Remove the hook. Its target optimization was NO-GO, so retaining the
   method has little continuing value.
2. If it must remain, gate it behind `bench-internals`.
3. Make it `pub unsafe fn` and document the exact contract: every pointer
   must reference a currently mapped, live, owned small block; segment
   metadata must be valid; and the owner-only mutation/exclusivity rules must
   be upheld.
4. Prefer `&mut self` if that accurately represents the owner-only mutation
   discipline.

This should be fixed before starting the next optimization wave.

### R24-8 test overstates what it proves

`r24_8_dealloc_batch_multi_flush` checks that 200 subsequent allocations are
non-null and distinct after freeing an earlier group of 200 through the global
allocator.

That does not prove that the original 200 blocks were actually reclaimed:

- the global allocator remains active for `Vec`, `HashSet` and test-harness
  allocations;
- the subsequent 200 allocations may be carved from new blocks or new
  segments even if the original blocks leaked;
- distinct non-null addresses prove allocation success, not reclamation of
  the prior offsets.

The mutation experiment that removes an intermediate flush proves that the
fixed-size staging logic cannot simply omit a flush without overflowing its
array. It does not independently prove that every flush performs semantically
correct reclamation.

There is also a comment-count mismatch. For `N=200` and a 64-entry stage,
after the first 16 retained magazine entries there are two full intermediate
64-entry flushes and one final 56-entry flush: three flushes total, not “three
intermediate plus one final”.

Improve this test by using an isolated `HeapCore`/`AllocCore` and an exact
oracle, for example:

- assert the original offsets reappear from the expected free structure;
- inspect an authoritative free/live-count or bitmap state;
- assert the expected segment ownership/liveness transition;
- avoid unrelated allocations through the allocator under test while the
  oracle is being evaluated.

Until then, rename the test or narrow its comments so that they claim only the
multi-flush control-flow coverage it actually establishes.

## What Round 24 taught us about the remaining gaps

### Cold/bulk free is the clearest CPU victim

The split measurement reports approximately:

- cold allocation: Sefer `91.05/83.5` versus mimalloc `71.81`;
- cold free: Sefer `108.77` versus mimalloc `30.24`;
- the overflow operation accounts for about 61.5% of Sefer's measured free
  half.

The precise refill numbers are derived rather than measured as a fully
isolated primitive, so they should not be treated as direct timings.
Nevertheless, the direction is persuasive: the largest remaining cold gap is
on the free side, not the scalar hot allocation side.

### Hot interleaved churn does not exercise the overflow victim

The interleaved hot-free measurement does not overflow the magazine and lands
near the independently measured cheap-free cost. Therefore a large
optimization of overflow handling can improve cold/bulk free while leaving the
canonical interleaved churn table almost unchanged.

This distinction should be explicit in future headlines. Otherwise a real
bulk-free improvement may look like “no production gain” merely because the
headline benchmark never visits the optimized state.

### The 1024-operation teardown cliff is a retention policy problem

The checked-in teardown experiment reports:

- near parity for smaller cases;
- a `2.69x` gap at 1024 operations;
- 248 decommit/release events at 1024 and zero at the smaller tested cases;
- about `22.09 µs` without teardown versus `123.94 µs` with teardown for
  Sefer at 1024.

This strongly implicates the four-segment pool cap rather than steady-state
allocation logic. It is a real stress-path weakness, but it is not evidence
that the ordinary hot path became slow.

Previous pool sweeps already show the tradeoff: larger caps reduce release
pressure, but consume tens or hundreds of MiB per heap. A blind global default
increase would trade a benchmark win for potentially severe multi-thread RSS.

## Recommended Round 25

### P0 — repair the diagnostic API soundness hole

Remove or properly gate and mark
`dbg_overflow_bitmap_clear_pass` as described above. Add a policy check that
benchmark-only raw-pointer hooks cannot silently enter safe production-facing
APIs.

This is correctness work, not performance work, but it is the required first
step.

### P1 — independently sweep full-magazine flush size

The strongest small, untried CPU experiment is to keep:

- `TCACHE_CAP = 16`;
- every other cache policy unchanged;

and sweep only the number of entries flushed on overflow:

`FLUSH_N = 4, 8, 12, 16`.

The current half-flush policy flushes 8 entries. For a sequential free of 64
blocks:

- half flush: 6 overflow events × 8 blocks = 48 blocks flushed;
- full flush: 3 overflow events × 16 blocks = 48 blocks flushed.

For 256 blocks:

- half flush: 30 events × 8 = 240 blocks;
- full flush: 15 events × 16 = 240 blocks.

For these multiples of 16, full flush performs the same total per-block
free-list work and leaves the same final 16 cached blocks, but it:

- halves the number of per-event metadata/setup sequences;
- removes the compact-and-shift of the retained half;
- makes magazine emptying a simpler state transition.

This experiment is different from the old `TCACHE_CAP` sweep, which changed
capacity and flush behavior together. It can be tested without increasing
per-heap cache memory.

The risk is burst reuse. After the 17th free, half-flush retains nine prior
blocks plus the current block, whereas full-flush retains only the current
block. The gate must therefore cover both sides:

1. bulk free at `N=17, 32, 64, 256, 1024`;
2. free-then-immediate-reallocate bursts;
3. oscillating live-set sizes around 8–24 blocks;
4. interleaved hot churn;
5. refill count, tcache hit rate and wall-clock, not only Ir.

Kill the experiment if reduced overflow setup is paid back by refill thrash.
If it wins both bulk and burst gates, it is the best near-term candidate for a
real default-production improvement.

### P2 — solve teardown with an RSS-bounded adaptive budget

Run the already motivated cap sweep `4/8/16/32` on the exact teardown workload,
but record peak committed bytes and a many-thread case alongside latency.

Do not promote a larger fixed per-heap default based on the single-thread
1024-operation result. Prefer an adaptive design:

- retain more committed segments only after demonstrated rapid reuse;
- acquire capacity from a process-wide budget on the cold segment-empty path;
- return budget when a heap becomes idle or decay expires;
- keep hot allocation/free paths free of new shared atomics.

A process-wide token budget would let a genuinely hot heap exceed four
segments without multiplying the worst-case committed allowance by every
thread. The token operation belongs on rare pool growth/shrink transitions,
not on each allocation.

The acceptance gate should require a meaningful reduction of the 1024
teardown gap while bounding:

- single-thread peak commit;
- 8/32-thread aggregate peak commit;
- idle decay behavior;
- segment reserve/release/decommit counts;
- steady-state hot-path instructions.

### P3 — consider run-encoded free batches only if P1 is insufficient

The current intrusive free list writes linkage into each freed block. In a
contiguous bulk run, this produces per-block payload writes and per-block
metadata work even though the offsets are arithmetically predictable.

An architectural alternative is a small run descriptor:

- record `(segment, first_offset, count, stride)` for a homogeneous contiguous
  batch;
- allocate from the run arithmetically;
- materialize ordinary free-list nodes only when a run must be split or
  escaped to a general structure.

This could remove many cold payload writes and reduce bitmap operations, so it
has greater upside than another instruction-level rewrite of `clear_magazine`.
It also has much higher correctness complexity:

- mixed segments/classes;
- partial consumption;
- decommit/liveness accounting;
- double-free protection;
- coexistence with ordinary `BinTable` nodes;
- metadata capacity and overflow.

Treat it as design-first work with model tests and an isolated victim judge.
Do not implement it unless full flush leaves a material, wall-clock-confirmed
free-side gap.

### P4 — finish the batch staging boundary before expanding the API

The batch API has no committed public consumer, so it should not outrank
default-production work. If continued:

1. measure beyond the 64-entry stage boundary;
2. replace the global-allocator correctness test with an exact isolated
   oracle;
3. consider lazy construction of the 64-entry stage so batches that never
   overflow the magazine do not initialize even 512 bytes;
4. consider `MaybeUninit` only after a safe lazy form is measured and shown
   insufficient—the unsafe surface is not justified by speculation.

Do not publish the batch API solely because its microbenchmark is now faster.
First identify a consumer whose end-to-end workload benefits.

### Conditional — NUMA directory

Earlier evidence found an approximately `140x` high-segment-count directory
cliff under `numa-aware`, because that feature still falls back to an `O(S)`
scan. A node-indexed directory remains one of the few asymptotically large
opportunities.

Its priority is conditional on a real NUMA consumer and a reproducible
multi-node workload. Without that, it is a high-complexity opt-in optimization
with no impact on default production.

## What should not be optimized next

Avoid another round on these paths without new evidence:

- bitmap-clear prepasses;
- generic dynamic bulk bitmap masks;
- isolated helper costs that disappear or reverse after inlining;
- ordinary interleaved scalar churn;
- increasing `TCACHE_CAP` and flush size together;
- publishing batch APIs without an end-to-end consumer.

R24 demonstrated why: the compiler context dominates the first two, the hot
scalar path is already strong, and coupled parameter sweeps obscure the real
cause.

## Code and project improvements

### 1. Make feature claims exact

Performance reports must distinguish:

- default `production`;
- `production + batch-api`;
- `production + numa-aware`;
- benchmark-internal configurations.

Replace the Round 24 “plain production changed once” wording with the accurate
statement that plain production changed zero times and the opt-in batch
configuration changed once.

### 2. Define a strict benchmark-hook policy

Benchmark-only hooks should be:

- gated behind `bench-internals`;
- non-public outside the crate where possible;
- `unsafe` whenever validity depends on raw-pointer ownership or lifetime;
- documented with explicit preconditions;
- deleted when the experiment that needed them is rejected.

Add source review/checklist enforcement for safe functions that accept raw
pointers and touch allocator metadata.

### 3. Separate semantic tests from control-flow tests

Tests should state the exact invariant they prove. For batch deallocation,
separate:

- “multiple staging flushes execute without bounds failure”;
- “all blocks become semantically free”;
- “the expected blocks are recycled”;
- “segment live counts/decommit state remain correct”.

One global-allocator test cannot reliably prove all four.

### 4. Add workload-state labels to every performance headline

At minimum label:

- virgin versus recycled;
- cache hit versus miss;
- cheap free versus overflow free;
- with versus without teardown;
- scalar versus batch;
- default versus opt-in feature composition.

R24's state split is a major methodological improvement. Make it a required
schema rather than a one-off report convention.

### 5. Control performance-document growth

The reviewed range adds roughly ten thousand lines, overwhelmingly in
documentation, reports and raw logs, for one retained opt-in runtime
optimization. The new current-state-first layout is a good correction, but
the repository still needs a sustainable evidence policy:

- keep a short authoritative current-state table;
- store machine-readable raw measurements in compact files;
- generate repetitive tables where practical;
- archive superseded narratives rather than appending corrections across
  several active documents;
- link every headline to its exact feature set, judge and raw record.

The goal is not fewer measurements. It is one discoverable source of truth
instead of requiring readers to reconstruct the current answer from several
rounds of amendments.

### 6. Use two-dimensional acceptance gates

Every cache/pool optimization should have:

- a CPU/latency judge for the target workload;
- a memory/retention judge for the counter-workload.

For example, a pool-cap change is not GO from latency alone, and a full-flush
change is not GO from bulk free alone. This prevents local benchmark wins from
silently moving cost into RSS or refill frequency.

## Proposed task queue

| Priority | Task | Expected value | Stop condition |
|---|---|---|---|
| P0 | Remove or make unsafe and benchmark-only the overflow bitmap hook | Restores sound API boundary | No safe raw-pointer metadata write remains |
| P0 | Correct Round 24 production wording | Prevents false acceleration claim | Feature composition is explicit |
| P1 | Fixed-cap `FLUSH_N=4/8/12/16` sweep | Best small default-production CPU candidate | Reject on burst/refill regression |
| P1 | Exact isolated batch multi-flush correctness oracle | Makes R24-8 evidence trustworthy | Old omission mutation fails semantically |
| P2 | Pool cap `4/8/16/32` plus RSS/thread sweep | May close teardown outlier | Reject if aggregate commit is unbounded |
| P2 | Design global/adaptive pool budget if sweep wins | Keeps latency gain without per-thread RSS explosion | No hot-path shared atomic |
| P3 | Batch stage boundary `80/81/128/200/512/1024` | Validates retained batch optimization | Reject 64 cap if large batches regress |
| P3 | Run-encoded free design study | Architectural bulk-free upside | Only proceed with wall-clock victim |
| Conditional | NUMA node directory | Removes known `O(S)` cliff | Requires a real NUMA consumer |

## Final assessment

Round 24 is successful as an **evidence-quality wave**:

- it corrected an overstated interpretation;
- isolated the cold-free and teardown victims;
- rejected two compiler-sensitive slowdowns;
- and removed a large fixed initialization cost from the opt-in batch path.

It is not a radical acceleration wave for default production. The most honest
next step is neither another generic bitmap micro-optimization nor a larger
cache by fiat. It is:

1. repair the new soundness hole;
2. independently test full-magazine flush at fixed capacity;
3. address teardown through an RSS-bounded adaptive retention policy;
4. pursue run encoding or NUMA indexing only when a real workload justifies
   their architectural cost.

The likely ceiling for ordinary hot scalar churn is now low single-digit
percent. Large remaining wins are possible, but only in explicitly named
states: cold/bulk free, teardown-heavy segment churn, large batch operations,
or high-segment-count NUMA lookup.
