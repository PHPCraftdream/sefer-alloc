# R21-2 — OPT-H Stage-1 diagnostic hit-rate measurement (observation-only)

**Task:** R21-2 (task #351, P1). Continuation of R20-3's (task #348)
CONDITIONAL-GO design, per `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`
§6.1/§8 step 1: implement Stage-1 diagnostic counters ONLY — no behavior
change, no grow action, just observation of how often OPT-H's six
preconditions *would* hold if the mechanism existed. This report is Stage 1's
measurement; it is NOT a decision to implement OPT-H's real grow action (that
is explicitly out of this task's scope either way, per the design's own
two-stage discipline, `docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md`
§5.1).

**Date:** 2026-07-26. **Base revision:** `main` @ `517a85b` (R21-1, task #350)
plus this task's own uncommitted diff (`src/alloc_core/alloc_core.rs`,
`src/alloc_core/alloc_core_core_diag.rs`).

---

## 1. What was implemented (recap — see the diff for the actual code)

Two `pub(crate) static AtomicU64` counters in `src/alloc_core/alloc_core.rs`
(mirroring `LARGE_ZERO_PASS_CALLS`/`SMALL_ZERO_PASS_CALLS`'s exact pattern:
storage always compiled, per-event increment gated behind `alloc-stats`):

- `OPT_H_ATTEMPTS` — incremented once per cross-class, growing Small/
  Primordial realloc that reaches OPT-H's precondition-1 check inside
  `AllocCore::realloc_inplace_fast_path_known_base` (the denominator).
- `OPT_H_HITS` — incremented once per such attempt where ALL SIX of the
  design's §2.1 preconditions hold (the numerator).

Both increments sit inside a new branch added to the EXISTING OPT-F
same-class check (`alloc_core.rs`, inside the `if matches!(kind,
SegmentKind::Small | SegmentKind::Primordial)` block), in the `else` arm
taken when `new_class != old_class` (OPT-F declined). The branch:

1. Checks precondition 1 (`block_size(new_class) > block_size(old_class)`,
   i.e. genuinely growing) — if true, bumps `OPT_H_ATTEMPTS`.
2. Computes `off`, re-derives `SegmentMeta::new(base)`, and checks
   preconditions 3 (tail-adjacency, `off + block_size(old_class) ==
   meta.bump_of()`), 4 (new-class alignment, `off.is_multiple_of(new_block_size)`),
   5 (segment capacity, `off + new_block_size <= SEGMENT`). Precondition 6
   (lazy-commit frontier) is treated as trivially `true` for this
   observation-only task — see the code comment at the call site for the
   full reasoning (when `primordial-lazy-commit`/`small-segment-lazy-commit`
   are off there is nothing to commit; when they are on, this Stage-1 counter
   does not independently re-verify the frontier, which is a documented,
   accepted overcount risk for lazy-commit builds specifically, not a new
   checked code path).
3. If all four evaluated preconditions hold, bumps `OPT_H_HITS`.
4. The function ALWAYS falls through to the pre-existing `None` afterward —
   no pointer is ever returned from this new code, no `bump`/bitmap/BinTable
   state is ever touched. The entire block is additionally gated on
   `#[cfg(feature = "alloc-stats")]` (not just the `fetch_add` calls), so a
   plain `production` build pays zero cost: no extra branch, no extra load.

