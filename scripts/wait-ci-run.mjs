// Wait for one or more GitHub Actions runs to reach a terminal state, with
// BOUNDED output — a replacement for `gh run watch <id> --exit-status` when
// the watcher is launched as a detached background process (the shape this
// project's post-push "confirm CI went green" step uses, per CLAUDE.md's
// "Then confirm CI went green — do not assume it").
//
// THE PROBLEM THIS EXISTS FOR
//
// `gh run watch` is a TTY-first command. Its refresh loop
// (cli/cli `pkg/cmd/run/watch/watch.go`, `watchRun`) renders the FULL run —
// header, every job, and with `RenderJobs(cs, jobs, true)` every STEP of every
// job — into a buffer, then calls `IO.RefreshScreen()` and copies the buffer to
// stdout, every `--interval` seconds (default 3). `RefreshScreen()` is
// TTY-gated (cli/cli `pkg/iostreams/iostreams.go`: `if s.IsStdoutTTY()` — it
// emits the cursor-home + clear-to-bottom escapes and nothing else), and
// `StartAlternateScreenBuffer()` is gated on `stdoutIsTTY` too. So on a TTY the
// render repaints IN PLACE and stdout stays one screen tall; with stdout
// redirected to a pipe or a file — exactly what a backgrounded launch does —
// nothing clears anything and every refresh is APPENDED. Output then grows
// without bound for the whole life of the run.
//
// Measured against this repo's own CI (run 33304038013, 44 jobs, 32m18s):
// one render is ~23.6 KB / 466 lines (`gh run view <id> --verbose`), so at the
// default 3 s interval that is ~7.9 KB/s ≈ 470 KB/min. Captures of past
// backgrounded watches of this repo's CI bear that out: 1.7 MB to 7.2 MB of
// stdout each, 75 to 323 refreshes.
//
// Second, independent defect in the same command: `watchRun` has NO retry. A
// single transient API error breaks the loop and returns, so the watcher exits
// non-zero on a run that is perfectly healthy. Observed for real in this
// repo's own history, not hypothesised — one capture ends:
//   failed to get jobs: Get "https://api.github.com/repos/.../jobs?per_page=100": unexpected EOF
// followed by the caller's `CI_EXIT=1`. A green run reported as a failure is
// worse than a noisy one.
//
// Ruled out while diagnosing, so nobody re-investigates them:
//   - stdin: `watchRun` never reads stdin when a run-id argument is given
//     (`opts.Prompt` stays false; the only prompt path is the no-argument one,
//     which errors out with "run ID required when not running interactively").
//   - a crash in terminal-size / isatty detection: every TTY-dependent call in
//     the watch path is a plain `if IsStdoutTTY()` guard, and `TerminalWidth()`
//     falls back to a default width on error. Non-TTY is a supported mode; it
//     is just an unbounded-output one.
//   - an internal timeout / retry ceiling / backoff giving up: there is none.
//     The loop is `for run.Status != Completed` and only breaks on an API
//     error (the case above), which surfaces as a non-zero exit with a
//     message, not silence.
//   - a simple output-byte or elapsed-time cap in the launching harness, as
//     the single explanation for the five consecutive backgrounded watches of
//     run 33304038013 that were each reported killed while `gh run view`
//     showed the run still legitimately `in_progress`. Two control probes were
//     run in the same harness: 1.89 MB of stdout over 240 s, and 9.94 MB over
//     105 s — both exited normally; and several of the historical 6.6-7.2 MB
//     watch captures above end with their caller's own `CI_EXIT=0`, i.e. they
//     were not killed either. So the harness's exact kill rule is NOT
//     established. What IS established is that this command is the only
//     long-running one in that session writing megabytes to the capture pipe,
//     and that it dies on its own on a transient API hiccup. Both are fixed by
//     not running it, which is what this script does.
//
// Upstream has no fix available today: `gh run watch` in gh 2.91.0 offers only
// `--compact`, `--exit-status` and `-i/--interval` (`--compact` shrinks each
// render but does not bound the total), and cli/cli#13992 — "Add --quiet
// support to pr checks --watch and run watch", opened 2026-07-28, still open,
// and motivated by this exact coding-agent use case — is the request for the
// flag that would make a wrapper viable. So this is a self-contained poller
// rather than a `gh run watch` wrapper.
//
// WHAT THIS DOES INSTEAD
//
// Polls `gh run view <id> --json status,conclusion,workflowName,url,jobs` on a
// slow interval and prints ONE short line per run per observed state CHANGE,
// plus a liveness line at most every HEARTBEAT_MS so a long silent stretch
// reads as alive rather than hung (the same reasoning as scripts/lib.mjs's
// heartbeat, task #1232). Worst case at the defaults — 45 min ceiling, 20 s
// interval — is a few hundred short lines, kilobytes total, against `gh run
// watch`'s megabytes.
//
// USAGE
//
//   node scripts/wait-ci-run.mjs <run-id> [<run-id> ...] [options]
//   node scripts/wait-ci-run.mjs --sha <commit-sha> [options]
//
//   --sha <commit-sha>    watch every run GitHub started for that commit
//                         (this repo pushes trigger two workflows, "CI" and
//                         "Kani verification", so a push is normally two run
//                         ids — one invocation covers both). Needs the FULL
//                         40-char SHA; `gh run list --commit` matches an
//                         abbreviated SHA against nothing and returns `[]`.
//   --interval <seconds>  poll interval (default 20, minimum 5)
//   --timeout <minutes>   overall ceiling (default 45)
//   --repo <OWNER/REPO>   passed through to gh; default is the repo of the
//                         working tree this script lives in
//   -h, --help
//
// EXIT CODES
//
//   0  every watched run completed with conclusion 'success'
//   1  at least one watched run reached a terminal non-success conclusion
//   2  the ceiling elapsed with at least one run still not completed
//   3  usage error, or `gh` failed repeatedly / matched no runs
//
// The final line is always a single `PASS —`/`FAIL —`/`TIMEOUT —`/`ERROR —`
// summary, so a caller that reads only the tail still gets the verdict.

