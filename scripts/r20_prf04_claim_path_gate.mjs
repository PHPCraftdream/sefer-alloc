// Read-only R20 PRF-04 claim-path deriver. No subprocess, build, measurement or file writes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = new URL('../', import.meta.url);
const files = ['docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md',
  'scripts/r20_prf04_claim_path_gate.mjs'];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const hashes = () => files.map(path => ({ path, sha256: sha(readFileSync(new URL(path, root))) }));
const hashText = () => hashes().map(x => `${x.sha256}  ${x.path}\n`).join('');
const arms = ['A1', 'A2', 'B1', 'B2', 'C0'];
const aArms = ['A1', 'A2'], bArms = ['B1', 'B2'];
const families = ['cold', 'recycled'];
const sizes = ['n', '2n', '4n'];
const N = 16; // PRF04_N, frozen before B existed
// cold is not affine in N (glibc calloc of the route directory, see the prereg): two disjoint windows.
const windowsOf = f => (f === 'cold' ? [['n', '2n'], ['2n', '4n']] : [['n', '2n']]);
const cellName = (f, s) => `r20_prf04_${f}_first_claim_${s}`;
const hex = x => assert.match(x, /^[0-9a-f]{64}$/);
const text = x => assert.ok(typeof x === 'string' && x.trim().length > 0);
const integer = x => { assert.ok(Number.isSafeInteger(x) && x >= 0); };
const pct = (num, den) => `${num < 0 ? '-' : '+'}${(Math.abs(num) / den * 100).toFixed(2)}% (numerator ${num}, denominator ${den})`;
function source(r) {
  for (const k of ['patchSha256', 'untrackedSha256', 'identitySha256']) hex(r.source[k]);
  assert.match(r.source.baseHead, /^[0-9a-f]{40}$/);
  const captured = Date.parse(r.source.capturedAt), started = Date.parse(r.startedAt);
  assert.ok(Number.isFinite(captured) && Number.isFinite(started) && captured < started);
  hex(r.binarySha256); text(r.receipt);
}
// One-sided owner budget as exact integer arithmetic: sB - sA <= 5%*sA <=> 20*(sB-sA) <= sA.
const claimPass = (sB, sA) => 20 * (sB - sA) <= sA;
function derive(d, frozen) {
  try {
    assert.equal(d.version, 1); assert.deepEqual(d.hashes, frozen);
    hex(d.environmentSha256); text(d.inventoryReceipt); text(d.oracleReceipt);
    let aSrc, bSrc, aBin, bBin, cBin;
    const byCell = new Map();
    for (const c of d.cells ?? []) {
      assert.ok(families.includes(c.family)); assert.ok(sizes.includes(c.size));
      assert.ok(arms.includes(c.arm)); assert.equal(c.name, cellName(c.family, c.size));
      assert.equal(c.environmentSha256, d.environmentSha256);
      source(c); integer(c.ir);
      const key = `${c.family}|${c.arm}|${c.size}`;
      assert.ok(!byCell.has(key), 'duplicate cell row');
      byCell.set(key, c);
      if (bArms.includes(c.arm)) {
        bSrc ??= c.source; assert.deepEqual(c.source, bSrc);
        bBin ??= c.binarySha256; assert.equal(c.binarySha256, bBin);
      } else {
        aSrc ??= c.source; assert.deepEqual(c.source, aSrc);
        if (c.arm === 'C0') { cBin ??= c.binarySha256; assert.equal(c.binarySha256, cBin); }
        else { aBin ??= c.binarySha256; assert.equal(c.binarySha256, aBin); }
      }
    }
    assert.ok(aSrc && bSrc, 'missing arm rows');
    assert.notEqual(aSrc.identitySha256, bSrc.identitySha256);
    assert.notEqual(aBin, bBin); assert.notEqual(aBin, cBin);
    // Optional mimalloc proxy pairs: PRF-03 exact-zero rule (none declared for this gate).
    let mimallocFailure = false;
    const mimallocRows = [];
    for (const name of new Set((d.mimalloc ?? []).map(m => m.name))) {
      const rows = arms.map(a => (d.mimalloc ?? []).filter(r => r.name === name && r.arm === a)[0]);
      assert.ok(rows.every(Boolean), `mimalloc cell ${name} lacks a 5-arm set`);
      for (const r of rows) { source(r); integer(r.ir); assert.equal(r.environmentSha256, d.environmentSha256); }
      for (const r of rows) {
        if (bArms.includes(r.arm)) { assert.deepEqual(r.source, bSrc); assert.equal(r.binarySha256, bBin); }
        else if (r.arm === 'C0') { assert.deepEqual(r.source, aSrc); assert.equal(r.binarySha256, cBin); }
        else { assert.deepEqual(r.source, aSrc); assert.equal(r.binarySha256, aBin); }
      }
      const aIr = rows[0].ir; // A1
      const deltas = rows.slice(1).map(r => r.ir - aIr);
      const pass = deltas.every(x => x === 0);
      mimallocFailure ||= !pass;
      mimallocRows.push({ name, A: aIr, deltas, pass });
    }
    const oracleCheck = (f, o) => {
      assert.ok(o, `missing oracle record for ${f}`);
      integer(o.bumpDelta); integer(o.reclaimDelta); integer(o.newHeaps);
      assert.ok([4, 5].includes(o.claimSourcesLen));
      assert.equal(o.missingFifthExplicitZero, o.claimSourcesLen < 5);
      if (f === 'cold') return o.bumpDelta === N && o.reclaimDelta === 0 && o.newHeaps === N;
      return o.reclaimDelta === N && o.bumpDelta === 0 && o.newHeaps === 0;
    };
    const tables = { families: [], mimalloc: mimallocRows };
    let incomplete = false, invalid = false, noGo = false, inconclusive = false;
    const unjudgeable = [];
    for (const f of families) {
      const row = { family: f, arms: {}, status: 'judged', checks: {} };
      if (arms.some(a => !byCell.has(`${f}|${a}|2n`))) {
        row.status = 'UNJUDGEABLE (no 2N sibling; excluded from verdict, never a pass)';
        unjudgeable.push(f); tables.families.push(row); continue;
      }
      if (!sizes.every(s => arms.every(a => byCell.has(`${f}|${a}|${s}`)))) {
        incomplete = true; row.status = 'INCOMPLETE (missing 4N cell; prereg requires 4N)';
        tables.families.push(row); continue;
      }
      const v = (a, s) => byCell.get(`${f}|${a}|${s}`).ir;
      const wins = windowsOf(f);
      const slopes = {}, konst = {}; // slopes[a] = window-0 slope s; wslopes[a][k] = window k
      const wslopes = {};
      for (const a of arms) {
        wslopes[a] = wins.map(([x, y]) => v(a, y) - v(a, x));
        slopes[a] = wslopes[a][0]; konst[a] = v(a, 'n') - slopes[a];
        for (const w of wslopes[a]) assert.ok(Number.isSafeInteger(w));
        row.arms[a] = { n: v(a, 'n'), '2n': v(a, '2n'), '4n': v(a, '4n'), slope: slopes[a],
          slope2: wslopes[a][1], construction: konst[a] };
      }
      // T_s per window (rule 2.2); window 0 is the reported T_s and the construction budget.
      const tS = wins.map((_, k) => Math.max(10, Math.abs(wslopes.A1[k] - wslopes.A2[k]), Math.abs(wslopes.C0[k] - wslopes.A1[k])));
      const tAA = wins.map((_, k) => Math.max(10, Math.abs(wslopes.C0[k] - wslopes.A1[k])));
      row.T_s = tS[0]; row.windows = wins.length;
      // A/A agreement per window (floor is C0-based; T_s already contains the A/A term by construction).
      row.checks.AA = wins.map((_, k) => {
        const dev = Math.abs(wslopes.A1[k] - wslopes.A2[k]);
        const pass = dev <= tAA[k];
        if (!pass) inconclusive = true;
        return { window: k, deviation: dev, floor: tAA[k], pass };
      });
      const aaOk = row.checks.AA.every(x => x.pass);
      // Linearity (rule 2.3) only for affine families; cold uses two windows instead (prereg).
      if (f === 'recycled') {
        const linFail = arms.filter(a => Math.abs((v(a, '4n') - v(a, '2n')) - 2 * slopes[a]) > tS[0]);
        row.checks.linearity = arms.map(a => ({ arm: a, deviation: Math.abs((v(a, '4n') - v(a, '2n')) - 2 * slopes[a]), bound: tS[0], pass: !linFail.includes(a) }));
        if (linFail.length) { invalid = true; row.status = 'INVALID (4N non-linearity)'; }
      }
      // Oracle.
      const o = (d.oracle ?? {})[f];
      const oOk = (() => { try { return oracleCheck(f, o); } catch { return false; } })();
      row.checks.oracle = { record: o ?? null, pass: oOk };
      if (!oOk) {
        if (!o) { incomplete = true; row.status = 'INCOMPLETE (missing oracle record)'; }
        else { invalid = true; row.status = row.status === 'judged' ? 'INVALID (oracle violation)' : `${row.status} + INVALID (oracle violation)`; }
      }
      // R-claim: one-sided 5% owner budget, per window and pair. sA <= 0 -> INCOMPLETE.
      const pairs = [];
      for (let k = 0; k < wins.length; k++) for (const a of aArms) {
        if (!(wslopes[a][k] > 0)) { incomplete = true; continue; }
        for (const b of bArms) {
          const num = wslopes[b][k] - wslopes[a][k], den = wslopes[a][k];
          const pass = claimPass(wslopes[b][k], wslopes[a][k]);
          pairs.push({ pair: `${b} vs ${a}`, window: k, numerator: num, denominator: den, percent: pct(num, den), budget: '+5% (owner, one-sided)', pass });
          if (!pass) noGo = true;
        }
      }
      row.checks.R_claim = pairs;
      // R-construct: one-sided, budget_K = T_s (window 0).
      const konstPairs = [];
      for (const a of aArms) for (const b of bArms) {
        const delta = konst[b] - konst[a];
        const pass = delta <= tS[0];
        konstPairs.push({ pair: `${b} vs ${a}`, K_B: konst[b], K_A: konst[a], delta, budget_K: tS[0], pass });
        if (!pass) noGo = true;
      }
      row.checks.R_construct = konstPairs;
      if (row.status === 'judged' && !aaOk) row.status = 'INCONCLUSIVE (wide A/A)';
      tables.families.push(row);
    }
    if (mimallocFailure) noGo = true;
    let verdict;
    if (incomplete) verdict = 'INCOMPLETE';
    else if (invalid) verdict = 'INVALID';
    else if (noGo) verdict = 'NO-GO';
    else if (inconclusive) verdict = 'INCONCLUSIVE';
    else if (unjudgeable.length) verdict = 'UNJUDGEABLE'; // an excluded family never yields GO
    else verdict = 'GO';
    return { verdict, unjudgeableFamilies: unjudgeable, tables };
  } catch (e) { return { verdict: 'INCOMPLETE', reason: e.message }; }
}
function render(r) {
  const out = [`verdict: ${r.verdict}`];
  if (r.reason) out.push(`reason: ${r.reason}`);
  if (r.unjudgeableFamilies?.length) out.push(`UNJUDGEABLE (excluded, never a pass): ${r.unjudgeableFamilies.join(', ')}`);
  for (const m of r.tables?.mimalloc ?? [])
    out.push(`| mimalloc | ${m.name} | A=${m.A} | deltas=${m.deltas} | pass=${m.pass} |`);
  for (const f of r.tables?.families ?? []) {
    out.push(`| family ${f.family} | status ${f.status} | T_s ${f.T_s ?? '-'} | windows ${f.windows ?? '-'} |`);
    for (const [a, x] of Object.entries(f.arms ?? {}))
      out.push(`|   ${a} | n=${x.n} 2n=${x['2n']} 4n=${x['4n']} | slope s=${x.slope} s2=${x.slope2} | K=${x.construction} |`);
    for (const c of f.checks?.R_claim ?? [])
      out.push(`|   R-claim ${c.pair} window ${c.window} | ${c.percent} | budget ${c.budget} | pass=${c.pass} |`);
    for (const c of f.checks?.R_construct ?? [])
      out.push(`|   R-construct ${c.pair} | K_B-K_A=${c.delta} | budget_K=${c.budget_K} | pass=${c.pass} |`);
    for (const c of f.checks?.AA ?? [])
      out.push(`|   A/A window ${c.window} | |s_A1-s_A2|=${c.deviation} | floor=${c.floor} | pass=${c.pass} |`);
    for (const c of f.checks?.linearity ?? [])
      out.push(`|   linearity ${c.arm} | dev=${c.deviation} | bound=${c.bound} | pass=${c.pass} |`);
    if (f.checks?.oracle) out.push(`|   oracle | pass=${f.checks.oracle.pass} | record=${JSON.stringify(f.checks.oracle.record)} |`);
  }
  return out.join('\n');
}
function selftest() {
  const h = 'a'.repeat(64);
  const frozen = [{ path: 'synthetic', sha256: h }];
  const src = id => ({ baseHead: 'a'.repeat(40), patchSha256: h, untrackedSha256: h,
    identitySha256: id.repeat(64), capturedAt: '2026-10-11T00:00:00Z' });
  const idOf = arm => bArms.includes(arm) ? 'b' : 'a';
  const binOf = arm => (arm === 'C0' ? 'c' : idOf(arm)).repeat(64);
  const build = () => {
    const d = { version: 1, hashes: frozen, environmentSha256: h,
      inventoryReceipt: 'synthetic', oracleReceipt: 'synthetic', cells: [], oracle: {
        cold: { bumpDelta: 16, reclaimDelta: 0, newHeaps: 16, claimSourcesLen: 4, missingFifthExplicitZero: true },
        recycled: { bumpDelta: 0, reclaimDelta: 16, newHeaps: 0, claimSourcesLen: 5, missingFifthExplicitZero: false } },
      mimalloc: [] };
    for (const f of families) for (const arm of arms) {
      // recycled is affine (slope 1000, K 3000); cold is deliberately NOT affine (windows 1000 and 3000).
      const ir = f === 'cold' ? { n: 4000, '2n': 5000, '4n': 8000 } : { n: 4000, '2n': 5000, '4n': 7000 };
      for (const s of sizes) d.cells.push({ name: cellName(f, s), family: f, size: s, arm,
        ir: ir[s], environmentSha256: h, binarySha256: binOf(arm), source: src(idOf(arm)),
        startedAt: '2026-10-11T01:00:00Z', receipt: 'synthetic' });
    }
    for (const arm of arms) d.mimalloc.push({ name: 'mimalloc_small_churn_16b', arm,
      ir: 9000, environmentSha256: h, binarySha256: binOf(arm), source: src(idOf(arm)),
      startedAt: '2026-10-11T01:00:00Z', receipt: 'synthetic' });
    return d;
  };
  assert.equal(derive(build(), frozen).verdict, 'GO');
  const mutate = (label, fn, expected = 'INCOMPLETE') => {
    const x = build(); fn(x); const r = derive(x, frozen);
    const got = typeof expected === 'string' ? r.verdict : expected(r);
    assert.equal(got, typeof expected === 'string' ? expected : true,
      `mutant [${label}] expected ${JSON.stringify(expected)}, got ${r.verdict}`);
    console.log(`  selftest mutant PASS: ${label}`);
  };
  mutate('B slope +4% of s_A stays inside one-sided budget',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'B1' && (c.size === '2n' || c.size === '4n'))
      .forEach(c => c.ir += c.size === '2n' ? 40 : 120),
    r => r.verdict === 'GO' && r.tables.families[0].checks.R_claim.every(p => p.pass));
  mutate('B slope +6% of s_A exceeds 5% budget',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'B1' && (c.size === '2n' || c.size === '4n'))
      .forEach(c => c.ir += c.size === '2n' ? 60 : 180), 'NO-GO');
  mutate('A1 vs A2 beyond C0-based floor',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'A2' && (c.size === '2n' || c.size === '4n'))
      .forEach(c => c.ir += c.size === '2n' ? 50 : 150), 'INCONCLUSIVE');
  mutate('A1 vs A2 beyond floor in window 2 only',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'A2' && c.size === '4n').forEach(c => c.ir += 50), 'INCONCLUSIVE');
  mutate('missing 2N sibling -> family UNJUDGEABLE, excluded, verdict is never GO',
    x => x.cells = x.cells.filter(c => !(c.family === 'cold' && c.size === '2n')),
    r => r.verdict === 'UNJUDGEABLE' && r.unjudgeableFamilies.includes('cold')
      && r.tables.families[0].status.startsWith('UNJUDGEABLE'));
  mutate('both families without 2N -> UNJUDGEABLE, never a vacuous GO',
    x => x.cells = x.cells.filter(c => c.size !== '2n'), 'UNJUDGEABLE');
  mutate('recycled 4N non-linearity', x => x.cells.filter(c => c.family === 'recycled' && c.size === '4n').forEach(c => c.ir = 8000), 'INVALID');
  mutate('cold is exempt from the affine test (its 4N is already non-affine in the fixture)', x => {}, r => r.verdict === 'GO');
  mutate('cold window-2-only B excess +6% of the window',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'B1' && c.size === '4n').forEach(c => c.ir += 180), 'NO-GO');
  mutate('cold window-2-only B excess +4% of the window stays inside',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'B1' && c.size === '4n').forEach(c => c.ir += 120),
    r => r.verdict === 'GO');
  mutate('B faster than A (negative excess) is never punished',
    x => x.cells.filter(c => c.family === 'recycled' && bArms.includes(c.arm)).forEach(c => c.ir -= { n: 100, '2n': 200, '4n': 400 }[c.size]),
    r => r.verdict === 'GO' && r.tables.families[1].checks.R_claim.some(p => p.percent.startsWith('-')));
  mutate('recycled oracle shows bump instead of reuse',
    x => x.oracle.recycled = { ...x.oracle.recycled, bumpDelta: 8, reclaimDelta: 0 }, 'INVALID');
  mutate('mimalloc delta = 1',
    x => x.mimalloc.filter(m => m.arm === 'B1').forEach(m => m.ir = 9001), 'NO-GO');
  mutate('K_B - K_A = 100 > budget_K = T_s = 10',
    x => x.cells.filter(c => c.family === 'cold' && c.arm === 'B1')
      .forEach(c => c.ir += 100), 'NO-GO');
  mutate('missing cold oracle record', x => delete x.oracle.cold, 'INCOMPLETE');
  mutate('missing 4N cell (2N present) -> data incomplete',
    x => x.cells = x.cells.filter(c => !(c.family === 'cold' && c.size === '4n')), 'INCOMPLETE');
  mutate('missing inventoryReceipt', x => delete x.inventoryReceipt);
  mutate('broken binary hash', x => x.cells[0].binarySha256 = 'd'.repeat(64));
  mutate('capturedAt not before startedAt', x => x.cells[0].source.capturedAt = x.cells[0].startedAt);
  mutate('B shares A source identity', x => x.cells.filter(c => bArms.includes(c.arm))
    .forEach(c => c.source.identitySha256 = x.cells[0].source.identitySha256));
  mutate('malformed claim_sources_len = 3', x => x.oracle.cold.claimSourcesLen = 3, 'INVALID');
  mutate('fifth missing but not flagged', x => x.oracle.recycled.missingFifthExplicitZero = true, 'INVALID');
  assert.equal(derive({}, frozen).verdict, 'INCOMPLETE');
  console.log('PASS: synthetic selftest only (base GO + negative controls + empty input); no measurement, no real GO');
}
const args = process.argv.slice(2);
try {
  if (args.length === 1 && args[0] === '--hashes') process.stdout.write(hashText());
  else if (args.length === 1 && ['--self-test', '--selftest'].includes(args[0])) selftest();
  else if (args.length === 1 && args[0] === '--check-hashes') {
    assert.equal(readFileSync(new URL('docs/perf/R20_PRF04_CLAIM_PATH_PREREG.sha256', root), 'utf8'), hashText());
    console.log('PASS: frozen document/script hashes');
  } else {
    assert.ok(args.length === 2 && args[0] === '--input', 'expected --input FILE; no data means INCOMPLETE');
    assert.equal(readFileSync(new URL('docs/perf/R20_PRF04_CLAIM_PATH_PREREG.sha256', root), 'utf8'), hashText());
    const result = derive(JSON.parse(readFileSync(resolve(args[1]), 'utf8')), hashes());
    console.log(render(result)); process.exitCode = result.verdict === 'GO' ? 0 : 1;
  }
} catch (e) { console.log(JSON.stringify({ verdict: 'INCOMPLETE', reason: e.message })); process.exitCode = 1; }
