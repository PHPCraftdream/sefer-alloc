// Read-only R20 PRF-04 deterministic judge, retaining PRF-01 v2 identity discipline.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  CELLS,
  ORACLE_COUNTERS,
  RELEASE_LAYER,
  BUDGETS,
  FROZEN_FILES,
  validateActivation,
  isL2AllocCaller,
} from './r20_prf04_heap_reuse_support.mjs';

const root = new URL('../', import.meta.url);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
export const hashes = () => FROZEN_FILES.map(path => ({
  path,
  sha256: sha(readFileSync(new URL(path, root))),
}));
export const hashText = () => hashes().map(item => `${item.sha256}  ${item.path}\n`).join('');
const ARMS = ['A1', 'A2', 'B1', 'B2', 'C0'];
const CELLS_BY_NAME = new Map(CELLS.map(cell => [cell.name, cell]));
const N = CELLS.find(cell => cell.n === 16).n;
const text = value => assert.ok(typeof value === 'string' && value.trim().length > 0);
const hex = value => assert.match(value, /^[0-9a-f]{64}$/);
const commit = value => assert.match(value, /^[0-9a-f]{40}$/);
const nat = value => assert.ok(Number.isSafeInteger(value) && value >= 0);
const same = (a, b) => assert.deepEqual(a, b);
const features = value => {
  assert.ok(Array.isArray(value));
  same([...value].sort(), ['bench-internals', 'internals', 'production']);
};
const time = value => {
  text(value);
  const parsed = Date.parse(value);
  assert.ok(Number.isFinite(parsed));
  return parsed;
};
const source = value => {
  commit(value.baseHead);
  for (const key of ['patchSha256', 'untrackedSha256', 'identitySha256']) hex(value[key]);
  time(value.capturedAt);
};
const abs = value => value < 0n ? -value : value;

function verdict(flags) {
  if (flags.incomplete) return 'INCOMPLETE';
  if (flags.invalid) return 'INVALID';
  if (flags.noGo) return 'NO-GO';
  if (flags.noisy) return 'INCONCLUSIVE';
  if (flags.unjudgeable) return 'UNJUDGEABLE';
  return 'GO (deterministic; native deferred)';
}

function validateFallback(receipt, sourcePin) {
  assert.ok(receipt && typeof receipt === 'object');
  assert.equal(receipt.reason, 'guard-drop-uncollectable');
  assert.equal(receipt.rehearsal, true);
  assert.equal(receipt.source?.baseHead, sourcePin.baseHead);
  assert.equal(receipt.source?.identitySha256, sourcePin.identitySha256);
  assert.ok(time(receipt.source?.capturedAt) < time(receipt.originalCapturedAt));
  for (const key of ['symbolReceipt', 'profileReceipt', 'rawLogReceipt']) text(receipt[key]);
}

function validateIdentity(d) {
  assert.equal(d.version, 1);
  hex(d.environmentSha256);
  same(d.inventory, CELLS.map(cell => cell.name));
  for (const key of ['inventoryReceipt', 'identityReviewReceipt', 'environmentReceipt',
    'oracleReceipt']) text(d[key]);
  if (d.releaseLayer !== RELEASE_LAYER) {
    throw Object.assign(
      new Error('frozen RELEASE_LAYER mismatch'),
      { invalid: true },
    );
  }
  assert.equal(d.nativeDeferred?.deferred, true);
  text(d.nativeDeferred.method);
  const pins = d.identities;
  for (const arm of ['A', 'B']) {
    const pin = pins[arm];
    assert.equal(pin.label, arm);
    source(pin.source);
    assert.equal(pin.measurementCommit, pin.source.baseHead);
    hex(pin.environmentSha256);
    assert.equal(pin.environmentSha256, d.environmentSha256);
    for (const role of ['iai', 'controls', 'oracle', 'control']) hex(pin.binaries[role]);
    assert.ok(time(pin.source.capturedAt) < time(pin.buildStartedAt));
    text(pin.buildReceipt);
    features(pin.features);
    same(pin.primaryCfg, []);
    const expectedOracleCfg = arm === 'A'
      ? ['prf04_heap_reuse_activation']
      : ['prf04_heap_reuse_activation', 'prf04_heap_reuse_oracle'];
    same(pin.oracleCfg, expectedOracleCfg);
    for (const key of ['rustc', 'target', 'profile', 'flags']) text(pin.toolchain[key]);
  }
  same(pins.A.toolchain, pins.B.toolchain);
  same(pins.C0.source, pins.A.source);
  hex(pins.C0.binarySha256);
  hex(pins.C0.controlsBinarySha256);
  text(pins.C0.buildReceipt);
  assert.ok(time(pins.C0.buildStartedAt) > time(pins.C0.source.capturedAt));
  assert.notEqual(pins.C0.binarySha256, pins.A.binaries.iai);
  assert.notEqual(pins.C0.controlsBinarySha256, pins.A.binaries.controls);
  if (d.rehearsal !== true) {
    assert.notEqual(pins.A.source.identitySha256, pins.B.source.identitySha256);
    assert.notEqual(pins.A.measurementCommit, pins.B.measurementCommit);
    assert.equal(d.ancestry.result, 'A ancestor B');
    same(d.ancestry.chain, [pins.A.source.baseHead, pins.B.source.baseHead]);
    text(d.ancestry.receipt);
  }
  if (RELEASE_LAYER === 'L1') validateFallback(d.fallbackReceipt, pins.A.source);
}