import { spawn } from 'node:child_process';
import { REPO_ROOT } from './lib.mjs';

/** Poll interval floor. Below this the poller is API-chatty for no benefit. */
const MIN_INTERVAL_S = 5;
const DEFAULT_INTERVAL_S = 20;
/** Overall ceiling. This repo's CI runs ~25-35 min; 45 leaves real headroom. */
const DEFAULT_TIMEOUT_MIN = 45;
/** Print a liveness line at least this often even if nothing changed. */
const HEARTBEAT_MS = 60_000;
/** Kill a single `gh` invocation that has not answered in this long. */
const GH_CALL_TIMEOUT_MS = 60_000;
/** Give up after this many consecutive failed `gh` invocations. */
const MAX_CONSECUTIVE_GH_FAILURES = 5;

const EXIT_OK = 0;
const EXIT_RUN_FAILED = 1;
const EXIT_TIMEOUT = 2;
const EXIT_ERROR = 3;

const USAGE = `usage:
  node scripts/wait-ci-run.mjs <run-id> [<run-id> ...] [options]
  node scripts/wait-ci-run.mjs --sha <commit-sha> [options]

options:
  --sha <commit-sha>    watch every run for that commit (full 40-char SHA)
  --interval <seconds>  poll interval (default ${DEFAULT_INTERVAL_S}, minimum ${MIN_INTERVAL_S})
  --timeout <minutes>   overall ceiling (default ${DEFAULT_TIMEOUT_MIN})
  --repo <OWNER/REPO>   passed through to gh
  -h, --help            this message

exit: 0 all success | 1 a run failed | 2 ceiling elapsed | 3 usage/gh error`;

/** Wall-clock as `m:ss` / `h:mm:ss`, for the per-line elapsed prefix. */
function fmtElapsed(ms) {
  const total = Math.floor(ms / 1000);
  const s = String(total % 60).padStart(2, '0');
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Run `gh` with argv handed straight to the OS (no `shell: true` — same
 * reasoning as scripts/lib.mjs's run(): Node 22+ DEP0190 makes the shell path
 * re-split multi-word arguments). Resolves to { code, stdout, stderr }; a
 * child that outlives GH_CALL_TIMEOUT_MS is killed and reported as a failure
 * rather than stalling the poll loop forever.
 */
function gh(args) {
  return new Promise((resolve) => {
    const child = spawn('gh', args, {
      cwd: REPO_ROOT,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill();
    }, GH_CALL_TIMEOUT_MS);
    child.stdout.on('data', (b) => {
      stdout += b.toString();
    });
    child.stderr.on('data', (b) => {
      stderr += b.toString();
    });
    child.on('error', (err) => {
      clearTimeout(timer);
      resolve({ code: null, stdout, stderr: `${stderr}${err.message}` });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({
        code,
        stdout,
        stderr: timedOut
          ? `${stderr}gh did not answer within ${GH_CALL_TIMEOUT_MS / 1000}s`
          : stderr,
      });
    });
  });
}

function parseArgs(argv) {
  const opts = {
    runIds: [],
    sha: null,
    intervalS: DEFAULT_INTERVAL_S,
    timeoutMin: DEFAULT_TIMEOUT_MIN,
    repo: null,
    help: false,
  };
  const num = (raw, flag) => {
    const n = Number(raw);
    if (!Number.isFinite(n) || n <= 0) throw new Error(`${flag} needs a positive number, got ${JSON.stringify(raw)}`);
    return n;
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '-h' || a === '--help') opts.help = true;
    else if (a === '--sha') opts.sha = argv[++i];
    else if (a === '--interval') opts.intervalS = num(argv[++i], '--interval');
    else if (a === '--timeout') opts.timeoutMin = num(argv[++i], '--timeout');
    else if (a === '--repo') opts.repo = argv[++i];
    else if (/^\d+$/.test(a)) opts.runIds.push(a);
    else throw new Error(`unrecognized argument ${JSON.stringify(a)}`);
  }
  if (opts.sha === undefined || opts.repo === undefined) throw new Error('a flag is missing its value');
  if (opts.intervalS < MIN_INTERVAL_S) {
    throw new Error(`--interval below the ${MIN_INTERVAL_S}s floor: ${opts.intervalS}`);
  }
  return opts;
}

