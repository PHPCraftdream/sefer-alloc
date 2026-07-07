# X-arc PERF-3 honest-reject — run-encoded freelist for the recycle path

**REJECT.** The `alloc-runfreelist` feature (Ф1 storage+layout, Ф2 flush-side
contiguous-run detection, Ф3 drain-side stride-reconstruction, Ф4 lifecycle
seams — all landed and reviewed GO by @o46m) was measured against the 11-bench
iai reference AND the wall-clock `global_alloc` cold-storm criterion (the exact
`bench_direct_alloc` pattern the design doc §1 named as the target). The feature
**regressed every single one of the 11 iai benches**, including the four explicit
cold/recycle targets it was designed to improve, and the wall-clock judge
**confirmed** the regression direction and magnitude on both the 16 B and 64 B
storm shapes (+40 % / +43 %). The "eliminate the dependent-load pointer chase on
the recycle drain path" hypothesis (design doc §1) is **refuted by direct
measurement** in both judges: the saved `read_next` loads are dramatically
outweighed by the per-flush cost the implementation adds (offset collection +
insertion sort + contiguous scan + descriptor push, on top of — not instead of —
the classic per-block `write_next` chain build).

The feature stays **off / opt-in-only** (NOT added to `production`). The source
is left in place as an experimental, off-by-default feature — see "Source
disposition" for the reasoning. `git diff` against the pre-doc tree is empty in
`src/`; this doc is the only new file.

Recorded per the project's reject-with-numbers precedent (PERF-2 / X4-A / X5 /
X6 ledger entries), so the next reader does not re-run the same experiment blind.
This is the companion to PERF-2: PERF-2 established that mimalloc's small-size
advantage is a *structurally cheaper refill*, not a deeper magazine, and named
"cheaper per-block work on the hot recycle path" as the winning family of attack.
PERF-3 was the concrete realization of that family for the recycle flush→drain
path — and it measured a loss, not a win. The two honest-rejects together
sharply narrow where sefer's remaining small-size gap can close.

## GO/NO-GO criteria (fixed in advance — design doc §3-Ф5)

- **GO:** cold/recycle benches show ≥5 % improvement in Ir (iai) **and** ≥5 %
  in wall-clock on `global_alloc`'s 16 B and 64 B cases; **and** zero regression
  (≤+1 % Ir) on the remaining 7 iai benches.
- **NO-GO (honest-reject):** if ANY of — (a) cold/recycle improvement <5 %,
  (b) regression >1 % on ANY of the other 7 iai benches, (c) wall-clock doesn't
  confirm the iai trend.

**Verdict (mechanical application, numbers below): NO-GO.** Triggered by ALL
THREE failure modes simultaneously — not a close call:

1. (a) cold/recycle improvement <5 %: the cold/recycle benches did not improve
   at all — they regressed by **+23 % to +31 %** in Ir. The criterion's "≥5 %
   improvement" precondition is missed by ~26–36 percentage points in the wrong
   direction.
2. (b) regression >1 % on ANY of the other 7: ALL 7 non-target benches
   regressed by **+0.75 % to +4.33 %**. Six of the seven breach the ≤+1 % Ir
   ceiling; only `realloc_grow` (+0.75 %) sits inside it.
3. (c) wall-clock does not confirm the iai trend: confirmed — the wall-clock
   trend is the SAME direction as iai (regression), and of comparable or larger
   magnitude (+40 % / +43 % vs +23 % / +31 % Ir). The two judges agree,
   unanimously, on every bench.

## Setup and method

- **Source under test:** the `alloc-runfreelist` feature (Cargo.toml:
  `alloc-runfreelist = ["alloc-core"]`), gating Ф1's `src/alloc_core/run_stack.rs`
  (`RunStack` storage + `Layout::run_stack_off` / `small_meta_end` shift),
  Ф2's flush-side contiguous-run detector in `flush_run`
  (`alloc_core.rs:2198`), Ф3's drain-side stride reconstruction in
  `drain_freelist_batch` (`alloc_core.rs:2881`), and Ф4's `decommit_empty_segment`
  RunStack clear (`alloc_core.rs:1257`). With the feature OFF the byte layout is
  identical to the pre-PERF-3 build (the production-judge neutrality gate,
  verified in Ф1–Ф4 and re-confirmed by this run's baseline reproducing the
  11-bench reference exactly — see "Baseline" below).
