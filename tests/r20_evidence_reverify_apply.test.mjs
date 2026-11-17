import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync,
  readlinkSync, renameSync, rmSync, symlinkSync, writeFileSync,
} from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { apply, REGISTRY_HEADER } from '../scripts/r20_evidence_reverify_apply.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const HEADER = REGISTRY_HEADER;
const REQUIRED_HEADER = ['cell_id', 'class', 'gate', 'expects', 'requires_mutant', 'note'];
const GENERATED = join(REPO, 'scripts', 'generate-evidence-registry-md.mjs');
const VERIFIER = join(REPO, 'scripts', 'verify-evidence-registry.mjs');
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const ID = 'E0-PASS';
const RECEIPT = `docs/evidence/_raw_reverify_${ID}.log`;
const RECEIPT_NAME = `_raw_reverify_${ID}.log`;
const HEAD = gitText(['rev-parse', 'HEAD']);
const TREE = gitText(['rev-parse', 'HEAD^{tree}']);
function gitText(args) {
  const result = spawnSync('git', args, { cwd: REPO, encoding: 'utf8' });
  assert.equal(result.status, 0, `read-only git ${args.join(' ')}: ${result.stderr}`);
  return result.stdout.trim();
}
function gitShow(ref) {
  const result = spawnSync('git', ['show', ref], { cwd: REPO, encoding: 'buffer' });
  assert.equal(result.status, 0, `read-only git show ${ref}: ${result.stderr?.toString()}`);
  return result.stdout;
}

// Test-local RFC 4180 encoding/parsing deliberately do not call apply's writer.
function csvField(value) {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}
function csvRecord(cells) { return `${cells.map(csvField).join(',')}\n`; }
function csvFile(header, rows) { return csvRecord(header) + rows.map(csvRecord).join(''); }
function parseCsv(text) {
  const rows = [];
  let cells = [], field = '', quoted = false;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (quoted) {
      if (ch === '"' && text[i + 1] === '"') { field += '"'; i += 1; }
      else if (ch === '"') quoted = false;
      else field += ch;
    } else if (ch === '"' && field === '') quoted = true;
    else if (ch === ',') { cells.push(field); field = ''; }
    else if (ch === '\n') { cells.push(field); rows.push(cells); cells = []; field = ''; }
    else if (ch !== '\r') field += ch;
  }
  if (field !== '' || cells.length) { cells.push(field); rows.push(cells); }
  return rows;
}
function rowObject(cells) { return Object.fromEntries(HEADER.map((key, i) => [key, cells[i] ?? ''])); }
function values(row) { return HEADER.map((key) => row[key] ?? ''); }
function assertSpawnOk(args, cwd, label) {
  const result = spawnSync(process.execPath, args, { cwd, encoding: 'utf8' });
  assert.equal(result.status, 0, `${label}\nstdout: ${result.stdout}\nstderr: ${result.stderr}`);
  return result.stdout;
}
function put(root, path, bytes) {
  const target = join(root, path);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, bytes);
  return target;
}

