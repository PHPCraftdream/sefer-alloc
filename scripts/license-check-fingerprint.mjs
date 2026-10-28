#!/usr/bin/env node
// Fingerprint of everything the CI `licenses` job's verdict depends on (docs/CI_TIERS.md).
// `cargo deny check licenses bans sources` is a pure function of these inputs, so a run whose
// fingerprint already passed once may skip the check: the job keys an Actions-cache marker by it.
//   - every tracked Cargo.lock (root, fuzz/, compile_fail fixtures; the job checks each, see
//     scripts/license-check-workspaces.mjs) by path: the resolved graphs (names, versions, sources,
//     checksums; crates.io content is immutable per checksum, so a crate's license metadata
//     cannot change under a fixed lock);
//   - deny.toml: the policy (allow-list, clarifications, ban and source rules);
//   - workspace members' name/version/license/license-file/publish from `cargo metadata --no-deps`
//     (only these fields, so a root Cargo.toml edit that adds a [[bench]] does not count);
//   - the cargo-deny version (SPDX parsing and license-text detection ship in the binary);
//   - the `licenses` job text in ci.yml, this script and license-check-workspaces.mjs (a changed
//     check, rule or workspace list re-runs it).
// Text inputs are hashed with CRLF normalised to LF.
//
// Usage:
//   node scripts/license-check-fingerprint.mjs --metadata <members.json> --tool "<cargo deny --version>"
//        [--lock <path>]... [--deny deny.toml] [--workflow .github/workflows/ci.yml] [--job licenses]
//   (without --lock: every tracked Cargo.lock)
//   node scripts/license-check-fingerprint.mjs --selftest
// Prints `fingerprint=<sha256>` (also appended to $GITHUB_OUTPUT when set) and one line per input.
import { createHash } from 'node:crypto';
import { appendFileSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { lockDirs, trackedFiles } from './license-check-workspaces.mjs';

const SCHEMA = 2;
const sha = (s) => createHash('sha256').update(s).digest('hex');
const text = (s) => s.replace(/\r\n/g, '\n');

/** The workspace members' license-relevant fields, order-independent. */
export function memberLicenses(metadata) {
  assert.ok(Array.isArray(metadata.packages) && metadata.packages.length > 0, 'cargo metadata: no packages');
  return metadata.packages
    .map((p) => ({
      name: p.name,
      version: p.version,
      license: p.license ?? null,
      license_file: p.license_file ?? null,
      publish: p.publish ?? null,
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/** Lines of the `  <job>:` block of a workflow, up to the next top-level job. */
export function jobBlock(workflow, job) {
  const lines = text(workflow).split('\n');
  const start = lines.findIndex((l) => l === `  ${job}:`);
  assert.ok(start >= 0, `workflow has no job \`${job}\``);
  let end = lines.findIndex((l, i) => i > start && /^  [A-Za-z0-9_-]+:\s*$/.test(l));
  if (end < 0) end = lines.length;
  return lines.slice(start, end).join('\n').trimEnd();
}

/** @returns {{fingerprint: string, parts: Record<string, string>}} */
export function fingerprint({ locks, deny, metadata, tool, workflow, job, script, workspacesScript }) {
  const paths = Object.keys(locks).sort();
  assert.ok(paths.length > 0, 'no Cargo.lock given');
  for (const p of paths) assert.ok(locks[p].includes('[[package]]'), `${p}: no [[package]] entries`);
  assert.ok(/^cargo-deny \S+/.test(tool.trim()), `unexpected cargo-deny version string: ${tool}`);
  const parts = {
    schema: String(SCHEMA),
    locks: sha(JSON.stringify(paths.map((p) => [p, sha(text(locks[p]))]))),
    deny: sha(text(deny)),
    members: sha(JSON.stringify(memberLicenses(metadata))),
    tool: tool.trim(),
    job: sha(jobBlock(workflow, job)),
    script: sha(text(script)),
    workspaces: sha(text(workspacesScript)),
  };
  return { fingerprint: sha(JSON.stringify(parts)), parts };
}

function selftest() {
  const pkg = (name, license = 'MIT OR Apache-2.0', extra = {}) => ({ name, version: '0.1.0', license, ...extra });
  const LOCK = 'version = 4\n\n[[package]]\nname = "a"\nversion = "1.0.0"\n';
  const base = {
    locks: { 'Cargo.lock': LOCK, 'fuzz/Cargo.lock': LOCK },
    deny: '[licenses]\nallow = ["MIT"]\n',
    metadata: { packages: [pkg('root'), pkg('member')] },
    tool: 'cargo-deny 0.19.9\n',
    workflow: 'jobs:\n  deny:\n    steps: [a]\n  licenses:\n    steps: [b]\n  later:\n    steps: [c]\n',
    job: 'licenses',
    script: 'x',
    workspacesScript: 'w',
  };
  const lock = (p, s) => ({ locks: { ...base.locks, [p]: s } });
  const fp = (over = {}) => fingerprint({ ...base, ...over }).fingerprint;
  const same = (over, msg) => assert.equal(fp(over), fp(), msg);
  const differs = (over, msg) => assert.notEqual(fp(over), fp(), msg);
  let n = 0;
  const t = (f, ...a) => { f(...a); n++; };

  t(same, {}, 'deterministic');
  t(differs, lock('Cargo.lock', LOCK.replace('1.0.0', '1.0.1')), 'resolved version change');
  t(differs, lock('fuzz/Cargo.lock', LOCK.replace('1.0.0', '1.0.1')), 'version change in a second workspace');
  t(differs, lock('Cargo.lock', `${LOCK}\n[[package]]\nname = "b"\nversion = "2.0.0"\n`), 'new dependency');
  t(differs, lock('tests/x/Cargo.lock', LOCK), 'new workspace');
  t(differs, { locks: { 'Cargo.lock': LOCK, 'other/Cargo.lock': LOCK } }, 'workspace moved');
  t(same, { locks: { 'fuzz/Cargo.lock': LOCK, 'Cargo.lock': LOCK } }, 'lock order');
  t(same, lock('Cargo.lock', LOCK.replace(/\n/g, '\r\n')), 'CRLF-only lock change');
  t(differs, { deny: '[licenses]\nallow = ["MIT", "GPL-3.0"]\n' }, 'policy change');
  t(differs, { metadata: { packages: [pkg('root'), pkg('member', 'GPL-3.0')] } }, 'member license change');
  t(differs, { metadata: { packages: [pkg('root'), pkg('member', undefined, { license_file: 'LICENSE' })] } }, 'member license-file change');
  t(same, { metadata: { packages: [pkg('member'), pkg('root')] } }, 'member order');
  t(same, { metadata: { packages: [pkg('root', undefined, { targets: [{ name: 'new_bench' }] }), pkg('member')] } }, 'non-license member field (new [[bench]])');
  t(differs, { tool: 'cargo-deny 0.20.0' }, 'cargo-deny version change');
  t(same, { tool: 'cargo-deny 0.19.9' }, 'version string whitespace');
  t(differs, { workflow: base.workflow.replace('steps: [b]', 'steps: [b2]') }, 'licenses job edit');
  t(same, { workflow: base.workflow.replace('steps: [a]', 'steps: [a2]').replace('steps: [c]', 'steps: [c2]') }, 'other job edit');
  t(differs, { script: 'y' }, 'fingerprint script edit');
  t(differs, { workspacesScript: 'w2' }, 'workspace-list script edit');
  t(assert.throws, () => fp({ locks: {} }), /no Cargo\.lock/, 'no locks');
  t(assert.throws, () => fp(lock('fuzz/Cargo.lock', '')), /fuzz\/Cargo\.lock: no \[\[package\]\]/, 'empty lock');
  t(assert.throws, () => fp({ job: 'missing' }), /no job `missing`/, 'missing job');
  t(assert.throws, () => fp({ tool: 'oops' }), /cargo-deny version/, 'bad tool string');
  t(assert.throws, () => fp({ metadata: { packages: [] } }), /no packages/, 'empty metadata');
  t(assert.equal, jobBlock(base.workflow.trimEnd(), 'later'), '  later:\n    steps: [c]', 'last job runs to EOF');
  console.log(`license-check-fingerprint selftest: ${n} checks passed`);
}

function arg(name, fallback) {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : fallback;
}

if (process.argv.includes('--selftest')) {
  selftest();
} else {
  const metadataPath = arg('--metadata');
  const tool = arg('--tool');
  assert.ok(metadataPath && tool, 'usage: --metadata <members.json> --tool "<cargo deny --version>"');
  const lockArgs = process.argv.flatMap((a, i) => (a === '--lock' ? [process.argv[i + 1]] : []));
  const lockPaths = lockArgs.length > 0
    ? lockArgs
    : lockDirs(trackedFiles()).map((d) => (d === '.' ? 'Cargo.lock' : `${d}/Cargo.lock`));
  const { fingerprint: value, parts } = fingerprint({
    locks: Object.fromEntries(lockPaths.map((p) => [p, readFileSync(p, 'utf8')])),
    workspacesScript: readFileSync(join(dirname(fileURLToPath(import.meta.url)), 'license-check-workspaces.mjs'), 'utf8'),
    deny: readFileSync(arg('--deny', 'deny.toml'), 'utf8'),
    metadata: JSON.parse(readFileSync(metadataPath, 'utf8')),
    tool,
    workflow: readFileSync(arg('--workflow', '.github/workflows/ci.yml'), 'utf8'),
    job: arg('--job', 'licenses'),
    script: readFileSync(fileURLToPath(import.meta.url), 'utf8'),
  });
  for (const [k, v] of Object.entries(parts)) console.log(`${k}: ${v}`);
  console.log(`fingerprint=${value}`);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `fingerprint=${value}\n`);
}
