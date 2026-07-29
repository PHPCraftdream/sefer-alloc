# R28-1 — `flush_class` isolation: the magazine-overflow free path's larger non-isolable remainder, finally isolated

**Task #430 (R28-1), Round 28.** MEASUREMENT-ONLY, per this project's
"measured, not spun" convention. `docs/perf/OPEN_ITEMS.md` item 1's "Next
trigger" bullet has, since R24-2, named `flush_class` isolation as the
overflow region's larger untried lever — the ~487 Ir "non-isolable
remainder" R24-2 §5.1 flagged (`flush_class(8 blocks)` + the 8-pointer
compaction shift + the final magazine push, fused in one straight-line block
with "no workload-level separation point" at the time). Four consecutive
optimization attempts in the immediate overflow region (R24-3, R24-4, R25-3,
R26-7) have all been NO-GO. This task answers the open "Next trigger"
question with a real number instead of leaving it an estimate — **it does
NOT attempt a 5th optimization**.

**Date:** 2026-07-29. **Base revision measured:** `main` @
`66282c36415f750c16ec3f279e1abc430db3997c` (working tree carrying only this
task's own additive edits at measurement time — `git status --short` shows
only `README.md`, `benches/perf_gate_iai.rs`,
`src/registry/heap_core_diag.rs` modified, plus new files under
`docs/perf/`/`docs/checkpoints/`/`docs/reviews/` from other in-flight work,
none of which this task touches). **Platform measured:** WSL2 (Ubuntu,
kernel `6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro x86-64,
`valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc
`1.98.0-nightly (bd08c9e71 2026-06-25)`, CPU `11th Gen Intel(R) Core(TM)
i7-11800H @ 2.30GHz (8C/16T)` — same toolchain/host as every other
`npm run iai` measurement in this doc tree.

**Measurement only. No production behavior changed:** one new
`bench-internals`-gated `unsafe fn` hook
(`HeapCore::dbg_flush_class_only`, `src/registry/heap_core_diag.rs`) that
calls the EXISTING `AllocCore::flush_class` verbatim, and two new bench
arms. No production call site touched, no existing function body edited.

---

## 0. Headline

| question | answer |
|---|---|
| Is `flush_class(8 blocks)` cleanly isolable now? | **YES** — via a new `bench-internals`-gated `unsafe fn` hook that calls it standalone, following the R24-2/R25-1 precedent exactly, `bench-internals`-gated **from creation** (not retrofitted). |
| `flush_class(8 blocks)`'s own isolated Ir? | **449 Ir** (`Ir(dealloc_flush_class_only_16b) − Ir(dealloc_flush_class_only_16b_prefix)` = 4,338 − 3,889). Two independent `npm run iai` runs, byte-identical. |
| Per-block cost? | **56.1 Ir/block** (449 / 8). |
| How does this reconcile with R24-2's ~487/~470 Ir remainder estimate? | Reconciles closely. Re-measured today, one overflow event (`n17 − n16`) is **581 Ir** (vs R24-2's 571 — see §3 for the ~10 Ir drift explanation); subtracting the historical 84 Ir bitmap-clear cost (mechanism unchanged, hook removed R27-10 but the loop it measured still runs unmodified inline) gives a **497 Ir** remainder, of which `flush_class` alone is now measured at **449 Ir (90.3%)** and compaction + final push is the residual **~48 Ir (9.7%)**. |
| Actionable for a 5th optimization attempt? | **NO — the honest read is this region is exhausted for further micro-optimization at this task's scope.** See §5 for the reasoning. This measurement closes out the "Next trigger" question; it is not a green light for another attempt. |

---

## 1. What `flush_class` actually does today (read first, per the task brief)

Re-read `AllocCore::flush_class` / `AllocCore::flush_run`
(`src/alloc_core/alloc_core_small_magazine.rs:491-696`) in full — code has
moved since R24-2 (now in this file rather than being described only by line
numbers in `heap_core_free.rs`), but the algorithm is unchanged from R24-2's
description:

1. **`flush_class`** (lines 491-570): groups the input `blocks` slice into
   RUNS of consecutive same-segment pointers (Э8 batching, one
   `segment_base_of_ptr` compare per block, no sorting), and for each run
   calls `flush_run`, skipping any run whose `base` was ALREADY recycled by
   an earlier run within the same call (the L-4/UBFIX-11 double-free-across-
   runs guard, `recycled_bases`/`recycled_n`, a fixed 16-entry array — `AllocCore`
   allocates no `Vec`/`Box`, M5).
