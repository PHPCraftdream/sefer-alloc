# Source review round 9: incremental acceptance record

Date: 2026-09-30. Scope: root `src/`; companion-crate audits are excluded.
This is an unfinished round, not a release GO. Review baseline:
`77058068f534dde21725cf9c04eef79fef07069a`.

## Accepted commits

Derived from `git log --reverse --format="%H %s" 77058068..85551d7f`:

| Commit | Classification | Task |
|---|---|---|
| `ece93a6c5d300fbc500b98814c52d734d3d97761` | docs-only | Independent XS round 9 report |
| `f7e1c5bba6a8a90ccb55e3c272be31acb97aca0d` | correctness, diagnostic API | R9-01 kind guards and regression; test inventory 286 |
| `09fa1f3a1deea7f64a0b4a2d3d8038147130fa12` | docs-only | XXS bounded-maintenance design advice |
| `85551d7ffd332bdba87aa28c79e49e3f0fe522d5` | docs and executable example/guards | R9-03/R9-04 current stats and sidecar contracts |

The manifest's own commit is resolved with `git log -1 --format=%H --
docs/perf/round-manifests/SRC_REVIEW_R9_MANIFEST.md`. Later fixes require
additional rows; the table does not claim to cover future commits.

## Findings and validation

- R9-01 P2: accepted. Guards run after numeric membership/canonical-root
  lookup and before Small views; class bounds remain. Large returns the
  documented sentinel/false. Native negative control in the worker removed
  the guards and failed both Large witnesses; Small positive control passed.
  Parent reran `r9_diagnostic_reader_kinds` (3 tests) and
  `regression_flush_class_unsafe_boundary` (2 tests) with
  `alloc-core,internals`: exit 0, task `02fd04e9-3570-4da5-b06b-d083c91c9187`.
  Parent focused Miri with the same features and a prepared sysroot: 3/3,
  exit 0, task `60342d2b-943e-4f47-8ef9-916bad23eba3`. No borrow tracking
  or validation was disabled. This is not installed-global-allocator Miri.
- R9-03 P3 and R9-04 P4: accepted documentation/example changes. Parent
  compiled and executed `sefer_alloc_examples` (3/3) and
  `no_stale_doc_references` (30/30) with
  `production,internals,bench-internals,batch-api`, exit 0, task
  `e2f7cbfa-fadf-4a9a-8838-47b37132cd74`. Runtime algorithms were unchanged.
- R9-02 P3: OPEN. A separate HS worktree implements the consultant's
  bounded numeric background cursor. Explicit trim/TLS/claim retain their
  complete retirement contract. Implementation and acceptance are pending.
- Expanded rustdoc: OPEN, observed failure in that same parent task.
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` with the expanded feature
  set found broken/private links outside the original example. A separate
  HS worktree fixes them; a worker's exact-production rustdoc pass is not
  evidence that this expanded invocation passes.

## Unchanged limits

No measured speedup, RSS improvement, wall-clock reclaim SLA, complete
feature/platform certification or release GO is asserted. The installed
global-allocator Miri acceptance gap remains open; the small diagnostic
Miri pass above does not close it. Production's feature composition and
package versions are unchanged. No push was performed.
