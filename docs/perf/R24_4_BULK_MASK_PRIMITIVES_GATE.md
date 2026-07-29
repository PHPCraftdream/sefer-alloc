# R24-4 — bulk-mask primitives (`clear_many`/`set_many`): NO-GO (measured regression)

**Task #382 (R24-4), Round 24.** The `SegmentBitmap::clear_many`/`set_many`
accumulator primitive and its first application site (`alloc_batch`'s deferred-
magazine-bit-clear step). The primitive was implemented, fully correctness-
verified (unit + integration tests, both mutation-confirmed non-vacuous), and
applied at site #1 — but the in-context Ir gate measured a **+14 Ir/block
REGRESSION** (not the expected RMW-coalescing win), making the merge a NO-GO.
All production code was reverted; the tree is clean at HEAD (`e530a9f`,
unchanged from R24-3). Site #2 (`flush_all_tcache` teardown) was NOT attempted
(per the task's "STOP at the first site on regression" gate).

**Date:** 2026-07-27. **Base revision measured:** `main` @ `e530a9f`
(HEAD = R24-3's clean-reverted state). **Platform measured:** WSL2 (Ubuntu
24.04, kernel 6.x-microsoft-standard-WSL2) under Windows 10 Pro x86-64,
`valgrind 3.22.x`, `iai-callgrind-runner 0.14.2`, WSL rustc `1.98.0-nightly`
— same toolchain/host family as R24-2/R24-3.

---

## 0. Headline: the bulk primitive is a measured regression at site #1

The decisive measurement is the SAME in-context `alloc_batch` arm measured at
two code versions — HEAD's old per-block loop (BASELINE) vs the
primitive-applied tree (AFTER) — under the identical feature set
`production batch-api` (`alloc_batch` is gated `fastbin + batch-api`, which is
NOT part of `production`; per the R23-7 batch-API-consumer-status decision, no
in-tree production caller exists). iai `Ir` is a deterministic instruction
count, so the delta is an exact integer, not a noisy estimate.

| arm | BASELINE (old per-block loop) | AFTER (bulk `clear_many`) | delta |
|---|---:|---:|---:|
| **`alloc_batch_drain15_16b`** (magazine_drained=15) | **3,685 Ir** | **3,894 Ir** | **+209 Ir (+5.7% REGRESSION)** |
| **`alloc_batch_drain8_16b`** (magazine_drained=8) | **3,640 Ir** | **3,752 Ir** | **+112 Ir (+3.1% REGRESSION)** |
| per-block overhead (delta / drained) | — | — | **+13.9 / +14.0 Ir/block** |

**Expected:** a reduction (the arithmetic ceiling said 8 consecutive 16 B
blocks occupy 1 bitmap byte, so ~8 RMW → 1 possible; for a 15-block fresh-carve
drain, ~15 RMW → ~2, saving ~13 RMW ≈ 40–50 Ir).

**Measured:** +209 Ir / +112 Ir — a regression that scales linearly with the
drain count at ~14 Ir/block. The regression is specific to the `alloc_batch`
path: every reference arm is byte-identical between BASELINE and AFTER
(`small_churn_16b`=8052, `dealloc_free_only_16b_n8`=7371, `_n16`=7715,
`_n17`=8286, `dealloc_overflow_bitmap_clear_only_16b`=7455,
`dealloc_prealloc_only_16b`=7007, `carve_batch_only_16b`=68284,
`cold_alloc_free_256x16b`=50180 — all UNCHANGED), confirming same
toolchain/host and that the bulk primitive is the sole cause.

(Note: these reference arms are +1–4 Ir higher than R24-2's published numbers
because this run is under `production batch-api`, which adds `experimental` —
a tiny codegen change to the shared prefix. The BEFORE/AFTER comparison is
under the SAME feature set, so this constant offset cancels; it does not
affect the verdict. The `Ir` values are NOT compared against R24-2's
production-only figures.)

---

## 1. The primitive design (implemented, verified, then reverted)

`SegmentBitmap::clear_many(&mut self, offsets: &[u32])` /
`set_many(&mut self, offsets: &[u32])` (`src/alloc_core/segment_bitmap.rs`),
with a private shared accumulator body `mask_many` + `flush`:

- Walk `offsets`, OR each offset's bit-mask into a running per-byte
  accumulator (`acc`), keyed on `cur_byte` (the current `byte_idx` from
  `locate`).
- Flush with ONE read-modify-write (`Node::read_u8` + OR/AND-complement +
  `Node::write_u8`) whenever `byte_idx` changes or at the end.
- Empty input is a no-op (seed-and-return).
- Requires NO sorting; non-monotonic orders that revisit a byte simply flush
  the accumulator each step and degrade to exactly N individual RMWs (never
  MORE RMWs than the per-offset loop). Semantically identical to
  `for off in offsets { self.clear(off) }` for every input.

Domain wrappers `MagazineBitmap::clear_magazine_many` /
`mark_magazine_many` (and symmetric `set_many` use) forward trivially. The
primitive is pure safe data + arithmetic routing every raw memory touch
through the `node` seam — **zero `unsafe` added** (the module stays
`unsafe`-free, matching its existing discipline). `#[inline(always)]` on every
method (matching the module's "zero codegen-change hot-path" promise). No new
fields on `SegmentBitmap` (still `#[repr(transparent)]` over `*mut u8`).

The `AllocBitmap` domain wrappers (`mark_free_many` / `mark_alloc_many`) were
deliberately NOT added: no site in this task consumes them, so adding them
would be dead code requiring `#[allow(dead_code)]` suppression — a gratuitous
API-surface addition this project's "no half-wired features" discipline
rejects. They can be added by a future task that actually wires a call site.

---

## 2. Site #1 application (`alloc_batch` deferred-clear)

`HeapCore::alloc_batch`'s step-3 deferred-clear
(`src/registry/heap_core_alloc.rs:1005–1033`) — the loop whose own comment
already named the bulk primitive as "the natural follow-up" — was rewritten
from a per-block loop into run-grouped `clear_magazine_many`:

```text
// OLD (HEAD, restored after revert):
for &p in &out[..magazine_drained] {
    let base = os::segment_base_of_ptr(p);
    let off = (p as usize - base as usize) as u32;
    SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
}

// NEW (applied, measured, reverted):
let drained = &out[..magazine_drained];
let mut i = 0;
while i < drained.len() {
    let base = os::segment_base_of_ptr(drained[i]);
    let mut run_offsets: [u32; super::tcache::TCACHE_CAP] = [0; super::tcache::TCACHE_CAP];
    let mut n = 0;
    while i < drained.len() {
        let p = drained[i];
        if os::segment_base_of_ptr(p) != base { break; }
        run_offsets[n] = (p as usize - base as usize) as u32;
        n += 1;
        i += 1;
    }
    SegmentMeta::new(base).magazine_bitmap().clear_magazine_many(&run_offsets[..n]);
}
```

The run-grouping mirrors `flush_class`'s established same-base-run pattern
(`src/alloc_core/alloc_core_small_magazine.rs:532–569`): walk `drained`,
detect consecutive same-`base` runs, dispatch each run to one
`clear_magazine_many` call. The fixed `[u32; TCACHE_CAP]` offsets buffer
(`TCACHE_CAP = 16`) needs no `Vec`/`Box` (M5: `HeapCore` allocates none);
`magazine_drained ≤ TCACHE_CAP` bounds a single run.

This site's loop is genuinely DYNAMIC-length (`magazine_drained` is a runtime
count) — so it is NOT R24-3's specific fixed-length-`FLUSH_N`-constant-unroll
trap. That lowered (but, as §4 shows, did NOT eliminate) the Heisenberg risk.

---

## 3. The in-context Ir gate FAILED — root-cause analysis

### 3.1 The arithmetic ceiling was real but misleading

`MIN_BLOCK = 16`, one bit per slot, so 8 consecutive 16 B blocks occupy
exactly 1 bitmap byte. A fresh-carve magazine drain (the bench's shape:
refill carves consecutive blocks by bump) produces offset-consecutive blocks,
so for `magazine_drained = 15` the 15 cleared bits land in 2 bitmap bytes →
the ceiling said ~15 RMW could become ~2 RMW, saving ~13 RMW. At ~3–4 Ir per
RMW (read + AND + write), that is a ~40–50 Ir ceiling saving — the basis for
expecting a win.

### 3.2 Why the ceiling was not realized: the replacement's overhead exceeded the RMW savings

The bulk primitive replaces the old loop's per-block work
(`locate` + `read` + `AND` + `write`, ~4 Ir, all on a HOT cache line — the
block was just magazine-popped, its bitmap byte is in L1) with a structure
whose per-block overhead is LARGER:

1. **A per-block stack-array STORE the old loop never did.** The run-grouping
   writes each offset into `run_offsets[n]` (`[u32; 16]` on the stack) before
   `clear_magazine_many` reads it back. That is an extra store + reload per
   block that the old loop's register-resident `off` did not pay.
2. **The accumulator's per-offset control flow.** `mask_many` does, per
   offset: `locate` + a `byte_idx != cur_byte` compare + branch + `acc |=
   mask`. The old `clear_magazine` did `locate` + read + AND + write with NO
   per-offset branch. The accumulator SAVES the per-offset read+write (deferred
   to the once-per-byte flush) but ADDS the compare + branch + accumulate; on a
   hot L1 line the saved read+write is ~2 cheap Ir while the added
   compare+branch+accumulate is ~3–4 Ir — a net per-offset LOSS before the
   flush even factors in.
3. **Per-call fixed costs.** The `[u32; 16]` buffer is zero-initialized
   (`[0; TCACHE_CAP]`) on every outer iteration; for the common single-segment
   drain (one outer iteration) that is 16 zero-stores the old loop never did.
   This is a per-call cost, not per-block — but it contributes to the arm's
   total regression.

The measured **+14 Ir/block** (remarkably consistent: +209/15 = 13.9,
+112/8 = 14.0) is the per-block signature of (1) + (2): the stack-array store
plus the accumulator's per-offset compare/branch/accumulate exceeds the old
loop's hot-cache-line RMW. The RMW coalescing (turning ~15 RMW into ~2) saves
~13 × ~3 Ir ≈ 40 Ir per call — but the replacement's per-block overhead
(~14 Ir × 15 blocks = ~210 Ir) is ~5× larger than the saving. The ceiling
treated "1 RMW" as a costly unit; in this hot-cache-line context a RMW is
~3–4 Ir and the bulk primitive's bookkeeping costs more than it coalesces.

### 3.3 This is the SAME Heisenberg CLASS as R24-3, via a DIFFERENT mechanism

R24-3's NO-GO: replacing a compile-time-constant-length loop lost the
compiler's full unroll + CSE, adding loop overhead that exceeded the saved
cost. The lesson R24-2 §5.1 drew: an isolated standalone-hook measurement
overstates the recoverable cost because the compiler's in-context optimization
changes the marginal cost.

R24-4's NO-GO is the **same class** — an arithmetic ceiling on operation COUNT
(RMW count) did not predict in-context instruction COST — but a **different
mechanism**: this site's loop is dynamic-length (R24-3's specific
constant-unroll trap does NOT apply here), yet a different Heisenberg risk
materialized: the bulk replacement's own per-offset bookkeeping (stack-array
store + accumulator control flow) costs more in-context than the hot-cache-line
RMWs it coalesces, because those RMWs were already cheap. **"Dynamic-length
loop" was a necessary but NOT sufficient condition to avoid the Heisenberg
class.** The general lesson, reinforced across R24-3 and R24-4: an operation-
count ceiling is not a reliable predictor of in-context instruction cost; the
per-operation cost AND the replacement's structural overhead both dominate,
and only an in-context before/after measurement decides.

