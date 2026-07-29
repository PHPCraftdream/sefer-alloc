# R29-10 — alloc-hit `clear_magazine` isolation: the magazine-hit fast path's per-pop residency-bit clear, finally isolated

**Task #441 (R29-10), Round 29.** MEASUREMENT-ONLY, per this project's
"measured, not spun" convention. `docs/perf/IAI_BASELINE.md`'s R3 honest-reject
(2026-07-13) rejected DEFERRING `MagazineBitmap::clear_magazine` off the alloc
issue path for correctness reasons (both the own-thread free oracle and
`AllocCore::reclaim_offset_checked` need the bit exact at the ISSUE moment),
but its own text admitted *"No iai baseline was taken; there is nothing to
measure"* — the COST itself was never isolated. Five rounds of FREE-side
decomposition exist for the magazine-overflow region (R24-2 → R28-1), but ZERO
measurement existed for this ALLOC-side sub-mechanism, despite alloc and free
executing equally often in any real workload. This task isolates it. **It does
NOT attempt a Stage-2 optimization** — per this task's own anti-p-hacking guard,
any such follow-up must be an in-context A/B on `small_churn_16b`, never a
standalone-hook extrapolation.

**Date:** 2026-07-29. **Base revision measured:** `main` @
`e4576d933547e4f5976396f71d58016d011d79cb` (working tree carrying only this
task's own additive edits at measurement time — `git status --short` shows
only `README.md`, `benches/perf_gate_iai.rs`, `src/registry/heap_core_diag.rs`
modified, plus new files under `docs/perf/` from this task). **Platform
measured:** WSL2 (Ubuntu, kernel `6.18.33.2-microsoft-standard-WSL2`) under
Windows 10 Pro x86-64, `valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL
rustc `1.98.0-nightly (bd08c9e71 2026-06-25)`, CPU `11th Gen Intel(R) Core(TM)
i7-11800H @ 2.30GHz (8C/16T)` — same toolchain/host as every other
`npm run iai` measurement in this doc tree.

**Measurement only. No production behavior changed:** one new
`bench-internals`-gated `unsafe fn` hook
(`HeapCore::dbg_clear_magazine_on_hit`, `src/registry/heap_core_diag.rs`) that
inlines the production magazine-hit clear block byte-for-byte, and two new
bench arms. No production call site touched, no existing function body edited.

---

## 0. Headline

| question | answer |
|---|---|
| Is the alloc-hit `clear_magazine` block cleanly isolable now? | **YES** — via a new `bench-internals`-gated `unsafe fn` hook that runs the exact 3-line production block standalone, following the R28-1 precedent (`dbg_flush_class_only`) exactly, `bench-internals`-gated **from creation** (not retrofitted). |
| The block's own isolated Ir? | **195 Ir over 16 hits = 12.19 Ir/hit** (`Ir(alloc_clear_magazine_only_16b) − Ir(alloc_clear_magazine_only_16b_prefix)` = 4,477 − 4,282). Two independent `npm run iai` runs (full 75-bench suite each), byte-identical `Ir`. |
| Decomposition? | **~9.03 Ir `segment_base_of_ptr`** (R23-1's isolated figure, the same function called here) + **~3.16 Ir** for `SegmentMeta::new` + `magazine_bitmap()` + the `clear_magazine` bitmap RMW (12.19 − 9.03). |
| As a fraction of a magazine hit? | **54.5%** of the magazine-hit pop (12.19 / 22.375; the hit itself reproduced at 22.4 Ir/op, R23-3's figure, exactly — see §3.3). |
| Reconciliation with the reviewer's "~7 Ir" estimate? | The estimate was explicitly flagged UNVERIFIED and inconsistent with itself (`segment_base_of_ptr` alone is 9.03 Ir, so a combined figure could not be below 9). The real number (12.19) is ~1.7× the estimate but the same order of magnitude. Not treated as a target to hit or confirm, per the task's anti-p-hacking guard. |
| Actionable for a Stage-2 optimization? | **NO — close permanently.** The clear is correctness-required (R3 NO-GO on deferral), the dominant sub-cost (`segment_base_of_ptr`, 9 of the 12 Ir) is already R22-17's open-but-soundness-blocked item, and the residual bitmap RMW (~3 Ir) is the same class R24-3/R24-4 proved NO-GO to coalesce. See §5. |

---

## 1. What the alloc-hit `clear_magazine` block actually does (read first, per the task brief)

Re-read the production magazine-hit fast path
(`src/registry/heap_core_alloc.rs`, the RAD-5 E4 block) in full — the exact
block that runs on EVERY magazine hit under plain `production`
(`alloc-global + fastbin`):

```text
// src/registry/heap_core_alloc.rs (the RAD-5 E4 clear, runs on every pop):
{
    let base = os::segment_base_of_ptr(issued);
    let off = (issued as usize - base as usize) as u32;
    SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
}
```

`issued` is the pointer just popped from
`self.tcache.classes[c].slots[new_cnt]`. The block: (1) re-derives the
segment-aligned base from `issued` via a bitmask (`segment_base_of_ptr`, the
same ~9 Ir function R23-1 isolated on the FREE side), (2) computes the
segment-relative offset, (3) constructs a fresh `MagazineBitmap` view over the
segment header's residency bitmap and clears `issued`'s bit (a byte load +
AND-with-mask + store — `SegmentBitmap::clear`).

R3's honest-reject established this clear is **load-bearing at two sites** and
cannot be deferred to refill/flush time without (a) making a legitimately-freed
refilled block read a stale `1` → the own-thread free oracle treats it as an
in-magazine double-free → the block is **leaked**, and (b) making a genuine
remote-free note look like the duplicate leg of a cross-thread double-free in
`reclaim_offset_checked` → **dropped/leaked**. So the clear MUST happen at
issue time. R3's only gap was that it never measured the cost. This task does.

`alloc_magazine_hit_only_16b` (R23-3) already isolates the WHOLE magazine-hit
pop at 22.4 Ir/op — but that figure fuses this `clear_magazine` block with the
rest of the hit (count decrement, slot read, `virgin_mask` AND under
`virgin-zero-skip`, the `hardened` gen bump, the return). This task isolates
JUST the 3-line clear block.

---

## 2. Isolation design

Same shared-prefix-subtraction pattern this project's iai gate uses throughout
(`Ir(arm_with_extra_call) − Ir(baseline_arm)` isolates the extra call's cost —
R22-17/R23-1's `contains_base`/`segment_base_of_ptr` isolation, R24-2's
cheap-push/overflow pair, R28-1's `flush_class` isolation). R28-1
(`docs/perf/R28_1_FLUSH_CLASS_ISOLATION_GATE.md`) is the DIRECT methodological
template — this task mirrors it almost exactly in shape (it is the alloc-side
sibling of R28-1's free-side `flush_class` isolation).

**New hook — `HeapCore::dbg_clear_magazine_on_hit`**
(`src/registry/heap_core_diag.rs`), the minimum necessary. `#[doc(hidden)]`,
`#[cfg(all(feature = "alloc-global", feature = "fastbin", feature =
"bench-internals"))]`, `#[inline(always)]`, **`pub unsafe fn`** with a
documented `# Safety` contract — `bench-internals`-gated from the moment it was
created, per CLAUDE.md's benchmark-hook rule (the rule the R25-1 fix for
`dbg_overflow_bitmap_clear_pass` prompted — this hook follows the positive
`dbg_dealloc_own_thread_with_base` / `dbg_flush_class_only` pattern exactly,
not the R24-2-era safe-`pub-fn`-then-retrofit mistake). Unlike `dbg_flush_class_only`
(which delegates to one callable production function), this hook INLINES the
production block byte-for-byte: in production the three lines are straight-line
code inside the magazine-hit branch, not a callable function, so the faithful
isolation is an exact textual copy of that block (no alternate/bypass
implementation, no extra bookkeeping).

