import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { checkExtractionPolicyCases } from './ghost_extraction_policy_fixtures.mjs';
import { checkLayerRestructuringPolicyCases } from './ghost_layer_restructuring_policy_fixtures.mjs';

import {
  extractionChangedLines,
  extractionPolicy,
  hasLayerRestructuringAttempt,
  layerRestructuringPolicy,
  productionGhostTokens,
  rustCodeLine,
  zeroCostSkip,
  zeroCostVerdict,
} from '../scripts/ghost_diff.mjs';

const CARGO_CHANGE = new Map([['Cargo.toml', ['[workspace]']]]);
const EXTRACTION_MESSAGE = 'build(workspace): extract sefer-platform';
const EXTRACTION_POLICY = { mode: 'extraction', irTolerance: 0.001, reason: 'declared crate extraction with Cargo build-graph changes' };
const cargoPaths = (...paths) => new Map(paths.map((path) => [path, []]));
const EVIDENCE = { membersAdded: ['crates/platform/sefer-platform'], pathDepsAdded: [], renames: [{ from: 'src/platform.rs', to: 'crates/platform/sefer-platform/src/lib.rs' }] };
const BASE_IR = 1_000_000;
const WITHIN_EXTRACTION = { policy: EXTRACTION_POLICY, textSame: false, irBase: BASE_IR, irHead: BASE_IR + 1_000 };
const ZERO_COST_RUNNER = readFileSync(new URL('../scripts/ghost_zero_cost.mjs', import.meta.url), 'utf8');

test('production ghost verdict rejects only head-only tokens and reports both sides', () => {
  // Execute the runner's real token-difference and verdict-call fragment so a
  // rollback to head-only or base+head union behavior breaks this contract.
  assert.ok(ZERO_COST_RUNNER.includes("const productionGhostTokensForBinary = (bin) => productionGhostTokens(`${run('objdump', ['-t', '-C', bin])}\\n${readFileSync(bin).toString('latin1')}`);"));
  assert.ok(ZERO_COST_RUNNER.includes('const tokensBase = productionGhostTokensForBinary(binBase);'));
  assert.ok(ZERO_COST_RUNNER.includes('const tokensHead = productionGhostTokensForBinary(binHead);'));
  assert.ok(ZERO_COST_RUNNER.includes('tokens_base: tokensBase,\n    tokens: tokensHead,\n    tokens_new: tokensNew,'));
  const tokenDiff = /const tokensNew = tokensHead\.filter\(\(token\) => !tokensBase\.includes\(token\)\);/.exec(ZERO_COST_RUNNER);
  const verdictCall = /const verdict = zeroCostVerdict\(\{[^;]*ghostTokens: tokensNew\s*\}\);/s.exec(ZERO_COST_RUNNER);
  assert.ok(tokenDiff, 'runner must derive tokensNew with the base-exclusion matcher');
  assert.ok(verdictCall, 'actual runner verdict call must receive tokensNew');
  const runRunnerFragment = new Function('tokensBase', 'tokensHead', 'zeroCostVerdict', 'proofPolicy', 'textSame', 'irBase', 'irHead', 'requireIr',
    `${tokenDiff[0]}\n${verdictCall[0]}\nreturn { tokensNew, verdict };`);

  const check = (baseSource, headSource) => {
    const tokensBase = productionGhostTokens(baseSource);
    const tokensHead = productionGhostTokens(headSource);
    const { tokensNew, verdict } = runRunnerFragment(tokensBase, tokensHead, zeroCostVerdict, null, true, BASE_IR, BASE_IR, false);
    return { tokensBase, tokensHead, tokensNew, verdict };
  };

  const shared = check('base: ghost_event', 'head: ghost_event');
  assert.deepEqual(shared.tokensBase, ['ghost_event']);
  assert.deepEqual(shared.tokensHead, ['ghost_event']);
  assert.deepEqual(shared.tokensNew, []);
  assert.equal(shared.verdict.ok, true, 'a shared base/head token is diagnostic, not red');

  const headOnly = check('base has no ghost token', 'head: ghost_assert');
  assert.deepEqual(headOnly.tokensBase, []);
  assert.deepEqual(headOnly.tokensHead, ['ghost_assert']);
  assert.deepEqual(headOnly.tokensNew, ['ghost_assert']);
  assert.equal(headOnly.verdict.ok, false, 'a head-only token keeps the verdict red');
  assert.ok(headOnly.verdict.errors.includes('production ghost tokens detected'));

  const baseOnly = check('base: ghost::shadow', 'head has no ghost token');
  assert.deepEqual(baseOnly.tokensBase, ['ghost::shadow']);
  assert.deepEqual(baseOnly.tokensHead, []);
  assert.deepEqual(baseOnly.tokensNew, []);
  assert.equal(baseOnly.verdict.ok, true, 'a base-only token is diagnostic, not red');
});

