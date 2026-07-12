# iai-callgrind Ir baseline

Deterministic instruction-count (`Ir`) baseline for `benches/perf_gate_iai.rs`,
the reference future perf work (e.g. W4 `carve_batch`) diffs against.

- **Commit (this section, historical provenance):** post-W3 (`alloc-stats`
  gating; the W2 tombstone-rebuild is Ir-neutral). Original baseline was
  `4e139b0`; W3 gated the per-hit stats bump out of `production`, moving every
  hit-heavy bench BELOW the original baseline (small_churn −59, cold_16b −236,
  recycle_16b −477). This 10-bench table is retained ONLY for provenance — new
  work does NOT diff against it (see "Current reference for new work" below).
- **Features:** `production` (`alloc-global` + `alloc-xthread` + `alloc-decommit`
  + `fastbin`) — the same set the CI perf-gate benches with, so these numbers
  match `.github/workflows/perf-gate.yml`. (`stats` counters are OFF in
  `production`; add `alloc-stats` to restore them at ~+59/+236/+477 Ir.)
- **How to reproduce:** `npm run iai` (from repo root). Drives the Linux-only
  bench through WSL under `valgrind --tool=callgrind` (`scripts/iai.mjs`).
- **Runner:** `iai-callgrind-runner 0.14.2` in WSL (pinned `^0.14`, matching
  `iai-callgrind = "0.14"` in `Cargo.toml`); valgrind 3.22.0.
- **Determinism:** `Ir` is callgrind instructions-retired — deterministic
  run-to-run on the same binary+input. Two back-to-back full runs produced
  BYTE-IDENTICAL `Ir` for all benches (10 at the time of this section). That
  determinism is what makes this a judge on this Windows dev host (wall-clock
  is noise; `Ir` is not).

**Current reference for new work:** the "Post-PERF-PASS-5 reference
(2026-07-10)" section near the end of this file is the last fully-tabulated
reference and captures all 12 current bench fns — this is the FINAL
re-pin of this session's 5-pass performance investigation (tasks
#49-#53). Task #53 (PERF-PASS-5, `SegmentHeader`/`Tcache` cache-line
reorder) is a real win: the bootstrap constant (`large_alloc_free_cycle`)
drops from 39,561 to 34,929 Ir (−11.7%, a tighter struct means less to
zero/touch at heap-construction time), and every bench's raw Ir drops by
a similar constant amount — the bootstrap-adjusted marginal `Ir/op*` stays
flat (within noise) on the churn benches, honestly reflecting that this
pass's win is concentrated in one-time construction cost, not the hot
per-op path (consistent with the source review's own tempered
expectation: "layout alone won't close 2x"). All earlier sections below
(Post-PERF-PASS-1 through Post-PERF-PASS-3 — task #52/PERF-PASS-4 caused
no Ir shift, since the xthread/ring drain-guard and false-sharing
partition are invisible to every single-threaded iai bench, so no
separate re-pin section was needed for it) are retained for provenance
only. Regenerate the full table with `npm run iai` before diffing new
work; older baselines in this file (post-W3, post-X1+X2+X3,
post-PERF-PASS-1 through -3) are
historical provenance only — do not diff against them.

## Baseline (Ir per bench function)

| bench function              |         Ir |
| --------------------------- | ---------: |
| small_churn_16b             |     81,170 |
| aligned_churn_640b_a128     |     81,049 |
| large_alloc_free_cycle      |     72,345 |
| realloc_grow                |  1,521,067 |
| cold_alloc_free_256x16b     |    129,863 |
| cold_alloc_free_256x64b     |    129,373 |
| recycle_alloc_free_256x16b  |    182,150 |
| recycle_alloc_free_256x64b  |    181,678 |
| churn_256b                  |     81,045 |
| churn_write_256b            |     81,173 |

## Marginal Ir/op column (review finding F2, 2026-07-09)

`scripts/iai.mjs` prints a seventh column, **`Ir/op*`**, alongside
Ir/L1/L2/RAM/EstCycles. It exists because every bench function builds a FRESH
`SeferAlloc` in its own process (`SeferAlloc::new()` at the top of each bench in
`benches/perf_gate_iai.rs`), so EVERY raw Ir includes the full one-time heap
bootstrap (registry + primordial reserve + 32 KiB bitmap-init + Tcache-zero).
That constant dominates the small-op-count benches unevenly:
`small_churn_16b` (81,423 Ir) is ~90 % bootstrap, `cold_alloc_free_256x16b`
(125,354 Ir) only ~58 %. A nominal "≤ +1 % Ir" threshold is therefore 2–10×
softer or harder *per operation* depending purely on the bench's bootstrap
share — the same headline percent measures different per-op strictness. The
performance review flagged this as finding **F2** and recommended surfacing a
bootstrap-adjusted per-op figure (see
`docs/reviews/2026-07-09-performance-review.md` §F2).

**Definition:** `Ir/op = (Ir − B) / ops`, where `B` is the raw Ir of
`large_alloc_free_cycle` (the bootstrap proxy — one Large alloc+free, touching
no magazine / no small-class carve / no freelist, so its Ir is essentially the
per-process constant; this is the same decomposition the X7 hardened-tier
table below already uses), and `ops` is the bench's alloc+free op-**pair** count
(`CHURN_OPS=64`; `COLD_BATCH=256`; recycle/multiseg 2-round loops = 512/68;
`SEGCYCLE_ROUNDS×SEGCYCLE_BATCH=204`; `realloc_grow` = 16 growth steps). The
op-counts are encoded in `BENCH_OPS` in `scripts/iai.mjs`, mirrored from the
bench constants. `large_alloc_free_cycle` itself IS the constant, so its
marginal figure prints `-`.

The proxy slightly OVER-estimates pure bootstrap (it includes one Large
op-pair), so the marginal column is a mild LOWER-bound on true per-op cost —
the conservative direction for a regression guard. It is a best-effort SIGNAL
column: **Ir stays the pass/fail judge**; a missing bootstrap row (e.g. the
proxy filtered out of a run) prints `-` and never affects the verdict.

Sanity-check against the CAP=16 baseline in
`PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md` (B = 73,011):
`small_churn_16b` → (81,423 − 73,011)/64 = **131.4 Ir/op**;
`cold_alloc_free_256x16b` → (125,354 − 73,011)/256 = **204.5 Ir/op**;
`recycle_alloc_free_256x16b` → (179,180 − 73,011)/512 = **207.4 Ir/op** —
matching the review's ≈131 / ≈204 hand-figures. Future GO/NO-GO thresholds
should be phrased in this marginal unit, not raw sums (F2 rec. 2).

## W4 result (E1 `carve_batch` + E3 batched `dec_live`; E2/E4 rejected)

E1 (`AllocCore::carve_batch` — one hoisted `align_up` div / bump load-store /
`live += n` / `is_decommitted` check / per-distinct-page marking per refill
run, replacing the per-block `carve_block` loop in `refill_class_bump`; plus
removal of the post-`free_exhausted` redundant `drain_freelist_batch` re-read)
and E3 (one `sub_live(k)` + single decommit check in `flush_run`, replacing the
per-accepted-block loop). E2 (`REFILL_N` const LUT) was REJECTED — the `[u16;49]`
load REGRESSED cold +32 / recycle +62 vs the inlined `udiv`, so it was reverted.
E4 (heap_core branch-fold) was DROPPED — the two sites sit in separate cfg
regions with fall-through semantics, not a self-contained fold; churn is already
at/below baseline and the risk to the won front is not worth a speculative
−1 branch.

| bench function              |   baseline |    W4 (E1+E3) |     delta |
| --------------------------- | ---------: | ------------: | --------: |
| cold_alloc_free_256x16b     |    129,863 |       123,516 |    −6,347 |
| cold_alloc_free_256x64b     |    129,373 |       123,023 |    −6,350 |
| recycle_alloc_free_256x16b  |    182,150 |       175,896 |    −6,254 |
| recycle_alloc_free_256x64b  |    181,678 |       175,418 |    −6,260 |
| small_churn_16b             |     81,170 |        80,797 |      −373 |
| churn_256b                  |     81,045 |        80,672 |      −373 |
| churn_write_256b            |     81,173 |        80,800 |      −373 |

Cold dropped ~6.3k Ir (the target); recycle also dropped ~6.3k (the post-latch
redundant-drain removal helps it too — better than the "neutral" bar); churn is
UNREGRESSED (slightly below baseline). All correctness pins green; carve_batch
is byte-identical to the per-block carve loop (M2 untouched — carve never writes
the bitmap; D1 exact +n; page-dedication "first class wins"; SEGMENT boundary
check; decommit recommit-on-reuse), verified by `tests/regression_carve_batch.rs`.

## W7a note (generation→u64 + TaggedPtr repack)