function validateSettings(settings, symbol) {
  assert.ok(Array.isArray(settings));
  const normalized = settings.flatMap((value, index) => {
    if (value === '--toggle-collect' && settings[index + 1]) return ['--toggle-collect=' + settings[index + 1]];
    if (value.startsWith('--toggle-collect=')) return [value];
    if (value === '--callgrind-out-file=profile.out' || value.startsWith('--callgrind-out-file=')) {
      return ['--callgrind-out-file'];
    }
    return [value];
  }).filter((value, index, values) => value !== '--toggle-collect' || values[index + 1] !== '--toggle-collect');
  assert.ok(normalized.includes('--collect-atstart=no'));
  assert.ok(normalized.includes('--toggle-collect=' + symbol));
  assert.ok(normalized.includes('--callgrind-out-file'));
  assert.ok(normalized.includes('--separate-threads=yes'));
}

function validateRows(d) {
  assert.ok(Array.isArray(d.iai));
  const rowArms = d.rehearsal === true ? ['A1'] : ARMS;
  const expected = new Set(CELLS.flatMap(cell => rowArms.map(arm => `${cell.name}/${arm}`)));
  assert.equal(d.iai.length, expected.size);
  const byCell = new Map();
  const runs = new Set();
  for (const row of d.iai) {
    const cell = CELLS_BY_NAME.get(row.name);
    assert.ok(cell, `unknown cell ${row.name}`);
    assert.ok(rowArms.includes(row.arm));
    const key = `${row.name}/${row.arm}`;
    assert.ok(expected.delete(key), `duplicate row ${key}`);
    nat(row.Ir);
    assert.ok(row.Ir > 0);
    hex(row.environmentSha256);
    assert.equal(row.environmentSha256, d.environmentSha256);
    const isC0 = row.arm === 'C0';
    const arm = row.arm.startsWith('B') ? 'B' : 'A';
    const pin = isC0 ? d.identities.C0 : d.identities[arm];
    const role = cell.layer === 'L1' ? 'iai' : 'controls';
    const binary = isC0
      ? (cell.layer === 'L1' ? pin.binarySha256 : pin.controlsBinarySha256)
      : d.identities[arm].binaries[role];
    assert.equal(row.binarySha256, binary);
    same(row.source, isC0 ? d.identities.A.source : pin.source);
    assert.ok(time(row.startedAt) > time(pin.buildStartedAt));
    features(row.features);
    text(row.receipt);
    const boundary = row.boundary;
    nat(boundary.inclusiveIr);
    assert.equal(boundary.inclusiveIr, row.Ir);
    const calls = cell.layer === 'L1' ? 1 : cell.n;
    assert.equal(boundary.executions, calls);
    assert.equal(boundary.expectedExecutions, calls);
    same(boundary.symbol, cell.expectedSymbols);
    assert.equal(boundary.entryPoint, 'Custom');
    assert.equal(boundary.collectAtStart, false);
    assert.equal(boundary.defaultToggle, false);
    assert.equal(boundary.separateThreads, true);
    assert.equal(boundary.toggleCollect, boundary.symbol[0]);
    assert.equal(boundary.dispatcherGroup, cell.group);
    assert.equal(boundary.dispatcherIndex, cell.index);
    text(boundary.callerContext);
    if (cell.layer === 'L2' && ['cold', 'recycled_alloc', 'wave'].includes(cell.family)) {
      assert.ok(isL2AllocCaller(boundary.callerContext),
        'L2 incoming caller must be the strict FirstAlloc forwarding wrapper');
    }
    validateSettings(boundary.settings, boundary.symbol[0]);
    for (const key of ['symbolReceipt', 'disassemblyReceipt', 'profileReceipt', 'logReceipt']) text(boundary[key]);
    assert.equal(boundary.binarySha256, binary);
    const oracle = row.oracle;
    assert.ok(oracle && typeof oracle === 'object');
    same(Object.keys(oracle).sort(), [...ORACLE_COUNTERS, 'observer', 'hint', 'bump', 'capScan', 'recovery',
      'highWaterDelta', 'actualTargetCalls', 'configConflictsDelta', 'linkage'].sort());
    const active = oracle.observer === true;
    if (!active) for (const counter of ORACLE_COUNTERS) assert.equal(oracle[counter], null);
    else {
      assert.ok(Number.isSafeInteger(oracle.heap_reuse_spill_bits_delta));
      nat(oracle.heap_reuse_take_hits);
    }
    for (const key of ['hint', 'bump', 'capScan', 'recovery', 'highWaterDelta', 'actualTargetCalls']) nat(oracle[key]);
    assert.equal(oracle.configConflictsDelta, 0);
    assert.equal(oracle.actualTargetCalls, calls);
    const linkage = oracle.linkage;
    assert.ok(linkage && typeof linkage === 'object');
    const oracleRole = cell.layer === 'L1' ? 'oracle' : 'control';
    const oraclePin = isC0 ? d.identities.A : d.identities[arm];
    assert.equal(linkage.binarySha256, oraclePin.binaries[oracleRole]);
    assert.equal(linkage.primaryBinarySha256, binary);
    same(linkage.source, row.source);
    features(linkage.features);
    same(linkage.cfg, oraclePin.oracleCfg);
    assert.equal(linkage.environmentSha256, d.environmentSha256);
    text(linkage.receipt);
    text(linkage.featureExactReceipt);
    assert.ok(time(linkage.startedAt) > time(oraclePin.buildStartedAt));
    const activation = { arm, family: cell.family, observer: oracle.observer, ...oracle };
    assert.ok(validateActivation(activation), `activation invalid ${key}`);
    const runId = row.runId ?? row.receipt;
    assert.ok(!runs.has(runId), `duplicate run id ${runId}`);
    runs.add(runId);
    if (!byCell.has(row.name)) byCell.set(row.name, []);
    byCell.get(row.name).push(row);
  }
  assert.equal(expected.size, 0);
  for (const rows of byCell.values()) rows.sort((a, b) => rowArms.indexOf(a.arm) - rowArms.indexOf(b.arm));
  return byCell;
}

