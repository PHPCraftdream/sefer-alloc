#!/usr/bin/env node
// Third-party license coverage for the CI `licenses` job (docs/CI_TIERS.md).
//   --list   print the directory of every tracked Cargo.lock (root first): the job runs
//            `cargo deny check licenses bans sources` once per directory, so fuzz/ and the
//            compile_fail fixtures, each its own workspace with its own lock, are checked too.
//   --check  fail on (a) a tracked Cargo.toml with its own [workspace] table but no tracked
//            Cargo.lock beside it (a workspace the job would silently skip); (b) a `uses:` in
//            .github/workflows/*.yml whose owner/repo is not in ACTIONS, or whose license is not
//            allowed by deny.toml's [licenses] allow list; (c) an ACTIONS entry no workflow uses.
//   --selftest  the checks above against in-memory fixtures and their mutants.
// No network: ACTIONS records each action's license, read from its repository when added.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

/** Third-party GitHub Actions: owner/repo -> SPDX expression of its repository license. */
export const ACTIONS = {
  'actions/cache': 'MIT',
  'actions/checkout': 'MIT',
  'actions/setup-node': 'MIT',
  'actions/upload-artifact': 'MIT',
  'dtolnay/rust-toolchain': 'MIT',
  'taiki-e/install-action': 'Apache-2.0 OR MIT',
};

const text = (s) => s.replace(/\r\n/g, '\n');

