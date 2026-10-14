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

import fs from 'node:fs';

const RUNNER_RE = /^scripts\/r20_[a-z0-9_]+_run\.mjs$/;
const OUT_DIR_RE = /^(docs\/perf\/R20_[A-Z0-9_]+|target\/r20[a-z0-9_-]*)$/;
const ALLOWED_STAGES = ['prepare', 'build', 'oracle', 'measure', 'rehearsal', 'cleanup'];
const ALLOWED_KEYS = ['runner', 'stages', 'rehearsal', 'slop', 'outDir', 'note'];
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
  if (rehearsal && outDir !== undefined) {
    return fail('outDir is not allowed with rehearsal:true (--out-dir is rejected by the runner for rehearsal)');
  }
  if (stageList.includes('rehearsal') && outDir !== undefined) {
    return fail("stages must not contain 'rehearsal' when outDir is set (--out-dir is rejected by the runner for rehearsal)");
  }
  if (rehearsal && !stageList.includes('rehearsal')) {
    stageList.unshift('rehearsal');
  }
  return { runner, stages: stageList.join(','), slop, rehearsal, outDir, note };
}

function printEnv(res) {
  const lines = [
    `RUNNER_INPUT=${res.runner}`,
    `STAGES_INPUT=${res.stages}`,
    `SLOP_INPUT=${res.slop}`,
    `NOTE_INPUT=${res.note}`,
  ];
  if (res.outDir !== undefined) lines.push(`OUT_DIR_INPUT=${res.outDir}`);
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
  console.log(JSON.stringify({ status: 'PASS', checks }));
}

main();