function derive(d, frozen = hashes()) {
  const tables = { checks: [], windows: [], linearity: [], noise: [] };
  const flags = { incomplete: false, invalid: false, noGo: false, noisy: false, unjudgeable: false };
  try {
    same(d.hashes, frozen);
    validateIdentity(d);
    const byCell = validateRows(d);
    if (d.rehearsal === true) {
      return {
        version: 1,
        rehearsal: true,
        verdict: 'REHEARSAL: A-only evidence validated; no gate verdict',
        flags,
        tables,
      };
    }
    if (Object.hasOwn(d, 'counterfactual')) flags.invalid = true;
    judgeStart(byCell, flags, tables);
    judgeWave(byCell, flags, tables);
    judgeWindowFamilies(byCell, flags, tables);
    return { version: 1, verdict: verdict(flags), flags, tables,
      native: { status: 'DEFERRED', reason: d.nativeDeferred.method } };
  } catch (error) {
    if (error.invalid) flags.invalid = true;
    else flags.incomplete = true;
    const result = verdict(flags);
    return { version: 1, verdict: result, reason: error.stack ?? error.message,
      flags, tables };
  }
}

function recordNoise(label, values, flags, tables) {
  const [a1, a2, c0] = values.map(BigInt);
  const aa = abs(a1 - a2);
  const c0Delta = abs(c0 - a1);
  const pass = aa <= BigInt(BUDGETS.noiseAbsolute) && c0Delta <= BigInt(BUDGETS.noiseAbsolute);
  if (!pass) flags.noisy = true;
  tables.noise.push({ quantity: label, aa: aa.toString(), c0: c0Delta.toString(),
    ceiling: BUDGETS.noiseAbsolute, pass });
  return [aa, c0Delta].reduce((max, item) => item > max ? item : max, BigInt(BUDGETS.noiseAbsolute));
}

