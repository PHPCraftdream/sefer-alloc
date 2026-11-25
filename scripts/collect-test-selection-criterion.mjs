import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { parseCoreLog } from './test-selection-criterion/cargo-log.mjs';
import { CORE_ROWS, WINDOW_START as SINCE } from './test-selection-criterion/model.mjs';
import { assertSelectionIntegrity, sealSelection } from './test-selection-criterion/selection-integrity.mjs';

export const MAX_GH_CALLS = 45;
export const COLLECTION_CALL_COST = Object.freeze({ listing: 1, green: 3, red: 4 });
const DEFAULT_REPO = 'PHPCraftdream/sefer-alloc';
const DEFAULT_INPUTS = fileURLToPath(new URL('../docs/perf/test_selection_criterion_inputs/', import.meta.url));
const CORE = 'test (core tiers)';
const SELECTOR = 'select tests (diff + test-impact map)';
const CORE_ABSENT = 'core job missing or ambiguous';
const JOB_CONCLUSIONS = new Set(['action_required', 'cancelled', 'failure', 'neutral', 'skipped', 'stale', 'startup_failure', 'success', 'timed_out']);
const MAX_BYTES = 200 * 1024;
const PAGE_SIZE = 100;
const ATTEMPT_JOBS_PAGE_SIZE = 30;
const RUN_LIST_LIMIT = 1000;

const idOf = value => (typeof value === 'string' && /^[0-9]+$/.test(value)) ||
  (Number.isSafeInteger(value) && value > 0) ? String(value) : null;
const stringOrNull = value => typeof value === 'string' ? value : null;
const stringArray = value => Array.isArray(value) && value.every(x => typeof x === 'string');
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const fits = value => Buffer.byteLength(json(value)) <= MAX_BYTES - 2048;
const command = args => {
  const visible = [];
  for (let i = 0; i < args.length; i++) {
    if (args[0] === 'run' && args[1] === 'download' && args[i] === '--dir') i++;
    else visible.push(args[i]);
  }
  return ['gh', ...visible.map(arg => /^[\w./:-]+$/.test(arg) ? arg : JSON.stringify(arg))].join(' ');
};
const truncated = (what, args) => `# TRUNCATED — see ${what} — full output reproducible via ${command(args)}`;
const gh = args => spawnSync('gh', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, windowsHide: true });

