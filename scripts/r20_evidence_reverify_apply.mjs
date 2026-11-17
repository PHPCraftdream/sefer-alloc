#!/usr/bin/env node
// Apply r20-gates EV-1 reverify output (verdict.txt + proposed_rows.csv + receipts) to the evidence registry.
// Validate all inputs before writing evidence; see docs/evidence/REGISTRY.md.
import { createHash } from 'node:crypto';
import {
  existsSync, lstatSync, mkdirSync, readFileSync, rmSync, writeFileSync,
  realpathSync, unlinkSync, mkdtempSync, copyFileSync,
} from 'node:fs';
import { dirname, isAbsolute, join, parse, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';

export const REGISTRY_REL = 'docs/evidence/registry.csv';
export const RECEIPTS_DIR_REL = 'docs/evidence';
export const CONFIG_PATHS = ['.github/evidence-reverify.json', '.github/gate-run.json'];
export const RECEIPT_LIMIT = 200 * 1024;
export const REGISTRY_HEADER = ['id', 'transition', 'entry', 'features', 'os', 'toolchain', 'profile', 'source_sha', 'source_tree', 'binary_id', 'activation', 'positive_marker', 'mutant', 'mutant_rejected', 'receipt', 'receipt_sha256', 'scope', 'status', 'status_reason', 'owner_phase', 'source_cell'];

// --- small utils ---
function sha256(buf) { return createHash('sha256').update(buf).digest('hex'); }
function fail(msg) { throw new Error(msg); }
function isMissing(error) { return error?.code === 'ENOENT'; }

export function parseCsv(text) {
  if (typeof text !== 'string') throw new TypeError('CSV text must be a string');
  const rows = [];
  let cells = [];
  let field = '';
  let inQuotes = false;
  let closedQuote = false;
  let recordStart = 0;

  for (let i = 0; i < text.length; i += 1) {
    const char = text[i];
    if (inQuotes) {
      if (char === '"' && text[i + 1] === '"') {
        field += '"';
        i += 1;
      } else if (char === '"') {
        inQuotes = false;
        closedQuote = true;
      } else {
        // LF is valid inside a quoted CSV field (registry cells are multiline).
        // A bare CR is not a record or field newline; accept only CRLF there.
        if (char === '\r' && text[i + 1] !== '\n') throw new Error('CSV quote');
        field += char;
      }
    } else if (char === ',') {
      cells.push(field);
      field = '';
      closedQuote = false;
    } else if (char === '\r' || char === '\n') {
      if (char === '\r' && text[i + 1] !== '\n') throw new Error('CSV record');
      const end = i + (char === '\r' ? 2 : 1);
      cells.push(field);
      rows.push({ cells, raw: text.slice(recordStart, end) });
      cells = [];
      field = '';
      closedQuote = false;
      recordStart = end;
      if (char === '\r') i += 1;
    } else if (char === '"') {
      if (field !== '' || closedQuote) throw new Error('CSV quote');
      inQuotes = true;
    } else {
      if (closedQuote) throw new Error('CSV quote');
      field += char;
    }
  }
  if (inQuotes) throw new Error('CSV quote');
  if (recordStart < text.length) {
    cells.push(field);
    rows.push({ cells, raw: text.slice(recordStart) });
  }
  return rows;
}

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = resolve(SCRIPT_DIR, '..');
const REQUIRED_REL = 'docs/evidence/required_cells.csv';
const GENERATED_REL = 'docs/evidence/REGISTRY.md';
const ID_RE = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const STATUS_REASON = (status) => `E0 verified; preserve ${status}`;

function csvField(value) {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}
function csvRecord(cells, ending = '\n') {
  return cells.map(csvField).join(',') + ending;
}
function recordEnding(raw) {
  if (raw.endsWith('\r\n')) return '\r\n';
  if (raw.endsWith('\n')) return '\n';
  return '';
}
function rawCells(raw) {
  const body = raw.slice(0, raw.length - recordEnding(raw).length);
  const fields = [];
  let start = 0, quoted = false;
  for (let i = 0; i < body.length; i += 1) {
    if (quoted) {
      if (body[i] === '"' && body[i + 1] === '"') i += 1;
      else if (body[i] === '"') quoted = false;
    } else if (body[i] === '"') quoted = true;
    else if (body[i] === ',') { fields.push(body.slice(start, i)); start = i + 1; }
  }
  fields.push(body.slice(start));
  return fields;
}
function inside(parent, child) {
  const rel = relative(parent, child);
  return rel === '' || (!rel.startsWith(`..${sep}`) && rel !== '..' && !isAbsolute(rel));
}
function checkLexicalContainment(parent, child, label) {
  if (!inside(resolve(parent), resolve(child))) fail(`${label}: path escapes its root`);
}
function decodeUtf8(buffer, label) {
  try { return new TextDecoder('utf-8', { fatal: true }).decode(buffer); }
  catch { fail(`${label}: invalid UTF-8`); }
}
function parseTable(text, label, expectedHeader = null) {
  const records = parseCsv(text);
  if (!records.length) fail(`${label}: empty CSV`);
  const header = records[0].cells;
  if (expectedHeader && (header.length !== expectedHeader.length ||
      header.some((value, i) => value !== expectedHeader[i]))) fail(`${label}: unexpected header`);
  for (let i = 1; i < records.length; i += 1) {
    if (records[i].cells.length !== header.length) fail(`${label}: record ${i + 1} has wrong column count`);
  }
  return { records, header };
}
function rowsById(table, label) {
  const idIndex = table.header.indexOf('id');
  if (idIndex < 0) fail(`${label}: missing id column`);
  const result = new Map();
  for (const record of table.records.slice(1)) {
    const id = record.cells[idIndex];
    if (!ID_RE.test(id)) fail(`${label}: invalid ID ${JSON.stringify(id)}`);
    if (result.has(id)) fail(`${label}: duplicate ID ${id}`);
    result.set(id, record);
  }
  return result;
}
function asObject(header, cells) {
  return Object.fromEntries(header.map((name, i) => [name, cells[i]]));
}
function verifyVerdict(text) {
  if (!text.endsWith('\n') || text.includes('\r') || text.includes('\uFEFF')) fail('verdict.txt: expected LF-terminated format');
  const lines = text.slice(0, -1).split('\n');
  if (lines.length < 4 || lines[0] !== 'E0 PASS') fail('verdict.txt: expected E0 PASS');
  const headMatch = /^HEAD ([0-9a-f]{40})$/.exec(lines[1]);
  const treeMatch = /^TREE ([0-9a-f]{40})$/.exec(lines[2]);
  if (!headMatch || !treeMatch) fail('verdict.txt: malformed HEAD/TREE identity');
  const verdicts = new Map();
  for (const line of lines.slice(3)) {
    const match = /^([A-Za-z0-9][A-Za-z0-9._-]*): pass(?: — (.+))?$/.exec(line);
    if (!match) fail(`verdict.txt: malformed or non-pass result: ${line}`);
    if (verdicts.has(match[1])) fail(`verdict.txt: duplicate ID ${match[1]}`);
    const reason = match[2] ?? '';
    const preserveStatuses = [...reason.matchAll(/\bpreserve\s+([A-Z][A-Z0-9_-]*)\b/g)]
      .map((match) => match[1]);
    verdicts.set(match[1], { reason, preserveStatuses });
  }
  if (!verdicts.size) fail('verdict.txt: no passing IDs');
  return { head: headMatch[1], tree: treeMatch[1], verdicts };
}
function renderRegistry(validated) {
  const { registry, proposedById, proposedObjects } = validated;
  const merged = [registry.records[0].raw];
  for (const original of registry.records.slice(1)) {
    const id = original.cells[0];
    if (!proposedById.has(id)) { merged.push(original.raw); continue; }
    const replacement = proposedObjects.get(id);
    const old = asObject(REGISTRY_HEADER, original.cells);
    const values = REGISTRY_HEADER.map((key) =>
      key === 'status_reason' && replacement[key] === STATUS_REASON(replacement.status)
        ? old[key] : replacement[key]);
    const previousRawCells = rawCells(original.raw);
    const ending = recordEnding(original.raw);
    const encoded = values.map((value, i) =>
      value === original.cells[i] ? previousRawCells[i] : csvField(value));
    merged.push(encoded.join(',') + ending);
  }
  return Buffer.from(merged.join(''), 'utf8');
}
function runGenerator(validated, registryBytes) {
  const stage = mkdtempSync(join(tmpdir(), 'r20-evidence-apply-'));
  try {
    const scriptsDir = join(stage, 'scripts');
    const evidenceDir = join(stage, 'docs', 'evidence');
    mkdirSync(scriptsDir, { recursive: true });
    mkdirSync(evidenceDir, { recursive: true });
    copyFileSync(validated.generatorPath, join(scriptsDir, 'generate-evidence-registry-md.mjs'));
    writeFileSync(join(evidenceDir, 'required_cells.csv'), validated.requiredBytes);
    writeFileSync(join(evidenceDir, 'registry.csv'), registryBytes);
    const result = spawnSync(process.execPath, [join(scriptsDir, 'generate-evidence-registry-md.mjs')], {
      cwd: stage, encoding: 'utf8', windowsHide: true,
    });
    if (result.error || result.status !== 0) {
      fail(`staged registry generator failed: ${result.stderr || result.error || `exit ${result.status}`}`);
    }
    const generated = join(evidenceDir, 'REGISTRY.md');
    const info = inspectPath(generated, { expect: 'file' });
    return readFileSync(info.real);
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}
function applyWrites(validated, registryBytes, markdownBytes) {
  const writes = [{ path: validated.registryPath, bytes: registryBytes },
    { path: resolve(validated.root, GENERATED_REL), bytes: markdownBytes }];
  for (const [id, bytes] of validated.receiptBuffers) {
    writes.push({ path: resolve(validated.root, `${RECEIPTS_DIR_REL}/_raw_reverify_${id}.log`), bytes });
  }
  const snapshots = new Map();
  const changed = [];
  for (const item of writes) {
    const info = inspectPath(item.path, { allowMissing: true, expect: 'file' });
    snapshots.set(item.path, info.exists ? readFileSync(item.path) : null);
  }
  for (const rel of CONFIG_PATHS) {
    const path = resolve(validated.root, rel);
    const info = inspectPath(path, { allowMissing: true, expect: 'file' });
    if (info.exists) snapshots.set(path, readFileSync(path));
  }
  try {
    for (const item of writes) {
      changed.push(item.path);
      writeFileSync(item.path, item.bytes);
    }
    for (const rel of CONFIG_PATHS) {
      const path = resolve(validated.root, rel);
      const info = inspectPath(path, { allowMissing: true, expect: 'file' });
      if (info.exists) {
        snapshots.set(path, readFileSync(path));
        unlinkSync(path);
        changed.push(path);
      }
    }
  } catch (error) {
    const rollbackErrors = [];
    for (const path of changed.reverse()) {
      try {
        const prior = snapshots.get(path);
        if (prior === null) { if (existsSync(path)) unlinkSync(path); }
        else writeFileSync(path, prior);
      } catch (rollbackError) { rollbackErrors.push(`${path}: ${rollbackError.message}`); }
    }
    if (rollbackErrors.length) fail(`${error.message}; rollback incomplete: ${rollbackErrors.join('; ')}`);
    throw error;
  }
}
function validateInputs({ repoRoot, inDir }) {
  const root = resolve(repoRoot);
  const rootInfo = inspectPath(root, { expect: 'dir' });
  if (resolve(rootInfo.real) !== root) fail(`repository root resolves elsewhere: ${root}`);
  const input = resolve(inDir);
  const inputInfo = inspectPath(input, { expect: 'dir' });
  if (resolve(inputInfo.real) !== input) fail(`input directory resolves elsewhere: ${input}`);
  for (const rel of [REGISTRY_REL, REQUIRED_REL, GENERATED_REL, ...CONFIG_PATHS]) {
    const abs = resolve(root, rel);
    checkLexicalContainment(root, abs, rel);
    inspectPath(abs, { allowMissing: CONFIG_PATHS.includes(rel) || rel === GENERATED_REL, expect: 'file' });
  }
  const registryPath = resolve(root, REGISTRY_REL);
  const requiredPath = resolve(root, REQUIRED_REL);
  const registryInfo = inspectPath(registryPath, { expect: 'file' });
  const requiredInfo = inspectPath(requiredPath, { expect: 'file' });
  const generatorPath = resolve(root, 'scripts/generate-evidence-registry-md.mjs');
  checkLexicalContainment(root, generatorPath, 'generator');
  const generatorInfo = inspectPath(generatorPath, { expect: 'file' });
  const registryText = decodeUtf8(readFileSync(registryPath), REGISTRY_REL);
  const requiredBytes = readFileSync(requiredPath);
  parseTable(decodeUtf8(requiredBytes, REQUIRED_REL), REQUIRED_REL);
  const proposedPath = resolve(input, 'proposed_rows.csv');
  const verdictPath = resolve(input, 'verdict.txt');
  checkLexicalContainment(input, proposedPath, 'proposed_rows.csv');
  checkLexicalContainment(input, verdictPath, 'verdict.txt');
  inspectPath(proposedPath, { expect: 'file' });
  inspectPath(verdictPath, { expect: 'file' });
  const registry = parseTable(registryText, REGISTRY_REL, REGISTRY_HEADER);
  const proposed = parseTable(decodeUtf8(readFileSync(proposedPath), 'proposed_rows.csv'), 'proposed_rows.csv', REGISTRY_HEADER);
  const existingById = rowsById(registry, REGISTRY_REL);
  const proposedById = rowsById(proposed, 'proposed_rows.csv');
  const verdict = verifyVerdict(decodeUtf8(readFileSync(verdictPath), 'verdict.txt'));
  const identity = verdict;
  if (proposedById.size !== verdict.verdicts.size) fail('proposal/verdict ID sets differ');
  const proposedObjects = new Map();
  const receiptBuffers = new Map();
  for (const [id, proposalRecord] of proposedById) {
    if (!verdict.verdicts.has(id)) fail(`proposed row has no passing verdict: ${id}`);
    const oldRecord = existingById.get(id);
    if (!oldRecord) fail(`unknown registry ID: ${id}`);
    const old = asObject(REGISTRY_HEADER, oldRecord.cells);
    const candidate = asObject(REGISTRY_HEADER, proposalRecord.cells);
    if (old.status === 'KNOWN-RED' && candidate.status !== 'KNOWN-RED') fail(`${id}: cannot change existing KNOWN-RED status`);
    if (candidate.status !== old.status) fail(`${id}: status must be preserved (${old.status})`);
    if (candidate.status_reason !== old.status_reason && candidate.status_reason !== STATUS_REASON(old.status)) {
      fail(`${id}: invalid status_reason`);
    }
    if (candidate.status_reason === STATUS_REASON(old.status) && candidate.status !== old.status) {
      fail(`${id}: preserve status_reason requires existing status`);
    }
    const verdictEntry = verdict.verdicts.get(id);
    if (verdictEntry.preserveStatuses.some((status) => status !== old.status)) {
      fail(`${id}: verdict preserve status does not match existing status`);
    }
    if (candidate.source_sha !== identity.head || candidate.source_tree !== identity.tree) fail(`${id}: source identity mismatch`);
    if (!/^[0-9a-f]{64}$/.test(candidate.receipt_sha256)) fail(`${id}: malformed receipt_sha256`);
    const expectedReceipt = `docs/evidence/_raw_reverify_${id}.log`;
    if (candidate.receipt !== expectedReceipt) fail(`${id}: receipt path must be exactly ${expectedReceipt}`);
    const sourcePath = resolve(input, `_raw_reverify_${id}.log`);
    const targetPath = resolve(root, expectedReceipt);
    checkLexicalContainment(input, sourcePath, `${id} source receipt`);
    checkLexicalContainment(root, targetPath, `${id} target receipt`);
    const sourceInfo = inspectPath(sourcePath, { expect: 'file' });
    if (sourceInfo.stat.size > RECEIPT_LIMIT) {
      fail(`${id}: receipt exceeds 200 KiB; CLAUDE.md tier 2 policy`);
    }
    const targetInfo = inspectPath(targetPath, { allowMissing: true, expect: 'file' });
    const targetParentInfo = inspectPath(dirname(targetPath), { expect: 'dir' });
    if (!inside(inputInfo.real, sourceInfo.real) || !inside(targetParentInfo.real, dirname(targetPath)) ||
        (targetInfo.exists && !inside(targetParentInfo.real, targetInfo.real))) {
      fail(`${id}: receipt path realpath containment failure`);
    }
    const bytes = readFileSync(sourcePath);
    if (bytes.length > RECEIPT_LIMIT) {
      fail(`${id}: receipt exceeds 200 KiB; CLAUDE.md tier 2 policy`);
    }
    if (sha256(bytes) !== candidate.receipt_sha256) fail(`${id}: receipt SHA-256 mismatch`);
    receiptBuffers.set(id, bytes);
    proposedObjects.set(id, candidate);
  }
  for (const id of verdict.verdicts.keys()) if (!proposedById.has(id)) fail(`verdict ID has no proposed row: ${id}`);
  const directoryInfo = inspectPath(resolve(root, RECEIPTS_DIR_REL), { expect: 'dir' });
  if (!inside(root, directoryInfo.real)) fail('receipt target directory resolves outside repository root');
  return {
    root, input, registry, existingById, proposedById, proposedObjects, receiptBuffers,
    registryPath, requiredPath, generatorPath, registryInfo, requiredInfo, generatorInfo,
    requiredBytes, registryBytes: readFileSync(registryPath), directoryInfo,
  };
}

function inspectPath(path, { allowMissing = false, expect = 'any' } = {}) {
  const absolute = resolve(path);
  const root = parse(absolute).root;
  const parts = absolute.slice(root.length).split(sep).filter(Boolean);
  let current = root;
  for (let i = 0; i < parts.length; i += 1) {
    current = join(current, parts[i]);
    let stat;
    try { stat = lstatSync(current); }
    catch (error) {
      if (allowMissing && isMissing(error)) return { exists: false, nearest: dirname(current) };
      throw new Error(`${current}: ${error.message}`);
    }
    if (stat.isSymbolicLink()) fail(`${current}: symlink/junction is forbidden`);
    const final = i === parts.length - 1;
    if (!final && !stat.isDirectory()) fail(`${current}: parent is not a directory`);
    if (final && expect === 'file' && !stat.isFile()) fail(`${current}: expected regular file`);
    if (final && expect === 'dir' && !stat.isDirectory()) fail(`${current}: expected directory`);
  }
  return { exists: true, real: realpathSync(absolute), stat: lstatSync(absolute) };
}

export function apply({ repoRoot = DEFAULT_ROOT, inDir, dryRun = false } = {}) {
  if (!inDir) fail('inDir is required');
  const validated = validateInputs({ repoRoot, inDir });
  const registryBytes = renderRegistry(validated);
  const plan = {
    registry: [validated.registryPath],
    generated: [resolve(validated.root, GENERATED_REL)],
    receipts: [...validated.receiptBuffers.keys()].map((id) =>
      resolve(validated.root, `${RECEIPTS_DIR_REL}/_raw_reverify_${id}.log`)),
    delete: CONFIG_PATHS.map((path) => resolve(validated.root, path)).filter(existsSync),
  };
  let markdownBytes = null;
  if (!dryRun) markdownBytes = runGenerator(validated, registryBytes);
  if (dryRun) return { dryRun: true, plan };
  applyWrites(validated, registryBytes, markdownBytes);
  return { dryRun: false, plan };
}

function parseArgs(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    if (argv.length !== 1) fail('--help cannot be combined with other arguments');
    return { help: true };
  }
  let inDir, dryRun = false;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--dry-run') dryRun = true;
    else if (arg === '--dir') {
      if (inDir || !argv[i + 1] || argv[i + 1].startsWith('-')) fail('--dir requires exactly one path');
      inDir = argv[++i];
    } else if (arg.startsWith('-')) fail(`unknown option: ${arg}`);
    else if (inDir) fail('provide only one input directory');
    else inDir = arg;
  }
  if (!inDir) fail('provide --dir <dir> or a positional input directory');
  return { inDir, dryRun };
}
function printHelp() {
  console.log('Usage: node scripts/r20_evidence_reverify_apply.mjs --dir <dir> [--dry-run]');
  console.log('   or: node scripts/r20_evidence_reverify_apply.mjs <dir> [--dry-run]');
  console.log('Applies validated E0 PASS output; --dry-run validates and prints the plan only.');
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : '';
if (invokedPath && invokedPath === fileURLToPath(import.meta.url)) {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) printHelp();
    else {
      const result = await apply({ repoRoot: DEFAULT_ROOT, ...args });
      console.log(`${result.dryRun ? 'DRY-RUN' : 'APPLIED'} plan:`);
      for (const [kind, value] of Object.entries(result.plan)) {
        for (const path of value) console.log(`  ${kind}: ${path}`);
      }
    }
  } catch (error) {
    console.error(`ERROR: ${error.message}`);
    process.exitCode = 1;
  }
}
