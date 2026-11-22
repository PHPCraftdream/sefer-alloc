// Cargo-free checks for text inventories that are also useful on docs-only CI.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const defaultRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const rootPath = (root, ...parts) => join(root, ...parts);
const read = (root, ...parts) => readFileSync(rootPath(root, ...parts), 'utf8');
const isFile = path => statSync(path).isFile();

function entries(root, rel) {
  return readdirSync(rootPath(root, rel), { withFileTypes: true });
}

function countRootRs(root, rel) {
  const dir = rootPath(root, rel);
  return readdirSync(dir).filter(name => name.endsWith('.rs') && isFile(join(dir, name))).length;
}

function markdownSection(text, heading) {
  const start = text.indexOf(heading);
  assert.notEqual(start, -1, `markdown heading: ${heading}`);
  const body = text.slice(start);
  const end = body.indexOf('\n## ');
  return end === -1 ? body : body.slice(0, end);
}

function inventoryFiles(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) inventoryFiles(path, out);
    else if (entry.isFile()) out.push(path);
  }
  return out;
}

function flattenWhitespace(text) {
  return text.split(/[\t\n\v\f\r ]+/).filter(Boolean).join(' ');
}

function parseProductionFeatureList(cargo) {
  const line = cargo.split(/\r?\n/).find(item => item.trimStart().startsWith('production = ['));
  assert.ok(line, 'Cargo.toml must define `production = ["...", ...]`');
  const start = line.indexOf('[') + 1;
  const end = line.lastIndexOf(']');
  return line.slice(start, end).split(',').map(part => part.trim().replace(/^"|"$/g, '')).filter(Boolean);
}

function exactProductionBundle(rest, canonical) {
  if (!rest.startsWith(canonical)) return false;
  const tail = rest.slice(canonical.length);
  return !(/[A-Za-z0-9_\-/]/.test(tail[0] ?? '') || tail.trimStart().startsWith('+'));
}

/** Pin the root-only tests/*.rs count in docs/ARCHITECTURE.md. */
export function architectureTestFileCountMatchesReality(root = defaultRoot) {
  const count = countRootRs(root, 'tests');
  assert.ok(count > 0, 'no test files found');
  const text = read(root, 'docs', 'ARCHITECTURE.md');
  const needle = `(${count} files`;
  assert.ok(text.includes(needle), `docs/ARCHITECTURE.md test-file count is stale: there are ${count} root \`tests/*.rs\` files (not recursive helpers), but the doc does not contain the token \`${needle}\`. Update the \`tests/*.rs (<N> files)\` line to ${count}.`);
}

/** Pin root test/example/bench and Loom inventories in README and ARCHITECTURE. */
export function verificationInventoryMatchesDocs(root = defaultRoot) {
  const testsCount = countRootRs(root, 'tests');
  const examplesCount = countRootRs(root, 'examples');
  const benchesCount = countRootRs(root, 'benches');
  const rootLoom = entries(root, 'tests')
    .filter(entry => entry.isFile() && entry.name.endsWith('.rs') && entry.name.startsWith('loom_'))
    .map(entry => entry.name).sort();
  const readme = read(root, 'README.md');
  const architecture = read(root, 'docs', 'ARCHITECTURE.md');
  const readmeVerification = markdownSection(readme, '## Verification evidence');
  const architectureVerification = markdownSection(architecture, '## 8. Verification stack');

  for (const token of [
    `**${testsCount} integration test files**`,
    `**${examplesCount} example binaries**`,
    `**${benchesCount} benches**`,
    `**${rootLoom.length} root Loom models**`,
    `\`tests/*.rs\` (${testsCount} files)`,
    `\`examples/*.rs\` (${examplesCount} files)`,
    `\`benches/*.rs\` (${benchesCount} files)`,
  ]) {
    assert.ok(readmeVerification.includes(token), `README verification inventory is missing canonical token \`${token}\``);
  }
  for (const file of rootLoom) {
    const path = `tests/${file}`;
    assert.ok(readmeVerification.includes(path), `README Loom inventory is missing \`${path}\``);
    assert.ok(architectureVerification.includes(path), `ARCHITECTURE Loom inventory is missing \`${path}\``);
  }
  for (const suite of [
    'crates/primitives/once-ptr-cell/tests/loom_once_ptr_cell.rs',
    'crates/primitives/tagged-index-stack/tests/loom_aba.rs',
  ]) {
    assert.ok(readmeVerification.includes(suite), `README Loom inventory is missing member suite \`${suite}\``);
    assert.ok(architectureVerification.includes(suite), `ARCHITECTURE Loom inventory is missing member suite \`${suite}\``);
  }
  for (const text of [readmeVerification, architectureVerification]) {
    assert.ok(text.includes('narrow_domain_unchecked_storage'), 'verification docs must name the tagged-index-stack narrow-domain Miri target');
  }

  const containsStaleCount = (haystack, stale) => {
    let from = 0;
    while (true) {
      const index = haystack.indexOf(stale, from);
      if (index === -1) return false;
      if (!/[0-9]/.test(haystack[index - 1] ?? '')) return true;
      from = index + 1;
    }
  };
  for (const stale of ['111 integration test files', '5 example binaries', '9 benches', '11 loom models']) {
    assert.ok(!containsStaleCount(readmeVerification, stale), `README verification evidence retains stale inventory claim \`${stale}\``);
  }
}