function judgeStart(byCell, flags, tables) {
  const cell = CELLS.find(item => item.family === 'start');
  const [a1, a2, b1, b2, c0] = byCell.get(cell.name);
  const ts = recordNoise('start', [a1.Ir, a2.Ir, c0.Ir], flags, tables);
  for (const a of [a1, a2]) for (const b of [b1, b2]) {
    const delta = BigInt(b.Ir) - BigInt(a.Ir);
    const pass = delta <= ts + BigInt(BUDGETS.startExtra) && 20n * delta <= BigInt(a.Ir);
    flags.noGo ||= !pass;
    tables.checks.push({ cell: cell.name, pair: `${b.arm}/${a.arm}`,
      delta: delta.toString(), pass });
  }
}

function windowValue(byCell, layer, family, arm, n, pairRecycled = false) {
  const selected = CELLS.filter(cell => cell.layer === layer && cell.n === n &&
    (pairRecycled ? cell.family === 'recycled_alloc' || cell.family === 'recycled_release' : cell.family === family));
  assert.ok(selected.length, `missing ${layer}/${family}/${n} window cells`);
  return selected.reduce((sum, cell) => {
    const row = byCell.get(cell.name).find(item => item.arm === arm);
    return sum + BigInt(row.Ir);
  }, 0n);
}

function judgeWindows(byCell, layer, family, allowancePerWork, flags, tables, pair = false, slopeOnly = false) {
  const available = CELLS.some(cell => cell.layer === layer &&
    (pair ? cell.family === 'recycled_alloc' : cell.family === family));
  if (!available) return;
  const measuredFamily = pair ? 'recycled_alloc' : family;
  const irAt = arm => [0, 16, 32, 64].map(n => n === 0 ? 0n : pair
    ? windowValue(byCell, layer, 'recycled_alloc', arm, n, true)
    : windowValue(byCell, layer, family, arm, n));
  const ir = Object.fromEntries(ARMS.map(arm => [arm, irAt(arm)]));
  const familyIsSlope = slopeOnly || family === 'recycled';
  const first = familyIsSlope ? 2 : 1;
  for (let window = first; window <= 3; window++) {
    const work = BigInt(window === 3 ? 32 : 16);
    const waves = Object.fromEntries(ARMS.map(arm => [arm, ir[arm][window] - ir[arm][window - 1]]));
    const ts = recordNoise(`${layer}/${family}/w${window}`, [waves.A1, waves.A2, waves.C0], flags, tables);
    for (const a of ['A1', 'A2']) for (const b of ['B1', 'B2']) {
      const base = waves[a];
      if (base <= 0n) { flags.invalid = true; continue; }
      const delta = waves[b] - base;
      const allowance = allowancePerWork + (layer === 'L2' ? BUDGETS.l2Extra : 0);
      const pass = delta <= BigInt(allowance) * work + ts && 20n * delta <= base;
      flags.noGo ||= !pass;
      tables.windows.push({ layer, family, window, pair: `${b}/${a}`, delta: delta.toString(),
        base: base.toString(), allowance, work: work.toString(), ts: ts.toString(), pass });
    }
  }
  if (familyIsSlope) {
    for (const arm of ARMS) {
      const slopeN = ir[arm][2] - ir[arm][1];
      const slope2N = ir[arm][3] - ir[arm][2];
      const deviation = abs(slope2N - 2n * slopeN);
      const qualifiedTs = recordNoise(`${layer}/${family}/linearity`,
        [ir.A1[3] - ir.A1[2], ir.A2[3] - ir.A2[2], ir.C0[3] - ir.C0[2]], flags, tables);
      const pass = deviation <= qualifiedTs;
      if (!pass) flags.invalid = true;
      tables.linearity.push({ layer, family, arm, deviation: deviation.toString(),
        bound: qualifiedTs.toString(), pass });
    }
  }
}

