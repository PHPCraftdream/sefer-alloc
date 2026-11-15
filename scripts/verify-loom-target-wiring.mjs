// Every root Loom target must run in CI and the local Loom matrix.
// P1.2: member crates' Loom targets (crates/<theme>/<crate>/tests/loom_*.rs)
// must also be wired: each must appear as `--test <name>` in the expanded CI
// text and have a scripts/loom.mjs FEATURES key.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';
import { effectiveWorkflow } from './ci/effective-workflow.mjs';

const tests = readdirSync(join(REPO_ROOT, 'tests'))
  .filter((name) => /^loom_[A-Za-z0-9_]+\.rs$/.test(name))
  .map((name) => name.slice(0, -3));
const ci = effectiveWorkflow(REPO_ROOT).text
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

// Member crates: resolve package names from each two-level crate manifest and
// inventory their loom_*.rs test targets.
const crateLoomTargets = [];
for (const theme of readdirSync(join(REPO_ROOT, 'crates'))) {
  let crates;
  try { crates = readdirSync(join(REPO_ROOT, 'crates', theme)); } catch { continue; }
  for (const crate of crates) {
    const dir = join(REPO_ROOT, 'crates', theme, crate);
    const manifest = join(dir, 'Cargo.toml');
    const testsDir = join(dir, 'tests');
    if (!existsSync(manifest) || !existsSync(testsDir)) continue;
    const pkg = readFileSync(manifest, 'utf8').match(/^\s*name\s*=\s*"([\w-]+)"/m)?.[1];
    if (!pkg) continue;
    for (const name of readdirSync(testsDir)
      .filter((n) => /^loom_[A-Za-z0-9_]+\.rs$/.test(n))
      .map((n) => n.slice(0, -3))) {
      crateLoomTargets.push({ pkg, name });
    }
  }
}
const crateMissing = crateLoomTargets.flatMap(({ pkg, name }) => [
  ...(!ciTargets.has(name) ? [`${pkg}/${name}: CI`] : []),
  ...(!localTargets.has(name) ? [`${pkg}/${name}: scripts/loom.mjs`] : []),
]);
if (crateMissing.length !== 0) {
  console.error(`[verify-loom-target-wiring] FAIL: ${crateMissing.join(', ')}`);
  process.exit(1);
}
console.log(`[verify-loom-target-wiring] OK: ${tests.length} root Loom targets + ${crateLoomTargets.length} member-crate Loom targets wired in CI and local matrix`);
