// Graph stage of the module-dependency-graph tool (scripts/module-graph/;
// lexer.mjs → parse.mjs → THIS file; a report stage consumes the graph).
//
// `buildModuleGraph({ srcDir = "src", configPath = "scripts/module-graph/layers.json", maxSites = 5, git = true })`
// walks the module tree rooted at `<srcDir>/lib.rs`, resolves every use /
// `crate::`-style path reference to a module, aggregates edges, and checks
// the layer hypothesis + the plan's claims from the layers.json config.
//
// Returned graph (frozen shape): identity { headSha, srcTreeSha,
// contentSha256, srcDir, fileCount, generatedAtISO }; config { path, layers,
// boundaries, expected_back_edges }; modules [ { path, file, parent, cfgs,
// vis, docHidden, layer, layerOrder, crate } ] sorted by path; edges /
// topEdges / nestedEdges [ { source, target, count, sites (first maxSites of
// { file, line, cfg, reexport }), cfgOnly, cfgs } ]; backEdges / cycleEdges
// (nestedEdges + { sourceLayer, targetLayer, sourceCrate, targetCrate });
// claims [ { id, claim, source, status, evidence, measured } ];
// boundaryVisibility [ { crate, restrictedTotal, usedAcrossCount, items } ];
// docRefs [ { module, file, line, kind, segs, target, text } ] (comments
// only, never edges); unresolvedRefs [ { file, line, kind, segs } ];
// externUses; orphans; phantom [ { module, file, line, reason } ]; stats.
//
// Module paths are dot-joined, the root is the string "root". A module is a
// node iff its file exists; inline `mod x { … }` and unresolvable `mod x;`
// go to `phantom`, are not recursed, and their children stay unaddressable.
//
// Documented deviations from the task text (each forced by a contradiction
// inside the spec or by double counting):
//   1. Back-edge direction: the text says "source order > target order" (and
//      layers.json repeats it), but the SAME task requires xthread (7) →
//      global.fallback (8) and segment (4) → route (5) flagged as UPWARD
//      back-edges while route (5) → segment (4), alloc_core (6) → route (5)
//      and registry (7) → platform (2) stay unflagged ("exactly these
//      directions"). With bottom-to-top numbering (1 = primitives … 8 =
//      facade) an upward dependency is source order < target order;
//      implemented so, and the selftest encodes the same direction.
//   2. cycleEdges: ALL nested edges between the layers NAMED "segment" and
//      "route" in EITHER direction (the plan's open order question must be
//      observable both ways; the literal `<=` filter keeps only one).
//   3. `super::` depth: uses keep the marker count (exact); pathRefs/docRefs
//      lose it (parse.mjs strips leading `super` repetitions) — for those,
//      ancestors are tried NEAREST-FIRST (depth 1, 2, … to the root).
//   3b. Module-scope approximation: an unresolvable segment returns the
//      module reached so far once ≥1 leading segment resolved via child
//      descent or importMap hop — the spec's own example
//      (`crate::global::fallback::try_with_heap` → global.fallback) is the
//      1-unresolvable-segment case; `…state::tcache::Tcache::new` →
//      …state.tcache is the 2-segment case. FIRST-segment failures stay in
//      unresolvedRefs (data-quality signal).
//   4. Sites/refs are attributed per UNIQUE FILE to the file's PRIMARY module
//      (empty cfgs, then non-kani cfgs, else first) — else the kani #[path]
//      double-claimed reservation_state.rs would count everything twice.
//   5. importMap keeps the FIRST binding for a name (cfg-disjoint duplicate
//      re-decls exist, e.g. medium_promotion_*).
//   6. externUses counts kind-"extern" uses AND uniform uses whose first
//      segment names an external crate (std/core/alloc prelude + [package]
//      names + dependency keys from the Cargo.toml manifests, `-`/`_`
//      normalised); everything else unresolved lands in unresolvedRefs.
//   7. docRefs carry the raw comment text and resolved target (needed by the
//      claim checks); still never edges.
//   8. sourceCrate/targetCrate use the boundary prefix match (the
//      primitives-verify layer holds TWO crates); falls back to the layer's
//      crate list.
//   9. A code-edge claim with the count satisfied but expect_file/expect_line
//      unmet is "partial", not "confirmed".
//   10. stats.useItems counts every grouped-use leaf item including globs.
//   11. Site objects carry `reexport` (a `pub use` site; pathRefs never are).
//       The "no-incoming-code-edge" claim status counts only PRODUCTION code
//       refs: re-export sites (facade plumbing — the plan counts none of
//       them as usage) and cfg-mentions-`kani` sites (verification-only per
//       CLAUDE.md) are excluded from countedIncomingRefs; the raw counts
//       stay in measured so nothing is hidden.
//
// Usage: `node scripts/module-graph/graph.mjs --selftest` runs the in-memory
// micro-graph self-test; without --selftest it builds the graph of ./src and
// prints a compact JSON summary. Importing the module prints nothing.

