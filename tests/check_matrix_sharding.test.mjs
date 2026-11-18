// Black-box test for check-matrix CI sharding (p196a). No cargo, no writes. Ground truth:
// PER_PR_ROWS (kind check/test), argv rebuilt here from row fields. Actual side:
// `node scripts/run-check-matrix.mjs --list` — ONE JSON array of {id, argv, env} on stdout
// (no leading `cargo`; env = row overrides, {} if none); `--shard i/n` limits to one shard.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { effectiveWorkflow } from '../scripts/ci/effective-workflow.mjs';
import { PER_PR_ROWS } from '../scripts/check-matrix.mjs';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const RUNNER = fileURLToPath(new URL('../scripts/run-check-matrix.mjs', import.meta.url));
const ACTION = fileURLToPath(new URL('../.github/actions/ci-check-matrix/action.yml', import.meta.url));
const KINDS = ['--kind', 'check', '--kind', 'test'];
const CI_FLAGS = ['--keep-going', '--free-between-rows']; // mirrors the CI run line; --list must accept these

const ciLines = (() => {
  const lines = effectiveWorkflow(ROOT).text.split('\n'), start = lines.indexOf('  check-matrix:');
  assert.notEqual(start, -1, 'ci.yml has a check-matrix job');
  const end = lines.findIndex((l, i) => i > start && /^  [\w-]+:\s*$/.test(l));
  return lines.slice(start, end === -1 ? lines.length : end);
})();

// The strategy axis is the shard count ground truth — parse it, never hardcode.
const axisMatch = ciLines.join('\n').match(/^        shard: \[([0-9, ]+)\]$/m);
assert.ok(axisMatch, 'check-matrix strategy matrix declares a shard axis');
const SHARDS = axisMatch[1].split(',').map(Number);
assert.ok(SHARDS.length >= 2, 'at least 2 shards (duplicate counterfactual needs another group)');
assert.deepEqual([...SHARDS].sort((a, b) => a - b), SHARDS.map((_, i) => i + 1),
  'shard indices are exactly sequential 1..n, unique, no gaps');const N = SHARDS.length;

const rows = PER_PR_ROWS.filter((r) => r.kind === 'check' || r.kind === 'test');
const ids = rows.map((r) => r.id);

/** Expected {id, argv, env} from manifest row fields only. Mirrors the CI run
 * line: --keep-going means test rows carry `--no-fail-fast` right after the kind. */
const expected = new Map(rows.map((r) => {
  const argv = [r.kind, ...(r.kind === 'test' ? ['--no-fail-fast'] : []),
    ...(r.target ? [r.target.flag, r.target.name] : []),
    ...(r.features === '__all__' ? ['--all-features'] : r.features ? ['--features', r.features] : [])];
  return [r.id, { id: r.id, argv, env: r.env ?? {} }];
}));
/** Exact stdout-JSON listing; no banners on stdout, no tolerant transform. */
function list(extra = CI_FLAGS) {
  const out = execFileSync(process.execPath, [RUNNER, '--list', ...KINDS, ...extra],
    { encoding: 'utf8', cwd: ROOT });
  const parsed = JSON.parse(out);
  assert.ok(Array.isArray(parsed), '--list stdout is exactly one JSON array');
  return parsed.map((e) => {
    assert.ok(e.argv[0] !== 'cargo', 'listing argv has no leading cargo');
    return { id: e.id, argv: e.argv, env: e.env ?? {} };
  });
}/** Plan validator: each group an ordered subsequence, no empty/unknown/dropped/
 * duplicated rows, exactly-once coverage of the manifest ids. */