W7a widened `HeapSlot::generation` to `AtomicU64` (byte-identical Ir — the field
is written only on cold claim/recycle and compared only `== 1`) and repacked
`TaggedPtr` to `index:16 | tag:48` (all call sites are cold: bootstrap /
claim / recycle — none on the alloc/free hot loop). The repack shifts EVERY
bench by a uniform **−4 Ir** — a *decrease*, from the one-time bootstrap
`free_slots = TaggedPtr::empty()` store whose constant shrank from
`0x0000_0000_FFFF_FFFF` to `0x0000_0000_0000_FFFF` (a cheaper immediate that
falls inside each bench's first-claim window). Not a hot-path touch; accepted.
The current post-W7a table is the W4 table above minus 4 on every row
(e.g. `cold_alloc_free_256x16b` = 123,512; `small_churn_16b` = 80,793).

## Pre-X-arc full baseline (2026-07-05)

Fresh full-run snapshot taken before the perf/correctness X-arc (#182–187).
Confirms **zero drift since W7a** — every `Ir` is byte-identical to the post-W7a
values above (small_churn 80,793 · cold 123,512 · recycle 175,892 · realloc_grow
1,520,714). This is the reference the X-arc diffs against; X1 (in-place Large
realloc) targets `realloc_grow`.

This run also records the FULL callgrind metrics — L1/L2/RAM hits and
`Estimated Cycles` (`= L1 + 5·L2 + 35·RAM`, callgrind's default model). These
are what X3's cache-aware judge upgrade will diff against, since `Ir` counts a
`udiv` and a cache-missing load identically (both = 1 instruction) while cycles
do not. Numbers are deterministic run-to-run (callgrind).

| bench function              |        Ir |    L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ---------: | ------: | -------: | ----------: |
| small_churn_16b             |    80,793 |    141,802 |      64 |    5,201 |     324,157 |
| aligned_churn_640b_a128     |    80,677 |    141,690 |      64 |    5,201 |     324,045 |
| large_alloc_free_cycle      |    72,341 |    131,665 |      62 |    5,206 |     314,185 |
| realloc_grow                | 1,520,714 |  3,751,251 |  45,317 |   92,240 |   7,206,236 |
| cold_alloc_free_256x16b     |   123,512 |    193,440 |     107 |    5,277 |     378,670 |
| cold_alloc_free_256x64b     |   123,019 |    192,766 |     109 |    5,459 |     384,376 |
| recycle_alloc_free_256x16b  |   175,892 |    255,667 |     109 |    5,281 |     441,047 |
| recycle_alloc_free_256x64b  |   175,414 |    254,996 |     111 |    5,475 |     447,176 |
| churn_256b                  |    80,668 |    141,676 |      64 |    5,202 |     324,066 |
| churn_write_256b            |    80,796 |    141,929 |      64 |    5,205 |     324,424 |

**`realloc_grow` is the outlier the X-arc exists for:** 92,240 RAM hits and
45,317 L2 hits vs ~5.2k / ~100 for every other bench — memcpy floors made
visible. Its Estimated Cycles (7,206,236) is ~22× any other bench, a WIDER gap
than the 19× seen in `Ir` alone: the cache-miss cost of the copies is invisible
to `Ir` but real in cycles. X1 (in-place growth within the already-mapped
`span_usable`) removes the copy entirely on the fits-in-span path, so the cycle
metric should collapse even harder than `Ir`.

## Post-X1+X2+X3 reference (2026-07-05) — the CURRENT 11-bench table

After X1 (OPT-G in-place Large realloc, `754eee5`), X2 (#164 drain-side
magazine check, `7441dcc`) and X3 (judge upgrade: cache columns in
`scripts/iai.mjs` + the new `multiseg_cold_256k` bench). **Future diffs are
taken against THIS table** — adding the 11th bench fn shifted every old
bench's Ir via pure binary layout (identical benches shifted identically:
both cold +1,160, both recycle +2,066, small_churn/churn_256b both +321;
`large`/`realloc_grow` only +2), so the 10-bench numbers above are retired
as reference points.

| bench                       |        Ir |    L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ---------: | ------: | -------: | ----------: |
| small_churn_16b             |    81,396 |    142,717 |     160 |    5,217 |     326,112 |
| aligned_churn_640b_a128     |    81,405 |    142,728 |     160 |    5,219 |     326,193 |
| large_alloc_free_cycle      |    72,984 |    132,617 |     158 |    5,220 |     316,107 |
| realloc_grow                |   561,912 |  1,173,997 |   3,973 |   74,963 |   3,817,567 |
| cold_alloc_free_256x16b     |   125,215 |    195,546 |     172 |    5,323 |     382,711 |
| cold_alloc_free_256x64b     |   125,218 |    195,369 |     174 |    5,504 |     388,879 |
| recycle_alloc_free_256x16b  |   179,018 |    260,572 |     170 |    5,329 |     447,937 |
| recycle_alloc_free_256x64b  |   179,021 |    260,383 |     172 |    5,522 |     454,513 |
| churn_256b                  |    81,396 |    142,717 |     160 |    5,217 |     326,112 |
| churn_write_256b            |    81,524 |    142,972 |     160 |    5,218 |     326,402 |
| multiseg_cold_256k          |   111,642 |    189,151 |     184 |    5,514 |     383,061 |

**The X-arc headline, in both judges:** `realloc_grow` Ir 1,520,714 →
561,912 (**−63%**, X1 in-place growth + X2's magazine-routed realloc
alloc-leg) and Estimated Cycles 7,206,236 → 3,817,567 (**−47%**; RAM hits
92,240 → 74,963 — the memcpy floors are gone on every fits-in-span step).
X2's accepted documented costs (see commit `7441dcc`): +~630 Ir one-time
bootstrap per heap claim and ~+30 Ir per refill-miss; hot magazine push/pop
untouched (churn per-op below the pre-X2 baseline once the one-time constant
is excluded). `multiseg_cold_256k` (new, X3) is the designated judge for X5
(per-class segment queues): 34 × 256 KiB allocations span 3 segments and the
second round's refills walk all three via `find_segment_with_free`.

## X4 honest-rejects (2026-07-05) — both recycle experiments measured and declined

Recorded per the project's reject-with-numbers precedent (E2 REFILL_N LUT).
Final tree after X4 = pristine `2a23878` (zero diff; nothing shipped).

**A — `TCACHE_CAP` 16→32 (byte-budgeted both sides): REJECT.** Every bench
regressed, including the explicit target (recycle **+32,305** Ir; churn
+22.3k; cold +25.3k; large +18.3k). The bench shapes don't refill-miss enough
to amortize a doubled cap — each refill/flush just got twice as large (bigger
carve/flush batches, larger `Tcache` zero-init, longer M2 scan). Confirms the
FASTBIN P6 sweep's "CAP=32+ materially worse" even post-Э1 batched carve.

**B — 64-bit bloom signature gating the M2 in-magazine scan: REJECT (the
won-front rule).** Recycle won big (−19,147 / −14,235; cold −8,733 / −6,997)
but ALL THREE churn benches regressed ~+980 Ir — far past the ±10 hot-path
kill threshold. Mechanism: on churn the freed block was just popped from the
magazine, so its signature bit is still set → the gate never skips the scan
and is pure overhead (shift+and+test per free + push-side `|=` + a larger
`Tcache`). The bloom only earns its skip on cold/recycle, where freed blocks
came from the substrate (bit clear). Churn is the won front; the project does
not trade it — declined despite the net-positive arithmetic. If a future arc
revisits this, the shape to try is a signature that is CLEARED on pop (pop
knows the slot index; clearing exactly one bit is sound only with per-slot
bits, not a shared bloom — i.e. a 16-bit occupancy mask keyed by slot, which
is just the scan again). Recorded so the next reader does not re-run the same
experiment blind.

## X6 honest-reject (2026-07-05) — clz class_for vs the 16 KiB SIZE2CLASS LUT

**REJECT.** A clz-based `class_for` (14-byte `CLZ_BASE` per-pow2-bucket table +
≤6-step forward scan — the 49-class geometry has 1–5 irregular classes per
log2 bucket, no closed form) was proven **bitwise-identical** to the LUT over
8,280,074 (size, align) pairs, then measured against the 11-bench reference:

- Churn Ir: 0 delta (the compiler const-evals `class_for` for the benches'
  fixed sizes — both variants generate identical code there).
- `realloc_grow` (the one dynamically-sized path): **+658 Ir** — clz+scan
  costs more instructions than one indexed load.
- **Estimated Cycles regressed on 10/11 benches** (churn +72…+208; recycle
  +72/+140; multiseg +76; only cold_64b −64): RAM hits unchanged (±4), so the
  LUT's 16 KiB footprint never surfaced as misses even in callgrind's
  cold-start model, while the scan's extra loads did.

Caveat recorded: the tiny benches under-represent the LUT's real cache
pressure against an application working set — but this was not a near-tie
(10/11 EC regressions), so the footprint tie-break did not apply. If a future
arc revisits, the trigger should be a REAL-application cache profile (not
microbenches) showing SIZE2CLASS lines contending; the clz implementation and
the exhaustive differential test are recoverable from this ledger entry's
description.

## X5 honest-reject (2026-07-05) — per-class segment-queue bitmap (cheapest variant)

**REJECT.** The cheapest sound variant of the X5 idea — a per-segment `u64`
bitmap of non-empty classes (bit `c` set ⟺ `BinTable.head(c) != FREE_LIST_NULL`,
maintained at every empty↔nonempty transition, consulted by
`find_segment_with_free` instead of loading the BinTable head cache line) — was
implemented, proven correct by 8 dedicated regression tests (counterfactual-
verified: disabling any one transition makes the invariant test FAIL), and
measured against the 11-bench reference. It regressed the designated judge AND
the won front; declined per the decision rule.

### The change (recoverable from this description)

- `SegmentHeader` gains one owner-only `free_classes: u64` field (header stays
  well under PAGE; `page_map_off` unchanged). `Node::read_u64`/`write_u64`
  added to the seam. `SegmentMeta` gains `free_classes_of` /
  `set_free_classes` / `mark_class_free(c)` / `clear_class_free(c)`.
- All 7 `set_head` call sites maintain the bit:
  - empty→nonempty SET: `dealloc_small`, `flush_run` (if `old_head==NULL`),
    `reclaim_offset`, `reclaim_offset_checked`.
  - nonempty→empty CLEAR: `pop_free` (if `new_head==NULL`),
    `drain_freelist_batch` (if final `head_off==NULL`).
  - full reset: `decommit_empty_segment` does `set_free_classes(0)`; both
    header constructors (`small`/`large`) init to 0.
- `find_segment_with_free` replaces `bt.head(class_idx) != FREE_LIST_NULL` with
  `(meta.free_classes_of() & (1u64 << class_idx)) != 0`.
- Magazine push/pop (the hot path) UNTOUCHED — every touched site is the
  refill-miss / dealloc-slow path.

### Correctness (verified before measuring)

8 tests in `tests/regression_x5_free_classes_bitmap.rs` (cfg `alloc-decommit`):
`own_free_sets_bit`, `pop_to_empty_clears_bit`, `flush_run_sets_bit`,
`batch_drain_clears_bit`, `ring_drain_sets_bit` (xthread), `decommit_clears_all`,
`invariant_holds_across_churn` (asserts bit⟺head for ALL 49 classes after every
op), `segment_with_free_is_found` (behavioural: a segment with a free block is
always found — no false-OOM, no unnecessary carve). **Counterfactual-verified:**
commenting out the `dealloc_small` `mark_class_free` makes `own_free_sets_bit`
FAIL (`X5 invariant violated at class 3: bit_set=false head_nonempty=true`) and
`segment_with_free_is_found` FAIL (the scan skipped the segment). Full
`cargo test --features production` green.

### Measurement (vs the Post-X1+X2+X3 reference)

| bench                       | base Ir |   X5 Ir |  Δ Ir | base EC |    X5 EC |   Δ EC |
| --------------------------- | ------: | ------: | ----: | ------: | -------: | -----: |
| small_churn_16b             |  81,396 |  81,405 |   +9  | 326,112 |  326,334 |  +222  |
| aligned_churn_640b_a128     |  81,405 |  81,414 |   +9  | 326,193 |  326,347 |  +154  |
| large_alloc_free_cycle      |  72,984 |  72,989 |   +5  | 316,107 |  316,257 |  +150  |
| realloc_grow                | 561,912 | 562,012 | +100  |3,817,567|3,817,880 |  +313  |
| cold_alloc_free_256x16b     | 125,215 | 125,634 | +419  | 382,711 |  383,040 |  +329  |
| cold_alloc_free_256x64b     | 125,218 | 125,619 | +401  | 388,879 |  389,539 |  +660  |
| recycle_alloc_free_256x16b  | 179,018 | 179,828 | +810  | 447,937 |  449,311 | +1,374 |
| recycle_alloc_free_256x64b  | 179,021 | 179,831 | +810  | 454,513 |  455,853 | +1,340 |
| churn_256b                  |  81,396 |  81,405 |   +9  | 326,112 |  326,266 |  +154  |
| churn_write_256b            |  81,524 |  81,533 |   +9  | 326,402 |  326,556 |  +154  |
| **multiseg_cold_256k**      | 111,642 | 111,915 |+273  | 383,061 |  383,547 |  +486  |

### Why it regressed (the mechanism)

The bitmap is a NET COST at n=3 segments because the maintenance (a
read-modify-write on every empty↔nonempty transition: load `free_classes`, OR/
AND a mask, store) runs on the DEALLOC slow path that these benches exercise on
EVERY freed block whose segment's class list transitions, while the saving (one
BinTable-head cache line replaced by one u64 load on the refill-miss scan) is
collected only on `find_segment_with_free` — which at n=3 is a 3-iteration walk.
The scan was never the bottleneck: the BinTable head load and the `free_classes`
load sit in the SAME cache line as the header already read for `kind_at`, so the
"avoid a BinTable-line load" premise DOES NOT HOLD here — there is no extra cache
line to avoid (the header line is already hot). The +9 Ir on the churn benches
(the won front, ±10 kill threshold) is the per-dealloc `old_head == FREE_LIST_NULL`
branch + the `1u64 << class_idx` compute, paid even when the bit is already set
(nonempty→nonempty, the common churn case). Recycle's +810 / cold's +400 is the
RMW cost amplified across the many transitions those benches drive.

### Why the real payoff needs 100+ segments (and no current bench models that)

The O(n_segments) scan only hurts when `find_segment_with_free` walks MANY
segments that DO NOT have the wanted class. At n=3 (the designated judge) the
walk is 3 iterations and every segment's header line is already resident; the
bitmap cannot win. The structural argument for X5 only materialises at
n_segments ≫ 3, where (a) the scan cost grows linearly and (b) the BinTable
heads of distant segments stop being in cache. A real server with 100+ long-
lived small segments (the scenario the task names) is NOT modelled by any
current bench; `multiseg_cold_256k` spans only 3. So this is an HONEST REJECT
**for the measured regime**, not a refutation of the idea: the variant is
correctness-proven and recoverable, and a future arc that adds a ≥64-segment
bench (or profiles a real application) may flip the verdict. The shape to revisit
is the FULL per-class queue (skip non-matching segments entirely, not just a
per-segment bit probe), since the bitmap variant already loses to the BinTable
load at n=3 and the queue's only extra cost is the linked-list link maintenance
— which is the same RMW that already regressed here, so even the queue would need
the large-n scan to amortise it. Recorded so the next reader does not re-run the
same bitmap variant blind.

Final tree after X5 = pristine `490974d` (zero diff; nothing shipped).

## Hardened-tier costs (X7, 2026-07-06)

The X7 arc (Ф1–Ф5, task #188 umbrella; commits `cdc3361`/`345a2ce`/`d1e91ff`/
`3b0ed2c` + this phase) added a hardened-only per-granule generation counter
that closes the re-issue-before-drain cross-thread double-free leg (plan
§1–§5). The hardened tier is ADDITIVE over `production` (`hardened` pulls
`fastbin` + the gen-table metadata + the interior-pointer free guard); the
production hot path stays byte-for-byte identical (the Ф1–Ф4 production-judge
gates confirm 11/11 at every phase, and Ф5 re-confirms it here as the closure
gate). This section publishes the hardened-tier cost — the price of the
defence-in-depth feature, per the project's "publish the cost, no threshold"
rule for accepted feature costs.

**Method:** `node scripts/iai.mjs --features "production hardened"` (the
`--features` override was added to `scripts/iai.mjs` in this phase — backward-
compatible; no-arg default stays `production`, so CI / `npm run iai` is
byte-identical). Same iai-callgrind runner (0.14.2), same WSL valgrind 3.22.0,
same eleven bench functions, same callgrind cache model. The hardened numbers
below were taken in one run; the determinism argument (callgrind Ir is
reproducible run-to-run on the same binary+input) is unchanged.

| bench                       | prod Ir | hardened Ir | Δ Ir | Δ % | hardened EC | Δ EC |
| --------------------------- | ------: | ----------: | ---: | --: | ----------: | ---: |
| small_churn_16b             |  81,396 |     344,102 | +262,706 | +322.8% |   990,713 | +664,601 |
| aligned_churn_640b_a128     |  81,405 |     344,245 | +262,840 | +322.9% |   990,754 | +664,561 |
| large_alloc_free_cycle      |  72,984 |     335,179 | +262,195 | +359.2% |   979,903 | +663,796 |
| realloc_grow                | 561,912 |     824,375 | +262,463 |  +46.7% | 4,482,381 | +664,814 |
| cold_alloc_free_256x16b     | 125,215 |     389,584 | +264,369 | +211.1% | 1,049,340 | +666,629 |
| cold_alloc_free_256x64b     | 125,218 |     389,587 | +264,369 | +211.1% | 1,055,522 | +666,643 |
| recycle_alloc_free_256x16b  | 179,018 |     445,851 | +266,833 | +149.1% | 1,117,709 | +669,772 |
| recycle_alloc_free_256x64b  | 179,021 |     445,854 | +266,833 | +149.1% | 1,124,265 | +669,752 |
| churn_256b                  |  81,396 |     343,782 | +262,386 | +322.4% |   990,197 | +664,085 |
| churn_write_256b            |  81,524 |     343,910 | +262,386 | +321.9% |   990,487 | +664,085 |
| multiseg_cold_256k          | 111,642 |     372,141 | +260,499 | +233.3% | 1,043,727 | +660,666 |

**Reading the table honestly — the cost is a one-time bootstrap, NOT a per-op
tax.** The raw Ir delta looks catastrophic (+322% on churn!), but it is almost
entirely a CONSTANT additive ~262k Ir paid once per bench PROCESS, visible on
EVERY bench including `large_alloc_free_cycle` (which does ZERO small-block
magazine work — its full +262,195 Ir delta IS the bootstrap, nothing else).
That constant is the hardened tier's heap-claim setup: `init_gen_table_in_place`
zeroes the 256 KiB gen table on every fresh segment reserve, and the interior-
pointer free guard's one-time wiring runs at first claim. Subtracted out, the
MARGINAL per-op cost of the hardened feature is what the plan §5 predicted:

| bench                       | marginal Δ Ir (minus bootstrap) | marginal Δ % |
| --------------------------- | ------------------------------: | -----------: |
| small_churn_16b             |                          +511 | +0.6% |
| aligned_churn_640b_a128     |                          +645 | +0.8% |
| large_alloc_free_cycle      |                            0 |  0.0% |
| realloc_grow                |                          +268 |  0.0% |
| cold_alloc_free_256x16b     |                        +2,174 | +1.7% |
| cold_alloc_free_256x64b     |                        +2,174 | +1.7% |
| recycle_alloc_free_256x16b  |                        +4,638 | +2.6% |
| recycle_alloc_free_256x64b  |                        +4,638 | +2.6% |
| churn_256b                  |                          +191 | +0.2% |
| churn_write_256b            |                          +191 | +0.2% |
| multiseg_cold_256k          |                        −1,696 | −1.5% |

(The bootstrap constant is derived from `large_alloc_free_cycle`'s raw delta —
that bench issues one Large segment and frees it, touching no magazine, no
gen-table cell, no interior-pointer guard. Its entire hardened delta is
therefore the per-process setup cost. This is the cleanest decomposition
available; the marginal numbers are the per-op feature tax the plan §5
forecast at "±2–4% churn.")

**The headline:** the magazine hot path (`small_churn_16b`, `churn_256b`,
`churn_write_256b`) pays **+0.2–0.8% Ir** — the per-issue `bump_gen` RMW on the
gen-table cell (one `fetch_add(1, Relaxed)` into a cold metadata byte). The
refill-miss path (`recycle_*`) pays **+2.6%** — the RMW amplified across the
batch carve. `multiseg_cold_256k`'s −1.5% is binary-layout noise (the hardened
feature widens segment metadata, shifting code addresses — within callgrind's
determinism, cross-binary layout jitter of this magnitude is expected and is
NOT a per-op signal). This is the published cost of the defence-in-depth
feature; there is no "must not regress" threshold on it (plan §5: "порога 'не
хуже' нет — это осознанная плата за защиту"). The production hot path is
untouched (0% delta by construction — every hardened code path is behind
`#[cfg(feature = "hardened")]`).

**Production-judge re-confirmation (Ф5 closure gate):** the no-arg `npm run
iai` (plain `production`) still produces the Post-X1+X2+X3 reference table
above, byte-for-byte. The hardened code is all behind the feature flag; the
production binary is the same binary Ф4 shipped. This is the final production-
neutrality gate for the X7 arc.

Full hardened-tier metric set (Ir | L1 | L2 | RAM | Estimated Cycles), matching
the production reference table's column structure for direct comparison:

| bench                       |        Ir |     L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ----------: | ------: | -------: | ----------: |
| small_churn_16b             |   344,102 |     663,598 |     162 |    9,323 |     990,713 |
| aligned_churn_640b_a128     |   344,245 |     663,754 |     160 |    9,320 |     990,754 |
| large_alloc_free_cycle      |   335,179 |     652,858 |     162 |    9,321 |     979,903 |
| realloc_grow                |   824,375 |   1,694,526 |   4,102 |   79,067 |   4,482,381 |
| cold_alloc_free_256x16b     |   389,584 |     718,585 |     176 |    9,425 |   1,049,340 |
| cold_alloc_free_256x64b     |   389,587 |     718,397 |     190 |    9,605 |   1,055,522 |
| recycle_alloc_free_256x16b  |   445,851 |     786,589 |     179 |    9,435 |   1,117,709 |
| recycle_alloc_free_256x64b  |   445,854 |     786,390 |     193 |    9,626 |   1,124,265 |
| churn_256b                  |   343,782 |     663,152 |     162 |    9,321 |     990,197 |
| churn_write_256b            |   343,910 |     663,407 |     162 |    9,322 |     990,487 |
| multiseg_cold_256k          |   372,141 |     706,607 |     189 |    9,605 |   1,043,727 |

## Post-PERF-PASS-1 reference (2026-07-10) — the CURRENT 12-bench table

Task #49 (PERF-PASS-1, group G6/A1, source:
`docs/reviews/2026-07-10-perf-memory-layout-review.md` finding 3) added
`[profile.release]`/`[profile.bench]` codegen tuning (`lto = "thin"`,
`codegen-units = 1`) — no `[profile.*]` section existed before. This is a
pure codegen change (register allocation / code layout on the branchy
alloc/free fast path), not an allocator-behavior change, but it shifts
every bench's `Ir` and is therefore a re-pin point like every prior
binary-layout-affecting change in this file (W7a, the 11th/12th bench
additions). **Future diffs are taken against THIS table.**

