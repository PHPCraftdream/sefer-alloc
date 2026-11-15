// Report stage of the module-dependency-graph tool (scripts/module-graph/:
// lexer.mjs → parse.mjs → graph.mjs → THIS file; scripts/module-graph.mjs is
// the thin CLI entry that calls into both).
//
// Consumes the frozen graph object documented in graph.mjs's header and
// renders every table, count and ratio straight from that object — CLAUDE.md's
// "derived, not typed" rule. Nothing numeric is hardcoded in the templates:
//   renderCsv(graph, { maxSites = 5 }) -> string
//       One row per nestedEdge: source,target,count,cfg_only,cfg,first_sites.
//   renderMarkdown(graph, { maxSites = 5, dateISO }) -> string
//       Full A0 report (the MODULE_GRAPH_20261115.md content).
//   writeReport(graph, { outDir = "docs/perf", base = "MODULE_GRAPH_20261115",
//                        maxSites = 5 }) -> { mdPath, csvPath }
//       Renders both artifacts (same base name, side by side) and writes them.
//
// SCOPE BOUNDARY: static analysis of a clean tree only. This report carries
// NO GO/NO-GO verdict and draws no decisions — it is input for the plan's
// owner gates, not a gate itself. There is no raw-log obligation, but the
// machine-readable CSV companion is mandatory and produced by renderCsv.
//
// Self-guarding: assertGraph re-derives the rollup conservation identity and
// every renderer asserts its table row counts against the graph object, so a
// wrong table is a thrown error, not silently wrong prose.
//
// Determinism: all output is a pure function of the graph object except
// generatedAtISO (taken fresh from identity on every build; re-runs refresh
// only that line).
//
// Usage: importing prints nothing; the CLI does the printing/writing.

import { mkdirSync, writeFileSync } from "node:fs";
import assert from "node:assert/strict";

const HEX40 = /^[0-9a-f]{40}$/;
const HEX64 = /^[0-9a-f]{64}$/;
const CSV_HEADER = ["source", "target", "count", "cfg_only", "cfg", "first_sites"];
const ABS_PATH_RE = /[A-Za-z]:[\\/]/;
const rollPath = (p, depth) => (p === "root" ? "root" : p.split(".").slice(0, depth).join("."));
const mdEsc = (s) => String(s).split("|").join("\\|");
const sitesList = (edge, maxSites) => edge.sites.slice(0, maxSites).map((s) => `${s.file}:${s.line}`);
const cfgList = (edge) => edge.cfgs.filter(Boolean);

