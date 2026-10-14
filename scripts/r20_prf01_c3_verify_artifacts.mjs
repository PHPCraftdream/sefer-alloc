// Acceptance step for the R20 PRF-01 v2 gate (re-freeze 9; independent gate review, finding 4):
// receipt strings and pins in input.json are CLAIMS; this script verifies them against the run's
// out-dir. Run AFTER the judge and BEFORE the official verdict is treated as usable:
//
//   node scripts/r20_prf01_verify_artifacts.mjs <out-dir>
//
// Read-only; never re-measures. Exit 0 = accepted, 1 = failures (each reported).
// Verified: every receiptHashes entry exists and matches its sha256; every receipt string field of
// the input is anchored in receiptHashes and exists; ancestry agrees with manifest.json (A is the
// ancestor, chain = both baseHeads, distinct unless rehearsal); the input binary pins equal
// builds.json; a constructionGainReceipt, when present, points at an existing out-dir file whose
// sha256 matches and carries a nonempty decomposition.
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
import os from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { INVENTORY, ARMS, targetFor, summaryCounterAllowed, SUMMARY_ONLY_COUNTERS } from './r20_prf01_c3_support.mjs';

export function verifyArtifacts(outDir) {
  outDir = resolve(outDir);
    let failures = 0;
  const fail = m => { console.error(`FAIL: ${m}`); failures++; };
  const ok = m => console.log(`ok: ${m}`);
  const shaFile = p => createHash('sha256').update(readFileSync(p)).digest('hex');
  const readJson = name => {
    const p = join(outDir, name);
    if (!existsSync(p)) { fail(`missing stage file: ${name} in ${outDir}`); return null; }
    try { return JSON.parse(readFileSync(p, 'utf8')); }
    catch (e) { fail(`cannot parse ${name}: ${e.message}`); return null; }
  };
  const input = readJson('input.json');
  const manifest = readJson('manifest.json');
  const builds = readJson('builds.json');
  const oracle = readJson('oracle.json');
  if (!input || !manifest || !builds || !oracle) {
    console.error(`verify_artifacts: unusable out-dir: ${outDir}`);
    return { failures: failures + 1, refsCount: 0 };
  }

  // 1. C3 inventory and row-role schema.
  if (JSON.stringify(input.inventory) !== JSON.stringify(INVENTORY)) fail('input.inventory does not match shared C3 inventory');
  const requiredArms = ARMS;
  const seenRows = new Set();
  for (const r of input.iai ?? []) {
    const key = `${r.arm}/${r.name}`;
    if (!requiredArms.includes(r.arm) || !INVENTORY.includes(r.name)) fail(`unknown C3 inventory row: ${key}`);
    if (seenRows.has(key)) fail(`duplicate C3 inventory row: ${key}`);
    seenRows.add(key);
    const role = targetFor(r.name);
    const stageArm = r.arm.startsWith('A') ? 'A' : r.arm.startsWith('B') ? 'B' : 'C';
    const recorded = builds.arms?.[stageArm]?.[role]?.binarySha256;
    let pinned;
    if (r.arm === 'C0') pinned = role === 'iai' ? input.identities?.C0?.binarySha256 : role === 'controls' ? input.identities?.C0?.controlsBinarySha256 : input.identities?.C0?.binaries?.[role];
    else pinned = input.identities?.[stageArm]?.binaries?.[role];
    if (typeof recorded !== 'string' || !/^[0-9a-f]{64}$/.test(recorded) || !pinned || pinned !== recorded || r.binarySha256 !== recorded)
      fail(`${key}: missing or mismatched binarySha256 for builds.json ${role}`);
    if (r.oracle?.linkage) {
      for (const counter of SUMMARY_ONLY_COUNTERS) {
        if (!summaryCounterAllowed(r.oracle[counter], r.arm === 'C0' || r.arm.startsWith('A') ? 'A' : 'B'))
          fail(`${key}: oracle.${counter} is invalid for arm ${r.arm}`);
      }
    }
  }
  for (const name of INVENTORY) for (const arm of requiredArms)
    if (!seenRows.has(`${arm}/${name}`)) fail(`missing C3 inventory row: ${arm}/${name}`);

  // 1. receiptHashes map: every entry exists in the out-dir with the recorded sha256.
  const map = input.receiptHashes;
  const haveMap = map !== undefined && map !== null && typeof map === 'object' && !Array.isArray(map);
  const safeReceiptPath = p => typeof p === 'string' && p.trim().length > 0 && !p.includes(':') && !p.split(/[\\/]/).includes('..') && !p.startsWith('/') && !p.startsWith('\\\\');
  const contained = p => { const rel = resolve(outDir, p).slice(outDir.length).replaceAll('\\', '/'); return rel.startsWith('/') && !rel.slice(1).split('/').includes('..'); };
  if (!haveMap) {
    fail('input.json has NO receiptHashes map (it pre-dates the receipt-hash acceptance step; re-run the runner measure stage to regenerate the input)');
  } else {
    let good = 0;
  for (const [p, h] of Object.entries(map)) {
      if (!safeReceiptPath(p) || !contained(p)) { fail(`receiptHashes: unsafe out-dir path ${p}`); continue; }
      const f = join(outDir, p);
      if (!existsSync(f)) fail(`receiptHashes: ${p} does not exist`);
      else if (!/^[0-9a-f]{64}$/.test(String(h))) fail(`receiptHashes: ${p} has a malformed sha256`);
      else if (shaFile(f) !== h) fail(`receiptHashes: sha256 mismatch for ${p}`);
      else good++;
    }
    ok(`receiptHashes: ${good}/${Object.keys(map).length} anchored files verified`);
  }

  // 2. Every receipt string field of the input must be anchored in the map and exist.
  const refs = [];
  const ref = (p, where) => { if (typeof p === 'string' && p.trim().length > 0) refs.push([p, where]); };
  // identityReviewReceipt is a PROSE descriptor in the runner schema, not a file path: checked for
  // nonemptiness only. The other three top-level receipts are real out-dir files.
  if (typeof input.identityReviewReceipt !== 'string' || input.identityReviewReceipt.trim().length === 0)
    fail('identityReviewReceipt: missing or empty (prose descriptor)');
  for (const k of ['inventoryReceipt', 'environmentReceipt', 'oracleReceipt'])
    ref(input[k], k);
  for (const arm of ['A', 'B']) ref(input.identities?.[arm]?.buildReceipt, `identities.${arm}.buildReceipt`);
  ref(input.identities?.C0?.buildReceipt, 'identities.C0.buildReceipt');
  ref(input.sizeOf?.receipt, 'sizeOf.receipt');
  ref(input.ancestry?.receipt, 'ancestry.receipt');
  for (const r of input.iai ?? []) {
    const w = `${r.arm}/${r.name}`;
    ref(r.receipt, `${w}.receipt`);
    const p = r.producerIrReceipt;
    if (p) {
      ref(p.receipt, `${w}.producerIrReceipt.receipt`);
      ref(p.symbolReceipt, `${w}.producerIrReceipt.symbolReceipt`);
      ref(p.profileReceipt, `${w}.producerIrReceipt.profileReceipt`);
      ref(p.control?.receipt, `${w}.producerIrReceipt.control.receipt`);
      ref(p.control?.symbolReceipt, `${w}.producerIrReceipt.control.symbolReceipt`);
      ref(p.control?.profileReceipt, `${w}.producerIrReceipt.control.profileReceipt`);
    }
    const l = r.oracle?.linkage;
    if (l) { ref(l.receipt, `${w}.oracle.linkage.receipt`); ref(l.featureExactReceipt, `${w}.oracle.linkage.featureExactReceipt`); }
    ref(r.config?.declaredProvenance?.receipt, `${w}.config.declaredProvenance.receipt`);
    ref(r.config?.resolvedProvenance?.receipt, `${w}.config.resolvedProvenance.receipt`);
  }
  for (const [p, where] of refs) {
    if (!haveMap || !(p in map)) fail(`${where}: "${p}" is not anchored in receiptHashes`);
    else if (!safeReceiptPath(p) || !contained(p)) fail(`${where}: "${p}" is not a confined out-dir path`);
    else if (!existsSync(join(outDir, p))) fail(`${where}: "${p}" does not exist in the out-dir`);
  }
  if (haveMap) ok(`receipt fields: ${refs.length} checked against the map`);

  // 3. ancestry vs manifest: A is the ancestor, the chain is both baseHeads, distinct unless rehearsal.
  const headOf = arm => manifest.arches?.[arm]?.head;
  const rehearsal = input.rehearsal === true;
  const chain = input.ancestry?.chain;
  if (!Array.isArray(chain) || chain.length !== 2) fail('ancestry.chain must be a two-element array');
  else {
    const expect = [headOf('A'), rehearsal ? headOf('A') : headOf('B')];
    if (expect.some(h => typeof h !== 'string')) fail('manifest arches A/B are missing head fields');
    else {
      if (chain[0] !== expect[0] || chain[1] !== expect[1])
        fail(`ancestry.chain ${JSON.stringify(chain)} disagrees with manifest heads ${JSON.stringify(expect)}`);
      else ok('ancestry.chain agrees with manifest.json');
      if (!rehearsal && chain[0] === chain[1]) fail('non-rehearsal ancestry chain has equal A/B heads');
      if (input.identities?.A?.source?.baseHead !== chain[0]) fail('identities.A.source.baseHead != ancestry.chain[0]');
      if (input.identities?.B?.source?.baseHead !== chain[1]) fail('identities.B.source.baseHead != ancestry.chain[1]');
    }
  }

  if (input.identities?.C0?.source?.baseHead !== headOf('C')) fail('identities.C0.source.baseHead != manifest arches.C.head');
  else if (input.identities?.C0?.source?.baseHead !== headOf('A')) fail('identities.C0.source.baseHead != identities.A.source.baseHead');

  // 4. Input binary pins vs builds.json.
  for (const arm of ['A', 'B']) {
    for (const role of ['iai', 'controls', 'oracle', 'control', 'c3', 'trim', 'controlDistinct']) {
      const pinned = input.identities?.[arm]?.binaries?.[role];
      const recorded = builds.arms?.[arm]?.[role]?.binarySha256;
      if (!pinned || !recorded || pinned !== recorded)
        fail(`identities.${arm}.binaries.${role} missing or does not match builds.json`);
    }
  }
  for (const [field, role] of [['binarySha256', 'iai'], ['controlsBinarySha256', 'controls']]) {
    const pinned = input.identities?.C0?.[field];
    const recorded = builds.arms?.C?.[role]?.binarySha256;
    if (!pinned || !recorded || pinned !== recorded)
      fail(`identities.C0.${field} missing or does not match builds.json arms.C.${role}.binarySha256`);
  }
  for (const role of ['c3', 'trim', 'controlDistinct']) {
    const pinned = input.identities?.C0?.binaries?.[role];
    const recorded = builds.arms?.C?.[role]?.binarySha256;
    if (!pinned || !recorded || pinned !== recorded)
      fail(`identities.C0.binaries.${role} missing or does not match builds.json arms.C.${role}.binarySha256`);
  }
  ok('binary pins checked against builds.json');

  // 5. constructionGainReceipt: existing out-dir file, matching sha256, nonempty decomposition.
  let gainReceipts = 0;
  for (const r of input.iai ?? []) {
    const g = r.constructionGainReceipt;
    if (!g) continue;
    gainReceipts++;
    const f = typeof g.path === 'string' ? join(outDir, g.path) : null;
    if (!f || !fs.existsSync(f)) fail(`constructionGainReceipt (${r.arm}): path ${JSON.stringify(g.path)} does not exist in the out-dir`);
    else if (shaFile(f) !== g.sha256) fail(`constructionGainReceipt (${r.arm}): sha256 mismatch for ${g.path}`);
    if (typeof g.decomposition !== 'string' || g.decomposition.trim().length === 0)
      fail(`constructionGainReceipt (${r.arm}): decomposition is missing or empty`);
  }
  if (gainReceipts) ok(`constructionGainReceipt: ${gainReceipts} receipt(s) verified (file, sha256, nonempty decomposition)`);

  // oracle/ records referenced by the linkage receipts are covered by the map (prefix oracle/).
  if (failures === 0) {
    console.log(`verify_artifacts: PASS — ${outDir} is internally consistent (${refs.length} receipt fields, ${haveMap ? Object.keys(map).length : 0} anchored files)`);
  } else console.error(`verify_artifacts: FAIL — ${failures} problem(s) in ${outDir}`);
  return { failures, refsCount: refs.length };
}


