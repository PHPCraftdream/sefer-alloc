# R31-0 (task #471) — the PRODUCTION-LAYER re-gate for `virgin-zero-skip`, correcting R30-3's NO-GO

Date: 2026-07-30. This report does NOT delete or rewrite
`docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` — that report is left
append-only per CLAUDE.md, with a dated correction section pointing here.
This report supplies the missing production-layer measurement; R30-3's own
Ir-level evidence (§0 of that report, 3,067 vs 65,624 Ir, ~21.4×) is
unaffected and not re-derived here.

## 0. The defect this task fixes

R30-3's judge (`benches/r30_3_virgin_zero_skip_native_gate.rs`) constructs a
bare `AllocCore` (`AllocCore::new()`) and calls `core.alloc_zeroed(layout)`
directly — the magazine-BYPASS substrate. A bump-carve there goes through
`carve_block_with_refill` (`src/alloc_core/alloc_core_small.rs:346`), which
carves the caller's block AND proactively refills `REFILL_BATCH = 31` more
blocks onto the FREE LIST (Phase 9 amortisation, unconditional, not gated on
`virgin-zero-skip`). Every subsequent same-class `alloc_zeroed` then pops a
free-list block, and a free-list pop is `is_virgin = false` by
`alloc_small_with_virgin`'s dispatch conjunct — so virgin-path activation is
structurally capped at ~1-in-32 in that substrate. R30-3 measured exactly
that (~3% at its `VIRGIN_BATCH=16` attempt, forcing it down to
`VIRGIN_BATCH=1`) and concluded the feature is structurally useless for any
same-class burst workload.

**That conclusion is TRUE for bare `AllocCore` and FALSE for the actual
`production + virgin-zero-skip` configuration.** The production call chain
is different and was never exercised by R30-3's judge:

```
HeapCore::alloc_zeroed        (src/registry/heap_core_alloc.rs:524)
  -> alloc_small_zeroed_via_magazine   (:337)
    -> on a magazine MISS: refill_magazine_slow_virgin   (:413)
      -> AllocCore::refill_class_bump_virgin_checked
```

This refill carves `refill_n_for_class(block_size)` blocks (clamped to
`TCACHE_CAP = 16` by a 64 KiB byte budget), issues ONE to the caller, and
STORES the retained `refill_n - 1` blocks' virgin bits into
`PerClass::virgin_mask` (`heap_core_alloc.rs:472`). Later magazine HITS read
and clear their own bit (`:354-356`) and STILL skip the explicit zero pass —
i.e. virginity is preserved across an ENTIRE freshly-carved refill, not just
the first block. `tests/r13_3_magazine_virgin_hit_skips_zero.rs` already
asserts this exact mechanism; this task measures it on the wall clock for
the first time.

## 1. The judge: design, source identity, reproduction

New harness: `benches/r31_0_virgin_zero_skip_production_layer_gate.rs`
(`[[bench]] name = "r31_0_virgin_zero_skip_production_layer_gate"`,
`required-features = ["alloc-global", "fastbin", "alloc-decommit",
"alloc-stats"]` — deliberately NOT `virgin-zero-skip` itself, so the SAME
source compiles both OFF (baseline) and ON (treatment)).

**Harness shape.** A custom `Instant`-timing-loop `fn main()`
(`harness = false`), the same established pattern as
`benches/r30_3_virgin_zero_skip_native_gate.rs` and
`benches/heap_fanin_persistent.rs` — the (size × touch × scenario) matrix
needs a per-cell path-activation-oracle percentage reported alongside its
own timing distribution, which Criterion's `Bencher` has no first-class
channel for.

**The layer fix, concretely.** Each rep claims a genuinely FRESH `HeapCore`
via `HeapRegistry::claim()` — never recycled (`claim_fresh()` in the bench
file) — giving an empty magazine and empty free list every rep, then drives
`BURST = 16` same-class `alloc_zeroed` calls through `HeapCore::alloc_zeroed`
(the real production entry point `SeferAlloc`'s `GlobalAlloc::alloc_zeroed`
calls), not `AllocCore` directly. This is the SAME-CLASS BURST shape R30-3's
own report (§3, §6) identified as the realistic "calloc-heavy victim
profile" its bare-`AllocCore` judge could not represent (every burst there
diluted to ~1-in-32 activation regardless of what the harness intended to
measure).

