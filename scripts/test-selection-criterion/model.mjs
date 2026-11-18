import assert from 'node:assert/strict';
import { assertSelectionIntegrity } from './selection-integrity.mjs';

export const CORE_ROWS = Object.freeze(['test ()', 'test (--features experimental)', 'test (--features alloc-core)']);
export const WINDOW_START = '2026-11-17T13:04:56Z';
export const WINDOW_SHA = 'a28be5dace7d9517f3767fa37ee15a99a06d0dde';
export const MIN_RUNS = 30;

const cutoff = Date.parse(WINDOW_START);
const text = value => typeof value === 'string' && value.trim().length > 0;
const sourcePath = value => typeof value === 'string' && /^(?:[\w.-]+\/)*[\w.-]+\.rs$/.test(value) &&
  !value.split('/').some(part => part === '.' || part === '..');
const completed = conclusion => conclusion === 'success' || conclusion === 'failure';
const knownAttempt = attempt => Number.isSafeInteger(attempt) && attempt > 0;
const scopedEvidence = run => knownAttempt(run.attempt) && (Object.hasOwn(run, 'evidenceAttempt')
  ? run.evidenceAttempt === run.attempt : run.attempt === 1);
const COLLECTION_UNSTABLE = 'run metadata changed or could not be verified during collection';
const isUnstable = run => run.collectionCheck === 'unstable';
const excludedEventOrBranch = run => !['push', 'pull_request'].includes(run.event) ||
  (run.event === 'push' && text(run.headBranch) && run.headBranch !== 'main');
const isRed = run => run.status === 'completed' && run.conclusion === 'failure';

function selectionIssue(selection) {
  if (selection == null) return 'missing selection artifact';
  const { mode, reason, total, tests } = selection;
  if (!['all', 'subset'].includes(mode) || !text(reason) || !Number.isSafeInteger(total) || total < 0 ||
      (tests !== undefined && (!Array.isArray(tests) || !tests.every(sourcePath) ||
        new Set(tests).size !== tests.length || tests.length > total)) ||
      (mode === 'subset' && !Array.isArray(tests))) return 'malformed selection artifact';
  if (total === 0) return 'selector reported zero total targets; selection state is not reliable';
  return null;
}

function classifyFailure(kind, path, selection) {
  if (!selection) return [null, 'selection unavailable or out of scope'];
  if (kind === 'integration' && !path) return [null, 'missing observed integration target path'];
  if (selection.mode === 'all') return [false, 'selector chose all tests'];
  if (kind === 'lib') return [false, 'subset always runs library tests'];
  if (kind !== 'integration') return [true, 'target kind omitted by subset'];
  return selection.tests.includes(path) ? [false, 'observed path is selected'] : [true, 'observed path is not selected'];
}

function coreRulesOutTestFailure(core, rows) {
  if (!core) return false;
  const exactRows = rows.length === CORE_ROWS.length &&
    CORE_ROWS.every(name => rows.filter(row => row?.name === name).length === 1);
  const skipped = row => row.conclusion === 'skipped' && row.evidence === undefined;
  if (core.conclusion === 'skipped') {
    return (core.rows == null || Array.isArray(core.rows)) &&
      (rows.length === 0 || (exactRows && rows.every(row => skipped(row) ||
        (row.conclusion == null && row.evidence === undefined))));
  }
  if (!exactRows) return false;
  if (core.conclusion === 'cancelled') return rows.every(skipped);
  return completed(core.conclusion) && rows.every(row => skipped(row) ||
    (row.conclusion === 'success' && row.evidence === undefined) ||
    (core.conclusion === 'failure' && row.conclusion === 'failure' && row.evidence?.status === 'pre-test' &&
      (row.evidence.failures === undefined || (Array.isArray(row.evidence.failures) && !row.evidence.failures.length))));
}