**Why this is `unsafe fn` despite composing only safe `pub(crate)` primitives:**
`segment_base_of_ptr` / `SegmentMeta::new` / `magazine_bitmap` / `clear_magazine`
are each individually safe fns, but their COMBINATION derives an unchecked
metadata write from a caller raw pointer — `segment_base_of_ptr(issued)` masks
an arbitrary pointer to a segment-aligned address and `clear_magazine` then
writes the residency bitmap at that derived base with zero validation beyond
the pointer's own alignment. A foreign/null/interior `issued` makes the derived
`base` unmapped (crash) or alias an unrelated segment's bitmap (silent
corruption). This is the exact shape CLAUDE.md's benchmark-hook rule (R25-1)
requires to be `pub unsafe fn` with a documented `# Safety` contract rather
than a safe `pub fn` — the original `dbg_overflow_bitmap_clear_pass` bug was a
safe `pub fn` doing precisely this.

**Two new bench arms** (`benches/perf_gate_iai.rs`), gated
`#[cfg(all(target_os = "linux", feature = "alloc-global", feature =
"alloc-xthread", feature = "fastbin", feature = "bench-internals"))]` — the
`bench-internals` (and `fastbin`/`alloc-global`) is on the arm's OWN
`#[cfg(all(...))]`, not just the target-level `required-features`, matching the
established per-arm defensive pattern (R29-3's first attempt this round missed
this exact thing — see commit `db35617`):

