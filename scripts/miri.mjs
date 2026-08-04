// miri sweep — UB detection under strict provenance on the invariant / segment
// / align-regression tests. Native (nightly miri component). Mirrors the CI
// miri matrix in .github/workflows/ci.yml.
//
// Usage (from repo root):
//   node scripts/miri.mjs           # the full CI miri matrix (strict provenance)
//   node scripts/miri.mjs decommit_miri_cycle   # a subset (by test name)
//   node scripts/miri.mjs --plain   # the PLAIN-provenance matrix (exposed-
//                                    # provenance stacks; see PLAIN_MATRIX below)
//   node scripts/miri.mjs --plain regression_heap_xthread_large_free_no_leak
//   npm run miri
//
// Each entry is [features, testName]; miri is slow (segment tests run 1-8 min
// each), so keep the set to the focused invariant/UB targets per the project's
// short-scenario policy — not the whole suite.

import { REPO_ROOT, run, verdict } from './lib.mjs';

const MATRIX = [
  ['experimental', 'region_invariants'],
  // R34-5-followup (task #524): `internals` added — `decommit_miri_cycle`'s
  // `#![cfg(...)]` gate (added by R34-3/task #522) requires it; without it
  // this entry silently compiled to 0 tests (the "pass by absence" class
  // R13-5 fixed elsewhere in this project — this script was simply never
  // updated when R34-3 introduced the `internals` feature).
  ['alloc-core alloc-decommit internals', 'decommit_miri_cycle'],
  // R34-5-followup: `internals` added, same reason as above.
  ['alloc-global alloc-xthread internals', 'reclaim_offset_unit'],
  // task #52 (PERF-PASS-4, G9/C2): the ring-drain empty-guard's
  // `SegmentHeader::ring_drain_head` field, exercised via a REAL
  // `find_segment_with_free` scan (not the unconditional `dbg_drain_all_rings`
  // force-drain `reclaim_offset_unit` uses).
  // R34-5-followup: `internals` added, same reason as above.
  ['alloc-global alloc-xthread internals', 'regression_ring_drain_guard_miri'],
  ['alloc-core', 'regression_large_align_no_segment_exhaustion'],
  ['alloc-core', 'regression_page_aligned_no_segment_exhaustion'],
  ['alloc-core', 'regression_realloc_cross_class_shrink'],
  // R2-1 (T2): the move-leg OOB-read guard. The bogus `realloc(16b, 8 MiB,
  // 8 MiB)` scenario would `copy_nonoverlapping` 8 MiB out of a 4 MiB
  // segment — a read that escapes the segment's OS allocation. Under the fix
  // the span-consistency check returns null before any allocation/copy, so
  // this run validates the GREEN path is UB-free (the RED path's 8 MiB alloc
  // + OOB read is too slow for the miri matrix; verified RED under native
  // `cargo test` instead). Strict-provenance-clean (own-thread substrate
  // path, real sefer pointer, `contains_base`-proven base).
  ['alloc-core', 'regression_realloc_oob_old_layout'],
  // R3 (#155): fastbin / production-path miri coverage. The Э6 M2 oracle
  // strict-provenance claim (free path never touches the block body), the Э1
  // bump-direct carve pointer math (storm capped under cfg(miri)), and the Э3
  // own-segment cache invalidation on decommit.
  // R34-5-followup: `internals` added — this test's `#![cfg(...)]` gate
  // (added by R34-3/task #522) requires it.
  [
    'alloc-global alloc-xthread alloc-decommit fastbin internals',
    'regression_magazine_oracles',
  ],
  [
    'alloc-global alloc-xthread alloc-decommit fastbin',
    'regression_bump_direct_refill',
  ],
  // S3 (#168): the deterministic single-thread boundary sweep (S2) under miri —
  // UB-free pointer math / provenance across the size×align seam grid + the
  // realloc matrix. The grid is drastically shrunk under `cfg(miri)` inside the
  // test (a representative size/align subset, 4 realloc pairs, windowed canary)
  // so it finishes in ~40s; the native (non-miri) grid is exhaustive & unchanged.
  ['alloc-core', 'stress_boundary_sweep'],
  // PERF-PASS-2 (G5/C1, task #50): the virgin-segment `AllocBitmap` init-
  // elision poison-then-assert counterfactual. Under `cfg(miri)` the skip
  // does NOT fire (miri's `std::alloc` fallback is not guaranteed zeroed, so
  // the explicit zero-init stays unconditional there — see the matching
  // comments at both call sites) — so T1/T2's "reads back zero" assertions
  // hold trivially under miri regardless of the skip. What miri DOES usefully
  // scrutinise here is the new `dbg_alloc_bitmap_bytes_for` test-only
  // accessor's raw pointer-offset read loop (new code, `Node::offset`/
  // `Node::read_u8` in a loop over a caller-provided `out` slice) and T3's
  // M2 double-free-guard exercise on a freshly-reserved segment, for
  // strict-provenance UB.
  // R34-5-followup: `internals` added — this test's `#![cfg(...)]` gate
  // (added by R34-3/task #522) requires it.
  ['alloc-core internals', 'regression_virgin_bitmap_skip'],
  // W3: the stats-aggregator Stacked-Borrows counterfactual. The default
  // (non-ignored) test asserts the W3 shape — counter read off a shared
  // `&Slot`, never forming `&HeapCore` over the owner's protected `&mut` — is
  // SB-clean. The `#[ignore]`d `old_pattern_is_sb_ub` in the same file
  // reproduces the pre-W3 UB on demand (run with `-- --ignored`). Tiny and
  // fast under miri (no segment reservation — it models the aliasing shape).
  ['std', 'regression_w3_stats_aliasing_miri'],
  // R7-A5: directory sidecar below-threshold path under strict provenance.
  // The above-threshold path (materialising 32+ segments) is impractically
  // slow under miri; the below-threshold path exercises the null-pointer
  // guard + publish helpers + try_materialise early return.
  ['alloc-segment-directory', 'segment_directory_a5_miri'],
  // `regression_own_segment_cache_invalidation` deferred from the miri set
  // (R3, #155): ~100k interpreted allocations (18_000 blocks × 6 segments,
  // count is invariant-load-bearing so it cannot be cfg(miri)-capped) does not
  // finish in a CI-acceptable time. Its UB surface is covered by
  // `decommit_miri_cycle`.
];

