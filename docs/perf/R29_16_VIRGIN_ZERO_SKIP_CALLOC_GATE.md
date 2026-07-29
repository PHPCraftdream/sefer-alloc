# R29-16 (task #447) — `calloc`-shaped isolation gate for `virgin-zero-skip`

Date: 2026-07-29. Fills the measurement gap flagged by the independent
read-only review `docs/reviews/2026-07-29-oh-acceleration-code-project-review.md`
§1.3 and tracked as `docs/perf/OPEN_ITEMS.md` item 25 (originating in R9-5 /
R11-8 / R13-3): the `virgin-zero-skip` feature (R12-10, task #261) is built,
CI-tested (`ci.yml`'s `production virgin-zero-skip alloc-stats` row), and has
two CONDITIONAL-GO design docs, but its own design docs' Stage-0/Stage-3
promotion gate (`R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §11) was never run. The only
prior measurement, `docs/perf/R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md`,
explicitly says its own single-threaded, 8 KiB-class loop does not capture the
shape this feature targets. This report is that missing measurement — NOT a
promotion recommendation, a measurement only, per this task's brief.

**Scope correction made during this task (important for reading the numbers
below):** `virgin-zero-skip` gates ONLY the **Small**-classified `alloc_zeroed`
path (`AllocCore::alloc_zeroed`'s `AllocKind::Small` arm,
`src/alloc_core/alloc_core.rs:1305-1338`). The **Large**-classified path's own
freshness-skip (`is_fresh` from `alloc_large`) is a SEPARATE, ALREADY-
unconditional mechanism shipped in R8-8 (task #221) with no feature gate at
all — it is not what `virgin-zero-skip` controls and this task does not touch
it. `SegmentLayout::SMALL_MAX` under plain `production` (no `medium-classes`)
is **258,752 bytes (~253 KiB)** — confirmed by direct build (`cargo test`
printing `SegmentLayout::SMALL_MAX`), not assumed from the `EXTRAS` array's
visible top of `16384` (that is only the largest EXPLICIT extra class; the
40-entry geometric progression's own top entry, `258752`, exceeds it and wins
the merge-sort). So a size of "≥ 64 KiB" — the review's suggested candidate —
is still comfortably Small-classified (64 KiB is class index 42 of 49) and is
exactly the range `virgin-zero-skip` actually gates; it is NOT automatically
routed to the Large path the way it would be if `SMALL_MAX` were smaller.

## 1. Size choice: 64 KiB (65,536 bytes)

Verified against this project's own geometry before picking it (per this
task's brief), not assumed:

- `os::SEGMENT = 1 << 22` = 4 MiB (`src/alloc_core/os.rs:65`).
- `SegmentLayout::SMALL_MAX` = 258,752 bytes (~253 KiB) under plain
  `production` — confirmed by direct build (see above).
- 65,536 (64 KiB) resolves to small-class index 42 of 49 (confirmed via
  `AllocCore::dbg_layout_class_for`), comfortably below `SMALL_MAX` — the
  virgin-vs-recycled distinction under test is therefore the SMALL-path one,
  not a Large-path effect.
- 64 KiB is 8× R13-3's own `TARGET_CLASS = 30` (8,192 bytes) — large enough
  that `memset(65536)` is a real cost per the design docs' own analytical
  memset-bandwidth table (`R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §8: ~2.5 µs at
  64 KiB on defensible L2-bandwidth assumptions), not a rounding error next to
  bump-carve/free-list-pop overhead the way an 8 KiB memset (~0.3 µs) risked
  being.

## 2. No new hook was needed

Virgin vs. recycled states are constructed entirely from already-shipped,
already-safe surface:

- **Virgin:** a fresh `AllocCore::new()`'s first-ever `alloc_zeroed` at the
  64 KiB class — a genuine bump-carve on a segment that has never served this
  class before (`payload_virgin == true` on a real OS backend).
- **Recycled/dirty:** plain `alloc` → `write_bytes(0xAA, ...)` (dirty every
  byte) → `dealloc` (pushes onto the class's free list) → `alloc_zeroed` the
  same class/size, which is guaranteed (LIFO single-block free list at this
  class — the same guarantee `tests/alloc_zeroed_virgin_small_skip.rs`'s
  counterfactual test (b) already relies on and asserts) to pop the
  just-freed, just-dirtied block back off the free list: never virgin by the
  dispatch conjunct (`alloc_small_with_virgin`'s doc,
  `src/alloc_core/alloc_core_small.rs:255-263`), so `Node::zero` MUST run.

Both use only `AllocCore::alloc`/`alloc_zeroed`/`dealloc` (ordinary
`pub fn`/`pub unsafe fn` production API) plus the pre-existing
`#[doc(hidden)]` diagnostics `dbg_layout_class_for`/`dbg_block_size` (pure
reads, no raw-pointer metadata mutation — not the R25-1-class hazard the
CLAUDE.md benchmark-hook rule targets, so no `bench-internals`-gated
`unsafe fn` hook was required or added).

## 3. Stage 1 — iai isolation (the paired-prefix-subtraction judge)

Pattern: the established paired-prefix-subtraction shape (R28-1's
`flush_class` isolation, R29-10's `clear_magazine` isolation) — a "prefix" arm
measures only the shared setup; the paired arm does the identical setup plus
ONE timed `alloc_zeroed(64 KiB)` call. New arms in
`benches/perf_gate_iai.rs`: `alloc_zeroed_calloc_virgin_64k[_prefix]` and
`alloc_zeroed_calloc_recycled_64k[_prefix]`, gated
`alloc-core + alloc-decommit + virgin-zero-skip + bench-internals` (Linux
only, per the file's existing convention).

Two independent `npm run iai`-equivalent runs (`node scripts/iai.mjs
alloc_zeroed_calloc --features "production virgin-zero-skip bench-internals"`),
byte-identical (Ir is deterministic under Callgrind emulation on the same
binary+input):

| Arm | Ir | L1 | L2 | RAM | Est. Cycles |
|---|---:|---:|---:|---:|---:|
| `alloc_zeroed_calloc_virgin_64k_prefix` | 66,132 | 130,821 | 107 | 1,001 | 166,391 |
| `alloc_zeroed_calloc_virgin_64k` | 69,199 | 134,706 | 111 | 1,097 | 173,656 |
| `alloc_zeroed_calloc_recycled_64k_prefix` | 134,819 | 264,834 | 129 | 2,125 | 339,854 |
| `alloc_zeroed_calloc_recycled_64k` | 200,443 | 394,982 | 1,157 | 2,135 | 475,492 |

Isolated deltas (paired-prefix subtraction):

- **Virgin** (bump-carve + skip): `69,199 − 66,132 = 3,067 Ir` for ONE
  `alloc_zeroed(64 KiB)` call.
- **Recycled** (free-list pop + explicit `Node::zero`): `200,443 − 134,819 =
  65,624 Ir` for ONE `alloc_zeroed(64 KiB)` call.
- **Ratio: 65,624 / 3,067 ≈ 21.4×.** The recycled path costs roughly
  21 times as many instructions as the virgin path for the identical
  64 KiB `alloc_zeroed` call, almost entirely attributable to the explicit
  `Node::zero` memset the virgin path skips (65,536 bytes ÷ ~64 Ir-equivalent
  bytes/instruction-group is the right order of magnitude for a
  `write_bytes` loop over 65,536 bytes — the L2/RAM hit-count jump in the
  recycled row, 1,157 L2 hits and +10 RAM hits vs. the virgin row's 111/ +96,
  is exactly the cache-line-touching signature of a real memset over many
  cache lines, not a fixed dispatch-cost artifact).

**This is a real, large, deterministic instruction-count win for the
virgin-carve case.** It directly confirms the feature does what it claims:
skip a genuinely expensive memset when the block is provably virgin.

Raw logs: `docs/perf/_raw_r29_16_calloc_isolation_run1.log` (full 79-bench
suite output, includes these 4 arms among the existing perf-gate roster),
`docs/perf/_raw_r29_16_calloc_isolation_run2.log` (filtered rerun,
byte-identical Ir, confirming determinism). Summary CSV:
`docs/perf/R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE_summary.csv`.

## 4. Stage 2 — wall-clock gate at the same 64 KiB size

New bench `benches/r29_16_virgin_zero_skip_calloc_wallclock.rs`
(`cargo bench --bench r29_16_virgin_zero_skip_calloc_wallclock`), mirroring
R13-3's own was/now bench structure exactly (`sample_size(10)`, 200 ms
warm-up / 800 ms measurement — CLAUDE.md's fast comparative-gate profile).
Two scenarios: **virgin** (batch of 16 never-before-served 64 KiB blocks,
freed at the end) and **recycled** (one primed+dirtied block, then 64
`alloc_zeroed`+`dealloc` cycles of the same address).

"Was" = `--features "alloc-global fastbin alloc-decommit
alloc-segment-directory"` (virgin-zero-skip OFF). "Now" = same + `virgin-zero-skip`.

Multiple back-to-back reps (this dev host, shared with other processes;
absolute µs/batch, NOT criterion's own paired "change" line across separate
binaries — those compare against a stale same-named-benchmark-ID baseline
from a DIFFERENT feature-set binary and are not meaningful here, so this
report reads the raw per-run confidence intervals directly instead):

| Rep | virgin OFF (µs/16-batch) | virgin ON (µs/16-batch) | recycled OFF (µs/64-batch) | recycled ON (µs/64-batch) |
|---|---|---|---|---|
| clean pair | 31.2 – 32.4 (mean 31.8) | 30.2 – 31.5 (mean 30.9) | 118.7 – 121.4 (mean 120.0) | 117.2 – 119.6 (mean 118.3) |
| final pair | 57.1 – 80.2 (mean 69.3, wide CI) | 30.9 – 32.6 (mean 31.8) | 150.9 – 227.4 (mean 177.9, wide CI) | 120.2 – 125.3 (mean 123.1) |
| extra virgin-only reps (OFF) | 40.9 – 56.1 across 3 reps (mean ~45–53) | — | — | — |
| extra virgin-only reps (ON) | — | 43.3 – 47.5 across 2 of 3 reps (1 outlier at 68.1) | — | — |

## 5. Honest reading — the isolated Ir win is real; the wall-clock win at this
size does not surface cleanly, and there is a specific, verified reason why

**The wall-clock numbers at 64 KiB do NOT show a clean, reproducible
separation between ON and OFF.** Across repeated reps on this shared dev
host, both configurations' absolute times land in overlapping, wide ranges
(virgin: roughly 31–80 µs/16-batch across reps in EITHER configuration;
recycled: roughly 118–227 µs/64-batch). This is reported plainly, exactly as
R13-3's own report was: this is NOT "virgin-zero-skip has no real effect" —
the Ir isolation in §3 proves a real, deterministic, ~21× instruction-count
difference on the identical call. It means the wall-clock signal at this
sample size, on this host, is too noisy relative to the effect size to
demonstrate it directly — the same class of result CLAUDE.md's own
`sample_size(10)` fast-profile caveat warns about ("Numbers are rough,
directional... not authoritative").

**A specific, verified reason the wall-clock effect is smaller than the Ir
delta alone would predict:** `production`'s composition
(`alloc-global, alloc-xthread, alloc-decommit, fastbin,
alloc-segment-directory, primordial-lazy-commit, class-aware-dirty`,
`Cargo.toml:399`) includes `primordial-lazy-commit` but NOT
`small-segment-lazy-commit` — so an ordinary (non-primordial) Small segment's
pages are committed EAGERLY at reservation time (`Cargo.toml`'s
`small-segment-lazy-commit` feature doc, `:685-704`), not lazily. A fresh
64 KiB span (16 × 4 KiB pages) therefore still costs the OS's own
zero-fill-on-demand first-touch page-fault machinery on first write to each
page — REGARDLESS of whether that first write is our explicit `Node::zero`
pass or the caller's own first write to the returned memory. The Ir isolation
in §3 measures ONLY the software-visible instruction count (Callgrind
emulation does not model real page-fault trap/kernel-zero-fill cost at all —
this is consistent with `docs/perf/OPEN_ITEMS.md`'s R5-R2b honest-reject,
which independently found Ir and real wall-clock can diverge on
page-fault-adjacent effects). The recycled scenario's near-identical
ON/OFF wall-clock numbers (118.3 vs 120.0 µs, ~1.4% apart) is the expected
null result (neither configuration skips `Node::zero` there, so no
difference should exist) and serves as an internal consistency check that
the harness itself is not introducing a spurious ON/OFF bias; the virgin
scenario's similarly-small separation is the more informative result,
because the Ir numbers prove the software-level skip IS firing — its
wall-clock benefit is simply masked by a first-touch OS cost that is paid
either way under this feature set's committed-page policy.

**This does not contradict R13-3's original 8 KiB null finding — it extends
it with a mechanism.** R13-3 found no significant wall-clock difference and
attributed it (accurately, per its own text) to running an uncontended,
cheap-substrate shape where nothing large enough existed for the memset to
stand out. This report shows that even at a size where the memset is
provably substantial in isolation (3,067 vs 65,624 Ir, confirmed
deterministic), the wall-clock signal is ALSO masked here — but for a
different, verified reason: eager page commit under this project's default
`production` composition means the OS's own first-touch cost is paid
regardless of the software zero-skip, on this measured workload shape.

**A structurally different workload — where `small-segment-lazy-commit` is
also enabled, or where the SAME already-committed pages are recarved
repeatedly without ever being decommitted — might show a cleaner wall-clock
separation, since the OS first-touch cost would then be a one-time
per-segment-lifetime cost instead of one paid on every fresh carve. This is
NOT measured here (out of this task's scope) and is named as the natural
next step if a future round wants the wall-clock number to actually
materialize.**

## 6. Bottom line (measurement only, no production-default change made)

- **Isolated Ir delta at 64 KiB: real, large, and deterministic** —
  3,067 Ir (virgin, skip fires) vs. 65,624 Ir (recycled, explicit
  `Node::zero` runs) — a ~21.4× per-call instruction-count difference,
  confirming `virgin-zero-skip` does exactly what its design claims at the
  instruction level.
- **Wall-clock delta at the SAME 64 KiB size: inconclusive/noisy on this
  measured workload shape** — both scenarios show heavily overlapping
  ON/OFF ranges across repeated reps, with a specific, source-verified
  explanation (eager small-segment page commit under plain `production`
  masks the software-level saving behind an OS first-touch cost paid
  either way).
- No production default was changed. This is the Stage-0/Stage-1 evidence
  the design docs' own promotion gate required; a genuine promotion
  decision would still need the wall-clock signal to actually separate
  under SOME realistic workload shape (calloc-heavy, high thread count,
  or `small-segment-lazy-commit` combined) before a GO — that measurement
  is not this task's scope and is named as the open next step.

## 7. Reproduction

```
# Stage 1 (iai, Linux/WSL only):
node scripts/iai.mjs alloc_zeroed_calloc --features "production virgin-zero-skip bench-internals"

# Stage 2 (wall-clock, any platform):
cargo bench --bench r29_16_virgin_zero_skip_calloc_wallclock --features "alloc-global fastbin alloc-decommit alloc-segment-directory"
cargo bench --bench r29_16_virgin_zero_skip_calloc_wallclock --features "alloc-global fastbin alloc-decommit alloc-segment-directory virgin-zero-skip"
```

## 8. 2026-07-29 correction — the wall-clock "virgin" scenario's own design is invalid; §5's eager-page-commit explanation is unconfirmed

An independent readonly review (`docs/reviews/2026-07-29-r29-readonly-review.md`,
finding P1-4) found a real bug in `bench_virgin`
(`benches/r29_16_virgin_zero_skip_calloc_wallclock.rs`), confirmed here by
tracing the actual dispatch order in `alloc_small_with_virgin`
(`src/alloc_core/alloc_core_small.rs:274-297`): step 1 checks the current
segment's free list FIRST, and only falls through to a genuine bump-carve
(where `virgin-zero-skip` can fire) at step 3, if no free block exists
anywhere.

`bench_virgin`'s `b.iter()` closure allocates a batch of `VIRGIN_BATCH` (16)
blocks, then frees the WHOLE BATCH at the end of the SAME closure call — and
criterion invokes that closure many times per sample (thousands, at this
op's cost, within the 800 ms `measurement_time` budget). Every iteration
after the very first therefore begins with a free list already populated by
the PREVIOUS iteration's own `dealloc` calls: from iteration 2 onward, every
`alloc_zeroed` in the "virgin" scenario pops a RECYCLED, DIRTY block off
that free list (step 1) — never reaching the bump-carve path (step 3) where
`virgin-zero-skip` could actually fire. **The "virgin" wall-clock scenario,
as coded, measures the SAME recycled-block path as the "recycled" scenario
for all but the first of thousands of iterations.**

This directly undermines §5's claimed explanation ("eager small-segment page
commit under `production` masks the software saving") for why no ON/OFF
wall-clock separation was observed: the more direct and sufficient
explanation is that the bench itself does not exercise the virgin path
repeatedly, so no separation would be expected regardless of the page-commit
policy. §5's eager-commit mechanism may still be true in general (it is a
correctly-traced fact about `production`'s feature composition), but it was
NOT established as the operative cause HERE, and this report's §4/§5/§6
wall-clock conclusions should be read as **UNCONFIRMED, not negative** — the
wall-clock question this task set out to answer (does the Ir-level win
surface at the wall-clock level under a realistic workload) remains open,
not answered null.

**Not corrected in this pass** (filed as a follow-up item,
`docs/perf/OPEN_ITEMS.md`, tracked under item 25): redesigning
`bench_virgin` to genuinely exercise the bump-carve path on every timed
iteration (e.g. via `criterion::Bencher::iter_batched` with a fresh
heap/segment claimed in the untimed `setup` closure per iteration, or an
outer batch large enough that `SEGMENT`'s carve capacity is exhausted and a
new segment is reserved within the SAME timed iteration) and re-running
Stage 2. **§3's Stage 1 (iai) isolation is UNAFFECTED by this bug** — that
measurement uses fresh, single-shot `AllocCore` instances per Callgrind arm
(not a `criterion` closure reused across iterations) and independently
confirms virgin-vs-recycled dispatch via the `ptr == ptr2`
free-list-reuse assertion in the recycled arm; its 21.4x Ir ratio stands.