function basePassRow(overrides = {}) {
  return {
    id: ID, transition: 'synthetic transition', entry: 'fixture witness', features: 'alloc-global',
    os: 'linux x64', toolchain: 'rustc fixture 1.0', profile: 'debug', source_sha: HEAD,
    source_tree: TREE, binary_id: 'fixture-bin', activation: 'fixture activation runs all witnesses',
    positive_marker: 'fixture output: 3 witnesses passed successfully', mutant: 'fixture mutant rejected',
    mutant_rejected: 'true', receipt: RECEIPT, receipt_sha256: '', scope: 'crate-internal',
    status: 'PASS', status_reason: 'fixture pass reason', owner_phase: 'Ph0', source_cell: 'fixture:E0-PASS',
    ...overrides,
  };
}
function redRow() {
  return basePassRow({ id: 'E0-RED', status: 'KNOWN-RED', mutant_rejected: 'false', mutant: '',
    receipt: 'docs/evidence/_raw_reverify_E0-RED.log', receipt_sha256: '', status_reason: 'known red remains unchanged' });
}
function fixture(t, { rows = null, proposedRows = null, verdict = null,
  configRows = null, receiptPath = RECEIPT_NAME, receiptBytes = Buffer.from('verbatim receipt\n') } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'r20-evidence-apply-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'scripts'), { recursive: true });
  mkdirSync(join(root, 'docs/evidence'), { recursive: true });
  mkdirSync(join(root, '.github'), { recursive: true });
  copyFileSync(GENERATED, join(root, 'scripts/generate-evidence-registry-md.mjs'));
  copyFileSync(VERIFIER, join(root, 'scripts/verify-evidence-registry.mjs'));
  copyFileSync(join(REPO, 'docs/evidence/required_cells.csv'), join(root, 'docs/evidence/required_cells.csv'));
  put(root, 'docs/evidence/source_scope.csv', Buffer.from('id,paths,scope_base,anchors,note\n'));
  const oldReceipt = Buffer.from('old, pinned receipt\n');
  const originalRows = rows ?? [basePassRow({ os: 'historic OS', toolchain: 'old toolchain',
    profile: 'old profile', binary_id: 'old binary', activation: 'old activation was concrete',
    positive_marker: 'old marker had enough detail', receipt: 'docs/evidence/old.log',
    receipt_sha256: sha(oldReceipt), status_reason: 'pre-existing PASS reason (preserve verbatim)' }), redRow()];
  put(root, 'docs/evidence/old.log', oldReceipt);
  const untouched = [
    'E0-untouched', 'quoted untouched, unusual', 'fixture; untouched', 'std', 'fixture os',
    'stable\nfixture toolchain', 'debug', HEAD, TREE, '—', 'unchanged activation',
    'positive marker unchanged', 'unchanged mutant', 'false', 'docs/evidence/untouched.log',
    sha(Buffer.from('untouched receipt\n')), 'tool-only', 'PASS', 'reason with "quotes", comma', 'Ph0', 'fixture:untouched',
  ];
  const rawIdle = csvRecord(untouched);
  put(root, 'docs/evidence/untouched.log', Buffer.from('untouched receipt\n'));
  const red = originalRows.find((row) => row.status === 'KNOWN-RED');
  if (red) {
    const rel = red.receipt.slice('docs/evidence/'.length);
    const bytes = Buffer.from('preserve the KNOWN-RED receipt\n');
    put(root, `docs/evidence/${rel}`, bytes);
    red.receipt_sha256 = sha(bytes);
  }
  const headerBytes = csvRecord(HEADER);
  put(root, 'docs/evidence/registry.csv', Buffer.from(headerBytes + rawIdle + originalRows.map((row) => csvRecord(values(row))).join('')));
  const sourceReceipt = receiptPath.startsWith('/') ? receiptPath.slice(1) : receiptPath;
  put(root, sourceReceipt, receiptBytes);
  const proposal = proposedRows ?? [basePassRow({ receipt_sha256: sha(receiptBytes), status_reason: 'E0 verified; preserve PASS' })];
  put(root, 'verdict.txt', Buffer.from(verdict ?? `E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\n${ID}: pass — preserve PASS\n`));
  put(root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, proposal.map(values))));
  const config = JSON.stringify({ rows: configRows ?? [ID] });
  put(root, '.github/evidence-reverify.json', Buffer.from(config));
  put(root, '.github/gate-run.json', Buffer.from(JSON.stringify({ runner: 'fixture', stages: 'prepare', outDir: 'target/fixture' })));
  const required = [
    ...originalRows.map((row) => [row.id, 'fixture', 'C1', row.status, 'false', 'fixture row']),
    ['E0-untouched', 'fixture', 'C1', 'PASS', 'false', 'fixture untouched row'],
  ];
  put(root, 'docs/evidence/required_cells.csv', Buffer.from(csvFile(REQUIRED_HEADER, required)));
  return { root, rawIdle, originalRows, proposal, receiptBytes,
    originalRegistry: readFileSync(join(root, 'docs/evidence/registry.csv')) };
}