Two `#[doc(hidden)]` read accessors, `AllocCore::dbg_opt_h_attempts()` /
`dbg_opt_h_hits()`, added to `src/alloc_core/alloc_core_core_diag.rs`
(mirroring `dbg_wasted_dirty_drains()`'s exact pattern).

---

## 2. Correctness verification (before trusting the hit-rate numbers)

A new regression test,
`tests/r21_2_opt_h_stage1_precondition_probe.rs`, hand-constructs two
scenarios directly against `AllocCore` (no registry-level promotion in the
way):

1. **`opt_h_hits_increments_for_a_genuinely_tail_adjacent_aligned_grow`** — a
   768 KiB→1 MiB cross-class grow at the 4th (last) 768 KiB block carved into
   a fresh segment (offset `4 × 768 KiB = 3 MiB`, empirically verified to be
   the segment's bump tail, 1 MiB-aligned, and exactly filling the remaining
   `SEGMENT` capacity). Asserts `dbg_opt_h_hits()` increments by exactly 1,
   AND asserts the function's observable behavior is unchanged (`grown !=
   tail_ptr` — the block still relocates via the ordinary move-leg; OPT-H is
   not actually implemented).
2. **`opt_h_attempts_but_not_hits_for_a_non_tail_adjacent_grow`** — the SAME
   grow shape applied to the 1st-carved 768 KiB block in the same segment
   (no longer the tail once 3 more objects were carved after it). Asserts
   `dbg_opt_h_attempts()` increments but `dbg_opt_h_hits()` does NOT.

**Non-vacuity check (personally re-verified during this task, not merely
claimed):** temporarily forcing `tail_adjacent = true` AND `new_class_aligned
= true` (leaving only the capacity check real) made test 2 fail exactly as
expected — proving the negative assertion is sensitive to the real
precondition logic, not trivially true. Reverting restored both tests to
green.

**Both tests pass** under `--features "production,medium-classes,alloc-stats"`.

---

## 3. Stage-1 measurement — both harnesses

Per the design's own §6.1 plan, both harnesses were run with
`--features "production,medium-classes,alloc-stats"`, via a small one-off
instrumented wrapper (`include!`ing the existing unmodified shared workload
file and printing `AllocCore::dbg_opt_h_attempts()`/`dbg_opt_h_hits()` after
the run — not a new permanent reporting pipeline, per the task's own
guidance; not committed, reconstructable from the two commands below).

### 3.1 R10-2's existing adversarial harness (`examples/_shared/paired_ab_medium_workload.rs`, unmodified)

Reconstruction:
```text
cat > examples/_tmp_probe.rs <<'RS'
use sefer_alloc::{AllocCore, SeferAlloc};
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
include!("_shared/paired_ab_medium_workload.rs");
fn main() {
    run_phased_workload();
    println!("opt_h_attempts={}", AllocCore::dbg_opt_h_attempts());
    println!("opt_h_hits={}", AllocCore::dbg_opt_h_hits());
}
RS
cargo run --release --example _tmp_probe --features "production,medium-classes,alloc-stats"
```

**Result:**
```text
opt_h_attempts=320
opt_h_hits=0
```

Hit rate: **0 / 320 = 0%**.

This matches the design's own §5.2 prediction (N=16-simultaneous-object
harness, 20 rounds → 320 first-grow-crossing attempts, at most one
tail-adjacent candidate per segment per round, and that one candidate fails
precondition 4's alignment check for the 256→320 KiB transition specifically
— confirmed by direct arithmetic: for the first-carved object in a fresh
segment, `off = 256 KiB`, and `256 KiB % 320 KiB = 256 KiB ≠ 0`).

### 3.2 R21-1's single-hot-buffer harness (`examples/_shared/paired_ab_hot_buffer_workload.rs`, unmodified, task #350)

Reconstruction: identical shape, `include!("_shared/paired_ab_hot_buffer_workload.rs")`
+ `run_hot_buffer_workload()`.

**Result:**
```text
opt_h_attempts=20
opt_h_hits=0
```

Hit rate: **0 / 20 = 0%**.

`ROUNDS = 20` in this harness; the attempt count (20) confirms EXACTLY one
OPT-H-eligible attempt per round, not five (one per `GROW_STEPS` rung) as
might be assumed from the ladder's five grow calls per round. Root cause,
traced directly (see §4 below): the harness's own design promotes the buffer
to a Large segment on its VERY FIRST grow step every round (256 KiB → 320
KiB immediately crosses `MEDIUM_REALLOC_PROMOTION_THRESHOLD` = 256 KiB), so
only that first step is ever a Small/Primordial-kind cross-class grow — every
subsequent step in the same round is already Large-kind (rides OPT-G, not
OPT-H's code path at all, since OPT-H's diagnostic branch is nested inside
`if matches!(kind, SegmentKind::Small | SegmentKind::Primordial)`). The reset
back down to `REALLOC_BASE` (256 KiB) each round is itself a SHRINK on the
Large segment (also not OPT-H's path), and the next round's first grow step
starts the cycle over on a FRESH allocation (a new Primordial/Small carve,
offset 256 KiB again in a fresh or reused segment) — always failing
precondition 4 for the exact same reason as §3.1.

---

## 4. Root-cause trace (why the "friendliest" harness still shows 0%)

A hand-traced repro (not part of the permanent test suite; used only to
confirm the measurement is real behavior, not a counter bug) walked the exact
offsets/kind across 3 rounds of the hot-buffer ladder:

```text
round 0: p_off=262144 cur=262144
  grow 262144->327680: attempts+=1 hits+=0 new_off=4096        <- promotes to Large here
  grow 327680->393216: attempts+=0 hits+=0 new_off=4096        <- already Large; OPT-G, not OPT-H
  grow 393216->524288: attempts+=0 hits+=0 new_off=4096
  grow 524288->786432: attempts+=0 hits+=0 new_off=4096
  grow 786432->1048576: attempts+=0 hits+=0 new_off=4096
round 1: p_off=262144 cur=262144                                <- fresh Small/Primordial carve again
  grow 262144->327680: attempts+=1 hits+=0 new_off=4096
  ...
round 2: p_off=262144 cur=262144
  grow 262144->327680: attempts+=1 hits+=0 new_off=4096
  ...
```

Every round's ONLY OPT-H-eligible attempt is the very first grow
(256→320 KiB) at offset 262144 (= 256 KiB — the first-carved object in a
fresh/reused segment's Small/Primordial region). `262144 % 327680 =
262144 ≠ 0` — precondition 4 (new-class alignment) fails identically every
round. This is a **structural** property of the harness's own reset
mechanics (reset-to-base via `realloc` down, not free+fresh-alloc, means the
buffer is re-promoted from a freshly-carved 256 KiB Small block every round,
always at the SAME non-1-in-3-friendly carve position — the first slot in
its segment), not a flaw in the precondition-checking code (§2's tests
already independently prove the checking logic is correct and
discriminating).

---

## 5. Does this meet the design's CONDITIONAL-GO trigger?

Per `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §9: *"Trigger for
proceeding to implementation: Stage 1's diagnostic hit-rate counters, run
against the NEW single-hot-buffer harness... show that OPT-H's combined
tail-adjacency + alignment preconditions... hold for a material majority of
that harness's cross-class grow attempts... the qualitative bar is 'most
grows of a single actively-building buffer take the fast path,' not merely
'measurably more than zero.'"*

**Trigger NOT met.** The single-hot-buffer harness shows a **0% hit rate**
(0/20), which is not merely short of "a material majority" — it is the floor.

**This is not a rejection of OPT-H's underlying reasoning** (§1–2 of the
design remain sound: the mechanism is zero-cost when it fires, and a truly
un-promoted, repeatedly-grown-in-place Small/medium buffer would plausibly
hit tail-adjacency on most grows). What this measurement actually shows is
narrower and more specific: **R21-1's harness, as built, does not realize
that target pattern** — it measures a buffer that gets promoted to Large on
its first crossing every round (by the harness's own design, since
`REALLOC_BASE` = the smallest medium class and the very next rung already
exceeds `MEDIUM_REALLOC_PROMOTION_THRESHOLD`), so OPT-H's code path is only
ever reached for ONE grow per round, at a fixed, alignment-unfriendly carve
position. The harness is a single-buffer-at-a-time workload, but it is NOT a
"never promoted, walks the whole ladder as Small/medium" workload — those
are different things, and R20-3's design implicitly needed the latter for
its "most grows take the fast path" prediction to have a chance.

