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
  'loom_epoch',
  'loom_fallback_init',
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