- **Feature-flag syntax:** the iai script (`scripts/iai.mjs`) takes
  `--features '<set>'`; cargo receives `--features '<set>'` with the set
  space-joined inside single quotes (line 125). The OFF run used the script
  default `production`; the ON run used `--features "production alloc-runfreelist"`
  (the production bundle plus the experimental feature — the exact feature
  combination Ф6 would enable in `production` if this verdict were GO). Both
  runs produced distinct bench binaries (`perf_gate_iai-a47d1d86848035de` OFF vs
  `perf_gate_iai-88babc1e42f2a7fd` ON), confirming the feature compiled in and
  shifted codegen.
- **Judge 1 — iai (instruction count):** `npm run iai` (WSL + valgrind
  callgrind, `production` features, 11 bench fns in `perf_gate_iai.rs`).
  Deterministic Ir + cache columns (L1/L2/RAM/EstCycles). The cold/recycle iai
  benches use `COLD_BATCH = 256`.
- **Judge 2 — wall-clock criterion:** `cargo bench --features production --bench
  global_alloc` (OFF) vs `cargo bench --features "production alloc-runfreelist"
  --bench global_alloc` (ON), scoped to `"global_alloc/SeferAlloc"`. The
  `global_alloc` group's `bench_direct_alloc` (`benches/global_alloc.rs:68`) does
  `OPS = 1024` alloc-then-dealloc-all of distinct blocks through `SeferAlloc`'s
  `GlobalAlloc` face — the exact cold-storm shape (1024 distinct allocs → 1024
  frees in reverse order) the design doc §1 identified as producing long
  contiguous runs. Bench IDs `global_alloc/SeferAlloc/16B` and
  `global_alloc/SeferAlloc/64B` are the two sizes the GO criterion names.
  Windows wall-clock is noisy (sample_size 10, short warm-up per CLAUDE.md's
  fast-profile mandate), so the OFF run was saved as a named criterion baseline
  (`--save-baseline perf3_off`) and the ON run compared against it
  (`--baseline perf3_off`), giving a clean paired `change:` field instead of a
  diff against an unrelated cached run. The absolute means are reported
  alongside and are what the Δ% is computed from.
- **Per the short-scenario policy:** the `global_alloc` bench already uses
  `sample_size(10)` + 150 ms warm-up + 600 ms measurement (verified at
  `benches/global_alloc.rs:186-188`); unchanged for this measurement (this is
  measurement, not codegen).

## Baseline (feature OFF, `production`) — measured 2026-07-07

`npm run iai` at `--features production`. **Reproduces the confirmed-stable
11-bench reference exactly** (the table the task brief named:
`81,423 / 81,439 / 73,011 / 561,971 / 125,354 / 125,357 / 179,180 / 179,183 /
81,423 / 81,551 / 111,662`) — every Ir matches digit-for-digit, confirming the
feature-OFF build is byte-identical to the pre-PERF-3 production reference (the
neutrality gate) and that this is the correct baseline to diff the feature-ON
run against. No layout drift, no codegen drift.

| bench                       |        Ir |    L1 hits | L2 hits | RAM hits | Est. Cycles |
| --------------------------- | --------: | ---------: | ------: | -------: | ----------: |
| small_churn_16b             |    81,423 |    142,745 |     161 |    5,219 |     326,215 |
| aligned_churn_640b_a128     |    81,439 |    142,771 |     159 |    5,219 |     326,231 |
| large_alloc_free_cycle      |    73,011 |    132,644 |     162 |    5,221 |     316,189 |
| realloc_grow                |   561,971 |  1,174,078 |   3,988 |   74,960 |   3,817,618 |
| cold_alloc_free_256x16b     |   125,354 |    195,737 |     171 |    5,325 |     382,967 |
| cold_alloc_free_256x64b     |   125,357 |    195,561 |     173 |    5,505 |     389,101 |
| recycle_alloc_free_256x16b  |   179,180 |    260,773 |     174 |    5,335 |     448,368 |
| recycle_alloc_free_256x64b  |   179,183 |    260,585 |     176 |    5,527 |     454,910 |
| churn_256b                  |    81,423 |    142,741 |     161 |    5,223 |     326,351 |
| churn_write_256b            |    81,551 |    142,997 |     161 |    5,223 |     326,607 |
| multiseg_cold_256k          |   111,662 |    189,164 |     189 |    5,517 |     383,204 |

