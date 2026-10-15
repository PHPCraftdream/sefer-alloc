// Canonical R20 PRF-04 contract shared by the runner, judge and verifier.
// Rust cell names are checked against this inventory; do not duplicate it.
import assert from 'node:assert/strict';
export const RELEASE_LAYER = 'L2';
export const N = 16;
export const ORACLE_COUNTERS = Object.freeze([
  'heap_reuse_spill_bits_delta',
  'heap_reuse_take_hits',
]);
// Demangled inherent-impl form: `<r20_prf04_heap_reuse_l2_iai::hr::FirstAlloc<false>>::alloc_inner`.
const L2_FIRST_ALLOC = 'r20_prf04_heap_reuse_l2_iai::hr::FirstAlloc';
const L2_ALLOC_CALLER_PATTERN = new RegExp(
  `^(?:<${L2_FIRST_ALLOC}<(?:false|true)>>|${L2_FIRST_ALLOC}<_>)::alloc_inner$`);
export const isL2AllocCaller = name => L2_ALLOC_CALLER_PATTERN.test(name);
export function hasL2FirstBindPath(names) {
  return names.some(name => /tls_heap::bind_slow_tagged/.test(name)) &&
    names.some(name => /(?:registry::)?(?:claim|claim_impl|HeapRegistry)/i.test(name));
}
const sizes = [['n', N], ['2n', 2 * N], ['4n', 4 * N]];
const rows = [];
function add(layer, family, suffix, n, group, name) {
  const target = layer === 'L2'
    ? 'r20_prf04_heap_reuse_l2_iai'
    : 'r20_prf04_heap_reuse_iai';
  const functionName = layer === 'L2'
    ? family === 'wave' || family === 'cold' || family === 'recycled_alloc'
      ? 'first_global_alloc'
      : null
    : family === 'recycled' ? 'recycled'
      : family === 'wave' ? 'wave_claim'
        : family === 'wave_release' ? 'wave_release_measured'
          : 'cold';
  const expectedSymbols = functionName
    ? [layer === 'L2' ? `${target}::hr::first_global_alloc` : `${target}::hr::${functionName}`]
    : ['std::sys::thread_local::native::eager::destroy::<sefer_alloc::global::tls_heap::AbandonGuard>'];
  rows.push(Object.freeze({
    name,
    group,
    index: rows.filter(row => row.group === group).length,
    layer,
    family,
    n,
    expectedSymbols,
  }));
}
add('L1', 'start', 'one', 1, 'heap_reuse', 'r20_prf04hr_start');
for (const [suffix, n] of sizes) {
  add('L1', 'cold', suffix, n, 'heap_reuse', `r20_prf04hr_cold_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L1', 'recycled', suffix, n, 'heap_reuse', `r20_prf04hr_recycled_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L1', 'wave', suffix, n, 'heap_reuse', `r20_prf04hr_wave_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L1', 'wave_release', suffix, n, 'heap_reuse', `r20_prf04hr_wave_release_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L2', 'cold', suffix, n, 'heap_reuse_l2', `r20_prf04hr_l2_cold_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L2', 'recycled_alloc', suffix, n, 'heap_reuse_l2',
    `r20_prf04hr_l2_recycled_alloc_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L2', 'recycled_release', suffix, n, 'heap_reuse_l2',
    `r20_prf04hr_l2_recycled_release_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L2', 'wave', suffix, n, 'heap_reuse_l2', `r20_prf04hr_l2_wave_${suffix}`);
}
for (const [suffix, n] of sizes) {
  add('L2', 'wave_release', suffix, n, 'heap_reuse_l2',
    `r20_prf04hr_l2_wave_release_${suffix}`);
}
export const CELLS = Object.freeze(rows);
export const BUDGETS = Object.freeze({
  cold: 4,
  recycledPair: 4,
  spillRelease: 14,
  startExtra: 4,
  l2Extra: 3,
  relativePercent: 5,
  // v2: gating L2 price ceiling, percent of the same A window (thread lifecycle).
  lifecyclePercent: 1,
  noiseAbsolute: 10,
});
export const FROZEN_FILES = Object.freeze([
  'docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md',
  'scripts/r20_prf04_heap_reuse_gate.mjs',
  'scripts/r20_prf04_heap_reuse_run.mjs',
  'scripts/r20_prf04_heap_reuse_verify_artifacts.mjs',
  'scripts/r20_prf04_heap_reuse_support.mjs',
  'benches/r20_prf04_heap_reuse_iai.rs',
  'benches/r20_prf04_heap_reuse_l2_iai.rs',
  'benches/support/r20_prf04_heap_reuse.rs',
  'examples/r20_prf04_heap_reuse_oracle.rs',
  'examples/r20_prf04_heap_reuse_l2_oracle.rs',
]);
export function validateActivation({ arm, family, observer, ...record }) {
  if (family === 'recycled_alloc' || family === 'recycled_release') family = 'recycled';
  const present = ORACLE_COUNTERS.every(key => Object.hasOwn(record, key));
  const values = ORACLE_COUNTERS.map(key => record[key]);
  const valid = values.every((value, index) => value === null || (Number.isSafeInteger(value) &&
    (index === 0 || value >= 0)));
  if (arm === 'A') {
    return observer === false && present && valid && values.every(value => value === null || value === 0);
  }
  if (arm !== 'B' || observer !== true || !present ||
    !Number.isSafeInteger(values[0]) || !Number.isSafeInteger(values[1]) || values[1] < 0) return false;
  const [spill, take] = values;
  if (family === 'cold' || family === 'start') return spill === 0 && take === 0;
  if (family === 'recycled') return spill === 0 && take === 0;
  if (family === 'wave') return spill <= 0 && take > 0;
  if (family === 'wave_release') return spill > 0 && take === 0;
  return false;
}
export function syntheticIdentity(head, tree, sha256) { return { head, tree, sha256 }; }
export function syntheticReceipt(path, sha256) { return { path, sha256 }; }
export function fakeBenchProgram({ cells, arm, role }) {
  const encodedCells = JSON.stringify(cells, null, 2);
  if (cells.some(cell => cell.layer === 'L2' && cell.expectedSymbols[0].endsWith('child_first_alloc'))) {
    throw new Error('legacy child_first_alloc target is forbidden');
  }
  return `import fs from 'node:fs';
import path from 'node:path';
const cells = ${encodedCells};
const arm = ${JSON.stringify(arm)};
const role = ${JSON.stringify(role)};
const homeArg = process.argv.indexOf('--home');
if (homeArg < 0 || !process.argv[homeArg + 1]) throw new Error('missing --home');
const home = path.resolve(process.argv[homeArg + 1]);
const executable = process.argv[1];
const executableText = executable.split(String.fromCharCode(92)).join('/');
const output = [];
const debug = [];
const settings = cell => [
  '--tool=callgrind', '--error-exitcode=0', '--trace-children=yes', '--fair-sched=try',
  '--I1=32768,8,64', '--D1=32768,8,64', '--LL=8388608,16,64', '--cache-sim=yes',
  '--compress-strings=no', '--compress-pos=no', '--dump-line=yes', '--dump-instr=no',
  '--combine-dumps=no', '--separate-threads=yes',
  '--toggle-collect=' + cell.expectedSymbols[0],
  '--callgrind-out-file=' + home + '/' + cell.name + '/callgrind.out.%p.%t',
].join(' ');
for (const cell of cells) {
  const dir = path.join(home, cell.name);
  fs.mkdirSync(dir, { recursive: true });
  const symbol = cell.expectedSymbols[0];
  const caller = cell.layer === 'L1'
    ? 'r20_prf04_heap_reuse_iai::gate::' + cell.name
    : cell.family.includes('release') ? '__call_tls_dtors::drop'
      : 'r20_prf04_heap_reuse_l2_iai::hr::FirstAlloc<_>::alloc_inner';
  const command = executableText + ' --iai-run ' + cell.group + ' ' + cell.index + ' 0';
  const count = cell.layer === 'L1' ? 1 : cell.n;
  const perWork = arm === 'B' && cell.family === 'wave' ? 900 : 1000;
  const cost = cell.layer === 'L1' ? perWork * cell.n : perWork;
  const threads = cell.layer === 'L1' ? [1] : Array.from({ length: count + 1 }, (_, i) => i + 1);
  for (const tid of threads) {
    const target = cell.layer === 'L1' || tid !== 1;
    const targetIr = target ? cost : 0;
    const support = cell.family.includes('release')
      ? 'sefer_alloc::HeapLease::release' : 'sefer_alloc::global::tls_heap::bind_slow_tagged';
    const callCost = cell.layer === 'L2' && !cell.family.includes('release') ? 2 : 1;
    const bindSelf = cell.layer === 'L2' && !cell.family.includes('release') ? 1 : 0;
    const records = target
      ? ['fn=' + caller, '1 0', 'cfn=' + symbol, 'calls=1 0', '1 ' + targetIr,
        'fn=' + symbol, '1 ' + (targetIr - callCost), 'cfn=' + support,
      'calls=1 0', '1 ' + callCost, 'fn=' + support, '1 1',
      ...(cell.layer === 'L2' && !cell.family.includes('release')
        ? ['cfn=sefer_alloc::global::tls_heap::bind_slow_tagged', 'calls=1 0', '1 1',
          'fn=sefer_alloc::global::tls_heap::bind_slow_tagged', '1 ' + bindSelf,
          'cfn=sefer_alloc::global::tls_heap::HeapRegistry::claim_impl', 'calls=1 0', '1 1',
          'fn=sefer_alloc::global::tls_heap::HeapRegistry::claim_impl', '1 0'] : [])]
    : ['fn=synthetic::main_thread_setup', '1 0'];
    const profile = ['version: 1', 'creator: synthetic-node-bench',
      'pid: 200', 'part: 1', 'thread: ' + tid,
      'cmd: ' + command, 'events: Ir', 'positions: line', ...records,
      'totals: ' + targetIr, ''].join('\\n');
    fs.writeFileSync(path.join(dir, 'callgrind.out.200.' + tid), profile);
    fs.writeFileSync(path.join(dir, 'callgrind.log'),
      '==200== Command: ' + command + '\\n==200== Tool: callgrind\\n');
  }
  const total = count * cost;
  output.push('r20_prf04_heap_reuse_iai::gate::' + cell.name + '\\nInstructions: ' + total);
  debug.push('iai_callgrind_runner::runner::tool: Debug: callgrind: Arguments: ' + settings(cell));
}
process.stdout.write(output.join('\\n') + '\\n' + debug.join('\\n') + '\\n');
`;
}
export function fakeOracleProgram({ marker, arm, role }) {
  const calls = role === 'oracle' ? '1' : 'n';
  return `import fs from 'node:fs';
const [family, nText, requestedArm] = process.argv.slice(2);
const n = Number(nText);
const arm = ${JSON.stringify(arm)};
const role = ${JSON.stringify(role)};
fs.writeFileSync(${JSON.stringify(marker)}, 'launched:' + arm + ':' + role);
const candidate = arm === 'B';
const spillDelta = !candidate ? null : family === 'wave' ? -1
  : family === 'wave_release' ? 1 : 0;
const result = {
  arm, family, n, hint: 0, bump: 0, capScan: 0, recovery: 0, highWaterDelta: 0,
  heap_reuse_spill_bits_delta: spillDelta,
  heap_reuse_take_hits: candidate ? Number(family === 'wave') : null,
  observer: candidate, configConflictsDelta: 0,
  actualTargetCalls: ${calls}, requestedArm,
};
process.stdout.write(JSON.stringify(result) + '\\n');
`;
}
export function syntheticCallgrindProfile({ pid, thread, command, symbol, caller, ir = 100 }) {
  return [
    'version: 1', 'creator: Callgrind-3.22', 'pid: ' + pid, 'part: 1',
    'thread: ' + thread, 'cmd: ' + command, 'events: Ir', 'positions: line',
    'fn=' + caller, '1 0', 'cfn=' + symbol, 'calls=1 0', '1 ' + ir,
    'fn=' + symbol, '1 ' + (ir - 1), 'cfn=HeapRegistry::claim_impl',
    'calls=1 0', '1 1', 'fn=HeapRegistry::claim_impl', '1 1',
    'totals: ' + (ir + 1), '',
  ].join('\\n');
}

export async function runRunnerSelfTest({
  main,
  ctx,
  local,
  putText,
  spawnSync,
  sha,
  shaFile,
  REPO,
  FEATURES,
  CELLS,
  fakeBenchProgram,
  fakeOracleProgram,
  basename,
  resolve,
  join,
  fs,
  assert,
  process,
  CANONICAL_FROZEN_FILES,
  JUDGE,
}) {
  assert.equal(CELLS.find(cell => cell.layer === 'L2' && cell.family === 'cold').expectedSymbols[0],
    'r20_prf04_heap_reuse_l2_iai::hr::first_global_alloc', 'legacy target helper must not return');
  assert.ok(!isL2AllocCaller('r20_prf04_heap_reuse_l2_iai::hr::first_global_alloc'),
    'target cannot be its own caller');
  assert.ok(!hasL2FirstBindPath(['HeapRegistry::claim_impl', 'alloc::warm']),
    'warm-only dynamic descendants are not first-bind evidence');
  assert.ok(hasL2FirstBindPath(['tls_heap::bind_slow_tagged', 'HeapRegistry::claim_impl']),
    'actual bind and claim descendants qualify');
  const outDir = 'target/r20p4-selftest-' + process.pid + '-' + Date.now() + '-' +
    Math.random().toString(16).slice(2);
  const root = resolve(REPO, outDir);
  const snapshots = join(root, 'snapshots');
  const builds = join(root, 'builds');
  const fakeDir = join(root, 'fake');
  fs.mkdirSync(fakeDir, { recursive: true });
  fs.mkdirSync(join(builds, 'A-oracle'), { recursive: true });
  fs.writeFileSync(join(root, 'Cargo.lock'), '# deterministic fake lock\\n');
  const candidate = 'b'.repeat(40), base = 'a'.repeat(40);
  const marker = join(root, 'oracle-launch.marker');
  const fakeFiles = new Map();
  const identityTree = head => head === base ? 'c'.repeat(40) : 'd'.repeat(40);
  ctx.adapter = {
    snapshotRoot: snapshots,
    buildRoot: builds,
    lockPath: join(root, 'Cargo.lock'),
    candidate,
    base,
    tree: identityTree,
    resolveRef: ref => ref === 'origin/main' || ref === base ? base : candidate,
    archiveSnapshot(_label, _head, dest) {
      fs.mkdirSync(join(dest, 'benches/support'), { recursive: true });
      fs.mkdirSync(join(dest, 'examples'), { recursive: true });
      fs.writeFileSync(join(dest, 'Cargo.toml'), '[package]\\nname = "fake"\\nversion = "0.1.0"\\n');
      fs.writeFileSync(join(dest, 'Cargo.lock'), '# fake archive lock\\n');
    },
    envCapture() {
      const text = 'synthetic node harness\\nfeatures: ' + FEATURES + '\\n';
      return { text, environmentSha256: sha(text) };
    },
    cargoBuild({ arm, role, target, kind, rustflags }) {
      const layerCells = CELLS.filter(cell => (role === 'controls') === (cell.layer === 'L2'));
      const file = join(fakeDir, arm + '-' + role + '.fake.mjs');
      const buildDir = join(builds, arm + '-' + role);
      fs.mkdirSync(buildDir, { recursive: true });
      const buildFile = join(buildDir, basename(file));
      const text = role === 'oracle' || role === 'control'
        ? fakeOracleProgram({ marker, arm, role })
        : fakeBenchProgram({ cells: layerCells, arm, role });
      fs.writeFileSync(file, text);
      fs.writeFileSync(buildFile, text);
      fakeFiles.set(buildFile, { arm, role });
      return {
        role,
        target,
        kind,
        exe: buildFile,
        binarySha256: shaFile(buildFile),
        features: FEATURES.split(' '),
        rustflags: rustflags ?? '',
        cfg: role === 'oracle' ? ['prf04_heap_reuse_activation'] : [],
        targetDir: join(builds, arm + '-' + role),
      };
    },
    oracleRecord(file, args, options) {
      fs.writeFileSync(marker, file);
      return spawnSync(process.execPath, [file, ...args], options);
    },
    measure(binary, arm, role, cells, home, env) {
      const result = spawnSync(process.execPath, [binary.exe, '--home', home], {
        cwd: REPO,
        encoding: 'utf8',
        maxBuffer: 1 << 24,
      });
      assert.equal(result.status, 0, 'fake bench executable failed: ' + (result.stderr ?? ''));
      return { status: result.status, stdout: result.stdout ?? '', stderr: result.stderr ?? '' };
    },
    boundaryEvidence(_binary, tag, symbols) {
      const path = 'boundary/' + tag;
      const nmText = '00000010 T ' + symbols[0];
      assert.ok(nmText.length > 0, 'synthetic nm fixture must contain symbol text');
      const nmReceipt = putText(path + '-nm.log', nmText);
      const caller = [
        '00000000 <synthetic::caller>:',
        '   0: call 10 <' + symbols[0] + '>',
        '   5: ret',
      ].join('\n');
      const definitions = symbols.map(symbol => [
        '00000010 <' + symbol + '>:',
        '  10: ret',
      ].join('\n')).join('\n');
      const assembly = caller + '\n' + definitions + '\n';
      assert.ok(assembly.includes('\n') && assembly.split('\n').length > 2,
        'synthetic disassembly fixture must contain actual line breaks');
      const disassemblyReceipt = putText(path + '-disassembly.log', assembly);
      return { nmReceipt, disassemblyReceipt, verification: true };
    },
    judge(inputPath) {
      return spawnSync(process.execPath, [join(REPO, JUDGE), '--input', inputPath], {
        cwd: REPO,
        encoding: 'utf8',
        maxBuffer: 1 << 24,
      });
    },
    frozenHashes() {
      return CANONICAL_FROZEN_FILES.map(path => ({
        path,
        sha256: sha(fs.readFileSync(join(REPO, path))),
      }));
    },
    verify(out) {
      return spawnSync(
        process.execPath,
        [join(REPO, 'scripts/r20_prf04_heap_reuse_verify_artifacts.mjs'), out],
        { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 24 },
      );
    },
  };
  try {
    main(['--prepare', '--out-dir', outDir]);
    main(['--build', '--out-dir', outDir]);
    const oracleExe = join(builds, 'A-oracle', basename(ctx.builds.arms.A.oracle.exe));
    const original = fs.readFileSync(oracleExe);
    fs.writeFileSync(oracleExe, Buffer.from('changed fake executable'));
    assert.throws(() => main(['--oracle', '--out-dir', outDir]), /oracle executable drift before launch/);
    assert.equal(fs.existsSync(marker), false, 'mutated executable was launched');
    const mutationRejectedBeforeLaunch = true;
    fs.writeFileSync(oracleExe, original);
    main(['--oracle', '--out-dir', outDir]);
    main(['--measure', '--out-dir', outDir]);
    const judgeResult = ctx.adapter.judge(local('input.json'));
    const verdict = judgeResult.status === 0 ? JSON.parse(judgeResult.stdout) : null;
    const verifyResult = ctx.adapter.verify(outDir);
    if (verifyResult.status !== 0) {
      let diagnostic;
      try { diagnostic = JSON.parse(verifyResult.stdout ?? ''); }
      catch { diagnostic = null; }
      process.stderr.write('artifact verifier failures: ' + JSON.stringify(diagnostic?.failures?.slice(0, 30) ?? {
        stdout: (verifyResult.stdout ?? '').split('\\n').slice(0, 20),
        stderr: (verifyResult.stderr ?? '').split('\\n').slice(0, 20),
      }) + '\\n');
    }
    const checks = [{ name: 'prepare/build handlers', pass: fs.existsSync(local('builds.json')) },
      { name: 'prelaunch replacement guard', pass: mutationRejectedBeforeLaunch },
      { name: 'oracle launched Node executables', pass: fs.existsSync(marker) &&
        fs.readFileSync(marker, 'utf8').startsWith('launched:B:') },
      { name: 'oracle handler', pass: fs.existsSync(local('oracle.json')) },
      { name: 'measure launched fake Node executable', pass: fs.existsSync(local('input.json')) },
      { name: 'judge accepted synthetic input', pass: judgeResult.status === 0 &&
        verdict?.verdict === 'GO (deterministic; native deferred)' },
      { name: 'artifact verifier accepted output', pass: verifyResult.status === 0 }];
    if (checks.some(item => !item.pass)) throw new Error('self-test failed: ' + JSON.stringify(checks));
    console.log(JSON.stringify({ status: 'PASS', checks: checks.length, measurements: 0,
      stages: ['prepare', 'build', 'oracle', 'measure'], note: 'fake executable protocol' }));
  } finally {
    ctx.adapter = null; ctx.manifest = null; ctx.builds = null; ctx.oracle = null; ctx.outDir = null;
    fs.rmSync(root, { recursive: true, force: true });
  }
}

// Callgrind profile parser (moved from the runner; pure).
export function naturalBig(token) {
  assert.match(token, /^(?:\d+|0x[0-9a-fA-F]+)$/);
  return BigInt(token);
}
export function safeNumber(value) {
  assert.ok(value >= 0n && value <= BigInt(Number.MAX_SAFE_INTEGER));
  return Number(value);
}
export function positionToken(token, previous) {
  if (token === '*') { assert.notEqual(previous, null); return previous; }
  if (/^[+-]/.test(token)) {
    assert.notEqual(previous, null);
    const value = previous + (token[0] === '-' ? -1n : 1n) * naturalBig(token.slice(1));
    assert.ok(value >= 0n); return value;
  }
  return naturalBig(token);
}
export function profileFunctions(text) {
  const events = /^events: (.+)$/m.exec(text)?.[1].trim().split(/\s+/);
  assert.ok(events, 'Callgrind events absent');
  const irIndex = events.indexOf('Ir');
  assert.ok(irIndex >= 0, 'Callgrind Ir event absent');
  const positions = /^positions: (.+)$/m.exec(text)?.[1].trim().split(/\s+/) ?? ['line'];
  const ids = new Map(), functions = new Map();
  let current = null, callee = null, pending = false, previous = Array(positions.length).fill(null);
  const symbol = value => {
    const m = /^(?:\((\d+)\)\s*)?(.*)$/.exec(value);
    if (m[2]) return m[2];
    assert.ok(ids.has(m[1]), 'unknown compressed function ID'); return ids.get(m[1]);
  };
  const fn = name => {
    if (!functions.has(name)) functions.set(name, { selfIr: 0n, edges: new Map(), edgeCalls: new Map() });
    return functions.get(name);
  };
  for (const line of text.split(/\r?\n/)) {
    const m = /^(?:c?fn)=\((\d+)\)\s+(.+)$/.exec(line);
    if (m) ids.set(m[1], m[2]);
  }
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('fn=')) {
      current = symbol(line.slice(3)); fn(current); callee = null;
      previous = Array(positions.length).fill(null);
    } else if (line.startsWith('cfn=')) { callee = symbol(line.slice(4)); fn(callee); }
    else if (line.startsWith('calls=')) {
      assert.ok(current && callee && !pending, 'invalid Callgrind calls record');
      const fields = line.slice(6).trim().split(/\s+/);
      positions.forEach((_, i) => positionToken(fields[i + 1], previous[i]));
      const f = fn(current); f.edgeCalls.set(callee, (f.edgeCalls.get(callee) ?? 0n) + naturalBig(fields[0]));
      pending = true;
    } else if (/^[*+\-\d]/.test(line)) {
      assert.ok(current, 'Callgrind cost without function');
      const fields = line.trim().split(/\s+/);
      previous = positions.map((_, i) => positionToken(fields[i], previous[i]));
      const costs = fields.slice(positions.length).map(naturalBig), cost = costs[irIndex] ?? 0n;
      const f = fn(current);
      if (pending) { f.edges.set(callee, (f.edges.get(callee) ?? 0n) + cost); pending = false; }
      else f.selfIr += cost;
    } else if (line && !/^(?:#|[A-Za-z][A-Za-z_]*(?:=|:))/.test(line)) {
      throw Error('malformed Callgrind record');
    }
  }
  assert.ok(!pending, 'missing final Callgrind call cost');
  return functions;
}
export function argvTokens(value) {
  const result = []; let token = '', quote = null, active = false;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (quote) {
      if (c === quote) quote = null;
      else if (c === '\\' && quote === '"' && ['"', '\\'].includes(value[i + 1])) token += value[++i];
      else token += c;
      active = true;
    } else if (c === '"' || c === "'") { quote = c; active = true; }
    else if (/\s/.test(c)) { if (active) { result.push(token); token = ''; active = false; } }
    else { token += c; active = true; }
  }
  assert.equal(quote, null, 'unclosed argv quote'); if (active) result.push(token);
  return result;
}