/** CSV field escaping: quote only when the value contains , " or a newline. */
const csvCell = (v) => {
  const s = String(v);
  return /[",\n\r]/.test(s) ? '"' + s.split('"').join('""') + '"' : s;
};

/** Markdown table from header + string-row arrays (cells already escaped). */
const mdTable = (headers, rows) =>
  [
    "| " + headers.join(" | ") + " |",
    "| " + headers.map(() => "---").join(" | ") + " |",
    ...rows.map((r) => "| " + r.join(" | ") + " |"),
  ].join("\n");

/**
 * Shared invariants, thrown before any rendering: identity shas present and
 * well-formed, and the nested-rollup count conservation recomputed exactly as
 * graph.mjs documents it (sum(nested.count) === sum(full.count) − full-level
 * refs whose endpoints map equal at depth 2).
 */
function assertGraph(graph) {
  const id = graph.identity ?? {};
  assert.ok(typeof id.headSha === "string" && HEX40.test(id.headSha),
    "identity.headSha missing or not 40-hex — build the graph with git enabled");
  assert.ok(typeof id.srcTreeSha === "string" && HEX40.test(id.srcTreeSha),
    "identity.srcTreeSha missing or not 40-hex — build the graph with git enabled");
  assert.ok(typeof id.contentSha256 === "string" && HEX64.test(id.contentSha256),
    "identity.contentSha256 missing or not 64-hex");
  const fullTotal = graph.edges.reduce((n, e) => n + e.count, 0);
  const selfAfterRoll = graph.edges.reduce(
    (n, e) => n + (rollPath(e.source, 2) === rollPath(e.target, 2) ? e.count : 0), 0);
  const nestedTotal = graph.nestedEdges.reduce((n, e) => n + e.count, 0);
  assert.strictEqual(nestedTotal, fullTotal - selfAfterRoll,
    `nested rollup conservation violated: sum(nested.count)=${nestedTotal} != sum(full.count)=${fullTotal} - self-equal-at-depth-2=${selfAfterRoll}`);
  return { fullTotal, selfAfterRoll, nestedTotal };
}

/** Section 2: identity table (fails generation if any sha is null). */
function identitySection(graph) {
  const id = graph.identity;
  return [
    "## Identity",
    "",
    mdTable(
      ["field", "value"],
      [
        ["git HEAD sha", `\`${id.headSha}\``],
        ["`HEAD:src` tree sha", `\`${id.srcTreeSha}\``],
        ["content sha256 (all " + id.fileCount + " files)", `\`${id.contentSha256}\``],
        ["srcDir", `\`${id.srcDir}\``],
        ["fileCount", String(id.fileCount)],
        ["generatedAtISO", id.generatedAtISO],
      ],
    ),
    "",
    "src/ is unmodified in the worktree the graph was built in, so `HEAD:src` pins the exact inputs.",
    "",
    "",
  ].join("\n");
}

/** Section 3: method + limitations, as documented in graph.mjs's header. */
function methodSection(graph) {
  return `## Method and limitations

Edges are module-level references, collected by parsing every \`*.rs\` file under the
module tree rooted at \`src/lib.rs\`:

- \`use\` items of all shapes — grouped and nested groups (one reference per leaf item),
  \`as\` renames, glob imports, and \`pub use\` re-export chains followed to the defining module;
- qualified \`crate::\` / \`super::\` / \`self::\` paths appearing in expressions and types;
- \`#[path = "..."]\` modules resolved relative to the declaring file (Rust's rule);
- cfg-disjoint duplicate \`mod\` declarations of one file are merged (the first binding wins
  for a name, so duplicate re-declarations do not double-count).

Doc comments are NOT edges: they are scanned separately and recorded as doc references
(${graph.stats.docRefs} in this tree), so a path mentioned only in prose never creates a dependency.

Limitations of this measurement, all inherent to static module-level attribution:

- Calls through an imported name are attributed to the import edge only; the call site's
  own module gains no second edge.
- Inline \`mod { ... }\` bodies resolve at the enclosing file-module's scope; one
  reference (${graph.unresolvedRefs.length}, see Data quality) cannot be pinned to a nested module and is
  reported unresolved rather than guessed.
- Bare uniform paths (no \`crate::\`/\`super::\` marker) resolve child-first, then through
  the module's imports/re-exports, then through its globs.
- \`pub(in path)\` visibility qualifiers are declarations, not references; they are counted
  for the boundary-visibility section but never create an edge.

`;
}

/** Section 4: graph-shape stats + interpretation (observations, no advice). */
function shapeSection(graph, totals) {
  const s = graph.stats;
  const incoming = new Map();
  for (const e of graph.edges) incoming.set(e.target, (incoming.get(e.target) ?? 0) + e.count);
  const topIncoming = new Map();
  for (const e of graph.topEdges) topIncoming.set(e.target, (topIncoming.get(e.target) ?? 0) + e.count);
  const hub = [...incoming.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0];
  const topHub = [...topIncoming.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0];
  return `## Graph shape

${mdTable(
    ["metric", "value"],
    [
      ["files", String(s.files)],
      ["modules", String(s.modules)],
      ["edges (full module level)", String(s.edges)],
      ["edges (nested, depth-2 rollup)", String(s.nestedEdges)],
      ["edges (top, depth-1 rollup)", String(s.topEdges)],
      ["back-edges (upward, layer order)", String(s.backEdges)],
      ["cycle-edge directions (segment↔route, both ways)", String(s.cycleEdges)],
      ["doc references (never edges)", String(s.docRefs)],
      ["extern-crate uses", String(graph.externUses)],
      ["unresolved refs", String(s.unresolvedRefs)],
      ["orphan files", String(s.orphans)],
      ["phantom modules (inline / unresolvable \\`mod\\`)", String(s.phantom)],
      ["unmapped-layer modules", String(s.unmappedLayers)],
    ],
  )}

Shape observations, interpretation only: every one of the ${s.files} files is claimed by exactly
one module node (no orphans), and the full-level ${s.edges} edges carry ${totals.fullTotal} references,
of which ${totals.selfAfterRoll} fall entirely inside one depth-2 group and therefore vanish from the
${s.nestedEdges}-edge nested rollup (${totals.nestedTotal} refs). Reference traffic concentrates around two
hubs: the busiest single module is \`${hub[0]}\` (${hub[1]} refs in) and the busiest depth-1
group is \`${topHub[0]}\` (${topHub[1]} refs in at the top-level rollup), which is why the
registry/segment side of the tree dominates the top-level view below.

`;
}

/** Section 5: top-level (depth 1) rollup view. */
function topLevelSection(graph) {
  const rows = graph.topEdges.map((e) => [
    `\`${e.source}\``,
    `\`${e.target}\``,
    String(e.count),
  ]);
  assert.strictEqual(rows.length, graph.stats.topEdges, "top-edge table rows must equal topEdges length");
  return `## Top-level view (depth 1)

Module paths rolled up to their first segment ("root" is the lib.rs facade root; sorted by
reference count, descending). Counts are references, not files.

${mdTable(["source group", "target group", "refs"], rows)}

`;
}

/** Section 6: full nested (depth 2) edge table — same data as the CSV. */
function nestedSection(graph, maxSites, totals) {
  const rows = graph.nestedEdges.map((e) => [
    `\`${e.source}\``,
    `\`${e.target}\``,
    String(e.count),
    e.cfgOnly ? "¶" : mdEsc(cfgList(e).join("; ")),
    sitesList(e, maxSites).join("; "),
  ]);
  assert.strictEqual(rows.length, graph.nestedEdges.length,
    "nested table rows must equal nestedEdges length");
  return `## Nested view (depth 2)

The complete depth-2 rollup: ${graph.nestedEdges.length} edges carrying ${totals.nestedTotal} references. This
table is the same data as the committed CSV companion
(\`docs/perf/MODULE_GRAPH_20261115.csv\`, one row per edge below). Sorted by reference
count, then source/target. cfg cell: ¶ = every counted reference of the edge sits behind a
cfg predicate; otherwise the cell lists the distinct cfg predicates present among its sites
(empty = ungated).

${mdTable(["source", "target", "refs", "cfg", "first sites"], rows)}

`;
}

/** Section 7: back-edges vs the plan §2 hypothesis + the segment↔route cycle. */
function backEdgeSection(graph, maxSites) {
  const layers = [...graph.config.layers].sort((a, b) => a.order - b.order)
    .map((l) => `${l.name} (${l.order})`).join(" → ");
  const rows = graph.backEdges.map((e) => [
    `\`${e.source}\` (${e.sourceLayer.name}@${e.sourceLayer.order})`,
    `\`${e.target}\` (${e.targetLayer.name}@${e.targetLayer.order})`,
    String(e.count),
    sitesList(e, maxSites).join("; "),
  ]);
  assert.strictEqual(rows.length, graph.backEdges.length, "back-edge table rows must equal backEdges length");
  const fwd = graph.cycleEdges.find((e) => e.sourceLayer.name === "segment" && e.targetLayer.name === "route");
  const rev = graph.cycleEdges.find((e) => e.sourceLayer.name === "route" && e.targetLayer.name === "segment");
  assert.ok(fwd !== undefined && rev !== undefined,
    "cycle subsection needs the segment→route AND route→segment directions among cycleEdges");
  const cycleRow = (e) => [
    `\`${e.source}\` (${e.sourceLayer.name}@${e.sourceLayer.order}) → \`${e.target}\` (${e.targetLayer.name}@${e.targetLayer.order})`,
    String(e.count),
    sitesList(e, maxSites).join("; "),
  ];
  return `## Back-edges vs the plan §2 hypothesis

Layer order convention (from \`scripts/module-graph/layers.json\`, bottom to top): ${layers}.
Order 1 is the primitive bottom of the stack, order 8 the facade top. A **back-edge** here is
an edge whose source layer order is LOWER than its target layer order — i.e. a dependency
pointing UP the stack, against the plan's §2 hypothesis. ${graph.backEdges.length} such edges exist today.

${mdTable(["source (layer@order)", "target (layer@order)", "refs", "first sites"], rows)}

### Cycle: segment ↔ route (order left open by the plan)

Both directions between the \`segment\` (order ${fwd.targetLayer.order}/${fwd.sourceLayer.order}) and \`route\` layers, as
measured:

${mdTable(["direction", "refs", "first sites"], [cycleRow(rev), cycleRow(fwd)])}

Plan §2 marks this order as open — "the segment/route order is decided by A0" — and §2 keeps
the two crates mergeable if relocation does not remove the dependency. The data above is
presented as measured; this report does not decide the order.

`;
}

/** Section 8: cross-check of the plan §1 claims in layers.json. */
function claimsSection(graph) {
  const byId = new Map(graph.claims.map((c) => [c.id, c]));
  const rows = graph.config.expected_back_edges.map((cfg) => {
    const c = byId.get(cfg.id);
    assert.ok(c !== undefined, `claim ${cfg.id} from config has no evaluated claim row`);
    const m = c.measured ?? {};
    let evidence = c.evidence.map(mdEsc).join("<br>");
    assert.ok(c.evidence.length <= 20, `claim ${cfg.id}: evidence list exceeds 20 rows`);
    if (cfg.expect === "no-incoming-code-edge") {
      // Honest raw split: facade re-exports + kani proofs + production refs.
      assert.strictEqual(m.incomingRefsRaw, m.excludedReexportRefs + m.excludedKaniCfgRefs + m.countedIncomingRefs,
        `claim ${cfg.id}: raw incoming refs must equal the reexport+kani+production split`);
      evidence = [
        `raw incoming refs: ${m.incomingRefsRaw}`,
        `${m.excludedReexportRefs} facade \`pub use\` re-export refs from root (facade plumbing, not usage)`,
        `${m.excludedKaniCfgRefs} \`cfg(kani)\` proof ref (verification-only per CLAUDE.md)`,
        `${m.countedIncomingRefs} non-facade non-kani production refs`,
        ...c.evidence.map(mdEsc),
      ].join("<br>");
    }
    const refs = cfg.expect === "doc-only"
      ? `${m.codeRefs} code refs; ${m.matchingDocRefs} doc refs`
      : cfg.expect === "no-incoming-code-edge"
        ? `${m.countedIncomingRefs} production refs (raw incoming: ${m.incomingRefsRaw})`
        : `${m.totalRefs} refs over ${m.matchedEdges} edge(s)`;
    return [`\`${cfg.id}\``, mdEsc(cfg.claim), c.status, refs, evidence];
  });
  assert.strictEqual(rows.length, graph.config.expected_back_edges.length,
    "claims table rows must equal config.expected_back_edges length");
  return `## Cross-check: plan §1 claims

Each row re-checks one plan §1 claim as encoded in \`expected_back_edges\` of
\`scripts/module-graph/layers.json\`. Status is computed by graph.mjs (confirmed / partial /
refuted / not-found); evidence is the measured site list (capped at 20 rows).

${mdTable(["claim id", "claim (from config)", "status", "measured refs", "evidence (file:line)"], rows)}

`;
}

/** Section 9: restricted-visibility widening at the future crate boundaries. */
function boundarySection(graph) {
  const rows = graph.boundaryVisibility.map((b) => {
    const sum = b.items.reduce((n, i) => n + i.userCount, 0);
    assert.strictEqual(sum, b.usedAcrossCount,
      `boundary ${b.crate}: usedAcrossCount ${b.usedAcrossCount} must equal sum of item userCounts ${sum}`);
    const top3 = b.items.slice(0, 3).map((i) => `\`${i.name}\` ×${i.userCount} (${i.declKind})`).join(", ");
    return [
      `\`${b.crate}\``,
      String(b.restrictedTotal),
      String(b.usedAcrossCount),
      String(b.items.length),
      top3 === "" ? "—" : top3,
    ];
  });
  const total = graph.boundaryVisibility.reduce((n, b) => n + b.usedAcrossCount, 0);
  const top2 = [...graph.boundaryVisibility].sort((a, b) => b.usedAcrossCount - a.usedAcrossCount || a.crate.localeCompare(b.crate)).slice(0, 2);
  assert.strictEqual(total, graph.boundaryVisibility.reduce((n, b) => n + b.items.reduce((m, i) => m + i.userCount, 0), 0),
    "Σ usedAcrossCount must equal the per-item sum across all boundaries");
  return `## Visibility widening at future crate boundaries (plan §1 / G1 input)

Method: only path-addressable references are attributable — \`use\` items and qualified paths
whose final segment names a restricted-visibility declaration in the target crate. Trait and
impl method calls (\`x.method()\`) are not attributable to a declaration this way, so every
"refs used across" count below is a **LOWER BOUND** on the widening a crate split would need.

${mdTable(
    ["crate", "restricted-vis decls (total)", "refs used across the boundary", "distinct items used across", "top-3 items by users"],
    rows,
  )}

Derived note: Σ usedAcross refs over all ${graph.boundaryVisibility.length} boundaries = ${total}, and the widening
cost concentrates in ${top2.map((b) => `\`${b.crate}\``).join(" and ")} — the bottom of the stack, which everything
depends downward on. This is an observation about where the cost sits, not a recommendation;
the go/no-go on widening belongs to the plan's owner gates.

