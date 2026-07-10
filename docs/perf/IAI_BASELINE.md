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

**Current reference for new work:** the "Post-X1+X2+X3 reference (2026-07-05)"
section below captures 11 bench fns and is the last fully-tabulated reference
in this file. Since then a 12th bench fn — `seg_cycle_decommit_256k` (task #14,
the PERF-4 decommit→recycle segment-churn probe) — was added to
`benches/perf_gate_iai.rs`; adding a bench fn shifts every other bench's Ir via
pure binary layout, so once its numbers are captured the 11-bench table is
superseded as a diff target. Regenerate the full table with `npm run iai`
before diffing new work; this file retains the historical post-W3 (10-bench)
baseline above for provenance only — do not diff against it.

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
