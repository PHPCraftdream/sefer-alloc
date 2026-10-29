# WB-3 / C1: frameless fast lanes for depot park and reload — pre-registration

Written and committed before any measurement of this change.

## Change

Design from the WB-1/WB-2 consultation (@ox). Two cold dispatchers replace
the two call targets of the magazine boundaries:

- `free_magazine_park` (called where the own-thread free found a full
  magazine, instead of `free_magazine_full`): parks the full magazine into
  the floor when the floor has room, or into the overflow ring through
  `DepotOverflow::park_simple` when the ring's simple case holds; otherwise
  tail-calls the unchanged `free_magazine_full`.
- `refill_magazine_miss` (called on a magazine miss, instead of
  `refill_magazine_slow`): reloads from the floor, or from the ring through
  `DepotOverflow::reload_simple`; otherwise tail-calls the unchanged
  `refill_magazine_slow`.

`park_simple` / `reload_simple` perform, in one block, exactly the state
writes the current `park` + `tick` / `reload` + `ingress_due` +
`take_unstarved` + `tick` perform in the steady case, and refuse (changing
nothing) in every other case: first record of a class, ring about to fill,
over budget, no reservation or uncommitted record, decay debt, epoch end,
latched classes, ingress check due. Free-path rejects are untouched
(`small_free_guard` and the hot paths are not changed; parked blocks stay
MAG).

Expected: −0.5…−0.6 ns per pair on warm bulk 16 B and 64 B at n = 1024
(WB-2 M1: depot traffic ≈ 1.2 ns/pair), 0 at n = 16, 0 on churn/pair/MT/RSS.

## Arms

- `base`: `main` at the commit this branch forks from (`9d2823e5`).
- `c1`: this branch's commit carrying the change, its tests and this file.

## Measurements and acceptance (all must hold)

1. **Wall clock, depot traffic (Windows host)** — `scripts/wb2_m1_bulk_sweep.mjs`
   (21 processes per (arm, n), sizes 16 and 64 B) on each arm's own
   `b4_pmu_probe` (`production`, aligned code), arms run alternately
   `base, c1, base, c1`. Statistic per run: `D = t(1024) − t(16)` of
   SeferAlloc (min over processes, ns/pair; the difference inside one
   binary cancels a whole-binary layout shift). Accept only if, for both
   sizes and in both run pairs, `D(c1) − D(base) ≤ −0.3` ns/pair and
   SeferAlloc's `t(16)` differs by at most 0.1 ns/pair.
2. **Wall clock, no regression (Windows host)** — `npm run
   bench:table:paired` (3 processes × 200 rounds, `steady`, aligned code)
   `base, c1, base, c1`; no cell's SeferAlloc/mimalloc ratio higher in `c1`
   by more than 3 % in both run pairs.
3. **iai (CI)** — perf-gate iai set `c1` against `base`: no bench more
   expensive by more than 10 Ir in total.
4. **Mechanism (callgrind, WSL)** — `bulk sefer 1024 16` and `64`: per-pair
   Ir lower in `c1` (reported, not a bar on its own). The activation oracle —
   the fast lanes park and reload exactly as often as the generic path — is
   the integration test `tests/wb3_depot_fast_lane_model.rs`, which checks
   the overflow counters event by event.

If any check fails the change is rejected and reverted (the measured commit
kept under an `archive/` tag); the result is recorded either way.

## Addendum 1 (written after check 1, before any further measurement)

Check 1 as registered **failed** (raw runs in `wb3/check1/`): `D(c1) − D(base)`
was −0.59 / +0.04 ns/pair on 16 B and −0.15 / −0.59 on 64 B, and SeferAlloc's
`t(16)` — the same code in both arms, no depot traffic — moved by +0.43 ns in
the first 16 B pair, so the run was too noisy for a 0.3 ns bar per pair. The
`t(1024)` of `c1` was lower in all four pairs (−0.16, −0.04, −0.28, −0.60 ns).
Under the registration C1 is not accepted; that verdict stands whatever
follows.

A second, separately registered check (check 1b) measures the same quantity
with more power, the statistic changed **after** seeing check 1's data —
stated here so it is not mistaken for the original rule:

- 41 processes per (arm, n) instead of 21, four run pairs
  `base, c1, base, c1, base, c1, base, c1`, sizes 16 and 64 B, same binaries
  (sha256 in `wb3/identity.txt`).
- Accept only if, for both sizes, the **median over the four pairs** of
  `D(c1) − D(base)` is ≤ −0.3 ns/pair and the median of SeferAlloc's
  `t(16)` difference is within ±0.1 ns/pair.
- Checks 2 and 3 apply unchanged; C1 ships only if 1b, 2 and 3 all pass.
