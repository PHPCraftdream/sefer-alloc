#!/usr/bin/env node
// Workspace crate graph: declared Cargo deps (`cargo metadata --no-deps`,
// read-only, never compiles/resolves) + reference edges from a pure-text scan
// of `#[path = ...]` / `include*!` in Rust files.
//
// Why declared deps and NOT `meta.resolve`: resolve only covers the default
// feature set, so root optional deps behind features (alloc-core,
// experimental) were missing and root fast/seam/os jobs would be skipped for
// changes to core crates. Manifest `dependencies[]` lists ALL declared kinds
// (normal/dev/build, optional, target-specific); a feature-enabled or
// cfg-matched build still links them, so including them unconditionally is
// conservative = safe.
//
// Why reference edges: root tests `#[path]`-include crate sources directly,
// bypassing the Cargo graph; those files affect the root regardless of deps.
//
// Graph shape (JSDoc):
// {
//   packages: Map<name, { dir: string, deps: Set<name>, dependents: Set<name>,
//                         refDeps: Set<name> }>,  // refDeps: subset of deps from text refs
//   root: string,   // root package name (sefer-alloc)
// }
//
// Usage: node scripts/ci/crate-graph.mjs            (usage)
//        node scripts/ci/crate-graph.mjs --selftest (fixture selftest)
//        node scripts/ci/crate-graph.mjs --dump     (print the real graph)
import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

/**
 * Pure: build the graph from a `cargo metadata --no-deps` JSON object.
 * Edge dep -> dependent for every declared dependency whose package name is
 * another workspace member. Match on `dep.name` (package identity), never
 * `rename` (a code-level alias only). `meta.resolve` is deliberately ignored.
 * @returns {{ packages: Map<string, {dir: string, deps: Set<string>, dependents: Set<string>, refDeps: Set<string>}>, root: string | null }}
 */
export function graphFromMetadata(meta) {
  const wsRoot = (meta.workspace_root ?? '').replace(/\\/g, '/').replace(/\/$/, '');
  const members = new Map(); // name -> repo-relative posix dir ('' for root)
  for (const pkg of meta.packages ?? []) {
    if (!meta.workspace_members?.includes(pkg.id)) continue;
    // Lexical prefix strip (pure): manifest_path is absolute under wsRoot.
    const mp = pkg.manifest_path.replace(/\\/g, '/');
    const dir = wsRoot && mp.startsWith(`${wsRoot}/`)
      ? mp.slice(wsRoot.length + 1).replace(/\/Cargo\.toml$/, '')
      : mp.replace(/\/Cargo\.toml$/, '');
    members.set(pkg.name, dir);
  }
  // Root package: manifest directly under the workspace root, not crates/.
  let root = null;
  for (const [name, dir] of members) {
    if (!dir.startsWith('crates/')) { root = name; break; }
  }
  const graph = { packages: new Map(), root };
  for (const name of members.keys()) {
    const dir = members.get(name);
    graph.packages.set(name, { dir: dir || '.', deps: new Set(), dependents: new Set(), refDeps: new Set() });
  }
  if (!graph.packages.has(root)) {
    graph.packages.set(root, { dir: '.', deps: new Set(), dependents: new Set(), refDeps: new Set() });
  }
  for (const pkg of meta.packages ?? []) {
    const dependent = graph.packages.has(pkg.name) ? pkg.name : null;
    if (!dependent) continue;
    for (const dep of pkg.dependencies ?? []) {
      // Manifest-level: covers optional + target-specific + all dep kinds.
      // No guard on dep.kind/optional/target: conservative on purpose.
      const depName = dep.name;
      if (depName === dependent) continue; // self-dep: no edge
      if (graph.packages.has(depName)) {
        graph.packages.get(depName).dependents.add(dependent);
        graph.packages.get(dependent).deps.add(depName);
      }
    }
  }
  return graph;
}

/** Posix directory part of a repo-relative posix file path ('' at repo root). */
function posixDir(p) {
  const i = p.lastIndexOf('/');
  return i < 0 ? '' : p.slice(0, i);
}

