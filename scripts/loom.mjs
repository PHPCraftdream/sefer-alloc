// loom sweep — model-checks the concurrency protocols (registry claim/recycle,
// remote-free ring, cross-thread free, fallback init, bootstrap CAS, epoch,
// sharded). Native (no WSL): loom is a pure-Rust dependency gated behind the
// `--cfg loom` build cfg.
//
// Usage (from repo root):
//   node scripts/loom.mjs           # all loom_* test files
//   node scripts/loom.mjs loom_fallback_init   # a subset
//   npm run loom

import { REPO_ROOT, run, verdict } from './lib.mjs';

// Per-test feature sets — MUST mirror the ci.yml `loom` matrix. Running every
// model with one blanket `alloc-global,alloc-xthread` set silently compiled the
// experimental-tier models (`loom_sharded`, `loom_epoch`) with ZERO tests (their
// `#![cfg(feature = "experimental")]` gate excluded the whole file → "0 tests"
// that looks like a pass). Each model now builds under the exact features its
// `#![cfg(...)]` gate requires — identical to what CI runs.
// CRATE-P3: the four in-tree shadow-model harnesses (loom_bootstrap_cas,
// loom_chunk_cas, loom_overflow_sidecar_cas, loom_fallback_init) collapsed into
// ONE suite that model-checks the REAL `racy_ptr_cell::RacyPtrCell` type (the
// crate aliases its atomics to `loom` under `--cfg loom`). It lives in the
// `racy-ptr-cell` crate, not sefer's `tests/`, so it is run with `-p
// racy-ptr-cell` and no sefer features — flagged by the `crate:` prefix on its
// feature-map value, handled specially in the run loop below.
const CRATE_PREFIX = 'crate:';

const FEATURES = {
  loom_racy_ptr_cell: `${CRATE_PREFIX}racy-ptr-cell`,
  // CRATE-P4: the extracted `ring-mpsc` crate ships a real-type loom suite for
  // the MPSC ring + DirtyRouter protocols (the crate aliases its atomics to
  // `loom` under `--cfg loom`), run with `-p ring-mpsc` and no sefer features —
  // flagged by the `crate:` prefix, handled specially in the run loop below.
  // This is ADDITIVE: the seven in-tree shadow models below (loom_remote_ring,
  // loom_remote_ring_drain_guard, loom_heap_overflow,
  // loom_heap_overflow_drain_guard, loom_overflow_first_retry,
  // loom_dirty_publish, loom_dirty_multi_segment) STAY, because the shipping
  // in-tree `RemoteFreeRing`/`HeapOverflow` rings are NOT yet wired onto the
  // crate (the in-tree swap is a deferred follow-up), so their own loom coverage
  // must remain until that swap lands — otherwise the actual shipping rings
  // would have NO loom coverage.
  loom_ring_mpsc: `${CRATE_PREFIX}ring-mpsc`,
  // CRATE-P7: the extracted `tagged-index-stack` crate ships a real-type loom
  // suite for the ABA-tagged Treiber free-index stack (the crate aliases its
  // atomics to `loom` under `--cfg loom`), run with `-p tagged-index-stack` and
  // no sefer features — flagged by the `crate:` prefix. This REPLACES the
  // in-tree `loom_free_slots_aba` shadow model below: `heap_registry.rs` now
  // consumes the crate's `TaggedIndexStack`, so the crate's real-type suite IS
  // the coverage for the shipping code (the shadow model is deleted).
  loom_aba: `${CRATE_PREFIX}tagged-index-stack`,
  loom_xthread_protocol: 'alloc-core,alloc-xthread',
  loom_remote_ring: 'alloc-core,alloc-xthread',
  // task #52 (PERF-PASS-4, G9/C2): the ring-drain empty-guard model.
  loom_remote_ring_drain_guard: 'alloc-core,alloc-xthread',
  // #141: the A1 deferred-large push/drain model (found the #143 push leak).
  loom_deferred_large: 'alloc-core,alloc-xthread',
  // R2 (#154) + #164: magazine↔RemoteFreeRing composition shadow model.
  // `compose_finds_double_issue_hole_pre164` (#[should_panic] counterfactual)
  // + `compose_drain_sees_magazine_invariant_holds` (GREEN invariant, #164).
  loom_magazine_ring_compose: 'alloc-global,alloc-xthread',
  // task #204: the `alloc` Cargo feature (and the `Heap` type it gated) was
  // REMOVED and renamed to alloc-core/alloc-xthread/alloc-global. This mapping
  // still pointed at the deleted `alloc` feature, so cargo silently ignored the
  // unknown feature — but this file's synthetic `Node` model never depended on
  // it (only `#![cfg(loom)]`, no crate symbols): it compiles + passes with an
  // EMPTY feature set, which is exactly what the ci.yml `loom` matrix runs
  // (`- test: loom_thread_free / features: ""`). Mirror CI: empty feature set.
  loom_thread_free: '',
  // R7-A4: dirty-segment publish/swap/lost-wakeup model.
  loom_dirty_publish: 'alloc-core,alloc-xthread',
  // R7-A5: dirty word with multiple segments — two producers set bits for
  // different segments in the same u64 word.
  loom_dirty_multi_segment: 'alloc-core,alloc-xthread',
  // R6-OPT-P0-4: overflow-first composition (segment ring -> heap overflow ring
  // -> bounded spin-retry) double-saturation model + its counterfactual.
  loom_overflow_first_retry: 'alloc-global,alloc-xthread',
  // RAD-4b: HeapOverflow two-field-entry MPSC ring (torn-read counterfactual).
  loom_heap_overflow: 'alloc-global,alloc-xthread',
  // R2-4: HeapOverflow drain-guard (the return-actual-stop-position contract).
  loom_heap_overflow_drain_guard: 'alloc-global,alloc-xthread',
  loom_sharded: 'experimental',
  loom_epoch: 'experimental',
};