function evaluateRun(run) {
  if (run.selection != null) assertSelectionIntegrity(run.selection);
  const outsideScope = excludedEventOrBranch(run);
  if (isUnstable(run)) {
    return { run, category: outsideScope ? 'excluded' : 'uninformative',
      reason: outsideScope ? 'excluded event or branch' : COLLECTION_UNSTABLE,
      failures: [], unresolvedRed: !outsideScope && isRed(run) };
  }
  const scoped = scopedEvidence(run);
  const selection = scoped ? run.selection : null;
  const core = scoped ? run.core : null;
  const invalidSelection = selectionIssue(selection);
  const exclusion = outsideScope ? 'excluded event or branch' : !invalidSelection && selection.mode === 'all' &&
      selection.reason === 'full run requested' ? 'full run requested' : null;
  const usableSelection = !exclusion && !invalidSelection && core?.shadow === true ? selection : null;
  const issues = new Set();
  const failures = new Map();
  const coreRows = Array.isArray(core?.rows) ? core.rows : [];
  if (!text(run.event) || (run.event === 'push' && !text(run.headBranch))) issues.add('missing event or branch');
  if (run.status !== 'completed') issues.add('run not completed');
  if (!completed(run.conclusion)) issues.add(`run conclusion: ${run.conclusion ?? 'unknown'}`);
  if (!scoped) issues.add('selection and core evidence not scoped to this attempt');
  if (invalidSelection) issues.add(invalidSelection);
  if (!core) issues.add('core job missing');
  else {
    if (core.shadow !== true) issues.add('core shadow setup not proven skipped');
    if (!completed(core.conclusion)) issues.add(`core job conclusion: ${core.conclusion ?? 'unknown'}`);
    if (coreRows.length !== CORE_ROWS.length) issues.add('unexpected number of core test steps');
    const hasFailedStep = coreRows.some(row => row?.conclusion === 'failure');
    if (core.conclusion === 'failure' && !hasFailedStep) issues.add('core job failed outside the test steps');
    if (hasFailedStep && (core.conclusion === 'success' || run.conclusion === 'success')) {
      issues.add('test step failure contradicts job or run success');
    }
    if (core.conclusion === 'failure' && run.conclusion === 'success') issues.add('core job failure contradicts run success');
  }

  for (const name of CORE_ROWS) {
    const matches = coreRows.filter(row => row?.name === name);
    if (core && matches.length !== 1) issues.add(`${name}: missing or duplicate step`);
    for (const row of matches) {
      const evidence = row.evidence;
      if (row.conclusion === 'failure') {
        if (!evidence) issues.add(`${name}: missing Cargo log evidence`);
        else if (evidence.status !== 'test-failures') {
          issues.add(`${name}: ${evidence.status === 'pre-test' ? 'pre-test failure' : 'unresolved Cargo failure'}` +
            (text(evidence.reason) ? ` (${evidence.reason})` : ''));
        } else if (!Array.isArray(evidence.failures) || !evidence.failures.length) {
          issues.add(`${name}: malformed Cargo failure evidence`);
        }
      } else if (row.conclusion !== 'success') issues.add(`${name}: step conclusion ${row.conclusion ?? 'missing'}`);
      else if (evidence !== undefined) issues.add(`${name}: unexpected Cargo failure evidence on successful step`);

      const seen = new Set();
      for (const target of Array.isArray(evidence?.failures) ? evidence.failures : []) {
        if (!['integration', 'lib', 'doctest', 'other'].includes(target?.kind) || !text(target.name)) {
          issues.add(`${name}: malformed failed target`);
          continue;
        }
        const { kind, name: targetName } = target;
        const path = sourcePath(target.path) ? target.path : undefined;
        if (kind === 'integration' && !path) issues.add(`${name}: missing or invalid observed integration target path`);
        if (target.path !== undefined && !path) issues.add(`${name}: invalid failed target path`);
        const key = JSON.stringify([kind, targetName, kind === 'integration' ? path : null]);
        if (seen.has(key)) issues.add(`${name}: duplicate failed target`);
        seen.add(key);
        let failure = failures.get(key);
        if (!failure) {
          failure = { kind, name: targetName, ...(path ? { path } : {}), rows: [], miss: null, reason: 'failed test step not established' };
          failures.set(key, failure);
        }
        if (!failure.rows.includes(name)) failure.rows.push(name);
        if (row.conclusion === 'failure' && ['test-failures', 'unresolved'].includes(evidence.status)) {
          [failure.miss, failure.reason] = classifyFailure(kind, path, usableSelection);
        }
      }
    }
  }

  const category = exclusion ? 'excluded' : issues.size ? 'uninformative' : 'informative';
  const reason = exclusion ?? (issues.size ? [...issues].join('; ') : run.conclusion === 'failure'
    ? 'red run with complete core test evidence' : 'complete core test evidence');
  const unresolvedRed = category === 'uninformative' && isRed(run) &&
    (!scoped || (run.attempt > 1 && invalidSelection !== null) || !coreRulesOutTestFailure(core, coreRows));
  return { run, category, reason, failures: [...failures.values()], unresolvedRed };
}