2. **`flush_run`** (lines 586-695): for ONE same-`base` run, hoists
   `SegmentMeta`/`BinTable`/`AllocBitmap`/`bump`/`kind`/`payload_start`
   ONCE, then per block: two guards (`off < payload_start` in-metadata
   reject; `off >= bump` decommit-stale-free reject), a third M2 guard
   (`bm.is_free(off)`), then `Node::write_next` + `bm.mark_free(off)`,
   splicing the run onto the segment's freelist exactly as N sequential
   `dealloc_small` calls would (proven byte-identical in the doc comment).
   One `set_head` at the end of the run. Under `alloc-decommit`: a single
   batched `dec_live_and_maybe_decommit` call (`accepted_count` tracked
   per-run) — at most one decommit fires, and `release_or_pool_empty_segment`
   runs if it does.

The magazine-overflow call site is unchanged in shape (now in
`src/registry/heap_core_free.rs`'s `dealloc_own_thread_with_base`, lines
~744-816 as of this measurement): the bitmap-clear pass over
`slots[0..FLUSH_N]` (now inlined at the call site — `dbg_overflow_bitmap_clear_pass`
itself was removed R27-10, but the loop body it measured still runs,
unmodified, directly in `dealloc_own_thread_with_base`), then
`self.core.flush_class(c, &self.tcache.classes[c].slots[0..FLUSH_N])`, then
the 8-pointer compaction shift, then the final magazine push for the
just-freed block. `FLUSH_N = TCACHE_CAP / 2 = 8` (`src/registry/tcache.rs:124`),
unchanged.

---

## 2. Isolation design

Same shared-prefix-subtraction pattern this project's iai gate uses
throughout (`Ir(arm_with_extra_call) − Ir(baseline_arm)` isolates the extra
call's cost, e.g. R22-17/R23-1's `contains_base` isolation, R24-2's
cheap-push/overflow pair isolation).

**New hook — `HeapCore::dbg_flush_class_only`**
(`src/registry/heap_core_diag.rs`), the minimum necessary. `#[doc(hidden)]`,
`#[cfg(all(feature = "alloc-global", feature = "fastbin", feature =
"bench-internals"))]`, `#[inline(always)]`, **`pub unsafe fn`** with a
documented `# Safety` contract mirroring `AllocCore::flush_class`'s own
verbatim — `bench-internals`-gated from the moment it was created, per
CLAUDE.md's benchmark-hook rule (the rule the R25-1 fix for
`dbg_overflow_bitmap_clear_pass` prompted — this hook follows the positive
`dbg_dealloc_own_thread_with_base` pattern exactly, not the R24-2-era safe-`pub
fn` mistake). It does exactly one thing: `unsafe { self.core.flush_class(class_idx,
blocks) }` — no alternate/bypass implementation, the real production
function called standalone.

