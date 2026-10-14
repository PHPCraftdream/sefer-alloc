// Shared R20 PRF-01 C3 cell inventory and source/freeze metadata.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export const ARMS = Object.freeze(['A1', 'A2', 'B1', 'B2', 'C0']);
export const FEATURES = Object.freeze(['bench-internals', 'internals', 'production']);
export const V2_BENEFIT = Object.freeze(['multiseg_cold_256k', 'seg_cycle_decommit_256k', 'growth16_n', 'growth16_2n', 'growth16_4n']);
export const HOT_FAMILIES = Object.freeze(['small_churn_16b', 'churn_256b', 'churn_write_256b', 'aligned_churn_640b_a128']);
export const HOT_CELLS = Object.freeze(HOT_FAMILIES.flatMap(f => ['n', '2n', '4n'].map(n => `r20_prf01_${f}_${n}`)));
export const CONSTRUCTION = 'r20_prf01_start_one_alloc_free_16b';
export const MIMALLOC = Object.freeze(['mimalloc_small_churn_16b', 'mimalloc_small_churn_16b_2n', 'mimalloc_churn_256b', 'mimalloc_cold_alloc_free_256x16b', 'mimalloc_cold_alloc_free_256x16b_2n', 'mimalloc_cold_alloc_free_256x16b_4n', 'mimalloc_cold_alloc_only_256x16b', 'mimalloc_cold_alloc_only_256x16b_2n', 'mimalloc_cold_alloc_only_256x16b_4n', 'mimalloc_cold_alloc_free_256x64b', 'mimalloc_recycle_alloc_free_256x16b', 'mimalloc_recycle_alloc_free_256x64b', 'mimalloc_bootstrap_proxy']);
export const PUBLICATION = 'r19_prf01_publication';
// Immutable B candidate, bound by the pre-measurement re-freeze (prereg Re-freeze log).
export const CANDIDATE = '157d6c668c4c01f07c61efc5a68563d5f131cc0d';
export const SAME_WORD_PRODUCER = 'r19_prf01_joined_producer_same_word';
export const DISTINCT_WORD_PRODUCER = 'r20_prf01_c3_producer_distinct_words';
export const LARGE_REMOTE_FREE = 'r20_prf01_c3_remote_large_free';
export const TRIM_CELLS = Object.freeze(['r20_prf01_strict_trim_payload', 'r20_prf01_bounded_maintenance']);
export const TRIM_TARGET = 'r20_prf01_trim_iai';
export const INVENTORY = Object.freeze([...V2_BENEFIT, ...HOT_CELLS, CONSTRUCTION, ...MIMALLOC, PUBLICATION, SAME_WORD_PRODUCER, DISTINCT_WORD_PRODUCER, LARGE_REMOTE_FREE, ...TRIM_CELLS]);
export const PRODUCERS = Object.freeze({
  [SAME_WORD_PRODUCER]: Object.freeze({target: 'perf_gate_iai', cfg: 'r19_prf01_producer_control', symbol: '<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window', freeCount: 4096, blockBytes: 16}),
  [DISTINCT_WORD_PRODUCER]: Object.freeze({target: 'r20_prf01_c3_cells_iai', cfg: 'r20_prf01_c3_producer_control', symbol: 'r20_prf01_c3_cells_iai::gate::producer_free_window', freeCount: 4096, blockBytes: 1024}),
  [LARGE_REMOTE_FREE]: Object.freeze({target: 'r20_prf01_c3_cells_iai', cfg: 'r20_prf01_c3_producer_control', symbol: 'r20_prf01_c3_cells_iai::gate::producer_free_window', freeCount: 1, blockBytes: null, blockBytesSource: 'SegmentLayout::SMALL_MAX + 1', customSymbol: true}),
});
export const SUMMARY_COUNTERS = Object.freeze(['pending_words_visited', 'pending_words_exchanged', 'nonempty_cuts', 'discovery_records_reclaimed', 'routed_scans', 'summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw', 'strict_trim_remaining']);
export const SUMMARY_ONLY_COUNTERS = Object.freeze(['summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw']);
export const ALL_COUNTERS = Object.freeze([...SUMMARY_COUNTERS, 'quiescent_summary_missing_bits', 'sizeof_small_sidecar']);
export const C3_ORACLE_COUNTERS = Object.freeze(['pending_words_visited', 'summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw', 'discovery_records_reclaimed', 'quiescent_summary_missing_bits', 'sizeof_small_sidecar']);
export function summaryCounterAllowed(value, arm) {
  if (arm === 'A') return value === undefined || value === null || value === 0;
  return arm === 'B' && Number.isSafeInteger(value) && value >= 0;
}
export const CANDIDATE_ORACLE_INTERFACE = Object.freeze({example: 'examples/r20_prf01_c3_oracle.rs', invocation: '<oracle> --c3-json <cell> <processId>', requiredFields: C3_ORACLE_COUNTERS, note: 'Future C3 driver interface; this module neither implements nor assumes the candidate oracle.'});

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceFiles = {
  iai: path.join(root, 'benches/perf_gate_iai.rs'),
  controls: path.join(root, 'benches/r20_prf01_controls_iai.rs'),
  c3: path.join(root, 'benches/r20_prf01_c3_cells_iai.rs'),
  trim: path.join(root, 'benches/r20_prf01_trim_iai.rs'),
};
export const FROZEN_FILES = Object.freeze([
  'docs/perf/R20_PRF01_C3_DEFAULT_PREREG.md', 'docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md',
  'docs/reviews/2026-10-13-r20-prf01-remote-free-discovery-flag-design.md', 'docs/perf/R20_CONTROL_GEOMETRY_RULE.md',
  'docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md', 'Cargo.toml',
  'benches/r20_prf01_c3_cells_iai.rs', 'benches/r20_prf01_controls_iai.rs', 'benches/r20_prf01_trim_iai.rs',
  'benches/support/r20_prf01_controls.rs', 'benches/support/r19_prf01.rs', 'benches/perf_gate_iai.rs',
  'scripts/r20_prf01_c3_gate.mjs', 'scripts/r20_prf01_c3_run.mjs', 'scripts/r20_prf01_c3_verify_artifacts.mjs',
  'scripts/r20_prf01_sidecar_summary_gate.mjs', 'scripts/r20_prf01_sidecar_summary_run.mjs', 'scripts/r20_prf01_verify_artifacts.mjs',
  'scripts/r20_prf01_c3_support.mjs', 'scripts/r20_prf01_c3_fixture.mjs', 'scripts/r20_prf01_c3_runner_support.mjs', 'tests/r19_prf01_sidecar_oracle.rs', 'examples/r19_prf01_oracle.rs',
  'examples/r20_prf01_c3_oracle.rs', 'tests/r9_bounded_background_maintenance.rs',
]);