Regenerated via `npm run iai` (same runner: iai-callgrind 0.14.2 in WSL,
valgrind 3.22.0, same 12 bench functions as the production-judge table
above — this section is the `production` feature set, not `hardened`).
Verified independently twice — once by the implementing agent, once by
the orchestrator re-running `npm run iai` from a clean shell — both runs
produced byte-identical numbers, confirming determinism holds across the
LTO-tuned binary too.

| bench                       |        Ir |     L1 hits | L2 hits | RAM hits | Est. Cycles | Ir/op* |
| --------------------------- | --------: | ----------: | ------: | -------: | ----------: | -----: |
| small_churn_16b             |    80,282 |     140,901 |      57 |    5,200 |     323,186 |  124.3 |
| aligned_churn_640b_a128     |    80,290 |     140,913 |      56 |    5,202 |     323,263 |  124.5 |
| large_alloc_free_cycle      |    72,325 |     131,611 |      55 |    5,211 |     314,271 |      — |
| realloc_grow                |   559,882 |   1,171,145 |   3,866 |   74,945 |   3,813,550 | 30,472.3 |
| cold_alloc_free_256x16b     |   121,641 |     189,824 |     103 |    5,280 |     375,139 |  192.6 |
| cold_alloc_free_256x64b     |   121,644 |     189,650 |     105 |    5,458 |     381,205 |  192.7 |
| recycle_alloc_free_256x16b  |   172,049 |     249,677 |     104 |    5,285 |     435,172 |  194.8 |
| recycle_alloc_free_256x64b  |   172,052 |     249,490 |     107 |    5,475 |     441,650 |  194.8 |
| churn_256b                  |    80,282 |     140,900 |      57 |    5,201 |     323,220 |  124.3 |
| churn_write_256b            |    80,538 |     141,283 |      57 |    5,202 |     323,638 |  128.3 |
| multiseg_cold_256k          |   168,902 |     297,953 |     212 |    6,380 |     522,313 | 1,420.3 |
| seg_cycle_decommit_256k     |   202,692 |     340,616 |     212 |    6,379 |     564,941 |  639.1 |

