# Round 26: readonly performance, correctness and project review

Date: 2026-07-28  
Reviewed range: `9ddb062..8940b17`  
Mode: Git history and file inspection only

## Executive verdict

Round 26 did **not** accelerate the default shipping allocator.

- `DEFAULT_POOL_SEGMENTS` remains 4.
- The shipping `dealloc_batch_small` implementation remains unchanged.
- New `src/` code consists of diagnostic/test accessors and a retained
  `bench-internals`-only lazy-stage variant whose experiment was NO-GO.
- The shell-launcher repair improves developer tooling, not allocator runtime.

The round does contain an important positive performance result:

> An explicit pool configuration with an effective cap of 8 is about 16%
> faster than an effective cap of 4 on the specific 1024-byte
> batched-teardown stress workload.

R26-3 supports that result well:

- paired A/B/B/A, 20 pairs;
- cap 8 wins 20/20 pairs;
- `t=12.212` versus critical `2.101`;
- mean 147.58 ms versus 123.91 ms;
- 9 decommits per cap-4 process versus zero per cap-8 process;
- same-vs-same control correctly reports no difference.

However, the pending default-change recommendation is not currently
implementable as written:

- the effective cap is
  `min(pool_segments, pool_byte_cap / 4 MiB)`;
- defaults are 4 segments and 16 MiB;
- changing only `DEFAULT_POOL_SEGMENTS` from 4 to 8 while leaving the byte cap
  at 16 MiB still resolves to **4**;
- the R26 A/B arms explicitly use a generous 256 MiB byte cap, so they do not
  test the proposed one-constant default edit.

There is also a second measurement problem. R26-1 calls cap 8 “RSS-neutral”
based on peak RSS during a workload using `RSS_BATCH_SIZE=50`. That workload
does not demonstrate pressure beyond four retained segments and records
neither pool occupancy nor decommit events. R26-3, using the pressure-producing
batch size 120, shows cap 8 retaining approximately **4 MiB more** after the
workload. This is precisely the pool-retention trade-off the default decision
must measure.

The honest answer is:

> We confirmed that a pre-existing opt-in configuration can strongly improve
> one teardown-heavy workload. We did not speed up the default code, and the
> current evidence is insufficient to change the defaults safely.

## Scope and limitations

Inspected:

- all commits in `9ddb062..8940b17`;
- runtime and diagnostic source diffs;
- R26-1 subprocess RSS gate;
- R26-3 production-entry A/B/B/A gate and provenance;
- R26-5 per-block correctness oracle;
- R26-6 safety-contract correction;
- R26-7 lazy-stage experiment;
- R26-8 process-launcher repair;
- R26-9 adaptive-design closure;
- current pool configuration resolution and documented presets.

No build, test, benchmark, formatter, linter, Node script or project helper was
run. Reported measurements were reviewed from committed files and raw results
but not independently reproduced.

Pre-existing untracked work was not touched:

- `.claude/`;
- `docs/checkpoints/2026-07-28-round26-planned.md`;
- `docs/reviews/2026-07-28-r25-readonly-review.md`.

## What changed

| Commit | Result | Shipping performance impact |
|---|---|---|
| `5285e14` | Corrected R25-5 claims | Documentation |
| `779474e` | Rebuilt RSS gate with subprocess isolation | Diagnostic/measurement only |
| `5537a20` | Confirmed cap-8 latency through real global allocator | Measurement only |
| `418acd8` | Added configuration-identity evidence rule | Process documentation |
| `6c8c61c` | Added per-block batch-free oracle | Test/diagnostic only |
| `f1f04c2` | Corrected bitmap hook safety contract | Documentation |
| `8679105` | Tested lazy batch staging; NO-GO | Retained bench-only code, no shipping change |
| `e129107` | Replaced shell quoting with direct argv | Tooling |
| `8940b17` | Re-closed adaptive pool design | Documentation; closure is not justified by the relevant retention evidence |

No production allocation or deallocation algorithm changed in this range.

## Did we really accelerate code?

### Default production: no

The default effective pool remains:

```text
min(4 segments, 16 MiB / 4 MiB) = 4 segments
```

The normal scalar allocation/free paths are byte-identical to the prior
baseline. The retained lazy staging implementation compiles only with
`batch-api + bench-internals` and is slower once overflow begins.

Therefore Round 26 itself provides no default production speedup.

### Explicit effective cap 8: yes, for one workload

R26-3 is credible evidence that an allocator configured with:

```text
pool_segments = 8
pool_byte_cap = 256 MiB
```

is faster than:

```text
pool_segments = 4
pool_byte_cap = 256 MiB
```

on its selected workload.

