# Source review round 10: active remediation record

Date: 2026-09-30. Independent XS baseline:
`40dcce1141634018672c0cbcd07e2cfef7239101`.
Report: `docs/reviews/2026-09-30-192803-src-review-xs-sol-round-10.md`,
committed separately as `a40f8554e388bcabe1770c076ed041a770b01d53` (docs-only).

Confirmed queue: P0=0, P1=0, P2=0, P3=1, P4=3. This round is not complete
and does not claim a release GO or measured speedup.

Accepted: `e37d6e75c65e69f5155b17c3bc7d5bc7670c22e7` corrects the three
P4 contracts, exposes the unchanged live NUMA geometry calculation for its
pure witness, and tightens the feature-list guard; no allocator semantics
were changed. `dcbb8f8db5615b8ea0ff3270f5fcfafc4c80a4c8` commits the XXS
active-index advice (docs-only). Remaining runtime implementation is unaccepted.

## Tasks and factuality

- **R10-P3-1, OPEN:** owner hot discovery/drain loops inspect historical
  high-water, not just live candidates. Parent checked
  `drain_large_sidecar_ingress`, Large alloc, Small refill, routed negative
  directory fallback and table recycle/count semantics. A live-index design
  must cover both Small/Primordial and Large paths, mutation during scan,
  register/unregister/recycle/rollback/reuse and late publication after a cut.
  XXS advice is accepted; HS now implements an owner-only active-kind bitset
  in existing primordial metadata, with constant-time summary/leaf lookup.
  Strict and bounded cold scans and producer terminal operations stay unchanged.
  A producer hint alone is not an accepted substitute for a sound protocol.
- **R10-P4-1, accepted:** `Node::offset`'s segment-only proof did not describe
  the wider checked NUMA biased reservation. Contract and caller proof now
  require live provenance, fitting/nonwrapping offset and in-allocation or
  one-past result. The live checked calculation has a >SEGMENT witness plus
  too-short-extent and address-overflow controls.
  No UB is inferred merely from the inaccurate comment.
- **R10-P4-2, accepted:** `SeferAlloc` docs now list the actual production
  bundle. Parent strengthened the guard beyond a prefix comparison to reject
  a trailing extra feature and an identifier-extension suffix.
- **R10-P4-3, accepted:** internal `alloc_batch` docs now include empty input
  as a zero result. The public wrapper witness checks zero and unchanged
  reservation-failure count; it does not infer that no successful bootstrap
  reservation could occur (the wrapper can bind a heap before the empty call).

Parent personally read all seven changed files and repeated 33 doc guards,
one empty-batch case, five NUMA-seam cases with `numa_shim_mock`, and
all-feature root rustdoc with `-D warnings`: exit 0, task
`20c86212-2b20-4647-bc05-efbe1ff9020d`. Post-polish targeted clippy
`-D warnings` passed, task `7509f10a-1556-4179-9620-fe73d35d8075`.
No real multi-node placement or Miri proof is claimed by these doc fixes.

Runtime fixes remain isolated and unaccepted until parent reads their diffs
and reruns checks. Earlier R9 validation has its own manifest and is not
passed off as R10 runtime-fix evidence.

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
