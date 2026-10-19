import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, realpathSync,
  rmSync, symlinkSync, writeFileSync,
} from 'node:fs';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { HEADER, parseCsv, evaluateStep, mutantPreflight, mutantVerdict, validateRecipe,
  verifyArtifacts, verifyPerformed, cargoBuildSignature, encodeOutputFrames } from '../scripts/r20_evidence_reverify_support.mjs';

const ROOT = resolve(import.meta.dirname, '..');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const row = { activation: 'test activation', status: 'PASS', positive_marker: '', mutant_rejected: 'false' };
const sourceStep = { name: 'baseline', env: {}, argv: ['node', 'scripts/fake.mjs'],
  expect: { exit: 0, stdoutRegex: ['baseline-ok'] } };
const cargoStep = (name, argv, expect) => ({ name, env: { RUSTFLAGS: '--cfg loom' }, argv, expect });

const buildStep = cargoStep('compile mutant', ['cargo', 'test', '--release', '--features',
  'alloc-global,alloc-xthread,internals,tagged-index-stack/loom', '--test', 'loom_fixture', '--no-run'],
{ exit: 0, stdoutRegex: ['Finished'] });
const setup = { ...sourceStep, expect: { exit: 0, stdoutRegex: ['setup-ok'] } };
const runtime = cargoStep('run crashing mutant', ['cargo', 'test', '--release', '--features',
  'alloc-global,alloc-xthread,internals,tagged-index-stack/loom', '--test', 'loom_fixture', '--', '--nocapture'],
{ mutant: 'signal', stdoutRegex: ['running [1-9][0-9]* test', "process (?:did not|didn't) exit successfully.*STATUS_ACCESS_VIOLATION"] });
const testRecipe = {
  id: 'R4A-MUTANT',
  registryActivationHash: hash(Buffer.from(row.activation)),
  steps: [sourceStep],
  mutant: { file: 'tests/loom_fixture.rs', find: 'before', replace: 'after',
    steps: [buildStep, runtime], expect: { signal: true, stdoutRegex: ['SIGSEGV|ACCESS_VIOLATION'] } },
};
assert.equal(validateRecipe(testRecipe, testRecipe.id, row, hash).mutant.steps[1].expect.mutant, 'signal');
assert.throws(() => validateRecipe({ ...testRecipe,
  mutant: { ...testRecipe.mutant, steps: [{ ...setup, expect: { exit: 0, stdoutRegex: ['ok'] } },
    { ...runtime, expect: { exit: 101, stdoutRegex: ['x'] } }] },
}, testRecipe.id, row, hash), /expect|build|setup/);
  assert.throws(() => validateRecipe({ ...testRecipe,
    mutant: { ...testRecipe.mutant, steps: [buildStep, { ...runtime, argv: [...runtime.argv, '--test', 'different'] }] },
  }, testRecipe.id, row, hash), /signal mutant requires exit-zero setup/);

const evalFor = (step, result, output) => ({ ...result,
  passed: evaluateStep(step.expect, result, output).passed,
  matches: evaluateStep(step.expect, result, output).matches,
});
const setupResult = { ...evalFor(setup, { exit: 0, signal: null, error: null, expect: setup.expect }, 'setup-ok'), exit: 0 };
const buildResult = { ...evalFor(buildStep, { exit: 0, signal: null, error: null, expect: buildStep.expect, argv: buildStep.argv }, 'Finished'), name: buildStep.name, argv: buildStep.argv, expect: buildStep.expect, passed: true, expected: 0 };

const signalResult = { ...evalFor(runtime, { exit: 3221225477, signal: null, error: null, expect: runtime.expect },
  'running 1 test\nerror: test failed\nCaused by: process didn\'t exit successfully: (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)'),
  name: runtime.name, argv: runtime.argv, expect: runtime.expect, exit: 3221225477 };
assert.equal(evaluateStep(runtime.expect, signalResult, 'running 1 test').passed, false,
  'serialized result without successful cargo build must not pass crash receipt');
const linuxCargoReceipt = evaluateStep(runtime.expect, { ...signalResult, exit: 101, cargoBuildPassed: true },
  'running 1 test\nerror: test failed\nCaused by: process didn\'t exit successfully: (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)');
assert.equal(linuxCargoReceipt.passed, true);
const windowsSigned = evaluateStep(runtime.expect, { ...signalResult, exit: -1073741819 },
  'running 1 test\nSTATUS_ACCESS_VIOLATION\nerror: test failed\nCaused by: process didn\'t exit successfully: x (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)');
