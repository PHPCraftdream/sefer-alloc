// Read-only R20 PRF-04 heap-reuse artifact acceptance verifier.
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  CELLS,
  RELEASE_LAYER,
  FROZEN_FILES,
  isL2AllocCaller,
  hasL2FirstBindPath,
} from './r20_prf04_heap_reuse_support.mjs';
import { syntheticFixture } from './r20_prf04_heap_reuse_gate.mjs';

const sha = data => createHash('sha256').update(data).digest('hex');
const isObject = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const validHash = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const safeRelative = value => typeof value === 'string' && value.length > 0 && !isAbsolute(value) &&
  !value.includes('\\') && value.split('/').every(part => part !== '' && part !== '.' && part !== '..');

export async function verifyArtifacts(outDir) {
  const root = resolve(outDir);
  const checks = [];
  const failures = [];
  const check = (name, passed, detail) => {
    checks.push({ name, pass: Boolean(passed) });
    if (!passed) failures.push(`${name}: ${detail}`);
  };
  let parsers;
  let parserError = '';
  try { parsers = await import('./r20_prf04_heap_reuse_run.mjs'); }
  catch (error) { parserError = error.message; }
  const parseProfiles = parsers?.parseProfiles;
  const machineBoundary = parsers?.machineBoundary;
  check('callgraph-parser-integration', typeof parseProfiles === 'function' &&
    typeof machineBoundary === 'function', parserError || 'runner parser exports unavailable');
  const readJson = name => {
    try { return JSON.parse(fs.readFileSync(join(root, name), 'utf8')); }
    catch (error) { check(`stage:${name}`, false, error.message); return null; }
  };
  const input = readJson('input.json');
  const manifest = readJson('manifest.json');
  const builds = readJson('builds.json');
  const oracle = readJson('oracle.json');
  if (![input, manifest, builds, oracle].every(isObject)) return { pass: false, checks, failures };
  const map = input.receiptHashes;
  const haveMap = isObject(map) && Object.keys(map).length > 0;
  check('receiptHashes:required', haveMap, 'missing or empty receiptHashes');
  if (haveMap) for (const [name, expected] of Object.entries(map)) {
    const path = safeRelative(name) ? resolve(root, name) : '';
    const inside = Boolean(path) && !relative(root, path).startsWith(`..${sep}`) && relative(root, path) !== '..';
    let good = false;
    try {
      good = inside && validHash(expected) && fs.statSync(path).isFile() &&
        sha(fs.readFileSync(path)) === expected;
    }
    catch { good = false; }
    check(`receipt:${name}`, good, 'unsafe path, missing file, or digest mismatch');
  }
  const ref = (value, where) => {
    check(`ref:${where}`, typeof value === 'string' && safeRelative(value) && haveMap &&
      Object.hasOwn(map, value), 'required receipt is unsafe or not hash-anchored');
  };
  for (const key of ['inventoryReceipt', 'environmentReceipt', 'oracleReceipt']) ref(input[key], key);
  if (RELEASE_LAYER === 'L1') {
    const receipt = input.fallbackReceipt;
    check('fallback:contract', isObject(receipt) && receipt.reason === 'guard-drop-uncollectable' &&
      receipt.rehearsal === true && receipt.source?.baseHead === identities.A?.source?.baseHead &&
      receipt.source?.identitySha256 === identities.A?.source?.identitySha256 &&
      Number.isFinite(Date.parse(receipt.source?.capturedAt)) &&
      Date.parse(receipt.source.capturedAt) < Date.parse(receipt.originalCapturedAt),
    'fallback receipt must bind A identity and pre-build capture time');
    for (const key of ['symbolReceipt', 'profileReceipt', 'rawLogReceipt']) ref(receipt?.[key], `fallback.${key}`);
  }
  check('identityReviewReceipt', typeof input.identityReviewReceipt === 'string' &&
    input.identityReviewReceipt.trim().length > 0, 'required descriptor missing');
  for (const arm of ['A', 'B']) ref(input.identities?.[arm]?.buildReceipt, `${arm}.buildReceipt`);
  ref(input.identities?.C0?.buildReceipt, 'C0.buildReceipt');
  ref(input.ancestry?.receipt, 'ancestry.receipt');
  const identities = input.identities ?? {};
  const arches = manifest.arches ?? {};
  const rehearsal = input.rehearsal === true;
  for (const label of ['A', 'B', 'C']) {
    const source = label === 'C' ? identities.C0?.source : identities[label]?.source;
    const arch = label === 'C' ? arches.A : arches[label];
    if (label === 'C') check('identity:C-metadata', arches.C?.head === arches.A?.head &&
      arches.C?.tree === arches.A?.tree && arches.C?.snapshotSha256 === arches.A?.snapshotSha256,
      'C metadata must match A source head/tree/snapshot');
    check(`identity:${label}`, isObject(source) && validHash(source.identitySha256) &&
      typeof source.baseHead === 'string' && /^[0-9a-f]{40}$/.test(source.baseHead) &&
      source.baseHead === arch?.head && source.identitySha256 === (arch?.identitySha256 ?? arch?.snapshotSha256),
      'source identity differs from manifest');
  }
  check('identity:C-source-A', JSON.stringify(identities.C0?.source) === JSON.stringify(identities.A?.source),
    'C source must equal A source');
  check('ancestry:chain', Array.isArray(input.ancestry?.chain) &&
    input.ancestry.chain.length === 2 &&
    input.ancestry.chain[0] === arches.A?.head &&
    input.ancestry.chain[1] === (rehearsal ? arches.A?.head : arches.B?.head),
  'ancestry does not bind manifest');
  check('releaseLayer', input.releaseLayer === RELEASE_LAYER, `must equal ${RELEASE_LAYER}`);
  const manifestSha = (() => {
    try { return sha(fs.readFileSync(join(root, 'manifest.json'))); }
    catch { return ''; }
  })();
  const buildsSha = (() => {
    try { return sha(fs.readFileSync(join(root, 'builds.json'))); }
    catch { return ''; }
  })();
  check('builds:manifestSha256', builds.manifestSha256 === manifestSha, 'builds manifest digest mismatch');
  check('oracle:manifestSha256', oracle.manifestSha256 === manifestSha, 'oracle manifest digest mismatch');
  check('oracle:buildsSha256', oracle.buildsSha256 === buildsSha, 'oracle builds digest mismatch');
  for (const label of ['A', 'B']) for (const role of ['iai', 'controls', 'oracle', 'control']) {
    check(`binary:${label}:${role}`, identities[label]?.binaries?.[role] ===
      builds.arms?.[label]?.[role]?.binarySha256, 'binary pin mismatch');
  }
  for (const [key, role] of [['binarySha256', 'iai'], ['controlsBinarySha256', 'controls']])
    check(`binary:C:${role}`, identities.C0?.[key] === builds.arms?.C?.[role]?.binarySha256, 'C binary pin mismatch');
  // An A-only rehearsal measures A1 only (the judge's rowArms agree).
  const rowArms = rehearsal ? ['A1'] : ['A1', 'A2', 'B1', 'B2', 'C0'];
  const expected = new Set(CELLS.flatMap(cell => rowArms.map(arm => `${cell.name}/${arm}`)));
  const rows = input.iai;
  check('rows:array', Array.isArray(rows), 'input.iai must be an array');
  if (Array.isArray(rows)) {
    check('rows:cardinality', rows.length === expected.size, `expected ${expected.size} rows`);
    for (const row of rows) {
      const cell = CELLS.find(item => item.name === row.name);
      const id = `${row.name}/${row.arm}`;
      check(`row:${id}:known`, Boolean(cell) && expected.delete(id), 'unknown, duplicate, unexpected row');
      if (!cell) continue;
      const isC = row.arm === 'C0';
      const arm = row.arm.startsWith('B') ? 'B' : 'A';
      const role = cell.layer === 'L1' ? 'iai' : 'controls';
      const executableLabel = isC ? 'C' : arm;
      const binary = isC
        ? identities.C0?.[cell.layer === 'L1' ? 'binarySha256' : 'controlsBinarySha256']
        : identities[arm]?.binaries?.[role];
      const boundary = row.boundary ?? {};
      const activation = row.oracle;
      check(`row:${id}:binary`, row.binarySha256 === binary && boundary.binarySha256 === binary,
        'row executable pin mismatch');
      check(`row:${id}:dispatcher`, boundary.dispatcherGroup === cell.group &&
        boundary.dispatcherIndex === cell.index, 'dispatcher binding mismatch');
      const oracleArm = rehearsal || isC || row.arm.startsWith('A') ? 'A' : 'B';
      const oracleRecord = oracle.records?.[oracleArm]?.[row.name];
      check(`row:${id}:oracle`, isObject(oracleRecord?.row) &&
        Object.entries(activation ?? {}).filter(([key]) => key !== 'linkage').every(([key, value]) =>
          oracleRecord.row[key] === value), 'row counters differ from oracle record');
      for (const key of ['symbolReceipt', 'disassemblyReceipt', 'profileReceipt', 'logReceipt'])
        ref(boundary[key], `${id}.${key}`);
      ref(row.receipt, `${id}.debugReceipt`);
      const linkage = activation?.linkage;
      ref(linkage?.receipt, `${id}.oracleReceipt`);
      ref(linkage?.featureExactReceipt, `${id}.featureReceipt`);
      check(`row:${id}:linkage`, isObject(linkage) && linkage.primaryBinarySha256 === binary &&
        linkage.binarySha256 === oracleRecord?.binarySha256, 'oracle linkage mismatch');
      let inventory;
      try { inventory = JSON.parse(fs.readFileSync(join(root, boundary.profileReceipt), 'utf8')); }
      catch { inventory = null; }
      check(`row:${id}:profile-inventory`, Array.isArray(inventory) && inventory.length > 0,
        'raw profile receipt must be a nonempty array inventory');
      const profileTexts = [];
      if (Array.isArray(inventory)) for (const [index, entry] of inventory.entries()) {
        const path = entry?.receipt;
        const valid = isObject(entry) && safeRelative(path) && validHash(entry.sha256) && map?.[path] === entry.sha256;
        check(`row:${id}:profile:${index}`, valid, 'profile entry is unsafe or not hash-anchored');
        if (valid) {
          try { profileTexts.push(fs.readFileSync(resolve(root, path), 'utf8')); }
          catch { check(`row:${id}:profile-file:${index}`, false, 'profile file missing'); }
        }
      }
      const readText = (receipt, name) => {
        try { return fs.readFileSync(resolve(root, receipt), 'utf8'); }
        catch { check(`row:${id}:${name}`, false, 'raw receipt missing'); return ''; }
      };
      const log = readText(boundary.logReceipt, 'valgrind-log');
      const debug = readText(row.receipt, 'debug-log');
      const executable = builds.arms?.[executableLabel]?.[role]?.exe;
      check(`row:${id}:executable`, typeof executable === 'string' && executable.length > 0,
        'recorded executable path required');
      try {
        const parsed = parseProfiles(profileTexts, log, {
          cell, debug, homeName: boundary.homeName ?? `/${cell.name}/`,
          expectedExecutable: executable,
        });
        check(`row:${id}:parsed-boundary`, parsed.inclusiveIr === row.Ir &&
          parsed.inclusiveIr === boundary.inclusiveIr && parsed.executions === boundary.executions,
          'parsed inclusive Ir/executions mismatch');
        check(`row:${id}:caller-context`, boundary.callerContext === parsed.callerContext,
          'recorded incoming caller differs from parsed profile caller');
      } catch (error) { check(`row:${id}:callgraph`, false, error.message); }
      try {
        machineBoundary(readText(boundary.symbolReceipt, 'nm-receipt'),
          readText(boundary.disassemblyReceipt, 'disassembly-receipt'), cell.expectedSymbols);
      } catch (error) { check(`row:${id}:machine-boundary`, false, error.message); }
    }
    check('rows:complete', expected.size === 0, `missing rows: ${[...expected].slice(0, 5).join(', ')}`);
  }
  for (const name of FROZEN_FILES) {
    const item = input.hashes?.find(entry => entry.path === name);
    check(`frozen:${name}`, validHash(item?.sha256), 'frozen hash missing');
  }
  return { pass: failures.length === 0, checks, failures };
}

