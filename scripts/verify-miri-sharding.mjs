// Mechanical equivalence proof for the miri job sharding (2026-11-16).
//
// Contract: the union of `cargo miri test` commands of the pre-sharding jobs
// `miri-core`, `miri-fastbin`, `miri-plain` must equal — as a normalized
// multiset of (target, package, features, MIRIFLAGS, post-`--` args,
// provenance model, borrows) records — the union of their sharding
// replacements (`miri-core-pin-cache`, `miri-core-rest`,
// `miri-fastbin-oracles`, `miri-fastbin-integrity`, `miri-plain-heavy`,
// `miri-plain-rest`). Per old job, the union of its two shards must equal
// that old job's own record set. Nothing may be dropped, added, re-flagged
// or re-ordered semantically; splitting one multi-target cargo invocation
// into several invocations with identical flags/features/args is allowed
// (the normalized records are identical).
//
// (`--snapshot <git-ref>` regenerates the fixture; the checked-in copy lives
// at tests/miri_sharding_before_19cbc5f8.json). No Cargo, no network on the
// verify path; the snapshot path only writes the fixture file.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { effectiveWorkflow } from './ci/effective-workflow.mjs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export const OLD_JOBS = ['miri-core', 'miri-fastbin', 'miri-plain'];
export const NEW_JOBS = ['miri-core-pin-cache', 'miri-core-rest', 'miri-fastbin-oracles',
  'miri-fastbin-integrity', 'miri-plain-heavy', 'miri-plain-rest'];
export const SHARD_GROUPS = { 'miri-core': ['miri-core-pin-cache', 'miri-core-rest'],
  'miri-fastbin': ['miri-fastbin-oracles', 'miri-fastbin-integrity'],
  'miri-plain': ['miri-plain-heavy', 'miri-plain-rest'] };
const MIRI_SHARD_ACTIONS = NEW_JOBS.map((id) => `.github/actions/ci-${id}/action.yml`);

function shellTokens(s) {
  const re = /"([^"\n]*)"|'([^'\n]*)'|([^\s]+)/g;
  return [...s.matchAll(re)].map((m) => m[1] ?? m[2] ?? m[3]);
}
const tokens = (s) => String(s ?? '').split(/[\s,]+/).filter(Boolean);

// One normalized record per (command × explicit --test target), mirroring the
// identity rules of scripts/verify-miri-target-wiring.mjs: package, features
// (order-insensitive), post-`--` args, provenance model and borrows derived
// from the effective MIRIFLAGS (step-level env overrides the job-level env).
export function recordsForJob(jobBody, inheritedFlags) {
  const stepsAt = jobBody.indexOf('    steps:');
  if (stepsAt < 0) throw new Error('unsupported steps');
  const inherited = jobBody.slice(0, stepsAt).match(/MIRIFLAGS:\s*"([^"]*)"/)?.[1] ?? inheritedFlags ?? '';
  const steps = [...jobBody.matchAll(/^      - /gm)];
  const records = [];
  for (let k = 0; k < steps.length; k++) {
    const step = jobBody.slice(steps[k].index, steps[k + 1]?.index ?? jobBody.length);
    const flags = step.match(/MIRIFLAGS:\s*"([^"]*)"/)?.[1] ?? inherited;
    const run = step.match(/^\s*(?:- )?run:\s*([^\n]*)\n?/m);
    if (!run) continue;
    let script = run[1].length >= 2 && run[1].startsWith("'") && run[1].endsWith("'")
      ? run[1].slice(1, -1).replace(/''/g, "'") : run[1];
    if (/^[|>][-+]?\s*$/.test(script)) script = step.slice(run.index + run[0].length).split('\n').filter((s) => /^          /.test(s)).join('\n');
    script = script.replace(/\\\s*\n/g, ' ');
    if (run[1].startsWith('>')) script = script.replace(/\n/g, ' ');
    for (const command of script.matchAll(/cargo\s+(?:\+\S+\s+)?miri\s+test\s+([^\n]*?)(?=\s+2>&1|\s+\||\n|$)/g)) {
      const argv = shellTokens(command[1]);
      const tests = [], features = [];
      let pkg, args = [], locked = false;
      for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--') { args = argv.slice(i + 1); break; }
        if (a === '--test') tests.push(argv[++i]);
        else if (a === '--features' || a === '-F') features.push(...tokens(argv[++i]));
        else if (a === '-p' || a === '--package') pkg = argv[++i];
        else if (a === '--locked') locked = true;
        else throw new Error(`unsupported Cargo argument ${a}`);
      }
      for (const test of tests) records.push(JSON.stringify({
        test, pkg: pkg ?? '', features: [...new Set(features)].sort(), args,
        model: flags.includes('-Zmiri-strict-provenance') ? 'strict' : 'plain',
        borrows: flags.includes('-Zmiri-tree-borrows') ? 'tb' : 'sb', flags, locked,
      }));
    }
  }
  return records.sort();
}

