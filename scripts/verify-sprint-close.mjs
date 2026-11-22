#!/usr/bin/env node
// Sprint-close gate (docs/CI_TIERS.md): a sprint closes on a commit that carries the explicit `CI: full`
// trailer AND has a green FULL CI run on exactly that SHA. A run counts as full when every job gated by
// the tiered groups (always, fast, seam, os, crate_*) ran and succeeded; only the weekly-only and
// dispatch-only jobs may be skipped.
//
// Usage:
//   node scripts/verify-sprint-close.mjs <sha>      read the runs of <sha> from GitHub (needs `gh`)
//   node scripts/verify-sprint-close.mjs --selftest
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { hasFullTrailer, parseJobs } from './ci-plan.mjs';

const OPTIONAL_GROUPS = new Set(['weekly', 'dispatch']);

/** Regex-escape every regex metacharacter in a literal job-name fragment. */
const escapeLit = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Matcher for one declared job name. When the declaration contains complete
 * `${{ ... }}` placeholders, each expression becomes a nonempty wildcard and
 * every literal fragment (including punctuation and whitespace) is matched
 * verbatim; the whole name must match. Static declarations match exactly.
 * @returns {(apiName: string) => boolean}
 */
export function jobMatcher(declaredName) {
  if (!/\$\{\{.*?\}\}/.test(declaredName)) return n => n === declaredName;
  let re = '^';
  for (let rest = declaredName; rest.length;) {
    const i = rest.indexOf('${{');
    if (i < 0) { re += escapeLit(rest); break; }
    re += escapeLit(rest.slice(0, i));
    const j = rest.indexOf('}}', i + 3);
    if (j < 0) return n => n === declaredName; // unbalanced: fall back to exact
    re += '.+?';
    rest = rest.slice(j + 2);
  }
  const rx = new RegExp(re + '$');
  return n => rx.test(n);
}

/**
 * @param declared  parseJobs(ci.yml): [{ id, name, group }]
 * @param apiJobs   [{ name, conclusion }] of one workflow run
 * @returns {{ ok: boolean, problems: string[] }}
 */
export function fullRunVerdict(declared, apiJobs) {
  const problems = [];
  const bad = apiJobs.filter(j => j.conclusion === 'failure' || j.conclusion === 'cancelled' || j.conclusion === 'timed_out');
  for (const j of bad) problems.push(`job "${j.name}" ${j.conclusion}`);
  const optional = declared.filter(d => OPTIONAL_GROUPS.has(d.group)).map(d => jobMatcher(d.name || d.id));
  const isOptional = n => optional.some(match => match(n));
  for (const d of declared) {
    if (OPTIONAL_GROUPS.has(d.group)) continue;
    const match = jobMatcher(d.name || d.id);
    const mine = apiJobs.filter(j => match(j.name));
    if (mine.length === 0) problems.push(`no job ran for "${d.name || d.id}" (group ${d.group})`);
    else if (!mine.some(j => j.conclusion === 'success')) problems.push(`job "${d.name || d.id}" (group ${d.group}) did not succeed: ${mine.map(j => j.conclusion).join(',')}`);
  }
  for (const j of apiJobs) {
    if (j.conclusion === 'skipped' && !isOptional(j.name)) problems.push(`job "${j.name}" was skipped, so the run was not full`);
  }
  return { ok: problems.length === 0, problems };
}

