#!/usr/bin/env node
// Thin CLI entry for the module-dependency-graph tool (scripts/module-graph/:
// lexer.mjs → parse.mjs → graph.mjs → report.mjs; THIS file adds no business
// logic — it parses argv, builds the graph, prints or writes).
//
//   node scripts/module-graph.mjs                       # summary: identity + stats + one line per claim status
//   node scripts/module-graph.mjs --level top|nested    # print that rollup's edge list (source target count cfg)
//   node scripts/module-graph.mjs --json                # full graph JSON to stdout
//   node scripts/module-graph.mjs --csv <path>          # write the nested-edge CSV
//   node scripts/module-graph.mjs --report [dir]        # write <dir>/MODULE_GRAPH_20261115.md + .csv (default docs/perf)
//   node scripts/module-graph.mjs --emit-baseline       # print the derived ratchet baseline CSV to stdout
//                                                       #   (docs/layering_baseline.csv is derived from this)
//   node scripts/module-graph.mjs --check-backedges     # A1 ratchet lint against the baseline CSV (exit 1 on violation)
//
// Ratchet semantics (--check-backedges, baseline default docs/layering_baseline.csv,
// override with --baseline <path>): every known upward back-edge is pinned with its
// reference count; counts may only shrink, via an explicit edit of the baseline:
//   a) a NEW back-edge not in the baseline is RED;
//   b) a count GROWTH beyond the baseline count is RED;
//   c) a count drop is RED with a "lower the baseline" hint (lower it explicitly);
//   d) a VANISHED edge with a stale baseline row is RED — same rule as (c).
// Conduits: the group-wiring mod.rs modules `alloc_core` and `registry` are declared
// status "conduit" in layers.json — their own re-export edges are never back-edges;
// references THROUGH them resolve to the defining module (graph-level), so nothing is
// lost. Layers: segment and route are ONE layer `segment_route` (order 4) until the
// G1 order decision — their mutual references are same-layer, hence not back-edges.
//
// Shared flags: --src <dir> (default src) --config <path>
//               (default scripts/module-graph/layers.json) --max-sites <N> (default 5)
//
// Exit codes: 0 ok; 1 runtime error or (--check-backedges) check failure or
// missing/malformed baseline; 2 usage error.

import { writeFileSync } from "node:fs";
import { buildModuleGraph } from "./module-graph/graph.mjs";
import { checkBackedges, emitBaseline, loadBaseline } from "./module-graph/layering.mjs";
import { renderCsv, renderMarkdown, writeReport } from "./module-graph/report.mjs";

const USAGE = `usage: node scripts/module-graph.mjs [--level top|nested] [--json] [--csv <path>]
                    [--report [dir]] [--check-backedges] [--emit-baseline]
                    [--baseline <path>] [--src <dir>] [--config <path>] [--max-sites <N>]`;

function parseArgs(argv) {
  const opts = { src: "src", config: "scripts/module-graph/layers.json", baseline: "docs/layering_baseline.csv", maxSites: 5, modes: [] };
  const wantValue = new Set(["--src", "--config", "--max-sites", "--csv", "--baseline"]);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => {
      if (i + 1 >= argv.length) throw new Error(`missing value for ${a}`);
      return argv[++i];
    };
    switch (a) {
      case "--src": opts.src = val(); break;
      case "--config": opts.config = val(); break;
      case "--max-sites": {
        const n = Number(val());
        if (!Number.isInteger(n) || n < 1) throw new Error("--max-sites needs a positive integer");
        opts.maxSites = n;
        break;
      }
      case "--csv": opts.csvPath = val(); opts.modes.push("csv"); break;
      case "--baseline": opts.baseline = val(); break;
      case "--level": {
        const level = val();
        if (level !== "top" && level !== "nested") throw new Error("--level must be top or nested");
        opts.level = level;
        opts.modes.push("level");
        break;
      }
      case "--report": {
        const next = argv[i + 1];
        opts.reportDir = next !== undefined && !next.startsWith("--") ? (++i, next) : "docs/perf";
        opts.modes.push("report");
        break;
      }
      case "--json": opts.json = true; opts.modes.push("json"); break;
      case "--check-backedges": opts.checkBackedges = true; opts.modes.push("check"); break;
      case "--emit-baseline": opts.emitBaseline = true; opts.modes.push("emit"); break;
      default: throw new Error(`unknown flag: ${a}`);
    }
  }
  return opts;
}

