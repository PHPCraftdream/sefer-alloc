// Derives the committed PRF-03 gate artifacts (CSV tables, manifest, verbatim judge output, report tables)
// from a retained run directory. One checked script; every ratio/percentage is asserted before it is printed.
// Usage: node scripts/r19_prf03_gate_excerpt.mjs --run RUNDIR --judge JUDGE_OUTPUT.json --review REVIEW.json --out OUTDIR
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { resolve } from 'node:path';
import { diffProfiles } from './r19_prf03_cg_diff.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((acc, x, i, a) => (i % 2 === 0 ? [...acc, [x.replace(/^--/, ''), a[i + 1]]] : acc), []));
for (const k of ['run', 'judge', 'review', 'out']) assert.ok(args[k], `--${k} required`);
const run = resolve(args.run), out = resolve(args.out);
assert.ok(!existsSync(out), 'output exists; refusing overwrite');
mkdirSync(out, { recursive: true });
const sha = b => createHash('sha256').update(b).digest('hex');
const rd = p => readFileSync(resolve(run, p));
const bundle = JSON.parse(rd('bundle.json'));
const input = JSON.parse(rd(bundle.input.path));
const judgeBytes = readFileSync(resolve(args.judge));
const judge = JSON.parse(judgeBytes);
const review = readFileSync(resolve(args.review));
const env = JSON.parse(rd(bundle.environment.path));
assert.equal(input.iai.length, 135); assert.equal(input.native.length, 81);

// --- iai table, recomputed independently of the judge and cross-checked against it
const arms = ['A1', 'A2', 'B1', 'B2', 'C0'];
const iaiRows = [];
for (const c of input.inventory) {
  const v = arms.map(a => { const r = input.iai.find(x => x.name === c.name && x.arm === a); assert.ok(r); return r.hit - (r.prefill ?? 0); });
  const [a1, a2, b1, b2, c0] = v, T = Math.max(10, Math.abs(c0 - a1));
  const deltas = [b1 - a1, b1 - a2, b2 - a1, b2 - a2];
  const jt = judge.tables.iai.find(t => t.name === c.name);
  assert.ok(jt); assert.deepEqual([jt.A1, jt.A2, jt.B1, jt.B2, jt.C0, jt.T], [a1, a2, b1, b2, c0, T]); assert.deepEqual(jt.deltas, deltas);
  const pass = deltas.every(d => c.kind === 'R1' ? d <= -200 : ['R2', 'R3'].includes(c.kind) ? d <= T : c.kind === 'control' ? Math.abs(d) <= T : c.kind === 'mimalloc' ? d === 0 : true);
  assert.equal(jt.pass, pass);
  iaiRows.push({ name: c.name, kind: c.kind, a1, a2, b1, b2, c0, T, deltas, pass });
}
const csv = rows => rows.map(r => r.join(',')).join('\n') + '\n';
writeFileSync(resolve(out, 'iai_summary.csv'), csv([['name', 'kind', 'A1', 'A2', 'B1', 'B2', 'C0', 'T', 'B1-A1', 'B1-A2', 'B2-A1', 'B2-A2', 'pass'],
  ...iaiRows.map(r => [r.name, r.kind, r.a1, r.a2, r.b1, r.b2, r.c0, r.T, ...r.deltas, r.pass])]));

