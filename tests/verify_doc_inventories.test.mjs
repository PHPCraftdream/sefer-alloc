import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import {
  architectureTestFileCountMatchesReality,
  libRsUnsafeInventoryHeaderMatchesReadme,
  productionFeatureBundleDocSitesMatchCargoToml,
  readmeUnsafeInventoryMatchesReality,
  verificationInventoryMatchesDocs,
} from '../scripts/verify-doc-inventories.mjs';

function fixture(run) {
  const root = mkdtempSync(join(tmpdir(), 'verify-doc-inventories-'));
  const put = (path, text) => {
    const file = join(root, path);
    mkdirSync(join(file, '..'), { recursive: true });
    writeFileSync(file, text);
  };
  try {
    run({ root, put });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test('architecture_test_file_count_matches_reality passes and rejects stale count', () => {
  fixture(({ root, put }) => {
    put('tests/one.rs', '');
    put('tests/two.rs', '');
    put('tests/nested/ignored.rs', '');
    put('docs/ARCHITECTURE.md', '`tests/*.rs` (2 files)');
    assert.doesNotThrow(() => architectureTestFileCountMatchesReality(root));
    put('docs/ARCHITECTURE.md', '`tests/*.rs` (3 files)');
    assert.throws(() => architectureTestFileCountMatchesReality(root), /test-file count is stale/);
  });
});

test('verification_inventory_matches_docs passes and rejects a mutated inventory token', () => {
  fixture(({ root, put }) => {
    put('tests/check.rs', '');
    put('tests/loom_local.rs', '');
    put('examples/example.rs', '');
    put('benches/bench.rs', '');
    const memberA = 'crates/primitives/once-ptr-cell/tests/loom_once_ptr_cell.rs';
    const memberB = 'crates/primitives/tagged-index-stack/tests/loom_aba.rs';
    const readmeSection = [
      '## Verification evidence',
      '**2 integration test files** **1 example binaries** **1 benches** **1 root Loom models**',
      '`tests/*.rs` (2 files) `examples/*.rs` (1 files) `benches/*.rs` (1 files)',
      'tests/loom_local.rs', memberA, memberB, 'narrow_domain_unchecked_storage',
      '## Other section',
    ].join('\n');
    const architectureSection = [
      '## 8. Verification stack', 'tests/loom_local.rs', memberA, memberB,
      'narrow_domain_unchecked_storage', '## Other section',
    ].join('\n');
    put('README.md', readmeSection);
    put('docs/ARCHITECTURE.md', architectureSection);
    assert.doesNotThrow(() => verificationInventoryMatchesDocs(root));
    put('README.md', readmeSection.replace('**2 integration test files**', '**3 integration test files**'));
    assert.throws(() => verificationInventoryMatchesDocs(root), /README verification inventory is missing canonical token/);
  });
});

test('readme_unsafe_inventory_matches_reality checks exact paths and per-file counts', () => {
  fixture(({ root, put }) => {
    put('src/seam.rs', '  #![allow(unsafe_code)]\n// #[allow(unsafe_code)]\n');
    put('crates/member/site.rs.tmpl', '#[allow(unsafe_code)]\n');
    const readme = [
      '### Where `unsafe` lives (the complete list)',
      '#### Tier 1 — module-level seams',
      '| File | Sites | Audit notes |',
      '|---|---:|---|',
      '| [`src/seam.rs`](src/seam.rs) | 1 | — |',
      '#### Tier 2 — item-scoped allows',
      '| File | Sites | Audit notes |',
      '|---|---:|---|',
      '| [`crates/member/site.rs.tmpl`](crates/member/site.rs.tmpl) | 1 | — |',
    ].join('\n');
    put('README.md', readme);
    assert.doesNotThrow(() => readmeUnsafeInventoryMatchesReality(root));
    put('README.md', readme.replaceAll('crates/member/site.rs.tmpl', 'src/stale.rs'));
    assert.throws(() => readmeUnsafeInventoryMatchesReality(root), /tier 2 paths or per-file counts drifted/);
    put('README.md', readme.replaceAll('src/seam.rs', 'src/stale.rs'));
    assert.throws(() => readmeUnsafeInventoryMatchesReality(root), /tier 1 paths or per-file counts drifted/);
    put('README.md', readme.replace('| 1 | — |', '| 2 | — |'));
    assert.throws(() => readmeUnsafeInventoryMatchesReality(root), /tier 1 paths or per-file counts drifted/);
  });
});

test('lib_rs_unsafe_inventory_header_matches_readme passes and rejects stale pointer or wrong grep', () => {
  fixture(({ root, put }) => {
    const readme = ['## Intro', '### Where `unsafe` lives (the complete list)', 'body', '## Other'].join('\n');
    const lib = [
      '// ── Unsafe inventory — see README.md, not here ─────────────',
      '//',
      '// The complete list of `unsafe` seams lives in README.md § "Where `unsafe`',
      '// lives (the complete list)" (anchor `#where-unsafe-lives-the-complete-list`).',
      '//',
      '//   grep -rnE \'^\\s*#!?\\[allow\\(unsafe_code\\)\\]\' src/ crates/',
      '//',
      '// ── next section ────────────────────────────────────────────',
      '// later mention: grep -rnE \'^\\s*#!?\\[allow\\(unsafe_code\\)\\]\' src/ crates/',
    ].join('\n');
    const grep = "grep -rnE '^\\s*#!?\\[allow\\(unsafe_code\\)\\]' src/ crates/";
    put('README.md', readme);
    put('src/lib.rs', lib);
    assert.doesNotThrow(() => libRsUnsafeInventoryHeaderMatchesReadme(root));
    put('README.md', readme.replace('(the complete list)', '(renamed)'));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /authoritative unsafe-inventory heading line/);
    put('README.md', readme);
    put('src/lib.rs', lib.replace('lives (the complete list)', 'lives (the full list)'));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /exactly once \(found 0\) — the README section was renamed/);
    put('src/lib.rs', lib.replace(' (anchor `#where-unsafe-lives-the-complete-list`)', ''));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /exactly once \(found 0\)/);
    put('src/lib.rs', lib.replace('src/ crates/', 'src/'));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /canonical anchored seam grep as a full trimmed line exactly once \(found 0\)/);
    put('src/lib.rs', lib.replace("grep -rnE '^", "grep -rnE '"));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /canonical anchored seam grep as a full trimmed line exactly once \(found 0\)/);
    put('src/lib.rs', lib.replace(grep, ''));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /canonical anchored seam grep as a full trimmed line exactly once \(found 0\)/);
    put('src/lib.rs', lib.replace('// ── next section', `//   ${grep}\n//\n// ── next section`));
    assert.throws(() => libRsUnsafeInventoryHeaderMatchesReadme(root), /canonical anchored seam grep as a full trimmed line exactly once \(found 2\)/);
  });
});

test('production_feature_bundle_doc_sites_match_cargo_toml passes and rejects an extra feature', () => {
  fixture(({ root, put }) => {
    put('Cargo.toml', 'production = ["alpha", "beta", "gamma", "delta"]\n');
    const canonical = 'alpha + beta + gamma + delta';
    const sites = ['src/lib.rs', 'src/global/sefer_alloc/core.rs', 'README.md', 'docs/INTEGRATION.md'];
    for (const site of sites) put(site, `The production bundle is ${canonical}.`);
    assert.doesNotThrow(() => productionFeatureBundleDocSitesMatchCargoToml(root));
    put('README.md', `The production bundle is ${canonical} + surprise.`);
    assert.throws(() => productionFeatureBundleDocSitesMatchCargoToml(root), /stale `production` bundle/);
  });
});