// W6: the PLAIN-provenance matrix. `src/registry/bootstrap.rs` (~lines 126-136)
// documents that the exposed-provenance intrusive stacks — the A1
// `deferred_large` push/drain stack and the `abandoned_segs` stack — pack real
// pointer addresses via `expose_provenance` and re-derive them via
// `with_exposed_provenance_mut` BY DESIGN. That wildcard-provenance shape is
// rejected under `-Zmiri-strict-provenance` (correctly — it is the documented
// structural limit, not a bug), so these tests get ZERO miri coverage in the
// strict MATRIX above. Run them under PLAIN miri (Stacked Borrows, non-strict
// provenance — miri's default) instead: the `push.rs` / `drain.rs` /
// `heap_registry.rs` / `node.rs` pairs ARE validatable there. Small N per test
// (Large allocs, <=100 iterations) keeps each run miri-affordable. Kept SEPARATE
// from the strict MATRIX — a strict-clean test must NOT move here and vice-versa.
// Under plain miri the `expose_provenance`/`with_exposed_provenance_mut` pairs
// surface as integer-to-pointer cast WARNINGS (validated) — strict miri would
// hard-ERROR on the same casts, which is the whole reason for a plain job.
// Verified locally: `regression_xthread_large_free_no_leak` → 3 passed (~156s).
//
// NOT here: the explicit-`Heap`-face tests
// (`regression_heap_xthread_large_free_no_leak`,
// `regression_xthread_large_free_layout_mismatch`) call `Heap::new()` on a
// SPAWNED thread; that thread's per-thread primordial 4 MiB segment goes
// unreachable at thread exit, so miri's leak checker reports it — a per-thread-
// `Heap` miri artifact, NOT the exposed-provenance path (its p2i re-derivations
// warn cleanly there too). Suppressing it needs `-Zmiri-ignore-leaks`, which
// would void the "no_leak" oracle. Their cross-thread reclaim is covered on
// REAL threads under TSan (see scripts/tsan.mjs) instead.
const PLAIN_MATRIX = [
  // A1 deferred-large stack over the `SeferAlloc`/`HeapCore` (global) face.
  // R34-5-followup (task #524): `internals` added — this test's
  // `#![cfg(...)]` gate (added by R34-3/task #522) requires it; without it
  // this entry (and the two below, before this fix) compiled its `--test`
  // binary with the module `#[cfg]`d entirely out, so `cargo miri test` ran
  // "0 tests" and exited 0 -- a silent PASS that validated nothing (the
  // matrix-selection smoke-guard below only checks the row COUNT, not
  // whether the resulting binary actually contained any tests). CI's own
  // `ci.yml` miri-plain job was unaffected (it passes `internals` explicitly
  // on the command line, independent of this script's MATRIX), but this
  // LOCAL convenience script silently stopped validating anything for all
  // three plain-matrix entries from R34-3 until this fix (caught during
  // R34-5's zero-trust review).
  ['alloc-global alloc-xthread internals', 'regression_xthread_large_free_no_leak'],
  // task H1: the `thread_free` aliasing guard. Runs an owner `&mut HeapCore`
  // alloc loop CONCURRENTLY (real overlap, not the phase-serialised shape of
  // the test above) with a remote thread CASing the owner's cross-thread
  // free-stack head. BEFORE the H1 fix (head inline in `HeapCore`) this
  // reported a retag-write-vs-atomic-load data race under plain miri; AFTER
  // the fix (head hoisted into the `Sync` `HeapSlot` / `FALLBACK_TFS`, outside
  // every `&mut HeapCore` retag range) it is clean. Needs the elevated
  // preemption rate (see PLAIN_MIRIFLAGS) so the scheduler lands a remote CAS
  // inside a live owner alloc frame.
  [
    'alloc-global alloc-xthread internals',
    'regression_xthread_thread_free_alias_miri',
  ],
  // R34-5 (task #524, audit finding G1): the multi-producer SMALL-block
  // `RemoteFreeRing` push/drain path. The two entries above cover only the
  // LARGE cross-thread path (deferred_large AtomicPtr stack / thread_free
  // aliasing). This entry exercises 2 producer threads concurrently pushing
  // small-block offsets into the SAME per-segment ring (`Node::atomic_u32_at`
  // CAS-reserve) while the owner allocates (real `&mut HeapCore` overlap),
  // then force-drains. Needs the elevated preemption rate so the scheduler
  // interleaves a producer ring-push inside a live owner alloc frame.
  // Verified locally: 1 passed (~49s).
  [
    'alloc-global alloc-xthread internals',
    'regression_xthread_small_ring_miri',
  ],
];

