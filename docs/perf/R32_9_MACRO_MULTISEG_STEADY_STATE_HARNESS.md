# R32-9 (task #500) — the missing `>=64-live-segment` macro-bench harness: built, verified, first smoke-test read

Date: 2026-08-02.

landing_commit: 2ea920b98fbf5f75b9a92d74ed32fd8e96d04c65

## 0. What this is

This is **infrastructure**, not an optimization or a promotion decision. It
implements finding **F3** in `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`
("Meta-finding: the project's deterministic judge (`Ir`) is structurally
blind to the class of win that is left, and six separate items are now
blocked on the same missing macro-benchmark") and closes the missing
artifact `docs/perf/OPEN_ITEMS.md` item 34 has named since 2026-07-31: *"a
realistic >=64-live-segment / long-lived-process macro-benchmark."*

**What shipped:**

1. `benches/macro_multiseg_steady_state.rs` — a new, standalone
   `iai-callgrind` bench target (Linux-only, same platform-gating shape as
   the existing `benches/perf_gate_iai.rs`), reporting `Estimated
   Cycles`/RAM-hit cache-simulation numbers for a genuinely wide
   (`>=320 MiB` single-thread, `>=1.28 GiB` 4-thread) working set.
2. `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs` — the portable
   wall-clock companion (runs on any platform, including this project's
   Windows dev environment), driven through the same
   `HeapRegistry::claim`/`HeapCore` entry point `SeferAlloc`'s real
   `#[global_allocator]` implementation itself calls into.
3. `HeapCore::dbg_table_count` (`src/registry/heap_core_diag.rs`) — a new,
   `#[doc(hidden)]`, `alloc-global`-gated, thin delegation to the
   already-existing `AllocCore::dbg_table_count`. Both harnesses'
   path-activation oracle reads this to prove — not assume — that the
   target working-set shape was actually achieved.
4. `tests/heap_core_dbg_table_count.rs` — dedicated coverage for the new
   forwarder (grows-by-exactly-N assertion against real Large-segment
   registration through the same `HeapCore` handle).
5. `tests/dbg_hook_safety_tripwire.rs` — updated allowlist entry for the new
   hook (pure observer, read-only, no mutation).
6. This report + `docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS_summary.csv`
   (derived by `scripts/r32_9_derive_smoke_summary.mjs` from
   `docs/perf/_raw_r32_9_smoke_test.log`).

**No production code changed.** `Cargo.toml`'s `production` feature
composition is unchanged; the new `dbg_table_count` forwarder is
`#[doc(hidden)]` and reachable only behind `alloc-global` (already in
`production`, but the hook itself has zero production callers — it is a
pure read-only introspection accessor, matching the safety category of its
sibling `dbg_kind_at_tag`/`dbg_pooled_count`). Runtime behavior of the
allocator is byte-for-byte unchanged by this task.

## 1. Why this artifact matters — the four items it unblocks

`docs/perf/OPEN_ITEMS.md` item 34 records that four independently-filed,
independently-rejected findings all bottomed out on the identical
structural wall: every benchmark in this project's suite spans **at most 3
live segments** (`multiseg_cold_256k`, the widest existing one), so any
optimization whose payoff scales with segment COUNT is structurally
invisible to measurement here, regardless of its real-world value.

| Item | Mechanism | Verdict at n=3 | Own text's stated precondition |
|---|---|---|---|
| X5 (item 20) | per-class segment-queue bitmap | REJECT — "maintenance RMW cost dominates; no cache line actually avoided at this scale" | "a future arc that adds a >=64-segment bench... may flip the verdict" |
| T10 (item 22) | per-class "last found segment" hint | NO-GO — `[u16; 49]` init cost alone exceeds the churn kill-gate | "a future arc that adds a >=64-segment bench (or profiles a real application with 100+ long-lived small segments) may flip this verdict" |
| R1 (item 23) | per-segment availability hint | NO-GO (4th independent attempt) | "a future arc that adds a genuine >=64-segment bench... is the prerequisite for re-opening R1/X5/T10" |
| R15-1 (item 9) | nonempty-summary-word opt for `drain_dirty_segments` | honest reject, below noise floor at current scale | `MAX_SEGMENTS` raised by a large factor OR much-higher producer-class fan-in than N=8 |

**Does this harness satisfy each precondition?**

- **X5, T10, R1** — YES, directly. All three name "a >=64-segment bench" (or
  the R1 variant, "100+ simultaneously-live small segments") as their exact
  trigger. `multiseg_steady_state_1t`/`_mt4` (in
  `benches/macro_multiseg_steady_state.rs`) establish 80 live segments
  (oracle-verified `>= 64`) and keep them live through a steady-state churn
  region — exactly the shape all three ask for. A future task re-attempting
  any of these three mechanisms can now measure `Estimated Cycles`/RAM-hits
  at this scale instead of at n=3.
- **R15-1** — PARTIALLY. Its trigger has two independent halves: (a)
  `MAX_SEGMENTS` raised by a large factor (a capacity-axis claim, not a
  live-count claim — `MAX_SEGMENTS` is already 4096, unrelated to how many
  segments are simultaneously LIVE in a given workload), and (b) a
  much-higher producer-class fan-in than N=8 (a Small-class-COUNT axis, the
  number of distinct size classes actively producing dirty segments, not a
  segment-COUNT axis). This harness satisfies the *live-segment-count* half
  of R15-1's own concern (80 simultaneously-live segments, exceeding what
  any prior R15-1 measurement modeled) but does **NOT** by itself raise the
  producer-class fan-in — `multiseg_steady_state_1t`'s churn uses ONE Small
  class (16 B) and ONE Large-class rotation, not N=8+ distinct producing
  classes. A future task revisiting R15-1 would need to EITHER confirm the
  live-segment-count half alone is sufficient to flip the verdict, or extend
  this harness's churn step to fan out across more size classes — noted
  here explicitly so a future reader does not assume this harness alone
  fully reopens R15-1 on both named axes.

