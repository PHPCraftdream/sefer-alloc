# Source review round 10: active remediation record

Date: 2026-09-30. Independent XS baseline:
`40dcce1141634018672c0cbcd07e2cfef7239101`.
Report: `docs/reviews/2026-09-30-192803-src-review-xs-sol-round-10.md`,
committed separately as `a40f8554e388bcabe1770c076ed041a770b01d53` (docs-only).

Confirmed queue: P0=0, P1=0, P2=0, P3=1, P4=3. This round is not complete
and does not claim a release GO or measured speedup.

## Tasks and factuality

- **R10-P3-1, OPEN:** owner hot discovery/drain loops inspect historical
  high-water, not just live candidates. Parent checked
  `drain_large_sidecar_ingress`, Large alloc, Small refill, routed negative
  directory fallback and table recycle/count semantics. A live-index design
  must cover both Small/Primordial and Large paths, mutation during scan,
  register/unregister/recycle/rollback/reuse and late publication after a cut.
  XXS is advising on this central lifecycle/index change before HS implements.
  A producer hint alone is not an accepted substitute for a sound protocol.
- **R10-P4-1, OPEN:** `Node::offset`'s segment-only proof does not describe
  the wider checked NUMA biased reservation. HS audits actual call extents,
  then corrects the contract/per-call proof and a genuine geometry witness.
  No UB is inferred merely from the inaccurate comment.
- **R10-P4-2, OPEN:** `SeferAlloc` docs contain removed `class-aware-dirty`
  in the production list; HS reconciles the real manifest set and guards it.
- **R10-P4-3, OPEN:** internal `alloc_batch` describes zero as OOM-only,
  despite its empty-slice early return; HS corrects docs and behavior witness.

Fixes are isolated and unaccepted until parent reads diffs and reruns checks.
No new test/build/performance result belongs to this report yet. Earlier
R9 validation has its own manifest and is not passed off as R10-fix evidence.

## Acceptance contract

Require actual candidate-inspection counts after churn and sparse reuse,
not a vacuous constant return. Root provenance and outstanding credits must
remain unchanged; a candidate reclaimed during a traversal must not cause
another candidate to be skipped. Preserve full explicit trim/TLS/claim and
bounded background scans. No GlobalAlloc recursion from index storage;
no new producer RMW/global contention without a separate proven design.
Measure wall-clock/CPU separately before claiming speedups. No fake load.

Commit fixes separately, preserve user changes, remove accepted worktrees,
then commission independent XS round 11. No push or version changes.