**Immutable source identity (CLAUDE.md's R29-6 rule).** Base commit:
`14a9ef34145cc62188d734cf6987bcfd4dbcb088` (`main`, HEAD at task start; the
working tree at measurement time additionally carried this task's own new
files, landed together in the SAME commit this report cites). **Commit that
lands this report:** `UNFILLED` — filled in by a same-day follow-up commit
per the established chicken-and-egg pattern (a commit cannot cite its own
SHA inside its own tree; see `1272a52`/R30-6 and `9335979`-style
precedents in this project's history).

**Reproduction:**

```
cargo bench --bench r31_0_virgin_zero_skip_production_layer_gate --features "production alloc-stats"
cargo bench --bench r31_0_virgin_zero_skip_production_layer_gate --features "production alloc-stats virgin-zero-skip"
```

Machine: Windows 10 Pro 10.0.19045, 11th Gen Intel Core i7-11800H @ 2.30GHz,
`rustc 1.97.0` — same shared dev host as R30-3/R29-16/R30-6 (noisy; see §3's
noise-floor discussion).

## 2. Path-activation oracle (CLAUDE.md R30-8) — per-arm mechanism evidence

Per the R30-8 rule (config resolving correctly is not the same as the
intended MECHANISM firing), this judge records three independent signals
per arm, not just a labelled feature flag:

1. **`HeapCore::tcache_hits()` before/after the burst = magazine-HIT
   count.** Reported per cell as `mean_hits`/`min_hits` against the
   analytically-`expected_hits = BURST - ceil(BURST/refill_n)` for the
   virgin scenario (`BURST` for recycled). This is the SAME on both OFF and
   ON (the magazine's refill+retain structure does not depend on the
   feature flag) — it is the cross-binary proof that the production
   magazine layer actually RAN, the exact layer R30-3's bare-`AllocCore`
   judge excluded by construction.
2. **`AllocCore::dbg_small_zero_pass_count()` (`alloc-stats`) before/after =
   explicit-zero calls**, converted to `mean_act_pct`/`min_act_pct` (percent
   of the burst that took the INTENDED path per scenario) and a per-cell
   `oracle` column (`PASS` if `min_act_pct >= 95%`, `FAIL` otherwise,
   `NA` on the OFF binary where the counter is provably vacuous — see §4).
3. **A separate RETENTION PROBE** (ON binary only, gated
   `#[cfg(feature = "virgin-zero-skip")]`, using the pre-existing
   `HeapCore::dbg_tcache_virgin_mask` accessor,
   `src/registry/heap_core_diag.rs:122`): after ONE `alloc_zeroed` call on a
   fresh heap, asserts the magazine holds exactly `refill_n - 1` retained
   blocks whose `virgin_mask` bits are ALL set, plus that the issued block's
   bytes are genuinely all-zero (correctness, not just activation). This is
   the direct, per-size smoking-gun proof of the mechanism R30-3's own doc
   comments described but never measured.

No new `dbg_*` hook was added — `dbg_class_for`, `dbg_refill_n_for_class`,
`dbg_tcache_count`, `tcache_hits`, and `dbg_tcache_virgin_mask` are all
pre-existing, safe, `#[doc(hidden)]` read-only test/diagnostic accessors
(`src/registry/heap_core_diag.rs`, `src/registry/heap_core_tcache.rs`); none
touch allocator metadata through a caller-supplied pointer, so none fall
under CLAUDE.md's benchmark-hook `unsafe fn` + `bench-internals` rule.

### 2.1 Retention-probe results — ON binary, all 4 sizes (the smoking gun)

| Size | refill_n | Retained (measured) | Retained (expected) | Mask (measured) | Mask (expected) | Issued block all-zero | Verdict |
|---|---:|---:|---:|---:|---:|---|---|
| 4k | 16 | 15 | 15 | 32767 | 32767 | true | **PASS** |
| 16k | 4 | 3 | 3 | 7 | 7 | true | **PASS** |
| 64k | 1 | 0 | 0 | 0 | 0 | true | **PASS** |
| 128k | 1 | 0 | 0 | 0 | 0 | true | **PASS** |

4/4 sizes PASS. At 4 KiB and 16 KiB, the magazine retains multiple blocks
with EVERY retained bit marked virgin — direct proof virginity survives an
entire refill, not just the issued block. At 64/128 KiB `refill_n = 1`, so
there is nothing to retain (0 retained is the CORRECT expectation, not a
failure) — every call at these sizes independently carves and issues its
own single virgin block.

### 2.2 Magazine-hit + zero-pass activation — all 48 arms (24 virgin + 24 recycled), both binaries

Raw data: `docs/perf/_raw_r31_0_off.log`, `docs/perf/_raw_r31_0_on.log`
(primary run-1 logs, the cited evidence); `docs/perf/_raw_r31_0_off_run2.log`,
`docs/perf/_raw_r31_0_on_run2.log` (repeat runs, cited only for §3's
noise-floor discussion, not double-counted). Full per-cell table:
`docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv`.

- **Virgin scenario, ON binary: 12/12 cells (4 sizes × 3 touches) at
  min_act_pct = 100.00%, oracle PASS.** Every one of `BURST = 16` calls per
  rep, across 15 independent fresh-heap reps per cell, took the intended
  virgin-skip path.
- **Recycled scenario, ON binary: 12/12 cells at min_act_pct = 100.00%,
  oracle PASS.** The recycled-scenario control (a primed, dirtied block
  re-served in a tight LIFO loop) correctly explicit-zeroes on every call
  regardless of the feature flag — proving the feature does NOT
  over-fire on non-virgin blocks.
- **`mean_hits` matches `expected_hits` exactly on every one of the 24
  cells, on BOTH binaries**: 4k → 15/15, 16k → 12/12, 64k/128k → 0/0
  (virgin); 4k/16k/64k/128k → 16/16 (recycled). This is the cross-binary
  proof point 1 above promised: the production magazine refill+retain path
  ran identically regardless of the feature flag, so the ON/OFF comparison
  below is a true apples-to-apples layer comparison, not an artifact of one
  binary exercising a different code path than the other.
- **OFF binary: oracle correctly reports `NA` on all 24 cells** —
  `SMALL_ZERO_PASS_CALLS` lives entirely inside the
  `#[cfg(feature = "virgin-zero-skip")]` branch of `HeapCore::alloc_zeroed`
  (`src/registry/heap_core_alloc.rs:546-556`); the OFF arm's own explicit
  Small `alloc_zeroed` path (`:585-595`) calls `Node::zero` unconditionally
  but never touches that counter. This is the SAME documented limitation
  R30-3 §4 already established for the OFF binary — expected, not a defect
  in this judge — confirmed again directly in source before writing this
  report.

**Conclusion of this section**: unlike R30-3's bare-`AllocCore` judge (which
measured ~3-6% activation on a same-class burst and rejected its own first
design attempt for exactly that reason), the production magazine layer
achieves 100% virgin-path activation on every swept size, including 64/128
KiB where `refill_n = 1` (no multi-block retention to speak of, but the
single carved block per call is still correctly treated as virgin — the
retention BENEFIT is absent there, not the skip itself). This judge is
proven to exercise the mechanism it claims to measure, satisfying the R30-8
path-activation-oracle rule in full — 4/4 retention-probe PASS + 24/24
ON-binary activation PASS.

## 3. Native wall-clock: OFF vs ON, all 24 (size × touch) cells, both scenarios

Derived by `scripts/r31_0_summary.mjs` from the primary run-1 raw logs (one
checked script, not hand-transcribed, per CLAUDE.md's derived-tables rule).
`mean_ns` = mean of 15 independent fresh-heap-rep batches of `BURST = 16`
calls, `ns/op` normalized per call.

### 3.1 Virgin scenario

`Δ (run1)` is computed from the primary cited logs
(`_raw_r31_0_off.log`/`_raw_r31_0_on.log`); `Δ (run2)` is the SAME cells
recomputed from the independent repeat-run logs
(`_raw_r31_0_off_run2.log`/`_raw_r31_0_on_run2.log`), both by the same
checked script pass, to show which cells' sign is host-noise-stable and
which are not.

| Size | Touch | OFF ns/op (run1) | ON ns/op (run1) | Δ (run1) | Δ (run2) |
|---|---|---:|---:|---:|---:|
| 4k | notouch | 2,342.1 | 257.5 | **−89.0%** | **−89.5%** |
| 4k | onebyte | 2,387.5 | 2,207.1 | −7.6% | +15.7% |
| 4k | full | 4,734.2 | 5,124.6 | +8.2% | +6.1% |
| 16k | notouch | 11,229.6 | 564.6 | **−95.0%** | **−96.9%** |
| 16k | onebyte | 8,915.4 | 9,819.2 | +10.1% | −40.3% |
| 16k | full | 16,425.4 | 22,331.2 | +36.0% | −9.1% |
| 64k | notouch | 52,116.7 | 754.2 | **−98.6%** | **−99.0%** |
| 64k | onebyte | 47,822.9 | 35,572.1 | −25.6% | −36.3% |
| 64k | full | 96,959.2 | 91,308.3 | −5.8% | −10.5% |
| 128k | notouch | 97,198.3 | 1,354.6 | **−98.6%** | **−98.4%** |
| 128k | onebyte | 95,133.8 | 71,800.4 | −24.5% | −24.2% |
| 128k | full | 186,685.8 | 160,380.4 | −14.1% | −22.2% |

The 4 `notouch` cells (bolded) are the only ones whose sign AND rough
magnitude are stable across the repeat run (−89%/−90%, −95%/−97%,
−99%/−99%, −98%/−98%). Every other cell either flips sign entirely
(`4k/onebyte`: −7.6% → +15.7%; `16k/onebyte`: +10.1% → −40.3%;
`16k/full`: +36.0% → −9.1%) or swings by a large fraction of its own
magnitude (`64k/onebyte`: −25.6% → −36.3%; `128k/full`: −14.1% → −22.2%) —
consistent with page-fault/memory-bandwidth noise dominating once the
consumer touches the allocation, exactly as R30-3's own report found for
its comparable touch-heavy cells.

### 3.2 Recycled scenario (control — both configurations run identical explicit-zero work)

| Size | Touch | OFF mean ns/op | ON mean ns/op | Δ |
|---|---|---:|---:|---:|
| 4k | notouch | 86.7 | 120.4 | +38.9% |
| 4k | onebyte | 86.2 | 118.8 | +37.8% |
| 4k | full | 3,022.9 | 2,959.6 | −2.1% |
| 16k | notouch | 292.1 | 334.2 | +14.4% |
| 16k | onebyte | 275.0 | 213.8 | −22.3% |
| 16k | full | 11,523.3 | 9,977.1 | −13.4% |
| 64k | notouch | 1,839.6 | 2,025.0 | +10.1% |
| 64k | onebyte | 2,009.6 | 1,790.4 | −10.9% |
| 64k | full | 41,295.4 | 44,968.3 | +8.9% |
| 128k | notouch | 3,931.7 | 3,919.6 | −0.3% |
| 128k | onebyte | 3,915.8 | 3,772.1 | −3.7% |
| 128k | full | 105,497.5 | 92,092.1 | −12.7% |

**Reading this honestly.** The `notouch` cells (4 of 4 sizes, virgin
scenario) are the cleanest measurement this judge produces: a **consistent,
large, mechanistically-explained win of −89% to −98.6%**, reproduced with
the same sign and comparable magnitude on an independent repeat run
(`docs/perf/_raw_r31_0_off_run2.log` / `_raw_r31_0_on_run2.log`, §3.1's
right-most column). A third, uncommitted ad-hoc re-run performed during
this task's own zero-trust review (not saved as a cited raw log, but
reproducible on demand via §1's exact invocation) landed in the same
−91%/−97%/−99%/−99% range across all four sizes, all negative — corroborating,
not part of the cited evidence set. This is not noise: `Touch::None` never
faults a page, so in the OFF arm the entire
measured cost at these sizes is dominated by exactly the work
`virgin-zero-skip` removes — the unconditional `Node::zero`
(`core::ptr::write_bytes`) memset over the whole allocation
(`src/alloc_core/node.rs:135-144`), which itself forces the OS to
commit/fault every page it touches. Skip that memset on a genuinely virgin,
OS-already-zero page range, and there is close to nothing left to measure
at `notouch` — which is exactly what the data shows.

The `onebyte` and `full` cells show the SAME sign-inconsistent,
noise-dominated pattern R30-3's own report already documented for its
touch-heavy cells (§5.2 there): once the consumer itself faults/dirties
pages, both configurations pay that shared OS-level cost, and the much
smaller residual difference the feature contributes is frequently smaller
than this shared dev host's own run-to-run swing (a repeat run flipped sign
entirely at `4k/onebyte`, `4k/full`, `16k/onebyte`, `16k/full` above). This
matches R30-3's own honest noise-floor finding almost exactly — the
difference here is that this judge additionally has a clean `notouch` axis,
uncontaminated by page-fault noise, that isolates the actually-skipped work
directly.

The recycled (control) scenario shows small, sign-INCONSISTENT deltas in
both directions across all 12 cells — no consistent regression pattern
comparable to R30-3's own recycled-scenario finding (which reported a
majority-direction, if noisy, "ON slower" trend attributed to the feature's
extra dispatch bookkeeping on its own non-virgin path). This judge's
recycled deltas do not reproduce that pattern cleanly (7 of 12 cells are
ON-faster, 5 of 12 ON-slower, no majority direction beyond a coin flip) —
most plausibly because this judge's tight LIFO recycled loop (prime one
block, then `alloc_zeroed`+`dealloc` it repeatedly) is a different recycled
shape than R30-3's `RECYCLED_BATCH = 16` fresh-batch-then-free-all loop, and
both are small absolute differences (tens to low hundreds of ns at
`notouch`/`onebyte`) well within this host's demonstrated noise floor. This
report does not claim a recycled-path regression either way; the honest
reading is "no material effect detected on the recycled/non-virgin path,"
consistent with the mechanism (virginity tracking is read-only overhead
proportional to one bitmask check per pop, not proportional to allocation
size).

