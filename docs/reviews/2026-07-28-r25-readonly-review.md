# Round 25: readonly performance, correctness and project review

Date: 2026-07-28  
Reviewed range: `1164718..9ddb062`  
Mode: Git history and source/document inspection only

## Executive verdict

Round 25 did **not** make the shipping allocator faster.

Only one file under `src/` changed:
`src/registry/heap_core_diag.rs`. That change correctly closes the Round 24
soundness hole by making a measurement hook unsafe and compiling it out of
plain production. No production allocation, deallocation, refill, reclaim,
pool or segment-selection algorithm changed.

The round nevertheless produced useful results:

- the `STAGE_CAP=64` batch-only optimization from Round 24 was validated up
  to `N=1024` and still wins throughout that range;
- the proposed full-magazine flush policy was correctly rejected after a
  `2.42x` oscillating-workload regression;
- the batch multi-flush test was strengthened with a direct live-count oracle;
- the previously reported NUMA directory cliff was correctly recognized as
  already fixed in R11-6;
- the run-encoded free design was correctly deferred because its contiguity
  precondition does not hold for magazine overflow.

However, the main apparent GO-candidate of the wave,
`pool_segments: 4 -> 8`, is not ready for promotion. Its RSS/multi-thread
sweep conflicts with the allocator's documented first-materialisation-wins
configuration semantics. The sequential arms can reuse registry slots that
remain permanently configured by an earlier arm. Thus rows labelled
`cap=8/16/32` may actually run `cap=4`.

The honest answer is:

> Round 25 improved correctness and confidence in an earlier experimental
> batch optimization. It did not accelerate default production, and its only
> new default-production GO-candidate needs a corrected gate before it can be
> trusted.

## Scope and limitations

Inspected:

- all commits and diffs in `1164718..9ddb062`;
- allocator and registry files relevant to the changed hooks and pool sweep;
- the new tests and benchmark probes;
- R25-3, R25-5, R25-7 and R25-8 reports and summaries;
- current `OPEN_ITEMS.md`, feature declarations and benchmark scripts.

No build, test, benchmark, formatter, linter, Node script or project helper was
run. Checked-in measurement numbers are reviewed for internal consistency but
were not independently reproduced.

The pre-existing untracked `.claude/` directory was not modified.

During this review another agent committed `9ddb062`, which added the Round 25
CHANGELOG/checkpoint and the previously untracked R24 readonly review. That
concurrent docs-only commit is included in the final reviewed range.

## What actually changed

| Area | Result | Shipping runtime impact |
|---|---|---|
| P0 diagnostic hook | Made unsafe and gated behind `bench-internals` | Removes an unsafe public production surface; no hot-path acceleration |
| Full-flush sweep | `FLUSH_N=4/12/16` rejected | No retained runtime change |
| Batch oracle | Added isolated live-count test | Test-only |
| Pool-cap sweep | `4 -> 8` labelled GO-candidate | Measurement only; default remains 4 |
| Adaptive pool design | Closed because the fixed-cap result appeared sufficient | Documentation only; closure is premature |
| Batch stage boundary | Confirmed `STAGE_CAP=64` through `N=1024` | Validates Round 24's opt-in change; no new change |
| Run-encoded free | Conditional design, not recommended now | No runtime change |
| NUMA directory | Confirmed already fixed in R11-6 | No new change |
| Script repair | Adjusted Node shell argument quoting and feature set | Developer tooling only |
| Round checkpoint | Added CHANGELOG, checkpoint and tracked R24 review | Documentation only |

The only `src/` delta is the diagnostic-hook hardening. Therefore any claim
that this range itself accelerated allocator execution would be false.

## Did the earlier batch acceleration survive review?

Yes, within the measured range.

R25-7 compared `STAGE_CAP=64` with the old 512-entry stage at:

`N = 16, 64, 80, 81, 128, 200, 512, 1024`.

The 64-entry stage wins in both Ir and estimated cycles at every point:

| N | Ir improvement |
|---:|---:|
| 16 | 47.74% |
| 64 | 24.26% |
| 80 | 20.89% |
| 81 | 18.73% |
| 128 | 14.35% |
| 200 | 9.49% |
| 512 | 3.43% |
| 1024 | 1.36% |

The result has a coherent mechanism:

- removing the larger stack initialization saves about 4065 Ir;
- each extra intermediate flush costs about 109 Ir;
- the fixed saving is gradually consumed as `N` grows;
- the projected crossover is around `N=2700`, beyond the stated expected
  batch range.

This is credible validation of a real Round 24 optimization. It remains:

- opt-in under `batch-api`;
- absent from the normal production feature bundle;
- without an identified production consumer.

It therefore should be described as “validated experimental batch
acceleration”, not as a default allocator speedup.

## Findings

### P0 measurement validity: R25-5 RSS arms may not use their labelled cap

The RSS probe creates a local `SeferAlloc::with_config(...)` in newly spawned
threads for every arm and assumes:

> a fresh thread claims a fresh, never-before-configured registry slot.

That assumption contradicts the current registry implementation and its own
API documentation.

The actual lifecycle is:

1. a thread first allocates and claims a registry slot;
2. that slot's `HeapCore` is materialised with the requested config;
3. on thread exit, `HeapRegistry::recycle` pushes the whole slot onto the
   reusable free-slot stack;
4. the next thread normally reclaims a recycled slot before minting a fresh
   one;
5. an already materialised slot keeps its original config for the process
   lifetime;
6. `claim_with_config` only compares the requested config with the live one;
   on mismatch, release builds count `CONFIG_CONFLICTS` and silently use the
   old config.

`SeferAlloc::with_config` explicitly documents multiple differently
configured instances in one process as effectively unsupported for exactly
this reason.

The R25-5 RSS sweep runs, in one process:

- cap 4, then 8, 16 and 32;
- first for 1 thread, then 8 threads, then 32 threads.

Consequences:

- after the cap-4 arm exits, its materialised slot is available for immediate
  reuse;
- the cap-8 thread can reclaim that cap-4 slot;
- in release it continues with cap 4 and increments `config_conflicts`;
- at the 8-thread phase, recycled slots created by earlier arms can likewise
  dominate subsequent configurations;
- the probe does not assert the resolved cap on the RSS axis and does not
  assert a zero `config_conflicts` delta.

The direct `AllocCore` latency axis does self-verify `dbg_pool_cap()` and is
not affected. The `SeferAlloc` RSS/multi-thread axis is affected.

Therefore these conclusions are currently unsupported:

- “cap 8 has lower RSS than cap 4”;
- “cap 16/32 add no RSS cost”;
- “the per-thread fixed-cap trade-off does not exist”;
- “the adaptive/process-wide-budget design gate was not met”.

The near-identical cap-8/16/32 RSS rows cannot repair the problem; they are
also consistent with every arm silently reusing the same earlier
configuration.

#### Required correction

Rebuild the RSS gate with **one fresh process per tuple**
`(pool_segments, thread_count, repetition)`:

1. pass exactly one cap to the child process;
2. materialise every worker slot with that same config;
3. assert `config_conflicts == 0`;
4. expose and assert each worker's resolved pool cap, or use a diagnostic
   acknowledgement from the claimed heap;
5. take the baseline only after process bootstrap and before workload
   materialisation according to one fixed protocol;
6. run several paired/reordered repetitions, not one sequential point;
7. aggregate median/distribution and peak commit/RSS;
8. use a separate clean process for the next cap.

Also run the actual production-shaped teardown judge A/B/B/A with cap 4 and
8. The current latency arm uses a deliberately selected batch size of 120 to
create a six-segment demand cliff. That is useful isolation, but it is not by
itself a universal default-selection workload.

Until this is done:

- do not change `DEFAULT_POOL_SEGMENTS`;
- downgrade R25-5 from GO-candidate to “latency mechanism confirmed, RSS gate
  invalid”;
- reopen R25-6's adaptive/process-wide-budget decision.

### P1 documentation: the final Round 25 summary repeats the invalid RSS claim

The concurrent final docs commit `9ddb062` correctly states that Round 25 has
zero runtime improvements and adds the previously missing R24 review to Git.

Its Round 25 headline, CHANGELOG body and checkpoint also repeat that
`pool_segments 4 -> 8` wins on both latency and RSS and that no adaptive
trade-off exists. Those statements inherit the invalid RSS-arm assumption
described above and should be corrected alongside `OPEN_ITEMS.md`.

The repository-link integrity concern is now resolved: the cited R24 report is
tracked at the final reviewed HEAD.

### P1: R25-4 is an aggregate accounting oracle, not a per-block oracle

The new isolated test is a strong improvement over the old global-allocator
test. It proves:

- the three-flush path executes;
- aggregate `live_count` falls by exactly 184;
- the magazine ends with exactly 16 resident blocks;
- dropping two staged chunks changes the observed delta as predicted.

It does not prove the stronger sentence “every one of the N blocks is
correctly accounted for” at block identity level. A live-count delta is an
aggregate. In principle, one missed block and one wrongly processed block in
the same segment can preserve the same total.

