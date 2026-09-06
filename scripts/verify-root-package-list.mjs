import { spawnSync } from 'node:child_process';
import { REPO_ROOT } from './lib.mjs';

const exactForbidden = new Set([
  'tests/tagged_index_stack_compile_fail.rs',
  'tests/tagged_index_stack_ab_runner_scratch_guard.rs',
  'tests/support/tagged_index_stack_compile_fail.rs',
]);

function isForbidden(file) {
  return (
    exactForbidden.has(file) ||
    file.startsWith('docs/perf/_raw_tis_p3_ab_') ||
    /^docs\/perf\/TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_.+\.csv$/.test(file)
  );
}

const result = spawnSync(
  'cargo',
  ['package', '-p', 'sefer-alloc', '--list', '--allow-dirty'],
  {
    cwd: REPO_ROOT,
    encoding: 'utf8',
  },
);

if (result.error || result.status !== 0) {
  console.error(
    `[verify-root-package-list] cargo package --list failed${result.error ? `: ${result.error.message}` : ` (exit ${result.status})`}`,
  );
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  process.exit(1);
}

const files = result.stdout
  .split(/\r?\n/)
  .map((line) => line.trim().replaceAll('\\', '/'))
  .filter(Boolean);

if (files.length === 0) {
  console.error('[verify-root-package-list] cargo package --list returned no paths');
  process.exit(1);
}

const leaked = files.filter(isForbidden);
if (leaked.length > 0) {
  console.error('[verify-root-package-list] forbidden root-package paths leaked:');
  for (const file of leaked) console.error(`  - ${file}`);
  process.exit(1);
}

console.log(
  `[verify-root-package-list] OK — ${files.length} package paths checked; forbidden paths absent`,
);