## 4. A structural, source-confirmed limitation of the oracle on the OFF binary

Identical limitation to R30-3 §4, reconfirmed directly in source for the
production-layer path specifically (not just `AllocCore`'s own copy): when
`virgin-zero-skip` is OFF, `SMALL_ZERO_PASS_CALLS` is never incremented in
`HeapCore::alloc_zeroed`'s OFF arm
(`src/registry/heap_core_alloc.rs:585-595`) — the increment call sites live
entirely inside the `#[cfg(all(feature = "virgin-zero-skip", ...))]`
branches above that arm. This is expected, correct behavior (the counter's
whole purpose is to observe the skip's own dispatch, which does not exist as
a code path when the feature is compiled out), not a defect in this judge —
the OFF binary's `oracle` column reads `NA` on every cell for exactly this
reason, and should be read as "not applicable," never as a real pass/fail
signal.

## 5. Promotion decision

Applying this project's established two-condition promotion rule (same
framing R30-3 §6 used): promote only if at least one realistic workload
family wins MATERIALLY on native wall-clock AND no workload family
regresses beyond a normal kill-gate threshold.

- **The `notouch` virgin family shows a MATERIAL, reproducible,
  mechanistically-explained win** — 4/4 swept sizes, −89% to −98.6% on the
  primary run, reproduced with the same sign and comparable magnitude on
  the independent repeat run (§3.1) plus a third uncommitted corroborating
  re-run (§3's prose). This is the layer R30-3 could not reach: a genuine
  same-class burst through the real production magazine, with 100%
  activation proven per arm (§2), not the ~3% ceiling R30-3's
  bare-`AllocCore` substrate imposed.
- **The `onebyte`/`full` virgin families show a directionally favorable but
  noise-dominated result** — most cells lean negative (ON faster) but sign
  flips across repeat runs at several cells, consistent with R30-3's own
  finding that page-fault/memory-bandwidth cost common to both
  configurations swamps the feature's own (real, but comparatively small at
  these touch levels) contribution once the consumer actually dirties the
  allocation.