The current implementation structure and double-free bitmap make that class
of error unlikely, but the test should state precisely what its oracle sees.
For a true per-block oracle, additionally inspect every original offset:

- the first accepted 16 must be represented in the magazine bitmap/slots;
- every remaining accepted offset must be marked free in the authoritative
  allocation bitmap or reachable through the expected free structure;
- no offset may appear in both states.

The hardcoded `TCACHE_CAP=16` is acceptable as a deliberate regression
constant, but an internal diagnostic accessor would make future policy
changes easier to audit.

### P2: the repaired hook's safety contract is internally ambiguous

Making `dbg_overflow_bitmap_clear_pass` an unsafe function and gating it
behind `bench-internals` correctly closes the safe-code soundness hole.

Its safety text should still be tightened. It says pointers must reference
live blocks that “have not been freed back to the allocator”, while the only
caller:

- first calls `dealloc` on them;
- expects them to be magazine-resident;
- then clears their magazine bits.

In this allocator, magazine-resident blocks still count as live for
`live_count`, but they have already been deallocated by the public API.
“Live” and “not freed back” are therefore easy to interpret in conflicting
ways.

State the real required state directly:

- each pointer is owned by this heap;
- it is currently present in this heap's magazine;
- its magazine bitmap bit is set;
- the segment remains mapped and registered;
- no concurrent owner mutation occurs;
- the caller will restore/complete a consistent allocator state before any
  ordinary allocation or deallocation observes the temporarily cleared bit.

The last condition matters because the hook deliberately creates a temporary
disagreement between the magazine slots and their bitmap.

### P2 tooling: shell quoting fix is narrower than its shared abstraction

`scripts/lib.mjs::run` now adds double quotes around every whitespace-bearing
argument when `shell: true`.

That fixes the reported multi-word Cargo feature argument, but handwritten
shell quoting is platform-sensitive:

- Windows `cmd.exe`, POSIX shells and WSL command layers do not share one
  escaping grammar;
- `\"` is not a universal way to escape an embedded double quote;
- arguments containing shell metacharacters but no whitespace are still
  interpreted by the shell;
- the helper applies this transformation to all callers, not just Cargo
  feature lists.

Most observed callers execute a concrete program with an argument array and
do not need a shell. Prefer:

- `shell: false` as the default and normal path;
- direct executable plus argv for Cargo/Node/WSL;
- an explicit, narrowly named `runShell(commandString)` only where shell
  syntax is actually required;
- a small argument-roundtrip test covering spaces, quotes, `&`, `|`, `%`,
  parentheses and Unicode on supported hosts.

This is not an allocator runtime issue, but the verification pipeline is only
as reliable as its process launcher.

## Review of the rejected and deferred optimizations

### Full flush was correctly rejected

R25-3 held `TCACHE_CAP=16` fixed and swept only
`FLUSH_N=4/8/12/16`, which is the correct experiment.

The result is decisive:

- `FLUSH_N=16` improves bulk `N=1024` free by only 1.5%;
- the oscillating live-set judge regresses from 25,719 to 62,183 Ir;
- refill events increase from 1 to 20;
- hot scalar churn remains identical because it never overflows;
- 4 and 12 provide no compensating win.

Keeping `FLUSH_N=8` is justified. This also shows why overflow optimization
cannot ignore the subsequent allocation phase: reducing free-side setup can
destroy cache warmth and lose much more on refill.

### Run encoding is correctly outside the current plan

The R25-8 design identifies the load-bearing blocker:

- magazine entries are ordered by free time, not by address;
- an overflow slice is generally not offset-contiguous;
- producing arithmetic runs would require sorting/grouping work;
- the per-block double-free bitmap transition still cannot be removed.

Thus run encoding does not solve the bulk-free victim that motivated it.
Its only plausible use is a future contiguous batch consumer, where it may
remove dependent `read_next` loads on the allocation side.

Both required triggers are currently absent:

1. no production batch consumer;
2. no measurement showing the dependent-load chain dominates that consumer.

Do not implement this design now.

### NUMA recommendation is closed correctly

The previous readonly report repeated the old R10-6 NUMA cliff without
checking the later R11-6 implementation. Current source contains the
node-indexed `class_nonempty_by_node` directory and uses it in NUMA preference
order.

Round 25 correctly closes this item. No additional NUMA directory work should
be scheduled from the obsolete R10 measurement.

## What can still be accelerated strongly?

### 1. Revalidate pool cap 8; this is the strongest default-path candidate

