// Cargo-free extraction-policy counterfactuals shared by node --test and the
// ghost-zero-cost runner self-test. These checks never build or run a probe.
import assert from 'node:assert/strict';
import {
  cargoBuildGraphAdditions,
  extractionChangedLines,
  extractionPolicy,
  zeroCostVerdict,
} from '../scripts/ghost_diff.mjs';

const message = 'build(workspace): extract sefer-policy';
const changed = new Map([
  ['Cargo.toml', ['members = ["crates/core/sefer-policy"]', 'sefer-policy = { path = "crates/core/sefer-policy" }']],
  ['src/policy.rs', ['pub fn policy() {}']],
  ['crates/core/sefer-policy/src/lib.rs', ['pub fn policy() {}']],
]);
const rename = [{ from: 'src/policy.rs', to: 'crates/core/sefer-policy/src/lib.rs' }];
const fullEvidence = {
  membersAdded: ['crates/core/sefer-policy'],
  pathDepsAdded: ['crates/core/sefer-policy'],
  renames: rename,
};
const baseManifest = '[workspace]\nmembers = ["crates/core/base"]\n\n[dependencies]\nbase = { path = "crates/core/base" }\n';
const headManifest = '[workspace]\nmembers = ["crates/core/base", "crates/core/new"]\n\n[dependencies]\nbase = { path = "crates/core/base" }\nnew = { path = "crates/core/new" }\n';
const manifestDiff = [
  'diff --git a/Cargo.toml b/Cargo.toml',
  '--- a/Cargo.toml',
  '+++ b/Cargo.toml',
  '@@ -1 +1 @@',
  '-members = ["crates/core/base"]',
  '+members = ["crates/core/base", "crates/core/new"]',
  '@@ -4 +4,2 @@',
  '+new = { path = "crates/core/new" }',
].join('\n');

