// R16 item83 frozen local snapshots. Preparation/compile/activation do not measure.
// Only --run A1|A2|C0|B1|B2 authorizes iai; CLI filters retain original iai semantics.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, lstatSync, readdirSync } from 'node:fs';
import { dirname, join, resolve, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawn } from 'node:child_process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const STORE = join(ROOT, 'target', 'r16-perf83');
const BASE = '549d07b648ad48ee8e9d929bfec40157626a80b8';
const FEATURES = 'production bench-internals internals';
const OWNED = 'src/alloc_core/alloc_core/mem/realloc_fastpath.rs';
const PROBE = 'examples/r16_perf83_large_shrink_rss.rs';
const SCENARIOS = ['realloc_large_shrink_8_to_6mib', 'realloc_large_shrink_8_to_4p5mib',
  'realloc_large_shrink_8_to_3mib', 'realloc_large_grow_6_to_8mib', 'realloc_large_equal_8mib'];
const BENCHES = [...SCENARIOS.flatMap(n => [n, `${n}_prefix`]), 'large_alloc_free_cycle',
  'large_cache_prefill_only_4mib', 'large_cache_hit_only_4mib',
  'large_cache_free_slot_search_prefill_only', 'large_cache_free_slot_search_cycle_only',
  'realloc_grow', 'dealloc_realloc_burst_1088_16b_n17'];
const REQUIRED = ['Cargo.toml', 'Cargo.lock', 'benches/perf_gate_iai.rs',
  'tests/r16_perf83_activation.rs', PROBE, 'scripts/iai.mjs', 'scripts/lib.mjs',
  'scripts/r16_perf83_iai.mjs'];
const RSS_SCENARIOS = [...SCENARIOS, 'cycles8to6'];
const RSS_KEYS = ['arm', 'scenario', 'sample', 'rss_bytes', 'baseline_rss_bytes', 'peak_bytes',
  'final_free_rss_bytes', 'teardown_rss_bytes', 'pid', 'statistic', 'source_input_sha256', 'binary_sha256'].sort();
const CONFIG_KEYS = ['requested', 'decay_rate_bp', 'decay_interval_ms', 'headroom_bytes',
  'large_cache_budget_bytes', 'large_cache_total_slots', 'pool_cap', 'config_conflicts_delta'].sort();
