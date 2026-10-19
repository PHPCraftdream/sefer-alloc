#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import {
  appendFileSync, closeSync, existsSync, fstatSync, lstatSync, mkdirSync, openSync,
  readFileSync, readSync, realpathSync, rmSync, writeFileSync, writeSync,
} from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import {
  HEADER, ID, SHA, STAGES, parseCsv, readRegistry, selectIds, validateStep,
  evaluateStep, mutantVerdict, cargoBuildSignature, encodeOutputFrames, decodeOutputFrames,
  validateRecipe as validateRecipeData, verifyPerformed, verifyArtifacts,
} from './r20_evidence_reverify_support.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const EVIDENCE = join(ROOT, 'docs/evidence');
const RECIPE_ROOT = join(EVIDENCE, 'reverify-recipes');
const lf = (text) => text.replace(/\r\n?/g, '\n');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const quote = (value) => `"${String(value ?? '').replaceAll('"', '""')}"`;
const encode = (rows) => `${rows.map((row) => row.map(quote).join(',')).join('\n')}\n`;

function cli(argv) {
  const options = { stages: [], outDir: join(ROOT, '.rush/r20-evidence-reverify') };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--selftest') options.selftest = true;
    else if (arg === '--dry-run') options.dry = true;
    else if (['--prepare', '--build', '--oracle', '--measure'].includes(arg)) {
      options.stages.push(arg.slice(2));
    } else if (arg === '--stage') {
      const stage = argv[++index];
      if (!STAGES.has(stage)) throw Error('invalid stage');
      options.stages.push(stage);
    } else if (arg === '--rows') {
      options.rows = argv[++index]?.split(',');
    } else if (arg === '--config') {
      options.config = resolve(ROOT, argv[++index] || '');
    } else if (arg === '--out-dir') {
      options.outDir = resolve(ROOT, argv[++index] || '');
    } else {
      throw Error(`unknown option ${arg}`);
    }
  }
  return options;
}

function selected(options) {
  if (options.rows) return selectIds(options.rows);
  const path = options.config || join(ROOT, '.github/evidence-reverify.json');
  const config = JSON.parse(readFileSync(path, 'utf8'));
  if (!config || Array.isArray(config) || Object.keys(config).length !== 1 ||
      !Array.isArray(config.rows)) {
    throw Error('config must contain exactly rows');
  }
  return selectIds(config.rows);
}

function validateRecipe(data, id, row) {
  return validateRecipeData(data, id, row, hash);
}

function recipePath(id, root = RECIPE_ROOT) {
  return join(root, `${id}.json`);
}

function load(id, row, root = RECIPE_ROOT) {
  const path = recipePath(id, root);
  if (!existsSync(path)) return { manual: `missing recipe ${relative(ROOT, path)}` };
  try {
    return {
      recipe: validateRecipe(JSON.parse(readFileSync(path, 'utf8')), id, row),
      path: relative(ROOT, path).split(sep).join('/'),
      recipeHash: hash(readFileSync(path)),
    };
  } catch (error) {
    return { invalid: error.message, path: relative(ROOT, path).split(sep).join('/') };
  }
}

function mutantPreflight(mutant, root = ROOT) {
  const path = resolve(root, mutant.file);
  const relativePath = relative(root, path);
  if (isAbsolute(mutant.file) || path === root || isAbsolute(relativePath) || relativePath === '..' ||
      relativePath.startsWith(`..${sep}`) || !/^(?:src\/(?:[A-Za-z0-9_.-]+\/)*[A-Za-z0-9_.-]+\.rs|tests\/loom_[A-Za-z0-9_.-]+\.rs)$/.test(mutant.file) ||
      mutant.file.split('/').some((part) => part === '.' || part === '..')) throw Error('mutant path escape');
  if (!path.startsWith(root + sep)) throw Error('mutant path escape');
  let current = root;
  for (const part of mutant.file.split('/')) {
    current = join(current, part);
    if (existsSync(current) && lstatSync(current).isSymbolicLink()) throw Error('mutant symlink');
  }
  const real = realpathSync(path);
  if (!real.startsWith(root + sep)) throw Error('mutant realpath escape');
  const original = readFileSync(real);
  const source = lf(original.toString('utf8'));
  const find = lf(mutant.find);
  const replace = lf(mutant.replace);
  const count = source.split(find).length - 1;
  if (count !== 1) throw Error(`mutant exact count ${count}`);
  return { file: real, original, source, find, replace, originalHash: hash(original) };
}

async function applyMutant(mutant, root, run) {
  const snapshot = mutantPreflight(mutant, root);
  try {
    writeFileSync(snapshot.file, Buffer.from(snapshot.source.replace(snapshot.find, snapshot.replace)));
    return await run(snapshot);
  } finally {
    writeFileSync(snapshot.file, snapshot.original);
    if (hash(readFileSync(snapshot.file)) !== snapshot.originalHash) {
      throw Error('byte-exact restoration SHA mismatch');
    }
  }
}

function profile(data) {
  const steps = [...data.steps, ...(data.mutant?.steps || [])];
  const args = steps.flatMap((step) => step.argv).join(' ');
  const flags = steps.map((step) => step.env.MIRIFLAGS || '').join(' ');
  if (flags.includes('tree-borrows')) return 'Miri TB';
  if (flags.includes('strict-provenance')) return 'Miri SB strict-provenance';
  if (args.includes('miri')) return 'Miri SB (default)';
  if (args.includes('loom')) return 'Loom (--cfg loom --release)';
  return args.includes('--release') ? 'release' : 'debug';
}

function matches(step, output, code, signal, error, stderrOutput = '', cargoBuildPassed = false) {
  return evaluateStep(step.expect, { exit: code, signal, error, stderrOutput, cargoBuildPassed }, output);
}