function readOptional(path) {
  try { return readFileSync(path, 'utf8'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; return null; }
}

function writeChanged(path, value, prior = readOptional(path)) {
  const text = json(value);
  if (Buffer.byteLength(text) > MAX_BYTES) throw new Error(`Input exceeds 200 KiB: ${path}`);
  if (text !== prior) writeFileSync(path, text);
}

function parseJson(text, message) {
  try { return JSON.parse(text); }
  catch { throw new Error(message); }
}

function paginate(request, makeArgs, key, identity, limit = Infinity, pageSize = PAGE_SIZE) {
  const entries = new Map();
  let expected;
  for (let page = 1; ; page++) {
    const args = makeArgs(page);
    try {
      const data = parseJson(request(args), 'invalid JSON page');
      const items = data?.[key];
      const count = data?.total_count;
      if (!Array.isArray(items) || items.length > pageSize || !Number.isSafeInteger(count) || count < 0) {
        throw new Error('invalid page');
      }
      expected ??= count;
      let invalid = count !== expected;
      for (const item of items) {
        const id = item && identity(item);
        if (!id || entries.has(id)) invalid = true;
        else entries.set(id, item);
      }
      if (invalid) throw new Error('listing changed or contained invalid/duplicate rows');
      if (entries.size >= limit) throw new Error(`listing reached GitHub’s ${limit}-result search limit`);
      if (entries.size === expected) return { entries: [...entries.values()], issue: null };
      if (items.length < pageSize || entries.size > expected) throw new Error('listing ended before matching total_count');
    } catch (error) {
      return { entries: [...entries.values()], issue: `${key} listing incomplete: ${error.message}; ${command(args)}` };
    }
  }
}

function runIdentity(run) {
  return Number.isFinite(Date.parse(run.created_at)) && Date.parse(run.created_at) >= Date.parse(SINCE) &&
    typeof run.status === 'string' && typeof run.event === 'string' && typeof run.head_sha === 'string' &&
    (run.event !== 'push' || typeof run.head_branch === 'string') ? idOf(run.id) : null;
}

function compactSelection(source) {
  if (!source || !['all', 'subset'].includes(source.mode) || typeof source.reason !== 'string' || !source.reason ||
      !Number.isSafeInteger(source.total) || source.total < 0 ||
      (source.mode === 'subset' && !stringArray(source.tests))) throw new Error('invalid selection.json');
  const { mode, reason, total, tests, mapShas, crateOnly } = source;
  const selection = { mode, reason, total };
  for (const [key, value] of Object.entries({ tests, mapShas })) {
    if (value === undefined) continue;
    if (!stringArray(value)) throw new Error(`invalid selection.json ${key}`);
    selection[key] = value;
  }
  if (crateOnly !== undefined) {
    if (!stringArray(crateOnly?.crates)) throw new Error('invalid selection.json crateOnly');
    selection.crateOnly = { crates: crateOnly.crates };
  }
  return selection;
}

function exact(items, name) {
  const matches = items.filter(item => item?.name === name);
  return matches.length === 1 ? matches[0] : null;
}

function compactCore(job) {
  const steps = Array.isArray(job.steps) ? job.steps : [];
  return {
    id: idOf(job.id),
    conclusion: stringOrNull(job.conclusion),
    shadow: exact(steps, 'Selected tests')?.conclusion === 'skipped',
    rows: CORE_ROWS.map(name => ({ name, conclusion: stringOrNull(exact(steps, name)?.conclusion) })),
  };
}

function needsLog(core) {
  return core && (core.conclusion === 'failure' || core.rows.some(row => row.conclusion === 'failure'));
}

function collectLog(record, request, root) {
  const core = record.core;
  if (!needsLog(core)) return;
  const failed = core.rows.filter(row => row.conclusion === 'failure');
  const targets = failed.length ? failed : core.rows.filter(row => row.conclusion !== 'success');
  const pending = targets.filter(row => !row.evidence);
  if (!pending.length) return;
  const args = ['api', `${root}/jobs/${core.id}/logs`];
  try {
    const parsed = parseCoreLog(request(args), core.rows.map(({ name, conclusion }) => ({ name, conclusion })));
    for (const row of pending) {
      const evidence = parsed.find(item => item.name === row.name)?.evidence;
      row.evidence = evidence ?? { status: 'unresolved', reason: 'no row evidence found in core job log' };
    }
    if (!fits(record)) {
      for (const row of pending) delete row.evidence;
      record.issues.push(truncated('reduced core log evidence omitted', args));
    }
  } catch (error) {
    record.issues.push(`core log unavailable: ${error.message}; ${command(args)}`);
  }
}

function selectionArgs(id, repo) {
  return ['run', 'download', id, '--name', 'test-selection', '--repo', repo];
}

function collectSelection(record, request, repo) {
  const dir = mkdtempSync(join(tmpdir(), 'test-selection-criterion-'));
  const args = [...selectionArgs(record.id, repo), '--dir', dir.replaceAll('\\', '/')];
  try {
    request(args);
    const rawBuffer = readFileSync(join(dir, 'selection.json'));
    record.selection = sealSelection(compactSelection(JSON.parse(rawBuffer.toString('utf8'))), rawBuffer);
    if (!fits(record)) {
      record.selection = null;
      record.issues.push(truncated('selection artifact evidence omitted', args));
    }
  } catch (error) {
    const reason = error.code === 'ENOENT' ? 'selection.json missing' : error instanceof SyntaxError ? 'malformed selection.json' :
      error.code && error.code !== 'ERR_ASSERTION' ? 'selection.json could not be read' : error.message;
    record.issues.push(`selection artifact unavailable: ${reason}; ${command(args)}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function collectJobs(record, prior, request, root, retrySelector) {
  const jobsKnown = prior?.coreLookup === 'available';
  if (prior?.core && JOB_CONCLUSIONS.has(prior.core.conclusion)) {
    record.core = structuredClone(prior.core);
    record.coreLookup = 'available';
  }
  let selectorIssue = prior?.issues?.find(issue => issue.startsWith('selector job '));
  const selectorKnown = prior?.selection || (jobsKnown && !selectorIssue && !prior.issues?.some(issue => issue.startsWith('jobs listing incomplete:'))) || prior?.issues?.some(issue =>
    issue.startsWith('selection artifact unavailable:') || issue.startsWith('# TRUNCATED — see selection artifact evidence omitted'));
  if (!record.core || (retrySelector && !selectorKnown)) {
    const jobs = paginate(request, page => ['api', `${root}/runs/${record.id}/attempts/${record.attempt}/jobs?per_page=${PAGE_SIZE}&page=${page}`], 'jobs', job => idOf(job.id));
    if (jobs.issue) {
      record.issues.push(jobs.issue);
    } else {
      if (!record.core) {
        const job = exact(jobs.entries, CORE);
        if (job && JOB_CONCLUSIONS.has(job.conclusion) && (job.status === undefined || job.status === 'completed')) {
          record.core = compactCore(job);
          record.coreLookup = 'available';
        } else record.issues.push(job ? 'core job conclusion unavailable' : CORE_ABSENT);
      }
      if (!selectorKnown && retrySelector) {
        const selector = exact(jobs.entries, SELECTOR);
        selectorIssue = selector?.conclusion === 'success' ? undefined :
          `selector job ${selector ? `conclusion: ${stringOrNull(selector.conclusion)}` : 'missing or ambiguous'}`;
        if (!selectorIssue) return true;
      }
    }
  }
  if (selectorIssue && !selectorKnown && retrySelector) record.issues.push(selectorIssue);
  return Boolean(selectorKnown);
}

function attemptDetail(record, attempts, n, acquire, root) {
  const args = ['api', `${root}/runs/${record.id}/attempts/${n}`];
  try {
    const detail = parseJson(acquire(args), 'invalid attempt detail JSON');
    if (idOf(detail?.id) !== record.id || detail?.run_attempt !== n || detail?.status !== 'completed' ||
        typeof detail?.conclusion !== 'string' || !detail.conclusion.trim() ||
        !Number.isFinite(Date.parse(detail?.created_at)) ||
        (detail.head_sha !== undefined && detail.head_sha !== record.headSha)) {
      throw new Error('malformed attempt detail (expected matching id, run_attempt, completed status, conclusion, created_at, and head_sha when present)');
    }
    attempts.set(n, { created: Date.parse(detail.created_at), detail });
    return true;
  } catch (error) {
    record.issues.push(`attempt ${n} detail unavailable: ${error.message}; ${command(args)}`);
    return false;
  }
}

function selectorActivity(entries, from, to) {
  let provable = true;
  let executed = false;
  for (const job of entries.filter(item => item?.name === SELECTOR)) {
    const started = Date.parse(job?.started_at);
    if (!Number.isFinite(started)) { provable = false; continue; }
    if (started >= from && started < to) executed = true;
  }
  return { provable, executed };
}

function resolveRerun(record, acquire, root, repo) {
  const snapshots = [];
  const attempts = new Map();
  for (let n = 1; n <= record.attempt; n++) {
    if (!attemptDetail(record, attempts, n, acquire, root)) return { snapshots };
  }
  for (let n = 2; n <= record.attempt; n++) {
    if (attempts.get(n).created <= attempts.get(n - 1).created) {
      record.issues.push(`attempt ${n} was not created after attempt ${n - 1}; rerun attempt windows are not provable`);
      return { snapshots };
    }
  }
  const artifacts = paginate(acquire, page => ['api', `${root}/runs/${record.id}/artifacts?per_page=${PAGE_SIZE}&page=${page}`], 'artifacts', item => idOf(item?.id));
  if (artifacts.issue) {
    record.issues.push(artifacts.issue);
    return { snapshots };
  }
  const named = artifacts.entries.filter(item => item?.name === 'test-selection');
  const usable = named.filter(item => Number.isFinite(Date.parse(item?.created_at)));
  if (named.length !== 1 || usable.length !== 1) {
    record.issues.push(`test-selection artifact is missing or ambiguous for rerun (${named.length} named, ${usable.length} with a readable creation time)`);
    return { snapshots };
  }
  const artifactAt = Date.parse(usable[0].created_at);
  let source = null;
  for (let n = 1; n < record.attempt; n++) {
    if (artifactAt >= attempts.get(n).created && artifactAt < attempts.get(n + 1).created) source = n;
  }
  if (source === null) {
    record.issues.push(`test-selection artifact created at ${usable[0].created_at} is not provably bound to a single earlier attempt`);
    return { snapshots };
  }
  const jobs = new Map();
  for (let n = 1; n <= record.attempt; n++) {
    const listing = paginate(acquire,
      page => ['api', `${root}/runs/${record.id}/attempts/${n}/jobs?per_page=${ATTEMPT_JOBS_PAGE_SIZE}&page=${page}`],
      'jobs', job => idOf(job?.id), Infinity, ATTEMPT_JOBS_PAGE_SIZE);
    if (listing.issue) {
      record.issues.push(`attempt ${n} ${listing.issue}`);
      return { snapshots };
    }
    jobs.set(n, listing.entries);
  }
  const latestCore = exact(jobs.get(record.attempt), CORE);
  if (latestCore && JOB_CONCLUSIONS.has(latestCore.conclusion) && (latestCore.status === undefined || latestCore.status === 'completed')) {
    record.core = compactCore(latestCore);
    record.coreLookup = 'available';
  } else record.issues.push(latestCore ? 'core job conclusion unavailable' : CORE_ABSENT);
  const origin = exact(jobs.get(source), SELECTOR);
  const originRun = selectorActivity(jobs.get(source), attempts.get(source).created, attempts.get(source + 1).created);
  if (!originRun.provable || !originRun.executed || !origin || origin.status !== 'completed' || origin.conclusion !== 'success') {
    record.issues.push(`selector job in attempt ${source} is not provably completed and successful within that attempt`);
    return { snapshots };
  }
  for (let n = source + 1; n <= record.attempt; n++) {
    const later = selectorActivity(jobs.get(n), attempts.get(n).created, n < record.attempt ? attempts.get(n + 1).created : Infinity);
    if (!later.provable || later.executed) {
      record.issues.push(`selector re-execution in attempt ${n} is not ruled out; the artifact selection cannot be bound to attempts after ${source}`);
      return { snapshots };
    }
  }
  if (!record.selection) collectSelection(record, acquire, repo);
  for (let n = 1; n < record.attempt; n++) {
    const { detail } = attempts.get(n);
    const snapshot = {
      id: record.id, attempt: n, createdAt: detail.created_at, event: record.event,
      headBranch: record.headBranch, headSha: record.headSha, status: 'completed',
      conclusion: detail.conclusion, selection: n >= source ? record.selection : null,
      core: null, coreLookup: 'unavailable', issues: [], collectionCheck: 'stable',
    };
    const job = exact(jobs.get(n), CORE);
    if (job && JOB_CONCLUSIONS.has(job.conclusion) && (job.status === undefined || job.status === 'completed')) {
      snapshot.core = compactCore(job);
      snapshot.coreLookup = 'available';
    } else snapshot.issues.push(job ? 'core job conclusion unavailable' : CORE_ABSENT);
    if (snapshot.core || snapshot.selection) snapshot.evidenceAttempt = n;
    collectLog(snapshot, acquire, root);
    snapshots.push(snapshot);
  }
  return { snapshots };
}

function savedSelection(selection) {
  if (selection == null) return null;
  try { assertSelectionIntegrity(selection); return selection; }
  catch { return null; }
}

const HISTORY_INCOMPLETE = "attempt history incomplete";

function evidenceAttempt(record) {
  // Legacy collector records only acquired evidence for attempt 1.
  return record.evidenceAttempt ?? (record.attempt === 1 && record.status === "completed" &&
    ((record.coreLookup === "available" && record.core) || savedSelection(record.selection)) ? 1 : undefined);
}

function retainHistory(record, old, changedRun, observedRed, snapshots = []) {
  const history = [...(old?.priorAttempts ?? [])];
  let incomplete = old?.historyIncomplete === true;
  const archive = source => {
    const { priorAttempts, historyIncomplete, ...snapshot } = source;
    snapshot.evidenceAttempt = evidenceAttempt(snapshot);
    if (snapshot.selection && !savedSelection(snapshot.selection)) {
      snapshot.selection = null;
      incomplete = true;
    }
    const index = history.findIndex(item => ['attempt', 'status', 'conclusion', 'headSha']
      .every(key => item[key] === snapshot[key]));
    if (index < 0) history.push(snapshot);
    else if (history[index].collectionCheck === 'unstable' && snapshot.collectionCheck !== 'unstable') {
      history[index] = snapshot;
    }
  };
  if (old?.status === 'completed' && changedRun &&
      (old.collectionCheck !== 'unstable' || old.conclusion === 'failure')) archive(old);
  if (observedRed) archive(observedRed);
  for (const snapshot of snapshots) archive(snapshot);
  if (history.length) {
    // An API regression can return the same metadata again after an intervening state.
    // The model cannot treat that archived red as a separate attempt.
    if (history.some(snapshot => snapshot.attempt === record.attempt &&
        record.conclusion === 'failure' && snapshot.conclusion === record.conclusion &&
        snapshot.status === record.status && snapshot.headSha === record.headSha &&
        (snapshot.collectionCheck !== 'stable' || record.collectionCheck !== 'stable') &&
        record.collectionCheck !== 'unstable')) incomplete = true;
    record.priorAttempts = [];
    for (const snapshot of history) {
      record.priorAttempts.push(snapshot);
      if (!fits(record)) {
        record.priorAttempts.pop();
        incomplete = true;
      }
    }
  }
  if (incomplete) {
    record.historyIncomplete = true;
    record.issues.push(HISTORY_INCOMPLETE);
  }
}

function checkCollection(record, request, root) {
  const args = ['api', `${root}/runs/${record.id}`];
  try {
    const detail = parseJson(request(args), 'invalid run detail JSON');
    if (idOf(detail?.id) !== record.id || !Number.isSafeInteger(detail.run_attempt) || detail.run_attempt < 1 ||
        typeof detail.status !== 'string' || !detail.status.trim() ||
        (detail.conclusion !== null && (typeof detail.conclusion !== 'string' || !detail.conclusion.trim())) ||
        (detail.status === 'completed' && detail.conclusion === null) ||
        (detail.head_sha !== undefined && (typeof detail.head_sha !== 'string' || !detail.head_sha.trim()))) {
      throw new Error('malformed run detail (expected matching id, numeric run_attempt, status, conclusion and valid head_sha if present)');
    }
    const latest = { attempt: detail.run_attempt, status: detail.status, conclusion: detail.conclusion,
      headSha: detail.head_sha ?? record.headSha };
    const changes = Object.keys(latest).filter(key => latest[key] !== record[key]);
    if (changes.length) return { latest, issue: `run changed during collection: ${changes.map(key =>
      `${key} ${JSON.stringify(record[key])} -> ${JSON.stringify(latest[key])}`).join(', ')}; ${command(args)}` };
    return { issue: null };
  } catch (error) {
    return { issue: `run verification failed: ${error.message}; ${command(args)}` };
  }
}

export function collect({ inputsDir = DEFAULT_INPUTS, repo = DEFAULT_REPO, call = gh, maxCalls = MAX_GH_CALLS } = {}) {
  if (typeof repo !== 'string' || !/^[a-z\d][a-z\d-]*\/(?!\.{1,2}$)[a-z\d_.-]+$/i.test(repo)) throw new Error('expected --repo OWNER/REPO');
  if (!Number.isInteger(maxCalls) || maxCalls < 0 || maxCalls > MAX_GH_CALLS) throw new Error(`--max-calls must be an integer from 0 to ${MAX_GH_CALLS}`);
  if (typeof call !== 'function') throw new Error('call must be a synchronous function accepting gh arguments');
  mkdirSync(inputsDir, { recursive: true });
  const metaPath = join(inputsDir, 'collection.json');
  const previous = JSON.parse(readOptional(metaPath) ?? 'null');
  if (previous && (previous.repo.toLowerCase() !== repo.toLowerCase() || previous.since !== SINCE)) {
    throw new Error('inputs directory belongs to a different repository or collection window');
  }

  let calls = 0;
  function request(args) {
    if (calls >= maxCalls) throw new Error('gh call budget exhausted');
    calls++;
    let result;
    try { result = call(args); }
    catch (error) { result = { error }; }
    if (typeof result === 'string') return result;
    if (!result || result.error || result.status !== 0) {
      if (result?.error?.code === 'ENOBUFS') throw new Error(truncated('gh stdout exceeded the capture limit', args));
      const stderr = String(result?.stderr ?? '');
      const http = /\bHTTP\s+([1-5]\d{2})\b/i.exec(stderr)?.[1];
      const code = result?.error?.code;
      const details = [http && `HTTP ${http}`, typeof code === 'string' && /^E[A-Z][A-Z0-9]{0,31}$/.test(code) && code,
        Number.isInteger(result?.status) && `exit ${result.status}`].filter(Boolean);
      const reason = args[0] === 'run' && /no (?:valid )?artifacts|artifact.*not found/i.test(stderr) ?
        'test-selection artifact not found' : 'gh invocation failed';
      throw new Error(`${reason}${details.length ? ` (${details.join(', ')})` : ''}`);
    }
    return String(result.stdout ?? '');
  }

  const root = `repos/${repo}/actions`;
  const listing = paginate(request, page => ['api', `${root}/workflows/ci.yml/runs?created=%3E%3D${SINCE}&per_page=${PAGE_SIZE}&page=${page}`], 'workflow_runs', runIdentity, RUN_LIST_LIMIT);
  const metadata = { repo, since: SINCE, listingComplete: !listing.issue, issues: listing.issue ? [listing.issue] : [] };
  const pendingMetadata = { repo, since: SINCE, listingComplete: false, issues: ['collection not finalized'] };
  if (!previous || listing.issue) writeChanged(metaPath, listing.issue ? metadata : pendingMetadata);
  const priority = run => run.status !== 'completed' ? 3 : run.run_attempt !== 1 ? 2 : run.conclusion === 'failure' ? 0 : 1;
  const work = listing.entries.sort((a, b) => priority(a) - priority(b)).map(run => {
    const id = idOf(run.id);
    const path = join(inputsDir, `${id}.json`);
    const oldText = readOptional(path);
    const old = JSON.parse(oldText ?? 'null');
    const attempt = Number.isSafeInteger(run.run_attempt) && run.run_attempt > 0 ? run.run_attempt : undefined;
    const eligible = run.event === 'pull_request' || (run.event === 'push' && run.head_branch === 'main');
    const record = {
      id, createdAt: run.created_at, event: run.event, headBranch: stringOrNull(run.head_branch), headSha: run.head_sha,
      status: run.status, conclusion: stringOrNull(run.conclusion), selection: null, core: null, coreLookup: 'not-needed', issues: [], attempt,
    };
    const changedRun = !old || ["id", "createdAt", "event", "headBranch", "headSha", "status", "conclusion", "attempt"]
      .some(key => record[key] !== old[key]);
    const prior = !changedRun && attempt !== undefined && old?.collectionCheck === 'stable' &&
      old.evidenceAttempt === attempt ? old : null;
    let lookup = false;
    let download = false;
    if (!eligible) record.issues.push("excluded event or branch");
    else {
      if (attempt === undefined) {
        record.coreLookup = "unavailable";
        record.issues.push("attempt unknown: cannot authorize jobs or selection evidence");
      }
      if (run.status !== "completed") record.issues.push("run not completed");
      else if (!["success", "failure"].includes(record.conclusion)) {
        record.issues.push(`run conclusion is not success or failure: ${record.conclusion}`);
        lookup = attempt > 1;
      } else if (attempt !== undefined) {
        lookup = true;
        download = attempt === 1;
      }
    }
    if (lookup) {
      if (download) record.selection = savedSelection(prior?.selection);
      record.coreLookup = "unavailable";
    }
    return { path, oldText, old, record, prior, lookup, download, changedRun };
  });
  if (!listing.issue && work.some(item => item.changedRun || item.lookup)) writeChanged(metaPath, pendingMetadata);
  let unstable = 0;
  for (const { path, oldText, old, record, prior, download, lookup, changedRun } of work) {
    let observedRed;
    let rerunSnapshots = [];
    let transitioned = changedRun;
    if (lookup) {
      let acquisitionAttempted = false;
      const acquire = args => {
        acquisitionAttempted = true;
        return request(args);
      };
      if (record.attempt > 1) {
        const cachedSelection = savedSelection(prior?.selection);
        const cachedCore = prior?.core && JOB_CONCLUSIONS.has(prior.core.conclusion) ? structuredClone(prior.core) : null;
        if (cachedSelection && cachedCore) {
          record.selection = cachedSelection;
          record.core = cachedCore;
          record.coreLookup = 'available';
        } else rerunSnapshots = resolveRerun(record, acquire, root, repo).snapshots;
      } else {
        const selectorReady = collectJobs(record, prior, acquire, root, download);
        if (download && selectorReady && !record.selection) collectSelection(record, acquire, repo);
      }
      collectLog(record, acquire, root);
      // A matching fresh listing is enough when all evidence came from a guarded cache.
      const { latest, issue } = acquisitionAttempted ? checkCollection(record, request, root) : { issue: null };
      if (issue) {
        unstable++;
        transitioned ||= old?.collectionCheck === 'stable' && old.conclusion === 'failure';
        Object.assign(record, { collectionCheck: 'unstable', selection: null, core: null, coreLookup: 'unavailable' });
        record.issues.push(issue);
        if (latest) {
          if (record.status === 'completed' && record.conclusion === 'failure' &&
              (latest.status !== 'completed' || latest.conclusion !== 'failure' || latest.attempt !== record.attempt)) {
            observedRed = { ...record };
          }
          Object.assign(record, latest);
          transitioned ||= Boolean(old && Object.keys(latest).some(key => record[key] !== old[key]));
        }
      } else {
        record.collectionCheck = 'stable';
        if (record.core || record.selection) record.evidenceAttempt = record.attempt;
      }
    }
    retainHistory(record, old, transitioned, observedRed, rerunSnapshots);
    if (!listing.issue && json(record) !== oldText) writeChanged(metaPath, pendingMetadata);
    writeChanged(path, record, oldText);
  }
  if (unstable) metadata.issues.push(`collection verification unstable for ${unstable} run(s); see run issues`);
  writeChanged(metaPath, metadata);
  return { ...metadata, calls, runs: listing.entries.length };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: { 'inputs-dir': { type: 'string' }, repo: { type: 'string' }, 'max-calls': { type: 'string' } } });
    const raw = values['max-calls'];
    const result = collect({ inputsDir: values['inputs-dir'], repo: values.repo, maxCalls: raw === undefined ? undefined : /^\d+$/.test(raw) ? Number(raw) : NaN });
    console.log(json(result).trimEnd());
    if (!result.listingComplete) process.exitCode = 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