export function targetFor(name) {
  if (V2_BENEFIT.includes(name) || MIMALLOC.includes(name) || name === PUBLICATION || name === SAME_WORD_PRODUCER) return 'iai';
  if (HOT_CELLS.includes(name) || name === CONSTRUCTION) return 'controls';
  if (name === DISTINCT_WORD_PRODUCER || name === LARGE_REMOTE_FREE) return 'c3';
  if (TRIM_CELLS.includes(name)) return 'trim';
  return null;
}

function checkSource(file, names, externalMissing = false) {
  if (!fs.existsSync(file)) return names.map(name => ({name, status: externalMissing ? 'external-prerequisite-missing' : 'source-file-missing', file: path.relative(root, file).replaceAll('\\', '/')}));
  const source = fs.readFileSync(file, 'utf8');
  return names.map(name => ({name, status: new RegExp(`\\bfn\\s+${name.replace(/[.*+?^${}()|[\\]\\]/g, '\\$&')}\\s*\\(`).test(source) ? 'ok' : 'source-function-missing', file: path.relative(root, file).replaceAll('\\', '/')}));
}
export function validateSources() {
  const byTarget = Object.fromEntries(['iai', 'controls', 'c3', 'trim'].map(target => [target, INVENTORY.filter(name => targetFor(name) === target)]));
  return ['iai', 'controls', 'c3'].flatMap(target => checkSource(sourceFiles[target], byTarget[target]))
    .concat(checkSource(sourceFiles.trim, byTarget.trim, true));
}

