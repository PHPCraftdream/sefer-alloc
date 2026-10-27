# B2 — pre-registration (fixed before any J1′ v2 run of this variant)

Base: `main` @ `aa523fd1` (depot + B1 shipped). Measurement branch `b2-drain`
(harness commit cherry-picked from `b4-bulk`@`cd90113c`).
Variant (patch in `docs/perf/b4_ablate/patches/`):

- `b2r1_drain_one_pass` — `try_drain_freelist_batch` walks the chain once:
  per link the same validation as the former probe pass (non-hardened:
  `continuation_offset_or_fatal`; hardened: `valid_free_node`,
  `free_continuation`, head-revisit and Floyd cycle checks, all fatal), then
  `prepare_for`, then the state-byte write and `commit`. The second walk
  (re-read of `next`) is gone. Contract change, only on the spill-OOM path:
  a `prepare_for` refusal on the FIRST block is `Err` with nothing changed
  (as before); a refusal on a later block ends the batch there — the
  committed prefix is returned as a short drain, the refused block stays the
  head (before: `Err`, nothing committed). Every caller already treats a
  short drain as "fewer blocks here".

Callgrind (WSL, `b4_pmu_probe bulk sefer 64 16384 40`, Ir per alloc+free
pair, ИЗМЕРЕНО before this file): base 287.9 (drain 63.1); b2r1 278.7
(drain 53.9). Instructions are not time; the verdict is the time judge.

Protocol: J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
blocks, all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap.

Thresholds (same as B1):

1. Win: bulk16k OR bulk64 effect CI upper < 0.
2. No harm: pair16, pair64, larson effect ≤ +1 % and CI upper ≤ +1.5 %.
3. No harm, other kernels: not (CI lower > 0 AND effect > +2 %).

GO = 1 ∧ 2 ∧ 3. Path activation: every bulk portion refills the magazine
from the bin chain (bulk frees 16 384 blocks per pass, then reallocates
them), so the changed drain runs on bulk by construction; callgrind counts
the drain calls per variant.