function snapshotTree(root) {
  const entries = [];
  function visit(path, rel = '') {
    for (const name of readdirSync(path).sort()) {
      const abs = join(path, name), child = rel ? `${rel}/${name}` : name;
      const stat = lstatSync(abs);
      if (stat.isSymbolicLink()) entries.push([child, 'symlink', readlinkSync(abs)]);
      else if (stat.isDirectory()) { entries.push([child, 'directory']); visit(abs, child); }
      else entries.push([child, 'file', sha(readFileSync(abs))]);
    }
  }
  visit(root);
  return entries;
}
function callApply(fx, options = {}) { return apply({ repoRoot: fx.root, inDir: fx.root, ...options }); }
function noWriteFailure(fx, pattern) {
  const before = snapshotTree(fx.root);
  assert.throws(() => callApply(fx), pattern);
  assert.deepEqual(snapshotTree(fx.root), before, 'refusal changed fixture bytes/types/links');
}
function tryCreateLink(t, target, path, type) {
  try {
    symlinkSync(target, path, type);
    return true;
  } catch (error) {
    if (['EPERM', 'EACCES'].includes(error.code)) {
      t.diagnostic(`symlink case skipped: ${error.code} (${error.message})`);
      return false;
    }
    throw error;
  }
}
function runVerifier(root) {
  return spawnSync(process.execPath, [VERIFIER, '--strict', '--repo-root', root,
    '--registry', join(root, 'docs/evidence/registry.csv'),
    '--required', join(root, 'docs/evidence/required_cells.csv'),
    '--source-scope', join(root, 'docs/evidence/source_scope.csv')], {
    cwd: root, encoding: 'utf8',
    env: { ...process.env, GIT_DIR: gitText(['rev-parse', '--absolute-git-dir']), GIT_WORK_TREE: root },
  });
}
function makeInputRow(base, patch = {}) {
  return { ...base, ...patch, status_reason: `E0 verified; preserve ${base.status}` };
}
function splitRawRecords(text) {
  const records = [];
  let start = 0, quoted = false;
  for (let i = 0; i < text.length; i += 1) {
    if (quoted) {
      if (text[i] === '"' && text[i + 1] === '"') i += 1;
      else if (text[i] === '"') quoted = false;
    } else if (text[i] === '"') quoted = true;
    else if (text[i] === '\n') { records.push(text.slice(start, i + 1)); start = i + 1; }
  }
  if (start < text.length) records.push(text.slice(start));
  return records;
}
function expectedMergedBytes(originalBytes, proposal) {
  const records = splitRawRecords(originalBytes.toString('utf8'));
  const changed = new Map(proposal.map((row) => [row.id, row]));
  const result = [records[0]];
  for (const raw of records.slice(1)) {
    const parsed = parseCsv(raw);
    if (!parsed.length) { result.push(raw); continue; }
    const original = rowObject(parsed[0]);
    const replacement = changed.get(original.id);
    if (!replacement) { result.push(raw); continue; }
    const oldRawFields = rawCsvFields(raw);
    const merged = values({ ...original, ...replacement, status_reason: original.status_reason });
    result.push(`${merged.map((value, index) => value === parsed[0][index]
      ? oldRawFields[index] : csvField(value)).join(',')}\n`);
  }
  return Buffer.from(result.join(''));
}
function rawCsvFields(raw) {
  const body = raw.endsWith('\n') ? raw.slice(0, -1) : raw;
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
assert.deepEqual(REGISTRY_HEADER, HEADER, 'fixture schema must match production schema');

test('apply merges by ID, preserves raw records/KNOWN-RED, and validates generated outputs', (t) => {
  const fx = fixture(t, { receiptBytes: Buffer.from('raw\nreceipt bytes\n') });
  const proposal = makeInputRow(fx.proposal[0], {
    os: 'Linux x64', toolchain: 'rustc 1.102.0-nightly\nbinary: rustc', profile: 'debug changed',
  });
  fx.proposal = [proposal];
  put(fx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(proposal)])));
  const expected = Buffer.from(csvRecord(HEADER) + fx.rawIdle +
    csvRecord(values({ ...proposal, status_reason: fx.originalRows[0].status_reason })) +
    csvRecord(values(fx.originalRows[1])));
  const result = callApply(fx);
  assert.equal(result.dryRun, false);
  assert.deepEqual(readFileSync(join(fx.root, 'docs/evidence/registry.csv')), expected);
  assert.deepEqual(readFileSync(join(fx.root, 'docs/evidence/_raw_reverify_E0-PASS.log')), fx.receiptBytes,
    'raw receipt bytes are copied verbatim and SHA-pinned without newline normalization');
  assert.equal(existsSync(join(fx.root, '.github/evidence-reverify.json')), false);
  assert.equal(existsSync(join(fx.root, '.github/gate-run.json')), false);
  assertSpawnOk([join(fx.root, 'scripts/generate-evidence-registry-md.mjs'), '--check'], fx.root,
    'fixture generated Markdown check');
  const verifier = runVerifier(fx.root);
  if (gitText(['rev-parse', '--is-shallow-repository']) === 'true') {
    assert.notEqual(verifier.status, 0, `strict shallow fixture must refuse\nstdout: ${verifier.stdout}\nstderr: ${verifier.stderr}`);
    assert.match(`${verifier.stdout}\n${verifier.stderr}`, /identity-shallow/);
    assert.match(`${verifier.stdout}\n${verifier.stderr}`, /repository is shallow: source_sha ancestry vs HEAD cannot be verified/);
  } else {
    assert.equal(verifier.status, 0, `strict fixture verifier\nstdout: ${verifier.stdout}\nstderr: ${verifier.stderr}`);
    assert.match(verifier.stdout, /VERDICT: GREEN/);
  }
});