/** Compare README's file-by-file allow inventory to the canonical scan. */
export function readmeUnsafeInventoryMatchesReality(root = defaultRoot) {
  const files = [...inventoryFiles(rootPath(root, 'src')), ...inventoryFiles(rootPath(root, 'crates'))];
  assert.ok(files.length > 0, 'no source files found under src/ or crates/');
  const tiers = [new Map(), new Map()];
  for (const file of files) {
    const path = relative(root, file).replaceAll(/\\/g, "/");
    for (const line of readFileSync(file, 'utf8').split('\n')) {
      const match = /^\s*#(!?)\[allow\(unsafe_code\)\]/.exec(line);
      if (!match) continue;
      const tier = tiers[match[1] ? 0 : 1];
      tier.set(path, (tier.get(path) ?? 0) + 1);
    }
  }
  const inventory = read(root, 'README.md').replaceAll('\r\n', '\n').split('### Where `unsafe` lives (the complete list)')[1]?.split(/\n#{1,3} /)[0];
  assert.ok(inventory, 'README unsafe inventory section missing');
  const tables = [...inventory.matchAll(/^#### Tier ([12]) — [^\n]*\n+((?:\|[^\n]*(?:\n|$))+)/gm)];
  assert.deepEqual(tables.map(match => match[1]), ['1', '2'], 'README unsafe inventory tier tables missing, reordered, or duplicated');
  for (const [i, [, , table]] of tables.entries()) {
    const documented = new Map();
    for (const line of table.trimEnd().split('\n').slice(2)) {
      const match = /^\| \[`([^`]+)`\]\(([^)]+)\) \| (\d+) \|/.exec(line);
      assert.ok(match && match[1] === match[2], `README tier ${i + 1}: invalid path row: ${line}`);
      assert.ok(!documented.has(match[1]), `README tier ${i + 1}: duplicate path: ${match[1]}`);
      documented.set(match[1], Number(match[3]));
    }
    assert.deepEqual(documented, tiers[i], `README tier ${i + 1} paths or per-file counts drifted`);
  }
}

function fileToModulePath(src, file) {
  let rel = relative(src, file).replaceAll('\\', '/');
  if (rel.startsWith('../') || rel === '..') return null;
  rel = rel.replace(/\.rs$/, '');
  const parts = rel.split('/');
  const last = parts.at(-1);
  if (last === 'mod' || last === 'lib') return null;
  return parts.join('::') || null;
}

function isPureModulePath(token) {
  return token.includes('::') && !/[{}()]/.test(token) && /^[A-Za-z0-9_:]+$/.test(token);
}

function extractSeamBullets(libRs) {
  const startAnchor = 'confined modules lift this with';
  const endAnchor = 'is enforced by the compiler';
  const start = libRs.indexOf(startAnchor);
  assert.notEqual(start, -1, 'lib.rs seam-inventory start anchor `confined modules lift this with`');
  const afterStart = start + startAnchor.length;
  const end = libRs.indexOf(endAnchor, afterStart);
  assert.notEqual(end, -1, 'lib.rs seam-inventory end anchor `is enforced by the compiler`');
  const region = libRs.slice(afterStart, end);
  const seams = [];
  for (const line of region.split(/\r?\n/)) {
    const stripped = line.replace(/^\/\//, '').trimStart();
    if (!stripped.startsWith('* ')) continue;
    const afterStar = stripped.slice(2);
    const open = afterStar.indexOf('`');
    if (open === -1) continue;
    const rest = afterStar.slice(open + 1);
    const close = rest.indexOf('`');
    if (close === -1) continue;
    const token = rest.slice(0, close);
    if (isPureModulePath(token)) seams.push(token);
  }
  return seams;
}

/** Compare src/lib.rs's internal seam list to the source-text allow grep. */
export function libRsSeamInventoryMatchesCanonicalGrep(root = defaultRoot) {
  const src = rootPath(root, 'src');
  const files = [];
  const walk = dir => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.rs')) files.push(path);
    }
  };
  walk(src);
  assert.ok(files.length > 0, 'no source files found under src/');
  const actual = new Set();
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    if (text.split(/\r?\n/).some(line => line.trimStart().startsWith('#![allow(unsafe_code)]'))) {
      const modulePath = fileToModulePath(src, file);
      if (modulePath !== null) actual.add(modulePath);
    }
  }
  assert.ok(actual.size > 0, 'no tier-1 seams found under src/');
  const documented = new Set(extractSeamBullets(read(root, 'src', 'lib.rs')));
  assert.ok(documented.size > 0, 'no seam bullets found in src/lib.rs header — the inventory region anchors (`confined modules lift this with` / `is enforced by the compiler`) may have moved');
  const actualSorted = [...actual].sort();
  const documentedSorted = [...documented].sort();
  const missing = actualSorted.filter(name => !documented.has(name));
  const phantom = documentedSorted.filter(name => !actual.has(name));
  assert.ok(missing.length === 0 && phantom.length === 0,
    `src/lib.rs's prose seam inventory drifted from the canonical grep \`grep -rnE '^\\s*#!?\\[allow\\(unsafe_code\\)\\]' src/ crates/\` (src/ tier-1 modules only):\n  actual tier-1 src/ seams (${actual.size}): ${JSON.stringify(actualSorted, null, 2)}\n  documented in lib.rs (${documented.size}): ${JSON.stringify(documentedSorted, null, 2)}\n  MISSING from lib.rs (a real seam with no prose entry): ${JSON.stringify(missing, null, 2)}\n  PHANTOM in lib.rs (documented but no real seam file): ${JSON.stringify(phantom, null, 2)}\n  Add the missing module(s) to lib.rs's INTERNAL-seam inventory (or remove a phantom), matching the existing \`* \`module::path\` — ...\` bullet style.`);
}

