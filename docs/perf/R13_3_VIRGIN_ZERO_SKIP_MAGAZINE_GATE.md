# R13-3 (task #273) — `virgin-zero-skip` magazine-fix wall-clock gate

Honest "was / now" comparison for the R13-3 fix, run on this dev host via
`cargo bench --bench r13_3_virgin_zero_skip_wallclock` (criterion,
`sample_size(10)`, 200ms warm-up / 800ms measurement — the project's fast
comparative-gate profile, not a publication-grade benchmark; treat the
numbers as directional, not authoritative).

"Was" = `--features "alloc-global fastbin alloc-decommit alloc-segment-directory"`
(virgin-zero-skip OFF — the R12-10 magazine-bypass shape is what the fix
replaces, so this baseline uses the plain `alloc`+explicit-`Node::zero` path,
which is what a `virgin-zero-skip`-off build already does; it is NOT a
rebuild of the literal pre-R13-3 bypass code, since that code no longer
exists after this commit — see note below).
"Now" = same features + `virgin-zero-skip` (R13-3, this fix).

| Scenario | Was (µs/256-iter batch) | Now (µs/256-iter batch) | criterion verdict |
|---|---|---|---|
| cold virgin | 22.3 – 25.6 (mean ~23.8) | 24.1 – 26.0 (mean ~25.0) | No change detected (p=0.73) |
| warm reuse | 51.5 – 55.0 (mean ~52.9) | 50.4 – 54.5 (mean ~52.3) | No change detected (p=0.41) |
| mixed | 23.3 – 26.8 (mean ~25.1) | 23.3 – 25.2 (mean ~24.4) | No change detected (p=0.39) |

## Honest reading

**No scenario shows a statistically significant difference at this sample
size.** This is reported plainly rather than spun: the fix's justification is
NOT "N× faster on this microbenchmark."

**What this bench does NOT capture** (why "no measured win" does not mean "no
real defect existed"): this is a single-threaded, single-heap loop with zero
cross-thread activity. Under that shape, the substrate's `pop_free`
(BinTable) round-trip that the pre-R13-3 magazine-bypass design used on every
`alloc_zeroed` call is ALSO cheap (no ring drain has anything to find, no
directory contention) — so the specific cost this task worried about (paying
full substrate machinery instead of a magazine array-pop) does not show up
strongly when the substrate path itself is this uncontended and this fast.
The gap the R12-10 design docs left open (a genuine architectural risk: EVERY
`alloc_zeroed` call bypassing the magazine, unconditionally, regardless of
workload) is real independent of what this particular synthetic loop
measures — a mixed-class, multi-threaded, or directory-active workload could
plausibly show a larger gap than this bench does. That is future work, not
claimed here.

**Why R13-3 is still the correct fix, independent of this gate's numbers:**
1. **Defect 2 (resource retention) is a genuine, unconditional correctness
   fix** — a heap that calls only `alloc_zeroed` now drains its
   `HeapOverflow`/deferred-large stacks exactly like a heap that calls plain
   `alloc`, closing an unbounded-retention gap with no performance trade-off
   either direction (see `tests/r13_3_alloc_zeroed_only_drains_overflow.rs`,
   counterfactual-verified: fails with the pre-fix drain-less shape at
   `live_count == 1` instead of reaching `0`).
2. **Structural consistency removes an open-ended risk.** `alloc_zeroed` now
   goes through the SAME hit/miss code `alloc` does (`self.alloc`'s magazine
   pop, `refill_magazine_slow`'s miss path) instead of a parallel, ad hoc
   substrate call — any FUTURE optimization or fix to the magazine fast path
   automatically applies to `alloc_zeroed` too, and there is no longer a
   second, drifting code path that could silently regress.
3. **The virgin-skip's own value proposition (skip `Node::zero` for a
   genuinely virgin block) is unaffected and still verified**
   (`tests/r13_3_magazine_virgin_hit_skips_zero.rs`, counterfactual-verified:
   a magazine HIT of a virgin block correctly skips the zero pass, and this
   is the part of the design R12-10 already validated as a real win via
   `alloc_zeroed_virgin_small_skip.rs`'s cold-path tests, unchanged by this
   commit).

## Reproduction

```
cargo bench --bench r13_3_virgin_zero_skip_wallclock --features "alloc-global fastbin alloc-decommit alloc-segment-directory"
cargo bench --bench r13_3_virgin_zero_skip_wallclock --features "alloc-global fastbin alloc-decommit alloc-segment-directory virgin-zero-skip"
```