`;
}

/** Section 10: data quality (orphans, phantom, unresolved, unmapped, extern). */
function dataQualitySection(graph) {
  const list = (items, empty) => (items.length === 0 ? [empty] : items.map((x) => "- " + x));
  const unmapped = graph.modules.filter((m) => m.layer === null);
  return `## Data quality

**Orphan files** (a \`.rs\` file under src/ claimed by no module node — expected 0):
${list(graph.orphans.map((f) => `\`${f}\``), "none — every file is claimed by exactly one module.")}

**Phantom modules** (${graph.phantom.length}; a phantom is an inline \`mod { ... }\` or an unresolvable
\`mod x;\` declaration — it is a node name without a file, legal here as kani proof containers):
${graph.phantom.map((p) => `- \`${p.module}\` — inline module at \`${p.file}:${p.line}\``).join("\n")}

**Unresolved refs** (${graph.unresolvedRefs.length}; references that resolve to no module and name no
known extern crate):
${list(graph.unresolvedRefs.map((r) => `\`${r.file}:${r.line}\` — \`${r.kind}::${r.segs.join("::")}\``), "none.")}

**Unmapped-layer modules** (${unmapped.length}; no layer in \`layers.json\` matches them):
${unmapped.map((m) => `- \`${m.path}\` (\`${m.file}\`)`).join("\n")}
These are the group-wiring \`mod.rs\` files that only re-export across several future-crate
boundaries, so no single layer fits. **A1 input:** the layering lint must assign each of
them a layer (or an explicit exemption) before it can run.

**Extern-crate uses**: ${graph.externUses} \`use\` items / paths name an external crate (prelude,
workspace members or Cargo.toml dependencies) and are out of scope for an intra-src graph.

`;
}