function judgeWave(byCell, flags, tables) {
  for (const layer of ['L1', 'L2']) {
    if (!CELLS.some(cell => cell.layer === layer && cell.family === 'wave')) continue;
    const family = 'wave';
    const values = Object.fromEntries(ARMS.map(arm => [arm, [0, 16, 32, 64].map(n => {
      if (n === 0) return 0n;
      const cell = CELLS.find(item => item.layer === layer && item.family === family && item.n === n);
      return BigInt(byCell.get(cell.name).find(row => row.arm === arm).Ir);
    })]));
    for (let window = 1; window <= 3; window++) {
      const waves = Object.fromEntries(ARMS.map(arm => [arm, values[arm][window] - values[arm][window - 1]]));
      recordNoise(`${layer}/wave/w${window}`, [waves.A1, waves.A2, waves.C0], flags, tables);
      for (const a of ['A1', 'A2']) for (const b of ['B1', 'B2']) {
        const delta = waves[b] - waves[a];
        const sampleN = window === 3 ? 64 : window === 2 ? 32 : 16;
        const sampleCell = CELLS.find(cell => cell.layer === layer &&
          cell.family === family && cell.n === sampleN);
        const sampleRow = byCell.get(sampleCell.name).find(row => row.arm === b);
        const pass = waves[b] < waves[a] && sampleRow.oracle.heap_reuse_take_hits > 0;
        flags.noGo ||= !pass;
        tables.checks.push({ cell: `${layer}/wave/w${window}`, pair: `${b}/${a}`,
          delta: delta.toString(), strictBenefit: true, pass });
      }
    }
  }
}

function judgeWindowFamilies(byCell, flags, tables) {
  for (const layer of ['L1', 'L2']) {
    judgeWindows(byCell, layer, 'cold', BUDGETS.cold, flags, tables);
    if (layer === 'L2') judgeWindows(byCell, layer, 'recycled', BUDGETS.recycledPair, flags, tables, true);
    else judgeWindows(byCell, layer, 'recycled', BUDGETS.recycledPair, flags, tables);
    if (layer === 'L2' && RELEASE_LAYER === 'L1') continue;
    judgeWindows(byCell, layer, 'wave_release', BUDGETS.spillRelease, flags, tables, false, true);
  }
}

export { derive };

