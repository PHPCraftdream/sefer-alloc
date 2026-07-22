# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Round 12 — directory-aliasing/NUMA correctness fixes, exact-span Large, class-aware dirty routing, virgin-zero skip (R12-1..R12-14)

Round 12 — 14 commits (`79f4136`..`3dc7bd9`, inclusive of both ends),
2026-07-22 — the follow-up queue against two independent external reviews of
the Round 11 wave (one correctness-focused, one speed-focused), synthesized
into a single prioritized queue and executed task by task with the same
zero-trust discipline as prior rounds: delegate implementation, personally
read every diff, personally re-run the tests (not trust the agent's own
"tests passed" claim), personally reproduce red-before/green-after
counterfactuals for every safety-relevant change, then commit. Two tasks
(R12-8, R12-13) reached honest NO-GO/deferred verdicts with zero code
changed — both cited prior institutional decisions (the 2026-07-10 G1
honest-reject for R12-8; R12-3's own measured numbers for R12-13) rather
than re-deriving from scratch, and both are recorded as complete, correct
outcomes of this round's methodology, not shortfalls. (R12-13's original
"superseded" wording was itself corrected to "deferred — no demonstrated
production victim" in Round 13, R13-4/task #274, after an independent
review noted that the features R12-13 cited are opt-in and not part of
`production`; see that entry below.)