function evaluateHistory(run) {
  const row = evaluateRun(run);
  const unresolvedAttempts = [];
  const block = (attempt, reason) => unresolvedAttempts.push({ attempt: knownAttempt(attempt) ? attempt : null, reason });
  if (row.category !== 'excluded') {
    if (isUnstable(run)) block(run.attempt, COLLECTION_UNSTABLE);
    else if (!knownAttempt(run.attempt)) block(run.attempt, 'latest attempt unknown');
    else if (!scopedEvidence(run)) block(run.attempt, 'latest evidence not scoped to this attempt');
    else if (run.attempt > 1 && (selectionIssue(run.selection) || !run.core)) {
      block(run.attempt, 'latest rerun missing usable attempt-specific selection or core evidence');
    }
  }
  if (run.historyIncomplete === true) block(null, 'attempt history truncated');
  if (run.priorAttempts !== undefined && !Array.isArray(run.priorAttempts)) block(null, 'malformed attempt history');

  const seen = new Set();
  const covered = new Set();
  const historicalFailures = new Map();
  for (const prior of Array.isArray(run.priorAttempts) ? run.priorAttempts : []) {
    // Check receipts even when this snapshot cannot be used as evidence.
    if (prior?.selection != null) assertSelectionIntegrity(prior.selection);
    const attempt = prior?.attempt;
    if (!knownAttempt(attempt) || String(prior.id) !== String(run.id) || prior.status !== 'completed' ||
        (knownAttempt(run.attempt) && attempt > run.attempt) || Object.hasOwn(prior, 'priorAttempts')) {
      block(attempt, 'invalid archived attempt snapshot');
      if (String(prior?.id) === String(run.id) && isRed(prior)) row.unresolvedRed = true;
      continue;
    }
    if (seen.has(attempt)) block(attempt, 'duplicate archived attempt snapshots');
    seen.add(attempt);
    if (prior.historyIncomplete === true) block(attempt, 'archived attempt history truncated');
    if (['headSha', 'event', 'headBranch'].some(key => prior[key] !== run[key])) {
      block(attempt, 'archived attempt head SHA, event, or branch does not match latest run');
      if (isRed(prior)) row.unresolvedRed = true;
      continue;
    }

    if (isUnstable(prior) && !excludedEventOrBranch(prior)) {
      // A verified recollection supersedes transient non-red instability on the same attempt.
      if (attempt === run.attempt && run.collectionCheck === 'stable' && !isRed(prior)) continue;
      if (attempt !== run.attempt || !isUnstable(run)) block(attempt, COLLECTION_UNSTABLE);
      if (isRed(prior)) row.unresolvedRed = true;
      continue;
    }
    if (attempt === run.attempt) {
      const metadataChanged = ['createdAt', 'status', 'conclusion']
        .some(key => prior[key] !== run[key]);
      // A same-attempt archive never replaces the latest metadata or counts as another observation.
      if (!metadataChanged && !isRed(prior)) continue;
      if (metadataChanged && (!isRed(prior) || isRed(run))) block(attempt, 'uncertain same-attempt metadata change');
    }
    const historical = evaluateRun(prior);
    if (historical.unresolvedRed) row.unresolvedRed = true;
    if (!scopedEvidence(prior) || (historical.category === 'uninformative' &&
        (!isRed(prior) || historical.unresolvedRed))) {
      block(attempt, `unresolved archived attempt: ${historical.reason}`);
    } else if (attempt < run.attempt) covered.add(attempt);
    for (const failure of historical.failures) {
      const key = JSON.stringify([attempt, failure.kind, failure.name, failure.path]);
      if (!historicalFailures.has(key) || failure.miss === true) {
        historicalFailures.set(key, { ...failure, attempt, source: 'archived',
          ...(scopedEvidence(prior) ? { selection: prior.selection } : {}) });
      }
    }
  }
  if (row.reason !== 'excluded event or branch' && knownAttempt(run.attempt) && covered.size < run.attempt - 1) {
    block(null, 'missing or unresolved earlier attempt history');
  }
  // Count each target at most once per attempt, including conflicting same-attempt snapshots.
  const latestMisses = new Set(row.failures.filter(failure => failure.miss === true)
    .map(failure => JSON.stringify([run.attempt, failure.kind, failure.name, failure.path])));
  for (const [key, failure] of historicalFailures) {
    if (!latestMisses.has(key)) row.failures.push(failure);
  }
  return { ...row, unresolvedAttempts };
}

export function evaluateWindow(runs, { listingComplete = true } = {}) {
  const ids = new Set();
  const rows = runs.map(run => {
    assert(run && /^(?:[0-9]+)$/.test(String(run.id)), 'missing or invalid run ID');
    const id = String(run.id);
    assert(!ids.has(id), `duplicate run ID: ${id}`);
    ids.add(id);
    assert(typeof run.createdAt === 'string' && Date.parse(run.createdAt) >= cutoff, `invalid or pre-window run: ${id}`);
    return evaluateHistory(run);
  });
  const counts = { total: runs.length, informative: 0, uninformative: 0, excluded: 0, all: 0, subset: 0, misses: 0,
    unresolvedRed: 0, unresolvedAttempts: 0 };
  const misses = [];
  for (const { run, category, failures, unresolvedRed, unresolvedAttempts } of rows) {
    counts[category]++;
    if (unresolvedRed) counts.unresolvedRed++;
    if (unresolvedAttempts.length) counts.unresolvedAttempts++;
    if (category === 'informative') counts[run.selection.mode]++;
    for (const failure of failures) if (failure.miss === true) misses.push({ run, ...failure });
  }
  counts.misses = misses.length;
  assert.equal(counts.total, counts.informative + counts.uninformative + counts.excluded);
  assert.equal(counts.informative, counts.all + counts.subset);
  const subsetShare = counts.informative ? counts.subset / counts.informative : 0;
  assert(subsetShare >= 0 && subsetShare <= 1);
  assert.equal(Math.round(subsetShare * counts.informative), counts.subset);
  const shortfall = Math.max(0, MIN_RUNS - counts.informative);
  const verdict = shortfall || listingComplete !== true ? 'INSUFFICIENT_WINDOW'
    : counts.misses ? 'FAIL' : counts.unresolvedRed || counts.unresolvedAttempts ? 'INSUFFICIENT_WINDOW'
      : counts.subset ? 'PASS' : 'FAIL';
  return { verdict, rows, counts, misses, subsetShare, shortfall };
}