export function syntheticFixture(frozen = hashes()) {
  const h = value => value.repeat(64);
  const head = value => value.repeat(40);
  const makeSource = value => ({ baseHead: head(value), patchSha256: h(value), untrackedSha256: h(value),
    identitySha256: h(value), capturedAt: '2026-10-10T00:00:00Z' });
  const env = h('e');
  const makePin = (label, ch) => ({
    label,
    source: makeSource(ch),
    measurementCommit: head(ch),
    environmentSha256: env,
    binaries: {
      iai: h(ch),
      controls: h(ch === 'a' ? 'b' : 'c'),
      oracle: h(ch === 'a' ? 'd' : 'e'),
      control: h(ch === 'a' ? 'f' : '1'),
    },
    buildStartedAt: '2026-10-10T01:00:00Z',
    buildReceipt: `build/${label}`,
    features: ['bench-internals', 'internals', 'production'],
    primaryCfg: [],
    oracleCfg: label === 'A'
      ? ['prf04_heap_reuse_activation']
      : ['prf04_heap_reuse_activation', 'prf04_heap_reuse_oracle'],
    toolchain: { rustc: 'synthetic', target: 'synthetic', profile: 'bench', flags: 'none' },
  });
  const identities = {
    A: makePin('A', 'a'),
    B: makePin('B', 'b'),
    C0: {
      source: makeSource('a'),
      binarySha256: h('2'),
      controlsBinarySha256: h('3'),
      buildStartedAt: '2026-10-10T01:00:00Z',
      buildReceipt: 'build/C0',
    },
  };
  const d = {
    version: 1,
    hashes: frozen,
    environmentSha256: env,
    inventory: CELLS.map(cell => cell.name),
    releaseLayer: RELEASE_LAYER,
    inventoryReceipt: 'inventory',
    identityReviewReceipt: 'review',
    environmentReceipt: 'environment',
    oracleReceipt: 'oracle',
    nativeDeferred: { deferred: true, method: 'native measurement deferred' },
    identities,
    ancestry: { result: 'A ancestor B', chain: [head('a'), head('b')], receipt: 'ancestry' },
    iai: [],
  };
  let serial = 0;
  for (const cell of CELLS) {
    for (const arm of ARMS) {
      const isB = arm.startsWith('B');
      const isC0 = arm === 'C0';
      const pin = isB ? identities.B : identities.A;
      const binary = isC0
        ? (cell.layer === 'L1' ? identities.C0.binarySha256 : identities.C0.controlsBinarySha256)
        : pin.binaries[cell.layer === 'L1' ? 'iai' : 'controls'];
      const calls = cell.layer === 'L1' ? 1 : cell.n;
      const waveBenefit = isB && cell.family === 'wave';
      const base = cell.family === 'start' ? 1000 : cell.n * 1000;
      const ir = base - (waveBenefit ? cell.n * 100 : 0);
      const observer = isB;
      const row = { name: cell.name, arm, Ir: ir, environmentSha256: env, binarySha256: binary,
        source: isC0 ? identities.A.source : pin.source, startedAt: '2026-10-10T02:00:00Z',
        features: pin.features, receipt: `logs/${arm}-${cell.name}.log`, runId: `run-${++serial}`,
        boundary: {
          inclusiveIr: ir, executions: calls, expectedExecutions: calls, symbol: cell.expectedSymbols,
          entryPoint: 'Custom', collectAtStart: false, defaultToggle: false, separateThreads: true,
          toggleCollect: cell.expectedSymbols[0], dispatcherGroup: cell.group, dispatcherIndex: cell.index,
          callerContext: cell.layer === 'L2' && ['cold', 'recycled_alloc', 'wave'].includes(cell.family)
            ? 'r20_prf04_heap_reuse_l2_iai::hr::FirstAlloc<_>::alloc_inner' : 'synthetic target caller',
          settings: ['--collect-atstart=no', '--toggle-collect', cell.expectedSymbols[0],
            '--callgrind-out-file=profile.out', '--separate-threads=yes'],
          symbolReceipt: 'nm.txt', disassemblyReceipt: 'disassembly.txt',
          profileReceipt: `profiles/${arm}-${cell.name}.json`, logReceipt: `logs/${arm}-${cell.name}.log`,
          binarySha256: binary,
        },
        oracle: {
          observer,
          heap_reuse_spill_bits_delta: observer
            ? (cell.family === 'wave' ? -Math.max(1, cell.n - 1) : Number(cell.family === 'wave_release'))
            : null,
          heap_reuse_take_hits: observer ? (waveBenefit ? 1 : 0) : null,
          hint: 0, bump: 0, capScan: 0, recovery: 0, highWaterDelta: 0,
          actualTargetCalls: calls, configConflictsDelta: 0,
          linkage: {
            binarySha256: (isC0 ? identities.A : pin).binaries[cell.layer === 'L1' ? 'oracle' : 'control'],
            primaryBinarySha256: binary, source: isC0 ? identities.A.source : pin.source,
            features: pin.features, cfg: pin.oracleCfg, environmentSha256: env,
            receipt: 'oracle.json', featureExactReceipt: 'oracle.json',
            startedAt: '2026-10-10T01:30:00Z',
          },
        },
      };
      if (cell.family.startsWith('recycled_')) {
        row.oracle.heap_reuse_take_hits = observer ? 0 : null;
      }
      d.iai.push(row);
    }
  }
  return d;
}

