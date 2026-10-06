// Loom sweep of sidecar terminal publication, owner drain, registry leases,
// and the independent workspace concurrency types. Every root model has an
// exact feature set; cfg loom additionally requires tagged-index-stack/loom in
// EVERY root build (the crate is a cfg(any(loom, kani)) target dependency).
//
// Usage (from repo root):
//   node scripts/loom.mjs
//   node scripts/loom.mjs loom_terminal_owner_drain
//   npm run loom

import { REPO_ROOT, run, verdict } from './lib.mjs';

// Per-test feature sets — MUST mirror the ci.yml `loom` matrix. When editing,
// diff against ci.yml:2636 (`loom-misc` job) for the exact feature string —
// this file drifted out of sync with that line once already (fixed by
// 8027d9a) because there was no pointer to the exact line to check.
// Running every model with one blanket `alloc-global,alloc-xthread` set silently compiled the
// experimental-tier models (`loom_sharded`, `loom_epoch`) with ZERO tests (their
// `#![cfg(feature = "experimental")]` gate excluded the whole file → "0 tests"
// that looks like a pass). Each model now builds under the exact features its
// `#![cfg(...)]` gate requires — identical to what CI runs.
// CRATE-P3: the four in-tree shadow-model harnesses (loom_bootstrap_cas,
// loom_chunk_cas, loom_overflow_sidecar_cas, loom_fallback_init) collapsed into
// ONE suite that model-checks the REAL `once_ptr_cell::OncePtrCell` type (the
// crate aliases its atomics to `loom` under `--cfg loom`). It lives in the
// `once-ptr-cell` crate, not sefer's `tests/`, so it is run with `-p
// once-ptr-cell` and no sefer features — flagged by the `crate:` prefix on its
// feature-map value, handled specially in the run loop below.
const CRATE_PREFIX = 'crate:';

const FEATURES = {
  loom_once_ptr_cell: `${CRATE_PREFIX}once-ptr-cell`,
  // CRATE-P7: the extracted `tagged-index-stack` crate ships a real-type loom
  // suite for the ABA-tagged Treiber free-index stack (the crate aliases its
  // atomics to `loom` under `--cfg loom`), run with `-p tagged-index-stack` and
  // no sefer features — flagged by the `crate:` prefix. This REPLACES the
  // in-tree `loom_free_slots_aba` shadow model below: `heap_registry.rs` now
  // binds the crate's `StackHead` through its `StackStorage` impl, so the
  // crate's real-type suite IS
  // the coverage for the shipping code (the shadow model is deleted).
  loom_aba: `${CRATE_PREFIX}tagged-index-stack`,
  loom_sidecar_bitmap: 'alloc-core,alloc-xthread,tagged-index-stack/loom',
  loom_terminal_large: 'alloc-core,alloc-xthread,tagged-index-stack/loom',
  loom_terminal_owner_drain: 'alloc-core,alloc-xthread,tagged-index-stack/loom',
  loom_registry_free_slots: 'alloc-global,alloc-xthread,tagged-index-stack/loom',
  loom_r8_maintenance_lease: 'alloc-global,alloc-xthread,internals,tagged-index-stack/loom',
  loom_active_kind_index: 'alloc-global,alloc-xthread,internals,tagged-index-stack/loom',
  loom_r11_registry_claim: 'alloc-global,alloc-xthread,internals,tagged-index-stack/loom',
  loom_r11_ph4a_heap_lease: 'alloc-global,alloc-xthread,internals,tagged-index-stack/loom',
  loom_r11_ph4b_publish_recycle_drain: 'alloc-global,alloc-xthread,internals,tagged-index-stack/loom',
  loom_r11_small_sidecar: 'alloc-core,alloc-xthread,tagged-index-stack/loom',
  loom_sharded: 'experimental,tagged-index-stack/loom',
  loom_epoch: 'experimental,tagged-index-stack/loom',
  loom_r11_epoch_false_full: 'experimental,tagged-index-stack/loom',
};

const ALL = Object.keys(FEATURES);

const tests = process.argv.slice(2).length ? process.argv.slice(2) : ALL;

// Group tests that share a feature set into one cargo invocation (fewer
// rebuilds), preserving each test's correct gate.
const byFeature = new Map();
for (const t of tests) {
  // Use membership, not truthiness: an empty feature set is valid.
  if (!Object.prototype.hasOwnProperty.call(FEATURES, t)) {
    console.error(`[loom] unknown test "${t}" — not in the feature map`);
    process.exit(2);
  }
  const f = FEATURES[t];
  if (!byFeature.has(f)) byFeature.set(f, []);
  byFeature.get(f).push(t);
}

console.log(`[loom] tests: ${tests.join(', ')}\n`);

// Reject an empty selected group before launching Cargo.
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
  // Workspace suites are crate-scoped. tagged-index-stack needs its optional
  // loom dependency feature as well as cfg loom; root models forward that
  // feature explicitly in every FEATURES entry.
  const scopeArgs = isCrate
    ? crateName === 'tagged-index-stack'
      ? ['-p', crateName, '--features', 'loom']
      : ['-p', crateName]
    : features === ''
      ? []
      : ['--features', features];
  const { code, out } = await run(
    'cargo',
    ['test', '--release', ...scopeArgs, ...testArgs],
    {
      cwd: REPO_ROOT,
      env: { ...process.env, RUSTFLAGS: `${process.env.RUSTFLAGS ?? ''} --cfg loom`.trim() },
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