The benefit is mechanistically clear:

- workload demand reaches roughly six segments;
- cap 4 releases and later reserves segments again;
- cap 8 keeps them committed and reusable;
- cap 4 records nine decommit events and sixteen cumulative reservations;
- cap 8 records zero decommits and eight reservations;
- eliminating those OS lifecycle operations saves about 16% wall-clock.

This is not a general allocator-throughput result. It applies to an
intentionally pressure-producing workload that:

- prefills 120 independent 256-block sets before processing them;
- uses 1024-byte allocations;
- performs full teardown;
- repeatedly crosses the four-segment retention boundary.

Hot scalar churn that remains within the pool does not receive this gain.

### Batch lazy staging: measured NO-GO

The safe `Option<[ptr; 64]>` experiment produced:

- a 53–54 Ir win for `N<=16`;
- a loss beginning at `N=17`;
- +151 Ir at `N=64`;
- +589 Ir at `N=200`;
- +3076 Ir at `N=1024`.

The experiment correctly rejects the hypothesis. The remaining eager
64-entry initialization costs only about 54 Ir in-context, not the much
larger extrapolated estimate.

No further staging representation work is justified without a real consumer
whose batches are almost always at most 16.

## Critical findings

### P0: changing only `DEFAULT_POOL_SEGMENTS` from 4 to 8 is a no-op

`AllocCore::new_with_config` resolves the effective pool cap as:

```text
by_segments.min(by_bytes)
```

where:

```text
by_segments = resolved_pool_segments()
by_bytes = resolved_pool_byte_cap() / SEGMENT
```

Current defaults:

```text
DEFAULT_POOL_SEGMENTS = 4
DEFAULT_POOL_BYTE_CAP = 16 MiB
SEGMENT = 4 MiB
```

The pending task repeatedly says:

> promote `DEFAULT_POOL_SEGMENTS` 4→8

If implemented literally:

```text
min(8, 16 MiB / 4 MiB) = min(8, 4) = 4
```

The allocator would behave exactly as before. The proposed change would not
produce the R26-3 result.

To obtain an effective default cap of 8, the decision must explicitly change
both dimensions, for example:

```text
DEFAULT_POOL_SEGMENTS = 8
DEFAULT_POOL_BYTE_CAP = 32 MiB
```

or redesign the meaning of the defaults. The former doubles the documented
maximum retained committed pool memory per materialised heap from 16 to
32 MiB.

Required corrections:

1. Replace every “change `DEFAULT_POOL_SEGMENTS` 4→8” task with a paired
   `(segments, bytes) = (4,16 MiB) → (8,32 MiB)` decision.
2. Add a test asserting the proposed default resolves to 8.
3. Run the A/B against the actual prospective defaults, not a 256 MiB
   measurement-only byte ceiling.
4. Update README, integration docs and preset descriptions together.

Until this is addressed, the default-change task is technically malformed.

### P0: R26-1 does not prove cap 8 has no retention/RSS cost

R26-1 correctly fixes the cross-arm configuration contamination:

- one process per arm;
- requested cap recorded;
- resolved cap asserted;
- configuration-conflict delta asserted zero.

That is a real methodological improvement.

But its workload uses:

```text
RSS_BATCH_SIZE = 50
50 × 256 × 1024 bytes ≈ 12.5 MiB logical prefill
```

That fits at or below the current four-segment/16 MiB retention region once
allocator layout effects are considered. The probe:

- does not record `dbg_pooled_count`;
- does not record a pool-occupancy high-water mark;
- does not record decommit counts;
- does not assert that cap 4 was ever saturated;
- does not assert that cap 8 ever retained a fifth block.

Therefore identical peak RSS at caps 4/8/16/32 is expected even if larger caps
carry a real cost under a higher-pressure workload: the additional capacity
was not proven to be exercised.

The R26-1 report borrows “demand is six segments” from the separate
`LATENCY_BATCH_SIZE=120` experiment, but the RSS arm clamps its batch to 50.
The demand proof does not transfer across those different batch sizes.

Peak RSS while all arms hold the same live working set is also the wrong sole
metric for a retention policy. The pool's trade-off appears **after teardown**:
how much committed memory remains cached while the live working set is gone.

R26-3 directly exposes this:

| Configuration | Post-workload RSS | Post-workload commit |
|---|---:|---:|
| effective cap 4 | about 30.48 MiB | about 29.95 MiB |
| effective cap 8 | about 34.58 MiB | about 34.05 MiB |
| Difference | about +4.1 MiB | about +4.1 MiB |

The values recur throughout the A/B provenance. That is a deterministic
retention cost, not noise, and it contradicts the broad statement “there is no
cap-specific RSS cost to manage”.

