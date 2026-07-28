# R26-7 — LAZY staging-array gate: `Option<[*mut u8; STAGE_CAP]>` is NO-GO (crossover at N=17, the first overflow block)

**Task #416 (R26-7), Round 26.** A real A/B measurement of replacing
`dealloc_batch_small`'s eagerly-zero-initialized `[*mut u8; STAGE_CAP]` stack
array (`STAGE_CAP`=64) with a lazily-materialized `Option<[*mut u8; STAGE_CAP]>`
(starts `None`; materialized on the first overflow block via
`get_or_insert_with`). The hypothesis (suggested by the R25 readonly review):
for N ≤ `TCACHE_CAP` (16) the magazine never fills, so `stage` is never written
— the lazy variant would elide the ~512-byte stack zero-init R24-8 found
expensive, without re-litigating `STAGE_CAP` itself (unchanged at 64).
**Verdict: NO-GO.** The hypothesis was directionally correct (lazy IS cheaper
when never materialized) but the win is tiny (~53 Ir, ~1–2%) AND is immediately
dominated by per-overflow-block `Option`-discriminant-check overhead the moment
the magazine overflows (N > 16). The crossover lands at exactly N=17 — the first
overflow block. Every realistic batch size (R23-7's "tens to low hundreds") is
in the loss regime, where the lazy variant costs up to **+3,076 Ir (+1.67%) at
N=1024**. **This is the 4th consecutive NO-GO in this exact code region**
(R24-3/R24-4/R25-3/R26-7), all the same "added per-block bookkeeping costs more
than the one-time savings it enables" class. `dealloc_batch_small` is
UNCHANGED; the lazy variant stays as `bench-internals`-gated experimental bench
infra (same retained-arm precedent as R25-7).**

**Date:** 2026-07-28. **Base revision measured:** `main` @ `f1f04c2` (the lazy
variant + bench arms of THIS task compiled on top — a single-binary A/B, see §2.2;
`dealloc_batch_small` itself is byte-identical to HEAD).
**Platform/methodology:** WSL2 (Ubuntu, kernel `6.18.33.2-microsoft-standard-
WSL2`) under Windows 10 Pro x86-64, `valgrind 3.22.0`, `iai-callgrind-runner
0.14.2`, WSL rustc `1.98.0-nightly (bd08c9e71 2026-06-25)`. Same harness, same
features (`production batch-api bench-internals`), same fresh-carve same-segment
16 B methodology R24-8/R25-7 established.

**P2 framing (per task brief + R23-7):** the batch API has no downstream
consumer (`batch-api` is `["experimental","alloc-core"]`, NOT part of
`production`; no in-tree caller exists per R23-7's grep audit). This is
hypothesis-testing, not a user-visible optimization. Time-boxed accordingly.

---

## 0. Headline

| question | answer |
|---|---|
| Does the lazy `Option<[..]>` win when the array is never materialized (N ≤ 16)? | **YES, but marginally — ~53 Ir (~1–2%) cheaper, constant across N=0/1/8/16.** |
| Does the win survive the moment the magazine overflows (N ≥ 17)? | **NO — crossover at exactly N=17. Lazy is +55 Ir there, growing to +3,076 Ir (+1.67%) at N=1024.** |
| Is the isolated zero-init savings as large as R24-8/R25-7's linear model predicts? | **NO — the direct A/B isolates the 64-entry (512 B) zero-init at ~54 Ir (N=0), NOT the ~581 Ir a linear extrapolation from R24-8's 4,065 (the 512-vs-64 array-SIZE delta) would predict.** |
| Action / code change? | **NONE to `dealloc_batch_small`.** The lazy variant stays as `bench-internals`-gated experimental bench infra. `git diff HEAD -- src/registry/heap_core_dealloc_batch.rs` shows ONLY the new bench-gated methods appended — the shipping `dealloc_batch_small` is byte-identical. |
| Why NO-GO given the never-materialize win is real? | (1) the win is ~1%, within codegen noise; (2) realistic batch sizes (tens–low hundreds per R23-7) are ALL in the loss regime; (3) a batch that fits the magazine (N≤16) barely benefits from a batch API at all; (4) 4th consecutive NO-GO in this region — the hot path is already tightly compiled. |

---

## 1. The hypothesis and why this region made it worth testing despite the 3-for-3 NO-GO record

`dealloc_batch_small` (`src/registry/heap_core_dealloc_batch.rs:256-257`)
declares an EAGER, unconditionally-zero-initialized 64-pointer (512-byte) stack
array on EVERY call:

```rust
const STAGE_CAP: usize = 64;
let mut stage: [*mut u8; STAGE_CAP] = [core::ptr::null_mut(); STAGE_CAP];
```

R24-8 (task #386) reduced this from 512→64 entries because LLVM does NOT elide
the zero-init even though only a prefix (`stage[..staged]`) is ever read. The
crucial detail R24-8 left as a measured-but-not-fully-exploited fact: `stage` is
only ever WRITTEN TO past the point where the per-class magazine (`TCACHE_CAP`=
16) is already full — i.e. for a batch of N ≤ 16, `stage` is allocated and
zero-initialized but NEVER TOUCHED. For exactly that never-touched case, a lazy
`Option<[..]>` representation (materialize on first write) could elide the
zero-init entirely, which R24-8's own evidence framed as the dominant cost.

The R25 readonly review (`docs/reviews/2026-07-28-r25-readonly-review.md`)
suggested exactly this prototype. The reason it was worth running despite the
3-for-3 NO-GO record (R24-3/R24-4/R25-3) in this region: those three NO-GOs all
targeted the OVERFLOW path's per-block work (bitmap-clear merge, bulk-mask
primitives, FLUSH_N tuning) — costs that scale with N. The lazy-array idea
targets a DIFFERENT cost axis: a FIXED per-call zero-init paid regardless of N.
A fixed-cost elision that helps the SMALL-batch case (the case the prior sweeps
never measured, because their gates were all overflow-path) is not covered by
the prior NO-GOs' "per-block bookkeeping costs more than it saves" finding. The
measurement, not the hypothesis, decides — and it did (§3).

---

## 2. Methodology — single-binary A/B (cleaner than R25-7's two-run protocol)

### 2.1 The variant

`dbg_dealloc_batch_lazy` / `dealloc_batch_small_lazy`
(`src/registry/heap_core_dealloc_batch.rs`, `bench-internals`-gated) are
byte-for-byte copies of `dealloc_batch` / `dealloc_batch_small` with ONE
representation change:

```rust
// EAGER (shipping dealloc_batch_small):
let mut stage: [*mut u8; STAGE_CAP] = [core::ptr::null_mut(); STAGE_CAP];
...
stage[staged] = p;                              // write
unsafe { self.core.flush_class(c, &stage[..staged]) };  // read (×2)

// LAZY (experimental dealloc_batch_small_lazy):
let mut stage: Option<[*mut u8; STAGE_CAP]> = None;
...
let buf = stage.get_or_insert_with(|| [core::ptr::null_mut(); STAGE_CAP]);  // materialize-on-first-write
buf[staged] = p;                                 // write
unsafe { self.core.flush_class(c, &buf[..staged]) };   // read (mid-loop)
let buf = stage.as_ref().unwrap();               // read (final; staged>0 => Some, cannot panic)
unsafe { self.core.flush_class(c, &buf[..staged]) };
```

Every guard (ownership gate, F7/H1), oracle (all three M2), and `flush_class`
contract is identical — verified by line-by-line diff of the two functions. The
`get_or_insert_with` form (NOT `get_or_insert`) is load-bearing: `get_or_insert`
would eagerly construct the 512-byte array on EVERY overflow block and discard
it when already `Some`, re-introducing the exact zero-init cost (clippy's
`unnecessary_lazy_evaluations` suggestion is a documented false positive here,
silenced with `#[allow]` + an explanatory NOTE).

### 2.2 Single-binary A/B — both arms in ONE callgrind pass

Unlike R25-7 (which compared a `const` value requiring two recompilations +
runs), the eager and lazy variants are TWO FUNCTIONS in the SAME binary: the
eager arms call `(*heap).dealloc_batch(...)`, the lazy arms call
`(*heap).dbg_dealloc_batch_lazy(...)` — a one-line swap, identical body
otherwise. A SINGLE `npm run iai` run (`--features 'production batch-api
bench-internals'`, 79 benches) produces BOTH the eager-Ir and lazy-Ir at every
N deterministically. No recompilation, no run-to-run variance to reconcile — the
two functions share the exact same bootstrap, toolchain, and callgrind process.

The grid (the brief's exact N set) and the lazy variant's `stage` behavior at
each N:

| N | overflow blocks (N−16) | `stage` materialized? | intermediate flushes @64 | category |
|---:|---:|---|---:|---|
| 0 | 0 | **NO** | 0 | never-materializes (extreme) |
| 1 | 0 | **NO** | 0 | never-materializes |
| 8 | 0 | **NO** | 0 | never-materializes |
| 16 | 0 | **NO** | 0 | never-materializes (exactly fills magazine) |
| 17 | 1 | YES (first write) | 0 | **CROSSOVER** — first overflow block |
| 64 | 48 | YES | 0 | materializes, 1 final flush |
| 81 | 65 | YES | 1 | materializes, 1 interm + 1 final |
| 200 | 184 | YES | 2 | materializes, 2 interm + 1 final |
| 1024 | 1008 | YES | 15 | materializes, max flush count |

### 2.3 Cross-check — anchors reproduce R24-8/R25-7 exactly

The eager baseline arms reproduce prior rounds' published numbers EXACTLY,
confirming the measurement chain is equivalent before any lazy number is cited:

| arm | R24-8/R25-7 published | this task (eager) |
|---|---:|---:|
| `dealloc_batch_fresh_16_16b` | 4,449 | **4,449** ✓ |
| `dealloc_batch_fresh_64_16b` | 12,692 | **12,692** ✓ |
| `dealloc_batch_fresh_81_16b` | 17,164 (R25-7) | **17,164** ✓ |
| `dealloc_batch_fresh_200_16b` | 36,678 (R25-7) | **36,678** ✓ |
| `dealloc_batch_fresh_1024_16b` | 184,250 (R25-7) | **184,250** ✓ |
| `small_churn_16b` (reference) | 8,051 | **8,051** ✓ |
| `large_alloc_free_cycle` (reference) | 3,308 | **3,308** ✓ |

Reference arms (no `stage` dependency) are byte-identical to prior rounds — the
A/B differs ONLY in the `stage` representation.

---

## 3. Result — full N × variant table

### 3.1 Ir (the deterministic judge)

| N | eager Ir | lazy Ir | ΔIr (lazy−eager) | Δ% | verdict @ this N |
|---:|---:|---:|---:|---:|---|
| 0 | 2,510 | 2,456 | **−54** | −2.15% | lazy wins (isolated zero-init cost = ~54 Ir) |
| 1 | 3,397 | 3,344 | **−53** | −1.56% | lazy wins |
| 8 | 3,860 | 3,807 | **−53** | −1.37% | lazy wins |
| 16 | 4,449 | 4,396 | **−53** | −1.19% | lazy wins (last never-materialize N) |
| **17** | 6,217 | 6,272 | **+55** | +0.88% | **CROSSOVER — lazy loses at the first overflow block** |
| 64 | 12,692 | 12,843 | **+151** | +1.19% | lazy loses |
| 81 | 17,164 | 17,414 | **+250** | +1.46% | lazy loses |
| 200 | 36,678 | 37,267 | **+589** | +1.61% | lazy loses |
| 1024 | 184,250 | 187,326 | **+3,076** | +1.67% | lazy loses (max N) |

**The crossover is at N=17** — the first block that overflows the magazine. Lazy
wins a CONSTANT ~53 Ir for N ≤ 16 (the never-materialize regime); loses for every
N ≥ 17, with the loss growing roughly linearly in the overflow-block count.

### 3.2 Estimated Cycles (cache-aware — `L1 + 5·L2 + 35·RAM`)

| N | eager Cyc | lazy Cyc | ΔCyc (lazy−eager) | verdict @ this N |
|---:|---:|---:|---:|---|
| 0 | 17,275 | 17,197 | **−78** | lazy wins |
| 1 | 20,101 | 20,025 | **−76** | lazy wins |
| 8 | 21,037 | 20,961 | **−76** | lazy wins |
| 16 | 21,609 | 21,499 | **−110** | lazy wins |
| 17 | 24,433 | 24,591 | **+158** | lazy loses (crossover) |
| 64 | 32,815 | 33,201 | **+386** | lazy loses |
| 81 | 38,618 | 39,138 | **+520** | lazy loses |
| 200 | 64,137 | 65,187 | **+1,050** | lazy loses |
| 1024 | 262,009 | 267,221 | **+5,212** | lazy loses |

Cycles confirm the Ir verdict on both sides of the crossover (the win and the
loss). The L1/L2/RAM columns are within ±3 across eager/lazy at every N
(callgrind cache-sim noise on the shared bootstrap; `stage` is stack-local and
L1-resident, never reaching L2/RAM) — no cache-miss regression, exactly as
R25-7 found for the array-size axis.

---

## 4. Why NO-GO — the two cost axes and why the win cannot survive the crossover

### 4.1 The never-materialize win is real but tiny (~53 Ir, NOT ~581 Ir)

For N ≤ 16 the lazy variant saves a CONSTANT ~53 Ir (−53 to −54 at N=0/1/8/16),
the elided 64-entry (512-byte) stack zero-init. The N=0 case is the cleanest
isolation: eager allocates+zeroes `stage` and touches nothing else; lazy
declares `Option::None` and touches nothing — the −54 delta IS the isolated
zero-init cost.

**This is far smaller than a linear extrapolation from R24-8/R25-7 would
predict.** R25-7 measured the STAGE_CAP=512-vs-64 delta as a constant 4,065 Ir
(the difference between zeroing 4096 B vs 512 B = 3584 B) and derived a clean
109 Ir/intermediate-flush linear model. Naively extrapolating: 64 pointers ≈
4,065 × (512/3584) ≈ 581 Ir. **The direct A/B measured ~54 Ir — ~10× smaller.**
Why: R25-7's 4,065 measured the difference between TWO different array sizes in a
shared codegen context (stack-frame layout, alignment, store-width selection all
differ between a 4 KiB and 512 B array); the isolated 512 B zero-init in this
exact codegen is ~54 Ir because LLVM lowers it as a handful of wide vector
stores. **This is the same "standalone/estimated cost misleads" Heisenberg lesson
R24-2 §5.1, R24-3, and R24-4 all document** — a model of codegen cost is not a
measurement; the direct in-context A/B is the truth. The ~54 Ir figure is now
the calibrated truth for this cost.

### 4.2 The materialize loss is the Option-discriminant check, ~3 Ir/overflow-block

For N ≥ 17 the lazy variant costs MORE, growing with the overflow-block count:

| N | overflow blocks | ΔIr (lazy−eager) | marginal ΔIr/block |
|---:|---:|---:|---:|
| 17 | 1 | +55 | (baseline: first block also pays ~108 Ir materialization) |
| 64 | 48 | +151 | — |
| 81 | 65 | +250 | — |
| 200 | 184 | +589 | — |
| 1024 | 1008 | +3,076 | — |

Per-overflow-block cost (excluding the first block's materialization overhead):
`(ΔIr(N=1024) − ΔIr(N=64)) / (1008 − 48) = 2925 / 960 ≈ **3.05 Ir/overflow-block**`
(confirmed at the 64→200 span: `(589−151)/(184−48) = 438/136 ≈ 3.22`). This is
the `Option`-discriminant check + conditional branch on every
`get_or_insert_with` call — ~3 Ir/block the eager array's direct `stage[staged]
= p` store does not pay. The first overflow block (N=17) additionally pays the
materialization: `get_or_insert_with`'s closure invocation + the deferred
zero-init, total ~108 Ir (`−53` baseline → `+55` net).

### 4.3 The crossover at N=17 and why it kills the GO case

The breakeven for the per-block overhead vs the one-time savings:

```
3 × (overflow_blocks) > 53 + 108   (zero-init savings + first-block materialization)
overflow_blocks > 53               →  N > 53 + 16 ≈ 69 for the per-block cost alone to exceed savings
```

But the OBSERVED crossover is at N=17, not N≈69 — because the FIRST overflow
block's materialization overhead (~108 Ir) alone exceeds the entire ~53 Ir
zero-init savings. So the lazy variant is net-negative from the very first
overflow block onward; there is no "small overflow still wins" regime. The only
winning regime is N ≤ 16 (magazine never fills), and there the win is ~1%.

### 4.4 Why this matches the region's NO-GO pattern

This is the 4th consecutive NO-GO in the magazine-overflow region, all the same
class — "added per-block/per-event bookkeeping costs more than the one-time
savings it enables":

| task | idea | result | root cause |
|---|---|---|---|
| R24-3 (task #381) | `flush_magazine_class` bitmap-clear merge | +37 Ir/overflow-event | dynamic-length clear loop > CSE'd fixed unroll |
| R24-4 (task #382) | bulk-mask primitives (`clear_many`/`set_many`) | +14 Ir/block | accumulator per-offset bookkeeping > cheap hot-cache RMWs |
| R25-3 (task #397) | FLUSH_N sweep (4/12/16) | +0.7% to +14.4% | no gate-1 win; FLUSH_N=16 thrashes refill |
| **R26-7 (this)** | **lazy `Option<[..]>` staging array** | **+55 to +3,076 Ir for N≥17** | **Option-discriminant check/overflow-block > ~53 Ir one-time zero-init savings** |

R26-7's distinction (per §1): it targeted a FIXED per-call cost, not per-block
work — a genuinely different axis. The measurement confirms the fixed cost IS
real and elidable (~53 Ir), but the elision's mechanism (an `Option` checked on
every write) reintroduces a per-block cost that scales with N and overwhelms the
fixed savings the moment N > 16. The fixed-cost axis does not escape the class;
it just enters it through a different door.

---

## 5. Decision: NO-GO — `dealloc_batch_small` UNCHANGED

Per the task brief's decision framework (measure-then-decide-separately; do NOT
change `dealloc_batch_small` in this task even if GO):

- **The lazy variant wins only ~53 Ir (~1–2%) for N ≤ 16** — within codegen
  noise, and only for batches that fit the magazine (which barely benefit from a
  batch API at all).
- **It loses for every N ≥ 17**, growing to +3,076 Ir (+1.67%) at N=1024.
- **Realistic batch sizes are ALL in the loss regime:** R23-7
  (`docs/perf/R23_7_BATCH_API_CONSUMER_STATUS.md`) and R24-8 §2.3 both frame
  "tens to low hundreds of blocks per batch" as this project's realistic batch
  size — all > 16, all past the crossover.
- The 4-for-4 NO-GO record in this region now establishes that the
  magazine-overflow hot path is already tightly compiled and resistant to this
  entire class of change (fixed-cost elision via per-block bookkeeping,
  RMW-coalescing, constant tuning). The arithmetic ceiling (the ~53–581 Ir
  zero-init estimate) should not be cited as a savings target here without a
  fresh in-context measurement — the direct A/B is the truth, and it is ~54 Ir,
  not ~581.

**`dealloc_batch_small` is byte-identical to HEAD.** The lazy variant
(`dbg_dealloc_batch_lazy` / `dealloc_batch_small_lazy`) stays in the tree as
`bench-internals`-gated experimental bench infra — same retained-arm precedent
as R25-7's six arms, reusable regression infrastructure for any future
staging-array change. It is NOT reachable from plain `--features production`
(verified: `production`-only build has no `lazy` symbols, §6).

---

## 6. Files touched

- `src/registry/heap_core_dealloc_batch.rs` — **2 new `bench-internals`-gated
  methods appended** (`dbg_dealloc_batch_lazy` public entry wrapper +
  `dealloc_batch_small_lazy` private small-path copy), a byte-for-byte copy of
  the shipping pair with ONLY the `stage` representation changed. The shipping
  `dealloc_batch` / `dealloc_batch_small` are byte-identical to HEAD.
- `benches/perf_gate_iai.rs` — **4 new eager baseline arms**
  (`dealloc_batch_fresh_{0,1,8,17}_16b`, the N's R25-7's existing arms did not
  cover) + **9 new lazy variant arms** (`dealloc_batch_lazy_fresh_{0,1,8,16,17,
  64,81,200,1024}_16b`) + 13 no-op stubs + 13 `library_benchmark_group!` list
  entries. The lazy arms require `bench-internals` (the function they call is
  bench-gated); the eager arms match the existing `batch-api` gate.
- `README.md` — tier-2 unsafe-audit table: `heap_core_dealloc_batch.rs` row
  7→14 sites (the 7 new bench-gated `#[allow(unsafe_code)]` sites are
  textually counted, feature-gate-blind per project convention); summary line
  62→69 tier-2 sites (file count unchanged at 17). Tripwire
  `tests/no_stale_doc_references.rs::readme_unsafe_inventory_counts_match_reality`
  passes.
- `docs/perf/R26_7_LAZY_STAGE_ARRAY_GATE.md` — this report.
- `docs/perf/R26_7_LAZY_STAGE_ARRAY_GATE_summary.csv` — companion summary.
- `docs/perf/_raw_r26_7_lazy_stage.log` — full unfiltered `npm run iai` output
  (79 benches, exit 0). (`.gitignore` excludes `docs/perf/_raw_*.log`; `git add
  -f` at commit time, per the raw-log policy.)
- `docs/perf/OPEN_ITEMS.md` — updated (item 1's R26-7 paragraph appended).

---

## 7. Evidence

- **Raw log:** `docs/perf/_raw_r26_7_lazy_stage.log` (single run, 79 benches,
  exit 0). Both eager and lazy arms in ONE callgrind pass (single-binary A/B,
  §2.2); the 58 non-`dealloc_batch` reference arms are byte-identical in `Ir` to
  prior rounds (§2.3 cross-check), confirming the A/B differed only in the
  `stage` representation.
- **Summary CSV:** `docs/perf/R26_7_LAZY_STAGE_ARRAY_GATE_summary.csv`.
- **Prior reports this extends/cites:** `R24_8_DEALLOC_BATCH_INTERNALS_GATE.md`
  (the STAGE_CAP reduction whose zero-init cost this directly isolates at ~54
  Ir, refuting the ~581 Ir extrapolation), `R25_7_STAGE_CAP_BOUNDARY_GATE.md`
  (the N-sweep whose eager anchors this reproduces exactly), `R24_3` / `R24_4` /
  `R25_3` (the three prior NO-GOs this joins as the 4th), the R25 readonly
  review (the suggestion this prototype tests).
- **Source under test:** `src/registry/heap_core_dealloc_batch.rs:239`
  (`dealloc_batch_small`, unchanged) and the appended
  `dbg_dealloc_batch_lazy` / `dealloc_batch_small_lazy` (bench-gated copy with
  lazy `stage`).
- **Production-unaffected proof:** `cargo test --features production` green
  (pre-push gate); `cargo check --features production --lib` shows no `lazy`
  symbols (the bench-gate excludes them from a `production`-only build).