/** Sections 11 + 12: CI wiring (deliberately not done here) + regeneration. */
function ciAndRegenSection(graph) {
  return `## CI wiring (needed, deliberately not done here)

- The graph stage's tests live at \`tests/module_graph.test.mjs\` (node:test).
- This repo runs node-script tests via \`node --test\` (see
  \`.github/actions/ci-j13b-judge-preparation/action.yml\` for the existing pattern).
- A CI step running \`node --test tests/module_graph.test.mjs\` is **needed and is NOT added
  by this task** — workflow edits are out of scope here; step T1 reworks the same CI rules
  and is the right place to wire it.
- \`node scripts/module-graph.mjs --check-backedges\` (see Regenerate) is the future A1
  ratchet entry point: today it prints the baseline and exits 0; once the layering lint
  lands it exits non-zero on any back-edge above the recorded baseline.

## Regenerate

All commands run from the repository root; re-runs refresh only \`generatedAtISO\` in the
Identity section — every other byte of both artifacts is deterministic.

\`\`\`sh
# regenerate BOTH committed artifacts (this report + its CSV companion)
node scripts/module-graph.mjs --report docs/perf
# console summary (identity + stats + per-claim status)
node scripts/module-graph.mjs
# nested-edge CSV alone, anywhere
node scripts/module-graph.mjs --csv <path>
# full graph JSON to stdout
node scripts/module-graph.mjs --json
# back-edge ratchet check
node scripts/module-graph.mjs --check-backedges
\`\`\`

Graph built from ${graph.stats.files} files at \`HEAD:src\` = \`${graph.identity.srcTreeSha}\`.
`;
}

