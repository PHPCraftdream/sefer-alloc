# PRF-03 run01 — decision after the first full gate (reviewer role), 2026-10-12

Question put to the reviewer role (`rush run --role reviewer`, read-only): what to do after run01
(`docs/perf/R19_PRF03_B_VS_A_GATE.md`): strict NO-GO, a second candidate under the same frozen criteria, or an
amendment of the R3/control thresholds before any new measurement.

## Binding decisions

1. **Path (b): candidate B2 under the unchanged frozen criteria; path (c) rejected.** B `217c5339` is not merged; it and
   run01 stay as the original experiment. The official run01 verdict stays **INCONCLUSIVE** (noise has priority in the
   judge), not renamed NO-GO, but the iai axis fails independently of native noise (R3 +151 at T = 12 and the other
   violations in `R19_PRF03_B_run01/iai_summary.csv`). B2 is compared with A `6904dced`; both experiments are published,
   history is not replaced. Raising thresholds to values that justify the already observed +151 / +48 is a forbidden
   retroactive fit even if labelled "A3 before the next measurement"; an exception exists only for an independently
   proven defect of a criterion (wrong units, normalisation or measurement mechanism), with a separate decision, a new
   protocol version, an independent justification of the bounds and fresh data. No such defect of the R3 threshold is
   established: the R3 oracle shows no reserve/release, so "tens of thousands of OS cycles" does not excuse it.
2. **Harness: amendment A3 allowed only as removal of a confounder.** Observed: an identical +7 Ir in `memset` in the
   sefer cells and in the mimalloc cells; hypothesis (not established): path length / alignment. Equal-length names are a
   necessary control, not proof of the cause. Actions: fix the same shape of FULL build and run paths, argv lengths,
   controlled environment and working directories for A / B / C0 beforehand; keep C0 an independent build of the same
   A; run an A/A across directories of both shapes including mimalloc; if a residual difference remains, investigate
   addresses/sizes of the libc calls instead of tuning names until PASS. No subtracting +7, no dropping cells, no
   weakening exact zero. This changes none of the -200 bound, the T formula, the cell kinds or the native thresholds;
   runner identity is frozen before the new builds; a new full run is mandatory.
3. **B2 goals and untouchable invariants.** Keep R1 <= -200 Ir; reach R2/R3 <= T, all controls |delta| <= T and
   mimalloc delta = 0 in all four comparisons. The +48 memcpy is observed, its link to a larger structure is a
   hypothesis to confirm with type sizes and copy arguments. Allowed: compact owner-only representation of the parked
   route, removal of redundant moves, specialised fresh/retained registration and removal; a new allocation for packing
   needs a separate OOM/lifecycle assessment; moving cost outside the benchmark boundary is forbidden. Not to be weakened:
   `lookup` accepts only LIVE/PENDING/CONSUMING; the SEC-03 address check precedes `claim_owner_live`, which precedes the
   header claim and accounting; a failed CAS is a counted no-op and drain frees exactly once; `release_cached_large`
   stays the single unlink-before-unmap barrier on every exit; reuse only when root and payload match, LIVE published
   last; generation/exhaustion, rollback/OOM and standalone-ZST contracts. Before any measurement: review of the full
   B2 diff, retained/changed-payload tests, stale foreign free/realloc, PENDING, release ordering and failures,
   applicable Miri/Loom with discriminating negative controls; then fresh feature-exact/paired oracles and
   configuration receipts (old evidence is not transferred automatically).
4. **Native axis: a full restart is required for accepting B2.** Repeating only native for the unchanged B is allowed as
   a separate diagnostic experiment; it neither rescues B's iai failures nor replaces run01. Old iai and new native
   results must not be glued into one run. Procedure: finish the builds beforehand, agree an exclusive window without
   foreign build/bench sessions, fix CPU/affinity, power and environment, log Windows-host and WSL load before and
   during the measurement; run the whole native block (N = 1/4/8, nine fresh processes A/AA/B each with its own twin,
   the existing alternation). The quantitative quiet-window criterion is A/A noise <= 5 % on all six axes, then
   B <= A * (1 + actual noise); above that the verdict is INCONCLUSIVE again; no cherry-picking of N or samples and no
   repeating until the first GO without publishing every other attempt.
5. **Next step:** keep B/run01 unchanged, freeze the A3 harness without revising criteria, prepare and verify B2, then
   publish a full A-versus-B2 run in the agreed quiet window.