**The headline: LTO/codegen-units=1 is a real, if modest, win across the
board**, consistent with a branchy ~30-instruction fast path benefiting
from single-CGU register allocation:

| bench                        | pre-tuning Ir | post-tuning Ir |    Δ Ir |    Δ % |
| ----------------------------- | ------------: | --------------: | ------: | -----: |
| small_churn_16b               |        81,408 |          80,282 |  −1,126 | −1.4%  |
| aligned_churn_640b_a128       |        81,417 |          80,290 |  −1,127 | −1.4%  |
| large_alloc_free_cycle        |        72,982 |          72,325 |    −657 | −0.9%  |
| cold_alloc_free_256x16b       |       125,332 |         121,641 |  −3,691 | −2.9%  |
| cold_alloc_free_256x64b       |       125,335 |         121,644 |  −3,691 | −2.9%  |
| recycle_alloc_free_256x16b    |       178,070 |         172,049 |  −6,021 | −3.4%  |
| recycle_alloc_free_256x64b    |       178,073 |         172,052 |  −6,021 | −3.4%  |
| churn_256b                    |        81,408 |          80,282 |  −1,126 | −1.4%  |
| churn_write_256b              |        81,536 |          80,538 |    −998 | −1.2%  |
| multiseg_cold_256k            |       172,239 |         168,902 |  −3,337 | −1.9%  |
| seg_cycle_decommit_256k       |       210,043 |         202,692 |  −7,351 | −3.5%  |

