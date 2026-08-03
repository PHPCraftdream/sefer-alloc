# R32-3 (task #494) — realloc move leg / `try_promote_to_large` skip redundant `contains_base` recompute

Date: 2026-08-02 (R32-3 landed); this report added 2026-08-03 by R33-12 (task #517).

## 0. What this is and why this report exists

`HeapCore::realloc`'s own-segment move leg and `try_promote_to_large` both
already hold `base` (proven ours & live via an earlier `contains_base(base)`
check in the same call), but their closing dealloc went through
`self.dealloc(ptr, old_layout)` → `HeapCore::dealloc` → (alloc-xthread)
`dealloc_routing`, which RECOMPUTES `os::segment_base_of_ptr(ptr)` (~9.03 Ir,
R23-1/R29-10) and RE-RUNS `contains_base` (~8.2 Ir Tier-1 hit / ~12.0 Ir
Tier-2 miss, R22-17/R23-3) from scratch — even though
`dealloc_own_thread_with_base` exists specifically to take a pre-computed
base and skip this. Commit `5d72bc633193938181e2d06f8c584617ebaecf42`
(R32-3, task #494) routes both call sites through
`dealloc_own_thread_with_base(ptr, old_layout, base)` directly under
`alloc-global + fastbin` (and `dealloc_own_thread(ptr, old_layout)` otherwise),
mirroring `dealloc_routing`'s own `contains_base(base) == true` arm exactly,
including its `#[cfg]` split. See that commit's own message for the full
correctness argument (the OLD block at `ptr` is still LIVE until the closing
dealloc, so its segment's `live_count` stays > 0 throughout, and
`contains_base(base)` is still true exactly as when first checked).

This is a `perf(runtime)` change on the DEFAULT `production` path. It was the
ONLY shipping change in its round with no gate report at all: there was no
`docs/perf/R32_3_*.md`, no `_summary.csv`, no `_raw_*.log`, and no derive
script — the measured numbers (`realloc_grow` 492,694 → 492,574 Ir; four churn
benches byte-exact) existed ONLY in the commit message. That gap is finding
**F10 [P3]** in `docs/reviews/2026-08-03-round32-readonly-review.md` §7.

### The tension, stated honestly

CLAUDE.md's R22-14 boundary rule is phrased in terms of "a perf-gate report",
so a commit message arguably falls outside its letter. But the rule's own
stated TEST is "does the verdict rest on a number obtained by RUNNING
SOMETHING" — and here it plainly does. R32-4
(`docs/perf/R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE.md`) and R32-5
(`docs/perf/R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md`), comparable in size and
SMALLER in measured effect, both received a full report + raw logs + checked
derive script. The inconsistency is the finding.

This report (R33-12, task #517) closes it by producing the missing artifacts.
It does NOT re-decide R32-3's already-shipped change — its job is making the
ALREADY-PUBLISHED numbers reproducible from committed artifacts, exactly the
precedent CLAUDE.md's own R22-14 rule set for R21-2's throwaway probe
(promoting a reproducible measurement to a small permanent committed example
when the underlying measurement is genuinely reproducible from existing
infrastructure). The measurement here is cheaply reproducible: the pre-existing
`realloc_grow` iai bench plus the four churn benches this project runs routinely
for every kill-gate check, no new infrastructure.

## 1. The change

`src/registry/heap_core_free.rs`: in `HeapCore::realloc`'s own-segment move leg
and in `try_promote_to_large`, the closing `self.dealloc(ptr, old_layout)` was
replaced with `dealloc_own_thread_with_base(ptr, old_layout, base)` (under
`alloc-global + fastbin`) / `dealloc_own_thread(ptr, old_layout)` (otherwise),
reusing the `base` already proven earlier in the same call and skipping the
redundant `segment_base_of_ptr` recompute + `contains_base` re-run. No bench
file changed; `README.md`'s tier-2 `#[allow(unsafe_code)]` count dropped 7 → 6
(`try_promote_to_large`'s own item-level attribute is gone).

**Entry point under test:** `HeapCore::realloc`, reached from
`SeferAlloc::realloc` (the real `#[global_allocator]` face) — not a lower
`AllocCore`-only layer. This is the layer the promotion/verdict applies to:
the redundant recompute lived in `HeapCore`'s own realloc/promote legs, and
the measured `realloc_grow` bench drives `SeferAlloc::realloc` end-to-end.

## 2. Measurement — worktree-isolated, plain `production bench-internals`

### 2.1 Which benches

No new bench code was needed. `benches/perf_gate_iai.rs`'s pre-existing
`realloc_grow` (16 geometric doublings 64 B → 4 MiB; under plain `production`,
no `medium-classes`, every doubling that is not an OPT-F same-class / OPT-G
in-place-grow hit takes the move leg by construction) already drives the changed
code path, and the four churn benches (`small_churn_16b`,
`medium_class_dealloc_churn_16b`, `churn_256b`, `churn_write_256b`) — which
never call realloc — serve as the byte-exact kill-gates confirming no codegen
perturbation outside the realloc path.

### 2.2 Immutable source identity (CLAUDE.md's R29-6 rule)

- **BEFORE** (redundant recompute present): `git worktree add
  ../sefer-alloc-r517-before f3020fdb5e0f0dcd41f3bc46a3d2ab44e5fce0df`
  (R32-3's parent = `main`'s HEAD immediately before `5d72bc6` landed), no
  changes applied.
- **AFTER** (redundant recompute removed): `git worktree add
  ../sefer-alloc-r517-after 5d72bc633193938181e2d06f8c584617ebaecf42`
  (R32-3 itself), no changes applied.

Both are clean landed commits, so no patch hash or tree SHA is needed beyond
the commit SHAs themselves — the only difference between the two measured trees
is R32-3's diff to `src/registry/heap_core_free.rs` (+ `README.md`'s doc-count
update), and both are measured through byte-identical bench source.

Feature set: `production bench-internals` (the `npm run iai` /
`scripts/iai.mjs` default — the same feature set the commit message cites).
CPU: Intel Core i7-11800H 2.30GHz; OS: Windows 10 (WSL/valgrind callgrind).

### 2.3 Reproduction

```text
# BEFORE (isolated worktree at R32-3's parent):
git worktree add ../sefer-alloc-r517-before f3020fdb5e0f0dcd41f3bc46a3d2ab44e5fce0df
cd ../sefer-alloc-r517-before
node scripts/iai.mjs realloc_grow small_churn_16b medium_class_dealloc_churn_16b churn_256b churn_write_256b
# -> docs/perf/_raw_r32_3_realloc_before.log

# WIPE the shared iai target between the two worktree runs (see §2.4):
rm -rf /tmp/sefer-iai

# AFTER (isolated worktree at R32-3 itself):
git worktree add ../sefer-alloc-r517-after 5d72bc633193938181e2d06f8c584617ebaecf42
cd ../sefer-alloc-r517-after
node scripts/iai.mjs realloc_grow small_churn_16b medium_class_dealloc_churn_16b churn_256b churn_write_256b
# -> docs/perf/_raw_r32_3_realloc_after.log

# Derive the summary CSV (asserts the arithmetic, per CLAUDE.md's checked-script rule):
node scripts/r32_3_realloc_redundant_contains_base_summary.mjs
```

Raw logs (full `npm run iai`-style reports, not truncated):
`docs/perf/_raw_r32_3_realloc_before.log`,
`docs/perf/_raw_r32_3_realloc_after.log`. Summary CSV:
`docs/perf/R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE_summary.csv`, produced by
`scripts/r32_3_realloc_redundant_contains_base_summary.mjs` — the one checked
script; it hard-asserts (a) all four churn kill-gate deltas are exactly 0, (b)
the treatment arm's delta is negative, and (c) the treatment delta is exactly
−120 Ir (492,694 → 492,574) and exactly −7.5 Ir/step over 16 steps, matching
the commit message's own decomposition, before writing the CSV or printing a
number.

### 2.4 Reproduction trap (encountered and fixed during this task)

`scripts/iai.mjs` uses a fixed `/tmp/sefer-iai` target dir. Measuring two
different commits via two worktrees back-to-back WITHOUT wiping that target
made cargo reuse the FIRST run's compiled `sefer-alloc` artifact for the SECOND
run: the two worktrees are different paths but share the target, and cargo's
fingerprint concluded the benchmark was up-to-date and skipped recompiling
`sefer-alloc`. The symptom was the AFTER run finishing in ~2 s ("Finished", no
"Compiling sefer-alloc") and reporting `(No change)` against the BEFORE
baseline — a FALSE non-reproduction, because the AFTER binary was actually the
BEFORE binary (verified: identical benchmark binary path in both logs, and the
first AFTER run's `realloc_grow` showed `492694|492694` instead of the real
`492574`). The fix is the `rm -rf /tmp/sefer-iai` step in §2.3: each worktree
run then forces a full recompile (~2m50s) from its own source, and both logs
show `Instructions: N|N/A` (fresh measurement, no baseline contamination). This
trap is specific to the shared-target-dir + two-worktree measurement shape and
is noted here so a future re-runner does not reproduce the false-zero.

## 3. Result

| bench | before Ir (f3020fd) | after Ir (5d72bc6) | Δ Ir | note |
|---|---:|---:|---:|---|
| `realloc_grow` | 492,694 | 492,574 | **−120** | treatment — 16 realloc-grow steps via the move leg |
| `small_churn_16b` | 8,055 | 8,055 | 0 | kill-gate (never calls realloc) |
| `medium_class_dealloc_churn_16b` | 8,055 | 8,055 | 0 | kill-gate (never calls realloc) |
| `churn_256b` | 8,055 | 8,055 | 0 | kill-gate (never calls realloc) |
| `churn_write_256b` | 8,311 | 8,311 | 0 | kill-gate (never calls realloc) |

The re-measurement reproduces the commit message's cited numbers EXACTLY:
`realloc_grow` 492,694 → 492,574 (−120 Ir, −7.5 Ir/step × 16 steps), and all
four churn kill-gates byte-exact (0 Ir delta). The −120 Ir over 16 grow steps is
consistent with skipping one `segment_base_of_ptr` recompute (~9.03 Ir) +
`contains_base` re-run (~8.2 Ir Tier-1 hit) per move-leg dealloc, amortised
over the steps that actually take the move leg (not all 16 do — same-class
doublings and OPT-G in-place grows short-circuit before the closing dealloc).
The kill-gates being byte-exact confirms no codegen perturbation anywhere
outside the realloc path.

## 4. Verdict

**The already-published numbers reproduce exactly.** This report does not
re-litigate R32-3's decision (the change is already shipped and `production`'s
default); it exists to make the commit-message-only measurement reproducible
from committed artifacts (raw logs + checked derive script + summary CSV),
closing the R22-14-boundary-rule inconsistency F10 flagged. R32-3's own
correctness argument (the `base` is still ours & live at the closing dealloc)
lives in its commit message and is unchanged; the measurement here confirms the
Ir saving it claimed.
