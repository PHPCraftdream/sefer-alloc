# Source review round 9: incremental acceptance record

Date: 2026-09-30. Scope: root `src/`; companion-crate audits are excluded.
This is an unfinished round, not a release GO. Review baseline:
`77058068f534dde21725cf9c04eef79fef07069a`.

## Accepted commits

Derived from `git log --reverse --format="%H %s" 77058068..0bc0b691`:

| Commit | Classification | Task |
|---|---|---|
| `ece93a6c5d300fbc500b98814c52d734d3d97761` | docs-only | Independent XS round 9 report |
| `f7e1c5bba6a8a90ccb55e3c272be31acb97aca0d` | correctness, diagnostic API | R9-01 kind guards and regression; test inventory 286 |
| `09fa1f3a1deea7f64a0b4a2d3d8038147130fa12` | docs-only | XXS bounded-maintenance design advice |
| `85551d7ffd332bdba87aa28c79e49e3f0fe522d5` | docs and executable example/guards | R9-03/R9-04 current stats and sidecar contracts |
| `115ed78b698429d44f9c211a98dfef6485ac5f3c` | docs-only | Incremental round acceptance record |
| `1ecb8a55ca548baed244670e8e4c36b3a027abc9` | docs and source guard | Expanded rustdoc-link repair; no visibility/runtime changes |
| `f2f69e85f33d27507fca1eabffcb304db1e9e678` | docs-only | Strict-docs acceptance and gate disposition |
| `0bc0b69130450a8c268a4665cdc1896c28f807f3` | CI and executable source guard | Warning-strict root all-feature / metadata-derived docs.rs gate |

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
  Parent subsequently repeated with explicit
  `MIRIFLAGS="-Zmiri-strict-provenance"`: 3/3, exit 0, task
  `16cface6-ee9e-4b96-9b84-0fd1fbc072ab`.
- R9-03 P3 and R9-04 P4: accepted documentation/example changes. Parent
  compiled and executed `sefer_alloc_examples` (3/3) and
  `no_stale_doc_references` (30/30) with
  `production,internals,bench-internals,batch-api`, exit 0, task
  `e2f7cbfa-fadf-4a9a-8838-47b37132cd74`. Runtime algorithms were unchanged.
- R9-02 P3: OPEN. A separate HS worktree implements the consultant's
  bounded numeric background cursor. Explicit trim/TLS/claim retain their
  complete retirement contract. Implementation and acceptance are pending.
- Expanded rustdoc: accepted after the observed failure in that same parent task.
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` with the expanded feature
  set found broken/private links outside the original example. Parent read
  the entire 14-file docs/guard diff and repeated both exact `production`
  and expanded rustdoc with `-D warnings`: exit 0 for each. Doc guards
  passed 31/31, task `e09dd54f-ab58-4284-9f3b-dd9ee4505850`. This does not
  claim every feature set or a newly wired continuous CI rustdoc gate.
  Parent `--all-features` rustdoc also passed with `-D warnings`, task
  `6fb4144d-db1f-4495-88f2-def6fa83f415`; correctness item 156 retains
  the separate CI-wiring obligation until `0bc0b691`. Parent then checked
  all 32 doc guards, actionlint, all 101 sentinels and metadata-derived docs.rs
  docs (`b41900b4-0209-41c4-969d-d3c9df677028`,
  `8822e0b6-1544-4fb3-a236-7227f9229496`). Item 156 is closed for source/gate
  implementation and local validation, not remote CI execution.
- Parent all-target clippy with the expanded feature set and `-D warnings`
  passed on the accepted kind/stats tree, task
  `f684b53c-5732-476e-9ec4-e2445d1321a4`. Later source fixes need fresh checks.
  The repeat after strict-doc/CI acceptance also passed, task
  `b15e8cca-2ec4-4e71-ab97-899c3844204b`.

## Unchanged limits

No measured speedup, RSS improvement, wall-clock reclaim SLA, complete
feature/platform certification or release GO is asserted. The installed
global-allocator Miri acceptance gap remains open; the small diagnostic
Miri pass above does not close it. Production's feature composition and
package versions are unchanged. No push was performed.