function selftest() {
  let checks = 0;
  const hash = bytes => createHash('sha256').update(bytes).digest('hex');
  function fixture() {
    const dir = mkdtempSync(join(os.tmpdir(), 'r20-c3-verify-'));
    const files = new Map();
    const add = (p, text = `fixture:${p}`) => { const file = join(dir, p); fs.mkdirSync(resolve(file, '..'), { recursive: true }); writeFileSync(file, text); files.set(p, hash(text)); return p; };
    const heads = { A: 'a'.repeat(40), B: 'b'.repeat(40), C: 'a'.repeat(40) };
    const binaries = Object.fromEntries(['iai','controls','oracle','control','c3','trim','controlDistinct'].map((role, i) => [role, (i + 1).toString(16).repeat(64)]));
    binaries.controls = binaries.controlDistinct;
    const source = arm => ({ baseHead: heads[arm], patchSha256: 'd'.repeat(64), untrackedSha256: 'e'.repeat(64), identitySha256: (arm === 'A' ? 'f' : '1').repeat(64), capturedAt: '2026-10-13T00:00:00Z' });
    const receipt = name => add(`logs/${name}.log`);
    const identities = {
      A: { source: source('A'), binaries: { ...binaries }, buildReceipt: receipt('build-A') },
      B: { source: source('B'), binaries: { ...binaries }, buildReceipt: receipt('build-B') },
      C0: { source: source('A'), binarySha256: binaries.iai, controlsBinarySha256: binaries.controls, binaries: { ...binaries }, buildReceipt: receipt('build-C0') },
    };
    const iai = [];
    for (const name of INVENTORY) for (const arm of ['A1','A2','B1','B2','C0']) {
      const raw = receipt(`${arm}-${name}`);
      const row = { arm, name, receipt: raw, binarySha256: binaries[targetFor(name)], oracle: null };
      if (name === INVENTORY[0]) row.oracle = { linkage: { receipt: receipt(`${arm}-${name}-oracle`), featureExactReceipt: receipt(`${arm}-${name}-feature`) }, ...(arm === 'C0' || arm.startsWith('A') ? {} : { summary_words_visited: 0, summary_hint_loads: 0, summary_hint_rmw: 0 }) };
      iai.push(row);
    }
    const input = { version: 1, inventory: [...INVENTORY], receiptHashes: Object.fromEntries(files), identityReviewReceipt: 'synthetic review prose', inventoryReceipt: receipt('inventory'), environmentReceipt: receipt('environment'), oracleReceipt: receipt('oracle'), identities, ancestry: { chain: [heads.A, heads.B], receipt: receipt('ancestry') }, sizeOf: { A: 41984, B: 41984, receipt: receipt('size') }, iai };
    for (const p of files.keys()) input.receiptHashes[p] = files.get(p);
    for (const r of iai) for (const p of [r.receipt, r.oracle?.linkage?.receipt, r.oracle?.linkage?.featureExactReceipt].filter(Boolean)) input.receiptHashes[p] = files.get(p);
    writeFileSync(join(dir, 'input.json'), JSON.stringify(input));
    writeFileSync(join(dir, 'manifest.json'), JSON.stringify({ arches: Object.fromEntries(Object.entries(heads).map(([arm, head]) => [arm, { head }])) }));
    const buildArms = {};
    for (const arm of ['A','B','C']) buildArms[arm] = Object.fromEntries(Object.keys(binaries).map(role => [role, { binarySha256: binaries[role] }]));
    writeFileSync(join(dir, 'builds.json'), JSON.stringify({ arms: buildArms }));
    writeFileSync(join(dir, 'oracle.json'), '{}');
    return { dir, input };
  }
  const expect = (label, mutate, pass) => {
    const { dir, input } = fixture();
    try {
      if (mutate) mutate({ dir, input });
      if (mutate?.removeRaw) { rmSync(join(dir, 'logs/A1-multiseg_cold_256k.log')); delete input.receiptHashes['logs/A1-multiseg_cold_256k.log']; }
      writeFileSync(join(dir, 'input.json'), JSON.stringify(input));
      const result = verifyArtifacts(dir);
      if ((result.failures === 0) !== pass) throw new Error(`${label}: expected ${pass ? 'PASS' : 'FAIL'}, got ${result.failures} failures`);
      checks++;
      console.log(`selftest ${label}: ${pass ? 'PASS' : 'FAIL as expected'}`);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  };
  expect('baseline', null, true);
  expect('wrong receipt hash', x => { x.input.receiptHashes['logs/A1-multiseg_cold_256k.log'] = '0'.repeat(64); }, false);
  expect('missing raw log', x => { rmSync(join(x.dir, 'logs/A1-multiseg_cold_256k.log')); delete x.input.receiptHashes['logs/A1-multiseg_cold_256k.log']; }, false);
  expect('identity mismatch', x => { x.input.identities.A.source.baseHead = '9'.repeat(40); }, false);
  expect('role-inconsistent binary', x => { x.input.identities.A.binaries.controls = x.input.identities.A.binaries.iai; }, false);
  expect('C0 source mismatch', x => { x.input.identities.C0.source.baseHead = '9'.repeat(40); }, false);
  expect('unsafe receipt traversal', x => { const p = '../outside'; x.input.receiptHashes[p] = '0'.repeat(64); }, false);
  expect('A summary omitted, B present', x => { const r=x.input.iai.find(v=>v.arm==='A1'&&v.name===INVENTORY[0]); for (const k of SUMMARY_ONLY_COUNTERS) delete r.oracle[k]; const b=x.input.iai.find(v=>v.arm==='B1'&&v.name===INVENTORY[0]); b.oracle.summary_words_visited=2; }, true);
  expect('B summary omitted', x => { const r=x.input.iai.find(v=>v.arm==='B1'&&v.name===INVENTORY[0]); delete r.oracle.summary_words_visited; }, false);
  expect('A summary truly absent', x => { const r=x.input.iai.find(v=>v.arm==='A1'&&v.name===INVENTORY[0]); delete r.oracle.summary_words_visited; }, true);
  console.log(`C3 verifier selftest PASS (${checks} checks)`);
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
if (isMain && process.argv.includes('--selftest')) selftest();
else if (isMain) {
  const result = verifyArtifacts(process.argv[2] ?? '');
  process.exit(result.failures === 0 ? 0 : 1);
}