/** `--repo OWNER/REPO` as argv fragment, or nothing (gh infers from cwd). */
const repoArgs = (repo) => (repo ? ['--repo', repo] : []);

/** Resolve `--sha` to the run ids GitHub started for that commit. */
async function runIdsForSha(sha, repo) {
  const r = await gh([
    'run',
    'list',
    '--commit',
    sha,
    '--limit',
    '50',
    '--json',
    'databaseId,workflowName',
    ...repoArgs(repo),
  ]);
  if (r.code !== 0) throw new Error(`gh run list --commit ${sha} failed (exit ${r.code}): ${r.stderr.trim()}`);
  let list;
  try {
    list = JSON.parse(r.stdout);
  } catch {
    throw new Error(`gh run list --commit ${sha} returned unparseable JSON: ${r.stdout.slice(0, 200)}`);
  }
  if (!Array.isArray(list) || list.length === 0) {
    throw new Error(
      `no runs found for commit ${sha} — note --commit needs the FULL 40-char SHA, ` +
        'and a just-pushed commit can take a few seconds before its runs exist',
    );
  }
  return list.map((r2) => String(r2.databaseId));
}

/**
 * One `gh run view` poll. Returns the fields this script reports on, or
 * `{ error }` — the caller decides whether a failure is transient.
 */
async function pollRun(id, repo) {
  const r = await gh([
    'run',
    'view',
    id,
    '--json',
    'status,conclusion,workflowName,url,jobs',
    ...repoArgs(repo),
  ]);
  if (r.code !== 0) return { error: `gh run view ${id} failed (exit ${r.code}): ${r.stderr.trim() || '(no stderr)'}` };
  let j;
  try {
    j = JSON.parse(r.stdout);
  } catch {
    return { error: `gh run view ${id} returned unparseable JSON: ${r.stdout.slice(0, 200)}` };
  }
  const jobs = Array.isArray(j.jobs) ? j.jobs : [];
  return {
    status: j.status ?? '',
    conclusion: j.conclusion ?? '',
    workflowName: j.workflowName ?? `run ${id}`,
    url: j.url ?? '',
    jobsTotal: jobs.length,
    jobsDone: jobs.filter((job) => job.status === 'completed').length,
    running: jobs.filter((job) => job.status !== 'completed').map((job) => job.name),
    failedJobs: jobs
      .filter((job) => job.status === 'completed' && job.conclusion && job.conclusion !== 'success' && job.conclusion !== 'skipped')
      .map((job) => ({ name: job.name, conclusion: job.conclusion, url: job.url })),
  };
}