// --- native tables (medians over exactly 9 per-process values, never pooled)
const median = xs => { assert.equal(xs.length, 9); return [...xs].sort((a, b) => a - b)[4]; };
const nativeRows = input.native.map(r => [r.n, r.arm, r.sample, r.nsPerOp, r.p99Ns, r.config.runId, r.config.pid]);
writeFileSync(resolve(out, 'native_rows.csv'), csv([['n', 'arm', 'sample', 'nsPerOp', 'p99Ns', 'runId', 'pid'], ...nativeRows]));
const native = [];
for (const n of [1, 4, 8]) for (const metric of ['nsPerOp', 'p99Ns']) {
  const m = arm => median(input.native.filter(r => r.n === n && r.arm === arm).map(r => r[metric]));
  const A = m('A'), AA = m('AA'), B = m('B'), noise = Math.abs(AA - A) / A;
  const jt = judge.tables.native.find(t => t.n === n && t.metric === metric);
  assert.ok(jt); assert.equal(jt.A, A); assert.equal(jt.AA, AA); assert.equal(jt.B, B); assert.ok(Math.abs(jt.noise - noise) < 1e-12);
  native.push({ n, metric, A, AA, B, noise, pass: B <= A * (1 + noise) });
  assert.equal(jt.pass, B <= A * (1 + noise));
}
writeFileSync(resolve(out, 'native_summary.csv'), csv([['n', 'metric', 'medianA', 'medianAA', 'medianB', 'noise(|AA-A|/A)', 'noiseLimit', 'B<=A*(1+noise)'],
  ...native.map(r => [r.n, r.metric, r.A, r.AA, r.B, r.noise.toFixed(6), 0.05, r.pass])]));

// --- oracle summary (per arm / regime), taken from the retained raw outputs of the oracle processes
const oracle = [];
for (const p of bundle.processes) {
  if (!(p.role === 'primary' || p.role === 'paired') || !p.raw) continue;
  const r = JSON.parse(rd(p.raw.path));
  if (r.regime.startsWith('probe-')) continue;
  const s = r.structural;
  oracle.push([p.role, p.arm, r.regime, r.negativeControl, s.slotsBefore, s.slotsAfterAlloc ?? '', s.slotsAfterFree, s.cycleReservedDelta, s.cycleReleasedDelta,
    r.oracle.incarnationDelta ?? '', s.cachedCensus, r.oracle.tailHitDelta ?? '', r.routedScans ?? '']);
}
writeFileSync(resolve(out, 'oracle_summary.csv'), csv([['role', 'arm', 'regime', 'negativeControl', 'slotsBefore', 'slotsAfterAlloc', 'slotsAfterFree', 'cycleReserved', 'cycleReleased',
  'incarnationDelta', 'cachedCensus', 'tailHitDelta', 'routedScans'], ...oracle]));

// --- function-level diagnostics from the retained Callgrind profiles (A1 vs B1)
const profileOf = (name, arm) => { const p = bundle.processes.find(x => x.role === 'iai' && x.name === name && x.arm === arm); assert.ok(p); return rd(p.profile.path).toString(); };
const diag = [];
for (const name of ['small_churn_16b', 'cold_alloc_only_256x16b', 'mimalloc_recycle_alloc_free_256x16b', 'large_cache_payload_prefill_only_4mib', 'large_cache_payload_hit_only_4mib']) {
  const d = diffProfiles(profileOf(name, 'A1'), profileOf(name, 'B1'), 8);
  diag.push(`### ${name} (A1 exclusive ${d.totalA} Ir, B1 exclusive ${d.totalB} Ir, delta ${d.delta})\n` + d.rows.map(x => `${String(x.d).padStart(7)} ${String(x.a).padStart(8)} ${String(x.b).padStart(8)}  ${x.fn.slice(0, 140)}`).join('\n'));
}
writeFileSync(resolve(out, 'diagnostic_function_diff.txt'), '# Exclusive Ir per function, arm A1 vs arm B1 (columns: B-A, A, B). Derived by scripts/r19_prf03_cg_diff.mjs from retained raw profiles (tier 3, not committed).\n' + diag.join('\n\n') + '\n');

// --- verbatim judge output and review receipt; input.json by tier policy
writeFileSync(resolve(out, 'judge_output.json'), judgeBytes);
writeFileSync(resolve(out, 'review_receipt.json'), review);
const inputBytes = rd(bundle.input.path);
let inputNote;
if (inputBytes.length < 200 * 1024) { writeFileSync(resolve(out, 'input.json'), inputBytes); inputNote = 'verbatim'; }
else if (inputBytes.length < 2 * 1024 * 1024) { writeFileSync(resolve(out, 'input.json.gz'), gzipSync(inputBytes, { level: 9 })); inputNote = 'gzip (tier 2)'; }
else inputNote = 'hash only (tier 3)';