function gh(args) {
  const r = spawnSync('gh', args, { encoding: 'utf8', maxBuffer: 1 << 26 });
  if (r.status !== 0) throw new Error(`gh ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout;
}

function selftest() {
  // Mirror of the real ci.yml shape: the three matrix jobs declare complete
  // `${{ ... }}` placeholders in their names; every other job name is static.
  const ci = [
    'jobs:', '  changes:', '    name: plan', '  fmt:', '    name: fmt',
    '  test:', '    needs: changes', "    if: needs.changes.outputs.fast == 'true'", '    name: test (${{ matrix.label }})',
    '  miri:', '    needs: changes', "    if: needs.changes.outputs.seam == 'true'", '    name: miri',
    '  check-matrix:', '    needs: changes', "    if: needs.changes.outputs.fast == 'true'",
    '    name: check-matrix (shard ${{ matrix.shard }}/${{ strategy.job-total }})',
    '  arch:', '    needs: changes', "    if: needs.changes.outputs.os == 'true'", '    name: cross (${{ matrix.target }})',
    '  fuzz-run:', "    if: ${{ github.event.schedule == '0 6 * * 1' }}", '    name: fuzz run',
  ].join('\n');
  const declared = parseJobs(ci);
  let n = 0;
  const eq = (a, b, m) => { assert.deepEqual(a, b, m); n++; };
  const J = (name, conclusion = 'success') => ({ name, conclusion });
  const green = [
    J('plan'), J('fmt'), J('test (core tiers)'), J('miri'),
    J('check-matrix (shard 1/3)'), J('check-matrix (shard 2/3)'), J('check-matrix (shard 3/3)'),
    J('cross (a)'), J('cross (b)'), J('fuzz run', 'skipped'),
  ];
  eq(fullRunVerdict(declared, green).ok, true, 'a full green run: all matrix shards + legs, weekly job skipped');
  eq(fullRunVerdict(declared, green.filter(j => !j.name.startsWith('check-matrix (shard '))).problems, ['no job ran for "check-matrix (shard ${{ matrix.shard }}/${{ strategy.job-total }})" (group fast)'], 'zero check-matrix legs: exactly one named missing-job diagnostic');
  const oneRed = green.map(j => (j.name === 'check-matrix (shard 2/3)' ? J('check-matrix (shard 2/3)', 'failure') : j));
  eq(fullRunVerdict(declared, oneRed), { ok: false, problems: ['job "check-matrix (shard 2/3)" failure'] }, 'one red leg: ok=false and exactly one failed-job diagnosis');
  eq(fullRunVerdict(declared, green.filter(j => j.name !== 'miri').concat(J('miri', 'skipped'))).ok, false, 'a skipped seam job is not full');
  eq(fullRunVerdict(declared, green.filter(j => j.name !== 'miri')).problems.length, 1, 'a missing job is named');
  const shardSkip = green.map(j => (j.name === 'check-matrix (shard 1/3)' ? J('check-matrix (shard 1/3)', 'skipped') : j));
  eq(fullRunVerdict(declared, shardSkip).ok, false, 'a skipped required shard is not full');
  eq(fullRunVerdict(declared, shardSkip).problems.includes('job "check-matrix (shard 1/3)" was skipped, so the run was not full'), true, 'skipped shard diagnosed as not optional');
  eq(fullRunVerdict(declared, green.concat(J('extra', 'failure'))).ok, false, 'a failed job fails the run');
  eq(fullRunVerdict(declared, [J('plan'), J('fmt'), J('test (core tiers)'), J('check-matrix (shard 1/3)'), J('check-matrix (shard 2/3)'), J('check-matrix (shard 3/3)')]).ok, false, 'a docs-sized run is not full');
  eq(fullRunVerdict(declared, green.concat(J('fmt (extra)')).map(j => (j.name === 'fmt' ? null : j)).filter(Boolean))
    .problems, ['no job ran for "fmt" (group always)'], 'near-prefix "fmt (extra)" is not "fmt": exactly one missing diagnostic');
  // The matcher must not let one leg satisfy another declaration.
  eq(fullRunVerdict(declared, green.map(j => j.name === 'cross (a)' ? J('cross (a)', 'cancelled') : j)).ok, false, 'cancelled cross leg fails, not shadowed by a test leg');
  // anchored whole-name matching: no accidental suffix swallowing.
  eq(jobMatcher('miri')('miri (extra)'), false, 'static declaration is exact');
  eq(jobMatcher('cross (${{ matrix.target }})')('cross (x)'), true, 'one expression placeholder');
  eq(jobMatcher('check-matrix (shard ${{ matrix.shard }}/${{ strategy.job-total }})')('check-matrix (shard 2/3)'), true, 'two adjacent placeholders with literal separator');
  eq(jobMatcher('check-matrix (shard ${{ matrix.shard }}/${{ strategy.job-total }})')('check-matrix (shard 2/3) '), false, 'anchored: trailing whitespace not swallowed');
  eq(fullRunVerdict([{ id: 'test', name: 'test', group: 'fast' }], [J('test impact map')]),
    { ok: false, problems: ['no job ran for "test" (group fast)'] }, 'a static declaration is not satisfied by an impact-mapped name');
  eq(hasFullTrailer('x\n\nCI: full\n'), true, 'trailer helper is shared with the planner');
  console.log(JSON.stringify({ version: 1, status: 'PASS', scope: 'verify-sprint-close selftest', checks: n }));
}

function main() {
  if (process.argv.includes('--selftest')) return selftest();
  const sha = process.argv[2];
  if (!sha || sha.startsWith('-')) {
    console.error('usage: node scripts/verify-sprint-close.mjs <sha> | --selftest');
    process.exitCode = 2;
    return;
  }
  const full = spawnSync('git', ['rev-parse', sha], { encoding: 'utf8' }).stdout.trim();
  if (!/^[0-9a-f]{40}$/.test(full)) { console.error(`cannot resolve ${sha}`); process.exitCode = 2; return; }
  const message = spawnSync('git', ['log', '-1', '--format=%B', full], { encoding: 'utf8' }).stdout;
  const problems = [];
  if (!hasFullTrailer(message)) problems.push('the closing commit has no `CI: full` trailer');
  const declared = parseJobs(readFileSync('.github/workflows/ci.yml', 'utf8'));
  const runs = JSON.parse(gh(['run', 'list', '--workflow', 'ci.yml', '--commit', full, '--json', 'databaseId,event,conclusion,status', '--limit', '30']));
  const done = runs.filter(r => r.status === 'completed' && r.conclusion === 'success');
  let proof = null;
  for (const r of done) {
    const jobs = JSON.parse(gh(['run', 'view', String(r.databaseId), '--json', 'jobs'])).jobs;
    if (fullRunVerdict(declared, jobs).ok) { proof = r; break; }
  }
  if (!proof) problems.push(`no green full CI run on ${full} (${runs.length} run(s) seen, ${done.length} green)`);
  if (problems.length) {
    console.error(`verify-sprint-close: ${full.slice(0, 8)} is NOT a valid sprint close:\n- ${problems.join('\n- ')}`);
    process.exitCode = 1;
    return;
  }
  console.log(`verify-sprint-close: ${full.slice(0, 8)} carries \`CI: full\` and run ${proof.databaseId} (${proof.event}) is a green full run`);
}

if (process.argv[1]?.endsWith('verify-sprint-close.mjs')) main();