/**
 * Pure lexical resolution of a `#[path]`/include! literal relative to the
 * containing file's directory. Returns a repo-relative posix path, or null if
 * UNRESOLVABLE: empty, absolute, or `..` escaping the repo root.
 */
function lexResolve(baseDir, ref) {
  if (!ref) return null;
  const norm = ref.replace(/\\/g, '/');
  if (norm.startsWith('/') || /^[A-Za-z]:/.test(norm)) return null; // absolute
  const out = [];
  for (const part of (baseDir ? baseDir.split('/') : []).concat(norm.split('/'))) {
    if (!part || part === '.') continue;
    if (part === '..') {
      if (!out.length) return null; // escapes repo root
      out.pop();
    } else out.push(part);
  }
  return out.join('/');
}

/**
 * Pure: scan Rust file texts for `#[path = "..."]`, `include!("...")`,
 * `include_str!("...")`, `include_bytes!("...")` (whitespace-tolerant, see the
 * READS_FILES regex precedent in select-tests.mjs) and return reference edges
 * Map<fromCrate, Set<toCrate>>.
 *
 * Unresolvable references (absolute, repo-escaping, empty) are FAIL-CLOSED:
 * edge from the file's owner crate to EVERY workspace crate. Justification:
 * per-file conservatism keeps the rest of the graph usable, and only the
 * genuinely-unknown file degrades rootAffected/seam suppression to the old
 * safe (no-narrowing) behavior. Resolvable references to non-member paths
 * (e.g. ../docs) or to the SAME crate produce NO edge (a false positive here
 * would make everything affect everything).
 * @param {Array<{path: string, text: string}>} entries repo-relative posix paths
 */
export function scanReferenceEdges(entries, graph) {
  // Whitespace variants allowed: `# [ path = ... ]`, `include ! ( ... )`.
  const REF_RE = /#\s*\[\s*path\s*=\s*"([^"]*)"\s*\]|include(?:_str|_bytes)?\s*!\s*\(\s*"([^"]*)"/g;
  const edges = new Map();
  const add = (from, to) => {
    if (!edges.has(from)) edges.set(from, new Set());
    edges.get(from).add(to);
  };
  // Owner of a scanned/owned file: crateForPath misses the root crate (dir
  // '.'), so fall back to root for any path outside crates/. src/ tests/
  // examples/ benches/ of the root have owner = root crate.
  const ownerOf = p => {
    const c = crateForPath(p, graph);
    if (c) return c;
    const q = p.replace(/\\/g, '/');
    return q.startsWith('crates/') ? null : graph.root;
  };
  for (const e of entries ?? []) {
    const from = ownerOf(e.path);
    if (!from) continue; // file outside every member crate: nothing to own it
    const base = posixDir(e.path.replace(/\\/g, '/'));
    for (const m of (e.text ?? '').matchAll(REF_RE)) {
      const ref = m[1] ?? m[2];
      const resolved = lexResolve(base, ref);
      if (resolved === null) {
        // Fail-closed: unknown target => assume it can touch every crate.
        for (const name of graph.packages.keys()) if (name !== from) add(from, name);
        continue;
      }
      const to = ownerOf(resolved);
      if (to && to !== from) add(from, to);
    }
  }
  return edges;
}

/** Merge reference edges into the graph: refDeps kept distinct for --dump, deps merged. */
export function mergeRefEdges(graph, edges) {
  for (const [from, tos] of edges ?? []) {
    const pkg = graph.packages.get(from);
    if (!pkg) continue;
    pkg.refDeps ??= new Set(); // hand-built fixture graphs have no refDeps
    for (const to of tos) {
      if (!graph.packages.has(to)) continue;
      pkg.refDeps.add(to);
      pkg.deps.add(to);
      graph.packages.get(to).dependents.add(from);
    }
  }
  return graph;
}

/** Package name owning `path` (posix), via repo-relative dir prefix; null otherwise. */
export function crateForPath(path, graph) {
  if (!graph) return null;
  const p = path.replace(/\\/g, '/');
  for (const [name, pkg] of graph.packages) {
    const dir = pkg.dir === '.' ? '' : pkg.dir;
    if (dir && (p === `${dir}/Cargo.toml` || p.startsWith(`${dir}/`))) return name;
  }
  return null;
}

