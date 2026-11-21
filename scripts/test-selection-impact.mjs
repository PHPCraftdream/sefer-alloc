#!/usr/bin/env node
// TEST_SELECTION=enforce impact measurement: reads the compact CI inputs in
// docs/perf/test_selection_inputs/ (the exact `test-selection` artifacts of past
// ci.yml runs, runs.json provenance, and jobs/<id>.json step dumps of the
// `test (core tiers)` job) and renders docs/perf/TEST_SELECTION_ENFORCE_IMPACT.md
// — what the repository variable TEST_SELECTION=enforce (selective test running
// in the `test-core` CI job) WOULD have changed over that window of past runs.
// Numbers only: the report makes NO GO/NO-GO decision.
//
// Cargo-free, network-free, deterministic (no clock, no randomness, no machine
// paths): two consecutive --report generations are byte-identical. Reuses
// scripts/ci-job-durations.mjs (durationSeconds, cargoTestStepSeconds) — never
// re-implements them.
//
// Modes:
//   --report  print the markdown report to stdout AND write it to
//             docs/perf/TEST_SELECTION_ENFORCE_IMPACT.md (override: --report-path).
//   --check   compare the committed report with a fresh generation; nonzero
//             exit on any difference (the error line says to run --report).
//
// All file IO is redirectable for tests: --repo-root, --inputs, --report-path.
// Production defaults point into the repo. The named exports (loadInputs,
// classifyReason, computePerRun, computeAggregates, renderReport) are pure:
// importing this module has zero side effects and writes nothing.

import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { cargoTestStepSeconds, durationSeconds } from './ci-job-durations.mjs';
import { isAlwaysAll, isDataFile, isToolOnly } from './select-tests.mjs';
import { loadCrateGraph, crateForPath, affectedCrates, rootAffected } from './ci/crate-graph.mjs';
import { plan as ciPlan, parseJobs } from './ci-plan.mjs';