import { readFileSync, readdirSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { lexRust } from "./lexer.mjs";
import { parseRustFile } from "./parse.mjs";
import assert from "node:assert/strict";

const MARKERS = new Set(["crate", "super", "self"]);
const RESOLVE_DEPTH_CAP = 32;
const EXTERN_PRELUDE = ["std", "core", "alloc", "test", "proc_macro"];
const DOC_PATH_RE = /(?:crate|super|self)(?:::[A-Za-z_][A-Za-z0-9_]*)+/g;

const toPosix = (p) => p.split("\\").join("/");
const normPosix = (p) => {
  const out = [];
  for (const part of toPosix(p).split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") out.pop();
    else out.push(part);
  }
  return out.join("/");
};
const parentPosix = (p) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : ".");
const basePosix = (p) => (p.includes("/") ? p.slice(p.lastIndexOf("/") + 1) : p);

/** Relative-to-`rootDir` posix paths of every .rs file, sorted. */
function listRsFiles(rootDir) {
  const out = [];
  const walk = (rel) => {
    for (const e of readdirSync(rel === "" ? rootDir : join(rootDir, rel), { withFileTypes: true })) {
      if (e.isDirectory()) walk(rel === "" ? e.name : rel + "/" + e.name);
      else if (e.isFile() && e.name.endsWith(".rs")) out.push(rel === "" ? e.name : rel + "/" + e.name);
    }
  };
  walk("");
  return out.sort();
}

/**
 * External crate names for uniform-use classification: the Rust prelude plus
 * every [package] name and [dev-|build-]dependencies key found in the root
 * and the crates-dir per-crate Cargo.toml manifests (a missing `crates/`
 * dir is ignored, so the selftest works outside a repo). `-` and `_` forms
 * both registered.
 */
function externCrateNames(repoRoot) {
  const names = new Set(EXTERN_PRELUDE);
  const add = (n) => { if (n) { names.add(n); names.add(n.split("-").join("_")); } };
  const manifests = [join(repoRoot, "Cargo.toml")];
  try {
    for (const e of readdirSync(join(repoRoot, "crates"), { withFileTypes: true })) {
      if (e.isDirectory()) manifests.push(join(repoRoot, "crates", e.name, "Cargo.toml"));
    }
  } catch { /* no crates/ dir */ }
  for (const m of manifests) {
    let text;
    try { text = readFileSync(m, "utf8"); } catch { continue; }
    let section = "";
    for (const line of text.split(/\r?\n/)) {
      const head = line.match(/^\s*\[([^\]]+)\]\s*$/);
      if (head !== null) { section = head[1]; continue; }
      const key = line.match(/^\s*([A-Za-z0-9_.-]+)\s*=/);
      if (key === null) continue;
      if (section === "package" && key[1] === "name") add(line.match(/"([^"]+)"/)?.[1]);
      else if (section.endsWith("dependencies")) add(key[1]);
    }
  }
  return names;
}

/** `git rev-parse <ref>` at repoRoot, or null when git is unavailable. */
function gitRev(repoRoot, ref) {
  try {
    const r = spawnSync("git", ["rev-parse", ref], { cwd: repoRoot, encoding: "utf8" });
    return r.status === 0 ? r.stdout.trim() : null;
  } catch { return null; }
}

/** Merge duplicate `mod` entries of one file (cfg-disjoint double decls). */
function mergeMods(entries) {
  const byName = new Map();
  for (const m of entries) {
    let a = byName.get(m.name);
    if (a === undefined) byName.set(m.name, (a = []));
    a.push(m);
  }
  return [...byName.entries()].map(([name, decls]) => ({
    name,
    cfgs: decls.some((d) => !d.cfg) ? [] : decls.map((d) => d.cfg),
    vis: [...new Set(decls.map((d) => d.vis))],
    docHidden: decls.some((d) => d.docHidden),
    pathAttr: (decls.find((d) => d.pathAttr !== null) ?? { pathAttr: null }).pathAttr,
    line: decls[0].line,
    inline: decls.some((d) => d.inline),
  }));
}

/**
 * File of child module `name` declared in `dir`: an explicit `#[path]` is
 * resolved relative to the DECLARING FILE's directory (Rust's rule), else
 * `<dir>/<name>.rs`, else `<dir>/<name>/mod.rs`; null → phantom.
 */
function resolveModFile(fileSet, dir, name, pathAttr) {
  if (pathAttr !== null) {
    const p = normPosix(join(dir, pathAttr));
    return fileSet.has(p) ? p : null;
  }
  const direct = normPosix(join(dir, name + ".rs"));
  if (fileSet.has(direct)) return direct;
  const nested = normPosix(join(dir, name, "mod.rs"));
  return fileSet.has(nested) ? nested : null;
}