/** One short status line's worth of state, used both to print and to dedupe. */
function digest(s) {
  const head = `${s.status}${s.conclusion ? `/${s.conclusion}` : ''} jobs ${s.jobsDone}/${s.jobsTotal}`;
  if (s.running.length === 0) return head;
  // Cap the running-job list: a 44-job matrix would otherwise print a
  // multi-hundred-character line on every early poll.
  const shown = s.running.slice(0, 3).join(', ');
  const more = s.running.length > 3 ? ` +${s.running.length - 3} more` : '';
  return `${head} | running: ${shown}${more}`;
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error(`[wait-ci-run] ERROR — ${e.message}\n\n${USAGE}`);
    return EXIT_ERROR;
  }
  if (opts.help) {
    console.log(USAGE);
    return EXIT_OK;
  }
  if (opts.sha && opts.runIds.length > 0) {
    console.error(`[wait-ci-run] ERROR — pass run ids or --sha, not both\n\n${USAGE}`);
    return EXIT_ERROR;
  }

  let ids = opts.runIds;
  if (opts.sha) {
    try {
      ids = await runIdsForSha(opts.sha, opts.repo);
    } catch (e) {
      console.error(`[wait-ci-run] ERROR — ${e.message}`);
      return EXIT_ERROR;
    }
    console.log(`[wait-ci-run] commit ${opts.sha} -> ${ids.length} run(s)`);
  }
  if (ids.length === 0) {
    console.error(`[wait-ci-run] ERROR — no run id given\n\n${USAGE}`);
    return EXIT_ERROR;
  }

  const intervalMs = opts.intervalS * 1000;
  const ceilingMs = opts.timeoutMin * 60_000;
  const startedAt = Date.now();
  console.log(
    `[wait-ci-run] watching ${ids.length} run(s): ${ids.join(', ')} ` +
      `(poll ${opts.intervalS}s, ceiling ${opts.timeoutMin}m)`,
  );

  /** id -> last state we printed, so an unchanged run stays silent. */
  const lastDigest = new Map();
  /** id -> terminal poll result once `status === 'completed'`. */
  const finished = new Map();
  /**
   * id -> when a line was last printed FOR THAT run. Per-run, not global: with
   * a shared timestamp the first run polled each cycle would keep resetting it
   * and a quieter sibling (a `--sha` pair's Kani run next to CI) would never
   * reach its own heartbeat.
   */
  const lastPrintedAt = new Map(ids.map((id) => [id, Date.now()]));
  let consecutiveFailures = 0;

  for (;;) {
    const pending = ids.filter((id) => !finished.has(id));
    let sawFailureThisRound = false;
    for (const id of pending) {
      const s = await pollRun(id, opts.repo);
      if (s.error) {
        sawFailureThisRound = true;
        console.log(`[${fmtElapsed(Date.now() - startedAt)}] transient: ${s.error}`);
        lastPrintedAt.set(id, Date.now());
        continue;
      }
      // A run can briefly report status=completed with a null conclusion;
      // treat that as still-pending and let the next poll settle it.
      if (s.status === 'completed' && s.conclusion) finished.set(id, s);

      const d = digest(s);
      const stale = Date.now() - lastPrintedAt.get(id) >= HEARTBEAT_MS;
      if (lastDigest.get(id) !== d || stale || finished.has(id)) {
        lastDigest.set(id, d);
        lastPrintedAt.set(id, Date.now());
        console.log(`[${fmtElapsed(Date.now() - startedAt)}] ${s.workflowName} (${id}) ${d}`);
      }
    }

    consecutiveFailures = sawFailureThisRound ? consecutiveFailures + 1 : 0;
    if (consecutiveFailures >= MAX_CONSECUTIVE_GH_FAILURES) {
      console.error(
        `[wait-ci-run] ERROR — gh failed ${consecutiveFailures} polls in a row; giving up ` +
          `after ${fmtElapsed(Date.now() - startedAt)}`,
      );
      return EXIT_ERROR;
    }

    if (ids.every((id) => finished.has(id))) break;

    if (Date.now() - startedAt + intervalMs > ceilingMs) {
      const stuck = ids.filter((id) => !finished.has(id));
      console.error(
        `[wait-ci-run] TIMEOUT — ceiling of ${opts.timeoutMin}m elapsed with ` +
          `${stuck.length} run(s) still not completed: ${stuck.join(', ')} ` +
          '(the run may still be healthy — re-run with a larger --timeout)',
      );
      return EXIT_TIMEOUT;
    }
    await sleep(intervalMs);
  }

  const elapsed = fmtElapsed(Date.now() - startedAt);
  const bad = ids.filter((id) => finished.get(id).conclusion !== 'success');
  for (const id of ids) {
    const s = finished.get(id);
    console.log(`[wait-ci-run] ${s.workflowName} (${id}) concluded '${s.conclusion}' — ${s.url}`);
    for (const job of s.failedJobs) {
      console.log(`[wait-ci-run]   failed job: ${job.name} [${job.conclusion}] — ${job.url}`);
    }
  }
  if (bad.length > 0) {
    console.error(
      `[wait-ci-run] FAIL — ${bad.length} of ${ids.length} run(s) did not succeed after ${elapsed} ` +
        `(logs: gh run view ${bad[0]} --log-failed)`,
    );
    return EXIT_RUN_FAILED;
  }
  console.log(`[wait-ci-run] PASS — all ${ids.length} run(s) completed with 'success' after ${elapsed}`);
  return EXIT_OK;
}

process.exitCode = await main();
