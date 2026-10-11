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

## Amendment A2 (before any measured build, thresholds unchanged)

Applied by the orchestrator on the arbiter's decisions before the first build of any measured,
oracle, probe or twin binary of this gate (no gate build directory `target/gate-prf03-*`
existed; the `target/prf03-*.log|mjs|json` files of the candidate worktree are its test logs,
not gate artifacts); no threshold, arm, regime, cell kind or inventory rule changed. Earlier text stays as written; where it
conflicts, this amendment governs.

1. Routed scans. `routedScans` is the delta of `AllocCore::dbg_routed_miss_scans()` (routed
   negative-directory miss scans, incremented only with alloc-stats) over the whole hit body
   (prefill sequence plus tail), read outside the body by that row's paired oracle process.
   It is required, as an equal integer across all five arms, only on R1/R2/R3 rows. Control,
   affected and mimalloc rows carry `routedScans: null`: their bodies execute only under
   Callgrind, where no observer exists, and the judge rejects a number there. A value from a
   build without alloc-stats is never used (the counter is compiled out and reads 0). Sidecar
   drain and rescue scans (the `r18_sidecar_scan_bench` observer is outside the frozen feature
   set) are not observed by this gate; the report names them as unobserved, never as zero.
2. Subtrahend. Only R1/R2/R3 rows have a prefill body and an integer `prefill`. All other rows
   carry `prefill: null`; their value is the cell's own `hit` Instructions alone, never a
   placeholder 0.
3. Configuration of rows whose own process cannot read it back. Such a row carries its own pid
   and runId (iai: parsed from that cell's retained Callgrind/valgrind log; no log means
   INCOMPLETE) and copies `resolved` and `configConflictsDelta` from a probe process of the
   same source arm and heap-claim path, recorded in `config.resolvedProvenance` (probe runId,
   pid, binary SHA256, raw receipt SHA256, copied fields). R1/R2/R3 rows use their paired
   oracle as the probe. Non-regime iai rows use one probe per source arm and claim path
   (`lease`: `HeapRegistry::dbg_claim_lease`; `global`: a `SeferAlloc::new()` instance on the
   main thread of a fresh process, with an owned-heap check); mimalloc rows record cache
   budget, slots and maintenance as null (not applicable). `workCount` is the executed
   operation count where a process counts it (oracle, native); on non-regime iai rows it is
   null in both objects, and the declared constant lives in the inventory receipt.
   `oracleFeatures` on non-regime rows only declares the gate's paired feature set; no paired
   process exists for those rows.
4. Incarnation delta. `oracle.incarnationDelta` is the number of route incarnations issued
   process-wide during the R1 tail cycle. It is bracketed by two probe registrations on
   `RouteDirectory::global()` (public `register`, kind Large, a System-owned SEGMENT-aligned
   span that no allocator reservation can share, sentinel owner, each dropped at once, both
   outside the tail) and equals `after.incarnation() - before.incarnation() - 1`: 1 in A,
   0 in B. Consistency checks: the tail pointer equals the last prefix pointer (same root,
   payload, owner, kind Large), and the tail's live route incarnation is `before + 1` in A and
   equals the last prefix route's incarnation in B. No B-only observer is used.
5. Feature-exact receipt (selected route; B gains no hit bookkeeping). The same oracle source
   is built per source arm as primary (`production bench-internals internals`) and as paired
   (plus alloc-stats). Outside the tail, the primary build must show the hit exit by a
   signature that needs no alloc-stats:
   - occupied base cache slots are 1 before the tail alloc, 0 after it and 1 after the tail
     free;
   - `AllocCore::dbg_segments_reserved_total()` and `dbg_segments_released_total()` deltas are
     0 over the tail;
   - the tail pointer equals the cached block;
   - relative to the pre-prefill baseline, the Large route census is +1 after the tail alloc,
     and A 0 / B 1 after the tail free.
   Occupancy alone is insufficient: two exits of the cache arm take a slot, release it and
   reserve a fresh one. In the primary build `tailHitDelta` and routed scans are null. Every
   structural field equals the paired build of the same arm; the paired build adds
   `tailHitDelta = 1`. A negative control (a tail too large for the cached slot) shows the
   miss signature (occupancy unchanged, reserve +1, paired `tailHitDelta = 0`) in both builds.
   R2 (reserve = release = OBJECTS - slots over the tail; occupancy `slots` before and after)
   and R3 (same root, changed offset, reserve and release 0 over the tail alloc) are checked
   the same way in both builds. `featureExactReceipt` is a reviewed file citing, per arm: both
   oracle binary hashes, the raw outputs and their hashes, the source identity, and an
   enumeration of every exit of the cache arm of `alloc_large` with its (occupancy, reserve,
   release) signature, showing that only the hit exit is (-1, 0, 0).
6. Native twin. Each native process is immediately preceded by its own twin: the same example
   source and arm, built as `production bench-internals internals`, with the same
   `#[global_allocator] SeferAlloc::new()` and the same N.
   - After warmup, each twin worker reads `dbg_current_large_cache_total_slots()` (non-zero
     means an owned heap) and `dbg_current_large_cache_budget()` (`None` is recorded as
     `{policy:"unbounded"}`); all workers must agree.
   - The native row copies these into `resolved.cacheBudget` and `resolved.cacheSlots`, with
     provenance as in item 3. `resolved.maintenance` (`SeferAlloc::maintenance_running()`) and
     `resolved.workCount` (samples taken) are read in the native process itself.
   - A reviewed note must show that config resolution has no `bench-internals`/`internals` cfg,
     and that the two builds differ in exactly those features.
   - Each native process asserts, through public production APIs only: config-conflict and
     fallback-conflict deltas 0; maintenance off; a `heaps_claimed_high_water` delta of exactly
     N; and `segments_reserved_total`/`segments_released_total` deltas of 0 from the ready
     barrier to the done barrier. This shows every timed cycle was a cache hit, with no
     eviction or decay.
7. Judge changes are limited to items 1-2 and their selftest mutations. Items 3-6 are evidence
   rules enforced by the runner aggregate and by personal review. Re-frozen hashes below
   supersede the A1 freeze.

## Freeze hashes (no self-hash)

Companion `R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.sha256` contains the SHA256 of
raw document bytes and raw script bytes separately. Recipe from worktree cwd:
`node scripts/r19_prf03_large_route_retain_gate.mjs --hashes`.
Only those two named files participate, in document/script order; the companion
is excluded. `--check-hashes` compares recomputation with the companion. Future
input must cite both frozen digests; any later edit requires explicit re-freeze
BEFORE measurement, never retroactive threshold adjustment.
