#!/usr/bin/env node
// P1.5 selection potential: measure over git history how much CI time step
// P1.5 (selection by workspace crate graph) would have saved, as INPUT 1 to
// decision G1 (docs/perf/G1_DECISION_CRITERION.md). Numbers only: the report
// makes NO GO/NO-GO verdict.
//
// Cargo-free, network-free. Reuses scripts/ci-plan.mjs (CRATE_GROUPS, GROUPS,
// plan, parseJobs, cargoTomlBenchOnly, hasFullTrailer) — never re-implements it.
//
// Modes:
//   --report  print the markdown report to stdout AND write it to
//             docs/perf/P1_5_SELECTION_POTENTIAL.md (override: --report-path).
//             Two consecutive generations are byte-identical.
//   --check   compare the committed report with a fresh generation; nonzero
//             exit on any difference.
//
// All file IO is redirectable for tests: --repo-root, --report-path,
// --inputs (run JSON), --window-file (pre-computed window; hermetic mode,
// no git). Production defaults point into the repo.
//
// The computation lives in scripts/ci-selection-history/{manifest,model,report}.mjs.

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { parseJobs } from './ci-plan.mjs';
import { effectiveWorkflow } from './ci/effective-workflow.mjs';
import { parseMemberManifest, parseRootManifest } from './ci-selection-history/manifest.mjs';
import { aggregate, buildConsumers, buildWeightsFromJobs, computeRows, rootFeatureUsage } from './ci-selection-history/model.mjs';
import { METHOD_RULES, renderReport } from './ci-selection-history/report.mjs';

const SCRIPT = 'scripts/ci-selection-history.mjs';
const DEFAULT_REPORT = 'docs/perf/P1_5_SELECTION_POTENTIAL.md';
const DEFAULT_INPUTS = 'docs/perf/p1_5_inputs/full_run_43704208363.json';
const WINDOW_N = 120;
const REF = 'origin/main';

function fail(msg) {
  process.stderr.write(`${SCRIPT}: ${msg}\n`);
  process.exit(2);
}

function parseArgs(argv) {
  const out = { mode: 'report', windowN: WINDOW_N };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = (name) => { const v = argv[++i]; if (v === undefined) fail(`${name} needs a value`); return v; };
    if (a === '--report') out.mode = 'report';
    else if (a === '--check') out.mode = 'check';
    else if (a === '--report-path') out.reportPath = val(a);
    else if (a === '--inputs') out.inputs = val(a);
    else if (a === '--repo-root') out.repoRoot = val(a);
    else if (a === '--window-file') out.windowFile = val(a);
    else if (a === '--window-n') out.windowN = Number(val(a));
    else fail(`unknown argument: ${a}`);
  }
  return out;
}

