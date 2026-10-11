// Read-only PRF03 deriver. No subprocess, build, measurement or file writes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = new URL('../', import.meta.url);
const files = ['docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.md',
  'scripts/r19_prf03_large_route_retain_gate.mjs'];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const hashes = () => files.map(path => ({ path, sha256: sha(readFileSync(new URL(path, root))) }));
const hashText = () => hashes().map(x => `${x.sha256}  ${x.path}\n`).join('');
const primary = ['bench-internals', 'internals', 'production'];
const paired = ['alloc-stats', ...primary];
const arms = ['A1', 'A2', 'B1', 'B2', 'C0'];
const hex = x => assert.match(x, /^[0-9a-f]{64}$/);
const text = x => assert.ok(typeof x === 'string' && x.trim().length > 0);
const number = (x, positive = false) => assert.ok(Number.isFinite(x) && x >= 0 && (!positive || x > 0));
const integer = x => { number(x); assert.ok(Number.isSafeInteger(x)); };
const features = (x, expected) => assert.deepEqual([...x].sort(), [...expected].sort());
const median = xs => { assert.equal(xs.length, 9); return [...xs].sort((a, b) => a - b)[4]; };
function config(c, boundary) {
  assert.ok(c.requested && c.resolved);
  for (const k of ['cacheBudget', 'cacheSlots', 'maintenance', 'workCount']) assert.ok(k in c.requested);
  assert.deepEqual(c.requested, c.resolved);
  assert.equal(c.configConflictsDelta, 0);
  assert.equal(c.processBoundary, boundary);
  text(c.runId); integer(c.pid); assert.ok(c.pid > 0);
}
function source(r) {
  for (const k of ['patchSha256', 'untrackedSha256', 'identitySha256']) hex(r.source[k]);
  assert.match(r.source.baseHead, /^[0-9a-f]{40}$/);
  const captured = Date.parse(r.source.capturedAt), started = Date.parse(r.startedAt);
  assert.ok(Number.isFinite(captured) && Number.isFinite(started) && captured < started);
  hex(r.binarySha256);
}
function derive(d, frozen) {
  try {
    assert.equal(d.version, 1); assert.deepEqual(d.hashes, frozen);
    hex(d.environmentSha256); text(d.inventoryReceipt); text(d.featureExactReceipt);
    const names = new Set(), kinds = new Set();
    for (const c of d.inventory) {
      text(c.name); assert.ok(!names.has(c.name)); names.add(c.name); kinds.add(c.kind);
      assert.ok(['R1', 'R2', 'R3', 'control', 'affected', 'mimalloc'].includes(c.kind));
    }
    for (const kind of ['R1', 'R2', 'R3', 'control', 'affected', 'mimalloc']) assert.ok(kinds.has(kind));
    // R1 is exactly the hit-only cell; prefill-only is its subtrahend, carried in the same row.
    const r1 = d.inventory.filter(c => c.kind === 'R1');
    assert.equal(r1.length, 1); assert.equal(r1[0].name, 'large_cache_hit_only_4mib');
    for (const name of ['small_churn_16b', 'churn_256b'])
      assert.ok(d.inventory.some(c => c.name === name && c.kind === 'control'));
    for (const prefix of ['alloc_magazine_', 'cold_alloc_only_'])
      assert.ok(d.inventory.some(c => c.name.startsWith(prefix) && c.kind === 'control'));
    for (const name of ['large_alloc_free_cycle', 'realloc_grow'])
      assert.ok(d.inventory.some(c => c.name === name && c.kind === 'affected'));
    assert.ok(d.inventory.some(c => c.name.startsWith('large_cache_free_slot_search_') && c.kind === 'affected'));
    for (const c of d.inventory) {
      if (/^(alloc_magazine_|cold_alloc_only_)/.test(c.name)) assert.equal(c.kind, 'control');
      if (/^large_cache_free_slot_search_/.test(c.name)) assert.equal(c.kind, 'affected');
    }
    assert.equal(d.iai.length, d.inventory.length * 5);
    let failure = false, noisy = false;
    const tables = { iai: [], native: [] };
    let aSource, bSource, aBinary, bBinary, cBinary;
    for (const c of d.inventory) {
      const rows = arms.map(arm => {
        const found = d.iai.filter(r => r.name === c.name && r.arm === arm);
        assert.equal(found.length, 1); return found[0];
      });
      for (const r of rows) {
        assert.equal(r.environmentSha256, d.environmentSha256);
        features(r.features, primary); features(r.oracleFeatures, paired);
        text(r.receipt); source(r); config(r.config, 'isolated-process');
        integer(r.hit);
        if (['R1', 'R2', 'R3'].includes(c.kind)) {
          integer(r.prefill); integer(r.routedScans);
          assert.equal(r.routedScans, rows[0].routedScans);
        } else { assert.equal(r.prefill, null); assert.equal(r.routedScans, null); }
        assert.deepEqual(r.config.requested, rows[0].config.requested);
        if (['R2', 'R3'].includes(c.kind)) assert.equal(r.activation, true);
        const isB = r.arm.startsWith('B');
        if (c.kind === 'R1') {
          assert.equal(r.oracle.tailHitDelta, 1);
          assert.equal(r.oracle.incarnationDelta, isB ? 0 : 1);
          assert.equal(r.oracle.cachedCensus, isB ? 1 : 0);
        }
        if (isB) {
          bSource ??= r.source; bBinary ??= r.binarySha256;
          assert.deepEqual(r.source, bSource); assert.equal(r.binarySha256, bBinary);
        } else {
          aSource ??= r.source; assert.deepEqual(r.source, aSource);
          if (r.arm === 'C0') { cBinary ??= r.binarySha256; assert.equal(r.binarySha256, cBinary); }
          else { aBinary ??= r.binarySha256; assert.equal(r.binarySha256, aBinary); }
        }
      }
      const values = rows.map(r => r.hit - (r.prefill ?? 0));
      const [a1, a2, b1, b2, c0] = values;
      const t = Math.max(10, Math.abs(c0 - a1));
      assert.ok(Number.isSafeInteger(t));
      noisy ||= Math.abs(a2 - a1) > t;
      const deltas = [b1 - a1, b1 - a2, b2 - a1, b2 - a2];
      assert.ok(deltas.every(Number.isSafeInteger));
      const pass = deltas.every(delta => c.kind === 'R1' ? delta <= -200
        : ['R2', 'R3'].includes(c.kind) ? delta <= t
          : c.kind === 'control' ? Math.abs(delta) <= t
            : c.kind === 'mimalloc' ? delta === 0 : true);
      failure ||= !pass;
      assert.equal(t, Math.max(10, Math.abs(c0 - a1)));
      tables.iai.push({ name: c.name, kind: c.kind, A1: a1, A2: a2, B1: b1, B2: b2, C0: c0, T: t, deltas, pass });
    }
    assert.notEqual(aBinary, cBinary); assert.notEqual(aSource.identitySha256, bSource.identitySha256);
    assert.equal(d.native.length, 81);
    const runIds = new Set();
    let nativeA, nativeB;
    for (const n of [1, 4, 8]) {
      const groups = ['A', 'AA', 'B'].map(arm => {
        const rs = d.native.filter(r => r.n === n && r.arm === arm);
        assert.equal(rs.length, 9);
        for (const r of rs) {
          text(r.receipt);
          assert.equal(r.environmentSha256, d.environmentSha256);
          features(r.features, ['production']); source(r); config(r.config, 'isolated-process');
          assert.deepEqual(r.source, arm === 'B' ? bSource : aSource);
          if (arm === 'B') { nativeB ??= r.binarySha256; assert.equal(r.binarySha256, nativeB); }
          else { nativeA ??= r.binarySha256; assert.equal(r.binarySha256, nativeA); }
          assert.ok(!runIds.has(r.config.runId)); runIds.add(r.config.runId);
          assert.deepEqual(r.config.requested, rs[0].config.requested);
          number(r.nsPerOp, true); number(r.p99Ns, true);
        }
        return rs;
      });
      for (const rs of groups) assert.deepEqual(rs[0].config.requested, groups[0][0].config.requested);
      for (const metric of ['nsPerOp', 'p99Ns']) {
        const [a, aa, b] = groups.map(rs => median(rs.map(r => r[metric])));
        const noise = Math.abs(aa - a) / a;
        const pass = b <= a * (1 + noise);
        noisy ||= noise > 0.05; failure ||= !pass;
        tables.native.push({ n, metric, A: a, AA: aa, B: b, noise, maxNoise: 0.05, pass });
      }
    }
    return { verdict: noisy ? 'INCONCLUSIVE' : failure ? 'NO-GO' : 'GO', tables };
  } catch (e) { return { verdict: 'INCOMPLETE', reason: e.message }; }
}
function selftest() {
  const h = 'a'.repeat(64), frozen = [{ path: 'synthetic', sha256: h }];
  const src = x => ({ baseHead: 'a'.repeat(40), patchSha256: h, untrackedSha256: h,
    identitySha256: x.repeat(64), capturedAt: '2026-10-11T00:00:00Z' });
  const inventory = [['large_cache_hit_only_4mib', 'R1'], ['R2', 'R2'], ['R3', 'R3'],
    ['small_churn_16b', 'control'], ['churn_256b', 'control'], ['alloc_magazine_x', 'control'],
    ['cold_alloc_only_x', 'control'], ['large_alloc_free_cycle', 'affected'],
    ['realloc_grow', 'affected'], ['large_cache_free_slot_search_x', 'affected'], ['mimalloc', 'mimalloc']]
    .map(([name, kind]) => ({ name, kind }));
  let id = 1;
  const cfg = () => ({ requested: { cacheBudget: 1, cacheSlots: 8, maintenance: false, workCount: 1 },
    resolved: { cacheBudget: 1, cacheSlots: 8, maintenance: false, workCount: 1 },
    configConflictsDelta: 0, processBoundary: 'isolated-process', runId: String(id++), pid: id });
  const row = b => ({ environmentSha256: h, source: src(b ? 'b' : 'a'), binarySha256: (b ? 'b' : 'a').repeat(64),
    startedAt: '2026-10-11T01:00:00Z', config: cfg() });
  const d = { version: 1, hashes: frozen, environmentSha256: h, inventoryReceipt: 'synthetic',
    featureExactReceipt: 'synthetic', inventory, iai: [], native: [] };
  for (const c of inventory) for (const arm of arms) {
    const b = arm.startsWith('B');
    const regime = ['R1', 'R2', 'R3'].includes(c.kind);
    d.iai.push({ ...row(b), name: c.name, arm, features: primary, oracleFeatures: paired,
      receipt: 'synthetic', binarySha256: (arm === 'C0' ? 'c' : b ? 'b' : 'a').repeat(64),
      hit: b && c.kind === 'R1' ? 800 : 1000, prefill: regime ? 0 : null, routedScans: regime ? 0 : null, activation: true,
      oracle: { tailHitDelta: 1, incarnationDelta: b ? 0 : 1, cachedCensus: b ? 1 : 0 } });
  }
  for (const n of [1, 4, 8]) for (const arm of ['A', 'AA', 'B']) for (let i = 0; i < 9; i++)
    d.native.push({ ...row(arm === 'B'), receipt: 'synthetic', n, arm, features: ['production'], nsPerOp: 100, p99Ns: 100 });
  assert.equal(derive(d, frozen).verdict, 'GO');
  const mutate = (fn, expected = 'INCOMPLETE') => {
    const x = structuredClone(d); fn(x); assert.equal(derive(x, frozen).verdict, expected);
  };
  mutate(x => delete x.featureExactReceipt);
  mutate(x => x.iai[0].features.push('alloc-stats'));
  mutate(x => x.iai[0].oracle.tailHitDelta = 0);
  mutate(x => x.iai[1].source.identitySha256 = 'd'.repeat(64));
  mutate(x => x.iai[1].binarySha256 = 'd'.repeat(64));
  mutate(x => x.iai[0].config.configConflictsDelta = 1);
  mutate(x => x.native.pop());
  mutate(x => x.iai[0].environmentSha256 = 'd'.repeat(64));
  mutate(x => x.iai[0].config.resolved.cacheSlots = 9);
  mutate(x => x.iai[0].source.capturedAt = x.iai[0].startedAt);
  mutate(x => x.native[0].config.runId = x.native[1].config.runId);
  mutate(x => x.iai[0].hit = NaN);
  mutate(x => x.iai[15].routedScans = 0);
  mutate(x => x.iai[15].prefill = 0);
  mutate(x => x.iai[0].routedScans = null);
  mutate(x => x.iai[0].prefill = null);
  mutate(x => x.iai[2].routedScans = 1);
  mutate(x => x.inventory.pop());
  mutate(x => x.iai[2].hit = 801, 'NO-GO');
  mutate(x => x.iai[1].hit = 1011, 'INCONCLUSIVE');
  mutate(x => x.native.filter(r => r.arm === 'AA').forEach(r => r.nsPerOp = 106), 'INCONCLUSIVE');
  mutate(x => x.native.filter(r => r.arm === 'B').forEach(r => r.p99Ns = 101), 'NO-GO');
  assert.equal(derive({}, frozen).verdict, 'INCOMPLETE');
  console.log('PASS: synthetic selftest only; no measurement or real GO');
}
const args = process.argv.slice(2);
try {
  if (args.length === 1 && args[0] === '--hashes') process.stdout.write(hashText());
  else if (args.length === 1 && args[0] === '--check-hashes') {
    assert.equal(readFileSync(new URL('docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.sha256', root), 'utf8'), hashText());
    console.log('PASS: frozen document/script hashes');
  } else if (args.length === 1 && args[0] === '--selftest') selftest();
  else {
    assert.ok(args.length === 2 && args[0] === '--input', 'expected --input FILE; no data means INCOMPLETE');
    assert.equal(readFileSync(new URL('docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.sha256', root), 'utf8'), hashText());
    const result = derive(JSON.parse(readFileSync(resolve(args[1]), 'utf8')), hashes());
    console.log(JSON.stringify(result, null, 2)); process.exitCode = result.verdict === 'GO' ? 0 : 1;
  }
} catch (e) { console.log(JSON.stringify({ verdict: 'INCOMPLETE', reason: e.message })); process.exitCode = 1; }
