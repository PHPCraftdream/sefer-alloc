export const HEADER = [
  'id', 'transition', 'entry', 'features', 'os', 'toolchain', 'profile', 'source_sha',
  'source_tree', 'binary_id', 'activation', 'positive_marker', 'mutant',
  'mutant_rejected', 'receipt', 'receipt_sha256', 'scope', 'status',
  'status_reason', 'owner_phase', 'source_cell',
];
export const STAGES = new Set(['prepare', 'build', 'oracle', 'measure']);
export const ID = /^[A-Za-z0-9-]+$/;
export const SHA = /^[a-f0-9]{64}$/;

export function parseCsv(text) {
  const normalized = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const rows = [];
  let row = [];
  let field = '';
  let quoted = false;

  for (let index = 0; index < normalized.length; index += 1) {
    const char = normalized[index];
    if (quoted) {
      if (char === '"' && normalized[index + 1] === '"') {
        field += '"';
        index += 1;
      } else if (char === '"') {
        quoted = false;
      } else {
        field += char;
      }
    } else if (char === '"' && field === '') {
      quoted = true;
    } else if (char === ',') {
      row.push(field);
      field = '';
    } else if (char === '\n') {
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else {
      field += char;
    }
  }
  if (quoted) throw Error('CSV quote');
  if (field || row.length) {
    row.push(field);
    rows.push(row);
  }
  return rows.filter((cells) => !(cells.length === 1 && !cells[0].trim()));
}

export function selectIds(ids) {
  if (!Array.isArray(ids) || !ids.length ||
      ids.some((id) => typeof id !== 'string' || !ID.test(id)) ||
      new Set(ids).size !== ids.length) {
    throw Error('invalid/duplicate selected IDs');
  }
  return ids;
}

export function strict(object, allowed, where) {
  if (!object || typeof object !== 'object' || Array.isArray(object)) {
    throw Error(`${where} object`);
  }
  for (const key of Object.keys(object)) {
    if (!allowed.includes(key)) throw Error(`${where} unknown ${key}`);
  }
  for (const key of allowed) {
    if (!(key in object)) throw Error(`${where} missing ${key}`);
  }
}

export function readRegistry(text) {
  const rows = parseCsv(text);
  if (JSON.stringify(rows[0]) !== JSON.stringify(HEADER)) throw Error('registry header mismatch');
  const map = new Map();
  for (const cells of rows.slice(1)) {
    if (cells.length !== HEADER.length || map.has(cells[0])) {
      throw Error(`bad registry row ${cells[0]}`);
    }
    map.set(cells[0], Object.fromEntries(HEADER.map((key, index) => [key, cells[index]])));
  }
  return map;
}

// Admitted mutant targets: crate sources and Loom shadow models only.
export function assertMutantPath(file) {
  if (typeof file !== 'string' ||
      !/^(?:src\/(?:[A-Za-z0-9_.-]+\/)*[A-Za-z0-9_.-]+\.rs|tests\/loom_[A-Za-z0-9_.-]+\.rs)$/.test(file) ||
      file.split('/').includes('..') || file.split('/').includes('.')) {
    throw Error('unsafe mutant path');
  }
}

export function mutantPreflight(mutant, root, { resolvePath, relativePath, separator, isAbsolute, existsSync, lstatSync, realpathSync, readFileSync, writeFileSync, lf, hash }) {
  // Re-checked here: preflight must not rely on validateRecipe having run.
  assertMutantPath(mutant.file);
  const path = resolvePath(root, mutant.file);
  const rel = relativePath(root, path);
  if (isAbsolute(mutant.file) || path === root || isAbsolute(rel) || rel === '..' ||
      rel.startsWith(`..${separator}`)) throw Error('mutant path escape');
  if (path !== root && !path.startsWith(root + separator)) throw Error('mutant path escape');
  if (mutant.file.split('/').some((part) => part === '.' || part === '..')) {
    throw Error('mutant path traversal');
  }
  let current = root;
  for (const part of mutant.file.split('/')) {
    current = resolvePath(current, part);
    if (existsSync(current) && lstatSync(current).isSymbolicLink()) throw Error('mutant symlink');
  }
  const real = realpathSync(path);
  if (!real.startsWith(root + separator)) throw Error('mutant realpath escape');
  const original = readFileSync(real);
  const source = lf(original.toString('utf8'));
  const find = lf(mutant.find);
  const replace = lf(mutant.replace);
  const count = source.split(find).length - 1;
  if (count !== 1) throw Error(`mutant exact count ${count}`);
  return { file: real, original, source, find, replace, originalHash: hash(original) };
}

export function evaluateStep(expect, result, output) {
  const markers = expect.stdoutRegex.map((regex) => ({
    regex, matched: new RegExp(regex).test(output),
    actual: [...output.matchAll(new RegExp(regex, 'g'))].map((match) => match[0]),
  }));
  const timedOut = [124, 137].includes(result.exit) ||
    ['SIGTERM', 'SIGKILL'].includes(result.signal) || result.timedOut === true;
  const compileFailure = /(?:^|\n)error: could not compile|error\[E\d+\]:|failed to run custom build command|Could not compile/i.test(output);
  const crashReceipt = /(?:process (?:did not|didn't) exit successfully:[^\n]*(?:signal:\s*(?:11|7|6)|SIGSEGV|SIGBUS|SIGABRT|STATUS_ACCESS_VIOLATION|0xc0000005)|STATUS_ACCESS_VIOLATION|0xc0000005)/i.test(output);
  const runtimeSignal = ['SIGSEGV', 'SIGBUS', 'SIGABRT'].includes(result.signal) ||
    ([3221225477, -1073741819].includes(result.exit) && /STATUS_ACCESS_VIOLATION|0xc0000005/i.test(output));
  const signalAccepted = expect.mutant === 'signal' && !timedOut && !compileFailure &&
    /(?:^|\n)running [1-9]\d* test\b/.test(output) &&
    (runtimeSignal || (result.exit === 101 && result.cargoBuildPassed === true && crashReceipt));
  const exitAccepted = Number.isInteger(expect.exit) && result.exit === expect.exit &&
    !result.signal && !result.error && !timedOut && !compileFailure;
  const nonzeroAccepted = expect.exitNonZero === true && Number.isInteger(result.exit) &&
    result.exit !== 0 && result.exit !== 124 && result.exit !== 137 && !result.signal &&
    !result.error && !timedOut && !compileFailure;
  return { passed: !result.error && !timedOut && (signalAccepted || exitAccepted || nonzeroAccepted) &&
    markers.every((marker) => marker.matched), matches: markers, timedOut };
}
export function validateStep(step, where) {
  strict(step, ['name', 'env', 'argv', 'expect'], where);
  if (typeof step.name !== 'string' || !step.name.trim() || step.name.length > 120) {
    throw Error(`${where} name`);
  }
  if (!step.env || typeof step.env !== 'object' || Array.isArray(step.env)) {
    throw Error(`${where} env`);
  }
  for (const [key, value] of Object.entries(step.env)) {
    if (!/^(MIRIFLAGS|RUSTFLAGS|CARGO_[A-Z0-9_]+|LOOM_[A-Z0-9_]+)$/.test(key) ||
        typeof value !== 'string' || value.includes('\0')) {
      throw Error(`${where} env ${key}`);
    }
  }
  if (!Array.isArray(step.argv) || !step.argv.length ||
      step.argv.some((arg) => typeof arg !== 'string' || !arg || arg.includes('\0'))) {
    throw Error(`${where} argv`);
  }
  const [binary, ...args] = step.argv;
  if (binary === 'cargo') {
    const subcommand = args[0] === '+nightly' ? args[1] : args[0];
    if (!['test', 'miri'].includes(subcommand)) throw Error(`${where} command`);
  } else if (binary === 'node') {
    if (!/^scripts\/[A-Za-z0-9_.-]+\.mjs$/.test(args[0] || '')) {
      throw Error(`${where} node path`);
    }
  } else if (binary !== 'rustup') {
    throw Error(`${where} executable`);
  }
  if (!step.expect || typeof step.expect !== 'object' || Array.isArray(step.expect)) {
    throw Error(`${where} expect`);
  }
  for (const key of Object.keys(step.expect)) {
    if (!['exit', 'mutant', 'exitNonZero', 'stdoutRegex'].includes(key)) {
      throw Error(`${where} expect unknown ${key}`);
    }
  }
  const expectationKeys = Object.keys(step.expect);
  const markersKeyValid = expectationKeys.includes('stdoutRegex') &&
    expectationKeys.every((key) => ['exit', 'mutant', 'exitNonZero', 'stdoutRegex'].includes(key));
  const signalExpectation = step.expect.mutant === 'signal' && !('exit' in step.expect) &&
    !('exitNonZero' in step.expect) && markersKeyValid;
  const nonzeroExpectation = step.expect.exitNonZero === true && !('exit' in step.expect) &&
    !('mutant' in step.expect) && markersKeyValid;
  const exitExpectation = Number.isInteger(step.expect.exit) && !('mutant' in step.expect) &&
    !('exitNonZero' in step.expect) && markersKeyValid;
  if (!signalExpectation && !nonzeroExpectation && !exitExpectation) throw Error(`${where} expect keys`);
  if (signalExpectation && step.argv[0] === 'cargo' && args.includes('--no-run') &&
      (step.expect.exit !== 0 || step.expect.mutant !== undefined || step.expect.exitNonZero !== undefined)) {
    throw Error(`${where} Cargo build step must expect exit zero`);
  }
  const signalMarkers = step.expect.stdoutRegex;
  if (signalExpectation && step.argv[0] === 'cargo' && !args.includes('--no-run') &&
      (!signalMarkers.some((pattern) => /running(?: \[1-9\]\[0-9\]\*| 1 test| [1-9][0-9]* test)/.test(pattern)) || signalMarkers.length < 2)) {
    throw Error(`${where} cargo signal requires running-test and signal receipt markers`);
  }
  const patterns = step.expect.stdoutRegex;
  if (!Array.isArray(patterns) || !patterns.length) throw Error(`${where} patterns`);
  for (const pattern of [...patterns, ...(step.expect.stderrRegex || [])]) {
    if (typeof pattern !== 'string' || !pattern) throw Error(`${where} regex`);
    new RegExp(pattern);
  }
}

export function validateRecipe(data, id, row, hash) {
  strict(data, ['id', 'registryActivationHash', 'steps', 'mutant'], id);
  if (data.id !== id || !ID.test(id)) throw Error(`${id} mismatch`);
  if (!SHA.test(data.registryActivationHash) ||
      data.registryActivationHash !== hash(Buffer.from(row.activation, 'utf8'))) {
    throw Error(`${id} activation hash`);
  }
  if (!Array.isArray(data.steps) || !data.steps.length) throw Error(`${id} steps`);
  data.steps.forEach((step, index) => validateStep(step, `${id}.steps.${index}`));
  if (data.mutant !== null) {
    const mutant = data.mutant;
    strict(mutant, ['file', 'find', 'replace', 'steps', 'expect'], `${id}.mutant`);
    if (!mutant.expect || typeof mutant.expect !== 'object' || Array.isArray(mutant.expect) ||
        Object.keys(mutant.expect).some((key) => !['signal', 'exitNonZero', 'stdoutRegex'].includes(key))) {
      throw Error('mutant aggregate expectation keys');
    }

    assertMutantPath(mutant.file);
    if (typeof mutant.find !== 'string' || !mutant.find ||
        typeof mutant.replace !== 'string' || !mutant.replace || mutant.find === mutant.replace) {
      throw Error('bad mutant text');
    }
    if (!Array.isArray(mutant.steps) || !mutant.steps.length) throw Error('mutant steps');
    mutant.steps.forEach((step, index) => validateStep(step, `${id}.mutant.${index}`));
    const expectationKeys = Object.keys(mutant.expect);
    const expectationKind = expectationKeys.includes('signal') && mutant.expect.signal === true &&
      !expectationKeys.includes('exitNonZero')
      ? 'signal'
      : expectationKeys.includes('exitNonZero') && mutant.expect.exitNonZero === true &&
        !expectationKeys.includes('signal') ? 'exitNonZero' : null;
    if (!expectationKind || expectationKeys.length !== 2 ||
        !Array.isArray(mutant.expect.stdoutRegex) || !mutant.expect.stdoutRegex.length) {
      throw Error('mutant aggregate expectation');
    }
    if (mutant.expect.signal === true) {
      const runtimeStep = mutant.steps.at(-1);
      const setupSteps = mutant.steps.slice(0, -1);
      const buildMatchesRuntime = setupSteps.some((step) => step.argv[0] === 'cargo' &&
        step.argv.includes('--no-run') && JSON.stringify(step.env) === JSON.stringify(runtimeStep.env) &&
        cargoBuildSignature(step.argv, step.env) === cargoBuildSignature(runtimeStep.argv, runtimeStep.env));
      if (setupSteps.some((step) => step.expect.exit !== 0 || step.expect.exitNonZero !== undefined) ||
          !buildMatchesRuntime || runtimeStep.expect.mutant !== 'signal' || runtimeStep.argv[0] !== 'cargo' ||
          !runtimeStep.argv.includes('--nocapture') || runtimeStep.argv.includes('--no-run') ||
          runtimeStep.expect.stdoutRegex.length < 2 ||
          !runtimeStep.expect.stdoutRegex.some((pattern) => /running/.test(pattern))) {
        throw Error('signal mutant requires exit-zero setup and final runtime signal step');
      }
    }
    for (const pattern of mutant.expect.stdoutRegex) {
      if (typeof pattern !== 'string' || !pattern) throw Error('mutant regex');
      new RegExp(pattern);
    }
  }
  return data;
}

export function mutantVerdict(expect, steps) {
  const complete = Array.isArray(steps) && steps.length > 0 && steps.every((step) => step.passed);
  if (!complete) return false;
  const last = steps.at(-1);
  if (expect.signal === true) {
    return last.expect?.mutant === 'signal' && last.argv?.[0] === 'cargo' &&
      steps.slice(0, -1).every((step) => step.expect?.exit === 0 && step.passed && !step.error && !step.signal) &&
      last.passed;
  }
  return expect.exitNonZero === true && steps.some((step) =>
    (step.expect?.exitNonZero === true || step.exit !== 0) && step.exit !== null &&
    !step.signal && !step.error && !step.timedOut);
}

export function cargoBuildSignature(argv, env) {
  return JSON.stringify({ argv: argv.filter((arg) => arg !== '--no-run' && arg !== '--' && arg !== '--nocapture'), env });
}

// Shared receipt-frame codec: the runner encodes step output with
// encodeOutputFrames() and the verifier decodes it with decodeOutputFrames().
export function encodeOutputFrames(stdout, stderr) {
  let text = '';
  if (stdout && stdout.length) {
    text += `E0_STDOUT ${Buffer.from(stdout, 'utf8').toString('base64')}\n`;
  }
  if (stderr && stderr.length) {
    text += `E0_STDERR ${Buffer.from(stderr, 'utf8').toString('base64')}\n`;
  }
  return text;
}
export function decodeOutputFrames(text, channel) {
  return [...text.matchAll(/E0_(STDOUT|STDERR) ([A-Za-z0-9+/=]+)\n/g)]
    .filter((frame) => frame[1] === channel)
    .map((frame) => Buffer.from(frame[2], 'base64').toString('utf8')).join('');
}
export function verifyPerformed(actual, expected, where, passed, mutant = false, outputForStep = null, stderrForStep = null) {
  if (!Array.isArray(actual) || actual.length > expected.length || (passed && actual.length !== expected.length)) {
    throw Error(`${where} step count mismatch`);
  }
  let successfulCargoBuild = null;
  for (let index = 0; index < actual.length; index += 1) {
    const result = actual[index];
    const step = expected[index];
    const expectedExit = step.expect.mutant === 'signal' ? null :
      (step.expect.exitNonZero === true ? -1 : step.expect.exit);
    if (!result || result.name !== step.name ||
        JSON.stringify(result.argv) !== JSON.stringify(step.argv) ||
        JSON.stringify(result.env) !== JSON.stringify(step.env) ||
        JSON.stringify(result.expect) !== JSON.stringify(step.expect) ||
        result.expected !== expectedExit) {
      throw Error(`${where} step mismatch ${index}`);
    }
    const output = outputForStep ? outputForStep(result) : result.output || '';
    const stepResult = { ...result };
    const cargoBuildForStep = successfulCargoBuild && step.argv?.[0] === 'cargo' &&
      step.argv.includes('--test') && successfulCargoBuild.signature === cargoBuildSignature(step.argv, step.env);
    const evaluated = evaluateStep(step.expect, { ...stepResult, cargoBuildPassed: cargoBuildForStep }, output);
    if (step.argv?.[0] === 'cargo' && step.argv.includes('--no-run') && evaluated.passed &&
        result.exit === 0 && !result.signal && !result.error) {
      successfulCargoBuild = { signature: cargoBuildSignature(step.argv, step.env) };
    }
    if (result.passed !== evaluated.passed ||
        result.expected !== expectedExit ||
        result.exit !== undefined && result.exit !== null && !Number.isInteger(result.exit) ||
        !Array.isArray(result.matches) ||
        JSON.stringify(result.matches) !== JSON.stringify(evaluated.matches) ||
        result.signal !== undefined && result.signal !== null && typeof result.signal !== 'string' ||
        result.error !== undefined && result.error !== null && typeof result.error !== 'string') {
      throw Error(`${where} step result malformed/verdict inconsistent ${index}`);
    }
    if (passed && !evaluated.passed) throw Error(`${where} step did not pass ${index}`);
  }
  if (passed && mutant && expected.some((step) => step.expect.mutant === 'signal') &&
      !mutantVerdict({ signal: true }, actual)) throw Error(`${where} aggregate signal verdict did not pass`);
  if (passed && mutant && expected.some((step) => step.expect.exitNonZero === true) &&
      !mutantVerdict({ exitNonZero: true }, actual)) throw Error(`${where} aggregate exit verdict did not pass`);
}

export function verifyArtifacts({ out, chosen, ident, registry, recipeRoot, hash, existsSync, readFileSync,
  join, load, recipePath, lf, SHA, STAGES, HEADER, parseCsv, verifyPerformed }) {
  const summary = JSON.parse(readFileSync(join(out, 'summary.json'), 'utf8'));
  if (summary.identity?.head !== ident.head || summary.identity?.tree !== ident.tree) {
    throw Error('identity mismatch');
  }
  if (JSON.stringify(summary.selectedIds) !== JSON.stringify(chosen) ||
      !Array.isArray(summary.rows) || !Array.isArray(summary.stages) ||
      summary.stages.some((stage) => !STAGES.has(stage))) {
    throw Error('summary structure');
  }

  const csv = parseCsv(readFileSync(join(out, 'proposed_rows.csv'), 'utf8'));
  if (JSON.stringify(csv[0]) !== JSON.stringify(HEADER) ||
      csv.slice(1).some((row) => row.length !== HEADER.length)) {
    throw Error('proposal malformed');
  }
  const map = new Map();
  for (const result of summary.rows) {
    if (!chosen.includes(result.id) || map.has(result.id) ||
        !['pass', 'fail', 'needs-manual', 'INVALID'].includes(result.verdict)) {
      throw Error(`summary ID invalid ${result.id}`);
    }
    map.set(result.id, result);
    const registryRow = registry.get(result.id);
    if (!registryRow || registryRow.status !== result.status ||
        result.source_sha !== ident.head || result.source_tree !== ident.tree) {
      throw Error(`status/identity mismatch ${result.id}`);
    }
    const receipt = join(out, `_raw_reverify_${result.id}.log`);
    const file = recipePath(result.id, recipeRoot);
    if (!existsSync(file)) {
      if (result.recipe || result.recipe_sha256 ||
          (result.verdict === 'needs-manual' && !result.reason)) {
        throw Error(`missing recipe metadata mismatch ${result.id}`);
      }
      if (result.steps?.length || result.mutant_steps?.length || result.restored_steps?.length) {
        throw Error(`missing recipe has performed steps ${result.id}`);
      }
    } else {
      const loaded = load(result.id, registryRow, recipeRoot);
      if (!loaded.recipe || loaded.path !== result.recipe || loaded.recipeHash !== result.recipe_sha256 ||
          result.registry_activation_hash !== loaded.recipe.registryActivationHash) {
        throw Error(`recipe pin/activation mismatch ${result.id}`);
      }
      const data = loaded.recipe;
      const receipt = join(out, `_raw_reverify_${result.id}.log`);
      if (!existsSync(receipt)) throw Error(`receipt missing ${result.id}`);
      const receiptBytes = readFileSync(receipt);
      const frameFor = (step, channel) => {
        if (!Number.isInteger(step.start) || !Number.isInteger(step.end) ||
            step.start < 0 || step.end < step.start || step.end > receiptBytes.length) {
          throw Error(`step receipt range malformed ${result.id}`);
        }
        return decodeOutputFrames(receiptBytes.subarray(step.start, step.end).toString('utf8'), channel);
      };
      const outputForStep = (step) => frameFor(step, 'STDOUT');
      const stderrForStep = (step) => frameFor(step, 'STDERR');
      verifyPerformed(result.steps, data.steps, `${result.id} baseline`, result.verdict === 'pass', false,
        (step) => outputForStep(step), (step) => stderrForStep(step));
      if (data.mutant) {
        verifyPerformed(result.mutant_steps, data.mutant.steps,
          `${result.id} mutant`, result.verdict === 'pass', true,
          (step) => outputForStep(step), (step) => stderrForStep(step));
        verifyPerformed(result.restored_steps, data.steps,
          `${result.id} restored`, result.verdict === 'pass', false,
          (step) => outputForStep(step), (step) => stderrForStep(step));
        if (result.verdict === 'pass' && (!result.mutant_rejected ||
            result.mutant_original_sha256 !== result.mutant_restored_sha256 ||
            !SHA.test(result.mutant_original_sha256 || '') || !result.mutant_matches?.length ||
            result.mutant_matches.some((match) => !match.matched) ||
            !mutantVerdict(data.mutant.expect, result.mutant_steps))) {
          throw Error(`mutant/restoration mismatch ${result.id}`);
        }
      } else {
        const witnesses = data.steps.flatMap((step) =>
          step.expect.stdoutRegex.filter((regex) => /should panic/i.test(regex)));
        const isR0 = result.id === 'R0-21';
        if (result.mutant_steps?.length || result.restored_steps?.length ||
            result.mutant_rejected !== (isR0 && registryRow.mutant_rejected === 'true') ||
            (isR0 && result.verdict === 'pass' && (data.steps.length !== 3 || witnesses.length !== 2 ||
              data.steps.some((step) => step.expect.exit !== 0) ||
              result.steps.length !== data.steps.length ||
              result.steps.flatMap((step) => step.matches).filter((marker) =>
                /should panic/i.test(marker.regex) && marker.matched === true).length !== 2))) {
          throw Error(`unexpected embedded mutant cycle ${result.id}`);
        }
        if (isR0) verifyPerformed(result.steps, data.steps, 'R0-21 embedded mutant witnesses',
          result.verdict === 'pass', false, outputForStep, stderrForStep);
      }
    }
    if (result.verdict === 'pass') {
      if (!result.recipe || !SHA.test(result.recipe_sha256 || '') ||
          typeof result.toolchain !== 'string' || !result.toolchain || !result.steps.length ||
          !result.steps[0].passed || summary.toolchains?.[result.id] !== result.toolchain) {
        throw Error(`pass toolchain/steps mismatch ${result.id}`);
      }
    }
    if (result.receipt) {
      if (result.receipt !== `docs/evidence/_raw_reverify_${result.id}.log` ||
          !SHA.test(result.receipt_sha256 || '') || !existsSync(receipt) ||
          hash(Buffer.from(lf(readFileSync(receipt, 'utf8')), 'utf8')) !== result.receipt_sha256) {
        throw Error(`receipt/hash mismatch ${result.id}`);
      }
    } else if (result.steps?.length || result.verdict === 'pass') {
      throw Error(`attempt lacks receipt ${result.id}`);
    }
  }

  if (map.size !== chosen.length || chosen.some((id) => !map.has(id))) {
    throw Error('summary ID set mismatch');
  }
  const passed = summary.rows.filter((row) => row.verdict === 'pass');
  const proposals = csv.slice(1);
  if (proposals.length !== passed.length) throw Error('proposal/pass count mismatch');
  const ids = proposals.map((row) => row[0]);
  if (new Set(ids).size !== ids.length ||
      JSON.stringify([...ids].sort()) !== JSON.stringify(passed.map((row) => row.id).sort())) {
    throw Error('proposal IDs mismatch');
  }
  for (const cells of proposals) {
    const result = map.get(cells[0]);
    const registryRow = registry.get(cells[0]);
    if (cells[6] !== result.profile || cells[5] !== result.toolchain ||
        cells[13] !== (result.mutant_rejected ? 'true' : registryRow.mutant_rejected) ||
        cells[7] !== ident.head || cells[8] !== ident.tree ||
        cells[11] !== registryRow.positive_marker || cells[17] !== registryRow.status ||
        cells[14] !== result.receipt || cells[15] !== result.receipt_sha256) {
      throw Error(`proposal pins mismatch ${cells[0]}`);
    }
  }
  const failed = summary.rows.some((row) => ['fail', 'INVALID'].includes(row.verdict));
  const complete = summary.rows.every((row) => row.verdict === 'pass');
  const verdict = failed ? 'E0 FAIL' : complete ? 'E0 PASS' : 'E0 INCOMPLETE';
  if (readFileSync(join(out, 'verdict.txt'), 'utf8').split('\n')[0] !== verdict) {
    throw Error('verdict mismatch');
  }
  return summary;
}
