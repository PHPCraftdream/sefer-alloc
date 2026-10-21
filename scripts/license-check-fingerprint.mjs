#!/usr/bin/env node
// Fingerprint of everything the CI `licenses` job's verdict depends on (docs/CI_TIERS.md).
// `cargo deny check licenses bans sources` is a pure function of these inputs, so a run whose
// fingerprint already passed once may skip the check: the job keys an Actions-cache marker by it.
//   - Cargo.lock: the resolved graph (names, versions, sources, checksums; crates.io content is
//     immutable per checksum, so a crate's license metadata cannot change under a fixed lock);
//   - deny.toml: the policy (allow-list, clarifications, ban and source rules);
//   - workspace members' name/version/license/license-file/publish from `cargo metadata --no-deps`
//     (only these fields, so a root Cargo.toml edit that adds a [[bench]] does not count);
//   - the cargo-deny version (SPDX parsing and license-text detection ship in the binary);
//   - the `licenses` job text in ci.yml and this script (a changed check or rule re-runs it).
// Text inputs are hashed with CRLF normalised to LF.
//
// Usage:
//   node scripts/license-check-fingerprint.mjs --metadata <members.json> --tool "<cargo deny --version>"
//        [--lock Cargo.lock] [--deny deny.toml] [--workflow .github/workflows/ci.yml] [--job licenses]
//   node scripts/license-check-fingerprint.mjs --selftest
// Prints `fingerprint=<sha256>` (also appended to $GITHUB_OUTPUT when set) and one line per input.
import { createHash } from 'node:crypto';
import { appendFileSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const SCHEMA = 1;
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
export function fingerprint({ lock, deny, metadata, tool, workflow, job, script }) {
  assert.ok(lock.includes('[[package]]'), 'Cargo.lock: no [[package]] entries');
  assert.ok(/^cargo-deny \S+/.test(tool.trim()), `unexpected cargo-deny version string: ${tool}`);
  const parts = {
    schema: String(SCHEMA),
    lock: sha(text(lock)),
    deny: sha(text(deny)),
    members: sha(JSON.stringify(memberLicenses(metadata))),
    tool: tool.trim(),
    job: sha(jobBlock(workflow, job)),
    script: sha(text(script)),
  };
  return { fingerprint: sha(JSON.stringify(parts)), parts };
}

function selftest() {
  const pkg = (name, license = 'MIT OR Apache-2.0', extra = {}) => ({ name, version: '0.1.0', license, ...extra });
  const base = {
    lock: 'version = 4\n\n[[package]]\nname = "a"\nversion = "1.0.0"\n',
    deny: '[licenses]\nallow = ["MIT"]\n',
    metadata: { packages: [pkg('root'), pkg('member')] },
    tool: 'cargo-deny 0.19.9\n',
    workflow: 'jobs:\n  deny:\n    steps: [a]\n  licenses:\n    steps: [b]\n  later:\n    steps: [c]\n',
    job: 'licenses',
    script: 'x',
  };
  const fp = (over = {}) => fingerprint({ ...base, ...over }).fingerprint;
  const same = (over, msg) => assert.equal(fp(over), fp(), msg);
  const differs = (over, msg) => assert.notEqual(fp(over), fp(), msg);
  let n = 0;
  const t = (f, ...a) => { f(...a); n++; };

  t(same, {}, 'deterministic');
  t(differs, { lock: base.lock.replace('1.0.0', '1.0.1') }, 'resolved version change');
  t(differs, { lock: `${base.lock}\n[[package]]\nname = "b"\nversion = "2.0.0"\n` }, 'new dependency');
  t(same, { lock: base.lock.replace(/\n/g, '\r\n') }, 'CRLF-only lock change');
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
  const { fingerprint: value, parts } = fingerprint({
    lock: readFileSync(arg('--lock', 'Cargo.lock'), 'utf8'),
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