test('apply preserves CONDITIONAL status and existing reason while updating verified metadata', (t) => {
  const receiptBytes = Buffer.from('conditional reverify receipt\n');
  const original = basePassRow({ status: 'CONDITIONAL', status_reason: 'Existing CONDITIONAL reason (preserve verbatim)' });
  const proposal = basePassRow({
    status: 'CONDITIONAL', status_reason: 'E0 verified; preserve CONDITIONAL',
    os: 'Linux x64 refreshed', receipt_sha256: sha(receiptBytes),
  });
  const fx = fixture(t, {
    rows: [original], proposedRows: [proposal], receiptBytes,
    verdict: `E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\n${ID}: pass — preserve CONDITIONAL\n`,
  });

  callApply(fx);

  const registry = parseCsv(readFileSync(join(fx.root, 'docs/evidence/registry.csv'), 'utf8'));
  const actual = rowObject(registry.find((cells) => cells[0] === ID));
  assert.equal(actual.status, 'CONDITIONAL');
  assert.equal(actual.status_reason, original.status_reason);
  assert.equal(actual.os, proposal.os);
  assert.deepEqual(readFileSync(join(fx.root, 'docs/evidence/_raw_reverify_E0-PASS.log')), receiptBytes);
  assert.equal(existsSync(join(fx.root, '.github/evidence-reverify.json')), false);
  assert.equal(existsSync(join(fx.root, '.github/gate-run.json')), false);
});

test('apply refuses changing an existing CONDITIONAL status without writing', (t) => {
  const original = basePassRow({ status: 'CONDITIONAL', status_reason: 'Existing CONDITIONAL reason' });
  const proposal = basePassRow({ status: 'PASS', status_reason: 'E0 verified; preserve CONDITIONAL' });
  const fx = fixture(t, {
    rows: [original], proposedRows: [proposal],
    verdict: `E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\n${ID}: pass — preserve CONDITIONAL\n`,
  });
  const before = snapshotTree(fx.root);

  assert.throws(() => callApply(fx), /status must be preserved \(CONDITIONAL\)/);
  assert.deepEqual(snapshotTree(fx.root), before, 'refused apply must not change the full fixture tree');
});

test('dry-run validates the plan without changing fixture bytes or file types', (t) => {
  const fx = fixture(t);
  const before = snapshotTree(fx.root);
  const result = callApply(fx, { dryRun: true });
  assert.equal(result.dryRun, true);
  assert.ok(result.plan.registry.length === 1);
  assert.deepEqual(snapshotTree(fx.root), before);
});