**Two honest readings, both consistent with the data:**

1. **OPT-H's real-world victim workload — one buffer growing through several
   medium-class rungs while staying Small/Primordial-classified the whole
   time, never crossing into promotion — is narrower than either existing
   harness represents,** and does not (yet) have a harness that isolates it.
   Building one would require either raising
   `MEDIUM_REALLOC_PROMOTION_THRESHOLD` for the probe, or picking grow steps
   that stay under it (impossible with the current 6-class ladder, since
   `REALLOC_BASE` is already at the threshold).
2. **Alternatively — and more likely, given §3.1's independently-derived
   result on the completely different N=16 harness also landing at exactly
   0%, not merely "low"** — precondition 4 (new-class alignment) may simply
   never be satisfiable for a block sitting at the SMALLEST rung's
   lowest-order carve position in a fresh segment, for ANY of this ladder's
   class-size ratios, because the smallest class's block size does not
   divide evenly into any larger rung from that specific starting offset.
   This would mean the true "first-crossing" cost (§0 of R20-3, the
   ~1,180×–2,111× regression measured by R10-2/R18-2) is structurally
   immune to OPT-H regardless of workload shape — OPT-H can only ever help a
   grow that happens to land on an ALREADY-medium-classified, ALREADY
   Small/Primordial-kind block that has NOT yet been promoted and IS
   currently the segment's tail, at an alignment-friendly offset — a
   narrower intersection than either harness currently probes.