/** Per-job normalized record sets from an expanded workflow text. */
export function jobRecords(workflowText) {
  const jobs = [...workflowText.matchAll(/^  ([\w-]+):\s*$/gm)];
  const out = {};
  for (let j = 0; j < jobs.length; j++) {
    const id = jobs[j][1];
    if (![...OLD_JOBS, ...NEW_JOBS].includes(id)) continue;
    const body = workflowText.slice(jobs[j].index, jobs[j + 1]?.index ?? workflowText.length);
    out[id] = recordsForJob(body);
  }
  return out;
}

const sortedUnion = (lists) => lists.flat().sort();
const equalSets = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
const diff = (a, b) => {
  const bc = new Map();
  for (const x of b) bc.set(x, (bc.get(x) ?? 0) + 1);
  for (const x of a) bc.set(x, (bc.get(x) ?? 0) - 1);
  return [...bc].filter(([, n]) => n !== 0);
};

export function verifySharding(beforeRecords, afterText) {
  const failures = [];
  const fail = (s) => failures.push(s);
  const before = beforeRecords, after = jobRecords(afterText);
  for (const id of OLD_JOBS) if (!before[id]) fail(`before workflow: job ${id} not found`);
  for (const id of NEW_JOBS) if (!after[id]) fail(`after workflow: shard job ${id} not found`);
  if (failures.length) return { ok: false, failures, before, after };
  const oldUnion = sortedUnion(OLD_JOBS.map((id) => before[id]));
  const newUnion = sortedUnion(NEW_JOBS.map((id) => after[id]));
  if (!equalSets(oldUnion, newUnion)) {
    for (const [record, n] of diff(oldUnion, newUnion)) {
      fail(`record set mismatch (${n > 0 ? 'missing after' : 'new after'}): ${record}`);
    }
  }
  for (const [oldId, shards] of Object.entries(SHARD_GROUPS)) {
    const shardUnion = sortedUnion(shards.map((id) => after[id]));
    if (!equalSets(before[oldId], shardUnion)) {
      for (const [record, n] of diff(before[oldId], shardUnion)) {
        fail(`${oldId} vs [${shards.join(', ')}] mismatch (${n > 0 ? 'missing' : 'extra'}): ${record}`);
      }
    }
    const overlap = shards.filter((id) => after[id].some((r) => shardUnion.indexOf(r) !== shardUnion.lastIndexOf(r)));
    if (overlap.length) fail(`duplicate record across shards of ${oldId}: ${overlap.join(', ')}`);
  }
  return { ok: failures.length === 0, failures, before, after };
}

function gitShow(refPath) {
  const r = spawnSync('git', ['show', `${refPath}`], { encoding: 'utf8', maxBuffer: 1 << 26, cwd: root });
  if (r.status !== 0) throw new Error(`git show ${refPath} failed: ${r.stderr}`);
  return r.stdout;
}

function fullSha(ref) {
  const r = spawnSync('git', ['rev-parse', ref], { encoding: 'utf8', cwd: root });
  if (r.status !== 0) throw new Error(`git rev-parse ${ref} failed: ${r.stderr}`);
  return r.stdout.trim();
}

