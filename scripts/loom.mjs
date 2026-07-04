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
const FEATURES = {
  loom_bootstrap_cas: 'alloc-global',
  loom_fallback_init: 'alloc-global',
  loom_registry: 'alloc-global',
  loom_free_slots_aba: 'alloc-global',
  loom_xthread_protocol: 'alloc-core,alloc-xthread',
  loom_remote_ring: 'alloc-core,alloc-xthread',
  // #141: the A1 deferred-large push/drain model (found the #143 push leak).
  loom_deferred_large: 'alloc-core,alloc-xthread',
  // R2 (#154): the magazine↔RemoteFreeRing composition shadow model. Its
  // primary test is a `#[should_panic]` counterfactual pinning the #164
  // residual hole (green while the hole exists; flips to a green invariant
  // test when #164 lands).
  loom_magazine_ring_compose: 'alloc-global,alloc-xthread',
  loom_thread_free: 'alloc',
  loom_sharded: 'experimental',
  loom_epoch: 'experimental',
};

const ALL = Object.keys(FEATURES);

const tests = process.argv.slice(2).length ? process.argv.slice(2) : ALL;

// Group tests that share a feature set into one cargo invocation (fewer
// rebuilds), preserving each test's correct gate.
const byFeature = new Map();
for (const t of tests) {
  const f = FEATURES[t];
  if (!f) {
    console.error(`[loom] unknown test "${t}" — not in the feature map`);
    process.exit(2);
  }
  if (!byFeature.has(f)) byFeature.set(f, []);
  byFeature.get(f).push(t);
}

console.log(`[loom] tests: ${tests.join(', ')}\n`);

let allOk = true;
for (const [features, group] of byFeature) {
  console.log(`\n[loom] --features ${features}: ${group.join(', ')}`);
  const testArgs = group.flatMap((t) => ['--test', t]);
  const { code, out } = await run(
    'cargo',
    ['test', '--release', '--features', features, ...testArgs],
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
  const ok = verdict(`loom ${features}`, code, out) && ranSomething;
  if (!ranSomething) {
    console.log(`[loom ${features}] FAIL (0 tests ran — feature gate excluded the model)`);
  }
  allOk = allOk && ok;
}

process.exit(allOk ? 0 : 1);
