# P1.4 — sefer-platform extraction

Plan step P1.4 (`docs/plans/2026-11-15-workspace-restructure-plan.md`): the
platform seam moved out of the root crate into
`crates/platform/sefer-platform`. This report contains **no measured
numbers**; every table and counter below is produced by
`node scripts/p14_visibility_report.mjs --write` (compares `origin/main` with
the working tree and asserts the relations it prints) and by
`node scripts/p14_extraction_inventory.mjs` (decision inventory).

## Generated tables

Generated: `node scripts/p14_visibility_report.mjs --write`.

<!-- BEGIN generated -->

### (a) Visibility widening (origin/main `pub(crate)`/private -> `pub` in sefer-platform)

| source @ origin/main | new file | symbol | origin vis | class | raw-ptr / safety note |
|---|---|---|---|---|---|
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `SEGMENTS_RESERVED_TOTAL` | `pub(crate)` | static |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `SEGMENTS_RELEASED_TOTAL` | `pub(crate)` | static |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `SEGMENTS_RESERVE_FAILED_TOTAL` | `pub(crate)` | static |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `SEGMENT` | `pub(crate)` | const |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `PAGE` | `pub(crate)` | const |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `MAX_REALISTIC_PAGE_SIZE` | `pub(crate)` | const |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `segment_base_of` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `segment_base_of_ptr` | `pub(crate)` | safe, address arithmetic only | takes raw ptr: yes; no deref/read/write in the body (derived from the source); does not dereference |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve_biased` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve_exact` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve_capacity_exact` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve_lazy` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve_small_lazy` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reserve_lazy_for_measurement` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::as_ptr` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reservation` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::reservation_len` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::advise_huge_pages` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::try_disable_transparent_huge_pages` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `Segment::try_advise_huge_pages_range` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `release_segment` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `try_release_segment` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `segments_reserved_total` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `segments_released_total` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `decommit_pages` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `commit_pages` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `commit_pages_for_measurement` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `recommit_pages` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `reserve_depot_overflow` | `pub(crate)` | safe fn |  |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `commit_depot_overflow` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/os.rs` | `crates/platform/sefer-platform/src/os.rs` | `decommit_depot_overflow` | `pub(crate)` | unsafe fn | unsafe fn, `# Safety` doc present |
| `src/alloc_core/platform/fatal.rs` | `crates/platform/sefer-platform/src/fatal.rs` | `fatal` | `pub(crate)` | safe fn |  |
| `src/global/process_exit.rs` | `crates/platform/sefer-platform/src/process_exit.rs` | `process_exit_in_progress` | `pub(crate)` | safe fn |  |

| class | widened symbols |
|---|---|
| const | 3 |
| safe fn | 19 |
| safe, address arithmetic only | 1 |
| static | 3 |
| unsafe fn | 8 |
| **total widened** | **34** |

### (b) Symbol counts (visible = `pub`/`pub(crate)` items)

| source @ origin/main | visible at origin | moved to sefer-platform | stayed in root |
|---|---|---|---|
| `src/alloc_core/platform/os.rs` | 36 | 33 | 3 |
| `src/alloc_core/platform/fatal.rs` | 1 | 1 | 0 |
| `src/global/process_exit.rs` | 1 | 1 | 0 |
| **total** | **38** | **35** | **3** |

