// Gate-config validator for the r20-gates workflow (.github/workflows/r20-gates.yml).
//
// One CLI, three modes:
//   node scripts/r20-gate-config.mjs [.github/gate-run.json]   PR mode: read+validate the
//                                                              committed config file and print
//                                                              KEY=value lines for GITHUB_ENV.
//   node scripts/r20-gate-config.mjs --from-env                Dispatch mode: validate the
//                                                              RUNNER_INPUT/STAGES_INPUT/
//                                                              SLOP_INPUT/NOTE_INPUT env vars
//                                                              (the same code path) and print
//                                                              the same KEY=value lines,
//                                                              overwriting the raw job-env
//                                                              values with validated ones.
//   node scripts/r20-gate-config.mjs --selftest                In-memory checks (no repo
//                                                              writes); runs in CI.
//
// Output contract: stdout carries ONLY `KEY=value` lines (single-line values, no \r or \n —
// rejected up front), so the workflow captures the command output and appends it to GITHUB_ENV.
// On any schema violation the script prints a message to stderr and exits 1.
//
// Duplicates in `stages` are an error (kept deterministic; no silent dedup).
// `outDir` is incompatible with rehearsal (scripts/r20_prf01_sidecar_summary_run.mjs
// forbids `--out-dir` together with `--rehearsal`); rehearsal:true prepends the
// `rehearsal` stage to the stage list when absent.
//
// r6 A/B iai fields (benchTarget/features/cells/baseSha/thresholds) are
// optional and consumed by scripts/r20_r6_ab_iai_run.mjs; validated with the
// same single-line allowlists the runner itself re-checks.

import fs from 'node:fs';

const RUNNER_RE = /^scripts\/r20_[a-z0-9_]+_run\.mjs$/;
const OUT_DIR_RE = /^(docs\/perf\/R20_[A-Z0-9_]+|target\/r20[a-z0-9_-]*)$/;
const ALLOWED_STAGES = ['prepare', 'build', 'oracle', 'measure', 'rehearsal', 'cleanup'];
const ALLOWED_KEYS = ['runner', 'stages', 'rehearsal', 'slop', 'outDir', 'note',
  'benchTarget', 'features', 'cells', 'baseSha', 'thresholds'];
const BENCH_TARGET_RE = /^[a-z][a-z0-9_]{0,63}$/;
const FEATURES_RE = /^[a-z0-9][a-z0-9 _-]{0,199}$/;
const CELL_RE = /^[A-Za-z0-9_]+$/;
const SHA_RE = /^[0-9a-f]{40}$/;
const CONTROL_RE = /[\x00-\x1f\x7f]/;

function fail(msg) {
  console.error(process.argv.includes('--selftest') ? msg : `::error::${msg}`);
  return null;
}