// --- manifest: identities and hashes of everything retained outside Git
const lines = [];
const A = bundle.sources.A, B = bundle.sources.B;
lines.push(`armA baseHead ${A.baseHead} tree ${A.tree} identitySha256 ${A.identitySha256}`);
lines.push(`armB baseHead ${B.baseHead} tree ${B.tree} identitySha256 ${B.identitySha256}`);
lines.push(`environmentSha256 ${input.environmentSha256}`);
lines.push(`rustc ${env.rustc}; cargo ${env.cargo}; target ${env.target}`);
lines.push(`input.json ${sha(inputBytes)} ${inputBytes.length} bytes, committed as: ${inputNote}`);
lines.push(`bundle.json ${sha(rd('bundle.json'))} ${rd('bundle.json').length} bytes, tier 3 (not committed)`);
for (const [k, b] of Object.entries(bundle.builds).sort(([x], [y]) => x.localeCompare(y))) lines.push(`binary ${k} ${b.binarySha256}`);
lines.push(`judge_output.json ${sha(judgeBytes)}`);
lines.push(`review_receipt.json ${sha(review)}`);
lines.push(`processes ${bundle.processes.length} (iai ${bundle.processes.filter(p => p.role === 'iai').length}, oracle/probe ${bundle.processes.filter(p => ['primary', 'paired'].includes(p.role)).length}, twin ${bundle.processes.filter(p => p.role === 'twin').length}, native ${bundle.processes.filter(p => p.role === 'native').length})`);
writeFileSync(resolve(out, 'MANIFEST.txt'), lines.join('\n') + '\n');

// --- report tables (markdown), printed for inclusion in the report by the author of the report
const pct = (num, den) => `${num}/${den} = ${(100 * num / den).toFixed(2)} %`;
const md = [];
md.push('| cell | kind | A1 | A2 | B1 | B2 | C0 | T | B-A (4 cross pairs) | pass |', '|---|---|---:|---:|---:|---:|---:|---:|---|---|');
for (const r of iaiRows) md.push(`| \`${r.name}\` | ${r.kind} | ${r.a1} | ${r.a2} | ${r.b1} | ${r.b2} | ${r.c0} | ${r.T} | ${[...new Set(r.deltas)].join(' / ')} | ${r.pass ? 'yes' : '**NO**'} |`);
md.push('', '| n | metric | median A | median AA | median B | A/A noise (num/den) | limit | B vs A (num/den) | B<=A(1+noise) |', '|---:|---|---:|---:|---:|---:|---:|---|---|');
const f = x => (Number.isInteger(x) ? String(x) : x.toFixed(1));
for (const r of native) md.push(`| ${r.n} | ${r.metric} | ${f(r.A)} | ${f(r.AA)} | ${f(r.B)} | ${(100 * r.noise).toFixed(2)} % (${f(Math.abs(r.AA - r.A))}/${f(r.A)}) | 5 % | ${f(r.B - r.A)}/${f(r.A)} = ${(100 * (r.B - r.A) / r.A).toFixed(1)} % | ${r.pass ? 'yes' : 'NO'} |`);
const failing = iaiRows.filter(r => !r.pass);
md.push('', `iai cells failing their frozen criterion: ${pct(failing.length, iaiRows.length)}; native axes with A/A noise above 5 %: ${pct(native.filter(r => r.noise > 0.05).length, native.length)}.`);
writeFileSync(resolve(out, 'report_tables.md'), md.join('\n') + '\n');
console.log(JSON.stringify({ verdict: judge.verdict, iaiFailing: failing.length, iaiCells: iaiRows.length, nativeNoisy: native.filter(r => r.noise > 0.05).length, out }));