/** Pin every prose spelling of the production Cargo feature bundle. */
export function productionFeatureBundleDocSitesMatchCargoToml(root = defaultRoot) {
  const features = parseProductionFeatureList(read(root, 'Cargo.toml'));
  assert.ok(features.length >= 2, `parsed an implausibly small \`production\` feature list: ${JSON.stringify(features)}`);
  const canonical = features.join(' + ');
  const anchor = features.length >= 4 ? features.slice(0, 4).join(' + ') : canonical;
  const sites = ['src/lib.rs', 'src/global/sefer_alloc/core.rs', 'README.md', 'docs/INTEGRATION.md'];
  const offenders = [];
  for (const rel of sites) {
    const text = readFileSync(rootPath(root, rel), 'utf8');
    const flat = flattenWhitespace(text);
    let from = 0;
    let canonicalHits = 0;
    while (true) {
      const relOff = flat.slice(from).indexOf(anchor);
      if (relOff === -1) break;
      const pos = from + relOff;
      if (exactProductionBundle(flat.slice(pos), canonical)) canonicalHits += 1;
      else offenders.push(`${rel}: stale \`production\` bundle (expected exactly ${features.length} features) near: \`${flat.slice(pos, Math.min(pos + 80, flat.length))}\``);
      from = pos + anchor.length;
    }
    if (canonicalHits === 0) offenders.push(`${rel}: the canonical \`production\` bundle \`${canonical}\` does not appear at all — the doc site may have dropped the prose expansion entirely`);
  }
  assert.ok(offenders.length === 0, `the \`production\` feature bundle in Cargo.toml is \`${canonical}\` (${features.length} features), but these doc sites that spell the bundle out in prose do not match (stale, extra, or missing). Cargo.toml's \`production = [...]\` is the single source of truth — update every prose site to the full list:\n${offenders.join('\n')}`);
}

const checks = [
  ['architecture_test_file_count_matches_reality', architectureTestFileCountMatchesReality],
  ['verification_inventory_matches_docs', verificationInventoryMatchesDocs],
  ['readme_unsafe_inventory_matches_reality', readmeUnsafeInventoryMatchesReality],
  ['lib_rs_seam_inventory_matches_canonical_grep', libRsSeamInventoryMatchesCanonicalGrep],
  ['production_feature_bundle_doc_sites_match_cargo_toml', productionFeatureBundleDocSitesMatchCargoToml],
];

export function main() {
  let failed = false;
  for (const [name, check] of checks) {
    try {
      check();
      console.log(`[verify-doc-inventories] ${name}: OK`);
    } catch (error) {
      failed = true;
      console.error(`[verify-doc-inventories] ${name}: ${error.message}`);
    }
  }
  if (failed) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