function runStep(step, log, id, kind) {
  return new Promise((resolveRun, rejectRun) => {
    let fd;
    try {
      fd = openSync(log, 'a+');
      writeSync(fd, Buffer.from(`\nE0_STEP ${JSON.stringify({
        id, kind, name: step.name, argv: step.argv, env: step.env, expect: step.expect,
      })}\n`));
      const start = fstatSync(fd).size;
      let error = null;
      const child = spawn(step.argv[0], step.argv.slice(1), {
        cwd: ROOT,
        env: { ...process.env, ...step.env },
        stdio: ['ignore', fd, fd],
      });
      child.on('error', (cause) => { error = cause.message; });
      child.on('close', (code, signal) => {
        try {
          const rawEnd = fstatSync(fd).size;
          const buffer = Buffer.alloc(rawEnd - start);
          const reader = openSync(log, 'r');
          let offset = 0;
          while (offset < buffer.length) {
            const count = readSync(reader, buffer, offset, buffer.length - offset, start + offset);
            if (!count) throw Error('short raw receipt read');
            offset += count;
          }
          closeSync(reader);
          writeSync(fd, Buffer.from(`\nE0_STEP_END ${JSON.stringify({ code, signal, error })}\n`));
          const captured = buffer.toString('utf8');
          // Structured receipt frames, encoded with the same shared helper the
          // verifier decodes with (verifyArtifacts -> frameFor).
          writeSync(fd, Buffer.from(encodeOutputFrames(captured, '')));
          const end = fstatSync(fd).size;
          closeSync(fd);
          const result = matches(step, captured, code, signal, error, captured,
        Boolean(step.argv[0] === 'cargo' && step.argv.includes('--test') &&
          runStep.lastSuccessfulBuild &&
          runStep.lastSuccessfulBuild.signature === cargoBuildSignature(step.argv, step.env)));
      if (step.argv[0] === 'cargo' && step.argv.includes('--no-run') && code === 0 && !signal && !error) {
        runStep.lastSuccessfulBuild = { signature: cargoBuildSignature(step.argv, step.env) };
      }
      resolveRun({
        name: step.name, argv: step.argv, env: step.env, exit: code, signal, error,
        expected: step.expect.mutant === 'signal' ? null :
          (step.expect.exitNonZero === true ? -1 : step.expect.exit), expect: step.expect, passed: result.passed,
        start, end, matches: result.matches,
      });
        } catch (cause) {
          rejectRun(cause);
        }
      });
    } catch (error) {
      if (fd !== undefined) closeSync(fd);
      rejectRun(error);
    }
  });
}

async function runSteps(steps, log, id, kind) {
  const results = [];
  for (const step of steps) {
    const result = await runStep(step, log, id, kind);
    results.push(result);
    if (!result.passed) break;
  }
  return results;
}

const outputFor = (log, step) => decodeOutputFrames(readFileSync(log).subarray(step.start, step.end).toString('utf8'), 'STDOUT');

function aggregateMutant(mutant, steps, log) {
  const output = steps.map((step) => outputFor(log, step)).join('\n');
  return mutant.expect.stdoutRegex.map((regex) => ({
    regex,
    matched: new RegExp(regex).test(output),
    actual: [...output.matchAll(new RegExp(regex, 'g'))].map((match) => match[0]),
  }));
}

function git(args) {
  const result = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8' });
  if (result.status !== 0) throw Error(`git ${args.join(' ')}: ${result.stderr}`);
  return result.stdout.trim();
}

const sourceIdentity = () => ({
  head: git(['rev-parse', 'HEAD']),
  tree: git(['rev-parse', 'HEAD^{tree}']),
});

function ciClean() {
  if (process.platform !== 'linux' || process.env.GITHUB_ACTIONS !== 'true') {
    throw Error('execution requires Linux GitHub Actions');
  }
  if (git(['status', '--porcelain'])) throw Error('execution requires clean checkout');
}

const needsNightly = (data) => [...data.steps, ...(data.mutant?.steps || [])].some((step) =>
  step.argv[0] === 'cargo' && (step.argv.includes('+nightly') || step.argv.includes('miri')));

function setup(recipes) {
  if (!recipes.some(needsNightly)) return '';
  const result = spawnSync('rustup', [
    'toolchain', 'install', 'nightly', '--profile', 'minimal', '--component', 'miri',
  ], { cwd: ROOT, encoding: 'utf8' });
  if (result.status !== 0) throw Error(`nightly install failed ${result.stderr}`);
  return (result.stdout || '') + (result.stderr || '');
}

function rustc(nightly) {
  const result = spawnSync('rustc', nightly ? ['+nightly', '-Vv'] : ['-Vv'], {
    cwd: ROOT,
    encoding: 'utf8',
  });
  if (result.status !== 0) throw Error(`rustc version failed ${result.stderr}`);
  return result.stdout.trim();
}

const metadata = (path, value) => appendFileSync(path, `E0_METADATA ${JSON.stringify(value)}\n`);

function verify(out, chosen, ident, registry, recipeRoot = RECIPE_ROOT) {
  return verifyArtifacts({
    out, chosen, ident, registry, recipeRoot, hash, existsSync, readFileSync,
    join, load, recipePath, lf, SHA, STAGES, HEADER, parseCsv, verifyPerformed,
  });
}

function safeOutput(path) {
  const relativePath = relative(ROOT, path);
  if (path === ROOT || relativePath === '..' || relativePath.startsWith(`..${sep}`) ||
      path.startsWith(EVIDENCE + sep) || path.startsWith(join(ROOT, 'src') + sep) ||
      path.startsWith(join(ROOT, 'tests') + sep) || path === join(ROOT, 'scripts') ||
      path.startsWith(join(ROOT, 'scripts') + sep)) {
    throw Error('unsafe output path');
  }
}

