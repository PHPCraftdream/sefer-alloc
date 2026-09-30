// Every root Loom target must run in CI and the local Loom matrix.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const tests = readdirSync(join(REPO_ROOT, 'tests'))
  .filter((name) => /^loom_[A-Za-z0-9_]+\.rs$/.test(name))
  .map((name) => name.slice(0, -3));
const ci = readFileSync(join(REPO_ROOT, '.github/workflows/ci.yml'), 'utf8')
  .split(/\r?\n/)
  .filter((line) => !/^\s*#/.test(line))
  .join('\n');
const local = readFileSync(join(REPO_ROOT, 'scripts/loom.mjs'), 'utf8')
  .split(/\r?\n/)
  .filter((line) => !/^\s*\/\//.test(line))
  .join('\n');
const ciTargets = new Set([...ci.matchAll(/--test\s+(loom_[A-Za-z0-9_]+)/g)].map((match) => match[1]));
const localTargets = new Set([...local.matchAll(/^\s*(loom_[A-Za-z0-9_]+):\s*['`]/gm)].map((match) => match[1]));

const missing = tests.flatMap((name) => [
  ...(!ciTargets.has(name) ? [`${name}: CI`] : []),
  ...(!localTargets.has(name) ? [`${name}: scripts/loom.mjs`] : []),
]);
if (tests.length === 0 || missing.length !== 0) {
  console.error(`[verify-loom-target-wiring] FAIL: ${missing.join(', ') || 'no root Loom targets'}`);
  process.exit(1);
}
console.log(`[verify-loom-target-wiring] OK: ${tests.length} root Loom targets wired in CI and local matrix`);