test('extraction build-graph counterfactual suite', () => {
  assert.equal(checkExtractionPolicyCases(), 9);
});

test('layer restructuring policy counterfactual suite', () => {
  assert.equal(checkLayerRestructuringPolicyCases(), 13);
});

test('layer declaration cannot use runtime skip or prefix behavior', () => {
  const message = 'perf(runtime): restructure\n\nRestructuring: layers';
  assert.equal(hasLayerRestructuringAttempt(message), true);
  assert.equal(layerRestructuringPolicy([message], new Map(), [], null), null);
  assert.ok(ZERO_COST_RUNNER.includes('if (reason && !proofPolicy && !layerAttempt)'));
  assert.ok(ZERO_COST_RUNNER.includes('const prefix = layerAttempt ? null : declaredRuntimePrefix'));
});

test('extraction changed-line scan reads Rust code, not comments or prose', () => {
  assert.equal(rustCodeLine('//! delegates the Miri `System.alloc` fallback'), '');
  assert.equal(rustCodeLine('let span = 64; // miri note'), 'let span = 64; ');
  assert.equal(rustCodeLine('let url = "https://example.invalid/a//b"; // doc'), 'let url = "https://example.invalid/a//b"; ');
  assert.ok(/cfg\s*\(\s*sefer_ghost\s*\)/.test(rustCodeLine('#[cfg(sefer_ghost)]')));
  const renames = [{ status: 'R100', from: 'src/platform.rs', to: 'crates/platform/sefer-platform/src/platform.rs' }];
  const changed = new Map([
    ['Cargo.toml', ['members = ["crates/platform/sefer-platform"]']],
    ['src/platform.rs', ['pub fn platform() {}', '#[cfg(sefer_ghost)]']],
    ['crates/platform/sefer-platform/src/platform.rs', ['pub fn platform() {}', '#[cfg(sefer_ghost)]']],
  ]);
  const scoped = extractionChangedLines(changed, renames, new Map([
    ['crates/platform/sefer-platform/src/platform.rs', ['pub fn platform() {}']],
  ]));
  assert.deepEqual([...scoped.keys()], ['Cargo.toml', 'crates/platform/sefer-platform/src/platform.rs']);
});

test('zeroCostSkip retains existing clean-runtime and fail-closed cases', () => {
  const runtimeSubject = ['perf(runtime): preserve fast path'];
  assert.notEqual(zeroCostSkip(runtimeSubject, new Map([['src/a.rs', ['let y = 1;']]])), null);
  assert.equal(zeroCostSkip(['test(verification): ghost checks'], new Map([['src/a.rs', ['x']]])), null);
  assert.equal(zeroCostSkip(runtimeSubject, new Map([['src/a.rs', ['x']]]), [' M src/a.rs']), null);
  assert.equal(zeroCostSkip(runtimeSubject, new Map([['build.rs', ['x']]])), null);
  assert.notEqual(zeroCostSkip(runtimeSubject, new Map([['src/a.rs', ['StateByte::shadow_reject(site);']]])), null);
});

test('extractionPolicy recognizes the required workspace subject and Cargo changes', () => {
  assert.deepEqual(extractionPolicy([EXTRACTION_MESSAGE], CARGO_CHANGE, [], EVIDENCE), EXTRACTION_POLICY);
  assert.deepEqual(extractionPolicy(EXTRACTION_MESSAGE, cargoPaths('Cargo.toml', 'crates/core/sefer-policy/Cargo.toml'), [], EVIDENCE), EXTRACTION_POLICY);
});

test('extractionPolicy recognizes a complete Extraction trailer in the full message', () => {
  const message = 'build(workspace): move a crate\n\nDetails of the workspace move.\nExtraction: sefer-platform';
  assert.deepEqual(extractionPolicy(message, cargoPaths('Cargo.toml'), [], EVIDENCE), EXTRACTION_POLICY);
  assert.deepEqual(extractionPolicy(['docs: record move\n\nExtraction: sefer-segment-os'], CARGO_CHANGE, [], EVIDENCE), EXTRACTION_POLICY);
});

test('extractionPolicy fails closed without a marker or Cargo path, and on dirty state', () => {
  assert.equal(extractionPolicy(['build(workspace): reorganize files'], CARGO_CHANGE, [], EVIDENCE), null);
  assert.equal(extractionPolicy(EXTRACTION_MESSAGE, cargoPaths('src/lib.rs'), [], EVIDENCE), null);
  assert.equal(extractionPolicy(EXTRACTION_MESSAGE, ['Cargo.toml'], [' M src/lib.rs'], EVIDENCE), null);
  assert.equal(extractionPolicy('build(workspace): note extract later', CARGO_CHANGE, [], EVIDENCE), null);
  assert.equal(extractionPolicy(EXTRACTION_MESSAGE, CARGO_CHANGE, [], null), null);
});