// Pure validator: returns { runner, stages, slop, rehearsal, outDir, note } or null.
// `fileExists(path)` is injected so --selftest can run without touching the repo.
function validateConfig(cfg, fileExists) {
  if (typeof cfg !== 'object' || cfg === null || Array.isArray(cfg)) {
    return fail('gate config must be a JSON object');
  }
  for (const key of Object.keys(cfg)) {
    if (!ALLOWED_KEYS.includes(key)) {
      return fail(`unknown field '${key}' (expected: ${ALLOWED_KEYS.join(', ')})`);
    }
  }
  const str = (v) => typeof v === 'string' && !/[\r\n]/.test(v);
  const runner = cfg.runner;
  if (!str(runner) || runner.length === 0) {
    return fail("field 'runner' must be a single-line string");
  }
  if (!RUNNER_RE.test(runner)) {
    return fail(`runner must match ${RUNNER_RE.source}, got: '${runner}'`);
  }
  if (!fileExists(runner)) {
    return fail(`runner file not found in this checkout: ${runner}`);
  }
  const rawStages = cfg.stages;
  if (!str(rawStages) || rawStages.trim().length === 0) {
    return fail("field 'stages' must be a non-empty single-line comma-separated string");
  }
  const stageList = [];
  for (const item of rawStages.split(',')) {
    const s = item.trim();
    if (s.length === 0) continue;
    if (!ALLOWED_STAGES.includes(s)) {
      return fail(`invalid stage '${s}' (allowed: ${ALLOWED_STAGES.join(', ')})`);
    }
    if (stageList.includes(s)) {
      return fail(`duplicate stage '${s}' in 'stages'`);
    }
    stageList.push(s);
  }
  if (stageList.length === 0) {
    return fail("field 'stages' contains no valid stage names");
  }
  for (const key of ['rehearsal', 'slop']) {
    if (cfg[key] !== undefined && typeof cfg[key] !== 'boolean') {
      return fail(`field '${key}' must be a boolean`);
    }
  }
  const rehearsal = cfg.rehearsal === true;
  const slop = cfg.slop === true;
  let outDir;
  if (cfg.outDir !== undefined) {
    if (!str(cfg.outDir)) return fail("field 'outDir' must be a single-line string");
    if (!OUT_DIR_RE.test(cfg.outDir)) {
      return fail(`outDir must match ${OUT_DIR_RE.source}, got: '${cfg.outDir}'`);
    }
    outDir = cfg.outDir;
  }
  let note = '';
  if (cfg.note !== undefined) {
    if (!str(cfg.note)) return fail("field 'note' must be a single-line string");
    if (cfg.note.length > 200) return fail('note must be at most 200 characters');
    if (CONTROL_RE.test(cfg.note)) return fail('note must not contain control characters');
    note = cfg.note;
  }
  // r6 A/B iai runner fields (scripts/r20_r6_ab_iai_run.mjs). All optional;
  // the runner re-validates them locally, but the committed config is checked
  // here so a typo fails the cheap CI check, not a gate run.
  let benchTarget = 'perf_gate_iai';
  if (cfg.benchTarget !== undefined) {
    if (typeof cfg.benchTarget !== 'string' || !BENCH_TARGET_RE.test(cfg.benchTarget)) {
      return fail(`benchTarget must match ${BENCH_TARGET_RE.source}, got: '${cfg.benchTarget}'`);
    }
    benchTarget = cfg.benchTarget;
  }
  let features = 'production bench-internals internals';
  if (cfg.features !== undefined) {
    if (typeof cfg.features !== 'string' || !FEATURES_RE.test(cfg.features)) {
      return fail(`features must match ${FEATURES_RE.source}, got: '${cfg.features}'`);
    }
    features = cfg.features;
  }
  let cells;
  if (cfg.cells !== undefined) {
    if (typeof cfg.cells !== 'string') return fail("field 'cells' must be a single-line comma-separated string");
    const cellList = [];
    for (const item of cfg.cells.split(',')) {
      const c = item.trim();
      if (c.length === 0) continue;
      if (!CELL_RE.test(c)) return fail(`invalid cell name '${c}' (must match ${CELL_RE.source})`);
      if (cellList.includes(c)) return fail(`duplicate cell '${c}' in 'cells'`);
      cellList.push(c);
    }
    if (cellList.length === 0) return fail("field 'cells' contains no cell names");
    cells = cellList.join(',');
  }
  let baseSha;
  if (cfg.baseSha !== undefined) {
    if (typeof cfg.baseSha !== 'string' || !SHA_RE.test(cfg.baseSha)) {
      return fail(`baseSha must match ${SHA_RE.source}, got: '${cfg.baseSha}'`);
    }
    baseSha = cfg.baseSha;
  }
  let thresholds;
  if (cfg.thresholds !== undefined) {
    const t = cfg.thresholds;
    if (typeof t !== 'object' || t === null || Array.isArray(t)) {
      return fail("field 'thresholds' must be an object mapping cell -> max tolerated delta_pct");
    }
    const keys = Object.keys(t);
    if (keys.length === 0) return fail("field 'thresholds' must contain at least one cell");
    for (const k of keys) {
      if (!CELL_RE.test(k)) return fail(`invalid thresholds key '${k}' (must match ${CELL_RE.source})`);
      if (typeof t[k] !== 'number' || !Number.isFinite(t[k]) || Math.abs(t[k]) > 100000) {
        return fail(`thresholds['${k}'] must be a finite number with |value| <= 100000`);
      }
    }
    thresholds = t;
  }
  if (rehearsal && outDir !== undefined) {
    return fail('outDir is not allowed with rehearsal:true (--out-dir is rejected by the runner for rehearsal)');
  }
  if (stageList.includes('rehearsal') && outDir !== undefined) {
    return fail("stages must not contain 'rehearsal' when outDir is set (--out-dir is rejected by the runner for rehearsal)");
  }
  if (rehearsal && !stageList.includes('rehearsal')) {
    stageList.unshift('rehearsal');
  }
  return { runner, stages: stageList.join(','), slop, rehearsal, outDir, note,
    benchTarget, features, cells, baseSha, thresholds };
}

function printEnv(res) {
  const lines = [
    `RUNNER_INPUT=${res.runner}`,
    `STAGES_INPUT=${res.stages}`,
    `SLOP_INPUT=${res.slop}`,
    `NOTE_INPUT=${res.note}`,
  ];
  if (res.outDir !== undefined) lines.push(`OUT_DIR_INPUT=${res.outDir}`);
  lines.push(`BENCH_TARGET_INPUT=${res.benchTarget}`);
  lines.push(`FEATURES_INPUT=${res.features}`);
  if (res.cells !== undefined) lines.push(`CELLS_INPUT=${res.cells}`);
  if (res.baseSha !== undefined) lines.push(`BASE_SHA_INPUT=${res.baseSha}`);
  if (res.thresholds !== undefined) lines.push(`THRESHOLDS_INPUT=${JSON.stringify(res.thresholds)}`);
  for (const line of lines) process.stdout.write(`${line}\n`);
}

function readConfigFile(path) {
  let text;
  try {
    text = fs.readFileSync(path, 'utf8');
  } catch (e) {
    return fail(`cannot read ${path}: ${e.message}`);
  }
  try {
    return JSON.parse(text);
  } catch (e) {
    return fail(`${path}: invalid JSON: ${e.message}`);
  }
}