const args = process.argv.slice(2);
const plain = args.includes('--plain');
// The positional args are TEST NAMES (each MATRIX entry is `[features, test]`).
// They are NOT feature names: an entry with several features
// (`'alloc-global alloc-xthread alloc-decommit fastbin'`) must be selected as a
// whole by its test name — never token-matched against the space-joined feature
// string. Filter strictly on the test name (column 1) to keep that distinction.
const filter = args.filter((a) => a !== '--plain');
const matrix = plain ? PLAIN_MATRIX : MATRIX;
const knownTests = new Set(matrix.map(([, t]) => t));

// Regression-guard against the silent-0-runs class of bug (task #29 in loom.mjs;
// task #18 here): a filter token that matches NO test name — a stale/typo test
// name, or a bare feature token mistaken for a test name (e.g. `alloc-decommit`,
// one of the feature words of a multi-feature entry) — must hard-fail loudly, not
// silently drop to an empty run. Validate every requested name up front.
const unknown = filter.filter((t) => !knownTests.has(t));
if (unknown.length) {
  console.error(
    `[miri] unknown test name(s): ${unknown.join(', ')} — not a test in the ${
      plain ? 'PLAIN_MATRIX' : 'MATRIX'
    }. Pass test names (column 1 of the matrix), not feature names.`,
  );
  console.error(`[miri] known tests: ${[...knownTests].join(', ')}`);
  process.exit(2);
}

const entries = filter.length
  ? matrix.filter(([, t]) => filter.includes(t))
  : matrix;

// Smoke-guard (mirrors loom.mjs): report the resolved entry count and hard-fail
// if it is ZERO — a matrix/filter combination should never resolve to an empty
// run, which would look green while validating nothing.
console.log(
  `[miri] ${plain ? 'PLAIN' : 'strict'} matrix: ${entries.length} entr${
    entries.length === 1 ? 'y' : 'ies'
  } selected — ${entries.map(([, t]) => t).join(', ') || '(none)'}`,
);
if (entries.length === 0) {
  console.error(
    `[miri] FAIL: 0 entries selected — stale/empty matrix or filter matched nothing`,
  );
  process.exit(2);
}

// The strict job pins `-Zmiri-strict-provenance`; the plain job DROPS it (the
// exposed-provenance re-derivations require the default, non-strict model). Both
// keep `-Zmiri-disable-isolation`.
// The plain job adds an elevated `-Zmiri-preemption-rate` so the scheduler
// interleaves a remote cross-thread-free CAS INSIDE a live owner `alloc(&mut
// self)` frame — the schedule the task H1 aliasing guard
// (`regression_xthread_thread_free_alias_miri`) and the R34-5 multi-producer
// small-ring test (`regression_xthread_small_ring_miri`) need to exercise.
// The remaining plain test (`regression_xthread_large_free_no_leak`) is
// phase-serialised and indifferent to the rate.
const env = {
  ...process.env,
  MIRIFLAGS: plain
    ? '-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5'
    : '-Zmiri-strict-provenance -Zmiri-disable-isolation',
};

let allOk = true;
for (const [features, test] of entries) {
  console.log(`\n[miri] ${test} (features: ${features})`);
  // `run()` defaults to `shell: false`, so the space-joined `features` value
  // reaches cargo as ONE argv element (no shell to re-split it on whitespace).
  // The previous COMMA-join here existed only to dodge `shell: true`'s
  // whitespace-splitting (DEP0190); cargo accepts `--features "a b c"` and
  // `--features "a,b,c"` identically either way.
  const featuresArg = features.trim();
  const { code, out } = await run(
    'cargo',
    ['+nightly', 'miri', 'test', '--features', featuresArg, '--test', test],
    { cwd: REPO_ROOT, env },
  );
  allOk = verdict(`miri:${test}`, code, out) && allOk;
}

console.log(`\n[miri] overall: ${allOk ? 'PASS' : 'FAIL'}`);
process.exit(allOk ? 0 : 1);