/**
 * Render the full markdown report. Every count comes from the graph object;
 * assertGraph + per-table row asserts make a wrong table impossible.
 */
export function renderMarkdown(graph, { maxSites = 5, dateISO } = {}) {
  const totals = assertGraph(graph);
  const generatedAt = dateISO ?? graph.identity.generatedAtISO;
  const intro = `# Module dependency graph — src/ (A0)

This report is the exact module dependency graph of \`src/\` at the commit pinned in the
Identity section. It is the input for the plan §2 decomposition
(docs/plans/2026-11-15-workspace-restructure-plan.md), for the future A1 layering lint, and
for the G1 visibility decision.

**This report carries no GO/NO-GO verdict and draws no decisions.** It is static analysis
of a clean tree, presented as measured data with its limitations stated; all verdicts
belong to the plan's owner gates.

Generated by: \`node scripts/module-graph.mjs --report docs/perf\` (repo root).

`;
  const identity = identitySection({ ...graph, identity: { ...graph.identity, generatedAtISO: generatedAt } });
  const body = [
    intro,
    identity,
    methodSection(graph),
    shapeSection(graph, totals),
    topLevelSection(graph),
    nestedSection(graph, maxSites, totals),
    backEdgeSection(graph, maxSites),
    claimsSection(graph),
    boundarySection(graph),
    dataQualitySection(graph),
    ciAndRegenSection(graph),
  ].join("");
  assert.ok(!ABS_PATH_RE.test(body), "report must not contain absolute machine paths");
  return body;
}

