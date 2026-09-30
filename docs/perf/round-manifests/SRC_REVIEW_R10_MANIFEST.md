# Source review round 10: accepted remediation and evidence

Date: 2026-09-30. Independent XS baseline:
`40dcce1141634018672c0cbcd07e2cfef7239101`.
Report: `docs/reviews/2026-09-30-192803-src-review-xs-sol-round-10.md`,
committed separately as `a40f8554e388bcabe1770c076ed041a770b01d53` (docs-only).

Confirmed queue at review: P0=0, P1=0, P2=0, P3=1, P4=3. All four findings
have accepted fixes. Independent XS11 is pending; no release GO or measured
speedup is claimed.

Accepted: `e37d6e75c65e69f5155b17c3bc7d5bc7670c22e7` corrects the three
P4 contracts, exposes the unchanged live NUMA geometry calculation for its
pure witness, and tightens the feature-list guard; no allocator semantics
were changed. `dcbb8f8db5615b8ea0ff3270f5fcfafc4c80a4c8` commits the XXS
active-index advice (docs-only). Runtime `69b86478bb7563ef2beec5de807fbbe3902a26ef`
and model wiring `2cbf515aa39b33da2c2b3eb616af05aace5f5e69` are accepted.

## Commit classification

Derived from `git log --reverse --format="%H %s" a40f8554..2cbf515a`:

| Commit | Classification |
|---|---|
| `4f07786b00a5a3b2dccebf3a2bc01a8443f877f7` | docs-only queue |
| `e37d6e75c65e69f5155b17c3bc7d5bc7670c22e7` | contract docs, unchanged pure geometry extraction, guards |
| `dcbb8f8db5615b8ea0ff3270f5fcfafc4c80a4c8` | docs-only advice |
| `cc5d32e46a25b0ca1af6a7d65c8f4863b430e861` | docs-only acceptance record |
| `a3ffebd7962a1c746499ecc9855683139b3d56bd` | formatting-only old route-boundary test |
| `69b86478bb7563ef2beec5de807fbbe3902a26ef` | production candidate enumeration, tests and inventory |
| `2cbf515aa39b33da2c2b3eb616af05aace5f5e69` | CI/local Loom wiring, sentinel floor and coupled card |

Report commit `a40f8554` precedes this table. Resolve this record's own latest
finalization commit with `git log -1 --format=%H --` followed by its path;
it cannot embed its own hash without changing it.

## Tasks and factuality

- **R10-P3-1, accepted:** owner hot discovery/drain loops previously inspected
  historical high-water, not just live candidates. Parent checked
  `drain_large_sidecar_ingress`, Large alloc, Small refill, routed negative
  directory fallback and table recycle/count semantics. A live-index design
  must cover both Small/Primordial and Large paths, mutation during scan,
  register/unregister/recycle/rollback/reuse and late publication after a cut.
  XXS advice is accepted; HS implemented an owner-only active-kind bitset
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

Parent read the entire source/test diff and reran the following checks.
Earlier R9 validation has its own manifest and is not passed off as R10
runtime-fix evidence.

## Runtime acceptance

- The 1040-byte exact membership index is explicitly initialized in the
  existing primordial metadata page; layout assertions keep it aligned,
  non-overlapping and covered by eager/lazy commit. Registration uses its
  passed kind, not an uninitialized header. Prepare failures leave membership
  untouched; actual removal clears the resolved slot. Pool Small stays active,
  cache Large is inactive; slot 0 stays Primordial/Small.
- Hot loops use numeric next-kind positions and fresh canonical roots, with
  advance before removal. NUMA fallback revalidates its numeric slot. Full
  strict and bounded cold scans, credits and producer operations are unchanged.
  No new production unsafe site or producer atomic was added.
- Actual hot counters at H=128/A_L=1 report one Large probe for Large alloc
  and genuine first Small magazine miss. Routed negative/rescue reports two
  Small probes. This reduces ghost-slot candidate work, not necessarily all
  allocation work: a live Small candidate can still require word cuts.
  No latency/RSS result is inferred.
- Parent focused native: **69 passed**, task
  `00f15a52-e34c-466a-9db3-3633e74d684f`, including late-publish barriers,
  final Small credit, independent cold census, logical boundaries/mutant,
  prepare OOM/retry, M5, pool/cache and R9/owner controls.
- No-decommit/no-directory matrix: six positive tests (its cfg-excluded R9
  target ran zero and is not counted). Reduced membership/mutant plus lease:
  three model tests. Mock-NUMA current-root fallback/layout: four tests.
  Task `c02cbc1e-c9ef-4f01-9134-73e7adfd91af`. These abstract models do not
  prove full index registration/rollback or every production branch.
- Explicit strict-provenance Miri actual bootstrap and three-Small-root
  recycle/drop each ran one selected positive test, exit 0, task
  `ef4129ae-e117-42ea-a6a4-c8214ae2e883`. Validation/borrow tracking stayed on.
  This is not installed-global-allocator Miri.
- Final full native root suite with
  `production,internals,bench-internals,alloc-stats,batch-api`, offline/locked,
  no-fail-fast, two test threads: **718 passed, 6 ignored**, 300 result groups
  (246 with executing tests), exit 0, task
  `f6df1149-2fe0-4a8f-802f-bc8e8ae5a550`. Counts were parsed from saved
  Cargo result summaries; cfg-empty/ignored cases are not claimed as passed.
- Post-split all-target clippy `-D warnings` passed in task
  `37dd7c5f-987e-4de3-97d5-a7bd62114c93` (that task's earlier full-suite
  job was not green). Root all-feature rustdoc `-D warnings` passed in
  `184388a3-c529-4272-94b9-234cb8b28142`. Full 505-target fmt check passed
  after the formatting-only fix, `ea4f2a5d-0097-4623-9bcb-437405355295`.
  Parent actionlint and all 103 static sentinels passed; new model is wired
  in CI and the local Loom runner, not just present as a test file.

### Failures found and repaired during acceptance

The first focused run stopped on stale seven-model inventory; updated it to
eight models and wired the new test. First full suite then found
`segment_table_impl.rs` at 1022 lines against its 1000-line cap. Parent moved
the new enumeration/census implementation to `active_kind_ops.rs` without
changing behavior or raising the cap. A repeat full suite caught the live
item-87 Next-trigger count still at101 while its headline/floor were103;
both are now103 and all33 doc guards plus the cap passed. These failed runs
(`bcf2b0c4-28a6-491e-b3a1-85e7d6864e26`,
`37dd7c5f-987e-4de3-97d5-a7bd62114c93`) are not counted as green. Tool result
delivery timeouts were recovered from original completed results; no duplicate
test job was launched to guess completion and no unrelated process was killed.

## Remaining validation limits

Real multi-node/Unix/32-bit execution, TSan, full installed-allocator Miri and
latency/RSS measurements are not certified here. The old installed-allocator
Miri acceptance gap remains explicit. Focused positive cases do not close it.

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