/** Default mode: identity + stats + one line per claim status. */
function printSummary(g) {
  const id = g.identity;
  console.log(`head ${id.headSha}`);
  console.log(`tree HEAD:src ${id.srcTreeSha}`);
  console.log(`content sha256 ${id.contentSha256} (${id.fileCount} files)`);
  console.log(`src ${id.srcDir} generated ${id.generatedAtISO}`);
  const s = g.stats;
  console.log(
    `files ${s.files} | modules ${s.modules} | edges(full) ${s.edges} | edges(nested) ${s.nestedEdges} |` +
    ` edges(top) ${s.topEdges} | back-edges ${s.backEdges} | cycle-directions ${s.cycleEdges} |` +
    ` docRefs ${s.docRefs} | externUses ${g.externUses} | unresolved ${s.unresolvedRefs} |` +
    ` orphans ${s.orphans} | phantom ${s.phantom} | unmappedLayers ${s.unmappedLayers}`,
  );
  for (const c of g.claims) console.log(`${c.id}: ${c.status}`);
}

/** --level mode: one "source target count cfg" line per rollup edge. */
function printLevel(g, level) {
  const list = level === "top" ? g.topEdges : g.nestedEdges;
  for (const e of list) {
    const cfg = e.cfgOnly ? "¶" : (e.cfgs.filter(Boolean).join(";") || "-");
    console.log(`${e.source} ${e.target} ${e.count} ${cfg}`);
  }
}

function run() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(`error: ${err.message}\n${USAGE}`);
    process.exit(2);
  }
  if (opts.modes.length > 1) {
    console.error(`error: pick at most one output mode (got: ${opts.modes.join(", ")})\n${USAGE}`);
    process.exit(2);
  }
  const g = buildModuleGraph({ srcDir: opts.src, configPath: opts.config, maxSites: opts.maxSites });
  if (opts.checkBackedges) {
    let baselineRows;
    try {
      baselineRows = loadBaseline(opts.baseline);
    } catch (err) {
      console.error(`layering lint: cannot load baseline ${opts.baseline}: ${err.message}`);
      process.exit(1);
    }
    const findings = checkBackedges(g, baselineRows, { maxSites: opts.maxSites });
    if (findings.length === 0) {
      const n = g.backEdges.length;
      console.log(`layering lint: OK — ${n} back-edge(s) within the ${baselineRows.length}-row baseline (${opts.baseline})`);
      return;
    }
    console.error(`layering lint: ${findings.length} violation(s) against ${opts.baseline}:`);
    for (const f of findings) console.error(f.message);
    process.exit(1);
  }
  if (opts.emitBaseline) {
    process.stdout.write(emitBaseline(g));
    return;
  }
  if (opts.json) {
    console.log(JSON.stringify(g, null, 1));
    return;
  }
  if (opts.level) {
    printLevel(g, opts.level);
    return;
  }
  if (opts.csvPath) {
    writeFileSync(opts.csvPath, renderCsv(g, { maxSites: opts.maxSites }));
    return;
  }
  if (opts.reportDir) {
    const { mdPath, csvPath } = writeReport(g, { outDir: opts.reportDir, maxSites: opts.maxSites });
    console.log(`wrote ${mdPath}`);
    console.log(`wrote ${csvPath}`);
    return;
  }
  printSummary(g);
}

try {
  run();
} catch (err) {
  console.error(`error: ${err?.stack ?? err}`);
  process.exit(1);
}