test('CLI dry-run prints the receipt plan without changing fixture bytes or file types', (t) => {
  const fx = fixture(t);
  const script = join(fx.root, 'scripts/r20_evidence_reverify_apply.mjs');
  copyFileSync(join(REPO, 'scripts/r20_evidence_reverify_apply.mjs'), script);
  const before = snapshotTree(fx.root);
  const result = spawnSync(process.execPath, [script, '--dir', fx.root, '--dry-run'], {
    cwd: fx.root, encoding: 'utf8',
  });
  assert.equal(result.status, 0, `CLI dry-run\nstdout: ${result.stdout}\nstderr: ${result.stderr}`);
  assert.match(result.stdout, /DRY-RUN/);
  assert.match(result.stdout, /receipts:/);
  assert.deepEqual(snapshotTree(fx.root), before);
});

test('dry-run accepts a pass verdict without an optional reason', (t) => {
  const fx = fixture(t, { verdict: `E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\n${ID}: pass\n` });
  const before = snapshotTree(fx.root);
  const result = callApply(fx, { dryRun: true });
  assert.equal(result.dryRun, true);
  assert.deepEqual(snapshotTree(fx.root), before);
});

test('staged generator failure leaves the fixture unchanged', (t) => {
  const fx = fixture(t);
  put(fx.root, 'scripts/generate-evidence-registry-md.mjs',
    Buffer.from("process.stderr.write('fixture generator fail');process.exit(2);"));
  const before = snapshotTree(fx.root);
  assert.throws(() => callApply(fx), /staged registry generator failed: fixture generator fail/);
  assert.deepEqual(snapshotTree(fx.root), before);
});

test('refuses malformed proposals without writing', (t) => {
  const cases = [
    { name: 'non-pass result', edit: (fx) => put(fx.root, 'verdict.txt', Buffer.from(`E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\n${ID}: fail — did not pass\n`)), error: /malformed or non-pass result/ },
    { name: 'bad receipt hash', edit: (fx) => { const row = { ...fx.proposal[0], receipt_sha256: '0'.repeat(64) }; put(fx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(row)]))); }, error: /SHA-256 mismatch/ },
    { name: 'non-pass proposal status', edit: (fx) => { const row = { ...fx.proposal[0], status: 'FAIL' }; put(fx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(row)]))); }, error: /status must be preserved/ },
    { name: 'unknown proposed ID', edit: (fx) => { const row = { ...fx.proposal[0], id: 'EXTRA' }; put(fx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(row)]))); put(fx.root, 'verdict.txt', Buffer.from(`E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\nEXTRA: pass — preserve PASS\n`)); }, error: /unknown registry ID: EXTRA/ },
    { name: 'duplicate proposed ID', edit: (fx) => put(fx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(fx.proposal[0]), values(fx.proposal[0])]))), error: /duplicate ID E0-PASS/ },
    { name: 'verdict missing proposal', edit: (fx) => put(fx.root, 'verdict.txt', Buffer.from(`E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\nE0-PASS: pass — preserve PASS\nE0-OTHER: pass — preserve PASS\n`)), error: /proposal\/verdict ID sets differ/ },
    { name: 'proposal row absent from verdict', edit: (fx) => {
      const extra = { ...fx.proposal[0], id: 'EXTRA' };
      put(fx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(fx.proposal[0]), values(extra)])));
    }, error: /proposal\/verdict ID sets differ/ },
  ];
  for (const item of cases) {
    const fx = fixture(t);
    item.edit(fx);
    const before = snapshotTree(fx.root);
    assert.throws(() => callApply(fx), item.error, item.name);
    assert.deepEqual(snapshotTree(fx.root), before, `${item.name}: refused apply must not write`);
  }
});

