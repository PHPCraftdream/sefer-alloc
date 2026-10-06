#!/usr/bin/env node
// Pin evidence receipts: compute sha256 (LF-normalized content) of every
// receipt path token of each registry row and record the ';'-joined digest
// list in the row's receipt_sha256 column.
//
// Why: the judge (scripts/verify-evidence-registry.mjs) requires strong-status
// rows (PASS / KNOWN-DEFECT / KNOWN-RED) to pin the exact bytes of every cited
// receipt file, so that later edits to a receipt cannot silently retro-validate
// an old claim. Digests are computed after normalizing CRLF/CR to LF so that
// checkout settings do not change the pin.
//
// Usage:
//   node scripts/pin-evidence-receipts.mjs           # update registry.csv in place
//   node scripts/pin-evidence-receipts.mjs --check   # verify only; exit 1 on drift
//
// Note: the orchestrator re-runs this script after editing receipts directly
// (e.g. R6a-G/G2 landing updates) so pins always reflect current receipt bytes.

import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = (spawnSync('git', ['rev-parse', '--show-toplevel'],
  { encoding: 'utf8' }).stdout || '').trim();
const registryPath = join(root, 'docs', 'evidence', 'registry.csv');
const check = process.argv.slice(2).includes('--check');

function parseCSV(text) {
  text = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const rows = []; let row = [], field = '', q = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (q) {
      if (c === '"') { if (text[i + 1] === '"') { field += '"'; i++; } else q = false; }
      else field += c;
    } else {
      if (c === '"' && field === '') q = true;
      else if (c === ',') { row.push(field); field = ''; }
      else if (c === '\n') { row.push(field); rows.push(row); row = []; field = ''; }
      else field += c;
    }
  }
  if (field !== '' || row.length) { row.push(field); rows.push(row); }
  return rows;
}
const csvField = (s) =>
  /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s;

const lf = (s) => s.replace(/\r\n?/g, '\n');
const sha256Lf = (path) =>
  createHash('sha256').update(lf(readFileSync(path, 'utf8'))).digest('hex');

const parsed = parseCSV(readFileSync(registryPath, 'utf8'));
const header = parsed[0];
const rows = parsed.slice(1).map((cells) => {
  const o = {}; header.forEach((h, i) => (o[h] = cells[i] ?? '')); return o;
});

const isExternal = (t) => /^external:([0-9a-f]{64})$/i.test(t.trim());
const STRONG = new Set(['PASS', 'KNOWN-DEFECT', 'KNOWN-RED']);
let changed = 0; const drift = [];
for (const row of rows) {
  const paths = row.receipt.split(';').map((t) => t.trim())
    .filter((t) => t !== '' && !isExternal(t));
  if (paths.length === 0 && !STRONG.has(row.status)) {
    // nothing to pin for this row; leave the column untouched
    continue;
  }
  const pins = paths.map((rel) => {
    const abs = join(root, rel);
    if (!existsSync(abs) || !statSync(abs).isFile()) return null;
    return sha256Lf(abs);
  });
  if (pins.some((s) => s === null)) {
    drift.push(`${row.id}: missing receipt file, cannot pin`);
    continue;
  }
  const want = pins.join(';');
  if (row.receipt_sha256 !== want) {
    if (check) drift.push(`${row.id}: receipt_sha256 drift for ${paths.join(', ')}`);
    else { row.receipt_sha256 = want; changed++; console.log(`pinned ${row.id}`); }
  }
}

if (check) {
  if (drift.length > 0) {
    for (const d of drift) console.error('DRIFT: ' + d);
    process.exit(1);
  }
  console.log('receipt pins OK');
} else {
  const out = [header.join(',')]
    .concat(rows.map((r) => header.map((h) => csvField(r[h])).join(',')))
    .join('\n') + '\n';
  writeFileSync(registryPath, out, 'utf8');
  console.log(`updated ${changed} row(s)`);
}