(`pre-tuning Ir` is this session's last recorded pre-PERF-PASS-1 `npm run
check` run, itself post-#38's fmt-drift commit — the immediate prior state
before task #49 touched `Cargo.toml`. `realloc_grow` omitted from the delta
table: its `Ir` is dominated by the large-block memcpy floor per the X-arc
section above, and LTO's win there is proportionally tiny — 561,912-class
baseline vs 559,882 — well within the same pattern.) No regressions: `npm
run iai` reports **12 without regressions; 0 regressed** on this table.

## G1 honest-reject (2026-07-10) — magazine double-free oracle fold into AllocBitmap

Recorded per the project's reject-with-numbers precedent (E2 REFILL_N LUT,
X4, X5, X6). Task #50 (PERF-PASS-2) investigated folding the in-magazine
double-free scan (`dealloc_own_thread_with_base`'s O(count) pointer scan,
`src/registry/heap_core.rs`) into `AllocBitmap` — the source review's
top-ranked finding, estimated at ~25-35% of the small-alloc gap vs
mimalloc. **REJECT — not implemented, no code changed for this part.**

**Why:** tracing every real call site (not just the review's sketch) shows
the redefinition ("bit 1 = not owned by user, set on magazine push, clear
on pop") requires *inverting* an existing, documented, load-bearing
optimization at multiple sites, not a free relabeling:

- `refill_class_bump_impl`'s freelist-drain leg (`drain_freelist_batch`/
  `pop_free`) already calls `mark_alloc` on the premise "the block leaves
  the free list and is handed to the caller" — false once the destination
  can be the magazine instead of the user, at every one of these call
  sites (`alloc_core.rs` ~2933-2936, ~3083, ~3162, ~3209).
- `refill_class_bump`'s bump-carve leg (`carve_batch`) deliberately leaves
  the bit unset as a documented prior optimization — also inverted by the
  proposed redefinition.
- `reclaim_offset_checked` (the cross-thread ring-drain path, `#[cfg(feature
  = "fastbin")]`) already runs `is_free(off)` PLUS a separate `is_in_magazine`
  O(count) scan specifically *because* today's bitmap is blind to magazine
  residency. Folding residency into the bit would make `is_in_magazine`
  redundant — a real behavior change to the H1-adjacent cross-thread reclaim
  protocol that both the task and the source review explicitly wanted to
  leave untouched.

A single alloc call can legitimately set up to 32 consecutive bits in one
shot (1 requested + `REFILL_BATCH = 31` refilled blocks,
`carve_block_with_refill`), which the review's simple "set on push, clear on
pop" framing did not account for.

**Verification the reject is sound (not a shortcut):** the M2 double-free
counterfactual tests (`t2_double_free_magazine_block_is_noop`,
`t2_triple_free_still_noop`, `t2_double_free_64b_class`) were temporarily
broken (in-magazine scan disabled) to confirm they are non-vacuous — all
three went RED as expected, `T3` (which relies on the separate bitmap
oracle, untouched by this reject) correctly stayed green. Reverted; full M2
suite green again.

**Measured (honest, not just reasoned):** the magazine-hit benches this
finding specifically targeted show **exactly 0.0 Ir/op delta** (marginal,
bootstrap-subtracted) vs the Post-PERF-PASS-1 reference — `small_churn_16b`
124.3→124.3, `aligned_churn_640b_a128` 124.5→124.5, `churn_256b`
124.3→124.3, `churn_write_256b` 128.3→128.3. No regression, no improvement —
consistent with "no code changed" rather than a silent behavior shift.

**If a future arc revisits this:** the shape to try is NOT a simple bit
redefinition but a design that (a) audits and updates every `mark_alloc`/
`mark_free` call site's semantics consistently (the four sites named above,
at minimum), and (b) resolves whether `is_in_magazine`'s separate scan in
`reclaim_offset_checked` becomes provably redundant or must be kept for the
cross-thread case specifically — that analysis was not completed here (out
of scope once the sub-part was correctly identified as non-trivial) and is
the actual blocker, not a fundamental soundness objection to the idea.

Final tree change for this rejected sub-part: none (zero diff for G1
specifically; task #50's other sub-parts — G5/C1 virgin-init elision, G10/D2
code-motion — landed independently, see the reference table below).

## Post-PERF-PASS-2 reference (2026-07-10) — the CURRENT 12-bench table

Task #50 (PERF-PASS-2, group G5/C1, source:
`docs/reviews/2026-07-10-perf-large-segment-review.md` finding 3) elided the
fresh-segment `AllocBitmap` virgin-init: `AllocCore::reserve_small_segment`
and `bootstrap::primordial` now skip the explicit 32 KiB zero-write under
`cfg(not(miri))`, relying on the OS's demand-zero guarantee for freshly
reserved pages (verified empirically on this platform by
`tests/regression_virgin_bitmap_skip.rs`'s poison-then-assert counterfactual
tests). This is a real allocator-behavior change — every bench pays the
primordial-segment bootstrap once via `SeferAlloc::new()`/`AllocCore::new()`,
so the bootstrap constant itself drops (`large_alloc_free_cycle`:
72,325 → 39,532 Ir). **Future diffs are taken against THIS table.**

G1 (folding the magazine double-free oracle into the same bitmap) was
investigated and REJECTED — see the section above — so this table reflects
ONLY the virgin-init elision (G5/C1) and the code-motion cleanup (G10/D2,
`dealloc_foreign_slow` outlining, Ir-neutral by construction).

Regenerated via `npm run iai`, same runner (iai-callgrind 0.14.2 in WSL,
valgrind 3.22.0, same 12 bench functions). Verified independently twice —
once by the implementing agent, once by the orchestrator re-running `npm
run iai` from a clean shell after personally reviewing and lightly cleaning
up the diff (removing two lines of dead code the agent left behind) — both
runs produced byte-identical numbers.

| bench                       |        Ir |     L1 hits | L2 hits | RAM hits | Est. Cycles | Ir/op* |
| --------------------------- | --------: | ----------: | ------: | -------: | ----------: | -----: |
| small_churn_16b             |    47,489 |      75,851 |      50 |    4,690 |     240,251 |  124.3 |
| aligned_churn_640b_a128     |    47,498 |      75,868 |      48 |    4,689 |     240,223 |  124.5 |
| large_alloc_free_cycle      |    39,532 |      66,561 |      51 |    4,698 |     231,246 |      — |
| realloc_grow                |   527,064 |   1,106,071 |   3,859 |   74,440 |   3,730,766 | 30,470.8 |
| cold_alloc_free_256x16b     |    88,818 |     124,750 |      94 |    4,766 |     292,030 |  192.5 |
| cold_alloc_free_256x64b     |    88,821 |     124,604 |      94 |    4,948 |     298,254 |  192.5 |
| recycle_alloc_free_256x16b  |   139,376 |     184,631 |      97 |    4,773 |     352,171 |  195.0 |
| recycle_alloc_free_256x64b  |   139,379 |     184,444 |      97 |    4,966 |     358,739 |  195.0 |
| churn_256b                  |    47,489 |      75,852 |      50 |    4,689 |     240,217 |  124.3 |
| churn_write_256b            |    47,745 |      76,236 |      50 |    4,689 |     240,601 |  128.3 |
| multiseg_cold_256k          |    70,525 |     102,884 |      69 |    4,875 |     273,854 |  455.8 |
| seg_cycle_decommit_256k     |   104,335 |     145,546 |      69 |    4,875 |     316,516 |  317.7 |

**The headline: virgin-init elision is a large, real win on every fresh-
segment-heavy bench, and Ir-neutral (as expected) on pure magazine-hit
churn:**

| bench                        | Post-PERF-PASS-1 Ir/op* | Post-PERF-PASS-2 Ir/op* |     Δ |
| ----------------------------- | -----------------------: | ------------------------: | ----: |
| small_churn_16b               |                    124.3 |                     124.3 |   0.0 |
| aligned_churn_640b_a128       |                    124.5 |                     124.5 |   0.0 |
| churn_256b                    |                    124.3 |                     124.3 |   0.0 |
| churn_write_256b              |                    128.3 |                     128.3 |   0.0 |
| cold_alloc_free_256x16b       |                    192.6 |                     192.5 |  −0.1 |
| cold_alloc_free_256x64b       |                    192.7 |                     192.5 |  −0.2 |
| multiseg_cold_256k            |                  1,420.3 |                     455.8 | −964.5 |
| seg_cycle_decommit_256k       |                    639.1 |                     317.7 | −321.4 |

`multiseg_cold_256k` and `seg_cycle_decommit_256k` (the fresh-segment-heavy
benches) show the real win — each fresh segment carved during the bench no
longer pays the 32 KiB bitmap zero-loop. Churn benches (pure magazine-hit,
no fresh segments after the first) are exactly flat, honestly reflecting
that this pass changed cold-path behavior only. No regressions: `npm run
iai` reports **12 without regressions; 0 regressed** on this table.

## Post-PERF-PASS-3 reference (2026-07-10) — the CURRENT 12-bench table

Task #51 (PERF-PASS-3, group G2/B1 + G11/D3, source: docs/reviews/2026-07-10-
perf-churn-reuse-review.md + docs/reviews/2026-07-10-perf-large-segment-
review.md) added the Mechanism-2 empty-small-segment hysteresis pool and
large-cache best-fit. This is deliberately NOT an `npm run iai` story — the
iai bench suite (256B/1024B-scale, ≤3 segments per bench) does not exercise
the working-set-oscillation-across-a-segment-boundary shape the pool exists
to fix; that shape is judged by the `working_set_cycle` wall-clock bench
(task #49) instead, since iai/Ir is provably blind to the page-fault/syscall
cost class this mechanism targets (see the churn-reuse review's own finding
3). The table below is regenerated here purely to re-pin the binary-layout-
scale shift the new code causes (every build now includes the pool
bookkeeping, whether or not a given bench's workload ever touches it).

| bench                       |        Ir |     L1 hits | L2 hits | RAM hits | Est. Cycles | Ir/op* |
| --------------------------- | --------: | ----------: | ------: | -------: | ----------: | -----: |
| small_churn_16b             |    47,509 |      75,891 |      38 |    4,693 |     240,336 |  124.2 |
| aligned_churn_640b_a128     |    47,518 |      75,907 |      36 |    4,693 |     240,342 |  124.3 |
| large_alloc_free_cycle      |    39,561 |      66,619 |      39 |    4,701 |     231,349 |      — |
| realloc_grow                |   527,089 |   1,106,123 |   3,833 |   74,445 |   3,730,863 | 30,470.5 |
| cold_alloc_free_256x16b     |    88,823 |     124,746 |      79 |    4,771 |     292,126 |  192.4 |
| cold_alloc_free_256x64b     |    88,826 |     124,600 |      79 |    4,953 |     298,350 |  192.4 |
| recycle_alloc_free_256x16b  |   139,321 |     184,526 |      82 |    4,774 |     352,026 |  194.8 |
| recycle_alloc_free_256x64b  |   139,324 |     184,335 |      83 |    4,970 |     358,700 |  194.8 |
| churn_256b                  |    47,509 |      75,890 |      38 |    4,694 |     240,370 |  124.2 |
| churn_write_256b            |    47,765 |      76,272 |      38 |    4,696 |     240,822 |  128.2 |
| multiseg_cold_256k          |    70,548 |     102,873 |      55 |    4,884 |     274,088 |  455.7 |
| seg_cycle_decommit_256k     |   104,374 |     145,423 |      55 |    4,884 |     316,638 |  317.7 |

Every bench moves by ≤55 Ir vs the Post-PERF-PASS-2 reference (noise-band,
consistent with pure binary-layout shift, not an algorithmic change to any
of the 12 exercised workloads). No regressions: `npm run iai` reports **12
without regressions; 0 regressed** on this table.

**The real judge — `working_set_cycle` (wall-clock, task #49's bench),
personally reru­n by the orchestrator against this build:**

| size | decommit_calls delta | wall-clock change |
|---|---:|---|
| 64B | 0 (fully absorbed by the pool) | −16.3% (improved) |
| 256B | 173 (bench's 64-working-set demand exceeds the 4-segment pool at this size) | not statistically significant (noise) |
| 1024B | 367 (same — demand exceeds pool capacity) | −13.8% (improved) |

The pool fully eliminates decommit churn only when a workload's oscillating
footprint fits inside the bounded 4-segment/16 MiB cap (as the 64B case
does); at larger sizes or wider working sets the pool still absorbs a
meaningful fraction of the churn and the wall-clock improves, but does not
reach the `fastbin`-bisect upper bound this session's investigation
identified — reported honestly rather than tuning the pool size to make one
specific bench look artificially good.

## Post-PERF-PASS-5 reference (2026-07-10) — the FINAL 12-bench table of this session's investigation

Task #53 (PERF-PASS-5, group G7, source: docs/reviews/2026-07-10-perf-
memory-layout-review.md findings 1/5/6 + docs/reviews/2026-07-10-perf-
fastpath-review.md finding 2) reordered `SegmentHeader` and restructured
`Tcache` for cache-line locality (see the two commits landing this
task for the full per-change writeup). This is the **final** re-pin of
this session's 5-pass performance investigation (tasks #49-#53).

| bench                       |        Ir |     L1 hits | L2 hits | RAM hits | Est. Cycles | Ir/op* |
| --------------------------- | --------: | ----------: | ------: | -------: | ----------: | -----: |
| small_churn_16b             |    42,880 |      65,883 |     150 |    4,872 |     237,153 |  124.2 |
| aligned_churn_640b_a128     |    42,889 |      65,897 |     149 |    4,872 |     237,162 |  124.4 |
| large_alloc_free_cycle      |    34,929 |      56,607 |     149 |    4,883 |     228,257 |      — |
| realloc_grow                |   522,484 |   1,096,093 |   3,849 |   74,725 |   3,730,713 | 30,472.2 |
| cold_alloc_free_256x16b     |    84,240 |     114,752 |     163 |    4,978 |     289,797 |  192.6 |
| cold_alloc_free_256x64b     |    84,243 |     114,604 |     163 |    5,162 |     296,089 |  192.6 |
| recycle_alloc_free_256x16b  |   134,386 |     174,167 |      85 |    5,086 |     352,602 |  194.3 |
| recycle_alloc_free_256x64b  |   134,329 |     173,919 |      85 |    5,280 |     359,144 |  194.1 |
| churn_256b                  |    42,880 |      65,884 |     150 |    4,871 |     237,119 |  124.2 |
| churn_write_256b            |    43,007 |      66,137 |     150 |    4,873 |     237,442 |  126.2 |
| multiseg_cold_256k          |    66,009 |      92,906 |      79 |    5,166 |     274,111 |  457.1 |
| seg_cycle_decommit_256k     |   100,023 |     135,640 |      79 |    5,166 |     316,845 |  319.1 |

**The headline: a real win concentrated in one-time bootstrap cost, flat
(within noise) on the per-op marginal cost:**

| bench                        | Post-PERF-PASS-3 Ir | Post-PERF-PASS-5 Ir |    Δ Ir |    Δ % | Ir/op* Δ |
| ----------------------------- | -------------------: | --------------------: | ------: | -----: | -------: |
| small_churn_16b                |               47,509 |                42,880 |  −4,629 |  −9.7% |      0.0 |
| large_alloc_free_cycle          |               39,561 |                34,929 |  −4,632 | −11.7% |        — |
| cold_alloc_free_256x16b         |               88,823 |                84,240 |  −4,583 |  −5.2% |     −0.2 |
| recycle_alloc_free_256x16b      |              139,321 |               134,386 |  −4,935 |  −3.5% |     −0.5 |
| multiseg_cold_256k              |               70,548 |                66,009 |  −4,539 |  −6.4% |     +1.4 |
| seg_cycle_decommit_256k         |              104,374 |               100,023 |  −4,351 |  −4.2% |     +1.4 |

Every bench drops by roughly the same ~4,400-4,900 raw Ir (the tighter
`SegmentHeader`/`Tcache` layout means less zero-init/construction work per
heap), while the bootstrap-adjusted marginal `Ir/op*` is essentially flat
on the churn benches (0.0 delta) — consistent with the source review's own
tempered claim that layout alone would not close the 2x small-alloc gap
on its own, being a supporting optimization rather than the primary lever
(that role belongs to G1, investigated and honestly rejected this session,
and G2/Mechanism-2, the session's largest wall-clock win). No regressions:
`npm run iai` reports **12 without regressions; 0 regressed** on this
table.

### Session summary — five passes, eleven action groups

This concludes the 5-pass implementation of docs/perf/PERF_PLAN_2026-07-10-
post-review-action-plan.md (itself the synthesis of five parallel research
reviews, `docs/reviews/2026-07-10-perf-*.md`):

- **PERF-PASS-1** (task #49): `[profile.release]` LTO tuning, bench-harness
  fixes (untimed teardown + the `working_set_cycle` judge), vmem
  reserve-then-commit-exact (Windows) / exact-mmap-first (Unix).
- **PERF-PASS-2** (task #50): fresh-segment `AllocBitmap` virgin-init
  elision, `dealloc_foreign_slow` outlining. G1 (magazine double-free
  oracle fold) investigated and honestly REJECTED — see that section above.
- **PERF-PASS-3** (task #51): the Mechanism-2 committed-segment hysteresis
  pool (this session's largest wall-clock win — full decommit-churn
  elimination at 64B, partial at larger sizes) + large-cache best-fit.
- **PERF-PASS-4** (task #52): the ring-drain empty-guard (dead
  `RemoteFreeRing::is_empty()` wired via a targeted `tail_relaxed()`
  primitive) + `HeapSlot`/`RemoteFreeRing` false-sharing partition (the
  physical residue of the H1 hoist).
- **PERF-PASS-5** (task #53): `SegmentHeader`/`Tcache` cache-line reorder
  (this table) — `AllocCore` field reorder (G7/ML6) measured and reported
  as a no-op under the current `repr(Rust)` layout algorithm, honestly,
  rather than forced.

Every task followed this repo's zero-trust methodology: `sx`-agent
implementation, personal diff review, personal re-run of the full test
suite / clippy / `npm run check` / `npm run iai` by the orchestrator (not
just trusting the implementing agent's own report), before each commit.

## RAD-5 GO (2026-07-11/12) — `MagazineBitmap`, a second orthogonal per-segment bitmap

Task #58 (RAD-5, plan Phase 5/E4, `docs/perf/PERF_PLAN_2026-07-10-radical-
audit-implementation-plan.md`) revisited the G1 honest-reject above with a
different shape: instead of *redefining* `AllocBitmap`'s semantics (the
shape G1 rejected — see that section), add a second, orthogonal bitmap
(`src/alloc_core/magazine_bitmap.rs`, `MagazineBitmap`) recording ONLY
magazine residency, leaving every `AllocBitmap` call site byte-identical.
This closes G1's stated blocker cleanly: no `mark_alloc`/`mark_free`
call-site semantics change, so `carve_batch`'s leave-unset optimization and
the freelist-drain legs are untouched.

Replaces two O(count) scans with an O(1) bitmap probe: the own-thread free
path's in-magazine double-free oracle (`heap_core.rs`'s
`dealloc_own_thread_with_base`, previously the Э10 branchless chunked scan
over `tcache.classes[c].slots[0..cnt]`) and the cross-class magazine
predicate inside `refill_magazine_slow` / `dbg_drain_all_rings`
(previously an O(cnt) scan of every OTHER class's slots via
`before`/`after` split halves). Mark on magazine push (own-thread free,
both the in-place push and the overflow-push leg) and on refill for every
block landing in the magazine; clear on magazine pop (alloc hit, refill
issue) and magazine flush (both production half-flush and the test-only
full flush). 32 KiB / segment (0.78% of 4 MiB), carved into segment
metadata right after `AllocBitmap`, with the same virgin-init-skip
discipline (`cfg(not(miri))` elision at the two fresh-reserve call sites,
unconditional re-init on decommit-reset) extended and counterfactual-tested
in `tests/regression_virgin_bitmap_skip.rs`.

**Verdict: GO.** Measured independently by the orchestrator (not just
trusting the implementing agent's own numbers) — clean stash/pop
apples-to-apples on this exact tree, `npm run iai` (iai-callgrind
0.14.2 / WSL / valgrind), 12 without regressions, 0 regressed both with
and without the diff:

| bench                       | baseline Ir | RAD-5 Ir | Δ raw Ir | baseline Ir/op* | RAD-5 Ir/op* | Δ Ir/op* |
| ---------------------------- | ----------: | -------: | -------: | --------------: | -----------: | -------: |
| small_churn_16b              |      37,265 |   33,938 |   −3,327 |            125.0 |          73.0 |    −52.0 |
| aligned_churn_640b_a128       |      37,274 |   33,948 |   −3,326 |            125.2 |          73.2 |    −52.0 |
| churn_256b                   |      37,265 |   33,938 |   −3,327 |            125.0 |          73.0 |    −52.0 |
| churn_write_256b             |      37,392 |   34,130 |   −3,262 |            127.0 |          76.0 |    −51.0 |
| cold_alloc_free_256x16b      |      81,130 |   75,987 |   −5,143 |            202.6 |         182.5 |    −20.1 |
| recycle_alloc_free_256x16b   |     133,803 |  123,712 |  −10,091 |            204.2 |         184.5 |    −19.7 |

- **Churn kill gate (±10 raw Ir threshold, X4-B precedent):** all four
  churn benches IMPROVED by ~3,260–3,330 raw Ir — roughly 300× past the
  kill threshold in the favorable direction, not a marginal pass.
- **Cold/recycle honest-budget gate (task spec: 15–25 Ir/op improvement
  against a 68 Ir/op budget):** landed at 19.7–20.1 Ir/op improvement,
  inside the expected window.
- The implementing agent's own independently-run numbers (same method,
  separate WSL invocation) were 51.0/49.0 Ir/op churn improvement and
  16.7/16.9 Ir/op cold/recycle improvement — small run-to-run variance
  (~1-3 Ir/op) from the orchestrator's rerun, same direction and order of
  magnitude, both readings comfortably clear both gates.

**Why the a-priori cost-model prediction (regression expected) was wrong:**
the replaced scan sat inside a runtime-variable-trip-count loop
(`while i < chunks` / `while i < cnt`) on the free hot path even though it
executes 0-1 times in practice for these benches (`cnt` small); replacing
it with the bitmap's unconditional straight-line probe let the compiler
generate a tighter fast path overall, outweighing the new store's own
cost. Not measurement noise — reproduced 3× byte-identical by the
implementing agent, and independently reproduced by the orchestrator via a
full stash/pop clean-baseline rerun (numbers above).

**Verification (personally re-run by the orchestrator, not trusted from
the agent's report):** every one of the 8 changed/new files read line by
line; the layout-offset chain (`magazine_bitmap_off` → `remote_ring_off` →
`gen_table_off` → `run_stack_off` → `primordial_*_off`) traced and
confirmed self-consistent via the existing `small_meta_end() + PAGE <=
SEGMENT` const-asserts (all 3 CI feature matrices compile clean); `cargo
clippy --all-targets -D warnings` clean on all 3 matrices; `cargo fmt
--check` clean; `cargo test --release --features production` — 48/49 test
binaries green, the one failure being the pre-existing, unrelated
`docs/ARCHITECTURE.md` test-file-count drift (134 files, fixed separately);
the M2 counterfactual personally broken (`if false &&
meta.magazine_bitmap().is_in_magazine(off)`) and confirmed RED
(`in_magazine_double_free_is_noop` failed with the exact "same pointer
issued twice" signature) then restored and confirmed GREEN;
`drain_resident_xthread_double_free_no_corruption`,
`refill_window_does_not_double_issue_in_out_buffer_resident_block`,
`realloc_path_drain_respects_magazine` all green; `miri` run directly on
`regression_virgin_bitmap_skip` (the load-bearing virgin-init-skip
extension) — all 3 tests (T1/T2/T3) pass, confirming the skip is sound for
the new bitmap too, not just `AllocBitmap`.

Files: `src/alloc_core/magazine_bitmap.rs` (new), `src/alloc_core/mod.rs`,
`src/alloc_core/segment_header.rs`, `src/alloc_core/bootstrap.rs`,
`src/alloc_core/alloc_core_small.rs`, `src/alloc_core/alloc_core_small_pool.rs`,
`src/registry/heap_core.rs`, `tests/regression_virgin_bitmap_skip.rs`.

**Note on baseline staleness:** the Post-PERF-PASS-5 reference table above
predates RAD-1 through RAD-4 and UBFIX-1 through UBFIX-12 (all landed
between that table and this entry) and is no longer the live baseline for
future comparisons — this entry's "baseline" column is a fresh clean-HEAD
measurement taken immediately before this diff, not a read from that
table. A general re-pin of the reference table is left as a follow-up (not
blocking this GO decision, which only needed the relative before/after
delta on this exact tree).

## RAD-4b GO (2026-07-12) — `HeapOverflow`, a slot-resident second-chance
## overflow ring closing RAD-4's owner-starved residual

Task #72 (RAD-4b) revisited RAD-4's (`8b91b85`) explicitly-accepted residual:
under FULL owner starvation (the owning thread performs zero `alloc()` calls
for a producer burst's entire duration), `push_with_overflow_retry`'s bounded
retry has nothing to wait on, and the design conceded to the original
documented-sound bounded leak — measured at 744/1000 blocks lost in
`tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_bounded`'s
pathological shape. The task brief posed three candidate designs and invited a
4th if none held up.

### Design comparison (the honest call)

1. **Real backpressure (`dealloc()` blocks until drained).** REJECTED. The
   crate has zero blocking primitives anywhere (`grep -rn "spin_loop\|park\|
   Condvar" src/` — only spin-hints exist), is `no_std`-capable (a `Condvar`/
   futex needs OS-specific gating that would either break `no_std` or add a
   large new conditional surface), and — the decisive argument — blocking
   does not actually strengthen the guarantee: if the owner thread is
   genuinely dead (not merely busy), a blocked producer waits FOREVER, an
   unrecoverable deadlock strictly worse than the bounded leak it replaces.
   Converts a resource-cost failure mode into an availability failure mode
   for every thread in the process that frees memory. New synchronisation
   primitive = new H1-class risk surface for a net-negative reliability
   trade.