**Why `flush_class` is safely callable standalone here (the "Heisenberg
risk" R24-2 §5.1 flagged, addressed):** `flush_class`'s only precondition is
that every entry of `blocks` is a currently-LIVE, this-heap-owned allocation
of the given `class_idx`, freed at most once by the call. It does NOT
require the blocks to currently be magazine-resident, or the surrounding
`HeapCore.tcache` state to be in any particular shape — `flush_class` never
reads `self.tcache` (it operates purely on `AllocCore`, which has no
magazine concept; see `AllocCore::flush_class`'s own doc, "matches per-block
`dealloc_small` path"). So the bench arms below construct `blocks` as 8
freshly-allocated, still-live pointers (never pushed into the magazine at
all) rather than trying to reproduce the exact in-magazine residency state —
this is a **legitimate use of the real function**, not an invented
mechanism, because `flush_class`'s contract only cares about liveness/class/
once-only, never about magazine membership.

**Two new bench arms** (`benches/perf_gate_iai.rs`), gated
`#[cfg(all(target_os = "linux", feature = "alloc-global", feature =
"alloc-xthread", feature = "fastbin", feature = "bench-internals"))]`:

| arm | isolates | technique |
|---|---|---|
| `dealloc_flush_class_only_16b_prefix` | shared setup only (8 cheap alloc+free pushes to warm the allocator into the same state, then 8 more fresh allocs left LIVE — the `flush_class` input) | prefix (never calls the hook) |
| `dealloc_flush_class_only_16b` | prefix + one standalone `flush_class(class_idx, &flush_input)` call on the 8 live blocks | shared-prefix vs the prefix arm |

`Ir(dealloc_flush_class_only_16b) − Ir(dealloc_flush_class_only_16b_prefix)`
isolates `flush_class`'s own cost on 8 blocks — no bitmap-clear pass, no
compaction shift, no final push mixed in, unlike R24-2's fused overflow-arm
body measurement.

---

## 3. Results — real, deterministic `npm run iai` numbers (two independent runs, byte-identical `Ir`)

Raw evidence (both runs full stdout; 611 lines each, no truncation needed):

- `docs/perf/_raw_r28_1_run1.log`
- `docs/perf/_raw_r28_1_run2.log`

Both runs: **71 benches** (69 pre-existing + 2 new), byte-identical `Ir` for
every row (confirmed via `diff` of the extracted `Instructions:` lines across
both full-suite runs — zero differences). Reference arms reproduced their
expected shapes from R24-2/R23-3, with one small drift noted below.

### 3.1 Raw Ir table (new arms + the rows they derive against)

| bench | raw Ir | role |
|---|---:|---|
| `dealloc_flush_class_only_16b_prefix` (new) | 3,889 | prefix (setup only, `flush_class` never called) |
| `dealloc_flush_class_only_16b` (new) | 4,338 | prefix + one standalone `flush_class(8 blocks)` call |
| `dealloc_free_only_16b_n16` (R24-2, re-measured) | 7,711 | sweep N=16 (16 cheap pushes, no overflow) |
| `dealloc_free_only_16b_n17` (R24-2, re-measured) | 8,292 | sweep N=17 (1 overflow event) |
| `dealloc_own_thread_body_only_16b` (R23-3, re-measured) | 12,422 | own-thread body reconciliation ref |

### 3.2 Measurement — `flush_class(8 blocks)` standalone

| derivation | Ir | scope |
|---|---:|---|
| `Ir(dealloc_flush_class_only_16b) − Ir(dealloc_flush_class_only_16b_prefix)` = 4,338 − 3,889 | **449** | standalone `flush_class` call, 8 blocks |
| per block | 449 / 8 | **56.1 Ir/block** |

**`flush_class(8 blocks)`'s own isolated cost is 449 Ir — 56.1 Ir/block.**
Two independent `npm run iai` runs (full 71-bench suite each) produced
byte-identical `Ir` for both new arms.

### 3.3 Reconciliation with R24-2's ~487/~470 Ir remainder estimate

R24-2 measured (at its own base revision `14a86ce`): one overflow event
(`n17 − n16`) = 571 Ir; bitmap-clear pass (via the now-removed
`dbg_overflow_bitmap_clear_pass` hook) = 84 Ir; non-isolable remainder
(`flush_class` + compaction + final push) = 571 − 84 = **487 Ir** (the
report's §4.4 rounds this to "~470" in its §6 prose — the exact §4.4 table
value is 469.8, i.e. ≈470; the OPEN_ITEMS.md card cites "~487 Ir" for the
same quantity — both refer to the identical 571 − 84 subtraction, the
~470/~487 difference in the two citations is 553.8-vs-571 SCOPE B/A
rounding, not two different measurements).

**Re-measured today** (base revision `66282c3`, ~5 rounds and many unrelated
commits later): `n17 − n16` = 8,292 − 7,711 = **581 Ir**, ~10 Ir (1.8%) above
R24-2's 571. This is normal cross-round drift from unrelated code changes
touching the same hot functions over 5 rounds (compiler codegen shifts from
nearby edits, not a regression in this overflow arm specifically — no commit
in this window targeted the overflow arm itself per `git log`). The
bitmap-clear pass's mechanism is UNCHANGED since R24-2 (the loop body
`dbg_overflow_bitmap_clear_pass` used to measure is still present, just
inlined directly at the call site rather than behind a separate hook after
R27-10's removal) — its 84 Ir figure is taken as still representative (no
hook remains to re-isolate it without resurrecting the exact hook this task
was explicitly told not to resurrect).

Using today's 581 Ir overflow-event total and the historical 84 Ir
bitmap-clear:

```
remainder (flush_class + compaction + push) = 581 − 84 = 497 Ir
flush_class alone (measured this task)      = 449 Ir
compaction + final push (derived residual)  = 497 − 449 = 48 Ir
```

**This reconciles closely with R24-2's estimate**: 497 Ir vs R24-2's 487 Ir
(2.1% apart, within the ~10 Ir cross-round drift already noted). The new
data refines the picture R24-2 could only bound as one fused quantity:
**`flush_class` alone is 449 Ir — 90.3% of the previously-fused remainder —
and compaction + the final push together cost only ~48 Ir (9.7%).**

---

## 4. What this decomposition means for the overflow event as a whole

Full decomposition of one overflow event (581 Ir total, today's measurement):

| sub-cost | Ir | % of overflow event |
|---|---:|---:|
| bitmap-clear pass (8 blocks, historical R24-2 figure, mechanism unchanged) | 84 | 14.5% |
| **`flush_class` (8 blocks) — isolated this task** | **449** | **77.3%** |
| compaction shift + final push (derived residual) | 48 | 8.3% |
| **total** | **581** | **100%** |

`flush_class` is now confirmed as the single largest sub-cost inside one
overflow event — more than 5× the bitmap-clear pass, and roughly 9× the
compaction+push residual. At 56.1 Ir/block for 8 blocks, and given
`flush_run`'s per-block work (payload-bound guard, stale-free guard, M2
`is_free` guard, `Node::write_next`, `bm.mark_free`, plus the once-per-run
hoisted metadata reads and the batched `dec_live_and_maybe_decommit`), this
is dominated by real per-block bookkeeping that R24-2's own §6 already
anticipated ("dominated by `mark_free` + `dec_live` + decommit-check per
block") — not a single large avoidable cost, but ~56 Ir spread across
several small, individually-necessary per-block operations plus the
once-per-run metadata setup amortized over 8 blocks.

---

## 5. Verdict: honest read is this region is exhausted at this task's scope — NOT a green light for a 5th attempt

**This measurement's value is closing out the "Next trigger" question, not
opening a 5th optimization attempt.** Reasoning:

1. **The per-block work `flush_run` does is already minimal and mostly
   necessary.** Reading §1's mechanism: two cheap guards (bounds compares),
   one M2 correctness guard (double-free detection — not optional, cutting
   it reintroduces the exact double-free class M2 exists to prevent), one
   freelist link write, one bitmap bit set. The once-per-run metadata hoist
   (`SegmentMeta`/`BinTable`/bitmap views, `bump`, `kind`, `payload_start`)
   is ALREADY amortized across the whole run (Э8 batching) — R24-2's own
   report and this file's doc comments show this was already optimized
   relative to N independent `dealloc_small` calls. There is no un-hoisted
   per-block metadata read left to hoist further.
2. **Four consecutive NO-GOs already probed adjacent levers in this exact
   region** (R24-3 bitmap-clear/flush_class fusion, R24-4 bulk-mask
   primitives, R25-3 FLUSH_N tuning, R26-7 lazy staging array) — all found
   that added per-block bookkeeping or restructuring costs more than the
   savings it enables, or that the savings are dominated by unrelated
   overhead the moment a realistic workload shape is used. `flush_class`
   itself was the ONE lever those four attempts did not directly touch
   (R24-3 touched the bitmap-clear pass adjacent to it; none touched
   `flush_run`'s own per-block guard/write sequence), so this measurement
   was the legitimate remaining gap — but the isolated number (56.1 Ir over
   ~8-10 real memory operations per block) does not suggest a large
   avoidable constant the way, e.g., STAGE_CAP's 512-byte eager zero-init
   did (R24-8's actionable 4,065 Ir GO).
3. **The compaction + final push residual (~48 Ir) is now measured as
   small** — not a hidden larger target hiding behind `flush_class`. There
   is no "the real cost was actually in compaction, not flush_class"
   surprise to chase.
4. **A hypothetical `flush_class` restructuring would need to beat ~56
   Ir/block of guard+write+bitmap work without breaking M2 double-free
   safety or the Э8 same-segment run-batching invariant** — the same class
   of tradeoff R24-3/R24-4/R25-3/R26-7 already tested and found NO-GO for
   nearby code in this exact function family.

**Recommendation: do not open a 5th optimization attempt in this immediate
overflow-arm region based on this measurement alone.** If a future round
wants to revisit magazine-overflow cost, the more promising angle (not
explored by this task, out of scope here) would be reducing HOW OFTEN
overflow fires (workload-shape-dependent, `FLUSH_N`/`TCACHE_CAP` sizing —
already swept NO-GO in R25-3) or eliminating overflow's fixed
bitmap-clear+flush+compact+push sequence structurally (a different kind of
redesign than incremental per-block tuning) — not another attempt to shave
`flush_class`'s own per-block cost, which this task's data suggests is
already close to the minimum a correctness-preserving implementation can
do.

---

## 6. Verification performed

- **Read the mechanism FIRST** (§1): `flush_class`/`flush_run`'s current
  implementation, confirmed unchanged in algorithm from R24-2's description
  (only file location moved).
- **Chose the isolation technique per the established pattern** (§2):
  shared-prefix subtraction (one operation difference — the hook call),
  matching R24-2/R23-1's precedent exactly.
- **New hook is `pub unsafe fn` + `bench-internals`-gated from creation**,
  not retrofitted — verified via `git log -p` on this task's own diff (no
  intermediate safe-`pub-fn` state ever existed in this task's commits).
- **Two independent `npm run iai` runs** (71 benches each, `production
  bench-internals`, matching the bench target's `required-features`) —
  byte-identical `Ir` for every bench including both new arms, confirmed via
  a full-file `diff` of extracted `Instructions:` lines.
- **`cargo test --features production`** — full suite green after fixing the
  README unsafe-inventory drift the new tier-2 site introduced (see §7).
- **`cargo clippy --all-targets --features production,bench-internals -- -D
  warnings`** — clean.
- **`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/ | wc -l`** = 62,
  matching the updated README figure (61 → 62, one new tier-2 item-scoped
  site).
- **No production behavior changed**: the one new `src/` item is a
  `#[doc(hidden)]`, `bench-internals`-gated `unsafe fn` thin wrapper calling
  an EXISTING production function verbatim — no production call site
  touched, no existing function body edited. `production`'s own feature
  composition confirmed unchanged (`grep -n "^production = "
  Cargo.toml` untouched by this diff).

---

## 7. Files touched

- `src/registry/heap_core_diag.rs` — added `HeapCore::dbg_flush_class_only`
  (`pub unsafe fn`, `#[doc(hidden)]`, `#[cfg(all(feature = "alloc-global",
  feature = "fastbin", feature = "bench-internals"))]`, `#[inline(always)]`,
  `bench-internals`-gated from creation). **One new hook — the only one this
  task added.**
- `benches/perf_gate_iai.rs` — added `dealloc_flush_class_only_16b_prefix`,
  `dealloc_flush_class_only_16b`; registered both in the `perf_gate`
  `library_benchmark_group!` list (71 benches total, up from 69). Zero
  changes to any pre-existing bench fn's body.
- `README.md` — updated the tier-2 unsafe-inventory table
  (`heap_core_diag.rs`'s row: 3 → 4 sites) and the total count (61 → 62
  tier-2 item-scoped allows), plus the R24-6/R25-1 note extended to mention
  this task's hook as the positive `bench-internals`-from-creation precedent
  (required by `tests/no_stale_doc_references.rs`'s
  `readme_unsafe_inventory_counts_match_reality` drift check, which caught
  this and failed the pre-push gate until fixed).
- `docs/perf/R28_1_FLUSH_CLASS_ISOLATION_GATE.md` — this report.
- `docs/perf/R28_1_FLUSH_CLASS_ISOLATION_GATE_summary.csv` — companion
  machine-readable summary.
- `docs/perf/_raw_r28_1_run1.log` / `_raw_r28_1_run2.log` — full raw
  `npm run iai` stdout for the two independent, byte-identical-`Ir` runs
  cited in §3. `git add -f` needed (`.gitignore` excludes
  `docs/perf/_raw_*.log` by default, R13-10/task #280).
- `docs/perf/OPEN_ITEMS.md` — item 1's Current-state card updated with this
  task's number and verdict (see the dated paragraph added below the card).
- `Cargo.toml` — **untouched**.

**Files needing `git add -f`** (gitignored by `.gitignore`,
`/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r28_1_run1.log`
- `docs/perf/_raw_r28_1_run2.log`