The valid direct-`AllocCore` half of R25-5 shows:

- cap 4: 20 decommits/releases;
- cap 8: zero;
- cap 16/32: also zero;
- observed pool demand: six segments.

This robustly demonstrates the mechanism: when a workload repeatedly needs
six segments, a cap of four forces unnecessary OS lifecycle work and a cap of
eight absorbs it.

The point wall-clock values suggest roughly a 32% cap-4-to-cap-8 improvement
in that isolated workload, but they are shared-host single points and should
not be promoted as a stable percentage yet.

If the corrected subprocess RSS gate confirms acceptable memory behavior,
raising the default to 8 could be the largest remaining simple
default-production win for teardown-heavy workloads. It will not improve
ordinary hot churn, and it may retain up to four additional 4 MiB segments per
busy heap in a higher-demand counter-workload. That is why the corrected
many-thread memory gate is mandatory.

### 2. Reopen an adaptive/global pool budget if fixed cap 8 costs memory

R25-6 was closed only because the invalid RSS axis appeared to show no
trade-off. If the corrected gate finds a cap-8 RSS penalty, revisit:

- per-heap hot retention above four only after recent reuse;
- process-wide tokens limiting aggregate committed pooled segments;
- token acquisition/release only on rare pool grow/shrink transitions;
- no shared atomic on allocation or deallocation hot paths;
- decay/idle return of excess tokens.

This can preserve the six-segment teardown win without granting every thread
an unconditional eight-segment committed allowance.

### 3. Lazily initialize batch staging

`dealloc_batch_small` still unconditionally initializes:

```text
[*mut u8; 64] = [null; 64]
```

even when:

- the batch contains at most 16 accepted owned blocks;
- every block fits in the magazine;
- the staging array is never read.

The earlier 512-to-64 result proves stack initialization is not elided.
Therefore a safe lazy representation is worth a narrow batch-only gate:

- begin with `Option<[ *mut u8; 64 ]>::None`;
- instantiate the array only on the first actual overflow block;
- keep the current flush logic after materialisation.

For `N<=16` this may remove the remaining 512-byte initialization without
adding unsafe code. For larger batches it should be close to the current path
plus one cold branch. Measure `N=0/1/8/16/17/64/81/200/1024`.

This is unlikely to be radical for the project because `batch-api` has no
consumer, but it may be a meaningful percentage improvement for small batch
calls. Do not use `MaybeUninit` until the safe lazy form is measured.

### 4. Adoption can matter more than another allocator micro-optimization

The project has already measured a real warm batch advantage, and R25 confirms
the deallocation staging design remains beneficial. The larger opportunity
may now be finding an actual workload able to express:

- homogeneous allocation batches;
- homogeneous deallocation batches;
- contiguous carve-produced groups.

Only after an end-to-end consumer exists do run descriptors, wider flushes or
public batch surface become economically justified.

For ordinary `Box`/`Vec` scalar traffic, these mechanisms provide no gain.

### 5. Linux sub-region remap remains conditional, not a next default task

The current open-items record still identifies a Linux-only sub-region
`mremap` possibility for specific medium/extents realloc workloads. Its upside
can be asymptotically large by avoiding copy, but:

- it has no Windows equivalent in the proposed form;
- medium-class promotion previously failed a realloc regression gate;
- the required workload-shape measurement/consumer is absent;
- correctness complexity is high.

Keep it as a consumer-triggered design, not the next general optimization.

## What should not be optimized next

Do not spend the next wave on:

- another `FLUSH_N` ratio;
- bitmap-clear prepasses or generic bulk masks;
- run encoding for magazine overflow;
- NUMA directory lookup;
- ordinary scalar interleaved churn;
- cap 16/32 for the measured six-segment workload;
- publishing batch APIs without a consumer.

These areas are now either measured NO-GO, already fixed, or lack a victim.

## Code and project improvements

### 1. Make configuration identity part of every benchmark result

Every configuration sweep must prove the runtime used the labelled value.

Required evidence per arm:

- requested value;
- resolved value;
- configuration-conflict delta;
- process/heap identity;
- whether the object was newly materialised or recycled.

A row without this evidence should not be eligible for GO.

### 2. Use subprocess isolation for process-lifetime allocator state

The registry, heap slots, sidecars and allocator configuration intentionally
survive thread reuse. Sequential in-process A/B experiments are therefore
unsafe unless the measured state has an explicit reset.

Create one common subprocess harness for:

- pool configuration sweeps;
- first-heap commit/RSS;
- registry materialisation;
- sidecar residency;
- any feature whose state persists across thread exit.