## 2. Workload design

### 2.1 The floor: `>=64` live segments, held for the whole timed region

`FLOOR_LARGE_OBJECTS = 80` distinct objects, each sized `SMALL_MAX + PAGE`
(just over the Small/Large boundary — one dedicated `SEGMENT` = 4 MiB each),
are allocated ONCE at setup and held live for the **entire** timed region —
never freed until teardown. 80 gives 25% headroom past the 64 threshold
(`docs/perf/OPEN_ITEMS.md` item 34's own named number) so the
path-activation oracle's `>= 64` check has room to fail loudly on a real
regression instead of sitting exactly on the boundary.

At `SEGMENT = 4 MiB` (`src/alloc_core/os.rs`), 80 segments is a **~320 MiB**
working set (single-thread arm) / **~1.28 GiB** (4-thread arm, own floor per
thread) — genuinely too large to fit in a typical L2 (commonly 0.25-2 MiB)
or even many L3 caches (commonly 8-32 MiB) in full. This report does NOT
assume any specific real-hardware cache size, per the task's own framing:
`iai-callgrind`'s cache SIMULATION model (a synthetic LRU model of L1/L2/LL,
not real hardware performance counters) is what actually produces the
`Estimated Cycles`/RAM-hit numbers, so whatever the simulated cache sizes
are, a working set two to three orders of magnitude larger categorically
will not fit them.

### 2.2 Steady-state, not a burst

The floor is established once; the TIMED region then runs `CHURN_ROUNDS`
rounds of mixed alloc/dealloc churn ON TOP of the already-live floor —
segments accumulate and STAY live for the whole timed region. This is the
survey's explicit "not allocate N, free N in a burst" requirement, applied
at the SEGMENT-COUNT axis specifically (the workload's segment count is
flat and high throughout the timed region, not spiking then draining).

### 2.3 Mixed Small/Large size distribution

Each churn round does BOTH: (a) a Small-class churn step (16 B blocks,
byte-identical shape to `perf_gate_iai.rs::small_churn_16b`, so this
harness's Small half is directly comparable to the existing tiny-working-set
gate's own number), and (b) a Large-class churn step (allocate + immediately
free one dedicated-segment object, same size as the floor objects, rotating
through the large-cache's 8 base slots without ever touching the floor
itself).

### 2.4 Single-thread and multi-thread variants

Per the task's own staging suggestion ("a single-thread >=64-segment
variant is also useful and cheaper to build first"), both are shipped as
PERMANENT sibling arms, not a staged replacement of one by the other:

- `multiseg_steady_state_1t` — the cheaper single-thread baseline.
- `multiseg_steady_state_mt4` — 4 threads, each running the IDENTICAL
  per-thread workload (own `HeapCore`, own oracle-verified 80-segment
  floor), for genuine cross-thread registry/table contention the
  single-thread variant cannot exercise.

The wall-clock companion (`examples/r32_9_macro_multiseg_steady_state_ab_gate.rs`)
mirrors this as a `THREAD_COUNTS = [1, 4]` sweep, both arms measured under
subprocess-per-arm isolation.

## 3. Path-activation oracle (CLAUDE.md R30-8 rule)

Per the rule's own wording — *"the harness must prove it actually achieved
its target working-set shape... not just 'trust the allocation count'"* —
both harnesses hard-assert `HeapCore::dbg_table_count() >=
MIN_LIVE_SEGMENTS_ORACLE` (64) on every claimed heap, read back IMMEDIATELY
after the floor is established and BEFORE any timed churn begins.