function git(root, args) {
  const r = spawnSync('git', args, { encoding: 'utf8', cwd: root, maxBuffer: 1 << 26 });
  if (r.status !== 0) throw new Error(`git ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout;
}

/** Git window: the last N first-parent MERGE commits; files = diff vs first parent. */
export function gitWindow(root, n, ref) {
  const shas = git(root, ['log', '--first-parent', '--merges', `-${n}`, '--format=%H %P', ref])
    .split('\n').filter(Boolean).map(l => {
      const [sha, ...parents] = l.split(' ');
      return { sha, p1: parents[0] };
    });
  const commits = shas.map(({ sha, p1 }) => {
    const files = git(root, ['diff', '--name-only', p1, sha]).split('\n').filter(Boolean);
    const cargoTomlDiff = files.includes('Cargo.toml')
      ? git(root, ['diff', '-U0', p1, sha, '--', 'Cargo.toml']) : undefined;
    return { sha, files, cargoTomlDiff };
  });
  const headSha = git(root, ['rev-parse', ref]).trim();
  const baseSha = shas.length ? shas[shas.length - 1].p1 : headSha;
  return { commits, headSha, baseSha, n: commits.length, ref };
}

/** seamTests word-set helper, same construction as ci-plan.mjs's own. */
function seamTestNames(root) {
  const names = new Set();
  for (const f of ['scripts/loom.mjs', 'scripts/miri-matrix.mjs', 'scripts/miri.mjs']) {
    const p = join(root, f);
    if (!existsSync(p)) continue;
    for (const m of readFileSync(p, 'utf8').matchAll(/\b([a-z][a-z0-9_]+)\b/g)) names.add(m[1]);
  }
  return names;
}

export function loadContext(root) {
  const rootToml = readFileSync(join(root, 'Cargo.toml'), 'utf8');
  const rootManifest = parseRootManifest(rootToml);
  const memberManifests = rootManifest.members.map(m =>
    parseMemberManifest(readFileSync(join(root, m, 'Cargo.toml'), 'utf8')));
  const memberNames = new Set(memberManifests.map(m => m.name));
  const memberDirs = new Set(rootManifest.members.map(m => m.split('/').pop()));
  const rootName = rootManifest.packageName;
  const consumers = buildConsumers(rootManifest, rootName, memberManifests, memberNames);
  const expanded = effectiveWorkflow(root);
  const usage = rootFeatureUsage(expanded.text, parseJobs(expanded.text));
  return { rootManifest, memberManifests, memberNames, memberDirs, rootName, consumers, ciText: expanded.text, usage };
}

export function computeAll({ commits, headSha, baseSha, n, ref }, ctx, runJsonPath) {
  const runJson = JSON.parse(readFileSync(runJsonPath, 'utf8'));
  const seamTests = ctx.seamTests;
  const { rows, buckets } = computeRows({
    commits, seamTests, memberDirs: ctx.memberDirs, consumers: ctx.consumers,
    rootManifest: ctx.rootManifest, rootName: ctx.rootName, usage: ctx.usage,
    weights: ctx.weights,
  });
  // every commit appears exactly once
  const seen = new Set();
  for (const r of rows) {
    if (seen.has(r.sha)) throw new Error(`commit ${r.sha} appears more than once`);
    seen.add(r.sha);
  }
  if (rows.length !== commits.length) throw new Error('row count mismatch');
  const reselect = (sha, policy) => {
    const r = rows.find(x => x.sha === sha);
    return { current: [...r.policies[policy].current], p15: [...r.policies[policy].p15] };
  };
  const agg = {
    pull_request: aggregate(rows, 'pull_request', ctx.weights, reselect),
    push: aggregate(rows, 'push', ctx.weights, reselect),
  };
  const weightsInfo = {
    runId: String(runJson.run_id), event: runJson.event, createdAt: runJson.created_at,
    jobCount: runJson.job_count ?? (runJson.jobs?.length ?? 0),
    mapped: ctx.weightsInfo.mapped, unmapped: ctx.weightsInfo.unmapped,
    excluded: ctx.weightsInfo.excluded.length, skipped: ctx.weightsInfo.skipped.length,
    zeroTimestamp: ctx.weightsInfo.zeroTimestamp,
  };
  const report = renderReport({
    window: { n, baseSha, headSha, ref }, method: METHOD_RULES,
    rows, buckets, agg, weights: weightsInfo,
  });
  for (const fraction of [agg.pull_request.shareCommits, agg.pull_request.shareSeconds, agg.push.shareCommits, agg.push.shareSeconds]) {
    if (!(fraction >= 0 && fraction <= 1)) throw new Error('share outside [0,1]');
  }
  return report;
}

function prepare(root, args) {
  const ctx = loadContext(root);
  ctx.seamTests = seamTestNames(root);
  const runJsonPath = args.inputs ?? join(root, DEFAULT_INPUTS);
  const runJson = JSON.parse(readFileSync(runJsonPath, 'utf8'));
  const jobs = parseJobs(ctx.ciText);
  const winfo = buildWeightsFromJobs(runJson, jobs);
  ctx.weights = winfo.weights;
  ctx.weightsInfo = winfo;
  const window = args.windowFile
    ? readWindowFile(args.windowFile)
    : gitWindow(root, args.windowN, REF);
  return { window, ctx, runJsonPath };
}

function readWindowFile(path) {
  const w = JSON.parse(readFileSync(path, 'utf8'));
  return { commits: w.commits, headSha: w.headSha, baseSha: w.baseSha, n: w.commits.length, ref: w.ref ?? '(window file)' };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(args.repoRoot ?? resolve(dirname(fileURLToPath(import.meta.url)), '..'));
  let report;
  try {
    const { window, ctx, runJsonPath } = prepare(repoRoot, args);
    report = computeAll(window, ctx, runJsonPath);
  } catch (e) {
    if (process.env.P15_DEBUG && e.stack) process.stderr.write(e.stack + '\n');
    fail(e.message);
  }
  const reportPath = resolve(args.reportPath ?? join(repoRoot, DEFAULT_REPORT));
  if (args.mode === 'check') {
    let committed;
    try { committed = readFileSync(reportPath, 'utf8'); } catch {
      process.stderr.write(`${SCRIPT}: --check: report file missing: ${args.reportPath ?? DEFAULT_REPORT}\n`);
      process.exit(1);
    }
    if (committed !== report) {
      process.stderr.write(`${SCRIPT}: --check FAILED: ${args.reportPath ?? DEFAULT_REPORT} differs from a fresh generation (run \`node ${SCRIPT} --report\` and commit the result)\n`);
      process.exit(1);
    }
    process.stdout.write(`${SCRIPT}: --check ok (${args.reportPath ?? DEFAULT_REPORT})\n`);
    return;
  }
  mkdirSync(dirname(reportPath), { recursive: true });
  writeFileSync(reportPath, report);
  process.stdout.write(report);
}

// Run main() only when executed directly as the CLI, never on import —
// importing must have no side effects and write nothing (same guard shape as
// scripts/g1-widening-count.mjs: the -e clause covers `node -e import(...)`).
if (
  process.argv[1]
  && !process.execArgv.some((a) => a === '-e' || a === '--eval')
  && pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) main();