2. **Slot-resident buffer + provenance-exposed `SegmentHeader` field.**
   PARTIALLY adopted, redesigned. The task brief's sketch (a new
   `owner_overflow: *const _` header field, provenance-exposed like
   `owner_thread_free`) was replaced with something cheaper and lower-risk:
   every segment ALREADY carries its owner's heap-slot id in `owner_state`
   (`unpack_owner_id`, stamped by `stamp_segment_owner` on every alloc — the
   same field `dbg_owner_id_for` already reads cross-thread). A remote
   producer resolves the owning `&'static HeapSlot` via a single
   bounds-checked `bootstrap::ensure().slots[owner_id]` array index — plain
   safe Rust, no new `SegmentHeader` field, no provenance-exposure machinery,
   zero layout risk to that already-heavily-audited 120-byte struct.
3. **Properly fix M-7 (tag `next_abandoned`).** REJECTED for this task, as
   the brief itself anticipated: `next_abandoned` is a SEGMENT-identity
   queue (one link per segment); the lost item here is a BLOCK inside a
   still-live segment — reusing that link only helps if the whole segment
   is requeued, a coarser and semantically different operation. Also touches
   `SegmentHeader` layout a second time in the same session (RAD-3/RAD-5
   already shifted it) and would need a full re-derivation of M-7's
   documented-dormant-hazard safety argument under a third concurrent
   consumer — the audit's own "riskiest of the three" verdict, confirmed.