const ALL = Object.keys(FEATURES);

const tests = process.argv.slice(2).length ? process.argv.slice(2) : ALL;

// Group tests that share a feature set into one cargo invocation (fewer
// rebuilds), preserving each test's correct gate.
const byFeature = new Map();
for (const t of tests) {
  // NB: use `in`/hasOwnProperty, NOT `if (!f)` — a valid feature value can be
  // the EMPTY string (`loom_thread_free: ''`, mirroring the ci.yml matrix). A
  // falsy `!f` check would mis-classify that legitimate entry as "unknown test"
  // and abort — the very stale-name → 0-runs class of bug this script guards.
  if (!Object.prototype.hasOwnProperty.call(FEATURES, t)) {
    console.error(`[loom] unknown test "${t}" — not in the feature map`);
    process.exit(2);
  }
  const f = FEATURES[t];
  if (!byFeature.has(f)) byFeature.set(f, []);
  byFeature.get(f).push(t);
}

console.log(`[loom] tests: ${tests.join(', ')}\n`);

// Regression-guard against the stale-feature-name → silent-0-runs class of bug
// (task #29: `loom_thread_free` was mapped to the DELETED `alloc` feature and
// never actually selected). Log the resolved test count per entry up front, and
// hard-fail if any entry selected ZERO tests — a mapping should never resolve to
// an empty group.
for (const [features, group] of byFeature) {
  const label = features === '' ? '(no features)' : `--features ${features}`;
  console.log(`[loom] ${label}: ${group.length} test(s) selected — ${group.join(', ')}`);
  if (group.length === 0) {
    console.error(`[loom] FAIL: 0 tests selected for ${label} — stale/empty feature mapping`);
    process.exit(2);
  }
}
console.log('');

let allOk = true;
for (const [features, group] of byFeature) {
  const isCrate = features.startsWith(CRATE_PREFIX);
  const crateName = isCrate ? features.slice(CRATE_PREFIX.length) : null;
  const label = isCrate
    ? `-p ${crateName}`
    : features === ''
      ? '(no features)'
      : `--features ${features}`;
  console.log(`\n[loom] ${label}: ${group.join(', ')}`);
  const testArgs = group.flatMap((t) => ['--test', t]);
  // A `crate:<name>` entry runs the extracted crate's own real-type loom suite
  // via `-p <name>` and NO sefer features (the crate has none). Otherwise:
  // an empty feature set must OMIT `--features` entirely — cargo rejects an
  // empty `--features ''` argument (mirrors the ci.yml `loom_thread_free`
  // features: "" entry).
  const scopeArgs = isCrate
    ? ['-p', crateName]
    : features === ''
      ? []
      : ['--features', features];
  const { code, out } = await run(
    'cargo',
    ['test', '--release', ...scopeArgs, ...testArgs],
    {
      cwd: REPO_ROOT,
      env: { ...process.env, RUSTFLAGS: `${process.env.RUSTFLAGS ?? ''} --cfg loom`.trim() },
      shell: true,
    },
  );
  // Guard against the silent "0 tests" trap: a feature-gated-out file reports
  // "running 0 tests" and exit 0. If NO test binary actually ran a test, treat
  // the group as a failure so a mis-mapped feature set can never look green.
  const ranSomething = /running [1-9]\d* test/.test(out) || /test result: ok\. [1-9]/.test(out);
  const ok = verdict(`loom ${label}`, code, out) && ranSomething;
  if (!ranSomething) {
    console.log(`[loom ${label}] FAIL (0 tests ran — feature gate excluded the model)`);
  }
  allOk = allOk && ok;
}

process.exit(allOk ? 0 : 1);