/** The `allow = [...]` list of deny.toml's [licenses] table. */
export function denyAllow(deny) {
  const lines = text(deny).split('\n');
  const start = lines.findIndex((l) => l.trim() === '[licenses]');
  assert.ok(start >= 0, 'deny.toml: no [licenses] table');
  let body = '';
  for (let i = start + 1; i < lines.length && !/^\[/.test(lines[i]); i++) body += `${lines[i].replace(/#.*$/, '')}\n`;
  const m = /(^|\n)allow\s*=\s*\[([^\]]*)\]/.exec(body);
  assert.ok(m, 'deny.toml: [licenses] has no allow list');
  const ids = [...m[2].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  assert.ok(ids.length > 0, 'deny.toml: empty allow list');
  return new Set(ids);
}

/** True if an SPDX expression of OR / AND terms (no parentheses) is satisfiable by `allow`. */
export function spdxAllowed(expr, allow) {
  assert.ok(!/[()]/.test(expr), `unsupported SPDX expression: ${expr}`);
  return expr.split(/\s+OR\s+/).some((alt) => alt.split(/\s+AND\s+/).every((id) => allow.has(id.trim())));
}

/** owner/repo of every third-party `uses:` (local `./` actions excluded), with its locations. */
export function actionUses(workflows) {
  const uses = new Map();
  for (const [file, body] of Object.entries(workflows)) {
    text(body).split('\n').forEach((l, i) => {
      const m = /^\s*(?:-\s+)?uses:\s*['"]?([^'"\s#]+)/.exec(l);
      if (!m || m[1].startsWith('./')) return;
      const ref = m[1].split('@')[0];
      const repo = ref.split('/').slice(0, 2).join('/');
      if (!uses.has(repo)) uses.set(repo, []);
      uses.get(repo).push(`${file}:${i + 1}`);
    });
  }
  return uses;
}

/** Problems for `files` = { path: content } of every tracked Cargo.toml, Cargo.lock and workflow. */
export function problems({ files, deny, actions = ACTIONS }) {
  const out = [];
  const allow = denyAllow(deny);
  for (const [path, body] of Object.entries(files)) {
    if (!/(^|\/)Cargo\.toml$/.test(path) || !/^\[workspace\]\s*$/m.test(text(body))) continue;
    const lock = path.replace(/Cargo\.toml$/, 'Cargo.lock');
    if (!(lock in files)) out.push(`uncovered workspace: ${path} has [workspace] but no tracked ${lock}`);
  }
  const workflows = Object.fromEntries(Object.entries(files).filter(([p]) => /^\.github\/workflows\/[^/]+\.ya?ml$/.test(p)));
  const uses = actionUses(workflows);
  for (const [repo, at] of uses) {
    if (!(repo in actions)) out.push(`action not in ACTIONS: ${repo} (${at[0]})`);
    else if (!spdxAllowed(actions[repo], allow)) out.push(`action license not allowed by deny.toml: ${repo} = ${actions[repo]}`);
  }
  for (const repo of Object.keys(actions)) if (!uses.has(repo)) out.push(`stale ACTIONS entry (no workflow uses it): ${repo}`);
  return out;
}

/** Directories holding a tracked Cargo.lock, root (".") first. */
export function lockDirs(paths) {
  return paths
    .filter((p) => /(^|\/)Cargo\.lock$/.test(p))
    .map((p) => (p === 'Cargo.lock' ? '.' : dirname(p)))
    .sort((a, b) => (a === '.' ? -1 : b === '.' ? 1 : a.localeCompare(b)));
}

export function trackedFiles() {
  return execFileSync('git', ['ls-files', '-z'], { encoding: 'utf8' }).split('\0').filter(Boolean);
}

function selftest() {
  const deny = '[licenses]\nversion = 2\nallow = [\n    "Apache-2.0",\n    "MIT", # comment\n]\n\n[bans]\nallow = ["GPL-3.0"]\n';
  const wf = 'jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v5\n      - uses: owner/tool/sub@abc # pin\n      - uses: ./local\n';
  const base = {
    'Cargo.toml': '[package]\nname = "r"\n\n[workspace]\nmembers = []\n',
    'Cargo.lock': 'x',
    'fuzz/Cargo.toml': '[package]\nname = "f"\n[workspace]\n',
    'fuzz/Cargo.lock': 'x',
    'crates/m/Cargo.toml': '[package]\nname = "m"\n# [workspace] mentioned only in a comment\n',
    '.github/workflows/ci.yml': wf,
  };
  const actions = { 'actions/checkout': 'MIT', 'owner/tool': 'Apache-2.0 OR MIT' };
  const run = (over = {}, acts = actions, d = deny) => problems({ files: { ...base, ...over }, deny: d, actions: acts });
  let n = 0;
  const t = (f, ...a) => { f(...a); n++; };
  const red = (p, re, msg) => assert.ok(p.some((x) => re.test(x)), `${msg}: expected ${re}, got ${JSON.stringify(p)}`);

  t(assert.deepEqual, run(), [], 'clean fixture');
  t(assert.deepEqual, [...denyAllow(deny)].sort(), ['Apache-2.0', 'MIT'], 'allow list of [licenses] only');
  const noLock = { ...base };
  delete noLock['fuzz/Cargo.lock'];
  t(red, problems({ files: noLock, deny, actions }), /uncovered workspace: fuzz\/Cargo\.toml/, 'workspace without lock');
  t(red, run({ '.github/workflows/x.yml': '      - uses: evil/action@v1\n' }), /action not in ACTIONS: evil\/action/, 'unknown action');
  t(red, run({}, { ...actions, 'owner/tool': 'GPL-3.0' }), /license not allowed by deny\.toml: owner\/tool/, 'disallowed action license');
  t(red, run({}, { ...actions, 'actions/cache': 'MIT' }), /stale ACTIONS entry \(no workflow uses it\): actions\/cache/, 'stale entry');
  t(assert.equal, spdxAllowed('MIT AND NCSA', new Set(['MIT'])), false, 'AND needs every term');
  t(assert.equal, spdxAllowed('NCSA OR MIT', new Set(['MIT'])), true, 'OR needs one alternative');
  t(assert.throws, () => spdxAllowed('(MIT OR Apache-2.0) AND NCSA', new Set(['MIT'])), /unsupported/, 'parentheses rejected');
  t(assert.throws, () => denyAllow('[bans]\nallow = ["MIT"]\n'), /no \[licenses\]/, 'missing [licenses]');
  t(assert.deepEqual, lockDirs(['b/Cargo.lock', 'Cargo.lock', 'a/x/Cargo.lock', 'Cargo.toml']), ['.', 'a/x', 'b'], 'lock dirs, root first');
  t(assert.deepEqual, [...actionUses({ w: wf }).keys()], ['actions/checkout', 'owner/tool'], 'uses parsing (sub-path, pin comment, local excluded)');
  console.log(`license-check-workspaces selftest: ${n} checks passed`);
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  if (process.argv.includes('--selftest')) {
    selftest();
  } else if (process.argv.includes('--list')) {
    for (const d of lockDirs(trackedFiles())) console.log(d);
  } else if (process.argv.includes('--check')) {
    const paths = trackedFiles().filter((p) => /(^|\/)Cargo\.(toml|lock)$|^\.github\/workflows\/[^/]+\.ya?ml$/.test(p));
    const files = Object.fromEntries(paths.map((p) => [p, /Cargo\.lock$/.test(p) ? '' : readFileSync(p, 'utf8')]));
    const found = problems({ files, deny: readFileSync('deny.toml', 'utf8') });
    for (const p of found) console.error(`[license-check-workspaces] ${p}`);
    assert.equal(found.length, 0, `${found.length} license-coverage problem(s)`);
    console.log(`[license-check-workspaces] OK: ${lockDirs(paths).length} Cargo workspaces, ${actionUses(Object.fromEntries(Object.entries(files).filter(([p]) => p.startsWith('.github/')))).size} third-party actions covered`);
  } else {
    console.error('usage: node scripts/license-check-workspaces.mjs --list | --check | --selftest');
    process.exit(2);
  }
}