/** `changed` crates plus all transitive reverse dependents (dependents of dependents, ...). */
export function affectedCrates(changed, graph) {
  const affected = new Set();
  const stack = [...changed];
  while (stack.length) {
    const name = stack.pop();
    if (affected.has(name)) continue;
    const pkg = graph.packages.get(name);
    if (!pkg) continue; // unknown name: ignore, callers handle it separately
    affected.add(name);
    for (const d of pkg.dependents) if (!affected.has(d)) stack.push(d);
  }
  return affected;
}

/**
 * True iff any direct workspace dependency of the root package is affected.
 * Why direct-only suffices: if root depends transitively on an affected crate
 * X via Y, then Y itself depends on X, so Y is a reverse dependent of X and is
 * in `affected` too — the 3-level fixture in selftest asserts this.
 */
export function rootAffected(affected, graph) {
  return [...graph.packages.get(graph.root)?.deps ?? []].some(d => affected.has(d));
}

/** Collect repo-relative posix paths of *.rs files under `dirs`, skipping target/. */
function collectRustFiles(rootDir, dirs) {
  const out = [];
  const seen = new Set(); // absolute paths (member dirs may overlap root dirs)
  const walk = abs => {
    if (seen.has(abs)) return;
    seen.add(abs);
    let entries;
    try { entries = readdirSync(abs, { withFileTypes: true }); }
    catch { return; } // missing dir is fine
    for (const ent of entries) {
      if (ent.name === 'target') continue; // build artifacts, never sources
      const child = join(abs, ent.name);
      if (ent.isDirectory()) walk(child);
      else if (ent.isFile() && ent.name.endsWith('.rs')) out.push(child);
    }
  };
  for (const d of dirs) walk(join(rootDir, d));
  return out.map(f => relative(rootDir, f).replace(/\\/g, '/'));
}

/**
 * Build the workspace-member dependency graph. Returns null on any spawn or
 * parse failure (caller falls back to the conservative plan).
 * Edges: declared manifest deps (all kinds, optional, target-specific) via
 * graphFromMetadata, plus reference edges from a text scan of root
 * src/tests/examples/benches and each member crate's whole tree — a crate file
 * `#[path]`/include!-ed into a root test affects the root regardless of the
 * Cargo graph.
 * @returns {{ packages: Map<string, {dir: string, deps: Set<string>, dependents: Set<string>, refDeps: Set<string>}>, root: string } | null}
 */
export function loadCrateGraph(rootDir) {
  const r = spawnSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], {
    cwd: rootDir, encoding: 'utf8', maxBuffer: 1 << 28,
  });
  if (r.status !== 0 || !r.stdout) {
    console.error(`crate-graph: cargo metadata failed: ${r.stderr?.slice(0, 200)}`);
    return null;
  }
  let meta;
  try { meta = JSON.parse(r.stdout); } catch (e) {
    console.error(`crate-graph: cargo metadata output unparseable: ${e.message}`);
    return null;
  }
  const graph = graphFromMetadata(meta);
  if (!graph?.root) {
    console.error('crate-graph: no root package in metadata');
    return null;
  }
  const scanDirs = ['src', 'tests', 'examples', 'benches'];
  for (const [name, pkg] of graph.packages) {
    if (pkg.dir !== '.') scanDirs.push(pkg.dir); // member crates: whole tree
  }
  const entries = [];
  for (const rel of collectRustFiles(rootDir, scanDirs)) {
    let text = '';
    try { text = readFileSync(join(rootDir, rel), 'utf8'); }
    catch (e) { console.error(`crate-graph: unreadable ${rel}: ${e.message}`); }
    entries.push({ path: rel, text });
  }
  mergeRefEdges(graph, scanReferenceEdges(entries, graph));
  return graph;
}

