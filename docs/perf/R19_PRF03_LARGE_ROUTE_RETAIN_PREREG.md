# PRF03 Large route retention — preregistration (phase 2, before B)

No measurements or runtime changes in this phase. Authority: owner Question4 in
`docs/reviews/2026-10-11-owner-questions-decisions-ox.md` (entire Question4),
with the user's subsequent permission to implement B on the reported 615 Ir.
`R19_PRF03_ROUTE_ATTRIBUTION.md` remains INCOMPLETE: missing exact-layer
activation oracle and actual iai consistency. Permission is not a completed
measurement receipt; neither that report nor item86 is rewritten here.

## Frozen criteria

Primary entry point: HeapCore alloc/dealloc through a registry lease; identical
prefill and hit bodies, production bench-internals internals, WITHOUT alloc-stats.
R1 is hit Instructions minus prefill Instructions, one tail alloc/free cycle.
A1/A2 use ONE A binary; B1/B2 use ONE B binary; C0 uses a different binary built
from the SAME A source identity and environment. Collect all five observations
for each cell, not separate executables for hit and prefill.

For EACH cell T = max(10, abs(C0-A1)). All cross A1/A2 versus B1/B2 comparisons
must pass (conservative repeatability, not cherry-picking means):

- R1: B <= A - 200 Ir. R1 is EXACTLY the pair `large_cache_prefill_only_4mib` /
  `large_cache_hit_only_4mib` of `benches/perf_gate_iai.rs` (HeapCore, features
  `production bench-internals internals`); the inventory names the single R1 cell
  `large_cache_hit_only_4mib` and its row carries both measured bodies (`hit`, `prefill`).
- R2: eviction-dominated, working set beyond cache capacity; B-A <= T.
- R3: alternate align 8 / 65536 with changed payload; B-A <= T.
  For R2/R3 abs(delta)<=T OR better is equivalent to delta<=T.
- A2 must agree with A1 within T; otherwise INCONCLUSIVE, no GO.
- affected, NOT controls: large_alloc_free_cycle, every large_cache_free_slot_search_*
  cell, realloc_grow. Report deltas but do not impose the control bound on them.
- controls: small_churn_16b, churn_256b, every alloc_magazine_* and
  cold_alloc_only_* cell: abs(B-A)<=T. Mimalloc cells: delta exactly zero.

Before measurement freeze a complete named cell inventory extracted from the
chosen benchmark source, with source evidence, including all family members.
The JSON input carries `inventory` with `{name, kind}` (kind R1/R2/R3/control/
affected/mimalloc). No unknown, duplicate or missing cells. The judge requires
all explicit names and all families; inventory completeness still needs personal
source review, not merely successful parsing.

## Activation, feature-exact blocker

Outside timed bodies, on the exact HeapCore tail: hit delta 1; incarnation delta
A=1/B=0; Large census cached A=0/B=1 (relative to isolated prefill baseline, not
process-total including other heaps). Record routed scans per arm, >=0 and equal
across A/B; no unobserved scan activity. Record R2 actual eviction and R3 changed
payload activation before trusting labels. No cost/benefit across mixed regimes.

KNOWN ISSUE: phase1 observed dbg_large_cache_hits ==0 without alloc-stats; the
increment is alloc-stats-gated. Do NOT add alloc-stats to primary Instructions.
A paired oracle run uses production bench-internals internals alloc-stats on
identical HeapCore/body/source, reporting hit delta1 and the other oracles.
This alone is insufficient to prove the feature-exact primary path. GO remains
blocked until a reviewed feature-exact path receipt or a justified B implementation
of bench-internals hit bookkeeping (with its A/B accounting) exists. Select and
justify that decision during B, not by silently changing this preregistration.
JSON `featureExactReceipt` must reference an actual retained review receipt;
`oracleFeatures` must name the paired features exactly. Receipt existence and
source linkage require personal review; the judge cannot prove prose true.

## Native secondary axis

Real installed GlobalAlloc, N=1/4/8, nine fresh isolated processes PER arm A,
independent AA repeat, and B per N (81 processes total). Fixed work count per
process; raw per-process ns/op and per-process latency p99 in nanoseconds.
Report median ns/op and median process-p99 separately, NOT pooled p99.
Pre-register a 5% maximum relative A/A noise band for BOTH axes (engineering
choice for 'too wide', not an already measured noise value).
For each axis noise=abs(median(AA)-median(A))/median(A).
Noise>0.05 => INCONCLUSIVE, no GO. Otherwise median(B)<=median(A)*(1+noise).
Do not substitute 5% for the actually observed narrower noise band.
No MT speedup claim from Ir; every process has a distinct runId and OS pid,
explicit isolated-process boundary and matching environment identity.

