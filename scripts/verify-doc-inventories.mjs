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

const unsafeInventoryHeading = 'Where `unsafe` lives (the complete list)';
const unsafeInventoryAnchor = '#where-unsafe-lives-the-complete-list';
const unsafeInventoryPointer = `README.md § "${unsafeInventoryHeading}" (anchor \`${unsafeInventoryAnchor}\`)`;
const canonicalSeamGrep = "grep -rnE '^\\s*#!?\\[allow\\(unsafe_code\\)\\]' src/ crates/";

/** Pin lib.rs's `// ── Unsafe inventory` comment section to README's authoritative section and the canonical grep. */
export function libRsUnsafeInventoryHeaderMatchesReadme(root = defaultRoot) {
  const headingLine = `### ${unsafeInventoryHeading}`;
  assert.ok(read(root, 'README.md').split(/\r?\n/).includes(headingLine),
    `README.md is missing the authoritative unsafe-inventory heading line \`${headingLine}\``);
  const lines = read(root, 'src', 'lib.rs').replaceAll('\r\n', '\n').split('\n');
  const start = lines.findIndex(line => line.startsWith('// ── Unsafe inventory'));
  assert.ok(start !== -1, 'src/lib.rs is missing the `// ── Unsafe inventory` section banner');
  const section = [];
  for (const line of lines.slice(start + 1)) {
    const trimmed = line.trimStart();
    if (!trimmed.startsWith('//') || trimmed.startsWith('// ──')) break;
    section.push(trimmed.replace(/^\/\/ ?/, ''));
  }
  const pointerHits = flattenWhitespace(section.join('\n')).split(unsafeInventoryPointer).length - 1;
  assert.ok(pointerHits === 1,
    `src/lib.rs's Unsafe inventory section must reference \`${unsafeInventoryPointer}\` exactly once (found ${pointerHits}) — the README section was renamed or the pointer drifted`);
  const grepHits = section.filter(line => line.trim() === canonicalSeamGrep).length;
  assert.ok(grepHits === 1,
    `src/lib.rs's Unsafe inventory section must carry the canonical anchored seam grep as a full trimmed line exactly once (found ${grepHits}), pointing at src/ crates/ with the comment-proof anchor: \`${canonicalSeamGrep}\``);
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
  ['lib_rs_unsafe_inventory_header_matches_readme', libRsUnsafeInventoryHeaderMatchesReadme],
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