**Production vs. opt-in — what actually changed for default `--features
production` users.** One feature joined `production` this round
(`primordial-lazy-commit`, R12-9, user-confirmed separately from the
measured GO per this project's "production feature-composition changes need
explicit sign-off" convention); one feature (`page-map-diag`, R12-11)
flipped `production`'s *default* by making previously-always-on bookkeeping
opt-in instead — a smaller, faster default carve path, with the diagnostic
capability preserved behind the new feature for anyone who needs it. Five
new opt-in, non-`production` experimental features were added
(`exact-span-large`, `large-reserved-capacity`, `class-aware-dirty`,
`virgin-zero-skip`, `page-map-diag`); `batch-api` gained a hard dependency
on `experimental`. Two P0 correctness fixes (R12-1, R12-2) landed directly
in the always-on directory-scan path with no feature gate — they fix
genuine bugs, not opt-in behavior.

**P0 — correctness fixes (unconditional, no feature gate):**

- **R12-1 (`79f4136`) — close a formal aliasing-UB window in the
  directory-driven segment scan.** `find_segment_with_free_impl`'s scan
  loop held a live `&'static SegmentDirectory` across a call
  (`validate_directory_candidate`) that can itself materialize a
  `&'static mut SegmentDirectory` on the same allocation via its self-heal
  path — `&T`/`&mut T` simultaneously live over one allocation, aliasing UB
  under Stacked/Tree Borrows regardless of the single-threaded owner
  discipline (which only rules out a data race, not the aliasing-model
  violation). Fixed by reading each directory word BY VALUE
  (`os::read_directory_class_words`, a raw-pointer `.read()` with no
  reference retained) instead of holding a long-lived reference across the
  mutating call. New regression test manufactures the exact
  mutation-during-scan interleaving; miri cannot reach the directory's
  above-threshold materialization path in practical time (documented
  pre-existing limitation), so the test is a behavioral-equivalence pin
  plus a guard against future regressions, not a red/green UB detector —
  documented honestly rather than overclaimed.
- **R12-2 (`89b6ce2`) — dense NUMA node-id → bucket mapping fixes locality
  on >8-node hosts.** The directory's `node_bucket` used the raw OS NUMA
  node id as a direct array index clamped at `MAX_NODES = 8`; `numa-shim`
  scans up to 64 real node ids, so every node id ≥ 8 silently fell into the
  shared "unknown" bucket regardless of how many distinct high-numbered
  nodes were actually in play — a thread on node 9 could be handed a
  node-10 segment ahead of its own node-9 segment, defeating R11-6's
  locality optimization on exactly the large machines it targets. Fixed
  with a dense `node_ids: [u32; MAX_NODES]` registration table (a node
  claims the next free bucket slot on first use) instead of raising
  `MAX_NODES` to 64 outright (rejected: ~7× sidecar memory tax paid by
  every heap for a rare case). A genuine regression in the fix's own
  test-only rebuild path (reset-vs-preserve the registration table) was
  caught during development and fixed, documented at length in
  `segment_directory.rs`.

**P0-perf — new opt-in experimental features (not in `production`):**

- **R12-3 (`2593d30`) — exact-span Large allocation.** Every Large request
  previously reserved a minimum of one whole 4 MiB `SEGMENT` regardless of
  actual size (a 260 KiB request paid for 4 MiB, ~15.8× amplification).
  `exact-span-large` sizes the physical reservation to
  `round_up(header + size, OS page)` instead — the stale comment claiming
  vmem required SEGMENT-multiple sizing did not hold up under inspection
  (both backends already support arbitrary `size != align`). Measured (own
  new `examples/r12_3_exact_span_measure.rs` harness, independently
  reproduced by the orchestrator): 260 KiB 15.78×→1.05×, 512 KiB
  8.00×→1.01×, 1 MiB 4.00×→1.00×, 1.75 MiB 2.29×→1.00×, 4 MiB
  2.00×→1.00×. Trade-off: OPT-G's in-place realloc-grow fast path loses
  most of its committed headroom (addressed by R12-4).
- **R12-4 (`fc155c9`) — reserved-capacity Large realloc, a Windows-effect
  follow-up to R12-3.** `large-reserved-capacity` reserves (but does not
  commit) a geometric 2× VA span up front, committing only the exact-span
  request; a growing realloc that fits within the reserved span commits
  just the missing tail (one `VirtualAlloc(MEM_COMMIT)`-class call) instead
  of falling back to alloc+copy+free. No new `unsafe`: reuses
  `aligned_vmem::reserve_aligned_lazy`/`commit_range` end to end (already
  used by the small-segment lazy-commit path). Measured: 2/4 growth-chain
  legs stay in-place (vs. 0/4 with exact-span-large alone), 33% fewer
  copied bytes, while commit-charge stays well below the pre-R12-3 4-MiB-
  rounded baseline.

**P1 — production-reachable fixes and new opt-in features:**

- **R12-5 (`7186f80`) — bound the cached `current_node()` staleness to a
  periodic refresh.** R11-5's NUMA-node cache was invalidated only at
  registry-slot claim/recycle boundaries; a long-lived, non-pinned thread
  the OS migrates mid-claim stayed pinned to the stale node for the rest of
  that claim — unbounded in wall-clock time. Fixed with a forced re-query
  every 128 cache hits (`NUMA_NODE_REFRESH_PERIOD`, chosen to match the
  order of magnitude of the directory's own sibling re-validation cadence),
  charged only to refill-miss/reservation call sites, never the bump-
  pointer hot path — measured ~2.5 ns added per cache hit, ~70–90× cheaper
  than the real syscall it occasionally replaces.
- **R12-6 (`4ea904f`) — finalize overflow-emptied segments beyond the
  64-base dedup cap.** `drain_heap_overflow`'s fixed 64-entry on-stack
  dedup buffer silently dropped pool/release finalization for the 65th+
  distinct segment emptied by cross-thread overflow-ring reclaims in a
  single drain pass (native `HEAP_OVERFLOW_CAP` is 2048, so this was
  reachable, if rare) — segments stayed correctly usable but sat outside
  pool-cap accounting at inflated RSS indefinitely. Fixed with a rare
  post-drain fallback sweep, gated on an `emptied_overflowed` flag so the
  common case pays nothing extra. New 66-distinct-segment regression test
  confirmed capped-at-64 behavior pre-fix, uncapped post-fix.
- **R12-7 (`f615703`) — class-aware dirty routing, wall-clock-gated then
  implemented.** `drain_dirty_segments` routed purely by segment, visiting
  every segment dirty for ANY class on a refill miss (R9-6 measured ~82%
  wasted visits at 4 concurrent producer classes, ~95% at 8, but deferred
  the wall-clock question). This round's own criterion bench confirmed the
  waste ratio is real (+134–171% ns/owner_alloc at N=1→N=4), then
  implemented `class-aware-dirty`: a lazily-materialized per-(segment,
  class) dirty-bit sidecar that lets a refill scan only the sought class's
  own word slice. Lost-wakeup safety by construction: a per-class bit is a
  visit HINT only, the drain body always fully drains the whole ring once a
  segment is visited, so a stale/redundant bit costs at most one wasted
  visit, never a silently-skipped entry — proved with a dedicated loom
  suite including a `#[should_panic]` counterfactual showing a genuinely
  partial drain design (the rejected alternative) *does* lose an entry
  under loom's interleaving search. Post-implementation re-measurement: ~19–
  23× reduction in ns/owner_alloc at N=8.
- **R12-8 — unify `AllocBitmap`+`MagazineBitmap` into a 2-bit
  `BlockStateMap`: NO-GO, no code changed.** Independent re-derivation
  reached the identical conclusion the codebase's own 2026-07-10 "G1
  honest-reject" record already established: the merge requires inverting
  load-bearing semantics at 15+ `AllocCore`-layer call sites that are
  deliberately magazine-blind (`carve_batch`'s leave-unset optimization,
  the freelist-drain legs, etc.), reopening a safety-critical double-free-
  detection boundary that was deliberately kept orthogonal by the
  already-shipped `MagazineBitmap` design (RAD-5, which got the ~50 Ir/op
  win *without* the invasive semantics change). No unexploited win remains
  of the size this task hoped for.
- **R12-9 (`a9ec36d`, `3b98ae4`) — split `alloc-lazy-commit` into
  `primordial-lazy-commit` (now in `production`) and
  `small-segment-lazy-commit` (stays opt-in).** The combined feature gave a
  ~5.1× smaller first-heap commit but was kept out of `production` because
  the full policy (lazy-committing every small segment) caused a 50–75×
  commit/decommit-syscall regression on decommit-heavy lifecycles (R8-10).
  Splitting isolates the two OS-reservation call sites: the primordial
  segment is reserved exactly once per process and is structurally excluded
  from the decommit lifecycle (`dec_live_and_maybe_decommit` hard-gates on
  `SegmentKind::Small`, which `Primordial` never satisfies), so it is safe
  from the R8-10 regression class by construction. User confirmed promoting
  `primordial-lazy-commit` to `production` after the measured ~5.14× win
  was independently reproduced.
- **R12-10 (`698cfca`) — virgin-carve zero-skip for Small `alloc_zeroed`
  (`virgin-zero-skip`, opt-in).** Implements a design verified twice
  (R9-5, R11-8, both CONDITIONAL GO): a genuinely first-touch bump-carved
  block on an OS-zero-guaranteed segment can skip `alloc_zeroed`'s explicit
  memset. A new owner-only `payload_virgin` bit tracks this per segment,
  withheld unconditionally under miri (matching the R9-1/#221 lesson that
  miri's `std::alloc` fallback does not zero-fill), defensively cleared on
  the one decommit-retain code path that could re-expose a
  decommitted-then-recommitted payload (currently dead in production, kept
  as a fail-safe). Personally verified non-vacuous: neutered the
  free-list-pop dispatch leg to wrongly claim virginity and confirmed 4 of 7
  tests failed with a genuine dirty-byte leak, then restored and reconfirmed
  green.

**P2 — smaller fixes, documentation, and re-evaluations:**

- **R12-11 (`5199148`) — gate `PageMap` maintenance behind a diagnostic-
  only feature.** The per-page class-tracking table was never load-bearing
  for production class routing (the class is always carried by the
  caller's `Layout` or the `RemoteFreeRing` entry) but was still maintained
  unconditionally on every carve/bootstrap/decommit-reset — until an
  inventory found it *is* a genuine test oracle for the §13 counterfactual
  regression gates, so it was feature-gated (`page-map-diag`) rather than
  deleted. Measured iai win on the carve/decommit-reset hot paths this
  closes (largest deltas: `multiseg_cold_256k` 490.1→329.2 Ir/op,
  `seg_cycle_decommit_256k` 339.7→286.1 Ir/op).
- **R12-12 (`a7db75a`) — `batch-api` marked honestly experimental**, per
  two consecutive external reviews (starting at R10): `#[doc(hidden)] pub
  unsafe fn alloc_batch`/`dealloc_batch` is still formally public Rust API
  for anyone who enables the feature. `batch-api` now requires
  `experimental` (nesting it under the crate's existing no-semver-
  guarantees umbrella); `#[doc(hidden)]` dropped from the `SeferAlloc`
  face in favor of a visible `# ⚠ EXPERIMENTAL / UNSTABLE` rustdoc section.
  No signature, behavior, or safety-contract change to any function.
- **R12-13 (`6d6e279`) — page-run layer design (R11-7): DEFERRED, NO-GO,
  no code changed.** R11-7 bundled two sub-problems: (a) per-object RSS
  waste and (b) `SegmentTable`-slot/syscall pressure at high live-object
  counts. R12-3's `exact-span-large` closes (a) almost completely
  (15.8×→~1.00–1.05× amplification) **when that opt-in feature is
  enabled**; (b) has no demonstrated victim anywhere in this codebase's
  tests/benches — and three of R11-7's four target size classes route
  through the cheaper Small-class path instead of Large only when the
  opt-in `medium-classes-wide` feature is enabled (`SMALL_MAX` = 1.75 MiB
  there). Neither feature is part of `production`, and `medium-classes-wide`
  was separately NO-GO'd for `production` over a large realloc regression,
  so `production`'s actual composition still routes 1.25–1.75 MiB objects
  through Large with whole-`SEGMENT` rounding today — this document's
  original "SUPERSEDED" wording read as though `production` itself had
  already closed the gap, which an independent review correctly flagged as
  premature; the wording was corrected to "DEFERRED — no demonstrated
  production victim" in Round 13 (R13-4, task #274,
  `docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md`, renamed from
  `..._SUPERSEDED.md`), with no change to the underlying technical
  analysis or numbers. The design doc is annotated with a pointer to the
  verdict, not deleted, in case a real `MAX_SEGMENTS`-bound workload is
  measured in the future.
- **R12-14 (`3dc7bd9`) — made the R12-1/R12-2 directory regression tests
  density-agnostic under `--all-features`.** Both tests were tuned against
  `production`'s `SMALL_MAX` (~253 KiB, ~16 blocks/segment) and silently
  broke under `medium-classes-wide`'s 1.75 MiB `SMALL_MAX` (exactly one
  block per segment) — not a directory bug, a hardcoded test-density
  assumption. Fixed by deriving allocation counts/classes from measured
  density and project constants instead of literals tuned for one feature
  combination.

### BREAKING CHANGE — `alloc-runfreelist` feature removed

The `alloc-runfreelist` experimental performance feature (PERF-3, the
run-encoded freelist / `RunStack`) has been **removed entirely** — the feature
flag, the source module, the cfg-gated branches in shared hot-path files, the
specialized test files, and the CI job that exercised the gated test bodies.
This is a semver-breaking feature removal, the same treatment the
abandon/adopt substrate got in round4.

**Why.** The feature reached a documented NO-GO verdict (Ф5 honest-reject):
it **regressed every one of the 11 iai benches**, including the four
cold/recycle targets it was designed to improve, by **+23 %–+31 % (Ir)**
instead of the predicted ≥5 % improvement. The wall-clock judge confirmed the
regression direction and magnitude (**+40 %/+43 %** on the 16 B/64 B cold
storm). See `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md` §Verdict for the full
measurement. The feature was never added to `production`, never recommended for
use, and was not under active development; retaining it as a "ready starting
point for a future re-run" was pure maintenance drag — every
small-segment-lifecycle change since had to keep accounting for a
known-losing implementation with its own metadata layout, hot/cold branches
in shared hot-path files, and hundreds of lines of specialized tests. See
`docs/agent_reviews_round5/code_quality_review.md` (finding #5) and
`docs/reviews/2026-07-13-round4-remediation-plan.md` (#97 / R4-5, never done).

**What was removed:**
- The `alloc-runfreelist = ["alloc-core"]` feature declaration (`Cargo.toml`).
- `src/alloc_core/run_stack.rs` (the `RunStack` type, `RunDesc`, `FOOTPRINT`,
  `RUNSTACK_CAPACITY`, and all six accessors `init_in_place`/`push`/`pop`/
  `peek`/`is_empty`/`clear_all`) and its `pub mod run_stack;` wiring in
  `src/alloc_core/mod.rs`.
- The `#[cfg(feature = "alloc-runfreelist")]` arms in `drain_freelist_batch`
  (`alloc_core_small.rs`), `flush_run` (`alloc_core_small_magazine.rs`),
  `decommit_empty_segment` (`alloc_core_small_pool.rs`), the bootstrap init
  (`bootstrap.rs`), the recycle init (`alloc_core_small.rs`), and
  `small_meta_end`/`run_stack_off` (`segment_header.rs`) — collapsed to just
  the shipped (classic linked-list) path.
- The tests `regression_r2_3_run_stack_class_guard.rs`,
  `regression_run_stack_decommit.rs`, `regression_run_stack_drain.rs`,
  `regression_run_stack_flush.rs`, `regression_run_stack_layout.rs`.
- The `cargo test --features "production alloc-runfreelist"` step in
  `scripts/check-all.mjs` and `.github/workflows/ci.yml` (`test-gated-bodies`).

**What was kept (NOT removed):** `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`
(the experiment's negative RESULT stays as institutional memory per this
project's honest-reject convention) and `docs/design/RUN_ENCODED_FREELIST_PLAN.md`
(the design plan that led to the experiment). The confined-`unsafe` count
dropped by 12 (6 in `run_stack.rs` + 3 in `alloc_core_small.rs` + 1 in
`alloc_core_small_magazine.rs` + 1 in `bootstrap.rs` + 1 in
`alloc_core_small_pool.rs`).

**Migration.** This feature was experimental and never recommended for use;
it was not part of `production` and had no non-test consumer. There is no
migration path because nothing depended on it. Any downstream `Cargo.toml`
listing `alloc-runfreelist` in its feature list will get a Cargo error
("unknown feature `alloc-runfreelist`") and should simply drop the feature.

### BREAKING CHANGE — `AllocCore`/`HeapCore::dbg_push_to_ring` narrowed to `unsafe fn`

`AllocCore::dbg_push_to_ring` and its `HeapCore` thin-delegation wrapper were
safe `#[doc(hidden)]` test hooks — the PRODUCER side of the cross-thread free
simulation — so fully-safe Rust could drive a deterministic stale-note→double-
issue chain under the `production` feature set (round5 `memory_safety_review`
R5-MS-4, HIGH): `alloc` a block, `dbg_push_to_ring` a "remote free" note for it
(no liveness/uniqueness check), `dealloc` it (own-thread free), `alloc`-re-issue
the same address (the hot path pops the freelist before draining the ring), then
`dbg_drain_all_rings` processes the STALE note — the re-issued block's bitmap
reads "allocated", the magazine predicate is always-false on a bare `AllocCore`,
and the generational guard is compiled out under `production`, so drain does
`write_next`/`mark_free` on the LIVE re-issue, yielding two live owners of one
range. No threads, no `unsafe` blocks, no type-system violation downstream — the
unsoundness was in the seam's contract, not any one caller's misuse (R5-F1 had
already fixed a `heap_xthread.rs` caller that misused this seam; this fix closes
the seam itself).

**Why.** The obligation the producer must uphold — "this push is at most one
logical remote free; the block is not freed/re-issued between the push and the
consuming drain" — is exactly the class of caller obligation Rust expresses via
`unsafe fn` + a `# Safety` doc, the same reasoning as R6-MS-1/2
(`dealloc`/`realloc`) and R6-MS-3 (`flush_class`). Under `production` the drain's
own guards are insufficient on their own, so the boundary moved from prose to the
compiler.

**What changed:** both `dbg_push_to_ring` entry points are now `pub unsafe fn`
with full `# Safety` docs and a tier-2 item-level `#[allow(unsafe_code)]` (the
`HeapCore` wrapper is `unsafe fn` too, so the chain is not left reachable
through it — mirroring R6-MS-1/2 making both `AllocCore` and `HeapCore`
`dealloc`/`realloc` unsafe). Every call site across `tests/`/`benches/` got an
`unsafe {}` block and a per-site `// SAFETY:` comment; the honoring callers
(single remote free) state the contract, the defensive callers (deliberate
contract-stress of the drain's `is_free`/magazine/generation guards) state which
guard recovers benignly. The drain side (`dbg_drain_all_rings` and the
`_checked`/`_impl` siblings) is LEFT safe — it is the consumer, and with the
producer now `unsafe fn` a contract-honoring caller can never produce a stale
note, so drain can never hit the chain; its reclaim guards remain defence-in-
depth. The `hardened`-only generational guard is NOT made unconditional — a
contract-honoring caller cannot hit the wrap-1/256 residual, so it stays a
probabilistic misuse backstop, not the primary soundness mechanism. New
`tests/regression_push_to_ring_unsafe_boundary.rs` proves the compile boundary
and the contract-honoring single-owner path. The two-tier confined-unsafe
inventory (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`) grew by
two item-level sites (54 → 56).

### BREAKING CHANGE — `AllocCore`/`HeapCore::{dealloc,realloc}` narrowed to `unsafe fn`

`AllocCore::dealloc`, `AllocCore::realloc`, `HeapCore::dealloc`, and
`HeapCore::realloc` were safe `pub fn`s accepting a caller-supplied raw
pointer and `Layout` with no way to verify the pointer is a live allocation
start, that the `Layout` matches the actual block, or that the block hasn't
already been freed — so fully safe Rust could trigger real memory-safety
bugs (round5 memory_safety_review R5-MS-1/MS-2, CRITICAL, the fifth time this
class of finding was raised, this time with concrete counterexamples):
resurrecting a freed block via `realloc`'s same-class in-place branch,
overlapping `copy_nonoverlapping` UB via a `realloc` racing a LIFO re-issue,
releasing a live `Large` segment via an interior-pointer `dealloc`, and
double-freeing a stale-after-reuse pointer.

**Why.** These preconditions (valid live allocation start, matching layout,
freed at most once) are exactly the class of caller obligation Rust expresses
via `unsafe fn` + a `# Safety` doc, not prose a safe caller can violate
without a compiler warning — the same reasoning as the prior raw-memory-hook
narrowing above, applied to the allocator's two most load-bearing entry
points.

**What changed:** all four methods are now `pub unsafe fn` with full
`# Safety` docs. The `#[global_allocator]` adapter (`SeferAlloc`'s
`GlobalAlloc` impl) is unaffected at the API level — `GlobalAlloc::dealloc`/
`realloc` were already `unsafe fn`; they now call the core methods inside
their existing unsafe context. Every internal call site across `src/`,
`tests/`, `benches/`, `fuzz/`, and `examples/` was updated with an `unsafe {}`
block and a per-site `// SAFETY:` comment. The two-tier confined-unsafe
inventory (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`,
CLAUDE.md/README/ARCHITECTURE/`src/lib.rs`) grew by these four new
item-level sites.

The `#[ignore]`d residual test `regression_xthread_double_free_no_corruption`
(which pinned a known cross-thread double-free residual as RED, tracked
under task #164/X7) was removed: its scenario is a genuine caller-side
double-free, which is now documented caller UB under the `unsafe fn`
contract rather than a soundness gap a safe caller could trigger. The
defence-in-depth regression coverage for the retained M2/X7 defensive
drain logic (which must still degrade a contract-violating double-free
benignly rather than corrupting memory) is preserved via the hardened
sibling test in the same file.

**Migration.** `AllocCore`/`HeapCore` are `#[doc(hidden)]` re-exports, never
stable public API. Any downstream call site now needs an `unsafe {}` block
around `dealloc`/`realloc`; the safety contract itself (valid live
allocation, matching layout, freed once) is unchanged — only its enforcement
moved from prose to the compiler. Code going through the public
`#[global_allocator]`/`GlobalAlloc` surface is unaffected.

### BREAKING CHANGE — registry control-plane fields narrowed to `pub(crate)`

`HeapSlot::state`, `HeapSlot::generation`, and `Registry`'s `slots`/`count`/
`free_slots`/`abandoned_segs` fields were `pub` (reachable through the
doc-hidden `pub mod registry` surface). Narrowed to `pub(crate)` to close
R4-MS-4 (CRITICAL) — a public field let safe downstream code force a
`LIVE → FREE` transition and re-push a slot onto `free_slots`, letting a
second thread's ordinary `claim()` steal a slot a first thread still had
cached, breaking the single-writer invariant `unsafe impl Sync for HeapSlot`
depends on.

**Why.** These fields were never intended as stable public API (every one
carries a "NOT stable public API" doc note and exists only because the
crate's `#[doc(hidden)] pub mod` test-only-export pattern requires the
enclosing module to be `pub`). The narrowing closes a real capability-boundary
gap; it does not change any documented, supported behavior.

**What was removed:** direct field access to the items above from outside the
crate. Replaced with narrow `#[doc(hidden)]` accessors on `Registry`
(`dbg_slot_state`, `dbg_slot_generation`, and one `unsafe fn
dbg_slot_preset_generation` for the one legitimate test that presets a
generation) for the tests that legitimately needed to observe this state.

**Migration.** No production code referenced these fields directly (they were
never part of the crate's supported public API). A downstream crate that was
relying on direct field access — an unsupported use of a `#[doc(hidden)]`
surface — will fail to compile (E0616) and should route through
`SeferAlloc`'s supported API instead; there is no supported use case this
narrowing removes.

### BREAKING CHANGE — public raw-memory test hooks narrowed to `unsafe fn`

Eight doc-hidden `pub fn` hooks (`RemoteFreeRing::{init,over}_test_buffer`,
`RunStack::{push,pop,peek,is_empty,init_in_place,clear_all}`,
`segment_header::{gen_at,bump_gen,init_gen_table_in_place}`,
`alloc_core_small.rs`'s `dbg_corrupt_freelist_head_next`/
`dbg_drain_freelist_batch`/`dbg_alloc_bitmap_bytes_for`/
`dbg_magazine_bitmap_bytes_for`/`dbg_payload_start_for`,
`alloc_core.rs`'s `dbg_unregister`/`dbg_recycle`, `numa::bind_segment`)
accepted a caller-supplied raw pointer/base with an unenforceable prose-only
safety contract — a safe downstream call with an invalid pointer could
trigger a library-side invalid read/write with zero `unsafe` at the call
site (R4-MS-3).

**Why.** The validity/size/alignment/lifetime/exclusivity of a caller-supplied
pointer is fundamentally unverifiable by the callee; that contract belongs in
the function signature (`unsafe fn` + `# Safety`), not in prose a caller can
ignore without a compiler warning.

**What changed:** each hook above is now `pub unsafe fn` with a `# Safety`
doc section. This introduced a second, item-level tier of confined `unsafe`
(alongside the existing 13 module-level seams) — see the source-of-truth
inventory command in `CLAUDE.md`/`README.md`/`docs/ARCHITECTURE.md`/
`src/lib.rs`, now `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`.

**Migration.** These are `#[doc(hidden)]` items, never stable public API. Any
downstream call site now needs an `unsafe { }` block; the safety contract
itself is unchanged (only its enforcement moved from prose to the compiler).

### BREAKING CHANGE — removal of the abandon/adopt substrate

The abandoned-segments / adoption substrate (the unreachable segment-transfer
protocol that predated Phase 12.5's whole-slot reuse) has been **removed
entirely**. This is a semver-breaking API removal. It mirrors the
`LargeCacheMode::{Background, Both}` removal precedent ("make invalid states
unrepresentable"); git history preserves the code if a future
decommit-when-empty policy ever needs to reintroduce segment transfer.

**Why.** The substrate was unreachable on every production path: whole-slot
reuse (Phase 12.5) recycles a slot's `HeapCore` whole on thread exit, so
`abandon_segments` / `try_adopt` were never called. It was also internally
inconsistent even on its own terms — `try_adopt` ignored the result of
`register_segment_internal` (silently proceeding even if registration failed),
`reset_stamp_cache` (documented as required on cross-heap segment transfer)
was never called, and its intrusive linked-list field (`SegmentHeader::next_abandoned`)
was shared with the LIVE `deferred_large` cross-thread-free stack (a separate,
production feature). Retaining it as a "loom-proven basis for a future policy"
was therefore an illusion: the documented future scenario already diverged
from the code's live invariants, and a naive reactivation would clobber the
`deferred_large` stack. See `docs/agent_reviews_round4/code_quality_review.md`
(finding #4) and `docs/reviews/2026-07-13-round4-remediation-plan.md` (#97 /
R4-5).

**What was removed:**
- `HeapRegistry::{abandon_segments, push_abandoned_segment,
  pop_abandoned_segment, try_adopt}` and the private helpers
  `push_abandoned_segment_into` / `abandon_one_segment`
  (`src/registry/heap_registry.rs`).
- `Registry::abandoned_segs` field and the abandoned-head packing helpers
  `pack_abandoned_head` / `unpack_abandoned_head` / `abandoned_head_is_empty` /
  `ABANDONED_HEAD_EMPTY` / `ABANDON_TAG_MASK` / `ABANDON_TAG_BITS` /
  `ABANDON_SEG_SHIFT` / `ABANDON_SEG_SIZE` (`src/registry/bootstrap.rs`).
- `OWNER_STATE_ABANDONED`, `unpack_owner_state`, `unpack_owner_gen`, and
  `OWNER_GEN_MASK` (`src/alloc_core/segment_header.rs`) — used only by the
  abandon/adopt CAS. (`owner_state`, `OWNER_STATE_LIVE`, `pack_owner`,
  `unpack_owner_id`, `OWNER_ID_NONE` are RETAINED: the LIVE owner-id
  resolution path for cross-thread free routing still uses them.)
- The dead adoption forwarders `register_segment_internal` /
  `set_small_current_internal` (`src/registry/heap_core.rs`) and
  `register_segment` / `set_small_current` (`src/alloc_core/alloc_core.rs`)
  — their sole caller was `try_adopt`.
- The tests `loom_abandoned_segs_aba.rs`,
  `regression_abandoned_stack_safe_api_uaf.rs`,
  `regression_abandon_a1_next_abandoned_field_sharing.rs`, and
  `loom_registry.rs` (entirely); the abandon-specific tests in
  `registry_basic.rs` and `regression_gen_table_lifecycle_seams.rs` (Seam 3);
  and the abandoned-head packing Kani proofs in `src/kani_proofs.rs`.

**What was kept (NOT removed):** `SegmentHeader::next_abandoned` (the field)
and `next_abandoned_atomic()` (the accessor), the `ABANDONED_TAIL` sentinel,
and the entire `src/alloc_core/deferred_large/` module — these are the LIVE
`deferred_large` cross-thread-free stack, a separate production feature that
reuses the same header field. Its tests (`loom_deferred_large`,
`regression_xthread_large_free_no_leak`) pass unchanged.

**Migration.** No production code referenced the removed items. Downstream
code that reached the `#[doc(hidden)] pub mod registry` surface and called
`HeapRegistry::abandon_segments` / `push_abandoned_segment` /
`pop_abandoned_segment` / `try_adopt` will fail to compile (E0425/E0061) and
should drop the call — whole-slot reuse (the only production teardown path)
makes segment abandonment unnecessary.

### BREAKING CHANGE — removal of `Default for AllocCore`

The `Default` impl on `AllocCore` (feature = `alloc-core`) has been **removed
entirely**. This is a semver-breaking API removal.

**Why.** `AllocCore::new()` / `AllocCore::new_with_config()` return
`Option<Self>` because the very first thing construction does is a real
multi-MiB OS memory reservation for the primordial segment, which can fail
under memory pressure / OOM / `rlimit`. The `Default` impl hid that
fallibility behind `.expect(...)`, i.e. a panic. Generic code across the
ecosystem treats `T::default()` / `T: Default` as a conventionally-cheap,
infallible operation (`Option::<T>::unwrap_or_default()`, `#[derive(Default)]`
on a containing struct, `mem::take`, collection `resize_with(Default::default)`,
etc.) — none of those call sites expect a multi-MiB syscall plus a latent
panic. The implementation had no internal callers (verified by grepping the
whole tree), so the impl was a footgun for hypothetical generic-bound users
rather than a load-bearing convenience. See
`docs/reviews/2026-07-12-round3-remediation-plan.md` (R3-C / N3).

**What was removed:**
- The `impl Default for AllocCore` block in `src/alloc_core/alloc_core.rs`
  (and its doc comment).

**Migration.** Replace any `AllocCore::default()` (or `T: Default`-driven
construction) with an explicit `AllocCore::new().expect("...")` or
`AllocCore::new_with_config(cfg).expect("...")` — making both the fallibility
and the panic visible *at the call site*, where they belong, rather than
hidden inside a trait impl elsewhere. If you want to preserve the exact old
message, use `AllocCore::new().expect("AllocCore::new: primordial segment
reservation failed (OOM)")`.

### BREAKING CHANGE — removal of `LargeCacheMode::{Background, Both}`

The `LargeCacheMode` enum (feature = `alloc-decommit`) has been reduced to
its single implemented variant, `Lazy`. The `Background` and `Both`
variants — placeholders for a background scavenger thread that was never
implemented — have been **removed entirely**. This is a semver-breaking
API removal.

**Why.** `Background` and `Both` had no implemented behaviour: they were
stored by the builder and silently degraded to `Lazy` at runtime. An
earlier fix (T5) made materialising a heap with either variant `panic!`
at resolution time, but that panic was reachable lazily through the
`GlobalAlloc::alloc` entry point (first-bind materialises the per-thread
heap), which conflicted with the crate's "never panics" guarantee on its
allocation entry points. Removing the variants outright ("make invalid
states unrepresentable") is safer than either the silent no-op or the
panic: there is no longer an unrepresentable promise to reject. See
`docs/reviews/2026-07-12-round3-remediation-plan.md` (решение №2).

**What was removed:**
- The `Background` and `Both` variants of `LargeCacheMode`.
- The resolution-time `panic!` match in `AllocCore::new_with_config`
  (T5's eager rejection) — nothing left to reject.
- The two `should_panic` regression tests
  (`background_mode_panics_at_materialisation`,
  `both_mode_panics_at_materialisation`) in `tests/large_cache_mode.rs`.

**Forward compatibility.** `LargeCacheMode` is now marked
`#[non_exhaustive]`. Reintroducing a variant alongside a real future
background-scavenger implementation will be a *non-breaking* addition,
not another breaking change. Code that constructs `LargeCacheMode::Lazy`
is unaffected; any code that referenced `Background`/`Both` will fail to
compile (E0599 — no such variant) and should drop the reference.

**Migration.** Remove any `.mode(LargeCacheMode::Background)` or
`.mode(LargeCacheMode::Both)` call — `Lazy` (the default, and the only
mode that ever had implemented behaviour) is what both were already
doing.

### BREAKING CHANGE — removal of the `Heap` / `with_heap` public face and the `alloc` feature

The explicit `Heap` type (`src/heap/heap.rs`), its TLS binding `with_heap` /
`with_heap_try` (`src/heap/tls.rs`), and the `alloc` Cargo feature that gated
them have been **removed entirely**. This is a semver-breaking API removal.

**Why.** `Heap` was a thin wrapper around `AllocCore` with no per-thread
magazine cache. The production `#[global_allocator]` face (`SeferAlloc`, backed
by the registry-resident `HeapCore`) already has the magazine fast path and
does not use `Heap` at all. A head-to-head benchmark
(`docs/HEAP_BENCH.md`, preserved as a historical record) showed `Heap` running
~9–12x slower than mimalloc on the steady-state alloc/dealloc hot path — the
gap that triggered the decision to remove `Heap` rather than invest in speeding
it up, since the magazine-backed `SeferAlloc` path supersedes it entirely.

**What was removed:**
- The `Heap` struct and its `impl` (`new`/`alloc`/`dealloc`/`realloc`/
  `alloc_zeroed`/`dealloc_any_thread`/`Drop`).
- The `with_heap` and `with_heap_try` TLS bindings and the
  `RefCell<Option<Heap>>` thread-local.
- The `alloc` Cargo feature (it gated only `Heap`/`with_heap`).
- The `src/heap/` module entirely (`heap.rs`, `tls.rs`, `thread_free.rs`,
  `mod.rs` — all existed solely for `Heap`).
- The `benches/heap_alloc.rs` bench and its `[[bench]]` entry.
- The `regression_with_heap_no_panic` test (tested the `with_heap` no-panic
  contract — coverage of `with_heap` is removed by design).
- The `regression_heap_xthread_large_free_no_leak` test (the `Heap`-face A1
  regression; the parallel `HeapCore`-face regression
  `regression_xthread_large_free_no_leak` remains and covers the same fix).
- The `heap_cross_thread` and `heap_miri_xthread` tests (exercised
  `Heap::dealloc_any_thread`; `HeapCore` does not expose a public cross-thread-
  free entry point, so these cannot be faithfully rewritten without inventing
  new public API. Cross-thread coverage lives on against `SeferAlloc`/
  `HeapCore` via `global_alloc_mt.rs`, `concurrent_stress.rs`, etc.).

**What was rewritten (coverage preserved):**
- `heap_cross_segment`, `heap_diag`, `heap_differential`, `heap_invariants`,
  `heap_soak`: rewrote from `Heap` to `AllocCore` directly (faithful 1:1
  substitution — under the single-thread `alloc` feature `Heap` was a pure
  pass-through to `AllocCore`). The two `with_heap` TLS tests in
  `heap_invariants` were removed (they tested `with_heap` specifically).
- `numa_alloc`: tests 1 and 3 already used `AllocCore` directly (unchanged);
  test 2 (`cross_node_handoff_safe`, which used `Heap::dealloc_any_thread`)
  was removed (cross-thread NUMA-handoff coverage lost — `HeapCore` does not
  expose `dealloc_any_thread`; see "coverage lost" below).
- `stamp_cache` test 3: rewrote from `Heap::dealloc_any_thread` end-to-end
  cross-thread free to a direct `dbg_owner_id_for` stamp readback (preserves
  the OPT-C stamp-cache coverage; loses the end-to-end cross-thread-free leg).
- `regression_xthread_large_free_layout_mismatch`: deleted only the `heap_face`
  submodule (the `HeapCore`-face tests remain).
- `regression_hardened_interior_ptr`: both tests already used `HeapCore`/
    `AllocCore` (not `Heap`); only a doc comment was updated.

**Coverage lost (cannot be faithfully rewritten without new public API):**
- `Heap::dealloc_any_thread` cross-thread free via the explicit-`Heap` face:
  `HeapCore` does not expose a public `dealloc_any_thread` equivalent (cross-
  thread routing lives inside the private `dealloc_routing`, reachable only
  via `SeferAlloc::dealloc`). The miri-targeted `heap_miri_xthread` and the
  `numa_alloc::cross_node_handoff_safe` tests exercised this path directly.
  Miri coverage of the substrate continues via `decommit_miri_cycle.rs`; cross-
  thread NUMA coverage is a decision point for a human (whether to expose a
  `HeapCore::dealloc_any_thread`-shaped public API or accept the loss).
- `with_heap` no-panic reentrancy contract: removed by design (the API is
  gone). The production `SeferAlloc` face has its own reentrancy-safe TLS
  binding (`global::tls_heap`) which is structurally reentrancy-free (raw
  `Cell<*mut HeapCore>`, no `RefCell` borrow state).

**Migration.** Users of `Heap`/`with_heap` should switch to `SeferAlloc`
(`#[global_allocator] static A: SeferAlloc = SeferAlloc;`) or, for direct
substrate access, `AllocCore` (`alloc-core` feature). There is no `Heap`-
shaped replacement with `dealloc_any_thread`; cross-thread free is reached via
the `SeferAlloc` global face.

**Feature rewiring.** `alloc-xthread` and `alloc-global` previously depended
on `alloc`; they now depend on `alloc-core` directly (the `alloc` feature's
only content was `Heap`/`with_heap`, so depending on it would be a no-op).
The `production` feature bundle (`alloc-global + alloc-xthread + alloc-decommit
+ fastbin`) is unchanged in effect.

### Security & compliance remediation (SEC-1 through SEC-6)

A `/fxx` security/compliance audit
([`docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md`](docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md),
research-only — no source touched) found the unsafe-confinement, dependency,
secrets, and MSRV claims all VERIFIED as advertised, and ten lower-severity
process/documentation gaps. SEC-1 through SEC-6 close six of them (three
MEDIUM, three LOW). No code defect was found or fixed — the pass hardens
disclosure, CI supply-chain posture, and the user-facing hardened-tier docs.

- **SEC-1 (`c3389de`, #198) — `SECURITY.md` shipped with a non-functional
  e-mail fallback.** The fallback section carried the literal placeholder
  `REPLACE_WITH_REAL_EMAIL` plus a `<!-- PLACEHOLDER: ... -->` banner, and no
  real maintainer address exists anywhere in the repo to source a genuine one
  from (`Cargo.toml` has no `authors`/email field). Rather than invent a
  plausible-looking placeholder, the e-mail fallback channel is **removed
  entirely** (−15 lines); private disclosure now relies solely on **GitHub
  Security Advisories**, which was already the preferred channel and remains
  fully functional.
- **SEC-2 (`94fc4f4`, #199) — `SECURITY.md` supported-versions table was
  stale.** It declared "`0.1.x` (current) — Yes" while the published crate is
  `0.3.0` — literally promising patches only for the `0.1.x` line. Reworded to
  "**Latest `0.x` release (see crates.io)**" so the table does not go stale
  again on the next patch/minor bump.
- **SEC-3 (`c81246f`, #200) — README's X7 residual disclosure was stale.**
  The README "documented residual" paragraph (≈line 701) still cited #164 as
  the pending fix and the `hardened` feature-matrix row (≈line 778) described
  only the H1 interior-pointer guard, with no mention of the X7 generational-
  ring arc that closed the re-issue-before-drain leg under `--features
  hardened`. (The X7 closure and its 1/256 wrap were fully documented
  internally — `DURABILITY.md`, this CHANGELOG, the X7 plan — but absent from
  the surface a security-conscious consumer evaluating `hardened` would
  actually read; audit finding §1.5.) Both passages now state the residual
  taxonomy correctly: two of three legs closed on plain `production` (X2/#164,
  R1), the third closed under `hardened` **except the 1/256 wrap**, which is
  named explicitly as the accepted probabilistic residual-of-the-residual.
  The plain-production residual disclosure is not weakened.
- **SEC-4 (`fd05274`, #201) — `permissions: contents: read` added to all
  three workflows.** `.github/workflows/{ci,release,perf-gate}.yml` previously
  ran with the repository-default `GITHUB_TOKEN` scope (legacy read/write on
  older repos). Traced every job/step in all three files: no job needs
  contents-write — `ci.yml` is checkout+cargo; `release.yml` publishes via the
  separate `CARGO_REGISTRY_TOKEN` secret, not `GITHUB_TOKEN`; `perf-gate.yml`
  caches/uploads via its own scoped backends. Workflow-level `contents: read`
  applied to all three; no job needed a broader override.
- **SEC-5 (`d70cd19`, #202) — new `deny.toml` + CI `deny` job
  (cargo-deny).** Closes audit gaps §1.3 (cargo-audit never run, tool absent
  locally) and §1.6#3/§2.2 (license compatibility manually assessed, not
  machine-checked). `cargo-deny` was chosen over `cargo-audit`-alone because
  it covers both RustSec advisories **and** license compatibility in one
  tool/one job. New `deny.toml` at the repo root: `[advisories]` with a
  narrow per-ID-documented ignore list; `[licenses]` allow-list built from
  cargo-deny's actual report against the current full-feature tree (MIT /
  Apache-2.0 / Zlib — narrower than the audit's manual §2.2 inventory,
  reconciled in the task report; no copyleft found either way); `[bans]`
  permissive (duplicate-version = warn); `[sources]` crates.io-only. At the
  time, two narrowly-scoped dev-only ignore entries: **RUSTSEC-2025-0141**
  (`bincode` 1.3.3 unmaintained; reaches this workspace ONLY through
  `iai-callgrind`, the Linux-only CI perf-gate bench — NOT in the published
  runtime tree) and **RUSTSEC-2026-0173** (`proc-macro-error2` 2.0.1
  unmaintained; same `iai-callgrind` dev-only chain). A third was added later
  this session — see the "CI fixes" subsection below.
- **SEC-6 (`91a6dac`, #203) — SHA-pinned `actions/checkout@v5` in
  `release.yml`.** Scoped to the token-bearing workflow per audit finding
  §1.6#2 (this is the only workflow carrying `CARGO_REGISTRY_TOKEN`, so
  tag-rewrite supply-chain risk matters most here). `actions/checkout@v5` →
  pinned to the exact commit SHA `v5` currently resolves to (verified via
  `git ls-remote`), with a trailing `# v5` comment for readability.
  `dtolnay/rust-toolchain@stable` was **deliberately left tag-pinned** — it is
  a moving branch by design (tracks the latest stable toolchain), and pinning
  it to a SHA would defeat its purpose; the conscious decision is recorded in
  the commit message.

### PERF-1 — README bench-doc sync (`650a3ed`, #205)

The README carried two disagreeing cold-direct tables: the dedicated "Cold
first-touch" section still showed P3-era numbers (16 B 1.60× slower, 64 B
1.15× slower, 256 B parity, 1024 B 1.84× faster), while the main dated
"Performance" table already had the correct post-X-arc re-measurement. A
full-file grep found **five** total occurrences of the stale ratios (the intro
bullet, the P0–P6 narrative, the "where we still trail" callout, the dedicated
Cold first-touch table + prose, and the Honest verdict bullets). All five were
synced to the post-X-arc measured ratios — **2.5× / 2.1× / 1.8× slower on
16 B / 64 B / 256 B cold-direct, 1.12× faster on 1024 B** (measured
2026-07-06 post-X-arc) — each explicitly labeled as post-X-arc vs preserved
P3-era history (the P3-era history is not erased; it carries a provenance
note). Docs-only; no source change.

### PERF-2 — TCACHE_CAP / FLUSH_N sweep: honest-reject (`e6f5112`, #206)

**REJECT (all three candidates).** A `/fxx` research hypothesis (#206 / PERF-2)
proposed that a larger per-class magazine (`TCACHE_CAP`, default 16) would
amortize refill/flush orchestration cost on storm-shaped alloc/free patterns
(the cold first-touch gap vs mimalloc). Tested `TCACHE_CAP = 32 / 64 / 128`
against the default `16` on **both** judges: the 11-bench iai
instruction-count gate and the wall-clock `global_alloc` criterion bench (the
exact 1024-op cold-storm shape the hypothesis targeted). Every candidate
**regressed every bench, including the explicit targets** (cold / recycle /
the `global_alloc` storm), and the regressions grew **monotonically and
super-linearly** with CAP. Pure experiment — **zero source changes survived**
(`git diff` to `src/` empty at the end; this doc is the only new file).
Recorded per the project's reject-with-numbers precedent so the next reader
does not re-run the same sweep blind. Full tables and mechanism in
[`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`](docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md).

- **CAP=32 reproduces X4-A** (the 2026-07-05 reject) within binary-layout
  noise: recycle +32,779 Ir (+18%), churn +22,863 (+28%), cold +25,763 (+21%),
  every other bench regressed too. Mechanism (X4-A's, re-confirmed): each
  refill/flush doubled in size (bigger carve/flush batches, larger `Tcache`
  zero-init at heap claim, longer M2 in-magazine scan); the benches don't
  refill-miss enough to amortize the larger batches.
- **CAP=64** is strictly worse on every bench (monotonic): recycle +88,949 Ir
  (+50%), churn +56,033 (+69%), cold +66,881 (+53%).
- **CAP=128 — super-linear regression; the decisive signal.** The `Tcache`
  struct footprint grows from **6.4 KiB → 50.2 KiB/thread** for `slots` alone
  (`49 × 128 × 8 B`) and **spills L1** — visible in the L2-hit column jumping
  ~160 → ~1000 on `small_churn_16b`: the magazine metadata itself stopped
  being L1-resident. Wall-clock confirmed on the exact storm shape: the sefer-
  vs-mimalloc gap on `global_alloc/16B` **WIDENED from 2.5× to 4.9×** at
  CAP=128 instead of narrowing (64B 2.3×→5.1×, 256B 1.46×→3.9×, 1024B
  0.90×→2.0×). The storm hypothesis's own arithmetic ("1024/16 = 64 refills →
  1024/128 = 8 refills, an 8× amortization win") is overwhelmed by the
  per-refill cost growth (8× larger carve batch + L1-spill). The companion
  predictions also failed against measurement: `churn_256b`/`small_churn_16b`
  were predicted CAP-insensitive but regressed monotonically (the first alloc
  of each iteration triggers a full refill — larger CAP = larger refill batch
  + larger `Tcache` zero-init); `large_alloc_free_cycle` regressed too
  despite doing NO small-block magazine work (pure `Tcache` zero-init at heap
  claim).

**Verdict.** mimalloc's advantage is **NOT a deeper magazine — it is a
structurally cheaper refill** (a `mmap`/page free list with no per-refill
orchestration equivalent), which a larger CAP cannot replicate and in fact
punishes. The CAP parameter is already at its optimum (16); CAP=64 and CAP=128
are the two never-before-measured values and are strictly worse. The shape that
could win is a **cheaper refill, not a rarer refill** — exactly the family
PERF-3 (below) then attempted on the recycle flush→drain path.

### PERF-3 — run-encoded freelist arc (Ф0–Ф5): IMPLEMENTED then honest-rejected

PERF-2 named "cheaper per-block work on the hot recycle path" as the winning
family of attack. PERF-3 was the concrete realization of that family for the
recycle flush→drain path: encode contiguous same-class runs as compact
`(start_off, count)` descriptors so the drain side reconstructs member
addresses by stride arithmetic (`start_off + i*block_size`) instead of pointer-
chasing `Node::read_next` per block. Design:
[`docs/design/RUN_ENCODED_FREELIST_PLAN.md`](docs/design/RUN_ENCODED_FREELIST_PLAN.md).
Verdict (Ф5):
[`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`](docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md).
Five phases, each committed between phases with a zero-trust review by @o46m
(GO each on Ф1–Ф4); the Ф5 measurement is the honest-reject.

- **Ф0 (`2732dfc`, #207) — design doc.** No src/ code; mirrors the X7 plan's
  structure (key insight → fixed decisions → phases Ф0–Ф6 → risks →
  readiness). Targets the "cheaper refill, not rarer refill" family PERF-2
  identified.
- **Ф1 (`5c5b6af`, #208) — `RunStack` storage + Layout.** New module
  `src/alloc_core/run_stack.rs` (`RunStack`, `RunDesc { start_off, count }`
  compact descriptors for contiguous-offset same-class runs,
  `RUNSTACK_CAPACITY = 8` per class, `Layout::run_stack_off` /
  `small_meta_end` shift to carve the RunStack region into segment metadata).
  New **`alloc-runfreelist`** Cargo feature (`= ["alloc-core"]`, **opt-in,
  NOT in `production`**). Storage only — no allocator behavior wired up yet.
  Production-judge 11/11 byte-identical (neutrality gate).
- **Ф2 (`7d5bada`, #209) — flush-side contiguous-run detection in `flush_run`.**
  Under the feature, `flush_run` collects accepted (guard-passing) freed
  offsets and detects contiguous sub-runs to divert into RunStack descriptors.
  **Empirical finding:** the magazine's LIFO refill returns blocks in
  **descending** address order, so a flush batch built from magazine order is
  descending, not ascending, and **in-place adjacency detection found ~0%
  contiguity** on the target `bench_direct_alloc` pattern. **Sorting
  (ascending) recovered 100% adjacency** on the target pattern — so the
  landed detector is sort-then-detect, not in-place. (This finding is load-
  bearing for the Ф5 mechanism analysis below.)
- **Ф3 (`f13ec4b`, #210) — drain-side stride reconstruction in
  `drain_freelist_batch` — "the heart."** Full `#[cfg(feature =
  "alloc-runfreelist")]` / `#[cfg(not(...))]` body split. Feature-on: drain
  RunStack for the class FIRST (pop descriptors, reconstruct member offsets by
  `start_off + i*block_size` instead of pointer-chasing `read_next`, guard
  `bm.is_free(off)` before `mark_alloc` + hand-out — fail-safe skip, never
  panics), THEN drain the classic linked list for remaining `out` capacity.
  The `is_free` guard is **mandatory defense-in-depth** (plan §2.3): the M2
  bitmap stays the ground truth, not the descriptor — a reconstructed offset
  that is somehow not free is skipped, never mis-linked.
- **Ф4 (`3e097be`, #211) — `decommit_empty_segment` clears RunStack +
  drain-overflow fix.** (a) On decommit, `RunStack::clear_all(base)` runs
  before `set_decommitted` — stale descriptors would otherwise reconstruct
  addresses into unmapped payload memory on a later drain (opposite policy
  from X7's gen table, which is deliberately NOT re-zeroed: RunStack
  descriptors are address hints into payload, so stale hints are unsafe, not
  merely stale). (b) Also fixed a narrow **descriptor-overflow-on-drain leak**
  found during Ф3's review: classes with `block_size > 8192 B` could have a
  descriptor larger than a single drain call's `out` capacity — fixed via a
  truncated-remainder pushback (`RunStack::push` of the un-drained tail when
  `out` fills mid-descriptor).
- **Ф5 (`154d1fa`, #212) — THE VERDICT: NO-GO / honest-reject.** Measurement-
  only phase (no source changes). Applied the pre-declared GO/NO-GO gate
  (design doc §3-Ф5) mechanically. The feature **REGRESSED every one of the 11
  iai benches**: the 4 cold/recycle targets (the feature's whole design goal)
  regressed **+23% to +31% Ir** (needed ≥5% **improvement** — `cold_16b`
  +23.04%, `cold_64b` +23.89%, `recycle_16b` +31.03%, `recycle_64b` +31.03%);
  the other 7 regressed **+0.75% to +4.33%** (6 of 7 breach the ≤+1% ceiling;
  only `realloc_grow` +0.75% sits inside it, because its hot path is the
  large-block realloc copy, not the small-block recycle path). Wall-clock
  confirmed on the exact storm shape: **+40.5%** on `global_alloc/16B`,
  **+42.5%** on `64B`, +43.2% on `256B`, +68.9% on `1024B` (criterion's own
  paired `change:` field p = 0.00 < 0.05 on every row). All three NO-GO
  triggers fire simultaneously — not a close call.

  **Root cause (confirmed by @o46m's independent code review):** the landed Ф2
  implementation **AUGMENTS** the classic per-block `write_next` chain-build
  rather than **diverting** from it — every accepted block still pays the full
  classic linked-list cost, and the sort/detect/push/rebuild machinery runs as
  an **ADDITIONAL** pass on top, not instead of it. The single `read_next`
  load the drain side saves per run-member block is dwarfed by this added
  flush-side cost. The **L1-hits column is the smoking gun**: for
  `recycle_alloc_free_256x16b`, ON L1 hits = 336,531 vs OFF 260,773 — a rise
  of **+75,758 L1 hits**, almost exactly matching the +55,593 rise in Ir (the
  new instructions are predominantly L1-resident memory ops: the offset
  array, the sort permutation, the RunStack slots). There is no level of the
  cache hierarchy where the feature wins (L2 flat ~174→~176, RAM flat-to-
  slightly-up 5,335→5,419). The "eliminate the dependent-load pointer chase"
  hypothesis is **refuted**: the pointer chase was already prefetcher-covered
  and cheap (the design doc's own §5 readiness note had flagged this as the
  failure mode). The design doc §1's honesty caveat — "this plan introduces a
  different representation, where hoist is *possible*" — was correct that the
  hoist is possible; the measurement shows it is not *profitable*.

  **Disposition: feature stays OFF / opt-in-only** (`alloc-runfreelist`, NOT
  in `production`; Ф6 is not triggered). Source is **KEPT, not reverted** —
  (1) **zero production cost**: the feature-off build is byte-identical to the
  pre-PERF-3 build (the neutrality gate, verified again by Ф5's baseline
  reproducing the 11-bench reference digit-for-digit); (2) the code is
  **correct, reviewed, and tested** — Ф1–Ф4 each passed @o46m zero-trust
  review, each has dedicated regression tests (`tests/regression_run_stack_*`),
  and the M2-double-free-through-run and decommit-clears-runstack safety cases
  are explicitly covered; (3) the loss is an **algorithmic-cost loss, not a
  correctness loss**, and the algorithm can be revisited — a future "PERF-3.5"
  reworking `flush_run` to genuinely **DIVERT** (skip `write_next` for detected
  run-members — write the descriptor instead of the chain link) rather than
  augment could in principle tip the trade; the storage (Ф1), drain-side
  reconstruction (Ф3), and lifecycle seams (Ф4) are reusable as-is, only Ф2's
  flush-side algorithm would need rework. (Precedent: PERF-2 left no source
  because it temp-edited a constant — nothing reusable to keep; PERF-3 landed
  four phases of real reviewed implementation, and the honest-reject is of the
  *measured outcome*, not the *code quality*.)

**Combined with PERF-2, this establishes:** sefer's remaining small-size gap
vs mimalloc is **not closeable by either a deeper magazine OR a cheaper-per-
block recycle representation of this shape** — the gap is structural in the
refill/flush orchestration itself (`find_segment_with_free` / latch /
carve-batch machinery), which is where a future PERF-4 should look.

### New dev scripts

- **`scripts/bench-table.mjs` — `npm run bench:table` (`73a6b2b`).** Runs the
  comparative wall-clock bench and **always prints the SAME canonical tables**
  (ns/op units, fixed bench set, vs-mimalloc ratio column). Written because
  ad-hoc benchmark tables varied in units/subset/format run to run — once
  causing a spurious apparent "20 ns → 40 ns regression" that was actually a
  µs-per-1024-op-batch vs ns-per-op unit mixup. The canonical table is now the
  single source of truth whenever comparative numbers are asked for.
- **`scripts/check-all.mjs` — `npm run check` (`29087c5`).** Single pre-push
  gate: `cargo fmt --check`, `clippy -D warnings` across all three CI feature-
  matrix entries (`""`, `--features experimental`, `--all-features`), `cargo
  test` under `production` and `production alloc-runfreelist`, then `npm run
  iai` (the deterministic judge). Fails fast at the first red step. Written
  after a push this session shipped 17 commits with a red CI (rustfmt drift
  accumulated across the PERF-3 phases, plus two ci.yml jobs pointing at a
  Cargo feature and test files deleted by task #204 — see the next section)
  that this command would have caught locally in under 5 minutes. It does NOT
  replace CI (CI additionally runs miri, loom, TSan, multi-arch, no_std, MSRV)
  but catches the most common drift class before a push, not after.

### CI fixes — found and fixed via a red CI run this session

A push mid-session went red on CI (Actions run 28846975468); the fixes below
landed in the same session. All are style/lint/drift — zero behavior change
(verified via judge byte-identical + full test suite green on each).

- **`d9767fe` — `cargo fmt --all` + clippy fixes across the CI feature
  matrix.** The PERF-3 arc (Ф1–Ф5) landed real code without a final
  `cargo fmt --all` + full clippy-matrix pass, so CI's fmt and clippy gates
  were red on push. `cargo fmt --all`: mechanical reformat (line-wrapping) in
  `alloc_core.rs`'s Ф2–Ф4 additions and the new `regression_run_stack_*.rs`
  test files. `clippy -D warnings` across all three CI matrix entries:
  `needless_return` (`return k;` → `k` in the `alloc-runfreelist` branch of
  `drain_freelist_batch`, tail position under `--all-features`),
  `manual_is_multiple_of` (`off % MIN_BLOCK as u32 == 0` → `.is_multiple_of(…)`
  in `remote_free_ring.rs`), `bool_assert_comparison` +
  `nonminimal_bool` (`assert_eq!(expr, true)` → `assert!(expr)` in
  `regression_gen_wrap_boundary.rs` / `regression_run_stack_layout.rs` — same
  assertions, same failure messages), `doc_lazy_continuation` (a blank `//!`
  line to split a markdown-list lazy continuation in
  `regression_gen_wrap_boundary.rs` /
  `regression_refill_window_double_issue.rs`), `assertions_on_constants`, and
  `iter_cloned_collect`. Purely style/lint; zero semantic change.
- **`ad1d533` — two CI workflow jobs referenced code deleted by task #204
  (Heap removal).** The `loom (loom_thread_free)` matrix entry passed
  `--features "alloc"` (a Cargo feature that no longer exists — it only ever
  gated the removed `Heap` type; the test's synthetic `Node` model never
  actually depended on it — feature set changed to `""`). The `thread-
  sanitizer` job ran `--test heap_cross_thread` and
  `--test regression_heap_xthread_large_free_no_leak` — **both test files were
  deleted in task #204** (no faithful `HeapCore` substitute existed; see the
  Heap-removal section above). A drift the removal task's own CI runs hadn't
  caught until this session's push.
- **`e1ff1e9` — added RUSTSEC-2026-0204 (crossbeam-epoch) to `deny.toml`'s
  ignore list.** A **new** advisory, unrelated to any change in this session —
  discovered via the `cargo-deny` CI job (SEC-5) failing on push (Actions run
  28848487484). `crossbeam-epoch` 0.9.18's `fmt::Display` impl dereferences a
  raw pointer that can be a null `Shared`/`Atomic` sentinel (fixed upstream in
  ≥0.9.20). Unlike the two existing dev-only ignore entries (bincode,
  proc-macro-error2, both via `iai-callgrind`), this one is **NOT** purely a
  dev-dependency chain: `cargo tree -i crossbeam-epoch` shows both the
  dev-only `criterion → rayon → crossbeam-deque → crossbeam-epoch` path AND a
  direct optional dependency via `Cargo.toml`'s `experimental` feature
  (`dep:crossbeam-epoch`, backing `src/concurrent/hand.rs`'s epoch-reclaimed
  slot). Verified this crate's own code does **not** trigger the vulnerable
  path: grepped `src/` for any `fmt::Display`/`format!`/`{}`-style formatting
  of a `crossbeam_epoch::Shared`/`Atomic` value — none exists; `hand.rs` only
  dereferences these via `.as_ref()`/pointer-load APIs, not the affected
  Display path. The ignore is therefore **sound for current usage**, but
  flagged in the `deny.toml` comment that a future addition of Display/format
  logging on an epoch pointer would silently reintroduce the bug under this
  ignore — re-grep before trusting the note stays valid. A
  `cargo update -p crossbeam-epoch` bump (≥0.9.20) is the proper fix,
  deferred as a dependency-version decision per project convention.

### Round 6 tail — cleanup, module splits, OPT-P0 perf batch (R6-CQ-5..7, R6-OPT-A1..A6, R6-OPT-P0-1..4, R6-REGRESSION)

The tail of round 6 — 21 commits (`e73dbec`..`461fe8f`), 2026-07-14..16 —
applies the same judge-driven methodology as the PERF-2 / PERF-3 arcs above
to three new *axes* the existing benches did not measure: **OS commit
charge**, **cross-thread-free tail latency**, and **the SMALL_MAX
fragmentation cliff**. Each candidate source change was preceded by a
dedicated diagnostic judge (the A-series harnesses), and every change was
measured against the deterministic `npm run iai` instruction-count gate so
zero of these wins came at a throughput cost (confirmed by the
cross-version wall-clock report at the end of the round). Two genuine
regressions the P0 work introduced were found and fixed in-flight and are
documented below as such, not spun — this project's honest-reject
convention.

**R6-CQ-5 — remove future-only dead scaffolding (`e73dbec`).** The
abandon/adopt removal left three executable-but-unreachable scaffolds kept
under `#[allow(dead_code)]` "in case the substrate returns": `HeapCore::
reset_stamp_cache`, the full-reset `AllocCore::decommit_empty_segment`
variant (every production caller uses the `_for_release`/`_impl` pair), and
`HeapSlot::new_uninit` (plus the `HeapSlotRemote::new_uninit` it transitively
dead-coded). All three confirmed zero real callers via whole-tree grep and
deleted. The load-bearing finding: `HeapSlot::new_uninit` *deliberately
diverged* from the real bootstrap — it set `next_free = NEXT_FREE_TAIL
(u32::MAX)` while the real bootstrap relies on the OS-zeroed reservation
and lets `push_free_slot` write `next_free` lazily (RAD-1); the scaffold's
own doc called this an "intentional, observationally-equivalent divergence,"
but a future caller trusting it as documentation would get the wrong initial
state. The actual lazy-`next_free` invariant is already preserved in prose at
`bootstrap.rs:39-49`, so nothing was lost. (Investigated and *not* removed:
`HeapOverflow::new_uninit`, kept alive by `new_boxed_for_test`'s real callers.)

**R6-CQ-6 — purge stale abandon/adopt architecture text (`139d4eb`).**
Docs/comments still described the *removed* abandon/adopt lifecycle,
referencing functions that no longer exist (`abandon_segments` et al.). The
real teardown path is whole-slot reuse (`tls_heap.rs`), not abandon/adopt.
Fixed across `Cargo.toml` (description + `alloc-xthread` feature doc), the
field rename `SegmentHeader::next_abandoned` → `deferred_next` (the field is
actually the live `deferred_large` cross-thread-free stack's link, rippled
through 14 source files + the tests that name it), `HeapCore::id`'s doc, and
README/`ARCHITECTURE.md`/`src/global/sefer_alloc.rs`/`src/registry/mod.rs`.
New guard `tests/no_stale_doc_references.rs::
no_stale_abandon_adopt_substrate_references` bans the removed API's exact
identifiers (`try_adopt`, `abandon_segments`, `push_abandoned_segment`,
`pop_abandoned_segment`, `abandoned_segs`, `OWNER_STATE_ABANDONED`) outside
the two files that legitimately name them in past tense — scoped to exact
identifiers, not the bare word-stems, so it doesn't false-positive on the
live `AbandonGuard` name, the `ABANDONED_TAIL` sentinel, or unrelated prose.
Counterfactual-verified by injecting a forbidden token and watching the guard
fail with the exact injected line.

**R6-CQ-7a/b/c — split the three remaining monoliths (`13a1f86`, `49f3a29`,
`fd2c770`).** Continues round 4's already-precedented "split one type's
`impl { .. }` block across thematic sibling files" pattern, applied to the
last three monoliths (`alloc_core.rs`, `heap_core.rs`, `segment_header.rs` —
round4 R4-10 / round5 code_quality_review #7, both only partially remediated
until now). **Pure code movement, zero behaviour change:** `npm run iai`
shows `Instructions: "(No change)"` on all 12 perf-gate benches against the
persisted baseline after each of the three splits; the two-tier
confined-`unsafe` grep count stayed at 46 (moved `unsafe fn`s keep their
`#[allow(unsafe_code)]` + `# Safety` docs, just relocated). The two
highest-risk moves per split (`dealloc`/`realloc` for 7b, `magic_at`/
`BinTable::head` for 7c) were byte-diffed against the pre-move source. New
sibling files: `alloc_core_core_diag.rs` (391 lines, the non-small-subsystem
`dbg_*` cluster); `heap_core_{alloc,free,tcache,ownership}.rs` (heap_core.rs
1987 → 606 lines); `segment_header_{layout,views,meta_fields,gen_table}.rs`
(segment_header.rs 1759 → 1041 lines). A handful of `private fn` →
`pub(super)`/`pub(crate)` widenings were the only mechanical adjustments
needed to compile.

**R6-OPT-A1..A6 — six new diagnostic judges (the "Stage A" harnesses).**
The round's design rule was *measure before you change*: each P0 source
change was preceded by the dedicated harness that would honestly prove
whatever win it claimed, so a result that isn't visible on the right axis is
not claimed. All six are `harness = false` custom timing-loop binaries or
process-per-sample runners (Criterion's `b.iter()` model cannot express
"alloc N, hold live, batch-time the free with percentiles" nor read this
crate's `dbg_*` counters at precise checkpoints) — measurement-only,
no allocator source touched, confined-unsafe count unchanged at 46.

- **R6-OPT-A1 — Windows commit-charge probe (`6d1b7ce`).**
  `examples/first_alloc_process.rs` measured only Working Set (RSS), never
  Windows commit charge — so a real cost was invisible: on Windows `crates/
  vmem` commits the full exact size of the Registry + inline `HeapOverflow`
  in one `VirtualAlloc(MEM_COMMIT)` (~125 MiB predicted), demand-zero and
  therefore absent from RSS until pages are touched. Added `commit_kib()`
  (reads `PagefileUsage` from the *same* `GetProcessMemoryInfo` call already
  made for RSS; the field was already declared, just never surfaced).
  Measured finding: RSS Delta 1 heap = 120 KiB vs Commit Delta 1 heap =
  132,620 KiB (**129.51 MiB**) — a ~129.4 MiB gap completely invisible to
  the pre-existing RSS-only judge, closely matching the review's ~125 MiB
  prediction. This gap is the quantity R6-OPT-P0-2 is meant to shrink.
- **R6-OPT-A2 — persistent-thread fan-in throughput judge (`6fa6776`).**
  `benches/heap_fanin_persistent.rs` spawns producer threads *once* per cell
  and reuses them (the existing `heap_fanin_production.rs` re-spawned/joined
  per iteration, so thread lifecycle dominated). Matrix T × burst ×
  {active,slow,paused,exited} owner state; reports p50/p99/max per-op
  wall-clock + `DBG_RING_*` overflow/retry/exhausted-per-op. A real
  cross-cell state-leak bug (recycling a heap then re-claiming it inherited
  the prior cell's `RemoteFreeRing`/`HeapOverflow` state via whole-slot
  reuse) was found during the orchestrator's zero-trust re-run and fixed;
  a `verify_repeated_cell_consistency()` regression guard is wired into
  `main()` so the class can't silently return.
- **R6-OPT-A3 — SMALL_MAX cliff independent-alloc sweep (`b6bcaa2`).**
  `benches/medium_size_sweep.rs`: there is a sharp architectural cliff at
  258,752 B (`SMALL_MAX`, last small class) vs 262,144 B (one byte over):
  below it, many objects share one 4 MiB segment via the per-segment
  freelist; above it, *every* object gets its own dedicated 4 MiB span +
  one `SegmentTable` slot. No existing bench measured this. Confirmed the
  cliff directly: at n=64, **258,752 B reserves +4 segments (fragmentation
  0.9871) vs 262,144 B +64 segments (fragmentation 0.0625)** — a 16×
  segment-count ratio matching the ~15-usable-blocks-per-4-MiB-segment
  theory. The harness handles real allocator OOM at n=1024 for post-cliff
  sizes (~4 GiB VA exhaustion on this host) cleanly — the OOM-at-scale is
  itself evidence of the cliff's cost.
- **R6-OPT-A4 — deterministic multi-segment directory judge (`3686412`).**
  `benches/segment_directory_sweep.rs`: `find_segment_with_free_impl` is
  O(segments) on a free-list miss, but the only existing judge
  (`multiseg_cold_256k`) builds just 3 segments — deep in the flat region.
  Three prior optimization attempts (X5, T10, R5-R1) measured ~zero
  improvement against that judge not because the scan is secretly O(1), but
  because nobody ever measured it far enough into S. Confirmed the
  flat-then-rising curve: S=1/3/16 in the 36–140 ns range, S=64 climbs to
  652–3,749 ns, S=256 to 18,590–25,668 ns, S=1023 to **92,716–169,875 ns**
  (kill-gate ratio 3742× post/S=3 vs 1.13× S=3/S=1, so divergence is
  concentrated at high S and the existing small-S IAI judge stays neutral).
- **R6-OPT-A5 — dealloc-only unbound-thread judge (`8248cb0`).**
  `examples/dealloc_only_unbound_thread.rs` + `scripts/dealloc-only-bench.mjs`:
  a worker that only ever receives a pointer and frees it (never allocates
  itself) is a common pattern no existing bench measured. Pre-fix the
  commit-charge ratio (treatment free-only / control alloc-then-free) sat at
  **1.00×** at every cell — both bind identically via `current_heap()`
  regardless of which call triggered it, exactly the pre-fix convergence
  this harness exists to make P0-1's win visible against.
- **R6-OPT-A6 — real-installed-allocator paired A/B/B/A runner (`57bf118`).**
  `examples/paired_ab_{sefer,mimalloc,system}.rs` (three binaries each
  genuinely installing its *own* `#[global_allocator]` — `bench:table`'s
  direct-call comparison is honest but not the codegen shape of a real
  production binary where every allocation routes through one
  `#[global_allocator]`) + `scripts/paired-ab-runner.mjs` (`npm run
  paired-ab`). A/B/B/A ordering per block (not A/B/A/B) specifically cancels
  linear host-drift/thermal trends; default N=20 paired blocks (the
  threshold for resolving <20% claims, matching R5-R2's actual N). The
  mandated same-vs-same control (`--arms sefer,sefer`) was independently
  re-verified at N=12 (t=-0.018 vs crit=2.228, sign test 7/12) — cleanly
  "NOT statistically distinguishable from noise," proving the runner doesn't
  manufacture a false signal.

**R6-OPT-P0-1 — dealloc without binding a heap (`09fe56f`).** `SeferAlloc::
dealloc` unconditionally called `current_heap()`, which for a thread whose
TLS is null (a worker that only ever frees foreign pointers) *claimed a full
registry slot* (`HeapCore::new` → reserve/commit a 4 MiB primordial segment)
just to free one foreign pointer. Extracted `HeapCore::dealloc_foreign_slow`'s
heap-instance-independent routing body into `dealloc_foreign_routing(..,
our_head: Option<…>)`; a new `tls_heap::current_for_dealloc()` maps both
null and TORN states to a `ForeignNoBind` variant that never binds, never
touches the fallback lock, and routes via `dealloc_foreign_routing(.., None)`
(any pointer reaching `dealloc` on a bind-less thread is foreign by
construction). Deliberate, documented trade-off: a TORN thread freeing a
pointer that happens to be fallback-owned now pushes onto the fallback's own
ring instead of taking its direct free path — still correct (ring-push is
safe regardless of caller identity; the fallback drains its own ring
lazily), just marginally less optimal in this already-rare corner. Verified
via RED-counterfactual (reverting to old routing fails both new tests
`dealloc_only_no_bind.rs` / `dealloc_only_no_bind_torn.rs` for the right
reason).

**R6-OPT-P0-2 — chunk the Registry + lazy HeapOverflow sidecar (two rounds,
`e4b3e1d` + `8dc6fe8`).** The Registry held `slots: [HeapSlot; 4096]` inline
as ONE giant `reserve_aligned` reservation, paid in full by every process's
first heap claim — ~125 MiB under `production` (inline `HeapCore` magazine
+ decommit state + inline `HeapOverflow`), committed in one `VirtualAlloc`
call with no OS-level commit-only-touched-pages for a reservation of this
shape. **Round 1 (`e4b3e1d`):** split the slot array into 64 chunks of 64
slots (`RegistryChunk`, new `src/registry/registry_chunk.rs`), materialised
lazily per chunk via the same `CAS(null→SENTINEL)→reserve→publish(Release)
/spin(Acquire)` protocol the old whole-registry ensure used. Commit Delta 1
heap: **~129.5 MiB → ~5.98 MiB (~21.7×)**. **Round 2 (`8dc6fe8`):** the
remaining dominant cost was `HeapOverflow` — a `[AtomicUsize; 2048] +
[AtomicU32; 2048]` pair inline in *every* `HeapSlot` (24 KiB/slot), paid by
every claimed slot whether or not it ever overflows. Split into a small
always-inline "emergency" tier (`INLINE_CAP = 64` entries, 768 B/slot) plus
a lazily-materialised sidecar (`HeapOverflowSidecar`, M5-clean reserve in
the existing `os.rs` seam mirroring round 1's chunk pattern) covering the
remaining 1984 entries, null until first genuine overflow past the inline
tier. Commit Delta 1 heap: ~5.98 MiB → **~4.52 MiB**; combined round 1 + 2:
**~129.5 MiB → ~4.52 MiB (~28.6×)**. The wedge hazard (a producer winning
the tail-CAS for a sidecar index ≥ `INLINE_CAP` and *then* discovering OOM
would strand that index forever) is fixed by calling
`ensure_overflow_sidecar` *before* the tail CAS — on failure, push returns
false without advancing tail, identical externally to "ring full," which
every caller already treats as the documented-sound bounded leak.

**R6-OPT-P0-3a — exact medium size classes, feature-gated (`b98f082`).** Six
exact "medium" size classes (256 / 320 / 384 / 512 / 768 KiB, 1 MiB) added
to `SIZE_CLASS_TABLE` behind a new purely-opt-in **`medium-classes`** feature
(additive over `alloc-core`, **NOT** part of `production` or any default-on
bundle). Eliminates the ~16× segment-count/fragmentation cliff at the old
`SMALL_MAX` boundary for allocations in this range — they now share a 4 MiB
segment with ~4–15 same-class siblings instead of each claiming a dedicated
Large segment. Reuses the existing small-segment substrate verbatim (one
segment, one size class, `BinTable`/`PageMap`/bump-carve) — no new segment
kind, no page-run layer. This is the "cheap first experiment" (radical-optimization
review SS4 sub-task 3a); the larger page-run redesign (3b) is deferred pending
evidence this substrate reuse isn't sufficient. The R6-OPT-A3 judge confirms
the fix at the exact predicted boundary: **16.00× segments/reserved-bytes at
n=64 (258,752 B vs 262,144 B) collapses to 1.00×** with the feature on. Ten
pre-existing regression tests that hardcoded byte sizes (usually 512 KiB)
that silently flipped from "Large" to "Small" under `medium-classes` were
bumped to sizes that stay genuinely Large in every feature combination;
`SIZE2CLASS` went `const → static` (`large_const_arrays` lint once the table
grew ~16 → ~64 KiB), a storage-class fix not a behaviour change.

**R6-OPT-P0-4 — overflow-first remote-free retry inversion (`345fa9b`).**
Inverted the cross-thread-free fallback order in `HeapCore::
push_with_overflow_retry`: try the heap-level `HeapOverflow` second-chance
ring *immediately* on a full segment ring, before any spinning, instead of
exhausting the whole `RING_PUSH_RETRY_SPINS` (8,192) budget against the ring
first. Each failed counted push ticked two locked-RMW diagnostic counters,
so the old policy could burn up to 8,193 checks / 16,386 RMWs on a single
logical free before ever trying the already-provisioned overflow ring (8×
the capacity). New policy: (1) one counted `RemoteFreeRing::push`; (2) on
failure, an immediate `push_to_heap_overflow`; (3) only if *both* fail (rare
double-saturation), a bounded spin-retry against both tiers using new
*uncounted* variants so failed polls inside the loop no longer tax either
ring's diagnostic counters. Common case: **2 checks total instead of up to
8,193**. On the R6-OPT-A2 judge (T=32/63, saturated ring), p99 tail latency
dropped from **tens of ms to hundreds-to-low-thousands of ns (~10,000×)**,
overflow/op near zero. This commit is also the *source* of the two
regressions below — the retry-loop reshape it introduced had a pathological
busy-spin budget that the A-series judges (which own a draining owner) did
not exercise.

**R6-REGRESSION — bound `push_with_overflow_retry`'s wall-clock cost
(`ba34fd5`).** P0-4's bounded retry loop scaled its iteration budget from
`RING_PUSH_RETRY_SPINS` (8,192) to `RETRY_LOOP_ITERATIONS` (2,097,152 =
8,192 × 256) as a *flat, uninterrupted* `core::hint::spin_loop()` busy-spin.
Under sustained double-saturation (both the segment ring and the heap-level
overflow full) with a live-but-never-draining owner (the `owner=paused`
shape A2 models), nearly every push burned most/all of its 2,097,152-
iteration budget purely on CPU — `spin_loop()` is a CPU-level hint (e.g.
`PAUSE`), never an OS-level yield, so it gave the scheduler no chance to run
the stalled owner. Confirmed independently: A2's `--reduced`
T=32/burst=100,000/`owner=paused` cell **burned 4,420 CPU-seconds over ~4
minutes with zero output before being killed**. A first fix attempt (same
total budget reshaped into 8,192-iteration rounds with `yield_now()`
between rounds) did *not* resolve it — `yield_now()` is a scheduling hint
with no other runnable work to hand the CPU to when every contending thread
is itself spin-then-yield-looping (~9 CPU-seconds/wall-second at 32 threads/
16 cores). **Fix adopted:** cap to `RETRY_ROUND_MAX_ROUNDS = 8` rounds of
8,192 tight-spin polls each with a real `std::thread::sleep(200 µs)` OS-level
block between rounds (native only; miri keeps a single pure-spin round).
Round 1 stays a pure tight spin with no sleep before it, so the
moderately-contended actively-draining-owner workload task #136's
high-contention judge exercises resolves within round 1 and pays no new
latency. Only a push that survives 8 full rounds (a push that can genuinely
never succeed once the fixed 2,304 combined ring+overflow capacity is
exhausted with a permanently-stalled owner) concedes to the documented
bounded leak after ~1.4 ms of sleep instead of a multi-second CPU burn. New
`tests/regression_paused_owner_wallclock.rs`; RED-counterfactual (pre-fix
source) lands all 3 attempts at ~20–21 s, GREEN with the fix at 0.7–1.9 s.

**R6-REGRESSION-2 — progress-detected stop condition (`1da4497`).**
R6-REGRESSION fixed the CPU-burn near-livelock but, by cutting the retry
budget to a fixed 8 rounds, *reintroduced* the task #136 throughput
regression under host load — the #136 judge went flaky, `exhausted_delta` up
to 821 during load spikes. Root tension: a *paused* owner (never drains)
needs a FAST give-up, while a *live-but-CPU-starved* owner (IS draining,
slowly) needs PATIENCE. No fixed round/iteration budget can distinguish
them — tuning the number only moves the failure between the two judges.
**Fix:** the retry loop's stop condition is now **drain-progress detection,
not a round count.** Both tiers' drain cursors (advanced *only* by the
owner's drain) are snapshotted before round 1 and re-read after every
fully-failed probe round via two new read-only `pub(crate)` accessors
(`RemoteFreeRing::head_relaxed()`, `HeapOverflow::head_relaxed()` — cheap
Relaxed loads of monotonic owner-advanced cursors; no ring protocol, layout,
or Ordering touched; no new `unsafe`). If either cursor moved, the owner
drained something — the stall counter resets and the push keeps waiting.
Only after `RETRY_STALLED_ROUNDS_GIVE_UP = 128` *consecutive* zero-progress
rounds (~0.3–2 s of continuously observed zero drain progress) does it
concede; `RETRY_ROUND_SAFETY_CAP = 4096` total rounds hard-bounds the wait.
The load-bearing 200 µs between-round sleep is kept unchanged. Each
concession memoizes its `(segment base, ring head, overflow head)` snapshot
in a per-thread const-init TLS `Cell` so a sustained stall does not re-pay
the full patience per push; the memo is written *only* on concession, so any
judge run that satisfies `exhausted_delta == 0` never populates it. K
calibration (measured): K=4 → 6/10 judge failures even on an idle host;
K=32 → 10/10 calm but 3/5 under a 16-thread CPU hog; **K=128 → 10/10 calm +
8/8 under the hog, all `exhausted_delta = 0`**. RED-counterfactual #2
(emulated pre-R6-REGRESSION flat 2,097,152-iteration spin) → paused-owner
wallclock test fails all 3 at 15.2–15.7 s; restored → 95–210 ms calm, 7.9 s
under the hog. The `tests/remote_fanin.rs` harness-1/2.5 liveness fix (the
owner loop now runs until every producer has finished via an `Arc<AtomicBool>`
handshake, draining every 4096 allocs) closes the remaining flake — every
prior failing run's concessions occurred strictly *after* the owner's fixed
N×2-alloc loop had completed, i.e. the paused-owner shape where conceding is
the documented-correct outcome.

**R6-REVIEW residuals — N-way stall memo + doc accuracy (`f27d060`).**
Address the non-blocking findings from an independent `@fl` review of the
P0 wave; no behaviour change on any already-green path. **F2 (perf
robustness):** the fast-concede memo was single-entry — a paused owner of
2+ saturated segments with a producer whose frees interleave across them
(A,B,A,B,…) overwrote the memo every push with the other segment's tuple,
so the memo never matched and every push re-paid the full 128-round patience
(a linear-in-push-count cost the memo exists to bound). Replaced with a
per-thread 4-way cache (`STALL_CONCESSION_WAYS = 4`): const-init, `Copy`,
no allocation; lookup fast-concedes iff *any* slot matches; soundness
unchanged (written only on concession, so a zero-concession run never
populates it; the post-round progress check still resets to full patience
the moment either drain cursor advances). New
`tests/regression_paused_owner_multisegment.rs`: passes in ~0.7 s with the
4-way cache; RED-counterfactual (forced to 1 way) fails all 3 attempts at
**77–105 s** — the exact single-entry thrash F2 fixes. F3/F5/F1/F4 are doc
fixes: `DBG_RING_PUSH_RETRY_EXHAUSTED`'s doc rewritten to the actual control
flow; dead `RETRY_LOOP_ITERATIONS` constant + its references scrubbed;
`ARCHITECTURE.md`'s loom-model count corrected (13 → 16, the 3 missing
entries added); a self-contradicting comment in `registry_chunk.rs` rewritten.

**Cross-version wall-clock report (`461fe8f`).** New
[`docs/perf/R6_CROSS_VERSION_BENCH.md`](docs/perf/R6_CROSS_VERSION_BENCH.md)
+ a README "Cross-version comparison" subsection: a same-harness three-way
comparison of sefer-alloc across published **0.2.1** (tag `sefer-alloc-v0.2.1`),
the tree **immediately before the round-6 wave** (`57bf118`), and current
HEAD (`f27d060`) — full per-family tables with the vs-mimalloc-ratio
methodology (host-drift-normalised) and the 0.2.1 not-apples-to-apples
caveats. **Headline:** every *large* wall-clock win landed between 0.2.1 and
the pre-round-6 tree (OPT-G in-place realloc → ms-scale copy-and-free to
µs-scale; Э6 churn → 256 B/1024 B throughput), **NOT** in the round-6 wave
itself. **The round-6 wave (before-wave → now) is flat-to-slightly-better on
wall-clock throughput and regresses no family beyond host noise**, by design:
it targeted **OS commit charge (≈7.4× lower for the first heap — 33.3 MiB →
4.5 MiB on the `bench:table` harness)**, **cross-thread-free tail latency**,
and **the SMALL_MAX fragmentation cliff** — axes `bench:table` does not
measure (see the A-series judges above). Probable modest wins on the 4 MiB
large-alloc/free path (~30–35% faster, 78/85 ns → 53/56 ns) and the 1024 B
teardown/decommit diagnostic, both inside this host's noise band. The 0.2.1
column was produced by porting the current bench harness onto the release
tag, preserved as the local `bench/0.2.1` branch so 0.2.1 stays
re-measurable. (Note on the commit-charge figure: the A1/P0-2
`first_alloc_process.rs` probe measures a stricter "genuinely fresh single
process" baseline and reports the larger **~129.5 MiB → ~4.52 MiB (~28.6×)**
reduction above; the cross-version doc's 33.3 → 4.5 MiB figure is the same
axis measured in the `bench:table` harness context.)

### Round 7 — segment directory, lazy commit, crate extraction (r7-a0..a6, r7-b0..b6, crate-extraction P1-P10)

Round 7 — 54 commits (`c0c011f`..`c815927`), 2026-07-16..19 — three
workstreams run under the same judge-driven methodology as the Round 6 tail
above (a dedicated diagnostic harness precedes every source change; the
deterministic `npm run iai` instruction-count gate is the authoritative
judge; honest-reject is mandatory), plus a crate-extraction campaign that
grew the workspace from 4 to 11 companion crates, plus a deep-audit-driven
hardening batch. The headline shape mirrors Round 6's: one big **GO**
(Workstream A, the segment directory), two **documented NO-GOs** preserved
as institutional memory (the Workstream-B first-heap commit target as
originally built, and the `ring-mpsc` in-tree swap), and one headline that
was a NO-GO on first attempt but **salvaged later in the same round by a
different mechanism** (R7-B6 lazy-commits the primordial segment). Every
number below is from the cited perf report or commit message; nothing is
inferred.

**Workstream A — segment directory, r7-a0..a6 — verdict GO (`f7d3a1c`..`0eb4794`).**
Replaces the O(segments) linear scan in `find_segment_with_free_impl` (the
refill-miss path Round 6's R6-OPT-A4 judge had proved blows up to ~100 µs at
S=1023) with an owner-only per-class bitmap sidecar, materialised lazily at
≥32 segments. Built incrementally behind the new experimental **`alloc-segment-directory`**
feature (additive over `alloc-core`, **NOT** in `production`, off by default;
feature-OFF byte-identical at every phase):

- **r7-a0 (`f7d3a1c`) — baseline + observability.** Six process-wide
  `AtomicU64` counters (`directory_hits`, `directory_stale_hits`,
  `directory_fallback_scans`, `directory_words_examined`,
  `dirty_segments_drained`, `full_scan_slots_examined`) +
  `benches/directory_threshold_probe.rs` (the S=32..63 transition-zone probe).
  Baseline confirmed (class 48, holes=0%): S=16 ~219 ns, S=32 ~442 ns,
  S=64 ~1.1 µs, S=256 ~17 µs, S=1023 ~102 µs — the O(S) cliff, with
  per-slot cost ~14 ns at S≤63 (cache-hot) rising to ~100 ns at S=1023. The
  S=32 transition-zone data is what fixed the **materialisation threshold at
  32** (the scan is already ~442 ns / p99 ~1 µs there; a ~100 ns directory
  lookup is a clear net win from there up).
- **r7-a1 (`5b5532c`) — the sidecar.** `SegmentDirectory { class_nonempty:
  [[u64; MAX_SEGMENTS/64]; SMALL_CLASS_COUNT] }` — plain u64 words (owner-only
  single-writer), 6.1 KiB (49 classes) / 6.9 KiB (55 under `medium-classes`),
  reserved lazily via a new M5-clean `os.rs` membrane
  (`reserve_directory_sidecar` + deref helpers in the existing tier-1 seam),
  `None` on OOM (mechanism stays off, linear scan runs). Nothing queries it
  yet.
- **r7-a2 (`b2eb7a3`) — incremental bitmap maintenance.** Wires
  `publish_nonempty` / `publish_empty` / `clear_segment_directory` /
  `sync_directory_for_segment` into every BinTable-head-mutating path (pop,
  drain, dealloc, flush, recycle, pool/unpool) so the bitmap is exact by the
  time A3 queries it. Correctness oracle: a randomised 300/500-op workload
  asserts the incrementally-maintained bitmap EXACTLY equals a fresh
  `rebuild_from_table` at periodic checkpoints.
- **r7-a3 (`66d0ac3`) — directory-accelerated lookup (fallback retained).**
  A directory-hit path in front of the unchanged guarded linear scan. Every
  load-bearing side effect of the scan (the Variant-2 remote-ring drain, the
  pool/decommit hysteresis, `unpool_if_present`, the ring-head cache refresh)
  is preserved byte-for-byte; a directory miss falls through to the scan. The
  directory is an **accelerator, not yet authoritative** — the scan stays as
  the correctness oracle and OOM-degradation path. Deliberately disabled under
  `numa-aware` (the two-pass local/foreign preference would be silently
  dropped); the bitmap is still maintained there for a future node-aware query.
- **r7-a4 (`7cc3ccf`) — remote dirty routing.** Replaces "drain every
  candidate's ring" with a per-slot dirty bitmap (`[AtomicU64; 16]`, 128 B in
  `HeapSlotRemote`): a cross-thread producer `fetch_or`s a bit Release AFTER
  a successful publish; the owner `swap(0, Acquire)s` and drains only dirty
  segments. Lost-wakeup-safe (bit set after the ring Release; a producer
  arriving mid-drain re-sets it; slot reuse revalidated). The full linear scan
  (the fallback) still drains every ring unconditionally, so an un-bit-set
  publish is never a lost free, only a bounded deferral. No new `unsafe`.
- **r7-a5 (`6eb425a`) — correctness matrix + heavy tools.** A 64-case proptest
  (per CLAUDE.md) asserting incremental bitmap == fresh rebuild for every
  (class, slot); gap-fill deterministic tests (recycle+reuse different class,
  decommit/reset/recommit, 55-class medium rebuild); 3 loom models of the
  dirty bitmap; a strict-provenance miri target. loom + miri RUN on this host
  (loom 3/3 + 3/3, miri 8.3 s PASS); TSan/ASan are Linux-CI-only (deferred,
  noted honestly). **No correctness bug found in A1–A4 production code.**
- **r7-a6 (`0eb4794`) — GO/NO-GO verdict: GO.** Against the pre-registered
  gates (full table in
  [`docs/perf/R7_DIRECTORY_GONOGO.md`](docs/perf/R7_DIRECTORY_GONOGO.md)):
  refill-miss at holes=0% collapsed from **S=256 ~15–19 µs → ~170–244 ns
  (60–98×)** and **S=1023 ~92–95 µs → ~376–552 ns (166–254×)** on both mean and
  p99 — far past the 10× gate; remote dirty=0% **S=1023 103 µs → 800 ns
  (129×)**; ≤16 directory words examined per lookup by construction; S≤16
  identical code (not materialised below the threshold); memory 6.1 KiB sidecar
  + 128 B/slot dirty control. The one **CI-DEFERRED** gate is G8 (IAI
  instruction-count churn ≤1%, Valgrind is Linux-only); the wall-clock churn
  proxy showed no regression (largest adverse +11.6%, within the host's
  ±15–20% noise). Documented trade-off (not a gate failure — the gate measures
  dirty=0%): at high remote-dirty density (10–100%) the drain-first path costs
  more than the linear scan's lazy drain, though absolute times stay low
  (1–3 µs). The directory stays behind its opt-in feature — enabling by
  default and making the fallback non-authoritative are separate downstream
  decisions.

**Workstream B — incremental / lazy Windows commit, r7-b0..b5 — verdict NO-GO
on the primary criterion (`e5310a0`..`40fdcd3`).** A new experimental feature
**`alloc-lazy-commit`** (additive over `alloc-core`, **NOT** in `production`,
off by default; on Unix/miri it falls back to eager; `numa-aware` forces eager)
to reserve a new small segment's 4 MiB span `MEM_RESERVE`-only and commit just
`[0, meta_end + LAZY_FIRST_CHUNK)` up front, growing the commit frontier
incrementally as carves advance. Built in the same incremental phase style:

- **r7-b0 (`e5310a0`)** — vmem primitives only: `reserve_aligned_lazy(size,
  align, initial_commit)` and `commit_range(base, start, end) -> bool`
  (returns false on OOM, never panics), all in the designated `crates/vmem`
  `#![allow(unsafe_code)]` seam.
- **r7-b1 (`0c981d7`)** — the `committed_payload_end` frontier on
  `SegmentHeader` + the lazy `reserve_small_segment` arm; a temporary
  "commit-whole-remaining-payload" safety net keeps B1 non-faulting until B2.
  Deliberately keeps the **primordial** segment eager (it hosts the
  self-hosted registry accessed at arbitrary offsets during bootstrap).
- **r7-b2 (`e5cb929`)** — fallible incremental grow-on-carve: on a carve past
  the frontier, commit `[frontier, round_up(carve_end, GROW_CHUNK))` BEFORE
  advancing bump/handing out the pointer; `carve_batch` does ONE commit for the
  whole batch span (not per block); failure leaves everything unchanged. The
  eager path is a pure no-op (`frontier == SEGMENT`).
- **r7-b3 (`2c3dbea`)** — lazy-commit-aware decommit/recommit: pool-admission
  decommits only above the initial chunk and resets to a clean carve target;
  retain-decommit keeps the initial chunk committed so reuse is fault-free;
  reuse drops the full-payload recommit. Savings preserved across a segment's
  second life.
- **r7-b4 (`f5f84ac`)** — correctness matrix + the `dbg_arm_commit_fail_at(k)`
  fault-injection hook (fails exactly the k-th commit, 1-based, one-shot,
  self-disarming): 21 tests proving commit failure is fully recoverable
  (frontier/state unchanged after an injected failure, retry succeeds).
- **r7-b5 (`40fdcd3`) — GO/NO-GO verdict: NO-GO on the primary criterion (K1),
  honestly.** Full table in
  [`docs/perf/R7_INCREMENTAL_COMMIT.md`](docs/perf/R7_INCREMENTAL_COMMIT.md).
  The headline target — first-heap Windows commit **4.52 MiB → ≤0.9 MiB** — is
  **unreachable by `alloc-lazy-commit` as built**: the first-heap commit charge
  is entirely dominated by the primordial segment (4 MiB eager), and the very
  first `alloc` triggers `registry::ensure()` which materialises it; no
  `reserve_small_segment` runs on the first-heap path. So the lazy path — which
  applies only to *subsequent* small segments — measured **4,628 KiB (4.52 MiB),
  unchanged across all swept chunk sizes** (K1 FAIL). This is a design-plan
  mismatch (the plan's 0.9 MiB budget assumed the primordial would participate),
  reported as such, not a measurement failure. **All secondary criteria PASS:**
  first-alloc latency +6.2% (≤10%), dense cold within noise (≤3%), steady churn
  no measurable regression, commit-syscall count scales per-chunk not per-alloc
  (B2), commit failure fully recoverable (B4's 21 tests), Linux/miri eager
  fallback transparent. Documented trade-off: the cold-path `segment_decommit_cycle`
  bench regresses ~50–75× with the feature ON (incremental `VirtualAlloc` syscalls)
  — opt-in, off by default, does not touch steady state. Chunk size kept at 256
  KiB (all four swept sizes give identical first-heap commit and near-identical
  steady-state; no data-driven reason to change). `alloc-lazy-commit` stays
  opt-in/experimental; the stated future work to actually hit 0.9 MiB was
  "lazy-commit the primordial + the already-chunked registry." **R7-B6 did the
  first of those — see below.**

**R7-B6 — lazy-commit the primordial segment (the deferred salvage),
`8977e88`.** A separate, later commit that revisited Workstream B's headline
NO-GO and landed the win via a **different mechanism** — it does not retract
the B5 verdict, it closes the gap B5 identified. The SAFE "Option A"
(pre-computed footprint): `bootstrap::primordial()` now reserves the 4 MiB VA
but commits only `[0, primordial_meta_end() + LAZY_FIRST_CHUNK)` up front,
where `primordial_meta_end()` is the exact page-aligned end of EVERY region
bootstrap writes (header, page map, bin table, gen table/bitmaps, remote ring,
segment registry, hash table, free-list array + top) — so all bootstrap writes
land inside the committed prefix by construction (no per-write commit dance).
Later carves reuse the existing B2 grow-on-carve path. A compile-time assert
that `primordial_meta_end() + LAZY_FIRST_CHUNK <= SEGMENT` makes a future
metadata growth fail the build. **Measured first-heap commit Δ: ~4.52 MiB →
~0.887 MiB (~5.2×), inside the ≤0.9 MiB target.** Gated `alloc-lazy-commit
AND NOT numa-aware`; the eager path (feature off, or numa-aware) is
byte-identical. `production alloc-lazy-commit` boots 395/0 (no panic / fault /
access-violation — bootstrap does not fault under the feature); feature-off
356/0 (eager path byte-identical). To avoid any future confusion:
`docs/perf/R7_INCREMENTAL_COMMIT.md` carries a top banner documenting the B6
reversal and inline "superseded by R7-B6" annotations at the B5-era stale
claims, and `c815927` later swept the same annotations through the
cross-version doc — so the historical B5 numbers stay accurate for what B5
measured while never reading as present-tense fact.

**r7-a7 / final-run fixes — `42f8343`, `a834fca`, `49046ef`.** Three
"final-run" fixes (#170) landed as the workstreams closed. **`42f8343`
(r7-a7)** clears the segment-directory bits on the B3 lazy-commit
pool-admission path — B3 zeroed all BinTable heads on pool admission but did
NOT clear the directory, so `publish_nonempty` bits survived as stale
positives and desynced the incremental bitmap from a fresh rebuild (manifested
under `--all-features` as a `directory mismatch at class=54`); the
counterfactual reproduces the mismatch. **`a834fca` (test-only)** gates the
B1–B4 lazy-commit tests off the `numa-aware` eager fallback so they don't hit
the Windows-lazy branch under `--all-features` (where numa-aware is on and the
lazy path is deliberately eager). **`49046ef`** comma-joins the feature list in
`scripts/miri.mjs` so multi-feature entries survive Windows shell re-splitting
— the old space-separated value made 3+-feature entries hard-error and
2-feature entries degrade to a `0 passed` **vacuous green**
(`decommit_miri_cycle`, `regression_ring_drain_guard_miri` were silently
validating nothing under strict-provenance miri).

**Re-sweep r7-c1 — `TCACHE_CAP` {32, 64}, third rejection — `cf22c96`.** The
post-RAD-5 re-sweep the R7 plan mandated: RAD-5 (`MagazineBitmap`) removed the
O(count) in-magazine M2 duplicate scan that was the old rationale for why
larger caps were expensive — so the hypothesis was that the cost model had
changed enough to make a larger `TCACHE_CAP` viable. **Verdict: NO-GO for both
32 and 64; `TCACHE_CAP` stays at 16** — this is the **third** time this
parameter has been tested and rejected (X4-A 2026-07-05 → PERF-2 `e6f5112`
2026-07-07 → r7-c1, see PERF-2 above). RAD-5 did remove the scan cost, but the
deterministic IAI judge (Ir/op via WSL callgrind, the authoritative judge)
confirmed the dominant costs remain: churn Ir/op **+13.2 % (CAP=32) / +38.8 %
(CAP=64)** — hard-fails the ≤2 % churn gate — and first-heap commit **+8.8 %
(+408 KiB) / +26.4 % (+1.22 MiB)**, enlarging each of the 64 first-chunk slots
and eating the R6 first-heap-commit win (the plan's explicit NO-GO-even-if-
wall-clock-improves guard). `PerClass` grows 136 → 264 B at CAP=32, bootstrap
zero-init Ir +89 %, cache footprint ~2×. Cold-direct DID improve (−6.5 % /
−12.5 % Ir/op) but cannot outweigh churn + commit. The noisy wall-clock showed
a spurious ~40 % churn improvement at CAP=32 that the deterministic IAI
contradicts (+13 %) — documented as host noise, not trusted. Zero production
code changed (CAP swept then restored; `git diff src/` empty). Full tables in
[`docs/perf/R7_TCACHE_SWEEP.md`](docs/perf/R7_TCACHE_SWEEP.md).

**Re-sweep r7-c2 — small-segment pool-cap sweep → documented presets, default
unchanged — `ad443d9`.** Sweep of `pool_segments` {0, 1, 4, 8, 16}
(`pool_byte_cap` scaled to match) on the production feature set. The judge is
the deterministic decommit-call count (wall-clock is host-noisy; IAI N/A — the
pool cap is a runtime knob, not a compile-time instruction change). The default
**stays at `pool_segments=4` / `pool_byte_cap=16 MiB`** — it already eliminates
working-set-oscillation decommit churn at the most common small sizes (16 B/64
B: zero decommit calls); raising it costs 2–4× retained RSS for diminishing,
within-noise latency returns. The deliverable is **three documented tuning
presets** (recipes over the existing `SmallSegmentPoolConfig` API, not new
constructors): **low-rss** (`pool_segments(0)`/`pool_byte_cap(0)` — 0 MiB
retained, max decommit churn; containers/serverless/embedded), **balanced**
(the current default; kills 16 B/64 B oscillation churn), and **throughput**
(`pool_segments(16)`/`pool_byte_cap(64 MiB)` — kills churn up to 256 B, halves
1024 B churn; RAM-rich hosts with oscillating working sets). OOM-drain
correctness confirmed: the pool remains a reclaimable soft reserve at every
cap (the unbounded-recycle + 10 pool tests stay green). Zero production change.
Full tables in
[`docs/perf/R7_POOL_CAP_PRESETS.md`](docs/perf/R7_POOL_CAP_PRESETS.md).

**docs(r7) — benchmark results + cross-version report — `5511af0`, `b8d11f4`.**
**`5511af0`** lands
[`docs/perf/R7_BENCH_RESULTS.md`](docs/perf/R7_BENCH_RESULTS.md): the
directory win as a clean OFF-vs-ON table (refill-miss collapses O(S)→~O(1),
up to ~166–180× at S=1023, ~29–39× at S=256, parity at S≤3), plus the canonical
`npm run bench:table` 3-arm comparison (SeferAlloc vs mimalloc vs System) —
steady-state churn is SeferAlloc's strength (**1.08–10.15× faster than
mimalloc**, the advantage growing with block size — 10× at 1024 B; 5–8× faster
than System across the board); cold-direct at small sizes is the weak spot
(2–2.7× slower than mimalloc at 16–64 B, crossing over to faster at 1024 B);
`segment_decommit_cycle` 4.13× faster than mimalloc; `Vec_push` 1.36× faster;
teardown diagnostic intentionally slower. **`b8d11f4`** lands
[`docs/perf/R7_CROSS_VERSION_BENCH.md`](docs/perf/R7_CROSS_VERSION_BENCH.md)
+ a README "Cross-version comparison — 0.2.1 → 0.3.0 (post-round7)"
subsection: same-harness run of published **0.2.1** vs current 0.3.0
(`49046ef`). Headline (0.3.0 over 0.2.1): churn **+1.0–2.3×**, churn+write up
to **2.26×**, `segment_decommit_cycle` **~318×**, `working_set_cycle` up to
**4.03×**; no real regression (cold-direct/teardown deltas within ±15–20 %
host noise). Documents the two root-cause overhauls between 0.2.1 and 0.3.0:
the ~318× decommit-cycle win (Mechanism-2 small-segment hysteresis pool +
OPT-E large cache) and the ~128 MiB → ~6 MiB (~21.7×) Windows first-alloc
commit-charge cut (the R6-OPT-P0-2 chunked Registry). *(Note for future
readers: this `b8d11f4` report is the Round-7-era cross-version doc — distinct
from, and later superseded by, the more complete `docs/perf/R8_CROSS_VERSION_BENCH.md`
from a subsequent round.)*

**Crate-extraction campaign, P1–P10 (`99e3238`..`0ff8497`).** A focused
campaign extracting independently-testable crates out of the monolith — 7 new
workspace member crates + the `aligned-vmem 0.2` release + `malloc-bench-rs`
publish-prep, taking the workspace from 4 to 11 companion crates. Each new
crate is a single-file seam crate, `#![forbid(unsafe_code)]` or a single
documented `#![allow(unsafe_code)]` reason, with a real-type loom suite where
concurrency is involved (and `#[should_panic]` counterfactuals proving the
harness is non-vacuous).

- **P1 — `malloc-bench-rs` (`99e3238`).** `run_with`/`sweep_with` with an
  `on_thread_start(thread_index)` hook (fires per worker before the start
  barrier) so a caller can pin worker i to core i; `examples/malloc_macro.rs`
  re-plumbed as a thin driver over the crate, retiring its second copy of the
  larson/mstress workload (task-#28 drift liability). Publish-prep only
  (`--dry-run` clean; no version bump, no publish).
- **P2 — `aligned-vmem 0.2` (`4ec1516`).** One coherent 0.2 release (the
  version bump 0.1→0.2 was explicitly approved): real `page_size()` via
  `sysconf`/`GetSystemInfo` (correctness fix for macOS 16 KiB pages); fallible
  `try_*` API returning `Result<_, VmemError>`; `decommit_lazy` (Linux/macOS
  `MADV_FREE`); optional `huge-pages`; a `mock` feature (recording call log +
  fail-N-th fault injection); and `leak_zeroed_pages` folding the
  3×-repeated leaked-zeroed-sidecar pattern (registry_chunk, heap_overflow
  sidecar, directory sidecar) into one helper. Absorbing sefer's
  `COMMIT_FAIL_*` into the mock was deferred (sefer builds vmem without `mock`
  — see #186 below).
- **P3 — `racy-ptr-cell` (`63991cc`).** The
  `UNINIT(null) → INITIALIZING(sentinel) → READY(*mut T)` lazy CAS-published
  pointer cell, unifying 4 in-tree loom shadow models
  (`loom_bootstrap_cas`, `loom_chunk_cas`, `loom_fallback_init`,
  `loom_overflow_sidecar_cas` — deleted) onto ONE real-type suite. The crate
  aliases its atomics to loom under `--cfg loom`; ships the two non-vacuousness
  counterfactuals (Relaxed-publish causality violation; spin-on-READY
  livelock). Registry chunk cells swapped onto it (M5-critical: OOM-rollback /
  re-race / Release-publish preserved); a `cfg(loom)` shim keeps the const
  `REGISTRY` static compiling under the global `--cfg loom`.
- **P5 — `size-classes` (`121d657`).** The const size-class scheme extracted;
  `src/alloc_core/size_classes.rs` becomes a thin compat shim (numa.rs-over-
  numa-shim precedent) building sefer's one concrete instantiation. New
  const-generic `SizeClasses::build(Params{...})` so arbitrary parameterizations
  become property-testable; `HUGE_THRESHOLD` becomes a policy `Param`. Fixes
  the "every align≥512 silently falls to whole-segment" bug class via a provably-
  equivalent jump slow path.
- **P6 — `globalalloc-model` (`b420d39`).** The differential op-stream + M1–M4
  oracle harness, unified out of THREE drifted in-tree copies
  (`tests/alloc_core_differential.rs`, `tests/heap_differential.rs`,
  `fuzz/fuzz_targets/global_alloc_ops.rs` — now thin consumers each keeping
  only an adapter + its historical size Config + entry point). All 14 oracle
  assert sites now live only in the crate (net −399 lines). Two front-ends
  (proptest `Strategy`, `Arbitrary`) over ONE model power cargo test, the miri
  bounded run, and libFuzzer.
- **P7 — `tagged-index-stack` (`0ecfaa4`).** The ABA-tagged Treiber free-index
  stack that lived across `tagged_ptr.rs` + `heap_registry.rs` — extracted and
  `heap_registry` swapped onto it (xthread-critical, landed cleanly, no escape
  hatch). Preserves the two hard-won subtleties (H-2 drain-to-empty packs the
  RUNNING tag, never tag 0; RAD-1 `store_next` is the only link write and only
  during push). **`src/registry/tagged_ptr.rs` removed entirely**; the 680-line
  `tests/loom_free_slots_aba.rs` shadow model **deleted**, replaced by the
  crate's real-type loom suite which ships TWO `#[should_panic]` counterfactuals
  (untagged-head slot corruption; H-2 tag-reset stale-CAS) — both confirmed to
  panic, proving both the ABA tag and the H-2 fix load-bearing.
- **P4 — `ring-mpsc` (`4c20f0c`).** The Vyukov bounded-MPSC index-ring protocol
  (raw + owned tiers, drain-stop contract, `DirtyRouter`) captured additively
  with an 11-test real-type loom suite (7 properties + 4 `#[should_panic]`
  counterfactuals). **The in-tree swap of `RemoteFreeRing`/`HeapOverflow` onto
  the crate was NOT done** (sanctioned escape hatch) — and the later
  CRATE-P4-followup re-investigation confirmed that swap is a real NO-GO (see
  below).
- **P8 — `proc-memstat` (`4075490`).** `proc_memstat::snapshot() -> MemStat
  {rss, commit, peak_rss}` — one same-instant query so rss/commit are coherent.
  Refolds 6 copies of the `GetProcessMemoryInfo` FFI across 5 example files
  into one reader. (A later follow-up, `583cd8f`, fixed a hardcoded 4 KiB Linux
  page-size bug here — see hardening batch.)
- **P9 — `proc-probe` (`c3c2440`).** The RESULT-protocol emit lib
  (`emit`/`emit_u64`/`emit_i64`/`emit_f64`/`emit_ns` → `"RESULT key=value"`
  stdout) + the config-driven A/B/B/A paired runner. The 3 probe examples now
  emit via `proc_probe::emit_*` and read via `proc_probe::snapshot()`; the
  statistical core (paired t-test, sign test, the A/B/B/A block loop,
  same-vs-same control) is UNCHANGED.
- **P10 — deferred/skipped verdict (`0ff8497`).** Read-only file-or-drop
  research re-evaluating every candidate the first pass did NOT file, now that
  P1–P9 shipped. **Net: 0 file as crates.** `carved-mem` DROP (the `'static`
  atomic-view lifetime is load-bearing for `#![forbid(unsafe_code)]`; a general
  crate would ripple every `// SAFETY` into a generic caller obligation);
  `intrusive-once-stack` DROP (ring-mpsc P4 already banked the MPSC value; the
  unique idempotent-double-push guard is welded to raw-address-in-`AtomicU64` +
  lifecycle-link tricks that extraction loses); `iai-judge` + `criterion-arms`
  DROP as crates (their one worthwhile in-place win folds into proposed hygiene
  H2 — a bench-emitted MANIFEST). All 3 skip groups (gen-slot retired;
  tcache-magazine trivial; the bitmap/table/directory/large-cache/xthread-SM
  cluster as internal ABI or ~80 % convention) confirmed. Proposes 4 in-place
  hygiene sub-tasks (H1 single-source sanitizer matrix > H2 bench-emitted
  MANIFEST > H3 dead-`dbg_*`-hook detection > H4 fold `rss_probe.rs` onto
  proc-memstat), not filed.

**CRATE-P4-followup (#187) — `ring-mpsc` in-tree swap = verified NO-GO —
`d062798`.** The sanctioned P4 escape hatch was re-investigated (not merely
inherited) against source, and the swap of the two shipping cross-thread-free
rings onto `crates/ring-mpsc` is **NO-GO on BOTH tiers** — zero code changed.
Full rationale in
[`docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md`](docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md).
**Tier A (`remote_free_ring.rs`):** structural layout incompatibility — the
shipping ring uses `AtomicU32` cursors + an `overflow` side word +
`CURSOR_BLOCK = 128` (the PERF-PASS-4 / #52 cache-line-separation fix:
`head`@0 consumer-only, `tail`@64 producer) + a hardened
`[gen|class|off]` generation-stamped entry; `ring-mpsc`'s `RawStore` is a
fixed `usize`-cursor, no-side-word, adjacent layout. Swapping would break the
cache-line fix and every compile-time offset assert (wired through
`small_meta_end()` into 20+ call sites), or require a large risky `RawStore`
generalization. **Tier B (`heap_overflow.rs`):** the two-tier inline+sidecar
store straddles an inline array AND a lazily-mmap'd sidecar in one cursor pair
(`ring-mpsc` is single-region), AND the wedge-hazard sidecar-before-tail-CAS
ordering lives INSIDE `push`'s loop (which `MpscRing::push` owns opaquely) —
forcing it risks the permanent-wedge hazard the module doc warns is worse than
the bounded loss. **The swap is pure dedup (zero runtime benefit) over the most
safety-critical path in the codebase.** Consequence: **all 7 in-tree
ring/dirty loom models are KEPT** (the shipping code is unchanged, so its
coverage must stay — the #174 lesson); the crate's `loom_ring_mpsc` suite is
additive real-type coverage of the extracted protocol only.

**Crate-extraction review + follow-up fixes — `1d39e43`, `9d6c9f4`, `583cd8f`,
`3d25263`, `6ce2df5`, `0ff8497`'s hygiene.** **`1d39e43`** applies the `@fh`
phase-review findings (verdict SHIP-WITH-FIXES): F1 HIGH and **CI-breaking,
reproduced E0015** — under `RUSTFLAGS=--cfg loom` with `alloc-global`,
`Registry::new()` (const fn) called `TaggedIndexStack::new()` which is non-const
under loom, so the `static REGISTRY` wouldn't const-evaluate; fixed exactly as
P3 did for `RacyPtrCell` (a const-capable `loom_shim` stand-in used
`#[cfg(loom)]` only); plus F2/F3 medium (missing LICENSE files for size-classes
+ proc-probe; README loom row corrected) and low/nit comment/doc accuracy. The
F9 proc-memstat Linux hardcoded-4 KiB-page bug was filed (then fixed — see
next). **`583cd8f`** fixes that bug: the Linux aperture read `/proc/self/statm`
(page counts) and multiplied by a hardcoded `PAGE_SIZE=4096`, under-reporting
RSS/commit 4×/16× on 16 KiB / 64 KiB-page kernels (aarch64, ppc64); replaced
with a page-size-independent `/proc/self/status` read (kB-denominated). **`9d6c9f4`
(#186, CRATE-P2-followup)** absorbs sefer's `COMMIT_FAIL_*` into a NEW distinct
vmem opt-in feature `fault-injection` (the mock feature couldn't take it over
— it replaces the whole backend with a stub, but the R7-B4 tests drive a REAL
`AllocCore` through real reservation/carve/decommit); sefer's `os.rs` now
delegates to `aligned_vmem::fault_injection` and the R7-B4/B2 tests stay green
unmodified (non-vacuous — they arm the fault via the delegated hook). **`3d25263`
(HYGIENE #188)** repoints two stale TSan-runner test targets removed in
`dfc1a34` to existing successors, unbreaking `[tsan] production`. **`6ce2df5`**
drops a redundant closure in `examples/malloc_macro.rs` flagged by
`clippy --all-features` (a CRATE-P1 follow-on the crate-scoped clippy run
missed).

**Platform, CI, and hardening batch — the deep-audit follow-throughs.** A
cluster of independent fixes from the 10-agent deep audit + the audit's
safe-code-soundness follow-up, all individually verified with counterfactuals:

- **PLAT-1 (`65ae170`).** `Layout::small_meta_end()`/`primordial_meta_end()`
  rounded their decommit/recommit-boundary offsets to the fixed 4 KiB `PAGE`
  constant — on a 16 KiB-page (Apple Silicon) or 64 KiB-page (some Linux/aarch64)
  machine the boundary lands mid-real-page and `madvise`/`VirtualFree` silently
  round it, breaking the M6 RSS-reclaim promise with no red CI signal. Fix: a
  compile-time `MAX_REALISTIC_PAGE_SIZE = 64 KiB` superset bound (the literal
  audit suggestion — calling `page_size()` at runtime — does not compile, both
  are `const fn` used in true const contexts); plus a belt-and-suspenders test
  asserting both boundaries are multiples of the REAL runtime page size.
- **`regression_magic_at_atomic_load` SIGSEGV (`f165ced`).** Root-caused via
  gdb + empirical repro (40/40 crashes without `alloc-decommit`, 0/40 with):
  the test deliberately races a cross-thread stale/duplicate Large free; under
  `alloc-decommit` the pages stay mapped (safe), without it `dealloc` calls
  `os::release_segment` immediately and the remote thread's `magic_at()` read
  races an actual unmap. Not a production soundness bug (reading a released
  segment's header is fundamental caller UB for any allocator, already
  documented) — the fix narrows the test's cfg gate to `alloc-decommit`, where
  its setup degrades benignly; the R6-MS-5 atomic-load regression stays fully
  covered there.
- **safe-surface empirical M1/M3 test (`403e216`).** A new zero-`unsafe` test
  installing `SeferAlloc` as `#[global_allocator]` and churning
  `Box`/`Vec`/`Arc` across 6 threads × 1500 iters × 6 size classes, with every
  allocation tracked in a `[start,end)`-keyed live table checked against its
  address-order predecessor/successor (provably sufficient for overlap
  detection) and sentinel-stamped at both ends, re-verified mid-life and at
  Drop. **Empirically confirms the actual safe-code soundness boundary this
  project depends on** (`alloc` must never hand out a pointer aliasing a
  still-live allocation — M1/M3 in `INVARIANTS.md`): 9,000 allocations/run,
  246 full-table sentinel verify passes/run, **zero M1/M3 violations** across
  10/10 runs. The narrower-than-it-sounds framing matters: the #202 SIGSEGV was
  a deliberate double-free through `unsafe fn dealloc` — caller UB by contract,
  unreachable from safe code; this is the first empirical check of the real M1/M3
  boundary.
- **docs(soundness) (`7bca3cf`).** Formalises the UB-vs-soundness distinction
  for M2/M3 in `INVARIANTS.md` (citing #202 as the worked example) and lands
  the 10-agent deep-audit reports + `SUMMARY.md`. Closes the T0.5
  soundness-boundary chain: #202 (fix) → #212 (empirical stress test) → #213
  (this docs commit).
- **DEBT-1 (`d8cc157`).** Wires 6 of 13 `tests/loom_*.rs` files that were
  never CI `--test` steps (`loom_dirty_publish`, `loom_dirty_multi_segment`,
  `loom_heap_overflow`, `loom_heap_overflow_drain_guard`,
  `loom_overflow_first_retry`, `loom_remote_ring_drain_guard`) into the
  existing jobs whose feature strings already match — no new jobs. CI was the
  only automated net for the shipping `RemoteFreeRing`/`HeapOverflow`/dirty-
  segment cross-thread protocols, and it had a silent gap.
- **TEST-1/TEST-2 (`e9d179b`).** 26 sites across 3 lazy-commit test files
  predicted the `committed_payload_end` frontier using a stale
  `cfg(all(windows, …))` split — wrong for Unix + `alloc-lazy-commit` +
  not(`numa-aware`) (the frontier bookkeeping is platform-independent). Masked
  because the only CI job exercising `alloc-lazy-commit` also always enables
  `numa-aware`, which independently forces the eager `SEGMENT` frontier — so
  the wrong assertion passed by accident. Fix: replaced every platform-based
  split with a pure `cfg(feature = "numa-aware")` split matching the actual
  production gate; the 20 previously-silent sites now run on every platform.
- **CONC-1 (`a64a539`).** A loom model of the GENUINE dirty-bitmap
  producer/consumer race: all 3 existing tests in `loom_dirty_multi_segment.rs`
  `.join()` every producer before the consumer runs, so loom never explored a
  drain genuinely racing in-flight `dirty.fetch_or`. Adds a concurrent
  producer/consumer model + a `#[should_panic]` counterfactual (Relaxed dirty
  word severs the happens-before chain) proving the harness is non-vacuous.
  Severity was already documented as low (a linear fallback scan independently
  guarantees correctness; the dirty bitmap is a pure optimization layer).
- **TEST-3 (`a08092f`).** `#[should_panic]` counterfactuals added to the 3
  remaining loom files that lacked one (`loom_epoch`, `loom_sharded`,
  `loom_dirty_publish`), each backing the file's own prose claim that removing
  a specific guard makes loom fail — 9 of 13 in-tree loom files now ship a live
  regression counterfactual.
- **DOCS-SYNC (`33929b9`).** README + `src/lib.rs` synced after the workspace
  grew 4 → 11 crates and R6 file-splits scattered tier-2 unsafe sites across
  new files: the "four companion crates" → eleven; the crates.io/docs.rs badge
  table 4 → 11 rows; the external-publishable-crates unsafe-story table 4 → 11
  rows; the tier-2 item-scoped unsafe table 6 stale filenames / 21 sites → 14
  files / 33 sites (matching the self-verifying grep exactly). New guard
  `tests/no_stale_doc_references.rs::readme_unsafe_inventory_counts_match_reality`
  re-derives the counts from the same grep and asserts the README tokens match
  — counterfactual-verified non-vacuous (corrupting 17 → 18 fails it).
- **HYGIENE-GRAB-BAG (`dbfeca3`).** Four independent low-risk fixes, zero
  production allocator logic changes: (API-1) README + `src/lib.rs` now flag
  `ring-mpsc` as a real, tested, but currently zero-production-consumer
  workspace member (the in-tree swap was NO-GO — `d062798`) so it doesn't
  silently bit-rot; (API-2) `#[non_exhaustive]` on two pre-1.0 mock enums;
  (DEBT-5) deleted genuinely-orphaned `RemoteFreeRing::is_empty` (zero callers
  since phase12.6, superseded by `tail_relaxed()`) and fixed the bare
  `#[allow(dead_code)]` on `overflow()` to match its file's convention;
  (LINTS-1) centralised the duplicated `unexpected_cfgs` lint table into a
  `[workspace.lints.rust]`.
- **`ffd3215`** applies an `@fxx` follow-up-batch review (verdict
  SHIP-WITH-FIXES; #181 bootstrap-safety CONFIRMED by independent write-by-write
  trace): F1 medium serialises the 4 real-backend `crates/vmem` fault-injection
  tests behind a process-global `Mutex<()>` (they share `FAIL_NEXT`/`FAIL_AT_*`
  atomics and libtest runs them in parallel); F4 nit strengthens
  `reserve_lazy`'s debug_assert to check all three documented preconditions.
- **`b37ef98`, `327449e`.** Two CI-only fixes a local Windows `npm run check`
  cannot reach: Unix-only clippy errors in `crates/vmem`'s `libc_mmap`
  (redundant nested `unsafe {}`, unused `mut`); and allowing the Unicode-3.0
  license for the `unicode-ident` transitive dep (cargo-deny CI failure).

**docs closeout — `75343532`, `f0dd9a9`, `64952a0`, `c815927`.**
**`75343532`** lands the 5-lane crate-extraction reports + `SUMMARY` +
`DEFERRED_AND_SKIPPED` rationale + session checkpoints. **`64952a0` (DEBT-2,
task #208)** is an honest **"no bug here"** outcome worth noting as such: the
audit's DEBT-2 finding claimed `crates/vmem/tests/fault_injection.rs` was
missing a `Mutex<()>` serialization guard, but `ffd3215` (the SAME follow-up
batch DEBT-2 cites as its own source) had already applied it before the
10-agent audit even started — the finding was stale at the moment it was
written; task #208 closes as a documentation correction only, no code change.
**`f0dd9a9`** lands 5 session checkpoints. **`c815927`** (technically the
round's final commit, though task-tagged R8-4) marks the B5-era stale claims in
the R7 perf reports as superseded by R7-B6 — no numeric measurement changed,
just inline annotations pointing back to `8977e88` so the historical B5 numbers
stay accurate for what B5 measured while never reading as present-tense fact.

### Round 8 — directory promotion, Large zero-skip, medium-classes GO verdict, lazy-commit pool Phase 1 (R8-1..R8-10)

Round 8 — 15 commits (`af7b039`..`68f5da7`), 2026-07-19..20 — the external
perf/correctness review's R8 task queue. Three workstreams: finishing the
Round 7 segment-directory story (R8-1..R8-3, ending in the directory's
**first-time promotion into the `production` bundle**), a set of
constant-factor / layout / zero-skip optimizations (R8-5, R8-6, R8-8), and
two measurement-only GO/NO-GO verdicts (R8-7 batch ceiling, R8-9
medium-classes) plus one lazy-commit pool tradeoff (R8-10 Phase 1). Honest
verdicts throughout: GO on the directory sub-chain and the medium-classes
feature, GO-but-measurement-only on the batch ceiling (later downgraded by
R9-9), and an explicit acknowledgment that R8-8's Large zero-skip shipped
with a Miri correctness bug found and fixed in the next round (R9-1).

**Production vs. opt-in — what actually changed for default `--features
production` users.** This is the round where the segment directory first
went **default-on instead of opt-in**. `production` is now
`["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
"alloc-segment-directory"]` — the last entry added by R8-3:

- **Landed in `production` (default behavior changed):**
  - **R8-3 (`ec7ac34`)** — promotes `alloc-segment-directory` into the
    `production` bundle. The single behavior-shifting promotion of the
    round: every default `--features production` build now pulls in the
    directory, so the 166–254× refill-miss speedup from Round 7's r7-a6
    (and the R8-1/R8-2 fixes below) reaches ordinary users for the first
    time. First time the directory went default-on instead of opt-in.
  - **R8-2 (`09237e0`)** — authoritative directory-miss. The miss path's
    early-return becomes reachable from production as a consequence of
    R8-3's promotion (the logic is gated on `alloc-segment-directory`).
  - **R8-6 (`e9db718`)** — segment-layout `payload_start`/`decommit_start`
    split. Always compiled (no feature gate); tightens the payload
    boundary on 4 KiB-page systems in every config including production.
  - **R8-8 (`93dba14`)** — Large-path zero-skip in `alloc_zeroed`. Always
    compiled (the `alloc_large` path); skips the explicit `Node::zero`
    pass on genuinely fresh OS reservations in every config including
    production. *(Shipped with a Miri correctness bug — see the R8-8
    entry below and the R9-1 forward-reference; on real OS backends the
    optimization is correct as shipped.)*
  - **R8-5 (`fa2c064`)** — frontier-stamp fix. The source change lands in
    production-compiled files, but the behavioral change is feature-gated
    inside `alloc-lazy-commit` (which `production` does not enable), so it
    is **inert under stock production** and only active for opt-in
    `alloc-lazy-commit` users. Listed in this group because the code is
    in the production-compiled tree, not because production behavior
    shifts.
- **Stayed opt-in (NOT in `production`):**
  - **`medium-classes`** — R8-9's GO verdict is measurement-only; the
    feature itself remains experimental/opt-in.
  - **`alloc-lazy-commit` pool changes** — R8-10 Phase 1's
    stop-decommitting-on-admission fix is gated on `alloc-lazy-commit`
    (not in `production`).

**Directory sub-chain — R8-1..R8-3, finishing Round 7's Workstream A.**
Round 7 shipped the segment directory behind an opt-in feature and proved
its HIT speedup, but two regressions kept it opt-in: an O(D×49)
post-drain resweep (R7-A1's full-sweep `sync_directory_for_segment` at
every ring drain) and an O(S) linear-scan fallback on every genuine
directory MISS. R8 closes both, then promotes the result.

- **R8-1 (`af7b039`) — incremental per-class directory sync.** Each
  ring-drain closure now accumulates a `u64` bitmask of the classes it
  actually touched (`entry_class_idx` unpacks just the class from the
  packed ring entry), and the post-drain sync
  (`sync_directory_for_segment_classes`) re-checks only those classes —
  `O(popcount(changed_classes))` instead of `O(SMALL_CLASS_COUNT)`. The
  old full-sweep `sync_directory_for_segment` is deleted (zero callers
  after migrating all 4 drain sites). Eliminates the review's measured
  **1.4–2× regression at dirty ≥ 10% density** (the O(D×49) per-lookup
  cost). `tests/dirty_directory_incremental_sync.rs` proves the
  load-bearing case no existing directory test covered — a single drain
  reclaiming blocks of TWO different classes into the SAME segment must
  set BOTH classes' bits (a last-class-only bug would pass every
  existing test); verified non-vacuous by reverting the OR-accumulation
  to overwrite and watching the test go red.
- **R8-2 (`09237e0`) — authoritative directory-miss with periodic
  self-heal.** The HIT-only speedup left the MISS path unchanged: a
  genuine miss unconditionally fell through to the O(S) linear scan, so
  a miss cost the same as if the directory didn't exist — defeating the
  directory's whole point for cold-growth/carve-storm workloads. Fix:
  trust a genuine directory miss as authoritative in the common case
  (immediate `return None`, skipping the O(S) scan — the caller carves a
  fresh segment, same as it would after a full scan also finds nothing),
  bounded by a periodic safety net: every `DIRECTORY_MISS_FULL_SCAN_PERIOD`
  (256) misses a re-validation full scan still runs, and if it finds a
  segment the directory missed the bit is repaired in-place (self-heal)
  with a canary counter (`DIRECTORY_MISS_SELF_HEAL`) expected to stay 0.
  Two new stats counters
  (`DIRECTORY_AUTHORITATIVE_MISS`, `DIRECTORY_MISS_SELF_HEAL`).
  `tests/directory_authoritative_miss.rs` proves (1) a genuine miss skips
  the O(S) scan via a counter-delta proof independent of wall-clock
  noise, (2) the periodic pass fires exactly at the period boundary, (3)
  self-heal repairs manufactured directory drift. **R8-2 follow-up
  (`38f4108`)** excludes `numa-aware` from that test file: under
  `numa-aware` the entire directory-driven lookup block (including this
  task's authoritative-miss and self-heal logic) is compiled out, so
  under `--all-features` every genuine miss silently fell through with
  counters at 0, failing the first assertion — caught by `npm run
  check`'s `--all-features` matrix entry.
- **R8-3 (`ec7ac34`) — promote `alloc-segment-directory` into the
  `production` bundle.** With both keeping-it-opt-in regressions fixed
  (R8-1, R8-2), `production` now also pulls in
  `alloc-segment-directory`. Verified before promoting (not just
  delegated): full production suite with the feature explicitly combined
  (175 binaries, 397 tests, 0 failed), then re-run with just
  `--features production` to confirm the promotion took effect. Synced
  the 3 living doc references that spell out production's constituent
  feature list (README ×2, `src/lib.rs` ×1); left dated historical
  review snapshots untouched. Found and fixed two clippy regressions
  that would have broken `npm run check`'s `--all-features` gate and a
  plain `--features production` build before landing (an `unused_mut`
  under `numa-aware` from R8-2's own `periodic_revalidation_active`, and
  a `dead_code` `SENTINEL` under production-without-`hardened`), plus a
  stale `tests/*.rs` count in `ARCHITECTURE.md` (**171 → 173**,
  `180bb8a`).

**Lazy-commit frontier — R8-5 (`fa2c064`).** `reserve_aligned_lazy` is
genuinely lazy (partial 2-phase reserve+commit) ONLY on real Windows; the
Unix and miri `reserve_aligned_lazy_raw` implementations ignore
`initial_commit` and commit/mmap the whole segment up front. Despite this,
both production sites that stamp `committed_payload_end` after a lazy
reservation (`reserve_small_segment`, `bootstrap::primordial`) used only a
`numa-aware` cfg split, so Unix/miri understated the frontier at
`meta_end + LAZY_FIRST_CHUNK` even though the OS had already committed
everything — every carve past that artificial frontier still ran through
the grow-on-carve path (bounds check + a commit syscall that is a
correctness no-op on those platforms + an atomic counter bump) for zero
benefit. Fix: 3-way split at both sites — `numa-aware` stays `SEGMENT`
(unchanged), real Windows-not-miri keeps the genuine lazy value, Unix/miri
now also gets `SEGMENT` immediately, matching OS-level reality and
restoring `alloc-lazy-commit`'s promised zero-cost-when-unneeded property
there. Deliberately reverses part of this session's own earlier task #191
(`e9d179b`), which had simplified 26 test-assertion sites down to a
`numa-aware`-only split (correct at the time, since the production code
had no platform gate); those sites are re-split to match. **Stays opt-in:
`production` does not enable `alloc-lazy-commit`, so this is inert under
stock production**; verified on the Windows-lazy leg (39/39) and the
numa-aware leg (34/34). The true-Unix (non-miri) leg could not be tested
directly this session (no Linux environment) — covered by code review + a
passing miri run that exercises the same branch, to be confirmed by CI.

**Segment layout — R8-6 (`e9db718`), split `payload_start` from
`decommit_start`.** Task #205 (`65ae170`, Round 7) fixed a real platform
bug — `small_meta_end()`/`primordial_meta_end()` were aligned to the
compile-time `PAGE` (4 KiB) constant, but decommit/recommit operate on the
REAL OS page size (16/64 KiB on ARM/some Linux); a 4 KiB-aligned decommit
boundary on those platforms lands mid-real-page and the OS silently rounds
it, reclaiming the wrong byte range. #205's fix over-aligned both
functions to `MAX_REALISTIC_PAGE_SIZE` (64 KiB) — correct, but it
conflated two distinct concepts and cost ~56–64 KiB of payload per 4 MiB
segment on ordinary 4 KiB-page systems (`SMALL_META_END` jumped 73728 →
131072) even though those systems never needed the extra margin. Fix:
split into (1) `small_meta_end()`/`primordial_meta_end()`, reverted to
tight `PAGE`-alignment — used by bump init, the H-1 "is this offset in
metadata" guard, primordial registry/hash/free-list placement, and
page-map marking, none of which have any OS-page interaction; and (2) new
`small_decommit_start()`/`primordial_decommit_start()`, runtime
(non-const) functions that round the tight boundary up to the REAL
`aligned_vmem::page_size()` — used ONLY by the 3 actual decommit/recommit
syscall sites. On 4 KiB-page systems `small_decommit_start()` collapses to
exactly `small_meta_end()` — zero waste; on 16/64 KiB-page systems it
returns the same value #205 used to force unconditionally, so the
real-page-safety guarantee is unchanged. `MAX_REALISTIC_PAGE_SIZE` stays
load-bearing (wired into a `debug_assert` in both new functions guarding
the "superset of every real page size" invariant). Measured payload
recovery on this 4 KiB-page host: `SMALL_META_END` **131072 → 73728, 56.0
KiB recovered** per segment. Independently re-run under `cargo +nightly
miri test` (96s, 3/3 pass) — miri's strict-provenance checking is exactly
the tool that would catch a boundary off-by-one in this class of change.

**Large-path zero-skip — R8-8 (`93dba14`) — SHIPPED WITH A MIRI BUG, FIXED
BY R9-1.** `AllocCore::alloc_large`/`alloc_large_slow` now return
`(*mut u8, bool)`, where the bool is true iff the pointer is a genuinely
fresh OS reservation (unconditionally OS-zero-filled on every real
platform) rather than a `large_cache` HIT (a reused segment that may still
hold the prior occupant's bytes). `AllocCore::alloc_zeroed` and
`HeapCore::alloc_zeroed` dispatch on this signal for Large-classified
requests and skip the explicit `Node::zero` pass only when the reservation
is fresh; a cache hit is still zeroed explicitly. Small-classified
requests are unaffected (explicitly out of scope — the small-path
equivalent is R9-5, design-only). `tests/alloc_zeroed_fresh_large_skip.rs`
includes a byte-level regression guard that plants a `0xAA` pattern into a
freed large segment, forces a confirmed cache-hit re-allocation via
`alloc_zeroed`, and asserts the returned memory is fully zeroed (i.e. the
skip must NOT fire on a cache hit); verified non-vacuous by inverting the
`is_fresh` condition and confirming the test fails exactly on the planted
pattern.

> **⚠ Forward-reference — Miri correctness bug, fixed in the next round
> (R9-1 / `860d897`).** R8-8 as shipped reported `is_fresh = true`
> unconditionally, but miri's `std::alloc` fallback in `crates/vmem` does
> NOT zero (unlike every real OS backend). A fresh Large `alloc_zeroed`
> under miri could therefore skip the explicit `Node::zero` pass and
> return uninitialized memory, violating the `alloc_zeroed` contract.
> This is not a clean unqualified win: R9-1 (`860d897`) withholds the
> skip under miri (`alloc_large_slow` returns `cfg!(not(miri))` instead
> of unconditional `true`) and adds the `LARGE_ZERO_PASS_CALLS` /
> `dbg_large_zero_pass_count` diagnostic counter the original R8-8 test
> lacked (the original test would stay green even with an unconditional
> memset reintroduced — it proved the cache-hit zeroing but not that the
> optimization itself fired). On real OS backends R8-8's optimization is
> correct as shipped; the bug is miri-specific. The fix itself belongs to
> the Round 9 section; this entry acknowledges the gap honestly rather
> than reading as a standalone clean win.

**Medium-classes 256 KiB–1 MiB — R8-9 (`9afba66`), verdict GO
(measurement-only, feature stayed opt-in).** Runs the
`benches/medium_size_sweep.rs` harness (R6-OPT-A3, built as Stage A but
never previously run for a verdict — this is the missing Stage B) at
quick then `--reduced` tiers, `medium-classes` OFF vs ON. Findings: the
review's "16× fewer segments near the 253 KiB cliff" claim is **confirmed
precisely at n=64** (64 → 4 segments; cardinality caveat: ~15× at n=1024,
formally infinite at n≤8 where the pre-cliff side fits in the primordial
segment); free latency improves **48–600× across the covered range** (e.g.
256 KiB n=64: 93,234 → 200 ns, ~466×); the n=1024 address-space OOM is
eliminated for every covered size (the OFF path literally cannot reserve
the ~4 GiB 1024 dedicated 4 MiB spans demand, exhausting at object 1023);
a real warm freelist exists where the Large path structurally cannot have
one (256 KiB reuse rounds: ~90 µs → ~60 ns free from round 1 on, +0 segs;
the Large path re-reserves and re-releases 64 spans every round). No
regression for sizes whose path doesn't change (240/252 KiB, 258,752 B,
1.5/2/4 MiB byte-identical across configs). New finding: a **second cliff
now sits at the new `SMALL_MAX` = 1 MiB** — 1.5/2 MiB still pay the full
dedicated-span cost in both configs. Verdict: **GO (strongly) on the
existing 6 classes**; **CONDITIONAL GO, split, on extending further** —
clear case for closing the 1 MiB–2 MiB gap (design call: fixed classes vs
general page-run layer, left open), weak case for finer sub-1 MiB
granularity (rounding waste bounded ~20–31%, already-covered common
sizes). Measurement-only: no `src/` touched. **The feature stays
experimental / opt-in — promotion is a separate decision this report only
supplies evidence for.** Full method, raw numbers, and caveats in
[`docs/perf/R8_9_MEDIUM_CLASSES_VERDICT.md`](docs/perf/R8_9_MEDIUM_CLASSES_VERDICT.md).

**Batch-alloc ceiling — R8-7 (`de4c4ae`), verdict GO but measurement-only
(later downgraded by R9-9).** An external perf review speculated a public
batch/scoped alloc API (`alloc_batch`/`dealloc_batch`) could give 1.5–3×
on bulk small-object patterns by amortising TLS lookup/classification/
routing over many blocks per call. Contemplative analysis flagged the
trap: no consumer of such an API exists in this repo today, so a bench
purpose-built around a not-yet-existing signature would only prove the
mechanism works, not that it's worth shipping — circular. Instead:
measure the ceiling via the already-existing internal batch primitives
(`AllocCore::refill_class_bump` / `flush_class`, already used in
production on the magazine-miss/overflow paths) called directly from a
new bench arm, zero new public API. Measured (3 runs, 1024-block cold
bulk, criterion fast profile): **16 B: 2.73× average ceiling (GO, clears
the review's 1.5× floor); 64 B: 1.71× (GO); 256 B: 1.20× (NO-GO, below
floor)**. Verdict GO — task graduates to the signature-design phase
(explicitly not started here). This is a **ceiling, not a
shippable-API forecast** — a real public API pays extra argument
validation the raw internal call does not. Full method, raw per-run
numbers, and caveats in
[`docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md`](docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md).
*(Forward-reference: R9-9 / `5e467ec` later downgraded this measurement
by sweeping smaller batch sizes — 8/16/32/64, not just 1024 — and adding
a real-`SeferAlloc` arm; the small-batch numbers compress the ceiling
materially. The R9-9 numbers and verdict belong to the Round 9 section;
this entry records the R8-7 result as measured and flags the downgrade
honestly.)*

**Lazy-commit pool — R8-10 Phase 1 (`852828e`), stop decommitting pooled
small segments on admission.** An external perf review found
empty→pool→reuse→refill cycles on Windows `alloc-lazy-commit` cost
**50–75× more commit/decommit syscalls** than the eager path for the
identical cycle. Root cause: `release_or_pool_empty_segment` decommitted
the payload above the initial lazy chunk and reset all metadata (bump,
free lists, `is_decommitted`) the instant a segment was admitted to the
hysteresis pool — defeating the pool's own purpose (a segment pushed to
the front as "the warmest entry, expected back imminently" was
immediately decommitted, so first reuse always paid a recommit). Fix,
both sites together (removing one without the other is a correctness
bug, not just a missed optimization): `alloc_core_small_pool.rs` removes
the `alloc-lazy-commit` decommit block from
`release_or_pool_empty_segment` (pool admission now behaves identically
on the eager and lazy-commit legs — nothing is reset, the segment stays
exactly as committed as it was on emptying); `alloc_core_small.rs`
removes the matching pop-pooled-segment-as-carve-target block in
`reserve_small_segment` (which relied on admission having reset the
segment into a clean carve target; pooled-segment reuse now goes
exclusively through `find_segment_with_free`'s free-list path, same as
the eager leg). `tests/lazy_commit_b3_recycle.rs` rewritten under the
new invariant: a full empty→pool→reuse→refill cycle costs exactly zero
`GROW_COMMIT_COUNT` and zero `dbg_decommit_count()` deltas; verified
non-vacuous by reverting only the two src files and confirming 4/5 tests
go red (a `GROW_COMMIT_COUNT` delta of 15 and a decommit delta of 2,
matching the review's 50–75× claim). **Stays opt-in (`alloc-lazy-commit`,
not in `production`).**

> **⚠ Forward-reference — the latency-first RSS tradeoff this lands was
> criticized by R9-7.** R8-10 Phase 1 is a latency-first tradeoff: by not
> decommitting pooled segments it cuts reuse latency, but it costs RSS —
> today's pool retains **exactly 16 MiB of committed payload per
> materialized heap** while pooled
> (`DEFAULT_POOL_SEGMENTS = 4` × `SEGMENT = 4 MiB`; the only drain is
> `maybe_decay_small_pool`, which fully releases one FIFO-oldest segment
> per `decay_interval`, default 1 s — no intermediate
> "committed-but-cheap-to-revive" state). Round 9's R9-7
> (`docs/perf/R9_7_LAZY_COMMIT_POLICY_DESIGN.md`, design-only, no code
> change) characterized this tradeoff explicitly and designed (but did
> not ship) a third "decommitted-but-still-pooled" state that would let a
> memory-constrained deployment trade some latency back for lower
> committed RSS. The R9-7 design itself belongs to the Round 9 section;
> this entry records that R8-10's latency-first choice is the one R9-7
> later pushed back on, not a free win.

**Misc — test/formatting/doc fixes.**

- **`180bb8a`** — fixes a stale `tests/*.rs` file count in
  `ARCHITECTURE.md` (**171 → 173**, a side effect of R8-1/R8-2 each
  adding one new test file); caught by the self-verifying
  `architecture_test_file_count_matches_reality` test.
- **`f919d5b`** — lands session checkpoint files.
- **`f97cf1f`** — fixes pre-existing rustfmt drift in
  `crates/vmem/src/lib.rs` (pure line-wrapping of one `#[cfg_attr(...)]`
  attribute, no semantic change; pre-existing since task #210 never ran
  `cargo fmt`; blocked a clean `npm run check`).
- **`ac110b6`** — corrects `misaligned_offset_guard`'s release-build
  truncation math. The release-build branch asserted that packing a
  `MIN_BLOCK+1` offset truncates to `off16=0` on round-trip; that's wrong
  (`off16 = off >> MIN_BLOCK_SHIFT` is a floor division, so
  `(MIN_BLOCK+1) >> MIN_BLOCK_SHIFT == 1`, unpacking back to `MIN_BLOCK`,
  not 0). The assertion was never exercised under a normal `cargo test`
  (which runs the `debug_assertions` branch, where `pack_entry_hardened`
  panics instead), only under `--release --all-features` — a combination
  no CI matrix entry or prior `npm run check` happened to hit. Found
  while zero-trust reviewing R8-10 and running the full suite under
  `--all-features`; confirmed unrelated by reproducing against pristine
  HEAD 3×.
- **`68f5da7`** (the round's final commit) — hardens the
  `backshift_no_latency_spike_at_threshold_boundary` regression test's
  per-dealloc wall-clock max/median ≤ 30× check against OS noise (a
  single dealloc occasionally stalls multi-ms from scheduler preemption,
  page faults, AV I/O hooks — reproduced repeatedly this session with no
  code change, ratios varying 42×–607×, 1/6 to 6/6 pass rate). Wraps
  only the noisy max/median ratio check in a bounded 3-attempt retry; the
  membership/correctness assertions stay unconditional (a genuine
  per-delete `O(HASH_CAPACITY)` regression reproduces deterministically
  every attempt, so detection power is preserved).

### Round 9 — Miri correctness fix, medium-classes-wide prototype, directory drift hardening, honest downgrades (R9-1..R9-9)

Round 9 — 11 commits (`860d897`..`d26e042`), 2026-07-20 (a single day) —
the external review's follow-up queue against the Round 8 HEAD. This round
is overwhelmingly **research / measurement / design work, not production
hot-path changes**: of the nine R9-numbered tasks, only **two** touch
production-compiled source for real (R9-1's Miri correctness fix and
R9-8's directory-drift bound); the other seven are measurement reports
(R9-2, R9-3, R9-6, R9-9), a new opt-in prototype feature that stays
opt-in (R9-4), or design-only docs (R9-5, R9-7). The round's character is
honest verdicts over shipped wins: two **downgrades** of prior GOs (R9-4's
density came in below the review's guess; R9-9 downgraded R8-7's batch
ceiling to CONDITIONAL-NO-GO), two **CONDITIONAL-GOs** that defer to a
future wall-clock measurement (R9-3's promotion gate, R9-6's class-aware
dirty routing), and two **design-only** outcomes (R9-5, R9-7) that are
treated as fully successful results, not shortfalls — each found a real
reason a rushed prototype would have been dead code or unsound, and
declined to ship that.

**Production vs. opt-in — what actually changed for default `--features
production` users.** Round 9 is the first round since the directory's R8-3
promotion where the production bundle itself is stable: no feature is
added to or removed from `production` this round. Only two code changes
reach production-compiled source:

- **R9-1 (`860d897`)** — Large zero-pass Miri fix. Always compiled (the
  `alloc_large` path), but the behavior change is **miri-only**: on every
  real OS backend R8-8's optimization was already correct, so
  `cfg!(not(miri))` evaluates the same as the old unconditional `true`
  there. No real-OS user sees a behavior shift; only a miri run does.
- **R9-8 (`5a4ba62`)** — directory drift recovery. The directory is in
  `production` since R8-3, so the per-class miss-streak + OOM-rescue scan
  reach production users — but as **defense-in-depth against a
  hypothetical drift the invariant-preserving API cannot construct**
  (task #214's `assert_directory_equals_rebuild` oracle proves the
  incremental directory tracks true state in every tested scenario), not
  a fix for a known bug.
- **Everything else is measurement/design/opt-in:** R9-2 / R9-3 / R9-6 /
  R9-9 are `docs/perf/*.md` + benches/tests only (no `src/`); R9-5 / R9-7
  are `docs/perf/*.md` design docs only; and R9-4's `medium-classes-wide`
  is a new opt-in feature (not in `production`, not in any default bundle,
  and its own follow-up fix `78fd98d` keeps it from regressing
  `--all-features`).

**R9-1 (`860d897`) — Miri correctness fix for R8-8's Large zero-skip.**
Closes the P0 bug R8-8 shipped with (flagged forward in the R8-8 entry
above). `alloc_large_slow` reported `is_fresh = true` unconditionally, but
miri's `std::alloc` fallback in `crates/vmem` does NOT zero (unlike every
real OS backend), so a fresh Large `alloc_zeroed` under miri could skip
the explicit `Node::zero` pass and return uninitialized memory — violating
the `alloc_zeroed` contract. Confirmed by reading the vmem miri fallback
directly before fixing anything. Fix: `alloc_large_slow` now returns
`cfg!(not(miri))` instead of unconditional `true`; both consumers
(`AllocCore::alloc_zeroed`, `HeapCore::alloc_zeroed`) are otherwise
unchanged since they already branch on the freshness bool. The original
R8-8 test also couldn't prove the optimization itself fired — it would
stay green even with an unconditional `memset` reintroduced. Added a
process-wide diagnostic counter (`LARGE_ZERO_PASS_CALLS` /
`dbg_large_zero_pass_count`) bumped at both zero-pass call sites, and
rewrote `tests/alloc_zeroed_fresh_large_skip.rs` to assert exact deltas
per platform (0 under a real OS, 1 under miri). Verified non-vacuous by
counterfactual (reverting the `!is_fresh` guard turns the rewritten test
red with the expected delta mismatch), then re-confirmed under real miri
(the miri-only workload was shrunk `LARGE` 2 MiB → 1 MiB+4 KiB, `ITERS`
80 → 4 under `cfg(miri)` only, since the logic under test has no size- or
iteration-count-dependent branch — cut the miri run from hours to ~17 min
without weakening native-path coverage). Also lands doc drift the same
review flagged (Cargo.toml's `alloc-segment-directory` comment still
calling it experimental post-R8-3; ARCHITECTURE.md's M6 decommit section
describing the pre-R8-10 lifecycle; `segment_directory_a5.rs`'s pooled-
segment test doc).

**R9-2 (`3021b16`) — fresh post-Round8 cross-version bench, verdict: Round
8 did not move the default-bundle wall-clock past the noise floor.**
Refreshes the cross-version wall-clock comparison (0.3.0 = current `main`
vs 0.2.1 = `bench/0.2.1`) anchored at current HEAD (`860d897`, Round 8 +
R9-1), same methodology as Round 7's. *(Filename/content note: this
report lives at `docs/perf/R8_CROSS_VERSION_BENCH.md` — the filename says
"R8" but the file IS the R9-2 deliverable, a fresh post-Round8 re-anchor
of R7's methodology; there is no R9-named cross-version file, matching the
R7→R8 continuation convention.)* Top-line: the production wall-clock
bundle did not move meaningfully vs R7 — Round 8 + R9-1 were correctness /
feature-gated opt-in (`medium-classes`, `alloc-lazy-commit`) /
constant-factor work, none of which the default production bundle would
be expected to surface. The Round 7 multiplicative reuse-cycle wins are
re-confirmed intact: the decommit cycle **~292× faster**, the oscillating
working-set cycle up to **~3.44× faster at 1024 B**, and the churn family
at 64 B+ still winning **~1.4–2.0× at 64–256 B and ~2.0–10.8× at 1024 B**
(vs 0.2.1). No real regression; the ns/op columns stay within the
documented ±15–20% inter-run host-noise floor. Full method and numbers in
[`docs/perf/R8_CROSS_VERSION_BENCH.md`](docs/perf/R8_CROSS_VERSION_BENCH.md).

**R9-3 (`c8f5f32`) — `medium-classes` production-promotion gate, verdict GO
(by IAI) / status stayed CONDITIONAL-GO.** R8-9 gave a GO verdict for
`medium-classes` but only measured its own target range (256 KiB–1 MiB)
via `AllocCore` directly. The external review flagged three structural
side-effects hitting every build that R8-9 did not check — `SIZE_CLASS_TABLE`
growing 49→55, per-`HeapCore` tcache footprint growth, and first-heap
commit charge — so this task runs all three plus the deterministic IAI
instruction-count gate for the unaffected sizes (16–1024 B) through the
real `HeapCore`/`GlobalAlloc` production path. Findings: tcache footprint
exactly **+816 B/HeapCore** (`PerClass`=136 B × 6 new classes — confirmed,
not estimated, matching the review's estimate exactly); IAI Ir on the
small-size gates **+0.49% to +0.67% total, +0.1% to +0.5% per-op marginal**
(bootstrap-spread from the larger table zero-init, not a per-op cost);
first-heap commit charge **+48 KiB at the 1-heap bootstrap** (within one
page of the chunked-registry chunk-0 prediction) and **+4 KiB/slot
steady-state** (noise-floor). The criterion wall-clock showed **+37–56%
uniform across ALL sizes including 16 B** — declared a host-load artifact
and overruled by IAI (a real regression could not be uniform at 16 B,
which has zero interaction with the new classes). This wall-clock overrule
is methodologically debatable and was later criticized by external review,
so the status stayed **CONDITIONAL-GO**: the gate evidence is GO by the
deterministic judge, but the report does NOT itself flip the `Cargo.toml`
bundle, so the feature remains experimental/opt-in and promotion is not
enacted. The one large deterministic delta found — `realloc_grow` **+173.9%
Ir / +101.3% EstCycles** — is the feature working as designed on sizes it
targets (the geometric realloc sweep now passes through the new 256 KiB–1
MiB classes doing real small-path work where the Large path previously did
almost nothing), not a regression on the unaffected sizes this gate
protects. Full method, raw numbers, and the K-table in
[`docs/perf/R9_3_MEDIUM_CLASSES_PRODUCTION_GATES.md`](docs/perf/R9_3_MEDIUM_CLASSES_PRODUCTION_GATES.md).

**R9-4 (`f469343`) — `medium-classes-wide` prototype (1.25/1.5/1.75 MiB),
verdict: do-not-promote-as-is (density under-delivered).** Adds a new
opt-in feature `medium-classes-wide` (requires `medium-classes`,
transitively `alloc-core`; not in `production` or any default bundle) that
appends three exact classes (1.25 / 1.5 / 1.75 MiB) on top of the existing
six-class `medium-classes` `EXTRAS` list, growing `SMALL_CLASS_COUNT`
55→58 and `SMALL_MAX` 1 MiB→1.75 MiB. Purely additive: the existing
six-class `EXTRAS` list is byte-identical to pre-R9-4 (verified by test),
so R9-3's just-landed promotion-gate measurements stay valid. The external
review guessed ~3×/2×/2× objects-per-segment density for the three new
classes; this task **measured the real density empirically instead of
trusting the guess and found it 2×/1×/1×** — one block lower than guessed
for every class, because `carve_block`'s `align_up(bump, block_size)`
requirement (load-bearing: the free path derives block start via
`align_down(ptr, block_size)`) wastes one block of segment capacity at the
start, so `empirical_density = floor(SEGMENT / block_size) - 1`. Only
**1.25 MiB** delivers a real density win (2× vs the Large path's 1×); **1.5
and 1.75 MiB fit exactly 1 block per segment** (same 1× as the existing
Large path, though they still gain the warm-freelist win R8-9 measured for
same-size reuse — ~90 µs free → ~60 ns freelist push/pop). A new mini-cliff
sits at ~1.3 MiB (the rounding threshold into the 1.5 MiB class). 12 new
tests in `tests/medium_classes_wide_correctness.rs` pin class placement,
boundary routing (both directions), encoding-headroom ceilings, the
density finding (pinned by actual carve+segment-residency checks, not just
arithmetic), and topology non-disturbance of the 6-class substrate.
Recommendation: **do not promote as-is**; if promoted, restrict to just the
1.25 MiB class, or pair with a future page-run layer for 1.5–2 MiB (2 MiB
itself is out of scope here — it would also be 1×, needs a larger
medium-arena segment). This is an honest downgrade the sub-agent found
itself, reported against the review's optimistic guess. Full method,
geometry, and the verdict in
[`docs/perf/R9_4_1_75MIB_CLASSES_PROTOTYPE.md`](docs/perf/R9_4_1_75MIB_CLASSES_PROTOTYPE.md).

**R9-4 follow-up (`78fd98d`) — `--all-features` test-regression fix.**
R9-4's `medium-classes-wide` appends 3 classes on top of `medium-classes`
when both are enabled (exactly what `--all-features` does), which silently
broke three pre-existing tests that hardcoded plain-`medium-classes`
topology (55 classes / `SMALL_MAX` = 1 MiB) without accounting for
`medium-classes-wide` raising those to 58 / 1.75 MiB:
`tests/medium_classes_correctness.rs` and
`tests/segment_directory_a5.rs` (hardcoded 55-class assumptions, made
conditional on `cfg!(feature = "medium-classes-wide")`), and
`tests/regression_inplace_large_realloc.rs` (three tests used 1.5 MiB as a
hardcoded "definitely Large" size — no longer true under
`medium-classes-wide` where `SMALL_MAX` = 1.75 MiB, so 1.5 MiB routes
through the Small path and breaks the Large-path in-place-grow
optimization these tests exist to verify; bumped to 2 MiB, matching the
"unambiguously Large under every feature combination" convention).
Discovered by the orchestrator's own verification of R9-9 (running the
full suite under `--all-features`, not just isolated feature combos) and
fixed in a separate commit before R9-9 landed — an oversight during R9-4's
own review (each feature combo had been tested in isolation, never both
on at once). Verified: full `cargo test --release --all-features` now green
(180 test binaries).

**R9-5 (`7bdbc0f`) — virgin zero-skip for Small `alloc_zeroed`,
DESIGN-ONLY.** Designs a per-segment `payload_virgin` bool that would let
`alloc_zeroed` skip the explicit zero pass for a genuinely virgin
(never-before-carved) small block, mirroring the Large-path skip (R8-8 /
R9-1) at a finer (per-carve, not per-segment-reservation) granularity. All
five correctness risk areas the task enumerated (pooling, lazy-commit
incremental commit, release-vs-decommit+recommit macOS crux, batched
carve, remote-free reclaim) are resolved with file:line evidence and
independently spot-checked in this review. This exact idea was a
documented **NO-GO on 2026-07-10** (two blocking reasons: no per-block
virgin state, and an unresolved macOS `MADV_DONTNEED` risk) and was
re-flagged as unresolved by a 2026-07-19 deep audit; this design shows
R8-10 (2026-07-20, the day after that audit) removed the only production
code path that produced the macOS-dangerous decommit-then-reuse state,
dissolving that risk. The remaining objection (narrow win, cold-path
only) is acknowledged, not rebutted. **No code shipped:** a
substrate-only prototype (`AllocCore::alloc_zeroed`'s small arm) would be
fully testable but **production-inert**, since `HeapCore::alloc_zeroed`'s
small arm never delegates to `AllocCore::alloc_zeroed` (grep-confirmed
zero call sites) — the production win requires plumbing the virgin bit
through the magazine refill path, an open storage-design question staged
as a future 4-stage plan (measurement gate → substrate prototype →
magazine plumbing → promotion gate). Estimated win ceiling (analytical,
memset bandwidth): **~130 ns at 4 KiB to ~70–90 µs at 1 MiB** per
genuinely-virgin call; zero benefit on steady-state churn. Design-only is
the correct outcome here, not a shortfall: shipping the substrate
prototype would have been dead code. Full design in
[`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md`](docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md).

**R9-6 (`fd28ff8`) — class-aware dirty routing waste, verdict CONDITIONAL-GO.**
The external review found `drain_dirty_segments` (R7-A4) drains EVERY dirty
segment regardless of which size class `find_segment_with_free_impl` is
currently searching for — O(D) where D = dirty segment count, when per-
(segment, class) tracking could make it O(D_class). This task judges the
claim via one new diagnostic counter, not an implementation: `WASTED_DIRTY_DRAINS`
(directory_stats.rs) + `dbg_wasted_dirty_drains()`, bumped when a drain
visit produces zero reclaimed blocks of the sought `class_idx` (the sought
class is already available at the call site; `drain_dirty_segments` gained
one additive `class_idx` parameter, no control-flow change). Purely
diagnostic, Relaxed ordering, gated behind `alloc-stats` — zero behavior
change to the drain algorithm. The new judge test
(`tests/r9_6_class_aware_dirty_judge.rs`) drives a genuine mixed-class
remote-fan-in workload (N=1/2/4/8 producer threads each freeing a distinct
class into a shared owner while the owner continuously allocates one of
those classes) through the real `HeapCore::dealloc` cross-thread-free path.
Measured (3-run median): waste scales **super-linearly** with class count —
**~2% at N=1, ~56% at N=2, ~82% at N=4, ~95% at N=8**, consistently ABOVE
the naive (N−1)/N bound because the actively-consumed target class's dirty
bit clears faster than the collateral classes'. Confirms the review's
mechanism is real. Verdict **CONDITIONAL-GO**, not unconditional — this
measures counter ratios, not wall-clock win, and the absolute drain counts
are modest in this bench's shape (up to ~823 wasted drains per 4000 owner
allocs at N=8); next step before implementing is a wall-clock criterion
bench on the same workload shape (a >5% win at N=4 upgrades to GO). Full
method, raw counts, and the recommendation in
[`docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md`](docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md).

**R9-7 (`021c098`) — low-RSS pool policy for the lazy-commit tradeoff,
DESIGN-ONLY.** Designs a third pooled-segment state (decay-gated
decommit-and-reset to a blank carve target) to let a memory-constrained
deployment trade some latency back for lower committed RSS, addressing the
R8-10 latency-first tradeoff the R8-10 entry forward-references above
(R8-10's pool retains **exactly 16 MiB of committed payload per
materialized heap** while pooled — `DEFAULT_POOL_SEGMENTS = 4` × `SEGMENT`
= 4 MiB — with no intermediate "committed-but-cheap-to-revive" state).
**Central finding: the review's own suggested shape (decommit the payload,
keep free-list metadata intact) is UNSOUND.** The free-list `next` link is
stored in the first word of each free block's BODY (`Node::write_next` /
`read_next` write/read block payload directly), not in metadata — so
decommitting the payload destroys the chain. On Windows/Linux this
silently leaks every block past the head; on macOS (non-zero-guaranteed
recommit) it produces wild pointers, UB in production (hardened's
membership guard is not in the production bundle). Corrected design: reuse
the existing full-reset decommit primitive (`release_follows=false`) to
produce a blank carve target, reused via fresh carve rather than
free-list pop. Explains why decay-gating (vs R8-10's rejected
admission-gating) avoids the 50–75× commit/decommit blowup: the decommit
rate is bounded by the decay clock, not the allocation rate, and is zero
for any workload that stays hot. Cross-references R9-5: this design's
age-0→1 transition is exactly the "new decommit policy" R9-5 flagged as
needing a `payload_virgin=false` reset if R9-5's virgin-zero-skip ever
ships. **No prototype shipped:** the implementation surface (4
touch-points, including re-introducing a primitive R9-5 characterized as
fragile-to-reintroduce) exceeds this task's minimal/safe bar — ships a
safe zero-new-surface interim stopgap (shrink `pool_segments` + shorten
`decay_interval`, existing knobs only) plus a staged 4-phase plan for a
future task. Design-only is the correct outcome: the review's shape was
wrong, and shipping it would have been a silent-leak bug. Full design in
[`docs/perf/R9_7_LAZY_COMMIT_POLICY_DESIGN.md`](docs/perf/R9_7_LAZY_COMMIT_POLICY_DESIGN.md).

**R9-8 (`5a4ba62`) — directory drift recovery (per-class miss-streak +
OOM-rescue scan), verdict GO (implemented).** Two defense-in-depth fixes
to the R8-2 directory-authoritative-miss fast path (which trusts a
directory MISS for up to `DIRECTORY_MISS_FULL_SCAN_PERIOD - 1` consecutive
misses before a periodic re-validation scan). The external review found
R8-2 tracked that streak with a SINGLE `u32` shared across every size
class — so a drift-affected class's rescan could be delayed by cross-class
traffic, worst case ~255 wasted 4 MiB segments (~1 GiB VA) before the
shared counter trips. Two fixes, both verified non-vacuous by
counterfactual:

1. **Per-class miss-streak** — `directory_miss_streak` is now
   `[u8; SMALL_CLASS_COUNT]`, indexed by `class_idx`; each class trips its
   own rescan independent of other classes' traffic. Period dropped
   **256 → 64 (per-class)**, capping the worst-case drift bound at 64
   segments = **256 MiB, 4× tighter** than before (and strictly improving
   detection for a low-activity drifted class the shared counter could
   starve indefinitely).
2. **Rescue scan before OOM** — right before `reserve_small_segment`
   surfaces an OOM (table full or OS reservation failure), a forced O(S)
   linear scan runs as a last resort, bypassing the directory-trust fast
   path; if it finds a real free block the directory hid it self-heals the
   bit and serves that block instead of OOMing. Wired into both small-alloc
   OOM branches (`alloc_small`, and the magazine refill path via the
   checked variant to avoid a cross-thread double-issue). A new
   `DIRECTORY_RESCUE_OOM_AVOIDED` counter keeps this distinguishable from
   the periodic `DIRECTORY_MISS_SELF_HEAL` canary. Large allocations don't
   consult the directory (no `BinTable`), so no rescue is needed there.

A genuine drift remains **essentially impossible to construct** through the
invariant-preserving API (task #214's oracle proves the incremental
directory tracks true state in every tested scenario) — these are a safety
net against an undiscovered edge case or future regression, not a fix for
a known bug. Verified non-vacuous by counterfactual: temporarily routing
all classes through streak slot 0 (simulating the old shared counter) makes
the new decoupling test fail at its exact load-bearing assertion; reverted
and confirmed green. Also fixes a stale test-file-count assertion in
`docs/ARCHITECTURE.md` (**175 → 178**) that R9-6 had left behind. Full
design, counterfactuals, and the worst-case math in
[`docs/perf/R9_8_DIRECTORY_DRIFT_RECOVERY.md`](docs/perf/R9_8_DIRECTORY_DRIFT_RECOVERY.md).

**R9-9 (`5e467ec`) — batch-API realistic-size follow-up, verdict
CONDITIONAL-NO-GO (DOWNGRADE of R8-7).** R8-7 measured a 2.73×/1.71×/1.20×
GO/GO/NO-GO batch-alloc ceiling, but only at **batch=1024** and only
comparing **AllocCore-direct** arms (both bypassing `SeferAlloc`/TLS/
registry entirely). The external review's follow-up asked for realistic
batch sizes (8–64) and a comparison against the real public
`SeferAlloc`/`GlobalAlloc` scalar path. Added a sibling criterion group
sweeping N ∈ {8, 16, 32, 64, 1024} at the same three sizes (16/64/256 B)
with a third arm measuring `sefer.alloc`/`dealloc` through the real
`GlobalAlloc` impl; R8-7's own group is left untouched as the historical
batch=1024 baseline. Two findings:

- **The batch/scalar ceiling degrades sharply at realistic batch sizes.**
  16 B drops from 2.60× (n=1024) to **1.24–1.50× (n=8–64)**; 64 B from
  1.67× to **1.13–1.51×**; 256 B stays near 1.1× throughout. Two
  compounding causes: amortization thins out with fewer calls to amortize
  over, and a fixed cold-page-fault cost (fresh `AllocCore` per iteration)
  compresses the ratio toward 1.0 at small N.
- **The three-way comparison is the more decisive finding.** At every
  realistic batch size, the real `SeferAlloc` scalar path (warm per-thread
  tcache) is **2–30× FASTER than the `AllocCore` batch primitive** — the
  tcache already amortizes per-call overhead for small N, leaving nothing
  for a batch API to amortize until N is large enough to overflow the
  tcache. Even at n=1024 the batch primitive only beats real `SeferAlloc`
  scalar at 16 B (1.96×); it loses at 64 B and 256 B.

**Updated verdict: CONDITIONAL-NO-GO for realistic callers.** R8-7's GO at
16 B/64 B was specific to the unrealistic batch=1024 case. The one
surviving signal is narrow: 16 B at batch ≥ ~1024 for a caller that
genuinely issues such batches (no such caller exists in this repo today —
the same circularity concern R8-7 already flagged). For the 8–64 range the
review asked about, the verdict is **NO-GO**. The R8-7 report stays valid
as the historical batch=1024 / AllocCore-only baseline; nothing here
contradicts it. This is an honest downgrade of a prior GO, reported as
such. Full method, the three-arm grid, and the per-size verdict table in
[`docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md`](docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md).

**Misc — `d26e042` (the round's final commit) — checkpoint-only.** Lands
the Round 9 completion checkpoint
(`docs/checkpoints/2026-07-20-r9-complete.md`); no `src/`, test, or
`Cargo.toml` change.

### Round 10 — external-review follow-up: correctness fixes, honest gate corrections, batch API reversal (R10-1..R10-7)

Round 10 — 8 commits (`b2ef79e`..`9611a56`, inclusive of both ends),
2026-07-21 (a single day) — the external-review follow-up queue against
the Round 9 HEAD. The round's defining trait is that it is **unusually
self-correcting**: four of its eight entries revisit and revise a claim
an earlier round made, in every direction — downward (R10-2 builds the
wall-clock gate R9-3 deferred and flips R9-3's ambiguous "overruled by
IAI" wall-clock to a decisive NO-GO), upward (R10-7 builds the warm-batch
arm R9-9 only inferred and reverses R9-9's CONDITIONAL-NO-GO to a GO),
and as a magnitude correction that keeps the direction but fixes the
number (R10-5 corrects R9-4's ~1,500× "consolation prize" to the real
~2.3×); R10-4 adds a fourth, stranger shape — a CONDITIONAL-GO whose own
design identifies a strictly superior alternative and so declines to
ship. As in Round 9, this is **mostly measurement / design / docs work,
not production hot-path changes**: of the eight commits, only three
touch production-compiled source for real (R10-1's diagnostic gating,
R10-3's correctness fix, R10-7's new experimental surface), and only
R10-3 carries an observable behavior shift; the rest are perf reports
(R10-2, R10-5, R10-6), a design-only doc (R10-4), or a README + unsafe-
inventory sync (`6a11c61`). That the round spends four of its eight
entries correcting itself — building the gate a prior round deferred,
measuring the arm a prior round only inferred, correcting the baseline a
prior round mis-framed — is treated below as the round's central result
and a feature of this project's methodology, not a list of embarrassments
to downplay.

**Production vs. opt-in — what actually changed for default `--features
production` users.** As in Round 9, the production bundle's feature set
is unchanged this round: no feature is added to or removed from
`production`. Three commits reach production-compiled source, with
sharply different blast radii:

- **R10-1 (`b2ef79e`)** — diagnostic-counter gating. The
  `LARGE_ZERO_PASS_CALLS` static and `dbg_large_zero_pass_count()`
  accessor stay always-compiled (matching `WASTED_DIRTY_DRAINS` /
  `FOREIGN_OR_UNROUTABLE_FREES`), but both increment sites move behind
  `alloc-stats`. **Zero observable behavior change** on any build:
  production (alloc-stats off) loses the two counter bumps from the
  zeroed-allocation path and reads `0` from the accessor, exactly as the
  sibling counters do; the byte-content zero-fill guard (the
  load-bearing information-disclosure assertion) stays unconditional.
- **R10-3 (`abaad9c`)** — directory correctness fix (see below). The one
  real behavior shift in the round, and it is **beneficial**: the
  `changed_classes` bit is now set only when a block was genuinely
  reclaimed into the `BinTable`, eliminating spurious directory-syncs for
  classes whose ring entries were all rejected. The
  `reclaim_offset`/`reclaim_offset_checked` return value is also
  corrected (was "did decommit fire?", now "was the block reclaimed?").
- **R10-7 (`9611a56`)** — adds `HeapCore::alloc_batch` /
  `SeferAlloc::alloc_batch` / `dealloc_batch`, all `#[doc(hidden)]`
  experimental surface, **not wired into `GlobalAlloc`**; the production
  alloc/dealloc path is unchanged and the new code is only reachable via
  the opt-in experimental API.
- **Everything else is measurement/design/docs:** R10-2 / R10-5 / R10-6
  are `docs/perf/*.md` + probe binaries/benches only (no production
  `src/`); R10-4 is a `docs/perf/*.md` design doc only; `6a11c61` is
  README + a stale line-ref sync.

**R10-1 (`b2ef79e`) — gate `LARGE_ZERO_PASS_CALLS` increments under
`alloc-stats` (hygiene).** Pure hygiene fix against the diagnostic
counter R9-1 introduced. R9-1 bumped `LARGE_ZERO_PASS_CALLS`
unconditionally at both zero-pass call sites (`AllocCore::alloc_zeroed`,
`HeapCore::alloc_zeroed`); this matches the convention the rest of the
directory/drain diagnostic family already follows: the static and the
`dbg_large_zero_pass_count()` accessor stay always-compiled so the read
surface is stable across feature sets (reading `0` when no increment was
compiled in), while the increments themselves move behind `alloc-stats`
so the zeroed-allocation path carries no bookkeeping unless the caller
opts in. `tests/alloc_zeroed_fresh_large_skip.rs`'s counter-delta
assertions are gated the same way; the byte-content zero-fill checks (the
load-bearing information-disclosure guard) stay unconditional. **Zero
behavior change** on any build without `alloc-stats`.

**R10-3 (`abaad9c`) — gate `changed_classes` on actual reclaim success
(correctness fix).** Found while fixing the R9-6 `WASTED_DIRTY_DRAINS`
metric, and the root cause ran deeper than the metric itself. A rejected
cross-thread ring entry (double-free guard, in-magazine duplicate, stale
generation, garbled offset) never mutated the segment's `BinTable`, but
`drain_dirty_segments`'s `changed_classes` bitmap set the class bit
unconditionally anyway — under-counting `WASTED_DIRTY_DRAINS` (a drain
that rejected every entry of the sought class still looked "not wasted")
and triggering a spurious directory-sync for an unchanged class. The
deeper bug: `reclaim_offset` and `reclaim_offset_checked` returned
`dec_live_and_maybe_decommit`'s result (true = decommit fired), **not**
whether the block was actually reclaimed — under `not(alloc-decommit)`
this was always `false`, making the return value useless as a
reclaim-success signal. Restructured both functions to return true iff
the block was linked into the `BinTable`; `dec_live_and_maybe_decommit`
is now called separately at each of the 6 call sites (3 in
`alloc_core_small.rs`, the `dbg_drain_all_rings_impl` test hook, and the
2 `HeapOverflow` drain sites in `heap_core_xthread.rs`) after a
successful reclaim. `changed_classes` is now gated on this corrected
reclaimed signal everywhere it is accumulated. New counterfactual test
(`tests/r10_3_rejected_entry_changed_classes.rs`) drives a genuine
cross-thread double-free through the production alloc path and proves
the fix red-before/green-after; R9-6's judge was re-measured post-fix and
is unchanged for that workload (it has no rejected entries), narrowing
but not eliminating its "lower bound" caveat (down to only the
empty-ring-visit exclusion).

**R10-2 (`c8d53af`) — native A/B/B/A wall-clock gate for
`medium-classes`, verdict NO-GO on realloc (corrects R9-3's ambiguity).**
Builds the methodologically clean process-level judge the external review
asked for after R9-3's single noisy criterion run (`+37…+56%` uniform
across ALL sizes including 16 B, overruled by IAI) proved neither
acceptable as a regression nor dismissable as noise. Two probe binaries
(`paired_ab_medium_{off,on}`), byte-identical source differing only in
the `production` vs `production,medium-classes` Cargo feature set, driven
through `scripts/r10_2_medium_gate.mjs` — 20 A/B/B/A blocks × 3
independently-timed phases (alloc/free/realloc) × 4 launches = **240
fresh process launches**, reusing `scripts/paired-ab-runner.mjs`'s A/B/B/A
+ paired-t-test + sign-test machinery. Results (all statistically
unambiguous, t / sign): **alloc ~31× faster** (t=55.8, 20/20), **free
~211× faster** (t=88.3, 20/20), but **realloc ~2,111× slower** (t=-53.6,
20/20) — the baseline's Large path grows in-place within its dedicated
4 MiB span at near-zero cost, while `medium-classes`' dense packing
forces a move-leg (alloc + memcpy + dealloc) on every cross-class
realloc-grow. This is the wall-clock confirmation of R9-3's `+173.9%` Ir
finding on `realloc_grow`, now decisive instead of ambiguous. The realloc
kill-gate (>20% regression) fires and, per the task's explicit design, is
**not** overruled by the alloc/free wins the way R9-3's noisy run was
overruled by IAI. **Verdict: NO-GO on promoting `medium-classes` into
`production` as-is** (stays opt-in); ships a break-even analysis (~205
reallocs per alloc/free cycle) and three mitigation directions (in-place
medium-class grow, growth headroom, or a documented realloc-light
deployment profile). This is a measured resolution of the ambiguity R9-3
left open, not a contradiction of R9-3's measurements. Independently
re-verified at `--quick` (4 pairs, t=-36.8, 4/4). Full method, raw
numbers, and the break-even table in
[`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`](docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md).

**R10-4 (`fed3d45`) — run-origin oracle design for wide-class alignment,
verdict CONDITIONAL-GO but a strictly superior alternative exists
(DESIGN-ONLY).** Design-only deliverable (the mandatory design-review
gate for correctness-sensitive changes to the cross-thread reclaim path).
Answers whether `carve_block`'s `align_up(bump, block_size)` can be
relaxed to `align_up(bump, class_align)` for the `medium-classes-wide`
1.25/1.5/1.75 MiB classes, recovering R9-4's measured 2/1/1 density to
the theoretical 3/2/2 — and what breaks: the reclaim guard's "offset is a
multiple of `block_size`" defence-in-depth invariant. Full inventory of
**19 `block_size`-multiple assumption sites** (11 unaffected, 4 need a
new guard, 4 are comment/logic updates). Two concrete oracle designs,
both proven at-least-as-safe as the current check: **Oracle A**
(per-segment carved-starts bitmap, strictly stronger, +32 KiB/segment)
and **Oracle B** (per-class run-origin array reusing the already-reserved
second `BinTable` slot, zero new metadata, equivalent containment).
Reclaim-path overhead estimated at +1–3 cycles for wide classes only,
negligible against the ~100-cycle reclaim path. **Verdict:
CONDITIONAL-GO** — technically sound, but the design itself identifies
the page-run layer (R8-9/R9-4's alternative direction) as **strictly
superior**: 3–6× more density (11/9/8 vs 3/2/2) with zero guard breakage
and zero new metadata. Stage 2 (prototype) is **deliberately not
started**; it needs explicit human/roadmap sign-off given the correctness
surface and the identified better alternative — a genuine product
decision, not one this session makes unilaterally. Full inventory and
both designs in
[`docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md`](docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md).

**R10-5 (`fdd360d`) — warm-vs-warm Large-cache-hit gate for 1.5/1.75 MiB
(magnitude correction of R9-4).** Corrects a ~600×-inflated claim from
R9-4. R9-4 framed the 1.5/1.75 MiB classes' density-1× recycle speed as a
"consolation prize" (~90 µs Large recycle → ~60 ns freelist push/pop),
but that ~90 µs was measured against a Large-cache **miss** (full
`VirtualFree`+`VirtualAlloc`), not a **hit** — and `production` keeps the
Large cache active (`OPT-E`, `LARGE_CACHE_SLOTS=8`), which recycles a
warm span via cheap in-process bookkeeping with no syscall. This gate
builds the fair warm-vs-warm comparison: two probe binaries differing
only in Cargo features, working set (`WS_LEN=6`) kept below
`LARGE_CACHE_SLOTS` so the baseline's steady-state allocs provably hit
the warm cache — **proven**, not assumed, via a `large_cache_hits`
diagnostic counter emitted and checked (18012 = `WS_LEN × (ROUNDS +
WARMUP_ROUNDS − 1)`, zero variance across all 40 baseline launches) —
then a 20-pair A/B/B/A wall-clock comparison per size. **Result: the
small path is still faster, but by ~2.3× (76–80 ns → 31–34 ns per
recycle, t=14.7–17.3, sign 20/20), not R9-4's ~1,500×.** R9-4's direction
was right; its magnitude was inflated ~600× by comparing against the
wrong baseline. Recommendation: keep 1.5/1.75 MiB in `medium-classes-wide`
(they still earn a real, statistically unambiguous win on the recycle
axis) and correct R9-4 §2.4's baseline framing to cite the warm-hit
number, not the cache-miss number. Full method, the cache-hit proof gate,
and the corrected numbers in
[`docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`](docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md).

**R10-6 (`cab6573`) — NUMA-aware segment-directory scan cliff, measured
140×, verdict CONDITIONAL-GO (measurement + design, no prototype).**
Measures the O(S) segment-scan cliff that R7/R8's directory work
eliminated for non-NUMA, but which is **still fully present** under
`--features numa-aware` — the directory-driven lookup is compiled out
there, falling back to the two-pass local-first/foreign-fallback linear
scan. Re-ran the existing R7-A0 `segment_directory_sweep` bench under
three matched feature configs on this host (NUMA scan-only vs non-NUMA
directory-ON vs non-NUMA directory-OFF) so ratios cancel host-load drift.
**Measured cliff: 524 ns / 12.8 µs / 69.6 µs at S=64/256/1023 under
`numa-aware`, vs 59 / 160 / 497 ns directory-accelerated — 140× at
S=1023, the same order of magnitude R7 eliminated for non-NUMA.**
Single-node test-host caveat documented explicitly: the measurement is a
**lower bound** (the foreign-fallback pass would only make it worse on
real multi-node hardware). Secondary finding: a fixed ~293 ns
`current_node()` syscall overhead per scan, separate from the directory
cliff, flagged as a cheaper orthogonal fix to evaluate first. Stage 2
(design) ran since the cliff proved significant: two node-aware directory
approaches, recommending Approach A (node-indexed bitmap
`class_nonempty_by_node`, ~49 KiB for `MAX_NODES=8`) over Approach B
(global directory + per-node membership filter, ~7 KiB but more complex
query logic); verified as a **strict extension** of R8-1/R8-2/R9-8's
incremental-sync, authoritative-miss, and drift-recovery machinery — does
not reopen any of it. **Verdict: CONDITIONAL-GO, no prototype this
session.** `numa-aware` is opt-in and lower priority than the
just-completed `medium-classes` workstream; recommends waiting for a real
multi-node user request or a `numa-aware` production-promotion decision.
Measurement-only: no `src/`, `Cargo.toml`, or `tests/` files touched.
Full method, raw bench logs, and the design in
[`docs/perf/R10_6_NUMA_DIRECTORY_JUDGE.md`](docs/perf/R10_6_NUMA_DIRECTORY_JUDGE.md).

**`6a11c61` — doc-only: bench-table sync + tier-2 unsafe count 33→35.**
Bundles two independent pending doc fixes, both benign: (1) the README
bench tables (churn+write, churn non-writing, cold-direct) and "Honest
verdict" bullets synced to a fresh `npm run bench:table` pass from
earlier this session, alongside a stale line-reference fix in
`scripts/bench-table.mjs` (`benches/global_alloc.rs:460-469` →
`:628-637`, the Churn+teardown diagnostic's real doc-comment location);
(2) the self-verifying unsafe-inventory line updated 17 tier-1 + 33
tier-2 → **35 tier-2** for R10-7's two new item-scoped unsafe sites
(`bump_gen` call sites in the new `HeapCore::alloc_batch`), matching the
DOCS-SYNC precedent. Verified: the canonical
`grep -rnE '^\s*#!?\[allow(unsafe_code)\]' src/ crates/` returns exactly
52 (17 + 35).

**R10-7 (`9611a56`) — tcache-aware batch primitive, verdict GO (reverses
R9-9's CONDITIONAL-NO-GO).** Refutes R9-9's CONDITIONAL-NO-GO, which was
based on an **untested inference** ("even warmed, batch would still be
slower than or comparable to" the real SeferAlloc scalar path) — R9-9
never built a warm-batch arm to check. This task builds it.

- **Part 1 (benches-only):** added two arms to
  `bench_batch_ceiling_followup` on a persistent warm `AllocCore` —
  `batch_core_warm` and a same-substrate `scalar_core_warm` diagnostic.
  Verified `refill_class_bump` drains the warm freelist first (same
  substrate `alloc_small` pops), so a warmed `AllocCore` is genuinely
  warm for the batch primitive — no forwarder needed. **Result:
  warm-batch beats the warm SeferAlloc scalar path by 1.3–3.3× at every
  (size, N) from n=8 to n=1024**, and the pure batch-mechanism win on one
  substrate is 1.5–2.2×. R9-9's inferred sign was wrong at every data
  point.
- **Part 2 (real code, justified by Part 1's numbers):** implemented the
  design a real batch API would ship — `HeapCore::alloc_batch` drains the
  warm per-thread magazine first, batch-refills only the remainder via
  `AllocCore::refill_class_bump_checked` (no block ever parked in the
  magazine); `SeferAlloc::alloc_batch` / `dealloc_batch` wrappers, all
  `#[doc(hidden)]` experimental surface (not committed public API,
  matching R8-7's `refill_class_bump` / `flush_class` precedent). 7
  correctness tests in `tests/batch_tcache.rs` (aliasing, cross-compat
  with scalar dealloc, warm steady-state cycles, null-skip, mixed size
  classes, N > `TCACHE_CAP`). **Measured: beats the real production
  scalar path by 1.1–1.6×, though 1.1–2.2× slower than the
  AllocCore-direct ceiling** (the magazine's per-block bitmap
  bookkeeping and `dealloc_batch`'s un-batched free loop are the
  honestly-documented cost of the realistic path).

**Verdict: GO for the mechanism and the experimental primitive.** The
project's no-committed-public-surface stance is unchanged — promotion
still needs a real consumer and a batch-optimized `dealloc`. This is a
measured reversal of R9-9, not a contradiction of R9-9's data: R9-9
measured a cold-batch / cold-scalar ceiling correctly, but inferred
(without measuring) the warm case; R10-7 measures the warm case and the
inference was wrong. Full method, the warm-arm grid, and the per-(size,
N) numbers in
[`docs/perf/R10_7_BATCH_WARM_ARM.md`](docs/perf/R10_7_BATCH_WARM_ARM.md).

### Round 11 — batch/NUMA correctness fixes, two closed perf cliffs, three design-only stages (R11-1..R11-8)

Round 11 — 8 commits (`33581bd`..`229e25f`, inclusive of both ends),
2026-07-21 (a single day) — the follow-up queue against the **same**
external review that produced Round 10 (that review, read line-by-line
against source immediately after Round 10 landed, surfaced two real
defects in Round 10's own deliverables plus one strong new optimization
idea; the queue below is that review's own prioritized ordering, followed
verbatim). As in Round 9 and Round 10, this is **mostly measurement /
design work, not production hot-path changes** — of the eight commits,
three ship real production-compiled fixes/features (R11-1, R11-2, R11-4),
one is a pure measurement-driven cache (R11-5), one is a mechanical
extension of already-designed machinery (R11-6), and three are
design-only docs with zero `src/`, `Cargo.toml`, or `tests/` changes
(R11-3, R11-7, R11-8). The round's defining trait, distinct from Round
10's self-correction pattern, is an unusually high proportion of **real
bugs caught during zero-trust review before landing**, not after: R11-1's
second, deeper defect (the predicate shortcut) went beyond what the
source review's own prose stated and was found only by the orchestrator's
independent re-analysis; R11-4's missing `hardened` guards and R11-6's
vacuous headline test were both caught by the same personal
red-before/green-after discipline this project's methodology requires
between phases, not by the sub-agents that wrote the code. In every one
of these three cases the pattern is the same — delegate the
implementation, independently re-verify against source and tests, catch
a real gap the delegate's own summary did not surface, fix it, and
personally reproduce red-before/green-after — and it is called out below
per-commit rather than flattened into "and then it was fixed."

**Production vs. opt-in — what actually changed for default `--features
production` users.** The production bundle's feature set is unchanged
this round: no feature is added to or removed from `production`. Three
commits reach production-compiled source:

- **R11-1 (`33581bd`)** — real correctness fix inside the `batch-api`
  experimental surface (see below), plus that surface moves from
  `#[doc(hidden)]`-only to `#[doc(hidden)]` **and** gated behind a new,
  not-default, not-`production` `batch-api` Cargo feature. Zero observable
  change for any build that doesn't opt into `batch-api`.
- **R11-2 (`7ff0772`)** — real correctness fix to `HeapCore::drain_heap_overflow`,
  the cross-thread `HeapOverflow` second-chance ring drain, which is on the
  unconditional cross-thread free path. **Beneficial** behavior change:
  reclaimed blocks now correctly become visible to directory-driven lookups
  and emptied segments now correctly re-enter pool/release accounting; no
  new observable failure mode.
- **R11-4 (`ff9ad7a`)** — adds `HeapCore::dealloc_batch`, a new fast path
  inside the same `batch-api`-gated experimental surface R11-1 covers.
  Not reachable without opting in; the scalar `dealloc` path used by every
  other feature bundle is untouched.
- **R11-5 (`9b48844`)** — adds a `cached_numa_node` field to `AllocCore`,
  compiled and populated only under `--features numa-aware` (an already
  opt-in, non-`production` bundle); zero footprint under non-`numa-aware`
  builds.
- **R11-6 (`89865ae`)** — the `SegmentDirectory`'s node dimension is
  `NODE_BITMAPS == 1` under non-`numa-aware` — byte-for-byte the
  pre-R11-6 flat bitmap, zero memory tax on non-NUMA builds (including
  `production`). Only `numa-aware` builds pay the ~55 KiB and gain the
  112× fix.
- **Everything else is design-only docs:** R11-3, R11-7, R11-8 are
  `docs/perf/*.md` (plus, for R11-3, a throwaway `examples/` probe) with
  no `src/`, `Cargo.toml`, or `tests/` file touched.

**R11-1 (`33581bd`) — close the M2 double-issue window in `alloc_batch`'s
magazine-prefix drain (real correctness fix, plus a second defect found
beyond the source review).** `HeapCore::alloc_batch`'s magazine-drain step
(introduced by R10-7) cleared each block's magazine-residency bit
immediately on pop; the refill-remainder step's predicate opened with an
`if k == c { return false; }` short-circuit copy-pasted verbatim from
`refill_magazine_slow`. That shortcut is sound **only** in
`refill_magazine_slow`'s own context, where its key invariant
(`count[c] == 0` at refill time — nothing of class `c` is claimed yet)
actually holds; `alloc_batch` violates that precondition, since its own
magazine-drain step has already pulled class-`c` blocks into
`out[0..magazine_drained]` before the predicate ever runs. Together: a
stale cross-thread double-free ring entry for a magazine-drained block
sailed through both checks and was re-issued into `out[filled..]`,
producing a **duplicate pointer within one `alloc_batch` call**. A
caller-side double-free is already a contract violation, but the M2
defense-in-depth exists to degrade it to a no-op, not amplify it into a
double issue. The source review's own prose flagged only the first half
(the immediate bit-clear); the second, deeper half — that
`alloc_batch`'s copy-pasted predicate shortcut was itself unsound in its
new context — was found by the orchestrator's own independent
re-analysis before any fix was written, and fed into the implementation
prompt explicitly so the fix would not ship incomplete. Fix, both halves
required together: (1) defer the magazine-residency bit clear to one bulk
pass **after** the refill step completes, so bits stay SET through the
window the predicate needs them; (2) drop the `if k == c` shortcut from
**`alloc_batch`'s own predicate only** — `refill_magazine_slow`'s
closure is untouched, since its invariant is genuinely sound in its own
call context. New counterfactual test
(`alloc_batch_no_duplicate_on_stale_xthread_double_free_entry`) proves
both halves are needed: with the fix reverted, it fails with a duplicate
pointer at the exact magazine/refill boundary — personally reproduced
red-before, confirmed green-after. Also resolves a documentation/
API-boundary gap the same review flagged: `#[doc(hidden)]` hides an item
from rustdoc but not from the semver/ABI surface, so `alloc_batch` /
`dealloc_batch` (on both `HeapCore` and `SeferAlloc`) now additionally
gate behind a new `batch-api` feature, not part of `production` or any
default bundle.

**R11-2 (`7ff0772`) — sync the directory and finalize pool/release
accounting on `HeapOverflow` drain (real correctness fix, two
pre-existing gaps made newly visible by R10-3, not regressions from
R10-3 itself).** `drain_heap_overflow` reclaimed blocks from the
cross-thread second-chance ring but discarded both signals every other
reclaim call site in the codebase already acts on: (1) it never synced
the segment directory, so a block genuinely freed on the `BinTable` via
`HeapOverflow` still read as absent to any directory-driven lookup — up
to **~256 MiB of wasted segment activity per class in the worst case**
before the periodic 256-miss rescan or OOM-rescue recovered it; (2) it
discarded `dec_live_and_maybe_decommit`'s `true` return, the signal to
call `release_or_pool_empty_segment`, leaking emptied segments out of
pool-cap/RSS accounting entirely. Fix is split by safety requirement, not
uniform: directory sync is done **inline**, per successful reclaim — it
only flips already-read directory bits, so it's safe immediately; pool/
release finalization is **deferred** to one bulk pass after the whole
drain completes, because a later ring entry in the same drain pass could
target a base whose metadata an inline `release_or_pool_empty_segment`
call would just have freed or decommitted. Emptied bases are collected
deduplicated into a small fixed-size on-stack array — no heap allocation
anywhere in this path, since it's allocator-internal code and a `Vec`
here would recursively call back into whatever allocator backs it. Two
new counterfactual tests
(`overflow_drain_syncs_segment_directory`,
`overflow_drain_finalizes_emptied_segment`), both proven red-before/
green-after. `sync_directory_for_segment_classes` and
`release_or_pool_empty_segment` are bumped `pub(super)` → `pub(crate)`,
mirroring the exact precedent R10-3 set for
`dec_live_and_maybe_decommit`. Also fixed along the way: a pre-existing
double-free in the directory-sync test's own cleanup path that only
surfaced under `hardened`, and an overstated doc comment mischaracterizing
the dedup array as dead code when it is real defense-in-depth against
`SegmentMeta::dec_live`'s saturating-sub clamping.

**R11-3 (`a3a31da`) — realloc-aware Small→Large promotion design for
`medium-classes`, verdict CONDITIONAL-GO, design-only.** Investigates
recovering R10-2's ~2,111× realloc regression without losing
`medium-classes`' ~31×/~211× alloc/free wins. `HeapCore::realloc`'s move
leg has no in-place fast path for a Small/medium block growing into a
*different*, larger size class, so growing a buffer through the medium
ladder pays a full-buffer copy at every class boundary crossed; the
proposed fix diverts a growing realloc directly into a Large-classified
allocation once it crosses a threshold, so every subsequent growth step
rides the existing OPT-G in-place grow for free instead of paying another
copy. Follows the same two-stage discipline R10-4 established: design and
measure first, prototype only on separate future authorization — no
shipping file is touched. A throwaway measurement harness
(`examples/r11_3_promotion_probe.rs`) gets honest numbers without
modifying `HeapCore::realloc` or `AllocCore`, by reproducing the
diversion's externally observable effect at the call site and verifying
every subsequent realloc hits the real OPT-G in-place-grow path via
pointer-identity assertion. Swept three candidate thresholds (3 runs ×
30 rounds each):

  - 128 KiB: 7→2 move legs, 2059→160 KiB copied, **28.6× faster**
  - 256 KiB: 7→4 move legs, 2059→520 KiB copied, **7.8× faster**
  - 384 KiB: 7→5 move legs, 2059→844 KiB copied, **4.1× faster**

128 KiB gives the biggest win but promotes objects that may never grow
again; 384 KiB leaves most of the win on the table. **256 KiB
recommended** as the balance point. Commit/RSS cost is real and
threshold-invariant (~+116%, 17.6→38.1 MiB for 8 concurrently-live
promoted objects) because it's driven by the pad target crossing a
segment-rounding boundary, not by the threshold itself — flagged as a
separate open tunable for stage 2, not resolved here. Re-ran the existing
R10-2 judge to confirm plain medium alloc/free is unaffected (same order
of magnitude). **Verdict: CONDITIONAL-GO for a dedicated stage-2
prototyping session.** The design shows zero new bookkeeping is needed —
`dealloc`/`realloc` already route purely off `SegmentHeader::kind_at(base)`,
not the caller's `Layout`, so a promoted block becomes an ordinary Large
allocation the instant it's promoted, no new feature flag required. Full
method and numbers in
[`docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`](docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md).

**R11-4 (`ff9ad7a`) — batch-optimize `dealloc_batch`, a real gap caught
and fixed during zero-trust review before landing.** `dealloc_batch`
previously just looped the scalar dealloc path one block at a time,
paying N independent TLS-adjacent lookups and, on magazine overflow, N/8
separate half-flushes each re-deriving the same per-run segment metadata.
The new `HeapCore::dealloc_batch` (`src/registry/heap_core_dealloc_batch.rs`)
classifies the layout once, then for the Small-classified/fastbin case
partitions blocks into this-heap-owned (the same `contains_base`
ownership test the scalar path uses) vs. everything else (foreign,
cross-thread, null), which falls back unchanged to the existing, fully
correct scalar path. Owned blocks fill the magazine directly up to
`TCACHE_CAP`; any overflow routes through one `AllocCore::flush_class`
call instead of the scalar path's dribble of 8-block half-flushes. No M2
guard logic is reimplemented — the fast path calls the identical
`pub(crate)` accessors the scalar path already uses, in the same order.
**Zero-trust review caught a real gap before this landed**: the fast
path's ownership gate (`contains_base`) does not distinguish Small from
Large segments, so it initially omitted two `hardened`-only guards the
scalar path applies before its M2 oracles — F7 (a pointer that actually
lives in a Large segment, freed via a Small-classified layout) and H1 (an
interior, non-block-start pointer). Without them, a caller-contract-
violating free through `dealloc_batch` specifically would read/write a
Large block's own payload bytes as if they were a Small segment's bitmap
under a `hardened` build (part of `--all-features`) — exactly the
corruption F7's own doc comment exists to prevent. Both guards are now
ported in the scalar path's exact order (F7, then H1, then the three M2
oracles), with two new counterfactual tests
(`tests/r11_4_dealloc_batch_hardened_guards.rs`) proving each, personally
reproduced red-before (both guards) and green-after before committing.
Also fixed during the same review: the scalar fallback for non-owned
blocks initially reconstructed a `Layout` from `block_size(c)` with
`align=1` instead of threading the caller's original layout through —
under `alloc-xthread` this could tag a cross-thread free's ring entry
with the wrong class, since `class_for` is alignment-sensitive; fixed by
passing the original layout unchanged. Two more counterfactual tests
cover the base mechanism
(`tests/r11_4_dealloc_batch_same_segment_double_free.rs`,
`tests/r11_4_dealloc_batch_mixed_ownership.rs`), all proven red-before/
green-after. **Measured 1.16×–1.38× faster at the realistic bulk-free
target (n=1024)** across three sizes (release,
`production+alloc-stats+batch-api`); small batches (n=8–32) are noisy/
mixed, as expected, since the fast path's per-block checks only pay off
once `flush_class` batching actually triggers past `TCACHE_CAP`.

**R11-5 (`9b48844`) — cache `current_node()` on `AllocCore`, ~233×/~396×
measured on this host.** `numa::current_node()` was called fresh on every
`find_segment_with_free` miss plus at every new small/large segment
reservation. Its platform implementations are not cheap: Linux loops over
up to 64 candidate NUMA nodes, opening and reading a sysfs cpumap file
for each one; Windows makes two Win32 API calls
(`GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`) — real kernel
transitions either way, paid on every miss rather than once per process.
Adds a `cached_numa_node: Option<u32>` field on `AllocCore`, populated
lazily and consulted from all four hot call sites. Because registry slots
are recycled across different OS threads
(`HeapRegistry::claim`/`recycle`), an unconditional cache would be wrong
— a stale node from a slot's previous owner would silently apply to a new
owning thread for the entire lifetime of its claim. The cache is
invalidated uniformly at `claim()`/`claim_with_config()` time, immediately
before the slot is handed to its new owner; soundness rests on the same
claim/recycle CAS handoff (Release on recycle, Acquire on the next claim)
the registry already establishes, so a plain field write is sufficient —
no extra atomic or fence needed. `docs/PHASE_NUMA_DESIGN.md` gets a new
§4.1 documenting the invalidation policy and the resulting staleness
bound: a migrated thread's reads may now lag the OS's real answer for the
duration of the current slot claim — performance-only staleness, never
UB. New regression test (`tests/numa_cache_invalidation.rs`, gated on a
new test-only `numa-aware-mock` feature) scripts one NUMA node, populates
the cache, recycles the slot, scripts a different node, re-claims, and
asserts the cache is invalidated before any populate — proven
red-before/green-after by temporarily disabling the invalidation call
sites and confirming a stale value leaks across claims. **Measured on
this host (Windows, single-NUMA): ~230ns → ~985ps per call (~233×),
~227µs → ~573ns for a batch of 1024 calls (~396×).** Also fixed along the
way: `numa-shim`'s mock call-log recorder used `borrow_mut()`, which
panics on reentry (the mock's own `Vec::push` allocates via the global
allocator, re-entering `current_node()` when sefer-alloc is the installed
global allocator) — switched to `try_borrow_mut()`, silently dropping
only the reentrant log entry.

**R11-6 (`89865ae`) — node-indexed NUMA segment directory closes the 140×
scan cliff R10-6 measured, verdict implemented (GO), one vacuous test
caught and fixed before landing.** Implements R10-6's already-designed
Approach A now that R10-6's own GO trigger ("cache shipped AND cliff
still dominant") is satisfied by R11-5. `SegmentDirectory` gains an outer
node dimension, `class_nonempty_by_node[bucket][class][word]`; under
non-`numa-aware`, `NODE_BITMAPS == 1` — byte-for-byte the pre-R11-6 flat
bitmap, zero memory tax. Under `numa-aware`, `NODE_BITMAPS == MAX_NODES +
1` (8 + 1 for a dedicated "unknown node" bucket, ~55 KiB total). The
directory-driven lookup, previously compiled out entirely under
`numa-aware`, is now unconditional, scanning buckets in
local → unknown → foreign-ascending order — preserving the two-pass
local-first/foreign-fallback preference the R7 plan binds as a hard
constraint. Candidate validation was extracted into one shared
`validate_directory_candidate` choke point so NUMA and non-NUMA scans use
byte-for-byte identical criteria; R8-2's authoritative-miss trust and
R9-8's streak/rescue-scan machinery are untouched — only the directory
block's own cfg gate changed. **Zero-trust review caught a real gap
before this landed**: the headline correctness test (local-first/
foreign-fallback preservation) passed even against a **deliberately
broken** bucket-scan order. Root cause: `alloc_small` tries
`pop_free(self.small_cur)` before ever calling the directory-scan
function, and the test's construction left `small_cur`'s final state to
chance — the decisive alloc call most likely resolved via that fast
path, never touching the directory the test claimed to exercise, making
the test vacuous with respect to what it claimed to prove. Fixed by
adding a `#[doc(hidden)]` test hook that calls the directory-scan
function directly, bypassing `alloc_small`'s fast path entirely, so the
directory's bucket order is unconditionally the deciding factor.
Personally reproduced red-before (revert the bucket order to naive
descending — the fixed test now correctly fails, returning the foreign
node instead of local) and green-after before committing. **Re-measured
fresh** (`benches/segment_directory_sweep.rs`, same harness R10-6 used):

  - S=64:    284ns → 72ns    (3.9×)
  - S=256:   12,218ns → 176ns (69×)
  - S=1023:  62,866ns → 560ns (112×)

The curve is now flat in S (O(1)), matching the non-NUMA directory-
accelerated numbers plus the R11-5 cached `current_node()` residual. The
cliff is closed.

**R11-7 (`c22807d`) — page-run layer design for the 1.25–2 MiB density
gap, verdict CONDITIONAL-GO, design-only, the largest structural change
in the queue.** R10-4's own design sketched a "page-run layer" — a
larger, dedicated arena for the `medium-classes-wide` 1.25/1.5/1.75 MiB
range — as the real long-term fix for the density gap R8-9, R9-4, and
R10-4 all independently found (these classes pack near-1× in a standard
4 MiB segment). Follows the same two-stage discipline as R10-4 and
R11-3: design-doc first, prototype only on separate future
authorization. Re-verifies the density win against real constants read
fresh this session: an 8 MiB arena delivers density **5/4/3/3** for
1.25/1.5/1.75/2.0 MiB (vs today's 2/1/1/1 — 2 MiB isn't a class today,
R9-4 explicitly excluded it for exactly this reason); a 16 MiB arena
delivers 11/9/8/7 but doubles per-arena commit cost for a workload that
may only populate a few blocks. **Recommends the single fixed 8 MiB
arena** over both the larger 16 MiB option and per-class-tier arena
sizing. Does the exhaustive due diligence the task required: every
`SegmentKind::` call site inventoried and classified (**44 matches across
17 files**), and a systematic interaction check against every
prior-session mechanism that assumes segment-uniformity (M2 bitmaps,
`RemoteFreeRing`/`HeapOverflow`, the R7–R11-6 segment directory, the NUMA
node-indexed directory, the decommit large-cache and empty-segment
pool) — **6 of 11 need a genuinely new parallel mechanism, only 2 are
reused as-is.** Corrects R10-4's own one-line framing that the page-run
layer needs "zero guard-invariant changes": true for carve alignment, but
address resolution needs a second masking constant and a two-step
disambiguation, since `segment_base_of_ptr`'s O(1) masking is calibrated
to the global `SEGMENT` constant — worked through concretely (a parallel
`PageRunTable`, a dedicated `PageRunFreeRing` with a wider packed-offset
field: 8 MiB needs 23 bits vs `SEGMENT`'s 22, confirmed to still fit the
existing `u32` packing with headroom, though the `hardened` ring's exact
bit budget is flagged as not fully derived — an explicit open item for
stage 2, not silently assumed to work). **Verdict: CONDITIONAL-GO**, but
explicitly states the true design surface is closer to "a second,
smaller segment-table subsystem living alongside the existing one" than
a bounded patch — roughly 2–3× the correctness-surface size of R10-4's or
R11-3's own designs, comparable in total scope to the original
`medium-classes` build-out, not a single-session task. Six explicit open
questions left for a future stage-2 session, not silently assumed. Full
inventory and interaction table in
[`docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md`](docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md).

**R11-8 (`229e25f`) — virgin-zero skip for Small `alloc_zeroed`,
independent re-verification of a prior session's design, verdict
unchanged (CONDITIONAL-GO), design-only.** Deliberately ordered last by
the source review ("big potential, but harder to prove") — this is
correctness-critical in a way none of the round's other perf work is: a
wrong implementation would return **uninitialized memory** from
`alloc_zeroed`, a direct `GlobalAlloc` contract violation with
security-relevant implications, not merely a regression. Discovered
before writing anything: this exact topic already has a complete,
committed design doc from an earlier session
(`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md`, commit `7bdbc0f`,
2026-07-20), itself a reconciliation of an earlier NO-GO and a
deep-audit's "medium risk, unresolved" rating, reaching a design-only
CONDITIONAL-GO. Rather than silently duplicate or blindly trust that
prior work, this task's own doc
(`docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md`) **independently
re-verifies R9-5's conclusions from the current tree** — re-reading every
cited call site fresh rather than trusting citations (the substrate has
moved since R9-5: `alloc_core_small.rs` grew from ~2100 to 2267 lines) —
and adds what R9-5 didn't produce: a formal four-conjunct testable
predicate (`is_virgin(segment, offset, carve) :=` dispatch-is-fresh-carve
AND offset-in-this-carve's-range AND
segment-never-decommit-recommitted AND `not(miri)`), a full verification
ledger tracing five preliminary hypotheses against source, and this
round's kill-gate table format. **The verdict does not differ from
R9-5's**: all ten correctness/soundness kill-gate criteria pass (pooling
can never be marked virgin, lazy-commit grow-on-carve is
zero-guaranteed, decommit-in-place has zero production callers today and
the design sets its tracking bit defensively so a future reintroduction
fails safe, batched carve and magazine-refill interleaving both get
correct per-run granularity, cross-thread frees never write block bytes,
the `hardened` generation-bump writes disjoint metadata not the payload,
Miri safety mirrors the Large-path's proven `cfg!(not(miri))` fix
exactly) — **CONDITIONAL-GO for staged future work, NO-GO for a
same-session prototype.** The CONDITIONAL qualifier is entirely about
production reach and win narrowness, not correctness:
`HeapCore::alloc_zeroed`'s small arm still bypasses
`AllocCore::alloc_zeroed` entirely (calls `self.alloc` + unconditional
`Node::zero`), so a substrate-only prototype would be fully testable but
production-inert under any `production`/`fastbin` build — the real
implementation surface is plumbing the virgin signal through the
magazine refill path, which has a genuinely open storage-design question
(per-slot bool array vs. a whole-class short-circuit bit vs. a stolen
pointer tag, especially its interaction with `hardened`'s tagged-pointer
scheme) that neither this document nor R9-5 resolves. The win itself is
real but narrow: benefits only genuinely-first-touch,
never-reused `alloc_zeroed` calls, zero benefit on the steady-state churn
patterns the rest of this round's work targets.

## [0.3.0] - 2026-07-04

0.3.0 is the first `0.3.x` release (the current crates.io live version is
`0.2.1`; see the yank notes below). It bundles four workstreams, each
implemented with line-by-line zero-trust review, per-fix counterfactual
verification, and a commit between phases: the **P0–P7 perf arc**
(#144–#163, beat `mimalloc` on small/medium), a **reliability, stress &
release-doc pass** (R1–R4 / S1–S3 / D1, #153–#168), **two post-tag review
passes** (#164–#178 — a hardening/H1 pass then a perf/reliability/CI pass
W1–W6, both driven by fresh `/fxx` audits with per-fix counterfactuals), the
**post-review hardening pass** (#129–#143), and the **initial phase A–F pass**.
Sections below are grouped per workstream.

### Performance & correctness — the X-arc (#182–#188, 2026-07-05/06)

The post-W7 arc that attacked the last "cardinal" costs found by a fresh
audit. Judge-driven end to end: every change was measured by the
deterministic callgrind judge (`npm run iai`) against a pinned reference
table, adversarially reviewed, and either kept with numbers or
honest-rejected with numbers (four experiments were rejected — the ledger in
[`docs/perf/IAI_BASELINE.md`](docs/perf/IAI_BASELINE.md) records all
tables so no experiment is re-run blind).

- **X1 — OPT-G in-place Large→Large realloc growth (#182).** When the grown
  size (clamped to `MIN_BLOCK`, symmetric with the #138 consistency check)
  still fits the segment's committed `span_usable`, `realloc` updates the
  header's `large_size` and returns the SAME pointer — zero alloc/copy/
  dealloc. Large reservations round up to whole 4 MiB segments and `vmem`
  commits the entire span, so growth cannot fault; `dealloc` routes Large
  frees by segment kind, so the grown block frees correctly. Shrinks still
  take the slow path (RSS reclaim preserved). An adversarial review caught
  (and a counterfactual test now pins) a MIN_BLOCK-clamp leak the first cut
  had. `realloc_grow`: **1,520,714 → 617,859 Ir**.
- **X2 — #164 narrowed: drain-side magazine check (#183).** The ring↔magazine
  cross-thread double-free residual was closed on its *in-magazine leg*: the
  owner's ring drain now consults an `is_in_magazine` predicate (generic
  closure threaded from `HeapCore` via split borrows) immediately before
  linking, on ALL production drains — refill-miss, the realloc alloc-leg
  (rerouted through the magazine-aware `HeapCore::alloc`; the blind path was
  found by adversarial review), and the dbg seam. A magazine-resident block's
  ring entry is dropped; the magazine copy stays canonical. The *re-issue-
  before-drain* leg is **proven** information-theoretically indistinguishable
  from a delayed genuine cross-thread free (design doc §8 impossibility
  postscript) — full closure needs generational ring entries (X7, hardened,
  future arc). Costs accepted and documented: +~630 Ir one-time bootstrap
  per heap claim, ~+30 Ir per refill-miss; hot magazine push/pop untouched.
  Bonus: `realloc_grow` → **561,912 Ir** (the alloc-leg now hits the
  magazine). loom green model + two new counterfactual regression tests.
  - **Correction (R1, 2026-07-06):** the X2 fix as originally shipped left a
    SECOND, decidable leg open — the **refill-window in-out-buffer** leg.
    `refill_class_bump_impl` pulls freelist blocks into `out[0..filled]`
    BEFORE draining rings; the predicate's `if k == c { return false; }`
    shortcut (justified only by count[c]==0 borrow-safety) was blind to those
    magazine-destined blocks, so a stale ring note was reclaimed → relinked →
    the SAME refill loop re-pulled the block → double-issue at consecutive
    positions. Task R1 closed it by wrapping the predicate with an
    out-membership guard (`is_in_magazine(ptr,k) || (k == c &&
    out[..filled].contains(ptr))`) — zero cost when the ring is empty.
    Counterfactual regression test:
    `refill_window_does_not_double_issue_in_out_buffer_resident_block`
    (reverting the guard → P double-issued at positions [14, 15]). The §8
    impossibility theorem is now correctly scoped to leg 3 only (re-issue-
    before-drain); the taxonomy is three legs, not two.
  - **Cleanup (R2, 2026-07-06):** the X-arc retrospective (C2) flagged
    `AllocCore::realloc` as production-dead yet carrying a full duplicate of
    the OPT-F/OPT-G in-place logic also present in `try_realloc_inplace` —
    an unmarked divergence hazard. Resolved by extracting the shared
    detection into one private helper, `realloc_inplace_fast_path`, called
    by both `AllocCore::realloc` (substrate-level fallback to its own
    alloc+copy+dealloc) and `try_realloc_inplace` (the `alloc-global`-gated
    thin wrapper `HeapCore::realloc` consumes). Single source of truth; no
    behaviour change. The same pass rewrote `HeapCore::realloc`'s doc
    comment, which still described the pre-#164 "delegate to
    `self.core.realloc`" flow, to match the actual body (try_realloc_inplace
    → `HeapCore::alloc` + copy + `HeapCore::dealloc`), and replaced a dead
    `if p != ptr { stamp }` branch (unreachable: `try_realloc_inplace`
    always returns the same pointer) with a `debug_assert_eq!`. MUST-1/A1
    and #169 stamp semantics unchanged; both invariant-guarding suites
    (`regression_realloc_xthread_stamp`, `regression_inplace_large_realloc`)
    stayed green without assertion edits.
- **X3 — judge upgrade (#184).** `scripts/iai.mjs` now surfaces the full
  callgrind metric set (Ir | L1 | L2 | RAM | Estimated Cycles) — Ir counts a
  `udiv` and a cache-missing load identically, cycles do not; the X-arc's own
  memcpy story is the proof (realloc_grow Ir −63% but cycles −47% with RAM
  hits 92,240 → 74,963). New `multiseg_cold_256k` bench (3-segment scan
  judge, seeded for future segment-queue work). `docs/perf/FAULT_PROBE.md`
  records the honest negative verdict on a WSL2 page-fault judge.
- **X4/X5/X6 — four honest-rejects with full tables (#185–#187).**
  Magazine CAP 16→32 (every bench regressed, recycle +32,305 — the target
  itself); a 64-bit bloom gating the M2 in-magazine scan (recycle −19k but
  churn +980 — the won front is not traded); clz `class_for` vs the 16 KiB
  SIZE2CLASS LUT (bitwise-identical over 8.28M pairs, but Estimated Cycles
  regressed on 10/11 benches); a per-segment free-classes bitmap for the
  segment scan (every bench regressed incl. the designated judge). All four
  experiments' mechanisms and revisit-triggers are in the ledger.
- **X-arc headline:** `realloc_grow` **1,520,714 → 561,912 Ir (−63 %)** and
  **7,206,236 → 3,817,567 Estimated Cycles (−47 %)**; all other benches within
  documented cold constants of their pre-arc values; every M2/D1 guarantee
  intact and one double-free leg newly closed. X7 (hardened generational ring
  entries — the only path to the remaining, proven-undetectable double-free
  leg) landed as a follow-up arc; see the "X7" subsection below.

### Hardening — the X7 generational-ring arc (#188–#193, 2026-07-06)

The X-arc closed the *in-magazine* and *refill-window* legs of the cross-thread
double-free residual (X2 #164, R1). The third and final leg — *re-issue-before-
drain* (a block popped from the magazine and re-issued before the owner's lazy
drain catches a stale cross-thread-free note) — is information-theoretically
indistinguishable from a genuine delayed free on the bare `GlobalAlloc`
interface. X7 closes it under `--features hardened` via a per-granule
generation counter: the ring note now carries the block's generation at
remote-free time, and the drain drops a note whose generation no longer matches
the block's current life. Delivered in five phases (Ф1–Ф5), each committed
between phases with a zero-trust review and a production-judge gate.

- **Ф1 (`cdc3361`, #189) — gen table in segment metadata.** A 256 KiB table of
  `AtomicU8` (one byte per `MIN_BLOCK = 16` granule, `#[cfg(feature =
  "hardened")]`-gated) carved into the segment metadata region, below
  `small_meta_end`. Not decommitted with the payload → numbering is continuous
  across decommit-reset; dies only with full segment release. Byte-level
  `gen_at`/`bump_gen` accessors (Relaxed load / `fetch_add(1, Relaxed)`). Miri-
  clean (exposed-provenance standalone-buffer tests). Production-judge 11/11
  byte-identical.
- **Ф2 (`345a2ce`, #190) — hardened ring-entry repack.** The ring's `u32` slot
  entry repacks under hardened to `[gen:8|class:6|off16:18]` (was
  `[off:22|class:10]`). Const-asserts pin the bit layout (sum == 32, gen == 8);
  the `RING_SLOT_EMPTY = u32::MAX` non-collision is structurally guaranteed
  (`class=63` is unreachable: `SMALL_CLASS_COUNT = 49 < 64`). Round-trip +
  field-independence + misalignment-guard regression tests. Non-hardened path
  byte-identical.
- **Ф3 (`d1e91ff`, #191) — the three touches.** (a) issue bumps the gen
  (`bump_gen` at magazine pop + `pop_free`); (b) remote free stamps the current
  gen into the note (`dealloc_routing` Variant-2); (c) drain compares, AFTER all
  existing guards, BEFORE `write_next`: mismatch ⇒ drop. The pinned-red
  `#[ignore]` test `residual_xthread_double_free_no_corruption` (scenario
  A→B→I→D) turns GREEN under `hardened` — the pinned bug becomes the feature
  proof. loom model + `should_panic` counterfactual; production-judge 11/11
  byte-identical.
- **Ф4 (`3b0ed2c`, #192) — lifecycle-seam tests.** Pins the three seams the gen
  table touches: (1) decommit-reset continuity (the table is NOT re-zeroed —
  numbering persists; fresh segments ARE zeroed by `init_gen_table_in_place`);
  (2) recycle/release drops stale notes via the EXISTING `contains_base`/
  `magic_at` guards (the gen table is unmapped before any post-recycle read);
  (3) adopt/abandon — the table travels with the segment unchanged (`abandon`
  touches only `owner_state`, never metadata bytes).
- **Ф5 (#193) — honest costs, wrap boundary, docs sync, final runs.** This
  phase. (a) Published the hardened-tier cost in
  [`docs/perf/IAI_BASELINE.md`](docs/perf/IAI_BASELINE.md): marginal per-op
  cost is **+0.2–0.8% Ir** on the magazine hot path (the per-issue `bump_gen`
  RMW), **+2.6%** on refill-miss paths, plus a one-time **~262k Ir bootstrap**
  per heap-claim (gen-table zeroing) — the published price of the defence-in-
  depth feature (plan §5: "порога 'не хуже' нет — это осознанная плата за
  защиту"). (b) Wrap-1/256 boundary test
  (`tests/regression_gen_wrap_boundary.rs`): pins the EXACT 256-modulus of the
  accepted residual — `stamped_gen == current_gen` is TRUE at k=256 bumps
  (collision), FALSE at k=255/257, const-derived from `ENTRY_GEN_BITS == 8`.
  (c) Docs sync: `DURABILITY.md` (+gen counter inventory row, accepted-residual
  verdict category), `RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8.4 (→
  IMPLEMENTED), `FASTBIN_DESIGN.md` residual banner (→ CLOSED under hardened).
  (d) Final loom/miri runs green across both profiles; TSan deferred to CI on
  push (Linux-only, not runnable on the Windows dev host).

**Residual after X7:** leg 3 (re-issue-before-drain) is closed under
`--features hardened`. The only remaining leak is the **1/256 wrap** (≥256
re-issues of one block without an intervening drain → the stamped gen
coincidentally matches the current gen mod 256) — an accepted probabilistic
residual by design (plan §2.5 rejected doubling the ring footprint for a `u64`
note), pinned to its exact modulus by the Ф5 boundary test. The production hot
path is byte-for-byte untouched (every X7 code path is behind the hardened
cfg). Full phased account:
[`docs/design/X7_GENERATIONAL_RING_PLAN.md`](docs/design/X7_GENERATIONAL_RING_PLAN.md).

### Performance — the P0–P7 "beat mimalloc on small/medium" arc (#144–#163)

A seven-phase perf campaign against `mimalloc` on the two fronts where 0.3.0
lost: cold first-touch of tiny blocks (16–64 B) and 256 B churn. The governing
rule was **every speedup removes a *tautology*, never a *guard*** — no
correctness guarantee was surrendered (M2 exact double/foreign-free no-op, D1
live-count accuracy, A1 cross-thread reclaim, `#![forbid(unsafe_code)]` by
default with `production` = `#![deny(unsafe_code)]` + 8 named seams — all
intact — M2's exact-no-op scope being the live/mapped,
single-legged free, with the cross-thread-double-free ring-in-flight case a
pre-existing documented residual limit, #164); in P6 the M2 guard was
**strengthened for the two own-thread resting places** (magazine + BinTable,
see Э6 below). Each phase was implemented, line-by-line zero-trust reviewed,
counterfactually verified, and committed between phases. See
[`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`](docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md)
for the full diagnosis and
[`docs/ALLOC_BENCH.md`](docs/ALLOC_BENCH.md) for the P0→P5 measurement tables.

The six eurekas that landed (P1–P3, P6):

- **Э1 (P3) — bump-direct batched carve — front A's main lever (#147).** A
  freshly bump-carved block already satisfies the M2 bitmap invariant
  (`bit 0 = allocated`); the old refill drove every virgin block on a
  `carve → write_next → bitmap RMW → head-store → pop → read_next → bitmap RMW`
  round-trip through the `BinTable` only to move it to "free" and instantly
  back to "allocated" — a tautology (~40 instructions/block). New
  `AllocCore::refill_class_bump` carves a batch straight from the bump cursor
  into the magazine (`bump += n·block_size`, `live_count += n`) **without
  touching the bitmap** (bit 0 is already correct), ~6–8 instructions/block.
  Source order preserved: freelist / cross-thread ring-drain are still tried
  BEFORE bump-carve, so freed blocks never go stale (no RSS drift). M2
  byte-identical (a double-free of such a block still `mark_free`s, and the
  second free still sees "already free" → no-op); D1 exact (same batch inc).
  The P7 alloc-side bulk-bypass became unnecessary and was retired (the
  dealloc-side bulk-flush is kept). This roughly halved the cold tiny-block
  gap and brought cold 256 B to parity.
- **Э2 (P1) — one-branch teardown resolver (#145).** After #129 every alloc
  compared `p == TORN` (`usize::MAX`) and `p == null` (`0`) — two branches on
  the process's hottest path for a once-per-thread teardown case. Since the
  two sentinels are the range ends, one compare
  (`p.addr().wrapping_sub(1) < usize::MAX − 1`) catches both; the cold split
  (`0 → bind_slow`, `MAX → Fallback`) only runs off the fast path. Semantics
  identical (same #129 counterfactual test), minus a branch.
- **Э4 (P1) — classify once (#145).** `class_for` was recomputed 2–3× per
  alloc and 2× per free; the class `c` (a pure function of size+align) is now
  threaded once through the path (the magazine miss resolves `c` and hands it
  straight to `refill_class_bump(c, …)`; the dealloc overflow resolves `c` once
  and passes it to `flush_class` / `dealloc_small(base, ptr, c)`), removing 1–2
  loads from the 16 KiB `SIZE2CLASS` table plus branches per op. (P1 introduced
  thin `alloc_small_class` / `dealloc_small_class` wrappers for the bulk-bypass
  callers; P3 retired those wrappers with the P7 bypass, but the classify-once
  threading they enabled survives on the live refill/dealloc paths.)
- **Э5 (P1) — a counter that doesn't count (#145).** The per-hit
  `tcache_hits.fetch_add` was a `lock xadd` even after #133 removed the
  *contention* (the owner is the sole writer). Replaced with a
  `load(Relaxed); store(+1, Relaxed)` pair — same atomic visibility for
  `stats()`, no lock prefix. TSan/miri-clean.
- **Exact 256 B size class (P1, #145).** `SMALL_CLASS_COUNT` 48 → 49 adds an
  exact-256 B class (the public size-class type has been a `&'static [..]`
  slice since #136, so this is not a breaking change). This narrows — but does
  not close — the 256 B churn gap.
- **Э6 (P6) — oracle-in-metadata: the 256 B churn loss is ELIMINATED, and M2
  got STRONGER (#150–#152).** The P5 docs blamed the residual 256 B loss on
  "the M2 bitmap price"; that framing was incomplete. The real cost was a
  stale per-heap key (`TCACHE_KEY`) stamped into the freed block's **body**
  (word1) and read back as a magazine double-free fast-path filter. On the
  non-writing churn bench the key survived across the free, forcing a
  slow-path scan on every free AND touching a cold/conflict cache line at the
  256 B stride (the "256 B churn loss" — never the bitmap itself). Э6 removes
  `TCACHE_KEY` entirely: the two exact oracles (in-magazine array scan + the
  `BinTable` `is_free` bitmap line — both hot metadata) now run on every free
  with no block-body filter, and **the free path never touches the block
  body**. This is not a trade — M2 is **strengthened for the two own-thread
  resting places (magazine + BinTable)**: the pre-Э6 flushed-double-free-
  after-user-write hole (a double-free after the user overwrote word1 could
  double-issue) is now CLOSED, because the oracle no longer depends on
  block-body contents. **The cross-thread-double-free ring-in-flight case
  remains a documented residual limit (#164):** the oracles are blind to a
  block whose cross-thread free is still undrained in its segment's
  `RemoteFreeRing` (the ring push sets neither oracle), so an own-thread free of
  such a block still slips through — pre-existing since fastbin, neither opened
  nor closed by Э6, pinned RED by
  `tests/regression_xthread_double_free_residual.rs`. Counterfactual proof: `tests/regression_magazine_oracles.rs`
  test (c) is RED pre-Э6, GREEN on Э6. Bonus: our free path is now cheaper than
  mimalloc's on this pattern — mimalloc writes `next` into the block body on
  every free; we write nothing to it. Cold carve is untouched (Э6 targets only
  the churn free path).

The P7 arc (P7.0–P7.4, #159–#163) — an **instruction-count** optimization of
the steady-state cold recycle path (the freelist round-trip P7.0 isolated —
NOT page faults; at criterion steady state the instance is reused, so the cost
is per-block metadata ceremony on the refill/flush path). Five more eurekas,
each proven **byte-identical** by counterfactual regression tests:

- **Э7 (P7.2) — batch freelist drain in `refill_class_bump`, the main cold
  lever (#161).** One segment's freelist is drained in a **single walk**: the
  head-read, `set_head`, and `inc_live` are hoisted out of the per-block loop
  (one head-store + one live-count update for the whole run). The genuinely
  per-block work stays per block: the dependent `read_next` load and the
  `mark_alloc` bitmap RMW (the M2/D1 guards) still run once per block. The
  drained blocks are byte-identical to the per-block loop's output.
- **Э8 (P7.3) — batch flush in `flush_class` (#162).** Symmetric on the dealloc
  side: same-segment runs flush in one pass with `set_head` and the bump-load
  hoisted out of the loop. Every guard stays per block: `is_free`, `off >= bump`,
  and `dec_live` all still run once per flushed block — no guard collapsed,
  only shared head/bump bookkeeping pulled out.
- **Э9 (P7.1) — classify-once + base-once on the `HeapCore` alloc/free faces
  (#160).** A duplicate `class_for` and `segment_base_of` per op were removed —
  both are resolved once and threaded through. Same values, fewer loads; both
  sides win, risk ~0.
- **Э10 (P7.4) — branchless chunked in-magazine M2 scan (#163).** The
  in-magazine double-free oracle (the Э6 array scan) is now a branchless
  chunked scan — same exact membership test, no per-element branch. M2
  membership is byte-identical; the scan bounds are counterfactually pinned.
- **Э11 (P7.2) — stamp-dedupe (#161).** A redundant owner-stamp on the batched
  drain path was de-duplicated (stamped once for the drained run, not per
  block). Same stamp result.

Э3 (P2, own-segment cache) was implemented and gated but is honestly modest
(the win is skipping the probe arithmetic + a likely L1 miss; `contains_base`
was already O(1)); it does not move the headline tables.

### Measured result (single noisy Windows dev host, criterion FAST profile — ratios are the signal)

- **Cold tiny blocks (front A) — the big win.** 16 B `2.6× → 1.60× slower`;
  64 B `2.0× → 1.15× slower`; cold 256 B reached **parity** (1.03×). Not full parity
  on the tiniest cold sizes, but the tautological carve→BinTable→pop round-trip
  is gone — what remains is honest per-block work (page-map writes, page faults
  on genuinely fresh pages).
- **Churn tiny blocks — lead widened.** 16 B `1.26× → 1.63× faster`; 64 B
  `1.23× → 1.69× faster` (Э2 + Э4 + Э5 compounding on the hit path).
- **256 B churn (front B) — the loss is ELIMINATED (Э6, P6).** Through P5 the
  exact-256 B class only narrowed this from `1.25× → 1.16× slower` and never
  overtook. Э6 removed the real cause (the stale block-body key, not the
  bitmap): on the artificial **non-writing** pattern 256 B churn reached
  **≈ parity** (`~1.03×`, was 1.16–1.25× SLOWER), and on the realistic
  **writing** pattern (`global_alloc_churn_write`, new in P6.0 — real code
  writes to what it allocates) sefer-alloc now **leads at every size**:
  16 B 1.63×, 64 B 1.69×, **256 B 1.14× faster**, 1024 B 5.42× faster. The
  earlier "honest ceiling" framing (256 B is the M2 bitmap price) is retired —
  the price was a per-heap key in the block body, and it is gone.
- **Cold tiny (16–64 B) — unchanged, still trails 1.15–1.60×.** Э6 does not
  touch the cold carve path (page-fault-bound honest per-block work); no claim
  of improvement there.
- **Large (≥1 KiB) — the crushing lead is retained.** Cold 1.84× faster,
  churn 5.42× faster (writing) / retained; the OPT-E large-cache headline
  (13–34× at 4/16/64 MiB) is unchanged.
- **P7 cold recycle — an instruction-count reduction; wall-clock MODEST and
  within noise on this host (no overclaim).** P7 batches the freelist
  drain/flush (Э7/Э8), classifies once (Э9), and makes the M2 scan branchless
  (Э10) on the steady-state cold recycle path. On this noisy single-host
  wall-clock the cold-tiny numbers moved only within run-to-run noise: 16 B
  `1.60× → ~1.5× slower`, cold 256 B `parity → ~1.06× faster`, 64 B unchanged
  (`~1.15×`) — the 16 B row alone spanned 18–24 µs across samples. **We do NOT
  claim the plan's projected ~1.1–1.2× cold-tiny figure as achieved** — the
  wall-clock on this machine cannot cleanly resolve the per-op instruction
  savings. The real, DETERMINISTIC proof is the iai `Ir` gate on Linux CI (see
  the `recycle_*` benches below); the P7 cold verdict is **pending that gate**.
  Churn (the won front) is **UNREGRESSED** (16 B still ~1.6× faster, 256 B
  still ≈ parity). Guarantees intact: the batching removed only shared-
  bookkeeping tautologies and kept every per-block guard (`is_free`,
  `off >= bump`, `mark_alloc`, `dec_live`); M2 / D1 / A1 /
  `#![forbid(unsafe_code)]` by default (`production` = `#![deny(unsafe_code)]`
  + 8 named seams) all hold.

The rigorous, DETERMINISTIC proof is the `perf_gate_iai` instruction-count
gate (Valgrind, Linux-only CI): the P0 benches
(`cold_alloc_free_256x16b` / `_256x64b`, `churn_256b`, #144), the P6
`churn_write_256b` bench (#150), and the P7.0 two-round
`recycle_alloc_free_256x16b` / `_256x64b` benches (#159 — round 2 drains what
round 1 freed, isolating exactly the Э7/Э8 recycle path the single-round
`cold_*` benches are blind to) exist for exactly this and confirm the per-op
`Ir` deltas; their `Ir` baseline is captured on the first Linux perf-gate run.
The P7 cold verdict specifically is **pending this Linux Ir gate** — the
wall-clock numbers above are noisy comparative measurements from a single
noisy Windows dev host, not a statistical suite.

### Reliability, stress & release-doc pass (R1–R4, S1–S3, D1 — #153–#168)

A post-perf pass that hardens the guarantees, adds adversarial boundary
coverage, and reconciles the release docs — strictly from the safe
`GlobalAlloc` envelope (each block freed exactly once, same layout; misuse
from `unsafe` callers is out of scope). No correctness guarantee was
weakened; M2 was *strengthened* in R1.

#### Fixed

- **R1 — the magazine-push `off >= bump` guard closes a real M2 gap.** The
  Э6 in-magazine free path could push a not-yet-carved (`off >= bump`)
  offset into the per-thread magazine, from which a later alloc could hand
  out a block the substrate never carved. The push now rejects any
  `off >= bump` offset (byte-identical to the flush-side guard).
  Counterfactual-pinned by `tests/regression_magazine_bump_guard.rs` (RED
  without the guard).

#### Changed — honesty of the M2 scope

- **R2 — the ring↔magazine cross-thread double-free residual is documented,
  pinned, and modelled (real fix tracked as #164).** A block whose
  cross-thread free is still in-flight in a segment's `RemoteFreeRing` (not
  yet drained by the owner) sets neither own-thread oracle (it is in neither
  the magazine `slots` scan nor the `BinTable` `is_free` bitmap), so a
  concurrent own-thread double-free of it is not detected. This is a
  pre-existing limit (present in the live 0.2.1 `fastbin` too), NOT
  introduced by the perf arc. Pinned by
  `tests/regression_xthread_double_free_residual.rs` (`#[ignore]`), modelled
  by `tests/loom_magazine_ring_compose.rs` (loom also showed the naive
  "own-free reads the ring" fix is itself holed — the real fix must let the
  drain see the magazine, hence #164). `docs/INVARIANTS.md` / README now
  qualify "never UB" to live/mapped memory and reference this residual.

#### Internal — verification

- **R3 — `production` is now covered by sanitizers in CI:** a ThreadSanitizer
  job on the `production` feature set plus `miri` over the `fastbin` magazine
  tests (and loom variants). Zero races, zero UB.
- **R4 — code-doc hygiene:** stale `40`→`49` size-class counts, the slot-0
  FIFO wording, the unsafe-seams comment, and stale `realloc` / no-`Box`-on-
  bind notes corrected across the substrate source.
- **S1 — bounded concurrent boundary-stress harness**
  (`tests/stress_concurrent_boundaries.rs`): multi-thread hammering of the
  class / align / segment seams with allocation canaries + distinctness +
  M2/D1 assertions, all from the safe envelope. Bounded to ~1 s by default; a
  heavier run is opt-in via `SEFER_STRESS_HEAVY` / `SEFER_STRESS_OPS` /
  `SEFER_STRESS_MAX_THREADS`.
- **S2 — deterministic single-thread exhaustive boundary sweep**
  (`tests/stress_boundary_sweep.rs`): every class/align seam × a realloc
  matrix (~2100 cases in ~0.5 s; the grid auto-reduces under `cfg(miri)`).
- **S3 — the stress harnesses run under sanitizers in CI:** S1 under TSan,
  S2 under miri, with reduced budgets so CI stays fast. Neither S1/S2 nor the
  sanitizers found any new bug.
- **D1 — release-doc accuracy pass** (docs-only): the unsafe-seam inventory
  (+`registry::bootstrap`), the M2 scope, purged env-vars, `production` =
  `+fastbin`, the 1024-segment-ceiling reframe, and every verification
  counter were reconciled against verified ground truth before the tag.

### Post-tag-review pass (#167 H1, #164 design, C2-regression fix)

A second four-agent review of the fully-composed 0.3.0 tree, each finding
verified by a personal counterfactual before commit.

#### Added

- **`hardened` feature (#167 / H1) — opt-in defence-in-depth against
  UNSAFE-CALLER misuse, default OFF, NOT in `production`.** Adds an
  interior-pointer free guard on **both** own-thread free faces — the
  `SeferAlloc` per-thread magazine (`HeapCore`) and the substrate
  (`AllocCore::dealloc_small`, which the explicit `Heap`/`with_heap` face and
  any direct `AllocCore` user reach): a free of a pointer that is not a block
  start (`off % block_size(class) != 0`) becomes a detected no-op instead of a
  mis-indexed bitmap read → magazine double-issue / free-list corruption. The
  check is a modulo-per-free (a real division), so it is honestly a paid check
  and stays behind the feature — the `production` hot path is byte-identical.
  The cross-thread leg is already covered unconditionally by `reclaim_offset`.
  Other misuse vectors were cost-evaluated and honestly rejected (mismatched-
  layout free needs a per-block size word — reintroduces the block-body write
  Э6 removed). Pinned by `regression_hardened_interior_ptr`.

#### Fixed

- **C2 realloc regression (a tag blocker, found by the review):
  `HeapCore::realloc`'s own-segment branch bypassed segment-ownership stamping
  and the A1 deferred-large drain.** The 0.3.0 C2 optimization delegated
  own-thread realloc straight to `AllocCore::realloc`, so a Vec grown via
  realloc lived in an UNSTAMPED Large segment (`owner_thread_free == null`); a
  cross-thread free of it silently no-op'd → the 4+ MiB segment and its slot
  leaked → cumulative `MAX_SEGMENTS` exhaustion → abort. The resurrected
  A1/#114 leak-to-abort, on the everyday "Vec grows on one thread, freed on
  another" pattern. Fixed by mirroring `alloc`'s two hooks (stamp the result
  when it relocated; drain when the new size is Large). Pinned by
  `regression_realloc_xthread_stamp`.
- **`AllocCore::reclaim_offset` panicked instead of skipping on a garbled ring
  entry.** The class field carries 10 bits (0..1023) while only 49 classes
  exist; a corrupted entry indexed `SIZE_CLASS_TABLE` out of bounds → panic
  inside the `#[global_allocator]` → abort, violating the function's own
  documented "no abort — just skip" contract. Fixed with a class-bounds check.
  Pinned by `regression_reclaim_offset_garbled_class`.

#### Internal

- **CI blind spots closed:** a `windows-latest` `production` job (the
  aligned-vmem `VirtualAlloc`/`MEM_DECOMMIT` path was only tested locally),
  the workspace member crates' own suites (`aligned-vmem` etc.), an MSRV
  (1.88) `cargo check`, a `production hardened` matrix row, aarch64 gains a
  `production` cross run, and `release.yml` gains a tag==version guard + a
  pre-publish test gate (and a fix to the root-crate `cargo pkgid` version
  parse). The `loom_magazine_ring_compose` model and the `hardened` row were
  also wired into CI.
- **SAFETY / doc-rot corrections** (docs-only): the `TORN` (#129) SAFETY
  comment no longer rests on the false "reverse TLS declaration order"
  guarantee (rewritten to the three real reasons); the stale
  `install_thread_free` "Box-allocates" claim corrected.

### Second review pass — perf, reliability & CI hardening (W1–W6)

A second `/fxx` review of the fully-composed tree. A **deterministic
instruction-count (`Ir`) judge was built first** (W1) so every perf change is
proven on the noisy Windows dev host *before* push, not left to Linux CI. Each
change was verified by a personal counterfactual and committed between phases.

#### Performance

- **W4 — `carve_batch` + batched `dec_live`: the cold 16–64 B path drops
  ~6.3k `Ir`.** A cold refill carved blocks one at a time through `carve_block`,
  paying a runtime `align_up` division on the loop-carried `bump` dependency
  chain plus a per-block `SegmentMeta` view, `bump` load/store, `is_decommitted`
  check, `inc_live`, and page-map probe — most of them tautological after the
  first block of a run. `AllocCore::carve_batch` carves a whole run from the
  bump cursor in one shot (one `align_up`, one `set_bump`, one `add_live(n)`,
  one recommit check, page-map marking only on a page change), byte-identical to
  the per-block loop (the alloc bitmap is never touched — a bump-carved block is
  already `bit0 = allocated`, so M2 is untouched; D1 exact `+n`; same SEGMENT
  boundary, page dedication, and decommit recommit-on-reuse). `refill_class_bump`
  also drops a now-dead redundant freelist re-read after the `free_exhausted`
  latch. `flush_run`'s per-block `dec_live_and_maybe_decommit` becomes one
  `sub_live(k)` + a single decommit check (`live` reaches 0 only at the last
  accepted block, so the decommit decision is identical). Measured (W1 Ir judge,
  `production`): `cold_alloc_free_256x16b` 129,863 → 123,516 (−6,347),
  `cold_alloc_free_256x64b` −6,350, `recycle_alloc_free_256x16b` −6,254; churn
  is unregressed. Two candidates were **honest-rejected with numbers**: a
  `REFILL_N` const LUT regressed cold +32 Ir vs the inlined `udiv`; a
  `heap_core` branch-fold was not a self-contained `match`. Pinned by
  `regression_carve_batch` (+ `alloc_core_differential` M1–M4 and
  `regression_magazine_oracles` M2).
- **W3 — `alloc-stats` gate: `production` lands *below* the pre-W3 baseline on
  the hit-heavy benches.** The per-hit `tcache_hits` / `large_cache_hits`
  increments are now gated behind a new `alloc-stats` feature (default OFF, NOT
  in `production`). With it off, the magazine (churn) and large-cache hit fast
  paths carry no counter bookkeeping and those two `stats()` fields read `0`
  (all other `stats()` fields unaffected; the counter storage always exists in
  the slot, so toggling never changes layout/ABI). Gating the bump out brings
  `production` below baseline: `small_churn_16b` −59, `churn_256b` −59,
  `recycle_256b` −477, `cold_256b` −236 Ir. Enable
  `--features "production alloc-stats"` to poll the counters.

#### Fixed

- **W3 — closed a Stacked-Borrows aliasing gap in the stats aggregators.** The
  process-wide `stats().tcache_hits` / `.large_cache_hits` aggregators read each
  heap's counter through `(*heap_ptr).…`, materialising a shared
  `&HeapCore`/`&AllocCore` over a struct the OWNING thread concurrently holds a
  protected `&mut` into — a foreign read of a protected `Unique`: UB under
  Stacked Borrows (miri's default model), fine under Tree Borrows. The two hit
  counters now live in the shared, `Sync` `HeapSlot` (already read by the
  aggregator via `&HeapSlot` for `initialised`); the owner increments them
  through a stable `&'static AtomicU64` planted at `HeapRegistry::claim`, and the
  aggregators read the slot's atomic directly — no `&HeapCore` is ever formed.
  Personally verified under miri: the old shape is SB UB, the new shape is
  SB-clean (`regression_w3_stats_aliasing_miri`).
- **W2 — `SegmentTable` tombstone-rebuild: killed a long-horizon probe cliff.**
  The open-addressing `contains_base` hash tombstoned deleted entries but never
  converted them back to empty, so `#empty` was monotonically non-increasing:
  every register/unregister cycle with a fresh base (large-cache eviction,
  decommit-recycle, ASLR) consumed one empty slot forever. Once `#empty` hit 0,
  a `contains_base` MISS — the hot case, since every cross-thread free begins
  with one on the caller's own table — probed the ENTIRE table. A long-running
  server (the DBMS/async profile the crate targets) degraded to ~`HASH_CAPACITY`
  metadata loads per cross-thread free. Fixed with an exact tombstone counter
  that rebuilds the hash from the live slot set once tombstones exceed
  `HASH_CAPACITY/4` (O(1) amortised per delete; the read path stays branch-free).
  Membership is transparent across rebuilds. Ir byte-identical on all hot benches
  (zero instructions added to the measured paths). Pinned by
  `regression_segment_table_tombstone_rebuild`.

#### Internal — tooling, CI, docs

- **W1 — a deterministic WSL `Ir` judge (`npm run iai`, `scripts/iai.mjs`).**
  Drives the Linux-only `benches/perf_gate_iai.rs` through WSL under
  `valgrind --tool=callgrind` and tables the per-bench instruction count.
  Instruction counts are byte-deterministic run-to-run, which makes this a judge
  on the noisy Windows host where wall-clock is not. `docs/perf/IAI_BASELINE.md`
  records the reference table.
- **W5 — MSRV / macOS / fuzzing.** Silenced a `cargo +1.88 check --all-features`
  dead-code false-positive on `ABANDON_SEG_SIZE` (an MSRV-invisible `const _`
  assert reference); added a `macos-latest` allocator job (real Darwin runs the
  `madvise(MADV_DONTNEED)` decommit path) plus an honesty note that XNU
  `MADV_DONTNEED` is lazy (RSS reclaim best-effort; correctness unaffected as
  `alloc_zeroed` zeroes explicitly); widened the fuzz align corridor to
  `2^0..2^21` (exercising #130's large-align math), added a third fuzz target
  `heap_core_ops` (the fastbin magazine via the `SeferAlloc` `GlobalAlloc`
  face), seed corpora, and a build-only `fuzz-build` CI job.
- **W6 — sanitizer coverage gaps.** Added a plain-provenance `miri-plain` CI job
  for the exposed-provenance intrusive stacks (A1 `deferred_large` /
  `abandoned_segs`), which the strict-provenance miri jobs cannot validate by
  design; and added the two Large cross-thread tests
  (`regression_realloc_xthread_stamp`, `regression_heap_xthread_large_free_no_leak`)
  to the ThreadSanitizer list.

### Long-run durability pass — counter-wrap hardening (W7)

Auditing what happens on ultra-long runs (days/weeks of uptime, billions of
ops): every monotonic/wrapping counter was enumerated and its wrap boundary
either made unreachable (widen/repack, at proven-zero hot-path cost) or pinned
and tested across the boundary. **Honest framing: none of these was a live bug
today** — the pass makes long-run robustness auditable and future-proof. The
full inventory is [`docs/DURABILITY.md`](docs/DURABILITY.md).

- **W7a — `HeapSlot::generation` → `AtomicU64`; `TaggedPtr` repacked to
  `index:16 | tag:48`.** Generation wrapped at 2^32 thread-deaths (reachable on
  a thread-per-request server over months) — though it turned out to be consumed
  only by a `== 1` first-materialise gate, with the stale-TLS hazard actually
  guarded by the `TORN` sentinel, so the wrap was defence-in-depth, not a live
  ABA. The `free_slots` ABA tag was 32-bit (the documented probabilistic wrap);
  repacking the index half from 32 to 16 bits (MAX_HEAPS = 4096 needs 13, pinned
  by a `const` assert) gives the tag 48 bits → wrap at ~89 years. Generation is
  Ir byte-identical; the repack is a uniform −4 Ir (a *decrease*, from the
  cheaper bootstrap `empty()` constant — cold path). Boundary tests in
  `regression_counter_wrap` preset each counter near its limit and cross it.
- **W7b — pinned the `RemoteFreeRing` u32 cursor wrap.** The per-segment ring's
  `head`/`tail` genuinely wrap on a long run (2^32 cross-thread frees on one hot
  segment — reachable), but the ring is wrap-SAFE by design (`wrapping_sub`
  occupancy + `i % RING_CAP` indexing, whose continuity across `u32::MAX` needs
  `2^32 % RING_CAP == 0`). That power-of-two dependency was unstated — now a
  `const` assert — and `regression_ring_cursor_wrap` drives the real ring across
  the boundary (FIFO order, full-ring overflow, occupancy, and a concurrent
  hammer). Counterfactuals confirm both guards bite. Ir byte-identical.
- **W7c — `docs/DURABILITY.md`.** The authoritative counter inventory (width /
  wrap semantics / reachability arithmetic / verdict / covering test) and the
  rule that a new monotonic counter lands only with a row here + a
  boundary-crossing test, proven Ir-neutral.

### Post-review hardening pass (#129–#143)

This and the phase A–F pass below hardened 0.3.0 before its first publish:
the post-review pass (#129–#143, 2026-07-02/03) driven by a four-agent audit
with per-fix counterfactual verification, and the phase A–F pass
(2026-06-30). Entries are grouped per pass.

#### Fixed

- **#129 — BLOCKER: `tls_heap`'s stale-LOCAL TLS resolver could hand out two
  `&mut HeapCore` for the same recycled registry slot.** `tls_heap`'s `LOCAL`
  (a `Cell`, no `Drop`) and `GUARD` (`AbandonGuard`, has `Drop`) are declared
  in an order where `GUARD` drops FIRST on thread teardown — recycling the
  registry slot — while `LOCAL` survives holding its now-stale pre-recycle
  pointer. Every resolver treated any non-null `LOCAL` as "my own live slot";
  the documented generation-guard was never actually read on the alloc path.
  Reachable from correct code: an application `thread_local` with a `Drop`
  impl that allocates, first touched before the thread's first `sefer-alloc`
  allocation, is destroyed after `GUARD` — its `Drop` could resolve to the
  stale, already-recycled slot, handing out a second live `&mut HeapCore`
  concurrently with whoever re-claimed it (a data race / UAF). Fixed with a
  `TORN` sentinel (`usize::MAX`, never dereferenced): `AbandonGuard::drop`
  stamps `LOCAL = TORN` before recycling; all three TLS resolvers check
  `TORN` before treating a non-null `LOCAL` as live, and route post-teardown
  deallocs through the always-live fallback heap instead.
- **#130 — BLOCKER: `alloc_large` with `align >= SEGMENT` leaked to abort or
  returned a misaligned pointer (UB).** `alloc_large` places a large block at
  `base + align_up(header, align.max(PAGE))`, but `base` is only
  `SEGMENT`-aligned (4 MiB). For `align == SEGMENT`, the block itself landed
  `SEGMENT`-aligned at `base + SEGMENT` — an address `dealloc`'s
  `base & !(SEGMENT-1)` computation never resolves back to the registered
  `base`, so every such `dealloc` silently no-op'd, leaking the segment and
  its `SegmentTable` slot until `MAX_SEGMENTS` (1024) exhausted and the
  process aborted. For `align > SEGMENT`, the returned pointer inherited only
  4 MiB alignment roughly half the time — violating the `GlobalAlloc`
  contract (UB in the caller). Both reachable from a valid `Layout` (e.g.
  `#[repr(align(4194304))]`, huge-page buffers). Fixed by rejecting
  `align >= SEGMENT` up front with a null return (a legal, documented alloc
  failure) — exotic alignments at or above the segment size are unsupported
  by the dedicated-segment large path.
- **#131 — `ensure_slow`'s OOM path panicked without rolling back the
  bootstrap sentinel, livelocking every future registry access.** The CAS
  winner publishes `SENTINEL_INITIALIZING` before reserving VM for the
  `Registry`; on OOM the old code called `.expect(..)`, which panicked
  without ever restoring a real pointer or rolling the sentinel back to
  null. Every loser thread spinning on the sentinel spun forever, and every
  future `ensure()` call also spun forever (CAS(null, SENTINEL) never
  succeeds against a non-null stuck sentinel) — a process-wide livelock on
  the next registry touch. Worse, unwinding the panic itself allocates,
  reentering `ensure()` against the same stuck state before the panic even
  finished. Fixed: on reservation failure, roll `REGISTRY_PTR` back to null
  (Release) before terminating via `std::process::abort` (not `panic!` —
  `abort` performs no unwind and no allocation, so it cannot reenter
  `ensure()`).
- **#134 — `large_cache`'s `usable_size` was recomputed from mutable header
  fields, corrupting the RSS byte-budget.** At deposit time (both the
  own-thread `dealloc` Large branch and `reclaim_large_segment`),
  `usable_size` was recomputed from the header's `large_size`/`large_align`.
  On a large-cache HIT, a larger cached span can be reused for a smaller
  request, and the hit path rewrites the header's logical size/align to the
  smaller request — so on the segment's NEXT free, the recomputed
  `usable_size` under-reports the segment's true physical span. This let
  `large_cache_used_bytes` under-count real RSS, admitting more spans than
  the configured budget should allow (unbounded RSS amplification), and
  corrupted the cache-hit size-ratio matching. Fixed by adding a new
  `SegmentHeader::span_usable` field — the segment's PHYSICAL committed span,
  set once at the original OS reservation and carried forward verbatim
  (never recomputed) through every subsequent cache-hit reuse. Both deposit
  sites now read `header.span_usable` instead of recomputing from
  `large_size`/`large_align`.
- **#139 — miri could not validate the `registry` module: the ~22 MB
  `Registry` reservation was uninitialised under miri's `std::alloc`
  fallback.** `bootstrap::ensure_slow` relies on OS zero-pages
  (`VirtualAlloc`/`mmap`) for every `Registry` field it does not explicitly
  write. Under miri, `aligned-vmem`'s reservation falls back to
  `std::alloc`, which does NOT zero memory — so reads of `count`,
  `abandoned_segs`, and friends hit uninitialised memory (UB), aborting miri
  before it could validate anything in the registry module (including the
  #133 per-heap-counter aggregation and the #131 sentinel rollback). Fixed
  with a `#[cfg(miri)]`-only `write_bytes(base, 0, REGISTRY_SIZE)` right
  after the reservation — compiled out entirely on real targets (zero
  production cost). Full strict-provenance cleanliness of the tagged-pointer
  infrastructure is separately tracked as #140.

- **#142 — cross-thread `thread_free` access violated the aliasing model
  (Stacked AND Tree Borrows).** Expanding miri to the A1 cross-thread path
  showed the deferred-free push's `head.load` was UB under both experimental
  borrow models: the `owner_thread_free` stamp inherited the owner's
  `&mut self`-rooted reference provenance, so one remote thread's
  `compare_exchange` through it was a "foreign write" that Disabled the
  shared parent tag and forbade a second remote's read. Fixed with the same
  exposed-provenance discipline as #140: the stamp sites `expose_provenance()`
  the atomic's address (taken via `addr_of!`, no intermediate `&` retag) and
  `Node::atomic_ptr_ref` reconstructs the remote's `&AtomicPtr` via
  `with_exposed_provenance_mut` — a wildcard pointer outside the owner's
  borrow tree. Verified under miri with BOTH models on both faces' A1 tests
  and `heap_cross_thread` (all were UB before this fix).
- **#143 — `push_large_deferred_free` silently dropped a push (permanent
  leak) under concurrent head contention.** Found by the new
  `loom_deferred_large` model (#141) and confirmed by a 2M-trial
  `std::thread` reproduction: the double-push claim-CAS lived INSIDE the
  head-CAS retry loop, so after losing the head CAS to a concurrent pusher
  of a DIFFERENT base, the retry's claim always failed (the link word had
  already left `ABANDONED_TAIL`) and the function returned through the guard
  bail-out without ever winning `head` — the segment never entered the
  deferred-free stack (an A1-class permanent leak). Fixed by hoisting the
  claim CAS to run exactly once, before the head-CAS retry loop.
- **Full-review follow-up — the #138 layout-consistency mitigation
  over-rejected legitimate tiny-size frees.** The alloc path clamps every
  request to `MIN_BLOCK` (16) before it reaches the header's `large_size`,
  but the mitigation compared the freeing caller's RAW `layout.size()` — so
  a legitimate cross-thread free of a `size < 16`, `align > SMALL_MAX` block
  (a valid `Layout` via the raw alloc API) always mismatched, was dropped,
  and permanently leaked the segment + its table slot (the #114/#130
  leak-to-abort class, narrow trigger). `large_layout_consistent` now clamps
  the caller's size symmetrically before comparing.

#### Performance

- **#133 — per-heap hit counters replace a contended global-lock `fetch_add`
  on the hot path.** `DBG_TCACHE_HITS` (magazine-hit) and
  `LARGE_CACHE_HITS` (large-cache-hit) were process-global `AtomicU64`s
  bumped by every thread on otherwise fully-per-thread hot paths — a
  contended cache line that ping-ponged across cores. Moved to per-heap
  fields (`HeapCore::tcache_hits`, `AllocCore::large_cache_hits`),
  incremented `Relaxed` by the owning thread only; the process-wide view
  (`stats()`, tests) is reconstructed by summing every minted heap slot's
  counter, gated by a new `HeapSlot::initialised: AtomicBool` (Release-set
  after the heap is fully constructed; the aggregator Acquire-loads it to
  avoid reading a not-yet-initialised slot). Measured: churn −20.9 % (16 B),
  −19.6 % (64 B).
- **#135 — `SegmentTable::register`/`unregister`/`recycle` and
  `HeapCore::realloc`'s ownership test are now O(1), not O(segment count).**
  `register` used to scan `[0, count)` for a NULL slot; `unregister`/
  `recycle` scanned for a matching base. All three are now O(1) via a
  free-list stack of recycled slot indices (carved in the primordial
  segment) plus a field-specific `segment_id_at` header read that indexes
  the slot directly. `HeapCore::realloc`'s ownership check switched from
  `segment_bases().any(...)` (O(count)) to `AllocCore::contains_base` (O(1)
  hash probe, same semantics). Also hardens `dealloc_routing`'s M2 routing:
  `self.core.contains_base(base)` is now checked FIRST (O(1), reads only the
  caller's own table, no cross-thread memory read) — proven equivalent to
  the prior `owner_tf.is_null() || owner_tf == our_head` branch for every
  segment the caller owns; only a miss falls through to the field-specific
  cross-thread header reads.

#### Changed

- **#136 — public API polish before the first 0.3.0 publish (pre-release, not
  a breaking change for any published version).**
  - `SegmentLayout::SIZE_CLASS_TABLE` / `SIZE2CLASS` are now `&'static [..]`
    slices instead of fixed-size arrays (`[usize; 48]` / `[u8; N]`). The
    class-count grew silently 40→48 in 0.3.0; a fixed-length public type would
    have made every future class re-tune a breaking change. A slice view has
    no length in its type.
  - `LargeCacheConfig::budget_bytes(0)` now means "cache disabled" (every
    deposit released to the OS), stored verbatim as `Some(0)`. Previously `0`
    was silently remapped to `None` ("unbounded") — the opposite of what `0`
    intuitively suggests. Unbounded is still the default (don't call
    `budget_bytes`).
  - `LargeCacheMode` is now `#[non_exhaustive]` (adding a variant in a future
    release is no longer breaking).
  - Internal-but-`pub` items reachable only through `#[doc(hidden)]` modules
    (e.g. `AllocCore::segment_bases`, `HeapCore::segment_bases`) are now
    `#[doc(hidden)]`, and stale `SMALL_ALIGN_MAX`/`SMALL_MAX` docs were
    corrected to match the #114/B1 divisibility-aware small path (align > 16
    is served by the small path up to `SMALL_MAX`, not routed to Large).
  - rustdoc builds clean (0 warnings) under both the default and `production`
    feature sets; docs.rs is configured to render with `production`.

- **#132 — the explicit `Heap`/`with_heap` public face lacked the A1
  cross-thread Large-segment reclaim fix.** `SeferAlloc` (via `HeapCore`) got
  the A1 fix in 0.3.0; `Heap::dealloc_any_thread` did not — a cross-thread
  free of a Large/huge segment through the explicit `Heap` API still no-op'd
  and leaked the segment permanently until the owning `Heap` dropped. Both
  faces now share the same extracted deferred-free primitive
  (`alloc_core::deferred_large`), including the double-push guard hardening,
  so a remote free of a Large segment is reclaimed on the owner's next large
  allocation regardless of which public face is used.
- **#132 — `with_heap` panicked on a reentrant borrow or TLS teardown.**
  `with_heap`'s documented `# Panics` behaviour (`RefCell::borrow_mut`
  panicking on a reentrant call, or on TLS-destructor-already-ran) was a
  footgun for a public allocator API — e.g. a `Drop` impl that allocates via
  `with_heap` during thread teardown would abort instead of degrading
  gracefully. `with_heap` now uses the same no-panic
  `try_with`/`try_borrow_mut` mechanics as the crate-internal
  `with_heap_try` and returns `None` (its signature has always been
  `Option<R>`) instead of panicking.
- **#138 — A1 post-reuse defensive mitigation for cross-thread Large-segment
  double-free.** A1's deferred-free stack fully closes the PRE-reuse
  double-free window (a double-free of a Large segment not yet reclaimed is
  a sound no-op, guarded by `push_large_deferred_free`'s double-push CAS
  guard). The POST-reuse window remained: a stale free arriving after the
  segment was already reclaimed and handed to a brand-new allocation is, by
  address alone, indistinguishable from a legitimate free of that new
  occupant. Both cross-thread Large-free routing paths
  (`HeapCore::dealloc_routing`, `Heap::dealloc_any_thread`) now check that
  the freeing `Layout`'s size matches the CURRENT occupant's `large_size`
  header field (`alloc_core::deferred_large::large_layout_consistent`)
  before queuing the segment for reclaim; a mismatch is dropped as a no-op
  instead of corrupting the reused segment. **Honest scope: this is a
  mitigation, not a full fix** — a reuse that happens to request the
  bit-identical size is not caught (double-free remains UB by the
  `GlobalAlloc` contract). New regression tests:
  `tests/regression_xthread_large_free_layout_mismatch.rs`
  (`xthread_large_free_mismatched_layout_is_dropped`,
  `xthread_large_free_consistent_layout_is_reclaimed`, plus a `Heap`-face
  counterpart), counterfactual-verified against both call sites.

#### Internal

- **#137 — CI never exercised the `fastbin` (magazine/tcache) path or the
  flagship `production` feature bundle**, and `loom_fallback_init` (the
  fallback-heap lazy-init state machine) existed but was absent from the
  loom CI matrix (model-checked locally, never gated in CI). Added
  `--features "alloc-global alloc-xthread fastbin"` and
  `--features production` to the test matrix, `--no-fail-fast` to the test
  runner (a failure in one test binary no longer masks failures in later
  ones), and `loom_fallback_init` to the loom matrix.
- **#138 — loom-model honesty audit.** Every `tests/loom_*.rs` file's doc
  comment now states whether it models a currently-live production code
  path, a removed/superseded one, or a dead (currently-unreachable) one:
  `loom_thread_free.rs` models the Phase 10 intrusive-TFS push/drain of
  individual freed blocks, which was superseded by the non-intrusive
  per-segment `RemoteFreeRing` (modelled separately, faithfully, in
  `loom_remote_ring.rs`) — retained for its generic CAS-push counterfactual,
  not as a validator of any current path. `loom_registry.rs` models the
  Phase 12.4 segment-adoption CAS protocol, whose only producer
  (`HeapRegistry::abandon_segments`) is unreachable from any production path
  today (Phase 12.5 replaced thread-exit abandonment with whole-heap slot
  reuse) — retained as a pre-validated substrate for a future
  decommit-when-empty policy. `tagged_ptr.rs`'s doc comment referenced a
  push-pop-repush ABA loom model in `loom_registry.rs` that was never
  actually written (that file models a different protocol entirely); the
  reference is corrected and the missing ABA model for the `free_slots`
  `TaggedPtr` stack is tracked as follow-up debt, not written in this pass.
  A loom model for the A1 `deferred_large` push/drain (Large-segment
  reclaim) is also tracked as follow-up debt — judged out of scope for this
  hardening pass (see the task report for the full audit table).

- **#140 — explicit provenance APIs for the registry's lock-free stacks.**
  The `REGISTRY_PTR` sentinel is now constructed with
  `core::ptr::without_provenance_mut` (strict-provenance-clean; it is only
  ever compared, never dereferenced), and every cross-allocation packed-word
  store/load pair in `abandoned_segs` and the A1 deferred-large stack calls
  `expose_provenance` / `with_exposed_provenance_mut` explicitly, with a
  documented "Provenance model" section explaining why full
  `-Zmiri-strict-provenance` is structurally unreachable for
  cross-allocation intrusive stacks (an exposed-provenance shape by design,
  not a bug). No lock-free semantics changed.
- **#141 — the two missing loom models were written**, closing the debt the
  #138 audit recorded above: `loom_deferred_large.rs` (the A1 push/drain
  Treiber stack including the double-push guard — the model that found
  #143) and `loom_free_slots_aba.rs` (the `free_slots`/`TaggedPtr`
  push-pop-repush ABA scenario). Both ship `should_panic` counterfactuals
  proving non-vacuity and are wired into the CI loom matrix.

### Initial pass — phases A–F (2026-06-30)

Post-0.2.1 hardening pass — six phases (A–F), each independently reviewed,
counterfactual-verified, and committed.

#### Fixed

- **A1 — permanent leak: cross-thread free of a Large/huge segment.** A
  remote free of a Large segment no-op'd instead of reclaiming it — the
  segment (≥4 MiB) and its `SegmentTable` slot leaked forever under any
  allocate-here/free-there workload (the canonical case: an async runtime
  migrating a task holding a large buffer to a different worker thread). Now
  reclaimed via a per-heap deferred-free stack, drained lazily on the
  owner's next large allocation.
- **A2 — `fastbin` buildable without `alloc-xthread` (unsound).** A
  cross-thread free with `fastbin` alone had no ownership-checked routing
  path — a data race into another thread's private magazine. `fastbin` now
  requires `alloc-xthread` (Cargo feature unification + a `compile_error!`
  guard).
- **B1 — page-aligned allocations (512 B – 16 KiB, `align` a multiple of
  512/1024/2048/4096) still burned a dedicated Large segment**, the last gap
  in #114's fix. Eight page-aligned size classes added to the table.
- **Latent `realloc` cross-class-shrink bug**, exposed by B1: `AllocCore::realloc`'s
  in-place fast path aliased a shrink across size classes, corrupting the
  smaller class's free list on a later layout-derived free. Restricted to
  same-class in-place; a cross-class shrink now relocates.
- **F1 — fallback-heap init livelock.** If the CAS winner initialising the
  process-global fallback heap hit primordial OOM, every other thread
  spun forever waiting for a `READY` that would never come. Losers now
  observe the rollback and re-race the CAS.

#### Changed — performance

- **C1 — the per-thread magazine (`fastbin`) now serves `align > 16`
  requests** (tokio task cells, page-aligned buffers), not just the
  historical `align <= 16` case — the main remaining hot-path gap for the
  workload #114/B1 targeted.
- **C2 — `realloc`'s in-place fast path is now reachable through the
  `#[global_allocator]` face**, not just the lower-level `AllocCore` API; a
  same-class resize through `SeferAlloc` no longer pays a redundant
  alloc+copy+dealloc.
- **D1 — `LARGE_CACHE_SLOTS` raised 2 → 8**, with a correctness fix: eviction
  now uses a true insertion-order FIFO (a monotonic sequence number) instead
  of an index-order assumption that only held at 2 slots. A workload cycling
  more than two distinct large sizes now gets real cache reuse instead of
  thrashing to an OS round-trip on every allocation.
- **D3 — magazine refill is now a per-class byte budget** (≈64 KiB) instead
  of a fixed 16-block count for every class; a large size class no longer
  parks several MiB in one idle thread's cache after a single refill.

#### Added

- **`SeferAlloc::stats() -> AllocStats`** — a cheap, lock-free, process-wide
  diagnostic snapshot (cache hits, decommit calls, cross-thread reclaims,
  ring overflows, segments reserved/released, heaps claimed). Previously
  every one of these counters was crate-internal and invisible in
  production; `segments_reserved_total - segments_released_total` is the
  single most useful field for spotting a segment leak before it escalates
  to an OOM abort. `#[non_exhaustive]`, stable field set across every
  feature combination.
- **D2 — process-wide `RemoteFreeRing` overflow counter**, feeding
  `AllocStats::ring_overflows`.
- Rustdoc: a "Multi-thread safety" section on `SeferAlloc` spelling out the
  `alloc-global`-without-`alloc-xthread` footgun (cross-thread frees leak
  monotonically), and a "std-only" note.

#### Internal

- CI: `-D warnings` restored on the clippy gate after a warnings-cleanup
  pass; miri matrix extended to the task-#114 align-regression tests; a
  process-global-state test flake in `heap_core_bulk_bypass` fixed at its
  real root cause (whole-heap slot reuse carrying stale P7 state across
  tests in one binary).

## [0.2.1] - 2026-06-30

> ⚠️ **Superseded by `0.3.0`; to be yanked from crates.io once `0.3.0` is
> published.** `0.2.1` ships `fastbin = ["alloc-global"]`, which is buildable
> *without* `alloc-xthread` — a cross-thread free with `fastbin` alone has no
> ownership-checked routing path and races into another thread's private
> magazine (data race / UB). Fixed in `0.3.0` (phase A2: `fastbin` now
> requires `alloc-xthread`, enforced by Cargo feature unification + a
> `compile_error!` guard). Upgrade to `0.3.0`.

### Fixed — `align > 16` allocations no longer burn a dedicated segment each

`SizeClasses::class_for(size, align)` unconditionally returned `None` for
any `align > SMALL_ALIGN_MAX` (= `MIN_BLOCK` = 16). Every allocation with
a larger alignment — including the `tokio::runtime::task::core::Cell<T,S>`
shape (≈640 B, `#[repr(align(128))]` against false sharing) — was routed
to the dedicated-segment Large path, consuming a full ~4 MiB segment and
one `SegmentTable` slot per request.

Under concurrent task-spawning workloads (canonical reproducer: the
shamir-db `duplex_throughput/duplex_cap32/32` bench — 32 in-flight
tokio tasks × 55 iterations), cumulative live segments exceeded
`MAX_SEGMENTS = 1024`, then `alloc_large_slow → SegmentTable::register`
returned `None`, then the `GlobalAlloc` face returned null, then
`std::alloc::handle_alloc_error` aborted the process with
`memory allocation of 640 bytes failed`.

`class_for` now searches for the smallest small class whose
`block_size >= max(size, align)` AND `block_size % align == 0`. M4
(alignment fidelity) is preserved: the segment base is `SEGMENT`-aligned,
the offset within is a multiple of `block_size`, and `block_size` is a
multiple of `align`, so the returned pointer is naturally `align`-aligned
without any per-block padding. The fast path for `align ≤ MIN_BLOCK = 16`
(the typical case) is byte-identical to the previous behaviour — one
`SIZE2CLASS` load. The slow path is a forward walk over at most
`SMALL_CLASS_COUNT = 40` entries; in practice it settles in 0–3 steps
for power-of-two alignments common in async runtimes (32 / 64 / 128 / 256).

For `(640, align=128)` the resolver picks the existing class with
`block_size = 768` (768 % 128 == 0). Per-allocation memory cost drops
from ~4 MiB to ~768 B, and the per-process `SegmentTable` is no longer
touched on the hot path.

Regression test: `tests/regression_large_align_no_segment_exhaustion.rs`
(2048 sequential `(640, 128)` allocations + 1500 sequential allocations
each for 4 representative `(size, align)` shapes). Counterfactual
verified — reverting the fix makes the test fail on iteration 1023
(= `MAX_SEGMENTS − 1`, primordial segment holds the first slot).

Single-threaded substrate change; no concurrency-protocol or wire-format
implications. Full test suite under `features = ["production"]` —
including loom (`loom_bootstrap_cas`, `loom_xthread_protocol`,
`loom_thread_free`) — green.

## [0.2.0] - 2026-06-29

> ⚠️ **Yanked from crates.io.** Superseded by `0.2.1`, which fixes the #114
> `align > 16` segment-exhaustion bug: an `align > 16` allocation (e.g. the
> `tokio` task-cell shape, `#[repr(align(128))]`) burned a full ~4 MiB
> segment each and could exhaust `MAX_SEGMENTS = 1024` and abort the process
> under ordinary async workloads. Upgrade to `0.2.1` or later.

### Changed — BREAKING: `SeferMalloc` renamed to `SeferAlloc`

The headline `#[global_allocator]` type is renamed from `SeferMalloc` to
`SeferAlloc`. The "malloc" suffix was a libc convention inherited from
C-wrapper allocators (`mimalloc`, `jemalloc`, `tcmalloc`) and clashed
with sefer-alloc's positioning as a pure-Rust allocator with no C deps.
The new name aligns the type with the crate name and the Rust ecosystem's
modern `*-alloc` convention.

**Migration:** rename every occurrence of `SeferMalloc` in your code to
`SeferAlloc`. The constructors (`new()`, `with_config(...)`) and the
public API surface are otherwise unchanged.

```rust
// Before (0.1.x):
use sefer_alloc::SeferMalloc;
#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// After (0.2.0):
use sefer_alloc::SeferAlloc;
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
```

`LargeCacheConfig`, `LargeCacheMode`, `Region`, `Handle`, `SyncRegion`,
`AllocCore`, and every other public type are unchanged.

Internal: `src/global/sefer_malloc.rs` → `src/global/sefer_alloc.rs`
(module file rename). User-facing docs (`README.md`, `docs/INTEGRATION.md`,
`docs/ARCHITECTURE.md`) updated to use "alloc face" terminology consistently;
historical / planning docs (`ALLOC_PLAN.md`, `FINDINGS_PHASE12.md`, etc.)
keep their original "malloc face" language as historical record.

`0.1.0` is yanked from crates.io to direct fresh installs to `0.2.0`;
existing `Cargo.lock` references continue to work.

### Changed — const-builder config API replaces env vars (alloc-decommit)

- **`LargeCacheConfig` const builder** — new type (re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`). All five knobs
  that were previously set via environment variables are now expressed at
  compile time via a `const fn` builder chain:

  ```rust
  use sefer_alloc::{SeferMalloc, LargeCacheConfig, LargeCacheMode};

  const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
      .budget_bytes(512 * 1024 * 1024)
      .headroom_bytes(64 * 1024 * 1024)
      .decay_interval_ms(200)
      .decay_rate_percent(25)
      .mode(LargeCacheMode::Lazy);

  #[global_allocator]
  static GLOBAL: SeferMalloc = SeferMalloc::with_config(CONFIG);
  ```

- **`SeferMalloc::with_config(config: LargeCacheConfig) -> Self`** (`const fn`,
  only under `alloc-decommit`) — constructs the allocator with a custom
  large-cache config. The config is plumbed into each per-thread `AllocCore`
  on first TLS bind.

- **`SeferMalloc::new()`** unchanged — equivalent to
  `SeferMalloc::with_config(LargeCacheConfig::DEFAULT)`.

- **`AllocCore::new_with_config(config: LargeCacheConfig) -> Option<Self>`**
  (`alloc-decommit` only) — new constructor for direct `AllocCore` users.

- **Env vars removed entirely** — `SEFER_LARGE_CACHE_BUDGET`,
  `SEFER_LARGE_CACHE_HEADROOM_BYTES`, `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS`,
  `SEFER_LARGE_CACHE_DECAY_RATE`, `SEFER_LARGE_CACHE_MODE` are no longer read.
  The allocation-free env-var parser in `src/alloc_core/os.rs` is deleted.
  Default values are byte-identical to what the parsers produced when no variable
  was set (headroom=256 MiB, interval=1000 ms, rate=10 %, budget=unbounded,
  mode=Lazy).

- **Tests updated** — `tests/large_cache_budget.rs`, `tests/large_cache_decay.rs`,
  and `tests/large_cache_mode.rs` no longer use `std::env::set_var`. The
  env-var test cases are replaced with equivalent `AllocCore::new_with_config`
  tests that are deterministic and safe to run in parallel.

## [0.1.0] - 2026-06-28

### Changed — workspace extraction (tasks #74–#86)

Four independently-publishable companion crates extracted from sefer-alloc
into `crates/`. Each is a real crates.io package someone can `cargo add`
on its own:

- **`sefer-region`** (`crates/region/`) — typed handle store
  (`Handle<T>` / `Region<T>` / `SyncRegion<T>`). `#![forbid(unsafe_code)]`.
  ([docs.rs/sefer-region](https://docs.rs/sefer-region) — link live after publish.)

- **`aligned-vmem`** (`crates/vmem/`) — OS virtual-memory aperture:
  SEGMENT-aligned `mmap`/`VirtualAlloc` + page decommit/recommit.
  `#![allow(unsafe_code)]` — sole purpose IS the OS unsafe, single
  responsibility, small codebase, independently auditable.
  ([docs.rs/aligned-vmem](https://docs.rs/aligned-vmem) — link live after publish.)

- **`numa-shim`** (`crates/numa/`) — dependency-free NUMA detection and
  binding. Linux `mbind(2)` via `syscall(2)` (no `libnuma`), Windows
  `VirtualAllocExNuma`. `#![allow(unsafe_code)]` — sole purpose IS the NUMA
  syscall unsafe, single responsibility, independently auditable.
  ([docs.rs/numa-shim](https://docs.rs/numa-shim) — link live after publish.)

- **`malloc-bench-rs`** (`crates/malloc-bench/`) — portable `GlobalAlloc`
  benchmark harness (larson + mstress). Callable against any allocator without
  installing it as `#[global_allocator]`. Not in sefer-alloc's runtime dep
  tree.
  ([docs.rs/malloc-bench-rs](https://docs.rs/malloc-bench-rs) — link live after publish.)

**sefer-alloc itself** re-exports `sefer-region`'s surface for backward
compatibility — existing `use sefer_alloc::{Region, Handle, SyncRegion}` code
compiles unchanged. `alloc_core::os` and `alloc_core::numa` are now thin
interop wrappers that delegate to `aligned-vmem` and `numa-shim` respectively.

**Audit story improved:** an auditor no longer has to navigate the full
allocator codebase to verify the OS-memory unsafe. `aligned-vmem` (~few hundred
lines, single purpose) and `numa-shim` (~few hundred lines, single purpose) can
each be audited in complete isolation with `cargo test` confirming green.

### Added — large-cache redesign Phase 3 (alloc-decommit, mode-selector + future stub)

- **`LargeCacheMode { Lazy, Background, Both }`** enum, re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`. The mode is selected
  via the new `SEFER_LARGE_CACHE_MODE` env var (case-insensitive: `lazy` /
  `background` / `both`; unrecognised values fall back to `Lazy`).

- **Default = `Lazy`** — Phase 2 behaviour is preserved bit-for-bit. Setting
  `SEFER_LARGE_CACHE_MODE=background` currently prints a one-time process
  warning ("background mode requested but not yet implemented — falling back
  to lazy") and continues with lazy decay. The full background-thread
  implementation has identified risks documented inline (Mutex refactor +
  HeapRegistry iteration API + safe spawn timing + TSan validation) and is
  intentionally deferred to a follow-up; the mode-selector plumbing lets a
  future commit turn it on without any user-facing API change.

- **`tests/large_cache_mode.rs`** — 3 new tests covering default-Lazy,
  per-shard mode storage, and env-var parsing.

### Changed — large-cache redesign Phase 2 (alloc-decommit)

- **Lazy exponential decay**: large-cache excess over the headroom target
  decays toward the OS at 10 %/tick by default. On every large `alloc` or
  `free`, a single `Instant::now()` comparison checks whether
  `decay_interval` has elapsed; if so, `excess = used - headroom` and
  `release = excess × rate` bytes are FIFO-evicted to the OS. No background
  thread — the decay is fully inline, paying nothing while the process is idle
  (mobile/embedded friendly). Phase 3 will add an optional background thread.

- **Three new env vars** (all read once at `AllocCore::new`, allocation-free):
  - `SEFER_LARGE_CACHE_DECAY_RATE` — integer percent (`"10"`, `"10%"`;
    default 10). Parsed without floats to avoid any floating-point dependency.
  - `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS` — integer ms (default 1000).
  - `SEFER_LARGE_CACHE_HEADROOM_BYTES` — bytes with K/M/G suffix (default
    256 MiB). The cache is allowed to hold up to this many bytes; only the
    excess above it is subject to decay.

- **Generalized `os::read_env_var_raw(name_nul, buf)`**: the allocation-free
  env-var reader is now parameterized on the variable name (NUL-terminated
  `&[u8]`). `read_env_budget_raw` is kept as a thin backward-compatible
  wrapper. This lets all three decay env parsers share the same reentrancy-safe
  pattern without duplicating the Windows/Unix platform dispatch.

- **Test seams** (`dbg_set_decay_config`, `dbg_force_decay_tick`,
  `dbg_decay_config`): deterministic test control without sleep or real
  wall-clock advances. `dbg_force_decay_tick` rewinds `last_decay_tick` by
  `decay_interval` and immediately invokes one decay step.

- **`tests/large_cache_decay.rs`**: 5 new tests covering excess release,
  headroom invariant, no-op when under target, interval guard, and env-var
  parsing.

### Changed — large-cache redesign Phase 1 (alloc-decommit)

- **Removed `MAX_CACHED_LARGE_BYTES`** (was 64 MiB per-span cap). Spans of
  any size can now enter the large-cache, removing the arbitrary ceiling that
  prevented caching of 100 MiB+ allocations.

- **Per-shard byte-budget admission** replaces the old per-span cap. A new
  `AllocCore::large_cache_budget_bytes: Option<usize>` field (under
  `alloc-decommit`) tracks the total bytes of all cached spans. When the
  budget would be exceeded, the oldest cached slot (FIFO: lowest index) is
  evicted to the OS before the new span is admitted. `None` = unbounded
  (default when the env var is not set).

- **`SEFER_LARGE_CACHE_BUDGET` environment variable** is read once at
  `AllocCore::new()` via a raw OS call (no heap allocation — safe even when
  `SeferMalloc` is the `#[global_allocator]`). Accepted formats: `"64M"`,
  `"2G"`, `"1024"` (raw bytes), etc. Parsed case-insensitively.

- **`large_cache_used_bytes` invariant counter**: maintained on every deposit
  and every eviction / cache hit. Verified by new tests via
  `dbg_large_cache_used()` / `dbg_large_cache_slot_sizes()` test seams.

### Removed

- **`byte` / `byte-sharded` features** — research-tier `ByteRegion` /
  `ByteAllocator` / `ShardedByteArena` removed. They were never expected to
  compete with mimalloc (see the BYTE_BENCH / BYTE_SHARDED_BENCH writeups in
  git history) and are fully superseded by the production stack (`alloc-global`
  + `alloc-xthread` + `alloc-decommit`). Old Phase 4 / Phase 7d log entries
  below are intentionally left intact as historical record.

### Deprecated

- **`experimental` concurrent regions** (`EpochRegion`, `LockFreeRegion`,
  `ShardedRegion`) — marked `#[deprecated]`. Superseded by the production
  `alloc-xthread` cross-thread free path. `PinnedRunner` is NOT deprecated.

### Summary

The initial public release.

**Pure Rust, no C / C++ libraries.** Unlike `mimalloc` (C++), `jemalloc`
(C), `snmalloc` (C++), `tcmalloc` (C++), or the typical `libnuma`-wrapping
NUMA crates, `sefer-alloc` is 100 % Rust — it calls into the OS directly
(`mmap` / `VirtualAlloc` / `mbind` etc.), but does not link a single C or
C++ library. The only C dependency in the repository is the optional
`mimalloc` dev-dependency used as a baseline in benchmarks (never on a
consumer's runtime path).

Two faces on one verified substrate:

- **`Region<T>` / `Handle<T>`** — a safe-by-construction handle store
  (default `std`, also `no_std` + `alloc`). `#![forbid(unsafe_code)]`
  at the top — the only `unsafe` is `slotmap`'s audited core wrapped
  by a typed membrane.

- **`SeferMalloc`** — a drop-in `#[global_allocator]` (opt-in
  `production` feature = `alloc-global + alloc-xthread +
  alloc-decommit`). Up to **~18× faster than `mimalloc` on cached
  large alloc/free** after the OPT-E large-cache (4 MiB cycle ≈ 45 ns
  vs ~718 ns ≈ **~16×**; 16 MiB ≈ 48 ns vs ~869 ns ≈ **~18×** — single
  Windows dev host, criterion `sample_size(10)`, see
  `docs/ALLOC_BENCH.md`); competitive with `mimalloc` on multi-thread
  cross-thread paths (`examples/malloc_macro.rs`). Confined-`unsafe`
  inventory under `production` (eight files): `alloc_core::{os, node}`
  + `global::{sefer_malloc, tls_heap, fallback}` +
  `registry::{heap_slot, heap_registry}`. `numa-aware` adds one more
  (`alloc_core::numa`). The crate is `#![deny(unsafe_code)]` (or
  `#![forbid]` in the default `std`-only build) and every `unsafe`
  block carries a `// SAFETY:` proof; compile-enforced.

Verification stack: 51 integration test files, 6 loom models
(`tests/loom_*.rs`), proptest differential vs reference model, miri
with strict-provenance (CI gate), ThreadSanitizer (×3 verified
clean on cross-thread + decommit), Valgrind memcheck clean,
aarch64 13/13 under qemu-user, libFuzzer (`region_ops`,
`global_alloc_ops`), soak / RSS / tokio-burn-in harnesses,
criterion benches with flamegraph profiling. Full details in
`docs/ARCHITECTURE.md` and `docs/ALLOC_BENCH.md`.

### Added

- **OPT-B (#67) — O(1) `SegmentTable::contains_base`**: a self-hosted
  open-addressing hash (2048 slots, 16 KiB in the primordial segment)
  replaces the O(count) linear scan. Tombstone encoding for removed
  entries keeps probe chains intact under recycle/decommit churn.
  Matters at DBMS scale (50–100+ live segments).
- **OPT-C (#66) — lazy `stamp_segment_owner`**: `HeapCore` caches the
  last-stamped segment base; cache-hit fast path is a single Relaxed
  load + ownership compare (no Release-store), skipping the costly
  MFENCE on 99 % of hot-segment allocations.
- **OPT-E (#65) — large-segment free-cache** (the headline win):
  1-2 fixed slots per `AllocCore` hold freed OS reservations; the
  next similarly-sized `alloc_large` reuses without mmap.
  **Measured: 4 MiB from 254 µs to 42 ns (~6,000× speedup, 18× faster
  than mimalloc 788 ns); 16 MiB from 701 µs to 48 ns.** Pages stay
  committed inside the cache (eliminates Windows
  `VirtualAlloc(MEM_COMMIT)` cost on hit). Bounded RSS at
  `LARGE_CACHE_SLOTS × MAX_CACHED_LARGE_BYTES = 2 × 64 MiB =
  128 MiB`. Gated on `alloc-decommit` for `SegmentTable` `unregister`
  consistency.
- **OPT-F (#64) — in-place small→small realloc**:
  `AllocCore::realloc` short-circuits when `new_size` resolves to the
  same or smaller size class as `old_size` — returns the same pointer,
  no copy, no alloc, no dealloc. Bench `realloc_in_place_unfavorable`
  improved 28.6 %.
- **OPT-G (#63) — `production` feature alias** + README guidance:
  `production = ["alloc-global", "alloc-xthread", "alloc-decommit"]`
  is the recommended set for long-running multi-thread workloads
  (DBMS, async runtimes); without `alloc-decommit` the
  `SegmentTable` slot-recycle path is disabled and the 1024-slot
  table is a hard ceiling.
- **NUMA-aware path** (Phases A–E of #58): opt-in `numa-aware`
  feature, default OFF. New confined-`unsafe` module
  `src/alloc_core/numa.rs` (Linux `mbind(2)` via `syscall(2)` —
  avoids `libnuma` dep — `MPOL_PREFERRED`; Windows
  `VirtualAllocExNuma`; macOS / miri no-op). Layout-stable
  `SegmentHeader::node_id` (present in every build).
  `reserve_small_segment` / `alloc_large` stamp the current thread's
  NUMA node; `find_segment_with_free` prefers local-node segments
  with foreign-node fallback. Tests: `numa_seam` (5),
  `numa_segment_id` (2), env-guarded `numa_alloc` (3, run with
  `SEFER_NUMA_TEST=1` under multi-NUMA topology). Honest caveat:
  QEMU verifies correctness, not latency-asymmetry; real measurement
  requires 2-socket hardware. See `docs/PHASE_NUMA_DESIGN.md`.
- **SegmentTable slot-recycle** (#60): under `alloc-decommit`, an
  empty decommitted segment NULLs its table slot for future
  re-registration, lifting the hard `MAX_SEGMENTS = 1024` cumulative
  ceiling. Found by the #52 tokio burn-in hitting OOM at >512
  concurrent tasks. New `recycle` (atomic NULL + `release_segment`)
  and partner `unregister` (NULL without release; used by OPT-E
  cache deposit).
- **strict-provenance miri fix** (#59): converted 11 sites of the
  `os::segment_base_of(ptr as usize) as *mut u8` idiom to the
  provenance-preserving `os::segment_base_of_ptr(ptr) =
  ptr.map_addr(|a| a & !(SEGMENT - 1))`. The CI miri job (which
  runs with `-Zmiri-strict-provenance`) now passes
  `decommit_miri_cycle` and `reclaim_offset_unit`.
- **Highload-hardening harnesses**:
  - `examples/soak_xthread.rs` (#51) — N-thread × hours stability
    test (32 / 64 / 128 workers); end-of-run invariant
    `total_alloc == total_free`.
  - `examples/rss_probe.rs` (#53) — measures peak / final RSS under
    sustained asymmetric cross-thread free; smoke: `alloc-decommit`
    keeps peak 13 % lower (91 → 79 MB).
  - `examples/tokio_burn_in.rs` (#52) — SeferMalloc installed as
    `#[global_allocator]` under tokio multi-thread runtime with a
    DBMS-pipeline-shaped workload.
  - `benches/large_realloc.rs` (#54) — three groups (large
    alloc+free, geometric realloc grow, realloc under neighbour
    pressure) comparing SeferMalloc, mimalloc, System through their
    `GlobalAlloc` traits.
- **Low-noise criterion benches** (#62): `benches/heap_xthread.rs`
  (direct ring push/drain, no channels) and
  `benches/heap_async_pattern.rs` (synthetic async-like pattern
  without tokio) — allocator visibility rises from 1.7 % to 13 % of
  self-time vs the noisier `global_alloc` / `large_realloc` benches.
- **Comprehensive verification runs** (one-off, evidence preserved
  in `docs/`):
  - ThreadSanitizer ×3 clean on `race_repro`, `race_norecycle`,
    `global_alloc_mt`, `heap_cross_thread`; ×3 clean on
    `decommit_stale_ring`, `decommit_soak`.
  - aarch64 (qemu-user 8.2.2) 13/13 tests pass, with honest caveat
    about TCG vs real ARM weak-memory.
  - Valgrind memcheck clean on three cross-thread test binaries;
    helgrind / DRD inapplicable to lock-free atomic code (known
    Valgrind limitation — TSan is the right tool).
  - Full Linux feature-matrix (6 combos × 248 tests) all green.
- **Documentation**:
  - `docs/ARCHITECTURE.md` — compact technical overview (synthesis
    of design memos).
  - `docs/PHASE_NUMA_DESIGN.md` (#55) — full NUMA design.
  - `docs/PROFILE_FLAMEGRAPHS.md` (#61) — flamegraph profiling
    report on 4 scenarios with 6 prioritised optimisation
    candidates (OPT-B/C/E/F/G all realised in this release; OPT-H
    documented but deferred as low impact).
  - `docs/ALLOC_BENCH.md` — extensive update with OPT-E large-cache
    numbers, NUMA section, honest verdicts.
- **OSS infrastructure** (preparing for crates.io publication):
  `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`.
  `Cargo.toml` metadata refreshed for crates.io (description
  mentions both faces, `keywords` rebalanced to `["allocator",
  "arena", "generational", "handle", "lock-free"]`, `categories`
  extended with `concurrency` and `no-std`, `repository` /
  `homepage` / `documentation` URLs added).
- **Build infrastructure**: `cargo-fuzz` metadata fix to enable
  `cargo fuzz build` (#56); `region_ops.rs` idiom corrected to match
  `arbitrary` 1.4.2 (#56); `malloc_macro` registered as
  `[[example]]` with `required-features` (was missing, causing CI
  `cargo test` without `--tests` to fail with E0601).

- **Phase 35 — M6 decommit: return empty segments to the OS** (behind a new
  opt-in `alloc-decommit = ["alloc-core"]` feature; **default OFF — the default
  build is byte-for-byte unchanged**). When a small segment's live-block count
  drops to zero and it is not the current carve target, its payload pages
  `[small_meta_end, SEGMENT)` are returned to the OS (`VirtualFree MEM_DECOMMIT`
  / `madvise MADV_DONTNEED`; no-op under miri) and the segment is reset to a
  clean blank (`bump = small_meta_end`, `BinTable` heads = NULL, payload
  page-map = Free, alloc-bitmap = 0, `decommitted` flag set); the payload is
  recommitted on the first reuse. This bounds steady-state RSS under churn (the
  one honest gap in `ALLOC_BENCH`). Bookkeeping: a new **owner-only** `u32`
  `live_count` field in `SegmentHeader` (present in every build's layout so the
  byte layout is stable; mutated only under the feature) — `+1` on
  `pop_free`/`carve_block` hand-out, `−1` on `dealloc_small`/`reclaim_offset`;
  refill blocks net to zero (carve `+1`, push-to-free-list `−1`). **No
  crossbeam-epoch / M11 barrier is needed** — Variant-2 (Phase 12.6) already
  removed the only reason the original plan reached for epoch: the cross-thread
  freer never dereferences the block (it pushes `offset|class` into the
  in-metadata `RemoteFreeRing`, and metadata pages are never decommitted). The
  full safety argument is recorded in code at the decommit point and in
  `docs/PHASE35_DECOMMIT_DESIGN.md` §1. A **post-decommit stale-free guard**
  (`off >= bump` after the reset) in both `dealloc_small` and `reclaim_offset`
  closes the window where a late free / double-free / stale ring entry targeting
  a reset segment would write a free-list `next` into a decommitted page. NO new
  dependency, NO new `unsafe` site (the OS seam already existed; the bookkeeping
  is plain safe arithmetic through the `node` seam). Tests (`alloc-decommit`):
  `decommit_soak` (decommit fires on `live→0` + recommit readback; counterfactual
  proven — the soak goes red if the hook is disconnected), `decommit_stale_ring`
  (stale ring entry into a decommitted segment is a no-op, no UAF),
  `decommit_miri_cycle` (bounded miri decommit/recommit bookkeeping). Verified:
  full suite green WITH and WITHOUT the feature (incl. `alloc_core_differential`,
  the heap suite, `race_repro`/`race_norecycle`/`global_alloc_mt`), clippy clean,
  miri on the bounded cycle. `heap_cross_segment`'s strict free-list-reuse
  invariant is relaxed under `alloc-decommit` to a bounded-footprint invariant
  (decommitted segments are legitimately re-carved, not free-list-reused).

- **Phase 12 — production multithreaded trust + Phase 12.6 cross-thread-free
  reclaim** (behind `alloc-xthread`). The installed `#[global_allocator]` is now
  a SOUND multithreaded drop-in: heap-as-shard isolation (each heap = a shard
  owned by one thread via a FREE/LIVE slot token), a self-hosted `HeapRegistry`,
  raw-pointer TLS with a never-null fallback heap, and loom-gated segment
  adoption (12.1–12.5). **Phase 12.6** closes the cross-thread-free
  *reclaim*: a non-intrusive per-segment MPSC ring carries each freed block's
  `offset | class` (the freer has the `Layout`; the owner's `page_map` class is
  unreliable for the mixed-class pages a shared bump cursor produces — the true
  root, found via ThreadSanitizer + a Linux free-list audit; NOT a data race).
  The owner reclaims lazily on its alloc-slow-path. This removes the Phase-12.5
  bounded-leak *discard* — cross-thread-freed blocks are now **reused**. Also
  fixed a real `SegmentHeader` data race (field-specific `bump`/`magic`/
  `owner_thread_free` accessors). Verified on Windows + Linux: `race_repro` ×5,
  `race_norecycle` (reliable Linux repro), isolated ring + reclaim unit tests,
  loom protocol/ring models with counterfactuals, full suite, clippy.
  See `docs/RACE_DRAIN_RECLAIM.md` (§13 root, §14 fix) and
  `docs/CROSS_THREAD_STATE_MACHINES.md` (the state-machine spec).
- **Phase 13.1 — O(1) size-class lookup** (`const SIZE2CLASS` table replacing the
  per-alloc linear scan).

- **Phase 11 -- the `malloc` face: `SeferMalloc` (`#[global_allocator]`) +
  no-panic hardening + honest mimalloc verdict** (behind a new opt-in
  `alloc-global = ["alloc"]` feature). `SeferMalloc` is an `unsafe impl
  GlobalAlloc` over the per-thread segment heap (one substrate, two faces: the
  typed `Handle` face and this raw `*mut u8` drop-in face), routing
  `alloc`/`dealloc`/`realloc`/`alloc_zeroed` through the no-panic TLS binding
  `with_heap_try` (returns null / no-ops instead of panicking — a panic in a
  global allocator aborts the process). **No-panic hardening:** the substrate's
  alloc-path panic sites were made graceful — the `alloc_small` `.expect` is
  gone, `SegmentTable::register` and `Segment::reserve` now return `Option`
  (null on failure, never `assert!`-panic). **Reentrancy-freedom (M5)** holds on
  the malloc path (no `Vec`/`Box`/`std::alloc`/`format!`). The `unsafe impl
  GlobalAlloc` is the documented malloc-face seam (every method `// SAFETY:`);
  `unsafe` stays confined. **Honest verdict (`docs/ALLOC_BENCH.md`):** on the
  alloc/dealloc hot path `SeferMalloc` is competitive with `mimalloc` (faster at
  1024 B and on realistic `Vec` push/grow churn; ~1.2-2x behind on small
  fixed-size churn) and consistently **~2.5-5x faster than the Windows system
  allocator** — safe by construction. Proven working as a real
  `#[global_allocator]` for a single-threaded workload
  (`examples/global_allocator.rs`: 100 k-`Vec` + 10 k-`HashMap`), and correct via
  direct-API tests (`tests/global_alloc.rs`: aligned, non-overlapping, reusable,
  realloc-prefix-preserving, 20 k churn). **NOT yet production-trusted:** as a
  *process-wide multithreaded* `#[global_allocator]` (e.g. under libtest's
  reentrancy-heavy harness) the current TLS binding returns null on
  reentrant/early-init/teardown access and aborts — a bootstrap-safe,
  reentrancy-tolerant TLS discipline is the remaining work, alongside the
  deferred heavy gate (`cargo-fuzz` CPU-hours, aarch64 multi-arch CI,
  ThreadSanitizer) and the Phase-10 deferrals (abandoned-heap adoption, M6
  decommit wiring). Honestly documented; for a process-wide allocator today, use
  `mimalloc`.
- **Phase 10 -- cross-thread free (M7), opt-in via `alloc-xthread`** (extends
  the `alloc` feature). Correct, lock-free cross-thread `dealloc` behind a
  new opt-in `alloc-xthread = ["alloc"]` sub-feature. When a thread frees a
  block it does NOT own, it pushes it onto the owning heap's atomic Treiber
  stack via a `compare_exchange` loop (the Phase-7b linearization protocol,
  re-based onto the Phase 8/9 segment substrate). The owner drains the stack
  in bulk on its next operation and returns each block to its per-class
  `FreeList`. O(1) owner lookup via `segment_base_of(ptr)` -> segment header
  -> `owner_thread_free` pointer (a stable `*const AtomicPtr<u8>` stored in
  each segment's header, pointing to the owning heap's `Box`-allocated Treiber
  head). The `ThreadFreeStack` is pure safe composition over
  `core::sync::atomic::AtomicPtr` + the `Node` seam (one new
  `Node::deref_atomic_ptr` in the existing `node` unsafe seam; no new unsafe
  module). **Thread-death soundness via abandonment-leak:** under
  `alloc-xthread`, `Heap::drop` intentionally LEAKS its segments (via
  `ManuallyDrop<AllocCore>`) and the Treiber head (via
  `ManuallyDrop<ThreadFreeStack>`) so that late cross-thread `dealloc` calls
  from other threads never touch unmapped memory or a freed `Box` -- segments
  stay mapped, the `AtomicPtr` stays allocated. This is a BOUNDED leak on
  thread death (one heap per thread), acceptable for the target long-lived
  thread-pool workload. Full abandoned-heap adoption (reclaiming leaked
  segments) is a Phase 11 deliverable. **Default `alloc` (no `alloc-xthread`)
  is unchanged Phase 9:** the single-thread-owner allocator with no
  `ThreadFreeStack`, no owner stamping, and normal segment release on
  `Heap::drop` (sound: single owner, no cross-thread refs). **Large / unstamped
  cross-thread free:** under `alloc-xthread`, a cross-thread free of a large
  block (`SegmentKind::Large`) or a block in an unstamped segment
  (`owner_thread_free == null`) is a documented no-op -- the block is
  conservatively leaked until the owning heap drops (or until Phase 11
  adoption). This avoids mis-accounting and is sound. **Decommit (M6) is NOT
  delivered** -- the `os::decommit_pages` / `os::recommit_pages` seam landed in
  Phase 10 (ready to wire) but is not integrated into the heap path. M6 is a
  Phase 11 deliverable. The soak test (`tests/heap_soak.rs`) asserts bounded
  segment growth via free-list reuse, not via decommit. Verification: **loom**
  model-check (`tests/loom_thread_free.rs`, 2 pushers + 1 drainer,
  `preemption_bound = 3`) with a proven counterfactual -- the naive non-CAS
  push demonstrably loses blocks under loom (the
  `counterfactual_naive_push_loses_blocks` test `#[should_panic]`s).
  Cross-thread differential proptest (`tests/heap_cross_thread.rs`, 64 cases,
  multiple threads, pattern write+readback -- non-vacuous). Soak test
  (`tests/heap_soak.rs`) -- bounded segment usage under sustained churn.
  Miri-clean on the cross-thread atomic seam (`tests/heap_miri_xthread.rs`,
  2-thread alloc/free, with `-Zmiri-ignore-leaks` for the intentional
  abandonment-leak).
- **Phase 9 -- per-thread heap + intrusive free lists (the lock-free fast
  path)** (behind a new opt-in `alloc` feature = `["alloc-core"]`). Each
  thread owns a `Heap` with per-size-class intrusive free lists stored inside
  the freed blocks themselves (via the Phase 8 `node` seam -- zero metadata
  allocation). The hot path (`alloc_small` / `dealloc_small`) is a single
  pointer read/write -- no lock, no atomic, no `Vec`/`Box`/`std::alloc` (M5
  reentrancy-freedom upheld). On free-list drain, a batch refill carves
  blocks from the Phase 8 `AllocCore` substrate. TLS heap binding via
  `std::thread_local!` with lazy, allocation-free init (`with_heap`); heap
  released on thread exit. Large/huge allocations route through the Phase 8
  dedicated-segment path. No new `unsafe` module -- the heap is pure safe
  composition over the Phase 8 `os` + `node` seams. Cross-thread free is
  Phase 10. Differential proptest (M1--M4 through the heap, 64 cases),
  targeted unit tests (alignment, reuse, refill, realloc, churn, multi-thread
  isolation), miri-clean. Single-thread throughput bench vs mimalloc and the
  system allocator (`benches/heap_alloc.rs`, `docs/HEAP_BENCH.md`): the heap
  matches the system allocator but is ~7--12x slower than mimalloc on the hot
  path; the architecture is structurally correct (same design as mimalloc) and
  the constant-factor gap is implementation overhead targeted for Phase 11.
- **Phase 8 — segment substrate + self-hosted metadata (the Membrane
  Inversion)** (behind a new opt-in `alloc-core` feature). The foundation of a
  real general-purpose allocator: the safe slot-table discipline stops
  *consuming* `Vec<T>` and starts *governing* OS-backed, SEGMENT-aligned memory
  (default 4 MiB), with the allocator's own metadata **carved from the segments
  it manages** (no `Vec`/`HashSet`/`std::alloc` on any alloc path). `unsafe`
  stays confined to exactly two documented seams: `os` (the OS aperture —
  `VirtualAlloc`/`VirtualFree` on windows, `mmap`/`munmap` on unix, via an
  over-reserve+trim for SEGMENT alignment; replaces `std::alloc` entirely) and
  `node` (the intrusive free-list node r/w, generalising the `hand` discipline).
  Everything between — `SegmentTable` (self-hosted generational registry),
  `PageMap`/`BinTable` (per-segment page descriptors + per-class free bins), the
  primordial `bootstrap`, the ~40-class size scheme, and `AllocCore`'s
  single-threaded `alloc`/`dealloc`/`realloc`/`alloc_zeroed` — is pure safe
  integer arithmetic (the Cartographer). Invariants **M1–M8** documented
  (`docs/INVARIANTS.md`, spec in `docs/ALLOC_PLAN.md` §4) and encoded as a
  differential proptest (M1–M4 vs a reference model), targeted unit tests, and a
  **runtime reentrancy audit (M5)** — a counting global allocator proves the
  alloc path never recurses into `std::alloc`. The core is **miri-clean**:
  because miri cannot execute the raw OS FFI, the `os` aperture has a
  `#[cfg(miri)]`-only fallback to `std::alloc` (test instrumentation; the
  production aperture is unchanged and the M5 proof runs without miri). Single
  confined unsafe per seam; `forbid`/`deny(unsafe_code)` everywhere else.
  **Supersedes** the Phase-4 `byte_region.rs` `std::alloc` fallback and its
  `Vec`/`HashSet` metadata. Per-thread heaps (Phase 9), cross-thread free +
  decommit (Phase 10), and the `GlobalAlloc` face (Phase 11) build on this.
- Initial scaffold of the `sefer-alloc` crate.
- Single-threaded `Region<T>` — a thin typed membrane over the
  [`slotmap`](https://crates.io/crates/slotmap) crate (`insert` / `get` /
  `get_mut` / `remove` / `contains` / `iter` / `clear`, all `O(1)`), built under
  `#![forbid(unsafe_code)]`; `slotmap`'s audited `unsafe` owns the dense
  generational engine, including version-saturation slot retirement.
- Typed, copyable `Handle<T>` — a newtype over `slotmap::DefaultKey` with
  hand-written `Copy`/`Eq`/`Hash`/`Debug` impls that hold for every `T`.
- `SyncRegion<T>` — the always-shippable concurrent baseline: a
  `RwLock<Region<T>>` with a guard API plus one-shot convenience methods, with
  poison recovery, still `#![forbid(unsafe_code)]`.
- `LockFreeRegion<T>` (behind the opt-in `experimental` feature) — **lock-free
  reads** via `arc-swap` RCU with page-granularity copy-on-write: readers load
  an immutable snapshot and resolve handles without any lock; rare writers
  serialise, copy only the touched page, and publish atomically. Values live
  behind `Arc<T>`; reclamation is plain `Arc` refcounting. **Zero `unsafe` of
  our own** — the crate stays `#![forbid(unsafe_code)]` with the feature on.
- `EpochRegion<T>` (behind `experimental`) — the fixed-capacity epoch tier with
  O(1) per-slot writes: lock-free reads via a seqlock-validated
  `(generation, value)` publication protocol and `crossbeam-epoch` reclamation.
  Introduces the crate's **single confined `unsafe` organ** (`concurrent::hand`,
  `AtomicSlot<T>`); confinement is compiler-enforced (`#![deny(unsafe_code)]`
  crate-wide under the feature, lifted only in that one module). The publication
  protocol is **loom-model-checked**; live values are dropped on region drop
  (I5). miri cannot run the tier only because `crossbeam-epoch`'s global
  collector is not miri-clean upstream — our `unsafe` is not implicated.
- `ShardedRegion<T>` and `ShardedHandle<T>` (behind `experimental`, Phase 7a) —
  **N-way parallel writes** via the single-writer principle: a `Box<[EpochRegion]>`
  of shards plus a thread-local router that lazily binds each writer thread to one
  shard (atomic round-robin), so two writers in different shards never meet on a
  lock. Reads stay the untouched lock-free `EpochRegion` seqlock. **Pure safe
  composition — zero new `unsafe`**; the module compiles under the crate's
  unsafe-confinement. `ShardedHandle` carries the shard id so reads/removes route
  back to the owning shard. Honest 7a edge: a claimed shard is not released
  (fits a bounded pool of long-lived threads; the shard lifecycle + lock-free
  cross-thread remove land in 7b). A multi-shard differential proptest (I1–I4
  across shards) and a routed concurrent stress test guard it; a write-scaling
  bench (`benches/sharded_write.rs`) compares it to the `SyncRegion` / `Arc<Mutex>`
  baselines.
- **Phase 7b — lock-free cross-thread removal + shard lifecycle** (behind
  `experimental`). A non-owner thread can now `remove` a handle WITHOUT taking
  the owning shard's writer mutex: `AtomicSlot::try_evict_at` performs a
  generation **`compare_exchange`** as the single linearization point — exactly
  one thread wins per generation, so exactly one schedules `defer_destroy` and
  decrements the (now `AtomicUsize`) live count (no double-free, no
  lost-live-value). The freed index is enqueued to a per-shard remote-free queue
  the owner drains on its next op (free list stays owner-only). `EpochRegion`
  gains `remote_evict`; `ShardedRegion::remove` routes owner-path vs lock-free
  remote-path by the calling thread's shard. Shards are now **releasable**: a
  thread-local `Drop` guard frees the shard's `occupied` token on thread exit,
  so a dead thread's shard can be adopted by a new thread while its live slots
  stay resolvable (reads are ownership-free). The relaxed "any thread may evict"
  contract is **loom-model-checked** (`tests/loom_sharded.rs`, 1 owner + 1
  remote-remover + 1 reader, `preemption_bound = 3`) — verified to FAIL on the
  naive load-then-swap protocol. `unsafe` stays confined to `concurrent/hand.rs`.
- **Phase 7c — thread-per-core pinning** (behind a new opt-in `pinning` feature
  = `["experimental", "dep:core_affinity"]`). `ShardedRegion::bind_current_thread_to_shard`
  deterministically routes a thread to a specific shard (the auto round-robin
  claim cannot), and `PinnedRunner` spawns one worker per core, pins worker *i*
  to core *i* (via `core_affinity`, a safe wrapper — **zero new `unsafe`**), and
  binds it to shard *i* — so `shard == core` and the hot path has no lock and no
  cross-shard contention (also why it composes with `glommio`/`monoio`/`tokio`
  current-thread-per-core without "lock across `.await`"). `core_affinity` is an
  **optional** dependency: the default and `experimental` builds do not pull it.
  Pinning is best-effort (honoured per OS); the shard binding (the routing
  truth) always holds, so tests assert routing, not affinity. A `pinned_write`
  bench compares pinned vs unpinned with an honest, workload-dependent verdict.
- **Phase 7d — `ShardedByteArena`** (behind a new opt-in `byte-sharded` feature
  = `["byte"]`, research-flagged). N per-thread `ByteRegion` shards
  (`Box<[Mutex<ByteRegion>]>`) for parallel raw allocation: a thread binds to its
  own shard via a TLS round-robin router, so threads in different shards never
  contend on one lock. Cross-thread `dealloc`/`realloc` route to the owning shard
  via a scan over `ByteRegion::contains_ptr` (safe pointer-comparison, no
  dereference) — a pointer is never freed against the wrong shard. `prewarm()`
  carves a chunk per shard and touches its pages up front to remove cold-start
  latency (callable from a background thread; the arena is `Send + Sync`). The
  only added `unsafe` is a one-line `unsafe impl Send for ByteRegion` (the region
  owns all its memory; access is `Mutex`-serialised) — everything else is safe
  composition; `unsafe` stays confined to `src/byte/*`. Correctness (cross-thread
  free, concurrent per-shard churn, bounded chunk growth, realloc byte
  preservation) is covered by `tests/byte_sharded.rs` and is **miri-clean**.
  Honest verdict (`docs/BYTE_SHARDED_BENCH.md`): it parallelises across shards
  but is NOT a `mimalloc` competitor and never returns memory to the OS until
  drop — research, not production.
- `ByteRegion` and `ByteAllocator` (behind the research-flagged `byte` feature)
  — the descent to raw bytes: a size-classed free-list byte arena whose
  placement logic is pure safe integer arithmetic (the Cartographer), with the
  single irreducible `*mut u8` aperture confined and documented, plus an
  experimental `unsafe impl GlobalAlloc` delegating through a `Mutex`. The
  second confined-`unsafe` module; confinement stays compiler-enforced. The
  whole byte tier is **miri-clean**. Honest scope: it does not aim to beat the
  system allocator / `mimalloc` (see `docs/BYTE_BENCH.md`); resocks5's global
  allocator stays `mimalloc` regardless.
- Safety invariants I1–I5 documented (`docs/INVARIANTS.md`) and encoded as
  unit tests plus a proptest differential harness against a reference model
  (`tests/differential.rs`).
- Full detailed implementation plan — per-phase goals, deliverables, steps, and
  gates, plus dependency DAG, risk register, decisions log, and open questions
  (`docs/PLAN.md`) — alongside architecture notes (`docs/DESIGN.md`).
- Dual MIT / Apache-2.0 licensing; MSRV pinned to 1.88.
