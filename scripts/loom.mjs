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

const ALL = [
  'loom_bootstrap_cas',
  // #141: the A1 deferred-large push/drain model (found the #143 push leak)
  // and the free_slots/TaggedPtr ABA model. Both gates are covered by the
  // alloc-global,alloc-xthread feature superset used below.
  'loom_deferred_large',
  'loom_epoch',
  'loom_fallback_init',
  'loom_free_slots_aba',
  // R2 (#154): the magazine↔RemoteFreeRing composition shadow model. Its
  // primary test is a `#[should_panic]` counterfactual pinning the #164
  // residual hole (green while the hole exists; flips to a green invariant
  // test when #164 lands). Pure `cfg(loom)` — no feature gate of its own.
  'loom_magazine_ring_compose',
  'loom_registry',
  'loom_remote_ring',
  'loom_sharded',
  'loom_thread_free',
  'loom_xthread_protocol',
];

const tests = process.argv.slice(2).length ? process.argv.slice(2) : ALL;
const testArgs = tests.flatMap((t) => ['--test', t]);

console.log(`[loom] tests: ${tests.join(', ')}\n`);

const { code, out } = await run(
  'cargo',
  [
    'test',
    '--release',
    '--features',
    'alloc-global,alloc-xthread',
    ...testArgs,
  ],
  {
    cwd: REPO_ROOT,
    env: { ...process.env, RUSTFLAGS: `${process.env.RUSTFLAGS ?? ''} --cfg loom`.trim() },
    shell: true,
  },
);

const ok = verdict('loom', code, out);
process.exit(ok ? 0 : 1);