assert.equal(windowsSigned.passed, true);
assert.equal(buildResult.passed, true);
assert.equal(signalResult.passed, true);
assert.equal(mutantVerdict(testRecipe.mutant.expect, [buildResult, signalResult]), true);
assert.equal(mutantVerdict(testRecipe.mutant.expect, [
  buildResult,
  evalFor(runtime, { exit: 101, signal: null, error: null, expect: runtime.expect, cargoBuildPassed: false,
    stderrOutput: '' }, 'running 1 test'),
]), false);
assert.equal(mutantVerdict(testRecipe.mutant.expect, [buildResult,
  evalFor(runtime, { exit: 124, signal: null, error: null, timedOut: true, expect: runtime.expect }, 'running 1 test')]), false);

const scratch = join(ROOT, '.rush/evidence-reverify-support-selftest');
rmSync(scratch, { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });
try {
  const recipeRoot = join(scratch, 'recipes');
  const out = join(scratch, 'out');
  mkdirSync(recipeRoot);
  mkdirSync(out);
  const file = join(recipeRoot, 'FIXTURE.json');
  const good = {
    id: 'FIXTURE', registryActivationHash: hash(Buffer.from(row.activation)), steps: [sourceStep], mutant: null,
  };
  writeFileSync(file, `${JSON.stringify(good)}\n`);
  const recipeBytes = readFileSync(file);
  const recipeHash = hash(recipeBytes);
  const frameText = encodeOutputFrames('baseline-ok\n', '');
  const raw = Buffer.from('E0_METADATA untrusted-command-metadata\n' +
    `E0_STEP ${JSON.stringify({ name: 'baseline' })}\n${frameText}E0_STEP_END {"code":0}\n`);
  const start = raw.indexOf(Buffer.from(frameText));
  const end = start + Buffer.byteLength(frameText);
  writeFileSync(join(out, '_raw_reverify_FIXTURE.log'), raw);
  const ident = { head: 'a'.repeat(40), tree: 'b'.repeat(40) };
  const resultStep = {
    ...sourceStep, exit: 0, expected: 0, signal: null, error: null,
    start, end, matches: [{ regex: 'baseline-ok', matched: true, actual: ['baseline-ok'] }],
    passed: true, timedOut: false,
  };
  const result = {
    id: 'FIXTURE', status: 'PASS', verdict: 'pass', source_sha: ident.head, source_tree: ident.tree,
    recipe: 'fixture', recipe_sha256: recipeHash, registry_activation_hash: good.registryActivationHash,
    receipt: 'docs/evidence/_raw_reverify_FIXTURE.log', receipt_sha256: hash(raw),
    toolchain: 'fixture', profile: 'debug', steps: [resultStep], mutant_steps: [], restored_steps: [],
    mutant_rejected: false,
  };
  const registry = new Map([['FIXTURE', { ...row, id: 'FIXTURE' }]]);
  const adapter = {
    out, chosen: ['FIXTURE'], ident, registry, recipeRoot, hash,
    existsSync: (path) => path !== file || true,
    readFileSync, join, load: () => ({ recipe: good, path: 'fixture', recipeHash }),
    recipePath: () => file, lf: (text) => text.replace(/\r\n?/g, '\n'), SHA: /^[a-f0-9]{64}$/,
    STAGES: new Set(['oracle']), HEADER,

    parseCsv: () => [HEADER, HEADER.map((key) => ({
      ...row, id: 'FIXTURE', source_sha: ident.head, source_tree: ident.tree, profile: 'debug',
      toolchain: 'fixture', receipt: result.receipt, receipt_sha256: result.receipt_sha256,
    })[key] ?? '')], verifyPerformed,
  };
  const summary = { identity: ident, selectedIds: ['FIXTURE'], stages: ['oracle'],
    toolchains: { FIXTURE: 'fixture' }, rows: [result] };
  writeFileSync(join(out, 'summary.json'), JSON.stringify(summary));
  writeFileSync(join(out, 'proposed_rows.csv'), `${HEADER.map((key) => `\"${key}\"`).join(',')}\n${HEADER.map((key) => `\"${({
    ...row, id: 'FIXTURE', source_sha: ident.head, source_tree: ident.tree, profile: 'debug',
    toolchain: 'fixture', receipt: result.receipt, receipt_sha256: result.receipt_sha256,
  })[key] ?? ''}\"`).join(',')}\n`);
  writeFileSync(join(out, 'verdict.txt'), 'E0 PASS\n');
  const writeSummary = (rows) => writeFileSync(join(out, 'summary.json'),
    JSON.stringify({ identity: ident, selectedIds: ['FIXTURE'], stages: ['oracle'],
      toolchains: { FIXTURE: 'fixture' }, rows }, null, 2));
  writeSummary([result]);
  // A receipt written by the shared frame encoder must pass the verifier end-to-end.
  verifyArtifacts(adapter);

  // Negative: flip one byte inside the E0_STDOUT frame payload.
  const payloadAt = raw.indexOf(Buffer.from('E0_STDOUT ')) + 'E0_STDOUT '.length;
  const tampered = Buffer.from(raw);
  tampered[payloadAt] = tampered[payloadAt] === 0x59 ? 0x5a : 0x59;
  assert.notEqual(Buffer.compare(raw, tampered), 0);
  writeFileSync(join(out, '_raw_reverify_FIXTURE.log'), tampered);
  assert.throws(() => verifyArtifacts(adapter), /step did not pass|malformed/,
    'tampered receipt frame payload must be rejected');
  writeFileSync(join(out, '_raw_reverify_FIXTURE.log'), raw);

  // Negative: a step without a valid numeric start/end range.
  const noRange = JSON.parse(JSON.stringify(result));
  delete noRange.steps[0].start;
  delete noRange.steps[0].end;
  writeSummary([noRange]);
  assert.throws(() => verifyArtifacts(adapter), /step receipt range malformed/,
    'missing step receipt range must be rejected');
  writeSummary([result]);
  verifyArtifacts(adapter);

  const traversal = { file: '../tests/loom_x.rs', find: 'a', replace: 'b' };
  assert.throws(() => mutantPreflight(traversal, ROOT, {
    resolvePath: resolve, relativePath: (from, to) => to.slice(from.length + 1), separator: sep,
    isAbsolute: (value) => value.startsWith('/'), existsSync: () => false, lstatSync: () => ({}),
    realpathSync: resolve, readFileSync, writeFileSync, lf: (value) => value, hash,
  }), /unsafe mutant path|escape|traversal/);

  // Path admission tests (real fs, scratch strictly inside this worktree).
  const deps = {
    resolvePath: resolve, relativePath: (from, to) => relative(from, to), separator: sep,
    isAbsolute: (value) => isAbsolute(value), existsSync, lstatSync, realpathSync,
    readFileSync, writeFileSync, lf: (value) => value, hash,
  };
  const tmp = mkdtempSync(join(ROOT, '.rush', 'scratch-'));
  try {
    mkdirSync(join(tmp, 'tests'), { recursive: true });
    writeFileSync(join(tmp, 'tests/loom_x.rs'), 'before\n');
    const validMutant = { file: 'tests/loom_x.rs', find: 'before', replace: 'after',
      steps: [buildStep, runtime], expect: testRecipe.mutant.expect };
    const snapshot = mutantPreflight(validMutant, tmp, deps);
    assert.ok(snapshot.file.endsWith(join('tests', 'loom_x.rs')));
    assert.equal(validateRecipe({ ...good, mutant: validMutant }, 'FIXTURE', row, hash)
      .mutant.file, 'tests/loom_x.rs');
    // Every rejected path exists, so a policy refusal cannot pass as ENOENT.
    writeFileSync(join(tmp, 'tests/other.rs'), 'before\n');
    writeFileSync(join(tmp, '../x.rs'), 'before\n');
    for (const bad of ['tests/other.rs', '../x.rs', join(tmp, 'tests/loom_x.rs')]) {
      assert.throws(() => validateRecipe({ ...good,
        mutant: { ...validMutant, file: bad } }, 'FIXTURE', row, hash), /unsafe mutant path/);
      assert.throws(() => mutantPreflight({ ...validMutant, file: bad }, tmp, deps),
        /unsafe|escape|traversal/);
    }
    rmSync(join(tmp, '../x.rs'), { force: true });
    // Symlink pointing outside the admitted root.
    const outside = join(ROOT, '.rush', 'scratch-outside-target.rs');
    writeFileSync(outside, 'before\n');
    let symlinkReason = null;
    try {
      symlinkSync(outside, join(tmp, 'tests/loom_link.rs'));
    } catch (error) {
      symlinkReason = `${error.code || 'ERR'}: ${error.message}`;
    }
    if (symlinkReason) {
      console.log(`skipped: symlink не поддерживается ОС (${symlinkReason})`);
    } else {
      assert.throws(() => mutantPreflight({ ...validMutant, file: 'tests/loom_link.rs' },
        tmp, deps), /symlink/);
      console.log('symlink rejection case executed');
    }
    rmSync(outside, { force: true });
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

console.log('PASS EV-1 schema, aggregate evaluator, receipt frame codec, and path admission tests');