test('refuses tampering with a preserved KNOWN-RED row and oversized receipts', (t) => {
  const redFx = fixture(t);
  const redOriginal = rowObject(parseCsv(readFileSync(join(redFx.root, 'docs/evidence/registry.csv'), 'utf8'))
    .find((cells) => cells[0] === 'E0-RED'));
  const tampered = makeInputRow(redOriginal, { status: 'PASS' });
  tampered.receipt = 'docs/evidence/_raw_reverify_E0-RED.log';
  tampered.receipt_sha256 = sha(readFileSync(join(redFx.root, 'docs/evidence/_raw_reverify_E0-RED.log')));
  put(redFx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(redFx.proposal[0]), values(tampered)])));
  put(redFx.root, 'verdict.txt', Buffer.from(`E0 PASS\nHEAD ${HEAD}\nTREE ${TREE}\nE0-PASS: pass — preserve PASS\nE0-RED: pass — preserve KNOWN-RED\n`));
  noWriteFailure(redFx, /cannot change existing KNOWN-RED status/);

  const largeBytes = Buffer.alloc(200 * 1024 + 1, 0x61);
  const bigFx = fixture(t, { receiptBytes: largeBytes });
  const largeProposal = { ...bigFx.proposal[0], receipt_sha256: sha(largeBytes) };
  put(bigFx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(largeProposal)])));
  noWriteFailure(bigFx, /receipt exceeds 200 KiB/);

  const pathFx = fixture(t);
  const traversal = { ...pathFx.proposal[0], receipt: '../../outside.log' };
  put(pathFx.root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, [values(traversal)])));
  noWriteFailure(pathFx, /receipt path must be exactly docs\/evidence\/_raw_reverify_E0-PASS\.log/);

  const sourceFx = fixture(t);
  const external = put(sourceFx.root, 'external.log', Buffer.from('not a receipt'));
  rmSync(join(sourceFx.root, RECEIPT_NAME));
  if (tryCreateLink(t, external, join(sourceFx.root, RECEIPT_NAME), 'file')) {
    noWriteFailure(sourceFx, /symlink\/junction is forbidden/);
  }

  const destinationFx = fixture(t);
  const dest = join(destinationFx.root, `docs/evidence/_raw_reverify_${ID}.log`);
  const outside = put(destinationFx.root, 'external-destination.log', Buffer.from('outside'));
  if (tryCreateLink(t, outside, dest, 'file')) {
    noWriteFailure(destinationFx, /symlink\/junction is forbidden/);
  }
});

test('refuses a receipt destination parent junction without changing the tree', (t) => {
  const fx = fixture(t);
  const evidence = join(fx.root, 'docs/evidence');
  const moved = join(fx.root, 'docs/evidence-real');
  renameSync(evidence, moved);
  if (!tryCreateLink(t, moved, evidence, 'junction')) {
    renameSync(moved, evidence);
    return;
  }
  noWriteFailure(fx, /symlink\/junction is forbidden/);
});

function historicalRegistryObjectsPresent(commit) {
  return [commit, `${commit}^`].every((ref) => spawnSync('git',
    ['cat-file', '-e', `${ref}:docs/evidence/registry.csv`], { cwd: REPO, stdio: 'ignore' }).status === 0);
}