export function checkExtractionPolicyCases() {
  let count = 0;
  const check = (name, fn) => { fn(); count++; };

  check('comment-only Cargo edit plus extraction trailer is not graph evidence', () => {
    const diff = 'diff --git a/Cargo.toml b/Cargo.toml\n@@ -1 +1 @@\n+# extraction documentation only';
    const graph = cargoBuildGraphAdditions(diff, baseManifest, baseManifest);
    assert.deepEqual(graph, { membersAdded: [], pathDepsAdded: [] });
    assert.equal(extractionPolicy(`docs: note\n\nExtraction: sefer-policy`, changed, [], { ...graph, renames: rename }), null);
  });

  check('new workspace member without src-to-crates rename is not enough', () => {
    assert.equal(extractionPolicy(message, changed, [], {
      membersAdded: ['crates/core/sefer-policy'], pathDepsAdded: [], renames: [],
    }), null);
  });

  check('root manifest graph additions come from added hunk lines and base/head graph delta', () => {
    assert.deepEqual(cargoBuildGraphAdditions(manifestDiff, baseManifest, headManifest), {
      membersAdded: ['crates/core/new'], pathDepsAdded: ['crates/core/new'],
    });
    const commentOnly = cargoBuildGraphAdditions(
      '+# added comment mentioning "crates/core/new"', baseManifest, baseManifest,
    );
    assert.deepEqual(commentOnly, { membersAdded: [], pathDepsAdded: [] });
  });

  check('full extraction with member, path dependency, and rename admits bounded proof', () => {
    const policy = extractionPolicy(message, changed, [], fullEvidence);
    assert.deepEqual(policy, {
      mode: 'extraction', irTolerance: 0.001,
      reason: 'declared crate extraction with Cargo build-graph changes',
    });
    const verdict = zeroCostVerdict({ policy, textSame: false, irBase: 1_000_000, irHead: 1_001_000 });
    assert.equal(verdict.ok, true);
  });

  check('qualifying rename with Ir delta beyond threshold remains red', () => {
    const policy = extractionPolicy(message, changed, [], fullEvidence);
    assert.notEqual(policy, null);
    const verdict = zeroCostVerdict({ policy, textSame: false, irBase: 1_000_000, irHead: 1_001_001 });
    assert.equal(verdict.ok, false);
    assert.ok(verdict.errors.includes('IR delta exceeds extraction tolerance'));
  });

  check('dirty, missing evidence, forbidden paths, and production ghost changes fail closed', () => {
    assert.equal(extractionPolicy(message, changed, [' M src/policy.rs'], fullEvidence), null);
    assert.equal(extractionPolicy(message, changed, [], null), null);
    assert.equal(extractionPolicy(message, new Map([...changed, ['build.rs', ['fn main() {}']]]), [], fullEvidence), null);
    assert.equal(extractionPolicy(message, new Map([...changed, ['src/ghost.rs', ['ghost_event!(...)']]]), [], fullEvidence), null);
  });

  const platformEvidence = {
    membersAdded: ['crates/platform/sefer-platform'],
    pathDepsAdded: ['crates/platform/sefer-platform'],
    renames: [{ status: 'R096', from: 'src/alloc_core/platform/os.rs', to: 'crates/platform/sefer-platform/src/os.rs' }],
  };

  check('renamed files contribute only their rename-aware edit hunks', () => {
    const moved = 'pub unsafe fn commit_pages(base: *mut u8, start: usize, end: usize) -> bool { unsafe { vmem::commit_range(base, start, end) } }';
    const full = new Map([
      ['Cargo.toml', ['members = ["crates/platform/sefer-platform"]']],
      ['src/alloc_core/platform/os.rs', [moved, '#[cfg(sefer_ghost)]']],
      ['crates/platform/sefer-platform/src/os.rs', [
        '#[cfg(sefer_ghost)]',
        moved,
        '//! delegates the SEGMENT OS aperture and the Miri `System.alloc` fallback',
        '/// provenance follows -Zmiri-strict-provenance',
      ]],
    ]);
    const hunks = new Map([['crates/platform/sefer-platform/src/os.rs', [
      '//! delegates the SEGMENT OS aperture and the Miri `System.alloc` fallback',
      '/// provenance follows -Zmiri-strict-provenance',
    ]]]);
    const scoped = extractionChangedLines(full, platformEvidence.renames, hunks);
    assert.deepEqual([...scoped.keys()], ['Cargo.toml', 'crates/platform/sefer-platform/src/os.rs']);
    assert.deepEqual(scoped.get('crates/platform/sefer-platform/src/os.rs'), hunks.get('crates/platform/sefer-platform/src/os.rs'));
    assert.deepEqual(extractionPolicy(message, scoped, [], platformEvidence), {
      mode: 'extraction', irTolerance: 0.001,
      reason: 'declared crate extraction with Cargo build-graph changes',
    });
    assert.equal(extractionPolicy(message, full, [], platformEvidence), null);
    assert.equal(extractionChangedLines(full, platformEvidence.renames, 'not a map'), null);
  });

  check('Miri and loom prose in comments does not block a qualifying extraction', () => {
    const changed = new Map([
      ['Cargo.toml', ['members = ["crates/platform/sefer-platform"]', 'sefer-platform = { path = "crates/platform/sefer-platform" }']],
      ['crates/platform/sefer-platform/src/os.rs', [
        '//! delegates the SEGMENT OS aperture, fatal and the Miri `System.alloc` fallback',
        '/// provenance follows -Zmiri-strict-provenance; loom and kani models cover',
        '/// the checking builds (docs/GHOST_CHECKS_PLAN.md).',
        '#[cfg(any(',
        '    feature = "primordial-lazy-commit",',
        '    feature = "small-segment-lazy-commit",',
        '    feature = "large-reserved-capacity",',
        '))]',
        'pub unsafe fn commit_pages(base: *mut u8, start: usize, end: usize) -> bool { unsafe { vmem::commit_range(base, start, end) } }',
      ]],
      ['crates/platform/sefer-platform/README.md', ['documented alongside the loom and miri models']],
    ]);
    assert.deepEqual(extractionPolicy(message, changed, [], platformEvidence), {
      mode: 'extraction', irTolerance: 0.001,
      reason: 'declared crate extraction with Cargo build-graph changes',
    });
  });

  check('ghost code in a changed code line or a ghost source path keeps the proof required', () => {
    const codeLines = [
      'ghost_assert!(route_id != 0);',
      '#[cfg(sefer_ghost)]',
      '#[cfg(any(loom, miri, kani, sefer_ghost))]',
      '#[cfg(any(  loom ,  miri ,  kani ,  sefer_ghost  ))]',
      'let logged = "ghost_assert! logged";',
    ];
    for (const codeLine of codeLines) {
      const changed = new Map([
        ['Cargo.toml', ['members = ["crates/platform/sefer-platform"]']],
        ['crates/platform/sefer-platform/src/os.rs', ['//! Miri fallback documented', codeLine]],
      ]);
      assert.equal(extractionPolicy(message, changed, [], platformEvidence), null, codeLine);
    }
    const ghostPath = new Map([
      ['Cargo.toml', ['members = ["crates/platform/sefer-platform"]']],
      ['src/ghost/route_probe.rs', ['pub fn probe() {}']],
    ]);
    assert.equal(extractionPolicy(message, ghostPath, [], platformEvidence), null);
  });

  return count;
}