function validatePlan(groups) {
  const pos = new Map(ids.map((id, i) => [id, i]));
  const seen = [];
  for (const g of groups) {
    let last = -1;
    assert.ok(g.length, 'plan: no empty shard group');
    for (const id of g) {
      const p = pos.get(id);
      assert.ok(p !== undefined, `plan: unknown row ${id}`);
      assert.ok(p > last, `plan: shard not an ordered subsequence at ${id}`);
      last = p; seen.push(id);
    }
  }
  const counts = seen.reduce((m, id) => m.set(id, (m.get(id) ?? 0) + 1), new Map());
  for (const id of ids) {
    const c = counts.get(id) ?? 0;
    assert.equal(c, 1, `plan: row ${id} seen ${c}x (expected exactly once)`);
  }
}
test('check-matrix wiring: shard axis env in ci.yml, flags+guards in action.yml', () => {
  const ci = ciLines.join('\n');
  assert.match(ci, / {6}CHECK_MATRIX_SHARD: \$\{\{ matrix\.shard \}\}/);
  assert.match(ci, / {6}CHECK_MATRIX_SHARDS: \$\{\{ strategy\.job-total \}\}/);
  const action = readFileSync(ACTION, 'utf8');
  assert.match(action, /^ {4}- run: node scripts\/run-check-matrix\.mjs --keep-going --free-between-rows --kind check --kind test --shard "\$CHECK_MATRIX_SHARD\/\$CHECK_MATRIX_SHARDS"$/m);
  const lines = action.split('\n');
  const GUARD = "      if: env.CHECK_MATRIX_SHARD == '1'";
  let guarded = 0;
  for (let i = 0; i < lines.length; i++) {
    if (!/^ {4}- run: node /.test(lines[i])) continue;
    if (lines[i].includes('run-check-matrix.mjs')) {
      assert.doesNotMatch(lines[i + 1] ?? '', /^\s+if:/, 'runner step is ungated');
      continue;
    }
    assert.equal(lines[i + 1], GUARD, `guard literal directly after ${lines[i].trim()}`);
    guarded++;
  }
  assert.equal(guarded, 14, 'exactly 14 shard-1-guarded node run steps (incl. vmem)');
  assert.ok(lines.some((l, i) => /^ {4}- run: node .*vmem/.test(l) && lines[i + 1] === GUARD),
    'vmem-* steps are part of the guarded set');
});
test('full --list equals the manifest check/test rows in order with exact argv/env', () => {
  const actual = list();
  assert.deepEqual(actual.map((e) => e.id), ids, 'full listing is manifest order');
  for (const e of actual) assert.deepEqual(e, expected.get(e.id), `record for ${e.id}`);
});
test('each configured shard lists exactly its ordered subsequence with exact argv/env', () => {
  const groups = [];
  for (let i = 1; i <= N; i++) {
    const shard = list(['--shard', `${i}/${N}`, ...CI_FLAGS]);
    assert.ok(shard.length > 0, `shard ${i} is non-empty`);
    for (const e of shard) assert.deepEqual(e, expected.get(e.id), `shard ${i} record ${e.id}`);
    groups.push(shard.map((e) => e.id));
  }
  validatePlan(groups); // exactly-once coverage, per-shard order, no empties/unknowns
});

const shardIdGroups = () => Array.from({ length: N }, (_, i) =>
  list(['--shard', `${i + 1}/${N}`, ...CI_FLAGS]).map((e) => e.id));

test('RED counterfactuals: dropping, then duplicating, a real shard row throws naming the row', () => {
  const groups = shardIdGroups();
  const row = groups[0][0];
  assert.throws(() => validatePlan([groups[0].slice(1), ...groups.slice(1)]),
    (err) => err.message.includes(row) && err.message.includes('seen 0x'),
    `drop of ${row} must be a named defect`);
  // duplicate into the LAST shard at its manifest-order position: shard order stays
  // intact, so only the duplicate count is defective.
  const dup = groups[N - 1].slice();
  const at = dup.findIndex((id) => ids.indexOf(id) > ids.indexOf(row));
  dup.splice(at === -1 ? dup.length : at, 0, row);
  assert.throws(() => validatePlan([...groups.slice(0, -1), dup]),
    (err) => err.message.startsWith(`plan: row ${row} seen 2x (expected exactly once)`),
    `duplicate of ${row} must be a named defect`);
});

test('malformed --shard specs exit nonzero under --list, naming the flag', () => {
  for (const spec of ['bogus', `0/${N}`, `${N + 1}/${N}`, '2/1']) {
    assert.throws(() => execFileSync(process.execPath,
      [RUNNER, '--list', '--shard', spec], { encoding: 'utf8', cwd: ROOT }),
    (err) => err.status > 0 && /--shard/.test(String(err.stderr ?? err.message)),
    `--shard ${spec} must fail loudly and name the flag`);
  }
});
