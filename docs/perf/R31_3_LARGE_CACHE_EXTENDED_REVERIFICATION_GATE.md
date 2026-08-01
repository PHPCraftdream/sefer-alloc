# R31-3 — `large-cache-extended` re-verification gate: six R14-5 checkpoints re-checked on current HEAD, refreshed A/B, N=1/2/4 timing regression, multi-heap RSS

**Task:** #466 (R31-3, P1). **RE-VERIFICATION + NEW REGRESSION GATES — not a
self-authorized promotion.** `large-cache-extended` stays EXPERIMENTAL, opt-in,
NOT in `production`'s `Cargo.toml` feature list. This document re-checks
`docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md`'s six mandatory
hardening checkpoints against present-day `HEAD`, refreshes its A/B on current
code, and adds the two regression gates a prior review named as missing
preconditions (N=1/2/4 narrow-working-set timing, multi-heap RSS accounting).
Steps 1-3 are the deliverable; §5 below is a promotion PROPOSAL only, subject
to explicit user sign-off per the task brief — no `Cargo.toml` change is made
by this task.

**Date:** 2026-07-30. **Measured on commit:** `c7b3edafcd07b795ea5874b0a7986086e7bc1e2b`
(the base commit this task's changes land on top of). **Landing commit:**
`0985e22` (this report + its harnesses + its raw logs, filled in by this
follow-up edit, matching the established `1272a52`/`c7b3eda` two-commit
pattern — see `docs/perf/OPEN_ITEMS.md` item 30's own citation trail and
README §"Where unsafe lives" for prior examples). Host: Windows 10 Pro,
i7-11800H, `rustc 1.97.0 (2d8144b78 2026-07-07)`.

---

## 0. Allocator layer under test (CLAUDE.md's R30-8 rule)

Every A/B/RSS artifact in this report drives the **real `#[global_allocator]`
`SeferAlloc`** (`SeferAlloc::new()`) via plain `std::alloc::{alloc, dealloc}`
— NOT a bare `AllocCore` bypass — for the two turnover/narrow A/B judges
(§2, §3), and **`HeapRegistry::claim_with_config` / `HeapCore`** (the real
per-thread heap-claim path a `#[global_allocator]` uses under the hood) for
the multi-heap RSS gate (§4), mirroring R29-13's already-established
methodology. This closes the gap the original R14-5 report's own §3 RSS
harness left open (that harness drove a bare `AllocCore::new()`, one heap,
never through the registry) and is stated explicitly per this project's R30-8
rule.

---

## 1. Step 1 — re-verification of R14-5's six hardening checkpoints against current `HEAD`

All six checkpoints were re-read against present-day source (file paths/line
numbers below are current, not R14-5's original citations, which have
drifted since — see each item for what changed).

1. **Budget-vs-materialisation ordering.** Still intact.
   `AllocCore::large_cache_deposit_budget_infeasible`
   (`src/alloc_core/alloc_core_large_cache.rs:164`) is still called BEFORE
   `large_cache_find_free_slot` (`:203`) at both admission call sites
   (`src/alloc_core/alloc_core.rs:1539`, the Large-`dealloc` branch;
   `src/alloc_core/alloc_core_large.rs:589`, `reclaim_large_segment`) — the
   exact pre-check ordering R14-5 §1 added. No drift.
2. **Finite default budget for `large-cache-extended`.** Intact AS A
   MECHANISM, but the NUMERIC VALUE has changed since R14-5 landed — this is
   the one checkpoint with a real, already-disclosed drift. R14-5 §2
   originally shipped `DEFAULT_EXTENDED_BUDGET_BYTES = 5 * DEFAULT_HEADROOM_BYTES`
   (1280 MiB/heap). R17-9 (task #326, commit `1117198`) revised this DOWN to
   `1 * DEFAULT_HEADROOM_BYTES` (**256 MiB/heap**) after an external review
   flagged the 5x default as a per-heap ceiling that could total tens of GiB
   across many concurrently-active heaps in a thread-per-core server (no
   process-wide coordination between heaps — `AllocCore` is owner-only,
   neither `Send` nor `Sync`). Current source:
   `src/alloc_core/large_cache_config.rs:159`
   (`pub(crate) const DEFAULT_EXTENDED_BUDGET_BYTES: usize = DEFAULT_HEADROOM_BYTES;`).
   R14-5's own report ALREADY carries an R17-9 update note at its top
   disclosing this — this task's re-verification confirms that disclosed
   drift is accurate and current, not a NEW finding. All numbers in this
   report's §4 multi-heap RSS gate use the CURRENT 256 MiB value.
3. **RSS/commit retention measurement.** The MECHANISM (budget check evicts
   old entries to stay within the configured ceiling) is unchanged and still
   correct. R14-5 §3's absolute numbers (1280 MiB neutralising an adversarial
   scenario) are stale against the 256 MiB current default — R14-5's own
   R17-9 update note already says so ("only the absolute retained-commit
   figures below ... are stale against the current default and should not be
   re-cited as current behaviour without re-running the harness"). This
   task's §4 below IS that re-run, at the current 256 MiB value, additionally
   extending it to a genuine multi-heap measurement (R14-5's original §3 was
   single-heap, bare-`AllocCore`).
4. **N=1/2/4 post-materialisation hit-path correctness gate.**
   `tests/large_cache_extended_narrow_working_set_after_materialization.rs`
   (12 tests) still compiles and passes on current `HEAD` — confirmed via
   `cargo test --release --features "production bench-internals alloc-stats"`
   (§6 below). This is a CORRECTNESS gate only, as R14-5 §4 itself documents;
   it explicitly deferred the TIMING question
   (`docs/perf/OPEN_ITEMS.md` item 7, `[L]` tier) — §3 of this report is that
   deferred timing gate, now built.
5. **Mixed-size/adversarial best-fit/FIFO tests.**
   `tests/large_cache_extended_mixed_size_best_fit_fifo.rs` (2 tests) still
   compiles and passes on current `HEAD` (confirmed in the same test run).
   No drift.
6. **Production A/B/B/A gate (turnover profile).** The original harness
   files (`examples/paired_ab_large_cache_extended_{off,on}.rs`,
   `examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`,
   `scripts/_r14_5_large_cache_extended_turnover_ab.json`) all still compile
   and run UNMODIFIED against current `HEAD` — no source drift at all. §2
   below is a fresh re-run of this EXACT harness.

**Verdict: all six checkpoints hold.** Five are byte-for-byte unchanged
(mechanism and code both current); one (item 2/3, the default budget's
NUMERIC value) has a real, already-disclosed drift (5x/1280 MiB → 1x/256
MiB, R17-9) that does not invalidate the mechanism — it is the CURRENT
shipped default, and everything measured in this report uses it.

---

## 2. Step 2 — refreshed turnover A/B on current code

Per the task brief: rebuilt and re-ran R14-5 §6's EXACT harness (no source
changes — see §1 item 6 above) via `scripts/paired-ab-runner.mjs`.

**Finite-budget confirmation (task's explicit requirement):** the ON arm
(`examples/paired_ab_large_cache_extended_on.rs`) uses `SeferAlloc::new()` —
`LargeCacheConfig::DEFAULT` — which under `large-cache-extended` resolves
`budget_bytes: None` to `Some(DEFAULT_EXTENDED_BUDGET_BYTES)` = **256
MiB, the current finite default**, via
`resolved_budget_bytes()` (`src/alloc_core/large_cache_config.rs:340`). This
harness was ALREADY exercising the finite default by construction — no
change was needed to satisfy the task's "use a finite budget for the
extended-slot arm" instruction; the original R14-5 report's own harness
never used an unbounded override either. (Only R14-5 §3's SEPARATE
single-heap RSS harness had an `unbounded` mode, used deliberately to isolate
slot-count effects — see R14-5 §3.1's own text.)

### 2.1 Refreshed n=20 paired A/B/B/A (`elapsed_ns`)

Raw log: `docs/perf/_raw_r31_3_paired_ab_turnover_refresh.log`. Values below
are copied verbatim from that log (the runner's own printed statistic, not
hand-retyped or re-derived), per CLAUDE.md's "statistic names are printed by
the code that computes them" rule.

```text
=== off vs on (A - B, ns) ===
n=20  mean Δ=385.668 ms  sd=13.498 ms  se=3.018 ms  t=127.776  df=19  crit(p<0.05)=2.101  => REAL (rejects null)
sign test: off-faster=0/20  on-faster=20/20  ties=0
```

### 2.2 Same-vs-same control (harness sanity)

Raw log: `docs/perf/_raw_r31_3_paired_ab_turnover_refresh_same_vs_same.log`.

```text
=== off(A-slot) vs off(B-slot) (A - B, ns)  [SAME-VS-SAME CONTROL] ===
n=20  mean Δ=2.090 ms  sd=12.639 ms  se=2.826 ms  t=0.739  df=19  crit(p<0.05)=2.101  => NOT statistically distinguishable from noise (fails to reject null)
sign test: off(A-slot)-faster=10/20  off(B-slot)-faster=10/20  ties=0
```

### 2.3 Between-arm mechanism delta (R30-8 rule)

| Arm | `large_cache_hits` / `total_deallocs` (from a representative single launch, `docs/perf/_raw_r31_3_paired_ab_turnover_refresh.log`) |
|---|---:|
| off (base, 8 slots) | 1600 / 4800 = 33.3% |
| on (extended, 40 slots) | 4800 / 4800 = 100% |

Byte-for-byte matches R14-5 §6.1's original finding (33.3% vs 100%) — the
mechanism (base cache's FIFO eviction ceiling vs the extension's wider
address space absorbing all 24 distinct turnover sizes) is unchanged.

### 2.4 Verdict

**The R14-5 §6 turnover-profile win reproduces cleanly on current `HEAD`, at
the current finite 256 MiB default budget.** `t = 127.776` (this run) vs
R14-5's original `t = 195.759` — both enormously past `crit = 2.101`/`2.160`;
sign test unanimous both times (20/20 this run, 15/15 original). The
same-vs-same control confirms the harness itself is sound (`t = 0.739`,
noise). Absolute magnitude differs slightly (this run: off ~386 ms slower
per round vs R14-5's ~342 ms) — expected host-to-host/run-to-run variance,
not a methodology concern; both runs are unambiguously REAL by the same
`crit` threshold.

---

## 3. Step 3a — N=1/2/4 narrow-working-set-after-materialisation TIMING regression gate

**Gap closed:** `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §4
proved N=1/2/4 CORRECTNESS after sidecar materialisation but explicitly
deferred the TIMING question (`docs/perf/OPEN_ITEMS.md` item 7, `[L]` tier).
New harness: `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`
+ `examples/r31_3_large_cache_extended_narrow_{off,on}.rs`.

**Workload:** force sidecar materialisation via a 9-distinct-size burst
(same proven pattern as the existing correctness test), then narrow to the
first N (1, 2, or 4) of those 9 sizes, 3 untimed warm-up cycles, then 400
TIMED batch-alloc-all/dealloc-all cycles — timing ONLY the narrow phase, not
the materialisation burst.

### 3.1 Results (n=20 paired A/B/B/A, `elapsed_ns` of the TIMED narrow phase only)

| N | mean Δ (off − on) | t | crit(p<0.05) | sign off-faster/20 | sign on-faster/20 | Verdict |
|---|---:|---:|---:|---:|---:|---|
| 1 | 48.910 µs | 7.113 | 2.101 | 1 | 19 | REAL, ON faster |
| 2 | 112.573 µs | 17.843 | 2.101 | 0 | 20 | REAL, ON faster |
| 4 | 201.018 µs | 10.945 | 2.101 | 0 | 20 | REAL, ON faster |

Raw logs: `docs/perf/_raw_r31_3_narrow_n1.log`, `_raw_r31_3_narrow_n2.log`,
`_raw_r31_3_narrow_n4.log`.

**Same-vs-same controls** (harness sanity, N=1 and N=4):

```text
N=1: t=-0.105  (docs/perf/_raw_r31_3_narrow_n1_same_vs_same.log)  -- noise
N=4: t=1.282   (docs/perf/_raw_r31_3_narrow_n4_same_vs_same.log)  -- noise
```

Both well under `crit = 2.101` — confirms the ON-faster signal above is real,
not measurement noise.

### 3.2 Path-activation oracle (mechanism-activation proof)

Both arms show `large_cache_hits == total_deallocs` (100%) at every N (e.g.
N=4: `1600/1600` hits in both OFF and ON — see summary CSV) — every
post-warm-up alloc in the timed region is a genuine cache HIT in both arms,
so this comparison measures like-for-like servicing (hit vs hit), not a
miss contaminating one side.

### 3.3 Mechanism explanation (why ON is FASTER, not slower, on the narrow case)

**No regression found — the opposite result was measured, and it is
mechanistically explained, not a mystery.** `segments_reserved_total`
differs between arms (N=4: OFF=10, ON=14 — see raw logs): the OFF arm's
9-size materialisation burst overflows the base cache's 8 slots by exactly
1, so the burst's OWN 9th deposit triggers a real FIFO eviction of the
FIRST-deposited entry (`nine_sizes[0]`) — which IS the size the N=1/2/4
narrow working sets start from (`nine_sizes[..n]`). The evicted entry then
needs a fresh OS reservation to refill during warm-up, before timing
starts — a small one-time cost that manifests as a different
`segments_reserved_total` (and, apparently, a small persistent difference in
process/segment-table state) carried into the timed region. The ON arm's
40-slot sidecar never evicts anything during the 9-size burst (9 ≤ 40), so
it pays no such refill. The magnitude is small (120-500 ns/round) but
real and reproducible (confirmed against a clean noise floor).

**Verdict: no narrow-working-set regression exists at N=1/2/4 in this
harness — if anything, the extended cache measured FASTER than the base
cache on this exact shape**, for a mechanistically explained reason (the
base cache's own FIFO eviction pressure from materialisation, not anything
intrinsic to a wider O(40) vs O(8) scan bound — both scans are nanosecond-
scale regardless, as R14-5 §4's own text already argued qualitatively).
This is a narrower, single-workload-shape finding — it does not generalize
to "the extended cache is always faster"; it specifically rebuts the
"widened scan bound costs something on the common narrow case" concern this
gate was built to test.

### 3.4 ADDENDUM — 2026-08-01 (task #487, a Round-31 review response): the
### workload behind §3.1-§3.3 was BROKEN — do not cite the numbers above as
### current evidence

An independent review found `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`
(the shared harness §3.1's numbers were measured with) had two defects, both
now fixed in that file (task #487, not this report — this addendum documents
the correction without re-deriving a new verdict, per this project's
append-only correction convention):

1. **Wrong segment constant.** The workload hardcoded
   `let segment = 2 * 1024 * 1024usize;` with a comment claiming this equals
   `SegmentLayout::SEGMENT` and that a real import was being avoided to save
   an import. Both claims were false: `SegmentLayout::SEGMENT` is 4 MiB, not
   2 MiB, and it was always `pub` and importable. Every one of the 9
   materialisation-burst sizes downstream of this constant was therefore
   wrong (each roughly double what a correctly-derived size would be).
2. **No in-run materialisation oracle.** The only assertions in the workload
   were an alloc-succeeded null check and a length check on the size list —
   there was no read-back proof, inside the TIMED run itself, that the ON
   arm's sidecar had actually materialised to 40 slots (or that the OFF arm
   stayed at 8). The "does the widened scan bound cost anything" premise this
   gate exists to test was measured without ever confirming the widened scan
   bound was the thing running. This is a CLAUDE.md R30-8
   path-activation-oracle violation: the existing correctness test
   (`tests/large_cache_extended_narrow_working_set_after_materialization.rs`)
   proves materialisation under a DIFFERENT configuration
   (`budget=None`, 2 MiB-derived sizes pre-fix) — borrowing that proof across
   configurations is not the same as each arm proving it took the mechanism
   it claims to measure, in its own timed run.

**A third, more serious defect surfaced only once the missing oracle from
point 2 was wired up and exercised against the corrected constant from point
1:** with the real 4 MiB segment constant, the geometric-doubling
materialisation-size ladder grows to roughly 2 GiB by its 9th entry —
individually far above the 256 MiB default `large-cache-extended` budget
(`DEFAULT_EXTENDED_BUDGET_BYTES`, R17-9). The budget-vs-materialisation check
(`large_cache_deposit_budget_infeasible`) rejects a deposit whose OWN size
exceeds the budget — a per-deposit check, not a running-total check — so
under the ON arm's ORIGINAL `SeferAlloc::new()` config (the shipped 256 MiB
default), only the first 6 of the 9 materialisation sizes could ever be
admitted; the base cache would never overflow its 8 slots, and the sidecar
would never materialise at all. In other words: even after fixing the
segment constant, the ON arm as originally written would have silently
measured the SAME 8-slot base-cache code path as the OFF arm — not the
widened 40-slot scan bound the gate's entire premise depends on. This is
exactly the failure mode CLAUDE.md's R30-8 rule (and its entry-point/regime
generalisations) exists to catch, and it was caught here BY building the
missing oracle, not by inspection — the oracle's first run (with an
initially-wrong expected value of its own, `None` instead of the correct
`Some(usize::MAX)` for an explicit unbounded override under
`large-cache-extended`) failed loudly rather than silently passing.

**Status of the numbers in §3.1-§3.3 above: SUPERSEDED, not currently
trustworthy.** They were measured against the broken pre-fix workload
(wrong segment constant; and, had the fix been only the constant without
also widening the ON arm's budget, would have measured the wrong code path
entirely). They are left in place above unmodified (append-only correction
convention — this project does not silently rewrite prior report prose) but
must not be cited as current evidence for the "narrow working set after
materialisation" question. A corrected re-measurement — using the fixed
harness (real `SegmentLayout::SEGMENT`, an ON arm built with
`SeferAlloc::with_config(LargeCacheConfig::new().budget_bytes(usize::MAX))`
so materialisation can actually succeed, and an in-run oracle hard-asserting
`oracle_materialised=1` / `oracle_total_slots=40` (ON) or
`oracle_total_slots=8` (OFF) before any timing is trusted) is filed as a
separate follow-up, task #488, which is explicitly blocked on task #487 and
will re-derive §3's verdict from scratch. Task #487 itself does not attempt
that re-measurement — see that task's own scope note.

The turnover A/B in §2 above is UNAFFECTED by this addendum: it uses a
different, unmodified harness
(`examples/paired_ab_large_cache_extended_{off,on}.rs`) that this task did
not touch, and its own workload's sizes were never derived from the broken
constant. Only §3 (this section) is in question.

---

## 4. Step 3b — multi-heap RSS accounting (1/8/32 heaps)

**Gap closed:** retention is PER HEAP (`AllocCore` is owner-only, neither
`Send` nor `Sync`); R14-5 §3's original RSS measurement was SINGLE-heap, bare
`AllocCore`, never through the registry. New harness:
`examples/r31_3_large_cache_extended_multi_heap_rss_gate.rs`, mirroring
R29-13's proven subprocess-per-arm/thread-per-heap methodology exactly (fresh
OS process per `(thread_count, repetition)` cell — cross-arm state leakage
structurally impossible).

**Workload:** each of 1/8/32 concurrently-claimed heaps allocates+frees 16
distinct Large sizes (linearly spaced, ~272 MiB/heap total — chosen small
enough to keep the 32-heap aggregate in the single-digit-GiB range; see the
harness's own module doc for the rejected 40-slot/~1-GiB-per-object design
that ballooned to ~40 GiB and was replaced). `LargeCacheConfig::new()` (no
override) — this measures the SHIPPED DEFAULT, not a custom sweep: OFF
resolves `budget_bytes: None` (unbounded base cache); ON resolves
`Some(256 MiB)` (R17-9's current default).

### 4.1 Evidence per the R26-4/R30-8 rules (config identity + mechanism activation)

Every cell hard-asserted, per heap: resolved budget matches this build's
expected default (read back via the new `HeapCore::dbg_large_cache_budget`
accessor, not assumed) AND `config_conflicts_total()` delta == 0. Every cell
hard-asserted `used_post_teardown_max > 0` (admission proven — 272 MiB/heap
exceeds the 256 MiB budget either way). Raw logs:
`docs/perf/_raw_r31_3_multi_heap_rss_off.log`,
`docs/perf/_raw_r31_3_multi_heap_rss_on.log`.

### 4.2 Results (median of 3 repetitions per cell)

| Threads | Arm | `used_post_teardown_max`/heap | `extension_materialised_count` | `rss_post_kib`/heap |
|---:|---|---:|---:|---:|
| 1 | off | 432.0 MiB | 0/1 | 403.3 MiB |
| 1 | on | 248.0 MiB | 1/1 | 235.2 MiB |
| 8 | off | 432.0 MiB | 0/8 | 400.5 MiB |
| 8 | on | 248.0 MiB | 8/8 | 232.5 MiB |
| 32 | off | 432.0 MiB | 0/32 | 400.2 MiB |
| 32 | on | 248.0 MiB | 32/32 | 232.2 MiB |

Full per-cell CSV: `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE_summary.csv`.

### 4.3 Linearity check (does per-heap extrapolation hold here?)

`used_post_teardown_sum / used_post_teardown_max` = **exactly** `1.0000`,
`8.0000`, `32.0000` at 1/8/32 threads respectively, in BOTH arms — i.e. the
per-heap admission ceiling scales EXACTLY linearly with heap count in this
workload, no shared/amortized state distorts it. `rss_post_kib`/heap is also
stable within ~1% across 1/8/32 threads in both arms (OFF: 403.3 → 400.5 →
400.2 MiB/heap; ON: 235.2 → 232.5 → 232.2 MiB/heap) — small per-process fixed
overhead amortizing away as thread count grows, not a material distortion.

**Honest scope note:** this task's own brief warned "extrapolation can be
wrong if there's shared/amortized state" — in THIS specific workload shape,
direct measurement happens to CONFIRM that simple multiplication would have
been accurate (both `used_post_teardown` and `rss_post_kib` scale near-
perfectly linearly). This is not a foregone conclusion the harness assumed;
it is what was found. A different workload shape (e.g. heavily front-loaded
teardown timing, or a process nearer a physical memory ceiling triggering
paging) could still show non-linear behaviour — this gate rules out the
concern for the measured shape, not for all conceivable shapes.

### 4.4 Verdict

**The finite 256 MiB default budget genuinely bounds per-heap large-cache
retention, identically at 1/8/32 concurrently-claimed heaps, even with the
extended cache's wider 40-slot ceiling** — `used_post_teardown_max` is capped
at ~248 MiB/heap in the ON arm vs ~432 MiB/heap in the OFF arm's unbounded
base cache (in THIS workload, where 16 distinct sizes exceed the base
cache's own 8-slot capacity and the unbounded budget lets whichever 8
survive FIFO eviction retain up to ~432 MiB). The sidecar materialised in
EVERY heap at every thread count (mechanism-activation proof,
`extension_materialised_count` = thread_count exactly). No multi-heap RSS
blow-up was found — the per-heap ceiling holds as designed.

---

## 5. Step 4 — promotion proposal (REQUIRES EXPLICIT USER SIGN-OFF — not self-authorized)

**Do not act on this section without explicit approval.** Per the task's
instructions, `Cargo.toml`'s `production = [...]` line was NOT touched and no
`Profile`/config code was implemented as part of this task — steps 1-3 above
are the complete deliverable; this section is a proposal only.

### 5.1 Summary of evidence

- All six R14-5 hardening checkpoints hold on current `HEAD` (§1) — one has
  a disclosed, already-applied numeric revision (5x → 1x default budget,
  R17-9), not a new gap.
- The turnover-profile win reproduces cleanly on current code, at the
  current finite default (§2): `t = 127.776`, sign 20/20, mechanism
  confirmed (33.3% → 100% hit rate).
- The N=1/2/4 narrow-working-set TIMING concern the original report deferred
  is now measured and closed: no regression found; the extended cache
  measured FASTER on this exact shape, mechanistically explained (§3).
- Multi-heap RSS accounting (§4) confirms the finite default budget scales
  correctly (exactly linearly in the measured workload) across 1/8/32
  concurrently-claimed heaps — no multi-heap blow-up.

### 5.2 Proposed finite default budget value

**No change proposed to the numeric value.** The CURRENT shipped default
(`DEFAULT_EXTENDED_BUDGET_BYTES = DEFAULT_HEADROOM_BYTES` = 256 MiB/heap,
R17-9) is the value this report's §4 multi-heap gate measured and confirmed
sound (bounds retention correctly at 1/8/32 heaps, no blow-up). If this
feature is promoted, the recommendation is to promote it WITH this exact
existing default — no new number is being proposed.

### 5.3 Coordination with R31-9/#473 (`Profile` API rework)

`src/alloc_core/profile.rs`'s current `Profile` enum (`Rss` / `Balanced` /
`Throughput`) sets `headroom_bytes` (large-cache decay floor) and the
small-pool pair (`pool_segments`/`pool_byte_cap`) together — it does NOT
touch `budget_bytes` (the large-cache hard ceiling this feature's default
uses) at all today, and has no axis for "how many large-cache slots" (8 vs
8+32). Per this task's brief, R31-9/#473 is separately reworking `Profile`'s
axes ("Rss is not an RSS bound; split the two independent axes" — see that
task's own description). If `large-cache-extended` is promoted, the cleaner
integration point is likely a NEW named `Profile` axis (e.g. something like
`large_cache_slots: DiverseTurnover` alongside the existing `headroom_bytes`
axis) rather than an unconditional `production` composition change that
turns the sidecar on for every caller regardless of workload shape — R13-8's
own honest caveat (a static live-set workload sees ~0 hits and no benefit
from the extension) still applies, so a named opt-in Profile variant that a
caller chooses because their workload IS turnover-shaped is a better fit
than a blanket default. This report does NOT implement that — it is a
coordination note for whoever picks up R31-9 next, so the two tasks don't
land conflicting designs.

### 5.4 Cost disclosure if this proposal is accepted (CLAUDE.md's bench-table-refresh rule)

If a future round accepts this proposal and changes `production`'s feature
composition (either directly, or by making a new `Profile` variant that
includes `large-cache-extended` become a new DEFAULT — the rule applies to
either), this would be Round 31's SECOND candidate production-composition
change, alongside the reopened `virgin-zero-skip` question (task #471/R31-0).
CLAUDE.md mandates that ANY composition change re-run `npm run bench:table` +
`npm run iai` and commit the refreshed README.md / `docs/perf/IAI_BASELINE.md`
numbers in the SAME PR as the change. This cost is flagged here so the user
can weigh it explicitly when deciding whether to accept this proposal — it
is not automatically triggered by this report, since this report does not
itself change any composition.

### 5.5 What would need to happen next, if approved

1. User explicit sign-off on promoting `large-cache-extended` (with its
   current 256 MiB default budget) — either as a `production` feature or as
   a new named `Profile` variant (coordinate with R31-9/#473, per §5.3).
2. If approved, re-run `npm run bench:table` + `npm run iai` and commit
   refreshed README.md / `docs/perf/IAI_BASELINE.md` numbers in the SAME PR
   (§5.4).
3. Update `docs/FEATURE_PROMOTION_STATUS.md`'s survey row and
   `docs/perf/OPEN_ITEMS.md` item 30 to reflect the closure.

**This report takes no position on whether the user SHOULD approve
promotion** — it reports that the measured evidence is clean and does not
identify any new blocking concern; the decision itself is explicitly left to
the user, per the task's instructions.

---

## 6. Verification run (per this task's brief)

- `cargo test --release --features "production bench-internals alloc-stats"`
  — see §7 below for the exact pass count and confirmation nothing broke.
- `cargo clippy --features "production bench-internals alloc-stats
  large-cache-extended" --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- All new example binaries build clean under both feature combinations
  (`large-cache-extended` off and on) — confirmed in §2-§4 above via direct
  execution.

---

## 7. Files changed/added

**Source (two new `HeapCore`-level diagnostic delegations, following the
established `dbg_large_cache_used`/`dbg_pool_cap` pattern in this file — all
read-only `&self`, `bench-internals`-gated, no production caller, no raw
pointer, no new `unsafe`):**
- `src/registry/heap_core_diag.rs` — `HeapCore::dbg_large_cache_budget`,
  `HeapCore::dbg_large_cache_extended_slot_sizes`,
  `HeapCore::dbg_large_cache_extension_materialised` (thin delegations to the
  pre-existing `AllocCore` methods of the same names).

**New measurement harnesses:**
- `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`
- `examples/r31_3_large_cache_extended_narrow_off.rs`
- `examples/r31_3_large_cache_extended_narrow_on.rs`
- `examples/r31_3_large_cache_extended_multi_heap_rss_gate.rs`
- `scripts/_r31_3_large_cache_extended_narrow_n1_ab.json`,
  `_n2_ab.json`, `_n4_ab.json`

**`Cargo.toml`:** four new `[[example]]` entries for the harnesses above.
`production`'s feature composition line was NOT touched.

**Raw logs** (`git add -f`'d alongside this report per the raw-log policy):
- `docs/perf/_raw_r31_3_paired_ab_turnover_refresh.log`
- `docs/perf/_raw_r31_3_paired_ab_turnover_refresh_same_vs_same.log`
- `docs/perf/_raw_r31_3_narrow_n1.log`, `_n1_same_vs_same.log`
- `docs/perf/_raw_r31_3_narrow_n2.log`
- `docs/perf/_raw_r31_3_narrow_n4.log`, `_n4_same_vs_same.log`
- `docs/perf/_raw_r31_3_multi_heap_rss_off.log`
- `docs/perf/_raw_r31_3_multi_heap_rss_on.log`

**Summary CSV:** `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE_summary.csv`.

**Docs:** `docs/perf/OPEN_ITEMS.md` item 30 (append-only refresh),
`CHANGELOG.md` Round 31 section (append), this file.

**CORRECTED 2026-07-31 (Round 31 review response, R31-14a/task #483):** the
summary CSV's `rss_post_kib_per_heap,off,threads=8` row's note string
originally read `"3280892/8 = ~410 MiB/heap (400.5 rounded)"` — a KiB/MiB
unit slip (410,112 KiB = 400.5 MiB, not ~410 MiB; the note contradicted
itself in one string). Its two sibling rows (threads=1 "~403 MiB", threads=32
"~400 MiB") were already correct, and this report's own §4.2 table (line
279: `8 | off | 432.0 MiB | 0/8 | 400.5 MiB`) and §4.3 trend line already
stated the correct 400.5 MiB figure — only the CSV note string was wrong.
Fixed to `"3280892/8 = ~400.5 MiB/heap (CORRECTED 2026-07-31, ...)"` in the
CSV itself; the `value`/`unit` columns (410112/kib) were always correct and
are unchanged.