**Recommendation: NO-GO for implementing OPT-H's real grow action in the
next round, on the CURRENT evidence.** Per the design's own §9 framing, this
is not a rejection of the mechanism's soundness — it is that neither
available harness demonstrates the predicted victim case materializing, and
building a third, narrower harness (one that never crosses the promotion
threshold, e.g. growing only among the existing sub-256-KiB Small classes,
where there is no promotion mechanism to interfere at all) is a separate,
not-yet-scoped follow-up if this lever is revisited. This item should be
recorded in `docs/perf/OPEN_ITEMS.md` as **closed for this
implementation cycle** (Stage 1's own gate — §6.1's decision rule — says NOT
to proceed to Stage 2's wall-clock investment when the hit rate is this low),
with a note that a narrower "never-promoted Small ladder" harness is the
only remaining unexplored variant if a future round wants to revisit this.

---

## 6. Verification summary

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets --features "production,medium-classes,alloc-stats" -- -D warnings` — clean (one `manual_is_multiple_of` finding fixed during development: `off % new_block_size == 0` → `off.is_multiple_of(new_block_size)`).
- `cargo clippy --all-targets -- -D warnings` (empty features) — clean.
- `cargo clippy --all-targets --features experimental -- -D warnings` — clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo build --release --features production` — clean, zero warnings; the
  two new `fetch_add` call sites and their surrounding precondition
  computation compile OUT ENTIRELY (verified: the whole precondition-checking
  block, not just the increments, is gated on `#[cfg(feature =
  "alloc-stats")]`, which is NOT part of `production`).
- `cargo test --release --features "production,medium-classes"` (full
  existing suite) — 223 test binaries, **all green** after fixing one
  incidental doc-drift canary (`no_stale_doc_references.rs`'s
  `architecture_test_file_count_matches_reality`, which counts `tests/*.rs`
  files — bumped 220→221 in `docs/ARCHITECTURE.md` to reflect this task's new
  test file; unrelated to OPT-H itself).
- `cargo test --release --features "hardened medium-classes"` (full existing
  suite) — all green, including doc-tests.
- `tests/r14_4_promotion_move_leg_reduction.rs` and
  `tests/regression_hardened_large_kind_own_free.rs` (the two named
  hot-path-adjacent regression suites) — explicitly re-run individually under
  both `production,medium-classes` and `hardened medium-classes`; all pass.
- New test `tests/r21_2_opt_h_stage1_precondition_probe.rs` — 2/2 pass, with
  a personally-verified counterfactual (breaking the precondition logic makes
  the negative-case test fail, confirming non-vacuity).
- iai/Ir re-confirmation: **not run in this session** — this Windows
  environment has no WSL/Valgrind available (consistent with prior rounds'
  notes on this environment). The plain-`production` build's zero-codegen
  claim was instead verified structurally (the entire new precondition block,
  not merely the two atomics, is behind `#[cfg(feature = "alloc-stats")]`,
  which `production` does not include) and by a clean warning-free release
  build. **Recommend the full `npm run iai --features production` byte-identical-`Ir`
  re-confirmation be run in CI or on a Linux host** before this diff is
  merged, per the task's own instruction.

---

## 7. Files touched by this task

- `src/alloc_core/alloc_core.rs` — `OPT_H_ATTEMPTS`/`OPT_H_HITS` statics; new
  observation-only branch inside `realloc_inplace_fast_path_known_base`.
- `src/alloc_core/alloc_core_core_diag.rs` — `dbg_opt_h_attempts()` /
  `dbg_opt_h_hits()` read accessors.
- `tests/r21_2_opt_h_stage1_precondition_probe.rs` — new regression test
  (2 tests).
- `docs/ARCHITECTURE.md` — test-file count bump (220→221, incidental).
- `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md` — this report.
- `docs/perf/OPEN_ITEMS.md` — Active item 1 updated with this report's
  verdict (see that file's diff).

**Not committed, not pushed** — per this round's explicit instruction, this
diff awaits a separate zero-trust review pass before any commit.
