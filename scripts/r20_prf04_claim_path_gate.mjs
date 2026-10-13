// Read-only R20 PRF-04 claim-path deriver. No subprocess, build, measurement or file writes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = new URL('../', import.meta.url);
// Frozen inputs: the prereg, this judge, the runner, the rule, the cell bodies, the oracle and the A-only evidence.
const files = ['docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md', 'scripts/r20_prf04_claim_path_gate.mjs',
  'scripts/r20_prf04_claim_path_run.mjs', 'docs/perf/R20_CONTROL_GEOMETRY_RULE.md',
  'benches/r20_prf04_claim_iai.rs', 'benches/support/r20_prf04.rs', 'examples/r20_prf04_claim_path_oracle.rs',
  'docs/perf/_raw_r20_prf04_harness_characterization.log', 'docs/perf/_raw_r20_prf04_cold_profile.log'];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const hashes = () => files.map(path => ({ path, sha256: sha(readFileSync(new URL(path, root))) }));
const hashText = () => hashes().map(x => `${x.sha256}  ${x.path}\n`).join('');
const arms = ['A1', 'A2', 'B1', 'B2', 'C0'];
const aArms = ['A1', 'A2'], bArms = ['B1', 'B2'];
const families = ['cold', 'recycled', 'wave'];
const sizes = ['n', '2n', '4n'];
const COUNT = { n: 16, '2n': 32, '4n': 64 }; // PRF04_N, frozen before B existed
// cold and wave are not affine in N (host-allocator pattern, see the prereg): three windows, the first one
// being the whole body of the N cell (claims 1..N plus the constant), so no claim range is left unjudged.
const windowsOf = f => (f === 'recycled' ? [['n', '2n']] : [['0', 'n'], ['n', '2n'], ['2n', '4n']]);
const cellName = (f, s) => (f === 'start' ? 'r20_prf04_start_first_claim' : `r20_prf04_${f}_first_claim_${s}`);
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
const claimPass = (sB, sA) => 20n * (BigInt(sB) - BigInt(sA)) <= BigInt(sA);
// Layout-noise ceiling (one tenth of the owner's 5 % budget): numerator |A1-A2| or |C0-A1|, denominator the A1 value.
const noiseCeiling = a1 => Math.max(10, Math.floor(a1 / 200));
function pathOk(f, arm, o) {
  const n = f === 'start' ? 1 : o.n;
  if (f === 'start' || f === 'cold') return o.bumpDelta === n && o.reclaimDelta === 0 && o.newHeaps === n;
  if (f === 'recycled') return o.reclaimDelta === n && o.hintDelta === n && o.bumpDelta === 0 && o.newHeaps === 0;
  if (arm === 'A') return o.hintDelta === 1 && o.bumpDelta === n - 1 && o.freeSetDelta === 0 && o.newHeaps === n - 1;
  return o.hintDelta === 1 && o.freeSetDelta === n - 1 && o.bumpDelta === 0 && o.newHeaps === 0;
}
function derive(d, frozen) {
  try {
    assert.equal(d.version, 1); assert.deepEqual(d.hashes, frozen);
    hex(d.environmentSha256); text(d.inventoryReceipt); text(d.oracleReceipt);
    let aSrc, bSrc, aBin, bBin, cBin;
    const byCell = new Map();
    for (const c of d.cells ?? []) {
      assert.ok(['start', ...families].includes(c.family)); assert.ok(sizes.includes(c.size));
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
    // Per-arm path oracle: every (arm, family, size) record is bound to its arm's source identity.
    const oracleState = f => {
      const bad = [], missing = [];
      for (const arm of ['A', 'B']) for (const s of (f === 'start' ? ['n'] : sizes)) {
        const o = d.oracle?.[arm]?.[f]?.[s];
        if (!o) { missing.push(`${arm}/${f}/${s}`); continue; }
        try {
          for (const k of ['bumpDelta', 'hintDelta', 'reclaimDelta', 'newHeaps', 'freeSetDelta', 'claimSourcesLen', 'n']) integer(o[k]);
          assert.equal(o.arm, arm); hex(o.binarySha256); assert.deepEqual(o.source, arm === 'A' ? aSrc : bSrc);
          assert.equal(o.environmentSha256, d.environmentSha256);
          assert.equal(o.claimSourcesLen, arm === 'B' ? 5 : 4);
          assert.equal(o.missingFifthExplicitZero, o.claimSourcesLen < 5);
          assert.equal(o.n, f === 'start' ? 1 : COUNT[s]);
        } catch { missing.push(`${arm}/${f}/${s} (malformed or unbound)`); continue; }
        if (!pathOk(f, arm, o)) bad.push(`${arm}/${f}/${s}`);
      }
      return { bad, missing };
    };
    const tables = { families: [], mimalloc: mimallocRows };
    let incomplete = false, invalid = false, noGo = false, inconclusive = false;
    const unjudgeable = [];
    // Construction: the direct start cell (bootstrap + first claim), one-sided, budget_K = T_s of that cell.
    {
      const row = { family: 'start (construction)', arms: {}, status: 'judged', checks: {} };
      if (arms.some(a => !byCell.has(`start|${a}|n`))) {
        row.status = 'UNJUDGEABLE (no start cell; construction excluded, never a pass)';
        unjudgeable.push('start');
      } else {
        const v = a => byCell.get(`start|${a}|n`).ir;
        for (const a of arms) row.arms[a] = { n: v(a) };
        const ceil = noiseCeiling(v('A1'));
        const aa = Math.abs(v('A1') - v('A2')), c0 = Math.abs(v('C0') - v('A1'));
        const tS = Math.max(10, aa, c0);
        row.T_s = tS;
        row.checks.noise = { aa, c0, ceiling: ceil, pass: aa <= ceil && c0 <= ceil };
        if (!row.checks.noise.pass) { inconclusive = true; row.status = 'INCONCLUSIVE (layout noise above the ceiling)'; }
        row.checks.R_construct = [];
        for (const a of aArms) for (const b of bArms) {
          const delta = v(b) - v(a), pass = delta <= tS;
          row.checks.R_construct.push({ pair: `${b} vs ${a}`, delta, budget_K: tS, pass });
          if (!pass) noGo = true;
        }
        const st = oracleState('start');
        row.checks.oracle = { ...st, pass: !st.bad.length && !st.missing.length };
        if (st.missing.length) { incomplete = true; row.status = 'INCOMPLETE (oracle)'; }
        else if (st.bad.length) { invalid = true; row.status = 'INVALID (oracle violation)'; }
      }
      tables.families.push(row);
    }
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
      const v = (a, s) => (s === '0' ? 0 : byCell.get(`${f}|${a}|${s}`).ir);
      const wins = windowsOf(f);
      const ws = {}; // ws[arm][k] = window k value
      for (const a of arms) {
        ws[a] = wins.map(([x, y]) => v(a, y) - v(a, x));
        for (const w of ws[a]) assert.ok(Number.isSafeInteger(w));
        row.arms[a] = { n: v(a, 'n'), '2n': v(a, '2n'), '4n': v(a, '4n'), windows: ws[a] };
      }
      // T_s per window (rule 2.2) and the layout-noise ceiling per window.
      const tS = wins.map((_, k) => Math.max(10, Math.abs(ws.A1[k] - ws.A2[k]), Math.abs(ws.C0[k] - ws.A1[k])));
      row.T_s = tS[0]; row.windows = wins.length;
      row.checks.noise = wins.map((_, k) => {
        if (!(ws.A1[k] > 0)) { incomplete = true; return { window: k, pass: false, reason: 'non-positive A1 window' }; }
        const ceiling = noiseCeiling(ws.A1[k]);
        const aa = Math.abs(ws.A1[k] - ws.A2[k]), c0 = Math.abs(ws.C0[k] - ws.A1[k]);
        const pass = aa <= ceiling && c0 <= ceiling;
        if (!pass) inconclusive = true;
        return { window: k, aa, c0, ceiling, pass };
      });
      const noiseOk = row.checks.noise.every(x => x.pass);
      // Linearity (rule 2.3) only for the affine family; cold and wave use two windows (rule 3a).
      if (f === 'recycled') {
        const dev = a => Math.abs((v(a, '4n') - v(a, '2n')) - 2 * ws[a][0]);
        row.checks.linearity = arms.map(a => ({ arm: a, deviation: dev(a), bound: tS[0], pass: dev(a) <= tS[0] }));
        if (row.checks.linearity.some(x => !x.pass)) { invalid = true; row.status = 'INVALID (4N non-linearity)'; }
      }
      const st = oracleState(f);
      row.checks.oracle = { ...st, pass: !st.bad.length && !st.missing.length };
      if (st.missing.length) { incomplete = true; row.status = 'INCOMPLETE (oracle)'; }
      else if (st.bad.length) { invalid = true; row.status = row.status === 'judged' ? 'INVALID (oracle violation)' : `${row.status} + INVALID (oracle violation)`; }
      // R-claim: one-sided 5% owner budget, per window and pair. A non-positive A window -> INCOMPLETE.
      const pairs = [];
      for (let k = 0; k < wins.length; k++) for (const a of aArms) {
        if (!(ws[a][k] > 0)) { incomplete = true; continue; }
        for (const b of bArms) {
          const num = ws[b][k] - ws[a][k], den = ws[a][k];
          const pass = claimPass(ws[b][k], ws[a][k]);
          pairs.push({ pair: `${b} vs ${a}`, window: k, numerator: num, denominator: den, percent: pct(num, den), budget: '+5% (owner, one-sided)', pass });
          if (!pass) noGo = true;
        }
      }
      row.checks.R_claim = pairs;
      if (row.status === 'judged' && !noiseOk) row.status = 'INCONCLUSIVE (layout noise above the ceiling)';
      tables.families.push(row);
    }
    if (mimallocFailure) noGo = true;
    let verdict;
    if (incomplete) verdict = 'INCOMPLETE';
    else if (invalid) verdict = 'INVALID';
    else if (noGo) verdict = 'NO-GO';
    else if (inconclusive) verdict = 'INCONCLUSIVE';
    else if (unjudgeable.length) verdict = 'UNJUDGEABLE'; // an excluded family or cell never yields GO
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
      out.push(`|   ${a} | n=${x.n} 2n=${x['2n'] ?? '-'} 4n=${x['4n'] ?? '-'} | windows=${x.windows ?? '-'} |`);
    for (const c of f.checks?.R_claim ?? [])
      out.push(`|   R-claim ${c.pair} window ${c.window} | ${c.percent} | budget ${c.budget} | pass=${c.pass} |`);
    for (const c of f.checks?.R_construct ?? [])
      out.push(`|   R-construct ${c.pair} | Ir_B-Ir_A=${c.delta} | budget_K=${c.budget_K} | pass=${c.pass} |`);
    for (const c of [].concat(f.checks?.noise ?? []))
      out.push(`|   noise${c.window === undefined ? '' : ` window ${c.window}`} | |A1-A2|=${c.aa} |C0-A1|=${c.c0} | ceiling=${c.ceiling} (0.5% of A1, floor 10) | pass=${c.pass} |`);
    for (const c of f.checks?.linearity ?? [])
      out.push(`|   linearity ${c.arm} | dev=${c.deviation} | bound=${c.bound} | pass=${c.pass} |`);
    if (f.checks?.oracle) out.push(`|   oracle | pass=${f.checks.oracle.pass} | bad=${f.checks.oracle.bad} | missing=${f.checks.oracle.missing} |`);
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
  const orec = (arm, f, s) => {
    const n = f === 'start' ? 1 : COUNT[s];
    const base = { n, arm, binarySha256: (arm === 'A' ? 'e' : 'f').repeat(64), source: src(arm === 'A' ? 'a' : 'b'),
      environmentSha256: h, claimSourcesLen: arm === 'B' ? 5 : 4, missingFifthExplicitZero: arm === 'A',
      bumpDelta: 0, hintDelta: 0, reclaimDelta: 0, newHeaps: 0, freeSetDelta: 0 };
    if (f === 'start' || f === 'cold') Object.assign(base, { bumpDelta: n, newHeaps: n });
    else if (f === 'recycled') Object.assign(base, { reclaimDelta: n, hintDelta: n });
    else if (arm === 'A') Object.assign(base, { hintDelta: 1, reclaimDelta: 1, bumpDelta: n - 1, newHeaps: n - 1 });
    else Object.assign(base, { hintDelta: 1, freeSetDelta: n - 1, reclaimDelta: n });
    return base;
  };
  const build = () => {
    const d = { version: 1, hashes: frozen, environmentSha256: h,
      inventoryReceipt: 'synthetic', oracleReceipt: 'synthetic', cells: [], oracle: { A: {}, B: {} }, mimalloc: [] };
    for (const arm of ['A', 'B']) {
      d.oracle[arm].start = { n: orec(arm, 'start', 'n') };
      for (const f of families) d.oracle[arm][f] = Object.fromEntries(sizes.map(s => [s, orec(arm, f, s)]));
    }
    const push = (f, s, arm, ir) => d.cells.push({ name: cellName(f, s), family: f, size: s, arm, ir, environmentSha256: h,
      binarySha256: binOf(arm), source: src(idOf(arm)), startedAt: '2026-10-11T01:00:00Z', receipt: 'synthetic' });
    for (const arm of arms) {
      push('start', 'n', arm, 50000);
      // cold and wave are deliberately NOT affine (windows 1000 and 3000); recycled is affine (slope 1000).
      for (const s of sizes) {
        push('cold', s, arm, { n: 4000, '2n': 5000, '4n': 8000 }[s]);
        push('recycled', s, arm, { n: 4000, '2n': 5000, '4n': 7000 }[s]);
        push('wave', s, arm, { n: 6000, '2n': 7000, '4n': 10000 }[s]);
      }
    }
    for (const arm of arms) d.mimalloc.push({ name: 'mimalloc_small_churn_16b', arm,
      ir: 9000, environmentSha256: h, binarySha256: binOf(arm), source: src(idOf(arm)),
      startedAt: '2026-10-11T01:00:00Z', receipt: 'synthetic' });
    return d;
  };
  const cellsOf = (x, f, arm, size) => x.cells.filter(c => c.family === f && c.arm === arm && (size === undefined || c.size === size));
  assert.equal(derive(build(), frozen).verdict, 'GO');
  const mutate = (label, fn, expected = 'INCOMPLETE') => {
    const x = build(); fn(x); const r = derive(x, frozen);
    const got = typeof expected === 'string' ? r.verdict : expected(r);
    assert.equal(got, typeof expected === 'string' ? expected : true,
      `mutant [${label}] expected ${JSON.stringify(expected)}, got ${r.verdict}`);
    console.log(`  selftest mutant PASS: ${label}`);
  };
  const row = (r, name) => r.tables.families.find(f => f.family === name);
  // R-claim, per window and family.
  mutate('cold B slope +4% of the window stays inside the one-sided budget',
    x => { cellsOf(x, 'cold', 'B1', '2n').forEach(c => c.ir += 40); cellsOf(x, 'cold', 'B1', '4n').forEach(c => c.ir += 120); },
    r => r.verdict === 'GO' && row(r, 'cold').checks.R_claim.every(p => p.pass));
  mutate('cold B slope +6% of the window exceeds the 5% budget',
    x => { cellsOf(x, 'cold', 'B1', '2n').forEach(c => c.ir += 60); cellsOf(x, 'cold', 'B1', '4n').forEach(c => c.ir += 180); }, 'NO-GO');
  mutate('cold regression confined to claims 17..32 (window 1 only)',
    x => { cellsOf(x, 'cold', 'B1', '2n').forEach(c => c.ir += 60); cellsOf(x, 'cold', 'B1', '4n').forEach(c => c.ir += 60); }, 'NO-GO');
  mutate('cold window-2-only B excess +6%', x => cellsOf(x, 'cold', 'B1', '4n').forEach(c => c.ir += 180), 'NO-GO');
  mutate('cold window-2-only B excess +4% stays inside', x => cellsOf(x, 'cold', 'B1', '4n').forEach(c => c.ir += 120), r => r.verdict === 'GO');
  mutate('wave window-2-only B excess +6% (wave is judged by the same rule)', x => cellsOf(x, 'wave', 'B2', '4n').forEach(c => c.ir += 180), 'NO-GO');
  mutate('recycled B +6% of the slope', x => { cellsOf(x, 'recycled', 'B1', '2n').forEach(c => c.ir += 60); cellsOf(x, 'recycled', 'B1', '4n').forEach(c => c.ir += 180); }, 'NO-GO');
  mutate('B faster than A (negative excess) is never punished',
    x => cellsOf(x, 'recycled', 'B1').concat(cellsOf(x, 'recycled', 'B2')).forEach(c => c.ir -= { n: 100, '2n': 200, '4n': 400 }[c.size]),
    r => r.verdict === 'GO' && row(r, 'recycled').checks.R_claim.some(p => p.percent.startsWith('-')));
  // Construction: the direct start cell, never a derived intercept.
  mutate('start cell: B costs T_s+1 more', x => cellsOf(x, 'start', 'B1').forEach(c => c.ir += 11), 'NO-GO');
  mutate('start cell: B costs exactly T_s more passes', x => cellsOf(x, 'start', 'B1').forEach(c => c.ir += 10), r => r.verdict === 'GO');
  mutate('cold regression confined to claims 1..N is caught by window 0 (whole N body)',
    x => cellsOf(x, 'cold', 'B1', 'n').forEach(c => c.ir += 300), 'NO-GO');
  mutate('a small first-N excess inside 5% of the body is not a construction verdict',
    x => cellsOf(x, 'cold', 'B1', 'n').forEach(c => c.ir += 150), r => r.verdict === 'GO');
  mutate('missing start cell -> construction UNJUDGEABLE, verdict never GO',
    x => x.cells = x.cells.filter(c => c.family !== 'start'), r => r.verdict === 'UNJUDGEABLE' && r.unjudgeableFamilies.includes('start'));
  // Noise ceiling and A/A.
  mutate('A1 vs A2 beyond the ceiling', x => { cellsOf(x, 'cold', 'A2', '2n').forEach(c => c.ir += 50); cellsOf(x, 'cold', 'A2', '4n').forEach(c => c.ir += 150); }, 'INCONCLUSIVE');
  mutate('A1 vs A2 beyond the ceiling in window 2 only', x => cellsOf(x, 'cold', 'A2', '4n').forEach(c => c.ir += 50), 'INCONCLUSIVE');
  mutate('C0 far from A1 is capped by the ceiling, never widens the budget',
    x => { cellsOf(x, 'recycled', 'C0', '2n').forEach(c => c.ir += 400); cellsOf(x, 'recycled', 'C0', '4n').forEach(c => c.ir += 800); }, 'INCONCLUSIVE');
  mutate('start cell noise above the ceiling', x => cellsOf(x, 'start', 'C0').forEach(c => c.ir += 600), 'INCONCLUSIVE');
  // Families and linearity.
  mutate('recycled 4N non-linearity', x => cellsOf(x, 'recycled', 'A1', '4n').concat(cellsOf(x, 'recycled', 'A2', '4n'), cellsOf(x, 'recycled', 'B1', '4n'),
    cellsOf(x, 'recycled', 'B2', '4n'), cellsOf(x, 'recycled', 'C0', '4n')).forEach(c => c.ir = 8000), 'INVALID');
  mutate('cold is exempt from the affine test', x => {}, r => r.verdict === 'GO');
  mutate('missing 2N sibling -> family UNJUDGEABLE, excluded, verdict is never GO',
    x => x.cells = x.cells.filter(c => !(c.family === 'wave' && c.size === '2n')),
    r => r.verdict === 'UNJUDGEABLE' && r.unjudgeableFamilies.includes('wave') && row(r, 'wave').status.startsWith('UNJUDGEABLE'));
  mutate('all families without 2N -> UNJUDGEABLE, never a vacuous GO', x => x.cells = x.cells.filter(c => c.family === 'start' || c.size !== '2n'), 'UNJUDGEABLE');
  mutate('missing 4N cell (2N present) -> data incomplete', x => x.cells = x.cells.filter(c => !(c.family === 'cold' && c.size === '4n')), 'INCOMPLETE');
  // Oracle: per arm, bound to the arm identity, path-exact.
  mutate('recycled oracle on B shows a bump instead of a reuse', x => Object.assign(x.oracle.B.recycled['2n'], { bumpDelta: 32, reclaimDelta: 0, hintDelta: 0, newHeaps: 32 }), 'INVALID');
  mutate('wave oracle on B never touched the free set (central B path inactive)',
    x => Object.assign(x.oracle.B.wave['4n'], { freeSetDelta: 0, bumpDelta: 63, newHeaps: 63 }), 'INVALID');
  mutate('wave oracle on A claims free-set hits', x => Object.assign(x.oracle.A.wave.n, { freeSetDelta: 15, bumpDelta: 0, newHeaps: 0 }), 'INVALID');
  mutate('missing oracle record of one arm and size', x => delete x.oracle.A.cold['2n'], 'INCOMPLETE');
  mutate('missing B oracle entirely', x => delete x.oracle.B, 'INCOMPLETE');
  mutate('oracle record not bound to the arm source', x => x.oracle.B.cold.n.source = src('a'), 'INCOMPLETE');
  mutate('oracle record with a bad binary hash', x => x.oracle.A.start.n.binarySha256 = 'z', 'INCOMPLETE');
  mutate('oracle label disagrees with its counter length', x => x.oracle.A.cold.n.claimSourcesLen = 5, 'INCOMPLETE');
  mutate('malformed claim_sources_len = 3', x => x.oracle.B.cold['4n'].claimSourcesLen = 3, 'INCOMPLETE');
  mutate('fifth missing but not flagged', x => x.oracle.A.recycled.n.missingFifthExplicitZero = false, 'INCOMPLETE');
  // Controls, identity, data hygiene.
  mutate('mimalloc delta = 1', x => x.mimalloc.filter(m => m.arm === 'B1').forEach(m => m.ir = 9001), 'NO-GO');
  mutate('missing inventoryReceipt', x => delete x.inventoryReceipt);
  mutate('broken binary hash', x => x.cells[0].binarySha256 = 'd'.repeat(64));
  mutate('capturedAt not before startedAt', x => x.cells[0].source.capturedAt = x.cells[0].startedAt);
  mutate('B shares the A source identity', x => x.cells.filter(c => bArms.includes(c.arm)).forEach(c => c.source.identitySha256 = x.cells[0].source.identitySha256));
  mutate('hash drift between the input and the frozen list', x => x.hashes = [{ path: 'synthetic', sha256: 'b'.repeat(64) }]);
  assert.equal(derive({}, frozen).verdict, 'INCOMPLETE');
  console.log('PASS: synthetic selftest only (base GO + negative controls + empty input); no measurement, no real GO');
}
const args = process.argv.slice(2);
const companion = () => readFileSync(new URL('docs/perf/R20_PRF04_CLAIM_PATH_PREREG.sha256', root), 'utf8');
try {
  if (args.length === 1 && args[0] === '--hashes') process.stdout.write(hashText());
  else if (args.length === 1 && ['--self-test', '--selftest'].includes(args[0])) selftest();
  else if (args.length === 1 && args[0] === '--check-hashes') {
    assert.equal(companion(), hashText());
    console.log('PASS: frozen document/script/runner/cell hashes');
  } else {
    assert.ok(args.length === 2 && args[0] === '--input', 'expected --input FILE; no data means INCOMPLETE');
    assert.equal(companion(), hashText());
    const result = derive(JSON.parse(readFileSync(resolve(args[1]), 'utf8')), hashes());
    console.log(render(result)); process.exitCode = result.verdict === 'GO' ? 0 : 1;
  }
} catch (e) { console.log(JSON.stringify({ verdict: 'INCOMPLETE', reason: e.message })); process.exitCode = 1; }