**Why this is a genuine oracle, not a restatement of the input.**
`dbg_table_count` reads `SegmentTable::count` — the table's registered
high-water slot count — off the SAME `HeapCore` the churn workload runs
against, via a diagnostic accessor that did not exist at the `HeapCore`
level before this task (only at the lower `AllocCore` level). A bug in
registration, hysteresis pooling, or premature recycling would silently
under-fill the intended working set even though `FLOOR_LARGE_OBJECTS = 80`
allocation CALLS were made — the oracle catches that by reading the
allocator's own bookkeeping back, not by trusting the loop counter. Because
this harness's floor objects are never freed during the assert window, the
general "high-water count >= true live count" relationship (documented on
`SegmentTable::count` itself: "the number of LIVE (non-NULL) segments is
`self.bases().count()`", which can be `<=` the high-water mark once
recycling has occurred) collapses to EQUALITY here — nothing is ever
recycled before the assert fires, so `dbg_table_count()`'s value at the
oracle checkpoint IS the true live count, not merely an upper bound on it.

**A failed oracle panics loudly.** Both harnesses panic with an explicit
message naming the observed vs. required count if the floor under-fills —
never a silent short-scale measurement mislabeled as a `>=64-segment` one.

## 4. What FOUR prior findings this unblocks — see §1 above

(Kept as its own numbered section per the task's explicit instruction to
address OPEN_ITEMS item 34's four prior findings; the actual analysis lives
in §1 to avoid duplicating the table.)

## 5. How to run

### 5.1 The `iai-callgrind` bench (Linux + Valgrind only)

```text
cargo bench --bench macro_multiseg_steady_state --features "alloc-global bench-internals"
```

Same platform constraint as the existing per-commit gate
(`benches/perf_gate_iai.rs`): `iai-callgrind` requires Valgrind, which is
Linux-only. On Windows/macOS this target compiles to a no-op `fn main`
(confirmed in this task — see §6.1) and produces no `Estimated
Cycles`/RAM-hit numbers; it must be run on Linux CI or a Linux dev box to
get the cache-simulation numbers this harness exists to produce. **This is
the one part of this task's deliverable that could NOT be smoke-tested on
this Windows dev environment** — flagged honestly rather than assumed to
work; a future Linux-side task should run it once and record the first
real `Estimated Cycles`/RAM-hit baseline.

### 5.2 The wall-clock companion (any platform)

```text
cargo run --release --example r32_9_macro_multiseg_steady_state_ab_gate --features "production bench-internals"
```

(`bench-internals` is not strictly required by this example's own code —
it only needs `alloc-global` — but is included in the invocation above for
consistency with this project's usual measurement-build convention; the
`[[example]]` Cargo.toml entry itself only lists `alloc-global` as
`required-features`.)

## 6. Smoke test — proving the harness works (task instruction #4)

### 6.1 iai-callgrind bench: compiles clean on Windows (no-op path), Linux run not available

`cargo build --bench macro_multiseg_steady_state --features "alloc-global
bench-internals"` and `cargo build --bench macro_multiseg_steady_state
--features "production bench-internals"` both succeed with zero errors on
this Windows dev environment (the no-op `fn main` path — see
`benches/macro_multiseg_steady_state.rs`'s own module doc for why). `cargo
clippy --bench macro_multiseg_steady_state --features "production
bench-internals" -- -D warnings` and the `--all-features` variant are both
clean. **No Linux host was available in this task's environment**, so the
real `Estimated Cycles`/RAM-hit numbers this bench exists to produce were
NOT obtained this task — this is stated honestly, not glossed over, per
this backlog's own "measurement-first, honest-null-is-fine" posture (tasks
#497/#499's precedent). A future task running on Linux CI (or a Linux dev
box) is the natural next step to get the first real cache-simulation
baseline.

### 6.2 Wall-clock companion: full run, all headline numbers derived + asserted

`docs/perf/_raw_r32_9_smoke_test.log` is the raw stdout of one full run of
`examples/r32_9_macro_multiseg_steady_state_ab_gate` (release build, `
--features production`) on this Windows dev host — 2 thread-count arms x 5
repetitions each = 10 subprocess-isolated cells, oracle-checked and
config-conflict-checked on every cell.
`scripts/r32_9_derive_smoke_summary.mjs` parses that raw log's own
machine-readable CSV block, ASSERTS every headline claim below in-script
(per CLAUDE.md's R30-9 rule — a wrong number here would be a failing
`node` script, not a hand-transcription a reviewer has to independently
re-derive), and writes
`docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS_summary.csv`.

**Headline results** (derived + asserted by the script above, reproduced
here from its own stdout):

| threads | reps | min table count observed | oracle | config_conflicts_delta | ns/op median | ns/op range |
|---:|---:|---:|---|---:|---:|---|
| 1 | 5 | 81 | 5/5 PASS | 0 (5/5) | 47.7 | [42.3, 51.1] |
| 4 | 5 | 81 | 5/5 PASS | 0 (5/5) | 59.4 | [52.9, 95.3] |