Wall-clock `global_alloc/SeferAlloc` (criterion, Windows, saved as
`perf3_off` baseline; mean of `[low .. mean .. high]`):

| bench                          |   mean (OFF) |
| ------------------------------ | -----------: |
| global_alloc/SeferAlloc/16B    |   32.283 µs  |
| global_alloc/SeferAlloc/64B    |   32.773 µs  |
| global_alloc/SeferAlloc/256B   |   34.825 µs  |
| global_alloc/SeferAlloc/1024B  |   36.000 µs  |

## Candidate (feature ON, `production + alloc-runfreelist`) — measured 2026-07-07

`npm run iai -- --features "production alloc-runfreelist"`. Every bench
**regressed**; the four explicit cold/recycle targets regressed the most. The
L1-hits column is the key diagnostic (see "Mechanism analysis"): for the recycle
benches it jumps +76 k (260 k → 336 k), i.e. the run-detection machinery adds
more L1 traffic than the pointer-chase it removes.

| bench                       |   OFF Ir |    ON Ir |     Δ Ir |    Δ % |  ON L1 | Δ L1 |
| --------------------------- | -------: | -------: | -------: | -----: | -----: | ---: |
| small_churn_16b             |   81,423 |   84,669 |   +3,246 |  +3.99%| 149,110| +6,365 |
| aligned_churn_640b_a128     |   81,439 |   84,685 |   +3,246 |  +3.99%| 149,135| +6,364 |
| large_alloc_free_cycle      |   73,011 |   76,168 |   +3,157 |  +4.33%| 138,890| +6,246 |
| realloc_grow                |  561,971 |  566,211 |   +4,240 |  +0.75%|1,181,850| +7,772 |
| cold_alloc_free_256x16b     |  125,354 |  154,232 |  +28,878 | +23.04%| 235,652| +39,915 |
| cold_alloc_free_256x64b     |  125,357 |  155,314 |  +29,957 | +23.89%| 237,339| +41,778 |
| recycle_alloc_free_256x16b  |  179,180 |  234,773 |  +55,593 | +31.03%| 336,531| +75,758 |
| recycle_alloc_free_256x64b  |  179,183 |  234,776 |  +55,593 | +31.03%| 336,342| +75,757 |
| churn_256b                  |   81,423 |   84,669 |   +3,246 |  +3.99%| 149,106| +6,365 |
| churn_write_256b            |   81,551 |   84,991 |   +3,440 |  +4.22%| 149,552| +6,555 |
| multiseg_cold_256k          |  111,662 |  114,819 |   +3,157 |  +2.83%| 195,415| +6,251 |

### Wall-clock confirmation at feature ON (the decisive signal)

`cargo bench --features "production alloc-runfreelist" --bench global_alloc --
"global_alloc/SeferAlloc" --baseline perf3_off`. The feature makes sefer
**dramatically worse** on the exact storm shape it was designed to improve, and
criterion's own paired `change:` field confirms the regression is statistically
significant on every row (p = 0.00 < 0.05):

| bench                          |  OFF mean |   ON mean |     Δ wall | criterion `change:` (mean) |
| ------------------------------ | --------: | --------: | ---------: | --------------------------: |
| global_alloc/SeferAlloc/16B    | 32.283 µs | 45.359 µs |  **+40.5%**|                     +44.14% |
| global_alloc/SeferAlloc/64B    | 32.773 µs | 46.713 µs |  **+42.5%**|                     +44.49% |
| global_alloc/SeferAlloc/256B   | 34.825 µs | 49.867 µs |  **+43.2%**|                     +46.77% |
| global_alloc/SeferAlloc/1024B  | 36.000 µs | 60.818 µs |  **+68.9%**|                     +69.16% |

The wall-clock judge confirms the iai judge in direction and rough magnitude
(the wall-clock regression is in fact *larger* than the Ir regression on the
cold/recycle benches — consistent with the feature adding L1-pressure-visible
memory traffic, not just pure-ALU instructions; see mechanism below). The GO
criterion's "wall-clock must confirm the iai trend" clause is satisfied — the
trend is regression in both — which itself is a NO-GO trigger (criterion (c)).