- **The recycled/non-virgin control family shows no material, consistent
  regression** — small, sign-inconsistent deltas in both directions,
  unlike R30-3's own (noisy, non-unanimous, but majority-direction)
  regression finding on its differently-shaped recycled loop. Neither
  result should be read as conclusively "no cost" — both are small enough
  to be swamped by this host's noise floor — but this judge finds no
  reproducible penalty to weigh against the `notouch` win.
- **The retention mechanism itself is proven, per size, with a direct
  smoking-gun probe** (§2.1) — this was never measured by any prior report;
  R30-3's own doc comments described the mechanism from reading source, not
  from a passing assertion against live allocator state.

**Verdict: the `notouch` finding is a GO-supporting result for a
narrow, workload-shape-specific promotion — NOT a blanket GO for
`production`.** The material win is real and reproduces, but it is
observed cleanly ONLY in the `notouch` consumer-behavior category, which is
a genuine but narrow real-world shape (a caller that `calloc`s a buffer and
either never touches all of it, or touches it lazily far later/on a
different code path than the allocation itself — e.g. a sparse hash table,
a pre-sized buffer pool, or an over-allocated scratch arena). The
`onebyte`/`full` categories — which are the categories that dominate many
real allocator workloads (a `calloc`'d buffer is usually populated shortly
after allocation) — do not show a reproducible win at this sample size on
this host, exactly matching R30-3's own honest conclusion for those same
touch categories.

