// Read-only R20 PRF-04 heap-reuse (P4-1) deriver. No subprocess, build,
// measurement or file writes. Style and plumbing follow the directly preceding
// gate of the same family, scripts/r20_prf04_claim_path_gate.mjs.
//
// Spec: docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md (frozen 2026-10-14).
// Arms A1/A2 (one A binary), S1/S2 (one S binary), C0 (A's source, other path
// label + registered -C debuginfo=line-tables-only). Every threshold below is
// FROZEN before any S number was seen; nothing here is chosen after data.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = new URL('../', import.meta.url);
// Frozen inputs, EXACT order of the prereg "Integrity" paragraph: the prereg,
// this judge, the runner, the control-geometry rule, both iai benches, both
// support bodies and the oracle. The companion .sha256 excludes itself.
const files = ['docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md', 'scripts/r20_prf04_heap_reuse_gate.mjs',
  'scripts/r20_prf04_heap_reuse_run.mjs', 'scripts/r20_prf04_heap_reuse_collect.mjs',
  'scripts/r20_prf04_heap_reuse_profile.mjs', 'scripts/r20_prf04_heap_reuse_run_selftest.mjs',
  'docs/perf/R20_CONTROL_GEOMETRY_RULE.md', 'benches/r20_prf04_heap_reuse_iai.rs', 'benches/r20_prf04_heap_reuse_l2_iai.rs',
  'benches/support/r20_prf04_heap_reuse.rs', 'benches/support/r20_prf04.rs',
  'examples/r20_prf04_heap_reuse_oracle.rs', 'examples/r20_prf04_heap_reuse_l2_oracle.rs'];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const hashes = () => files.map(path => {
  const p = new URL(path, root);
  if (!existsSync(p)) throw new Error(`frozen file does not exist yet: ${path} (the gate cannot run before it lands)`);
  return { path, sha256: sha(readFileSync(p)) };
});
const hashText = () => hashes().map(x => `${x.sha256}  ${x.path}\n`).join('');
const arms = ['A1', 'A2', 'S1', 'S2', 'C0'];
const aArms = ['A1', 'A2'], sArms = ['S1', 'S2']; // S = the candidate
const families = ['cold', 'recycled', 'wave', 'wave_release', 'l2_cold', 'l2_recycled', 'l2_wave_release'];
const sizes = ['n', '2n', '4n'];
export const RELEASE_LAYER = 'L2';
const COUNT = { n: 16, '2n': 32, '4n': 64 };
// Affine and non-affine families have explicitly registered windows.
const windowsOf = f => { if(f==='recycled'||f==='l2_recycled')return[['n','2n']]; if(f==='wave_release'||f==='l2_wave_release')return[['n','2n'],['2n','4n']]; return [['0','n'],['n','2n'],['2n','4n']]; };
// Descriptive 4N linearity: affine form for the recycled families, the w2=2*w1
// form for the release families; violation -> INVALID (prereg, wave-release).
const linearityInvalid = f => f === 'recycled' || f === 'l2_recycled' || f === 'wave_release' || f === 'l2_wave_release';
const cellName = (f, s) => (f === 'start' ? 'r20_prf04hr_start' : `r20_prf04hr_${f}_${s}`);
const P = { cold:4,recycled:4,wave:0,wave_release:14,l2_cold:7,l2_recycled:7,l2_wave_release:14 };
const WINDOW_OPS = 16n;
const NOISE_CEILING = 10; // fixed by the prereg; never raised after data
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
const pathOk = (f, arm, o) => {
  const n = o.n;
  if (f === 'start') return o.bumpDelta === 1 && o.newHeaps === 1 && o.reclaimDelta === 0 && o.freeSetDelta === 0 && o.popcount === 0;
  if (f === 'cold') return o.bumpDelta === n && o.newHeaps === n && o.hintDelta === 0 && o.reclaimDelta === 0 && o.freeSetDelta === 0 && o.popcount === 0;
  if (f === 'l2_cold') return o.shippingLayer === true && o.targetClaimDelta === n && o.firstAllocActivated === true && pathOk('cold', arm, o);
  if (f === 'recycled') return o.hintDelta === n && o.reclaimDelta === n && o.bumpDelta === 0 && o.newHeaps === 0 && o.freeSetDelta === 0 && o.popcount === 0;
  if (f === 'l2_recycled') return o.shippingLayer === true && o.targetClaimDelta === n && o.firstAllocActivated === true && pathOk('recycled', arm, o);
  if (f === 'wave' && arm === 'A') return o.hintDelta === 1 && o.bumpDelta === n - 1 && o.freeSetDelta === 0 && o.newHeaps === n - 1 && o.popcount === 0;
  if (f === 'wave') return o.hintDelta === 1 && o.freeSetDelta === n - 1 && o.bumpDelta === 0 && o.newHeaps === 0 && o.popcount === 64 - n;
  if (f === 'wave_release' || f === 'l2_wave_release') return (f !== 'l2_wave_release' || (o.shippingLayer === true && o.targetClaimDelta === 64 && o.firstAllocActivated === true)) && o.bumpDelta === 0 && o.hintDelta === 0 && o.reclaimDelta === 0 && o.freeSetDelta === 0 && o.newHeaps === 0 && o.popcountAvailable === (arm === 'S') && o.popcount === (arm === 'S' ? n - 1 : 0);
  return false;
};
function derive(d, frozen, selectedReleaseLayer = RELEASE_LAYER) {
  try {
    if (d.releaseLayer !== selectedReleaseLayer) throw new Error('release layer does not match frozen judge constant');
    assert.equal(d.releaseMode, selectedReleaseLayer, 'release mode does not match frozen layer');
    const fallback = selectedReleaseLayer === 'L1';
    if (d.rehearsal === true) return { verdict: 'REHEARSAL:A_ONLY', tables: { families: [] } };
    assert.equal(d.version, 1); assert.deepEqual(d.hashes, frozen);

    if (fallback) assert.equal(d.fallbackReceipt?.releaseLayer, 'L1', 'L1 release mode requires prerefreeze trial-rehearsal receipt binding');
    hex(d.environmentSha256); text(d.inventoryReceipt); text(d.oracleReceipt);
    text(d.symbolReceipt);
    let layerBins = {}, layerArmBins = {}, aSrc, sSrc, aBin = {}, sBin = {}, cBin = {};
    const byCell = new Map();
    for (const c of d.cells ?? []) {
      assert.ok(families.includes(c.family) || c.family === 'start'); assert.ok(sizes.includes(c.size));
      assert.ok(arms.includes(c.arm)); assert.equal(c.name, cellName(c.family, c.size));
    if (c.family.startsWith('l2')) assert.equal(c.layer, 'L2'); else assert.equal(c.layer, 'L1');
      assert.equal(c.environmentSha256, d.environmentSha256);
      source(c); integer(c.ir);
      const bk = `${c.arm}|${c.layer}`; if (layerBins[bk] !== undefined) assert.equal(c.binarySha256,layerBins[bk]); else layerBins[bk]=c.binarySha256;
      if (c.arm === 'C0') { const k=`C0|${c.layer}`; if(layerArmBins[k]!==undefined) assert.equal(c.binarySha256,layerArmBins[k]); else layerArmBins[k]=c.binarySha256; }
      else if (c.arm === 'A1'||c.arm==='A2') { const k=`A|${c.layer}`; if(layerArmBins[k]!==undefined) assert.equal(c.binarySha256,layerArmBins[k]); else layerArmBins[k]=c.binarySha256; }
      else { const k=`S|${c.layer}`; if(layerArmBins[k]!==undefined) assert.equal(c.binarySha256,layerArmBins[k]); else layerArmBins[k]=c.binarySha256; }
      const key = `${c.arm}|${c.layer}|${c.family}|${c.size}`;
      assert.ok(!byCell.has(key), 'duplicate arm/layer/cell row'); byCell.set(key,c);
      if (sArms.includes(c.arm)) {
        sSrc ??= c.source; assert.deepEqual(c.source, sSrc);
        sBin[c.layer] ??= c.binarySha256; assert.equal(c.binarySha256,sBin[c.layer]);
      } else {
        aSrc ??= c.source; assert.deepEqual(c.source, aSrc); // C0 shares the A source identity
        if (c.arm === 'C0') { cBin[c.layer] ??= c.binarySha256; assert.equal(c.binarySha256,cBin[c.layer]); }
        else { aBin[c.layer] ??= c.binarySha256; assert.equal(c.binarySha256,aBin[c.layer]); }
      }
    }
    assert.ok(fallback ? aSrc && sSrc : aSrc && sSrc, 'missing arm rows');
    assert.notEqual(aSrc.identitySha256, sSrc.identitySha256);
    for (const layer of ['L1','L2']) { assert.notEqual(aBin[layer],sBin[layer]); assert.notEqual(aBin[layer],cBin[layer]); }
    if (fallback) {
      assert.ok(byCell.has('A1|L1|start|n') && byCell.has('A2|L1|start|n') && byCell.has('S1|L1|start|n') && byCell.has('S2|L1|start|n') && byCell.has('C0|L1|start|n'));
    }
    // Per-arm path oracle: every (arm, family, size) record is bound to its arm's
    // source identity + oracle binary hash + environment, length-agnostic.
    const oracleState = f => {
      const bad = [], missing = [];
      for (const arm of ['A', 'S']) for (const s of (f === 'start' ? ['n'] : sizes)) {
        const o = d.oracle?.[arm]?.[f]?.[s];
        if (!o) { missing.push(`${arm}/${f}/${s}`); continue; }
        try {
          for (const k of ['bumpDelta','hintDelta','reclaimDelta','newHeaps','freeSetDelta','popcount','claimSourcesLen','n']) integer(o[k]);
          assert.equal(o.arm,arm); hex(o.binarySha256); assert.deepEqual(o.source,arm==='A'?aSrc:sSrc);
          assert.equal(o.environmentSha256,d.environmentSha256); assert.equal(o.claimSourcesLen,arm==='S'?5:4);
          assert.equal(o.missingFreeSetExplicitZero,o.claimSourcesLen<5); assert.equal(o.n,f==='start'?1:COUNT[s]);
          assert.ok(Number.isSafeInteger(o.configConflictsDelta)); assert.equal(o.requestedKind,arm==='A'?'FreshFirst':'ReuseFirst');
        } catch (e) { missing.push(`${arm}/${f}/${s} (${e.message})`); continue; }
        if (o.popcountAvailable !== (arm === 'S')) bad.push(`${arm}/${f}/${s} (popcount availability mismatch)`);
        if (o.configConflictsDelta !== 0) bad.push(`${arm}/${f}/${s} (config conflicts delta != 0)`);

        if (!pathOk(f, arm, o)) bad.push(`${arm}/${f}/${s}`);
      }
      return { bad, missing };
    };
    const tables = { families: [] };
    let incomplete = false, invalid = false, noGo = false, inconclusive = false;
    const unjudgeable = [];
    // Construction: the direct start cell, one-sided, budget_K = T_s + 4 (frozen).
    {
      const row = { family: 'start (construction)', arms: {}, status: 'judged', checks: {} };
      assert.ok(arms.every(a => byCell.has(`${a}|L1|start|n`)));
      for (const a of arms) if (byCell.has(`${a}|L1|start|n`)) row.arms[a] = { n: byCell.get(`${a}|L1|start|n`).ir };
      const v = a => byCell.get(`${a}|L1|start|n`).ir;
      const aa = Math.abs(v('A1') - v('A2')), c0 = Math.abs(v('C0') - v('A1'));
      const tS = Math.max(NOISE_CEILING, aa, c0);
        row.T_s = tS;
        row.checks.noise = { aa, c0, ceiling: NOISE_CEILING, pass: aa <= NOISE_CEILING && c0 <= NOISE_CEILING };
        if (!row.checks.noise.pass) { inconclusive = true; row.status = 'INCONCLUSIVE (layout noise above the ceiling)'; }
        row.checks.R_construct = [];
        for (const a of aArms) for (const b of sArms) {
          if (!byCell.has(`${a}|L1|start|n`) || !byCell.has(`${b}|L1|start|n`)) { incomplete = true; continue; }
          const delta = v(b) - v(a), pass = delta <= tS + 4;
          row.checks.R_construct.push({ pair: `${b} vs ${a}`, delta, budget_K: tS + 4, pass });
          if (!pass) noGo = true;
        }
      const st = oracleState('start');
      row.checks.oracle = { ...st, pass: !st.bad.length && !st.missing.length };
      if (st.missing.length) { incomplete = true; row.status = 'INCOMPLETE (oracle)'; }
      else if (st.bad.length) { invalid = true; row.status = 'INVALID (oracle violation)'; }
      tables.families.push(row);
    }
    const activeFamilies = families.filter(f => !fallback || f !== 'l2_wave_release');
    const activeArms = fallback ? ['A1','A2','S1','S2','C0'] : ['A1','A2','S1','S2','C0'];
    if (fallback) assert.ok(d.cells.every(c => c.family !== 'l2_wave_release'), 'fallback must omit L2 release rows');
    // Family rows are evaluated only for the selected release contract.
    for (const f of activeFamilies) {
      const row = { family: f, arms: {}, status: 'judged', checks: {} };
      const layer = f => f.startsWith('l2') ? 'L2' : 'L1';
      const pick = (f,a,s) => byCell.get(`${a}|${layer(f)}|${f}|${s}`);
      if (!activeArms.every(a => pick(f,a,'2n'))) {
        row.status = 'UNJUDGEABLE (no 2N sibling; excluded from verdict, never a pass)';
        unjudgeable.push(f); tables.families.push(row); continue;
      }
      if (!sizes.every(s => activeArms.every(a => pick(f,a,s)))) {
        incomplete = true; row.status = 'INCOMPLETE (missing cell; the prereg requires N/2N/4N)';
        tables.families.push(row); continue;
      }
      const v = (a, s) => (s === '0' ? 0 : pick(f,a,s).ir);
      const wins = windowsOf(f);
      const ws = {}; // ws[arm][k] = window k value
      for (const a of activeArms) {
        ws[a] = wins.map(([x, y]) => v(a, y) - v(a, x));
        for (const w of ws[a]) assert.ok(Number.isSafeInteger(w));
        row.arms[a] = { n: v(a, 'n'), '2n': v(a, '2n'), '4n': v(a, '4n'), windows: ws[a] };
      }
      // T_s per window (rule 2.2), fixed noise ceiling of 10 (prereg; never A1/200).
      const layerArms = ['A1','A2','S1','S2','C0'];
      const tS = wins.map((_, k) => Math.max(NOISE_CEILING, Math.abs(ws.A1[k] - ws.A2[k]), Math.abs(ws.C0[k] - ws.A1[k])));
      row.T_s = tS; row.windows = wins.length;
      row.checks.noise = wins.map((_, k) => {
        if (!(ws.A1[k] > 0)) { incomplete = true; return { window: k, pass: false, reason: 'non-positive A1 window' }; }
        const aa = Math.abs(ws.A1[k] - ws.A2[k]), c0 = Math.abs(ws.C0[k] - ws.A1[k]);
        const pass = aa <= NOISE_CEILING && c0 <= NOISE_CEILING;
        if (!pass) inconclusive = true;
        return { window: k, aa, c0, ceiling: NOISE_CEILING, pass };
      });
      const noiseOk = row.checks.noise.every(x => x.pass);
      // Linearity: recycled/l2_recycled affine check; wave_release/l2_wave_release
      // descriptive 4N check. Violation -> INVALID for all four (prereg).
        if (linearityInvalid(f)) {
        const affine = f === 'recycled' || f === 'l2_recycled';
        const dev = a => affine ? Math.abs((v(a,'4n')-v(a,'2n'))-2*ws[a][0]) : Math.abs(ws[a][wins.length-1]-2*ws[a][wins.length-2]);
        row.checks.linearity = activeArms.map(a=>({arm:a,deviation:dev(a),bound:tS[0],pass:dev(a)<=tS[0]}));
        if (row.checks.linearity.some(x=>!x.pass)) { invalid=true; row.status='INVALID (4N non-linearity)'; }
      }
      const st = oracleState(f);
      row.checks.oracle = { ...st, pass: !st.bad.length && !st.missing.length };
      if (st.missing.length) { incomplete = true; row.status = 'INCOMPLETE (oracle)'; }
      else if (st.bad.length) { invalid = true; row.status = row.status === 'judged' ? 'INVALID (oracle violation)' : `${row.status} + INVALID (oracle violation)`; }
      // R-relative (5 %, exact BigInt), R-absolute (16*P + T_s) and, for the wave
      // claim family only, the frozen benefit criterion 4*w_S <= w_A on every
      // window and pair, independent of T_s.
      const pairs = [];
      for (let k = 0; k < wins.length; k++) for (const a of ['A1','A2']) {
        if (!(ws[a][k] > 0)) { incomplete = true; continue; }
        for (const b of sArms) {
          const num = ws[b][k] - ws[a][k], den = ws[a][k];
          const relative = 20n * BigInt(num) <= BigInt(den);
          if (f === 'wave_release' || f === 'l2_wave_release') {
            const ops = BigInt(COUNT[wins[k][1]]) - (wins[k][0] === '0' ? 0n : BigInt(COUNT[wins[k][0]]));
            const absolute = BigInt(num) <= 14n * ops + BigInt(tS[k]);
            const pass = relative && absolute;
            pairs.push({ pair: `${b} vs ${a}`, window: k, numerator: num, denominator: den, percent: pct(num, den), relative, absolute, allowance: '14 Ir/release + T_s', benefit: null, budget: '+5% relative AND +14 Ir/release+T_s, one-sided', pass });
            if (!pass) noGo = true;
            continue;
          }
          const absolute = f === 'wave' ? null : BigInt(num) <= (BigInt(COUNT[wins[k][1]]) - (wins[k][0] === '0' ? 0n : BigInt(COUNT[wins[k][0]]))) * BigInt(P[f] ?? 0) + BigInt(tS[k]);
          const benefit = f === 'wave' ? BigInt(4) * BigInt(ws[b][k]) <= BigInt(ws[a][k]) : null; // wave (claim, L1) only
          const pass = relative && (absolute !== false) && (benefit !== false);
          pairs.push({ pair: `${b} vs ${a}`, window: k, numerator: num, denominator: den, percent: pct(num, den),
            relative, absolute, allowance: `16*${P[f] ?? 0}+T_s`, benefit, budget: '+5% relative AND 16*P+T_s absolute, one-sided', pass });
          if (!relative || absolute === false || benefit === false) noGo = true;
        }
      }
      row.checks.R_claim = pairs;
      if (row.status === 'judged' && !noiseOk) row.status = 'INCONCLUSIVE (layout noise above the ceiling)';
      tables.families.push(row);
    }
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
  for (const f of r.tables?.families ?? []) {
    out.push(`| family ${f.family} | status ${f.status} | T_s ${f.T_s ?? '-'} | windows ${f.windows ?? '-'} |`);
    for (const [a, x] of Object.entries(f.arms ?? {}))
      out.push(`|   ${a} | n=${x.n} 2n=${x['2n'] ?? '-'} 4n=${x['4n'] ?? '-'} | windows=${x.windows ?? '-'} |`);
    for (const c of f.checks?.R_claim ?? [])
      out.push(`|   R-claim ${c.pair} window ${c.window} | ${c.percent} | rel=${c.relative} abs=${c.absolute} benefit=${c.benefit} | budget ${c.budget} | pass=${c.pass} |`);
    for (const c of f.checks?.R_construct ?? [])
      out.push(`|   R-construct ${c.pair} | Ir_S-Ir_A=${c.delta} | budget_K=${c.budget_K} | pass=${c.pass} |`);
    for (const c of [].concat(f.checks?.noise ?? []))
      out.push(`|   noise${c.window === undefined ? '' : ` window ${c.window}`} | |A1-A2|=${c.aa} |C0-A1|=${c.c0} | ceiling=${c.ceiling} (fixed 10) | pass=${c.pass} |`);
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
    identitySha256: (id==='a'?'a':'b').repeat(64), capturedAt: '2026-10-11T00:00:00Z' });
  const idOf = arm => sArms.includes(arm) ? 'b' : 'a'; // hex-safe identity tags
  const binOf = (arm,layer) => (arm==='C0'?'c':(layer==='L2'?(idOf(arm)==='a'?'d':'e'):(idOf(arm)==='a'?'a':'b'))).repeat(64);
  const orec = (arm, f, s) => {
    const n = f === 'start' ? 1 : COUNT[s];
    const base = { n, arm, binarySha256: (arm === 'A' ? 'e' : 'f').repeat(64), waveSlots: 64,
      targetClaimDelta: f==='l2_wave_release'?64:n, firstAllocActivated: ['cold','wave','l2_cold','l2_recycled','l2_wave_release'].includes(f), shippingLayer:f.startsWith('l2'),
      requestedKind: arm === 'A' ? 'FreshFirst' : 'ReuseFirst', popcount: 0, configConflictsDelta: 0, bumpDelta: f==='start'?1:0, hintDelta:0, reclaimDelta:0, newHeaps:f==='start'?1:0, freeSetDelta:0 };
    if (f === 'cold' || f === 'l2_cold') Object.assign(base, { bumpDelta: n, newHeaps: n, ...(f==='l2_cold'?{targetClaimDelta:n,firstAllocActivated:true,shippingLayer:true}:{}) });
    else if (f === 'recycled' || f === 'l2_recycled') Object.assign(base, { hintDelta: n, reclaimDelta:n, ...(f==='l2_recycled'?{targetClaimDelta:n,firstAllocActivated:true,shippingLayer:true}:{}) });
    else if (arm === 'A' && f === 'wave')
      Object.assign(base, { hintDelta: 1, bumpDelta: n - 1, newHeaps: n - 1, targetClaimDelta:n, firstAllocActivated:true, popcount:0 });
    else if (f === 'wave') Object.assign(base, { hintDelta: 1, freeSetDelta: n - 1, targetClaimDelta:n, firstAllocActivated:true, popcount:64-n });
    else if (f === 'wave_release' || f === 'l2_wave_release') Object.assign(base, { waveSlots: 64, bumpDelta:0, hintDelta:0, reclaimDelta:0, newHeaps:0, freeSetDelta:0, popcount: arm === 'S' ? n - 1 : 0, ...(f==='l2_wave_release'?{targetClaimDelta:64,firstAllocActivated:true,shippingLayer:true}:{}) });
    return base;
  };
  const build = (releaseLayer = RELEASE_LAYER) => {
    const d = { version: 1, releaseLayer, releaseMode: releaseLayer, hashes: frozen, environmentSha256: h, symbolReceipt: 'synthetic nm receipts', inventoryReceipt: 'synthetic', oracleReceipt: 'synthetic', cells: [], oracle: { A: {}, S: {} } };
    for (const arm of ['A', 'S']) {
      const bind = (o,f) => { const layer=f.startsWith('l2')?'L2':'L1'; return Object.assign(o, { environmentSha256: h, source: src(arm === 'A' ? 'a' : 'b'),
        claimSourcesLen: arm === 'S' ? 5 : 4, missingFreeSetExplicitZero: arm === 'A', popcountAvailable: arm === 'S',
        binarySha256: (arm === 'A' ? (layer==='L1'?'e':'a') : (layer==='L1'?'f':'b')).repeat(64) }); };
      d.oracle[arm].start = { n: bind(orec(arm, 'start', 'n'),'start') };
      for (const f of families) d.oracle[arm][f] = Object.fromEntries(sizes.map(s => [s, bind(orec(arm, f, s),f)]));
    }
    // Synthetic Ir per arm and size. A and S identical except where the frozen
    // criteria force S to be faster (wave benefit) or allow a small excess.
    const IR = {
      start: { A: 50000, S: 50000 },
      cold: { A: { n: 4000, '2n': 5000, '4n': 8000 }, S: { n: 4000, '2n': 5000, '4n': 8000 } },
      recycled: { A: { n: 4000, '2n': 5000, '4n': 7000 }, S: { n: 4000, '2n': 5000, '4n': 7000 } },
      wave: { A: { n: 6000, '2n': 7000, '4n': 10000 }, S: { n: 1400, '2n': 1600, '4n': 2200 } },
      // wave_release / l2_wave_release keep the descriptive 4N linearity (w2 = 2*w1).
      wave_release: { A: { n: 8000, '2n': 9000, '4n': 11000 }, S: { n: 8040, '2n': 9040, '4n': 11040 } },
      l2_cold: { A: { n: 9000, '2n': 19000, '4n': 49000 }, S: { n: 9030, '2n': 19030, '4n': 49030 } },
      l2_recycled: { A: { n: 4000, '2n': 5000, '4n': 7000 }, S: { n: 4000, '2n': 5000, '4n': 7000 } },
      l2_wave_release: { A: { n: 8000, '2n': 9000, '4n': 11000 }, S: { n: 8000, '2n': 9000, '4n': 11000 } },
    };
    const push = (f, s, arm) => { const layer=f.startsWith('l2')?'L2':'L1'; d.cells.push({ name: cellName(f,s), family:f,size:s,layer,arm,
      ir: IR[f][sArms.includes(arm)?'S':'A'][s] ?? IR[f][sArms.includes(arm)?'S':'A'],
      environmentSha256:h,binarySha256:binOf(arm,layer),source:src(idOf(arm)),startedAt:'2026-10-11T01:00:00Z',receipt:'synthetic' }); };
    for (const arm of arms) {
      push('start', 'n', arm);
      for (const f of families) for (const s of sizes) push(f, s, arm);
    }
    return d;
  };
  const cellsOf = (x, f, arm, size) => x.cells.filter(c => c.family === f && c.arm === arm && (size === undefined || c.size === size));
  assert.equal(derive(build(), frozen).verdict, 'GO');
  const fallbackBuild = build('L1');
  fallbackBuild.fallbackReceipt = { releaseLayer:'L1', sourceIdentitySha256:src('a').identitySha256, binarySha256:'d'.repeat(64), rawReceipt:'retained trial rehearsal bundle' };
  fallbackBuild.cells = fallbackBuild.cells.filter(c => c.family !== 'l2_wave_release');
  fallbackBuild.oracle.A.l2_wave_release = undefined; fallbackBuild.oracle.S.l2_wave_release = undefined;
  const fallbackSavedOracle = fallbackBuild.oracle.S.l2_recycled;
  delete fallbackBuild.oracle.S.l2_recycled;
  assert.equal(derive(fallbackBuild, frozen, 'L1').verdict, 'INCOMPLETE');
  fallbackBuild.oracle.S.l2_recycled = fallbackSavedOracle;
  assert.equal(derive(build('L2'), frozen, 'L1').verdict, 'INCOMPLETE');
  const mutate = (label, fn, expected = 'INCOMPLETE') => {
    const x = build(); fn(x); const r = derive(x, frozen);
    const got = typeof expected === 'string' ? r.verdict : expected(r);
    assert.equal(got, typeof expected === 'string' ? expected : true,
      `mutant [${label}] expected ${JSON.stringify(expected)}, got ${r.verdict}`);
    console.log(`  selftest mutant PASS: ${label}`);
  };
  const row = (r, name) => r.tables.families.find(f => f.family === name);
  mutate('(1) l2_cold +140 exceeds 16*7+T_s absolute boundary',
    x => cellsOf(x, 'l2_cold', 'S1', '2n').forEach(c => c.ir += 140), 'NO-GO');
  mutate('(2) +122 boundary stays GO',
    x => cellsOf(x, 'l2_cold', 'S1', '2n').forEach(c => c.ir += 122), r => r.verdict === 'GO' && row(r, 'l2_cold').checks.R_claim.every(p => p.absolute));
  mutate('(3) +123 exceeds absolute boundary -> NO-GO',
    x => cellsOf(x, 'l2_cold', 'S1', '2n').forEach(c => c.ir += 123), 'NO-GO');
  // (4) a relative 4.9% excess with a small absolute delta is not punished.
  mutate('(4) cold w2 +70 (2.33% < 5%, < 16*4+T_s) stays GO',
    x => cellsOf(x, 'cold', 'S1', '4n').forEach(c => c.ir += 70), r => r.verdict === 'GO' && row(r, 'cold').checks.R_claim.every(p => p.pass));
  // (5) negative deltas (S faster) are never punished.
  mutate('(5) S faster than A everywhere is never punished',
    x => cellsOf(x, 'recycled', 'S1').concat(cellsOf(x, 'recycled', 'S2')).forEach(c => c.ir -= { n: 100, '2n': 200, '4n': 400 }[c.size]),
    r => r.verdict === 'GO' && row(r, 'recycled').checks.R_claim.some(p => p.percent.startsWith('-')));
  // (6)/(7) noise ceiling: fixed 10, INCONCLUSIVE at 11, still passing at 10.
  mutate('(6) noise |A1-A2| = 11 above the fixed ceiling -> INCONCLUSIVE',
    x => cellsOf(x, 'cold', 'A2', '2n').forEach(c => c.ir += 11), 'INCONCLUSIVE');
  mutate('(7) noise |A1-A2| = exactly 10 passes',
    x => cellsOf(x, 'cold', 'A2', '2n').forEach(c => c.ir += 10), 'GO');
  // (8) oracle: the S wave path never touched the free set -> mechanism inactive.
  mutate('(8) wave oracle at S shows bump/free_set=0 (ReuseFirst inactive) -> INVALID',
    x => Object.assign(x.oracle.S.wave['4n'], { freeSetDelta: 0, bumpDelta: 63, newHeaps: 63 }), 'INVALID');
  // (9) popcount observer not confirmed at S.
  mutate('(9) popcountAvailable:false at S -> INVALID (mechanism unconfirmed)',
    x => sizes.forEach(s => x.oracle.S.cold[s].popcountAvailable = false), 'INVALID');
  mutate('base A observer absent is accepted', x => sizes.forEach(s => x.oracle.A.wave[s].popcountAvailable = false), 'GO');
  mutate('A positive popcount invalid', x => sizes.forEach(s => { x.oracle.A.wave[s].popcountAvailable = true; x.oracle.A.wave[s].popcount = 1; }), 'INVALID');
  mutate('S observer absent invalidates', x => sizes.forEach(s => x.oracle.S.wave[s].popcountAvailable = false), 'INVALID');
  mutate('(10) missing cold 4N cell -> INCOMPLETE',
    x => x.cells = x.cells.filter(c => !(c.family === 'cold' && c.size === '4n')), 'INCOMPLETE');
  // (11) a missing 2N sibling -> family UNJUDGEABLE, verdict never GO.
  mutate('(11) missing wave 2N -> UNJUDGEABLE, never GO',
    x => x.cells = x.cells.filter(c => !(c.family === 'wave' && c.size === '2n')),
    r => r.verdict === 'UNJUDGEABLE' && r.unjudgeableFamilies.includes('wave') && row(r, 'wave').status.startsWith('UNJUDGEABLE'));
  // (12) recycled 4N linearity.
  mutate('(12) recycled 4N non-linearity -> INVALID',
    x => arms.forEach(a => cellsOf(x, 'recycled', a, '4n').forEach(c => c.ir += 2000)), 'INVALID');
  // (13) the frozen wave benefit criterion.
  mutate('(13a) wave benefit violated (w_S = 0.29*w_A on window 1) -> NO-GO',
    x => cellsOf(x, 'wave', 'S1', '2n').forEach(c => c.ir += 90), 'NO-GO');
  mutate('(13b) wave benefit exactly at 0.25 (w_S = 0.25*w_A) passes',
    x => cellsOf(x, 'wave', 'S1', '2n').forEach(c => c.ir += 50), 'GO');
  // (14) construction budget_K = T_s + 4.
  mutate('(14a) start S costs T_s+5 -> NO-GO', x => cellsOf(x, 'start', 'S1').forEach(c => c.ir += 15), 'NO-GO');
  mutate('(14b) start S costs exactly T_s+4 -> pass', x => cellsOf(x, 'start', 'S1').forEach(c => c.ir += 14), 'GO');
  // (15) hash drift between the input and the frozen list.
  mutate('(15) hash drift', x => x.hashes = [{ path: 'synthetic', sha256: 'b'.repeat(64) }]);
  // (16) source binding broken (S carries the A identity).
  mutate('(16) S shares the A source identity', x => x.cells.filter(c => sArms.includes(c.arm)).forEach(c => c.source.identitySha256 = x.cells[0].source.identitySha256));
  // Extra negative controls: release linearity, R26-4 evidence, oracle binding,
  // non-positive A window, missing oracle rows.
  mutate('wave_release 4N descriptive non-linearity -> INVALID',
    x => arms.forEach(a => cellsOf(x, 'wave_release', a, '4n').forEach(c => c.ir += 500)), 'INVALID');
  mutate('L2 wave-release target claim mismatch -> INVALID',
    x => x.oracle.S.l2_wave_release['2n'].targetClaimDelta = 1, 'INVALID');
  mutate('L2 wave-release target claim zero -> INVALID',
    x => x.oracle.S.l2_wave_release['2n'].targetClaimDelta = 0, 'INVALID');
  mutate('config_conflicts nonzero -> INVALID',
    x => x.oracle.S.wave['2n'].configConflictsDelta = 1, 'INVALID');
  mutate('config_conflicts wrong type -> INCOMPLETE', x => x.oracle.S.wave['2n'].configConflictsDelta = '1');
  mutate('oracle record not bound to the arm source', x => x.oracle.S.cold.n.source = src('a'));
  mutate('missing oracle record of one arm and size', x => delete x.oracle.A.wave_release['2n']);
  mutate('oracle label disagrees with its counter length', x => x.oracle.A.cold.n.claimSourcesLen = 5);
  mutate('fifth missing but not flagged', x => x.oracle.A.recycled.n.missingFreeSetExplicitZero = false);
  mutate('non-positive A1 window -> INCOMPLETE, never a pass',
    x => cellsOf(x, 'cold', 'A1', 'n').forEach(c => c.ir = 0), 'INCOMPLETE');
  mutate('all families without 2N -> UNJUDGEABLE, never a vacuous GO',
    x => x.cells = x.cells.filter(c => c.family === 'start' || c.size !== '2n'), 'UNJUDGEABLE');
  mutate('broken binary hash', x => x.cells[0].binarySha256 = 'd'.repeat(64));
  mutate('missing symbol receipt', x => delete x.symbolReceipt);
  assert.equal(derive({}, frozen).verdict, 'INCOMPLETE');
  console.log('PASS: synthetic selftest only (base GO + negative controls + empty input); no measurement, no real GO');
}
const args = process.argv.slice(2);
const companion = () => {
  const p = new URL('docs/perf/R20_PRF04_HEAP_REUSE_PREREG.sha256', root);
  if (!existsSync(p)) throw new Error('companion hash file absent; orchestrator must re-freeze before production measurement');
  return readFileSync(p, 'utf8');
};
try {
  if (args.length === 1 && args[0] === '--hashes') process.stdout.write(hashText());
  else if (args.length === 1 && ['--self-test', '--selftest'].includes(args[0])) selftest();
  else if (args.length === 1 && args[0] === '--check-hashes') {
    assert.equal(companion(), hashText());
    console.log('PASS: preregistered frozen input hashes');
  } else {
    assert.ok(args.length === 2 && args[0] === '--input', 'expected --input FILE; no data means INCOMPLETE');
    assert.equal(companion(), hashText());
    const data = JSON.parse(readFileSync(resolve(args[1]), 'utf8'));
    const result = derive(data, hashes());
    console.log(render(result));
    process.exitCode = result.verdict === 'GO' || result.verdict === 'REHEARSAL:A_ONLY' ? 0 : 1;
  }
} catch (e) { console.log(JSON.stringify({ verdict: 'INCOMPLETE', reason: e.message })); process.exitCode = 1; }