async function selfTest() {
  const root = fs.mkdtempSync(join(resolve('target'), 'r20p4-verifier-selftest-'));
  const hashes = {};
  const put = (path, value) => {
    const bytes = Buffer.isBuffer(value) ? value : Buffer.from(value);
    fs.mkdirSync(dirname(join(root, path)), { recursive: true });
    fs.writeFileSync(join(root, path), bytes);
    hashes[path] = sha(bytes);
    return path;
  };
  const json = value => JSON.stringify(value);
  const updateInput = input => fs.writeFileSync(join(root, 'input.json'), json(input));
  const canonicalSettings = [
    '--tool=callgrind', '--error-exitcode=0', '--trace-children=yes', '--fair-sched=try',
    '--I1=32768,8,64', '--D1=32768,8,64', '--LL=8388608,16,64', '--cache-sim=yes',
    '--compress-strings=no', '--compress-pos=no', '--dump-line=yes', '--dump-instr=no',
    '--combine-dumps=no', '--separate-threads=yes',
  ];
  const fixture = syntheticFixture();
  const binaryChars = { A: '3', B: '7', C: 'b' };
  const sourceA = fixture.identities.A.source;
    const sourceB = fixture.identities.B.source;
  const manifest = { arches: {
    A: {
      head: sourceA.baseHead,
      tree: 'fake-A-tree',
      snapshotSha256: sourceA.identitySha256,
      identitySha256: sourceA.identitySha256,
    },
    B: { head: sourceB.baseHead, identitySha256: sourceB.identitySha256 },
    C: {
      head: sourceA.baseHead,
      tree: 'fake-A-tree',
      snapshotSha256: sourceA.identitySha256,
      identitySha256: sourceA.identitySha256,
    },
  } };
  put('manifest.json', json(manifest));
  const binaries = {
    A: { iai: '4'.repeat(64), controls: '5'.repeat(64), oracle: '6'.repeat(64), control: '7'.repeat(64) },
    B: { iai: 'b'.repeat(64), controls: 'c'.repeat(64), oracle: 'd'.repeat(64), control: 'e'.repeat(64) },
    C: { iai: 'f'.repeat(64), controls: '0'.repeat(64), oracle: '6'.repeat(64), control: '7'.repeat(64) },
  };
  const builds = { arms: {} };
  for (const label of ['A', 'B', 'C']) {
    binaries[label] ??= {};
    builds.arms[label] = {};
    for (const role of ['iai', 'controls', 'oracle', 'control']) {
      builds.arms[label][role] = {
        binarySha256: binaries[label][role], exe: `<repo>/target/fake-${label}-${role}`,
      };
    }
  }
  builds.manifestSha256 = hashes['manifest.json'];
  put('builds.json', json(builds));
  const identities = {
    A: { source: sourceA, binaries: binaries.A, buildReceipt: 'build/A' },
    B: { source: sourceB, binaries: binaries.B, buildReceipt: 'build/B' },
    C0: { source: sourceA, binarySha256: binaries.C.iai,
      controlsBinarySha256: binaries.C.controls, buildReceipt: 'build/C' },
  };
  for (const [path, text] of [
    ['inventory', 'synthetic inventory'], ['environment', 'synthetic environment'],
    ['oracle-receipt', 'synthetic oracle receipt'], ['build/A', 'A build receipt'],
    ['build/B', 'B build receipt'], ['build/C', 'C build receipt'], ['ancestry', 'A ancestor B'],
  ]) put(path, text);
  const oracle = {
    manifestSha256: hashes['manifest.json'],
    buildsSha256: hashes['builds.json'],
    records: { A: {}, B: {} },
  };
  const rows = fixture.iai.map((base, index) => {
    const cell = CELLS.find(item => item.name === base.name);
    const isC = base.arm === 'C0';
    const arm = base.arm.startsWith('B') ? 'B' : 'A';
    const role = cell.layer === 'L1' ? 'iai' : 'controls';
    const executable = builds.arms[isC ? 'C' : arm][role].exe;
    const binary = isC ? identities.C0[cell.layer === 'L1' ? 'binarySha256' : 'controlsBinarySha256']
      : identities[arm].binaries[role];
    const sourceArm = isC ? 'A' : arm;
    const symbol = cell.expectedSymbols[0];
    const perCallIr = cell.layer === 'L1' ? base.Ir : base.Ir / cell.n;
    const executions = cell.layer === 'L1' ? 1 : cell.n;
    const total = base.Ir;
    const targetTotalIr = cell.layer === 'L1' ? total : executions * perCallIr;
    const caller = cell.layer === 'L1'
      ? `r20_prf04_heap_reuse_iai::heap_reuse::${cell.name}`
      : cell.family.includes('release')
        ? 'std::sys::thread_local::destructors::__call_tls_dtors'
        : 'r20_prf04_heap_reuse_l2_iai::hr::FirstAlloc<_>::alloc_inner';
    const claim = ['cold', 'start', 'recycled', 'recycled_alloc', 'wave'].includes(cell.family);
    const support = claim ? 'tls_heap::bind_slow_tagged' : 'HeapLeaseDrop::drop';
    const cmd = `${executable} --iai-run ${cell.group} ${cell.index} 0`;
    const callgrindArgs = [...canonicalSettings, '--collect-atstart=no',
      `--toggle-collect=${symbol}`, `--callgrind-out-file=/home/${cell.name}/callgrind.out`].join(' ');
    const debug = `callgrind: Arguments: ${callgrindArgs}\n`;
    const log = `==123== Command: ${cmd}\n`;
    const profiles = [];
    const threads = cell.layer === 'L1' ? [1] : [1, ...Array.from({ length: executions }, (_, i) => i + 2)];
    const descents = cell.layer === 'L2' && claim
      ? [`tls_heap::bind_slow_tagged`, `HeapRegistry::claim_impl`]
        : cell.family === 'recycled' || cell.family === 'recycled_alloc' ||
          cell.family === 'recycled_release' ? ['HeapRegistry::claim', 'HeapLease::drop']
          : claim ? ['HeapRegistry::claim_impl'] : [support];
    if (cell.layer === 'L2' && claim && !hasL2FirstBindPath(descents)) {
      throw new Error('synthetic L2 profile lacks actual TLS bind and registry-claim descendants');
    }
    for (const thread of threads) {
      const child = cell.layer === 'L2' && thread !== 1;
      const target = cell.layer === 'L1' || child;
      const calls = cell.layer === 'L1' ? 1 : child ? 1 : 0;
      const targetCost = target ? perCallIr : 0;
      const l2Claim = cell.layer === 'L2' && claim;
      const body = target
        ? [`fn=(1) ${caller}`, '1 0', `cfn=(2) ${symbol}`, `calls=${calls} 0`,
          `1 ${targetCost}`, `fn=(2) ${symbol}`, `1 ${targetCost - (l2Claim ? 1 : descents.length)}`,
          ...(l2Claim
            ? ['cfn=(3) tls_heap::bind_slow_tagged', 'calls=1 0', '1 1',
              'fn=(3) tls_heap::bind_slow_tagged', '1 0',
              'cfn=(4) HeapRegistry::claim_impl', 'calls=1 0', '1 1',
              'fn=(4) HeapRegistry::claim_impl', '1 1']
            : [...descents.flatMap((name, index) => [`cfn=(${index + 3}) ${name}`, 'calls=1 0', '1 1']),
              ...descents.flatMap((name, index) => [`fn=(${index + 3}) ${name}`, '1 1'])])].join('\n')
        : 'fn=(1) main\n1 0';
      const profileTotal = target ? targetCost : 0;
      const profile = [
        'version: 1', 'creator: Callgrind-3.22', 'pid: 123', 'part: 1',
        `thread: ${thread}`, `cmd: ${cmd}`, 'events: Ir', 'positions: line',
        body, `totals: ${profileTotal}`, '',
      ].join('\n');
      const path = put(`profiles/${index}-${thread}.out`, profile);
      profiles.push({ receipt: path, sha256: hashes[path] });
    }
    const profileReceipt = put(`profiles/${index}-inventory.json`, json(profiles));
    const logReceipt = put(`logs/${index}.log`, log);
    const debugReceipt = put(`raw/${index}-debug.log`, debug);
    const nm = [...new Set(cell.expectedSymbols)].map(name => `00000000 T ${name}`).join('\n') + '\n';
    const validCaller = cell.layer === 'L1' ? /r20_prf04_heap_reuse_iai|heap_reuse/.test(caller)
      : cell.family.includes('release')
        ? /__call_tls_dtors|drop_slow|drop_in_place|Drop/.test(caller)
        : isL2AllocCaller(caller);
    const callerAsm = `00000000 <${caller}>:\n   0: call 10 <${symbol}>\n   5: ret\n`;
    if (!validCaller) throw new Error('synthetic caller fixture mismatch');
    const targetAsm = [...new Set(cell.expectedSymbols)]
      .map(name => `00000010 <${name}>:\n  10: ret`).join('\n') + '\n';
    const symbolReceipt = put(`boundary/${index}-nm.log`, nm);
    const disassemblyReceipt = put(`boundary/${index}-disassembly.log`, callerAsm + targetAsm);
    const oracleArm = base.arm.startsWith('B') ? 'B' : 'A';
    const oracleBinary = binaries[oracleArm][cell.layer === 'L1' ? 'oracle' : 'control'];
    const activation = { ...base.oracle };
    delete activation.linkage;
    oracle.records[oracleArm][cell.name] = { row: activation, binarySha256: oracleBinary };
    const source = isC ? sourceA : arm === 'B' ? sourceB : sourceA;
    const oraclePin = { oracleCfg: oracleArm === 'A'
      ? ['prf04_heap_reuse_activation'] : ['prf04_heap_reuse_activation', 'prf04_heap_reuse_oracle'] };
    const linkage = {
      binarySha256: oracleBinary, primaryBinarySha256: binary,
      source, features: base.features, cfg: oraclePin.oracleCfg,
      environmentSha256: fixture.environmentSha256, receipt: 'oracle-receipt',
      featureExactReceipt: 'oracle-receipt',
      startedAt: '2026-10-10T01:30:00Z',
    };
    const settings = ['--collect-atstart=no', '--toggle-collect', symbol,
      '--callgrind-out-file=profile.out', '--separate-threads=yes'];
    return {
      name: base.name, arm: base.arm, Ir: total, binarySha256: binary, receipt: debugReceipt,
      source, environmentSha256: fixture.environmentSha256, features: base.features,
      startedAt: '2026-10-10T02:00:00Z', runId: `${base.arm}-${base.name}`,
      oracle: { ...activation, linkage },
      boundary: { inclusiveIr: total,
        executions, expectedExecutions: executions, symbol: cell.expectedSymbols,
        entryPoint: 'Custom', collectAtStart: false, defaultToggle: false, separateThreads: true,
        toggleCollect: symbol, dispatcherGroup: cell.group, dispatcherIndex: cell.index,
        callerContext: caller, settings, binarySha256: binary,
        symbolReceipt, disassemblyReceipt, profileReceipt, logReceipt, homeName: `/home/${cell.name}/` },
    };
  });
  put('oracle.json', json(oracle));
  oracle.buildsSha256 = hashes['builds.json'];
  put('oracle.json', json(oracle));
  const input = {
    identities, releaseLayer: RELEASE_LAYER, inventoryReceipt: 'inventory',
    environmentReceipt: 'environment', oracleReceipt: 'oracle-receipt', identityReviewReceipt: 'reviewed',
    ancestry: { chain: [sourceA.baseHead, sourceB.baseHead], receipt: 'ancestry' },
    hashes: FROZEN_FILES.map(path => ({ path, sha256: 'f'.repeat(64) })), iai: rows, receiptHashes: hashes,
  };
  put('input.json', json(input));
  const baseline = await verifyArtifacts(root);
  if (!baseline.pass) throw new Error(`realistic baseline rejected: ${JSON.stringify(baseline.failures.slice(0, 20))}`);
  const cases = [{ name: 'real-profile-baseline', pass: true }];
  const profilePath = 'profiles/0-1.out';
  const originalProfile = fs.readFileSync(join(root, profilePath));
  fs.writeFileSync(join(root, profilePath), Buffer.from(`${originalProfile.toString()}# mutated\n`));
  cases.push({ name: 'profile-bytes-corruption', pass: !(await verifyArtifacts(root)).pass });
  fs.writeFileSync(join(root, profilePath), originalProfile);
  const logPath = join(root, rows[0].boundary.logReceipt);
  const originalLog = fs.readFileSync(logPath);
  fs.rmSync(logPath);
  cases.push({ name: 'missing-raw-log', pass: !(await verifyArtifacts(root)).pass });
  fs.writeFileSync(logPath, originalLog);
  input.identities.B.source.identitySha256 = '9'.repeat(64);
  fs.writeFileSync(join(root, 'input.json'), json(input));
  cases.push({ name: 'identity-drift', pass: !(await verifyArtifacts(root)).pass });
  input.identities.B.source.identitySha256 = sourceB.identitySha256;
  fs.writeFileSync(join(root, 'input.json'), json(input));
  const profileInventory = JSON.parse(fs.readFileSync(join(root, rows[0].boundary.profileReceipt), 'utf8'));
  const firstProfile = join(root, profileInventory[0].receipt);
  const profileBytes = fs.readFileSync(firstProfile);
  fs.writeFileSync(firstProfile, Buffer.from(profileBytes.toString().replace(`totals: ${rows[0].Ir}`, 'totals: 1')));
  cases.push({ name: 'profile-total-corruption', pass: !(await verifyArtifacts(root)).pass });
  fs.writeFileSync(firstProfile, profileBytes);
  if (cases.some(item => !item.pass)) throw new Error(`negative case escaped: ${JSON.stringify(cases)}`);
  console.log(JSON.stringify({ status: 'PASS', cases, checks: baseline.checks.length }));
  fs.rmSync(root, { recursive: true, force: true });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--self-test') await selfTest();
  else if (args.length === 1) {
    const result = await verifyArtifacts(args[0]);
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.pass ? 0 : 1;
  } else process.exitCode = 2;
}