function selftest() {
  const frozen = [{ path: 'synthetic', sha256: 'a'.repeat(64) }];
  const fixture = syntheticFixture(frozen);
  let checks = 0;
  const check = (mutate, expected) => {
    const sample = structuredClone(fixture);
    mutate(sample);
    const actual = derive(sample, frozen).verdict;
    assert.equal(actual, expected, `mutant ${checks + 1}: expected ${expected}, got ${actual}`);
    checks++;
  };
  assert.equal(derive(fixture, frozen).verdict, 'GO (deterministic; native deferred)');
  checks++;
  check(sample => sample.inventory.push('unknown-cell'), 'INCOMPLETE');
  check(sample => { sample.iai[0].name = 'unknown-cell'; }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'A1');
    row.oracle.observer = true;
  }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'A1');
    row.binarySha256 = 'f'.repeat(64);
  }, 'INCOMPLETE');
  check(sample => { sample.releaseLayer = 'L1'; }, 'INVALID');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('wave_release'));
    row.oracle.heap_reuse_spill_bits_delta = 0;
  }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'A1');
    row.boundary.executions++;
  }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('_wave_n'));
    row.oracle.heap_reuse_take_hits = 0;
  }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('recycled_n'));
    row.oracle.heap_reuse_take_hits = 1;
  }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'A1' && item.name.includes('_l2_cold_n'));
    row.boundary.callerContext = 'std::thread::spawn::closure';
  }, 'INCOMPLETE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'A1' && item.name.includes('_l2_cold_n'));
    row.binarySha256 = sample.identities.A.binaries.iai;
  }, 'INCOMPLETE');
  check(sample => {
    const rows = sample.iai.filter(item => item.arm === 'A2' && item.name.includes('_cold_2n'));
    for (const row of rows) {
      row.Ir += 11;
      row.boundary.inclusiveIr = row.Ir;
    }
  }, 'INCONCLUSIVE');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('_l2_cold_n'));
    row.Ir += 400;
    row.boundary.inclusiveIr = row.Ir;
  }, 'NO-GO');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('_cold_n'));
    row.Ir -= 100;
    row.boundary.inclusiveIr = row.Ir;
  }, 'NO-GO');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('wave_2n'));
    row.oracle.heap_reuse_take_hits = 0;
  }, 'INCOMPLETE');
  check(sample => {
    for (const row of sample.iai.filter(item => item.arm === 'B1' && item.name.includes('_cold_'))) {
      const change = row.name.endsWith('_n') ? 74 : row.name.endsWith('_2n') ? 138 : 276;
      row.Ir += change;
      row.boundary.inclusiveIr = row.Ir;
    }
  }, 'GO (deterministic; native deferred)');
  check(sample => {
    for (const row of sample.iai.filter(item => item.arm === 'B1' && item.name.includes('_cold_'))) {
      row.Ir -= row.name.endsWith('_n') ? 16 : row.name.endsWith('_2n') ? 32 : 64;
      row.boundary.inclusiveIr = row.Ir;
    }
  }, 'GO (deterministic; native deferred)');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('_l2_cold_n'));
    row.Ir += 100;
    row.boundary.inclusiveIr = row.Ir;
  }, 'GO (deterministic; native deferred)');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'B1' && item.name.includes('recycled_4n'));
    row.Ir += 11;
    row.boundary.inclusiveIr = row.Ir;
  }, 'INVALID');
  check(sample => {
    const row = sample.iai.find(item => item.arm === 'A1' && item.name.includes('_wave_n'));
    row.oracle.heap_reuse_take_hits = 1;
  }, 'INCOMPLETE');
  const rehearsal = structuredClone(fixture);
  rehearsal.rehearsal = true;
  rehearsal.iai = rehearsal.iai.filter(row => row.arm === 'A1');
  const rehearsalResult = derive(rehearsal, frozen);
  assert.match(rehearsalResult.verdict, /^REHEARSAL:/);
  checks++;
  console.log(JSON.stringify({ version: 1, status: 'PASS', scope: 'synthetic only', checks, measurements: 0 }));
}

const args = process.argv.slice(2);
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    if (args.length === 1 && args[0] === '--hashes') process.stdout.write(hashText());
    else if (args.length === 1 && args[0] === '--self-test') selftest();
    else if (args.length === 1 && args[0] === '--check-hashes') {
      const frozenPath = new URL('docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md.sha256', root);
      assert.equal(readFileSync(frozenPath, 'utf8'), hashText());
      console.log(JSON.stringify({ version: 1, status: 'PASS', hashes: hashes().length }));
    } else {
      assert.ok(args.length === 2 && args[0] === '--input', 'expected --input FILE');
      const input = JSON.parse(readFileSync(resolve(args[1]), 'utf8'));
      const result = derive(input);
      console.log(JSON.stringify(result, null, 2));
      process.exitCode = result.verdict.startsWith('REHEARSAL:') ||
        result.verdict.startsWith('GO') ? 0 : 1;
    }
  } catch (error) {
    console.log(JSON.stringify({ version: 1, verdict: 'INCOMPLETE', reason: error.message }));
    process.exitCode = 1;
  }
}