4. **The shape actually shipped: `HeapOverflow`.** A bounded (`HEAP_OVERFLOW_CAP
   = 2048`, 8× `RemoteFreeRing::RING_CAP`), slot-resident, per-HEAP MPSC ring
   (`src/registry/heap_overflow.rs`), reusing `RemoteFreeRing`'s
   already-proven Vyukov push/drain CAS-reserve protocol byte-for-byte in
   shape, built from plain safe-Rust atomics (no `unsafe`, no seam) since a
   `HeapSlot` is an ordinary `'static` Rust struct rather than raw `mmap`'d
   segment bytes. `push_with_overflow_retry` tries this ring AFTER its
   existing per-segment retry budget is exhausted, BEFORE conceding to the
   bounded leak. Each entry is `(segment_base, packed_offset_class)` — one
   heap owns many segments, so the base must travel with the entry (a
   per-segment ring does not need this).

**Honest scope of the guarantee.** No FIXED-capacity, non-blocking, `Box`-free
structure can give a mathematically absolute guarantee against a producer
population with unbounded throughput and a consumer that never runs again for
the rest of the process's life — this is true of `HeapOverflow` exactly as it
was already true of `RemoteFreeRing` (a bigger cap is a bigger bound, not
"unbounded"). What RAD-4b delivers is the strongest guarantee a bounded,
non-blocking, reentrancy-safe mechanism can: **zero loss for any burst that
fits the configured capacity** — which is the literal, honestly-measured
judge this task's mandate specifies. `HEAP_OVERFLOW_CAP = 2048` is 2× the
mandated pathological-starvation judge's own burst (N=1000, 8 producers).