R26-9's closure of the adaptive/process-wide design is therefore premature.

#### Required retention gate

Use subprocess isolation and configuration self-verification from R26-1, but
run the pressure-producing batch size 120 and record:

- requested and resolved segment and byte caps;
- `config_conflicts == 0`;
- peak live-set RSS/commit;
- RSS/commit immediately after full teardown;
- RSS/commit after 100 ms, 1 s and the configured decay interval;
- final and maximum pooled segment count per heap;
- decommit/release/reserve counters;
- 1, 8 and 32 simultaneous heaps.

The cap-4 arm must prove it saturated/decommitted, and the cap-8 arm must prove
it actually retained more than four segments. Without those counterfactuals,
an RSS equality is vacuous.

### P1: R26-3's “untimed warm-up” is actually timed

The examples say:

> One untimed warm-up batch, then 8 timed batches.

But `main` does:

```text
t0 = Instant::now()
run_workload()
```

and `run_workload` performs:

```text
run_latency_batch(...)        // labelled warm-up
for 0..8 { run_latency_batch(...) }
```

All nine batches are inside the timed interval. With batch size 120, the
metric covers 1080 cycles, not “960 timed cycles after an untimed warm-up”.

This does **not** invalidate the A/B direction:

- both arms time the same nine-batch shape;
- the result is large;
- the same-vs-same control is clean;
- the decommit counters independently confirm the mechanism.

It does make the report's timing description and per-run workload count
incorrect. Either:

- move the warm-up call before `t0`; or
- describe the metric honestly as nine timed batches / 1080 cycles.

Do not silently change it and compare new results with the old baseline.

### P1: a 250-line unsafe NO-GO implementation remains in `src/`

R26-7 retains:

- public `dbg_dealloc_batch_lazy`;
- private `dealloc_batch_small_lazy`;
- roughly 250 lines copied from the shipping batch deallocator;
- seven additional documented unsafe sites;
- thirteen benchmark arms/stubs.

The experiment is NO-GO at every batch size that overflows. Retaining a full
copy creates:

- future logic drift when the shipping guards or accounting change;
- doubled review burden in correctness-sensitive unsafe code;
- wider opt-in public surface;
- README unsafe-inventory noise;
- risk that tests cover one copy while the other diverges.

The justification cites reusable regression infrastructure, but the project's
own benchmark-hook policy says a NO-GO must re-evaluate and remove dependent
hooks unless continued value is explicit.

Keep the report, summary and raw results. Remove the lazy implementation and
its active benchmark arms from current source. Git history already preserves
the reproducer. If a future representation experiment is justified, restore
the exact commit temporarily or build a smaller local comparison.

### P1: new test-only hooks are not gated by `bench-internals`

Round 26 adds:

- `HeapCore::dbg_pool_cap`;
- `HeapCore::dbg_tcache_contains`;
- `HeapCore::dbg_is_free_for`.

They are safe read-only helpers, so they do not repeat the R24 soundness hole.
However, they have no production callers and are compiled into ordinary
feature combinations.

This contradicts the standing rule added after R25:

> Any hook with no production caller must default to `bench-internals`.

Gate the new helpers behind `bench-internals` and add that feature to the
specific examples/tests that consume them. Alternatively introduce a clearer
`test-internals` feature and make `bench-internals` imply it.

Safe does not mean it belongs in the production dependency surface.

### P2: the bitmap-clear benchmark still leaves a temporary invariant broken

R26-6 correctly rewrites the hook contract to require magazine-resident
blocks. The benchmark:

1. deallocates eight pointers into the magazine;
2. clears their magazine bitmap bits;
3. leaves the pointers in the magazine slots;
4. does not visibly flush or recycle the heap in the benchmark function.

After step 2, magazine slots and their double-free bitmap disagree. The unsafe
contract acknowledges that the caller must prevent ordinary allocator
operations from observing the temporary state, but the caller does not restore
the invariant before returning.

This may be harmless under iai's process isolation, but that execution
assumption should not be implicit in an unsafe API contract.

Prefer one of:

- delete the isolation hook now that the associated optimization region has
  four NO-GOs;
- restore the state after the measured region and arrange an identical
  cleanup in the subtraction baseline;
- explicitly make process termination the postcondition of this benchmark
  arm and prevent reuse of the heap.

### P2 tooling: the argv test is not part of the gate

Switching to `shell:false` direct argv is the correct repair. It removes the
cross-shell quoting problem instead of adding another quoting layer.

The new `argv-roundtrip-test.mjs` is not wired into `check-all.mjs` or another
committed automatic gate. It can therefore silently rot.

