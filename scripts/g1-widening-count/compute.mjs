// Core computation of scripts/g1-widening-count.mjs (the G1 widening-count
// input, plan docs/plans/2026-11-15-workspace-restructure-plan.md §G1 / §5 P2).
//
// Question answered: how many currently restricted-visibility items
// (`pub(crate)` / `pub(super)` / `pub(in …)`) would have to be widened to
// `pub` if the P2 crate split (layers.json section `boundaries`) happened.
//
// Method (conservative by design — see the report's Method section):
//   - The module graph, the conduit semantics and the symbol-name → module
//     resolution are REUSED, not reimplemented: `buildModuleGraph` from
//     scripts/module-graph/graph.mjs is called with a generated config whose
//     `boundaries` section is the P2 layout under the variant being counted
//     (split: sefer-segment and sefer-route separate; merged: one
//     sefer-segment-route). graph.mjs resolves `crate::`/`super::`/grouped
//     `use` and re-export hops through the declared conduits (alloc_core,
//     registry) to the DEFINING module; its `boundaryVisibility` rollup
//     already matches resolved target-module declarations by symbol name and
//     counts each item once per boundary with its user sites.
//   - NOT available in module-graph and therefore handled here: (a) `use`
//     re-export declarations inside the conduit mod.rs files themselves
//     (their primary module maps to no boundary, so boundaryVisibility skips
//     them) — attributed here to the boundary of the module the re-export
//     hop resolves to, via the conduit's own re-export edge sites; (b) the
//     raw-pointer/`unsafe` safety signal, which needs the declaration's
//     signature text (lexer + a bounded signature scan, reusing
//     scripts/module-graph/lexer.mjs so indexes/lines stay valid).
//   - Private (no `pub`) declarations are NOT counted: in the current single
//     crate they are unreachable from any module that would end up in a
//     different crate except through a restricted re-export, which IS
//     counted as its own declaration. Undercount, never overcount.
//   - Coverage assert: every module of src/ must map to exactly one boundary
//     of the variant, or be a declared conduit, or be the root — otherwise
//     the script fails loudly instead of silently dropping modules.
//   - Count conservation assert: sum(per-boundary item counts) === total
//     unique item count; each item key `file|line|name` appears exactly once.

import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { buildModuleGraph } from "../module-graph/graph.mjs";
import { lexRust } from "../module-graph/lexer.mjs";
import { parseRustFile } from "../module-graph/parse.mjs";
import assert from "node:assert/strict";

const ROOT_MODULE = "root";

/** Default P2 boundaries are taken from layers.json `boundaries` as-is. */
export function splitBoundaries(config) {
  return config.boundaries ?? [];
}

/** Merged variant: sefer-segment + sefer-route become one crate. */
export function mergedBoundaries(config) {
  const out = [];
  const segPrefixes = [];
  let merged = false;
  for (const B of config.boundaries ?? []) {
    if (B.crate === "sefer-segment" || B.crate === "sefer-route") {
      segPrefixes.push(...(B.prefixes ?? []));
      merged = true;
    } else out.push(B);
  }
  if (!merged) {
    // Generic configs (tests, other trees) without these crates: the merged
    // variant IS the split variant and the difference is empty.
    return splitBoundaries(config);
  }
  segPrefixes.sort();
  out.push({ crate: "sefer-segment-route", prefixes: segPrefixes });
  return out.sort((a, b) => a.crate.localeCompare(b.crate));
}

const inSet = (p, prefixes) => prefixes.some((pref) => p === pref || p.startsWith(pref + "."));

/** Longest exact-or-'.'-boundary prefix match; null when no boundary claims the module. */
export function boundaryOf(modulePath, boundaries) {
  let best = null;
  for (const B of boundaries) {
    for (const pref of B.prefixes ?? []) {
      if ((modulePath === pref || modulePath.startsWith(pref + ".")) && (best === null || pref.length > best.entryLen)) {
        best = { crate: B.crate, entryLen: pref.length };
      }
    }
  }
  return best === null ? null : best.crate;
}

const itemKey = (item) => item.declFile + "|" + item.declLine + "|" + item.name;

/** File texts are read once; `code` (lexed) keeps indexes/lines valid. */
function parseFileCached(cache, srcDir, file) {
  let entry = cache.get(file);
  if (entry === undefined) {
    const raw = readFileSync(join(srcDir, file), "utf8");
    const { code, comments } = lexRust(raw);
    entry = { raw, code, comments, parsed: parseRustFile({ raw, code, comments, filePath: file }) };
    cache.set(file, entry);
  }
  return entry;
}