### Verification

**RED→GREEN (personally re-verified, non-vacuous, exact historical
signature):** `push_to_heap_overflow`'s call site in `push_with_overflow_retry`
temporarily short-circuited to `if false && Self::push_to_heap_overflow(...)`,
re-ran `remote_fanin_owner_starved_residual_is_bounded` — **RED**, panicked
with `exhausted_delta == 744` (matching the task's own pre-existing 744/1000
historical measurement exactly, proving the counterfactual is not vacuous),
`reclaimed_after=1000` (the segment itself was still recovered — this is the
distinction between "block accounting lost" and "segment leaked", the same
distinction RAD-4's own module doc draws). Restored the real call — **GREEN**,
`exhausted_delta=0`, `reclaimed_after=1000`. Repeated after the final
optimisation pass (handle-hoist, below) — same RED (744) / GREEN (0) result,
confirming the optimisation did not change behaviour.

**loom:** `tests/loom_heap_overflow.rs` (new) isolates the ONE genuinely new
protocol detail beyond what `loom_remote_ring.rs` already proves for
`RemoteFreeRing`'s shared push/drain shape: `HeapOverflow`'s entry is a PAIR
of atomics (`base`, `packed`), publish-ordered `packed` (Relaxed) then `base`
(Release) so `base` is the "is this slot published" gate. A `#[should_panic]`
counterfactual (`counterfactual_wrong_publish_order_tears_entry`) inverts
that order and loom FINDS the interleaving producing a torn read (a correct
`base` paired with a stale/zero `packed`) — non-vacuous. 3/3 tests green.

**miri:** the existing full-integration harness
(`remote_fanin_miri_minimal_retry_ub_check`) did not complete in a reasonable
time on the development host even before this task (its own doc already
warned "impractically slow… even after aggressive scale-down"); `HeapOverflow`
growing the registry by ~24 KiB/slot × `MAX_HEAPS` made it measurably worse
(observed non-terminating after 5+ minutes / 18+ GB RSS under miri's
interpreter, vs. the pre-existing harness's own already-marginal runtime).
Added `tests/miri_heap_overflow_unit.rs` — a standalone, `Box`-allocated
`HeapOverflow` (via a new `#[doc(hidden)] pub` test constructor,
`HeapOverflow::new_boxed_for_test`, mirroring `RemoteFreeRing::
over_test_buffer`'s established isolated-ring-test pattern) with NO registry,
NO `bootstrap::ensure()` — just the ring's own push/drain, driven by two
concurrent producer threads plus a sequential wrap-adjacent test. Completes in
under 1 second under miri; both tests pass, no UB (the one informational
"integer-to-pointer cast" note is expected — `base` is intentionally stored as
`usize`, the same exposed-address discipline the crate's existing
`Node::atomic_ptr_ref` machinery already uses elsewhere, not a soundness
finding).

**Full xthread regression suite:** `regression_realloc_xthread_stamp`,
`regression_xthread_double_free_residual` (2 pass, 1 correctly still
`#[ignore]`d — the unrelated X7-pinned residual), `regression_xthread_large_
free_layout_mismatch`, `regression_xthread_large_free_no_leak`,
`fastbin_requires_xthread` — all green, unchanged. `heap_core_bulk_bypass`,
`heap_core_tcache*`, `registry_basic`, `regression_registry_initialised_gate`
— all green, unchanged.

**clippy** (all 3 CI feature-matrix entries: `""`, `--features experimental`,
`--all-features`) — clean, zero warnings. **`cargo fmt --check`** — clean.

**RSS judge** (`examples/first_alloc_process.rs`, the RAD-1 first-touch
regression guard): `rss_after_1_heap_kib − rss_before_kib` stayed at ~116–120
KiB (RAD-1's own ~0.1 MiB baseline), confirming `HeapOverflow`'s all-zero
initial state (`ENTRY_EMPTY_BASE = 0`, matching OS-zeroed pages exactly — the
same "never write it, so it's never first-touched" discipline RAD-1
established) means claiming a heap does NOT first-touch its ~24 KiB overflow
array. Growing `Registry`'s total VIRTUAL footprint by ~96 MiB (`24 KiB ×
MAX_HEAPS = 4096`) costs nothing in RSS for a slot that never overflows —
cheap on 64-bit address space.

### iai — measured, iterated three times, final numbers pass the churn gate

Isolated via a read-only `git worktree add --detach <tmp> HEAD` (HEAD =
`06c04ba`, the sibling UBFIX-13 task's landed commit, tree clean of this diff)
so the delta below is EXCLUSIVELY this task's contribution, not conflated with
concurrent sibling-task changes in the same session.

**Iteration 1 (initial `drain_heap_overflow` call in `alloc()`'s
non-fastbin branch + `refill_magazine_slow`'s fastbin branch, unconditional
2-atomic-load `HeapOverflow::drain` on every call):**

| bench | baseline (HEAD) | iter 1 | Δ |
|---|---:|---:|---:|
| small_churn_16b | 34,015 | 34,029 | +14 |
| churn_256b | 34,015 | 34,029 | +14 |
| cold_alloc_free_256x16b | 76,828 | 77,052 | +224 |
| recycle_alloc_free_256x16b | 125,260 | 125,634 | +374 |

`+14` on the churn benches exceeds the `±10` churn kill-gate (X4-B
precedent). Root cause: `refill_magazine_slow` already carries `drain_large_
deferred_free`'s "cheap when empty (one Acquire load)" cost (M-9, accepted);
`drain_heap_overflow` added a SECOND, structurally more expensive check
(a Vyukov ring needs BOTH cursors — `head`/`tail` — to prove empty, unlike a
Treiber stack's single `head.is_null()`), on the same magazine-miss refill
path the churn benches' first iteration hits exactly once.

**Iteration 2 (added `HeapOverflow::is_likely_empty`, a single-load
Relaxed-tail-vs-cached-`Relaxed`-head pre-check before the full Acquire-pair
drain):** `+12` — marginal improvement (2 Ir), confirmed the atomics
themselves were not the dominant cost.

**Iteration 3 (shipped): hoisted the `&'static HeapOverflow` handle to
`HeapCore` at claim time** (`HeapCore::bind_overflow`, mirroring
`bind_thread_free`/`bind_tcache_hits`'s existing claim-time-binding
discipline exactly), replacing `drain_heap_overflow`'s per-call
`bootstrap::ensure()` + `MAX_HEAPS`-bounds-checked array index with a
pre-resolved reference, and cached the ring's own `tail` progress in
`HeapCore::overflow_tail_cache` (an owner-private `usize`, refreshed from
`HeapOverflow::drain`'s return value) so the common "never overflowed" case
costs one `Relaxed` load against a plain cached integer, mirroring the
existing `last_stamped_segment` OPT-C cache immediately adjacent in the same
struct:

| bench | baseline (HEAD) | shipped | Δ | Ir/op* Δ |
|---|---:|---:|---:|---:|
| small_churn_16b | 34,015 | 34,024 | **+9** | +0.14 |
| aligned_churn_640b_a128 | 34,025 | 34,034 | +9 | +0.14 |
| churn_256b | 34,015 | 34,024 | +9 | +0.14 |
| churn_write_256b | 34,271 | 34,280 | +9 | +0.14 |
| cold_alloc_free_256x16b | 76,828 | 76,927 | +99 | +0.38 |
| recycle_alloc_free_256x16b | 125,260 | 125,389 | +129 | +0.50 |

**Churn kill gate: `+9` raw Ir — inside the `±10` threshold.** Reproduced
byte-identically across two independent re-runs (callgrind's Ir count is
deterministic, not statistically noisy). Cold/recycle carry a larger absolute
delta (many refills per bench run, each paying the one-time hoisted-handle
check) but the SAME relative order of magnitude as the already-accepted M-9
`drain_large_deferred_free` addition to this exact function.

**Verdict: GO**, with the residual `+9`/`+99`/`+129` Ir cost disclosed
plainly rather than chased to zero — the floor cost of a second opportunistic
empty-check on the SAME magazine-miss refill path M-9 already instrumented,
now genuinely as cheap as that mechanism's own single-cached-value check.

Files: `src/registry/heap_overflow.rs` (new), `src/registry/heap_core.rs`,
`src/registry/heap_registry.rs`, `src/registry/heap_slot.rs`,
`src/registry/mod.rs`, `src/alloc_core/alloc_core.rs` (one `small_cur()`
accessor), `tests/remote_fanin.rs` (harness 2 rewritten to assert
`exhausted_delta == 0`), `tests/loom_heap_overflow.rs` (new),
`tests/miri_heap_overflow_unit.rs` (new).