async function runOracle(plans, out, identity) {
  for (const plan of plans) {
    if (plan.manual || !plan.recipe) {
      plan.verdict = 'needs-manual';
      plan.reason = plan.manual || 'recipe missing';
      continue;
    }
    const log = join(out, `_raw_reverify_${plan.id}.log`);
    if (!existsSync(log)) writeFileSync(log, '');
    try {
      plan.toolchain = rustc(needsNightly(plan.recipe));
      plan.profile = profile(plan.recipe);
      metadata(log, {
        id: plan.id, head: identity.head, tree: identity.tree,
        os: `${os.platform()} ${os.arch()}`, toolchain: plan.toolchain,
        profile: plan.profile, recipe: plan.path,
        recipe_sha256: plan.recipeHash,
        registry_activation_hash: plan.recipe.registryActivationHash,
      });
      plan.steps = await runSteps(plan.recipe.steps, log, plan.id, 'baseline');
      if (plan.steps.length !== plan.recipe.steps.length || plan.steps.some((step) => !step.passed)) {
        plan.verdict = 'fail';
        plan.reason = 'baseline expectation failed';
        continue;
      }
      if (plan.recipe.mutant) {
        const mutant = plan.recipe.mutant;
        plan.mutantOriginalHash = plan.snapshot.originalHash;
        runStep.lastSuccessfulBuild = null;
        plan.mutantSteps = await applyMutant(mutant, ROOT, async (snapshot) => {
          if (snapshot.originalHash !== plan.mutantOriginalHash) {
            throw Error('source changed after preflight');
          }
          return runSteps(mutant.steps, log, plan.id, 'mutant');
        });
        plan.mutantRestoredHash = hash(readFileSync(plan.snapshot.file));
        metadata(log, { id: plan.id, kind: 'restored', sha256: plan.mutantRestoredHash });
        plan.mutantMatches = aggregateMutant(mutant, plan.mutantSteps, log);
        const killed = plan.mutantSteps.length === mutant.steps.length &&
          mutantVerdict(mutant.expect, plan.mutantSteps) && plan.mutantMatches.every((match) => match.matched);
        if (!killed || plan.mutantRestoredHash !== plan.mutantOriginalHash) {
          plan.verdict = 'fail';
          plan.reason = 'mutant expectation/restoration failed';
          continue;
        }
        plan.restored = await runSteps(plan.recipe.steps, log, plan.id, 'restored-baseline');
        if (plan.restored.length !== plan.recipe.steps.length ||
            plan.restored.some((step) => !step.passed) ||
            hash(readFileSync(plan.snapshot.file)) !== plan.mutantOriginalHash) {
          plan.verdict = 'fail';
          plan.reason = 'restored baseline failed';
          continue;
        }
        plan.mutantRejected = true;
      } else if (plan.id === 'R0-21') {
        plan.mutantRejected = true;
      }
      plan.verdict = 'pass';
      plan.reason = plan.row.status === 'KNOWN-RED'
        ? 'expected strict red+plain controls; preserve KNOWN-RED'
        : 'all recipe steps passed';
    } catch (error) {
      plan.verdict = 'fail';
      plan.reason = error.message;
    }
  }
}