Improvements:

- run it from `check-all`;
- include `&`, `|`, `%`, parentheses and Unicode in addition to quotes/tabs;
- reject `opts.shell === true` inside `run` so the documented prohibition is
  executable rather than advisory;
- keep a separately named `runShell` only if a future caller genuinely needs
  shell syntax.

## Correctness review

### Per-block deallocation oracle is a real improvement

R26-5 closes the principal limitation of the aggregate R25-4 test.

For each original block it now distinguishes:

- in magazine and not marked free;
- not in magazine and marked free;
- both states, which would permit duplicate reuse;
- neither state, which would represent a leak.

It also asserts:

- exactly 16 magazine-resident blocks;
- exactly 184 genuinely free blocks;
- the magazine set is precisely the first 16 input blocks.

This is substantially stronger than live-count equality alone and directly
tests the first-warm policy.

The test should remain after its accessors are moved behind an internal-test
feature.

### Hook soundness fix remains correct

`dbg_overflow_bitmap_clear_pass` is still:

- `unsafe fn`;
- gated by `bench-internals`;
- documented with ownership, mapping and exclusivity requirements.

R26-6 improves the semantic description from “never freed live block” to the
actual magazine-resident state. The remaining issue is restoration of the
temporary invariant, not safe-code reachability.

## What can still be strongly accelerated?

### 1. Pool-pressure policy is the only measured strong default-path lever

The measured 16% gain is the largest credible current production-path
opportunity. The choice is not “cap 4 or cap 8 at no cost”; it is:

```text
4 segments / 16 MiB
versus
8 segments / 32 MiB
```

for each materialised heap, with actual retained cost determined by workload
demand.

Three viable product directions:

1. Keep the balanced default at 4/16 MiB and point teardown-heavy users to an
   explicit 8/32 or existing 16/64 throughput recipe.
2. Promote the paired default to 8/32 only after an idle-retention
   multi-thread gate.
3. Add an adaptive pressure policy that temporarily grows from 4 toward 8
   after repeated cap-full release/re-reserve events.

Given the safety-first/general-purpose identity, option 1 is the lowest risk.
Option 3 has the greatest chance of capturing the latency win without granting
every heap a permanent 32 MiB ceiling.

### 2. Adaptive pool growth deserves a real design

R26-9 closed it using a peak-live-set RSS experiment that did not prove the
extra capacity was exercised. R26-3's post-workload values supply the missing
trigger.

A bounded design:

- start each heap at effective cap 4;
- count cap-full releases within a time window;
- grow temporary local headroom toward 8 after repeated pressure;
- acquire global/process tokens only on rare pool growth;
- return excess tokens after idle decay;
- never add a shared atomic to per-allocation/per-free hot paths;
- keep the hard byte ceiling authoritative.

Acceptance must cover:

- R26-3 latency and decommit count;
- post-teardown RSS;
- 1/8/32 heaps;
- burst then long idle;
- thread turnover;
- no change to scalar hot-path Ir.

### 3. A reservation-only overflow tier is a conditional alternative

If committed adaptive retention is too expensive, evaluate a second tier
beyond the four committed hot segments:

- decommit payload pages;
- retain only the VA reservation and segment-table identity for a short time;
- recommit on reuse;
- release on decay or budget pressure.

This cannot remove recommit/page-fault cost and earlier work correctly rejected
decommit-then-pool as a replacement for the hot committed pool. As an
**overflow-only second tier**, it may still avoid reserve/release and metadata
reinitialization while keeping committed RSS bounded.

Proceed only with an isolated breakdown showing reserve/table setup is a
material fraction after recommit cost. Otherwise it merely adds complexity.

### 4. Batch work should stop until a consumer exists

The project now has:

- a validated eager stage size;
- a strong per-block correctness oracle;
- four consecutive NO-GOs in nearby bookkeeping/overflow optimizations;
- no production batch consumer.

There is no case for more batch micro-optimization. The next trigger should be
an end-to-end downstream workload, not another synthetic representation.

### 5. `flush_class` isolation is a diagnostic task, not a promised speedup

`OPEN_ITEMS.md` still calls the roughly 487 Ir remainder an untried lever.
Four nearby experiments have shown that apparently removable components often
become more expensive in their final compiler context.

An isolation measurement may improve attribution, but do not open an
implementation task until it identifies:

- a concrete removable operation;
- a caller that pays it frequently;
- a design that preserves cache warmth and double-free accounting.

## What not to optimize next

Avoid:

