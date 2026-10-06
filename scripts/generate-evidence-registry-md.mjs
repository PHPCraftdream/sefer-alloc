// Deterministic Markdown rendering of the evidence registry for plan step 7
// (#2098/Ph7).
//
// docs/evidence/registry.csv is the source of truth, but a raw 21-column CSV
// is not reviewable in a PR diff or a browser. This script renders it (together
// with required_cells.csv) into docs/evidence/REGISTRY.md: status/phase/scope
// summaries, the required-cells cross-check table, the C1–C7+COST gate view and
// the full row table. The output is FULLY DETERMINED by the two input files —
// no timestamps, no locale-dependent sorting — so that `--check` in CI can
// verify the committed REGISTRY.md is exactly what the current CSVs produce.
// Edit registry.csv, never REGISTRY.md.
//
// Usage:
//   node scripts/generate-evidence-registry-md.mjs             # write REGISTRY.md
//   node scripts/generate-evidence-registry-md.mjs --check     # verify, exit 1 on drift
//   node scripts/generate-evidence-registry-md.mjs --out <p>   # custom output path
//
// Exit codes: 0 = written / up to date, 1 = --check drift, 2 = usage/IO error.
// Pure stdlib; read-only with respect to the repository except for the
// generated file itself.

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '..');
const REGISTRY = join(repoRoot, 'docs', 'evidence', 'registry.csv');
const REQUIRED = join(repoRoot, 'docs', 'evidence', 'required_cells.csv');
const OUT = join(repoRoot, 'docs', 'evidence', 'REGISTRY.md');

// ---------------------------------------------------------------- CLI ---

const argv = process.argv.slice(2);
let check = false, out = OUT;
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--check') check = true;
  else if (argv[i] === '--out') { out = resolve(argv[++i] ?? ''); }
  else { console.error(`unknown argument: ${argv[i]}`); process.exit(2); }
}

// ---------------------------------------------------------------- CSV ---
// Same minimal RFC4180 parser as the judge: quoted fields, "" escapes,
// CRLF/CR normalized to LF, BOM stripped.
function parseCsv(text) {
  text = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const rows = [];
  let row = [], field = '', inQuotes = false;
  const endField = () => { row.push(field); field = ''; };
  const endRow = () => { endField(); rows.push(row); row = []; };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') { field += '"'; i++; } else inQuotes = false;
      } else field += c;
    } else if (c === '"' && field === '') inQuotes = true;
    else if (c === ',') endField();
    else if (c === '\n') endRow();
    else field += c;
  }
  if (field !== '' || row.length > 0) endRow();
  return rows.filter((r) => !(r.length === 1 && r[0].trim() === ''));
}

const STATUS = ['PASS', 'FAIL', 'KNOWN-DEFECT', 'KNOWN-RED', 'MODEL-LIMIT',
  'NOT_RUN', 'CFG_EXCLUDED', 'BUILD_ONLY', 'INCONCLUSIVE', 'CI_PENDING',
  'CONDITIONAL'];
const GATES = ['C1', 'C2', 'C3', 'C4', 'C5', 'C6', 'C7', 'COST'];

const registryCsv = parseCsv(readFileSync(REGISTRY, 'utf8'));
const requiredCsv = parseCsv(readFileSync(REQUIRED, 'utf8'));
const regHeader = registryCsv[0];
const reqHeader = requiredCsv[0];
if (!regHeader || !regHeader.includes('id') || !regHeader.includes('status')) {
  console.error('registry.csv: unexpected header');
  process.exit(2);
}
const rows = registryCsv.slice(1).map((cells) =>
  Object.fromEntries(regHeader.map((h, i) => [h, cells[i] ?? ''])));
const requiredRows = requiredCsv.slice(1).map((cells) =>
  Object.fromEntries(reqHeader.map((h, i) => [h, cells[i] ?? ''])));
const byId = new Map(rows.map((r) => [r.id, r]));

// ----------------------------------------------------------- helpers ---

// Escape for a markdown table cell; deterministic truncation to 97 chars
// (codepoints) with a `…` suffix.
function cell(v, truncate = false) {
  let s = String(v ?? '');
  if (truncate && [...s].length > 97) s = [...s].slice(0, 97).join('') + '…';
  return s.replace(/\|/g, '\\|').replace(/\r?\n/g, '<br>');
}
const byCodepoint = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

// ------------------------------------------------------------ render ---

const L = [];
L.push('# Evidence registry (generated)');
L.push('');
L.push('> **ВНИМАНИЕ:** этот файл сгенерирован скриптом' +
  ' `scripts/generate-evidence-registry-md.mjs` из `docs/evidence/registry.csv`' +
  ' — НЕ править руками; правки только в `registry.csv`.');
L.push('');
L.push(`Источник: sha256 registry.csv = ${createHash('sha256').update(readFileSync(REGISTRY, 'utf8').replace(/\r\n?/g, '\n')).digest('hex').slice(0, 8)}`);
L.push('');