const RSS_EXAMPLE = 'r16_perf83_large_shrink_rss';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const quote = text => `'${text.replaceAll("'", "'\\''")}'`;
const git = (...args) => {
  assert(['status', 'diff', 'log', 'show', 'rev-parse'].includes(args[0]), 'git read allowlist');
  return execFileSync('git', args, { cwd: ROOT });
};
const linuxPath = path => {
  if (process.platform !== 'win32') return path;
  const m = /^([A-Za-z]):[\\/](.*)$/.exec(path);
  assert(m, 'drive-qualified path required');
  return `/mnt/${m[1].toLowerCase()}/${m[2].replaceAll('\\', '/')}`;
};
function sanitize(text) {
  for (const path of [ROOT, ROOT.replaceAll('\\', '/'), linuxPath(ROOT)].sort((a,b) => b.length-a.length)) {
    text = text.replaceAll(path, '<repo>');
  }
  // Arbitrary diagnostic/toolchain paths (not just the checkout prefix).
  return text.replace(/[A-Za-z]:[\\/][^\s"'<>|)]+/g, '<target>')
    .replace(/(^|[\s"'(=:,\[{])\/(?!\/)[^\s"'<>|)]+/gm, '$1<target>');
}
function safeFile(root, path) {
  assert(!isAbsolute(path) && !path.split(/[\\/]/).includes('..'), 'relative input required');
  const file = resolve(root, path);
  assert(file.startsWith(`${resolve(root)}/`) || file.startsWith(`${resolve(root)}\\`), 'input escape');
  let cursor = root;
  assert(!lstatSync(cursor).isSymbolicLink(), 'root symlink rejected');
  for (const part of path.split('/')) {
    cursor = join(cursor, part);
    assert(!lstatSync(cursor).isSymbolicLink(), `symlink rejected: ${path}`);
  }
  assert(lstatSync(file).isFile(), `regular input required: ${path}`);
  return file;
}
function inputs(root) {
  const excluded = new Set(['target', '.git', 'docs', '.github', 'node_modules', '.cache', '.idea', '.vscode']);
  const paths = [];
  function walk(path) {
    const dir = join(root, path);
    assert(!lstatSync(dir).isSymbolicLink(), `symlink directory rejected: ${path}`);
    for (const entry of readdirSync(dir).sort()) {
      if (excluded.has(entry)) continue;
      const p = path ? `${path}/${entry}` : entry;
      const stat = lstatSync(join(root, p));
      assert(!stat.isSymbolicLink(), `symlink input rejected: ${p}`);
      if (stat.isDirectory()) walk(p);
      else if (stat.isFile()) paths.push(p);
      else throw new Error(`nonregular input: ${p}`);
    }
  }
  // Include all local source/assets in build roots, including untracked files.
  for (const dir of ['src', 'crates', 'benches', 'tests', 'examples', 'scripts', '.cargo']) {
    if (existsSync(join(root, dir))) walk(dir);
  }
  for (const name of readdirSync(root).sort()) {
    if (/^(Cargo\.(toml|lock)|build\.rs|.*\.(toml|json)|\.gitattributes)$/.test(name)) {
      safeFile(root, name);
      paths.push(name);
    }
  }
  return paths.sort();
}
function generatedHarness(original) {
  function replace(old, replacement) {
    assert.equal(original.split(old).length, 2, `single harness anchor required: ${old}`);
    original = original.replace(old, replacement);
  }
  replace("const LINUX_TARGET = '/tmp/sefer-iai';", 'const LINUX_TARGET = \'"$PWD/target"\';');
  replace("      'cargo bench',", "      'CARGO_BUILD_JOBS=3',\n      'cargo bench --locked',");
  replace('`cd ${wslRoot}`', '`cd \'${wslRoot.replaceAll("\'", "\'\\\\\'\'")}\'`');
  const start = original.indexOf('  // --locked: use the crate');
  const end = original.indexOf('\n}\n', start);
  assert(start > 0 && end > start, 'runner install block anchors');
  original = original.slice(0, start) +
    "  throw new Error('R16 item83: matching preinstalled runner required; installs prohibited');" + original.slice(end);
  assert(!original.includes('cargo install'), 'no dependency installs');
  return original;
}
function manifest(root, paths) {
  return paths.map(p => [p, hash(readFileSync(safeFile(root, p)))]);
}
function fresh(file, bytes) {
  const local = resolve(file);
  assert(local.startsWith(`${ROOT}/`) || local.startsWith(`${ROOT}\\`), 'write outside repo rejected');
  let cursor = dirname(local);
  while (cursor !== ROOT) {
    if (existsSync(cursor)) assert(!lstatSync(cursor).isSymbolicLink(), 'symlink output ancestor rejected');
    const parent = dirname(cursor);
    assert(parent !== cursor, 'output ancestor escape');
    cursor = parent;
  }
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, bytes, { flag: 'wx' });
}
const jsonBytes = value => JSON.stringify(value, null, 2) + '\n';
function writeIdentity(label, identity) {
  const bytes = jsonBytes(identity);
  fresh(join(STORE, `${label}.identity.json`), bytes);
  fresh(join(STORE, `${label}.identity.sha256`), hash(bytes) + '\n');
}
function identity(label) {
  const bytes = readFileSync(join(STORE, `${label}.identity.json`));
  assert.equal(hash(bytes), readFileSync(join(STORE, `${label}.identity.sha256`), 'utf8').trim(), 'identity integrity');
  return JSON.parse(bytes);
}
function verify(label) {
  assert(['A', 'C0', 'B'].includes(label), 'invalid arm');
  assert(existsSync(join(STORE, label)), `missing snapshot ${label}; --prepare awaits mandatory RSS probe`);
  const id = identity(label), dir = join(STORE, label);
  assert.equal(id.base, BASE);
  assert.equal(id.tree, git('rev-parse', `${BASE}^{tree}`).toString().trim());
  assert.equal(id.features, FEATURES);
  assert.deepEqual(inputs(dir), id.inputs.map(([p]) => p), `${label}: input set changed`);
  assert.deepEqual(manifest(dir, id.inputs.map(([p]) => p)), id.inputs, `${label}: frozen source changed`);
  assert.equal(hash(JSON.stringify(id.inputs)), id.input_sha256);
  for (const p of REQUIRED) assert(id.inputs.some(([path]) => path === p), `missing input ${p}`);
  assert.equal(hash(readFileSync(join(STORE, 'bench.patch'))), id.bench_patch_sha256);
  assert.equal(hash(readFileSync(join(STORE, label === 'B' ? 'candidate.patch' : 'baseline.patch'))), id.src_patch_sha256);
  if (label === 'C0') assert.deepEqual(id.inputs, identity('A').inputs, 'C0 must be byte-identical to A');
  if (label === 'B') {
    const a = identity('A');
    assert.deepEqual(id.inputs.map(([p]) => p), a.inputs.map(([p]) => p));
    const changed = id.inputs.filter(([p,h], i) => h !== a.inputs[i][1]).map(([p]) => p);
    assert.deepEqual(changed, [OWNED], 'only owned candidate source may differ');
  }
  return { dir, id };
}
function prepare() {
  assert(existsSync(join(ROOT, PROBE)), `DEFERRED: mandatory RSS probe missing: ${PROBE}; no snapshots created`);
  assert.equal(git('rev-parse', 'HEAD').toString().trim(), BASE, 'unexpected HEAD');
  assert(!existsSync(STORE), 'store already exists: verify/reuse; never overwrite');
  const paths = inputs(ROOT);
  for (const p of REQUIRED) assert(paths.includes(p), `missing required input: ${p}`);
  const data = new Map(paths.map(p => [p, p.startsWith('src/') ? git('show', `${BASE}:${p}`) : readFileSync(safeFile(ROOT, p))]));
  data.set('scripts/iai.mjs', Buffer.from(generatedHarness(data.get('scripts/iai.mjs').toString())));
  const benchPatch = git('diff', '--binary', BASE, '--', 'benches/perf_gate_iai.rs');
  // A is pinned HEAD src even when candidate bytes are already in the working tree.
  const baselinePatch = Buffer.alloc(0);
  fresh(join(STORE, 'bench.patch'), benchPatch);
  fresh(join(STORE, 'baseline.patch'), baselinePatch);
  for (const label of ['A', 'C0']) {
    const dir = join(STORE, label);
    for (const [p, bytes] of data) fresh(join(dir, p), bytes);
    const entries = manifest(dir, paths);
    writeIdentity(label, { base: BASE, tree: git('rev-parse', `${BASE}^{tree}`).toString().trim(),
      features: FEATURES, inputs: entries, input_sha256: hash(JSON.stringify(entries)),
      src_patch_command: 'git diff --ignore-cr-at-eol -- src', src_patch_sha256: hash(baselinePatch),
      bench_patch_sha256: hash(benchPatch), harness_sha256: hash(data.get('scripts/iai.mjs')) });
    verify(label);
  }
  assert.deepEqual(identity('A'), identity('C0'));
  console.log('Frozen identical A/C0, pinned HEAD src; no build or measurement.');
}
function prepareB() {
  assert.equal(git('rev-parse', 'HEAD').toString().trim(), BASE);
  const a = verify('A'); verify('C0');
  assert(!existsSync(join(STORE, 'B')), 'B snapshot exists; never overwrite');
  const changed = git('diff', '--name-only', '--', 'src').toString().trim().split(/\r?\n/).filter(Boolean);
  assert.deepEqual(changed, [OWNED]);
  const patch = git('diff', '--ignore-cr-at-eol', '--', 'src');
  assert(patch.length > 0, 'candidate patch missing');
  assert.deepEqual(inputs(ROOT), a.id.inputs.map(([p]) => p), 'common input set changed since freeze');
  const dir = join(STORE, 'B');
  for (const [p,h] of a.id.inputs) {
    let bytes = readFileSync(safeFile(ROOT, p));
    if (p === 'scripts/iai.mjs') bytes = Buffer.from(generatedHarness(bytes.toString()));
    if (p !== OWNED) assert.equal(hash(bytes), h, `common input changed: ${p}`);
    fresh(join(dir, p), bytes);
  }
  fresh(join(STORE, 'candidate.patch'), patch);
  const entries = manifest(dir, a.id.inputs.map(([p]) => p));
  writeIdentity('B', { ...a.id, inputs: entries, input_sha256: hash(JSON.stringify(entries)), src_patch_sha256: hash(patch) });
  verify('B');
  console.log('Frozen B; only realloc_fastpath.rs differs.');
}
async function execute(command, args, cwd, stem, long = false) {
  for (const suffix of ['start.json', 'log', 'result.json']) {
    assert(!existsSync(join(STORE, `${stem}.${suffix}`)), `existing receipt: ${stem}.${suffix}`);
  }
  const start = { command: sanitize(command), args: args.map(sanitize), cwd: '<repo>',
    started: new Date().toISOString(), long_build_or_iai: long };
  fresh(join(STORE, `${stem}.start.json`), jsonBytes(start));
  let output = '', timedOut = false;
  const child = spawn(command, args, { cwd, shell: false, detached: process.platform !== 'win32' });
  const timer = long ? null : setTimeout(() => {
    timedOut = true;
    if (process.platform === 'win32') execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F']);
    else process.kill(-child.pid, 'SIGKILL');
  }, 600_000);
  for (const stream of [child.stdout, child.stderr]) stream.on('data', b => { output += b.toString(); });
  let spawnError;
  const code = await new Promise(resolve => {
    child.on('error', error => { spawnError = sanitize(error.message); });
    child.on('close', resolve);
  }).finally(() => { if (timer) clearTimeout(timer); });
  const log = sanitize(output);
  fresh(join(STORE, `${stem}.log`), log);
  const result = { ...start, ended: new Date().toISOString(), code, timedOut,
    spawn_error: spawnError ?? null, sanitized_log_sha256: hash(log), log_bytes: Buffer.byteLength(log) };
  fresh(join(STORE, `${stem}.result.json`), jsonBytes(result));
  // Full logs remain ignored scratch; a later capture must create <200KiB cited excerpts.
  assert(!spawnError && !timedOut && code === 0, `command failed: ${stem}; receipt saved`);
  return { output, result };
}
async function linux(dir, command, stem, long = false) {
  const args = ['bash', '-lc', `cd ${quote(linuxPath(dir))} && ${command}`];
  return execute(process.platform === 'win32' ? 'wsl.exe' : 'bash',
    process.platform === 'win32' ? args : args.slice(1), dir, stem, long);
}
async function versions(dir, stem) {
  // No `$` expansion or nested quotes: wsl.exe hands the line to an outer shell
  // first, which would expand/mangle them before bash sees the command.
  const command = 'set -e; rustc -Vv; cargo -V; valgrind --version; uname -srm; ' +
    'command -v iai-callgrind-runner >/dev/null; ' +
    'iai-callgrind-runner --version 2>&1 | head -n 1; ' +
    'iai-callgrind-runner --version 2>&1 | grep -q 0.14.2';
  const { output } = await linux(dir, command, stem);
  return sanitize(output);
}
function envelope(target = 'target') {
  return `RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/${target}"`;
}
function artifact(output, name, dir) {
  const artifacts = output.split(/\r?\n/).filter(s => s.startsWith('{')).map(s => JSON.parse(s))
    .filter(x => x.reason === 'compiler-artifact' && x.target.name === name && x.executable);
  assert.equal(artifacts.length, 1, `one executable artifact required: ${name}`);
  const exe = artifacts[0].executable, prefix = `${linuxPath(dir)}/`;
  assert(exe.startsWith(prefix), 'artifact outside snapshot');
  const p = exe.slice(prefix.length);
  assert(p.startsWith('target/'), 'binary must be in local target');
  return { path: p, sha256: hash(readFileSync(safeFile(dir, p))) };
}
function readReceipt(stem) {
  const bytes = readFileSync(join(STORE, `${stem}.json`));
  assert.equal(hash(bytes), readFileSync(join(STORE, `${stem}.sha256`), 'utf8').trim());
  return JSON.parse(bytes);
}
function saveReceipt(stem, value) {
  const bytes = jsonBytes(value);
  fresh(join(STORE, `${stem}.json`), bytes);
  fresh(join(STORE, `${stem}.sha256`), hash(bytes) + '\n');
}
function verifyPerf(label) {
  const frozen = verify(label), receipt = readReceipt(`${label}.compile`);
  assert.equal(receipt.input_sha256, frozen.id.input_sha256);
  assert.equal(receipt.identity_sha256, hash(readFileSync(join(STORE, `${label}.identity.json`))));
  assert.equal(receipt.features, FEATURES);
  assert(receipt.compiling_sefer_alloc, 'missing rebuild evidence');
  assert.equal(hash(readFileSync(safeFile(frozen.dir, receipt.binary.path))), receipt.binary.sha256, 'perf binary changed');
  const result = JSON.parse(readFileSync(join(STORE, `${label}.compile-build.result.json`)));
  assert.equal(result.code, 0);
  assert.equal(hash(readFileSync(join(STORE, `${label}.compile-build.log`))), result.sanitized_log_sha256);
  return { ...frozen, receipt };
}
async function compile(label) {
  const { dir, id } = verify(label);
  assert(!existsSync(join(dir, 'target')), 'compile requires fresh target; no stale build reuse');
  const tools = await versions(dir, `${label}.compile-tools`);
  const command = `${envelope()} cargo bench --locked --no-run --message-format=json-render-diagnostics --bench perf_gate_iai --features ${quote(FEATURES)}`;
  const { output, result } = await linux(dir, command, `${label}.compile-build`, true);
  verify(label);
  assert(/Compiling sefer-alloc\b/.test(output), 'fresh sefer-alloc compilation evidence required');
  saveReceipt(`${label}.compile`, { base: BASE, tree: id.tree, input_sha256: id.input_sha256,
    identity_sha256: hash(readFileSync(join(STORE, `${label}.identity.json`))), features: FEATURES,
    tools, command: sanitize(command), compiling_sefer_alloc: true, build_result: result,
    binary: artifact(output, 'perf_gate_iai', dir) });
  verifyPerf(label);
  console.log(`Compiled ${label}: fresh Linux nonstats binary receipt saved.`);
}
async function activation(label) {
  const { dir, id } = verify(label);
  const perf = existsSync(join(STORE, `${label}.compile.json`)) ? verifyPerf(label).receipt.binary : null;
  assert(!existsSync(join(dir, 'target', 'activation')), 'activation target exists; refuse stale/repeated receipt');
  const features = `${FEATURES} alloc-stats`;
  const command = `${envelope('target/activation')} cargo test --locked --no-run --message-format=json-render-diagnostics --features ${quote(features)} --test r16_perf83_activation`;
  const tools = await versions(dir, `${label}.activation-tools`);
  const { output } = await linux(dir, command, `${label}.activation-build`, true);
  assert(/Compiling sefer-alloc\b/.test(output), 'fresh stats build evidence required');
  const binary = artifact(output, 'r16_perf83_activation', dir);
  saveReceipt(`${label}.activation-binary`, { features, tools, binary, input_sha256: id.input_sha256, command: sanitize(command) });
  for (const scenario of SCENARIOS) {
    verify(label);
    assert.equal(hash(readFileSync(safeFile(dir, binary.path))), binary.sha256);
    const cmd = `ARM=${quote(label)} SCENARIO=${quote(scenario)} ${quote(`./${binary.path}`)} --exact single_realloc_activation --test-threads=1 --nocapture`;
    const { output: testOutput } = await linux(dir, cmd, `${label}.activation-${scenario}`);
    assert(/test result: ok\. 1 passed; 0 failed/.test(testOutput), 'one executed activation required');
    const rows = testOutput.split(/\r?\n/).map(line => line.slice(line.indexOf('{')))
      .filter(line => line.startsWith('{')).map(line => JSON.parse(line));
    assert.equal(rows.length, 1);
    const row = rows[0], inplace = scenario === SCENARIOS[4] || (label === 'B' && SCENARIOS.slice(0,2).includes(scenario));
    assert.equal(row.arm, label); assert.equal(row.scenario, scenario);
    assert.equal(row.moved, !inplace); assert.equal(row.inplace_large_delta, Number(inplace));
    assert.equal(row.inplace_small_delta, 0); assert.equal(row.decline_delta, Number(!inplace));
    assert(row.preserved_prefix && row.new_layout_dealloc && Number.isSafeInteger(row.pid));
    saveReceipt(`${label}.activation-${scenario}.verified`, { input_sha256: id.input_sha256, binary, outcome: row });
    verify(label);
    assert.equal(hash(readFileSync(safeFile(dir, binary.path))), binary.sha256);
  }
  if (perf) assert.deepEqual(verifyPerf(label).receipt.binary, perf, 'activation altered perf binary');
  console.log(`Five separate-process ${label} activation receipts verified.`);
}
async function run(name) {
  assert(['A1','A2','C0','B1','B2'].includes(name), 'explicit run label required');
  const label = name.startsWith('B') ? 'B' : name === 'C0' ? 'C0' : 'A';
  const { dir, id, receipt } = verifyPerf(label);
  for (const scenario of SCENARIOS) {
    const activation = readReceipt(`${label}.activation-${scenario}.verified`);
    assert.equal(activation.input_sha256, id.input_sha256, 'activation source mismatch');
    assert.equal(hash(readFileSync(safeFile(dir, activation.binary.path))), activation.binary.sha256, 'activation binary changed');
    const activationResult = JSON.parse(readFileSync(join(STORE, `${label}.activation-${scenario}.result.json`)));
    assert.equal(activationResult.code, 0);
    assert.equal(hash(readFileSync(join(STORE, `${label}.activation-${scenario}.log`))), activationResult.sanitized_log_sha256);
  }
  const tools = await versions(dir, `${name}.run-tools`);
  assert.equal(tools, receipt.tools, 'toolchain/runner changed since compile');
  saveReceipt(`${name}.pre-run`, { input_sha256: id.input_sha256, binary: receipt.binary,
    tools, features: FEATURES, benches: BENCHES, compile_receipt_sha256: hash(readFileSync(join(STORE, `${label}.compile.json`))) });
  const { output, result } = await execute(process.execPath, ['scripts/iai.mjs', ...BENCHES], dir, `${name}.run`, true);
  verifyPerf(label);
  const toolsAfter = await versions(dir, `${name}.post-run-tools`);
  assert.equal(toolsAfter, tools, 'tools changed during measurement');
  // cargo may re-fingerprint on drvfs; validity = the exact compiled binary ran unchanged.
  const ran = [...output.matchAll(/Running benches\/perf_gate_iai\.rs \((target\/release\/deps\/perf_gate_iai-[0-9a-f]+)\)/g)].map(m => m[1]);
  assert.deepEqual(ran, [receipt.binary.path], 'measured binary is not the compile-receipt binary');
  assert.equal(hash(readFileSync(safeFile(dir, receipt.binary.path))), receipt.binary.sha256, 'binary changed by measurement rebuild');
  for (const name of BENCHES) assert(output.includes(`perf_gate_iai::perf_gate::${name}`), `missing raw bench: ${name}`);
  saveReceipt(`${name}.post-run`, { input_sha256: id.input_sha256, binary: receipt.binary, result });
  console.log(`Saved ${name} actual iai receipt; no verdict computed by driver.`);
}
function parseProbeOutput(stdout, stderr, expect) {
  assert.equal(stderr.trim(), '', 'probe stderr must be empty');
  const lines = stdout.split(/\r?\n/).filter(line => line.trim() !== '');
  const configs = lines.filter(line => line.startsWith('config:'));
  const rows = lines.filter(line => line.startsWith('{'));
  assert.equal(configs.length, 1, 'exactly one config: line required');
  assert.equal(rows.length, 1, 'exactly one NDJSON line required');
  assert.equal(lines.length, 2, 'unexpected probe stdout lines');
  const row = JSON.parse(rows[0]), config = JSON.parse(configs[0].slice('config:'.length));
  assert.deepEqual(Object.keys(row).sort(), RSS_KEYS, 'RSS row schema');
  assert.deepEqual(Object.keys(config).sort(), CONFIG_KEYS, 'config schema');
  assert.equal(row.arm, expect.arm); assert.equal(row.scenario, expect.scenario);
  assert.equal(row.sample, expect.sample); assert.equal(row.statistic, 'per-process absolute RSS');
  assert.equal(row.source_input_sha256, expect.source); assert.equal(row.binary_sha256, expect.binary);
  for (const key of ['rss_bytes', 'baseline_rss_bytes', 'peak_bytes', 'final_free_rss_bytes', 'teardown_rss_bytes', 'pid']) {
    assert(Number.isSafeInteger(row[key]) && row[key] > 0, `invalid ${key}`);
  }
  assert(row.peak_bytes >= row.rss_bytes && row.peak_bytes >= row.baseline_rss_bytes, 'peak below resident bytes');
  assert.equal(config.config_conflicts_delta, 0, 'config conflict in fresh process');
  return { rowText: rows[0], row, config };
}
async function spawnCollect(dir, command) {
  const full = `cd ${quote(linuxPath(dir))} && ${command}`;
  const [cmd, args] = process.platform === 'win32' ? ['wsl.exe', ['bash', '-lc', full]] : ['bash', ['-lc', full]];
  let stdout = '', stderr = '', timedOut = false, spawnError = null;
  const child = spawn(cmd, args, { cwd: dir, shell: false, detached: process.platform !== 'win32' });
  const timer = setTimeout(() => {
    timedOut = true;
    if (process.platform === 'win32') execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F']);
    else process.kill(-child.pid, 'SIGKILL');
  }, 600_000);
  child.stdout.on('data', b => { stdout += b.toString(); });
  child.stderr.on('data', b => { stderr += b.toString(); });
  const code = await new Promise(resolve => {
    child.on('error', error => { spawnError = sanitize(error.message); });
    child.on('close', resolve);
  }).finally(() => clearTimeout(timer));
  return { code, stdout, stderr, timedOut, spawnError };
}
function verifyRss(label) {
  assert(['A', 'B'].includes(label), 'RSS arms are A and B');
  const frozen = verify(label), receipt = readReceipt(`${label}.rss.compile`);
  assert.equal(receipt.input_sha256, frozen.id.input_sha256);
  assert.equal(receipt.identity_sha256, hash(readFileSync(join(STORE, `${label}.identity.json`))));
  assert.equal(receipt.features, FEATURES);
  assert.equal(receipt.compiling_sefer_alloc, true);
  assert.equal(receipt.build_result.code, 0);
  assert.equal(hash(receipt.build_log), receipt.build_result.sanitized_log_sha256, 'RSS build log hash');
  assert.equal(hash(readFileSync(join(STORE, `${label}.rss-compile-build.log`))), receipt.build_result.sanitized_log_sha256);
  assert(receipt.binary.path.startsWith('target/rss/'), 'RSS binary must live in its own target');
  assert.equal(hash(readFileSync(safeFile(frozen.dir, receipt.binary.path))), receipt.binary.sha256, 'RSS binary changed');
  return { ...frozen, receipt };
}
async function rssCompile(label) {
  assert(['A', 'B'].includes(label), 'RSS arms are A and B');
  const { dir, id } = verify(label);
  assert(existsSync(join(dir, PROBE)), 'frozen snapshot lacks the RSS probe');
  assert(!existsSync(join(dir, 'target', 'rss')), 'RSS compile requires a fresh target/rss; no stale build reuse');
  const tools = await versions(dir, `${label}.rss-compile-tools`);
  const command = `${envelope('target/rss')} cargo build --locked --release --message-format=json-render-diagnostics --example ${RSS_EXAMPLE} --features ${quote(FEATURES)}`;
  const { output, result } = await linux(dir, command, `${label}.rss-compile-build`, true);
  verify(label);
  assert(/Compiling sefer-alloc\b/.test(output), 'fresh sefer-alloc compilation evidence required');
  const buildLog = readFileSync(join(STORE, `${label}.rss-compile-build.log`), 'utf8');
  assert.equal(hash(buildLog), result.sanitized_log_sha256);
  saveReceipt(`${label}.rss.compile`, { base: BASE, tree: id.tree, features: FEATURES, input_sha256: id.input_sha256,
    identity_sha256: hash(readFileSync(join(STORE, `${label}.identity.json`))), tools, command: sanitize(command),
    compiling_sefer_alloc: true, build_result: result, build_log: buildLog, binary: artifact(output, RSS_EXAMPLE, dir) });
  verifyRss(label);
  console.log(`Compiled ${label}: fresh Linux nonstats RSS probe receipt saved.`);
}
async function rssRun() {
  const arms = {};
  for (const label of ['A', 'B']) {
    arms[label] = verifyRss(label);
    for (const scenario of SCENARIOS) {
      const activation = readReceipt(`${label}.activation-${scenario}.verified`);
      assert.equal(activation.input_sha256, arms[label].id.input_sha256, 'activation source mismatch');
    }
    const tools = await versions(arms[label].dir, `rss.${label}.run-tools`);
    assert.equal(tools, arms[label].receipt.tools, 'toolchain changed since RSS compile');
  }
  saveReceipt('rss.pre-run', { features: FEATURES, scenarios: RSS_SCENARIOS, samples_per_scenario: 9,
    order: 'per scenario: odd samples A,B; even samples B,A',
    arms: Object.fromEntries(['A', 'B'].map(l => [l, { input_sha256: arms[l].id.input_sha256, binary: arms[l].receipt.binary,
      compile_receipt_sha256: hash(readFileSync(join(STORE, `${l}.rss.compile.json`))) }])) });
  const rows = [], configs = [], processes = [], pids = new Set();
  let reference = null, index = 0;
  try {
    for (const scenario of RSS_SCENARIOS) {
      for (let sample = 1; sample <= 9; sample++) {
        for (const arm of sample % 2 === 1 ? ['A', 'B'] : ['B', 'A']) {
          const before = verifyRss(arm), bin = before.receipt.binary;
          const env = `ARM=${quote(arm)} SCENARIO=${quote(scenario)} SAMPLE=${sample} SOURCE_INPUT_SHA256=${quote(before.id.input_sha256)} BINARY_SHA256=${quote(bin.sha256)}`;
          const out = await spawnCollect(before.dir, `${env} ${quote(`./${bin.path}`)}`);
          assert(!out.spawnError && !out.timedOut && out.code === 0, `probe process failed: ${arm}/${scenario}/${sample}`);
          const parsed = parseProbeOutput(out.stdout, out.stderr, { arm, scenario, sample, source: before.id.input_sha256, binary: bin.sha256 });
          assert(!pids.has(parsed.row.pid), 'duplicate probe pid'); pids.add(parsed.row.pid);
          const text = JSON.stringify(parsed.config);
          reference ??= text;
          assert.equal(text, reference, 'resolved allocator config differs between processes/arms');
          verifyRss(arm);
          rows.push(parsed.rowText);
          configs.push(JSON.stringify({ arm, scenario, sample, config: parsed.config }));
          processes.push({ index: index++, arm, scenario, sample, code: out.code, pid: parsed.row.pid,
            stdout_sha256: hash(out.stdout), stderr_sha256: hash(out.stderr) });
        }
      }
    }
  } catch (error) {
    fresh(join(STORE, 'rss.failure.json'), jsonBytes({ message: sanitize(error.message), completed_processes: processes }));
    throw error;
  }
  assert.equal(rows.length, 108);
  const rssLog = rows.join('\n') + '\n', configLog = configs.join('\n') + '\n';
  fresh(join(STORE, 'rss.log'), rssLog);
  fresh(join(STORE, 'rss.config.log'), configLog);
  fresh(join(STORE, 'rss.run.result.json'), jsonBytes({ processes }));
  for (const label of ['A', 'B']) {
    verifyRss(label);
    assert.equal(await versions(arms[label].dir, `rss.${label}.post-run-tools`), arms[label].receipt.tools, 'tools changed during RSS run');
  }
  saveReceipt('rss.post-run', { rss_log_sha256: hash(rssLog), config_log_sha256: hash(configLog),
    process_count: processes.length, resolved_config: JSON.parse(reference) });
  console.log('Saved 108 fresh-process RSS rows and config receipts; no verdict computed by driver.');
}
function selfCheck() {
  assert.equal(BENCHES.length, 17); assert.equal(new Set(BENCHES).size, 17);
  const generated = generatedHarness(readFileSync(safeFile(ROOT, 'scripts/iai.mjs'), 'utf8'));
  assert(generated.includes('CARGO_BUILD_JOBS=3') && generated.includes('cargo bench --locked'));
  assert(generated.includes('"$PWD/target"') && !generated.includes('cargo install'));
  // Node parses both generated and original harness without running either.
  execFileSync(process.execPath, ['--check', '--input-type=module'], { input: generated });
  const paths = inputs(ROOT);
  for (const p of REQUIRED) assert(paths.includes(p), `required input missing: ${p}`);
  assert.equal(sanitize(' /home/example/toolchain/bin/rustc D:/private/example.exe'), ' <target> <target>');
  assert.equal(sanitize('{"p":"/home/x/y","q":"x=/usr/bin"}'), '{"p":"<target>","q":"x=<target>"}');
  // Memory-only parser fixture: NOT RSS evidence.
  const hex64 = 'a'.repeat(64), bin64 = 'b'.repeat(64);
  const fixtureRow = { arm: 'A', scenario: 'cycles8to6', sample: 1, rss_bytes: 100, baseline_rss_bytes: 50, peak_bytes: 120,
    final_free_rss_bytes: 60, teardown_rss_bytes: 55, pid: 7, statistic: 'per-process absolute RSS',
    source_input_sha256: hex64, binary_sha256: bin64 };
  const fixtureConfig = { requested: 'x', decay_rate_bp: 1, decay_interval_ms: 2, headroom_bytes: 3,
    large_cache_budget_bytes: null, large_cache_total_slots: 8, pool_cap: 4, config_conflicts_delta: 0 };
  const expect = { arm: 'A', scenario: 'cycles8to6', sample: 1, source: hex64, binary: bin64 };
  const good = `config:${JSON.stringify(fixtureConfig)}\n${JSON.stringify(fixtureRow)}\n`;
  assert.equal(parseProbeOutput(good, '', expect).row.pid, 7);
  assert.throws(() => parseProbeOutput(good + good, '', expect));
  assert.throws(() => parseProbeOutput(good, 'warning', expect));
  assert.throws(() => parseProbeOutput(good.replace('"pid":7', '"pid":0'), '', expect));
  assert.throws(() => parseProbeOutput(good, '', { ...expect, binary: hex64 }));
  assert.throws(() => parseProbeOutput(good.replace('"peak_bytes":120', '"peak_bytes":99'), '', expect));
  assert.equal(RSS_SCENARIOS.length, 6);
  console.log(`Static anchors/syntax/17 rows/filesystem/sanitizer/probe-parser checks passed; RSS probe ${paths.includes(PROBE) ? 'present' : 'missing'}. No snapshots/builds/measurements.`);
}
try {
  const [mode, name, ...extra] = process.argv.slice(2);
  assert.equal(extra.length, 0, 'extra arguments rejected');
  if (mode === '--self-check') { assert(!name); selfCheck(); }
  else if (mode === '--prepare') { assert(!name); prepare(); }
  else if (mode === '--prepare-b') { assert(!name); prepareB(); }
  else if (mode === '--verify') {
    assert(!name); verify('A'); verify('C0');
    if (existsSync(join(STORE, 'B')) || existsSync(join(STORE, 'B.identity.json'))) verify('B');
    console.log('All existing frozen arms verified.');
  } else if (mode === '--compile' || mode === '--activation') {
    assert(['A','C0','B'].includes(name), 'explicit arm required');
    if (mode === '--compile') await compile(name); else await activation(name);
  } else if (mode === '--rss-compile') { assert(['A','B'].includes(name), 'explicit arm A or B required'); await rssCompile(name); }
  else if (mode === '--rss-run') { assert(!name); await rssRun(); }
  else if (mode === '--run') await run(name);
  else throw new Error('Usage: --self-check | --prepare | --prepare-b | --verify | --compile A|C0|B | --activation A|C0|B | --rss-compile A|B | --rss-run | --run A1|A2|C0|B1|B2');
} catch (error) {
  console.error(sanitize(error.message));
  process.exitCode = 1;
}
