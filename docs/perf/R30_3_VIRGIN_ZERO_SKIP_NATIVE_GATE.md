# R30-3 (task #452) — the activation-proven native wall-clock judge for `virgin-zero-skip`

Date: 2026-07-30. Supersedes the wall-clock portion of
`docs/perf/R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md` (task #447) — that
report's §3 (iai isolation, 3,067 vs 65,624 Ir, ~21.4×) **stays valid and is
NOT re-derived here**; only its §4 wall-clock bench is replaced. Source of
this task's design: `docs/reviews/2026-07-30-r29-followup-readonly-review.md`
§3 ("P0 — repair the `virgin-zero-skip` native judge"), an independent
review that traced the exact bug in
`benches/r29_16_virgin_zero_skip_calloc_wallclock.rs`'s `bench_virgin`: it
frees its whole batch inside the same `b.iter()` closure Criterion calls
thousands of times per sample, so from the second call onward every
`alloc_zeroed` pops a RECYCLED block off the free list instead of exercising
the bump-carve path `virgin-zero-skip` gates. Confirmed by this task by
tracing `alloc_small_with_virgin`'s dispatch order
(`src/alloc_core/alloc_core_small.rs:274-297`: step 1 checks the free list
first, step 3 is the bump-carve). That bench file's own source is
structurally unchanged since R29-16's follow-up commit `68e2019` (which
withdrew the conclusion, task marked correction-only, did not redesign the
harness) — confirmed by reading it fresh at the start of this task.

## 0. Verification-first framing (mandatory)

**The existing 3,067 vs 65,624 Ir (~21.4×) Callgrind result is valid
evidence that explicit zeroing WORK was skipped. It is explicitly NOT
evidence that native code runs 21.4× faster.** Callgrind's per-instruction
accounting of a bulk `REP`-prefixed `memset` does not model real hardware
memory-bandwidth/page-fault throughput. Every headline number in this report
is an independently measured native wall-clock time (`ns/op` from real
`std::time::Instant` deltas around real syscalls/memory operations on this
Windows dev host) — no Ir figure is cited, restated, or extrapolated as a
speed claim anywhere below.

## 1. The judge: design, source identity, reproduction

New harness: `benches/r30_3_virgin_zero_skip_native_gate.rs`
(`[[bench]] name = "r30_3_virgin_zero_skip_native_gate"`,
`required-features = ["alloc-global", "alloc-decommit", "alloc-stats"]` —
deliberately NOT `virgin-zero-skip` itself, so the SAME binary source
compiles both OFF (baseline) and ON (treatment), and NOT
`small-segment-lazy-commit` itself, so the same binary source also compiles
both under eager and lazy small-segment commit).

**Why a custom `Instant`-timing-loop `fn main()`, not Criterion.** The
matrix here is 4 sizes × 3 consumer behaviors × 2 scenarios per binary, and
every cell must print its own path-activation-oracle percentage next to its
own timing distribution. Criterion's `iter_batched` has no first-class
channel to report custom per-sample data (its `Bencher` closure's return
value becomes the NEXT untimed-teardown input, not extra reported data).
This project already has an established precedent for exactly this shape —
`benches/heap_fanin_persistent.rs`'s own module doc documents choosing a
plain `fn main()` + `Instant`-based timing loop over Criterion for the
identical underlying reason (a matrix too wide for Criterion's
per-`bench_function` summary model, needing custom per-cell data reported
alongside the timing); `benches/directory_threshold_probe.rs` is the same
pattern in miniature. This bench follows that established convention.