This removes a recurring class of order-dependent measurement errors.

### 3. Reopen the R25-5/R25-6 documentation verdict

`OPEN_ITEMS.md` currently says:

- the RSS sweep is done;
- cap 8 wins latency and RSS;
- no adaptive trade-off exists;
- only the default-change decision remains.

That state should be corrected after reviewing the slot-reuse issue. The
valid current statement is:

- cap 8 eliminates the isolated direct-`AllocCore` decommit cliff;
- the multi-thread RSS comparison is unresolved;
- default promotion and adaptive-policy closure are both pending a valid
  process-isolated gate.

### 4. Automate the benchmark-hook policy

The new `CLAUDE.md` rule is sensible, but prose alone did not prevent the
original defect and cannot enforce future changes.

Add a lightweight source/API check that flags:

- public `dbg_*` functions accepting raw pointers;
- such functions lacking `unsafe fn`;
- benchmark-only hooks not gated by `bench-internals`;
- hooks with zero remaining call sites after a NO-GO revert.

Human review remains necessary, but the common shape is mechanically
detectable.

### 5. Reduce report duplication and stale “current state”

Round 25 adds over seven thousand lines, mostly raw logs, duplicated probe
logic and narrative. `OPEN_ITEMS.md` simultaneously contains:

- current-state summaries;
- long historical appendices;
- stale next triggers;
- corrections to prior corrections.

Continue the current-state-first effort, but make the current section
generated from compact machine-readable verdict records. Keep raw logs as
artifacts or compressed archival evidence, not as the primary navigation
surface.

The new pool-sweep error illustrates the risk: once a conclusion is copied
into several files and closes a later task, correcting it becomes much more
expensive.

### 6. Stop calling empirically tuned replicas “exact”

The pool probe copies the churn primitives but selects
`LATENCY_BATCH_SIZE=120` empirically. That is a valid targeted pressure judge,
but it is not literally the exact dynamic Criterion execution schedule.

Use precise labels:

- “same primitive and batching shape”;
- “empirically selected six-segment pressure”;
- “actual Criterion end-to-end A/B”.

This makes the boundary between root-cause judge and user-facing benchmark
clear.

## Proposed Round 26

| Priority | Task | Acceptance gate |
|---|---|---|
| P0 | Correct R25-5 RSS sweep using one process per cap/thread/repetition | Every arm asserts resolved cap and zero config conflicts |
| P0 | Reopen R25-6/default-cap decision in docs | No GO claim rests on invalid RSS rows |
| P1 | Actual teardown A/B/B/A for cap 4 versus 8 | Stable latency win plus decommit mechanism |
| P1 | Corrected 1T/8T/32T RSS/commit gate | Aggregate memory remains within an explicit budget |
| P1 | Correct R25-5 claims in CHANGELOG, checkpoint and OPEN_ITEMS | No current-state document treats invalid RSS rows as GO evidence |
| P2 | Strengthen batch oracle to per-offset state | Every original block has exactly one valid free representation |
| P2 | Clarify diagnostic hook's transient-state safety contract | Caller contract matches the actual magazine-resident setup |
| P2 | Safe lazy batch-stage prototype | Wins N<=16; no regression through N=1024 |
| P3 | Replace shared shell quoting with direct argv execution | Cross-platform argument roundtrip tests pass |
| Conditional | Adaptive/global pool token design | Only if corrected cap-8 RSS gate exposes a trade-off |

## Final assessment

Round 25 is a good correction wave but not an acceleration wave.

What is solid:

- the P0 safe-code soundness hole is closed;
- `STAGE_CAP=64` is validated through realistic and larger batch sizes;
- `FLUSH_N=8` survives a properly multidimensional policy sweep;
- run encoding and redundant NUMA work were correctly declined;
- batch accounting tests are materially better.

What is not solid:

- the RSS evidence used to label cap 8 faster and cheaper;
- the closure of adaptive pool design based on that evidence;
- any claim that Round 25 changed shipping allocator speed.

The strongest remaining path is now very focused:

1. rerun the pool gate with process isolation and verified configuration;
2. promote cap 8 only if both latency and aggregate memory gates pass;
3. otherwise build an adaptive process-wide retention budget;
4. continue batch optimization only alongside a real consumer.

For ordinary hot scalar churn, no radical untried lever is visible in the
reviewed code. Large gains remain possible in teardown-heavy workloads and
consumer-driven batch paths, but they require correct workload-state and
configuration isolation rather than another local hot-loop rewrite.