test('extractionPolicy refuses build scripts, cargo config and toolchain inputs', () => {
  for (const path of ['build.rs', 'crates/example/build.rs', '.cargo/config.toml', 'nested/.cargo/config', 'rust-toolchain', 'rust-toolchain.toml']) {
    assert.equal(extractionPolicy(EXTRACTION_MESSAGE, cargoPaths('Cargo.toml', path), [], EVIDENCE), null, `${path} must block extraction policy`);
  }
});

test('ordinary verdicts require identical production text and identical IR', () => {
  assert.equal(zeroCostVerdict({ textSame: false, irBase: BASE_IR, irHead: BASE_IR }).ok, false, 'text drift is red even with identical IR');
  assert.equal(zeroCostVerdict({ textSame: true, irBase: BASE_IR, irHead: BASE_IR + 1 }).ok, false, 'ordinary IR drift is red');
  assert.deepEqual(zeroCostVerdict({ textSame: true, irBase: BASE_IR, irHead: BASE_IR }), {
    ok: true, errors: [], mode: 'exact', ir_delta: 0,
  });
});

test('extraction verdict accepts the inclusive positive and negative 0.1% boundaries', () => {
  assert.deepEqual(zeroCostVerdict(WITHIN_EXTRACTION), {
    ok: true, errors: [], mode: 'extraction', ir_delta: 1_000,
  });
  assert.deepEqual(zeroCostVerdict({ ...WITHIN_EXTRACTION, irHead: BASE_IR - 1_000 }), {
    ok: true, errors: [], mode: 'extraction', ir_delta: -1_000,
  });
});

test('extraction verdict rejects deltas just outside either tolerance boundary', () => {
  for (const irHead of [BASE_IR + 1_001, BASE_IR - 1_001]) {
    const verdict = zeroCostVerdict({ ...WITHIN_EXTRACTION, irHead });
    assert.equal(verdict.ok, false);
    assert.ok(verdict.errors.includes('IR delta exceeds extraction tolerance'));
  }
});

test('extraction verdict requires positive safe-integer IR counts', () => {
  const invalidCounts = [null, 0, NaN, Infinity, -Infinity, 1.5, Number.MAX_SAFE_INTEGER + 1];
  for (const count of invalidCounts) {
    for (const values of [{ irBase: count, irHead: BASE_IR }, { irBase: BASE_IR, irHead: count }]) {
      const verdict = zeroCostVerdict({ ...WITHIN_EXTRACTION, ...values });
      assert.equal(verdict.ok, false, `${String(count)} must be invalid`);
      assert.ok(verdict.errors.includes('extraction requires positive safe-integer IR counts'));
      assert.equal(verdict.ir_delta, null);
    }
  }
});

test('productionGhostTokens recognizes real Rust ghost identifiers and the ghost panic string', () => {
  // Identifiers/macros are present in src/lib.rs and production call sites;
  // shadow APIs and the panic form are in crates/primitives/sefer-ghost.
  const cases = [
    ['sefer_ghost', 'sefer_ghost'],
    ['ghost_assert', 'crate::ghost::ghost_assert!(condition);'],
    ['ghost_event', 'crate::ghost::ghost_event!("route-pin/held", 0, 1);'],
    ['ghost::shadow', 'ghost::shadow::record(event);'],
    ['shadow::record', 'shadow::record(event);'],
    ['shadow::count', 'shadow::count()'],
    ['shadow::reset', 'shadow::reset()'],
    ['shadow::overflowed', 'shadow::overflowed()'],
    ['ghost check', 'panic!("ghost check {rule} failed: {msg}")'],
  ];
  for (const [expected, source] of cases) {
    assert.ok(productionGhostTokens(source).includes(expected), `${expected} must be recognized`);
  }
  assert.deepEqual(productionGhostTokens('StateByte::shadow_reject(site);'), []);
});

test('ghost-token leakage rejects extraction even when IR is within tolerance', () => {
  const ghostTokens = productionGhostTokens('crate::ghost::ghost_event!("leak", 0, 1);');
  const verdict = zeroCostVerdict({ ...WITHIN_EXTRACTION, ghostTokens });
  assert.equal(verdict.ok, false);
  assert.ok(verdict.errors.includes('production ghost tokens detected'));
  assert.equal(verdict.ir_delta, 1_000);
});
