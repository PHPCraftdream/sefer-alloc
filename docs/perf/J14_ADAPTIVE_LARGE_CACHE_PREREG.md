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
  that would have prevented 90 % of the recent mistakes. Each decay interval forgets 1/16 of every weight; a quiet interval (no mistake, and cache hits that
  served under 1/8 of the learned extra — the retention is mostly unused) forgets half. With no weight left the extra is 0 and the cache is the pre-J14 one again.
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

## 6. Amendment 1 (written before any campaign; smoke-run reading, no gate changed)

A smoke run of `many` 128 x 16 MiB on Linux (one process each, not a campaign) showed the mechanism working — round 2 onwards served from the cache (cold 1.45 s,
steady rounds ~65 ms against ~560 ms without the policy) — and where the remainder sits: in the steady rounds the time is the touch phase (4 KiB pages, 60 ms for 2 GiB)
and not the allocator (alloc 0.3 ms, free ~5 ms); mimalloc touches the same memory in 8 ms because its blocks sit on huge pages (J13). The gates of section 4 stay as written
and apply to **x** (J14 alone). A fifth arm **xt** (x plus the J13 hint for fresh Large reservations of 8 MiB or more, tag `archive/g1-j14-xt`) is added to read the combination;
no gate rests on xt, its numbers are reported next to x. On Windows (no THP hint) x and xt are the same code path for these cells.

## 7. Amendment 2 (written after campaign 1 and before any new data; campaign 2 runs on fresh data)

Campaign 1 (`docs/perf/J14_ADAPTIVE_LARGE_CACHE.md`) is a NO-GO by the letter of section 4 and stays published as it is. Reading it found three defects of the design and two of the gates; this
amendment changes the policy and the gates for **campaign 2 only**. Nothing of campaign 1 is re-read under the new gates.

Defects of the design, each seen in campaign 1 and traced in the code:

1. **The policy lowered the cache for large spans.** `large-cache-adaptive` implies `large-cache-extended`, whose default byte budget is 256 MiB (R17-9) instead of the unbounded budget of the production cache. 16 x 128 MiB: the third span of 128 MiB is
   refused, so x cached 2 spans where base caches 8 (x/base 1.88, end RSS 132 MiB against 1028 MiB). The extension sidecar, which holds the ghost list and the histogram, is created only once the base slots are full; it never was, so nothing was learned.
2. **The ghost list is too short for the habit.** 64 ghosts of 16 MiB bound the distance of a mistake to 1 GiB, so a 2 GiB habit of 128 spans cannot be learned whole (campaign 1: x kept 1028 MiB of 2 GiB).
3. **Forgetting does not shrink the learned extra.** A quiet interval halves every weight, which leaves the 90 % bound where it was until a weight reaches zero; the decay then releases 10 % of the excess per interval (campaign 1: end RSS 1188 MiB after six intervals).

Changes of the policy (feature `large-cache-adaptive` only; every other build is unchanged):

- **No budget below the production cache.** Unless the caller set `budget_bytes`, the byte budget of the adaptive cache is unbounded as in `production`; the 8 base slots behave exactly as in `production`. The extension slots start at **0** (not 32) and open
  only under the learned target (`used + span <= headroom + extra`), as before. With the policy switched off (test hook) the fixed 40-slot extension and its 256 MiB budget are restored.
- **Ghost list 256** (4 KiB in the sidecar), so a habit of up to 256 spans of any size is visible.
- **Quiet interval** (no mistake, hits under 1/8 of the extra): every distance moves down one bucket (the extra halves) in addition to halving the weights, and the decay step releases **75 %** of the excess over the target instead of the configured rate (10 % by default).
  A non-quiet interval is unchanged.

Changes of the gates:

- **T1 (benefit), replaces the x / mimalloc <= 2.0 clause:** warm x / base upper bound **< 0.5** on `many` 128 x 16 MiB and **< 0.9** on `many` 16 x 128 MiB, on both platforms. The owner's target (not more than 2x behind mimalloc) is **not** a gate of this feature:
  the steady-round remainder at 128 x 16 MiB is the touch of 2 GiB on 4 KiB pages (campaign 1, section 5), which no retention policy changes. It is reported as x / mimalloc on both platforms and, on Linux, as xt / mimalloc (the combination with the J13 hint).
- **T2(a) (idle), replaces the 6-interval clause:** the `idle` cell runs **12 s** of light activity (one 4 MiB block every 100 ms) after the habit; x's end RSS at most base's end RSS **plus the 256 MiB headroom floor plus 32 MiB**. The floor is the documented
  per-heap retention of the default configuration; the policy may keep what the headroom keeps, not more.
- T0 (path activation), T2(b), T2(c) and the kill rule are unchanged; T2(c) now also covers `many` 16 x 128 MiB cold (x / base interval not wholly above 1.05), the cell where campaign 1 found the regression.
- **Arm xt** is rebuilt from the new J14 tree plus the J13 patch (tag `archive/g1-j13-thp`) and is read only; the runner now tests for the xt binary where it lives (inside WSL), the defect that dropped it from campaign 1.