/** Reconstruct the pre-sharding expanded workflow text from a git ref. */
export function buildBeforeText(ref) {
  const beforeCi = gitShow(`${ref}:.github/workflows/ci.yml`).replace(/\r\n/g, '\n');
  const beforeActions = OLD_JOBS.map((id) => gitShow(`${ref}:.github/actions/ci-${id}/action.yml`).replace(/\r\n/g, '\n'));
  // Inline each local miri action body the same way effectiveWorkflow does
  // (two-space indent shift is irrelevant to recordsForJob, which keys on the
  // step/run/MIRIFLAGS structure; but keep it identical for exactness).
  let text = beforeCi;
  for (let i = 0; i < OLD_JOBS.length; i++) {
    const call = `      - uses: ./.github/actions/ci-${OLD_JOBS[i]}`;
    const at = text.indexOf(call);
    if (at < 0) throw new Error(`pre-sharding ci.yml at ${ref}: action call for ${OLD_JOBS[i]} not found`);
    const body = beforeActions[i].split('\n').slice(4).map((l) => (l ? `  ${l}` : l)).join('\n');
    text = text.slice(0, at) + body.replace(/\n$/, '') + text.slice(at + call.length);
  }
  return text;
}

export const FIXTURE_PATH = resolve(root, 'tests', 'miri_sharding_before_19cbc5f8.json');
const FIXTURE_SCHEMA = 'miri-sharding-before/v1';

/** Snapshot the before-record sets from a git ref into the fixture JSON. */
export function snapshotFixture(ref) {
  const records = jobRecords(buildBeforeText(ref));
  for (const id of OLD_JOBS) if (!records[id]?.length) throw new Error(`snapshot at ${ref}: job ${id} produced no records`);
  return {
    schema: FIXTURE_SCHEMA,
    ref: fullSha(ref),
    source: [
      '.github/workflows/ci.yml',
      ...OLD_JOBS.map((id) => `.github/actions/ci-${id}/action.yml`),
    ],
    regeneratedBy: `node scripts/verify-miri-sharding.mjs --snapshot ${ref} --out <fixture>`,
    records,
  };
}

function validateFixture(fixture) {
  if (fixture.schema !== FIXTURE_SCHEMA) throw new Error(`fixture schema ${fixture.schema} != ${FIXTURE_SCHEMA}`);
  if (!fixture.ref || !/^[0-9a-f]{40}$/.test(fixture.ref)) throw new Error('fixture ref must be a full 40-char SHA');
  for (const id of OLD_JOBS) {
    const recs = fixture.records?.[id];
    if (!Array.isArray(recs) || recs.length === 0) throw new Error(`fixture: missing records for old job ${id}`);
    if (recs.some((r) => typeof r !== 'string' || !r.startsWith('{'))) throw new Error(`fixture: bad record in ${id}`);
  }
}

function parseArgs(argv) {
  const get = (name) => { const i = argv.indexOf(name); return i >= 0 ? argv[i + 1] : undefined; };
  return { snapshot: get('--snapshot'), out: get('--out') };
}

export function main() {
  const { snapshot, out } = parseArgs(process.argv.slice(2));
  if (snapshot) {
    const fixture = snapshotFixture(snapshot);
    const target = out ? resolve(root, out) : FIXTURE_PATH;
    writeFileSync(target, JSON.stringify(fixture, null, 2) + '\n');
    console.log(`[verify-miri-sharding] snapshot of ${snapshot} (${fixture.ref}) written to ${target}`);
    for (const id of OLD_JOBS) console.log(`[verify-miri-sharding]   ${id}: ${fixture.records[id].length} records`);
    return;
  }
  const fixture = JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'));
  validateFixture(fixture);
  const expanded = effectiveWorkflow(root);
  const result = verifySharding(fixture.records, expanded.text);
  const covered = Object.keys(result.before).length + Object.keys(result.after).length;
  let total = 0;
  for (const [id, records] of Object.entries(result.after)) { console.log(`[verify-miri-sharding] ${id}: ${records.length} records`); total += records.length; }
  const oldTotal = OLD_JOBS.reduce((n, id) => n + (result.before[id]?.length ?? 0), 0);
  console.log(`[verify-miri-sharding] before: ${oldTotal} records across ${OLD_JOBS.length} old jobs; after: ${total} records across ${NEW_JOBS.length} shard jobs`);
  for (const s of result.failures) console.log(`[verify-miri-sharding] RED: ${s}`);
  console.log(`[verify-miri-sharding] ${result.ok ? 'OK: sharded record sets are identical to the pre-sharding jobs' : `FAIL: ${result.failures.length} defects`}`);
  process.exitCode = result.ok ? 0 : 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