function envBool(v) {
  if (v === '') return undefined;
  if (v === 'true') return true;
  if (v === 'false') return false;
  return v;
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes('--selftest')) return selftest();
  let cfg;
  if (args.includes('--from-env')) {
    cfg = {
      runner: process.env.RUNNER_INPUT,
      stages: process.env.STAGES_INPUT,
      // only the exact dispatch booleans; anything else reaches validateConfig as a non-boolean
      slop: envBool(process.env.SLOP_INPUT ?? ''),
      rehearsal: false,
      note: process.env.NOTE_INPUT,
    };
  } else {
    const path = args[0] === undefined ? '.github/gate-run.json' : args[0];
    cfg = readConfigFile(path);
    if (cfg === null) process.exit(1);
  }
  const res = validateConfig(cfg, (p) => fs.existsSync(p) && fs.statSync(p).isFile());
  if (res === null) process.exit(1);
  printEnv(res);
}

function selftest() {
  const yes = () => true;
  const no = () => false;
  let checks = 0;
  const ok = (cond, name) => {
    checks += 1;
    if (!cond) {
      console.error(`selftest FAIL: ${name}`);
      process.exit(1);
    }
  };
  const good = {
    runner: 'scripts/r20_prf01_sidecar_summary_run.mjs',
    stages: 'prepare,build,oracle,measure',
  };
  ok(validateConfig(good, yes) !== null, 'valid config passes');
  ok(validateConfig({ ...good, note: 'a\nb' }, yes) === null, 'note newline rejected');
  ok(validateConfig({ ...good, stages: 'prepare\nbuild' }, yes) === null, 'stages newline rejected');
  ok(validateConfig({ ...good, extra: 1 }, yes) === null, 'unknown field rejected');
  ok(validateConfig({ ...good, runner: 'scripts/other.mjs' }, yes) === null, 'runner regex enforced');
  ok(validateConfig({ ...good, runner: 'scripts/../evil.mjs' }, yes) === null, 'path traversal rejected');
  ok(validateConfig(good, no) === null, 'missing runner file rejected');
  ok(validateConfig({ ...good, outDir: 'docs/perf/lower_case' }, yes) === null, 'outDir pattern enforced');
  ok(validateConfig({ ...good, outDir: 'target/../x' }, yes) === null, 'outDir traversal rejected');
  ok(validateConfig({ ...good, outDir: '' }, yes) === null, 'empty outDir rejected');
  ok(validateConfig({ ...good, slop: 'yes' }, yes) === null, 'non-bool slop rejected');
  ok(validateConfig({ ...good, rehearsal: 1 }, yes) === null, 'non-bool rehearsal rejected');
  ok(validateConfig({ ...good, rehearsal: true, outDir: 'target/r20x' }, yes) === null, 'rehearsal+outDir rejected');
  ok(validateConfig({ ...good, stages: 'rehearsal', outDir: 'target/r20x' }, yes) === null, 'stages rehearsal+outDir rejected');
  ok(validateConfig(null, yes) === null, 'null config rejected');
  let r = validateConfig({ ...good, rehearsal: true }, yes);
  ok(r !== null && r.stages.startsWith('rehearsal,'), 'rehearsal stage prepended');
  ok(validateConfig({ ...good, stages: 'prepare,prepare' }, yes) === null, 'duplicate stage rejected');
  ok(validateConfig({ ...good, note: 'x'.repeat(201) }, yes) === null, 'note > 200 rejected');
  ok(validateConfig({ ...good, note: 'a\x01b' }, yes) === null, 'control char in note rejected');
  r = validateConfig({ ...good, outDir: 'target/r20g0', note: 'ok=still-safe' }, yes);
  ok(r !== null && r.outDir === 'target/r20g0' && r.note === 'ok=still-safe', 'valid outDir+note pass');

  // r6 A/B iai fields.
  ok(validateConfig({ ...good, benchTarget: 'perf_gate_iai', features: 'production bench-internals internals' }, yes) !== null,
    'valid r6 ab fields pass');
  ok(validateConfig({ ...good, benchTarget: 'Perf Gate' }, yes) === null, 'benchTarget charset enforced');
  ok(validateConfig({ ...good, features: 'a,b' }, yes) === null, 'features comma rejected');
  r = validateConfig({ ...good, cells: 'small_churn_16b, cold_alloc_only_256x16b' }, yes);
  ok(r !== null && r.cells === 'small_churn_16b,cold_alloc_only_256x16b', 'cells trimmed and normalised');
  ok(validateConfig({ ...good, cells: 'a,a' }, yes) === null, 'duplicate cell rejected');
  ok(validateConfig({ ...good, baseSha: 'HEAD' }, yes) === null, 'baseSha must be a full SHA');
  ok(validateConfig({ ...good, thresholds: {} }, yes) === null, 'empty thresholds rejected');
  ok(validateConfig({ ...good, thresholds: { small_churn_16b: 1.0 } }, yes) !== null, 'valid thresholds pass');
  ok(validateConfig({ ...good, thresholds: { small_churn_16b: '5%' } }, yes) === null, 'non-numeric threshold rejected');
  console.log(JSON.stringify({ status: 'PASS', checks }));
}

main();
