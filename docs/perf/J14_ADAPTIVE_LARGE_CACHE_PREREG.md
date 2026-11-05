# J14 preregistration — an adaptive Large cache that learns the program's reuse habit

Written and committed **before** any code of the policy exists and before any measurement of it. Class if the gates pass: `perf(runtime)`
(promotion into `production`); before that the policy lives behind the opt-in feature `large-cache-adaptive`.

## 1. Observation

J11 (`docs/perf/J11_BIG_BLOCK_JUDGE_MAP.md`): freeing a heap of many Large blocks and allocating it again is 47.6x (Windows) and 63.1x (Linux) slower
than mimalloc for 128 x 16 MiB, 27.2x / 33.0x for 16 x 128 MiB. Mechanism, read from the code (`mem_impl.rs` Large deposit, `alloc_core_large_cache.rs`):
the production Large cache has **8 slots** (`LARGE_CACHE_SLOTS`; the 32-slot extension is the opt-in `large-cache-extended`), and its byte budget is
unbounded. When all 8 are full, a free evicts the FIFO-oldest span to the OS and caches the new one, so after freeing 128 blocks only the last 8 are kept
(end RSS 8 x 16 = 128 MiB and 8 x 128 = 1024 MiB in J11, plus the base). The next round re-reserves and re-faults the other 120. The 256 MiB headroom
governs only the time decay of what is already cached. mimalloc keeps the whole heap.

The owner's target (2026-11-05): Sefer must not be more than 2x behind mimalloc, and the retention must adapt to the program's habits — grow by how much
we guessed wrong, shrink when the habit is gone — rather than use a fixed number.

## 2. Policy (feature `large-cache-adaptive`, implies `large-cache-extended`)

Per heap (owner-only state inside the lazily materialised extension sidecar, cold Large paths only; no atomics; `AllocCore`'s size unchanged):

- **Ghost list.** A ring of the last `GHOST_N = 64` spans the cache gave back to the OS because of slot or budget pressure (eviction at deposit or a
  refused deposit), each with the running total of bytes given back right after it left. Decay releases are not recorded: they are the policy shrinking on purpose.
- **Mistake and its distance.** A mistake is an `alloc_large` that misses the cache while a ghost entry is compatible with it (the cache-hit size rule:
  `ghost >= usable` and `ghost <= usable * LARGE_CACHE_SIZE_FACTOR`); the ghost entry is consumed. Its *distance* is the bytes given back since that span
  left, the span included — exactly the extra retention that would have turned this miss into a hit.
- **Histogram and integral (owner's formula, 2026-11-05).** Distances go into a decaying histogram of 48 log2 buckets (a mistake adds weight 256 to its bucket).
  The learned extra is the upper bound of the smallest bucket whose cumulative weight (the integral from 0) covers 90 % of the total weight: the retention
  that would have prevented 90 % of the recent mistakes. Each decay interval forgets 1/16 of every weight; a quiet interval (no mistake and no cache hit)
  forgets half. With no weight left the extra is 0 and the cache is the pre-J14 one again.
- **Ceiling from the program itself.** `extra <= P - headroom`, `P` = the recent peak of live Large bytes of this heap (`P = max(P, live)` at each Large
  allocation, `P -= (P - live) / 8` at each decay interval). The cache never holds more than the program itself recently had live. Learned target
  `T = headroom + extra`.
- **Use of T.** While any mistake weight is remembered and `used + usable <= T`, a deposit that finds no free slot opens 8 more extension slots (up to 248,
  256 in total; occupancy words past the first live in the sidecar). The byte budget is raised to T unless the caller set `budget_bytes` itself (an explicit
  budget stays a hard cap). The time decay trims cached bytes above T instead of above the headroom, and keeps ticking while weight remains so the habit is
  forgotten. Without any mistake nothing differs from the fixed 40-slot extension.
- Without the feature every structure and algorithm behaves as before (8 slots, or 40 with `large-cache-extended`); the bitmask and slot-count change is a
  structural change behind it (the extension's active bound starts at 32 and moves only under the policy).

## 3. Entry point and measurement

- Real `SeferAlloc` through the `#[inline(never)]` shims of `examples/g1_big_judge.rs` (the shipped layer), arms base (main), x (`production
  large-cache-adaptive`), base2 (A/A copy of base), mimalloc; Windows and Linux/WSL2; 12 blocks x 2 repeats; J11 cells plus a new cell `idle`
  (the `many` 128 x 16 MiB habit for 3 rounds, then 6 decay intervals of small activity — one 1 MiB Large alloc/free every 100 ms — then end RSS).
- Counters (`bench-internals` builds only, a separate activation run, not timed): mistakes, B at the end of each round, cache hits, slots in use.

## 4. Gates (fixed now)

- **T0, path activation.** `many` 128 x 16 MiB in x: mistakes > 0 in round 1, cache hits >= 90 % of the Large allocations of rounds 2 and later, B above B0
  after round 2. `cycle` 1 MiB (one span, never over the base): no adaptive state at all. Failing T0: NULL.
- **T1, benefit (owner's target).** Warm time of `many` 128 x 16 MiB and `many` 16 x 128 MiB: x / mimalloc point estimate <= 2.0 on both platforms, and
  x / base upper bound < 0.5.
- **T2, cost.** (a) `idle`: x's end RSS at most base's plus 2 spans (32 MiB) after the 6 intervals (the habit stopped, B fell back); (b) at no sampled point
  does x hold more cached bytes than the peak live bytes of the run (activation run counter); (c) the cells without reuse pressure (`cycle`, `edge`, `grow`,
  WG-1 `large`): x / base interval not wholly above 1.05.
- **Kill.** Any cell with x / base wholly above 1.10, or T2(a)/(b) exceeded: NO-GO, no promotion.
- **Decision.** GO (promotion into `production`) only when T0, T1, T2 pass and nothing is killed; otherwise the feature stays opt-in or is removed, with a `bench` report.

## 5. Not claimed

A process that frees a habit-sized heap and then makes no further allocator call keeps it (decay runs from allocator calls, not from a timer); several heaps
each learn their own habit and each is bounded by its own peak, not by a process-wide one.
