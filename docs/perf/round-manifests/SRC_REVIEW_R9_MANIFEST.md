# Source review round 9: accepted fixes and verification

Date: 2026-09-30. Scope: root `src/`; companion-crate audits are excluded.
All confirmed R9 findings have accepted fixes. Independent round 10 is
pending; this is not a release GO. Review baseline:
`77058068f534dde21725cf9c04eef79fef07069a`.

## Accepted commits

Derived from `git log --reverse --format="%H %s" 77058068..40dcce11`:

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
| `7c6a0918888dff42c7e03a919bda695312897fff` | docs-only | Item 156 source/gate closure with local-only evidence |
| `4e8f76c2242bcd61d9b2279936c6c2c21b4afcb9` | docs-only | Explicit strict-provenance and post-CI lint receipts |
| `40dcce1141634018672c0cbcd07e2cfef7239101` | cold-path work bound, tests, inventory | R9-02 background cursor; full trim preserved; test inventory 287 |

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
- R9-02 P3: accepted in `40dcce11`, after parent read the full implementation
  and 10-test fixture. Background registry/fallback visits persist a numeric
  cursor and charge at most 64 ingress units. A Small unit includes root/word
  inspection and one unconditional AcqRel cut; NULL/finished slots and Large
  claim attempts also consume units. Cold cache/pool policy remains separate;
  no wall-clock or total-CPU bound is inferred from this ingress budget.
  Explicit trim/TLS/claim still use the complete sweep, not a partial step.
  No producer hint, additional producer RMW or retained cursor pointer was added.
  At stable geometry, `ceil((H + sum(W_i))/64) + 1` successful visits is a
  conservative reachability bound, conditional on exclusive access/progress.
  Native fixtures check idle cursor increments, early/late words, credits,
  reuse/removal/high-water growth, late publication after an idle round and
  busy fallback. The sequential pre-terminal-hint counterexample is not
  misrepresented as a weak-memory model.
  Parent targeted native acceptance passed (60 tests across seven groups),
  task `9ff3e6d6-516b-4f7b-b3df-77b367197262`.
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

## Integrated runtime acceptance

- Full root native suite, `production,internals,bench-internals,batch-api`,
  offline/locked, `--no-fail-fast`, two test threads: exit 0, **688 passed**,
  **6 ignored**, 289 result groups (232 with executing tests). Task
  `ef6a6b16-c668-4ea0-afac-0e7ac7d8d951`. Cargo's saved summaries produced
  these counts; ignored or cfg-empty groups are not presented as passed cases.
- Post-runtime all-target clippy `-D warnings`: exit 0. Eight reduced ordering
  model tests covering class/bit publication, credit lifetime, Large terminal
  admission and exclusive maintenance lease also passed, including negative
  controls. Task `7b529972-2bf3-4dc5-a407-7640adcfcee0`. These models are not
  a proof of every production branch, OS behavior or entire cursor algorithm.
- Parent explicit strict-provenance Miri, production/internals/bench-internals:
  one real Small word cut and NULL/Large-slot reuse each ran **one** selected
  test and passed. Task `aad4b46d-ebf7-45a2-a261-44015a5ccdf4`. The tool's
  result-delivery timeout was recovered from saved completed exit-0 jobs;
  jobs were not relaunched. Validation/borrow tracking were not disabled.
- Worker attempted the long Small sweep under Miri but stopped its own PIDs
  after 499 seconds/resource pressure; that attempt did **not** pass and is
  not counted above. Full installed-global-allocator Miri remains separately
  unaccepted. No host OOM or incomplete run is claimed as a code counterexample.

New runtime unsafe operation in `RouteSlots::scan_small_from` is constrained
by checked slot capacity, initialized owner-only registration and canonical
root identity. Registry mutation remains inside the successful maintenance
lease; failed CAS grants no access. All detached records and route borrows
end before directory synchronization, pooling or physical reservation release.
Class acquisition and credit accounting are unchanged; late publications
remain outstanding until a subsequent cut. No manual Send/Sync impl was added.

## Unchanged limits

No measured speedup, RSS improvement, wall-clock reclaim SLA, complete
feature/platform certification or release GO is asserted. The installed
global-allocator Miri acceptance gap remains open; the small diagnostic
Miri pass above does not close it. Production's feature composition and
package versions are unchanged. No push was performed.