const SCRIPT = 'scripts/test-selection-impact.mjs';
const DEFAULT_REPORT = 'docs/perf/TEST_SELECTION_ENFORCE_IMPACT.md';
const DEFAULT_INPUTS = 'docs/perf/test_selection_inputs';
const RANGES_FILE = 'ranges.json';
const CORE_JOB_NAME = 'test (core tiers)';
const CARGO_TEST_STEP = /^test \(/;
const GH_CALL_BUDGET = 45;
const REQUESTED_RUN_WINDOW = 40;
/** Workflow fallback / no-data reasons (ci.yml select-tests fallback() + select-tests.mjs select()). */
const SELECTOR_NO_DATA = /^(no successful test-impact-map run on main|map download failed|selector failed|no test-impact map available|changed files unknown)$/;
/** [class, reason suffix]; the reason STARTS with the changed file path. */
const REASON_SUFFIXES = [
  ['manifest-or-config', ' changes every build'],
  ['unmapped-changed-file', ' is executed by no mapped test'],
  ['unattributable', ' is not attributable to a test'],
];
export const CLASSES = ['full-run', 'selector-no-data', 'manifest-or-config', 'unmapped-changed-file', 'unattributable', 'unknown', 'subset'];

const NODE_TEST_FILE = /^tests\/[^/]+\.test\.mjs$/;
const PERIPHERAL_RUST_FILE = /^(?:benches|examples)\/.*\.rs$|^crates\/[^/]+\/[^/]+\/(?:benches|examples)\/.*\.rs$/;

/** A saved first-blocker path is only a candidate, never proof of a run subset. */
export function classifyCounterfactual(sel, sources = {}) {
  if (sel.mode !== 'all') return 'all';
  const m = /^(\S+) is (?:executed by no mapped test|not attributable to a test)$/.exec(sel.reason ?? '');
  if (!m) return 'all';
  const file = m[1];
  if (!NODE_TEST_FILE.test(file) && !PERIPHERAL_RUST_FILE.test(file)) return 'all';
  const texts = Object.values(sources).flat().filter((text) => typeof text === 'string');
  const referenced = texts.some((text) => text.includes(file) || text.includes('../' + file));
  return referenced ? 'all' : 'candidate';
}

const SHA = /^[0-9a-f]{40}$/;

/** Validate the saved graph-only ranges; a missing per-run record is unresolved. */
export function validateRanges(doc, runs) {
  ok(doc && doc.schema === 1 && Array.isArray(doc.runs), 'ranges.json schema 1 and runs[]');
  ok(typeof doc.note === 'string' && doc.note.includes('not exact event-base proofs'), 'ranges.json documents event-base limitation');
  const byId = new Map(runs.map((run) => [String(run.id), run]));
  assertEq(byId.size, runs.length, 'runs.json ids are unique');
  assertEq(doc.runs.length, runs.length, 'ranges.json ids match runs.json runs[] exactly (id count)');
  const ranges = new Map();
  for (const range of doc.runs) {
    ok(range && typeof range === 'object' && !Array.isArray(range), 'range entry is an object');
    const id = String(range.id);
    ok(byId.has(id), 'range ' + id + ' matches a recorded run');
    ok(!ranges.has(id), 'range ids are unique (' + id + ')');
    ok(SHA.test(range.head) && range.head === byId.get(id).head_sha, 'range ' + id + ' head is a full SHA matching runs.json');
    ok(range.status === 'graph-reconstructed' || range.status === 'unresolved', 'range ' + id + ' has known status');
    ok(Array.isArray(range.files), 'range ' + id + ' files is an array');
    const files = range.files;
    ok(files.every((f) => typeof f === 'string' && f.length > 0 && !f.startsWith('/') && !f.includes('\\')
      && !f.split('/').includes('..') && !/[\r\n]/.test(f) && !/^[A-Za-z]:/.test(f)),
    'range ' + id + ' files are repository-relative paths');
    assertEq(new Set(files).size, files.length, 'range ' + id + ' files are unique');
    assertEq([...files].sort().join('\n'), files.join('\n'), 'range ' + id + ' files are sorted');
    if (range.status === 'graph-reconstructed') {
      ok(SHA.test(range.base), 'range ' + id + ' base is a full SHA');
      ok(typeof range.method === 'string' && range.evidence && typeof range.evidence === 'object', 'range ' + id + ' graph evidence is complete');
      if (range.method === 'push-merge-parent1') {
        ok(byId.get(id).event === 'push' && byId.get(id).head_branch === 'main', 'range ' + id + ' push method matches run metadata');
        ok(Array.isArray(range.evidence.headParents) && range.evidence.headParents.length === 2
          && range.evidence.headParents.every((parent) => SHA.test(parent)), 'range ' + id + ' push parent evidence has two full SHAs');
        assertEq(range.evidence.headParents[0], range.base, 'range ' + id + ' push base is parent 1');
      } else if (range.method === 'pr-merge-base') {
        ok(byId.get(id).event === 'pull_request', 'range ' + id + ' PR method matches run metadata');
        ok(SHA.test(range.evidence.merge) && Array.isArray(range.evidence.mergeParents)
          && range.evidence.mergeParents.length === 2 && range.evidence.mergeParents.every((parent) => SHA.test(parent))
          && SHA.test(range.evidence.mergeBase), 'range ' + id + ' merge evidence is complete with full parent SHAs');
        assertEq(range.evidence.mergeParents[1], range.head, 'range ' + id + ' merge second parent is head');
        assertEq(range.base, range.evidence.mergeBase, 'range ' + id + ' base matches recorded merge-base');
      } else ok(false, 'range ' + id + ' method is recognized');
    } else {
      assertEq(range.base, null, 'range ' + id + ' unresolved base is null');
      assertEq(range.method, null, 'range ' + id + ' unresolved method is null');
      assertEq(range.evidence, null, 'range ' + id + ' unresolved evidence is null');
      assertEq(files.length, 0, 'range ' + id + ' unresolved files are empty');
    }
    ranges.set(id, range);
  }
  assertEq([...ranges.keys()].sort().join(','), [...byId.keys()].sort().join(','), 'ranges.json ids match runs.json runs[] exactly');
  return ranges;
}

/** Classification of saved conditional-range evidence, not proof of selectors. */
export function classifyConditionalRange(range, selection) {
  if (selection.reason === 'full run requested') return 'unchanged-full';
  if (!range || range.status !== 'graph-reconstructed') return 'unresolved';
  if (range.files.some(isAlwaysAll)) return 'unchanged-global';
  if (range.files.every(isDataFile)) return 'conditional-data';
  return 'map-dependent';
}

// ── P1.5b crate-graph shadow ────────────────────────────────────────────────────
/**
 * Ordered select-tests path-rule shadow for one saved file range. Reuses the real
 * exported rule predicates (isAlwaysAll, isDataFile, crateForPath, affectedCrates,
 * rootAffected) in select()'s decision order, but without maps/targets/sources:
 * map-covered and data-file changes can only be reported as map-dependent, never
 * claimed as subsets. Tool-only paths are skipped, as in select-tests.
 */
export function crateSelectShadow(files, graph) {
  const crateOnly = [];
  for (const f of files) {
    if (isAlwaysAll(f)) return { kind: 'all-manifest' };
    if (isToolOnly(f)) continue;
    if (isDataFile(f)) return { kind: 'map-dependent' };
    const c = crateForPath(f, graph);
    if (c && c !== graph.root) { crateOnly.push(c); continue; }
    return { kind: 'all-unmapped' };
  }
  if (crateOnly.length === 0) return { kind: 'empty' };
  const changed = [...new Set(crateOnly)].sort();
  const affected = affectedCrates(changed, graph);
  if (rootAffected(affected, graph)) return { kind: 'all-root-affected', changed, affected };
  return { kind: 'crate-only-subset', changed, affected };
}

/**
 * Per-run crate-graph shadow over the committed ranges: select-tests outcome plus
 * ci-plan groups before (crateGraph=null) vs after (real graph), with job counts
 * from the current workflow text. Historical map contents and the historical
 * ci.yml are unavailable; job counts apply the CURRENT gating to historical diffs.
 */
export function computeCrateGraphShadow(inputs, rows, graph, ciText) {
  const jobsByGroup = new Map();
  for (const j of parseJobs(ciText)) jobsByGroup.set(j.group, (jobsByGroup.get(j.group) ?? 0) + 1);
  const perRun = [];
  for (const r of rows) {
    const range = (inputs.ranges ?? new Map()).get(r.id);
    if (!range || range.status !== 'graph-reconstructed') {
      perRun.push({ row: r, status: range?.status ?? 'missing' });
      continue;
    }
    const sel = crateSelectShadow(range.files, graph);
    const trailerFull = r.mode === 'all' && r.reason === 'full run requested';
    const before = ciPlan({ files: range.files, event: r.event, trailerFull, crateGraph: null });
    const after = ciPlan({ files: range.files, event: r.event, trailerFull, crateGraph: graph });
    const on = (p) => Object.keys(p).filter((k) => p[k] && k !== 'full').sort();
    const beforeGroups = on(before);
    const afterGroups = on(after);
    const skipped = beforeGroups.filter((g) => !after[g]);
    const jobCount = (gs) => gs.reduce((n, g) => n + (jobsByGroup.get(g) ?? 0), 0);
    perRun.push({
      row: r, status: 'graph-reconstructed', sel, beforeGroups, afterGroups, skipped,
      files: range.files.length,
      jobsBefore: jobCount(beforeGroups), jobsAfter: jobCount(afterGroups),
    });
  }
  const scoped = perRun.filter((x) => x.status === 'graph-reconstructed');
  const unresolved = perRun.length - scoped.length;
  const crateOnly = scoped.filter((x) => x.sel.kind === 'crate-only-subset');
  const denominator = rows.length;
  const numerator = crateOnly.length;
  const skippedGroupTotal = scoped.reduce((n, x) => n + x.skipped.length, 0);
  const unchangedCount = scoped.filter((x) => x.skipped.length === 0).length;
  // Hypothetical selected=0 ceiling for crate-only runs (same proportionality rule
  // as the P1.5a sections): the whole root-crate test-core cargo-test time.
  const testSecondsSkipped = crateOnly.reduce((n, x) => n + x.row.testSeconds, 0);
  assertEq(scoped.length + unresolved, rows.length, 'crate-graph shadow: scoped + unresolved == run count');
  ok(numerator <= scoped.length, 'crate-graph numerator <= graph-reconstructed ranges');
  assertEq(unchangedCount, scoped.length - scoped.filter((x) => x.skipped.length > 0).length,
    'crate-graph shadow: unchanged count recomputed');
  return { perRun, scoped, unresolved, denominator, numerator, skippedGroupTotal, unchangedCount, testSecondsSkipped, jobsByGroup };
}


// ── formatting (seconds 1 decimal, percentages 2 decimals, sizes KiB 1 decimal) ─
const s1 = (x) => x.toFixed(1);
const p2 = (x) => x.toFixed(2);
const kib = (bytes) => (bytes / 1024).toFixed(1);
const bt = (s) => '`' + s + '`';
const share = (n, d) => n + ' / ' + d + ' = ' + (d === 0 ? 'not estimable' : p2((n / d) * 100) + '%');

function fail(msg) { throw new Error(msg); }
function ok(cond, what) { if (!cond) fail('assertion failed: ' + what); }
function assertEq(got, want, what) {
  if (typeof got === 'number' && typeof want === 'number') {
    if (!(Math.abs(got - want) <= 1e-9)) fail('assertion failed: ' + what + ': got ' + got + ', want ' + want);
  } else if (got !== want) fail('assertion failed: ' + what + ': got ' + JSON.stringify(got) + ', want ' + JSON.stringify(want));
}

// ── inputs ──────────────────────────────────────────────────────────────────────
/**
 * Load and cross-check the compact inputs directory:
 *   selections.json   { <runid>: {mode, reason, total, tests?, why?, mapShas?} }
 *   runs.json         { provenance, runs: [{id, event, conclusion, created_at, head_sha, head_branch}] }
 *   jobs/<id>.json    { run_id, jobs: [<the single "test (core tiers)" job>] }
 * Hard-fails on any structural mismatch (key sets, job count, step count,
 * missing timestamps, provenance accounting).
 * Optional ranges.json is saved graph evidence only; --report/--check never invokes Git.
 */
export function loadInputs(inputsDir) {
  const selections = JSON.parse(readFileSync(join(inputsDir, 'selections.json'), 'utf8'));
  const runsDoc = JSON.parse(readFileSync(join(inputsDir, 'runs.json'), 'utf8'));
  ok(selections && typeof selections === 'object' && !Array.isArray(selections), 'selections.json is an object keyed by run id');
  ok(Array.isArray(runsDoc?.runs) && runsDoc.runs.length > 0, 'runs.json has a non-empty runs[]');
  const runs = runsDoc.runs;
  const runIds = runs.map((r) => String(r.id));
  assertEq(new Set(runIds).size, runIds.length, 'runs.json ids are unique');
  const selIds = Object.keys(selections);
  assertEq([...runIds].sort().join(','), [...selIds].sort().join(','), 'selections.json keys match runs.json runs[] exactly');

  const jobsDir = join(inputsDir, 'jobs');
  const jobFiles = readdirSync(jobsDir).filter((f) => f.endsWith('.json')).sort();
  assertEq(jobFiles.length, runIds.length, 'exactly one jobs/<id>.json per run');
  const jobs = new Map();
  const pattern = '/' + CARGO_TEST_STEP.source + '/';
  for (const id of runIds) {
    const doc = JSON.parse(readFileSync(join(jobsDir, id + '.json'), 'utf8'));
    assertEq(String(doc?.run_id), id, 'jobs/' + id + '.json run_id matches its filename');
    const core = (doc?.jobs ?? []).filter((j) => j?.name === CORE_JOB_NAME);
    assertEq(core.length, 1, 'jobs/' + id + '.json: exactly one "' + CORE_JOB_NAME + '" job');
    const job = core[0];
    const steps = (job.steps ?? []).filter((st) => CARGO_TEST_STEP.test(st?.name ?? ''));
    ok(steps.length >= 1, 'jobs/' + id + '.json: at least one step matching ' + pattern);
    assertEq(steps.length, 3, 'jobs/' + id + '.json: exactly 3 cargo test steps (the workflow defines three)');
    for (const st of steps) {
      ok(durationSeconds(st.started_at, st.completed_at) !== null,
        'jobs/' + id + '.json step "' + st.name + '": non-null started_at/completed_at');
    }
    const summed = cargoTestStepSeconds(job, CARGO_TEST_STEP);
    ok(summed !== null, 'jobs/' + id + '.json: cargoTestStepSeconds non-null');
    assertEq(summed.raws.length, steps.length, 'jobs/' + id + '.json: every cargo test step contributed to the sum');
    jobs.set(id, job);
  }

  const prov = runsDoc.provenance ?? {};
  const calls = prov.gh_calls ?? {};
  assertEq(calls.total, (calls.run_list ?? 0) + (calls.artifact_download_attempts ?? 0) + (calls.jobs_api ?? 0),
    'gh_calls.total == run_list + artifact_download_attempts + jobs_api');
  for (const key of ['run_list', 'artifact_download_attempts', 'artifact_downloads_succeeded', 'jobs_api', 'total']) {
    ok(Number.isInteger(calls[key]) && calls[key] >= 0, 'gh_calls.' + key + ' is a nonnegative integer');
  }
  const budgetExceeded = Math.max(0, calls.total - GH_CALL_BUDGET);
  const order = Array.isArray(prov.download_attempt_order) ? prov.download_attempt_order : [];
  assertEq(order.length, calls.artifact_download_attempts, 'download_attempt_order length == artifact_download_attempts');
  const downloaded = order.filter((a) => a.downloaded).map((a) => String(a.id));
  assertEq(downloaded.length, calls.artifact_downloads_succeeded, 'downloaded count == artifact_downloads_succeeded');
  assertEq(downloaded.length, runIds.length, 'every downloaded id is a run with an artifact');
  const attempted = new Set(order.map((a) => String(a.id)));
  const noArtifact = order.filter((a) => !a.downloaded).map((a) => String(a.id)).sort();
  for (const id of runIds) ok(attempted.has(id), 'run ' + id + ' appears in download_attempt_order');
  for (const id of noArtifact) ok(!runIds.includes(id), 'no-artifact run ' + id + ' is absent from runs[]');
  for (const a of order) ok(typeof a.conclusion === 'string' && a.conclusion.trim().length > 0,
    'download_attempt_order entry ' + a.id + ' has a non-empty string conclusion');

  const rangePath = join(inputsDir, RANGES_FILE);
  const hasRanges = existsSync(rangePath);
  const rangesDoc = hasRanges ? JSON.parse(readFileSync(rangePath, 'utf8')) : null;
  const ranges = hasRanges ? validateRanges(rangesDoc, runs) : new Map();
  const rangeBytes = hasRanges ? statSync(rangePath).size : 0;
  ok(rangeBytes <= 200 * 1024, 'ranges.json <= 200 KiB');
  const inputBytes = statSync(join(inputsDir, 'selections.json')).size
    + statSync(join(inputsDir, 'runs.json')).size
    + jobFiles.reduce((acc, f) => acc + statSync(join(jobsDir, f)).size, 0);
  ok(inputBytes <= 200 * 1024, 'loaded historical input files total <= 200 KiB');
  return {
    selections, runs, jobs, ranges, rangeBytes, rangeHistoryRef: rangesDoc?.historyRef ?? null,
    provenance: prov, ghCalls: calls, budgetExceeded, attemptOrder: order, noArtifact,
    inputBytes, inputFiles: 2 + jobFiles.length, jobFileCount: jobFiles.length,
  };
}

// ── classification ──────────────────────────────────────────────────────────────
/**
 * Classify one `test-selection` artifact. Returns {mode, cls, file, savedTargets}.
 * Every artifact is classified without discarding unfamiliar reasons. A total of 0
 * is valid only for a workflow no-data reason or an unknown/unrecorded reason.
 */
export function classifyReason(sel) {
  ok(sel && typeof sel === 'object', 'selection artifact is an object');
  ok(sel.mode === 'all' || sel.mode === 'subset', 'mode is "all"|"subset" (got ' + JSON.stringify(sel.mode) + ')');
  ok(Number.isInteger(sel.total) && sel.total >= 0, 'artifact total is a nonnegative integer');
  if (sel.mode === 'subset') {
    ok(sel.total > 0, 'subset: total > 0');
    ok(Array.isArray(sel.tests), 'subset: tests is an array');
    ok(sel.tests.every((t) => typeof t === 'string'), 'subset: tests contains only strings');
    assertEq(new Set(sel.tests).size, sel.tests.length, 'subset: tests are unique');
    ok(sel.tests.length <= sel.total, 'subset: tests.length <= total');
    ok(sel.why && typeof sel.why === 'object', 'subset: why present');
    for (const t of sel.tests) ok(typeof sel.why[t] === 'string', 'subset: why entry for ' + t);
    return { mode: 'subset', cls: 'subset', file: null, reason: null, savedTargets: sel.total - sel.tests.length };
  }
  const recordedReason = typeof sel.reason === 'string' && sel.reason.length > 0 ? sel.reason : null;
  const reason = recordedReason ?? 'unknown';
  let cls = null;
  let file = null;
  if (recordedReason === null) {
    cls = 'unknown';
  } else if (reason === 'full run requested') {
    cls = 'full-run';
  } else if (SELECTOR_NO_DATA.test(reason)) {
    cls = 'selector-no-data';
  } else {
    for (const [c, suffix] of REASON_SUFFIXES) {
      if (reason.endsWith(suffix)) { cls = c; file = reason.slice(0, reason.length - suffix.length); break; }
    }
    if (cls === null) cls = 'unknown';
    if (file !== null) ok(file.length > 0 && reason.startsWith(file), 'extracted file path is the reason prefix: ' + file);
  }
  if (sel.total === 0 && cls !== 'selector-no-data' && cls !== 'unknown') {
    fail('artifact with total=0 must classify selector-no-data or unknown, got ' + cls + ': ' + JSON.stringify(reason));
  }
  return { mode: 'all', cls, file, reason, savedTargets: 0 };
}

// ── per-run rows ────────────────────────────────────────────────────────────────
/**
 * One row per run (ascending created_at): classification + test-core cargo-test
 * step seconds + proportional saved-seconds counterfactual (subset runs only).
 */
export function computePerRun(inputs) {
  const ordered = [...inputs.runs].sort((a, b) =>
    a.created_at + String(a.id) < b.created_at + String(b.id) ? -1 : 1);
  const out = [];
  for (const run of ordered) {
    const id = String(run.id);
    const sel = inputs.selections[id];
    const clsInfo = classifyReason(sel);
    const job = inputs.jobs.get(id);
    const testSeconds = cargoTestStepSeconds(job, CARGO_TEST_STEP).seconds;
    ok(Number.isFinite(testSeconds) && testSeconds >= 0, 'run ' + id + ': test seconds finite and >= 0');
    // Enforce counterfactual under the stated proportionality assumption:
    // a subset run skips savedTargets of `total` targets -> saves that fraction
    // of the run's cargo-test step seconds.
    const savedSeconds = clsInfo.mode === 'subset' ? testSeconds * clsInfo.savedTargets / sel.total : 0;
    const savedAgain = clsInfo.mode === 'subset' ? (testSeconds / sel.total) * clsInfo.savedTargets : 0;
    assertEq(savedSeconds, savedAgain, 'run ' + id + ': savedSeconds recomputed');
    ok(Number.isFinite(savedSeconds) && savedSeconds >= 0 && savedSeconds <= testSeconds,
      'run ' + id + ': 0 <= savedSeconds <= testSeconds');
    out.push({
      id, event: run.event, conclusion: run.conclusion, createdAt: run.created_at, headSha: run.head_sha,
      mode: sel.mode, cls: clsInfo.cls, file: clsInfo.file, reason: clsInfo.reason, total: sel.total,
      savedTargets: clsInfo.savedTargets,
      selCount: sel.mode === 'subset' ? sel.tests.length : null,
      testSeconds, savedSeconds,
    });
  }
  return out;
}

// ── aggregates ──────────────────────────────────────────────────────────────────
/** Window aggregates; every derived number is asserted before it can be printed. */
export function computeAggregates(inputs, rows) {
  const runCount = rows.length;
  const subsetRows = rows.filter((r) => r.cls === 'subset');
  const reducedRows = subsetRows.filter((r) => r.savedTargets > 0);
  const allModeRows = rows.filter((r) => r.mode === 'all');
  assertEq(subsetRows.length + allModeRows.length, runCount, 'subset + all == run count');

  const perClass = new Map(CLASSES.map((c) => [c, { count: 0, seconds: 0 }]));
  for (const r of rows) {
    ok(perClass.has(r.cls), 'class "' + r.cls + '" is one of the known classes');
    perClass.get(r.cls).count += 1;
    perClass.get(r.cls).seconds += r.testSeconds;
  }
  assertEq([...perClass.values()].reduce((a, c) => a + c.count, 0), runCount, 'per-class counts sum == run count');

  // denominator: window test-core cargo-test seconds over ALL runs
  const denominator = rows.reduce((acc, r) => acc + r.testSeconds, 0);
  assertEq(denominator, rows.map((r) => r.testSeconds).reduce((a, b) => a + b, 0), 'denominator recomputed');
  assertEq(denominator, [...perClass.values()].reduce((a, c) => a + c.seconds, 0), 'denominator == sum of per-class seconds');
  ok(denominator >= 0, 'denominator >= 0');

  // numerator: saved seconds over SUBSET runs only
  const numerator = subsetRows.reduce((acc, r) => acc + r.savedSeconds, 0);
  assertEq(numerator, subsetRows.map((r) => (r.testSeconds * r.savedTargets) / r.total).reduce((a, b) => a + b, 0),
    'numerator recomputed');
  ok(numerator >= 0, 'numerator >= 0');
  ok(numerator <= denominator, 'numerator (' + numerator + ') <= denominator (' + denominator + ')');
  const savedShare = denominator > 0 ? numerator / denominator : 0;
  ok(savedShare >= 0 && savedShare <= 1, 'saved share in [0,1]');

  const reducedShare = runCount > 0 ? reducedRows.length / runCount : 0;
  assertEq(reducedShare, runCount > 0 ? reducedRows.length / runCount : 0, 'reduced-run share formula');
  ok(reducedShare >= 0 && reducedShare <= 1, 'reduced-run share in [0,1]');

  // map currency
  const mapShaSet = new Set();
  for (const id of selIds(inputs)) for (const s of inputs.selections[id].mapShas ?? []) mapShaSet.add(s);
  const mapShas = [...mapShaSet].sort();
  const mapCurrent = rows.filter((r) => (inputs.selections[r.id].mapShas ?? []).includes(r.headSha)).length;
  ok(mapCurrent >= 0 && mapCurrent <= runCount, 'map-current count in [0, runCount]');

  // events and conclusions
  const byEvent = new Map();
  const conclusions = new Set();
  for (const r of rows) {
    byEvent.set(r.event, (byEvent.get(r.event) ?? 0) + 1);
    conclusions.add(r.conclusion);
  }
  assertEq([...byEvent.values()].reduce((a, b) => a + b, 0), runCount, 'event counts sum == run count');
  assertEq([...conclusions].sort().join(','), 'success', 'every run in the window concluded success');

  // deterministic per-class example reasons (count desc, then text asc)
  const examples = new Map();
  for (const r of rows) {
    const key = r.file ?? (r.cls === 'full-run' ? 'full run requested' : null);
    if (key === null) continue;
    const m = examples.get(r.cls) ?? new Map();
    m.set(key, (m.get(key) ?? 0) + 1);
    examples.set(r.cls, m);
  }

  const first = rows[0].createdAt;
  const last = rows[rows.length - 1].createdAt;
  const spanMs = Date.parse(last) - Date.parse(first);
  ok(Number.isFinite(spanMs) && spanMs > 0, 'window span positive');

  return {
    runCount, subsetCount: subsetRows.length, reducedCount: reducedRows.length, allModeCount: allModeRows.length,
    perClass, denominator, numerator, savedShare, reducedShare,
    mapShas, mapCurrent, byEvent, conclusions: [...conclusions].sort(), examples, first, last,
    spanHours: spanMs / 3.6e6,
  };
}

function selIds(inputs) { return Object.keys(inputs.selections); }

/** Re-verify every printed number against independent recomputation (render-time gate). */
function assertReportNumbers(inputs, rows, agg) {
  assertEq(rows.length, agg.runCount, 'render: row count == aggregate run count');
  let clsSum = 0, secsSum = 0, savedSum = 0, subsetCount = 0;
  for (const c of CLASSES) clsSum += agg.perClass.get(c).count;
  for (const r of rows) {
    secsSum += r.testSeconds;
    savedSum += r.savedSeconds;
    if (r.cls === 'subset') subsetCount += 1;
  }
  assertEq(clsSum, rows.length, 'render: per-class counts sum == run count');
  assertEq(subsetCount, agg.subsetCount, 'render: subset count');
  assertEq(agg.reducedCount, rows.filter((r) => r.cls === 'subset' && r.savedTargets > 0).length, 'render: reduced run count');
  assertEq(subsetCount + (rows.length - subsetCount), rows.length, 'render: subset + all == run count');
  assertEq(secsSum, agg.denominator, 'render: Σ per-run testSeconds == denominator');
  assertEq(savedSum, agg.numerator, 'render: Σ per-run savedSeconds == numerator');
  // percentages recomputed from the underlying values, two ways
  const pctA = agg.denominator === 0 ? 0 : (agg.numerator / agg.denominator) * 100;
  const pctB = agg.denominator === 0 ? 0 : (agg.numerator * 100) / agg.denominator;
  assertEq(pctA, pctB, 'render: saved percentage formula');
  assertEq(pctA.toFixed(2), (agg.denominator === 0 ? 0 : ((agg.numerator * 100) / agg.denominator)).toFixed(2), 'render: printed saved percentage matches underlying values');
  const redA = (agg.reducedCount / agg.runCount) * 100;
  assertEq(redA.toFixed(2), ((agg.reducedCount * 100) / agg.runCount).toFixed(2), 'render: printed reduced-run percentage matches underlying values');
  for (const c of CLASSES) {
    const n = agg.perClass.get(c).count;
    assertEq(((n / rows.length) * 100).toFixed(2), ((n * 100) / rows.length).toFixed(2), 'render: printed share for class ' + c);
  }
  let mapRuns = 0;
  for (const r of rows) if ((inputs.selections[r.id].mapShas ?? []).includes(r.headSha)) mapRuns += 1;
  assertEq(mapRuns, agg.mapCurrent, 'render: map-current count recomputed');
}

// ── report ──────────────────────────────────────────────────────────────────────
function classExamples(agg, cls) {
  const m = agg.examples.get(cls);
  if (!m || m.size === 0) return '—';
  return [...m.entries()]
    .sort((a, b) => (b[1] - a[1]) || (a[0] < b[0] ? -1 : 1))
    .map(([k, n]) => bt(k) + ' ×' + n)
    .join('; ');
}

export function renderReport(inputs, rows, agg, crate = null) {
  assertReportNumbers(inputs, rows, agg);
  const calls = inputs.ghCalls;
  const firstBlockerCandidates = rows.filter((r) =>
    classifyCounterfactual(inputs.selections[r.id]) === 'candidate');
  const candidateByClass = new Map(CLASSES.map((c) => [c, { count: 0, seconds: 0 }]));
  for (const r of firstBlockerCandidates) {
    const c = candidateByClass.get(r.cls);
    c.count += 1;
    c.seconds += r.testSeconds * (r.total - 0) / r.total;
  }
  const candidateCount = firstBlockerCandidates.length;
  const candidateSeconds = firstBlockerCandidates.reduce((sum, r) => sum + r.testSeconds * (r.total - 0) / r.total, 0);
  const L = [];
  const ev = (name) => agg.byEvent.get(name) ?? 0;
  const events = [...agg.byEvent.keys()].sort().map((k) => bt(k) + ' ' + agg.byEvent.get(k)).join(', ');
  const clsRow = (c) => {
    const e = agg.perClass.get(c);
    return '| ' + bt(c) + ' | ' + e.count + ' | ' + share(e.count, agg.runCount)
      + ' | ' + (e.count > 0 ? s1(e.seconds) : '—') + ' | ' + classExamples(agg, c) + ' |';
  };
  const unmappedFiles = [...(agg.examples.get('unmapped-changed-file') ?? new Map()).entries()]
    .sort((a, b) => (b[1] - a[1]) || (a[0] < b[0] ? -1 : 1))
    .map(([k, n]) => bt(k) + ' ×' + n).join('; ');
  const unattrFiles = [...(agg.examples.get('unattributable') ?? new Map()).entries()]
    .sort((a, b) => (b[1] - a[1]) || (a[0] < b[0] ? -1 : 1))
    .map(([k, n]) => bt(k) + ' ×' + n).join('; ');

  const noArtifactConclusions = new Map();
  const attemptConclusions = new Map(inputs.attemptOrder.map((a) => [String(a.id), a.conclusion]));
  const noArtifactDetails = inputs.noArtifact.map((id) => {
    const conclusion = attemptConclusions.get(id);
    noArtifactConclusions.set(conclusion, (noArtifactConclusions.get(conclusion) ?? 0) + 1);
    return id + ' (' + conclusion + ')';
  });
  assertEq([...noArtifactConclusions.values()].reduce((sum, count) => sum + count, 0), inputs.noArtifact.length,
    'no-artifact conclusion counts sum == no-artifact count');
  const noArtifactConclusionSummary = [...noArtifactConclusions.entries()]
    .sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
    .map(([conclusion, count]) => conclusion + ' ' + count).join(', ');

  L.push('# TEST_SELECTION=enforce impact (measurement input; no verdict)');
  L.push('');
  L.push('Generated by ' + bt(SCRIPT + ' --report') + ' from the compact CI inputs in '
    + bt(DEFAULT_INPUTS) + ' (deterministic: no clock, no randomness, no machine paths — two consecutive'
    + ' generations are byte-identical). It measures what the repository variable ' + bt('TEST_SELECTION=enforce')
    + ' (selective test running in the ' + bt('test-core') + ' job of ' + bt('.github/workflows/ci.yml')
    + ') WOULD have changed over the window of past CI runs below. Numbers only: it makes NO GO/NO-GO'
    + ' decision; every number is derived from the inputs in-script and asserted before printing.');
  L.push('');
  L.push('## Data and provenance');
  L.push('');
  L.push('- Source: artifact ' + bt('test-selection') + ' of ' + bt('.github/workflows/ci.yml')
    + ' runs (events ' + bt('pull_request') + ' and ' + bt('push') + ' to ' + bt('main')
    + ', completed runs). Sampling favored successful completed candidates, newest first within that success class; '
    + 'failed/cancelled completed runs were not sampled. This is NOT an unbiased sample of the latest ordinary runs; '
    + 'see runs.json provenance.');
  const budgetState = inputs.budgetExceeded > 0
    ? 'EXCEEDED by ' + inputs.budgetExceeded + ' calls'
    : 'WITHIN budget';
  L.push('- gh call accounting (from runs.json, not retyped): ' + calls.run_list + ' run list + '
    + calls.artifact_download_attempts + ' artifact-download attempts + ' + calls.jobs_api + ' jobs-api calls = '
    + calls.total + ' total against a ' + GH_CALL_BUDGET + '-total-call budget: ' + budgetState + '; '
    + calls.artifact_downloads_succeeded + ' downloads succeeded.');
  L.push('- Storage: inputs are projected/truncated, not full raw API dumps. ' + bt('jobs/<id>.json')
    + ' retains only the ' + bt(CORE_JOB_NAME) + ' job and its steps; other jobs and unrelated API fields were omitted. '
    + 'The retained test-core values are verbatim. Reproduce jobs API data with ' + bt('gh api repos/PHPCraftdream/sefer-alloc/actions/runs/<id>/jobs?per_page=100')
    + '. Compact inputs: ' + kib(inputs.inputBytes) + ' KiB over ' + inputs.inputFiles + ' files (' + bt('selections.json') + ', '
    + bt('runs.json') + ', ' + inputs.jobFileCount + ' × ' + bt('jobs/<id>.json')
    + '), measured with fs.statSync at generation time (loaded-input limit: 200 KiB).');
  L.push('- Artifact retention: 14 days (' + bt('retention-days: 14')
    + ' on the upload-artifact step of the ' + bt('select-tests') + ' job).');
  L.push('- Window: first created_at ' + agg.first + ', last ' + agg.last + '.');
  L.push('');
  L.push('## Window and population');
  L.push('');
  L.push('- Runs with a ' + bt('test-selection') + ' artifact: ' + agg.runCount + ' ('
    + calls.artifact_downloads_succeeded + ' of ' + calls.artifact_download_attempts + ' download attempts).');
  L.push('- Protocol incomplete: artifact window contains ' + agg.runCount + ' runs, below the requested approximately '
    + REQUESTED_RUN_WINDOW + '; success-preferred sampling is not the latest ordinary-runs population'
    + (inputs.budgetExceeded > 0 ? '; total-call budget exceeded by ' + inputs.budgetExceeded : '')
    + '. No further gh collection.');
  L.push('- Attempted but no artifact retrieved (' + inputs.noArtifact.length + '): '
    + noArtifactDetails.join(', ') + '; recorded conclusions: ' + noArtifactConclusionSummary
    + '. For each, the download failed/no artifact was retrieved; error detail was not retained, so the inputs do not establish whether an artifact existed or why retrieval failed. '
    + 'The workflow gates ' + bt('select-tests') + ' on ' + bt("needs.changes.outputs.fast == 'true'")
    + ', so a docs-only change that selects only the ' + bt('always') + ' group could skip the job; that is only a possible workflow-based explanation, not confirmed for these run ids.');
  L.push('- All ' + agg.runCount + ' runs concluded ' + bt('success') + ' (conclusions from the data).');
  L.push('- Events: ' + events + '.');
  L.push('- Window span: ' + agg.spanHours.toFixed(2) + ' h (first to last created_at).');
  L.push('');
  L.push('## Result');
  L.push('');
  L.push('- Subset-mode runs: ' + agg.subsetCount + ' of ' + agg.runCount + ' — share '
    + share(agg.subsetCount, agg.runCount) + '. Strictly reduced runs (subset mode and savedTargets > 0): '
    + agg.reducedCount + ' of ' + agg.runCount + ' — reduced-run share '
    + share(agg.reducedCount, agg.runCount) + '.');
  L.push('- Estimated saved test-core cargo-test seconds: ' + s1(agg.numerator) + ' / ' + s1(agg.denominator)
    + ' s = ' + (agg.denominator === 0 ? 'not estimable (0.0 / 0.0 s)' : p2(agg.savedShare * 100) + '%')
    + ' (numerator: Σ saved seconds over subset runs; denominator: Σ ' + bt('test (core tiers)')
    + ' cargo-test step seconds over ALL ' + agg.runCount + ' runs).');
  if (agg.reducedCount === 0) {
    if (agg.subsetCount === 0 && agg.allModeCount === agg.runCount) {
      L.push('- In this window enforce would have changed nothing: all ' + agg.runCount
        + ' runs selected ' + bt('all') + ', so the enforce counterfactual equals the recorded window.');
    } else {
      L.push('- No run in this window has a strict target reduction; subset runs that selected every target save no targets.');
    }
  } else {
    L.push('- ' + agg.reducedCount + ' run(s) have a strict target reduction; per-run skipped-target fractions and approximate seconds are shown below.');
  }
  L.push('');
  L.push('## Selection classes and recorded reasons');
  L.push('');
  L.push('| class | runs | share of runs | window test-core test seconds | example reason(s) |');
  L.push('|---|---|---|---|---|');
  for (const c of CLASSES) L.push(clsRow(c));
  L.push('');
  L.push('- ' + bt('full-run') + ' (' + agg.perClass.get('full-run').count + ' runs): a full run is requested by a '
    + bt('CI: full') + ' trailer on any commit of the range, by the schedule, or by ' + bt('workflow_dispatch')
    + ' (docs/CI_TIERS.md, "When the full set runs"); ' + bt('select-tests') + ' then returns ' + bt('all')
    + ' with reason ' + bt('full run requested') + ' before consulting the map. The window contains only '
    + bt('pull_request') + ' (' + ev('pull_request') + ') and ' + bt('push') + ' (' + ev('push')
    + ') events, so these requests came from the ' + bt('CI: full')
    + ' trailer — INFERENCE from the recorded event kinds (schedule/dispatch runs would carry those events).');
  L.push('- ' + bt('unmapped-changed-file') + ' (' + agg.perClass.get('unmapped-changed-file').count
    + ' runs): ' + unmappedFiles + '. Artifact reasons report these named files were absent from the maps used; map contents are not saved here.');
  L.push('- ' + bt('unattributable') + ' (' + agg.perClass.get('unattributable').count + ' runs): '
    + unattrFiles + '. For these changes the selector found no attribution to a test and conservatively returned ' + bt('all')
    + '; a bench/example change could still be exercised by a test, but the saved inputs show no such attribution.');
  L.push('- ' + bt('selector-no-data') + ', ' + bt('manifest-or-config') + ', and ' + bt('unknown')
    + ' counts are shown in the class table. A raw unfamiliar or missing all-mode reason is retained/classified as unknown.');
  L.push('');
  L.push('## Savings model (stated assumptions)');
  L.push('');
  L.push('- Proportionality: a subset run is priced as testSeconds × skippedTargets / total, where skippedTargets = total − selected; this scales all matched test-step time, including compilation/library overhead, by integration-target fraction even though ' + bt('--lib') + ' is always retained.');
  L.push('- This is an approximate model, not measured wall-time savings. It excludes other jobs and setup/cache/queue time. Test targets differ in duration; a subset may also skip doctests and example/bench compilation, which the model does not separately price.');
  L.push('- The ±30% same-job noise figure is contextual input from the user, not measured in this window; the report uses window sums, not per-job medians.');
  L.push('');
  L.push('## Risks and coverage limits of enforce mode (read from code and docs, no run)');
  L.push('');
  L.push('- A subset run skips doctests and does not compile examples/benches; the ' + bt('clippy')
    + ' rows (' + bt('--all-targets') + ') still cover their compilation (docs/CI_TIERS.md:175-181).');
  L.push('- Targets unmapped in every row (including ' + bt('no-coverage') + ' and other unmapped statuses) are always selected (scripts/select-tests.mjs:54-55; docs/CI_TIERS.md:167-173); this rule does not claim what those tests execute.');
  L.push('- A changed Rust or ' + bt('tests/') + ' file absent from mapped sets selects ' + bt('all')
    + ' (scripts/select-tests.mjs:18-25, 67-77); this rule does not establish that maps can never cover particular files.');
  L.push('- The selector chooses the tests whose maps cover changed files and returns args as ' + bt('--lib') + ' plus enabled selected targets (scripts/select-tests.mjs:36-77, 98-103; docs/CI_TIERS.md:167-181).');
  L.push('- The selector uses the latest successful ' + bt('main') + ' map (the ' + bt('Select') + ' step of the ' + bt('select-tests') + ' job in ' + bt('.github/workflows/ci.yml') + '); the map is valid only for its recorded SHA and rows (docs/CI_TIERS.md:134-156). A missing map (no successful map run found), failed map download, or selector error writes an all-mode fallback (the ' + bt('fallback') + ' function of the ' + bt('Select') + ' step in the ' + bt('select-tests') + ' job of ' + bt('.github/workflows/ci.yml') + '); there is no freshness guarantee, so scheduled maps can be stale beyond the nightly cadence.');
  L.push('- File-reader attribution uses source-text patterns (' + bt('READS_FILES')
    + ' in scripts/select-tests.mjs:18-25, 56-77). A test reading changed data through an unrecognized mechanism could be missed by attribution; unmapped tests remain selected.');
  L.push('- In the ' + bt('test-core') + ' job of ' + bt('.github/workflows/ci.yml') + ', artifact download and selection-args steps use ' + bt('continue-on-error')
    + ', and the three cargo-test commands consume their outputs. A plain test command results when the selector arg output is empty; this does not establish atomic fallback for partial outputs or rule out unrelated upload/runner failures.');
  L.push('- Nightly full runs on ' + bt('main') + ', an explicit ' + bt('CI: full') + ' trailer, and ' + bt('workflow_dispatch') + ' select all (selector ' + bt('--full') + '; docs/CI_TIERS.md:116-127); weekly runs add weekly-only jobs.');
  L.push('');
  L.push('## Map currency');
  L.push('');
  L.push('- Distinct map SHAs across the ' + agg.runCount + ' artifacts: ' + agg.mapShas.length + ' ('
    + agg.mapShas.map((s) => bt(s)).join(', ') + ') — each artifact carries the two row maps of one '
    + bt('test-impact-map') + ' run.');
  L.push('- Runs whose ' + bt('head_sha') + ' equals one of that run\'s ' + bt('mapShas') + ': '
    + share(agg.mapCurrent, agg.runCount) + ' — equality is observed from the inputs; a different source commit does not establish its age or ancestry. The map is scheduled nightly; freshness is not checked by the selector.');
  L.push('');
  L.push('## Per-run table');
  L.push('');
  L.push('Ordered by created_at (ascending). ' + bt('selected N/T; skipped K/T') + ' reports selected and skipped target counts over total for subset rows; all-mode rows show —. '
    + 'Estimated savings use that skipped fraction; test seconds are the three ' + bt('/^test \\(/')
    + ' steps of the ' + bt(CORE_JOB_NAME) + ' job. All-mode rows include their exact recorded reason or ' + bt('unknown') + '.');
  L.push('');
  L.push('| run | event | conclusion | created_at (UTC) | mode | class | recorded all-mode reason | total targets | selected N/T; skipped K/T | test-core test s | estimated saved s |');
  L.push('|---|---|---|---|---|---|---|---|---|---|---|');
  for (const r of rows) {
    const selectedSkipped = r.mode === 'subset'
      ? 'selected ' + r.selCount + '/' + r.total + '; skipped ' + r.savedTargets + '/' + r.total
      : '—';
    const savedCell = r.mode === 'subset' ? s1(r.savedSeconds) : '—';
    L.push('| ' + r.id + ' | ' + r.event + ' | ' + r.conclusion + ' | ' + r.createdAt + ' | ' + r.mode + ' | '
      + r.cls + ' | ' + (r.mode === 'all' ? bt(r.reason ?? 'unknown') : '—') + ' | ' + r.total + ' | '
      + selectedSkipped + ' | ' + s1(r.testSeconds) + ' | ' + savedCell + ' |');
  }
  L.push('');
  L.push('This report makes NO GO/NO-GO decision.');
  L.push('');
  L.push('## P1.5a first-blocker upper-bound model (2026-11-17)');
  L.push('');
  L.push('This dated addendum uses only each historical artifact\'s recorded first blocker; no current source tree is consulted for historical candidates. A candidate means that removing that one blocker could make the run a subset; it is NOT a replay or proof of a subset. The artifacts retain no original full diffs, maps, or unmapped-target sets, so additional changed files may still force `all`, and exact would-be-subset run count and seconds are not identifiable.');
  L.push('');
  L.push('- First-blocker candidates: ' + candidateCount + ' / ' + agg.runCount + '.');
  L.push('- Hypothetical-zero-selected-target proportional estimate for candidates: ' + s1(candidateSeconds) + ' / ' + s1(agg.denominator) + ' s = ' + (agg.denominator === 0 ? 'not estimable' : p2((candidateSeconds / agg.denominator) * 100) + '%') + '. This is an upper bound, not estimated realized savings.');
  L.push('');
  L.push('| historical class | first-blocker candidates | hypothetical-zero-selected test s |');
  L.push('|---|---:|---:|');
  for (const c of CLASSES) {
    const x = candidateByClass.get(c);
    L.push('| ' + bt(c) + ' | ' + x.count + ' | ' + (x.count ? s1(x.seconds) : '—') + ' |');
  }
  L.push('');
  L.push('Candidate runs (recorded first blocker only):');
  for (const r of firstBlockerCandidates) {
    L.push('- ' + bt(r.id) + ' — ' + bt(r.file) + ' (' + bt(r.cls) + '); hypothetical-zero-selected estimate ' + s1(r.testSeconds * (r.total - 0) / r.total) + ' s.');
  }
  L.push('');
  L.push('The hypothetical uses the existing proportionality rule at selected=0 (all `total` targets hypothetically skipped). It does not imply the selector would in fact produce zero targets, and the exact would-be-subset run count and seconds remain unidentifiable without the original full diffs, maps, and unmapped sets.');
  L.push('');
  const conditionalRows = rows.map((r) => {
    const range = (inputs.ranges ?? new Map()).get(r.id);
    const cls = classifyConditionalRange(range, inputs.selections[r.id]);
    const seconds = cls === 'conditional-data' && r.total > 0 ? r.testSeconds * (r.total - 0) / r.total : 0;
    return { row: r, range, cls, seconds };
  });
  const conditionalSeconds = conditionalRows.reduce((sum, x) => sum + x.seconds, 0);
  const conditionalCount = conditionalRows.filter((x) => x.cls === 'conditional-data').length;
  const countClass = (cls) => conditionalRows.filter((x) => x.cls === cls).length;
  const graphCount = conditionalRows.filter((x) => x.range?.status === 'graph-reconstructed').length;
  L.push('## P1.5a reconstructed conditional-range evidence');
  L.push('');
  L.push('Separate optional range-evidence file: ' + (inputs.rangeBytes ?? 0) + ' bytes (' + kib(inputs.rangeBytes ?? 0) + ' KiB); it is validated independently against a 200 KiB limit and is excluded from the historical compact-input byte/file totals above.');
  L.push('');
  const historyRef = inputs.rangeHistoryRef ? ' Saved first-parent history ref: `' + inputs.rangeHistoryRef + '`.' : '';
  L.push('The ranges below are reconstructed from saved Git graph evidence, not from retained webhook payloads: event-base SHAs were not retained.' + historyRef + ' For a pushed two-parent main merge, parent 1 is an explicit assumption for the event base; for a PR, a head matching parent 2 of a merge on the saved ref is used only as conditional graph evidence and merge-base is recorded. Neither proves the original event base. Files are classified using the current exported `isDataFile` and `isAlwaysAll` selector rules. Real historical maps, reader sets, and unmapped-target sets are unavailable; therefore **no exact historical selected subset is claimed**.');
  L.push('');
  L.push('- Graph-reconstructed file ranges: ' + graphCount + ' / ' + agg.runCount + '; unresolved: ' + countClass('unresolved') + ' / ' + agg.runCount + '.');
  L.push('- Proven actual subsets: 0 / ' + agg.runCount + ' certifiable without historical maps, reader sets, and unmapped-target sets.');
  L.push('- Conditional-data candidates: ' + conditionalCount + ' / ' + agg.runCount + ' (all changed paths classify as data and none is ALWAYS_ALL; strict subset is conditional on the historical map/readers/unmapped targets not selecting ALL targets).');
  L.push('- Recorded full runs: ' + countClass('unchanged-full') + ' / ' + agg.runCount + '; reconstructed ranges containing a global ALWAYS_ALL path: ' + countClass('unchanged-global') + ' / ' + agg.runCount + '; map-dependent: ' + countClass('map-dependent') + ' / ' + agg.runCount + '; unresolved: ' + countClass('unresolved') + ' / ' + agg.runCount + '.');
  L.push('- Conditional-data proportional ceiling: ' + s1(conditionalSeconds) + ' / ' + s1(agg.denominator) + ' test s = ' + (agg.denominator === 0 ? 'not estimable' : p2(conditionalSeconds / agg.denominator * 100) + '%') + '. This applies the existing total-skipped-target proportionality model (selected=0) to each conditional-data run; it is an upper bound, not measured or realized savings.');
  L.push('');
  L.push('| run | base | head | graph method | changed files | class | conditional-data ceiling s |');
  L.push('|---|---|---|---|---:|---|---:|');
  for (const x of conditionalRows) {
    const { row: r, range } = x;
    L.push('| ' + r.id + ' | ' + bt(range?.base ?? '—') + ' | ' + bt(r.headSha) + ' | ' + bt(range?.method ?? 'unresolved')
      + ' | ' + (range?.status === 'graph-reconstructed' ? range.files.length : '—') + ' | ' + x.cls + ' | '
      + (x.cls === 'conditional-data' && r.total > 0 ? s1(x.seconds) : '—') + ' |');
  }
  L.push('');
  L.push('Counts above are derived from the saved ' + conditionalRows.length + ' per-run records, not hardcoded. The empty-file case is accepted as a valid zero range; that alone does not establish a changed target subset.');
  if (crate) L.push(...renderCrateGraphSection(crate, agg));
  return L.join('\n') + '\n';
}

/**
 * P1.5b dated section: the §7 crate-graph shadow recomputed over the committed
 * window. Rendered only when the real workspace graph is loadable (fixtures
 * without a cargo project keep the pre-P1.5b output byte-identical).
 */
function renderCrateGraphSection(crate, agg) {
  const L = [];
  const p2b = (x) => x.toFixed(2);
  const shareB = (n, d) => n + ' / ' + d + ' = ' + (d === 0 ? 'not estimable' : p2b((n / d) * 100) + '%');
  const scoped = crate.scoped;
  L.push('## P1.5b crate-graph selection shadow (2026-11-17)');
  L.push('');
  L.push('Recomputed with the chunk 1-2 crate-graph selection in force (shadow math only: no CI change, no job was affected). Generated by `node scripts/test-selection-impact.mjs --report` whenever `cargo metadata` yields the workspace graph (scripts/ci/crate-graph.mjs); `--check` recomputes it identically. Changed files come from the committed ranges.json graph reconstruction. select-tests semantics reuse the real exported rule predicates (isAlwaysAll, isDataFile, crateForPath, affectedCrates, rootAffected) in select() decision order; ci-plan semantics import scripts/ci-plan.mjs `plan` directly (crateGraph=null vs the real graph). No verdict: numbers only.');
  L.push('');
  L.push('Metric (plan §7, recomputed with the fixed graph): numerator = completed window runs whose committed file range is confined to non-root workspace crates that do NOT reach the root via declared Cargo dependencies or reference edges (select-tests returns a reader-tests-only root subset — file readers stay selected — and ci-plan drops seam/os and every non-affected crate_* group while keeping fast); denominator = completed window runs carrying a `test-selection` artifact.');
  L.push('');
  L.push('- Crate-only root-unaffected runs (numerator): ' + crate.numerator + ' / ' + crate.denominator
    + ' of the artifact window (' + shareB(crate.numerator, crate.denominator) + '); scoped to '
    + scoped.length + ' graph-reconstructed ranges (' + crate.unresolved + ' unresolved range'
    + (crate.unresolved === 1 ? '' : 's') + ' excluded). The select-tests numerator equals the ci-plan numerator: both are ' + crate.numerator + '.');
  L.push('- Skipped job groups over the window (Σ per-run groups on before minus on after, current gating): '
    + crate.skippedGroupTotal + ' groups over ' + scoped.length + ' scoped runs; '
    + crate.unchangedCount + ' of ' + scoped.length + ' scoped runs are unchanged.');
  L.push('- Hypothetical test-core ceiling for crate-only runs (same selected=0 proportionality rule as the P1.5a sections): '
    + s1(crate.testSecondsSkipped) + ' / ' + s1(agg.denominator) + ' s = '
    + (agg.denominator === 0 ? 'not estimable' : p2b((crate.testSecondsSkipped / agg.denominator) * 100) + '%') + '. Upper bound, not measured savings.');
  L.push('- Current gating job counts used below (from `ci-plan.mjs` parseJobs over the CURRENT .github/workflows/ci.yml): '
    + [...crate.jobsByGroup.entries()].sort().map(([g, n]) => g + ':' + n).join(', ') + '.');
  L.push('');
  const changedRows = scoped.filter((x) => x.skipped.length > 0 || x.sel.kind === 'crate-only-subset');
  if (!changedRows.length) {
    L.push('- No scoped run changes under the new selection (no skipped group on any run); no per-run before/after table. Every scoped range either requests a full run (recorded `CI: full`) or contains a data-file / non-crate path the shadow cannot attribute without historical maps (shadow kind `map-dependent`); no range is confined to non-root crates.');
  }
  if (changedRows.length) {
    L.push('| run | event | files | select shadow | skipped groups (before → after) | jobs before → after |');
    L.push('|---|---|---:|---|---|---|');
    for (const x of changedRows) {
      const kind = x.sel.kind === 'crate-only-subset'
        ? '`reader-tests-only subset` (crates: ' + x.sel.changed.join(', ') + ')'
        : '`all` (' + x.sel.kind.replace(/^all-/, '') + ')';
      L.push('| ' + x.row.id + ' | ' + x.row.event + ' | ' + x.files + ' | ' + kind + ' | '
        + x.beforeGroups.join(',') + ' → ' + x.afterGroups.join(',') + ' | '
        + x.jobsBefore + ' → ' + x.jobsAfter + ' |');
    }
  }
  L.push('');
  L.push('What the window data cannot show: the artifacts retain no historical maps, reader sets, or unmapped-target sets, so map-covered or data-file ranges (class `map-dependent` here) cannot be claimed as would-be subsets even under the new selection; the crate-only numerator is therefore a lower bound conditional on a map existing (all ' + agg.runCount + ' runs had selector data). Under the fixed ci-plan semantics `fast` stays on for every crate change, so before/after differences can only be seam/os suppression (root unaffected by Cargo AND reference edges) or dropped non-affected crate_* groups. Job and group counts apply the CURRENT ci.yml gating and the CURRENT crate-graph job set to historical diffs — the window\'s actual ci.yml had neither `crate_*` groups nor graph narrowing, so the before/after columns are a counterfactual, not what ran. The ' + crate.unresolved + ' unresolved range has no reconstructed base and is excluded entirely. The proportional test-second ceiling scales the root-crate test-core job; per-crate `crate_*` job durations are not in the inputs, so crate-job savings are not priced.');
  L.push('');
  L.push('This section makes NO GO/NO-GO decision.');
  return L;
}

// ── CLI ─────────────────────────────────────────────────────────────────────────
function parseArgs(argv) {
  const out = { mode: 'report' };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = (name) => { const v = argv[++i]; if (v === undefined) fail(name + ' needs a value'); return v; };
    if (a === '--report') out.mode = 'report';
    else if (a === '--check') out.mode = 'check';
    else if (a === '--report-path') out.reportPath = val(a);
    else if (a === '--inputs') out.inputs = val(a);
    else if (a === '--repo-root') out.repoRoot = val(a);
    else fail('unknown argument: ' + a);
  }
  return out;
}

