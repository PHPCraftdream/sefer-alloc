# R24-3 — flush_magazine_class bitmap-clear merge: NO-GO (measured regression)

**Task #381 (R24-3), Round 24.** The production-code merge of the
magazine-overflow bitmap-clear pre-pass into `flush_run`. R24-2 (task #380)
isolated the standalone cost of the 8-block bitmap-clear pre-pass at **84 Ir**
per overflow event and projected a ceiling of **8.5% of batch-free cost** for
this merge. **This task's Ir judge measured a +37 Ir/overflow-event
REGRESSION (not the expected -84 Ir improvement)**, making the merge a NO-GO.
All production code was reverted; the tree is clean at HEAD (`3bc9c91`).

**Date:** 2026-07-27. **Base revision measured:** `main` @
`3bc9c91` (HEAD = R24-2's commit). **Platform measured:** WSL2 (Ubuntu,
kernel `6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro x86-64,
`valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc
`1.98.0-nightly (bd08c9e71 2026-06-25)` — same toolchain/host as R24-2.

---

## 0. Headline: the merge is a measured regression

| metric | R24-2 (pre-merge, HEAD) | R24-3 (merged) | delta |
|---|---:|---:|---:|
| **overflow event cost** (n17-n16) | **571 Ir** | **608 Ir** | **+37 Ir (REGRESSION)** |
| cheap push (n9-n8) | 43 Ir | 43 Ir | 0 (unchanged) |
| bitmap-clear standalone hook | 84 Ir | 84 Ir | 0 (unchanged) |
| N=64 batch-free (free_cost) | 5,920 Ir | 6,142 Ir | +222 Ir (+3.8%) |

**Expected:** -84 Ir/overflow event (the standalone-measured pre-pass cost),
yielding 8.5% batch-free savings.
**Measured:** +37 Ir/overflow event, yielding 3.8% batch-free regression.
**Total swing from expectation:** +121 Ir/overflow event.

The non-overflow paths are byte-identical (n8, n9, n16, small_churn_16b, prefix
— all UNCHANGED), proving the regression is specific to the overflow path and
the merge is the sole cause.

---

## 1. Design attempted (and why it was the correct shape)

### 1.1 Shape (a): unconditional clear loop inside flush_run

The merge folded the magazine-bit-clear pre-pass into `flush_run` as an
**unconditional** per-run clear loop, reusing `flush_run`'s already-hoisted
`SegmentMeta::new(base)`. Two new functions were added:

- `flush_magazine_class` — a `pub unsafe fn` wrapper (same R6-MS-3 contract as
  `flush_class`) that delegates to `flush_class_inner(..., clear_magazine=true)`.
- `flush_class_inner` — the shared run-grouping body, extracted from
  `flush_class`, taking a `clear_magazine: bool` parameter threaded to
  `flush_run`.

`flush_class` (used by `dealloc_batch`'s non-magazine staged blocks) was
unchanged in behavior: it calls `flush_class_inner(..., false)`, so no
magazine-bit clearing leaks into it. The two production call sites
(magazine-overflow arm in `heap_core_free.rs`, teardown in
`heap_core_tcache.rs`) were updated to call `flush_magazine_class` and their
separate pre-pass loops were deleted.

### 1.2 The correctness hazard was correctly handled

The unconditional clear loop was placed BEFORE `flush_run`'s per-block M2-guarded
accept loop, iterating `run` unconditionally (not gated by `is_free`/payload/bump
checks). This matches the pre-merge "clear all flushed bits, no exceptions"
semantics exactly — a guard-rejected block still gets its magazine bit cleared.

Three counterfactual tests were written and PASSED:
- **(a) M2 hazard test**: ring-DF'd block (already-free in alloc bitmap, M2-rejected
  by accept loop) still gets its magazine bit cleared by the unconditional loop.
  A mutation making the clear conditional (inside the accept loop) made this test
  go RED — confirming the test is non-vacuous.
- **(b) free-then-realloc**: all N blocks returned to the free list correctly,
  re-alloc returns the same set.
- **(c) HeapCore size**: 7,472 bytes at both HEAD and after merge (unchanged).

**The design was semantically correct. The failure was purely in the performance
gate.**

---

## 2. The Ir gate FAILED — root cause analysis

### 2.1 What was measured

The merged code's full `npm run iai` run (43 benches, `--features production`)
showed:

- **All non-overflow arms byte-identical to R24-2** (small_churn_16b=8051,
  n8=7367, n9=7410, n16=7711, prefix=7003, bitmap-clear-hook=7451).
- **All overflow arms INCREASED** by exactly +37 Ir per overflow event:
  n17: 8282→8319 (+37), n32: 9451→9525 (+74 = 2×37), n64: 12923→13145 (+222 =
  6×37).

Raw evidence: `docs/perf/_raw_r24_3_merged_run1.log` (full 43-bench stdout).

### 2.2 Root cause: fixed-length loop unrolling

The pre-merge pre-pass was a loop over a **compile-time-constant-length** slice:
```rust
// FLUSH_N = TCACHE_CAP / 2 = 8 (pub(crate) const, src/registry/tcache.rs:124)
for &flushed in &self.tcache.classes[c].slots[0..FLUSH_N] {
    let fbase = os::segment_base_of_ptr(flushed);
    let foff = (flushed as usize - fbase as usize) as u32;
    SegmentMeta::new(fbase).magazine_bitmap().clear_magazine(foff);
}
```

The compiler can **fully unroll** this 8-iteration loop (constant trip count),
eliminating loop overhead, bounds checking, and enabling CSE of
`segment_base_of_ptr` calls with the subsequent `flush_class` call's own
run-grouping (which calls `segment_base_of_ptr` on the same 8 pointers).

The merged clear loop inside `flush_run` iterates over a **dynamic-length** slice:
```rust
// run: &[*mut u8] — length unknown at compile time
for &mptr in run {
    let moff = (mptr as usize - base as usize) as u32;
    mb.clear_magazine(moff);
}
```

The compiler **cannot unroll** this loop (trip count is `run.len()`, a runtime
value). Each iteration pays loop overhead (increment, compare, branch,
bounds-check) that the unrolled pre-pass did not.

### 2.3 Why the standalone 84 Ir (R24-2) was not recoverable

R24-2 measured the bitmap-clear pre-pass at 84 Ir via a standalone hook
(`dbg_overflow_bitmap_clear_pass` + shared-prefix subtraction:
`7451 - 7367 = 84`). This was an **isolated measurement**: the hook ran the loop
by itself, with no subsequent `flush_class` to CSE against, and the compiler
could still unroll it (the hook received a fixed-length `&[*mut u8]` from the
bench, which happened to be length 8).

In the **real overflow path**, the pre-pass was immediately followed by
`flush_class`, which calls `segment_base_of_ptr` on the SAME 8 pointers (for
run-grouping). The compiler could see both sequences and CSE the shared
`segment_base_of_ptr` results, reducing the pre-pass's real in-context marginal
cost to well below 84 Ir — likely just the `clear_magazine` calls themselves
(~40-50 Ir) plus minimal overhead, with the `segment_base_of_ptr` cost shared
with `flush_class`'s run-grouping.

By merging the clear INTO `flush_run`, we:
1. **Eliminated** the pre-pass (saving its real in-context cost, which was less
   than 84 Ir due to CSE).
2. **Added** a new dynamic-length clear loop inside `flush_run` (adding loop
   overhead + clear_magazine calls that are now NOT CSE'd with anything).

The added dynamic-loop overhead exceeded the saved CSE'd pre-pass cost, yielding
a net +37 Ir regression.

This is the **exact Heisenberg risk** R24-2's own report warned about (§5.1):
the standalone measurement could not predict the in-context cost because the
compiler's optimization of the surrounding code changed the marginal cost.
R24-2 measured the bitmap-clear pass as cleanly isolable via the hook; what it
could NOT measure was how much of that 84 Ir was already being optimized away
by the compiler in the real overflow context.

### 2.4 Verification that the regression is real and merge-specific

- **Reference arms byte-identical**: small_churn_16b=8051, n8=7367, n9=7410,
  n16=7711, prefix=7003 — all match R24-2 exactly, confirming same
  toolchain/host, no environmental drift.
- **Regression is proportional to overflow count**: n17 (+37 = 1×37), n32 (+74 =
  2×37), n64 (+222 = 6×37) — a clean +37 Ir/overflow-event signature.
- **No-overflow arms unchanged**: the cheap push (43 Ir), interleaved hot free
  (8051 Ir), and all other paths are byte-identical — the merge only affects the
  overflow arm, exactly as expected from the code change.
- **Standalone hook unchanged**: `dealloc_overflow_bitmap_clear_only_16b` =
  7451 Ir (same as R24-2), confirming the hook itself was not modified.

---

## 3. What PASSED (before the Ir gate caught the regression)

| gate | result |
|---|---|
| Correctness: `cargo test --features production` (full suite) | **PASS** — all existing tests green (behavior-preserving merge) |
| Correctness: M2 hazard counterfactual (ring-DF'd block magazine bit cleared) | **PASS** — unconditional clear works; mutation (conditional clear) → RED |
| Correctness: free-then-realloc (same blocks returned) | **PASS** |
| HeapCore size: 7,472 bytes (unchanged) | **PASS** — confirmed at both HEAD and after merge |
| `cargo clippy --features production -- -D warnings` | **PASS** |
| `cargo check --all-features / --features experimental` | **PASS** |
| Ir gate: non-overflow arms | **PASS** — byte-identical to R24-2 |
| **Ir gate: overflow event cost** | **FAIL** — +37 Ir/overflow event (expected: -84 Ir) |

---

## 4. Conclusion: NO-GO

The merge of the magazine-bit-clear pre-pass into `flush_run` is a **NO-GO**.
The expected 84 Ir/overflow-event saving (8.5% of batch-free cost) was not
realized; instead, a +37 Ir/overflow-event regression (3.8% of batch-free cost
worse) was measured. The root cause is that the standalone-measured 84 Ir
(R24-2) overstated the real in-context cost of the pre-pass, because the
compiler was already optimizing it efficiently through constant-trip-count loop
unrolling and CSE with `flush_class`'s run-grouping. The merged dynamic-length
clear loop inside `flush_run` cannot be unrolled and pays loop overhead that
exceeds the saved cost.

**All production code was reverted. The tree is clean at HEAD (`3bc9c91`).
No behavior change shipped.**

### 4.1 Implication for the 84 Ir figure

R24-2's standalone measurement of 84 Ir for the bitmap-clear pre-pass is
**valid as a standalone measurement** but **not actionable as a savings target**.
The 84 Ir includes `segment_base_of_ptr` calls that the compiler shares with
`flush_class`'s run-grouping in the real overflow path. The recoverable savings
from eliminating the pre-pass are at most the non-shared portion (the
`clear_magazine` calls + `SegmentMeta::new` + loop overhead), which the compiler
was already reducing to near-minimum through unrolling. There is no merge design
that recovers the full 84 Ir without introducing a dynamic-length loop that costs
more than it saves.

### 4.2 What this means for R24-4 (bulk-mask primitives, task #382)

R24-4 (SegmentBitmap::clear_many / bulk-mask primitives) was BLOCKED BY this
task. Since this task is a NO-GO, R24-4 remains blocked and should NOT be
pursued on the basis of the 84 Ir figure — the same Heisenberg risk applies:
any bulk-clear primitive would replace the unrolled pre-pass with a
different code shape, and the compiler's existing optimization of the pre-pass
means the real savings would be less than the standalone measurement suggests.
R24-4 should only be pursued if a NEW measurement (accounting for compiler
optimization of the existing pre-pass in-context) shows actionable savings.

---

## 5. Evidence

- `docs/perf/_raw_r24_3_merged_run1.log` — full 43-bench `npm run iai` stdout
  for the merged code (`--features production`). The overflow-critical arms
  (n17, n32, n64) show the +37 Ir/event regression; all other arms are
  byte-identical to R24-2's published numbers. (`git add -f` needed —
  `.gitignore` excludes `docs/perf/_raw_*.log`.)
- `docs/perf/R24_3_FLUSH_MAGAZINE_CLASS_GATE_summary.csv` — companion
  machine-readable summary.
- R24-2's published numbers (`docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`
  §4.1) serve as the pre-merge baseline; the reference arms' byte-identical
  reproduction in this run confirms same toolchain/host.

---

## Files touched

- `docs/perf/R24_3_FLUSH_MAGAZINE_CLASS_GATE.md` — this report.
- `docs/perf/R24_3_FLUSH_MAGAZINE_CLASS_GATE_summary.csv` — companion summary.
- `docs/perf/_raw_r24_3_merged_run1.log` — raw iai evidence (`git add -f`).
- `docs/perf/OPEN_ITEMS.md` — item 1 gets a "NO-GO (task #381, R24-3)" note.
- **All `src/` and `tests/` changes were reverted** — the tree is clean at HEAD.