export function runRunnerSelftest({
  sourceIdentity, sha, parseProfiles, producerSettings, worktreeAddArgs,
  buildRolePlan, expectedOracleCfg,
  cleanupOwnership, rehearsalRustflags, rehearsalExitCode, rehearsalVerdict,
  distinguishRustflags,
}) {
  const sourceA = sourceIdentity('a'.repeat(40), 'b'.repeat(40), []);
  assert.deepEqual(sourceIdentity('a'.repeat(40), 'b'.repeat(40), []), sourceA,
    'same-source identity must be label independent');
  assert.notEqual(
    sourceIdentity('a'.repeat(40), 'b'.repeat(40), [{ path: FROZEN_FILES[0], sha256: sha('x') }]).identitySha256,
    sourceA.identitySha256,
    'overlay mutant must drift',
  );
  const frozenExpected = [{ path: 'sample', sha256: sha('x') }];
  assert.throws(
    () => assert.deepEqual([{ path: 'sample', sha256: sha('y') }], frozenExpected),
    'frozen snapshot mutant must reject',
  );
  const log = '==123== Command: /tmp/bench --iai-run r20_prf01_c3_cells 2 0';
  // Two cells of one bench binary share the toggle symbol; only the cell's own path segment selects it.
  const record = cell => 'callgrind: Arguments: --callgrind-out-file=home-r20_prf01_c3_producer_distinct_words/'
    + `r20_prf01_c3_cells_iai/${cell}/x --toggle-collect=exact::same_word ` + producerSettings.join(' ');
  const debug = [
    record('r20_prf01_c3_producer_distinct_words'),
    record('r20_prf01_c3_remote_large_free'),
  ].join('\n');
  const fixture = 'pid: 123\npart: 1\nthread: 1\ncmd: /tmp/bench --iai-run r20_prf01_c3_cells 2 0\nevents: Ir\ntotals: 10\nfn=std::thread::closure\n1 5\ncfn=exact::same_word\ncalls=1 1\n1 5\nfn=exact::same_word\n1 5\n';
  const childFixture = 'pid: 123\npart: 1\nthread: 2\ncmd: /tmp/bench --iai-run r20_prf01_c3_cells 2 0\nevents: Ir\ntotals: 5\nfn=std::thread::closure\n1 0\ncfn=exact::same_word\ncalls=1 1\n1 5\nfn=exact::same_word\n1 5\n';
  const profileOptions = {
    cellIndex: 2,
    producerSymbol: 'exact::same_word',
    groupName: 'r20_prf01_c3_cells',
    cellName: 'r20_prf01_c3_producer_distinct_words',
    debug,
    expectedExecutable: '/tmp/bench',
  };
  const proof = parseProfiles([fixture, childFixture], log, profileOptions);
  assert.equal(proof.childIr, 5);
  assert.equal(proof.boundaryExecutions, 1);
  assert.throws(() => parseProfiles(
    [fixture.replace('exact::same_word', 'exact::distinct_word')],
    log,
    profileOptions,
  ));
  assert.equal(INVENTORY.length, 37);
  assert.equal(new Set(INVENTORY).size, 37);
  assert.throws(() => assert.ok(/^[0-9a-f]{40}$/.test('TBD')));
  assert.deepEqual(worktreeAddArgs('/tmp/A', 'a'.repeat(40)), [
    'worktree', 'add', '--detach', '/tmp/A', 'a'.repeat(40),
  ]);
  assert.deepEqual(cleanupOwnership('/root', ['A', 'B', 'C']).ownedWorktrees, ['A', 'B', 'C']);
  assert.deepEqual(buildRolePlan('A', false), {
    primary: undefined,
    oracle: '--cfg r19_sidecar_summary_oracle',
    control: '--cfg r19_prf01_producer_control',
    c3Control: '--cfg r20_prf01_c3_producer_control',
  });
  assert.deepEqual(buildRolePlan('B', false), {
    primary: undefined,
    oracle: '--cfg r19_sidecar_summary_oracle --cfg r20_prf01_c3_oracle',
    control: '--cfg r19_prf01_producer_control',
    c3Control: '--cfg r20_prf01_c3_producer_control',
  });
  assert.deepEqual(buildRolePlan('B', true), {
    primary: distinguishRustflags,
    oracle: `${distinguishRustflags} --cfg r19_sidecar_summary_oracle`,
    control: `${distinguishRustflags} --cfg r19_prf01_producer_control`,
    c3Control: `${distinguishRustflags} --cfg r20_prf01_c3_producer_control`,
  });
  assert.deepEqual(expectedOracleCfg('B', false), ['r19_sidecar_summary_oracle', 'r20_prf01_c3_oracle']);
  assert.deepEqual(expectedOracleCfg('B', true), ['r19_sidecar_summary_oracle']);
  assert.equal(rehearsalRustflags('B', true), distinguishRustflags);
  assert.equal(rehearsalRustflags('A', true), undefined);
  assert.equal(rehearsalRustflags('B', false), undefined);
  assert.equal(rehearsalExitCode(), 0);
  for (const status of ['INCOMPLETE', 'judge-exit=0', 'judge-exit=1']) {
    const verdict = rehearsalVerdict(status, 'REHEARSAL: stale');
    assert.match(verdict, /^REHEARSAL: /);
    assert.equal((verdict.match(/REHEARSAL:/g) ?? []).length, 1);
  }
  assert.equal(targetFor(LARGE_REMOTE_FREE), 'c3');
  assert.equal(targetFor(TRIM_CELLS[0]), 'trim');
  for (const producer of Object.values(PRODUCERS)) {
    assert.ok(producer.symbol && producer.freeCount > 0);
  }
  console.log('C3 runner selftest PASS: synthetic parser boundary exact-symbol proof, distinct-symbol rejection; inventory 37');
}
function selftest() {
  assert.equal(INVENTORY.length, 37);
  assert.equal(new Set(INVENTORY).size, INVENTORY.length);
  assert.deepEqual(ARMS, ['A1', 'A2', 'B1', 'B2', 'C0']);
  assert.equal(targetFor(DISTINCT_WORD_PRODUCER), 'c3');
  assert.equal(targetFor(TRIM_CELLS[0]), 'trim');
  const status = validateSources();
  assert.ok(status.some(x => x.status === 'ok' && x.name === DISTINCT_WORD_PRODUCER));
  assert.ok(status.filter(x => x.file.endsWith('r20_prf01_trim_iai.rs')).every(x => ['ok', 'external-prerequisite-missing', 'source-function-missing'].includes(x.status)));
  assert.ok(FROZEN_FILES.includes('benches/r20_prf01_trim_iai.rs'));
  assert.equal(PRODUCERS[SAME_WORD_PRODUCER].blockBytes, 16);
  assert.equal(PRODUCERS[DISTINCT_WORD_PRODUCER].blockBytes, 1024);
  assert.equal(PRODUCERS[LARGE_REMOTE_FREE].blockBytes, null);
  assert.deepEqual(SUMMARY_ONLY_COUNTERS, ['summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw']);
  assert.equal(summaryCounterAllowed(0, 'A'), true);
  assert.equal(summaryCounterAllowed(1, 'A'), false);
  assert.equal(summaryCounterAllowed(0, 'B'), true);
  assert.equal(summaryCounterAllowed(null, 'B'), false);
  console.log(`support selftest PASS (${INVENTORY.length} cells; ${status.filter(x => x.status === 'external-prerequisite-missing').length} external prerequisites absent)`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url) && process.argv.includes('--selftest')) selftest();