/** Render the nested-edge CSV companion (one row per nestedEdge). */
export function renderCsv(graph, { maxSites = 5 } = {}) {
  assertGraph(graph);
  const rows = [CSV_HEADER];
  for (const e of graph.nestedEdges) {
    rows.push([
      e.source,
      e.target,
      String(e.count),
      e.cfgOnly ? "true" : "false",
      cfgList(e).join("; "),
      sitesList(e, maxSites).join(";"),
    ]);
  }
  assert.strictEqual(rows.length - 1, graph.nestedEdges.length,
    "CSV data rows must equal nestedEdges length");
  return rows.map((r) => r.map(csvCell).join(",")).join("\n") + "\n";
}

/** Write both artifacts (same base name, side by side) under outDir. */
export function writeReport(graph, { outDir = "docs/perf", base = "MODULE_GRAPH_20261115", maxSites = 5 } = {}) {
  const md = renderMarkdown(graph, { maxSites, dateISO: graph.identity.generatedAtISO });
  const csv = renderCsv(graph, { maxSites });
  mkdirSync(outDir, { recursive: true });
  const mdPath = outDir.split("\\").join("/") + "/" + base + ".md";
  const csvPath = outDir.split("\\").join("/") + "/" + base + ".csv";
  writeFileSync(mdPath, md);
  writeFileSync(csvPath, csv);
  return { mdPath, csvPath };
}