### 3.4 The ceiling's "scattered offsets degrade gracefully" property held — but the common case (consecutive) is where the win was supposed to come from, and it regressed there too

The primitive's design guaranteed "never MORE RMWs than the per-offset loop"
for scattered offsets. That holds. But the bench measures the BEST case
(fresh-carve consecutive offsets, the accumulator's strongest case), and even
there the per-offset bookkeeping overhead exceeds the RMW savings. The
scattered case would regress by at least as much (same per-offset overhead,
no coalescing offset).

---

## 4. What PASSED before the Ir gate caught the regression

The primitive and site-1 wiring were fully correctness-verified (all under
`production` / `production batch-api`); the failure was purely the perf gate.

| gate | result (before revert) |
|---|---|
| Unit test: `clear_many`/`set_many` bit-for-bit == per-offset loop (10 cases: consecutive, multi-byte, non-monotonic byte-revisit, duplicates, empty, single, mixed-seed, set-symmetry, roundtrip) | **PASS** — 10/10 |
| Unit-test non-vacuity mutation (disable accumulator's final flush → 8/10 RED) | **PASS** — catches the bug |
| Integration test: `alloc_batch` drains single-segment magazine, all drained blocks read "not magazine-resident" | **PASS** |
| Integration test: `alloc_batch` drains MULTI-segment magazine (forced via cross-segment same-thread frees), all drained blocks' bits cleared in their OWN segment bitmaps | **PASS** |
| Integration-test non-vacuity mutation (grouping processes only first run → multi-segment test RED, single-segment still GREEN) | **PASS** — catches the boundary bug |
| `cargo test --features production` full suite | **PASS** (after ARCHITECTURE.md test-count bump, which was also reverted) |
| `cargo clippy` on `""`, `production`, `--all-features` | **PASS** (clean, `-D warnings`) |
| `SegmentBitmap` no new fields / no `unsafe` | **PASS** |
| **Ir gate: `alloc_batch_drain15_16b` / `_drain8_16b`** | **FAIL** — +209 / +112 Ir regression |

The correctness work is recorded here in prose; the code itself is reverted
(R24-3 precedent of a clean revert on a negative result). The primitive
implementation + tests are recoverable from this session's edit history if a
future task re-attempts with a different design.

---

## 5. Site #2 (`flush_all_tcache` teardown) — NOT attempted

Per the task's explicit gate ("if the FIRST site's in-context measurement
shows a regression, STOP — do not apply the primitive at the second site"),
site #2 was not attempted. Site #2 (`heap_core_tcache.rs`'s `flush_all_tcache`,
the production teardown-trim primitive) is in `production` and clears magazine
bits via the same per-block `clear_magazine` loop shape. Given site #1's
regression traces to the bulk primitive's per-offset bookkeeping exceeding
the hot-cache-line RMW cost — a property of the primitive, not specific to
`alloc_batch` — there is no reason to expect site #2 would fare differently;
it uses the same bitmap on the same hot cache lines. Measuring it would very
likely reproduce the regression. (Site #2's loop is also dynamic-length, so
it would not hit R24-3's constant-unroll trap either — but as §3.3 shows,
that is not sufficient.)

---

## 6. Conclusion: NO-GO

The `SegmentBitmap::clear_many`/`set_many` accumulator primitive and its
application at `alloc_batch`'s deferred-clear site are a **NO-GO**. The
expected ~40–50 Ir/call RMW-coalescing win (from the "8 consecutive 16 B
blocks = 1 byte" arithmetic ceiling) was not realized; instead a +209 Ir /
+112 Ir regression (+14 Ir/block, scaling linearly with drain count) was
measured in-context. Root cause: the bulk primitive's per-offset bookkeeping
(a stack-array store + the accumulator's compare/branch/accumulate) costs more
in-context than the hot-cache-line RMWs it coalesces, because those RMWs were
already ~3–4 Ir each. The operation-count ceiling overstated the savings — the
same Heisenberg CLASS as R24-3, via a different mechanism.

**All production code, tests, and bench arms were reverted. The tree is clean
at HEAD (`e530a9f`, unchanged from R24-3). No behavior change shipped.**

### 6.1 Implication for future bulk-mask attempts

Two bitmap-clear NO-GOs in a row (R24-3 overflow clear, R24-4 alloc_batch
clear) indicate that **per-segment bitmap-clear loops in this allocator are
already efficiently compiled and are NOT a fruitful optimization target for
RMW-coalescing primitives.** The bitmaps are hot (L1-resident on the owner
thread), each RMW is ~3–4 Ir, and any bulk primitive's per-offset bookkeeping
overhead exceeds the RMW savings. A future attempt would need a design whose
per-offset overhead is genuinely ZERO (e.g. operating directly on the pointer
slice with no stack-array materialization AND no per-offset branch — hard to
achieve while remaining semantically equivalent to the loop), or target a
different cost category entirely (not RMW count). The arithmetic ceiling
("8 blocks → 1 byte") should not be cited as a savings target for these sites
without a fresh in-context measurement; it has now twice failed to predict the
real outcome.

### 6.2 What this confirms about the R24-3 lesson

R24-3 §4.2 warned that R24-4 "should only be pursued if a NEW measurement
(accounting for compiler optimization of the existing pre-pass in-context)
shows actionable savings." This task DID build the in-context measurement
first (the `alloc_batch_drain*` arms, measured before/after, NOT a standalone
hook), correctly avoiding R24-3's specific "isolated hook overstated the
cost" methodology trap — and the honest in-context measurement showed a
regression. The methodology was right; the primitive just does not win here.
This validates the "measure in-context before trusting an arithmetic ceiling"
discipline: it produced an honest NO-GO in one measurement round rather than
shipping a regression.

---

## 7. Evidence

- `docs/perf/_raw_r24_4_baseline.log` — full `npm run iai --features 'production
  batch-api'` stdout for the BASELINE tree (site-1 reverted to the old
  per-block loop; primitives + bench arms + tests present). 45 benches; the
  two `alloc_batch_drain*` arms and all reference arms. (`git add -f` needed —
  `.gitignore` excludes `docs/perf/_raw_*.log`.)
- `docs/perf/_raw_r24_4_after.log` — full stdout for the AFTER tree (site-1
  bulk-clear applied). Same 45 benches; the `alloc_batch_drain*` arms show the
  +209 / +112 Ir regression, all reference arms byte-identical to BASELINE.
- `docs/perf/R24_4_BULK_MASK_PRIMITIVES_GATE_summary.csv` — companion
  machine-readable summary (before/after Ir, deltas, reference arms, verdict).

Both runs are deterministic (`Ir` is an instruction count); the secondary
columns (Estimated Cycles etc.) show ±1 jitter between runs but the primary
`Ir` column — the comparison axis — is stable across the reference arms
(byte-identical) and shows the exact +209/+112 regression on the
`alloc_batch_drain*` arms.

---

## 8. Files touched (final state — code reverted, docs added)

- `docs/perf/R24_4_BULK_MASK_PRIMITIVES_GATE.md` — this report.
- `docs/perf/R24_4_BULK_MASK_PRIMITIVES_GATE_summary.csv` — companion summary.
- `docs/perf/_raw_r24_4_baseline.log` — raw BASELINE iai evidence (`git add -f`).
- `docs/perf/_raw_r24_4_after.log` — raw AFTER iai evidence (`git add -f`).
- `docs/perf/OPEN_ITEMS.md` — item 1 gets a "NO-GO (task #382, R24-4)" note.
- **All `src/`, `tests/`, `benches/`, and `docs/ARCHITECTURE.md` changes were
  reverted** — the tree is byte-identical to HEAD (`e530a9f`). No behavior
  change shipped.

---

## Post-publication note (R27-10, task #428)

The `dealloc_overflow_bitmap_clear_only_16b` reference arm (cited in §1 at
7,455 Ir among the byte-identical UNCHANGED reference arms confirming same
toolchain/host) and the `HeapCore::dbg_overflow_bitmap_clear_pass` hook it
calls were **removed** in R27-10 (task #428) after the bitmap-clear region
accumulated four consecutive NO-GOs. This report's NO-GO verdict is
unaffected — it rested on the bulk-mask primitive's +14 Ir/block in-context
regression (§1), not on this reference arm. The 7,455 figure remains a valid
historical measurement at this report's commit; the arm is preserved in git
history.