async function main() {
  const options = cli(process.argv.slice(2));
  if (options.selftest) {
    await selftest();
    return;
  }
  const chosen = selected(options);
  const registry = readRegistry(readFileSync(join(EVIDENCE, 'registry.csv'), 'utf8'));
  for (const id of chosen) {
    if (!registry.has(id)) throw Error(`unknown registry ID ${id}`);
  }
  const stages = options.stages.length ? options.stages : ['prepare', 'oracle'];
  if (new Set(stages).size !== stages.length) throw Error('duplicate stages');
  const identity = sourceIdentity();
  const plans = chosen.map((id) => {
    const loaded = load(id, registry.get(id));
    return {
      id, row: registry.get(id), ...loaded, steps: [], mutantSteps: [], restored: [],
      verdict: loaded.invalid ? 'INVALID' : loaded.manual ? 'needs-manual' : 'planned',
      reason: loaded.invalid || loaded.manual || '', mutantRejected: false,
    };
  });
  for (const plan of plans) {
    if (plan.recipe?.mutant) {
      try {
        plan.snapshot = mutantPreflight(plan.recipe.mutant);
      } catch (error) {
        plan.verdict = 'INVALID';
        plan.reason = error.message;
      }
    }
  }
  const invalid = plans.filter((plan) => plan.verdict === 'INVALID');
  if (invalid.length) {
    for (const plan of invalid) console.error(`${plan.id}: INVALID — ${plan.reason}`);
    process.exitCode = 1;
    return;
  }
  if (options.dry) {
    for (const plan of plans) {
      if (!plan.recipe) {
        console.log(`${plan.id}: ${plan.verdict} — ${plan.reason}`);
        continue;
      }
      console.log(`${plan.id}: ${plan.path} sha256=${plan.recipeHash}`);
      for (const step of plan.recipe.steps) {
        console.log(`  step ${step.name} env=${JSON.stringify(step.env)} ` +
          `argv=${JSON.stringify(step.argv)} expect=${JSON.stringify(step.expect)}`);
      }
      if (plan.recipe.mutant) {
        const mutant = plan.recipe.mutant;
        console.log(`  mutant file=${mutant.file} find=${JSON.stringify(mutant.find)} ` +
          `replace=${JSON.stringify(mutant.replace)} aggregate=${JSON.stringify(mutant.expect)}`);
        for (const step of mutant.steps) {
          console.log(`  mutant step ${step.name} env=${JSON.stringify(step.env)} ` +
            `argv=${JSON.stringify(step.argv)} expect=${JSON.stringify(step.expect)}`);
        }
      }
    }
    return;
  }
  if (stages.includes('measure')) {
    if (stages.length !== 1) throw Error('measure must be standalone');
    const summary = verify(options.outDir, chosen, identity, registry);
    if (summary.rows.some((row) => ['fail', 'INVALID'].includes(row.verdict))) process.exitCode = 1;
    return;
  }

  const out = resolve(options.outDir);
  safeOutput(out);
  if (stages.some((stage) => stage === 'build' || stage === 'oracle')) ciClean();
  mkdirSync(out, { recursive: true });
  const planData = {
    identity, selectedIds: chosen, stages,
    rows: plans.map((plan) => ({
      id: plan.id, status: plan.row.status, verdict: plan.verdict, reason: plan.reason,
      recipe: plan.path || null, recipe_sha256: plan.recipeHash || null,
      registry_activation_hash: plan.recipe?.registryActivationHash || null,
      mutant_original_sha256: plan.snapshot?.originalHash || null,
    })),
  };
  writeFileSync(join(out, 'plan.json'), `${JSON.stringify(planData, null, 2)}\n`);
  if (stages.length === 1 && stages[0] === 'prepare') return;

  if (stages.includes('build')) {
    const setupInfo = setup(plans.filter((plan) => plan.recipe).map((plan) => plan.recipe));
    for (const plan of plans.filter((item) => item.recipe && !item.manual)) {
      const log = join(out, `_raw_reverify_${plan.id}.log`);
      if (!existsSync(log)) writeFileSync(log, '');
      metadata(log, { id: plan.id, head: identity.head, tree: identity.tree, setup: setupInfo });
    }
  }
  if (stages.length === 1 && stages[0] === 'build') return;

  const before = stages.includes('oracle') ? git(['diff', '--binary', 'HEAD']) : null;
  if (stages.includes('oracle')) await runOracle(plans, out, identity);
  if (before !== null && (git(['rev-parse', 'HEAD']) !== identity.head ||
      git(['rev-parse', 'HEAD^{tree}']) !== identity.tree ||
      git(['diff', '--binary', 'HEAD']) !== before)) {
    throw Error('source identity/diff changed');
  }

  for (const plan of plans) {
    const log = join(out, `_raw_reverify_${plan.id}.log`);
    if (existsSync(log)) {
      plan.receipt = `docs/evidence/_raw_reverify_${plan.id}.log`;
      plan.receiptHash = hash(Buffer.from(lf(readFileSync(log, 'utf8')), 'utf8'));
    }
  }
  const proposed = plans.filter((plan) => plan.verdict === 'pass').map((plan) => {
    const row = {
      ...plan.row,
      os: `${os.platform()} ${os.arch()}`,
      toolchain: plan.toolchain,
      profile: plan.profile,
      source_sha: identity.head,
      source_tree: identity.tree,
      mutant_rejected: plan.mutantRejected ? 'true' : plan.row.mutant_rejected,
      receipt: plan.receipt,
      receipt_sha256: plan.receiptHash,
      status_reason: `E0 verified; preserve ${plan.row.status}`,
    };
    return HEADER.map((key) => row[key] ?? '');
  });
  writeFileSync(join(out, 'proposed_rows.csv'), encode([HEADER, ...proposed]));
  const summary = {
    identity,
    selectedIds: chosen,
    stages,
    toolchains: Object.fromEntries(plans.filter((plan) => plan.toolchain).map((plan) =>
      [plan.id, plan.toolchain])),
    rows: plans.map((plan) => ({
      id: plan.id, status: plan.row.status, verdict: plan.verdict, reason: plan.reason,
      source_sha: identity.head, source_tree: identity.tree, recipe: plan.path || null,
      recipe_sha256: plan.recipeHash || null,
      registry_activation_hash: plan.recipe?.registryActivationHash || null,
      receipt: plan.receipt || null, receipt_sha256: plan.receiptHash || null,
      toolchain: plan.toolchain || null, profile: plan.profile || null,
      steps: plan.steps, mutant_steps: plan.mutantSteps, restored_steps: plan.restored,
      mutant_matches: plan.mutantMatches || [],
      mutant_original_sha256: plan.mutantOriginalHash || null,
      mutant_restored_sha256: plan.mutantRestoredHash || null,
      mutant_rejected: plan.mutantRejected,
    })),
  };
  writeFileSync(join(out, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`);
  const failed = plans.some((plan) => ['fail', 'INVALID'].includes(plan.verdict));
  const allPassed = plans.every((plan) => plan.verdict === 'pass');
  const verdict = failed ? 'E0 FAIL' : allPassed ? 'E0 PASS' : 'E0 INCOMPLETE';
  const lines = summary.rows.map((row) => `${row.id}: ${row.verdict}${row.reason ? ` — ${row.reason}` : ''}`);
  writeFileSync(join(out, 'verdict.txt'),
    `${verdict}\nHEAD ${identity.head}\nTREE ${identity.tree}\n${lines.join('\n')}\n`);
  verify(out, chosen, identity, registry);
  for (const plan of plans) {
    console.log(`${plan.id}: ${plan.verdict}${plan.reason ? ` — ${plan.reason}` : ''}`);
  }
  if (failed) process.exitCode = 1;
}
function makeSyntheticResult(id, row, identity, recipeFile, recipeData, receiptHash, toolchain, steps) {
  return {
    id,
    status: row.status,
    verdict: 'pass',
    source_sha: identity.head,
    source_tree: identity.tree,
    recipe: relative(ROOT, recipeFile).split(sep).join('/'),
    recipe_sha256: hash(readFileSync(recipeFile)),
    registry_activation_hash: recipeData.registryActivationHash,
    receipt: `docs/evidence/_raw_reverify_${id}.log`,
    receipt_sha256: receiptHash,
    profile: profile(recipeData),
    toolchain,
    mutant_rejected: row.mutant_rejected === 'true',
    steps,
    mutant_steps: [],
    restored_steps: [],
  };
}

function createSelftestRegistry(id = 'SYNTH', status = 'PASS', mutantRejected = 'false') {
  const cells = HEADER.map(() => '');
  const set = (key, value) => { cells[HEADER.indexOf(key)] = value; };
  set('id', id);
  set('activation', `${id} activation`);
  set('scope', id === 'R0-21' ? 'shadow-model' : 'crate-internal');
  set('status', status);
  set('owner_phase', id === 'R0-21' ? 'Ph0' : 'SYNTH');
  set('mutant', 'self-test');
  set('mutant_rejected', mutantRejected);
  const registry = readRegistry(encode([HEADER, cells]));
  return { registry, row: registry.get(id) };
}

function makeSelftestStep(name, argv, env, regexes, expectedExit = 0, actualExit = expectedExit) {
  return {
    name, argv, env, expect: { exit: expectedExit, stdoutRegex: regexes },
    exit: actualExit, expected: expectedExit, signal: null, error: null,
    passed: actualExit === expectedExit,
    matches: regexes.map((regex) => ({ regex, matched: true, actual: [regex] })),
  };
}

function buildFixtureReceipt(steps, stepOutputs) {
  // Real receipt bytes, with structured frames written through the same
  // shared helper the verifier decodes with.
  let buffer = Buffer.from('E0_METADATA fixture receipt\n');
  const ranges = steps.map((step, index) => {
    const frames = Buffer.from(encodeOutputFrames(stepOutputs[index], ''));
    const start = buffer.length;
    buffer = Buffer.concat([buffer,
      Buffer.from(`E0_STEP ${JSON.stringify({ name: step.name })}\n`), frames]);
    return { start, end: buffer.length };
  });
  return { buffer, ranges };
}

function buildSelftestArtifacts({ out, fixture, identity, recipeFile, receiptName, stepOutputs }) {
  const { buffer, ranges } = buildFixtureReceipt(fixture.steps, stepOutputs);
  writeFileSync(join(out, receiptName), buffer);
  const receiptHash = hash(buffer);
  const results = fixture.steps.map((step, index) => ({
    name: step.name,
    argv: step.argv,
    env: step.env,
    expect: step.expect,
    exit: step.expect.exit,
    expected: step.expect.exit,
    passed: true,
    start: ranges[index].start,
    end: ranges[index].end,
    matches: step.expect.stdoutRegex.map((regex) => ({ regex, matched: true, actual: [regex] })),
  }));
  const summaryRow = makeSyntheticResult(fixture.id, fixture.row, identity, recipeFile,
    fixture.recipe, receiptHash, fixture.toolchain, results);
  const summary = {
    identity,
    selectedIds: [fixture.id],
    stages: ['oracle'],
    toolchains: { [fixture.id]: fixture.toolchain },
    rows: [summaryRow],
  };
  const proposal = {
    ...fixture.row,
    source_sha: identity.head,
    source_tree: identity.tree,
    profile: summaryRow.profile,
    toolchain: fixture.toolchain,
    receipt: summaryRow.receipt,
    receipt_sha256: receiptHash,
  };
  writeFileSync(join(out, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`);
  writeFileSync(join(out, 'proposed_rows.csv'),
    encode([HEADER, HEADER.map((key) => proposal[key] ?? '')]));
  writeFileSync(join(out, 'verdict.txt'), 'E0 PASS\n');
  return { summary, receiptBytes: buffer };
}

async function selftest() {
  // Point 5: both focused Node selftests must run under --selftest.
  for (const file of [
    'tests/evidence_reverify_support_selftest.mjs',
    'tests/evidence_reverify_runner_selftest.mjs',
  ]) {
    const focused = spawnSync(process.execPath, [join(ROOT, file)], {
      cwd: ROOT, encoding: 'utf8', timeout: 600000,
    });
    if (focused.error || focused.status !== 0) {
      throw Error(`focused selftest failed (${file}): ` +
        `${focused.stderr || focused.error?.message || 'nonzero exit'}`);
    }
  }
  // Unique scratch per run (parallel-safe), removed even when a check throws.
  const root = join(ROOT, `.rush/e0-selftest-${process.pid}-${Date.now()}`);
  try {
    await runSelftestChecks(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

async function runSelftestChecks(root) {
  let rejected = false;
  try { selectIds(['../bad']); } catch { rejected = true; }
  if (!rejected) throw Error('unsafe row id selector accepted');
  const schemaRow = createSelftestRegistry().row;
  const schemaStep = {
    name: 'schema fixture',
    argv: ['node', 'scripts/fake.mjs'],
    env: {},
    expect: { exit: 0, stdoutRegex: ['ok'] },
  };
  for (const invalidStep of [
    { ...schemaStep, env: { PATH: 'forbidden' } },
    { ...schemaStep, extra: true },
    { ...schemaStep, expect: { exit: 0, stderrRegex: ['not accepted'] } },
  ]) {
    let schemaRejected = false;
    try { validateStep(invalidStep, 'selftest'); } catch { schemaRejected = true; }
    if (!schemaRejected) throw Error('recipe step schema accepted forbidden data');
  }
  const schemaRecipe = {
    id: 'SYNTH', registryActivationHash: hash(Buffer.from(schemaRow.activation)),
    steps: [schemaStep], mutant: null,
  };
  validateRecipe(schemaRecipe, 'SYNTH', schemaRow);
  let activationRejected = false;
  try {
    validateRecipe({ ...schemaRecipe, registryActivationHash: '0'.repeat(64) }, 'SYNTH', schemaRow);
  } catch { activationRejected = true; }
  if (!activationRejected) throw Error('activation hash tamper accepted');
  const { registry, row } = createSelftestRegistry();
  const identity = { head: 'a'.repeat(40), tree: 'b'.repeat(40) };
  mkdirSync(root, { recursive: true });
  mkdirSync(join(root, 'tests'), { recursive: true });
  const original = Buffer.from('A\r\nunique\r\nB\r\n');
  const mutantFile = join(root, 'tests/loom_selftest.rs');
  writeFileSync(mutantFile, original);
  const mutant = {
    file: 'tests/loom_selftest.rs',
    find: 'unique\n',
    replace: 'mutated\n',
  };
  const snapshot = mutantPreflight(mutant, root);
  let awaitedReplacementObserved = false;
  try {
    await applyMutant(mutant, root, async () => {
      await Promise.resolve();
      awaitedReplacementObserved = readFileSync(mutantFile, 'utf8').includes('mutated');
      throw Error('expected test exception');
    });
  } catch { /* rollback must still execute after rejection */ }
  if (!awaitedReplacementObserved || !readFileSync(mutantFile).equals(original) ||
      hash(readFileSync(mutantFile)) !== snapshot.originalHash) {
    throw Error('async byte-exact mutant rollback failed');
  }
  for (const content of ['no match', 'unique\nunique\n']) {
    writeFileSync(mutantFile, content);
    let countRejected = false;
    try { mutantPreflight(mutant, root); } catch { countRejected = true; }
    if (!countRejected) throw Error('mutant fragment count accepted 0 or 2 matches');
  }

  const streamLog = join(root, 'stream.log');
  const streamSource = [
    'process.stdout.write("stdout-marker\\n".repeat(140000));',
    'process.stderr.write("stderr-marker\\n".repeat(140000));',
  ].join('');
  const streamStep = {
    name: 'stdout and stderr stream fixture',
    argv: [process.execPath, '-e', streamSource],
    env: {},
    expect: { exit: 0, stdoutRegex: ['stdout-marker', 'stderr-marker'] },
  };
  const streamResult = await runStep(streamStep, streamLog, 'STREAM', 'selftest');
  const stdoutMarker = streamResult.matches.find((marker) => marker.regex === 'stdout-marker');
  const stderrMarker = streamResult.matches.find((marker) => marker.regex === 'stderr-marker');
  if (streamResult.exit !== 0 || !streamResult.passed || !stdoutMarker?.matched ||
      !stdoutMarker.actual.length || !stderrMarker?.matched || !stderrMarker.actual.length ||
      readFileSync(streamLog).length < 2_000_000) {
    throw Error('stdout/stderr streaming assertion failed');
  }
  const metadataStep = {
    name: 'metadata-isolation',
    argv: [process.execPath, '-e', 'process.stdout.write("ordinary output\\n")'],
    env: {},
    expect: { exit: 0, stdoutRegex: ['metadata-isolation'] },
  };
  const metadataResult = await runStep(metadataStep, streamLog, 'STREAM', 'selftest');
  const metadataMarker = metadataResult.matches[0];
  if (metadataResult.exit !== 0 || metadataResult.passed || metadataMarker.matched ||
      metadataMarker.actual.length !== 0) {
    throw Error('step metadata leaked into streamed output matching');
  }

  const artifactRoot = join(root, 'artifacts');
  const recipeRoot = join(root, 'recipes');
  mkdirSync(artifactRoot);
  mkdirSync(recipeRoot);
  const recipeData = {
    id: 'SYNTH',
    registryActivationHash: hash(Buffer.from(row.activation)),
    steps: [{ name: 'first', argv: ['node', 'scripts/fake.mjs'], env: {},
      expect: { exit: 0, stdoutRegex: ['stdout-marker'] } },
      { name: 'second', argv: ['node', 'scripts/fake.mjs'], env: { RUSTFLAGS: 'fixture' },
        expect: { exit: 0, stdoutRegex: ['second-marker'] } }],
    mutant: null,
  };
  const recipeFile = recipePath('SYNTH', recipeRoot);
  writeFileSync(recipeFile, `${JSON.stringify(recipeData, null, 2)}\n`);
  const steps = [
    makeSelftestStep('first', recipeData.steps[0].argv, {}, ['stdout-marker']),
    makeSelftestStep('second', recipeData.steps[1].argv, recipeData.steps[1].env,
      ['second-marker']),
  ];
  const receiptName = '_raw_reverify_SYNTH.log';
  const fixture = {
    id: 'SYNTH', row, recipe: recipeData, steps, toolchain: 'fixture toolchain',
  };
  const baselineOutputs = recipeData.steps.map((step) => `${step.expect.stdoutRegex[0]}\n`);
  const baselineBuild = buildSelftestArtifacts({
    out: artifactRoot, fixture, identity, recipeFile, receiptName, stepOutputs: baselineOutputs,
  });
  const baselineSummary = baselineBuild.summary;
  const baselineReceiptBytes = baselineBuild.receiptBytes;
  verify(artifactRoot, ['SYNTH'], identity, registry, recipeRoot);
  const baselineRecipeBytes = readFileSync(recipeFile);
  const baselineCsv = readFileSync(join(artifactRoot, 'proposed_rows.csv'), 'utf8');
  const baselineReceiptHash = hash(baselineReceiptBytes);
  const badCases = [
    'missing-receipt', 'receipt-hash', 'recipe-file', 'recipe-hash',
    'toolchain-row', 'toolchain-map', 'toolchain-csv', 'step-missing',
    'step-extra', 'step-reordered', 'step-argv', 'step-env', 'step-expect',
  ];
  for (const kind of badCases) {
    writeFileSync(recipeFile, baselineRecipeBytes);
    const fresh = buildSelftestArtifacts({
      out: artifactRoot, fixture, identity, recipeFile, receiptName, stepOutputs: baselineOutputs,
    }).summary;
    if (JSON.stringify(fresh) !== JSON.stringify(baselineSummary)) {
      throw Error('self-test fixture did not return to its valid baseline');
    }
    verify(artifactRoot, ['SYNTH'], identity, registry, recipeRoot);

    if (kind === 'missing-receipt') rmSync(join(artifactRoot, receiptName));
    if (kind === 'receipt-hash') fresh.rows[0].receipt_sha256 = '0'.repeat(64);
    if (kind === 'recipe-file') writeFileSync(recipeFile, '{"id":"SYNTH"}\n');
    if (kind === 'recipe-hash') fresh.rows[0].recipe_sha256 = 'c'.repeat(64);
    if (kind === 'toolchain-row') fresh.rows[0].toolchain = 'forged';
    if (kind === 'toolchain-map') fresh.toolchains.SYNTH = 'forged';
    if (kind === 'step-missing') fresh.rows[0].steps = [fresh.rows[0].steps[0]];
    if (kind === 'step-extra') fresh.rows[0].steps.push(fresh.rows[0].steps[1]);
    if (kind === 'step-reordered') fresh.rows[0].steps.reverse();
    if (kind === 'step-argv') fresh.rows[0].steps[0].argv = ['node', 'forged.mjs'];
    if (kind === 'step-env') fresh.rows[0].steps[0].env = { RUSTFLAGS: 'forged' };
    if (kind === 'step-expect') fresh.rows[0].steps[0].expected = 9;
    writeFileSync(join(artifactRoot, 'summary.json'), `${JSON.stringify(fresh, null, 2)}\n`);
    writeFileSync(join(artifactRoot, 'proposed_rows.csv'), baselineCsv);
    if (kind === 'toolchain-csv') {
      const cells = parseCsv(baselineCsv)[1];
      cells[5] = 'forged';
      writeFileSync(join(artifactRoot, 'proposed_rows.csv'), encode([HEADER, cells]));
    }
    let rejectedArtifact = false;
    try { verify(artifactRoot, ['SYNTH'], identity, registry, recipeRoot); }
    catch { rejectedArtifact = true; }
    if (!rejectedArtifact) throw Error(`artifact verifier accepted isolated tamper: ${kind}`);
    if (hash(baselineReceiptBytes) !== baselineReceiptHash) throw Error('receipt fixture hash drift');
  }

  // End-to-end: the real writer (runSteps -> receipt) feeds the real verifier.
  const e2e = createSelftestRegistry('E2E');
  const e2eRoot = join(root, 'e2e-recipes');
  mkdirSync(e2eRoot);
  const e2eRecipe = {
    id: 'E2E',
    registryActivationHash: hash(Buffer.from(e2e.row.activation)),
    steps: [{ name: 'e2e', env: {},
      argv: ['node', 'scripts/ev1_receipt_fixture.mjs'],
      expect: { exit: 0, stdoutRegex: ['e2e-out', 'e2e-err'] } }],
    mutant: null,
  };
  const e2ePath = recipePath('E2E', e2eRoot);
  writeFileSync(e2ePath, `${JSON.stringify(e2eRecipe, null, 2)}\n`);
  const e2eLog = join(artifactRoot, '_raw_reverify_E2E.log');
  writeFileSync(e2eLog, '');
  const e2eSteps = await runSteps(e2eRecipe.steps, e2eLog, 'E2E', 'baseline');
  if (e2eSteps.length !== 1 || !e2eSteps[0].passed) throw Error('e2e real step did not pass');
  const writeE2e = (receiptBytes) => {
    const result = makeSyntheticResult('E2E', e2e.row, identity, e2ePath, e2eRecipe,
      hash(receiptBytes), 'e2e toolchain', e2eSteps);
    writeFileSync(join(artifactRoot, 'summary.json'), `${JSON.stringify({
      identity, selectedIds: ['E2E'], stages: ['oracle'],
      toolchains: { E2E: 'e2e toolchain' }, rows: [result],
    }, null, 2)}\n`);
    const proposal = { ...e2e.row, source_sha: identity.head, source_tree: identity.tree,
      profile: result.profile, toolchain: 'e2e toolchain', receipt: result.receipt,
      receipt_sha256: result.receipt_sha256 };
    writeFileSync(join(artifactRoot, 'proposed_rows.csv'),
      encode([HEADER, HEADER.map((key) => proposal[key] ?? '')]));
    writeFileSync(join(artifactRoot, 'verdict.txt'), 'E0 PASS\n');
  };
  const e2eBytes = readFileSync(e2eLog);
  writeE2e(e2eBytes);
  verify(artifactRoot, ['E2E'], identity, e2e.registry, e2eRoot);
  // One payload byte inside the step's frames flipped, hashes re-stamped: the
  // receipt hash check passes, so only frame replay can reject it.
  const frameAt = e2eBytes.indexOf('E0_STDOUT ', e2eSteps[0].start);
  if (frameAt < 0 || frameAt >= e2eSteps[0].end) throw Error('e2e receipt has no stdout frame');
  const forged = Buffer.from(e2eBytes);
  const at = frameAt + 'E0_STDOUT '.length;
  forged[at] = forged[at] === 0x41 ? 0x42 : 0x41;
  writeFileSync(e2eLog, forged);
  writeE2e(forged);
  let e2eForgedRejected = false;
  try { verify(artifactRoot, ['E2E'], identity, e2e.registry, e2eRoot); }
  catch (error) {
    // Must fail on replayed content, not on a stale hash.
    if (/sha|hash/i.test(error.message)) throw Error(`e2e forged receipt rejected for the wrong reason: ${error.message}`);
    e2eForgedRejected = true;
  }
  if (!e2eForgedRejected) throw Error('verifier accepted a forged real receipt frame');
  rmSync(e2eLog);

  const r0RegistryData = createSelftestRegistry('R0-21', 'PASS', 'true');
  const r0Root = join(root, 'r0-recipes');
  mkdirSync(r0Root);
  const panicRecipe = {
    id: 'R0-21',
    registryActivationHash: hash(Buffer.from(r0RegistryData.row.activation)),
    steps: [
      { name: 'loom_sidecar_bitmap target', argv: ['cargo', 'test', '--test', 'first'], env: {},
        expect: { exit: 0, stdoutRegex: ['negative_class - should panic'] } },
      { name: 'loom_terminal_large target', argv: ['cargo', 'test', '--test', 'second'], env: {},
        expect: { exit: 0, stdoutRegex: ['producer_owner_cut'] } },
      { name: 'loom_terminal_owner_drain target', argv: ['cargo', 'test', '--test', 'third'], env: {},
        expect: { exit: 0, stdoutRegex: ['premature_release - should panic'] } },
    ],
    mutant: null,
  };
  const panicPath = recipePath('R0-21', r0Root);
  writeFileSync(panicPath, `${JSON.stringify(panicRecipe, null, 2)}\n`);
  const panicSteps = panicRecipe.steps.map((step) => ({
    ...makeSelftestStep(step.name, step.argv, step.env, step.expect.stdoutRegex),
    matches: step.expect.stdoutRegex.map((regex) => ({
      regex, matched: true, actual: [regex],
    })),
  }));
  const panicFixture = {
    id: 'R0-21', row: r0RegistryData.row, recipe: panicRecipe, steps: panicSteps,
    toolchain: 'stable fixture toolchain',
  };
  buildSelftestArtifacts({ out: artifactRoot, fixture: panicFixture, identity,
    recipeFile: panicPath, receiptName: '_raw_reverify_R0-21.log',
    stepOutputs: panicRecipe.steps.map((step) => `${step.expect.stdoutRegex.join('\n')}\n`) });
  verify(artifactRoot, ['R0-21'], identity, r0RegistryData.registry, r0Root);
  const wrongR0 = JSON.parse(readFileSync(join(artifactRoot, 'summary.json'), 'utf8'));
  wrongR0.rows[0].steps = wrongR0.rows[0].steps.map((step) => ({
    ...step,
    matches: step.matches.map((marker) => /should panic/i.test(marker.regex)
      ? { ...marker, matched: false, actual: [] }
      : marker),
  }));
  writeFileSync(join(artifactRoot, 'summary.json'), `${JSON.stringify(wrongR0, null, 2)}\n`);
  let wrongR0Rejected = false;
  try { verify(artifactRoot, ['R0-21'], identity, r0RegistryData.registry, r0Root); }
  catch { wrongR0Rejected = true; }
  if (!wrongR0Rejected) throw Error('R0-21 accepted missing embedded mutant marker');

  const mixedRoot = join(root, 'mixed-recipes');
  mkdirSync(mixedRoot);
  const stableRow = { ...row, id: 'STABLE' };
  const nightlyRow = { ...row, id: 'NIGHTLY' };
  const stableRecipe = {
    ...recipeData,
    id: 'STABLE',
    steps: [recipeData.steps[0]],
  };
  const nightlyRecipe = {
    ...recipeData,
    id: 'NIGHTLY',
    steps: [{ ...recipeData.steps[0], argv: ['cargo', '+nightly', 'miri', 'test', '--test', 'fixture'] }],
  };
  const stablePath = recipePath('STABLE', mixedRoot);
  const nightlyPath = recipePath('NIGHTLY', mixedRoot);
  writeFileSync(stablePath, `${JSON.stringify(stableRecipe, null, 2)}\n`);
  writeFileSync(nightlyPath, `${JSON.stringify(nightlyRecipe, null, 2)}\n`);
  const stableStep = makeSelftestStep('first', stableRecipe.steps[0].argv, {}, ['stdout-marker']);
  const nightlyStep = makeSelftestStep('first', nightlyRecipe.steps[0].argv, {}, ['stdout-marker']);
  const stableReceipt = buildFixtureReceipt([stableStep], ['stdout-marker\n']);
  const nightlyReceipt = buildFixtureReceipt([nightlyStep], ['stdout-marker\n']);
  stableStep.start = stableReceipt.ranges[0].start;
  stableStep.end = stableReceipt.ranges[0].end;
  nightlyStep.start = nightlyReceipt.ranges[0].start;
  nightlyStep.end = nightlyReceipt.ranges[0].end;
  writeFileSync(join(artifactRoot, '_raw_reverify_STABLE.log'), stableReceipt.buffer);
  writeFileSync(join(artifactRoot, '_raw_reverify_NIGHTLY.log'), nightlyReceipt.buffer);
  const mixedRegistry = new Map([['STABLE', stableRow], ['NIGHTLY', nightlyRow]]);
  const mixedRows = [
    makeSyntheticResult('STABLE', stableRow, identity, stablePath, stableRecipe,
      hash(stableReceipt.buffer), 'stable toolchain', [stableStep]),
    makeSyntheticResult('NIGHTLY', nightlyRow, identity, nightlyPath, nightlyRecipe,
      hash(nightlyReceipt.buffer), 'nightly toolchain', [nightlyStep]),
  ];
  const mixedSummary = {
    identity, selectedIds: ['STABLE', 'NIGHTLY'], stages: ['oracle'],
    toolchains: { STABLE: 'stable toolchain', NIGHTLY: 'nightly toolchain' }, rows: mixedRows,
  };
  const mixedProposals = mixedRows.map((result, index) => ({
    ...[stableRow, nightlyRow][index], source_sha: identity.head, source_tree: identity.tree,
    profile: result.profile, toolchain: result.toolchain,
    receipt: result.receipt, receipt_sha256: result.receipt_sha256,
  }));
  writeFileSync(join(artifactRoot, 'summary.json'), `${JSON.stringify(mixedSummary, null, 2)}\n`);
  writeFileSync(join(artifactRoot, 'proposed_rows.csv'), encode([
    HEADER, ...mixedProposals.map((proposal) => HEADER.map((key) => proposal[key] ?? '')),
  ]));
  writeFileSync(join(artifactRoot, 'verdict.txt'), 'E0 PASS\n');
  verify(artifactRoot, ['STABLE', 'NIGHTLY'], identity, mixedRegistry, mixedRoot);

  console.log('PASS E0 selftest: streaming; real-receipt e2e; R0 witnesses; mixed toolchains; independent artifact tampering');
}

try {
  await main();
} catch (error) {
  console.error(`E0 error: ${error.message}`);
  process.exitCode = 1;
}