function main() {
  const exitErr = (code, msg) => { process.stderr.write(SCRIPT + ': ' + msg + '\n'); process.exit(code); };
  let args, report, reportPath;
  try {
    args = parseArgs(process.argv.slice(2));
    const repoRoot = resolve(args.repoRoot ?? resolve(dirname(fileURLToPath(import.meta.url)), '..'));
    const inputsDir = resolve(args.inputs ?? join(repoRoot, DEFAULT_INPUTS));
    reportPath = resolve(args.reportPath ?? join(repoRoot, DEFAULT_REPORT));
    const inputs = loadInputs(inputsDir);
    const rows = computePerRun(inputs);
    const agg = computeAggregates(inputs, rows);
    // P1.5b: render the crate-graph shadow section whenever the real workspace
    // graph loads (cargo metadata). A fixture repo without a cargo project keeps
    // the pre-P1.5b report byte-identical; a load failure degrades the same way.
    let crate = null;
    const graph = loadCrateGraph(repoRoot);
    if (graph && inputs.ranges.size > 0) {
      crate = computeCrateGraphShadow(inputs, rows, graph,
        readFileSync(join(repoRoot, '.github/workflows/ci.yml'), 'utf8'));
    }
    report = renderReport(inputs, rows, agg, crate);
  } catch (e) {
    exitErr(2, e.message);
  }
  if (args.mode === 'check') {
    let committed;
    try { committed = readFileSync(reportPath, 'utf8'); } catch { exitErr(1, '--check: report file missing: ' + reportPath); }
    if (committed !== report) {
      exitErr(1, '--check FAILED: ' + reportPath + ' differs from a fresh generation (run `node ' + SCRIPT + ' --report`)');
    }
    process.stdout.write(SCRIPT + ': --check ok\n');
    return;
  }
  mkdirSync(dirname(reportPath), { recursive: true });
  writeFileSync(reportPath, report);
  process.stdout.write(report);
}

// Run main() only when executed directly as the CLI, never on import —
// importing must have no side effects and write nothing (same guard shape as
// scripts/ci-selection-history.mjs: the -e clause covers `node -e import(...)`).
if (
  process.argv[1]
  && !process.execArgv.some((a) => a === '-e' || a === '--eval')
  && pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) main();
