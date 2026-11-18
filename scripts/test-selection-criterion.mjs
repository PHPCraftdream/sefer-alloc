import assert from 'node:assert/strict';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { MAX_GH_CALLS } from './collect-test-selection-criterion.mjs';
import { CORE_ROWS, evaluateWindow, MIN_RUNS, WINDOW_SHA, WINDOW_START } from './test-selection-criterion/model.mjs';

export { CORE_ROWS, evaluateWindow };

function readOptional(path, fallback) {
  try {
    return readFileSync(path);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    return fallback;
  }
}

const cell = value => String(value ?? '—').replace(/\r\n|[\x00-\x1f\x7f-\x9f\u2028\u2029]/g, ' ').replace(/[\\|`*_<>\[\]~]/g, '\\$&');
const row = (...values) => `| ${values.map(cell).join(' | ')} |`;
const text = value => typeof value === 'string' && value.trim().length > 0;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
const selectedPaths = selection => selection?.mode === 'subset' && Array.isArray(selection.tests) ? selection.tests : null;

function readCollection(inputs) {
  const raw = readOptional(join(inputs, 'collection.json'), undefined);
  if (raw === undefined) return null;
  const collection = JSON.parse(raw);
  assert(object(collection) && typeof collection.repo === 'string' &&
    /^[a-z\d][a-z\d-]*\/(?!\.{1,2}$)[a-z\d_.-]+$/i.test(collection.repo) &&
    collection.since === WINDOW_START && typeof collection.listingComplete === 'boolean' &&
    Array.isArray(collection.issues) && collection.issues.every(text), 'Invalid collection.json');
  return collection;
}

function readChecks(inputs) {
  const checks = JSON.parse(readOptional(join(inputs, 'local-checks.json'), '[]'));
  assert(Array.isArray(checks) && checks.every(check => object(check) && text(check.command) &&
    Number.isSafeInteger(check.exitCode) && check.exitCode >= 0), 'Invalid local-checks.json');
  assert.equal(new Set(checks.map(check => check.command.trim())).size, checks.length,
    'Duplicate command in local-checks.json');
  return checks;
}

function render(runs, collection, checks) {
  const listingComplete = collection?.listingComplete === true;
  const { verdict, rows, counts, misses, subsetShare, shortfall } = evaluateWindow(runs, { listingComplete });
  const evidenceRow = (run, ...values) => {
    const id = cell(run.id);
    const link = collection ? `[${id}](https://github.com/${collection.repo}/actions/runs/${run.id})` : id;
    return `| ${link} ${row(...values)}`;
  };
  const listingCalls = 1;
  const capacity = perRun => {
    const runs = Math.floor((MAX_GH_CALLS - listingCalls) / perRun);
    assert(listingCalls + runs * perRun <= MAX_GH_CALLS &&
      listingCalls + (runs + 1) * perRun > MAX_GH_CALLS);
    return runs;
  };
  const runColumns = ['Run', 'Created', 'Event', 'Branch', 'Head SHA', 'Latest attempt', 'Evidence attempt', 'Collection check', 'Run status', 'Run conclusion', 'Core lookup', 'Core conclusion', 'Shadow', ...CORE_ROWS, 'Selection', 'Selected / total', 'Artifact reason', 'Crate-only crates', 'Category', 'Reason', 'Failures', 'Record issues'];
  const share = counts.informative ? `${counts.subset}/${counts.informative} (${(subsetShare * 100).toFixed(2)}%)` : '0/0 (undefined)';
  const reasons = new Map();
  const selections = new Map();
  const count = (map, ...fields) => {
    const key = JSON.stringify(fields);
    map.set(key, (map.get(key) ?? 0) + 1);
  };
  const breakdown = map => {
    assert.equal([...map.values()].reduce((sum, n) => sum + n, 0), counts.total);
    return [...map.keys()].sort().map(key => row(...JSON.parse(key), map.get(key)));
  };
  for (const { run, category, reason } of rows) {
    count(reasons, category, reason);
    count(selections, category, run.selection?.mode ?? 'missing', run.selection?.reason ?? 'missing');
  }
  const redRuns = rows.filter(({ run }) => run.status === 'completed' && run.conclusion === 'failure');
  const redCounts = { informative: 0, uninformative: 0, excluded: 0 };
  for (const { category } of redRuns) redCounts[category]++;
  assert.equal(redRuns.length, redCounts.informative + redCounts.uninformative + redCounts.excluded);
  const unresolvedEvidence = rows.filter(entry => entry.unresolvedRed || entry.unresolvedAttempts.length);
  assert.equal(unresolvedEvidence.filter(entry => entry.unresolvedRed).length, counts.unresolvedRed);
  assert.equal(unresolvedEvidence.filter(entry => entry.unresolvedAttempts.length).length, counts.unresolvedAttempts);
  assert.equal(misses.length, counts.misses);
  const evidenceAttempt = run => run.evidenceAttempt ?? (run.evidenceAttempt === undefined && run.attempt === 1 ? '1 (legacy)' : 'unbound');
  const issues = run => Array.isArray(run.issues) && run.issues.length ? run.issues.join('; ') : 'None recorded';
  const attemptDetails = attempts => attempts.map(({ attempt, reason }) => `${attempt ?? 'unknown'}: ${reason}`).join('; ') || null;
  const failureSource = failure => failure.sourceType ?? failure.source ?? (failure.attempt == null ? 'latest' : 'archived');
  const observedMiss = miss => {
    const { run, kind, name, path, rows: steps, reason } = miss;
    const attempt = miss.attempt ?? run.attempt;
    const archived = miss.attempt != null;
    const snapshots = archived && Array.isArray(run.priorAttempts)
      ? run.priorAttempts.filter(entry => entry?.attempt === attempt) : [];
    // Duplicate archives cannot be attributed by attempt number alone.
    const selection = archived ? (snapshots.length === 1 ? snapshots[0].selection : null) : run.selection;
    const paths = selectedPaths(miss.selection ?? selection);
    return evidenceRow(run, attempt, failureSource(miss), kind, name, path, steps.join(', '),
      paths ? paths.join(', ') || '(empty subset)' : null, reason);
  };
  const available = (label, predicate) => {
    const ids = rows.filter(({ run }) => predicate(run)).map(({ run }) => cell(run.id));
    return `- ${label}: ${ids.length ? `${ids.length}/${counts.total} (run IDs: ${ids.join(', ')})` : `0/${counts.total}`}.`;
  };
  const verdictReasons = [];
  if (!listingComplete) verdictReasons.push('run listing completeness is not established');
  if (shortfall) verdictReasons.push(`${shortfall} more informative runs needed (${counts.informative}/${MIN_RUNS})`);
  if (counts.misses) verdictReasons.push(`${counts.misses} confirmed ${counts.misses === 1 ? 'miss' : 'misses'}`);
  if (counts.unresolvedRed) verdictReasons.push(`${counts.unresolvedRed} ${counts.unresolvedRed === 1 ? 'run has' : 'runs have'} unresolved red evidence`);
  if (counts.unresolvedAttempts) verdictReasons.push(`${counts.unresolvedAttempts} ${counts.unresolvedAttempts === 1 ? 'run has' : 'runs have'} unresolved attempt evidence`);
  if (!counts.subset) verdictReasons.push('no informative subset run');
  return [
    '# Test selection enforcement criterion', '',
    `**Verdict: ${verdict}**`,
    `Reason: ${verdictReasons.join('; ') || 'all criterion requirements met'}.`, '',
    `Window starts at ${WINDOW_START} (\`${WINDOW_SHA}\`).`,
    `Collection: ${cell(collection?.repo ?? 'not recorded')}; since ${cell(collection?.since ?? 'not recorded')}; listing complete: ${listingComplete ? 'yes' : 'not established'}.`, '',
    'Scope: ordinary pull-request runs or pushes to `main`, except `CI: full` runs identified by the exact selection-artifact reason `full run requested`. Scheduled and manually dispatched runs are excluded. Repeated pushes to the same PR count as distinct runs. Reruns are correlated, so each run ID contributes at most one observation.', '',
    'Latest-attempt metadata is authoritative for run status and denominator classification. Retained `priorAttempts` snapshots are evaluated for red failures; confirmed misses below identify the attempt and affected target. Evidence must be bound to its own attempt by `evidenceAttempt` (untagged legacy attempt 1 records are supported). Older evidence cannot be borrowed for the latest attempt. Captured misses from older red evidence remain in the report even after a green rerun.', '',
    'The guarded collector binds evidence with `evidenceAttempt` and makes one final GET `repos/<repo>/actions/runs/<id>` per run when the pass collects or refreshes candidate jobs, artifact, or log evidence, or retries an unstable check, to verify the run ID and compare attempt, status, conclusion, and head SHA when returned. If the run differs or the check is unavailable, it discards the candidate evidence and blocks PASS. Verified cache-only records need no extra GET when the fresh listing matches the last validated metadata and no evidence is acquired. On the first guarded refresh, unverified legacy evidence is not reused as current evidence; it can still be retained as historical evidence.', '',
    'The API does not offer a transaction spanning evidence collection and the disk write. A run can change after the final GET and before the write. Such a change does not mix the data already collected, but the verdict can lag the current run state until the next refresh.', '',
    'Selection artifact downloads are not attempt-scoped, so the collector does not download them for attempts greater than 1. The resulting lack of trustworthy selection evidence can block PASS even when the rerun is green.', '',
    'Actual shadow execution runs the full core test rows with selector setup skipped; the artifact describes what enforcement would have selected, not what actually ran. A required-features target is eligible when it actually ran in a row with the same features. Subset execution retains library tests, retains integration targets only when their observed paths are selected, and misses doctest and other target kinds.', '',
    'In `.github/workflows/ci.yml`, only the three `test-core` rows consume the `Selected tests` outputs. Other jobs and workspace-only tests outside those rows are not evaluated, even for crate-only selections.', '',
    'If failure attribution remains unresolved, a future shadow run should write structured failure-target evidence per core row (row/features, target kind, name and observed path, or an explicit pre-test/unresolved status) and always upload it, including on failure. Include the run ID and attempt ID in both the artifact name and its contents so the evidence can be attributed to the correct retry. No CI changes are made here.', '',
    'Rare failure classes may be unsampled; no observed misses does not prove future safety. Retain the nightly full run of `main` and `CI: full` at sprint close.', '',
    '## Numerical criterion', '',
    `PASS applies only to the three in-scope shadow \`test-core\` rows: the listing must be complete, at least ${MIN_RUNS} distinct run IDs must be informative (including at least one subset), and there must be zero confirmed misses and no unresolved red or attempt evidence.`,
    'An incomplete listing or too few informative runs yields `INSUFFICIENT_WINDOW`. Once those requirements are met, a confirmed miss yields `FAIL`; otherwise unresolved red or attempt evidence yields `INSUFFICIENT_WINDOW`. With neither a miss nor unresolved evidence, no informative subset run yields `FAIL` for lack of subset observations, not for a confirmed miss.',
    `Run IDs: ${counts.total} total, ${counts.informative} informative, ${counts.uninformative} uninformative, ${counts.excluded} excluded. Informative runs: ${counts.all} all, ${counts.subset} subset. Shortfall: ${shortfall}. Confirmed misses: ${counts.misses}. Run IDs with unresolved red evidence (latest or historical): ${counts.unresolvedRed}; with unresolved attempt evidence: ${counts.unresolvedAttempts}.`,
    `Latest-attempt red runs (completed with conclusion \`failure\`): ${redRuns.length} total, ${redCounts.informative} informative, ${redCounts.uninformative} uninformative, ${redCounts.excluded} excluded.`,
    `Subset share (informative subset runs / all informative runs): ${share}. A denominator of zero makes \`0/0\` undefined.`, '',
    '## Available inputs and remaining unknowns', '',
    'The recorded inputs are the run JSON records and the collection and local-check metadata summarized in this report. Evidence present in the run records:', '',
    available('Core job metadata', run => run.core != null),
    available('Selection artifact data', run => run.selection != null),
    available('Reduced Cargo failure evidence', run => Array.isArray(run.core?.rows) && run.core.rows.some(step => step?.conclusion === 'failure' && step.evidence != null)), '',
    '### Unresolved red and attempt evidence', '',
    'The criterion fails closed: an in-scope, completed failed run that is uninformative is unresolved unless the core evidence rules out a core test failure. An unavailable core job does not establish that tests were skipped, failed before running, or succeeded; explicit, consistent skipped, pre-test, or successful core evidence can rule out a test failure. Missing evidence is neither a confirmed miss nor proof that the selector retained every failing target.', '',
    ...(unresolvedEvidence.length ? [
      '| Run | Latest attempt | Unresolved red evidence | Unresolved attempts | Latest model reason | Recorded evidence gaps |',
      '| --- | --- | --- | --- | --- | --- |',
      ...unresolvedEvidence.map(entry => evidenceRow(entry.run, entry.run.attempt, entry.unresolvedRed ? 'Yes' : 'No',
        attemptDetails(entry.unresolvedAttempts), entry.reason, issues(entry.run))),
      '',
    ] : ['None recorded.', '']),
    '## Collection issues', '',
    ...(collection?.issues?.length ? collection.issues.map(issue => `- ${cell(issue)}`) : ['None recorded.']), '',
    '## Status reasons', '',
    '| Category | Reason | Runs |', '| --- | --- | ---: |', ...breakdown(reasons), '',
    '## Selection counts', '',
    '| Category | Selection | Artifact reason | Runs |', '| --- | --- | --- | ---: |', ...breakdown(selections), '',
    '## Run evidence', '',
    'Status and category refer to the latest attempt recorded. Evidence fields show what was recorded, not necessarily what the model accepted for that attempt. The Collection check column shows `collectionCheck`: an `unstable` marker means the final check found a change or could not verify the run, so the candidate evidence was discarded. A dash means no check marker was recorded, not that a check passed.', '',
    row(...runColumns),
    row(...runColumns.map(() => '---')),
    ...rows.map(({ run, category, reason, failures }) => evidenceRow(
      run, run.createdAt, run.event, run.headBranch, run.headSha, run.attempt, evidenceAttempt(run), run.collectionCheck, run.status, run.conclusion, run.coreLookup, run.core?.conclusion,
      run.core?.shadow, ...CORE_ROWS.map(name => (Array.isArray(run.core?.rows) ? run.core.rows : []).filter(step => step?.name === name).map(step => step.conclusion ?? 'missing').join(', ') || 'missing'),
      run.selection?.mode, run.selection?.total == null ? null :
        `${run.selection.mode === 'all' ? run.selection.total : selectedPaths(run.selection)?.length ?? '—'} / ${run.selection.total}`,
      run.selection?.reason, Array.isArray(run.selection?.crateOnly?.crates) ? run.selection.crateOnly.crates.join(', ') || 'None' : null,
      category, reason,
      failures.map(failure => `${failure.attempt == null ? '' : `attempt ${failure.attempt}: `}${failureSource(failure)}: ${failure.kind}: ${failure.name}${failure.path ? ` (${failure.path})` : ''} (${failure.rows.join(', ')}; ${failure.reason})`).join('; ') || 'None observed',
      issues(run),
    )), '',
    '## Observed misses', '',
    '| Run | Attempt | Evidence source | Kind | Target | Path | Core rows | Selected subset (paths) | Reason |',
    '| --- | --- | --- | --- | --- | --- | --- | --- | --- |',
    ...misses.map(observedMiss),
    '', ...(misses.length ? [] : ['No confirmed misses in the available evidence.', '']),
    '## Reproduce from recorded inputs', '',
    'To refresh the recorded CI inputs from the repository root, use the collector:', '',
    '```sh',
    'node scripts/collect-test-selection-criterion.mjs',
    '```', '',
    `The collector caps counted \`gh\` invocations at ${MAX_GH_CALLS}. Theoretical single-page budget with ${listingCalls} run-listing call and one jobs page per new run: ${capacity(3)} new green runs (3 calls each: jobs, artifact, final GET), or ${capacity(4)} new red runs with a failing core and log fetch (4 each: jobs, artifact, core log, final GET). These limits assume first attempts that download an artifact and only one kind of new run. Verified unchanged cache-only runs cost no per-run calls; they consume only listing pages. Extra listing or jobs pages reduce capacity. These are not observed totals; see the collector’s logged \`calls\` count for actual invocations.`, '',
    'The collector’s selection integrity receipt stores the original artifact SHA-256 (`sourceSha256`), the compact content SHA-256 (`compactSha256`), and the test count (`testCount`). To verify the source digest for an eligible first-attempt download, compare it with the SHA-256 of the exact `selection.json` bytes from `gh run download <ID> --name test-selection --repo PHPCraftdream/sefer-alloc`. Offline, the recorded inputs alone can verify only the compact digest and test count, not the original artifact digest.', '',
    'Without recollecting, regenerate `docs/perf/TEST_SELECTION_CRITERION.md` from `docs/perf/test_selection_criterion_inputs`, or check freshness without writing:', '',
    '```sh',
    'node scripts/test-selection-criterion.mjs --report',
    'node scripts/test-selection-criterion.mjs --check',
    '```', '',
    '## Appendix: Local checks', '',
    ...(checks.length ? ['| Command | Exit code |', '| --- | ---: |', ...checks.map(({ command, exitCode }) => row(command, exitCode))] : ['None recorded.']), '',
  ].join('\n');
}

function main() {
  const { values } = parseArgs({ options: {
    report: { type: 'boolean' }, check: { type: 'boolean' },
    inputs: { type: 'string', default: fileURLToPath(new URL('../docs/perf/test_selection_criterion_inputs', import.meta.url)) },
    'report-path': { type: 'string', default: fileURLToPath(new URL('../docs/perf/TEST_SELECTION_CRITERION.md', import.meta.url)) },
  } });
  if (Boolean(values.report) === Boolean(values.check)) throw new Error('Specify exactly one of --report or --check');
  const inputs = values.inputs;
  const output = values['report-path'];
  const runs = readdirSync(inputs).filter(name => /^\d+\.json$/.test(name)).sort().map(name => {
    const run = JSON.parse(readFileSync(join(inputs, name), 'utf8'));
    if (String(run?.id) !== name.slice(0, -5)) throw new Error(`Run ID does not match filename: ${name}`);
    return run;
  });
  runs.sort((a, b) => compare(Date.parse(a.createdAt), Date.parse(b.createdAt)) ||
    compare(BigInt(a.id), BigInt(b.id)) || compare(String(a.id), String(b.id)));
  const expected = Buffer.from(render(runs, readCollection(inputs), readChecks(inputs)));
  if (values.report) {
    writeFileSync(output, expected);
    process.stdout.write(expected);
  } else if (!readOptional(output, Buffer.alloc(0)).equals(expected)) {
    console.error(`Missing or stale report: ${output}`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
