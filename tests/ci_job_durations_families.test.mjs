// Tests for the --family aggregation in scripts/ci-job-durations.mjs (miri sharding,
// docs/perf/P0_2_TEST_MERGE_PILOT.md criterion (b)). Synthetic dumps only, like
// tests/ci_job_durations.test.mjs: an unsharded before job is compared at family level
// against >=1 sharded after jobs; the script must report SUM and critical-path (MAX).
// Node >= 24, ESM, zero npm dependencies, writes only under os.tmpdir().

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import {
  assertFamilies, computeFamilyRows, collectRuns, parseFamilies,
} from '../scripts/ci-job-durations.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = join(repoRoot, 'scripts', 'ci-job-durations.mjs');

function makeWorkspace() {
  const dir = mkdtempSync(join(tmpdir(), 'ci-job-durations-fam-'));
  const before = mkdirSync(join(dir, 'before'), { recursive: true });
  const after = mkdirSync(join(dir, 'after'), { recursive: true });
  return { dir, before, after };
}

/** One run dump with whole-job durations for each [name, durSeconds]. */
function runFile(side, fileName, entries) {
  const t = (sec) => new Date(1730000000000 + sec * 1000).toISOString();
  writeFileSync(join(side, fileName), JSON.stringify({
    jobs: entries.map(([name, dur]) => ({
      name, started_at: t(0), completed_at: t(dur), conclusion: 'success', steps: [],
    })),
  }));
}

test('parseFamilies: label/regexes parsed; malformed entry throws', () => {
  const [f] = parseFamilies(['miri core=^miri \\(core\\)$=>^miri \\(core, (rest)\\)$']);
  assert.equal(f.label, 'miri core');
  assert.ok(f.beforeRe.test('miri (core)'));
  assert.ok(f.afterRe.test('miri (core, rest)'));
  assert.throws(() => parseFamilies(['nonsense']), /bad --family entry/);
});

test('family aggregation: sum and critical path medians vs unsharded before job', () => {
  const ws = makeWorkspace();
  try {
    runFile(ws.before, 'r0.json', [['miri (core)', 300], ['miri (fastbin)', 100]]);
    runFile(ws.before, 'r1.json', [['miri (core)', 320], ['miri (fastbin)', 110]]);
    runFile(ws.before, 'r2.json', [['miri (core)', 340], ['miri (fastbin)', 120]]);
    // after: core split into two shards per run; fastbin into three
    runFile(ws.after, 'r0.json', [['miri (core, pin cache)', 100], ['miri (core, rest)', 150], ['miri (fastbin, a)', 40], ['miri (fastbin, b)', 40], ['miri (fastbin, c)', 40]]);
    runFile(ws.after, 'r1.json', [['miri (core, pin cache)', 110], ['miri (core, rest)', 160], ['miri (fastbin, a)', 45], ['miri (fastbin, b)', 45], ['miri (fastbin, c)', 45]]);
    runFile(ws.after, 'r2.json', [['miri (core, pin cache)', 120], ['miri (core, rest)', 170], ['miri (fastbin, a)', 50], ['miri (fastbin, b)', 50], ['miri (fastbin, c)', 50]]);
    const before = collectRuns(ws.before, /cargo test/);
    const after = collectRuns(ws.after, /cargo test/);
    const fams = parseFamilies([
      'miri core=^miri \\(core\\)$=>^miri \\(core, (pin cache|rest)\\)$',
      'miri fastbin=^miri \\(fastbin\\)$=>^miri \\(fastbin, ',
    ]);
    const { rows, detail } = computeFamilyRows({ before, after, families: fams, minRuns: 3 });
    assert.equal(rows.length, 4); // 2 families x (sum + critical path)
    const byName = new Map(rows.map((r) => [r.job, r]));
    const core = byName.get('miri core');
    // sums: before 300/320/340 (median 320); after 250/270/290 (median 270)
    assert.equal(core.b.med, 320);
    assert.equal(core.a.med, 270);
    assert.equal(core.verdict, 'fell');
    const coreCrit = byName.get('miri core (critical path)');
    // crit: before 300/320/340 (median 320); after max(100,150)/max(110,160)/max(120,170) = 150/160/170 (median 160)
    assert.equal(coreCrit.a.med, 160);
    assert.equal(coreCrit.b.med, 320);
    const fb = byName.get('miri fastbin');
    // sums: before 100/110/120 (median 110); after 120/135/150 (median 135) => grew
    assert.equal(fb.b.med, 110);
    assert.equal(fb.a.med, 135);
    assert.equal(fb.verdict, 'grew');
    assertFamilies(detail, { before, after }); // independent re-aggregation passes
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});

test('CLI: --family rows in md/csv; exit 1 when a family has < min-runs on a side', () => {
  const ws = makeWorkspace();
  try {
    runFile(ws.before, 'r0.json', [['miri-plain', 200]]);
    runFile(ws.before, 'r1.json', [['miri-plain', 210]]);
    runFile(ws.before, 'r2.json', [['miri-plain', 220]]);
    runFile(ws.after, 'r0.json', [['miri-plain (heavy regressions)', 150], ['miri-plain (rest)', 90]]);
    runFile(ws.after, 'r1.json', [['miri-plain (heavy regressions)', 160], ['miri-plain (rest)', 95]]);
    // only 2 after runs => insufficient at min-runs 3
    const csvPath = join(ws.dir, 'out.csv');
    const r = spawnSync(process.execPath, [SCRIPT,
      '--before', ws.before, '--after', ws.after,
      '--family', 'miri plain=^miri-plain$=>^miri-plain \\(',
      '--csv', csvPath], { encoding: 'utf8' });
    assert.equal(r.status, 1);
    assert.match(r.stderr, /insufficient samples/);
    assert.match(r.stdout, /\| miri plain \| job \| 3 \| 210\.00 \| 2 \|/);
    // same data, min-runs 2 => verdicts + criterion line
    const r2 = spawnSync(process.execPath, [SCRIPT,
      '--before', ws.before, '--after', ws.after,
      '--family', 'miri plain=^miri-plain$=>^miri-plain \\(',
      '--min-runs', '2'], { encoding: 'utf8' });
    assert.equal(r2.status, 0);
    assert.match(r2.stdout, /criterion \(b\).*DOES NOT HOLD/); // delta 175 -> grew
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});