- another staging-array representation;
- another `FLUSH_N` sweep;
- bitmap-clear coalescing;
- run encoding for non-contiguous magazine flushes;
- NUMA directory work already completed in R11-6;
- cap 16/32 experiments on a workload whose demand stops at six;
- public batch API promotion without a consumer.

## Project improvements

### 1. Treat paired configuration knobs as one decision

Every pool-cap report must carry:

- requested `pool_segments`;
- requested `pool_byte_cap`;
- resolved effective segment cap;
- maximum possible retained commit;
- observed pooled-count high water.

Reporting only the segment knob allowed an impossible default recommendation
to survive several tasks.

### 2. Separate live-set peak from retained-idle memory

Memory gates for caches/pools need at least:

- peak while allocations are live;
- immediately after teardown;
- after idle/decay;
- after thread exit/recycle.

Calling only the first one “RSS cost” hides the policy's main trade-off.

### 3. Require victim activation

A benchmark is not allowed to conclude that a capacity change is free unless
it proves:

- baseline cap saturated;
- candidate used capacity beyond baseline;
- relevant miss/decommit counter differs.

Requested/resolved configuration identity is necessary but not sufficient.
R26-1 proves it ran cap 8; it does not prove the workload used slots 5–8.

### 4. Remove rejected experimental code

Keep durable evidence in reports and Git history, not as duplicated unsafe
production-crate source. Add a close-task checklist:

- remove prototype methods;
- remove active bench arms that exist only for the rejected variant;
- restore unsafe inventory;
- retain raw/summary evidence;
- retain a compact reproduction commit reference.

### 5. Make internal diagnostic features coherent

The project currently mixes:

- safe public test hooks compiled in production;
- unsafe hooks gated by `bench-internals`;
- integration tests requiring public visibility;
- benchmark copies embedded in runtime modules.

Introduce a deliberate `test-internals`/`bench-internals` hierarchy and keep
all no-production-caller hooks out of ordinary builds.

### 6. Fix current-state documentation rather than append another caveat

The current state should say:

- no Round 26 shipping speedup;
- explicit effective cap 8 gives 16% on R26-3;
- peak live-set RSS was flat in R26-1's lower-pressure shape;
- cap 8 retained about 4 MiB more after R26-3;
- actual default decision is 4/16 versus 8/32;
- adaptive design remains open.

Appending this only as another historical note will leave the misleading
“RSS-neutral, no adaptive trade-off” headline active.

## Proposed Round 27

| Priority | Task | Acceptance condition |
|---|---|---|
| P0 | Correct default proposal from one knob to `(4,16 MiB) → (8,32 MiB)` | Proposed defaults resolve to effective cap 8 |
| P0 | Correct R26-1/R26-9 current-state verdict | Peak-live equality is not presented as zero retention cost |
| P0 | Pressure-active post-teardown RSS gate at batch 120 | Cap4 saturates/decommits; cap8 uses >4 pool entries |
| P1 | A/B actual prospective default pair | Same latency win as explicit config, with documented retention cost |
| P1 | 1/8/32-heap idle/decay memory curve | Aggregate retained commit stays within product budget |
| P1 | Reopen adaptive/global-token design | Design is gated by measured post-teardown cost |
| P1 | Remove R26-7 lazy NO-GO implementation and unsafe duplication | Shipping source returns to one batch-dealloc implementation |
| P2 | Gate new diagnostic helpers behind internal-test features | No new test-only API in ordinary production builds |
| P2 | Correct R26-3 timed workload description | Warm-up placement and cycle count match source |
| P2 | Wire argv roundtrip into check-all and forbid shell:true | Tooling regression test runs automatically |
| Conditional | Reservation-only overflow tier | Reserve/setup cost is measured material after recommit |

## Final assessment

Round 26 is another evidence/correctness wave, not a shipping acceleration
wave.

It successfully:

- validates the previous review's configuration-contamination finding;
- confirms a real 16% latency opportunity through the global allocator;
- strengthens batch-free correctness testing;
- rejects an unprofitable lazy-stage representation;
- repairs cross-platform process launching.

It also draws two incorrect product conclusions:

1. a one-constant default edit is proposed even though the unchanged byte cap
   would clamp it back to four;
2. cap 8 is called RSS-neutral even though the RSS workload did not prove it
   exercised slots 5–8, while the pressure-producing R26-3 data shows a
   repeatable post-workload retention increase.

The next large gain is not hidden in another magazine instruction. It is in
choosing or designing the pool retention policy correctly:

- explicit 8/32 for throughput-sensitive users;
- 4/16 for balanced memory behavior;
- or adaptive growth from 4 toward 8 under demonstrated pressure.

For ordinary hot scalar churn, no untried radical speedup is visible in the
reviewed code.