- **Path-activation oracle: 10/10 rows PASS** — every single (thread_count,
  repetition) cell independently confirmed `dbg_table_count() >= 64` (actual
  observed value: 81, i.e. the full `FLOOR_LARGE_OBJECTS = 80` plus the
  primordial segment) BEFORE its timed churn began. This is the harness
  proving it actually achieved its own target working-set shape, not this
  report asserting it did.
- **Config-conflict identity: 10/10 rows `config_conflicts_delta = 0`** —
  each cell ran in its own freshly-spawned subprocess (an empty registry by
  construction), so cross-arm/cross-thread registry-slot bleed is
  structurally impossible; the counter confirms it, not merely assumes it.
- **Non-degenerate, sensible numbers.** ~42-51 ns/op at 1 thread vs. ~53-95
  ns/op at 4 threads is the expected direction and rough magnitude for
  contention overhead on a shared registry/table substrate under 4-way
  concurrency — not a suspicious 0, not an overflow-shaped huge number, not
  identical-across-arms (which would suggest the workload wasn't actually
  running through the labelled thread count). This is offered as evidence
  the harness produces sensible output, **not as a claim about any specific
  optimization** — no mechanism under test here, this is the raw
  steady-state cost of the current allocator at this working-set size,
  nothing more.

**What this smoke test does NOT claim.** This is not a benefit/regression
verdict on anything. No mechanism (X5/T10/R1/R15-1, or any other) was
re-attempted under this harness in this task — per the task's own explicit
scope note ("this task's own deliverable is the INFRASTRUCTURE, not a
specific optimization verdict"), §7 below is deliberately left for a future
task.

## 7. What this task deliberately did NOT do (left for follow-up tasks)

- **No real Linux `Estimated Cycles`/RAM-hit numbers were obtained** (§6.1)
  — this Windows dev environment has no Valgrind. A future task (or CI) run
  on Linux is needed for the actual cache-simulation payload this harness
  exists to produce.
- **No mechanism (X5/T10/R1/R15-1) was re-attempted.** This task built and
  smoke-tested the INSTRUMENT only, exactly as scoped
  ("#501 (`OWN_CACHE_SIZE`) is already a separate pending task in this
  backlog that will use this harness properly").
- **R15-1's fan-in axis (§1) is not addressed** by this harness as shipped
  — only its live-segment-count axis is. A future R15-1 revisit may need to
  extend the churn step across more size classes, or separately confirm the
  live-segment-count half alone is sufficient.
- **F1 (bitmap placement) and F2 (`OWN_CACHE_SIZE` thrashing)** — the survey
  names these as the most directly relevant beneficiaries; per the task's
  own "optional, don't rush" framing, no first look was taken at either in
  this task (the harness itself, per §6, was judged to be the correct place
  to spend this task's time, not a rushed extra experiment on top of it).
  #501 is the already-filed follow-up for F2 specifically.

## 8. Feature/config notes

- `HeapCore::dbg_table_count` is gated on `alloc-global` only (matching
  `AllocCore::dbg_table_count`, which carries no additional feature gate
  beyond being `#[doc(hidden)]`) — it is NOT `bench-internals`-gated, unlike
  most other new diagnostic hooks in this project's recent history, because
  it is a pure read-only observer with an existing safety precedent
  (`dbg_kind_at_tag`) already living at the same minimal gate level in the
  same file. See `tests/dbg_hook_safety_tripwire.rs`'s updated
  `PURE_OBSERVERS` allowlist entry.
- `production`'s feature composition is unchanged by this task (verified
  against `Cargo.toml`).
- Full `cargo test --features production` run clean (one known, pre-existing
  flaky test — `xthread_large_double_free_no_double_reclaim`, tracked as
  item 12 in `docs/CORRECTNESS_OPEN_ITEMS.md` under task #498, unrelated to
  this task — failed once on a full-suite run and passed on immediate
  re-run and in isolation; not caused by this task's changes).

## 9. Provenance

- Base commit: `a632dd4bcb2d12a5b083fbd60058678feb63005c`.
- Immutable source-identity tree SHA (`git write-tree` against the real
  index with exactly this task's changed/added files staged, computed
  before the derive script ran, per CLAUDE.md's R29-6 rule, option 2):
  `6ca05075b66dc0901134cca6de40888850621603`.
- Raw log: `docs/perf/_raw_r32_9_smoke_test.log` (full, not truncated).
- Summary CSV: `docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS_summary.csv`
  (derived by `scripts/r32_9_derive_smoke_summary.mjs`).
- CPU/OS: Windows 10 Pro 10.0.19045 (no CPU-model probe was run for this
  infrastructure-smoke-test task; the numbers in §6.2 are relative
  same-host, same-run comparisons, not a cross-host performance claim).
