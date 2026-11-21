import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../scripts/ghost_probe_stability_report.mjs', import.meta.url));
const perf = new URL('../docs/perf/', import.meta.url);
const raw = readFileSync(new URL('_raw_ghost_zero_cost_probe_stability_20261121.log', perf), 'utf8');

function run(...args) {
  const result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', shell: false });
  assert.ifError(result.error);
  assert.equal(result.signal, null);
  return result;
}

function runWithRow(t, probe, pair, mutate) {
  const prefix = `PROBE ${probe} PAIR ${pair} `;
  const lines = raw.split(/\r?\n/);
  const matches = lines.filter(line => line.startsWith(prefix));
  assert.equal(matches.length, 1, `expected one raw row for ${probe}/${pair}`);
  const original = matches[0];
  const replacement = mutate(original);
  assert.notEqual(replacement, original);
  assert.ok(replacement.startsWith(prefix));
  lines[lines.indexOf(original)] = replacement;

  const dir = mkdtempSync(join(tmpdir(), 'ghost-probe-report-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const path = join(dir, 'raw.log');
  writeFileSync(path, lines.join('\n'));
  return run('--raw', path);
}

function withCounts(line, base, delta) {
  const pct = `${delta >= 0 ? '+' : ''}${(100 * delta / base).toFixed(15)}`;
  return line.replace(/ir_base=\d+ ir_head=\d+ delta=\S+ pct=\S+/,
    `ir_base=${base} ir_head=${base + delta} delta=${delta} pct=${pct}`);
}

function assertRejected(t, result, diagnostic) {
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, '');
  assert.match(result.stderr.trim(), diagnostic);
  t.diagnostic(result.stderr.trim());
}

test('default CLI emits historical Markdown, verdicts, and CSV', () => {
  const { status, stdout, stderr } = run();
  assert.equal(status, 0, stderr);
  assert.equal(stderr, '');
  const end = '<!-- END GENERATED GHOST PROBE RESULTS -->';
  const sections = stdout.split(`${end}\n\n`);
  assert.equal(sections.length, 2);
  const [markdown, csv] = sections;
  assert.ok(markdown.startsWith('<!-- BEGIN GENERATED GHOST PROBE RESULTS -->\n'));
  assert.equal(markdown.split('\n').filter(line => /^\| (old|pa|pb|pc) \|/.test(line)).length, 16);
  assert.match(markdown, /^\| old \| p23v0 \| 10225975 \| 10275190 \| \+0\.481274401707417% \(\+49215\/10225975\) \| false \|$/m);
  assert.match(markdown, /^\| pa \| ctl \| 10412630 \| 10412630 \| \+0\.000000000000000% \(\+0\/10412630\) \| true \|$/m);
  assert.match(markdown, /\*\*Calibration passes\.\*\*/);
  assert.match(markdown, /^- \*\*pa: qualifies\.\*\* All 4 pairs meet the absolute 0\.1% threshold\.$/m);
  assert.match(markdown, /^- \*\*pb: rejected\.\*\* Outside the absolute 0\.1% threshold on p22v0 .*, p23v0 .*\.$/m);
  assert.match(markdown, /^- \*\*pc: qualifies\.\*\* .*pc observed Ir 12132705 vs pa 10412630 \(higher\)\.$/m);

  const lines = csv.split('\n');
  assert.equal(lines.pop(), '');
  assert.equal(lines.shift(), 'source_sha,run_id,features,cpu_os,samples_per_arm,probe,pair,ir_base,ir_head,delta,pct,text_equal');
  assert.equal(lines.length, 16);
  const metadata = 'f6aef10dc301f18683e97b2fc032404cc4efdf2a,44928351811,production,x86_64 Ubuntu 26.04.1 LTS,1';
  assert.equal(lines[2], `${metadata},old,p23v0,10225975,10275190,49215,+0.481274401707417,false`);
  assert.equal(lines[4], `${metadata},pa,ctl,10412630,10412630,0,+0.000000000000000,true`);
  assert.equal(lines[9], `${metadata},pb,p22v0,10491189,10423452,-67737,-0.645656083404846,false`);
  assert.equal(lines[15], `${metadata},pc,p23v4,12135248,12132705,-2543,-0.020955484387299,false`);
});

test('rejects tampered delta with Ir counts unchanged', t => {
  const result = runWithRow(t, 'pa', 'p22v0', line => line.replace('delta=-4383', 'delta=-4384'));
  assertRejected(t, result, /^ghost_probe_stability_report: raw line \d+ \(pa\/p22v0\): delta -4384 does not match head − base \(-4383\)$/);
});

test('rejects tampered pct with Ir counts and delta unchanged', t => {
  const result = runWithRow(t, 'pa', 'p22v0', line => line.replace('pct=-0.042094765372345', 'pct=-0.042094765372346'));
  assertRejected(t, result, /^ghost_probe_stability_report: raw line \d+ \(pa\/p22v0\): pct -0\.042094765372346 does not match recomputed -0\.042094765372345$/);
});

test('rejects arithmetically consistent but drifted old calibration', t => {
  const result = runWithRow(t, 'old', 'p23v0', line => withCounts(line, 10225975, 49216));
  assertRejected(t, result, /^ghost_probe_stability_report: old\/p23v0: calibration expected delta \+49215 exactly; got \+49216$/);
});

test('accepts pa exactly at the absolute 0.1% threshold', t => {
  const { status, stdout, stderr } = runWithRow(t, 'pa', 'p22v0', line => withCounts(line, 10412000, -10412));
  assert.equal(status, 0, stderr);
  assert.equal(stderr, '');
  assert.match(stdout, /^\| pa \| p22v0 \| 10412000 \| 10401588 \| -0\.100000000000000% \(-10412\/10412000\) \| false \|$/m);
  assert.match(stdout, /\*\*pa: qualifies\.\*\*/);
});

test('rejects arithmetically consistent pa one instruction beyond the threshold', t => {
  const result = runWithRow(t, 'pa', 'p22v0', line => withCounts(line, 10412000, -10413));
  assertRejected(t, result, /^ghost_probe_stability_report: pa exceeds the absolute 0\.1% threshold: p22v0 -0\.\d{15}% \(-10413\/10412000\)$/);
});

test('rejects ctl text mismatch even with zero delta', t => {
  const result = runWithRow(t, 'pc', 'ctl', line => line.replace('text_equal=true', 'text_equal=false'));
  assertRejected(t, result, /^ghost_probe_stability_report: pc\/ctl: expected delta=0 and text_equal=true; got delta=0 text_equal=false$/);
});

test('--check verifies the real report and summary', () => {
  const { status, stdout, stderr } = run('--check');
  assert.equal(status, 0, stderr);
  assert.equal(stderr, '');
  assert.equal(stdout.trim(), '--check ok: generated Markdown block and summary CSV match');
});