## Mechanism analysis — WHY the numbers look this way

The design doc §1 hypothesized a **serial dependent-load pointer chase** on the
recycle drain path: `drain_freelist_batch` does `Node::read_next(block_nn)` per
block, where the address of the next block lives in the just-read block's body —
"one cold cache-line miss per block," the doc called it, and noted mimalloc
"pays this too; there is no way to hoist it" under a linked-list representation.
PERF-3 introduced a different representation (compact `(start_off, count)`
descriptors reconstructed by stride arithmetic) precisely to make that hoist
possible. The hypothesis was that eliminating `read_next` on drain would win.

The implementation does eliminate `read_next` for run-member blocks on drain
(verified: `drain_freelist_batch`'s run-loop at `alloc_core.rs:2915-3018` uses
`start + i*block_size`, no `Node::read_next`). **But it pays for that single
saved load with substantially more work on both sides of the recycle path:**

1. **The flush side does NOT eliminate the per-block `write_next` — it keeps it
   and adds a detection pass on top.** `flush_run` (`alloc_core.rs:2198`) under
   the feature first runs the *entire* classic LIFO-chain build unchanged
   (per-block `Node::write_next` + `mark_free`, lines 2282-2298), collecting
   accepted offsets into a fixed `[u32; 16]` array as it goes. Only AFTER the
   chain is fully built does a post-pass (lines 2309-2368) **insertion-sort**
   the accepted offsets ascending, **scan** the sorted order for contiguous
   sub-runs of length ≥2, **push** a `(start_off, count)` descriptor per sub-run
   onto the RunStack, and **rebuild** the linked-list head to skip the diverted
   members. So the feature's flush cost is: classic chain (unchanged) + offset
   collection + insertion sort + contiguous scan + descriptor pushes + head
   repair. The design doc §3-Ф2 described this as "refactor `flush_run` to ...
   push RunStack, fallback to linked-list" — i.e. *divert* blocks away from the
   linked list — but the landed implementation *augments* the linked list rather
   than replacing it, so the per-block `write_next` the design hoped to remove
   on flush is still executed for every accepted block. This is the dominant
   added cost: it shows up as the uniform +3 k–+4 k Ir baseline regression on
   *every* bench (even `large_alloc_free_cycle`, which does no small-block
   magazine work at all — the cost is paid at heap-claim/segment-init time as
   the larger `small_meta_end` RunStack region is zeroed, plus codegen shift
   from the new code paths being in the binary).