// (a) status summary
L.push('## Сводка по статусам');
L.push('');
L.push('| Статус | Строк |');
L.push('|---|---:|');
const byStatus = Object.fromEntries(STATUS.map((s) => [s, 0]));
for (const r of rows) if (r.status in byStatus) byStatus[r.status]++;
for (const s of STATUS) L.push(`| ${s} | ${byStatus[s]} |`);
L.push(`| TOTAL | ${rows.length} |`);
L.push('');

// (b) owner_phase summary
L.push('## Сводка по owner_phase');
L.push('');
L.push('| Фаза | Всего | PASS | NOT_RUN | Прочие |');
L.push('|---|---:|---:|---:|---:|');
const byPhase = new Map();
for (const r of rows) {
  const p = byPhase.get(r.owner_phase) ?? { total: 0, pass: 0, notRun: 0 };
  p.total++;
  if (r.status === 'PASS') p.pass++;
  if (r.status === 'NOT_RUN') p.notRun++;
  byPhase.set(r.owner_phase, p);
}
for (const p of [...byPhase.keys()].sort(byCodepoint)) {
  const { total, pass, notRun } = byPhase.get(p);
  L.push(`| ${cell(p)} | ${total} | ${pass} | ${notRun} | ${total - pass - notRun} |`);
}
L.push('');

// (c) scope summary
L.push('## Сводка по scope');
L.push('');
L.push('| Scope | Строк |');
L.push('|---|---:|');
const byScope = new Map();
for (const r of rows) byScope.set(r.scope, (byScope.get(r.scope) ?? 0) + 1);
for (const s of [...byScope.keys()].sort(byCodepoint)) {
  L.push(`| ${cell(s)} | ${byScope.get(s)} |`);
}
L.push('');

// (d) required cells
L.push('## Обязательные клетки (required_cells)');
L.push('');
L.push('| cell_id | gate | class | expects | Статус в реестре | note |');
L.push('|---|---|---|---|---|---|');
for (const q of [...requiredRows].sort((a, b) => byCodepoint(a.cell_id, b.cell_id))) {
  const row = byId.get(q.cell_id);
  const status = row ? row.status : 'MISSING → NOT_RUN';
  L.push(`| ${cell(q.cell_id)} | ${cell(q.gate)} | ${cell(q.class)} | ${cell(q.expects)} | ${cell(status)} | ${cell(q.note)} |`);
}
L.push('');

// (e) gates C1–C7 + COST
L.push('## Гейты C1–C7 + COST');
L.push('');
L.push('| Gate | Клетки | Статусы |');
L.push('|---|---|---|');
for (const g of GATES) {
  const cells = requiredRows.filter((q) => q.gate === g)
    .sort((a, b) => byCodepoint(a.cell_id, b.cell_id));
  if (cells.length === 0) continue;
  const statuses = cells.map((q) => {
    const row = byId.get(q.cell_id);
    return `${q.cell_id}=${row ? row.status : 'MISSING → NOT_RUN'}`;
  });
  L.push(`| ${g} | ${cells.length} | ${cell(statuses.join(', '))} |`);
}
L.push('');

// (f) full row table
L.push('## Полная таблица строк');
L.push('');
L.push('| id | status | owner_phase | entry | features | os | profile | activation | positive_marker | mutant | receipt | scope | mutant_rejected | source_sha | status_reason |');
L.push('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
const fullCols = ['id', 'status', 'owner_phase', 'entry', 'features', 'os',
  'profile', 'activation', 'positive_marker', 'mutant', 'receipt', 'scope', 'mutant_rejected', 'source_sha', 'status_reason'];
for (const r of [...rows].sort((a, b) => byCodepoint(a.id, b.id))) {
  const tds = fullCols.map((c) => {
    if (c === 'receipt') {
      const tok = String(r.receipt ?? '').split(';').map((t) => t.trim()).filter((t) => t !== '')[0] ?? '';
      return cell(tok, true);
    }
    return cell(r[c], c === 'status_reason' ? r.status === 'PASS' : true);
  });
  L.push(`| ${tds.join(' | ')} |`);
}
L.push('');

const md = L.join('\n') + '\n';

// ------------------------------------------------------------- write ---

if (check) {
  if (!existsSync(out)) {
    console.error(`GENERATED-MD: missing ${out}`);
    process.exit(1);
  }
  const current = readFileSync(out, 'utf8').replace(/\r\n/g, '\n');
  if (current === md) {
    console.log('GENERATED-MD: up to date');
    process.exit(0);
  }
  const a = current.split('\n'), b = md.split('\n');
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  console.error(`GENERATED-MD: DRIFT at line ${i + 1} of ${out}`);
  console.error(`  expected: ${JSON.stringify(b[i] ?? '<eof>')}`);
  console.error(`  found:    ${JSON.stringify(a[i] ?? '<eof>')}`);
  process.exit(1);
} else {
  writeFileSync(out, md);
  console.log(`GENERATED-MD: wrote ${out} (${md.split('\n').length} lines)`);
}