Stayed in root os.rs (directory trio, asserted equal to the inventory's OS_STAY): `reserve_directory_sidecar`, `read_directory_class_words`, `read_directory_node_bucket`.

Root os.rs additionally keeps real `pub(crate)` safe wrappers with the pre-extraction signatures for the moved raw-pointer functions now `pub unsafe fn` in sefer-platform (asserted: signature, cfg gate and attrs equal to origin/main; body forwards to `sefer_platform::os`): `release_segment`, `try_release_segment`, `decommit_pages`, `commit_pages`, `recommit_pages`, `commit_depot_overflow`, `decommit_depot_overflow`.

### (b2) File decisions and root dependencies (from `scripts/p14_extraction_inventory.mjs`)

| file | decision | root `crate::` deps (origin/main) | reason |
|---|---|---|---|
| `src/alloc_core/platform/fatal.rs` | EXTRACT | none (leaf) | leaf: zero crate:: deps; #[cold] terminal handler; whole file moves |
| `src/alloc_core/platform/os.rs` | SPLIT | `alloc_core::segment_directory::SegmentDirectory`, `alloc_core::sidecar::AccountedSidecar`, `alloc_core::sidecar::SidecarKind`, `alloc_core::sidecar::reserve_zeroed_with`, `alloc_core::segment_directory::WORDS_PER_CLASS`, `alloc_core::segment_header::NO_NODE_RAW`, `alloc_core::segment_directory::MAX_NODES` | backup half (Segment/SEGMENT/PAGE/counters/release/decommit/depot) moves; directory-sidecar trio stays: depends on alloc_core::segment_directory::SegmentDirectory + alloc_core::sidecar::* + segment_header::NO_NODE_RAW |
| `src/alloc_core/platform/node.rs` | STAY | `alloc_core::os::SEGMENT`, `alloc_core::config::continuation_bounds` | root-blockers: continuation_offset (hardened) reads alloc_core::os::SEGMENT, continuation_offset_or_fatal + continuation_fatal read alloc_core::config::continuation_bounds (P1.3 not landed); rest is raw-pointer allocator-metadata membrane whose pub-widening is forbidden by the plan visibility rule; extractable remnant is too small |
| `src/alloc_core/platform/numa.rs` | STAY | `alloc_core::fatal::fatal`, `alloc_core::os::SEGMENT`, `alloc_core::os::SEGMENTS_RESERVE_FAILED_TOTAL`, `alloc_core::os::SEGMENTS_RESERVED_TOTAL`, `alloc_core::os::release_segment`, `alloc_core::node::Node` | NOT a leaf w.r.t. the extracted set: depends on alloc_core::fatal, alloc_core::os::{SEGMENT, SEGMENTS_*_TOTAL, release_segment} and alloc_core::node::Node (script output section 1). Joining the extracted crate would drag node.rs/os.rs residue with it; defer to the owner decision on whether numa.rs joins sefer-platform together with the full os split |
| `src/alloc_core/platform/sidecar.rs` | STAY | none (leaf) | generic primitive, but consumed by os.rs residue + large_cache_extended; deps segment-adjacent (SegmentDirectory layout fixups at call sites); not clearly platform-only |
| `src/alloc_core/platform/sidecar_stats.rs` | STAY | none (leaf) | leaf counters over aligned-vmem, but they are the acceptance oracle for sidecar.rs (which stays) and feed AllocStats/dbg hooks in root |
| `src/alloc_core/platform/size_classes.rs` | STAY | `alloc_core::os::SEGMENT`, `alloc_core::live_tag::LiveTag` | thin shim over the size-classes crate, but Params::huge_threshold = alloc_core::os::SEGMENT — a policy coupling into os; not leaf-w.r.t. platform |
| `src/global/process_exit.rs` | EXTRACT | none (leaf) | zero crate:: deps (no ghost bridge needed even though P1.1 is not landed); FFI + one atomic; consumed via global::mod re-export |

Decision counts: EXTRACT 2, SPLIT 1, STAY 5 (of 8 candidate files).

### (c) cfg gates of moved symbols (identical at origin/main and in sefer-platform: asserted)

| symbol | new file:line | gate |
|---|---|---|
| `MAX_REALISTIC_PAGE_SIZE` | `src/os.rs:120` | `#[cfg_attr( not(any( feature = "alloc-decommit", feature = "small-segment-lazy-commit", feature = "primordial-lazy-commit", feature = "large-reserved-capacity", feature = "internals" )), allow(dead_code) )]` |
| `Segment::reserve_biased` | `src/os.rs:181` | `#[cfg(not(feature = "numa-aware"))]` |
| `Segment::reserve_exact` | `src/os.rs:287` | `#[cfg(all( feature = "exact-span-large", not(feature = "numa-aware"), not(feature = "large-reserved-capacity") ))]` |
| `Segment::reserve_capacity_exact` | `src/os.rs:337` | `#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]` |
| `Segment::reserve_lazy` | `src/os.rs:398` | `#[cfg(all(feature = "primordial-lazy-commit", not(feature = "numa-aware")))]` |
| `Segment::reserve_small_lazy` | `src/os.rs:450` | `#[cfg(all(feature = "small-segment-lazy-commit", not(feature = "numa-aware")))]` |
| `Segment::reserve_lazy_for_measurement` | `src/os.rs:485` | `#[cfg(all( feature = "bench-internals", feature = "internals", feature = "alloc-decommit" ))]` |
| `Segment::advise_huge_pages` | `src/os.rs:526` | `#[cfg(not(feature = "numa-aware"))]` |
| `Segment::try_disable_transparent_huge_pages` | `src/os.rs:532` | `#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]` |
| `Segment::try_advise_huge_pages_range` | `src/os.rs:543` | `#[cfg_attr( any(not(feature = "alloc-decommit"), feature = "numa-aware"), allow(dead_code) )] #[cfg(not(feature = "numa-aware"))]` |
| `try_release_segment` | `src/os.rs:598` | `#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]` |
| `segments_reserved_total` | `src/os.rs:619` | `#[cfg_attr( not(any(feature = "alloc-global", feature = "internals")), allow(dead_code) )]` |
| `segments_released_total` | `src/os.rs:631` | `#[cfg_attr( not(any(feature = "alloc-global", feature = "internals")), allow(dead_code) )]` |
| `decommit_pages` | `src/os.rs:651` | `#[cfg(feature = "alloc-decommit")]` |
| `commit_pages` | `src/os.rs:719` | `#[cfg(any( feature = "primordial-lazy-commit", feature = "small-segment-lazy-commit", feature = "large-reserved-capacity" ))]` |
| `commit_pages_for_measurement` | `src/os.rs:749` | `#[cfg(all( feature = "bench-internals", feature = "internals", feature = "alloc-decommit" ))]` |
| `recommit_pages` | `src/os.rs:787` | `#[cfg(feature = "alloc-decommit")] #[cfg_attr(feature = "small-segment-lazy-commit", allow(dead_code))]` |
| `reserve_depot_overflow` | `src/os.rs:811` | `#[cfg(all( feature = "alloc-global", feature = "fastbin", feature = "alloc-decommit" ))]` |
| `commit_depot_overflow` | `src/os.rs:832` | `#[cfg(all( feature = "alloc-global", feature = "fastbin", feature = "alloc-decommit" ))]` |
| `decommit_depot_overflow` | `src/os.rs:852` | `#[cfg(all( feature = "alloc-global", feature = "fastbin", feature = "alloc-decommit" ))]` |
| `UNRESOLVED` | `src/process_exit.rs:24` | `#[cfg(windows)]` |
| `ABSENT` | `src/process_exit.rs:26` | `#[cfg(windows)]` |
| `QUERY` | `src/process_exit.rs:33` | `#[cfg(windows)]` |
| `resolve` | `src/process_exit.rs:38` | `#[cfg(windows)]` |

Gated symbols: 24 of 40 scanned items.

### (d) `#[inline]` / `#[inline(always)]` / `#[cold]` on moved symbols

| symbol | origin/main | sefer-platform | change |
|---|---|---|---|
| `segment_base_of` | — | `#[inline]` | + #[inline] |
| `segment_base_of_ptr` | `#[inline(always)]` | `#[inline(always)]` | unchanged |
| `fatal` | `#[cold]` `#[inline(never)]` | `#[cold]` `#[inline(never)]` | unchanged |
| `resolve` | `#[cold]` `#[inline(never)]` | `#[cold]` `#[inline(never)]` | unchanged |
| `process_exit_in_progress` | `#[inline]` | `#[inline]` | unchanged |

Changes relative to origin/main: 1 (`segment_base_of`: added `#[inline]`).

### (e) Feature forwarding: root `sefer-alloc` -> `sefer-platform`

| feature | root forwards | crate-side enables |
|---|---|---|
| `alloc-decommit` | `sefer-platform/alloc-decommit` | — (empty toggle) |
| `numa-aware` | `sefer-platform/numa-aware` | — (empty toggle) |
| `exact-span-large` | `sefer-platform/exact-span-large` | — (empty toggle) |
| `large-reserved-capacity` | `sefer-platform/large-reserved-capacity` | `aligned-vmem/lazy-commit` |
| `primordial-lazy-commit` | `sefer-platform/primordial-lazy-commit` | `aligned-vmem/lazy-commit` |
| `small-segment-lazy-commit` | `sefer-platform/small-segment-lazy-commit` | `aligned-vmem/lazy-commit` |
| `bench-internals` | `sefer-platform?/bench-internals` | `aligned-vmem/bench-internals`, `aligned-vmem/lazy-commit` |
| `internals` | `sefer-platform?/internals` | — (empty toggle) |
| `alloc-global` | `sefer-platform/alloc-global` | — (empty toggle) |
| `fastbin` | `sefer-platform/fastbin` | — (empty toggle) |

Crate features: 10; all forwarded by the root. Optional dependency: root `alloc-core` -> `dep:sefer-platform`.

<!-- END generated -->

## What moved

Per file: decision, reason and remaining root dependencies — table (b2)
above. Moved: `fatal.rs` whole, `process_exit.rs` whole, and the "backup" half
of `os.rs` (`Segment`, `SEGMENT`/`PAGE`, the `SEGMENTS_*` counters, release /
commit / decommit / recommit wrappers, depot-overflow storage). The root keeps
thin conduits (`alloc_core::os`, `alloc_core::fatal`, `global::process_exit`)
that re-export the crate's surface, so call-site paths are unchanged.

## What stayed in the root, and why

- `node.rs` — `continuation_offset` reads `SEGMENT`, the fatal-path functions
  read `config::continuation_bounds` (`ContinuationBounds`, P1.3 not landed);
  the rest is the raw-pointer free-list metadata membrane whose widening to
  `pub` is forbidden by the plan visibility rule.
- `numa.rs` — depends on `os`, `node` and `fatal`; moving it would drag
  `node.rs` along.
- `sidecar.rs`, `sidecar_stats.rs`, `size_classes.rs` — reasons are in table
  (b2) (sidecar consumers in root, acceptance oracle, `Params::huge_threshold`
  coupling to `SEGMENT`).
- Three directory functions of `os.rs` (`reserve_directory_sidecar`,
  `read_directory_class_words`, `read_directory_node_bucket`) — they depend on
  `SegmentDirectory`, the sidecar types and `segment_header`.

## Divergence from the plan

- The plan named `node` as a whole; it did **not** move.
- `numa`, `sidecar`, `size_classes` did not move.
- `process_exit` moved whole, **without** a ghost bridge: `crate::ghost` was
  never used by it.
- Conduits (re-exports in the old paths) replaced editing hundreds of
  call-sites.

## Widened to `pub`

See table (a). Every widened symbol was `pub(crate)` before. A safe `pub fn`
that takes a raw pointer and actually frees, commits or decommits memory would
be a hole of class R25-1 in a published crate, so the moved functions of that
kind are now `pub unsafe fn` with a `# Safety` contract, and the root keeps
real `pub(crate)` safe wrappers on the old path with the pre-extraction
signatures and cfg gates (root behaviour unchanged; call sites untouched; the
wrapper list is printed in table (b)). The only raw-pointer function left safe
is the pure address-arithmetic one (`segment_base_of_ptr`, never dereferences
its argument); the generator derives that from the source, asserts that no
other safe `pub fn` with a raw-pointer parameter remains, and asserts every
`pub unsafe fn` carries `# Safety`. Table (d) shows the only attribute
change: `segment_base_of` gained `#[inline]`; everything else is as it was.

## Features wiring

The root depends optionally on `sefer-platform` through `alloc-core`
(`dep:sefer-platform`) and forwards every crate feature (table (e)); the crate
features are empty toggles or `aligned-vmem` forwards.

## Gates

- ghost-zero-cost: expected identical `.text`.
- IAI without LTO: expected delta 0 against the P1.0 base; CI verifies.
- dbg-hook scanner: 9 new `REVIEWED_SURFACE` rows in
  `scripts/verify-dbg-hook-safety.mjs` for `os.rs` — safe `pub fn`s that
  became visible to the scanner.

## To be verified by CI

`sefer-platform-gates`, `CI: full`, ghost-zero-cost, IAI no-LTO, miri/loom
path pins, evidence-registry.

## Catalogue rules touched

- ghost-rules `cache/process-exit` (a `scopes` entry, not a `rules` entry):
  `files` updated to the crate. `verify-invariant-trailers.mjs` reads only
  `rules`, so the commit carries no `Invariant-Check` block for it (a block
  naming a scope id is rejected as an unknown rule).
- layering: baseline unchanged; claim-2 kept through the conduit.
- structure limits: baseline unchanged.
- decomposition digests: `changes` filter and the new `sefer-platform-gates`
  job.

## Open questions

- Link `sefer-ghost` after P1.1 merges (ghost is not used now).
- `node.rs` / `numa.rs` may leave the root after P1.3 / P2.0.
- `Cargo.lock` fixtures were updated.
- `classify-tests` / `license-check` stay red until the commit (dirty and
  untracked files).
- `scripts/p14_extraction_inventory.mjs` reads the working tree and its final
  assertion (`ghost-rules.json` must reference `src/global/process_exit.rs`)
  describes the pre-move state, so it fails on the migrated tree.

## Merge note: graph.mjs vs P1.1

P1.1 (sefer-ghost) also edits `scripts/module-graph/graph.mjs` and
`tests/module_graph.test.mjs`. The two changes solve different problems and
both are needed:

- P1.1 `externBinding`/`t.extern`: a *named* use-binding (`importMap`) whose
  first segment is an extern crate yields an external reference. It does not
  touch `node.globs`.
- P1.4: a *glob* `pub(crate) use sefer_platform::os::*;` is stored in
  `node.globs`; resolving its own segments re-enters the same node and the same
  glob with fresh state, so it recurses forever. P1.1 does not cover this, so
  the P1.4 guard (`globVisited` + shared `st` in `resolvePath`) stays.
- Conflicting hunks: `walkSegs` (binding branch from P1.1 is adjacent to the
  `for (const g of node.globs)` loop from P1.4), and `resolvePath` (state
  literal gains `globVisited`); `tests/module_graph.test.mjs` both append at
  the end of the file (keep both tests; P1.1 also edits `withGraph`).
- Optional hardening after the merge: skip a glob whose first segment is in
  `EXTERN_NAMES` in the glob loop, keeping the guard as the backstop.