2. **The drain side adds per-descriptor `pop` + per-member `is_free` guard +
   `mark_alloc` + stride arithmetic, plus a remainder-pushback path.** Even
   though `read_next` is gone for run-members, the run-loop
   (`alloc_core.rs:2915-3018`) still does a `RunStack::pop` per descriptor, a
   per-member `bm.is_free(off)` defense-in-depth guard (plan §2.3, load-bearing),
   a per-member `mark_alloc` (identical RMW to the linked-list drain — plan §2.3
   explicitly: "the run descriptor only changes HOW the address is obtained"),
   the `Node::deref` store into `out`, *and* (for the large-small-class tail,
   Ф4) a conditional `RunStack::push` of a truncated remainder when `out` fills
   mid-descriptor. The `is_free`+`mark_alloc` RMW pair is the same memory
   traffic the linked-list drain pays; only the single `read_next` load is
   saved. That is a bad trade in instruction count (the pop/sort/scan/pushback
   overhead dwarfs one saved load per block) and a break-even-to-bad trade in
   cache traffic (the saved `read_next` was largely covered by the L1
   stride-prefetcher at the regular block stride — the design doc's own §5
   readiness note flagged this as the failure mode: "if pointer-chase is NOT
   bottleneck on real workloads (for example, if the cache-line prefetcher
   already covers dependent loads on the typical stride)").

3. **The L1-hits column is the smoking gun.** For `recycle_alloc_free_256x16b`:
   OFF L1 hits = 260,773; ON L1 hits = 336,531 — a rise of **+75,758 L1 hits**,
   almost exactly matching the +55,593 rise in Ir (the new instructions are
   predominantly L1-resident memory ops: the offset array, the sort permutation
   array, the RunStack descriptor slots). The feature adds more L1 traffic than
   the pointer-chase removed from L1/L2. There is no level of the cache
   hierarchy where the feature wins: L2 hits are flat (~174 → ~176), RAM hits
   are flat-to-slightly-up (5,335 → 5,419). The dependent-load pointer chase was
   *not* the bottleneck — the L1 prefetcher was already covering it, and the
   replacement representation costs more total L1 ops.

4. **Why cold/regression scales with batch traffic but the flat baseline
   doesn't.** The +3 k–+4 k Ir regression on the churn/large/multiseg benches
   (which barely exercise the recycle flush→drain path) is the *fixed* cost of
   the feature: larger `small_meta_end` zeroing at segment init + codegen shift
   from the new code paths in the binary. The +23 % to +31 % regression on
   cold/recycle is the *variable* cost: those benches hammer flush→drain
   repeatedly (256-block cold batches, recycled), so the per-flush
   sort/scan/push and per-drain pop/guard/pushback overhead multiplies across
   every batch. `realloc_grow` (+0.75 %) is the one bench that stays inside the
   ≤+1 % gate because its hot path is the large-block realloc copy, not the
   small-block recycle path — the feature's code is in the binary but rarely
   executed.

The design doc §1's own honesty caveat — "this plan introduces a different
representation, where hoist is possible" — was correct that the hoist is
*possible*; what the measurement shows is that the hoist is not *profitable*.
The pointer-chase the design targeted is real but cheap (prefetcher-covered),
and the representation that removes it is expensive (extra sort/scan/push on
flush, extra pop/guard/pushback on drain). PERF-2's structural conclusion
("mimalloc's advantage is a cheaper refill, not a deeper magazine") named the
right *family* of attack ("cheaper per-block work on the recycle path"), but
this specific instance of that family makes per-block work *more expensive*,
not less — the win the design sought would have required eliminating the
per-block `write_next` on flush (true diversion, not augmentation), which the
landed implementation does not do.

## Other-bench regression check (the design doc's predictions)

The design doc §1 made explicit predictions about non-target benches; the
measurement refutes the optimistic ones and confirms the pessimistic caveat:

- **`large_alloc_free_cycle` (the cleanest "fixed cost" decomposition):** the
  design predicted non-target benches should be CAP-insensitive-style neutral.
  **Measured +4.33 %** — outside the ≤+1 % gate. This bench does NO small-block
  magazine work (it cycles one large block), so the regression is pure
  fixed-cost: the larger `small_meta_end` RunStack region (~3 KiB × the number
  of segments touched) is zeroed at segment claim, and the new code paths shift
  the binary's codegen layout. This is the same "fixed cost per heap/segment
  claim" mechanism PERF-2 identified for `Tcache` zero-init, just at a smaller
  magnitude (3 KiB RunStack vs 6.4–50 KiB Tcache).
- **`realloc_grow` (the one pass):** +0.75 %, the only bench inside the ≤+1 %
  Ir gate. Its hot path is large-block realloc-copy, which bypasses the
  small-block recycle path entirely; the feature's code is in the binary but
  rarely executed, so only the codegen-layout-shift component shows.
- **The cold/recycle targets:** the design predicted these should show the win
  (≥5 % improvement). **Measured +23 % to +31 % regression** — the opposite of
  the prediction, by a wide margin. The "1024-distinct-allocs-then-reverse-
  frees" storm pattern does produce long contiguous runs (the design's premise
  holds), but producing descriptors for those runs costs more than the pointer-
  chase it saves (the mechanism above).

## Source disposition — recommendation (NOT executed; for human decision)

Per the task description, the NO-GO disposition (revert to pre-Ф1 vs keep as
off-by-default experimental) is explicitly left to this phase to *recommend*,
not execute. The task further instructs: if leaning toward revert, STOP before
any revert and instead document the recommendation for the human to decide.
**This phase did NOT run `git revert`, did NOT modify any `src/` file, and did
NOT stage anything.** The recommendation below is analysis only.

**Recommendation: KEEP as an off-by-default experimental feature (do NOT
revert Ф1–Ф4).** Reasoning:

1. **Zero production cost.** The feature is OFF by default and NOT in
   `production`; the neutrality gate (verified again by this run's baseline
   reproducing the 11-bench reference exactly) guarantees the production build
   is byte-identical to the pre-PERF-3 build. Reverting gains nothing for
   production users — there is no regression to remove, because the feature is
   not enabled. The 399 lines of Ф2–Ф4 code and the `run_stack.rs` module
   compile to nothing under the default feature set.