const parentOf = (path) => (path === "root" ? null : path.includes(".") ? path.slice(0, path.lastIndexOf(".")) : "root");
const ascend = (path, n) => { let c = path; for (let k = 0; k < n; k++) { c = parentOf(c); if (c === null) return null; } return c; };
const rollPath = (p, depth) => (p === "root" ? "root" : p.split(".").slice(0, depth).join("."));
const inSet = (p, prefixes) => prefixes.some((pref) => p === pref || p.startsWith(pref + "."));
const fmtSite = (s) => s.file + ":" + s.line + (s.cfg ? " [cfg]" : "");
const byFileLine = (a, b) => (a.file + ":" + String(a.line).padStart(6)).localeCompare(b.file + ":" + String(b.line).padStart(6));
const docFileMatches = (f, want) => { const p = toPosix(f); return p === want || p.endsWith("/" + want) || want.endsWith("/" + p); };
const uniqBy = (list, keyOf) => { const seen = new Set(); return list.filter((x) => { const k = keyOf(x); if (seen.has(k)) return false; seen.add(k); return true; }); };
/** Leading crate/super/self entries of a use prefix; count = super depth. */
const markerCountOf = (prefix) => { let n = 0; while (n < prefix.length && MARKERS.has(prefix[n])) n += 1; return n; };
const superDepthOf = (prefix) => { let d = 0; for (const m of prefix) { if (m === "super") d += 1; else if (!MARKERS.has(m)) break; } return Math.max(1, d); };
/** Longest exact-or-'.'-boundary prefix match of `p` among `entries` ({key, make}). */
const longestMatch = (p, entries) => {
  let best = null;
  for (const { key, make } of entries) {
    if ((p === key || p.startsWith(key + ".")) && (best === null || key.length > best.entryLen)) {
      best = { ...make(p, key), entryLen: key.length };
    }
  }
  return best;
};
/**
 * Build the module graph. See the file header for the returned shape and the
 * documented deviations. Pure read-only over srcDir/configPath.
 */