| arm | isolates | technique |
|---|---|---|
| `alloc_clear_magazine_only_16b_prefix` | shared setup only (16 allocs + 16 frees → 16 magazine-resident blocks whose residency bits are SET via the own-thread free push's `mark_magazine`; no overflow since `TCACHE_CAP == MAGAZINE_FILL == 16`) | prefix (never calls the hook) |
| `alloc_clear_magazine_only_16b` | prefix + one `dbg_clear_magazine_on_hit(ptr)` call per resident block (16 calls) | shared-prefix vs the prefix arm |

`Ir(alloc_clear_magazine_only_16b) − Ir(alloc_clear_magazine_only_16b_prefix)`
isolates 16× the clear-block's combined cost; ÷16 gives the per-hit cost. The
blocks' bits ARE set at call time (the frees just set them via `mark_magazine`),
so this faithfully reproduces the real magazine-hit scenario (a resident block
whose bit gets cleared on pop) — clearing an already-clear bit would be the same
instruction count (the RMW is branchless), but the set-bit scenario is the
faithful one and is what this setup constructs.

---

## 3. Results — real, deterministic `npm run iai` numbers (two independent runs, byte-identical `Ir`)

Raw evidence (both runs full 75-bench stdout):

- `docs/perf/_raw_r29_10_run1.log` (739 lines — the baseline run; `N/A` comparison column)
- `docs/perf/_raw_r29_10_run2.log` (643 lines — the comparison run; `No change` on every row)

Both runs: **75 benches** (73 pre-existing + 2 new), byte-identical `Ir` for
the two new arms (4,282 / 4,477 in both — run2 reports `No change` against
run1's baseline for every bench). Reference arms reproduced their expected
shapes from R23-1/R23-3/R28-1 exactly (see §3.3).

### 3.1 Raw Ir table (new arms + the rows they derive against)

| bench | raw Ir | role |
|---|---:|---|
| `alloc_clear_magazine_only_16b_prefix` (new) | 4,282 | prefix (setup only, hook never called) |
| `alloc_clear_magazine_only_16b` (new) | 4,477 | prefix + 16 standalone clear-block calls |
| `alloc_magazine_prefill_only_16b` (R23-3, re-measured) | 7,450 | magazine-hit shared-prefix ref |
| `alloc_magazine_hit_only_16b` (R23-3, re-measured) | 7,808 | magazine-hit ref (prefill + 16 hits) |
| `dealloc_segment_base_of_ptr_probe_only_16b` (R23-1, re-measured) | 7,581 | `segment_base_of_ptr` isolation ref |
| `dealloc_prealloc_only_16b` (R23-1, re-measured) | 7,003 | `segment_base_of_ptr` shared-prefix ref |
| `dealloc_flush_class_only_16b_prefix` (R28-1, re-measured) | 3,889 | R28-1 ref |
| `dealloc_flush_class_only_16b` (R28-1, re-measured) | 4,338 | R28-1 ref (4,338 − 3,889 = 449) |

### 3.2 Measurement — the alloc-hit `clear_magazine` block

| derivation | Ir | scope |
|---|---:|---|
| `Ir(alloc_clear_magazine_only_16b) − Ir(alloc_clear_magazine_only_16b_prefix)` = 4,477 − 4,282 | **195** | 16 standalone clear-block calls |
| per hit | 195 / 16 | **12.19 Ir/hit** |

**The alloc-hit `clear_magazine` block's own isolated cost is 195 Ir / 16 hits =
12.19 Ir/hit.** Two independent `npm run iai` runs (full 75-bench suite each)
produced byte-identical `Ir` for both new arms.

### 3.3 Decomposition + reconciliation with R23-1 / R23-3

**Decomposition of the 12.19 Ir/hit:**

| sub-cost | Ir | source |
|---|---:|---|
| `segment_base_of_ptr` (the bitmask base re-derivation) | ~9.03 | R23-1's isolated figure for the SAME function (`(7,581 − 7,003) / 64` = 578/64 = 9.03 Ir/call); reproduced this run at 7,581/7,003 byte-identical |
| `SegmentMeta::new` + `magazine_bitmap()` + `clear_magazine` RMW (load + AND + store) | ~3.16 | derived residual (12.19 − 9.03) |
| **total clear block** | **12.19** | measured this task |

(The 9.03 is R23-1's free-side isolation of the identical function; the
~0.16 Ir gap vs a clean subtraction is loop-overhead/black-box parity noise
between the 16-iter and 64-iter probe loops — R23-1's probe carries a per-call
`black_box` this arm does not, so the residual is an upper bound on the clear
proper, not a separate anomaly.)

**Reconciliation with R23-3's magazine-hit figure:** the magazine-hit pop
itself (`alloc_magazine_hit_only_16b − alloc_magazine_prefill_only_16b` =
7,808 − 7,450 = 358 / 16 = **22.375 ≈ 22.4 Ir/op**) reproduces R23-3's
published 22.4 Ir/op exactly (5 rounds later, byte-identical). The clear block
is therefore **12.19 / 22.375 = 54.5% of a magazine hit** — a non-trivial
fraction, dominated by `segment_base_of_ptr` (the 9.03 Ir is ~74% of the
block's own 12.19 Ir, and ~40% of the entire 22.4 Ir hit).

---

## 4. What this number means

The clear block is a real, per-hit cost on the alloc fast path — every
magazine-resident pop pays 12.19 Ir, of which ~9 Ir is the `segment_base_of_ptr`
base re-derivation and ~3 Ir is the bitmap RMW. As 54.5% of the magazine hit,
it is the single largest sub-cost inside the hit (the rest — count decrement,
slot array read, `virgin_mask` AND under `virgin-zero-skip`, the `hardened` gen
bump, the return — is the other ~45%).

But the cost is **structurally fixed**, not avoidable:

1. **The clear itself cannot be deferred** — R3's honest-reject proved this
   conclusively: deferring makes the bit stale at the two load-bearing sites
   (own-thread free oracle, `reclaim_offset_checked`), leaking blocks in both
   cases. This is not a tunable tradeoff; it is a correctness invariant.
2. **The bitmap RMW (~3 Ir) is already minimal** — a branchless load + AND +
   store. R24-3 (`flush_magazine_class` merge) and R24-4 (bulk-mask
   `clear_many`/`set_many` primitives) both proved that coalescing per-offset
   bitmap RMWs into bulk primitives is a NO-GO in this codebase (the bulk
   primitive's own per-offset bookkeeping costs more than the hot-cache-line
   RMW it coalesces — the same Heisenberg class twice).
3. **The dominant sub-cost, `segment_base_of_ptr` (~9 Ir), is the SAME function
   the FREE path already pays** and is already the subject of R22-17's
   header-first-design open item — whose own conclusion was that no safe way
   was found to read a header before proving liveness without some other
   liveness proof that itself costs something. See §5.

---

## 5. Verdict: close permanently — no standalone follow-up warranted

**This measurement's value is closing R3's "never isolated" gap with a real
number, not opening an optimization.** Reasoning:

1. **R3 already settled the only obvious lever (deferral) as correctness-NO-GO.**
   The clear MUST run at issue time; the 12.19 Ir is a fixed per-hit cost, not
   a tunable one. There is no "batch it" or "move it" option left — R3 examined
   both and rejected them.
2. **The dominant sub-cost is not clear-magazine-specific.** 9 of the 12 Ir is
   `segment_base_of_ptr`, which is (a) already isolated and tracked as R22-17's
   header-first-design open item with a documented soundness blocker, and (b)
   paid equally on the free side. A standalone "clear_magazine optimization"
   would really be a `segment_base_of_ptr` optimization — which belongs to
   R22-17's existing item, not a new one here.
3. **The residual (~3 Ir) is the same bitmap-RMW class R24-3/R24-4 already
   proved NO-GO to coalesce.** Re-attempting it in the alloc-hit context would
   repeat a twice-rejected tradeoff in a near-identical mechanism.
4. **A theoretical lever exists but is speculative and out of scope here:**
   caching the segment base ALONGSIDE the slot pointer in the tcache
   (`(ptr, base)` pairs instead of bare `ptr`) would eliminate the per-hit
   `segment_base_of_ptr` re-derivation — but it doubles the magazine's per-slot
   footprint (cache-density risk) and, per this project's own R26-7 "~10×
   Heisenberg gap" lesson, any such attempt MUST be an in-context A/B on
   `small_churn_16b` (a real workload-embedded measurement), never a
   standalone-hook extrapolation. That is explicitly a SEPARATE follow-up task
   (NOT opened by this task) and is not clearly warranted given (2) above.

**Recommendation: close this item permanently.** The measured 12.19 Ir/hit is
small in absolute terms (vs 449 Ir `flush_class`, 571 Ir overflow, 4,065 Ir
STAGE_CAP), the clear is correctness-required, and the only theoretical lever
overlaps an existing open item (R22-17). R3's honest-reject now stands with a
measured cost attached — strictly more than R3 achieved ("nothing to measure").

---

## 6. Verification performed

- **Read the mechanism FIRST** (§1): the production magazine-hit clear block at
  `src/registry/heap_core_alloc.rs`, confirmed unchanged (the RAD-5 E4 block,
  lines ~222–236 as of this measurement).
- **Chose the isolation technique per the established pattern** (§2):
  shared-prefix subtraction (16 operation difference — the hook calls),
  matching R28-1/R23-1's precedent exactly.
- **New hook is `pub unsafe fn` + `bench-internals`-gated from creation**, not
  retrofitted — `unsafe fn` despite a safe-primitive body because the
  combination derives an unchecked metadata write from a raw pointer (the R25-1
  shape); documented `# Safety` contract on `issued`.
- **`bench-internals` is on the new arms' OWN `#[cfg(all(...))]`**, not just the
  target-level `required-features` (the per-arm defensive pattern R29-3's first
  attempt missed this round).
- **Two independent `npm run iai` runs** (75 benches each, `production
  bench-internals`, matching the bench target's `required-features`) —
  byte-identical `Ir` for both new arms (4,282 / 4,477), and all reference arms
  reproduced R23-1/R23-3/R28-1 exactly.
- **`cargo test --features production`** — full suite green, including
  `tests/no_stale_doc_references.rs::readme_unsafe_inventory_counts_match_reality`
  (the README tier-2 count was updated 67 → 68 for the one new site).
- **`cargo clippy --all-targets --features 'production bench-internals' -- -D
  warnings`** — clean (the `unsafe fn` with an all-safe-primitive body +
  unused `&self` triggers no lint; mirrors the `dbg_segment_base_of_ptr`
  precedent).
- **`cargo check --bench perf_gate_iai --features 'production bench-internals'`
  under WSL** — compiles (the bench is `target_os = "linux"`-gated).
- **`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/ | wc -l`** for the
  tier-2 component = **68** (67 + 1 new item-scoped site in `heap_core_diag.rs`),
  matching the updated README figure; distinct files unchanged at **18**.
- **No production behavior changed**: the one new `src/` item is a
  `#[doc(hidden)]`, `bench-internals`-gated `unsafe fn` that inlines an existing
  production block verbatim — no production call site touched, no existing
  function body edited. `production`'s own feature composition is unchanged.

---

## 7. Files touched

- `src/registry/heap_core_diag.rs` — added `HeapCore::dbg_clear_magazine_on_hit`
  (`pub unsafe fn`, `#[doc(hidden)]`, `#[cfg(all(feature = "alloc-global",
  feature = "fastbin", feature = "bench-internals"))]`, `#[inline(always)]`,
  `bench-internals`-gated from creation). **One new hook — the only `src/`
  change.**
- `benches/perf_gate_iai.rs` — added `alloc_clear_magazine_only_16b_prefix`,
  `alloc_clear_magazine_only_16b`; registered both in the `perf_gate`
  `library_benchmark_group!` list (75 benches total, up from 73). Zero changes
  to any pre-existing bench fn's body.
- `README.md` — updated the tier-2 unsafe-inventory summary (67 → 68 sites),
  the `heap_core_diag.rs` table row (corrected 4 → 7 — the row was pre-stale at
  4 vs an actual 6 from R29-3's two `dbg_decomp_*` additions; this task's
  correction reflects all seven current sites including the new hook), the
  R24-6/R25-1/R28-1 note ("All four" → "All seven" + a new bullet for
  `dbg_clear_magazine_on_hit`), and the `bench-internals` feature-table row
  (added `dbg_clear_magazine_on_hit` to the gated-hooks list).
- `docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md` — this report.
- `docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE_summary.csv` —
  companion machine-readable summary.
- `docs/perf/_raw_r29_10_run1.log` / `_raw_r29_10_run2.log` — full raw
  `npm run iai` stdout for the two independent, byte-identical-`Ir` runs cited
  in §3. `git add -f` needed (`.gitignore` excludes `docs/perf/_raw_*.log` by
  default, R13-10/task #280).
- `docs/perf/IAI_BASELINE.md` — R3 honest-reject section: dated append-note
  pointing to this measurement (append-correction convention; the original "no
  iai baseline was taken" text preserved verbatim).
- `docs/perf/OPEN_ITEMS.md` — new [L] item 17 (the alloc-hit `clear_magazine`
  cost) entered as a measured honest-reject, closed with the 12.19 Ir/hit figure.

**Files needing `git add -f`** (gitignored by `.gitignore`,
`/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r29_10_run1.log`
- `docs/perf/_raw_r29_10_run2.log`

**Cargo.toml — untouched** (the new arms' features are a superset of the bench
target's existing `required-features = ["alloc-global", "bench-internals"]`;
`production` supplies `alloc-xthread`/`fastbin`, and `bench-internals` is on
the arms' own `#[cfg]`).