function selftest() {
  // a->b->c means a depends on b... wait: names below: dep chain root -> leaf.
  const mk = entries => ({
    packages: new Map(entries.map(([n, deps]) => [n, { dir: `crates/t/${n}`, deps: new Set(deps), dependents: new Set(), refDeps: new Set() }])),
    root: 'rootpkg',
  });
  const fixEdges = g => {
    for (const [n, p] of g.packages) for (const d of p.deps) g.packages.get(d).dependents.add(n);
    return g;
  };
  let n = 0;
  const eq = (got, want, msg) => { assert.deepEqual(got, want, msg); n++; };

  // single changed crate, no dependents, root does not depend on it
  const solo = fixEdges(mk([['rootpkg', ['unrelated']], ['unrelated', []], ['standalone', []]]));
  eq([...affectedCrates(['standalone'], solo)].sort(), ['standalone'], 'single changed crate only');
  eq(rootAffected(affectedCrates(['standalone'], solo), solo), false, 'root unaffected for standalone');

  // chain: root -> mid -> leaf (root depends on mid, mid on leaf)
  const chain = fixEdges(mk([['rootpkg', ['mid']], ['mid', ['leaf']], ['leaf', []]]));
  // rootpkg depends on mid, mid on leaf: a leaf change reaches the root too.
  eq([...affectedCrates(['leaf'], chain)].sort(), ['leaf', 'mid', 'rootpkg'], 'chain pulls dependents');
  eq(rootAffected(affectedCrates(['leaf'], chain), chain), true, 'root affected via 2-step chain');
  eq(rootAffected(affectedCrates(['standalone'], chain), chain), false, 'root unaffected standalone in chain graph');

  // dev-dependency and build-dependency edges count (both stored as deps by loader)
  const devBuild = fixEdges(mk([['rootpkg', []], ['util', []], ['tester', ['util']], ['builder', ['util']]]));
  eq([...affectedCrates(['util'], devBuild)].sort(), ['builder', 'tester', 'util'], 'dev+build edges count');

  // crateForPath
  eq(crateForPath('crates/t/leaf/src/lib.rs', chain), 'leaf', 'crateForPath src file');
  eq(crateForPath('crates/t/leaf/Cargo.toml', chain), 'leaf', 'crateForPath manifest');
  eq(crateForPath('crates/t/other/src/lib.rs', chain), null, 'crateForPath negative other crate');
  eq(crateForPath('src/lib.rs', chain), null, 'crateForPath negative root src');
  eq(crateForPath('crates/t/leafish/src/x.rs', chain), null, 'no dir-prefix false positive');

  // HOLE 1: edges must come from DECLARED deps, not resolve. Fixture member
  // declares an optional dep; the fake resolve (as cargo's real one would for
  // a non-default feature) lacks the edge entirely — and is ignored anyway.
  const metaFix = () => ({
    workspace_root: '/repo',
    workspace_members: ['p-root', 'p-policy'],
    packages: [
      { id: 'p-root', name: 'sefer-alloc', manifest_path: '/repo/Cargo.toml',
        dependencies: [{ name: 'policy', optional: true }] },
      { id: 'p-policy', name: 'policy', manifest_path: '/repo/crates/t/policy/Cargo.toml', dependencies: [] },
    ],
    resolve: { nodes: [{ id: 'p-root', deps: [] }, { id: 'p-policy', deps: [] }] },
  });
  const gOpt = graphFromMetadata(metaFix());
  eq(rootAffected(affectedCrates(['policy'], gOpt), gOpt), true, 'optional declared dep reaches root');
  // Explicit counterfactual: the resolve-based (old) reading of this fixture
  // would give an EMPTY edge set => rootAffected=false. Assert that directly
  // by removing the declared deps the old loader never had.
  const metaNoDeps = metaFix();
  delete metaNoDeps.packages[0].dependencies;
  const gNoDeps = graphFromMetadata(metaNoDeps);
  eq([...gNoDeps.packages.get('sefer-alloc').deps], [], 'resolve-only counterfactual: no edges');
  eq(rootAffected(affectedCrates(['policy'], gNoDeps), gNoDeps), false, 'resolve-only counterfactual: rootAffected false');

  // HOLE 2: reference edges. root test path-includes a member crate file.
  const refG = fixEdges(mk([['rootpkg', []], ['policy', []], ['other', []]]));
  refG.packages.get('rootpkg').dir = '.';
  const withRefs = mergeRefEdges(structuredClone(refG),
    scanReferenceEdges([{ path: 'tests/loom_x.rs', text: '#[path = "../crates/t/policy/src/f.rs"] mod f;' }], refG));
  eq([...withRefs.packages.get('rootpkg').deps], ['policy'], 'path-attr ref edge root->policy');
  eq(rootAffected(affectedCrates(['policy'], withRefs), withRefs), true, 'ref edge makes root affected');
  // counterfactual: no scanned files => no ref edge => root unaffected
  const bare = mergeRefEdges(structuredClone(refG), scanReferenceEdges([], refG));
  eq(rootAffected(affectedCrates(['policy'], bare), bare), false, 'no-ref-edges counterfactual: rootAffected false');
  // spaced `# [ path = ... ]` variant detected too
  eq([...scanReferenceEdges([{ path: 'tests/loom_y.rs', text: '# [ path = "../crates/t/policy/src/g.rs" ] mod g;' }], refG).get('rootpkg')],
    ['policy'], 'spaced path attr detected');
  eq([...scanReferenceEdges([{ path: 'tests/z.rs', text: 'const A: &str = include_str!("../crates/t/policy/src/a.txt");' }], refG).get('rootpkg')],
    ['policy'], 'include_str! detected');
  eq([...scanReferenceEdges([{ path: 'tests/z.rs', text: 'let b = include_bytes!("../crates/t/policy/src/b.bin");' }], refG).get('rootpkg')],
    ['policy'], 'include_bytes! detected');
  // unresolvable absolute path: fail-closed edge to every OTHER crate
  const all = scanReferenceEdges([{ path: 'tests/abs.rs', text: '#[path = "C:/etc/f.rs"] mod f;' }], refG);
  const allSet = all.get('rootpkg');
  assert.ok(allSet.has('policy') && allSet.has('other') && allSet.size === 2, 'unresolvable ref -> all crates');
  n++;
  eq(rootAffected(affectedCrates(['policy'], mergeRefEdges(structuredClone(refG), all)), mergeRefEdges(structuredClone(refG), all)),
    true, 'unresolvable ref makes root affected');
  // `../` normalization from a deeply nested member file lands in root tests
  const deep = scanReferenceEdges([{ path: 'crates/t/policy/tests/deep/nest/a.rs',
    text: '#[path = "../../../../../../tests/loom_x2.rs"] mod x;' }], refG);
  eq([...deep.get('policy')], ['rootpkg'], 'deep ../ normalization -> root crate');
  // negatives: same-crate and non-member targets produce NO edge
  eq(scanReferenceEdges([{ path: 'tests/x.rs', text: '#[path = "helper.rs"] mod h;' }], refG).size, 0,
    'same-crate ref: no edge');
  eq(scanReferenceEdges([{ path: 'tests/x.rs', text: '#[path = "../docs/x.rs"] mod d;' }], refG).size, 0,
    'non-member ref: no edge');
  console.log(JSON.stringify({ version: 1, status: 'PASS', scope: 'crate-graph selftest', checks: n }));
}

function dump() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const g = loadCrateGraph(root);
  if (!g) { process.exitCode = 1; return; }
  for (const [name, pkg] of [...g.packages].sort()) {
    const deps = [...pkg.deps].sort().map(d => pkg.refDeps.has(d) ? `${d} (ref)` : d);
    console.log(`${name} (${pkg.dir}) deps=[${deps.join(',')}] dependents=[${[...pkg.dependents].sort()}]`);
  }
  console.log(`root=${g.root} rootDeps=[${[...g.packages.get(g.root)?.deps].sort().join(',')}]`);
}

function main() {
  if (process.argv.includes('--selftest')) return selftest();
  if (process.argv.includes('--dump')) return dump();
  console.log('usage: node scripts/ci/crate-graph.mjs [--selftest|--dump]');
}

if (process.argv[1]?.endsWith('crate-graph.mjs')) main();