export function buildModuleGraph({ srcDir = "src", configPath = "scripts/module-graph/layers.json", maxSites = 5, git = true } = {}) {
  const srcRoot = resolve(srcDir);
  const configPathAbs = resolve(configPath);
  const fileSet = new Set(listRsFiles(srcRoot));
  const EXTERN_NAMES = externCrateNames(dirname(srcRoot));

  // ── 1. parse cache: one lex+parse per unique file ─────────────────────
  const fileData = new Map();
  const parseOf = (rel) => {
    let d = fileData.get(rel);
    if (d === undefined) {
      const raw = readFileSync(join(srcRoot, rel), "utf8");
      const lx = lexRust(raw);
      d = { raw, comments: lx.comments, parsed: parseRustFile({ raw, code: lx.code, comments: lx.comments, filePath: rel }) };
      fileData.set(rel, d);
    }
    return d;
  };
  // ── 2. module tree walk (root = "root", file lib.rs) ──────────────────────
  const rootFile = "lib.rs";
  if (!fileSet.has(rootFile)) throw new Error(`buildModuleGraph: ${join(srcDir, "lib.rs")} not found`);
  const nodes = new Map();
  const phantom = [];
  const rootNode = { path: "root", name: "root", file: rootFile, parent: null, cfgs: [], vis: ["private"], docHidden: false, line: 0, children: new Map() };
  nodes.set("root", rootNode);
  const expand = (node) => {
    const dir = parentPosix(node.file);
    for (const m of mergeMods(parseOf(node.file).parsed.mods)) {
      const childPath = node.path === "root" ? m.name : node.path + "." + m.name;
      if (m.inline) { phantom.push({ module: childPath, file: node.file, line: m.line, reason: "inline" }); continue; }
      const file = resolveModFile(fileSet, dir, m.name, m.pathAttr);
      if (file === null) { phantom.push({ module: childPath, file: node.file, line: m.line, reason: "missing" }); continue; }
      const child = { path: childPath, name: m.name, file, parent: node.path, cfgs: m.cfgs, vis: m.vis, docHidden: m.docHidden, line: m.line, children: new Map() };
      node.children.set(m.name, child);
      nodes.set(childPath, child);
      expand(child);
    }
  };
  expand(rootNode);

  // ── 3. primary module per file (prefer ungated, then non-kani cfgs) ───────
  const fileModules = new Map();
  const primaryScore = (n) => (n.cfgs.length === 0 ? 2 : n.cfgs.some((c) => c.includes("kani")) ? 0 : 1);
  for (const node of nodes.values()) {
    const prev = fileModules.get(node.file);
    if (prev === undefined || primaryScore(node) > primaryScore(prev)) fileModules.set(node.file, node);
  }

  // ── 4. import maps + globs per node (any visibility) ──────────────────────
  for (const node of nodes.values()) {
    node.importMap = new Map();
    node.globs = [];
    for (const u of parseOf(node.file).parsed.uses) {
      const base = u.prefix.slice(markerCountOf(u.prefix));
      const superCount = superDepthOf(u.prefix);
      for (const item of u.items) {
        const segs = [...base, ...item.segs];
        if (item.glob) { node.globs.push({ kind: u.kind, segs, superDepth: superCount }); continue; }
        const name = item.rename !== null ? item.rename : item.name;
        if (!node.importMap.has(name)) node.importMap.set(name, { kind: u.kind, segs, superDepth: superCount });
      }
    }
  }

  // ── 5. path resolution (module-scope approximation; depth+cycle guarded) ──
  const startScope = (fromPath, kind, superDepth) =>
    kind === "crate" ? "root" : kind === "super" ? ascend(fromPath, Math.max(1, superDepth ?? 1)) : kind === "self" || kind === "uniform" ? fromPath : null;
  function walkSegs(nodePath, segs, st, approx) {
    st.depth += 1;
    try {
      if (st.depth > RESOLVE_DEPTH_CAP) return null;
      if (segs.length === 0) return { module: nodePath, viaGlob: st.viaGlob };
      const node = nodes.get(nodePath);
      if (node === undefined) return null;
      const s = segs[0];
      const rest = segs.slice(1);
      const child = node.children.get(s);
      if (child !== undefined) { st.walked = true; return walkSegs(child.path, rest, st, approx); }
      const binding = node.importMap.get(s);
      if (binding !== undefined) {
        const key = nodePath + "::" + s;
        if (st.visited.has(key)) return rest.length === 0 || st.walked ? { module: nodePath, viaGlob: st.viaGlob } : null;
        st.visited.add(key);
        st.walked = true;
        const t = walkSegsFrom(nodePath, binding, st);
        if (t === null) return rest.length === 0 || st.walked ? { module: nodePath, viaGlob: st.viaGlob } : null;
        if (rest.length === 0) return { module: t.module, viaGlob: st.viaGlob || t.viaGlob };
        return walkSegs(t.module, rest, st, approx);
      }
      for (const g of node.globs) {
        const gt = resolvePath(nodePath, g.kind, g.segs, { approxFinal: false, superDepth: g.superDepth });
        if (gt === null) continue;
        const gChild = nodes.get(gt.module)?.children.get(s);
        if (gChild !== undefined) { st.viaGlob = true; return walkSegs(gChild.path, rest, st, approx); }
      }
      if (approx && (rest.length === 0 || st.walked)) return { module: nodePath, viaGlob: st.viaGlob };
      return null;
    } finally { st.depth -= 1; }
  }
  const walkSegsFrom = (nodePath, binding, st) => {
    const scope = startScope(nodePath, binding.kind, binding.superDepth);
    // The CALLER's state (visited + depth + walked) is shared so re-export
    // cycles hit the guard instead of recursing forever.
    return scope === null ? null : walkSegs(scope, binding.segs, st, true);
  };
  function resolvePath(fromPath, kind, segs, { approxFinal = true, superDepth = 1 } = {}) {
    const scope = startScope(fromPath, kind, superDepth);
    return scope === null ? null : walkSegs(scope, segs, { visited: new Set(), viaGlob: false, depth: 0, walked: false }, approxFinal);
  }
  // For pathRefs/docRefs the parser lost the `super::` repetition count:
  // try ancestors nearest-first (deviation 3).
  const resolveAutoSuper = (fromPath, segs) => {
    for (let d = 1; ; d += 1) {
      const r = resolvePath(fromPath, "super", segs, { superDepth: d });
      if (r !== null) return r;
      if (ascend(fromPath, d) === null) return null;
    }
  };

  // ── 6. edges, sites, docRefs, unresolved/extern tally ─────────────────────
  const edges = new Map();
  const siteRefs = [];
  const unresolvedRefs = [];
  let externUses = 0;
  const addEdge = (source, target, site) => {
    if (source === target) return;
    const key = source + " -> " + target;
    let e = edges.get(key);
    if (e === undefined) edges.set(key, (e = { source, target, count: 0, sites: [], cfgOnly: true, cfgs: [] }));
    e.count += 1;
    if (e.sites.length < maxSites) e.sites.push(site);
    if (!e.cfgs.includes(site.cfg)) e.cfgs.push(site.cfg);
    if (!site.cfg) e.cfgOnly = false;
  };
  const modCfgOf = (node) => (node.cfgs.length > 0 ? node.cfgs.join(" && ") : "");
  let usesCount = 0, useItemsCount = 0, pathRefCount = 0;
  for (const file of fileSet) { // unique files; sites go to the PRIMARY module (deviation 4)
    const node = fileModules.get(file);
    const p = parseOf(file).parsed;
    usesCount += p.uses.length;
    useItemsCount += p.uses.reduce((n, u) => n + u.items.length, 0);
    pathRefCount += p.pathRefs.length;
    if (node === undefined) continue; // orphan file: counted in orphans below
    const modCfg = modCfgOf(node);
    for (const u of p.uses) {
      const base = u.prefix.slice(markerCountOf(u.prefix));
      for (const item of u.items) {
        if (item.glob) continue;
        const segs = [...base, ...item.segs];
        const t = resolvePath(node.path, u.kind, segs, { superDepth: superDepthOf(u.prefix) });
        if (t === null) {
          if (u.kind === "extern" || EXTERN_NAMES.has(segs[0])) externUses += 1;
          else unresolvedRefs.push({ file, line: u.line, kind: u.kind, segs });
          continue;
        }
        const site = { file, line: u.line, cfg: [modCfg, u.cfg].filter(Boolean).join(" && "), reexport: u.pubUse };
        addEdge(node.path, t.module, site);
        siteRefs.push({ fromModule: node.path, file, line: u.line, target: t.module, itemName: item.rename !== null ? item.rename : item.name, cfg: site.cfg, reexport: u.pubUse });
      }
    }
    for (const r of p.pathRefs) {
      const t = r.kind === "super" ? resolveAutoSuper(node.path, r.segs) : resolvePath(node.path, r.kind, r.segs);
      if (t === null) { unresolvedRefs.push({ file, line: r.line, kind: r.kind, segs: r.segs }); continue; }
      const site = { file, line: r.line, cfg: modCfg, reexport: false };
      addEdge(node.path, t.module, site);
      siteRefs.push({ fromModule: node.path, file, line: r.line, target: t.module, itemName: r.segs[r.segs.length - 1], cfg: modCfg, reexport: false });
    }
  }
  const edgesList = [...edges.values()].sort((a, b) => (a.source + a.target).localeCompare(b.source + b.target));

  // Doc references: scan ALL comments (doc + code); never edges (deviation 7).
  const docRefs = [];
  for (const file of fileSet) {
    const mod = fileModules.get(file);
    if (mod === undefined) continue;
    for (const c of parseOf(file).comments) {
      for (const m of c.text.matchAll(DOC_PATH_RE)) {
        const parts = m[0].split("::");
        const kind = parts[0];
        let segs = parts.slice(1);
        while (kind !== "crate" && segs.length > 0 && segs[0] === kind) segs = segs.slice(1);
        if (segs.length === 0) continue;
        const t = kind === "super" ? resolveAutoSuper(mod.path, segs) : resolvePath(mod.path, kind, segs);
        docRefs.push({ module: mod.path, file, line: c.line, kind, segs, target: t === null ? null : t.module, text: c.text });
      }
    }
  }

  // ── 7. rollups with count conservation (drop self-edges AFTER mapping) ────
  const rollupEdges = (list, depth) => {
    const out = new Map();
    for (const e of list) {
      const s = rollPath(e.source, depth), t = rollPath(e.target, depth);
      if (s === t) continue;
      const key = s + " -> " + t;
      let r = out.get(key);
      if (r === undefined) out.set(key, (r = { source: s, target: t, count: 0, sites: [], cfgOnly: true, cfgs: [] }));
      r.count += e.count;
      for (const site of e.sites) {
        if (r.sites.length >= maxSites) break;
        r.sites.push(site);
        if (!r.cfgs.includes(site.cfg)) r.cfgs.push(site.cfg);
        if (!site.cfg) r.cfgOnly = false;
      }
    }
    return [...out.values()].sort((a, b) => b.count - a.count || (a.source + a.target).localeCompare(b.source + b.target));
  };
  const topEdges = rollupEdges(edgesList, 1);
  const nestedEdges = rollupEdges(edgesList, 2);
  const fullTotal = edgesList.reduce((n, e) => n + e.count, 0);
  const selfAfterRoll = edgesList.reduce((n, e) => n + (rollPath(e.source, 2) === rollPath(e.target, 2) ? e.count : 0), 0);
  const nestedTotal = nestedEdges.reduce((n, e) => n + e.count, 0);
  assert.strictEqual(nestedTotal, fullTotal - selfAfterRoll, "rollup count conservation violated");
  const rollupDerivation = `sum(nested.count)=${nestedTotal} = sum(full.count)=${fullTotal} - full-level refs in edges self-equal after depth-2 mapping=${selfAfterRoll}`;

  // ── 8. layers, back-edges, segment/route cycle edges ──────────────────────
  const config = JSON.parse(readFileSync(configPathAbs, "utf8"));
  const layerOf = (p) => {
    let best = null;
    for (const L of config.layers ?? []) {
      for (const m of L.modules ?? []) {
        if ((p === m || p.startsWith(m + ".")) && (best === null || m.length > best.entryLen)) {
          best = { order: L.order, name: L.name, crates: L.crates ?? [], entryLen: m.length };
        }
      }
    }
    return best;
  };
  const boundaryOf = (p) => {
    let best = null;
    for (const B of config.boundaries ?? []) {
      for (const pref of B.prefixes ?? []) {
        if ((p === pref || p.startsWith(pref + ".")) && (best === null || pref.length > best.entryLen)) {
          best = { crate: B.crate, entryLen: pref.length };
        }
      }
    }
    return best === null ? null : best.crate;
  };
  const segLayer = (config.layers ?? []).find((l) => l.name === "segment");
  const rteLayer = (config.layers ?? []).find((l) => l.name === "route");
  const annotate = (e, s, t) => ({
    ...e,
    sourceLayer: { order: s.order, name: s.name },
    targetLayer: { order: t.order, name: t.name },
    sourceCrate: boundaryOf(e.source) ?? s.crates.join("/"),
    targetCrate: boundaryOf(e.target) ?? t.crates.join("/"),
  });
  const backEdges = [];
  const cycleEdges = [];
  for (const e of nestedEdges) {
    const s = layerOf(e.source), t = layerOf(e.target);
    if (s === null || t === null) continue; // unmapped module: not comparable
    if (s.order < t.order) backEdges.push(annotate(e, s, t)); // DEVIATION 1
    const pair = [s.order, t.order];
    if (segLayer !== undefined && rteLayer !== undefined && s.order !== t.order &&
        (pair.join(",") === [segLayer.order, rteLayer.order].join(",") ||
         pair.join(",") === [rteLayer.order, segLayer.order].join(","))) {
      cycleEdges.push(annotate(e, s, t)); // DEVIATION 2: both directions
    }
  }

  // ── 9. claims from expected_back_edges ────────────────────────────────────
  const evaluateClaim = (c) => {
    const from = (p) => inSet(p, c.from_prefixes ?? []);
    const to = (p) => inSet(p, c.to_prefixes ?? []);
    const matched = nestedEdges.filter((e) => from(e.source) && to(e.target));
    const totalRefs = matched.reduce((n, e) => n + e.count, 0);
    const base = { id: c.id, claim: c.claim, source: c.source };
    if (c.expect === "code-edge") {
      const sites = uniqBy(matched.flatMap((e) => e.sites), (s) => s.file + ":" + s.line);
      const fileSeen = c.expect_file === undefined || sites.some((s) => basePosix(s.file) === c.expect_file);
      const lineSeen = c.expect_line === undefined || sites.some((s) => basePosix(s.file) === c.expect_file && s.line === c.expect_line);
      const status = totalRefs === 0 ? "not-found"
        : (c.expect_min_sites === undefined || totalRefs >= c.expect_min_sites) && fileSeen && lineSeen ? "confirmed" : "partial";
      return { ...base, status, evidence: sites.slice(0, 20).map(fmtSite), measured: { matchedEdges: matched.length, totalRefs, expectMinSites: c.expect_min_sites ?? null, expectFileSeen: fileSeen, expectLineSeen: lineSeen } };
    }
    if (c.expect === "doc-only") {
      const docHits = docRefs.filter((d) => from(d.module) && d.target !== null && to(d.target));
      const status = totalRefs > 0 ? "refuted" : docHits.length > 0 ? "confirmed" : "not-found";
      return { ...base, status, evidence: status === "refuted" ? matched.flatMap((e) => e.sites).slice(0, 20).map(fmtSite) : docHits.slice(0, 10).map((d) => d.file + ":" + d.line + " " + d.kind + "::" + d.segs.join("::")), measured: { codeEdges: matched.length, codeRefs: totalRefs, matchingDocRefs: docHits.length } };
    }
    if (c.expect === "no-incoming-code-edge") {
      const rawIncoming = siteRefs.filter((r) => to(r.target) && !to(r.fromModule));
      // Production-code refs only: pub re-exports are facade plumbing and
      // kani-gated sites are verification-only (deviation 11).
      const counted = rawIncoming.filter((r) => !r.reexport && !r.cfg.includes("kani"));
      let docFound = null;
      if (c.expect_doc_comment !== undefined) {
        docFound = false;
        for (const file of fileSet) {
          if (!docFileMatches(file, c.expect_doc_comment.file)) continue;
          if (parseOf(file).comments.some((cm) => cm.text.includes(c.expect_doc_comment.contains))) { docFound = true; break; }
        }
      }
      const status = counted.length > 0 ? "refuted" : c.expect_doc_comment !== undefined && !docFound ? "not-found" : "confirmed";
      const evidence = counted.length > 0 ? counted.slice(0, 20).map((r) => r.file + ":" + r.line + (r.cfg ? " [cfg]" : ""))
        : docFound ? ["comment mentioning " + JSON.stringify(c.expect_doc_comment.contains) + " present in " + c.expect_doc_comment.file] : [];      return { ...base, status, evidence, measured: { incomingRefsRaw: rawIncoming.length, countedIncomingRefs: counted.length, excludedReexportRefs: rawIncoming.filter((r) => r.reexport).length, excludedKaniCfgRefs: rawIncoming.filter((r) => !r.reexport && r.cfg.includes("kani")).length, docCommentFound: c.expect_doc_comment !== undefined ? docFound : null } };
    }
    return { ...base, status: "error", evidence: [], measured: {}, reason: "unknown expect kind: " + String(c.expect) };
  };
  const claims = (config.expected_back_edges ?? []).map(evaluateClaim);
  for (const c of claims) {
    assert.ok(["confirmed", "partial", "refuted", "not-found", "error"].includes(c.status), `claim ${c.id}: no status`);
  }

  // ── 10. boundary visibility (restricted decls used across the boundary) ──
  const boundaryVisibility = (config.boundaries ?? []).map((B) => {
    const inside = (p) => p !== null && p !== undefined && inSet(p, B.prefixes ?? []);
    const declsByFile = new Map();
    let restrictedTotal = 0;
    for (const file of fileSet) {
      if (!inside(fileModules.get(file)?.path ?? null)) continue;
      const decls = uniqBy(parseOf(file).parsed.restrictedDecls, (d) => d.line + "|" + d.name);
      declsByFile.set(file, decls);
      restrictedTotal += decls.length;
    }
    const used = new Map(); // declKey(file|line|name) -> { decl, declFile, users: Map }
    let usedAcrossCount = 0;
    for (const r of siteRefs) {
      if (boundaryOf(r.fromModule) === B.crate) continue;
      if (boundaryOf(r.target) !== B.crate) continue;
      const tFile = nodes.get(r.target)?.file;
      if (tFile === undefined || boundaryOf(fileModules.get(tFile)?.path ?? null) !== B.crate) continue;
      const decls = declsByFile.get(tFile);
      if (decls === undefined) continue;
      for (const d of decls) {
        if (d.name !== r.itemName) continue;
        const key = tFile + "|" + d.line + "|" + d.name;
        let u = used.get(key);
        if (u === undefined) used.set(key, (u = { decl: d, declFile: tFile, users: new Map() }));
        const siteKey = r.file + ":" + r.line;
        if (!u.users.has(siteKey)) { u.users.set(siteKey, { file: r.file, line: r.line }); usedAcrossCount += 1; }
      }
    }
    const items = [...used.values()].map((u) => ({
      name: u.decl.name, declKind: u.decl.declKind, vis: u.decl.vis, declFile: u.declFile, declLine: u.decl.line,
      users: [...u.users.values()].sort(byFileLine).slice(0, 8).map((x) => x.file + ":" + x.line),
      userCount: u.users.size,
    })).sort((a, b) => b.userCount - a.userCount || a.declFile.localeCompare(b.declFile) || a.declLine - b.declLine);
    assert.strictEqual(items.reduce((n, i) => n + i.userCount, 0), usedAcrossCount, `boundary ${B.crate}: usedAcrossCount cross-check`);
    return { crate: B.crate, restrictedTotal, usedAcrossCount, items };
  });

  // ── 11. identity, stats, final asserts ────────────────────────────────────
  const hash = createHash("sha256");
  for (const file of fileSet) {
    hash.update(file + "\0");
    hash.update(readFileSync(join(srcRoot, file)));
    hash.update("\0");
  }
  const repoRoot = dirname(srcRoot);
  const identity = {
    headSha: git ? gitRev(repoRoot, "HEAD") : null,
    srcTreeSha: git ? gitRev(repoRoot, "HEAD:src") : null,
    contentSha256: hash.digest("hex"),
    srcDir,
    fileCount: fileSet.size,
    generatedAtISO: new Date().toISOString(),
  };  const known = new Set(nodes.keys());
  for (const e of edgesList) {
    assert.ok(known.has(e.source), `edge source not a known module: ${e.source}`);
    assert.ok(known.has(e.target), `edge target not a known module: ${e.target}`);
  }
  const orphans = [...fileSet].filter((f) => !fileModules.has(f));
  const moduleList = [...nodes.values()].map((n) => {
    const L = layerOf(n.path);
    return { path: n.path, file: n.file, parent: n.parent, cfgs: n.cfgs, vis: n.vis, docHidden: n.docHidden, layer: L === null ? null : L.name, layerOrder: L === null ? null : L.order, crate: boundaryOf(n.path) };
  }).sort((a, b) => a.path.localeCompare(b.path));
  const stats = {
    files: fileSet.size, modules: nodes.size, uses: usesCount, useItems: useItemsCount, pathRefs: pathRefCount,
    docRefs: docRefs.length, unresolvedRefs: unresolvedRefs.length, externUses, edges: edgesList.length,
    topEdges: topEdges.length, nestedEdges: nestedEdges.length, backEdges: backEdges.length, cycleEdges: cycleEdges.length,
    orphans: orphans.length, phantom: phantom.length, unmappedLayers: moduleList.filter((m) => m.layer === null).length,
  };
  return {
    identity,
    config: { path: configPathAbs, layers: config.layers ?? [], boundaries: config.boundaries ?? [], expected_back_edges: config.expected_back_edges ?? [] },
    modules: moduleList,
    edges: edgesList,
    topEdges,
    nestedEdges,
    backEdges,
    cycleEdges,
    claims,
    boundaryVisibility,
    docRefs,
    unresolvedRefs,
    externUses,
    orphans,
    phantom,
    stats,
    rollupDerivation,
  };
}

