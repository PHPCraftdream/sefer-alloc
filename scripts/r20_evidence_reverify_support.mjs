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
  const expectationKeys = Object.keys(step.expect);
  if (!('exit' in step.expect) || expectationKeys.length !== 2 ||
      expectationKeys.find((key) => key !== 'exit') !== 'stdoutRegex') {
    throw Error(`${where} expect keys`);
  }
  if (!Number.isInteger(step.expect.exit)) throw Error(`${where} exit`);
  const patterns = step.expect.stdoutRegex || step.expect.stderrRegex;
  if (!Array.isArray(patterns) || !patterns.length) throw Error(`${where} patterns`);
  for (const pattern of patterns) {
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
    if (typeof mutant.file !== 'string' ||
        !/^src\/(?:[A-Za-z0-9_.-]+\/)*[A-Za-z0-9_.-]+\.rs$/.test(mutant.file) ||
        mutant.file.split('/').includes('..')) {
      throw Error('unsafe mutant path');
    }
    if (typeof mutant.find !== 'string' || !mutant.find ||
        typeof mutant.replace !== 'string' || !mutant.replace || mutant.find === mutant.replace) {
      throw Error('bad mutant text');
    }
    if (!Array.isArray(mutant.steps) || !mutant.steps.length) throw Error('mutant steps');
    mutant.steps.forEach((step, index) => validateStep(step, `${id}.mutant.${index}`));
    strict(mutant.expect, ['exitNonZero', 'stdoutRegex'], `${id}.mutant.expect`);
    if (mutant.expect.exitNonZero !== true || !Array.isArray(mutant.expect.stdoutRegex) ||
        !mutant.expect.stdoutRegex.length) {
      throw Error('mutant aggregate expectation');
    }
    for (const pattern of mutant.expect.stdoutRegex) {
      if (typeof pattern !== 'string' || !pattern) throw Error('mutant regex');
      new RegExp(pattern);
    }
  }
  return data;
}

export function verifyPerformed(actual, expected, where, passed, mutant = false) {
  if (!Array.isArray(actual) || actual.length > expected.length) {
    throw Error(`${where} step count mismatch`);
  }
  for (let index = 0; index < actual.length; index += 1) {
    const result = actual[index];
    const step = expected[index];
    if (!result || result.name !== step.name ||
        JSON.stringify(result.argv) !== JSON.stringify(step.argv) ||
        JSON.stringify(result.env) !== JSON.stringify(step.env) ||
        JSON.stringify(result.expect) !== JSON.stringify(step.expect)) {
      throw Error(`${where} step mismatch ${index}`);
    }
    if (!Number.isInteger(result.expected) || result.expected !== step.expect.exit ||
        result.exit === undefined || !Array.isArray(result.matches) ||
        result.matches.length !== step.expect.stdoutRegex.length ||
        result.matches.some((marker, markerIndex) =>
          marker.regex !== step.expect.stdoutRegex[markerIndex] ||
          typeof marker.matched !== 'boolean' || !Array.isArray(marker.actual))) {
      throw Error(`${where} step result malformed ${index}`);
    }
    if (result.passed !== (result.exit === step.expect.exit &&
        result.matches.every((marker) => marker.matched) && !result.signal && !result.error)) {
      throw Error(`${where} step verdict inconsistent ${index}`);
    }
    const exitMatches = mutant
      ? result.expected !== 0 && result.exit !== 0 && result.exit === result.expected
      : result.expected === step.expect.exit;
    if (passed && (!result.passed || result.exit !== step.expect.exit || !exitMatches ||
        result.matches.some((marker) => !marker.matched))) {
      throw Error(`${where} step did not pass ${index}`);
    }
  }
  if (passed && actual.length !== expected.length) throw Error(`${where} missing steps`);
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
      verifyPerformed(result.steps, data.steps, `${result.id} baseline`, result.verdict === 'pass');
      if (data.mutant) {
        verifyPerformed(result.mutant_steps, data.mutant.steps,
          `${result.id} mutant`, result.verdict === 'pass', true);
        verifyPerformed(result.restored_steps, data.steps,
          `${result.id} restored`, result.verdict === 'pass');
        if (result.verdict === 'pass' && (!result.mutant_rejected ||
            result.mutant_original_sha256 !== result.mutant_restored_sha256 ||
            !SHA.test(result.mutant_original_sha256 || '') || !result.mutant_matches?.length ||
            result.mutant_matches.some((match) => !match.matched) ||
            result.mutant_steps.some((step) => step.exit === 0))) {
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
        if (isR0) {
          verifyPerformed(result.steps, data.steps, 'R0-21 embedded mutant witnesses',
            result.verdict === 'pass');
        }
      }
    }
    if (result.verdict === 'pass') {
      if (!result.recipe || !SHA.test(result.recipe_sha256 || '') ||
          typeof result.toolchain !== 'string' || !result.toolchain || !result.steps.length ||
          !result.steps[0].passed || summary.toolchains?.[result.id] !== result.toolchain) {
        throw Error(`pass toolchain/steps mismatch ${result.id}`);
      }
    }
    const receipt = join(out, `_raw_reverify_${result.id}.log`);
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
