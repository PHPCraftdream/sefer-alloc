import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import {
  architectureTestFileCountMatchesReality,
  libRsSeamInventoryMatchesCanonicalGrep,
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

test('lib_rs_seam_inventory_matches_canonical_grep passes and rejects a phantom seam', () => {
  fixture(({ root, put }) => {
    put('src/foo/bar.rs', '#![allow(unsafe_code)]\n');
    const lib = [
      '// confined modules lift this with',
      '// * `foo::bar` — documented seam',
      '// is enforced by the compiler',
    ].join('\n');
    put('src/lib.rs', lib);
    assert.doesNotThrow(() => libRsSeamInventoryMatchesCanonicalGrep(root));
    put('src/lib.rs', lib.replace('foo::bar', 'foo::missing'));
    assert.throws(() => libRsSeamInventoryMatchesCanonicalGrep(root), /prose seam inventory drifted/);
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