// ── CLI: compact JSON summary of the real tree / in-memory selftest ──────────

function main() {
  const g = buildModuleGraph({});
  const annotated = ({ source, target, count, sites, sourceLayer, targetLayer, sourceCrate, targetCrate }) =>
    ({ source, target, count, sites, sourceLayer, targetLayer, sourceCrate, targetCrate });
  console.log(JSON.stringify({
    identity: { headSha: g.identity.headSha, srcTreeSha: g.identity.srcTreeSha, contentSha256: g.identity.contentSha256, fileCount: g.identity.fileCount, generatedAtISO: g.identity.generatedAtISO },
    stats: g.stats,
    rollupDerivation: g.rollupDerivation,
    claims: g.claims.map((c) => ({ id: c.id, status: c.status, measured: c.measured, evidence: c.evidence })),
    backEdges: g.backEdges.map(annotated),
    cycleEdges: g.cycleEdges.map(annotated),
    topEdges: g.topEdges,
    nestedEdges: g.nestedEdges,
    boundaryVisibility: g.boundaryVisibility,
    orphans: g.orphans,
    phantom: g.phantom,
    unresolvedRefs: { count: g.unresolvedRefs.length, sample: g.unresolvedRefs.slice(0, 20) },
    externUses: g.externUses,
  }, null, 1));
}