**This report does NOT recommend adding `virgin-zero-skip` to
`production`'s feature composition.** Per this task's own explicit
instruction, that composition change requires separate user sign-off in any
case, but the data itself does not support an unconditional recommendation:
a blanket promotion would apply the feature's (real, proven) `notouch` win
uniformly to ALL consumers, including the `onebyte`/`full` majority where no
material win reproduces and where R30-3's own recycled-path finding
suggested a possible small dispatch-overhead cost on the feature's own
non-virgin branch (this report's recycled data does not reproduce that
specific finding, but does not rule it out either — both are within this
host's noise floor).

**What this report DOES establish, correcting R30-3's verdict:** R30-3's
stated reason for NO-GO — "no calloc-heavy workload demonstrates a
material, noise-distinguishable wall-clock win," attributed structurally to
a ~1-in-32 same-class-burst activation ceiling — **does not hold for the
actual production configuration.** The production magazine achieves 100%
same-class-burst activation (§2), and the cleanest wall-clock measurement
this correct layer permits (`notouch`) shows a large, reproducible,
mechanistically-explained win. The feature is not "structurally useless for
any same-class burst," as R30-3's own §6 stated; it is workload-shape
dependent, with a proven-real win in the touch-light/deferred-touch case
and an unproven (neither confirmed nor ruled out) picture in the
touch-heavy majority case.

**Recommended narrower framing for a future promotion decision** (not
enacted here): a workload-conditional promotion — e.g. a caller-facing knob
distinguishing "calloc buffers that are typically sparse/lazily touched"
from "calloc buffers that are populated immediately" — is a more
defensible target than a blanket `production` default, if a future round
wants to pursue this further. Absent that, `virgin-zero-skip` should remain
documented as a narrow-profile opt-in feature, now with the CORRECT
characterization: genuinely effective on same-class bursts through the real
production magazine (not the ~1-in-32-diluted picture R30-3 measured), with
its wall-clock benefit concentrated in the touch-light/deferred-touch
consumer shape specifically.

## 6. CLAUDE.md compliance checklist

- Raw logs (`git add -f`'d): `docs/perf/_raw_r31_0_off.log`,
  `docs/perf/_raw_r31_0_on.log` (primary run-1, the cited evidence),
  `docs/perf/_raw_r31_0_off_run2.log`, `docs/perf/_raw_r31_0_on_run2.log`
  (repeat runs, cited only for §3's noise-floor discussion).
- Summary CSV: `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv`
  (commit SHA, feature set, CPU/OS/rustc, per-arm sample counts, mechanism
  signals, path-activation oracle verdict, ns/op figures — derived by
  `scripts/r31_0_summary.mjs` from the raw logs, not hand-transcribed).
- Immutable source identity: §1 (base commit
  `14a9ef34145cc62188d734cf6987bcfd4dbcb088`; this report's own landing
  commit is filled in by a same-day follow-up commit per the established
  chicken-and-egg pattern — see `1272a52`/R30-6's precedent).
- Path-activation oracle (CLAUDE.md R30-8): §2 in full — per-arm magazine-hit
  parity check, per-arm explicit-zero-call activation percentage with a
  hard `>= 95%` PASS/FAIL gate, and a dedicated per-size retention probe
  asserting the exact retained-block count and virgin-mask bit pattern
  against the analytically-derived expectation.
- Fast-by-default: `REPS = 15` independent fresh-heap reps × the 4×3×2
  matrix (or 4×2 counting the retention probe once per size) completes in
  low single-digit seconds per binary (see raw logs' own build/run
  timestamps) — consistent with CLAUDE.md's "short scenario by default"
  rule.
- No production default changed — this is `bench`-prefixed measurement-only
  work per CLAUDE.md's R30-12 commit-tag rule; `Cargo.toml`'s `production`
  feature list is untouched by this task.