function historicalFixture(t, commit, ids) {
  const root = mkdtempSync(join(tmpdir(), 'r20-evidence-history-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'scripts'), { recursive: true });
  mkdirSync(join(root, 'docs/evidence'), { recursive: true });
  mkdirSync(join(root, '.github'), { recursive: true });
  copyFileSync(GENERATED, join(root, 'scripts/generate-evidence-registry-md.mjs'));
  copyFileSync(join(REPO, 'docs/evidence/required_cells.csv'), join(root, 'docs/evidence/required_cells.csv'));
  put(root, 'docs/evidence/source_scope.csv', Buffer.from('id,paths,scope_base,anchors,note\n'));
  const before = gitShow(`${commit}^:docs/evidence/registry.csv`);
  const after = gitShow(`${commit}:docs/evidence/registry.csv`);
  put(root, 'docs/evidence/registry.csv', before);
  const beforeRows = parseCsv(before.toString('utf8')).slice(1).map(rowObject);
  const afterRows = parseCsv(after.toString('utf8')).slice(1).map(rowObject);
  const beforeById = new Map(beforeRows.map((row) => [row.id, row]));
  const afterById = new Map(afterRows.map((row) => [row.id, row]));
  const actuallyChanged = [...afterById.keys()].filter((id) =>
    JSON.stringify(values(beforeById.get(id))) !== JSON.stringify(values(afterById.get(id)))).sort();
  assert.deepEqual(actuallyChanged, [...ids].sort(), `${commit}: expected only the recorded IDs to change`);
  const rows = ids.map((id) => afterById.get(id));
  const identity = { head: rows[0].source_sha, tree: rows[0].source_tree };
  assert.match(identity.head, /^[0-9a-f]{40}$/);
  assert.match(identity.tree, /^[0-9a-f]{40}$/);
  assert.ok(rows.every((row) => row.source_sha === identity.head && row.source_tree === identity.tree));
  const proposal = [];
  for (const row of rows) {
    const receipt = gitShow(`${commit}:docs/evidence/_raw_reverify_${row.id}.log`);
    const expectedPath = `docs/evidence/_raw_reverify_${row.id}.log`;
    assert.equal(row.receipt, expectedPath);
    assert.equal(row.receipt_sha256, sha(receipt), `${commit} ${row.id}: registry pin matches exact Git blob bytes`);
    put(root, `_raw_reverify_${row.id}.log`, receipt);
    proposal.push({ ...row, status_reason: `E0 verified; preserve ${row.status}` });
  }
  const verdictRows = rows.map((row) => `${row.id}: pass — preserve ${row.status}`);
  put(root, 'verdict.txt', Buffer.from(`E0 PASS\nHEAD ${identity.head}\nTREE ${identity.tree}\n${verdictRows.join('\n')}\n`));
  put(root, 'proposed_rows.csv', Buffer.from(csvFile(HEADER, proposal.map(values))));
  put(root, '.github/evidence-reverify.json', Buffer.from(JSON.stringify({ rows: ids })));
  put(root, '.github/gate-run.json', Buffer.from('{"runner":"historical fixture"}'));
  return { root, before, after, ids, proposal, identity };
}

for (const [commit, ids] of [
  ['366aab2a', ['R0-21', 'R0-22']],
  ['9e5fbe08', ['R0-16', 'R0-17', 'R0-19', 'R0-20', 'R0-21', 'R0-22',
    'R-Ph3a-native', 'R4c-05', 'R4c-06', 'R4c-07', 'R5b-10', 'R5b-18', 'R5b-19']],
]) {
  test(`historical replay ${commit}: reconstructed equivalent input for ${ids.length} rows`, (t) => {
    if (!historicalRegistryObjectsPresent(commit)) {
      t.skip('historical objects absent in shallow checkout; replay executed in full-history verification');
      return;
    }
    t.diagnostic('Reconstructed equivalent input: original verdict.txt/proposed_rows.csv were not tracked; receipt blobs and registry states are recovered read-only from Git.');
    const fx = historicalFixture(t, commit, ids);
    const expected = expectedMergedBytes(fx.before, fx.proposal);
    assert.deepEqual(expected, fx.after, `${commit}: reconstructed merge result matches committed registry bytes`);
    callApply({ root: fx.root, inDir: fx.root });
    const replayed = readFileSync(join(fx.root, 'docs/evidence/registry.csv'));
    assert.deepEqual(replayed, fx.after, `${commit}: replayed registry is byte-equal to historical after commit`);
      const actualById = new Map(parseCsv(replayed.toString('utf8')).slice(1).map((row) => [row[0], row]));
    const expectedById = new Map(parseCsv(expected.toString('utf8')).slice(1).map((row) => [row[0], row]));
    for (const id of ids) {
      assert.deepEqual(expectedById.get(id), actualById.get(id), `${commit} ${id}: changed row fields`);
    }
    const beforeRecords = splitRawRecords(fx.before.toString('utf8'));
    const afterRecords = splitRawRecords(fx.after.toString('utf8'));
    const rowIds = new Set(ids);
    assert.deepEqual(afterRecords.filter((raw) => !rowIds.has(parseCsv(raw)[0]?.[0])),
      beforeRecords.filter((raw) => !rowIds.has(parseCsv(raw)[0]?.[0])), `${commit}: non-selected raw records unchanged`);
    assert.equal(afterRecords.length, beforeRecords.length);
    for (const id of ids) {
      const after = actualById.get(id);
      assert.equal(after[17], id === 'R4c-05' || id === 'R4c-06' ? 'KNOWN-RED' : 'PASS');
    }
    assert.equal(existsSync(join(fx.root, '.github/evidence-reverify.json')), false);
    assert.equal(existsSync(join(fx.root, '.github/gate-run.json')), false);
  });
}