**Immutable source identity (CLAUDE.md's R29-6 rule).** Base commit:
`50d5adc9e99f7817f88097901d7d0497fae53ea3` (`main`, clean at task start
except this task's own new/modified files). This report, the new bench
file, and all `_raw_r30_3_*.log` files below land in the SAME commit as this
report — **commit that lands this report:** `d8f467b869c226150746532c484944958ee31808`
(this SHA was necessarily added in a small follow-up commit after the
landing commit itself, since a commit cannot cite its own SHA inside its
own tree — see that follow-up commit's message for the one-line
explanation, and `1272a52`/R30-6 for the established precedent for this
exact chicken-and-egg pattern): `git show
d8f467b869c226150746532c484944958ee31808:benches/r30_3_virgin_zero_skip_native_gate.rs`
recovers the exact harness that produced every number below, byte for byte.

**Reproduction** (four binaries — OFF/ON crossed with eager/lazy small-segment commit):

```
cargo bench --bench r30_3_virgin_zero_skip_native_gate --features "production bench-internals alloc-stats"
cargo bench --bench r30_3_virgin_zero_skip_native_gate --features "production bench-internals alloc-stats virgin-zero-skip"
cargo bench --bench r30_3_virgin_zero_skip_native_gate --features "production bench-internals alloc-stats small-segment-lazy-commit"
cargo bench --bench r30_3_virgin_zero_skip_native_gate --features "production bench-internals alloc-stats virgin-zero-skip small-segment-lazy-commit"
```

`numa-aware` is deliberately NOT included in any arm of this sweep — the
README's R29-15 Trap note documents that `numa-aware` silently bypasses
`small-segment-lazy-commit` entirely, which would make a "lazy-commit ON"
row secretly not be lazy at all. That is a different experiment than the one
this task asks for (eager vs lazy commit changing WHERE the zeroing saving
appears), so it is out of scope here by deliberate choice, not oversight.

Machine: Windows 10 Pro 10.0.19045, 11th Gen Intel Core i7-11800H @ 2.30GHz,
`rustc 1.97.0`, `cargo 1.97.0`. This is a shared dev host (other processes
active), consistent with R29-16's own stated caveat — see §4's noise
discussion.

## 2. The 8-point design and how each point is met

Source: the readonly review's §3, reproduced point by point.

1. **Feature OFF vs ON in separately built, immutable binaries.** Met — see
   §1's four `cargo bench` invocations; the same source file compiles to
   four distinct binaries via feature gating, never a runtime toggle.
2. **One-shot subprocesses, or `iter_batched` with a fresh heap/segment
   claimed in untimed setup.** Met, with a stronger primitive than
   `HeapRegistry::claim()`/`recycle()` would have given: every rep calls
   `AllocCore::new()` directly in untimed setup, a genuinely fresh OS
   primordial reservation with empty free lists every time.
   **`HeapRegistry::claim()`/`recycle()` was deliberately NOT used** —
   `HeapRegistry`'s slot lifecycle is first-claim-wins for a slot's whole
   process lifetime (`claim`'s re-claim of an already-materialised slot
   reuses the SAME `HeapCore`, segments and free lists included;
   `recycle` only flips the slot back to `FREE`, it does not tear the
   segments down — `src/registry/heap_registry.rs:118-181`). Using it here
   would have reproduced the R26-4 "same-process slot reuse" hazard
   (CLAUDE.md's config-sweep rule) one level up — a "fresh" claim on
   iteration 2+ could silently hand back a slot whose free list already
   holds prior iterations' blocks. `AllocCore::new()`'s `Drop` releases the
   OS reservation in untimed teardown, so no state survives across reps.
3. **Timed region allocates genuinely never-served blocks; does not free
   them until after the batch.** Met — see §3 below for the ONE deviation
   this point required (`VIRGIN_BATCH = 1`, not the originally planned 16).
4. **A path-activation oracle.** Met using an EXISTING counter, no new hook
   needed: `AllocCore::dbg_small_zero_pass_count()`
   (`src/alloc_core/alloc_core_core_diag.rs:516`, gated `alloc-stats`)
   already counts, process-wide, every Small `alloc_zeroed` call that took
   the explicit-`Node::zero` (non-virgin) path — bumped in exactly the
   branch `virgin-zero-skip` bypasses
   (`src/alloc_core/alloc_core.rs:1310-1319`,
   `src/registry/heap_core_alloc.rs:546-584`). Reading it before/after a
   batch gives the exact count of non-intended-path calls. **This oracle
   caught a real design bug during development of this very bench** — see
   §3.
5. **Sweep 4, 16, 64, 128 KiB.** Met — `SIZES` in the bench file. All four
   are confirmed Small-classified (`SegmentLayout::SMALL_MAX` ≈ 253 KiB
   under plain `production`, per R29-16's own confirmed geometry), so every
   arm exercises the `virgin-zero-skip`-gated Small `alloc_zeroed` path, not
   the separately-gated, always-on Large freshness skip.
6. **Three consumer behaviors.** Met — `Touch::None` (return without
   touching), `Touch::OneBytePerPage` (one `read_volatile` per 4 KiB page),
   `Touch::Full` (read+write every byte via `read_volatile`/`write_volatile`
   in a loop).
7. **Cross with `small-segment-lazy-commit`.** Met — §1's four invocations;
   results in §5.
8. **Paired process-level sampling; native time/distribution, not an IAI
   ratio.** Met — every cell reports mean/p50/min/max ns/op from real
   `Instant::now()` deltas across independent (fresh-heap) reps; no Ir
   number appears anywhere in this report or the bench source.

## 3. The ONE deviation from the original design, and why the oracle itself found it

The review's design sketch used a batch of several never-served blocks per
timed rep (mirroring R29-16's own `VIRGIN_BATCH = 16`). **A first
implementation attempt at exactly that (`VIRGIN_BATCH = 16`, one fresh
`AllocCore` per rep, all 16 calls timed, freed only after) was built, run,
and REJECTED by the oracle itself** — this is point 4 working exactly as
designed, catching a flaw before a false verdict could ship:

```
R30_3_ROW,virgin,4k,notouch,10,4714.4,4443.8,4068.8,6843.8,100.00,100.00,PASS   <- OFF binary (oracle vacuous, see below)
R30_3_ROW,virgin,4k,notouch,10,3975.6,3881.3,3225.0,5368.8,6.25,6.25,FAIL       <- ON binary: only 6.25% = 1/16 virgin
```

(Full ON-binary output from that rejected attempt reconstructed and preserved
at `docs/perf/_raw_r30_3_batch16_oracle_fail.log` — reconstructed from this
session's own tool-call history rather than a freshly re-captured run, since
the harness had already been corrected to `VIRGIN_BATCH = 1` by the time this
report was written; the file says so explicitly at its own header.)

Root cause, traced in source: `alloc_small_with_virgin`'s bump-carve path
(step 3) calls `carve_block_with_refill`
(`src/alloc_core/alloc_core_small.rs:346-376`), which carves the caller's
block AND ALSO proactively carves `REFILL_BATCH = 31` more blocks of the
SAME class and pushes every one onto the free list (Phase 9 amortisation,
unconditional — not gated on `virgin-zero-skip`, exists purely to amortise
carve overhead across a churn workload). The next 31 `alloc_zeroed` calls of
the same class all hit `alloc_small_with_virgin` step 1 (free-list pop) —
and ANY free-list pop is `is_virgin = false` by the dispatch conjunct
(`alloc_small_with_virgin`'s own doc,
`src/alloc_core/alloc_core_small.rs:255-263`), **even though those 31
refilled blocks are still OS-fresh/zero-filled and were never handed to any
caller or dirtied by anything**. So any `VIRGIN_BATCH > 1` on a single
class/heap structurally caps virgin-path activation at `1 / (1 + 31) ≈
3.1%` in steady state — far under `MIN_ACTIVATION_PCT = 95%` — regardless of
batch size.

**Fix:** `VIRGIN_BATCH = 1` (one genuinely virgin call per fresh
`AllocCore`, the same single-shot-process shape the existing
`perf_gate_iai.rs` IAI arms already use for their virgin arm).
`VIRGIN_REPS = 50` (vs. the recycled scenario's `REPS = 10`) compensates
with more independent samples, since each virgin rep's timed region is now
a single call rather than a 16-call batch. Re-run with this fix: **every
virgin cell on the ON binary reports 100.00% activation, 100.00% minimum
across all 50 reps, oracle PASS** (§5's tables).

**This finding is not just a bench-design footnote — it is itself relevant
to the promotion decision** (§6): it means `virgin-zero-skip`'s real-world
hit rate on ANY realistic same-class multi-block calloc burst (not just this
bench's artificial one-call-per-heap shape) is structurally diluted to
roughly 1-in-32 by the allocator's own unconditional refill-batch
amortisation, unless each call in the burst targets a genuinely different
class or a genuinely different segment. A single-call-per-heap workload
(this bench's virgin cells) is the ONLY shape that can exercise the skip on
(close to) every call; a same-class calloc loop — arguably the more
realistic "calloc-heavy" victim profile the feature's own design docs
target — will see it fire on roughly 3% of calls after the first, with the
rest paying the explicit-zero path regardless of the feature flag.

## 4. A structural, source-confirmed limitation of the oracle on the OFF binary

**When `virgin-zero-skip` is OFF, `SMALL_ZERO_PASS_CALLS` is never
incremented at all**, for either scenario — confirmed directly in source
(`src/alloc_core/alloc_core.rs:1305-1327`): the `#[cfg(not(feature =
"virgin-zero-skip"))]` arm always calls `Node::zero` unconditionally but
never touches the counter (the increment site lives entirely inside the
`#[cfg(feature = "virgin-zero-skip")]` branch, both in `AllocCore`'s own
`alloc_zeroed` and in `HeapCore`'s `alloc_zeroed` in
`src/registry/heap_core_alloc.rs`). This is expected, correct behavior (the
counter's whole purpose is to observe the skip's own dispatch, which does
not exist as a code path when the feature is compiled out) — but it means
**the path-activation oracle reports a vacuous 0.00%/PASS or 0.00%/FAIL on
every OFF-binary cell, and that FAIL/PASS marker carries no information
about the OFF binary's own correctness.** The OFF binary's cells are
included in the raw logs and summary CSV for completeness and for the
timing comparison in §5, but their `oracle` column should be read as
"not applicable" rather than a real pass/fail signal. This is stated
explicitly here rather than silently producing a misleading FAIL row on
every OFF/recycled cell (which is exactly what an unread CSV would suggest
is a genuine defect).

## 5. Headline results

### 5.1 Path-activation oracle — ON binary (the numbers that matter for point 4)

Eager commit (plain `production`, `virgin-zero-skip` ON), all 24 cells:

| Scenario | Sizes × touches | Min activation across all reps | Oracle |
|---|---|---:|---|
| virgin (`VIRGIN_BATCH=1`, `VIRGIN_REPS=50`) | 4k/16k/64k/128k × notouch/onebyte/full | **100.00%** (every cell, every rep) | **PASS** (12/12 cells) |
| recycled (`RECYCLED_BATCH=16`, `REPS=10`) | 4k/16k/64k/128k × notouch/onebyte/full | **100.00%** (every cell, every rep) | **PASS** (12/12 cells) |

Same result, byte-identical PASS/100% pattern, under `small-segment-lazy-commit`
ON (see `docs/perf/_raw_r30_3_on_lazy.log`). **All 48 ON-binary cells across
both commit policies pass the oracle at exactly 100% minimum activation —
the intended path dominates every single measured batch, not just on
average.** This is the evidence this task exists to produce: the new judge
proves it is actually exercising the mechanism it claims to measure, unlike
its predecessor.

Raw logs: `docs/perf/_raw_r30_3_on_eager.log`, `docs/perf/_raw_r30_3_on_lazy.log`
(+ `_raw_r30_3_on_eager_run2.log`, a repeat run used only for the noise-floor
discussion in §5.2, not double-counted as a fifth arm).

### 5.2 Native wall-clock: OFF vs ON, eager commit (representative size: 64 KiB)

| Scenario | Touch | OFF mean ns/op | ON mean ns/op | Δ | Run-to-run OFF spread (2 runs) |
|---|---|---:|---:|---:|---|
| virgin | notouch | 117,044 | 102,030 | −12.8% | 117,044 → 169,980 (+45%) |
| virgin | onebyte | 134,182 | 121,402 | −9.5% | 134,182 → 152,572 (+14%) |
| virgin | full | 187,798 | 166,532 | −11.3% | 187,798 → 205,210 (+9%) |
| recycled | notouch | 1,756.9 | 2,036.9 | +15.9% | 1,756.9 → 1,957.5 (+11%) |
| recycled | onebyte | 1,798.1 | 1,950.0 | +8.4% | 1,798.1 → 1,878.8 (+4%) |
| recycled | full | 33,521.2 | 50,260.0 | +49.9% | 33,521.2 → 46,904.4 (+40%) |

**The OFF binary's own repeated runs (identical binary, identical
invocation) already swing 4%–45% at this sample size on this shared dev
host** (right-most column; full raw data in
`docs/perf/_raw_r30_3_off_eager.log` vs
`docs/perf/_raw_r30_3_off_eager_run2.log`). The full 4/16/64/128 KiB × 3
touch × OFF/ON matrix (all 24 cells) is in
`docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE_summary.csv`; every cell's
OFF-vs-ON delta sign is inconsistent across sizes/touches (virgin ranges
−43.8% to +19.9%; recycled ranges +2.1% to +84.1%, ALWAYS in the "ON
slower" direction but by a widely varying and sometimes tiny absolute
amount — e.g. 66.2 ns → 121.9 ns absolute at 4k/onebyte, a 56 ns difference
easily inside this host's own measurement noise floor established by the
repeat-run column).

**Reading this honestly: at `VIRGIN_BATCH = 1`'s single-call timed region
(forced by §3's finding), this host's own measurement noise (tens of
microseconds of swing on `AllocCore::new()`-adjacent syscalls, page faults,
and OS scheduling on a shared dev machine) is comparable to or larger than
the actual skipped-memset cost the Ir isolation proved is real (a few
thousand instructions, translating to roughly hundreds of nanoseconds to
low microseconds of genuine memory-bandwidth-bound work at these sizes, not
tens of microseconds).** The virgin scenario's negative deltas (ON
appearing faster) are directionally consistent with "skipping work should
help," but are not distinguishable from this host's own run-to-run noise at
this sample size — exactly the same honest limitation R29-16's own §5
reported, now confirmed with a working oracle instead of a broken one.

**The recycled scenario's consistent-direction-but-noisy "ON slower"
pattern is a genuine, if small, finding worth naming plainly:** every
recycled cell (both commit policies) shows ON mean ns/op higher than OFF,
even though both configurations run the IDENTICAL explicit-`Node::zero`
work on the recycled path (the dispatch conjunct guarantees `is_virgin =
false` there regardless of the feature flag) — the ON binary additionally
pays `alloc_small_with_virgin`'s extra dispatch bookkeeping (the
`payload_virgin` bit read, the `(ptr, bool)` tuple return, and — under
`fastbin` builds — routing back through the magazine hit/miss machinery
`HeapCore::alloc_zeroed` uses per its own R13-3 doc) that the OFF binary's
plain `alloc_small` path does not. This is a small, structural dispatch
overhead the feature imposes on ITS OWN non-virgin path, not a Windows
noise artifact — it is directionally consistent across every one of 24
cells and both commit policies (48 cells total), which plain measurement
noise would not produce. In absolute terms it is small (tens to low
hundreds of ns at `notouch`/`onebyte`; the `full`-touch cells' larger
percentage deltas are dominated by page-fault/memory-bandwidth cost common
to both configurations, so the SAME small dispatch overhead becomes a
smaller relative fraction there even though the recorded percentage looks
larger due to run-to-run variance at that scale).

### 5.3 Lazy-commit crossing (point 7)

`small-segment-lazy-commit` ON changes WHERE first-touch page-commit cost
lands (deferred to actual write, vs. eager at reservation time), but does
**not** change the oracle-activation picture (§5.1: identical 100%/PASS
pattern) and does not change the qualitative wall-clock picture either —
OFF-vs-ON deltas under lazy commit
(`docs/perf/_raw_r30_3_off_lazy.log` vs `docs/perf/_raw_r30_3_on_lazy.log`,
full table in the summary CSV) show the same sign-inconsistent virgin
deltas and the same consistent-small-regression recycled pattern as eager
commit. This is a real, if negative, result for the "does lazy commit
reveal a cleaner win" hypothesis R29-16 §5 raised as an open next step:
**at this sample size/host, crossing with `small-segment-lazy-commit` does
NOT surface a cleaner separation.** `Touch::None` cells (which never fault
any page at all) are the ones most likely to isolate a pure
software-dispatch-only cost with neither eager nor lazy commit paying a
first-touch fault, and even those show the same noisy/inconsistent-sign
pattern for the virgin scenario.

## 6. Promotion decision

Per this task's promotion rule: only recommend adding `virgin-zero-skip` to
`production` if at least one realistic calloc-heavy workload wins
MATERIALLY on native wall-clock time AND no recycled/hot-churn workload
family regresses beyond this project's normal kill-gate threshold. This
project's established kill-gate convention (`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`
§4, `docs/perf/IAI_BASELINE.md`'s churn kill-gate precedent) uses an
explicit numeric regression bound scaled to the workload's own precision —
20% for a wall-clock realloc-phase judge, ±10 raw Ir for an
instruction-count churn judge. Applying the SAME discipline here on the
wall-clock axis this task measures:

- **No calloc-heavy workload shows a MATERIAL, noise-distinguishable native
  win.** The virgin scenario's OFF-vs-ON deltas range −43.8% to +19.9%
  across the swept matrix with an inconsistent sign, and this host's own
  same-binary repeat runs already swing 4%–45% at the SAME cells (§5.2) —
  the measured "win" is not distinguishable from measurement noise at this
  sample size. This does not mean the feature has zero effect (§0's Ir
  isolation proves real work IS skipped); it means this judge cannot
  demonstrate a MATERIAL wall-clock win at the confidence this report can
  honestly claim.
- **The recycled/hot-churn family DOES show a consistent-direction
  regression** — every one of 24 recycled cells (both commit policies, 48
  cells total) reports ON slower than OFF, small in absolute terms at
  `notouch`/`onebyte` sizes but directionally uniform enough (not
  noise-shaped) to be a real, if minor, dispatch-overhead cost the feature
  imposes on its own non-virgin path (§5.2's explanation). Whether this
  clears or fails a formal 20%-style kill-gate is ambiguous at small
  absolute magnitudes (a 56 ns absolute difference at 4k/onebyte is a
  "+84%" delta only because the baseline itself is ~66 ns) — but the
  DIRECTION is consistent and source-explainable, which is a stronger
  signal than the virgin scenario's sign-flipping noise.
- **§3's structural finding independently caps the feature's real-world
  applicability**: even in a genuinely calloc-heavy, never-served workload,
  `virgin-zero-skip` only fires on roughly 1-in-32 calls of the same class
  once the refill-batch amortisation kicks in (all but the very first call
  in any same-class burst hit the free list, not the bump-carve path) —
  this alone means "realistic calloc-heavy victim" is a much narrower
  target than "any workload calling `alloc_zeroed` a lot": it specifically
  needs EITHER a single call per class per heap lifetime, OR calls spread
  across many distinct classes/segments rather than a tight same-class
  burst.

**Verdict: NO-GO for `production` promotion at this time.** Neither
promotion condition is met — no calloc-heavy workload demonstrates a
material, noise-distinguishable wall-clock win at this sample size, and the
recycled family shows a small but consistent regression rather than a
neutral result. This is a measurement-confidence NO-GO, not a mechanism
NO-GO: §0/§3's Ir-level evidence and this report's 100%-activation oracle
both independently confirm the skip mechanism works exactly as designed
when it fires. The honest path forward, following this task's own accepted
outcomes list, is: **keep `virgin-zero-skip` opt-in, documented as a
narrow-profile feature** — genuinely useful only for a caller pattern of
"many distinct classes/segments each calloc'd once, never a same-class
burst" — rather than promoting it to `production`'s default composition or
closing it out entirely (the mechanism is real and cheap to keep available;
it is the BLANKET promotion, not the feature's existence, that this report
declines). A future round wanting a cleaner win would need either (a) a
much larger sample count / dedicated quiet host to shrink the noise floor
below the Ir-predicted effect size, or (b) a workload shape that avoids the
refill-batch dilution (§3) entirely, e.g. one call per class across many
classes in one batch instead of many calls of one class.

> **Dated correction (2026-07-30, Round 30 review response — see
> `docs/reviews/2026-07-30-r30-full-review.md` §4 P1-1).** This section's
> earlier text (immediately above, unchanged) states the recycled family
> shows a "direction-consistent regression ... 48/48 cells" with ON
> "ALWAYS" slower and a quoted range of `+2.1%` to `+84.1%`. Independently
> recomputed directly from this report's own committed
> `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE_summary.csv` (24
> OFF-vs-ON recycled cell pairs exist, not 48 — the "48" conflated the 24
> cells with the 2 commit policies swept, eager and lazy): **19 of 24
> cells are ON-slower on the mean (not 24/24), 20 of 24 on `p50`** (the
> `p50` exceptions are `eager/4k/full`, `eager/128k/onebyte`,
> `lazy/16k/full`, `lazy/64k/notouch`); the mean-delta exceptions (ON
> *faster*) are `eager/16k/notouch` (−8.6%), `lazy/128k/full` (−32.1%),
> `lazy/16k/full` (−4.7%), `lazy/16k/notouch` (−2.9%), and
> `lazy/64k/onebyte` (−1.1%). The quoted `+2.1%..+84.1%` range is the
> **eager-arm-only** min/max (`eager/4k/full` and `eager/4k/onebyte`); the
> full range across both commit policies is **−32.1% .. +136.8%**
> (`lazy/128k/full` at the low end, `lazy/4k/notouch` at the high end),
> which also makes "small" harder to sustain as a blanket descriptor for
> every cell, even though the absolute nanosecond gaps at the smallest
> sizes remain genuinely tiny (see the original text's own `notouch`/
> `onebyte` absolute-ns discussion, which is unaffected by this
> correction).
>
> Accordingly, "directionally consistent across every one of 24 cells and
> both commit policies (48 cells total), which plain measurement noise
> would not produce" should be read as: **a majority-direction trend
> (19/24 mean, 20/24 p50) consistent with, but not conclusively proven to
> be, extra dispatch bookkeeping on the feature's non-virgin path — 5/24
> cells (mean) show the opposite direction**, which is still compatible
> with a real small-magnitude effect swamped by noise in a minority of
> cells, but is weaker evidence for a purely structural, noise-immune
> cause than the original "ALWAYS"/"48/48" framing claimed.
>
> **This correction does not touch this report's other conclusions.** §6's
> NO-GO verdict stands unchanged: its primary, load-bearing justification
> is "no calloc-heavy workload demonstrates a material, noise-distinguishable
> wall-clock win" (the virgin-scenario sign-inconsistent null), which this
> finding does not affect. The recycled-family regression is now stated as
> a majority-direction (not unanimous) trend, which is if anything a MORE
> conservative reason to decline promotion (still no case for a win, and
> even the regression evidence is less clean than originally stated) — the
> verdict does not change in either direction.
>
> Numbers independently recomputed by the fixing task directly from the
> committed CSV (a Node one-liner grouping the 24 recycled cells by
> `commit_policy`+`size`+`touch` and comparing `virgin_zero_skip=false` vs
> `=true` rows), not copied from the review without checking.

## 7. CLAUDE.md compliance checklist

- Raw logs (`git add -f`'d): `docs/perf/_raw_r30_3_off_eager.log`,
  `docs/perf/_raw_r30_3_on_eager.log`, `docs/perf/_raw_r30_3_off_lazy.log`,
  `docs/perf/_raw_r30_3_on_lazy.log`,
  `docs/perf/_raw_r30_3_off_eager_run2.log`,
  `docs/perf/_raw_r30_3_on_eager_run2.log` (the two repeat runs cited only
  for the §5.2 noise-floor discussion), and
  `docs/perf/_raw_r30_3_batch16_oracle_fail.log` (§3's rejected-design
  evidence — explicitly labeled at its own header as reconstructed from this
  session's tool-call history, not a freshly re-captured run, since the
  harness had already moved on to `VIRGIN_BATCH = 1` by report-writing time).
- Summary CSV: `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE_summary.csv`
  (commit SHA, feature set, CPU/OS, per-arm sample counts, path-activation
  oracle pass/fail, and the headline ns/op figures per sweep arm).
- Immutable source identity: §1 (base commit
  `50d5adc9e99f7817f88097901d7d0497fae53ea3`; this report's own landing
  commit, `d8f467b869c226150746532c484944958ee31808`, is the permanent
  reference — filled in by a same-day follow-up commit per the
  chicken-and-egg pattern §1 explains, mirroring `1272a52`'s established
  R30-6 precedent).
- Fast-by-default: `VIRGIN_REPS=50`/`REPS=10` reps × the 4×3×2 matrix
  completes in low single digit seconds per binary (see raw logs' own
  timestamps) — no `sample_size`/criterion warm-up applicable since this is
  a custom timing loop, but the total measured wall-clock per invocation is
  consistent with this project's "short scenario by default" rule. Sample
  counts were WIDENED for the virgin scenario specifically (50 vs the
  originally planned matching-recycled 10) because `VIRGIN_BATCH=1`'s
  smaller per-rep timed region needed more independent samples for a stable
  mean under this fast profile — noted explicitly per CLAUDE.md's own
  guidance to flag such widenings.