function selftest() {
  let checks = 0;
  const ok = (cond, msg) => { checks += 1; assert.ok(cond, msg); };
  const tmp = mkdtempSync(join(tmpdir(), "sefer-modgraph-"));
  try {
    const src = join(tmp, "src");
    mkdirSync(join(src, "m2"), { recursive: true });
    writeFileSync(join(src, "lib.rs"), '/// see crate::m2\n#[cfg(feature = "a")]\nmod m1;\nmod m2;\npub use crate::m1::Thing;\n');
    writeFileSync(join(src, "m1.rs"), "pub struct Thing;\nuse crate::m2::helper;\n");
    writeFileSync(join(src, "m2", "mod.rs"), "pub fn helper() {}\npub(in crate::m1) fn f2() {}\nuse crate::m1::Thing;\n");
    const cfgPath = join(tmp, "layers.json");
    // Order encodes the same direction as the real tree (deviation 1): m2 sits
    // ABOVE m1, so m1 -> m2 is the dependency pointing up the stack.
    writeFileSync(cfgPath, JSON.stringify({
      root_module: "root",
      layers: [{ order: 1, name: "lower", modules: ["m1", "root"] }, { order: 2, name: "upper", modules: ["m2"] }],
      boundaries: [],
      expected_back_edges: [],
    }));
    const g = buildModuleGraph({ srcDir: src, configPath: cfgPath, git: false });
    ok(g.modules.length === 3 && ["root", "m1", "m2"].every((p) => g.modules.some((m) => m.path === p)), "three module nodes: root, m1, m2");
    ok(g.modules.some((m) => m.path === "m1" && m.cfgs.length === 1 && m.cfgs[0] === 'feature = "a"'), "m1 keeps its cfg predicate");
    const edge = (s, t) => g.edges.find((e) => e.source === s && e.target === t);
    ok(edge("m1", "m2") !== undefined, "edge m1 -> m2 found");
    ok(edge("m2", "m1") !== undefined, "edge m2 -> m1 found");
    ok(edge("root", "m1") !== undefined, "re-export edge root -> m1 found");
    ok(g.edges.length === 3, "exactly three edges");
    ok(edge("m2", "m1").count === 1, "pub(in crate::m1) f2 created no edge (count stays 1)");
    ok(edge("root", "m2") === undefined, "doc comment crate::m2 did not create an edge");
    const d = g.docRefs.find((x) => x.module === "root" && x.target === "m2");
    ok(d !== undefined && d.kind === "crate" && d.segs.join(".") === "m2", "doc ref recorded in docRefs with resolved target");
    ok(g.backEdges.length === 1 && g.backEdges[0].source === "m1" && g.backEdges[0].target === "m2", "m1 -> m2 flagged as back-edge (layer 1 -> 2)");
    ok(g.orphans.length === 0 && g.phantom.length === 0, "micro tree fully claimed, no phantom mods");
    ok(g.identity.headSha === null && g.identity.srcTreeSha === null, "git=false skips git identity");
    ok(g.unresolvedRefs.length === 0 && g.externUses === 0, "no unresolved/extern refs in the micro tree");
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  console.log(`graph selftest: ok (${checks} checks)`);
}

function runDirectly() {
  if (!process.argv[1]) return false;
  try {
    return import.meta.url === pathToFileURL(process.argv[1]).href;
  } catch {
    return false;
  }
}

if (runDirectly()) {
  if (process.argv.includes("--selftest")) selftest();
  else main();
}