2. **The code is correct, reviewed, and tested.** Ф1–Ф4 each passed zero-trust
   review by @o46m (GO each time), each has dedicated regression tests
   (`tests/regression_run_stack_*.rs`), and the M2-double-free-through-run and
   decommit-clears-runstack safety cases are explicitly covered. Reverting
   throws away correct, safety-argued code that the project paid real review
   cost to land.

3. **The loss is an algorithmic-cost loss, not a correctness loss, and the
   algorithm can be revisited.** The mechanism analysis above identifies the
   *specific* reason the feature loses: the landed flush-side *augments* the
   classic linked-list chain (keeps `write_next`) rather than *diverting* away
   from it, so the per-block flush cost the design hoped to remove is still
   paid. A future "PERF-3.5" that reworks `flush_run` to skip `write_next` for
   detected run-members (true diversion — write the descriptor instead of the
   chain link) could in principle tip the trade. The storage (Ф1), the
   drain-side reconstruction (Ф3), and the lifecycle seams (Ф4) are all
   reusable as-is; only the flush-side detection-and-diversion (Ф2) would need
   rework. Keeping the landed code preserves that option at zero production
   cost.

4. **Precedent: PERF-2 left no source (it was a constant sweep, nothing to
   keep); this is different.** PERF-2 reverted because it temp-edited a
   constant — there was no reusable implementation to preserve. PERF-3 landed
   four phases of real, reviewed implementation; the honest-reject is of the
   *measured outcome*, not the *code quality*. The code deserves to stay
   available behind its experimental flag for future revisitation, exactly as
   the design doc §2.8 framed it ("an experimental opt-in performance feature
   ... NOT part of `production` ... until Ф5 reaches GO").

5. **The only argument FOR revert is binary-size / repo-weight hygiene**, and
   it is weak: 399 lines in `alloc_core.rs` (mostly under `#[cfg]`) + one new
   ~330-line module + a handful of test files. Under the default feature set
   the `alloc_core.rs` additions compile out entirely; the binary-size cost to
   a production build is zero. The repo-weight cost is real but small, and the
   optionality value (point 3) outweighs it.

**This recommendation is flagged for human decision.** If the human prefers
binary/repo minimalism over optionality, the revert path is mechanically
`git revert 7d5bada f13ec4b 3e097be 5c5b6af` (Ф2, Ф3, Ф4, Ф1 in reverse order)
— but that is a MAJOR decision this phase explicitly does NOT execute.

## Verdict

**NO-GO (honest-reject).** All three failure modes fire simultaneously: the
cold/recycle targets regressed +23 % to +31 % in Ir (criterion (a), needed ≥5 %
improvement); six of seven non-target benches breached the ≤+1 % Ir gate
(criterion (b)); and the wall-clock judge confirmed the regression direction
and magnitude on both the 16 B (+40 %) and 64 B (+43 %) storm shapes
(criterion (c) — the trend is the same, which is itself the trigger). The
"eliminate the dependent-load pointer chase" hypothesis is refuted by direct
measurement: the saved `read_next` loads were largely prefetcher-covered and
cheap, while the replacement representation (sort + scan + descriptor push on
flush, pop + guard + pushback on drain) costs more L1 traffic and more
instructions than it saves. The two honest-rejects (PERF-2 on magazine depth,
PERF-3 on recycle-path representation) together establish that sefer's
remaining small-size gap vs mimalloc is not closeable by either a deeper
magazine or a cheaper-per-block recycle representation of this shape — the gap
is structural in the refill/flush orchestration itself (the `find_segment_with_
free` / latch / carve-batch machinery), which is where a future PERF-4 should
look.

**The feature stays OFF / opt-in-only** (NOT added to `production`); Ф6 (task
#213) is not triggered. Source is left in place as an experimental feature per
the disposition recommendation above. Final tree after PERF-3 = pristine in
`src/` (zero diff; this doc is the only new file).

*Structure, tone and rigor follow `PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md` (the
honest-reject precedent). Phased discipline (commit after each phase, zero-trust
review, judge) from CLAUDE.md "Phased delivery". The verdict is factual, not
negotiable post-measurement; the criteria were fixed in advance in the design
doc §3-Ф5.*