/**
 * Safety signal of a restricted declaration (CLAUDE.md boundary rule:
 * `pub unsafe fn` + `# Safety` on a crate boundary). Signals: `unsafe` in
 * the declaration, `*const`/`*mut` in the signature, or the identifier
 * names PointerType / NonNull / raw in the signature. `use` re-exports have
 * no signature and carry no signal. The signature scan takes the declaration
 * line plus following lines up to the first `{` or `;` (cap 20 lines) of the
 * LEXED code, so strings/comments cannot forge a signal.
 */
export function safetySignal(entry, declLine) {
  if (entry === null) return [];
  const lines = entry.code.split("\n");
  const start = Math.max(0, declLine - 1);
  const sig = [];
  for (let i = start; i < Math.min(lines.length, start + 20); i += 1) {
    sig.push(lines[i]);
    if (/[{;]/.test(lines[i])) break;
  }
  const text = sig.join(" ");
  const out = [];
  if (/(^|[^A-Za-z0-9_])unsafe([^A-Za-z0-9_]|$)/.test(text)) out.push("unsafe");
  if (/\*\s*(const|mut)/.test(text)) out.push("raw-ptr");
  if (/(^|[^A-Za-z0-9_])(PointerType|NonNull|raw[A-Za-z0-9_]*)/.test(text)) out.push("pointer-name");
  return out;
}

/**
 * Count widened items for ONE boundary variant. `srcDir`/`configPath` locate
 * the tree and the base layers.json; `boundaries` is the variant's boundary
 * list (already materialised). Returns { boundaries, perBoundary, total,
 * splitOnlyNote: null, unresolvedConduitDecls, coverageErrors }.
 */
export function computeVariant({ srcDir = "src", configPath = "scripts/module-graph/layers.json", boundaries }) {
  const baseConfig = JSON.parse(readFileSync(configPath, "utf8"));
  const conduitSet = new Set((baseConfig.conduits ?? []).flatMap((c) => c.modules ?? []));
  const variantConfig = {
    ...baseConfig,
    boundaries: [...boundaries].sort((a, b) => a.crate.localeCompare(b.crate)),
  };
  const tmp = mkdtempSync(join(tmpdir(), "g1-widening-"));
  let graph;
  try {
    const cfgPath = join(tmp, "layers.json");
    writeFileSync(cfgPath, JSON.stringify(variantConfig));
    // maxSites: effectively unbounded so every re-export site of the conduit
    // mod.rs files is visible for the conduit-decl attribution below.
    graph = buildModuleGraph({ srcDir, configPath: cfgPath, maxSites: 1e9, git: false });
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }

  const moduleByPath = new Map(graph.modules.map((m) => [m.path, m]));
  const boundaryByPath = new Map(graph.modules.map((m) => [m.path, boundaryOf(m.path, boundaries)]));

  // Coverage assert: every module is claimed by exactly one boundary, or is
  // a declared conduit, or is the root.
  const coverageErrors = [];
  for (const m of graph.modules) {
    if (m.path === ROOT_MODULE || conduitSet.has(m.path)) continue;
    if (boundaryByPath.get(m.path) === null) coverageErrors.push(m.path);
  }
  if (coverageErrors.length > 0) {
    throw new Error(
      "boundary coverage violated: modules mapped to no P2 boundary (and not conduits/root): " +
        coverageErrors.sort().join(", "),
    );
  }

  // Items from the graph's own boundaryVisibility rollup (restricted decls of
  // modules inside the boundary, matched by symbol name against refs resolving
  // into the boundary from outside).
  const byKey = new Map(); // key -> item
  for (const B of variantConfig.boundaries) {
    const bv = graph.boundaryVisibility.find((x) => x.crate === B.crate);
    if (bv === undefined) throw new Error(`boundaryVisibility missing for boundary ${B.crate}`);
    for (const it of bv.items) {
      const item = {
        name: it.name,
        declKind: it.declKind,
        vis: it.vis,
        declFile: it.declFile,
        declLine: it.declLine,
        userCount: it.userCount,
        users: it.users,
      };
      const key = itemKey(item);
      if (byKey.has(key)) throw new Error(`item counted twice: ${key}`);
      byKey.set(key, item);
    }
  }

  // Conduit `pub(crate) use` re-export declarations: their file's primary
  // module is the conduit itself (no boundary), so boundaryVisibility skips
  // them. Attribute each to the boundary of the module its re-export hop
  // resolves to, via the conduit's own re-export edge sites.
  const unresolvedConduitDecls = [];
  const fileCache = new Map();
  for (const conduit of conduitSet) {
    const node = moduleByPath.get(conduit);
    if (node === undefined) continue; // graph reports it in stats, not modules — nothing to do
    const entry = parseFileCached(fileCache, srcDir, node.file);
    for (const d of entry.parsed.restrictedDecls) {
      if (d.declKind !== "use") {
        // A non-use restricted decl in a conduit mod.rs would be a CLAUDE.md
        // violation (mod.rs reexports only); report, do not count.
        unresolvedConduitDecls.push({ file: node.file, line: d.line, name: d.name, reason: "non-use decl in conduit mod.rs" });
        continue;
      }
      const site = graph.edges
        .filter((e) => e.source === conduit)
        .flatMap((e) => e.sites.map((s) => ({ e, s })))
        .find(({ s }) => s.file === node.file && s.line === d.line && s.reexport);
      if (site === undefined) {
        unresolvedConduitDecls.push({ file: node.file, line: d.line, name: d.name, reason: "no re-export edge site" });
        continue;
      }
      const targetBoundary = boundaryByPath.get(site.e.target) ?? null;
      if (targetBoundary === null) {
        unresolvedConduitDecls.push({ file: node.file, line: d.line, name: d.name, reason: "re-export target outside every boundary: " + site.e.target });
        continue;
      }
      const item = {
        name: d.name,
        declKind: d.declKind,
        vis: d.vis,
        declFile: node.file,
        declLine: d.line,
        boundary: targetBoundary,
        userCount: site.e.count,
        users: site.e.sites.filter((s) => s.file !== node.file || !s.reexport).slice(0, 8).map((s) => s.file + ":" + s.line),
      };
      const key = itemKey(item);
      if (byKey.has(key)) throw new Error(`conduit item counted twice: ${key}`);
      byKey.set(key, item);
    }
  }

  // Attach boundary + safety signal to every item.
  const items = [...byKey.values()].map((it) => {
    const declModule = moduleByPath.get(
      [...moduleByPath.values()].find((m) => m.file === it.declFile)?.path ?? "",
    );
    const boundary = it.boundary ?? boundaryByPath.get(declModule?.path ?? "") ?? null;
    if (boundary === null) throw new Error(`item ${itemKey(it)} maps to no boundary`);
    const entry = it.declKind === "use" ? null : parseFileCached(fileCache, srcDir, it.declFile);
    return { ...it, boundary, safety: safetySignal(entry, it.declLine) };
  });

  const perBoundary = variantConfig.boundaries.map((B) => ({
    crate: B.crate,
    prefixes: [...(B.prefixes ?? [])].sort(),
    items: items
      .filter((it) => it.boundary === B.crate)
      .sort((a, b) => a.declFile.localeCompare(b.declFile) || a.declLine - b.declLine || a.name.localeCompare(b.name)),
  }));
  perBoundary.sort((a, b) => a.crate.localeCompare(b.crate));

  const total = byKey.size;
  const sum = perBoundary.reduce((n, b) => n + b.items.length, 0);
  assert.strictEqual(sum, total, "count conservation: sum(per-boundary) === total items");
  const seen = new Set();
  for (const it of items) {
    const key = itemKey(it);
    assert.ok(!seen.has(key), `each item exactly once: ${key}`);
    seen.add(key);
  }

  const identity = {
    contentSha256: graph.identity.contentSha256,
    srcTreeSha: graph.identity.srcTreeSha,
    fileCount: graph.identity.fileCount,
    unresolvedRefs: graph.stats.unresolvedRefs,
  };
  return { boundaries: variantConfig.boundaries, perBoundary, total, identity, unresolvedConduitDecls };
}

/** Items widened cross-crate ONLY in the split variant (split minus merged). */
export function splitOnlyItems(split, merged) {
  const mergedKeys = new Set();
  for (const b of merged.perBoundary) for (const it of b.items) mergedKeys.add(itemKey(it));
  const out = [];
  for (const b of split.perBoundary) {
    for (const it of b.items) {
      if (!mergedKeys.has(itemKey(it))) out.push({ ...it, boundary: b.crate });
    }
  }
  return out.sort((a, b) => a.declFile.localeCompare(b.declFile) || a.declLine - b.declLine || a.name.localeCompare(b.name));
}
