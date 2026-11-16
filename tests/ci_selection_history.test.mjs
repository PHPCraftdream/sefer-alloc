// Offline, hermetic node --test suite for scripts/ci-selection-history.mjs.
// Everything runs on synthetic fixtures in fresh mkdtemp dirs (style of
// tests/g1_widening_count.test.mjs); no git history, no network, no cargo,
// and NO tracked file is ever written — the CLI is only invoked with all
// paths redirected into the temp dir.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'node:test';

import { parseRootManifest, parseMemberManifest, closureDeps, featuresEnabling } from '../scripts/ci-selection-history/manifest.mjs';
import {
  ROOT_GROUPS, affectedSet, aggregate, buildConsumers, buildWeightsFromJobs,
  computeRows, groupSeconds, rootFeatureUsage, rootGroupBuilds, selectP15,
} from '../scripts/ci-selection-history/model.mjs';
import { renderReport, METHOD_RULES } from '../scripts/ci-selection-history/report.mjs';

const repoRoot = resolve(dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..');
const SCRIPT = join(repoRoot, 'scripts', 'ci-selection-history.mjs');

// ── fixtures ────────────────────────────────────────────────────────────────
const ROOT_TOML = `\
[package]
name = "sefer-alloc"

[workspace]
members = ["crates/alpha/widget", "crates/beta/gadget"]
resolver = "2"

[features]
default = ["std"]
std = []
alloc-core = ["std", "dep:widget"]

[dependencies]
gadget = { path = "crates/beta/gadget" }
widget = { path = "crates/alpha/widget", version = "0.1", optional = true }

[dev-dependencies]
devcrate = { path = "crates/alpha/devcrate" }

[target.'cfg(any(loom, kani))'.dependencies]
cfgcrate = { path = "crates/beta/cfgcrate", version = "0.1" }
`;
const WIDGET_TOML = `\
[package]
name = "widget"

[dependencies]
gadget = { path = "../gadget", optional = true }
`;
const GADGET_TOML = `\
[package]
name = "gadget"
`;

const CI_YML = `\
name: CI
on:
  push:
    branches: [main]
jobs:
  changes:
    name: plan (which job groups this change needs)
    runs-on: ubuntu-latest
    steps:
      - run: node scripts/ci-plan.mjs
  clippy:
    needs: changes
    if: needs.changes.outputs.fast == 'true'
    name: clippy
    runs-on: ubuntu-latest
    steps:
      - run: cargo clippy --features alloc-core -- -D warnings
  miri:
    needs: changes
    if: needs.changes.outputs.seam == 'true'
    name: miri (core)
    runs-on: ubuntu-latest
    steps:
      - run: cargo miri test
  msrv:
    needs: changes
    if: needs.changes.outputs.os == 'true'
    name: MSRV job
    runs-on: ubuntu-latest
    steps:
      - run: cargo check --features std
      - run: # comment mentioning --all-features must not forge usage
  docs-consistency:
    name: docs consistency (always)
    runs-on: ubuntu-latest
    steps:
      - run: node scripts/check.mjs
  weekly:
    if: github.event.schedule == '0 6 * * 1'
    name: weekly thing
    runs-on: ubuntu-latest
    steps:
      - run: cargo hack
`;

// seconds: fast=clippy 100, seam=miri 300, os=msrv 50.
const RUN_JSON = {
  run_id: 42, event: 'schedule', created_at: '2026-01-01T00:00:00Z', job_count: 7,
  jobs: [
    { id: 1, name: 'plan (which job groups this change needs)', conclusion: 'success', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:00:10Z' },
    { id: 2, name: 'clippy', conclusion: 'success', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:01:40Z' },
    { id: 3, name: 'miri (core)', conclusion: 'success', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:05:00Z' },
    { id: 4, name: 'MSRV job', conclusion: 'success', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:00:50Z' },
    { id: 5, name: 'docs consistency (always)', conclusion: 'success', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:00:20Z' },
    { id: 6, name: 'weekly thing', conclusion: 'skipped', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:00:00Z' },
    { id: 7, name: 'mystery job', conclusion: 'success', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:02:00Z' },
  ],
};

function writeTree(tmp) {
  for (const [rel, content] of [
    ['Cargo.toml', ROOT_TOML],
    ['crates/alpha/widget/Cargo.toml', WIDGET_TOML],
    ['crates/beta/gadget/Cargo.toml', GADGET_TOML],
    ['.github/workflows/ci.yml', CI_YML],
    ['run.json', JSON.stringify(RUN_JSON)],
  ]) {
    mkdirSync(dirname(join(tmp, rel)), { recursive: true });
    writeFileSync(join(tmp, rel), content);
  }
}

function fixture() {
  const tmp = mkdtempSync(join(tmpdir(), 'p15-selection-test-'));
  writeTree(tmp);
  return tmp;
}

function context(tmp) {
  const rootManifest = parseRootManifest(readFileSync(join(tmp, 'Cargo.toml'), 'utf8'));
  const widget = parseMemberManifest(readFileSync(join(tmp, 'crates/alpha/widget/Cargo.toml'), 'utf8'));
  const gadget = parseMemberManifest(readFileSync(join(tmp, 'crates/beta/gadget/Cargo.toml'), 'utf8'));
  const memberNames = new Set(['widget', 'gadget']);
  const memberDirs = new Set(['widget', 'gadget']);
  const consumers = buildConsumers(rootManifest, 'sefer-alloc', [widget, gadget], memberNames);
  const ciText = readFileSync(join(tmp, '.github/workflows/ci.yml'), 'utf8');
  const usage = rootFeatureUsage(ciText, ciPlan.parseJobs(ciText));
  const runJson = JSON.parse(readFileSync(join(tmp, 'run.json'), 'utf8'));
  const w = buildWeightsFromJobs(runJson, ciPlan.parseJobs(ciText));
  return { tmp, rootManifest, memberManifests: [widget, gadget], memberNames, memberDirs, rootName: 'sefer-alloc', consumers, ciText, usage, weights: w.weights, winfo: w };
}

import * as ciPlan from '../scripts/ci-plan.mjs';

const SEAM = new Set(['some_miri_test']);
const COMMITS = [
  { sha: 'a'.repeat(40), files: ['docs/guide.md'] },
  { sha: 'b'.repeat(40), files: ['crates/alpha/widget/src/lib.rs'] },
  { sha: 'c'.repeat(40), files: ['src/lib.rs', 'Cargo.lock'] }, // not eligible
];

function rowsFor(ctx, commits = COMMITS) {
  return computeRows({
    commits, seamTests: SEAM, memberDirs: ctx.memberDirs, consumers: ctx.consumers,
    rootManifest: ctx.rootManifest, rootName: ctx.rootName, usage: ctx.usage, weights: ctx.weights,
  });
}

// ── manifest parsing ────────────────────────────────────────────────────────
test('manifest parse finds the optional member consumer edge', () => {
  const tmp = fixture();
  const widget = parseMemberManifest(readFileSync(join(tmp, 'crates/alpha/widget/Cargo.toml'), 'utf8'));
  assert.deepEqual(widget.deps.get('gadget'), { path: '../gadget', optional: true });
  const root = parseRootManifest(readFileSync(join(tmp, 'Cargo.toml'), 'utf8'));
  assert.equal(root.packageName, 'sefer-alloc');
  assert.ok(root.deps.get('widget').optional && !root.deps.get('gadget').optional, 'optional flags');
  assert.deepEqual([...root.cfgDeps.keys()], ['cfgcrate'], 'cfg(loom/kani) dep table');
  assert.ok(root.devDeps.has('devcrate'));
  rmSync(tmp, { recursive: true, force: true });
});

test('feature closure maps a feature to its dep crate', () => {
  const tmp = fixture();
  const root = parseRootManifest(readFileSync(join(tmp, 'Cargo.toml'), 'utf8'));
  const depNames = new Set([...root.deps.keys()]);
  assert.deepEqual([...closureDeps(['alloc-core'], root.features, depNames)].sort(), ['widget']);
  assert.ok(featuresEnabling('widget', root.features, depNames).has('alloc-core'));
  assert.deepEqual([...closureDeps(['std'], root.features, depNames)].sort(), []);
  rmSync(tmp, { recursive: true, force: true });
});

// ── consumer graph ──────────────────────────────────────────────────────────
test('affected set = changed crates plus transitive consumers (optional consumer flagged in manifest, included in set)', () => {
  const ctx = context(fixture());
  assert.deepEqual(affectedSet(['gadget'], ctx.consumers), ['gadget', 'sefer-alloc', 'widget']);
  assert.deepEqual(affectedSet(['widget'], ctx.consumers), ['sefer-alloc', 'widget']);
});

// ── rootGroupBuilds ─────────────────────────────────────────────────────────
test('rootGroupBuilds: feature-enabled optional dep -> exactly the groups whose jobs enable the feature; mandatory -> all; dev/cfg -> conservative all + flag', () => {
  const ctx = context(fixture());
  const depNames = new Set([...ctx.rootManifest.deps.keys()]);
  // usage: fast (clippy) and os (msrv) pass --features alloc-core; seam does not.
  assert.deepEqual([...ctx.usage.get('fast').tokens].sort(), ['alloc-core']);
  assert.equal(ctx.usage.get('seam').tokens.size, 0, 'comment line forges no token');
  const rWidget = rootGroupBuilds('widget', ctx.rootManifest, ctx.usage);
  assert.deepEqual([...rWidget.groups].sort(), ['fast'], 'groups whose run lines enable alloc-core (clippy)');
  assert.equal(rWidget.flag, null);
  const rGadget = rootGroupBuilds('gadget', ctx.rootManifest, ctx.usage);
  assert.deepEqual([...rGadget.groups].sort(), [...ROOT_GROUPS].sort(), 'mandatory normal dep built by ALL root groups');
  for (const c of ['devcrate', 'cfgcrate']) {
    const r = rootGroupBuilds(c, ctx.rootManifest, ctx.usage);
    assert.deepEqual([...r.groups].sort(), [...ROOT_GROUPS].sort(), `${c}: conservative all`);
    assert.match(r.flag, /conservative ALL root groups/);
  }
});

// ── selection + subset sanity on a synthetic commit list ────────────────────
test('P1.5 ⊆ current for the PR policy; each commit appears exactly once; ineligible commit bucketed', () => {
  const tmp = fixture();
  const ctx = context(tmp);
  const { rows, buckets } = rowsFor(ctx);
  assert.equal(rows.length, COMMITS.length, 'one row per commit');
  assert.equal(new Set(rows.map(r => r.sha)).size, rows.length, 'every commit exactly once');
  assert.equal(buckets.get('ineligible-paths').count, 1, 'src/lib.rs + Cargo.lock commit is not eligible');
  for (const r of rows.filter(r => r.eligible)) {
    const cur = r.policies.pull_request.current, p15 = r.policies.pull_request.p15;
    for (const g of p15) assert.ok(cur.has(g), `P1.5 group ${g} must be in current for ${r.sha[0]}`);
  }
  rmSync(tmp, { recursive: true, force: true });
});

test('aggregates: sums recomputed twice, shares in [0,1], weights mapped with unmapped/weekly buckets', () => {
  const tmp = fixture();
  const ctx = context(tmp);
  const { rows, buckets } = rowsFor(ctx);
  assert.deepEqual(ctx.winfo.unmapped, ['mystery job'], 'unmapped run job reported');
  assert.equal(ctx.winfo.excluded.length, 1, 'weekly job excluded from both sides');
  assert.equal(ctx.winfo.skipped.length, 0, 'weekly skip counted as excluded, not skipped');
  assert.equal(ctx.weights.get('fast'), 100);
  assert.equal(ctx.weights.get('seam'), 300);
  assert.equal(ctx.weights.get('os'), 50);
  const reselect = (sha, policy) => {
    const r = rows.find(x => x.sha === sha);
    return { current: [...r.policies[policy].current], p15: [...r.policies[policy].p15] };
  };
  for (const policy of ['pull_request', 'push']) {
    const agg = aggregate(rows, policy, ctx.weights, reselect);
    assert.ok(agg.shareCommits >= 0 && agg.shareCommits <= 1);
    assert.ok(agg.shareSeconds >= 0 && agg.shareSeconds <= 1);
    assert.equal(agg.eligibleCount, 2);    const expected = policy === 'pull_request' ? 400 : 100; // push drops seam
    assert.equal(agg.todaySeconds, expected, 'docs commit selects nothing; widget commit selects fast(100)+seam(300) under PR');
  }
  rmSync(tmp, { recursive: true, force: true });
});

test('report bytes are deterministic and contain no machine paths', () => {
  const tmp = fixture();
  const ctx = context(tmp);
  const { rows, buckets } = rowsFor(ctx);
  const d = {
    window: { n: 3, baseSha: 'f'.repeat(40), headSha: 'e'.repeat(40), ref: 'origin/main' },
    method: METHOD_RULES, rows, buckets,
    agg: {
      pull_request: aggregate(rows, 'pull_request', ctx.weights, (sha) => { const r = rows.find(x => x.sha === sha); return { current: [...r.policies.pull_request.current], p15: [...r.policies.pull_request.p15] }; }),
      push: aggregate(rows, 'push', ctx.weights, (sha) => { const r = rows.find(x => x.sha === sha); return { current: [...r.policies.push.current], p15: [...r.policies.push.p15] }; }),
    },
    weights: { runId: '42', event: 'schedule', createdAt: '2026-01-01T00:00:00Z', jobCount: 7, mapped: ctx.winfo.mapped, unmapped: ctx.winfo.unmapped, excluded: ctx.winfo.excluded.length, skipped: ctx.winfo.skipped.length, zeroTimestamp: ctx.winfo.zeroTimestamp },
  };
  const a = renderReport(d), b = renderReport(d);
  assert.equal(a, b, 'byte-identical');
  assert.ok(!a.includes(tmpdir()), 'no machine paths');
  assert.ok(a.includes('NO GO/NO-GO'), 'no-verdict statement present');
  rmSync(tmp, { recursive: true, force: true });
});

// ── CLI ─────────────────────────────────────────────────────────────────────
function cliArgs(tmp, reportPath, extra = []) {
  return [SCRIPT, '--repo-root', tmp, '--report-path', reportPath,
    '--inputs', join(tmp, 'run.json'),
    '--window-file', join(tmp, 'window.json'), ...extra];
}

function withWindow(tmp, commits = COMMITS) {
  writeFileSync(join(tmp, 'window.json'), JSON.stringify({
    ref: 'origin/main', headSha: 'e'.repeat(40), baseSha: 'f'.repeat(40), commits,
  }));
}

test('CLI --report writes the file twice byte-identical; --check passes on it and fails tampered (nonzero exit)', () => {
  const tmp = fixture();
  withWindow(tmp);
  const reportPath = join(tmp, 'report.md');
  execFileSync(process.execPath, cliArgs(tmp, reportPath, ['--report']), { encoding: 'utf8' });
  const first = readFileSync(reportPath, 'utf8');
  execFileSync(process.execPath, cliArgs(tmp, reportPath, ['--report']), { encoding: 'utf8' });
  assert.equal(readFileSync(reportPath, 'utf8'), first, 'two generations byte-identical');
  execFileSync(process.execPath, cliArgs(tmp, reportPath, ['--check']), { encoding: 'utf8' }); // must not throw
  writeFileSync(reportPath, first.replace('Window CI seconds today', 'tampered'));
  assert.throws(() => execFileSync(process.execPath, cliArgs(tmp, reportPath, ['--check']), { encoding: 'utf8' }),
    /--check FAILED/, 'tampered report must fail --check');
  rmSync(tmp, { recursive: true, force: true });
});

test('importing the CLI module has no side effects: nothing written, exit 0', () => {
  const tmp = mkdtempSync(join(tmpdir(), 'p15-selection-import-'));
  execFileSync(
    process.execPath,
    ['-e', 'import(require("node:url").pathToFileURL(process.argv[1]).href)', SCRIPT],
    { cwd: tmp, encoding: 'utf8' },
  );
  assert.deepEqual(readdirSync(tmp), [], 'importing must write nothing');
  rmSync(tmp, { recursive: true, force: true });
});