## Configuration and provenance

Every observation carries REQUESTED/RESOLVED objects, exactly equal,
configConflictsDelta=0, processBoundary, runId, pid. Native processes carry these
individually, not one aggregate label. Include cache budget/slots, maintenance
policy, and workload size in those objects; compare matched configurations.
Record compiler, target, OS/CPU, allocator, flags, work count and units in the
retained environment manifest. Same environment digest throughout each judge.

BEFORE any measurement/build capture immutable source identity for A and B:
retain raw `git diff --binary HEAD --` bytes (includes staged and unstaged tracked
changes), base HEAD, and a sorted manifest of ALL `git ls-files --others
--exclude-standard -z` paths with SHA256, byte length and retained exact bytes.
Include phase1's untracked test and this phase's new files. Include ignored
source/build inputs separately if used; target outputs are not source inputs.
Identity digest recipe: SHA256 of UTF-8 JSON `[baseHead, patchSha256,
[[path,byteLength,sha256],...]]` with sorted paths, no whitespace; store the
patch, manifest and untracked bytes before build. No add/write-tree/commit.
Do not use the existing capture-measurement-identity script: it writes git objects
and misses untracked working-tree bytes. Re-capture B after implementation and
freeze before build/run. Record capture timestamp < every measurement start.
Archive input files and receipts by content hash; data is not a reconstructed
identity recipe. `source` records contain baseHead, patchSha256, untrackedSha256,
identitySha256 and capturedAt; all hashes are 64 lowercase hex.

## Deriver input/output and fail-closed behavior

`scripts/r19_prf03_large_route_retain_gate.mjs` is a read-only JSON deriver, NOT a
runner. `node ... --input FILE` emits one JSON containing tables and verdict;
non-GO exits nonzero. No arguments => INCOMPLETE. Invalid/missing evidence =>
INCOMPLETE; valid complete violated thresholds => NO-GO; noisy repeats =>
INCONCLUSIVE. All numeric values finite, safe, nonnegative where required.

Schema version1: top-level prereg hashes (`hashes`), environmentSha256,
inventoryReceipt, featureExactReceipt, inventory, iai and native arrays.
Each iai row: name, arm (A1/A2/B1/B2/C0), environmentSha256, binarySha256, source, startedAt,
features, oracleFeatures, receipt, config, hit, prefill, routedScans,
activation (R2/R3 must be true), oracle {tailHitDelta,incarnationDelta,cachedCensus}.
Each native row: n, arm (A/AA/B), environmentSha256, binarySha256, source, startedAt, features
(exactly production), receipt, config, nsPerOp, p99Ns. `config` contains requested,
resolved, configConflictsDelta, processBoundary, runId, pid.
R1 oracles are required in every five-arm row; all rows require routedScans.
`receipt` is the retained raw-data/paired-oracle evidence reference.

Synthetic `--selftest` checks rejection of missing fields, bad features,
wrong identities/binaries/oracles/config, incomplete processes, threshold
violations and wide noise, plus boundary arithmetic. Synthetic GO is an internal
assertion only: it is never printed as a real result. No actual data this phase.

## Amendment A1 (before any measurement, thresholds unchanged)

Applied by the orchestrator after reviewing the B candidate, before the first build of a measured
binary; no threshold, arm definition or regime was changed.

1. The R1 cell is pinned to the exact pair named above (the judge rejects any other R1 name).
2. The release-boundary observer used by the ordering test (a full directory census per cached
   release) is compiled only with `debug_assertions`, so it is absent from the release/bench
   profiles of every performance arm; arms A and B differ only by the B candidate itself.
3. Six pre-existing tests that encoded `fresh incarnation after a same-payload cache reuse`
   (or the exact source location of `release_cached`) were adapted to the retained-registration
   contract; their other assertions are unchanged. Re-frozen hashes below supersede the first freeze.

## Freeze hashes (no self-hash)

Companion `R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.sha256` contains the SHA256 of
raw document bytes and raw script bytes separately. Recipe from worktree cwd:
`node scripts/r19_prf03_large_route_retain_gate.mjs --hashes`.
Only those two named files participate, in document/script order; the companion
is excluded. `--check-hashes` compares recomputation with the companion. Future
input must cite both frozen digests; any later edit requires explicit re-freeze
BEFORE measurement, never retroactive threshold adjustment.
