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
import { join, resolve } from 'node:path';

const outDir = resolve(process.argv[2] ?? '');
let failures = 0;
const fail = m => { console.error(`FAIL: ${m}`); failures++; };
const ok = m => console.log(`ok: ${m}`);
const shaFile = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const readJson = name => {
  const p = join(outDir, name);
  if (!fs.existsSync(p)) { fail(`missing stage file: ${name} in ${outDir}`); return null; }
  try { return JSON.parse(fs.readFileSync(p, 'utf8')); }
  catch (e) { fail(`cannot parse ${name}: ${e.message}`); return null; }
};
const input = readJson('input.json');
const manifest = readJson('manifest.json');
const builds = readJson('builds.json');
const oracle = readJson('oracle.json');
if (!input || !manifest || !builds || !oracle) {
  console.error(`verify_artifacts: unusable out-dir: ${outDir}`);
  process.exit(1);
}

// 1. receiptHashes map: every entry exists in the out-dir with the recorded sha256.
const map = input.receiptHashes;
const haveMap = map !== undefined && map !== null && typeof map === 'object' && !Array.isArray(map);
if (!haveMap) {
  fail('input.json has NO receiptHashes map (it pre-dates the receipt-hash acceptance step; re-run the runner measure stage to regenerate the input)');
} else {
  let good = 0;
  for (const [p, h] of Object.entries(map)) {
    const f = join(outDir, p);
    if (!fs.existsSync(f)) fail(`receiptHashes: ${p} does not exist`);
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
  else if (!fs.existsSync(join(outDir, p))) fail(`${where}: "${p}" does not exist in the out-dir`);
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

// 4. Input binary pins vs builds.json.
for (const arm of ['A', 'B'])
  for (const role of ['iai', 'controls', 'oracle', 'control']) {
    const pinned = input.identities?.[arm]?.binaries?.[role];
    const recorded = builds.arms?.[arm]?.[role]?.binarySha256;
    if (typeof pinned === 'string' && typeof recorded === 'string' && pinned !== recorded)
      fail(`identities.${arm}.binaries.${role} != builds.json arms.${arm}.${role}.binarySha256`);
  }
for (const [field, role] of [['binarySha256', 'iai'], ['controlsBinarySha256', 'controls']]) {
  const pinned = input.identities?.C0?.[field];
  const recorded = builds.arms?.C?.[role]?.binarySha256;
  if (typeof pinned === 'string' && typeof recorded === 'string' && pinned !== recorded)
    fail(`identities.C0.${field} != builds.json arms.C.${role}.binarySha256`);
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
  process.exit(0);
}
console.error(`verify_artifacts: FAIL — ${failures} problem(s) in ${outDir}`);
process.exit(1);
